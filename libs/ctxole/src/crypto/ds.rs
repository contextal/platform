//! Ole data spaces and encryption transform
use super::Version;
use crate::Ole;
use ctxutils::io::*;
use std::io::{self, Read, Seek};

fn read_unicode_lpp4<R: Read + Seek>(r: &mut R) -> Result<String, io::Error> {
    let length = rdu32le(r)?;
    if length > 256 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "UTF-16 string too long, giving up",
        ));
    }
    let data_len = usize::try_from(length).unwrap(); // Safe due to previous if
    let mut data = vec![0u8; data_len];
    r.read_exact(&mut data)?;
    let padlen = ((length + 3) & !3) - length;
    r.seek(io::SeekFrom::Current(padlen.into()))?;
    Ok(utf8dec_rs::decode_utf16le_str(&data))
}

fn read_utf8_lpp4<R: Read + Seek>(r: &mut R) -> Result<String, io::Error> {
    let length = rdu32le(r)?;
    if length > 128 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "UTF-8 string too long, giving up",
        ));
    }
    let data_len = usize::try_from(length).unwrap(); // Safe due to previous if
    let mut data = vec![0u8; data_len];
    r.read_exact(&mut data)?;
    let padlen = ((length + 3) & !3) - length;
    r.seek(io::SeekFrom::Current(padlen.into()))?;
    Ok(String::from_utf8_lossy(&data).to_string())
}

/// The version of the data spaces structure
#[derive(Debug)]
pub struct DataSpaceVersionInfo {
    /// The functionality for which the DataSpaceVersionInfo structure specifies version information
    pub feature_identifier: String,
    /// The reader version of the data spaces structure
    pub reader_version: Version,
    /// The updater version of the data spaces structure
    pub updater_version: Version,
    /// The writer version of the data spaces structure
    pub writer_version: Version,
}

impl DataSpaceVersionInfo {
    fn new<R: Read + Seek>(ole: &Ole<R>) -> Result<Self, io::Error> {
        let entry = ole.get_entry_by_name("\u{6}DataSpaces/Version")?;
        let mut stream = ole.get_stream_reader(&entry);
        Ok(Self {
            feature_identifier: read_unicode_lpp4(&mut stream)?,
            reader_version: Version::new(&mut stream)?,
            updater_version: Version::new(&mut stream)?,
            writer_version: Version::new(&mut stream)?,
        })
    }

    fn is_valid(&self) -> bool {
        self.feature_identifier == "Microsoft.Container.DataSpaces"
            && self.reader_version.is((1, 0))
            && self.updater_version.is((1, 0))
            && self.writer_version.is((1, 0))
    }
}

/// The name of a specific storage or stream containing protected content
#[derive(Debug)]
pub struct DataSpaceReferenceComponent {
    /// Specifies whether the referenced component is a stream (0) or storage (1)
    pub ref_type: u32,
    /// The name of the stream
    pub ref_name: String,
}

impl DataSpaceReferenceComponent {
    fn new<R: Read + Seek>(r: &mut R) -> Result<Self, io::Error> {
        Ok(Self {
            ref_type: rdu32le(r)?,
            ref_name: read_unicode_lpp4(r)?,
        })
    }
}

/// Associates protected content with a specific data space definition
#[derive(Debug)]
pub struct DataSpaceMapEntry {
    /// Storage and stream containing protected content
    pub components: Vec<DataSpaceReferenceComponent>,
    /// The name of the data space definition associated with the protected content
    pub name: String,
    /// The transformations to apply to the protected content
    pub transforms: Vec<String>,
}

impl DataSpaceMapEntry {
    fn new<R: Read + Seek>(r: &mut R) -> Result<Self, io::Error> {
        let length = rdu32le(r)?;
        if length < 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "DataSpaceMapEntry underflow",
            ));
        }
        let mut r = SeekTake::new(r, length.into());
        let n_components = rdu32le(&mut r)?;
        if n_components > 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Too many DataSpaceMapEntry components, giving up",
            ));
        }
        let mut components: Vec<DataSpaceReferenceComponent> =
            Vec::with_capacity(n_components.try_into().unwrap()); // Safe due to previous if
        for _ in 0..n_components {
            components.push(DataSpaceReferenceComponent::new(&mut r)?);
        }
        let name = read_unicode_lpp4(&mut r)?;
        Ok(Self {
            components,
            name,
            transforms: Vec::new(),
        })
    }
}

/// Data space transformation map
#[derive(Debug)]
pub struct DataSpaceMap {
    /// The transformations to apply to the protected content
    pub entries: Vec<DataSpaceMapEntry>,
}

impl DataSpaceMap {
    fn new<R: Read + Seek>(ole: &Ole<R>) -> Result<Self, io::Error> {
        let entry = ole.get_entry_by_name("\u{6}DataSpaces/DataSpaceMap")?;
        let mut stream = ole.get_stream_reader(&entry);
        let header_len = rdu32le(&mut stream)?;
        if header_len != 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid DataSpaceMap header lenght ({header_len})"),
            ));
        }
        let n_entries = rdu32le(&mut stream)?;
        if n_entries > 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Too many DataSpaceMap entries, giving up",
            ));
        }
        let mut entries: Vec<DataSpaceMapEntry> = Vec::with_capacity(n_entries.try_into().unwrap()); // Safe due to previous if
        for _ in 0..n_entries {
            entries.push(DataSpaceMapEntry::new(&mut stream)?);
        }
        Ok(Self { entries })
    }
}

#[derive(Debug)]
struct DataSpaceDefinition(Vec<String>);

impl DataSpaceDefinition {
    fn new<R: Read + Seek>(ole: &Ole<R>, name: &str) -> Result<Self, io::Error> {
        let entry = ole.get_entry_by_name(&format!("\u{6}DataSpaces/DataSpaceInfo/{name}"))?;
        let mut stream = ole.get_stream_reader(&entry);
        let header_len = rdu32le(&mut stream)?;
        if header_len != 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid DataSpaceMap header lenght ({header_len})"),
            ));
        }
        let n_trans = rdu32le(&mut stream)?;
        if n_trans > 16 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Too many DataSpaceDefinition transforms, giving up",
            ));
        }
        let mut trans: Vec<String> = Vec::with_capacity(n_trans.try_into().unwrap()); // Safe due to previous if
        for _ in 0..n_trans {
            trans.push(read_unicode_lpp4(&mut stream)?);
        }
        Ok(Self(trans))
    }
}

/// The identity of a transform
#[derive(Debug)]
pub struct TransformInfoHeader {
    /// The type of transform to be applied
    pub transform_type: u32,
    /// An identifier associated with a specific transform
    pub transform_id: String,
    /// The friendly name of the transform
    pub transform_name: String,
    /// The reader version
    pub reader_version: Version,
    /// The updater version
    pub updater_version: Version,
    /// The writer version
    pub writer_version: Version,
}

impl TransformInfoHeader {
    fn new<R: Read + Seek>(r: &mut R) -> Result<Self, io::Error> {
        let _len = rdu32le(r)?;
        Ok(Self {
            transform_type: rdu32le(r)?,
            transform_id: read_unicode_lpp4(r)?,
            transform_name: read_unicode_lpp4(r)?,
            reader_version: Version::new(r)?,
            updater_version: Version::new(r)?,
            writer_version: Version::new(r)?,
        })
    }
}

/// Specifies the encryption used (informational only!!!)
#[derive(Debug)]
pub struct EncryptionTransformInfo {
    /// Transform header
    pub header: TransformInfoHeader,
    /// The name of the encryption algorithm
    pub name: String,
    /// The block size for the encryption algorithm
    pub block_size: u32,
    /// Extensible encryption mode
    pub cypher_mode: u32,
    /// Reserved (must be 4)
    pub reserved: u32,
}

impl EncryptionTransformInfo {
    fn new<R: Read + Seek>(ole: &Ole<R>, name: &str) -> Result<Self, io::Error> {
        let entry = ole.get_entry_by_name(&format!(
            "\u{6}DataSpaces/TransformInfo/{name}/\u{6}Primary"
        ))?;
        let mut stream = ole.get_stream_reader(&entry);
        let header = TransformInfoHeader::new(&mut stream)?;
        if header.transform_type != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid TransformType ({})", header.transform_type),
            ));
        }
        if header.transform_id != "{FF9A3F03-56EF-4613-BDD5-5A41C1D07246}" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid TransformID ({})", header.transform_id),
            ));
        }
        if header.transform_name != "Microsoft.Container.EncryptionTransform" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid TransformName ({})", header.transform_name),
            ));
        }
        if !header.reader_version.is((1, 0)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid ReaderVersion ({})", header.reader_version),
            ));
        }
        if !header.updater_version.is((1, 0)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid UpdaterVersion ({})", header.updater_version),
            ));
        }
        if !header.writer_version.is((1, 0)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid WriterVersion ({})", header.writer_version),
            ));
        }
        // FIXME: there should be an IRMDSTransformInfo here but it's not there
        Ok(Self {
            header,
            name: read_utf8_lpp4(&mut stream)?,
            block_size: rdu32le(&mut stream)?,
            cypher_mode: rdu32le(&mut stream)?,
            reserved: rdu32le(&mut stream)?,
        })
    }
}

/// Ole data spaces
#[derive(Debug)]
pub struct DataSpaces {
    /// The versiod on the data spaces
    pub version_info: DataSpaceVersionInfo,
    /// The data space map
    pub map: DataSpaceMap,
}

impl DataSpaces {
    /// Parse and return Ole data spaces
    pub fn new<R: Read + Seek>(ole: &Ole<R>) -> Result<Self, io::Error> {
        let version_info = DataSpaceVersionInfo::new(ole)?;
        let mut map = DataSpaceMap::new(ole)?;
        for e in &mut map.entries {
            let dsd = DataSpaceDefinition::new(ole, &e.name)?;
            e.transforms = dsd.0;
        }
        Ok(Self { version_info, map })
    }

    /// Returns the encryption transform
    pub(super) fn get_encryption_transform_info<R: Read + Seek>(
        &self,
        ole: &Ole<R>,
    ) -> Result<EncryptionTransformInfo, io::Error> {
        if !self.version_info.is_valid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid DataSpaceVersionInfo",
            ));
        }
        if self.map.entries.len() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid DataSpaceMap content({} entries are present)",
                    self.map.entries.len()
                ),
            ));
        }
        let dsme = &self.map.entries[0];
        if dsme.name != "StrongEncryptionDataSpace" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid DataSpaceName ({})", dsme.name),
            ));
        }
        if dsme.components.len() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid ReferenceComponents content({} components are present)",
                    self.map.entries.len()
                ),
            ));
        }
        let component = &dsme.components[0];
        if component.ref_type != 0 || component.ref_name != "EncryptedPackage" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid ReferenceComponents content(found {} with type {})",
                    component.ref_name, component.ref_type
                ),
            ));
        }
        let transform = &dsme.transforms[0];
        if transform != "StrongEncryptionTransform" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid TransformReference({})", transform),
            ));
        }
        EncryptionTransformInfo::new(ole, transform)
    }
}
