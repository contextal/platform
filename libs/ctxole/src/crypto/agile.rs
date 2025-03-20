//! Ole *Agile Encryption*
use super::EncryptionAlgo;
use super::OleKey;
use aes::cipher::{BlockDecryptMut as _, KeyIvInit as _, block_padding};
use base64::Engine as _;
use ctxutils::{cmp::Unsigned as _, io::*};
use serde::{Deserialize, Deserializer, de::Error as _};
use sha1::Sha1;
use sha2::{
    Sha256, Sha384, Sha512,
    digest::{Digest, DynDigest},
};
use std::borrow::Cow;
use std::io::{self, Read, Seek, Write};
use tracing::debug;

/// A base64-encoded binary sequence
#[derive(Debug)]
pub struct Base64Binary(Vec<u8>);

impl<'de> Deserialize<'de> for Base64Binary {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s: String = Deserialize::deserialize(deserializer)?;
        base64::prelude::BASE64_STANDARD
            .decode(&s)
            .map(Base64Binary)
            .map_err(|e| D::Error::custom(format!("Invalid Base64Binary value ({s}): {e}")))
    }
}

/// Block chaining type (counter)
#[derive(Debug)]
pub enum EncryptionChaining {
    /// Cipher Block Chaining
    Cbc,
    /// Cipher feedback with 8-bit window
    Cfb,
}

/// A complex type that specifies the encryption used within this element
#[derive(Deserialize, Debug)]
pub struct CtKeyData {
    /// The number of bytes used by a salt
    #[serde(rename = "@saltSize")]
    pub salt_size: usize,
    /// The number of bytes used to encrypt one block of data
    #[serde(rename = "@blockSize")]
    pub block_size: usize,
    /// The number of bits used by an encryption algorithm
    #[serde(rename = "@keyBits")]
    pub key_bits: usize,
    /// The number of bytes used by a hash value
    #[serde(rename = "@hashSize")]
    pub hash_size: usize,
    /// The cipher algorithm
    #[serde(rename = "@cipherAlgorithm")]
    pub cipher_algorithm: String,
    /// The chaining mode used by cipher_algorithm
    #[serde(rename = "@cipherChaining")]
    pub cipher_chaining: String,
    /// The hashing algorithm
    #[serde(rename = "@hashAlgorithm")]
    pub hash_algorithm: String,
    /// Randomly generated salt
    #[serde(rename = "@saltValue")]
    pub salt_value: Base64Binary,
}

/// Data used to verify whether the encrypted data passes an integrity check
#[derive(Deserialize, Debug)]
pub struct CtDataIntegrity {
    /// Encrypted key used for encrypting the hmac
    #[serde(rename = "@encryptedHmacKey")]
    pub encrypted_hmac_key: Base64Binary,
    /// Encrypted hmac
    #[serde(rename = "@encryptedHmacValue")]
    pub encrypted_hmac_value: Base64Binary,
}

/// Intermediate key and related encryption data
#[derive(Deserialize, Debug)]
pub struct CtPasswordKeyEncryptor {
    /// The number of bytes used by a salt
    #[serde(rename = "@saltSize")]
    pub salt_size: usize,
    /// The number of bytes used to encrypt one block of data
    #[serde(rename = "@blockSize")]
    pub block_size: usize,
    /// The number of bits used by an encryption algorithm
    #[serde(rename = "@keyBits")]
    pub key_bits: usize,
    /// The number of bytes used by a hash value
    #[serde(rename = "@hashSize")]
    pub hash_size: usize,
    /// The cipher algorithm
    #[serde(rename = "@cipherAlgorithm")]
    pub cipher_algorithm: String,
    /// The chaining mode used by cipher_algorithm
    #[serde(rename = "@cipherChaining")]
    pub cipher_chaining: String,
    /// The hashing algorithm
    #[serde(rename = "@hashAlgorithm")]
    pub hash_algorithm: String,
    /// Randomly generated salt
    #[serde(rename = "@saltValue")]
    pub salt_value: Base64Binary,
    /// The number of times to iterate the password hash when creating the key
    #[serde(rename = "@spinCount")]
    pub spin_count: u32,
    /// Verifier input (encrypted)
    #[serde(rename = "@encryptedVerifierHashInput")]
    pub encrypted_verifier_hash_input: Base64Binary,
    /// Verifier hash (encrypted)
    #[serde(rename = "@encryptedVerifierHashValue")]
    pub encrypted_verifier_hash_value: Base64Binary,
    /// Intermediate key
    #[serde(rename = "@encryptedKeyValue")]
    pub encrypted_key_value: Base64Binary,
}

impl CtPasswordKeyEncryptor {
    fn derive_partial_key(&self, password: &str) -> Vec<u8> {
        // Initial digest(salt + password.as_utf16le)
        let mut digest = self.cd_get_digest().unwrap();
        digest.update(self.cd_salt_value());
        for word in password.encode_utf16() {
            digest.update(&word.to_le_bytes());
        }
        let mut hash = vec![0u8; digest.output_size()];
        digest.finalize_into_reset(hash.as_mut_slice()).unwrap(); // Safe: vec has proper length

        // Iterations
        for iteration in 0..self.spin_count {
            digest.update(&iteration.to_le_bytes());
            digest.update(&hash);
            digest.finalize_into_reset(hash.as_mut_slice()).unwrap(); // Safe: vec has proper length
        }
        hash
    }

    fn derive_key_final(&self, partial: &[u8], block_key: &[u8]) -> Vec<u8> {
        let mut digest = self.cd_get_digest().unwrap();
        // Final (hash + blockKey)
        digest.update(partial);
        digest.update(block_key);
        let mut hash = vec![0u8; digest.output_size()];
        digest.finalize_into_reset(hash.as_mut_slice()).unwrap(); // Safe: vec has proper length
        let wanted_size = self.cd_key_size();
        if hash.len() != wanted_size {
            hash.resize(wanted_size, 0x36);
        }
        hash
    }

    fn get_key(&self, password: &str) -> Option<Vec<u8>> {
        let partial_key = self.derive_partial_key(password);
        debug!("Partial key: {:?}", partial_key);
        let iv = self.cd_iv(None);
        let verifier_hash_input_key = self.derive_key_final(
            &partial_key,
            &[0xfe, 0xa7, 0xd2, 0x76, 0x3b, 0x4b, 0x9e, 0x79],
        );
        let verifier_hash_input = self.cd_decrypt(
            &verifier_hash_input_key,
            &iv,
            &self.encrypted_verifier_hash_input.0,
        );
        let mut digest = self.cd_get_digest().unwrap();
        digest.update(&verifier_hash_input);
        let computed_hash = digest.finalize();
        debug!("Computed_hash {:?}", computed_hash);
        let verifier_hash_value_key = self.derive_key_final(
            &partial_key,
            &[0xd7, 0xaa, 0x0f, 0x6d, 0x30, 0x61, 0x34, 0x4e],
        );
        let verifier_hash_value = self.cd_decrypt(
            &verifier_hash_value_key,
            &iv,
            &self.encrypted_verifier_hash_value.0,
        );
        debug!("Verifier_hash_value {:?}", verifier_hash_value);
        if verifier_hash_value != *computed_hash {
            return None;
        }
        let intermediate_key = self.derive_key_final(
            &partial_key,
            &[0x14, 0x6e, 0x0b, 0xe7, 0xab, 0xac, 0xd0, 0xd6],
        );
        let key = self.cd_decrypt(&intermediate_key, &iv, &self.encrypted_key_value.0);
        debug!("Actual key {:?}", key);
        Some(key)
    }
}

/// Key encryptor
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CtKeyEncryptor {
    /// Intermediate key data
    pub encrypted_key: CtPasswordKeyEncryptor,
    /// Undocuented field
    #[serde(rename = "@uri")]
    pub uri: String,
}

/// A sequences of one(!) key encryptor
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CtKeyEncryptors {
    /// Key encryptors
    pub key_encryptor: CtKeyEncryptor,
}

/// *Agile encryption* data
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgileEncryption {
    /// Encryption key data
    pub key_data: CtKeyData,
    /// Data integrity
    pub data_integrity: Option<CtDataIntegrity>,
    /// Intermediate key data
    pub key_encryptors: CtKeyEncryptors,
}

impl AgileEncryption {
    /// Parse an *agile encryption* stream
    pub fn new<R: Read + Seek>(r: &mut R) -> Result<Self, io::Error> {
        let _reserved = rdu32le(r)?; // 0x40
        let ae: AgileEncryption =
            quick_xml::de::from_reader(std::io::BufReader::new(r)).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid Agile EncryptionInfo: {e}"),
                )
            })?;
        ae.validate()?;
        Ok(ae)
    }

    fn validate(&self) -> Result<(), io::Error> {
        let kd = &self.key_data;
        let ek = &self.key_encryptors.key_encryptor.encrypted_key;
        kd.cd_validate()
            .map_err(|e| io::Error::new(e.kind(), format!("Invalid keyData: {e}")))?;
        ek.cd_validate()
            .map_err(|e| io::Error::new(e.kind(), format!("Invalid PasswordKeyEncryptor: {e}")))?;
        if kd.cipher_algorithm != ek.cipher_algorithm {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid PasswordKeyEncryptor: invalid cipherAlgorithm {} vs {}",
                    ek.cipher_algorithm, kd.cipher_algorithm,
                ),
            ));
        }
        if kd.hash_algorithm != ek.hash_algorithm {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid PasswordKeyEncryptor: invalid hashAlgorithm {} vs {}",
                    ek.hash_algorithm, kd.hash_algorithm,
                ),
            ));
        }
        if !(1..=10_000_000).contains(&ek.spin_count) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid PasswordKeyEncryptor: invalid SpinCount ({})",
                    ek.spin_count
                ),
            ));
        }
        if ek.encrypted_verifier_hash_input.0.len() < ek.salt_size {
            // Note: should also be a multiple of block_size but we don't care here
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid PasswordKeyEncryptor: invalid encryptedVerifierHashValue length {} vs {}",
                    ek.encrypted_verifier_hash_value.0.len(),
                    ek.hash_size,
                ),
            ));
        }
        if ek.encrypted_verifier_hash_value.0.len() < ek.hash_size {
            // Note: should also be a multiple of block_size but we don't care here
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid PasswordKeyEncryptor: invalid encryptedVerifierHashValue length {} vs {}",
                    ek.encrypted_verifier_hash_value.0.len(),
                    ek.hash_size,
                ),
            ));
        }
        if ek.encrypted_key_value.0.len() < kd.cd_key_size() {
            // Note: should also be a multiple of block_size but we don't care here
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid PasswordKeyEncryptor: invalid encryptedKeyValue length {} vs {}",
                    ek.encrypted_key_value.0.len(),
                    kd.cd_key_size(),
                ),
            ));
        }
        // FIXME check hmac too
        Ok(())
    }

    /// Validate the provided password and return the derived key
    pub fn get_key(&self, password: &str) -> Option<OleKey> {
        let mut key = self
            .key_encryptors
            .key_encryptor
            .encrypted_key
            .get_key(password)?;
        key.truncate(self.key_data.cd_key_size());
        Some(OleKey { key })
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
        let kd = &self.key_data;
        let block_size = kd.cd_block_size();
        let mut todo = size;
        let mut cipher = [0u8; 4096];
        let mut segment = 0u32;
        while todo > 0 {
            let chunksz = 4096usize.umin(todo);
            let modsz = chunksz % block_size;
            let padded_chunksz = if modsz == 0 {
                chunksz
            } else {
                chunksz + block_size - modsz
            };
            r.read_exact(&mut cipher[0..padded_chunksz])?;
            let iv = kd.cd_iv(Some(&segment.to_le_bytes()));
            let plain = kd.cd_decrypt(key.as_slice(), iv.as_slice(), &cipher);
            w.write_all(&plain)?;
            todo = todo.saturating_sub(4096);
            segment += 1;
        }
        Ok(())
    }
}

trait CryptoData {
    fn cd_salt_size(&self) -> usize;
    fn cd_salt_value(&self) -> &[u8];
    fn cd_cipher_algorithm(&self) -> &str;
    fn cd_key_bits(&self) -> usize;
    fn cd_block_size(&self) -> usize;
    fn cd_hash_algorithm(&self) -> &str;
    fn cd_hash_size(&self) -> usize;
    fn cd_cipher_chaining(&self) -> &str;

    fn cd_key_size(&self) -> usize {
        self.cd_key_bits() / 8
    }

    fn cd_algo(&self) -> Result<EncryptionAlgo, io::Error> {
        match self.cd_cipher_algorithm() {
            "AES" => match self.cd_key_bits() {
                128 => Ok(EncryptionAlgo::Aes128),
                192 => Ok(EncryptionAlgo::Aes192),
                256 => Ok(EncryptionAlgo::Aes256),
                bits => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid keyBits ({}) for AES", bits),
                )),
            },
            // "RC2" => {}
            // "RC4" => {}
            // "DES" => {}
            // "DESX" => {}
            // "3DES" => {}
            // "3DES_112" => {}
            algo => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid/unsupported cipherAlgorithm ({})", algo),
            )),
        }
    }

    fn cd_chaining(&self) -> Result<EncryptionChaining, io::Error> {
        match self.cd_cipher_chaining() {
            "ChainingModeCBC" => Ok(EncryptionChaining::Cbc),
            "ChainingModeCFB" => Ok(EncryptionChaining::Cfb),
            chaining => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid CipherChaining ({})", chaining),
            )),
        }
    }

    fn cd_validate(&self) -> Result<(), io::Error> {
        let salt_size = self.cd_salt_size();
        if !(1..=65546).contains(&salt_size) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid saltSize ({})", salt_size),
            ));
        }
        if salt_size != self.cd_salt_value().len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "saltSize mismatch ({} vs {})",
                    salt_size,
                    self.cd_salt_value().len()
                ),
            ));
        }
        let block_size = self.cd_block_size();
        if !(2..4096).contains(&block_size) || block_size & 1 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid blockSize ({})", block_size),
            ));
        }
        let key_bits = self.cd_key_bits();
        if key_bits == 0 || key_bits & 0b111 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid keyBits ({})", key_bits),
            ));
        }
        match self.cd_algo()? {
            EncryptionAlgo::Aes128 if block_size == 16 => {}
            EncryptionAlgo::Aes192 if block_size == 16 => {}
            EncryptionAlgo::Aes256 if block_size == 16 => {}
            algo => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "invalid/unsupported cipherAlgorithm ({}) with blockSize ({}) and keyBits ({})",
                        algo, block_size, key_bits
                    ),
                ));
            }
        }
        self.cd_chaining()?;
        let digest = self.cd_get_digest()?;
        if digest.output_size() != self.cd_hash_size() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "hashSize mismatch ({} vs {})",
                    self.cd_hash_size(),
                    digest.output_size(),
                ),
            ));
        }
        Ok(())
    }

    fn cd_get_digest(&self) -> Result<Box<dyn DynDigest>, io::Error> {
        let digest: Box<dyn DynDigest> = match self.cd_hash_algorithm() {
            "SHA1" => Box::new(Sha1::new()),
            "SHA256" => Box::new(Sha256::new()),
            "SHA384" => Box::new(Sha384::new()),
            "SHA512" => Box::new(Sha512::new()),
            // "MD5" => 16,
            // "MD4" => 16,
            // "MD2" => 16,
            // "RIPEMD-128" => 16,
            // "RIPEMD-160" => 20,
            // "WHIRLPOOL" => 64,
            algo => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid hashAlgorithm ({})", algo),
                ));
            }
        };
        Ok(digest)
    }

    fn cd_pad_to_block_size<'a>(&self, unpadded: &'a [u8]) -> Cow<'a, [u8]> {
        let unpadded_len = unpadded.len();
        let block_size = self.cd_block_size();
        if unpadded_len % block_size != 0 {
            Cow::Borrowed(unpadded)
        } else {
            let pad_len = block_size - (unpadded_len % block_size);
            let mut owned = unpadded.to_vec();
            owned.resize(unpadded_len + pad_len, 0);
            Cow::Owned(owned)
        }
    }

    // PANICS!!!
    // The following functions are guaranteed not to panic if Self::validate() returned successfully

    fn cd_decrypt(&self, key: &[u8], iv: &[u8], cypher: &[u8]) -> Vec<u8> {
        let pcypher = self.cd_pad_to_block_size(cypher);
        let mut plain = vec![0u8; pcypher.len()];
        match self.cd_chaining().unwrap() {
            EncryptionChaining::Cbc => match self.cd_algo().unwrap() {
                EncryptionAlgo::Aes128 => {
                    cbc::Decryptor::<aes::Aes128>::new_from_slices(key, iv)
                        .unwrap() // Safe bc key.len is checked, iv.len() = block_size
                        .decrypt_padded_b2b_mut::<block_padding::NoPadding>(&pcypher, &mut plain)
                        .unwrap() /* Safe bc padded */;
                }
                EncryptionAlgo::Aes192 => {
                    cbc::Decryptor::<aes::Aes192>::new_from_slices(key, iv)
                        .unwrap() // Safe bc key.len is checked, iv.len() = block_size
                        .decrypt_padded_b2b_mut::<block_padding::NoPadding>(&pcypher, &mut plain)
                        .unwrap() /* Safe bc padded */;
                }
                EncryptionAlgo::Aes256 => {
                    cbc::Decryptor::<aes::Aes256>::new_from_slices(key, iv)
                        .unwrap() // Safe bc key.len is checked, iv.len() = block_size
                        .decrypt_padded_b2b_mut::<block_padding::NoPadding>(&pcypher, &mut plain)
                        .unwrap() /* Safe bc padded */;
                }
                _ => unreachable!(),
            },
            EncryptionChaining::Cfb => match self.cd_algo().unwrap() {
                EncryptionAlgo::Aes128 => {
                    cfb8::Decryptor::<aes::Aes128>::new_from_slices(key, iv)
                        .unwrap() // Safe bc key.len is checked, iv.len() = block_size
                        .decrypt_padded_b2b_mut::<block_padding::NoPadding>(&pcypher, &mut plain)
                        .unwrap() /* Safe bc padded */;
                }
                EncryptionAlgo::Aes192 => {
                    cfb8::Decryptor::<aes::Aes192>::new_from_slices(key, iv)
                        .unwrap() // Safe bc key.len is checked, iv.len() = block_size
                        .decrypt_padded_b2b_mut::<block_padding::NoPadding>(&pcypher, &mut plain)
                        .unwrap() /* Safe bc padded */;
                }
                EncryptionAlgo::Aes256 => {
                    cfb8::Decryptor::<aes::Aes256>::new_from_slices(key, iv)
                        .unwrap() // Safe bc key.len is checked, iv.len() = block_size
                        .decrypt_padded_b2b_mut::<block_padding::NoPadding>(&pcypher, &mut plain)
                        .unwrap() /* Safe bc padded */;
                }
                _ => unreachable!(),
            },
        }
        plain.truncate(cypher.len());
        plain
    }

    fn cd_iv(&self, block_key: Option<&[u8]>) -> Vec<u8> {
        let mut hash = if let Some(block_key) = block_key {
            // Hash of salt + block_key
            let mut digest = self.cd_get_digest().unwrap();
            digest.update(self.cd_salt_value());
            digest.update(block_key);
            digest.finalize().to_vec()
        } else {
            // Just the salt
            self.cd_salt_value().to_vec()
        };
        if hash.len() != self.cd_block_size() {
            hash.resize(self.cd_block_size(), 0x36);
        }
        hash
    }
}

impl CryptoData for CtKeyData {
    fn cd_salt_size(&self) -> usize {
        self.salt_size
    }
    fn cd_salt_value(&self) -> &[u8] {
        self.salt_value.0.as_slice()
    }
    fn cd_cipher_algorithm(&self) -> &str {
        self.cipher_algorithm.as_str()
    }
    fn cd_key_bits(&self) -> usize {
        self.key_bits
    }
    fn cd_block_size(&self) -> usize {
        self.block_size
    }
    fn cd_hash_algorithm(&self) -> &str {
        self.hash_algorithm.as_str()
    }
    fn cd_hash_size(&self) -> usize {
        self.hash_size
    }
    fn cd_cipher_chaining(&self) -> &str {
        self.cipher_chaining.as_str()
    }
}

impl CryptoData for CtPasswordKeyEncryptor {
    fn cd_salt_size(&self) -> usize {
        self.salt_size
    }
    fn cd_salt_value(&self) -> &[u8] {
        self.salt_value.0.as_slice()
    }
    fn cd_cipher_algorithm(&self) -> &str {
        self.cipher_algorithm.as_str()
    }
    fn cd_key_bits(&self) -> usize {
        self.key_bits
    }
    fn cd_block_size(&self) -> usize {
        self.block_size
    }
    fn cd_hash_algorithm(&self) -> &str {
        self.hash_algorithm.as_str()
    }
    fn cd_hash_size(&self) -> usize {
        self.hash_size
    }
    fn cd_cipher_chaining(&self) -> &str {
        self.cipher_chaining.as_str()
    }
}
