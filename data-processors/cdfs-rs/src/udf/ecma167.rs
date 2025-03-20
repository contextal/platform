//! ECMA 1.6.7 and OSTA UDF 2.60 structures
//!
//! A handful of parsed structures useful in extracting or inspecting UDF images
//!
//! The paragraph number in the specifications is indicated in parentheses

use ctxutils::io::{rdu8, rdu16le, rdu32le, rdu64le};
use std::fmt::{Debug, Display};
use std::io::{Read, Seek};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};

/// Crc calculator for Descriptor Tags
const TAG_CRC: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_XMODEM);

#[derive(Debug, Clone)]
/// Descriptor tag (7.2)
pub struct DescriptorTag {
    /// Tag Identifier
    pub identifier: u16,
    /// Descriptor Version
    pub version: u16,
    /// Tag Serial Number
    pub serial_number: u16,
    /// Descriptor CRC
    pub crc: u16,
    /// Descriptor CRC Length
    pub crclen: usize,
    /// Tag Location
    pub lba: u32,
}

impl DescriptorTag {
    /*
    * 1 => Primary Volume Descriptor (3/10.1)
    * 2 => Anchor Volume Descriptor Pointer (3/10.2)
    * 3 => Volume Descriptor Pointer (3/10.3)
    * 4 => Implementation Use Volume Descriptor (3/10.4)
    * 5 => Partition Descriptor (3/10.5)
    * 6 => Logical Volume Descriptor (3/10.6)
    * 7 => Unallocated Space Descriptor (3/10.8)
    * 8 => Terminating Descriptor (3/10.9 and 4/14.2)
    * 9 => Logical Volume Integrity Descriptor (3/10.10)

    * 256 => File Set Descriptor (4/14.1)
    * 257 => File Identifier Descriptor (4/14.4)
    * 258 => Allocation Extent Descriptor (4/14.5)
    * 259 => Indirect Entry (4/14.7)
    * 260 => Terminal Entry (4/14.8)
    * 261 => File Entry (4/14.9)
    * 262 => Extended Attribute Header Descriptor (4/14.10.1)
    * 263 => Unallocated Space Entry (4/14.11)
    * 264 => Space Bitmap Descriptor (4/14.12)
    * 265 => Partition Integrity Entry (4/14.13)
    * 266 => Extended File Entry (4/14.17)
     */
    #[instrument(skip_all)]
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        let mut buf = [0u8; 16];
        r.read_exact(&mut buf)?;
        let computed_sum = buf[0]
            .wrapping_add(buf[1])
            .wrapping_add(buf[2])
            .wrapping_add(buf[3])
            .wrapping_add(buf[5])
            .wrapping_add(buf[6])
            .wrapping_add(buf[7])
            .wrapping_add(buf[8])
            .wrapping_add(buf[9])
            .wrapping_add(buf[10])
            .wrapping_add(buf[11])
            .wrapping_add(buf[12])
            .wrapping_add(buf[13])
            .wrapping_add(buf[14])
            .wrapping_add(buf[15]);
        let br = &mut buf.as_slice();
        let identifier = rdu16le(br)?;
        let version = rdu16le(br)?;
        let chksum = rdu8(br)?;
        if chksum != computed_sum {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Tag (id: {identifier}) checksum mismatch"),
            ));
        }
        let rsvd = rdu8(br)?;
        if rsvd != 0 {
            warn!("Found tag (id: {identifier}) with bad reserved byte");
        }
        let serial_number = rdu16le(br)?;
        let crc = rdu16le(br)?;
        let crclen = usize::from(rdu16le(br)?);
        let lba = rdu32le(br)?;
        Ok(Self {
            identifier,
            version,
            serial_number,
            crc,
            crclen,
            lba,
        })
    }
}

struct TagReader<'a, R: Read> {
    r: &'a mut R,
    tag: DescriptorTag,
    crc_todo: usize,
    crc: Option<crc::Digest<'static, u16>>,
    limit: Option<u64>,
    crc_ok: bool,
}

impl<'a, R: Read> TagReader<'a, R> {
    fn new(r: &'a mut R) -> Result<Self, std::io::Error> {
        let tag = DescriptorTag::new(r)?;
        let crc_todo = tag.crclen;
        Ok(Self {
            r,
            tag,
            crc_todo,
            crc: Some(TAG_CRC.digest()),
            limit: None,
            crc_ok: false,
        })
    }

    fn new_with_limit(r: &'a mut R, limit: u64) -> Result<Self, std::io::Error> {
        Ok(Self {
            limit: Some(limit),
            ..Self::new(r)?
        })
    }

    fn verify_crc(&mut self) -> bool {
        while self.crc_todo > 0 {
            if std::io::copy(
                &mut self.take(self.crc_todo.try_into().unwrap_or(u64::MAX)),
                &mut std::io::sink(),
            )
            .is_err()
            {
                return false;
            }
        }
        self.crc_ok
    }

    fn set_limit(&mut self, limit: u64) {
        self.limit = Some(limit);
    }
}

impl<'a, R: Read> Read for TagReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let ret = self.r.read(buf);
        if let Ok(size) = ret {
            if let Some(mut crc) = self.crc.take() {
                let size = if let Some(limit) = self.limit {
                    size.min(self.crc_todo)
                        .min(limit.try_into().unwrap_or(usize::MAX))
                } else {
                    size.min(self.crc_todo)
                };
                crc.update(&buf[0..size]);
                self.crc_todo -= size;
                if self.crc_todo == 0 {
                    if crc.finalize() != self.tag.crc {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Tag checksum error",
                        ));
                    } else {
                        self.crc_ok = true;
                    }
                } else {
                    self.crc = Some(crc);
                }
            }
        }
        ret
    }
}

#[derive(Debug)]
/// Extent Descriptor (7.1 / 14.14.1)
pub struct ExtentAD {
    /// Extent Length
    pub length: u32,
    /// Extent Location
    pub lba: u32,
}

impl ExtentAD {
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        Ok(Self {
            length: rdu32le(r)?,
            lba: rdu32le(r)?,
        })
    }
}

#[derive(Debug, Clone)]
/// Long Allocation Descriptor (14.14.2)
pub struct LongAD {
    /// Extent Length
    pub length: u32,
    /// Extent Location
    pub lba: u32,
    /// Partition Reference Number
    pub part_num: u16,
    /// Flags (UDF 2.3.10.1)
    pub flags: u16,
    /// Implementation Use
    pub impl_use: [u8; 4],
}

impl LongAD {
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        let length = rdu32le(r)?;
        let lba = rdu32le(r)?;
        let part_num = rdu16le(r)?;
        let flags = rdu16le(r)?;
        let mut impl_use = [0u8; 4];
        r.read_exact(&mut impl_use)?;
        Ok(Self {
            length,
            lba,
            part_num,
            flags,
            impl_use,
        })
    }

    fn from_short(short: ExtentAD, part_num: u16) -> Self {
        Self {
            length: short.length,
            lba: short.lba,
            part_num,
            flags: 0,
            impl_use: [0; 4],
        }
    }

    fn from_extended(extended: ExtendedAD) -> Self {
        Self {
            length: extended.length, // FIXME: use recorded_length ??
            lba: extended.lba,
            part_num: extended.part_num,
            flags: extended.flags,
            impl_use: extended.impl_use,
        }
    }

    pub(crate) fn ad_is_continuation(&self) -> bool {
        self.length & 0xc0000000 == 0xc0000000
    }

    pub(crate) fn ad_is_recorded(&self) -> bool {
        self.length & 0xc0000000 == 0
    }

    pub(crate) fn ad_unmasked_length(&self) -> u32 {
        self.length & !0xc0000000
    }
}

// 14.14.3
#[derive(Debug)]
/// Extended Allocation Descriptor
pub struct ExtendedAD {
    /// Extent Length
    pub length: u32,
    /// Recorded Length
    pub recorded_length: u32,
    /// Information Length
    pub information_length: u32,
    /// Extent Location
    pub lba: u32,
    /// Partition Reference Number
    pub part_num: u16,
    /// Flags (UDF 2.3.10.1)
    pub flags: u16,
    /// Implementation Use
    pub impl_use: [u8; 4],
}

impl ExtendedAD {
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        let length = rdu32le(r)?;
        let recorded_length = rdu32le(r)?;
        let information_length = rdu32le(r)?;
        let lba = rdu32le(r)?;
        let part_num = rdu16le(r)?;
        let flags = rdu16le(r)?;
        let mut impl_use = [0u8; 4];
        r.read_exact(&mut impl_use)?;
        Ok(Self {
            length,
            recorded_length,
            information_length,
            lba,
            part_num,
            flags,
            impl_use,
        })
    }
}

#[derive(Debug)]
/// Anchor Volume Descriptor Pointer (10.2)
pub struct AnchorVolumeDescriptorPointer {
    /// Main Volume Descriptor Sequence Extent
    pub main: ExtentAD,
    /// Reserve Volume Descriptor Sequence Extent
    pub reserve: ExtentAD,
}

impl AnchorVolumeDescriptorPointer {
    #[instrument(skip(r))]
    pub(crate) fn new<R: Read>(r: &mut R, lba: u32) -> Result<Self, std::io::Error> {
        const MYLEN: u64 = 512 - 16;
        let mut tr = TagReader::new_with_limit(r, MYLEN)?;
        if tr.tag.identifier != 2 || tr.tag.lba != lba {
            debug!(
                "Invalid Anchor Volume Descriptor Pointer tag(id {}, lba {}:{})",
                tr.tag.identifier, tr.tag.lba, lba
            );
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid Anchor Volume Descriptor Pointer: incorrect tag",
            ));
        }
        let res = Ok(Self {
            main: ExtentAD::new(&mut tr)?,
            reserve: ExtentAD::new(&mut tr)?,
        });
        if !tr.verify_crc() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid Anchor Volume Descriptor Pointer: crc mismatch",
            ));
        }
        res
    }
}

#[derive(PartialEq, Eq)]
/// Character set specification (7.2.1)
pub struct CharSpec {
    /// Character Set Type
    pub cset_type: u8,
    /// Character Set Information
    pub cset_info: [u8; 63],
}

impl CharSpec {
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        let cset_type = rdu8(r)?;
        let mut cset_info = [0u8; 63];
        r.read_exact(&mut cset_info)?;
        Ok(Self {
            cset_type,
            cset_info,
        })
    }

    /// Checks if the defined character set is CS0 (7.2.2)
    pub fn is_osta_cs0(&self) -> bool {
        self.cset_type == 0 && &self.cset_info == b"OSTA Compressed Unicode\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"
    }
}

impl Debug for CharSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut x = f.debug_struct("CharSpec");
        x.field("cset_type", &self.cset_type);
        if let Ok(s) = String::from_utf8(self.cset_info.to_vec()) {
            x.field("cset_info", &s);
        } else {
            x.field("cset_info", &self.cset_info);
        }
        x.finish()
    }
}

#[derive(PartialEq, Eq)]
/// Fixed-length character fields (7.2.12)
pub struct Dstring(String);
impl Dstring {
    fn new(buf: &[u8]) -> Self {
        assert!(
            buf.len() >= 2,
            "Internal error, Dstring with insufficient length"
        );
        let compid = buf[0];
        let mut len = usize::from(buf[buf.len() - 1]);
        if len == 0 {
            // The length of a dstring includes the compression code byte (2.1.1) except for the
            // case of a zero length string. A zero length string shall be recorded by setting the
            // entire dstring field to all zeros.
            return Self(String::new());
        }
        len -= 1; // The length of a dstring includes the compression code byte
        let buf = &buf[1..(buf.len() - 1)];
        if len > buf.len() {
            warn!("Overflowing Dstring truncated");
            len = buf.len();
        }
        let chars = &buf[0..len];
        Self::decode_dchars(chars, compid)
    }

    fn new_nolen(buf: &[u8]) -> Self {
        if buf.is_empty() {
            return Self(String::new());
        }
        let compid = buf[0];
        let chars = &buf[1..];
        Self::decode_dchars(chars, compid)
    }

    fn decode_dchars(chars: &[u8], compid: u8) -> Self {
        Self(if compid == 8 {
            char::decode_utf16(chars.iter().map(|b| u16::from(*b)))
                .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect::<String>()
        } else if compid == 16 {
            char::decode_utf16(
                chars
                    .chunks_exact(2)
                    .map(|word| u16::from_be_bytes(word.try_into().unwrap())), // safe bc chunks_exact
            )
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect::<String>()
        } else {
            warn!("Invalid Dstring compression id {compid}");
            String::new()
        })
    }
}

impl Debug for Dstring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl Display for Dstring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        std::fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Clone)]
/// Entity identifier (7.4)
pub struct EntityId {
    /// Flags
    pub flags: u8,
    /// Identifier
    pub identifier: [u8; 23],
    /// Identifier Suffix
    pub suffix: [u8; 8],
}

impl EntityId {
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        let flags = rdu8(r)?;
        let mut identifier = [0u8; 23];
        r.read_exact(&mut identifier)?;
        let mut suffix = [0u8; 8];
        r.read_exact(&mut suffix)?;
        Ok(Self {
            flags,
            identifier,
            suffix,
        })
    }

    /// Checks if the entity falls within the *OSTA UDF Compliant* domain
    pub fn is_osta_udf_compliant(&self) -> bool {
        self.flags == 0 && &self.identifier == b"*OSTA UDF Compliant\x00\x00\x00\x00"
    }

    /// Return the identifier as a lossy string
    pub fn lossy_identifier(&self) -> String {
        String::from_utf8_lossy(&self.identifier)
            .trim_end_matches('\0')
            .to_string()
    }
}

impl Debug for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut x = f.debug_struct("EntityId");
        x.field("flags", &self.flags);
        if let Ok(s) = String::from_utf8(self.identifier.to_vec()) {
            x.field("identifier", &s);
        } else {
            x.field("identifier", &self.identifier);
        }
        if let Ok(s) = String::from_utf8(self.suffix.to_vec()) {
            x.field("suffix", &s);
        } else {
            x.field("suffix", &self.suffix);
        }
        x.finish()
    }
}

trait Sequenced<Rhs = Self> {
    fn is_equivalent_to(&self, other: &Rhs) -> bool;
    fn prevails_over(&self, other: &Rhs) -> bool;
}

#[derive(Debug)]
/// Primary Volume Descriptor (10.1)
pub struct PrimaryVolumeDescriptor {
    /// Volume Descriptor Sequence Number
    pub desc_sequence_number: u32,
    /// Primary Volume Descriptor Number
    pub number: u32,
    /// Volume Identifier
    pub identifier: Dstring,
    /// Volume Sequence Number
    pub sequence_number: u16,
    /// Maximum Volume Sequence Number
    pub max_sequence_number: u16,
    /// Interchange Level
    pub interchange_level: u16,
    /// Maximum Interchange Level
    pub max_interchange_level: u16,
    /// Character Set List
    pub charset_list: u32,
    /// Maximum Character Set List
    pub max_charset_list: u32,
    /// Volume Set Identifier
    pub set_identifier: Dstring,
    /// Descriptor Character Set
    pub desc_charset: CharSpec,
    /// Explanatory Character Set
    pub expl_desc_charset: CharSpec,
    /// Volume Abstract
    pub vol_abstract: ExtentAD,
    /// Volume Copyright Notice
    pub copyright_notice: ExtentAD,
    /// Application Identifier
    pub app_identifier: EntityId,
    /// Recording Date and Time
    pub datetime: UdfDate,
    /// Implementation Identifier
    pub impl_identifier: EntityId,
    /// Implementation Use
    pub impl_use: [u8; 64],
    /// Predecessor Volume Descriptor Sequence Location
    pub predecessor_seq_location: u32,
    /// Flags
    pub flags: u16,
}

impl PrimaryVolumeDescriptor {
    #[instrument(skip_all)]
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        let desc_sequence_number = rdu32le(r)?;
        let number = rdu32le(r)?;
        let mut buf = [0u8; 32];
        r.read_exact(&mut buf)?;
        let identifier = Dstring::new(&buf);
        let sequence_number = rdu16le(r)?;
        let max_sequence_number = rdu16le(r)?;
        let interchange_level = rdu16le(r)?;
        let max_interchange_level = rdu16le(r)?;
        let charset_list = rdu32le(r)?;
        let max_charset_list = rdu32le(r)?;
        let mut buf = [0u8; 128];
        r.read_exact(&mut buf)?;
        let set_identifier = Dstring::new(&buf);
        let desc_charset = CharSpec::new(r)?;
        let expl_desc_charset = CharSpec::new(r)?;
        let vol_abstract = ExtentAD::new(r)?;
        let copyright_notice = ExtentAD::new(r)?;
        let app_identifier = EntityId::new(r)?;
        let datetime = UdfDate::new(r)?;
        let impl_identifier = EntityId::new(r)?;
        let mut impl_use = [0u8; 64];
        r.read_exact(&mut impl_use)?;
        let predecessor_seq_location = rdu32le(r)?;
        let flags = rdu16le(r)?;
        Ok(Self {
            desc_sequence_number,
            number,
            identifier,
            sequence_number,
            max_sequence_number,
            interchange_level,
            max_interchange_level,
            charset_list,
            max_charset_list,
            set_identifier,
            desc_charset,
            expl_desc_charset,
            vol_abstract,
            copyright_notice,
            app_identifier,
            datetime,
            impl_identifier,
            impl_use,
            predecessor_seq_location,
            flags,
        })
    }
}

impl Sequenced for PrimaryVolumeDescriptor {
    fn is_equivalent_to(&self, other: &PrimaryVolumeDescriptor) -> bool {
        self.identifier == other.identifier
            && self.set_identifier == other.set_identifier
            && self.desc_charset == other.desc_charset
    }
    fn prevails_over(&self, other: &PrimaryVolumeDescriptor) -> bool {
        self.desc_sequence_number > other.desc_sequence_number
    }
}

#[derive(Debug)]
/// Partition map (10.7)
pub struct PartitionMap {
    /// A convenience index into the Partition Descriptor list
    pub partition_index: Option<usize>,
    /// Volume Sequence Number
    pub volume_sequence_number: u16,
    /// Partition Number
    pub partition_number: u16,
    /// Partition Type Identifier (Partition type 2 only)
    pub partition_type_id: Option<EntityId>,
    /// Type specific unparsed data (Partition type 2 only)
    pub data: Option<[u8; 24]>,
}

impl PartitionMap {
    #[instrument(skip_all)]
    fn new<R: Read>(r: &mut R) -> Result<Option<Self>, std::io::Error> {
        let pmtype = rdu8(r)?;
        let len = rdu8(r)?;
        if len < 4 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("PartitionMap with invalid len ({len})"),
            ));
        }
        if pmtype == 1 && len == 6 {
            return Ok(Some(Self {
                partition_index: None,
                volume_sequence_number: rdu16le(r)?,
                partition_number: rdu16le(r)?,
                partition_type_id: None,
                data: None,
            }));
        }
        if pmtype == 2 && len == 64 {
            let _rsvd1 = rdu16le(r)?;
            let partition_type_id = Some(EntityId::new(r)?);
            let volume_sequence_number = rdu16le(r)?;
            let partition_number = rdu16le(r)?;
            let mut data = [0u8; 24];
            r.read_exact(&mut data)?;
            return Ok(Some(Self {
                partition_index: None,
                volume_sequence_number,
                partition_number,
                partition_type_id,
                data: Some(data),
            }));
        }
        warn!("Skipping weird partition type ({pmtype}) len ({len})");
        std::io::copy(&mut r.take(len.into()), &mut std::io::sink())?;
        Ok(None)
    }

    /// Builds a convenience index into the Partition Descriptor list
    pub(crate) fn set_partition_index(&mut self, pds: &[PartitionDescriptor]) {
        self.partition_index = pds
            .iter()
            .position(|p| p.partition_number == self.partition_number);
    }
}

#[derive(Debug)]
/// Logical Volume Descriptor (10.6)
pub struct LogicalVolumeDescriptor {
    /// Volume Descriptor Sequence Number
    pub desc_sequence_number: u32,
    /// Descriptor Character Set
    pub desc_charset: CharSpec,
    /// Logical Volume Identifier
    pub identifier: Dstring,
    /// Logical Block Size
    pub block_size: u32,
    /// Domain Identifier
    pub domain_identifier: EntityId,
    /// Root File Set Descriptor
    pub root_desc: LongAD,
    /// Map Table Length
    map_table_length: u32,
    /// Number of Partition Maps
    n_partition_maps: u32,
    /// Implementation Identifier
    pub impl_identifier: EntityId,
    /// Implementation Use
    pub impl_use: [u8; 128],
    /// Integrity Sequence Extent
    pub integrity_seq_extent: ExtentAD,
    /// Partition Maps
    pub partition_maps: Vec<Option<PartitionMap>>,
}

impl LogicalVolumeDescriptor {
    #[instrument(skip_all)]
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        let desc_sequence_number = rdu32le(r)?;
        let desc_charset = CharSpec::new(r)?;
        let mut buf = [0u8; 128];
        r.read_exact(&mut buf)?;
        let identifier = Dstring::new(&buf);
        let block_size = rdu32le(r)?;
        let domain_identifier = EntityId::new(r)?;
        let root_desc = LongAD::new(r)?;
        let map_table_length = rdu32le(r)?;
        let n_partition_maps = rdu32le(r)?;
        let impl_identifier = EntityId::new(r)?;
        let mut impl_use = [0u8; 128];
        r.read_exact(&mut impl_use)?;
        let integrity_seq_extent = ExtentAD::new(r)?;
        Ok(Self {
            desc_sequence_number,
            desc_charset,
            identifier,
            block_size,
            domain_identifier,
            root_desc,
            map_table_length,
            n_partition_maps,
            impl_identifier,
            impl_use,
            integrity_seq_extent,
            partition_maps: Vec::new(),
        })
    }

    /// Checks if the LVD is UDF compliant
    pub fn is_compliant(&self) -> bool {
        &self.domain_identifier.identifier == b"*OSTA UDF Compliant\0\0\0\0"
    }
}

impl Sequenced for LogicalVolumeDescriptor {
    fn is_equivalent_to(&self, other: &LogicalVolumeDescriptor) -> bool {
        self.identifier == other.identifier && self.desc_charset == other.desc_charset
    }
    fn prevails_over(&self, other: &LogicalVolumeDescriptor) -> bool {
        self.desc_sequence_number > other.desc_sequence_number
    }
}

#[derive(Debug)]
/// Partition Descriptor (10.5)
pub struct PartitionDescriptor {
    /// Volume Descriptor Sequence Number
    pub desc_sequence_number: u32,
    /// Partition Flags
    pub flags: u16,
    /// Partition Number
    pub partition_number: u16,
    /// Partition Contents
    pub partition_contents: EntityId,
    /// Partition Contents Use
    pub partition_contents_use: [u8; 128],
    /// Access Type
    pub access_type: u32,
    /// Partition Starting Location
    pub partition_starting_location: u32,
    /// Partition Length
    pub partition_length: u32,
    /// Implementation Identifier
    pub impl_identifier: EntityId,
    /// Implementation Use
    pub impl_use: [u8; 128],
}

impl PartitionDescriptor {
    #[instrument(skip_all)]
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        let desc_sequence_number = rdu32le(r)?;
        let flags = rdu16le(r)?;
        let partition_number = rdu16le(r)?;
        let partition_contents = EntityId::new(r)?;
        let mut partition_contents_use = [0u8; 128];
        r.read_exact(&mut partition_contents_use)?;
        let access_type = rdu32le(r)?;
        let partition_starting_location = rdu32le(r)?;
        let partition_length = rdu32le(r)?;
        let impl_identifier = EntityId::new(r)?;
        let mut impl_use = [0u8; 128];
        r.read_exact(&mut impl_use)?;
        Ok(Self {
            desc_sequence_number,
            flags,
            partition_number,
            partition_contents,
            partition_contents_use,
            access_type,
            partition_starting_location,
            partition_length,
            impl_identifier,
            impl_use,
        })
    }
}

impl Sequenced for PartitionDescriptor {
    fn is_equivalent_to(&self, other: &PartitionDescriptor) -> bool {
        self.partition_number == other.partition_number
    }
    fn prevails_over(&self, other: &PartitionDescriptor) -> bool {
        self.desc_sequence_number > other.desc_sequence_number
    }
}

#[derive(Debug)]
/// Implementation Use Volume Descriptor (10.4)
pub struct ImplementationUseVolumeDescriptor {
    /// Volume Descriptor Sequence Number
    pub desc_sequence_number: u32,
    /// Implementation Identifier
    pub impl_identifier: EntityId,
    /// LVICharset (UDF 2.2.7.2.1)
    pub lv_charset: CharSpec,
    /// LogicalVolumeIdentifier (UDF 2.2.7.2.2)
    pub lv_identifier: Dstring,
    /// LVInfo1 (UDF 2.2.7.2.3)
    pub lv_info1: Dstring,
    /// LVInfo2 (UDF 2.2.7.2.3)
    pub lv_info2: Dstring,
    /// LVInfo3 (UDF 2.2.7.2.3)
    pub lv_info3: Dstring,
    /// ImplementationID (UDF 2.2.7.2.4)
    pub lv_impl_identifier: EntityId,
    /// ImplementationUse (UDF 2.2.7.2.5)
    pub lv_impl_use: [u8; 128],
}

impl ImplementationUseVolumeDescriptor {
    #[instrument(skip_all)]
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        let desc_sequence_number = rdu32le(r)?;
        let impl_identifier = EntityId::new(r)?;
        let lv_charset = CharSpec::new(r)?;
        let mut buf = [0u8; 128];
        r.read_exact(&mut buf)?;
        let lv_identifier = Dstring::new(&buf);
        let mut buf = [0u8; 36];
        r.read_exact(&mut buf)?;
        let lv_info1 = Dstring::new(&buf);
        r.read_exact(&mut buf)?;
        let lv_info2 = Dstring::new(&buf);
        r.read_exact(&mut buf)?;
        let lv_info3 = Dstring::new(&buf);
        let lv_impl_identifier = EntityId::new(r)?;
        let mut lv_impl_use = [0u8; 128];
        r.read_exact(&mut lv_impl_use)?;
        Ok(Self {
            desc_sequence_number,
            impl_identifier,
            lv_charset,
            lv_identifier,
            lv_info1,
            lv_info2,
            lv_info3,
            lv_impl_identifier,
            lv_impl_use,
        })
    }

    /// Checks if the IUVD is UDF compliant
    pub fn is_compliant(&self) -> bool {
        &self.impl_identifier.identifier == b"*UDF LV Info\0\0\0\0\0\0\0\0\0\0\0"
    }
}

fn maybe_push_desc<T: Sequenced>(item: T, list: &mut Vec<T>) {
    let pos = list.iter().position(|i| item.is_equivalent_to(i));
    if let Some(pos) = pos {
        let previous = list.get_mut(pos).unwrap(); // safe bc position()
        if item.prevails_over(previous) {
            *previous = item;
        }
    } else {
        list.push(item);
    }
}

#[derive(Debug)]
/// Volume Descriptor Sequence (8.4)
pub struct VolumeDescriptorSequence {
    /// Primary Volume Descriptors
    pub pvds: Vec<PrimaryVolumeDescriptor>,
    /// Implementation Use Descriptors
    pub iuvds: Vec<ImplementationUseVolumeDescriptor>,
    /// Partition Descriptors
    pub pds: Vec<PartitionDescriptor>,
    /// Logical Volume Descriptors
    pub lvds: Vec<LogicalVolumeDescriptor>,
}

impl VolumeDescriptorSequence {
    #[instrument(skip_all)]
    pub(crate) fn new<R: Read + Seek>(
        r: &mut R,
        pointer: &ExtentAD,
        ss: u64,
    ) -> Result<Self, std::io::Error> {
        let mut ret = Self {
            pvds: Vec::new(),
            iuvds: Vec::new(),
            pds: Vec::new(),
            lvds: Vec::new(),
        };
        let align_mask = ss - 1;
        let mut cur_off = u64::from(pointer.lba) * ss;
        let end_off = cur_off + u64::from(pointer.length);
        loop {
            if cur_off & align_mask != 0 {
                cur_off = (cur_off | align_mask) + 1;
            }
            if cur_off + 16 >= end_off {
                break;
            }
            r.seek(std::io::SeekFrom::Start(cur_off))?;
            let lba = cur_off / ss;
            let mut tr = TagReader::new(r)?;
            if tr.tag.lba != lba as u32 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Tag lba mismatch",
                ));
            }
            debug!("Sequence Tag: {:?}", tr.tag);
            cur_off += 16;
            match tr.tag.identifier {
                1 => {
                    if end_off - cur_off < 512 - 16 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Primary Volume Descriptor overflow",
                        ));
                    }
                    tr.set_limit(512 - 16);
                    cur_off += 512 - 16;
                    let pvd = PrimaryVolumeDescriptor::new(&mut tr)?;
                    debug!("{pvd:?}");
                    maybe_push_desc(pvd, &mut ret.pvds);
                }
                4 => {
                    if end_off - cur_off < 512 - 16 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Implementation Use Volume Descriptor overflow",
                        ));
                    }
                    tr.set_limit(512 - 16);
                    cur_off += 512 - 16;
                    let iuvd = ImplementationUseVolumeDescriptor::new(&mut tr)?;
                    debug!("{iuvd:?}");
                    // Note: although IUVDs come with a sequence number, no prevailing rules are provided
                    ret.iuvds.push(iuvd);
                }
                5 => {
                    if end_off - cur_off < 512 - 16 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Partition Descriptor overflow",
                        ));
                    }
                    tr.set_limit(512 - 16);
                    cur_off += 512 - 16;
                    let pd = PartitionDescriptor::new(&mut tr)?;
                    debug!("{pd:?}");
                    maybe_push_desc(pd, &mut ret.pds);
                }
                6 => {
                    if end_off - cur_off < 440 - 16 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Partition Descriptor overflow",
                        ));
                    }
                    tr.set_limit(440 - 16);
                    cur_off += 440 - 16;
                    let mut lvd = LogicalVolumeDescriptor::new(&mut tr)?;
                    let maps_len = u64::from(lvd.map_table_length);
                    cur_off += maps_len;
                    if cur_off > end_off {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Partition Map overflow",
                        ));
                    }
                    tr.set_limit(maps_len);
                    // Load partition maps
                    for _ in 0..lvd.n_partition_maps {
                        lvd.partition_maps.push(PartitionMap::new(&mut tr)?);
                    }
                    // Realign to then next 512 byte boundary
                    debug!("{lvd:?}");
                    maybe_push_desc(lvd, &mut ret.lvds);
                }
                7 => {
                    // Unallocated Space Descriptor
                    debug!("Unallocated Space Descriptor skipped");
                    tr.set_limit(8);
                    let _seq_num = rdu32le(&mut tr)?;
                    let n_alloc_descs = rdu32le(&mut tr)?;
                    cur_off += 8u64 + u64::from(n_alloc_descs) * 8;
                    if cur_off > end_off {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Unallocated Space Descriptor overflow",
                        ));
                    }
                }
                8 => {
                    debug!("Terminating Descriptor found, sequence complete");
                    break;
                }
                _ => {
                    warn!("Unexpected Descriptor found (type {})", tr.tag.identifier);
                    cur_off += 1;
                    continue;
                }
            }
            if !tr.verify_crc() {
                warn!("Descriptor crc mismatch");
            }
        }
        Ok(ret)
    }
}

#[derive(Debug, Clone)]
/// UDF date and time (7.3)
pub enum UdfDate {
    /// The datetime is present, valid and carries TZ info
    ValidTz(time::OffsetDateTime),
    /// The datetime is present, valid and is naive
    ValidNoTz(time::PrimitiveDateTime),
    /// The datetime is missing
    Unset,
    /// The datetime is present but not valid
    Invalid,
}

impl UdfDate {
    /// A datetime parser for UDF structures
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        let ty_tz = rdu16le(r)?;
        let yr = rdu16le(r)?;
        let mo = rdu8(r)?;
        let da = rdu8(r)?;
        let hr = rdu8(r)?;
        let mi = rdu8(r)?;
        let se = rdu8(r)?;
        let cs = rdu8(r)?;
        let hs = rdu8(r)?;
        let us = rdu8(r)?;
        if ty_tz == 0 && yr == 0 && [mo, da, hr, mi, se, cs, hs, us].iter().all(|v| *v == 0) {
            return Ok(Self::Unset);
        }
        if ty_tz >> 12 != 1 {
            // All timestamps shall be recorded in local time.
            return Ok(Self::Invalid);
        }
        let mo = match time::Month::try_from(mo) {
            Ok(v) => v,
            Err(_) => return Ok(Self::Invalid),
        };
        let date = match time::Date::from_calendar_date(i32::from(yr), mo, da) {
            Ok(v) => v,
            Err(_) => return Ok(Self::Invalid),
        };
        if cs > 99 || hs > 99 || us > 99 {
            return Ok(Self::Invalid);
        }
        let us = u32::from(us) + u32::from(hs) * 100 + u32::from(cs) * 10000;
        let time = match time::Time::from_hms_micro(hr, mi, se, us) {
            Ok(v) => v,
            Err(_) => return Ok(Self::Invalid),
        };
        let dt = time::PrimitiveDateTime::new(date, time);
        if ty_tz & 0b1111_1111_1111 == 0b1111_1111_1111 {
            return Ok(Self::ValidNoTz(dt));
        }
        let tz = (ty_tz & 0b0111_1111_1111) as i16; // safe bc mask
        let tz = if ty_tz & 0b1000_0000_0000 != 0 {
            -tz
        } else {
            tz
        };
        if !(-1440..=1440).contains(&tz) {
            return Ok(Self::Invalid);
        }
        let offset = match time::UtcOffset::from_whole_seconds(i32::from(tz) * 60) {
            Ok(v) => v,
            Err(_) => return Ok(Self::Invalid),
        };
        Ok(Self::ValidTz(dt.assume_offset(offset)))
    }

    /// Returns a string representation, if possible
    pub fn to_string_maybe(&self) -> Option<String> {
        match self {
            Self::ValidTz(t) => Some(t.to_string()),
            Self::ValidNoTz(t) => Some(t.to_string()),
            Self::Unset => None,
            Self::Invalid => Some("INVALID".to_string()),
        }
    }

    /// Returns the unix_timestamp, if possible
    pub fn to_ts_maybe(&self) -> Option<i64> {
        match self {
            Self::ValidTz(t) => Some(t.unix_timestamp()),
            Self::ValidNoTz(t) => Some(t.assume_offset(time::UtcOffset::UTC).unix_timestamp()),
            Self::Unset => None,
            Self::Invalid => Some(0),
        }
    }
}

#[derive(Debug)]
/// File Set Descriptor (14.1)
pub struct FileSetDescriptor {
    /// Recording Date and Time
    pub recording_datetime: UdfDate,
    /// Interchange Level
    pub interchange_level: u16,
    /// Maximum Interchange Level
    pub max_interchange_level: u16,
    /// Character Set List
    pub charset_list: u32,
    /// Maximum Character Set List
    pub max_charset_list: u32,
    /// File Set Number
    pub fileset_number: u32,
    /// File Set Descriptor Number
    pub fileset_desc_number: u32,
    /// Logical Volume Identifier Character Set
    pub lv_id_charset: CharSpec,
    /// Logical Volume Identifier
    pub lv_id: Dstring,
    /// File Set Character Set
    pub fileset_charset: CharSpec,
    /// File Set Identifier
    pub fileset_id: Dstring,
    /// Copyright File Identifier
    pub copyright_file_id: Dstring,
    /// Abstract File Identifier
    pub abstract_file_id: Dstring,
    /// Root Directory ICB
    pub root_dir_icb: LongAD,
    /// Domain Identifier
    pub domain_identifier: EntityId,
    /// Next Extent
    pub next_extent: LongAD,
    /// System Stream Directory ICB
    pub system_stream_dir_icb: LongAD,
}

impl FileSetDescriptor {
    #[instrument(skip(r))]
    pub(crate) fn new<R: Read>(r: &mut R, lba: u32) -> Result<Self, std::io::Error> {
        let mut tr = TagReader::new(r)?;
        if tr.tag.identifier != 256 || tr.tag.lba != lba {
            warn!(
                "Invalid File Set Descriptor tag(id {}, lba {}:{})",
                tr.tag.identifier, tr.tag.lba, lba
            );
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid File Set Descriptor tag",
            ));
        }
        let recording_datetime = UdfDate::new(&mut tr)?;
        let interchange_level = rdu16le(&mut tr)?;
        let max_interchange_level = rdu16le(&mut tr)?;
        let charset_list = rdu32le(&mut tr)?;
        let max_charset_list = rdu32le(&mut tr)?;
        let fileset_number = rdu32le(&mut tr)?;
        let fileset_desc_number = rdu32le(&mut tr)?;
        let lv_id_charset = CharSpec::new(&mut tr)?;
        let mut buf = [0u8; 128];
        tr.read_exact(&mut buf)?;
        let lv_id = Dstring::new(&buf);
        let fileset_charset = CharSpec::new(&mut tr)?;
        let mut buf = [0u8; 32];
        tr.read_exact(&mut buf)?;
        let fileset_id = Dstring::new(&buf);
        tr.read_exact(&mut buf)?;
        let copyright_file_id = Dstring::new(&buf);
        tr.read_exact(&mut buf)?;
        let abstract_file_id = Dstring::new(&buf);
        let root_dir_icb = LongAD::new(&mut tr)?;
        let domain_identifier = EntityId::new(&mut tr)?;
        let next_extent = LongAD::new(&mut tr)?;
        let system_stream_dir_icb = LongAD::new(&mut tr)?;
        if !tr.verify_crc() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid File Set Descriptor crc mismatch",
            ));
        }
        Ok(Self {
            recording_datetime,
            interchange_level,
            max_interchange_level,
            charset_list,
            max_charset_list,
            fileset_number,
            fileset_desc_number,
            lv_id_charset,
            lv_id,
            fileset_charset,
            fileset_id,
            copyright_file_id,
            abstract_file_id,
            root_dir_icb,
            domain_identifier,
            next_extent,
            system_stream_dir_icb,
        })
    }

    /// Checks if the FSD is UDF compliant
    pub fn is_compliant(&self) -> bool {
        &self.domain_identifier.identifier == b"*OSTA UDF Compliant\0\0\0\0"
    }
}

#[derive(Debug, Clone)]
/// Extent Descriptor (7.1)
pub struct RecordedAddress {
    /// Logical Block Number
    pub lba: u32,
    /// Partition Reference Number
    pub partition_reference_number: u16,
}

impl RecordedAddress {
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        Ok(Self {
            lba: rdu32le(r)?,
            partition_reference_number: rdu16le(r)?,
        })
    }
}

#[derive(Debug, PartialEq)]
enum ADType {
    Short,
    Long,
    Extended,
    Embedded,
    Invalid(u8),
}

#[derive(Debug, Clone)]
/// ICB Tag (14.6)
pub struct ICBTag {
    /// Prior Recorded Number of Direct Entries
    pub prior_entries: u32,
    /// Strategy Type
    pub strategy_type: u16,
    /// Strategy Parameter
    pub strategy_parameter: [u8; 2],
    /// Maximum Number of Entries
    pub max_entries: u16,
    /// Reserved
    pub reserved: u8,
    /// File Type
    pub file_type: u8,
    /// Parent ICB Location
    pub parent_icb_location: RecordedAddress,
    /// Flags
    pub flags: u16,
}

impl ICBTag {
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        Ok(Self {
            prior_entries: rdu32le(r)?,
            strategy_type: rdu16le(r)?,
            strategy_parameter: [rdu8(r)?, rdu8(r)?],
            max_entries: rdu16le(r)?,
            reserved: rdu8(r)?,
            file_type: rdu8(r)?,
            parent_icb_location: RecordedAddress::new(r)?,
            flags: rdu16le(r)?,
        })
    }

    fn ad_type(&self) -> ADType {
        match self.flags & 0b111 {
            0 => ADType::Short,
            1 => ADType::Long,
            2 => ADType::Extended,
            3 => ADType::Embedded,
            v => ADType::Invalid(v as u8),
        }
    }

    /// Checks if the ICB Tag refers to a directory
    pub fn is_directory(&self) -> bool {
        self.file_type == 4
    }

    /// Checks if the ICB Tag refers to a regular file
    pub fn is_regular(&self) -> bool {
        self.file_type == 5
    }

    /// Checks if the ICB Tag refers to a block device
    pub fn is_block(&self) -> bool {
        self.file_type == 6
    }

    /// Checks if the ICB Tag refers to a character device
    pub fn is_char(&self) -> bool {
        self.file_type == 7
    }

    /// Checks if the ICB Tag refers to a pipe
    pub fn is_fifo(&self) -> bool {
        self.file_type == 9
    }

    /// Checks if the ICB Tag refers to a symbolic link
    pub fn is_link(&self) -> bool {
        self.file_type == 12
    }
}

#[derive(Debug, Clone)]
/// Indicates the location type of the File Entry extents
pub enum FileDataLocation {
    /// The Extent is allocated on the disc as indicated by Allocation Descriptors
    UseADs(Vec<LongAD>),
    /// The Extent is embedded inside the File Entry
    Embedded(Vec<u8>),
}

#[derive(Debug, Clone)]
/// File Entry (14.9) and Extended File Entry (14.17)
pub struct FileEntry {
    /// ICB Tag
    pub icb_tag: ICBTag,
    /// Uid
    pub uid: u32,
    /// Gid
    pub gid: u32,
    /// Permissions
    pub permissions: u32,
    /// File Link Count
    pub file_link_count: u16,
    /// Record Format
    pub record_format: u8,
    /// Record Display Attributes
    pub record_display_attributes: u8,
    /// Record Length
    pub record_length: u32,
    /// Information Length
    pub information_length: u64,
    /// Object Size
    pub object_size: Option<u64>,
    /// Logical Blocks Recorded
    pub logical_blocks_recorded: u64,
    /// Access Date and Time
    pub access_time: UdfDate,
    /// Modification Date and Time
    pub modification_time: UdfDate,
    /// Creation Date and Time
    pub creation_time: UdfDate,
    /// Attribute Date and Time
    pub attribute_time: UdfDate,
    /// Checkpoint
    pub checkpoint: u32,
    /// Extended Attribute ICB
    pub extended_attribute_icb: LongAD,
    /// Stream Directory ICB
    pub stream_directory: Option<LongAD>,
    /// Implementation Identifier
    pub implementation_identifier: EntityId,
    /// Unique Id
    pub unique_id: u64,
    /// Extended Attributes
    pub extended_attributes: Vec<u8>,
    /// Location of the data (Allocation descriptors or embedded)
    pub data_location: FileDataLocation,
}

impl FileEntry {
    #[instrument(skip_all)]
    pub(crate) fn new<R: Read>(r: &mut R, icb: &LongAD, ss: u64) -> Result<Self, std::io::Error> {
        if icb.length < 16 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "File Entry Tag ICB overflow",
            ));
        }
        let mut tr = TagReader::new(r)?;
        if ![261, 266].contains(&tr.tag.identifier) || tr.tag.lba != icb.lba {
            warn!(
                "Invalid File Entry tag(id {}, lba {})",
                tr.tag.identifier, tr.tag.lba
            );
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid File Entry tag",
            ));
        }
        tr.set_limit(u64::from(icb.length).min(ss) - 16);
        let is_extended = tr.tag.identifier == 266;
        let mut res = Self {
            icb_tag: ICBTag::new(&mut tr)?,
            uid: rdu32le(&mut tr)?,
            gid: rdu32le(&mut tr)?,
            permissions: rdu32le(&mut tr)?,
            file_link_count: rdu16le(&mut tr)?,
            record_format: rdu8(&mut tr)?,
            record_display_attributes: rdu8(&mut tr)?,
            record_length: rdu32le(&mut tr)?,
            information_length: rdu64le(&mut tr)?,
            object_size: if is_extended {
                Some(rdu64le(&mut tr)?)
            } else {
                None
            },
            logical_blocks_recorded: rdu64le(&mut tr)?,
            access_time: UdfDate::new(&mut tr)?,
            modification_time: UdfDate::new(&mut tr)?,
            creation_time: if is_extended {
                UdfDate::new(&mut tr)?
            } else {
                UdfDate::Unset
            },
            attribute_time: UdfDate::new(&mut tr)?,
            checkpoint: rdu32le(&mut tr)?,
            extended_attribute_icb: {
                if is_extended {
                    // Skip reserved
                    rdu32le(&mut tr)?;
                };
                LongAD::new(&mut tr)?
            },
            stream_directory: if is_extended {
                Some(LongAD::new(&mut tr)?)
            } else {
                None
            },
            implementation_identifier: EntityId::new(&mut tr)?,
            unique_id: rdu64le(&mut tr)?,
            extended_attributes: Vec::new(),
            data_location: FileDataLocation::Embedded(Vec::new()),
        };
        let extended_attributes_len = rdu32le(&mut tr)?;
        let allocation_descriptors_len = rdu32le(&mut tr)?;

        if 176 + u64::from(extended_attributes_len) + u64::from(allocation_descriptors_len) > ss {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "File Entry overflows sector limit",
            ));
        }

        let mut extended_attributes = vec![0u8; extended_attributes_len as usize];
        tr.read_exact(&mut extended_attributes)?;
        res.extended_attributes = extended_attributes;

        let mut ad_data = vec![0u8; allocation_descriptors_len as usize];
        tr.read_exact(&mut ad_data)?;

        if !tr.verify_crc() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid File Entry: crc mismatch",
            ));
        }
        match res.icb_tag.ad_type() {
            // Note: unwraps are safe as we read from a buffer
            ADType::Short => {
                res.data_location = FileDataLocation::UseADs(
                    ad_data
                        .chunks_exact(8)
                        .map(|ref mut c| {
                            LongAD::from_short(ExtentAD::new(c).unwrap(), icb.part_num)
                        })
                        .collect(),
                );
            }
            ADType::Long => {
                res.data_location = FileDataLocation::UseADs(
                    ad_data
                        .chunks_exact(16)
                        .map(|ref mut c| LongAD::new(c).unwrap())
                        .collect(),
                );
            }
            ADType::Extended => {
                res.data_location = FileDataLocation::UseADs(
                    ad_data
                        .chunks_exact(24)
                        .map(|ref mut c| LongAD::from_extended(ExtendedAD::new(c).unwrap()))
                        .collect(),
                );
            }
            ADType::Embedded => res.data_location = FileDataLocation::Embedded(ad_data),
            ADType::Invalid(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid AD Type {e}"),
                ));
            }
        }
        if let FileDataLocation::UseADs(ads) = &res.data_location {
            if ads.iter().any(|ad| ad.ad_is_continuation()) {
                // FIXME: expand continuation allocation descriptors, recursively!
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Unsupported continuation allocation descriptor",
                ));
            }
        }
        let computed_size = match &res.data_location {
            FileDataLocation::UseADs(ads) => ads
                .iter()
                .map(|ad| u64::from(ad.ad_unmasked_length()))
                .try_fold(0u64, |acc, v| acc.checked_add(v)),
            FileDataLocation::Embedded(data) => u64::try_from(data.len()).ok(),
        };
        if let Some(size) = computed_size {
            if size != res.information_length {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Information length overflow",
                ));
            }
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Information length overflow",
            ));
        }
        Ok(res)
    }

    /// Returns permissions as a string
    pub fn perms_str(&self) -> String {
        fn perm2str(p: u32) -> String {
            format!(
                "{}{}{}{}{}",
                if p & 0x10 != 0 { 'D' } else { 'd' },
                if p & 0x8 != 0 { 'A' } else { 'a' },
                if p & 0x4 != 0 { 'R' } else { 'r' },
                if p & 0x2 != 0 { 'W' } else { 'w' },
                if p & 0x1 != 0 { 'X' } else { 'x' },
            )
        }
        perm2str((self.permissions >> 10) & 0x1f) + // u
            &perm2str((self.permissions >> 5) & 0x1f) + // g
            &perm2str(self.permissions & 0x1f) // o
    }

    /// Whether this is an Extended File Entry
    pub fn is_extended(&self) -> bool {
        self.object_size.is_some()
    }

    /// Whether the data is embedded in the entry rather than allocated
    pub fn is_embedded(&self) -> bool {
        matches!(self.icb_tag.ad_type(), ADType::Embedded)
    }
}

#[derive(Debug)]
/// File Identifier Descriptor (14.4)
pub struct FileIdentifierDescriptor {
    /// File Version Number
    pub version_number: u16,
    /// File Characteristics
    pub characteristics: u8,
    /// ICB
    pub icb: LongAD,
    /// Implementation use
    pub implementation_use: Vec<u8>,
    /// File Identifier
    pub identifier: Dstring,
}

impl FileIdentifierDescriptor {
    #[instrument(skip_all)]
    pub(crate) fn new<R: Read>(r: &mut R, ss: u64) -> Result<Self, std::io::Error> {
        let mut tr = TagReader::new(r)?;
        // FIXME: cannot really validate the lba of the tag
        if tr.tag.identifier != 257 {
            warn!(
                "Invalid File Identifier Descriptor tag(id {}, lba {})",
                tr.tag.identifier, tr.tag.lba
            );
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid File Identifier Descriptor tag",
            ));
        }
        tr.set_limit(ss - 16);
        let version_number = rdu16le(&mut tr)?;
        let characteristics = rdu8(&mut tr)?;
        let id_len = u16::from(rdu8(&mut tr)?);
        let icb = LongAD::new(&mut tr)?;
        let iu_len = rdu16le(&mut tr)?;

        if 38 + u64::from(id_len) + u64::from(iu_len) > ss {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "File Identifier Descriptor overflows sector limit",
            ));
        }
        let mut implementation_use = vec![0u8; usize::from(iu_len)];
        tr.read_exact(&mut implementation_use)?;
        let mut id_bin = vec![0u8; usize::from(id_len)];
        tr.read_exact(&mut id_bin)?;
        let identifier = Dstring::new_nolen(&id_bin);
        let align = 0u16
            .wrapping_sub(id_len)
            .wrapping_sub(iu_len)
            .wrapping_sub(2)
            & 0x3;
        let mut _pad = [0u8; 4];
        tr.read_exact(&mut _pad[0..align.into()])?;
        if !tr.verify_crc() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid Identifier Descriptor: crc mismatch",
            ));
        }
        Ok(Self {
            version_number,
            characteristics,
            icb,
            implementation_use,
            identifier,
        })
    }

    pub(crate) fn is_directory(&self) -> bool {
        self.characteristics & 0b10 != 0
    }

    pub(crate) fn is_deleted(&self) -> bool {
        self.characteristics & 0b100 != 0
    }

    pub(crate) fn is_parent(&self) -> bool {
        self.characteristics & 0b1000 != 0
    }
}
