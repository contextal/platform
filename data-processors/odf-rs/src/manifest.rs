// Based on schema
// https://docs.oasis-open.org/office/OpenDocument/v1.3/os/schemas/OpenDocument-v1.3-manifest-schema-rng.html

use crate::OdfError;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::io::BufRead;

impl Manifest {
    pub fn load_from_xml_parser<R: BufRead>(r: R) -> Result<Manifest, OdfError> {
        let manifest: Manifest = quick_xml::de::from_reader(r)?;
        Ok(manifest)
    }
}

#[derive(Deserialize, Debug)]
pub struct Manifest {
    #[serde(rename = "@xmlns:manifest")]
    pub manifest: Option<String>,
    #[serde(rename = "@version")]
    pub version: Option<String>,
    #[serde(rename = "file-entry")]
    pub file_entry: Option<Vec<FileEntry>>,
    #[serde(rename = "encrypted-key")]
    pub encrypted_key: Option<Vec<EncryptedKey>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntry {
    #[serde(rename(deserialize = "@full-path"))]
    pub full_path: String,
    #[serde(rename(deserialize = "@media-type"))]
    pub media_type: String,
    #[serde(rename(deserialize = "@preferred-view-mode"))]
    pub preferred_view_mode: Option<String>,
    #[serde(rename(deserialize = "@size"))]
    pub size: Option<u64>,
    #[serde(rename(deserialize = "@version"))]
    pub version: Option<String>,
    #[serde(rename(deserialize = "encryption-data"))]
    pub encryption_data: Option<EncryptionData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptionData {
    #[serde(rename(deserialize = "@checksum"))]
    pub checksum: String,
    #[serde(rename(deserialize = "@checksum-type"))]
    pub checksum_type: String,
    #[serde(rename(deserialize = "algorithm"))]
    pub algorithm: Algorithm,
    #[serde(rename(deserialize = "key-derivation"))]
    pub key_derevation: KeyDerevation,
    #[serde(rename(deserialize = "start-key-generation"))]
    pub start_key_generation: Option<StartKeyGeneration>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Algorithm {
    #[serde(rename(deserialize = "@algorithm-name"))]
    pub name: String,
    #[serde(rename(deserialize = "@initialisation-vector"))]
    pub initialisation_vector: String,
    // TODO: Investigate how to represent children
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StartKeyGeneration {
    #[serde(rename(deserialize = "@start-key-generation-name"))]
    pub name: String,
    #[serde(rename(deserialize = "@key-size"))]
    pub size: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeyDerevation {
    #[serde(rename(deserialize = "@iteration-count"))]
    pub iteration_count: u32,
    #[serde(rename(deserialize = "@key-derivation-name"))]
    pub name: String,
    #[serde(rename(deserialize = "@key-size"))]
    pub size: u32,
    #[serde(rename(deserialize = "@salt"))]
    pub salt: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedKey {
    #[serde(rename(deserialize = "CipherData"))]
    pub cipher_data: CipherData,
    #[serde(rename(deserialize = "encryption-method"))]
    pub encryption_method: Option<EncryptionMethod>,
    #[serde(rename(deserialize = "keyinfo"))]
    pub keyinfo: Keyinfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptionMethod {
    #[serde(rename(deserialize = "@PGPAlgorithm"))]
    pub pga_algorithm: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Keyinfo {
    #[serde(rename(deserialize = "PGPData"))]
    pub pga_algorithm: PGPData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PGPData {
    #[serde(rename(deserialize = "PGPKeyID"))]
    pub key_id: PGPKeyID,
    #[serde(rename(deserialize = "PGPKeyPacket"))]
    pub key_packet: PGPKeyPacket,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PGPKeyID {
    #[serde(rename(deserialize = "$text"))]
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PGPKeyPacket {
    #[serde(rename(deserialize = "$text"))]
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CipherData {
    #[serde(rename(deserialize = "CipherValue"))]
    pub value: CipherValue,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CipherValue {
    #[serde(rename(deserialize = "$text"))]
    pub data: String,
}
