//! # A library to read Ole files
//!
//! Provide functionality to read objects in the *Compound File Binary Format*
//!
//! The implementation, which is based entirely upon
//! [\[MS-CFB\]](https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-cfb/53989ce4-7b05-4f8d-829b-d08d6148375b), is
//! mostly focused towards malware analysis. For this reason it tries its best to mimic
//! the empirically determinated behaviour of MS products: this includes accepting
//! malformed (when not intentionally evil) content
//!
//! See [Ole] for the main interface documentation and code examples
//!

#![warn(missing_docs)]

pub mod crypto;
pub mod oleps;

use ctxutils::cmp::Unsigned as _;
use ctxutils::io::*;
use ctxutils::win32::*;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashSet;
use std::io::{self, Read, Seek};
use time::OffsetDateTime;

const DIFSECT: u32 = 0xffffffc;
const FATSECT: u32 = 0xfffffffd;
const ENDOFCHAIN: u32 = 0xfffffffe;
const FREESECT: u32 = 0xffffffff;
const MAXREGSID: u32 = 0xfffffffa;
const NOSTREAM: u32 = 0xffffffff;

/// The parser and stream reader for Ole objects
///
/// # Examples
/// ```no_run
/// use ctxole::Ole;
/// use std::fs::File;
/// use std::io::{self, BufReader};
///
/// let f = File::open("MyDocument.doc").unwrap();
/// let ole = Ole::new(BufReader::new(f)).unwrap();
/// let entry = ole.get_entry_by_name("WordDocument").unwrap();
/// let mut reader = ole.get_stream_reader(&entry);
/// let mut writer: Vec<u8> = Vec::new();
/// io::copy(&mut reader, &mut writer).unwrap();
/// ```
///
/// # Errors
/// Most fuctions return a [`Result<T, std::io::Error>`]
/// * Errors from the IO layer are bubbled
/// * Errors generated in the parser are reported with [`ErrorKind`](std::io::ErrorKind)
///   set to [`InvalidData`](std::io::ErrorKind#variant.InvalidData)
///
pub struct Ole<R: Read + Seek> {
    internal: OleCore<R>,
}

impl<R: Read + Seek> Ole<R> {
    /// Parses an Ole object and collects the relevant characteristics
    pub fn new(reader: R) -> Result<Self, io::Error> {
        Ok(Self {
            internal: OleCore::new(reader)?,
        })
    }

    /// Returns the major and minor versions of the Ole structure as a tuple
    pub fn version(&self) -> (u16, u16) {
        (self.internal.major_version, self.internal.minor_version)
    }

    /// Returns the number of entries in the Ole structure
    pub fn num_entries(&self) -> u32 {
        self.internal.dir_entries
    }

    /// Lists the anomalies detected in the main Ole structures
    pub fn anomalies(&self) -> &[String] {
        self.internal.anomalies.as_slice()
    }

    /// Returns an [`OleCrypto`](crypto::OleCrypto) suitable for analyzing Ole cryptography
    /// and for decrypting the content
    ///
    /// `Err` is returned in case the Ole object is not encrypted or when the cryptography
    /// data are corrupted, invalid or not supported
    ///
    /// # Note #
    /// In principle a full *Data Spaces* structure should be present inside the *Ole object*,
    /// in order for encryption to be defined.
    /// In practice files can be opened despite its absence or corruption.
    ///
    /// The best indicator of Ole encryption is the presence of the `EncryptionInfo` stream
    pub fn get_decryptor(&self) -> Result<crypto::OleCrypto, io::Error> {
        crypto::OleCrypto::new(self)
    }

    /// Retrieves a directory entry by name
    ///
    /// * Path components must be separated with a `/`
    /// * The `Root Entry` is implied and must be omitted, nor a leading `/` shall be included
    /// * No character mangling is performed, therfore care must be taken with
    ///   "weird" names e.g.: `"\u{5}SummaryInformation"`
    ///
    /// # Examples
    /// ```no_run
    /// use ctxole::Ole;
    /// use std::fs::File;
    /// use std::io::ErrorKind;
    ///
    /// let ole = Ole::new(File::open("MyDocument.doc").unwrap()).unwrap();
    /// let macros = match ole.get_entry_by_name("Macros/VBA/_VBA_PROJECT") {
    ///     Ok(v) => v,
    ///     Err(e) => match e.kind() {
    ///         ErrorKind::InvalidData => { panic!("An Ole parse problem was encountered: {}", e) },
    ///         ErrorKind::NotFound => { panic!("The requested entry could not be found") },
    ///         _ => { panic!("An error occurred: {}", e) }
    ///     }
    /// };
    /// ```
    ///
    /// # Errors
    /// * Errors from the IO layer are bubbled
    /// * Errors generated in the parser are reported with [`ErrorKind`](std::io::ErrorKind)
    ///   set to [`InvalidData`](std::io::ErrorKind#variant.InvalidData)
    /// * If the entry cannot be found the error returned has [`ErrorKind`](std::io::ErrorKind)
    ///   set to [`NotFound`](std::io::ErrorKind#variant.NotFound)
    pub fn get_entry_by_name(&self, name: &str) -> Result<OleEntry, io::Error> {
        self.internal.get_entry(name)
    }

    /// Retrieves a directory entry by `id`
    ///
    /// Same as [`get_entry_by_name`](Self::get_entry_by_name) but the entry lookup is by `id`
    pub fn get_entry_by_id(&self, id: u32) -> Result<OleEntry, io::Error> {
        if id >= self.internal.dir_entries {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "Entry {} is out of range (total entries: {})",
                    id, self.internal.dir_entries
                ),
            ))
        } else {
            Ok(self.internal.read_entry(id)?)
        }
    }

    /// Returns an iterator that walks the Ole directory tree
    ///
    /// When looking for specific entries, always prefer
    /// [`get_entry_by_name()`](Self::get_entry_by_name) instead
    ///
    /// See the remarks on [`OleEntryIterator`]
    pub fn ftw(&self) -> OleEntryIterator<R> {
        OleEntryIterator {
            ole: &self.internal,
            stack: Vec::new(),
            seen: HashSet::new(),
        }
    }

    /// Returns a reader for the specified entry
    pub fn get_stream_reader(&self, entry: &OleEntry) -> OleStreamReader<R> {
        OleStreamReader::new(&self.internal, entry)
    }

    /// Returns a digraph of the red/black tree in a format suitable for dot(ting)
    pub fn digraph(&self) -> String {
        self.internal.get_digraph()
    }
}

/// The representation of a *Compound File Directory Entry*
///
/// In principle this can be one of: "Root Entry" (root directory), "Storage Object" (directory)
/// or a "Stream Object" (file)
///
/// In practice this crate is deliberately loose about entries: only major issues are fatal;
/// most other divergences from the specs are merely logged as anomalies - see
/// [`anomalies`](Self::anomalies)
///
/// This crate will happily return readers for Storage object entries
#[derive(Debug, Clone)]
pub struct OleEntry {
    /// The `id` of the entry
    pub id: u32,
    /// The object type of the entry
    ///
    /// For the unallocated type (0) all the remaining fields are meaningless - see
    /// [`is_allocated`](Self::is_allocated)
    pub objtype: u8,
    /// The `name` of the entry
    pub name: String,
    /// The red/black tree value of the entry
    pub color: u8,
    left: u32,
    right: u32,
    child: u32,
    /// The entry CLSID as an array of values
    pub clsid: GUID,
    /// The state of the entry (typically for storage objects)
    pub state: u32,
    /// The creation time of the entry
    pub ctime: Option<OffsetDateTime>,
    /// The last modification time of the entry
    pub mtime: Option<OffsetDateTime>,
    start_sector: u32,
    /// The size (in bytes) of the entry
    pub size: u64,
    /// A list of non fatal inconguences found in the entry
    pub anomalies: Vec<String>,
}

impl Default for OleEntry {
    fn default() -> Self {
        OleEntry {
            id: 0,
            name: "".to_string(),
            objtype: 0,
            color: 0,
            left: NOSTREAM,
            right: NOSTREAM,
            child: NOSTREAM,
            clsid: GUID::null(),
            state: 0,
            ctime: None,
            mtime: None,
            start_sector: ENDOFCHAIN,
            size: 0,
            anomalies: Vec::new(),
        }
    }
}

impl OleEntry {
    /// Returns [true] if the entry is allocated, [false] otherwise
    pub fn is_allocated(&self) -> bool {
        self.objtype > 0
    }

    /// Returns [true] if the entry is a Storage Object or [false] if it's a Stream Object
    pub fn is_storage(&self) -> bool {
        self.objtype == 1 || self.objtype == 5
    }

    fn left(&self) -> Option<u32> {
        if self.is_allocated() && self.left <= MAXREGSID {
            Some(self.left)
        } else {
            None
        }
    }

    fn right(&self) -> Option<u32> {
        if self.is_allocated() && self.right <= MAXREGSID {
            Some(self.right)
        } else {
            None
        }
    }

    fn child(&self) -> Option<u32> {
        if self.is_storage() && self.child <= MAXREGSID {
            Some(self.child)
        } else {
            None
        }
    }

    fn is_mini(&self) -> bool {
        self.id > 0 && self.size < 4096
    }
}

#[derive(Debug)]
struct OleCore<R: Read + Seek> {
    f: RefCell<R>,
    major_version: u16,
    minor_version: u16,
    anomalies: Vec<String>,
    sector_size: u32,
    difat: Vec<u32>,
    fat: Vec<u32>,
    minifat: Vec<u32>,
    first_dir_sector: u32,
    dir_entries: u32,
    root: OleEntry,
}

impl<R: Read + Seek> OleCore<R> {
    const ORIGTS: i64 = -11644473600;

    fn new(f: R) -> Result<Self, io::Error> {
        let mut ret = Self {
            f: RefCell::new(f),
            major_version: 0,
            minor_version: 0,
            anomalies: Vec::new(),
            sector_size: 0,
            difat: Vec::new(),
            fat: Vec::new(),
            minifat: Vec::new(),
            first_dir_sector: 0,
            dir_entries: 0,
            root: OleEntry::default(),
        };
        let mut f = ret.f.get_mut(); /* Shadowing because moved above */
        let mut header = [0u8; 8];
        f.seek(io::SeekFrom::Start(0))?;
        f.read_exact(&mut header)?;
        if header != [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1] {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Not an ole file",
            ));
        }

        let clsid = GUID::from_le_stream(&mut f)?;
        if !clsid.is_null() {
            ret.anomalies.push("CLSID is not NULL".to_string());
        }
        ret.minor_version = rdu16le(&mut f)?;
        ret.major_version = rdu16le(&mut f)?;
        if ret.minor_version != 0x003e {
            ret.anomalies.push(format!(
                "Minor version set to {:04x} instead of {:04x}",
                ret.minor_version, 0x003e
            ));
        }
        if ret.major_version < 3 || ret.major_version > 4 {
            ret.anomalies.push(format!(
                "Major version set to {} (expected 3 or 4)",
                ret.major_version
            ));
        }
        let mut tmp16 = rdu16le(&mut f)?;
        if tmp16 != 0xfffe {
            ret.anomalies.push(format!(
                "Byte order set to {:04x} instead of {:04x}",
                tmp16, 0xfffe
            ));
        }
        tmp16 = rdu16le(&mut f)?;
        ret.sector_size = match tmp16 {
            0x9 => {
                if ret.major_version == 4 {
                    ret.anomalies.push(
                        "Major version 4 should have sector size of 4096 bytes instead of 512"
                            .to_string(),
                    );
                }
                512
            }
            0xc => {
                if ret.major_version == 3 {
                    ret.anomalies.push(
                        "Major version 3 should have sector size of 512 bytes instead of 4096"
                            .to_string(),
                    );
                }
                4096
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid sector size {}", 1 << tmp16),
                ));
            }
        };

        tmp16 = rdu16le(&mut f)?;
        if tmp16 != 6 {
            ret.anomalies.push(format!(
                "Mini sector size should be set to 64 but is set to {}",
                1 << tmp16
            ));
        }

        let mut buf = [0u8; 6];
        f.read_exact(&mut buf)?;
        if buf != [0u8; 6] {
            ret.anomalies
                .push("Reserved area is not zeroed".to_string());
        }

        let mut tmp32 = rdu32le(&mut f)?;
        let dir_sectors = if ret.major_version == 3 {
            if tmp32 != 0 {
                ret.anomalies.push(format!(
                    "Number of directory sectors should be 0 for major version 3 but is {}",
                    tmp32
                ));
            }
            0
        } else {
            tmp32
        };
        let fat_sectors = rdu32le(&mut f)?;
        ret.first_dir_sector = rdu32le(&mut f)?;
        let _have_transactions = rdu32le(&mut f)? != 0;
        tmp32 = rdu32le(&mut f)?;
        if tmp32 != 0x1000 {
            ret.anomalies.push(format!(
                "Mini Stream Cutoff Size should be {:x} but is {:x}",
                0x1000, tmp32
            ));
        }
        let first_minifat_sector = rdu32le(&mut f)?;
        let minifat_sectors = rdu32le(&mut f)?;
        let first_difat_sector = rdu32le(&mut f)?;
        let difat_sectors = rdu32le(&mut f)?;
        //Note: first_difat_sector is ENDOFCHAIN if difat_sectors == 0
        let mut inline_difat = [0u8; 109 * 4]; // first 109 difats are inlined in the header
        f.read_exact(&mut inline_difat)?;
        let mut difat_complete = ret.add_difats_from_buf(&inline_difat)?;
        if difat_complete {
            if difat_sectors > 0 {
                ret.anomalies
                    .push(format!("Found {} spurious DIFAT sector(s)", difat_sectors));
            }
        } else if difat_sectors > 0 {
            let mut difat_sector = first_difat_sector;
            for i in 0..difat_sectors {
                let mut sec = ret.read_sector(difat_sector)?;
                let nextsec = sec.split_off(sec.len() - 4);
                difat_complete = ret.add_difats_from_buf(&sec)?;
                difat_sector = u32::from_le_bytes(nextsec.try_into().unwrap());
                match difat_sector {
                    ENDOFCHAIN => {
                        if i != difat_sectors - 1 {
                            ret.anomalies.push(format!(
                                "Found ENDOFCHAIN on sector {} but {} were expected",
                                i + 1,
                                difat_sectors
                            ));
                        }
                        break;
                    }
                    DIFSECT | FATSECT | FREESECT => {
                        if i == difat_sectors - 1 {
                            break;
                        }
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "Found next sector with special offset 0x{:x} in DIFAT chain",
                                difat_sector
                            ),
                        ));
                    }
                    _ => (),
                };
                if difat_complete {
                    if i != difat_sectors - 1 {
                        ret.anomalies.push(format!(
                            "DIFAT completed on sector {} but {} were expected",
                            i + 1,
                            difat_sectors
                        ));
                    }
                    break;
                }
            }
            if difat_sector != ENDOFCHAIN {
                ret.anomalies.push(format!(
                    "Missing ENDOFCHAIN on last DIFAT sector, found 0x{:x} instead",
                    difat_sector
                ));
            }
        }
        if !fat_sectors.ueq(ret.difat.len()) {
            ret.anomalies.push(format!(
                "Number of FAT sectors in header is {} but {} were found in DIFAT",
                fat_sectors,
                ret.difat.len()
            ));

            // FIXME trim difats to fat_sectors ?
        }

        ret.read_fat()?;
        for i in 0..difat_sectors {
            if let Some(&DIFSECT) = first_difat_sector
                .checked_add(i)
                .and_then(|sector| usize::try_from(sector).ok())
                .and_then(|sector| ret.fat.get(sector))
            {
            } else {
                ret.anomalies.push(
                    "One or more DIFAT sectors are unreachable or not marked as DIFSECT"
                        .to_string(),
                );
                break;
            }
        }
        ret.read_root()?;
        ret.read_minifat(first_minifat_sector, minifat_sectors)?;
        let mut next_dir_sec = ret.first_dir_sector;
        let mut counted_dir_secs: u32 = 0;
        loop {
            counted_dir_secs += 1;
            next_dir_sec = match ret.get_next_sector(next_dir_sec) {
                Ok(n) => n,
                Err(n) => match n {
                    ENDOFCHAIN => {
                        break;
                    }
                    0 => {
                        ret.anomalies.push(format!(
                            "Directory sector {} is out of FAT",
                            counted_dir_secs
                        ));
                        break;
                    }
                    _ => {
                        ret.anomalies.push(format!(
                            "Directory sector {} ends with special sector 0x{:x}",
                            counted_dir_secs, n
                        ));
                        break;
                    }
                },
            }
        }
        if ret.major_version == 4 && dir_sectors != counted_dir_secs {
            ret.anomalies.push(format!(
                "Directory sector count is {} in the header but only {} are walkable",
                dir_sectors, counted_dir_secs
            ));
        }
        if let Some(dir_entries) = (ret.sector_size / 128).checked_mul(counted_dir_secs) {
            ret.dir_entries = dir_entries;
            Ok(ret)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Ole directory entries overflow",
            ))
        }
    }

    fn add_difats_from_buf(&mut self, buf: &[u8]) -> Result<bool, io::Error> {
        assert!(
            buf.len() % 4 == 0,
            "Internal error: add_difats_from_buf called with an invalid buffer"
        );
        for i in (0..buf.len()).step_by(4) {
            let v = u32::from_le_bytes(buf[i..i + 4].try_into().unwrap());
            match v {
                DIFSECT | FATSECT | ENDOFCHAIN => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Found DIFAT sector with special offset 0x{:x}", v),
                    ));
                }
                FREESECT => return Ok(true),
                _ => {
                    // Note: to avoid problems down the road, we preemptively ensure
                    // that all difat sectors are valid usize's
                    if usize::try_from(v).is_err() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "DIFAT sector usize overflow",
                        ));
                    }
                    self.difat.push(v);
                }
            }
        }
        Ok(false)
    }

    fn read_sector(&self, sector_number: u32) -> Result<Vec<u8>, io::Error> {
        self.seek_sector(sector_number)?;
        let mut buf = vec![0u8; usize::try_from(self.sector_size).unwrap()]; // Safe: sector_size fits
        self.f.borrow_mut().read_exact(&mut buf)?;
        Ok(buf)
    }

    fn seek_sector(&self, sector_number: u32) -> Result<(), io::Error> {
        if sector_number > MAXREGSID {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Cannot seek to special sector 0x{:x}", sector_number),
            ));
        }
        if let Some(offset) =
            (u64::from(sector_number) + 1).checked_mul(u64::from(self.sector_size))
        {
            self.f
                .borrow_mut()
                .seek(io::SeekFrom::Start(offset))
                .map(|_| ())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Cannot seek to sector {}: overflow", sector_number),
            ))
        }
    }

    fn seek_mini_sector(&self, mini_sector_number: u32) -> Result<(), io::Error> {
        // FIXME: > MAXREGSID ?
        if mini_sector_number == ENDOFCHAIN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Cannot seek to special mini sector 0x{:x}",
                    mini_sector_number
                ),
            ));
        }
        let stream_position = u64::from(mini_sector_number) * 64; // always safe
        let sector_number = self.find_relative_sector(
            self.root.start_sector,
            (stream_position / u64::from(self.sector_size)) as u32, // safe bc sector_size > 64
        )?;
        let sector_offset = (stream_position % u64::from(self.sector_size)) as u32; // safe bc modulo
        let absolute_position =
            (u64::from(sector_number) + 1) * u64::from(self.sector_size) + u64::from(sector_offset);
        self.f
            .borrow_mut()
            .seek(io::SeekFrom::Start(absolute_position))
            .map(|_| ())
    }

    fn read_fat(&mut self) -> Result<(), io::Error> {
        // Note: difat sectors are guaranteed to be valid usize's - see add_difats_from_buf
        for difat_sector in self.difat.iter() {
            let sec = self.read_sector(*difat_sector)?;
            for v in sec.chunks_exact(4) {
                self.fat.push(u32::from_le_bytes(v.try_into().unwrap()));
            }
        }
        let mut not_fatsect = false;
        let mut missing_sect = false;
        for difat_sector in self.difat.iter() {
            let dfs = (*difat_sector) as usize; // See note above
            if let Some(fs) = self.fat.get(dfs) {
                if *fs != FATSECT {
                    not_fatsect = true;
                }
            } else {
                missing_sect = true;
            }
        }
        if not_fatsect {
            self.anomalies
                .push("One or more FAT sectors are not marked as FATSEC".to_string());
        }
        if missing_sect {
            self.anomalies
                .push("One or more DIFAT sectors are missing from FAT".to_string());
        }
        Ok(())
    }

    fn get_next_sector(&self, sector: u32) -> Result<u32, u32> {
        let index = usize::try_from(sector).map_err(|_| 0u32)?;
        if let Some(next_sector) = self.fat.get(index) {
            if *next_sector > MAXREGSID {
                Err(*next_sector)
            } else {
                Ok(*next_sector)
            }
        } else {
            Err(0)
        }
    }

    fn get_next_mini_sector(&self, sector: u32) -> Result<u32, u32> {
        let index = usize::try_from(sector).map_err(|_| 0u32)?;
        if let Some(next_sector) = self.minifat.get(index) {
            if *next_sector > MAXREGSID {
                Err(*next_sector)
            } else {
                Ok(*next_sector)
            }
        } else {
            Err(0)
        }
    }

    fn read_minifat(&mut self, first: u32, count: u32) -> Result<(), io::Error> {
        let mut cur = Ok(first);
        for i in 0..count {
            cur = match cur {
                Ok(nsec) => {
                    let sec = self.read_sector(nsec)?;
                    for v in sec.chunks_exact(4) {
                        self.minifat.push(u32::from_le_bytes(v.try_into().unwrap()));
                    }
                    self.get_next_sector(nsec)
                }
                Err(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "MiniFAT sector {}/{} is out of FAT (chain start: {})",
                            i + 1,
                            count,
                            first
                        ),
                    ));
                }
                Err(esec) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "MiniFAT sector {}/{} ends with special sector 0x{:x} (chain start: {})",
                            i + 1,
                            count,
                            esec,
                            first
                        ),
                    ));
                }
            }
        }
        if cur != Err(ENDOFCHAIN) {
            self.anomalies.push("MiniFAT ends prematurely".to_string());
        }
        Ok(())
    }

    fn find_relative_sector(&self, first: u32, nsec: u32) -> Result<u32, io::Error> {
        let mut ret = first;
        for i in 0..nsec {
            ret = match self.get_next_sector(ret) {
                Ok(v) => v,
                Err(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "Cannot find sector {} in FAT chain starting at {}: out of FAT after {} steps",
                            nsec, first, i
                        ),
                    ));
                }
                Err(e) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "Cannot find sector {} in FAT chain starting at {}: found special sector 0x{:x} after {} steps",
                            nsec, first, e, i
                        ),
                    ));
                }
            }
        }
        Ok(ret)
    }

    fn read_root(&mut self) -> Result<(), io::Error> {
        let root = self.read_entry(0)?;
        if root.objtype != 5 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid Root Entry: type is {} instead of 5", root.objtype),
            ));
        }
        if root.name != "Root Entry" {
            self.anomalies
                .push(format!("Root Entry name is \"{}\"", root.name));
        }
        if root.ctime.map(|t| t.unix_timestamp()) != Some(Self::ORIGTS) {
            self.anomalies
                .push(format!("Root Entry ctime is non zero {:?}", root.ctime));
        }
        if root.color != 1 {
            self.anomalies
                .push("Root Entry color is not black".to_string());
        }
        self.root = root;
        Ok(())
    }

    fn read_entry(&self, nentry: u32) -> Result<OleEntry, io::Error> {
        let entries_per_sec = self.sector_size / 128;
        let sec = self.find_relative_sector(self.first_dir_sector, nentry / entries_per_sec)?;
        let off = (nentry % entries_per_sec) * 128;
        self.seek_sector(sec)?;
        let mut f = self.f.borrow_mut();
        f.seek(io::SeekFrom::Current(off.into()))?;
        let mut buf = [0u8; 128];
        f.read_exact(&mut buf)?;
        let mut ret = OleEntry {
            id: nentry,
            objtype: buf[66],
            ..Default::default()
        };
        if ret.objtype == 5 && ret.id != 0 {
            ret.anomalies
                .push("Non Root Entry has with a root type".to_string());
        }
        if ![0, 1, 2, 5].contains(&ret.objtype) {
            ret.anomalies
                .push(format!("Invalid object type {}", ret.objtype));
        }
        if ret.objtype == 0 {
            // Unallocated: the entry is garbage
            return Ok(ret);
        }

        let namelen: usize = u16::from_le_bytes(buf[64..66].try_into().unwrap()).into();
        if namelen == 0 || namelen > 64 || namelen & 1 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid directory entry: name length {} is invalid",
                    namelen
                ),
            ));
        }
        let mut namebuf: Vec<u16> = buf[0..namelen]
            .chunks_exact(2)
            .map(|v| u16::from_le_bytes(v.try_into().unwrap()))
            .collect();
        if namebuf.pop().unwrap(/* Safe: namelen >= 2 */) != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid directory entry: name is not null terminated",
            ));
        }
        ret.name = String::from_utf16(&namebuf).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid directory entry: name is not valid UTF-16",
            )
        })?;
        const ILLEGAL_CHARS: &[char] = &['/', '\\', ':', '!'];
        if ret.name.find(ILLEGAL_CHARS).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid directory entry: name \"{}\" contains invalid characters",
                    ret.name
                ),
            ));
        }
        ret.color = buf[67];
        if ret.color > 1 {
            ret.anomalies
                .push(format!("Invalid entry color {}", ret.color));
        }

        ret.left = u32::from_le_bytes(buf[68..72].try_into().unwrap());
        ret.right = u32::from_le_bytes(buf[72..76].try_into().unwrap());
        ret.child = u32::from_le_bytes(buf[76..80].try_into().unwrap());
        if !ret.is_storage() && ret.child != NOSTREAM {
            ret.anomalies.push("Stream entry with a child".to_string());
        }

        ret.clsid = GUID::from_le_bytes(&buf[80..96]).unwrap();
        ret.state = u32::from_le_bytes(buf[96..100].try_into().unwrap());

        ret.start_sector = u32::from_le_bytes(buf[116..120].try_into().unwrap());
        ret.size = u64::from_le_bytes(buf[120..128].try_into().unwrap());
        if self.major_version == 3 {
            ret.size &= 0xffffffff;
        }
        if ret.objtype == 1 && ret.size > 0 {
            ret.anomalies.push("Storage object with data".to_string());
        }
        ret.ctime = filetime_to_datetime(u64::from_le_bytes(buf[100..108].try_into().unwrap()));
        ret.mtime = filetime_to_datetime(u64::from_le_bytes(buf[108..116].try_into().unwrap()));
        Ok(ret)
    }

    fn get_entry(&self, name: &str) -> Result<OleEntry, io::Error> {
        if name.is_empty() {
            return Ok(self.root.clone());
        }
        let name = name.trim_start_matches('/');
        let mut nextid = self.root.child;
        let mut cur = OleEntry::default(); // initted to silence warning
        let mut steps: u32 = 0;
        for part in name.to_uppercase().split('/') {
            let part_len = part.encode_utf16().count();
            loop {
                if nextid > MAXREGSID {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("Directory entry {} not found", name),
                    ));
                }
                if steps >= self.dir_entries {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!(
                            "Search aborted after {} steps: probable loop in the directory tree",
                            steps
                        ),
                    ));
                }
                cur = self.read_entry(nextid)?;
                let cname = &cur.name.to_uppercase();
                let cname_len = cname.encode_utf16().count();
                /* FIXME String comparison should use Unicode Default Case Conversion Algorithm, simple
                 * case conversion variant (simple case foldings) which is not exposed by std
                 * See https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-cfb/d30e462c-5f8a-435b-9c4c-cc0b9ea89956
                 */
                steps += 1;
                nextid = if part_len < cname_len {
                    cur.left
                } else if part_len > cname_len {
                    cur.right
                } else if part < cname {
                    cur.left
                } else if part > cname {
                    cur.right
                } else {
                    break;
                };
            }
            steps += 1;
            nextid = cur.child;
        }
        Ok(cur)
    }

    /// Returns a digraph of the Ole structure
    ///
    /// Can be used to visualize the Ole tree in software like graphviz
    pub fn get_digraph(&self) -> String {
        let mut ret = String::new();
        ret += "digraph {{";
        for i in 0..self.dir_entries {
            let entry = self.read_entry(i).unwrap();
            let mut labels = ["left", "right", "child"].iter();
            for fun in [OleEntry::left, OleEntry::right, OleEntry::child] {
                let label = labels.next().unwrap();
                if let Some(chain) = fun(&entry) {
                    let next = self.read_entry(chain).unwrap();
                    ret += &format!(
                        "\"{}::{}\" -> \"{}::{}\" [ label=\"{}\" ]\n",
                        entry.id, entry.name, next.id, next.name, *label
                    );
                }
            }
        }
        ret += "}}";
        ret
    }
}

/// An iterator that walks the Ole directory tree
///
/// # Warning
/// Due to the Ole structure, it is possible to chain entries in convoluted ways
///
/// The iterator is safe from inifinte loops but may not reach all the entries
/// from all the possible paths
pub struct OleEntryIterator<'a, R: Read + Seek> {
    ole: &'a OleCore<R>,
    stack: Vec<(String, u32)>,
    seen: HashSet<u32>,
}

impl<R: Read + Seek> Iterator for OleEntryIterator<'_, R> {
    /// A tuple consisting of:
    /// * A `/` separated path
    /// * An Ole directory Entry
    type Item = (String, OleEntry);

    fn next(&mut self) -> Option<Self::Item> {
        if self.seen.is_empty() {
            if let Some(first_child) = self.ole.root.child() {
                self.seen.insert(0);
                self.stack.push(("".to_string(), first_child));
            }
        }
        if let Some((path, next_id)) = self.stack.pop() {
            if let Ok(cur) = self.ole.read_entry(next_id) {
                let mut next_path = &format!("{}{}/", path, cur.name);
                for f in [OleEntry::child, OleEntry::right, OleEntry::left] {
                    if let Some(next_id) = f(&cur) {
                        if self.seen.insert(next_id) {
                            self.stack.push((next_path.to_string(), next_id));
                        }
                    }
                    next_path = &path;
                }
                return Some((format!("{}{}", path, cur.name), cur));
            }
        }
        None
    }
}

/// A reader for an *Ole Stream Object*
pub struct OleStreamReader<'a, R: Read + Seek> {
    ole: &'a OleCore<R>,
    start_sector: u32,
    current_sector: u32,
    is_mini: bool,
    size: u64,
    done: u64,
    data: Vec<u8>,
    dirty: bool,
    end_of_last_full_block: u64,
}

impl<'a, R: Read + Seek> OleStreamReader<'a, R> {
    fn new(ole: &'a OleCore<R>, entry: &OleEntry) -> Self {
        let data_len = if entry.is_mini() {
            64
        } else {
            ole.sector_size as usize // Safe as sector_size max is 4096
        };
        let end_of_last_full_block = entry.size - (entry.size % data_len as u64);
        Self {
            ole,
            start_sector: entry.start_sector,
            current_sector: entry.start_sector,
            is_mini: entry.is_mini(),
            size: entry.size,
            done: 0,
            data: vec![0; data_len],
            dirty: true,
            end_of_last_full_block,
        }
    }

    fn read_current_sector(&mut self) -> io::Result<()> {
        assert!(
            self.current_sector <= MAXREGSID,
            "Internal error: read_current_sector called past EOF"
        );
        assert!(
            self.dirty,
            "Internal error: read_current_sector called with clean buffer"
        );
        assert!(
            self.done < self.size,
            "Internal error: read_current_sector called past EOF"
        );
        if self.is_mini {
            self.ole.seek_mini_sector(self.current_sector)?;
        } else {
            self.ole.seek_sector(self.current_sector)?;
        }
        let remaining = if self.done >= self.end_of_last_full_block {
            // Last sector is not full: limit read size
            (self.size - self.end_of_last_full_block) as usize
        } else {
            // Either non last or fully populated sector
            self.data.len()
        };
        self.ole
            .f
            .borrow_mut()
            .read_exact(&mut self.data[0..remaining])?;
        self.dirty = false;
        Ok(())
    }

    fn move_to_next_sector(&mut self) {
        self.current_sector = if self.is_mini {
            self.ole
                .get_next_mini_sector(self.current_sector)
                .unwrap_or(MAXREGSID + 1)
        } else {
            self.ole
                .get_next_sector(self.current_sector)
                .unwrap_or(MAXREGSID + 1)
        };
        self.dirty = true;
    }

    fn _rewind(&mut self) -> io::Result<()> {
        if self.current_sector != self.start_sector {
            self.current_sector = self.start_sector;
            self.dirty = true;
        }
        self.done = 0;
        Ok(())
    }

    fn _seek(&mut self, new_pos: u64) -> io::Result<u64> {
        if new_pos != self.done {
            if new_pos == 0 {
                self._rewind()?;
                return Ok(0);
            }
            if new_pos >= self.size {
                self.current_sector = MAXREGSID + 1;
                self.done = new_pos;
                self.dirty = true;
                return Ok(new_pos);
            }
            if new_pos < self.done {
                // Ole streams are singly linked: restart from the beginning
                self._rewind()?;
            }
            let sector_size = u64::try_from(self.data.len()).unwrap(); // Safe bc either sector_size or 64
            let target_sector_ord = new_pos / sector_size;
            let mut current_sector_ord = self.done / sector_size;
            while current_sector_ord < target_sector_ord {
                self.move_to_next_sector(); // sets dirty flag
                if self.current_sector > MAXREGSID {
                    // The sector chain ends before the stream size is reached
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "Entry is out of {}",
                            if self.is_mini { "MiniFAT" } else { "FAT" }
                        ),
                    ));
                }
                current_sector_ord += 1;
            }
            self.done = new_pos;
        }
        Ok(new_pos)
    }

    fn get_data_offset(&self) -> usize {
        (self.done as usize) % self.data.len() // Cast is safe bc mod
    }
}

impl<R: Read + Seek> Read for OleStreamReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut bytes_written: usize = 0;
        while bytes_written < buf.len() && self.done < self.size {
            if self.current_sector > MAXREGSID {
                break;
            }
            if self.dirty {
                self.read_current_sector()?;
            }
            let avail_stream_len = self.size - self.done;
            let data_offset = self.get_data_offset();
            let avail_data_len = self.data.len() - data_offset;
            let avail_in_len = avail_data_len.umin(avail_stream_len);
            let avail_out_len = buf.len() - bytes_written;
            let copy_len = avail_in_len.umin(avail_out_len);
            let src = &self.data[data_offset..(data_offset + copy_len)];
            let dst = &mut buf[bytes_written..(bytes_written + copy_len)];
            dst.copy_from_slice(src);
            self._seek(self.done + copy_len as u64)?; // Cast is safe due to umin
            bytes_written += copy_len;
        }
        Ok(bytes_written)
    }
}

impl<R: Read + Seek> Seek for OleStreamReader<'_, R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let new_pos: Option<u64> = match pos {
            io::SeekFrom::Start(v) => Some(v),
            io::SeekFrom::End(v) => u64::try_from(i128::from(self.size) + i128::from(v)).ok(),
            io::SeekFrom::Current(v) => u64::try_from(i128::from(self.done) + i128::from(v)).ok(),
        };
        if let Some(new_pos) = new_pos {
            self._seek(new_pos)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Illegal seek position",
            ))
        }
    }
}

/// Helper struct describing encryption
#[derive(Debug, Clone, Serialize)]
pub struct Encryption {
    /// Encryption algorithm name
    pub algorithm: String,
    /// Matching password
    pub password: String,
}

/// Error indicating lack of password needed to decrypt file
#[derive(Debug)]
pub struct NoValidPasswordError {
    algorithm: String,
}
impl std::fmt::Display for NoValidPasswordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "No valid password provided ({})", self.algorithm)
    }
}
impl std::error::Error for NoValidPasswordError {}
impl NoValidPasswordError {
    /// Creates new NoPasswordError
    pub fn new<S: AsRef<str>>(algorithm: S) -> Self {
        Self {
            algorithm: algorithm.as_ref().to_string(),
        }
    }
    /// Creates new std::io::Error containing NoPasswordError
    pub fn new_io_error<S: AsRef<str>>(algorithm: S) -> io::Error {
        io::Error::new(io::ErrorKind::Other, NoValidPasswordError::new(algorithm))
    }

    /// Returns algorithm
    pub fn algorithm(&self) -> String {
        self.algorithm.to_string()
    }
}
