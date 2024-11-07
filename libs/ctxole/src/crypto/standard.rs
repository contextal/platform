//! Ole *Standard Encryption*
use super::EncryptionAlgo;
use super::OleKey;
use aes::cipher::{inout, BlockDecrypt as _};
use ctxutils::{cmp::Unsigned as _, io::*};
use sha1::{
    digest::{crypto_common::KeyInit as _, Digest as _},
    Sha1,
};
use std::io::{self, Read, Seek, Write};
use tracing::{debug, warn};

/// Specifies properties of the encryption algorithm
#[derive(Debug, PartialEq)]
pub struct EncryptionHeaderFlags {
    /// Specifies whether CryptoAPI RC4 or ECMA-376 encryption is used
    pub crypto_api: bool,
    /// Specifies whether document properties are unencrypted
    pub doc_props: bool,
    /// Indicates if extensible encryption is used
    pub external: bool,
    /// Indicates that the protected content is an ECMA-376 document
    pub aes: bool,
}

impl EncryptionHeaderFlags {
    fn new<R: Read>(r: &mut R) -> Result<Self, io::Error> {
        let v = rdu32le(r)?;
        Ok(Self {
            crypto_api: v & 0b100 != 0,
            doc_props: v & 0b1000 != 0,
            external: v & 0b1_0000 != 0,
            aes: v & 0b10_0000 != 0,
        })
    }
}

/// Encryption properties for an encrypted stream
#[derive(Debug)]
pub struct EncryptionHeader {
    /// The properties of the encryption algorithm
    pub flags: EncryptionHeaderFlags,
    /// The encryption algorithm
    pub alg_id: u32,
    /// The hashing algorithm (always SHA-1)
    pub alg_id_hash: u32,
    /// The number of bits in the encryption key
    pub key_size: usize,
    /// Implementation-specific value that corresponds to constants accepted by the specified CSP
    pub provider_type: u32,
    /// A value that is undefined and must be ignored
    pub reserved1: u32,
    /// A value that must be 0 and must be ignored
    pub reserved2: u32,
    /// The CSP name
    pub csp_name: String,
    /// The encryption algorithm
    pub algorithm: EncryptionAlgo,
}

impl EncryptionHeader {
    fn new<R: Read + Seek>(r: &mut R) -> Result<Self, io::Error> {
        let flags = EncryptionHeaderFlags::new(r)?;
        let size_extra = rdu32le(r)?;
        if size_extra != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid EncryptionHeader SizeExtra ({size_extra})"),
            ));
        }
        let alg_id = rdu32le(r)?;
        let algorithm = if !flags.crypto_api || flags.external {
            None
        } else if !flags.aes {
            match alg_id {
                0x00000000 => Some(EncryptionAlgo::Rc4),
                0x00006801 => Some(EncryptionAlgo::Rc4),
                _ => None,
            }
        } else {
            match alg_id {
                0x00000000 => Some(EncryptionAlgo::Aes128),
                0x0000660e => Some(EncryptionAlgo::Aes128),
                0x0000660f => Some(EncryptionAlgo::Aes192),
                0x00006610 => Some(EncryptionAlgo::Aes256),
                _ => None,
            }
        };
        if algorithm.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid EncryptionHeader AlgId {alg_id}"),
            ));
        }
        let algorithm = algorithm.unwrap(); // Safe due tro previous check
        let alg_id_hash = rdu32le(r)?;
        if ![0x00000000, 0x00008004].contains(&alg_id_hash) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid EncryptionHeader AlgIDHash ({alg_id_hash})"),
            ));
        }
        let key_bits = rdu32le(r)?;
        match algorithm {
            EncryptionAlgo::Rc4
                if key_bits & 7 == 0 && (0x00000028..=0x00000080).contains(&key_bits) => {}
            EncryptionAlgo::Aes128 if key_bits == 128 => {}
            EncryptionAlgo::Aes192 if key_bits == 192 => {}
            EncryptionAlgo::Aes256 if key_bits == 256 => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid EncryptionHeader KeySize ({key_bits}) for AlgID {alg_id}"),
                ));
            }
        }

        let provider_type = rdu32le(r)?;
        let reserved1 = rdu32le(r)?;
        let reserved2 = rdu32le(r)?;
        let mut csp_name = String::new();
        let mut limited_reader = r.take(256);
        let mut utf16le_reader =
            utf8dec_rs::UTF8DecReader::for_label("UTF-16LE", &mut limited_reader).unwrap(); // UTF-16LE exists
        utf16le_reader.read_to_string(&mut csp_name)?;
        csp_name.pop();
        r.seek(io::SeekFrom::End(0))?; // skip to the end of csp_name if it's longer than we consumed
        Ok(Self {
            flags,
            alg_id,
            alg_id_hash,
            key_size: usize::try_from(key_bits / 8).unwrap(), // Safe due to check on key_bits above
            provider_type,
            reserved1,
            reserved2,
            csp_name,
            algorithm,
        })
    }
}

/// Data used to verify the decryption password / key
#[derive(Debug)]
pub struct EncryptionVerifier {
    /// Salt for key derivation
    pub salt: [u8; 16],
    /// The encrypted verifier
    pub encrypted_verifier: [u8; 16],
    /// The encrypted SHA-1 hash of the verifier
    pub encrypted_verifier_hash: Vec<u8>,
}

impl EncryptionVerifier {
    fn new<R: Read + Seek>(r: &mut R, enc_type: &EncryptionAlgo) -> Result<Self, io::Error> {
        let salt_size = rdu32le(r)?;
        if salt_size != 16 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid EncryptionVerifier SaltSize ({salt_size})"),
            ));
        }
        let mut salt = [0u8; 16];
        r.read_exact(&mut salt)?;
        let mut encrypted_verifier = [0u8; 16];
        r.read_exact(&mut encrypted_verifier)?;
        let verifier_hash_size = rdu32le(r)?;
        if verifier_hash_size != 20 {
            // Always sha-1
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid EncryptionVerifier VerifierHashSize ({verifier_hash_size})"),
            ));
        }
        let mut encrypted_verifier_hash: Vec<u8>;
        if let EncryptionAlgo::Rc4 = enc_type {
            encrypted_verifier_hash = vec![0u8; 20];
        } else {
            encrypted_verifier_hash = vec![0u8; 32];
        }
        r.read_exact(&mut encrypted_verifier_hash)?;
        Ok(Self {
            salt,
            encrypted_verifier,
            encrypted_verifier_hash,
        })
    }
}

/// *Standard encryption* data
#[derive(Debug)]
pub struct StandardEncryption {
    /// Header flags
    pub flags: EncryptionHeaderFlags,
    /// Header
    pub header: EncryptionHeader,
    /// Verifier
    pub verifier: EncryptionVerifier,
}

impl StandardEncryption {
    const ITERATIONS: u32 = 50_000;

    /// Parse a *standard encryption* stream
    pub fn new<R: Read + Seek>(mut r: R) -> Result<Self, io::Error> {
        let flags = EncryptionHeaderFlags::new(&mut r)?;
        let hdr_len = u64::from(rdu32le(&mut r)?);
        let mut small_r = SeekTake::new(&mut r, hdr_len);
        let header = EncryptionHeader::new(&mut small_r)?;
        if header.flags != flags {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Incompatible EncryptionInfo flags",
            ));
        }
        let verifier = EncryptionVerifier::new(&mut r, &header.algorithm)?;
        Ok(StandardEncryption {
            flags,
            header,
            verifier,
        })
    }

    fn derive_key(&self, password: &str) -> Vec<u8> {
        // Initial sha1(salt + password.as_utf16le)
        let mut sha1 = Sha1::new();
        sha1.update(self.verifier.salt);
        for word in password.encode_utf16() {
            sha1.update(word.to_le_bytes());
        }
        let mut hash = [0u8; 20];
        sha1.finalize_into_reset((&mut hash).into());
        // Iterations
        for iteration in 0..Self::ITERATIONS {
            sha1.update(iteration.to_le_bytes());
            sha1.update(hash);
            sha1.finalize_into_reset((&mut hash).into());
        }
        // A zero
        sha1.update(hash);
        sha1.update(0u32.to_le_bytes());
        sha1.finalize_into_reset((&mut hash).into());
        // x1 - filled with 6
        let mut base = [0x36; 64];
        for (x, h) in base.iter_mut().zip(hash) {
            *x ^= h;
        }
        sha1.update(base);
        let mut x1 = [0u8; 20];
        sha1.finalize_into_reset((&mut x1).into());
        // x2 - filled with \
        let mut base = [0x5c; 64];
        for (x, h) in base.iter_mut().zip(hash) {
            *x ^= h;
        }
        sha1.update(base);
        let mut x2 = [0u8; 20];
        sha1.finalize_into((&mut x2).into());
        // Truncate to key_size (guaranteed to fit)
        let mut res: Vec<u8> = Vec::with_capacity(20 + 20);
        res.extend_from_slice(&x1);
        res.extend_from_slice(&x2);
        res.truncate(self.header.key_size);
        res
    }

    /// Validate the provided password and return the derived key
    pub fn get_key(&self, password: &str) -> Option<OleKey> {
        let key = OleKey {
            key: self.derive_key(password),
        };
        debug!("Derived key for password '{password}': {key:x}");
        let mut verifier = [0u8; 16];
        let mut hash = [0u8; 32];
        let enc_verifier = &self.verifier.encrypted_verifier_hash;
        match &self.header.algorithm {
            EncryptionAlgo::Aes128 => {
                let decryptor = aes::Aes128Dec::new(key.as_slice().into());
                decryptor.decrypt_block_b2b(
                    (&self.verifier.encrypted_verifier).into(),
                    (&mut verifier).into(),
                );
                decryptor
                    .decrypt_block_b2b((&enc_verifier[0..16]).into(), (&mut hash[0..16]).into());
                decryptor
                    .decrypt_block_b2b((&enc_verifier[16..32]).into(), (&mut hash[16..32]).into());
            }
            EncryptionAlgo::Aes192 => {
                let decryptor = aes::Aes192Dec::new(key.as_slice().into());
                decryptor.decrypt_block_b2b(
                    (&self.verifier.encrypted_verifier).into(),
                    (&mut verifier).into(),
                );
                decryptor
                    .decrypt_block_b2b((&enc_verifier[0..16]).into(), (&mut hash[0..16]).into());
                decryptor
                    .decrypt_block_b2b((&enc_verifier[16..32]).into(), (&mut hash[16..32]).into());
            }
            EncryptionAlgo::Aes256 => {
                let decryptor = aes::Aes256Dec::new(key.as_slice().into());
                decryptor.decrypt_block_b2b(
                    (&self.verifier.encrypted_verifier).into(),
                    (&mut verifier).into(),
                );
                decryptor
                    .decrypt_block_b2b((&enc_verifier[0..16]).into(), (&mut hash[0..16]).into());
                decryptor
                    .decrypt_block_b2b((&enc_verifier[16..32]).into(), (&mut hash[16..32]).into());
            }
            _ => {
                warn!("RC4 is not allowed in Standard Encryption");
                return None;
            }
        };
        let hash = &hash[0..20];
        debug!("Decrypted_verifier: {:x?}", verifier);
        debug!("Decrypted_hash: {:x?}", hash);
        let mut sha1 = Sha1::new();
        sha1.update(verifier);
        let computed_hash = sha1.finalize();
        debug!("Computed_hash: {:x?}", computed_hash);
        if computed_hash.as_slice() == hash {
            Some(key)
        } else {
            None
        }
    }

    /// Decrypt the encrypted Ole stream with the given key
    ///
    /// Note: panics if the wrong key length is provided
    pub fn decrypt_stream<R: Read, W: Write>(
        &self,
        key: &OleKey,
        size: u64,
        mut r: R,
        mut w: W,
    ) -> Result<(), io::Error> {
        let mut todo = size;
        let mut cipher = [0u8; 4096];
        let mut plain = [0u8; 4096];
        while todo > 0 {
            let chunksz = 4096usize.umin(todo);
            let padded_chunksz = (chunksz + 15) & !15;
            r.read_exact(&mut cipher[0..padded_chunksz])?;
            let buf =
                inout::InOutBuf::new(&cipher[0..padded_chunksz], &mut plain[0..padded_chunksz])
                    .unwrap() // Safe because in and out have the same length
                    .into_chunks()
                    .0;
            match &self.header.algorithm {
                EncryptionAlgo::Aes128 => {
                    aes::Aes128Dec::new(key.as_slice().into()).decrypt_blocks_inout(buf)
                }
                EncryptionAlgo::Aes192 => {
                    aes::Aes192Dec::new(key.as_slice().into()).decrypt_blocks_inout(buf)
                }
                EncryptionAlgo::Aes256 => {
                    aes::Aes256Dec::new(key.as_slice().into()).decrypt_blocks_inout(buf)
                }
                _ => {
                    warn!("RC4 is not allowed in Standard Encryption");
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "RC4 is not allowed in Standard Encryption",
                    ));
                }
            };
            w.write_all(&plain[0..chunksz])?;
            todo = todo.saturating_sub(4096);
        }
        Ok(())
    }
}
