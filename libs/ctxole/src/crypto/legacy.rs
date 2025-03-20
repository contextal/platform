//! Legacy (in-place) Ole encryption

mod xor;
use super::EncryptionAlgo;
use super::standard::StandardEncryption;
use ctxutils::cmp::Unsigned as _;
use md5::{Digest as _, Md5};
use rc4::{self, KeyInit as _, Rc4, StreamCipher as _};
use sha1::Sha1;
use std::io::{self, Read, Seek};
pub use xor::*;

/// Office Binary Document RC4 Encryption data
pub struct BinaryRc4Encryption {
    /// Salt
    pub salt: [u8; 16],
    /// Encrypted verifier
    pub encrypted_verifier: [u8; 16],
    /// Encrypted MD5 hash
    pub encrypted_hash: [u8; 16],
}

impl BinaryRc4Encryption {
    /// Read Office Binary Document RC4 Encryption data
    pub fn new<R: Read>(r: &mut R) -> Result<Self, io::Error> {
        let mut salt = [0u8; 16];
        let mut encrypted_verifier = [0u8; 16];
        let mut encrypted_hash = [0u8; 16];
        r.read_exact(&mut salt)?;
        r.read_exact(&mut encrypted_verifier)?;
        r.read_exact(&mut encrypted_hash)?;
        Ok(Self {
            salt,
            encrypted_verifier,
            encrypted_hash,
        })
    }

    /// Validate the provided password and return the derived base key
    pub fn get_key(&self, password: &str, block_size: u16) -> Option<LegacyKey> {
        // Initial md5(password.as_utf16le)
        let mut md5 = Md5::new();
        for word in password.encode_utf16() {
            md5.update(word.to_le_bytes());
        }
        let mut hash = [0u8; 16];
        md5.finalize_into_reset((&mut hash).into());

        // md5(repeat 16x (hash[0..5] + salt))
        let mut buf = [0u8; 336];
        for i in 0..16 {
            let pos = i * 21;
            buf[pos..(pos + 5)].copy_from_slice(&hash[0..5]);
            buf[(pos + 5)..(pos + 21)].copy_from_slice(&self.salt);
        }
        md5.update(buf);
        md5.finalize_into_reset((&mut hash).into());
        let decryptor = BinaryRc4Key {
            key: hash[0..5].try_into().unwrap(),
            block_size,
        };

        // decrypt verifier and hash with block 0 key
        let key = decryptor.get_block_key(0);
        let mut rc4 = Rc4::new((&key).into());
        let mut verifier = [0u8; 16];
        rc4.apply_keystream_b2b(&self.encrypted_verifier, &mut verifier)
            .unwrap(); // Safe bs same sized
        let mut reference_hash = [0u8; 16];
        rc4.apply_keystream_b2b(&self.encrypted_hash, &mut reference_hash)
            .unwrap(); // Safe bs same sized
        md5.update(verifier);
        let mut computed_hash = [0u8; 16];
        md5.finalize_into((&mut computed_hash).into());

        // verify password
        if reference_hash == computed_hash {
            Some(LegacyKey::Rc4Key(decryptor))
        } else {
            None
        }
    }
}

/// 40-bit RC4 base key
#[derive(Debug, Clone)]
pub struct BinaryRc4Key {
    key: [u8; 5],
    block_size: u16,
}

impl BinaryRc4Key {
    fn get_block_key(&self, block_number: u32) -> [u8; 16] {
        // hash(hash[0..5] + block_number)
        let mut buf = [0u8; 9];
        buf[0..5].copy_from_slice(&self.key);
        buf[5..9].copy_from_slice(&block_number.to_le_bytes());
        let mut md5 = Md5::new();
        md5.update(buf);
        let mut hash = [0u8; 16];
        md5.finalize_into((&mut hash).into());

        // Return 128 bits
        hash[0..16].try_into().unwrap()
    }

    fn apply(&self, block_number: u32, buf: &mut [u8], offset: usize) {
        let mut skip = vec![0u8; offset];
        let block_key = self.get_block_key(block_number);
        let mut rc4 = Rc4::new(&block_key.into());
        rc4.apply_keystream(skip.as_mut_slice());
        rc4.apply_keystream(buf);
    }
}

/// Office Binary Document RC4 CryptoAPI Encryption
pub struct Rc4CryptoApiEncryption(pub StandardEncryption);

impl Rc4CryptoApiEncryption {
    /// Read Office Binary Document RC4 CryptoAPI Encryption
    pub fn new<R: Read + Seek>(r: &mut R) -> Result<Self, io::Error> {
        let se = StandardEncryption::new(r)?;
        if !matches!(se.header.algorithm, EncryptionAlgo::Rc4) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid EncryptionHeader algorithm",
            ));
        }
        Ok(Self(se))
    }

    /// Validate the provided password and return the derived base key
    pub fn get_key(&self, password: &str, block_size: u16) -> Option<LegacyKey> {
        // Initial sha1(salt + password.as_utf16le)
        let mut sha1 = Sha1::new();
        sha1.update(self.0.verifier.salt);
        for word in password.encode_utf16() {
            sha1.update(word.to_le_bytes());
        }
        let mut hash = [0u8; 20];
        sha1.finalize_into_reset(hash.as_mut_slice().into());
        let key = CryptoApiRc4Key {
            base: hash,
            key_size: self.0.header.key_size,
            block_size,
        };

        // decrypt verifier and hash with block 0 key
        let mut buf = [0u8; 16 + 20];
        buf[0..16].copy_from_slice(&self.0.verifier.encrypted_verifier);
        buf[16..].copy_from_slice(&self.0.verifier.encrypted_verifier_hash);
        key.apply(0, &mut buf, 0);

        sha1.update(&buf[0..16]);
        let mut computed_hash = [0u8; 20];
        sha1.finalize_into((&mut computed_hash).into());

        // verify password
        if buf[16..] == computed_hash {
            Some(LegacyKey::CryptoApiKey(key))
        } else {
            None
        }
    }
}

/// CryptoApi Rc4 base key
#[derive(Debug, Clone)]
pub struct CryptoApiRc4Key {
    base: [u8; 20],
    key_size: usize,
    block_size: u16,
}

impl CryptoApiRc4Key {
    fn apply(&self, block_number: u32, buf: &mut [u8], offset: usize) {
        let mut sha1 = Sha1::new();
        sha1.update(self.base.as_slice());
        sha1.update(block_number.to_le_bytes());
        let mut block_key = vec![0u8; 20];
        sha1.finalize_into(block_key.as_mut_slice().into());
        block_key.truncate(self.key_size);
        let mut skip = vec![0u8; offset];
        match self.key_size {
            5 => {
                block_key.resize(16, 0);
                let mut rc4 = Rc4::<rc4::consts::U16>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            6 => {
                let mut rc4 = Rc4::<rc4::consts::U6>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            7 => {
                let mut rc4 = Rc4::<rc4::consts::U7>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            8 => {
                let mut rc4 = Rc4::<rc4::consts::U8>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            9 => {
                let mut rc4 = Rc4::<rc4::consts::U9>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            10 => {
                let mut rc4 = Rc4::<rc4::consts::U10>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            11 => {
                let mut rc4 = Rc4::<rc4::consts::U11>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            12 => {
                let mut rc4 = Rc4::<rc4::consts::U12>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            13 => {
                let mut rc4 = Rc4::<rc4::consts::U13>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            14 => {
                let mut rc4 = Rc4::<rc4::consts::U14>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            15 => {
                let mut rc4 = Rc4::<rc4::consts::U15>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            16 => {
                let mut rc4 = Rc4::<rc4::consts::U16>::new(block_key.as_slice().into());
                rc4.apply_keystream(skip.as_mut_slice());
                rc4.apply_keystream(buf);
            }
            _ => unreachable!(),
        }
    }
}

/// Office binary decription key wrapper
#[derive(Debug, Clone)]
pub enum LegacyKey {
    /// Xor obfuscation key
    XorObfuscation(XorKey),
    /// 40-bit RC4 base key
    Rc4Key(BinaryRc4Key),
    /// CryptoApi Rc4 base key
    CryptoApiKey(CryptoApiRc4Key),
}

impl LegacyKey {
    /// Apply the key to the porovided cyphertext for in-place decryption
    pub fn apply(&self, buf: &mut [u8], stream_position: u64) {
        if let Self::XorObfuscation(k) = self {
            k.apply(buf, stream_position);
        } else {
            let block_size = match self {
                Self::Rc4Key(k) => k.block_size,
                Self::CryptoApiKey(k) => k.block_size,
                _ => unreachable!(),
            };
            let mut offset = (stream_position % u64::from(block_size)) as usize; // Safe bc mod
            let mut block_number = (stream_position / u64::from(block_size)) as u32; // Cast is intentional
            let mut start = 0usize;
            let mut todo = buf.len();
            while todo > 0 {
                let encrypted_size = todo.min(usize::from(block_size) - offset);
                match self {
                    Self::Rc4Key(k) => k.apply(
                        block_number,
                        &mut buf[start..(start + encrypted_size)],
                        offset,
                    ),
                    Self::CryptoApiKey(k) => k.apply(
                        block_number,
                        &mut buf[start..(start + encrypted_size)],
                        offset,
                    ),
                    _ => unreachable!(),
                }
                offset = 0;
                todo -= encrypted_size;
                start += encrypted_size;
                block_number += 1;
            }
        }
    }
}

/// Reader for Legacy Document encrypted and obfuscated streams
pub struct LegacyDecryptor<R: Read + Seek> {
    r: R,
    /// Decryption key
    pub key: Option<LegacyKey>,
    /// The size of the initial (unencrypted) portion of the stream
    pub header_size: u64,
}

impl<R: Read + Seek> LegacyDecryptor<R> {
    /// Create a new RC4 decrypting reader
    pub fn new(r: R, key: &LegacyKey, unencrypted_header_size: u64) -> Self {
        Self {
            r,
            key: Some(key.clone()),
            header_size: unencrypted_header_size,
        }
    }

    /// Create a new non-decrypting (passthrough) reader
    pub fn new_no_op(r: R) -> Self {
        // FIXME: drop and make new() take an Option<LegacyKey> instead?
        Self {
            r,
            key: None,
            header_size: 0,
        }
    }

    /// Access the inner (non-decrypting) R
    pub fn as_inner(&mut self) -> &mut R {
        &mut self.r
    }
}

impl<R: Read + Seek> Read for LegacyDecryptor<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        if self.key.is_none() {
            return self.r.read(buf);
        }
        let stream_position = self.r.stream_position()?;
        if stream_position < self.header_size {
            let needed = buf.len().umin(self.header_size - stream_position);
            return self.r.read(&mut buf[0..needed]);
        }
        let res = self.r.read(buf)?;
        self.key.as_ref().unwrap(/* checked at the beginning of the fn */).apply(
            buf, stream_position
        );
        Ok(res)
    }
}

impl<R: Read + Seek> Seek for LegacyDecryptor<R> {
    fn seek(&mut self, to: io::SeekFrom) -> Result<u64, io::Error> {
        self.r.seek(to)
    }
}
