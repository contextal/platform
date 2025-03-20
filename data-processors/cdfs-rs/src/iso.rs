//! Extractor for ISO9660 images
//!
//! This module supports extraction of metadata and files from iso9660 images
//! in canonical (typically .iso) and raw (.raw, .img, .bin, .nrg) formats with
//! or without a custom header
//!
//! The header presence and the raw sector size are autodetected

use ctxutils::io::{rdu8, rdu16be, rdu16le, rdu32be, rdu32le};
use std::collections::HashSet;
use std::io::{Read, Seek};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};

// FIXME: multi extent

/// The maximum number of supported volumes per image
const MAX_VOLUMES: usize = 32;
/// The maximum allowed image header size
const MAX_HEADER_SIZE: u64 = 512 * 1024;
/// The iso9660 sector size
const SECTOR_SIZE: u16 = 2048; // infallible Into (and safe as) u32, u64, usize

/// Both endian `u16` reader
#[inline]
pub(crate) fn rdu16both<R: Read>(r: &mut R, what: &str) -> Result<u16, std::io::Error> {
    let le = rdu16le(r)?;
    let be = rdu16be(r)?;
    if le != be {
        warn!("Both-order 16 bit value '{what}' mismatch ({le} vs {be})");
    }
    Ok(le)
}

/// Both endian `u32` reader
#[inline]
pub(crate) fn rdu32both<R: Read>(r: &mut R, what: &str) -> Result<u32, std::io::Error> {
    let le = rdu32le(r)?;
    let be = rdu32be(r)?;
    if le != be {
        warn!("Both-order 32 bit value '{what}' mismatch ({le} vs {be})");
    }
    Ok(le)
}

/// Iso9660 extractor
pub struct Iso9660<'r, R: Read + Seek> {
    r: &'r mut R,
    /// The (autodetected) header size
    pub image_header_size: u64,
    /// The (autodetected) sector size
    pub raw_sector_size: u64,
    /// The volumes present in the image
    pub volumes: Vec<Volume>,
    /// Bootable flag
    pub is_bootable: bool,
    /// Partitioned flag
    pub is_partitioned: bool,
}

impl<'r, R: Read + Seek> Iso9660<'r, R> {
    /// Creates a new iso9660 extractor
    #[instrument(skip_all)]
    pub fn new(r: &'r mut R) -> Result<Self, std::io::Error> {
        // A System area of 16 sectors is present at the beginning of the image
        // We skip the minimum (no header and raw_sector_size = SECTOR_SIZE)
        let mut pvd_offset = u64::from(SECTOR_SIZE) * 16;
        r.seek(std::io::SeekFrom::Start(pvd_offset))?;
        let mut volumes: Vec<Volume> = Vec::new();
        let mut is_bootable = false;
        let mut is_partitioned = false;

        // FIXME: can the PVD not be the first?
        // Search for the PVD
        let mut sector = [0u8; SECTOR_SIZE as usize];
        loop {
            r.read_exact(&mut sector)?;
            if let Some(off) = sector
                .as_slice()
                .windows(6)
                .position(|win| win == b"\x01CD001")
            {
                pvd_offset += off as u64; // safe cast because off < SECTOR_SIZE
                if off != 0 {
                    // Reread the whole pvd if we matched outside of a SECTOR_SIZE boundary
                    // Happens when:
                    // - raw_sector_size > SECTOR_SIZE
                    // - some header exists
                    // - both of the above
                    r.seek(std::io::SeekFrom::Start(pvd_offset))?;
                    r.read_exact(&mut sector)?;
                }
                let pvd = Volume::new(&sector)?;
                debug!("PVD: {pvd:#?}");
                volumes.push(pvd);
                break;
            }
            pvd_offset = r.seek(std::io::SeekFrom::Current(-5))?;
            if pvd_offset > MAX_HEADER_SIZE + u64::from(SECTOR_SIZE) * 16 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Primary Volume Descriptor not found",
                ));
            }
        }
        debug!("Primary volume descriptor offset: {pvd_offset}");

        // Find the next volume descriptor - the type doesn't matter ([1..] below)
        r.read_exact(&mut sector)?;
        let mut next_desc_offset =
            if let Some(off) = sector[1..].windows(5).position(|win| win == b"CD001") {
                pvd_offset + // previous volume offset
                u64::from(SECTOR_SIZE) + // the size of the previous volume sector
                (off as u64) // offset within the buffer - safe cast because off < SECTOR_SIZE
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Neither a Volume Descriptor nor a Volume Set terminator were found",
                ));
            };
        debug!("Next volume descriptor offset: {next_desc_offset}");

        // Compute header and raw sector sizes
        let raw_sector_size = next_desc_offset - pvd_offset;
        let raw_system_area_len = raw_sector_size * 16;
        let image_header_size = pvd_offset.checked_sub(raw_system_area_len).ok_or_else(|| {
            warn!("System area overflow");
            std::io::Error::new(std::io::ErrorKind::InvalidData, "System area overflow")
        })?;
        debug!("Image header size: {image_header_size}");
        debug!("Raw Sector size: {raw_sector_size}");

        // Read all volumes
        while volumes.len() < MAX_VOLUMES {
            r.seek(std::io::SeekFrom::Start(next_desc_offset))?;
            next_desc_offset += raw_sector_size;
            r.read_exact(&mut sector)?;
            if &sector[1..6] != b"CD001" {
                warn!("Unterminated Volume Descriptor Set");
                break;
            }
            if sector[0] == 255 {
                // Volume Descriptor Set Terminator
                break;
            }
            if !(1..3).contains(&sector[0]) {
                if sector[0] == 0 {
                    is_bootable = true;
                } else if sector[0] == 3 {
                    is_partitioned = true;
                }
                warn!("Unsupported Volume Descriptor type ({})", sector[0]);
                continue;
            }
            match Volume::new(&sector) {
                Ok(vol) => {
                    debug!("VOLUME {vol:#?}");
                    volumes.push(vol);
                }
                Err(e) => warn!("Volume error: {e}"),
            }
        }
        Ok(Self {
            r,
            image_header_size,
            raw_sector_size,
            volumes,
            is_bootable,
            is_partitioned,
        })
    }

    /// Opens an image [`Volume`] for extraction
    #[instrument(skip(self))]
    pub fn open_volume<'i>(
        &'i mut self,
        volume_number: usize,
    ) -> Option<Result<VolumeReader<'r, 'i, R>, std::io::Error>> {
        let volume = self.volumes.get(volume_number)?;
        Some(VolumeReader::new(self, volume.clone()))
    }
}

#[derive(Debug, Clone)]
/// Volume Descriptor
pub struct Volume {
    /// Volume Descriptor Type
    pub descriptor_type: u8,
    /// Volume Descriptor Version
    pub version: u8,
    /// Volume Flags
    pub flags: u8,
    /// System Identifier
    pub system_id: String,
    /// Volume Identifier
    pub volume_id: String,
    /// Volume Space Size
    pub volume_space_size: u32,
    /// Escape Sequences
    pub escapes: [u8; 32],
    /// Volume Set Size
    pub volume_set_size: u16,
    /// Volume Sequence Number
    pub volume_sequence_number: u16,
    /// Logical Block Size
    pub block_size: u16,
    /// Path Table Size
    pub path_table_size: u32,
    /// Location of Occurrence of Type L Path Table
    pub path_table_lba_le: u32,
    /// Location of Optional Occurrence of Type L Path Table
    pub opt_path_table_lba_le: u32,
    /// Location of Occurrence of Type M Path Table
    pub path_table_lba_be: u32,
    /// Location of Optional Occurrence of Type M Path Table
    pub opt_path_table_lba_be: u32,
    /// Volume Set Identifier
    pub volume_set_id: String,
    /// Publisher Identifier
    pub publisher_id: String,
    /// Data Preparer Identifier
    pub preparer_id: String,
    /// Application Identifier
    pub application_id: String,
    /// Copyright File Identifier
    pub copyright_file_id: String,
    /// Abstract File Identifier
    pub abstract_file_id: String,
    /// Bibliographic File Identifier
    pub bibliographic_file_id: String,
    /// Volume Creation Date and Time
    pub volume_creation_dt: IsoDate,
    /// Volume Modification Date and Time
    pub volume_modification_dt: IsoDate,
    /// Volume Expiration Date and Time
    pub volume_expiration_dt: IsoDate,
    /// Volume Effective Date and Time
    pub volume_effective_dt: IsoDate,
    /// File Structure Version
    pub file_structure_version: u8,
    /// Number of blocks per sector
    blocks_per_sector: u16,
    /// Whether this volume conforms to the joliet specifications
    pub is_joliet: bool,
    /// The root directory of this volume
    root: DirectoryRecord,
}

impl Volume {
    /// Parses a sector into a [`Volume`]
    #[instrument(skip_all)]
    fn new(sector: &[u8; SECTOR_SIZE as usize]) -> Result<Self, std::io::Error> {
        let block_size = rdu16both(&mut &sector[128..132], "Logical Block Size")?;
        if !block_size.is_power_of_two() || !(512..=SECTOR_SIZE).contains(&block_size) {
            // block size is a pow2 between 512 and SECTOR_SIZE
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid Volume block size {block_size}"),
            ));
        }
        let blocks_per_sector = SECTOR_SIZE / block_size; // safe and without reminder due to the prev constraint
        let escapes: [u8; 32] = sector[88..120].try_into().unwrap();
        let is_joliet = sector[0] == 2
            && escapes[0] == 0x25
            && escapes[1] == 0x2f
            && [0x40, 0x43, 0x45].contains(&escapes[2]);
        let root = DirectoryRecord::new(&mut &sector[156..190], is_joliet)?;
        if root.is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "The root Directory Record is empty",
            ));
        }
        let mut root = root.unwrap(); // safe bc is_none check above
        if !root.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "The root Directory Record is not a directory",
            ));
        }
        if root.is_interleaved() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "The root Directory Record is interleaved",
            ));
        }
        root.file_id = "".to_string();
        Ok(Self {
            descriptor_type: sector[0],
            version: sector[6],
            flags: sector[7],
            system_id: Self::iso_string(&sector[8..40], is_joliet),
            volume_id: Self::iso_string(&sector[40..72], is_joliet),
            volume_space_size: rdu32both(&mut &sector[80..88], "Volume Space Size")?,
            escapes,
            volume_set_size: rdu16both(&mut &sector[120..124], "Volume Set Size")?,
            volume_sequence_number: rdu16both(&mut &sector[124..128], "Volume Sequence Number")?,
            block_size,
            path_table_size: rdu32both(&mut &sector[132..140], "Path Table Size")?,
            path_table_lba_le: rdu32le(&mut &sector[140..144])?,
            opt_path_table_lba_le: rdu32le(&mut &sector[144..148])?,
            path_table_lba_be: rdu32be(&mut &sector[148..152])?,
            opt_path_table_lba_be: rdu32be(&mut &sector[152..156])?,
            volume_set_id: Self::iso_string(&sector[190..318], is_joliet),
            publisher_id: Self::iso_string(&sector[318..446], is_joliet),
            preparer_id: Self::iso_string(&sector[446..574], is_joliet),
            application_id: Self::iso_string(&sector[574..702], is_joliet),
            copyright_file_id: Self::iso_string(&sector[702..739], is_joliet),
            abstract_file_id: Self::iso_string(&sector[739..776], is_joliet),
            bibliographic_file_id: Self::iso_string(&sector[776..813], is_joliet),
            volume_creation_dt: IsoDate::from_volume_descriptor(
                &sector[813..830].try_into().unwrap(), // safe
                "Volume Creation Date",
            ),
            volume_modification_dt: IsoDate::from_volume_descriptor(
                &sector[830..847].try_into().unwrap(), // safe
                "Volume Modification Date",
            ),
            volume_expiration_dt: IsoDate::from_volume_descriptor(
                &sector[847..864].try_into().unwrap(), // safe
                "Volume Modification Date",
            ),
            volume_effective_dt: IsoDate::from_volume_descriptor(
                &sector[864..881].try_into().unwrap(), // safe
                "Volume Effective Date",
            ),
            file_structure_version: sector[881],
            blocks_per_sector,
            is_joliet,
            root,
        })
    }

    /// Iso string to String
    fn iso_string(buf: &[u8], joliet: bool) -> String {
        if joliet {
            char::decode_utf16(
                buf.chunks_exact(2)
                    .map(|word| u16::from_be_bytes(word.try_into().unwrap())), // safe bc chunks_exact
            )
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect::<String>()
            .trim_end_matches(' ')
            .to_string()
        } else {
            String::from_utf8_lossy(buf)
                .trim_end_matches(' ')
                .to_string()
        }
    }

    /// Returns whether this is a Primary Volume Descriptor
    pub fn is_primary(&self) -> bool {
        self.descriptor_type == 1
    }
}

/// A Volume Extractor
pub struct VolumeReader<'r, 'i, R: Read + Seek> {
    /// The image
    iso: &'i mut Iso9660<'r, R>,
    /// The Volume Descriptor metadata
    pub volume: Volume,
    /// The full list of files found when traversing the directory tree
    files: Vec<DirectoryRecord>,
    /// Directory loops are present
    pub has_loops: bool,
    /// Whether some entry is not reachable through all chains
    pub bad_chains: bool,
}

impl<'r, 'i, R: Read + Seek> VolumeReader<'r, 'i, R> {
    /// Creates a new volume extractor for the given image
    #[instrument(skip_all, fields(volume.volume_id))]
    fn new(iso: &'i mut Iso9660<'r, R>, volume: Volume) -> Result<Self, std::io::Error> {
        let mut ret = Self {
            iso,
            volume,
            files: Vec::new(),
            has_loops: false,
            bad_chains: false,
        };
        let mut dirs = HashSet::<u32>::new();
        debug!(
            "------------- Volume {} - DIR walk ------------",
            ret.volume.volume_id
        );
        ret.dirwalk(&ret.volume.root.clone(), &mut dirs);
        debug!(
            "------------- Volume {} - Path tables ------------",
            ret.volume.volume_id
        );
        let mut bad_chains = false;
        // L walk
        let is_joliet = ret.volume.is_joliet;
        for (lba, len, tbl_name) in [
            (
                ret.volume.path_table_lba_le,
                ret.volume.path_table_size.into(),
                "L table",
            ),
            (
                ret.volume.opt_path_table_lba_le,
                ret.volume.path_table_size.into(),
                "Optional L table",
            ),
        ] {
            if lba != 0 && len != 0 {
                let mut rdr = ret.get_reader(lba, len);
                loop {
                    let path_tbl = PathTable::new_le(&mut rdr, is_joliet);
                    match path_tbl {
                        Some(v) => {
                            let block = v.actual_lba();
                            debug!("L Path {v:#?}");
                            if !dirs.contains(&block) {
                                warn!("LBA {block} listed in primary {tbl_name} is not reachable");
                                bad_chains = true;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
        for (lba, len, tbl_name) in [
            (
                ret.volume.path_table_lba_be,
                ret.volume.path_table_size.into(),
                "M table",
            ),
            (
                ret.volume.opt_path_table_lba_be,
                ret.volume.path_table_size.into(),
                "Optional M table",
            ),
        ] {
            if lba != 0 && len != 0 {
                let mut rdr = ret.get_reader(lba, len);
                loop {
                    let path_tbl = PathTable::new_be(&mut rdr, is_joliet);
                    match path_tbl {
                        Some(v) => {
                            let block = v.actual_lba();
                            debug!("M Path {v:#?}");
                            if !dirs.contains(&block) {
                                warn!("LBA {block} listed in primary {tbl_name} is not reachable");
                                bad_chains = true;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
        ret.bad_chains |= bad_chains;
        Ok(ret)
    }

    /// Walks the directory tree from the root and populates the list of files
    #[instrument(skip_all, fields(dir.file_id))]
    fn dirwalk(&mut self, dir: &DirectoryRecord, dirs: &mut HashSet<u32>) {
        if !dir.is_dir() {
            warn!("Internal error: dirwalk called on a file ({})", dir.file_id);
            return;
        }
        if dir.is_interleaved() {
            warn!("Found illegal interleaved directory {}", dir.file_id);
            return;
        }
        let block = dir.actual_lba();
        if !dirs.insert(block) {
            self.has_loops = true;
            warn!("Duplicate directory LBA {block}");
            return;
        }
        dirs.insert(block);
        let mut child_dirs: Vec<DirectoryRecord> = Vec::new();
        let mut child_files: Vec<DirectoryRecord> = Vec::new();
        let is_joliet = self.volume.is_joliet;
        let mut rdr = self.get_reader(dir.actual_lba(), u64::from(dir.data_length));
        while rdr.todo > 0 {
            match DirectoryRecord::new(&mut rdr, is_joliet) {
                Ok(Some(mut child)) => {
                    if child.is_dir() {
                        if child.file_id != "\0" && child.file_id != "\u{1}" {
                            child.file_id = format!("{}/{}", dir.file_id, child.file_id);
                            debug!("DIR: {child:#?}");
                            child_dirs.push(child);
                        }
                    } else {
                        child.file_id = format!(
                            "{}/{}",
                            dir.file_id,
                            if is_joliet {
                                child
                                    .file_id
                                    .rfind(';')
                                    .map(|pos| &child.file_id[0..pos])
                                    .unwrap_or(&child.file_id)
                            } else {
                                child
                                    .file_id
                                    .rfind(';')
                                    .map(|pos| &child.file_id[0..pos])
                                    .unwrap_or(&child.file_id)
                                    .trim_end_matches('.')
                            }
                        );
                        debug!("FILE: {child:#?}");
                        child_files.push(child);
                    }
                }
                Ok(None) => rdr.seek_to_next_sector(),
                Err(e) => {
                    warn!("Directory Record error: {e}");
                    break;
                }
            }
        }
        self.files.append(&mut child_files);
        for child in child_dirs {
            self.dirwalk(&child, dirs);
        }
    }

    /// Returns a reader for an extent
    fn get_reader<'v>(&'v mut self, start_block: u32, size: u64) -> BlockReader<'r, 'i, 'v, R> {
        BlockReader {
            vr: self,
            current_block: start_block,
            offset: 0,
            todo: size,
            interleaved: None,
        }
    }

    /// Returns a reader for the specified file
    pub fn open_file<'v>(
        &'v mut self,
        file_number: usize,
    ) -> Option<(DirectoryRecord, BlockReader<'r, 'i, 'v, R>)> {
        let file = self.files.get(file_number)?;
        let start_block = file.actual_lba();
        let size = u64::from(file.data_length);
        let interleaved: Option<InterleavedExtent> = if file.is_interleaved() {
            Some(InterleavedExtent {
                block: 0,
                unit_size: u32::from(file.file_unit_size),
                gap_size: u32::from(file.interleave_gap_size),
            })
        } else {
            None
        };
        Some((
            file.clone(),
            BlockReader {
                vr: self,
                current_block: start_block,
                offset: 0,
                todo: size,
                interleaved,
            },
        ))
    }
}

/// An extent reader
pub struct BlockReader<'r, 'i, 'v, R: Read + Seek> {
    /// The volume reader
    vr: &'v mut VolumeReader<'r, 'i, R>,
    /// The block currently being read
    current_block: u32,
    /// The current offset withing the block
    offset: u16,
    /// The remaining size to read
    todo: u64,
    /// Interleaved support
    interleaved: Option<InterleavedExtent>,
}

struct InterleavedExtent {
    /// Interleaved block number
    block: u32,
    /// The Unit size
    unit_size: u32,
    /// Gap size
    gap_size: u32,
}

impl<'r, 'i, 'v, R: Read + Seek> BlockReader<'r, 'i, 'v, R> {
    fn seek_to_next_sector(&mut self) {
        assert!(
            self.interleaved.is_none(),
            "Cannot seek in interleaved extents"
        );
        if self.offset != 0 {
            self.current_block += 1;
            self.todo = self
                .todo
                .saturating_sub((self.vr.volume.block_size - self.offset).into());
            self.offset = 0;
        }
        let nblock = self.current_block % u32::from(self.vr.volume.blocks_per_sector);
        if nblock != 0 {
            let skip_blocks = u32::from(self.vr.volume.blocks_per_sector) - nblock;
            self.current_block += skip_blocks;
            self.todo = self
                .todo
                .saturating_sub(u64::from(skip_blocks) * u64::from(self.vr.volume.block_size));
        }
    }
}

impl<'r, 'i, 'v, R: Read + Seek> Read for BlockReader<'r, 'i, 'v, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let mut produced = 0usize;
        let mut avail_out = buf.len();

        while self.todo > 0 && avail_out > 0 {
            let sector = self.current_block / u32::from(self.vr.volume.blocks_per_sector);
            let sector_offset =
                self.vr.iso.image_header_size + u64::from(sector) * self.vr.iso.raw_sector_size;
            let block_in_sector = self.current_block % u32::from(self.vr.volume.blocks_per_sector);
            let offset_within_sector = block_in_sector * u32::from(self.vr.volume.block_size);
            let seek_offset =
                sector_offset + u64::from(offset_within_sector) + u64::from(self.offset);
            self.vr.iso.r.seek(std::io::SeekFrom::Start(seek_offset))?;
            let need = avail_out
                .min(self.todo.try_into().unwrap_or(avail_out))
                .min((self.vr.volume.block_size - self.offset).into());
            self.vr
                .iso
                .r
                .read_exact(&mut buf[produced..(produced + need)])?;
            avail_out -= need;
            self.todo -= need as u64; // safe due to min() above
            produced += need;
            self.offset += need as u16; // safe due to min() above
            if self.offset == self.vr.volume.block_size {
                self.offset = 0;
                self.current_block += 1;
                if let Some(ref mut inter) = self.interleaved {
                    inter.block += 1;
                    let pos = inter.block % (inter.unit_size + inter.gap_size);
                    if pos >= inter.unit_size {
                        inter.block += inter.gap_size;
                        self.current_block += inter.gap_size;
                    }
                }
            }
        }
        Ok(produced)
    }
}

/// An iso9660 date and time
#[derive(Debug, Clone, PartialEq)]
pub enum IsoDate {
    /// The datetime is present and valid
    Valid(time::OffsetDateTime),
    /// The datetime is missing
    Unset,
    /// The datetime is present but not valid
    Invalid,
}

impl IsoDate {
    /// A datetime parser for [`DirectoryRecord`]'s
    fn from_directory_record(buf: &[u8; 7]) -> Self {
        (|| {
            let year_since_1900 = buf[0];
            let month = time::Month::try_from(buf[1]).ok()?;
            let day = buf[2];
            let date =
                time::Date::from_calendar_date(i32::from(year_since_1900) + 1900, month, day)
                    .ok()?;
            let hour = buf[3];
            let minute = buf[4];
            let second = buf[5];
            let time = time::Time::from_hms(hour, minute, second).ok()?;
            let gmt_offset = buf[6] as i8; // cast is intended
            let offset =
                time::UtcOffset::from_whole_seconds(i32::from(gmt_offset) * 15 * 60).ok()?;
            Some(Self::Valid(
                time::PrimitiveDateTime::new(date, time).assume_offset(offset),
            ))
        })()
        .unwrap_or(Self::Invalid)
    }

    /// A datetime parser for [`Volume`]'s
    fn from_volume_descriptor(buf: &[u8; 17], what: &str) -> Self {
        if buf == b"0000000000000000\0" {
            return Self::Unset;
        }
        let s = String::from_utf8_lossy(&buf[0..16]);
        match (|| {
            let dt = time::PrimitiveDateTime::parse(
                &s[0..14],
                time::macros::format_description!("[year][month][day][hour][minute][second]"),
            )?;
            let centi: u16 = s[14..16].parse()?;
            let dt = dt.replace_millisecond(centi * 10)?;
            let offset: i32 = i32::from(buf[16]) * 15 * 60;
            let off = time::UtcOffset::from_whole_seconds(offset)?;
            Ok::<time::OffsetDateTime, Box<dyn std::error::Error>>(dt.assume_offset(off))
        })() {
            Ok(v) => Self::Valid(v),
            Err(e) => {
                debug!("Invalid IsoDate '{what}': {e}");
                Self::Invalid
            }
        }
    }
}

impl std::fmt::Display for IsoDate {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Valid(dt) => dt.fmt(fmt),
            Self::Invalid => write!(fmt, "<INVALID>"),
            Self::Unset => write!(fmt, ""),
        }
    }
}

/// Directory Record metadata
#[derive(Debug, Clone)]
pub struct DirectoryRecord {
    /// Location of Extent
    pub extent_lba: u32,
    /// Data Length
    pub data_length: u32,
    /// Extended Attribute Record Length
    pub ext_len: u8,
    /// Recording Date and Time
    pub recording_dt: IsoDate,
    /// File Flags
    pub file_flags: u8,
    /// File Unit Size
    pub file_unit_size: u8,
    /// Interleave Gap Size
    pub interleave_gap_size: u8,
    /// Volume Sequence Number
    pub volume_sequence_number: u16,
    /// File Identifier
    pub file_id: String,
}

impl DirectoryRecord {
    /// Parses a record into a [`DirectoryRecord`]
    fn new<R: Read>(r: &mut R, is_joliet: bool) -> Result<Option<Self>, std::io::Error> {
        Self::new_internal(r, is_joliet).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Directory Record overflow")
            } else {
                e
            }
        })
    }

    fn new_internal<R: Read>(r: &mut R, is_joliet: bool) -> Result<Option<Self>, std::io::Error> {
        let mut record_len = rdu8(r)?;
        if record_len == 0 {
            return Ok(None);
        }
        let ext_len = rdu8(r)?;
        let extent_lba = rdu32both(r, "Extent LBA")?;
        let data_length = rdu32both(r, "Data Length")?;
        let mut buf = [0u8; 7];
        r.read_exact(&mut buf)?;
        let recording_dt = IsoDate::from_directory_record(&buf);
        let file_flags = rdu8(r)?;
        let file_unit_size = rdu8(r)?;
        let interleave_gap_size = rdu8(r)?;
        let volume_sequence_number = rdu16both(r, "Volume Sequence Number")?;
        let file_id_len = rdu8(r)?;
        record_len -= 33;
        if record_len < file_id_len {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Directory Record File Identifier overflow",
            ));
        }
        record_len -= file_id_len;
        let mut buf = vec![0u8; usize::from(file_id_len)];
        r.read_exact(&mut buf)?;
        let file_id = Self::buf_to_string(&buf, is_joliet);
        let record_len = usize::from(record_len);
        let system_area = usize::from(file_id_len);
        let system_area = usize::from(file_id_len) + ((system_area ^ 1) & 1);
        if record_len > system_area {
            // This is only interesting if we want to handle Rock Ridge extensions
        }
        std::io::copy(&mut r.take(record_len as u64), &mut std::io::sink())?; // safe bc record_len < 256
        Ok(Some(Self {
            extent_lba,
            data_length,
            ext_len,
            recording_dt,
            file_flags,
            file_unit_size,
            interleave_gap_size,
            volume_sequence_number,
            file_id,
        }))
    }

    /// Returns whether the record is for a directory
    fn is_dir(&self) -> bool {
        self.file_flags & 2 != 0
    }

    /// Returns the logical block at which the data for the record start
    fn actual_lba(&self) -> u32 {
        self.extent_lba + u32::from(self.ext_len)
    }

    /// Checks if the record has interleaved data
    pub fn is_interleaved(&self) -> bool {
        self.file_unit_size != 0 && self.interleave_gap_size != 0
    }

    /// A convenience bytes to String converter
    fn buf_to_string(buf: &[u8], is_joliet: bool) -> String {
        if is_joliet && buf.len() > 1 {
            char::decode_utf16(
                buf.chunks_exact(2)
                    .map(|word| u16::from_be_bytes(word.try_into().unwrap())), // safe bc chunks_exact
            )
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect::<String>()
        } else {
            String::from_utf8_lossy(buf).to_string()
        }
    }
}

/// A Path Table entry
#[derive(Debug)]
struct PathTable {
    /// Location of Extent
    extent_lba: u32,
    /// The (1-based) index of the parent directory
    _parent_dir_number: u16,
    /// The name of this directory
    _dir_id: String,
    /// Extended Attribute Record Length
    ext_len: u8,
}

impl PathTable {
    /// Parses an entry for the L Path Table (little endian)
    fn new_le<R: Read>(r: &mut R, is_joliet: bool) -> Option<Self> {
        Self::new_common(r, true, is_joliet)
    }

    /// Parses an entry for the M Path Table (big endian)
    fn new_be<R: Read>(r: &mut R, is_joliet: bool) -> Option<Self> {
        Self::new_common(r, false, is_joliet)
    }

    /// Parses a Path Table entry (shared code)
    fn new_common<R: Read>(r: &mut R, le: bool, is_joliet: bool) -> Option<Self> {
        let id_len = rdu8(r).ok()?;
        let ext_len = rdu8(r).ok()?;
        let extent_lba = if le { rdu32le(r) } else { rdu32be(r) }.ok()?;
        let _parent_dir_number = if le { rdu16le(r) } else { rdu16be(r) }.ok()?;
        let mut dir_id = vec![0u8; usize::from(id_len)];
        r.read_exact(&mut dir_id).ok()?;
        let _dir_id = DirectoryRecord::buf_to_string(&dir_id, is_joliet);
        let skip_len = u64::from(id_len & 1);
        std::io::copy(&mut r.take(skip_len), &mut std::io::sink()).ok()?;
        Some(Self {
            extent_lba,
            _parent_dir_number,
            _dir_id,
            ext_len,
        })
    }

    /// Returns the logical block at which the data for the record start
    fn actual_lba(&self) -> u32 {
        self.extent_lba + u32::from(self.ext_len)
    }
}
