//! # Ole encryption
//!
//! This module is dedicated to processing cryptography data inside Ole objects
//! according to [\[MS-OFFCRYPTO\]](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-offcrypto/3c34d72a-1a61-4b52-a893-196f9157f083)
//!
//! Feature support:
//! - [x] XOR Obfuscation
//! - [x] Office Binary Document RC4 Encryption
//! - [x] Office Binary Document RC4 CryptoAPI Encryption
//! - [x] ECMA-376 *Standard Encryption*
//! - [x] ECMA-376 *Agile Encryption*
//! - [ ] ECMA-376 *Extensible Encryption*
//!
//! Agile algorithm support:
//! - [ ] RC2
//! - [ ] RC4
//! - [x] AES-128
//! - [x] AES-192
//! - [x] AES-256
//! - [ ] DES
//! - [ ] DESX
//! - [ ] 3DES
//! - [ ] 3DES_112
//!
//! Agile counters support:
//! - [x] CBC
//! - [x] CFB-8
//!
//! Agile hash support:
//! - [x] SHA1
//! - [x] SHA256
//! - [x] SHA384
//! - [x] SHA512
//! - [ ] MD5
//! - [ ] MD4
//! - [ ] MD2
//! - [ ] RIPEMD-128
//! - [ ] RIPEMD-160
//! - [ ] WHIRLPOOL

mod agile;
mod ds;
mod legacy;
mod standard;
use crate::Ole;
pub use agile::*;
use ctxutils::io::*;
pub use ds::*;
pub use legacy::*;
pub use standard::*;
use std::io::{self, Read, Seek, Write};
use tracing::{debug, warn};

/// Encryption algorithm
#[derive(Debug)]
pub enum EncryptionAlgo {
    /// RC4
    Rc4,
    /// AES-128
    Aes128,
    /// AES-192
    Aes192,
    /// AES-256
    Aes256,
}

impl std::fmt::Display for EncryptionAlgo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "{}",
            match self {
                Self::Rc4 => "RC4",
                Self::Aes128 => "AES-128",
                Self::Aes192 => "AES-192",
                Self::Aes256 => "AES-256",
            }
        )
    }
}

/// Version of a component
#[derive(Debug)]
pub struct Version {
    /// Major number
    pub major: u16,
    /// Minor number
    pub minor: u16,
}

impl Version {
    /// Reads and parse a component version
    pub fn new<R: Read>(r: &mut R) -> Result<Self, io::Error> {
        Ok(Self {
            major: rdu16le(r)?,
            minor: rdu16le(r)?,
        })
    }

    fn is(&self, other: (u16, u16)) -> bool {
        self.major == other.0 && self.minor == other.1
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Type of Ole encryption
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum EncryptionType {
    /// *Standard Encryption*
    Standard(StandardEncryption),
    /// *Agile Encryption*
    Agile(AgileEncryption),
}

/// *Encryption info*
#[derive(Debug)]
pub struct EncryptionInfo {
    /// *Encryption Version Info*
    pub version: Version,
    /// The type of encryption
    pub encryption_type: EncryptionType,
}

impl EncryptionInfo {
    fn new<R: Read + Seek>(ole: &Ole<R>) -> Result<Self, io::Error> {
        let entry = ole.get_entry_by_name("EncryptionInfo")?;
        let mut stream = ole.get_stream_reader(&entry);
        let version = Version::new(&mut stream)?;
        let encryption_type = if version.is((4, 4)) {
            // Agile Encryption
            EncryptionType::Agile(AgileEncryption::new(&mut stream).map_err(|e| {
                warn!("Failed to parse AgileEncryption: {e}");
                e
            })?)
        } else if version.minor == 3 && [3, 4].contains(&version.major) {
            // Extensible Encryption
            warn!("Extensible Encryption is not supported");
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Extensible Encryption is not supported",
            ));
        } else if version.minor == 2 && [2, 3, 4].contains(&version.major) {
            // Standard Encryption
            let se = StandardEncryption::new(&mut stream).map_err(|e| {
                warn!("Failed to parse StandardEncryption: {e}");
                e
            })?;
            if matches!(se.header.algorithm, EncryptionAlgo::Rc4) {
                warn!("RC4 is not allowed in Standard Encryption");
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "RC4 is not allowed in Standard Encryption",
                ));
            }
            EncryptionType::Standard(se)
        } else {
            warn!("Unsupported/Invalid EncryptionInfo version ({version})");
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsupported/Invalid EncryptionInfo version ({version})"),
            ));
        };
        Ok(Self {
            version,
            encryption_type,
        })
    }
}

/// Interface to Ole cryptography
#[derive(Debug)]
pub struct OleCrypto {
    /// *Data Spaces* (if present and valid)
    pub data_spaces: Option<DataSpaces>,
    /// *Encryption Transform Info* (if present and valid)
    pub transform_info: Option<EncryptionTransformInfo>,
    /// *Encryption Info*
    pub encryption_info: EncryptionInfo,
}

impl OleCrypto {
    /// Parse cryptography data from an Ole object
    pub fn new<R: Read + Seek>(ole: &Ole<R>) -> Result<Self, io::Error> {
        let (data_spaces, transform_info) = match DataSpaces::new(ole) {
            Ok(ds) => match ds.get_encryption_transform_info(ole) {
                Ok(ti) => (Some(ds), Some(ti)),
                Err(e) => {
                    debug!("Failed to parse EncryptionTransformInfo: {e}");
                    (Some(ds), None)
                }
            },
            Err(e) => {
                debug!("Failed to parse DataSapces: {e}");
                (None, None)
            }
        };
        let encryption_info = EncryptionInfo::new(ole).map_err(|e| {
            debug!("Failed to parse EncryptionInfo: {e}");
            e
        })?;
        Ok(Self {
            data_spaces,
            transform_info,
            encryption_info,
        })
    }

    /// Validate the provided password
    ///
    /// Returns the derived key if the password is valid or None otherwise
    pub fn get_key(&self, password: &str) -> Option<OleKey> {
        match self.encryption_info.encryption_type {
            EncryptionType::Standard(ref se) => se.get_key(password),
            EncryptionType::Agile(ref ae) => ae.get_key(password),
        }
    }

    /// Decrypts an encrypted Ole object
    pub fn decrypt<R: Read + Seek, W: Write>(
        self,
        key: &OleKey,
        ole: &Ole<R>,
        writer: W,
    ) -> Result<u64, io::Error> {
        let entry = ole.get_entry_by_name("EncryptedPackage").inspect_err(|e| {
            warn!("EncryptedPackage not found: {}", e);
        })?;
        let mut stream = ole.get_stream_reader(&entry);
        let stream_size = rdu64le(&mut stream)?;
        let decrypt_result = match self.encryption_info.encryption_type {
            EncryptionType::Standard(ref se) => {
                se.decrypt_stream(key, stream_size, &mut stream, writer)
            }
            EncryptionType::Agile(ref ae) => {
                ae.decrypt_stream(key, stream_size, &mut stream, writer)
            }
        };
        if let Err(e) = decrypt_result {
            debug!("Ole decrypt failed: {e}");
            Err(e)
        } else {
            Ok(stream_size)
        }
    }
}

/// Ole decryption key
#[derive(Debug)]
pub struct OleKey {
    key: Vec<u8>,
}

impl OleKey {
    /// Return the key as a slice of bytes
    pub fn as_slice(&self) -> &[u8] {
        self.key.as_slice()
    }
}

impl std::fmt::LowerHex for OleKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        for v in &self.key {
            write!(f, "{:02x}", v)?
        }
        Ok(())
    }
}

impl std::fmt::UpperHex for OleKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        for v in &self.key {
            write!(f, "{:02X}", v)?
        }
        Ok(())
    }
}
