//! Unzip library
//!
//! Written from scratch, based on APPNOTE 6.3.10
//!
//! # Design goals and implementation #
//!
//! The main development goal, besides data decompression, is to provide
//! extensive and transparent access to the numerous zip structures and
//! the metadata held within them
//!
//! The code is a mixture of native implementations, native crates and
//! FFI-wrapped external libraries (possibly imported through a crate).
//! The native implementation is always preferred unless a native crate
//! exists which is proven and reputable. A native implementation (code
//! or crate) is always preferred unless a FFI library exists which is
//! proven, reputable and performs at least 2x better.
//!
//! Two interfaces are provided:
//! - [`Zip`]: a lower-level iterative interface which visits each entry
//!   sequentially
//! - [`ZipRandomAccess`]: a higher-level interface providing access to
//!   entries by name (see caveats)
//!
//! # Supported Zip features #
//!
//! Supported methods:
//! - Store
//! - Shrink
//! - Reduce
//! - Implode
//! - Deflate
//! - Enhanced deflate (Deflate64)
//! - Bzip2
//! - LZMA (LZMA1)
//! - Zstandard
//! - XZ (LZMA2)
//!
//! Supported encryption types:
//! - Traditional PKWARE encryption
//! - WinZip AE-1 and AE-2 (AES128, AES192, AES256)
//!
//! # Examples #
//! ```no_run
//! use ctxunzip::Zip;
//!
//! let file = std::fs::File::open("archive.zip").unwrap();
//! let archive = Zip::new(file).unwrap();
//! let max_output_size = 12345u64;
//! for entry in archive {
//!     // Decompress the entry
//!     std::io::copy(&mut entry.unwrap().take_reader(max_output_size).unwrap(), &mut std::io::sink()).unwrap();
//! }

pub mod crypto;
mod expand;
mod explode;
mod inflate;
mod inflate64;
pub mod lzma;
mod unshrink;
mod utils;

use ctxutils::io::{rdu16le, rdu32le, rdu64le};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hasher;
use std::io::{Read, Seek};
use std::rc::Rc;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn};

const EOCD_SIGNATURE: &[u8] = b"PK\x05\x06";
const Z64_EOCD_LOCATOR_SIGNATURE: &[u8] = b"PK\x06\x07";
const Z64_EOCD_SIGNATURE: &[u8] = b"PK\x06\x06";
const CENTRAL_HEADER_SIGNATURE: &[u8] = b"PK\x01\x02";
const LOCAL_HEADER_SIGNATURE: &[u8] = b"PK\x03\x04";
const DATA_DESCRYPTOR_SIGNATURE: &[u8] = b"PK\x07\x08";

const GP_IS_ENCRYPTED: u16 = 1 << 0;
const GP_HAS_DATA_DESCRIPTOR: u16 = 1 << 3;
const GP_IS_STRONG_ENCRYPTED: u16 = 1 << 6;

#[derive(Default, Debug, Clone, Copy)]
struct SfxAdjustment(i64);

impl SfxAdjustment {
    fn adjust(&self, value: u64) -> u64 {
        value.checked_add_signed(self.0).unwrap_or(value)
    }
}

#[derive(Debug)]
/// End of central directory
pub struct EndOfCentralDirectory {
    /// number of this disk
    pub disk_number: u16,
    /// number of the disk with the start of the central directory
    pub cd_first_disk: u16,
    /// total number of entries in the central directory on this disk
    pub entries_this_disk: u16,
    /// total number of entries in the central directory
    pub entries_total: u16,
    /// size of the central directory
    pub cd_size: u32,
    /// offset of start of central directory with respect to the starting disk number
    pub cd_offset_on_first_disk: u32,
    /// archive comment
    pub comment: Vec<u8>,
    adjustment: SfxAdjustment,
}

impl EndOfCentralDirectory {
    /// Try to read and parse underlying data into a `EndOfCentralDirectory` structure.
    fn new<R: Read>(mut r: R, at_offset: u64) -> Result<Self, std::io::Error> {
        let mut signature = [0u8; 4];
        r.read_exact(&mut signature)?;
        assert_eq!(&signature, EOCD_SIGNATURE);
        let mut ret = Self {
            disk_number: rdu16le(&mut r)?,
            cd_first_disk: rdu16le(&mut r)?,
            entries_this_disk: rdu16le(&mut r)?,
            entries_total: rdu16le(&mut r)?,
            cd_size: rdu32le(&mut r)?,
            cd_offset_on_first_disk: rdu32le(&mut r)?,
            comment: Vec::new(),
            adjustment: SfxAdjustment::default(),
        };
        if ret.cd_offset_on_first_disk != 0xffffffff {
            if let Some(adjusted_cd_offset) = at_offset.checked_sub(u64::from(ret.cd_size)) {
                if u64::from(ret.cd_offset_on_first_disk) != adjusted_cd_offset {
                    ret.adjustment = SfxAdjustment(
                        adjusted_cd_offset.wrapping_sub(u64::from(ret.cd_offset_on_first_disk))
                            as i64,
                    );
                    debug!("SFX stub adjustment: {:?} bytes", ret.adjustment);
                }
            }
        }
        let comment_len = rdu16le(&mut r)?;
        if comment_len > 0 {
            r.take(comment_len.into()).read_to_end(&mut ret.comment)?;
            if ret.comment.len() != usize::from(comment_len) {
                warn!("Zip comment is truncated");
            }
        }
        Ok(ret)
    }
}

#[derive(Debug)]
/// End of central directory locator (zip64)
struct Z64EndOfCentralDirectoryLocator {
    /// number of the disk with the start of the zip64 end of central directory
    z64_eocd_first_disk: u32,
    /// relative offset of the zip64 end of central directory record
    z64_eocd_offset: u64,
    /// total number of disks
    _z64_n_disks: u32,
}

impl Z64EndOfCentralDirectoryLocator {
    /// Try to read and parse underlying data into a `Z64EndOfCentralDirectoryLocator` structure.
    fn new<R: Read>(mut r: R) -> Result<Option<Self>, std::io::Error> {
        let mut signature = [0u8; 4];
        r.read_exact(&mut signature)?;
        if signature != Z64_EOCD_LOCATOR_SIGNATURE {
            Ok(None)
        } else {
            Ok(Some(Self {
                z64_eocd_first_disk: rdu32le(&mut r)?,
                z64_eocd_offset: rdu64le(&mut r)?,
                _z64_n_disks: rdu32le(&mut r)?,
            }))
        }
    }
}

#[derive(Debug)]
/// End of central directory (zip64)
pub struct Z64EndOfCentralDirectory {
    /// version made by
    pub ver_made_by: u16,
    /// version needed to extract
    pub ver_to_extract: u16,
    /// number of this disk
    pub disk_number: u32,
    /// number of the disk with the start of the central directory
    pub cd_first_disk: u32,
    /// total number of entries in the central directory on this disk
    pub entries_this_disk: u64,
    /// total number of entries in the central directory
    pub entries_total: u64,
    /// size of the central directory
    pub cd_size: u64,
    /// offset of start of central directory with respect to the starting disk number
    pub cd_offset_on_first_disk: u64,
}

impl Z64EndOfCentralDirectory {
    /// Try to read and parse underlying data into a `Z64EndOfCentralDirectory` structure.
    fn new<R: Read>(mut r: R) -> Result<Self, std::io::Error> {
        let mut signature = [0u8; 4];
        r.read_exact(&mut signature)?;
        if signature != Z64_EOCD_SIGNATURE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid Zip64 end of central directory signature",
            ));
        }
        if rdu64le(&mut r)? < 44 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid Zip64 end of central directory length",
            ));
        }
        Ok(Self {
            ver_made_by: rdu16le(&mut r)?,
            ver_to_extract: rdu16le(&mut r)?,
            disk_number: rdu32le(&mut r)?,
            cd_first_disk: rdu32le(&mut r)?,
            entries_this_disk: rdu64le(&mut r)?,
            entries_total: rdu64le(&mut r)?,
            cd_size: rdu64le(&mut r)?,
            cd_offset_on_first_disk: rdu64le(&mut r)?,
        })
    }
}

/// Zip decompressor interface
pub struct Zip<R: Read + Seek> {
    /// The Zip source
    r: Rc<RefCell<R>>,
    /// End of central directory
    pub eocd: EndOfCentralDirectory,
    /// End of central directory (zip64 version)
    pub z64eocd: Option<Z64EndOfCentralDirectory>,
}

impl<R: Read + Seek> Zip<R> {
    /// Creates a new Zip processor
    pub fn new(mut r: R) -> Result<Self, std::io::Error> {
        const BUFSIZ: u64 = 4 * 1024;
        let mut buf = [0u8; BUFSIZ as usize];
        let fsize = r.seek(std::io::SeekFrom::End(0))?;
        let mut off = fsize.saturating_sub(BUFSIZ);
        let (eocd, z64loc) = 'eocd: loop {
            debug!("Scanning for end of central header @{off:x}");
            off = r.seek(std::io::SeekFrom::Start(off))?;
            let avail: u64 = (fsize - off).min(BUFSIZ);
            let buflen: usize = avail.try_into().unwrap();
            r.read_exact(&mut buf[0..buflen])?;
            for found_at in memchr::memmem::rfind_iter(&buf[0..buflen], EOCD_SIGNATURE) {
                let found_off = off + u64::try_from(found_at).unwrap();
                debug!("Found end of central header @{:x}", found_off);
                let eocd = match EndOfCentralDirectory::new(&buf[found_at..], found_off) {
                    Ok(eocd) => {
                        debug!("EOCD ok (from buf)");
                        eocd
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                        r.seek(std::io::SeekFrom::Start(found_off))?;
                        if let Ok(eocd) = EndOfCentralDirectory::new(&mut r, found_off) {
                            debug!("EOCD ok (from stream)");
                            eocd
                        } else {
                            continue;
                        }
                    }
                    Err(e) => return Err(e),
                };
                let z64loc = if found_at >= 20 {
                    Z64EndOfCentralDirectoryLocator::new(&buf[(found_at - 20)..found_at])?
                } else if found_off >= 20 {
                    r.seek(std::io::SeekFrom::Start(found_off - 20))?;
                    Z64EndOfCentralDirectoryLocator::new(&mut r)?
                } else {
                    None
                };
                break 'eocd (eocd, z64loc);
            }
            if off == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Not a valid zip file (end of central directory not found)",
                ));
            }
            off = off.saturating_sub(BUFSIZ - 3);
        };
        let z64eocd = if let Some(loc) = z64loc.as_ref() {
            if loc.z64_eocd_first_disk != u32::from(eocd.disk_number) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Zip64 end of central directory is on a different disk",
                ));
            }
            r.seek(std::io::SeekFrom::Start(loc.z64_eocd_offset))?;
            let z64eocd = Z64EndOfCentralDirectory::new(&mut r)?;
            if z64eocd.disk_number != loc.z64_eocd_first_disk {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Zip64 end of central directory and its locator do not agree on this disk number",
                ));
            }
            if z64eocd.cd_first_disk != z64eocd.disk_number {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Zip64 central directory is on a different disk",
                ));
            }
            Some(z64eocd)
        } else {
            if eocd.cd_first_disk != eocd.disk_number {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Central directory is on a different disk",
                ));
            }
            None
        };
        debug!("EOCD: {:#x?}", eocd);
        debug!("Zip64 EOCD locator: {:#x?}", z64loc);
        debug!("Z64EOCD: {:#x?}", z64eocd);
        Ok(Self {
            r: Rc::new(RefCell::new(r)),
            eocd,
            z64eocd,
        })
    }

    /// Whether the archive is Zip64
    pub fn is_zip64(&self) -> bool {
        self.z64eocd.is_some()
    }

    /// Number of this disk
    pub fn get_disk_number(&self) -> u32 {
        self.z64eocd
            .as_ref()
            .map(|eocd| eocd.disk_number)
            .unwrap_or_else(|| self.eocd.disk_number.into())
    }

    /// Number of the disk with the start of the central directory
    pub fn get_cd_first_disk(&self) -> u32 {
        self.z64eocd
            .as_ref()
            .map(|eocd| eocd.cd_first_disk)
            .unwrap_or_else(|| self.eocd.cd_first_disk.into())
    }

    /// Total number of entries in the central directory on this disk
    pub fn get_entries_this_disk(&self) -> u64 {
        self.z64eocd
            .as_ref()
            .map(|eocd| eocd.entries_this_disk)
            .unwrap_or_else(|| self.eocd.entries_this_disk.into())
    }

    /// Total number of entries in the central directory
    pub fn get_entries_total(&self) -> u64 {
        self.z64eocd
            .as_ref()
            .map(|eocd| eocd.entries_total)
            .unwrap_or_else(|| self.eocd.entries_total.into())
    }

    /// Size of the central directory
    pub fn get_cd_size(&self) -> u64 {
        self.z64eocd
            .as_ref()
            .map(|eocd| eocd.cd_size)
            .unwrap_or_else(|| self.eocd.cd_size.into())
    }

    /// Offset of start of central directory with respect to the starting disk number
    pub fn get_cd_offset_on_first_disk(&self) -> u64 {
        self.z64eocd
            .as_ref()
            .map(|eocd| eocd.cd_offset_on_first_disk)
            .unwrap_or_else(|| self.eocd.cd_offset_on_first_disk.into())
    }

    /// Adjust the given offset taking into consideration the SFX stub size
    fn sfx_adjust(&self, offset: u64) -> u64 {
        if self.z64eocd.is_none() {
            self.eocd.adjustment.adjust(offset)
        } else {
            offset
        }
    }

    /// Provides access to archive comment field
    pub fn comment(&self) -> &[u8] {
        &self.eocd.comment
    }

    /// Creates a non-consuming iterator
    pub fn iter(&self) -> ZipIterator<R> {
        ZipIterator {
            total_entries: self.get_entries_total(),
            current_entry: 0,
            current_offset: self.sfx_adjust(self.get_cd_offset_on_first_disk()),
            zip: self,
        }
    }
}

/// A non-consuming iterator over Zip entries
pub struct ZipIterator<'a, R: Read + Seek> {
    /// A Zip over which the iteration is going on
    zip: &'a Zip<R>,
    /// Total number of entries in the archive, used to identify when to stop iterating.
    total_entries: u64,
    /// Current entry of iteration.
    current_entry: u64,
    /// An offset pointing to central header signature of the "next" entry to read.
    current_offset: u64,
}

impl<'a, R: Read + Seek> IntoIterator for &'a Zip<R> {
    type Item = Result<Entry<R>, std::io::Error>;
    type IntoIter = ZipIterator<'a, R>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, R: Read + Seek> Iterator for ZipIterator<'a, R> {
    type Item = Result<Entry<R>, std::io::Error>;

    /// Try to iterate to the next entry.
    /// If next entry is available - try to parse its underlying structures into an `Entry`.
    fn next(&mut self) -> Option<Self::Item> {
        match self.current_entry == self.total_entries {
            true => None,
            false => Some({
                self.current_entry += 1;
                if let Err(e) = self
                    .zip
                    .r
                    .borrow_mut()
                    .seek(std::io::SeekFrom::Start(self.current_offset))
                {
                    return Some(Err(e));
                }
                let central_header = match CentralHeader::new(&self.zip.r) {
                    Ok(ch) => ch,
                    Err(e) => return Some(Err(e)),
                };
                trace!("{:#x?}", central_header);
                self.current_offset = match self.zip.r.borrow_mut().stream_position() {
                    Ok(pos) => pos,
                    Err(e) => return Some(Err(e)),
                };

                Ok(Entry::new(self.zip, central_header))
            }),
        }
    }
}

/// A consuming iterator over Zip entries
pub struct ZipIntoIterator<R: Read + Seek> {
    /// A Zip over which the iteration is going on
    zip: Zip<R>,
    /// Total number of entries in the archive, used to identify when to stop iterating.
    total_entries: u64,
    /// Current entry of iteration.
    current_entry: u64,
    /// An offset pointing to central header signature of the "next" entry to read.
    current_offset: u64,
}

impl<R: Read + Seek> IntoIterator for Zip<R> {
    type Item = Result<Entry<R>, std::io::Error>;
    type IntoIter = ZipIntoIterator<R>;

    fn into_iter(self) -> Self::IntoIter {
        Self::IntoIter {
            total_entries: self.get_entries_total(),
            current_entry: 0,
            current_offset: self.sfx_adjust(self.get_cd_offset_on_first_disk()),
            zip: self,
        }
    }
}

impl<R: Read + Seek> Iterator for ZipIntoIterator<R> {
    type Item = Result<Entry<R>, std::io::Error>;

    /// Try to iterate to the next archive entry.
    /// If next entry is available - try to parse its underlying structures into an `Entry`.
    fn next(&mut self) -> Option<Self::Item> {
        match self.current_entry == self.total_entries {
            true => None,
            false => Some({
                self.current_entry += 1;
                if let Err(e) = self
                    .zip
                    .r
                    .borrow_mut()
                    .seek(std::io::SeekFrom::Start(self.current_offset))
                {
                    return Some(Err(e));
                }
                let central_header = match CentralHeader::new(&self.zip.r) {
                    Ok(ch) => ch,
                    Err(e) => return Some(Err(e)),
                };
                trace!("{:#x?}", central_header);
                self.current_offset = match self.zip.r.borrow_mut().stream_position() {
                    Ok(pos) => pos,
                    Err(e) => return Some(Err(e)),
                };

                Ok(Entry::new(&self.zip, central_header))
            }),
        }
    }
}

#[derive(Debug, Clone)]
/// A Zip entry central header
pub struct CentralHeader {
    /// Software version used to create an entry.
    pub ver_made_by: u16,
    /// Minimal software version required for decompression/extraction.
    pub ver_to_extract: u16,
    /// General purpose bit flags.
    pub gp_flag: u16,
    /// Compression method used for the entry.
    pub compression_method: u16,
    /// Date/time of last modification of the file. Could be `None` if binary representation could
    /// not be parsed into valid date/time.
    pub mtime: Option<time::PrimitiveDateTime>,
    /// Expected CRC-32 checksum.
    pub crc32: u32,
    /// Compressed size in bytes.
    pub compressed_size: u64,
    /// Uncompressed size in bytes.
    pub uncompressed_size: u64,
    /// Byte-representation of the file name.
    pub file_name: Vec<u8>,
    /// File comment.
    pub comment: Vec<u8>,
    /// Extra fields.
    pub extras: ExtraFields,
    /// Archive's "disk number" where the entry starts.
    pub disk_number: u32,
    /// Internal file attributes.
    pub internal_attributes: u16,
    /// External file attributes.
    pub external_attributes: u32,
    /// Relative offset of local header.
    pub local_header_offset: u64,
}

impl CentralHeader {
    /// Attempts to read and parse Zip entry central header structure starting from current
    /// position of the provided Read implementation.
    fn new<R: Read>(r: &Rc<RefCell<R>>) -> Result<Self, std::io::Error> {
        let r = &mut *r.borrow_mut();
        let mut signature = [0u8; 4];
        r.read_exact(&mut signature)?;
        if signature != CENTRAL_HEADER_SIGNATURE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid central header signature",
            ));
        }
        let ver_made_by = rdu16le(r)?;
        let ver_to_extract = rdu16le(r)?;
        let gp_flag = rdu16le(r)?;
        let compression_method = rdu16le(r)?;
        let dostime = rdu16le(r)?;
        let dosdate = rdu16le(r)?;
        let mtime = dostime_to_time(dosdate, dostime);
        let crc32 = rdu32le(r)?;
        let mut compressed_size = rdu32le(r)?.into();
        let mut uncompressed_size = rdu32le(r)?.into();
        let fname_len = rdu16le(r)?;
        let extra_len = rdu16le(r)?;
        let comment_len = rdu16le(r)?;
        let mut disk_number = rdu16le(r)?.into();
        let internal_attributes = rdu16le(r)?;
        let external_attributes = rdu32le(r)?;
        let mut local_header_offset = rdu32le(r)?.into();
        let mut file_name = vec![0u8; fname_len.into()];
        r.read_exact(&mut file_name)?;
        let extras = ExtraFields::new(r, extra_len.into())?;
        let mut comment = vec![0u8; comment_len.into()];
        r.read_exact(&mut comment)?;

        // FIXME: what to do if z64 overflows? Use local data or?
        if let Some(mut buf) = extras.field_data(0x0001) {
            for val in [
                &mut uncompressed_size,
                &mut compressed_size,
                &mut local_header_offset,
            ] {
                if *val == 0xffffffff {
                    if let Ok(v) = rdu64le(&mut buf) {
                        *val = v;
                    } else {
                        warn!("Zip64 field overflow");
                        break;
                    }
                }
            }
            if disk_number == 0xffff {
                if let Ok(v) = rdu32le(&mut buf) {
                    disk_number = v;
                } else {
                    warn!("Zip64 field overflow");
                }
            }
        }
        Ok(Self {
            ver_made_by,
            ver_to_extract,
            gp_flag,
            compression_method,
            mtime,
            crc32,
            compressed_size,
            uncompressed_size,
            file_name,
            comment,
            extras,
            disk_number,
            internal_attributes,
            external_attributes,
            local_header_offset,
        })
    }

    /// A lossy UTF-8 representation of the entry file name
    ///
    /// The for the raw version see [`file_name`](Self::file_name)
    pub fn name(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.file_name)
    }
}

#[derive(Debug)]
/// A Zip entry local header
pub struct LocalHeader {
    /// Minimal software version required for decompression/extraction.
    pub ver_to_extract: u16,
    /// General purpose bit flags.
    pub gp_flag: u16,
    /// Compression method used for the entry.
    pub compression_method: u16,
    /// Time of last modification of the file. Could be `None` if binary representation could not
    /// be parsed into valid date/time.
    pub mtime: Option<time::PrimitiveDateTime>,
    /// Expected CRC-32 checksum.
    pub crc32: u32,
    /// Compressed size in bytes.
    pub compressed_size: u64,
    /// Uncompressed size in bytes.
    pub uncompressed_size: u64,
    /// Byte-representation of the file name.
    pub file_name: Vec<u8>,
    /// Extra fields.
    pub extras: ExtraFields,
    /// A field used for password check in Traditional PKWARE encryption.
    pwdcheck: u16,
}

impl LocalHeader {
    fn new<R: Read + Seek>(z: &Zip<R>, ch: &CentralHeader) -> Result<Self, std::io::Error> {
        let r = &mut *z.r.borrow_mut();
        r.seek(std::io::SeekFrom::Start(
            z.sfx_adjust(ch.local_header_offset),
        ))?;
        let mut signature = [0u8; 4];
        r.read_exact(&mut signature)?;
        if signature != LOCAL_HEADER_SIGNATURE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid local header signature",
            ));
        }
        let ver_to_extract = rdu16le(r)?;
        let gp_flag = rdu16le(r)?;
        let compression_method = rdu16le(r)?;
        let dostime = rdu16le(r)?;
        let dosdate = rdu16le(r)?;
        let mtime = dostime_to_time(dosdate, dostime);
        let mut crc32 = rdu32le(r)?;
        let mut compressed_size = rdu32le(r)?.into();
        let mut uncompressed_size = rdu32le(r)?.into();
        let fname_len = rdu16le(r)?;
        let extra_len = rdu16le(r)?;
        let mut file_name = vec![0u8; fname_len.into()];
        r.read_exact(&mut file_name)?;
        let extras = ExtraFields::new(r, extra_len.into())?;
        let is_zip64 = if let Some(mut buf) = extras.field_data(0x0001) {
            for val in [&mut uncompressed_size, &mut compressed_size] {
                if *val == 0xffffffff {
                    if let Ok(v) = rdu64le(&mut buf) {
                        *val = v;
                    } else {
                        warn!("Zip64 field overflow");
                        break;
                    }
                }
            }
            true
        } else {
            false
        };
        let data_offset = r.stream_position()?;
        let pwdcheck = if gp_flag & GP_HAS_DATA_DESCRIPTOR != 0 {
            // Have masked values in headers due to streaming and a data descriptor appended
            r.seek(std::io::SeekFrom::Current(
                ch.compressed_size.try_into().map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Cannot convert compressed size to i64",
                    )
                })?,
            ))?;
            let maybe_signature = rdu32le(r)?;
            crc32 = if maybe_signature
                == u32::from_le_bytes(DATA_DESCRYPTOR_SIGNATURE.try_into().unwrap())
            {
                rdu32le(r)?
            } else {
                maybe_signature
            };
            compressed_size = if is_zip64 {
                rdu64le(r)?
            } else {
                rdu32le(r)?.into()
            };
            uncompressed_size = if is_zip64 {
                rdu64le(r)?
            } else {
                rdu32le(r)?.into()
            };
            dostime
        } else {
            (crc32 >> 16) as u16
        };

        r.seek(std::io::SeekFrom::Start(data_offset))?;

        Ok(Self {
            ver_to_extract,
            gp_flag,
            compression_method,
            mtime,
            crc32,
            compressed_size,
            uncompressed_size,
            file_name,
            extras,
            pwdcheck,
        })
    }

    /// A lossy UTF-8 representation of the entry file name
    ///
    /// The for the raw version see [`file_name`](Self::file_name)
    pub fn name(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.file_name)
    }
}

impl PartialEq<LocalHeader> for CentralHeader {
    fn eq(&self, local: &LocalHeader) -> bool {
        // Note: ignoring ver_to_extract and extras as they mismatch a lot
        self.gp_flag == local.gp_flag
            && self.compression_method == local.compression_method
            && self.mtime == local.mtime
            && self.crc32 == local.crc32
            && self.compressed_size == local.compressed_size
            && self.uncompressed_size == local.uncompressed_size
            && self.file_name == local.file_name
    }
}

fn dostime_to_time(date: u16, time: u16) -> Option<time::PrimitiveDateTime> {
    let year: i32 = (((date >> 9) & 0x7f) + 1980).into();
    let month = time::Month::try_from(((date >> 5) & 0xf) as u8).ok()?;
    let day: u8 = (date & 0x1f) as u8;
    let hour: u8 = ((time >> 11) & 0x1f) as u8;
    let minute: u8 = ((time >> 5) & 0x3f) as u8;
    let second: u8 = ((time & 0x1f) << 1) as u8;
    Some(time::PrimitiveDateTime::new(
        time::Date::from_calendar_date(year, month, day).ok()?,
        time::Time::from_hms(hour, minute, second).ok()?,
    ))
}

#[derive(Debug, Clone)]
/// Zip extra fields
pub struct ExtraFields(Vec<u8>);
impl ExtraFields {
    /// Creates a new `ExtraFields` structure. Could fail if underlying `read_exact()` fails.
    fn new<R: Read>(r: &mut R, len: usize) -> Result<Self, std::io::Error> {
        let mut data = vec![0u8; len];
        r.read_exact(&mut data)?;
        Ok(Self(data))
    }

    /// Provides the total length of extra fields in bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the [`ExtraFields`] instance holds no data.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Retrieves the field data for the given `id`, if field with such identifier is present.
    pub fn field_data(&self, field_id: u16) -> Option<&[u8]> {
        let mut extradata: &[u8] = self.0.as_ref();
        loop {
            let id = rdu16le(&mut extradata).ok()?;
            let len: usize = rdu16le(&mut extradata).ok()?.into();
            if id == field_id {
                return extradata.get(..len);
            }
            extradata = extradata.get(len..)?;
        }
    }
}

/// An [`Entry`]'s apparent file type
#[derive(Debug, PartialEq)]
pub enum FileType {
    /// A variant to represent Text file type.
    Text,
    /// A variant to represent Binary file type.
    Binary,
}

impl AsRef<str> for FileType {
    /// Map a variant to its `&str` representation.
    fn as_ref(&self) -> &str {
        match self {
            FileType::Text => "text",
            FileType::Binary => "binary",
        }
    }
}

/// A Zip entry
pub struct Entry<R: Read + Seek> {
    /// An interface to a generic which implements Read + Seek.
    /// When `Entry` is constructed the value contains `Some` variant.
    /// And during the call to `take_reader` method it is replaced with `None` variant, to prevent
    /// creation of more than one `EntryReader`.
    reader: Option<Rc<RefCell<R>>>,
    /// Central header of corresponding archive entry.
    pub central_header: CentralHeader,
    /// Optional local header of corresponding archive entry.
    pub local_header: Option<LocalHeader>,
    /// A variant representing encryption type of the archive entry, which also provides an access
    /// to general and corresponding encryption interfaces.
    pub encryption: crypto::ZipEncryption,
    /// Compressed size in bytes.
    pub csize: u64,
    /// An u16 which represents a compression method.
    pub cmethod: u16,
    /// A place to hold a previously occurred error.
    err: String,
}

impl<R: Read + Seek> Entry<R> {
    fn new(z: &Zip<R>, central_header: CentralHeader) -> Self {
        let mut err = String::new();
        let mut csize = central_header.compressed_size;
        let mut cmethod = central_header.compression_method;
        let mut local_header: Option<LocalHeader> = None;
        if central_header.disk_number != z.get_disk_number() {
            err = "This entry is on a different disk".to_string();
        } else {
            match LocalHeader::new(z, &central_header) {
                Ok(lh) => {
                    csize = lh.compressed_size;
                    cmethod = lh.compression_method;
                    local_header = Some(lh);
                }
                Err(e) => err = format!("Local header error: {}", e),
            }
        };
        debug!("{:#x?}", local_header);
        let mut ret = Self {
            reader: Some(z.r.clone()),
            central_header,
            local_header,
            encryption: crypto::ZipEncryption::Null,
            csize,
            cmethod,
            err,
        };
        ret.parse_encryption_data();
        ret
    }

    fn parse_encryption_data(&mut self) {
        if !self.err.is_empty() {
            return;
        }
        let lh = self.local_header.as_ref().unwrap();
        if lh.gp_flag & GP_IS_ENCRYPTED == 0 {
            return;
        }
        if lh.gp_flag & GP_IS_STRONG_ENCRYPTED != 0 {
            self.err = "PKWARE strong encryption is not supported".to_string();
        }
        if self.cmethod == 99 {
            // FIXME test method 99 with strong encryption or without encryption at all!
            if let Some(buf) = lh.extras.field_data(0x9901) {
                match crypto::WzEncryption::new(
                    &mut *self
                        .reader
                        .as_ref()
                        .expect("at this stage the reader is always present")
                        .borrow_mut(),
                    buf,
                    self.csize,
                ) {
                    Ok(enc) => {
                        debug!("Wz encryption: {:x?}", enc);
                        self.csize = enc.get_actual_compressed_size();
                        self.cmethod = enc.get_actual_compression_method();
                        self.encryption = crypto::ZipEncryption::Wz(Rc::new(RefCell::new(enc)));
                    }
                    Err(e) => {
                        self.err = format!("Invalid WinZip AES entry extra fields: {}", e);
                    }
                }
            } else {
                self.err = "WinZip AES without extra field".to_string();
            }
        } else if let Some(actual_csize) = self.csize.checked_sub(12) {
            let pwdcheck = if lh.ver_to_extract >= 20 {
                crypto::PkPwdCheck::Byte((lh.pwdcheck >> 8) as u8)
            } else {
                crypto::PkPwdCheck::Word(lh.pwdcheck)
            };
            match crypto::PkEncryption::new(
                &mut *self
                    .reader
                    .as_ref()
                    .expect("at this stage the reader is always present")
                    .borrow_mut(),
                pwdcheck,
            ) {
                Ok(enc) => {
                    self.encryption = crypto::ZipEncryption::Pk(Rc::new(RefCell::new(enc)));
                    self.csize = actual_csize;
                }
                Err(e) => self.err = format!("PKWARE encryption header error: {}", e),
            }
        } else {
            self.err = "Encryption header size overflow".to_string();
        }
    }

    /// Returns any error that has occurred while parsing the entry
    pub fn get_error(&self) -> Option<&str> {
        if self.err.is_empty() {
            None
        } else {
            Some(self.err.as_str())
        }
    }

    /// Returns true if the entry is encrypted.
    pub fn is_encrypted(&self) -> bool {
        !matches!(self.encryption, crypto::ZipEncryption::Null)
    }

    /// Return the encryption type name
    ///
    /// This can be:
    /// - `None` - not encrypted
    /// - `Some("pk")` - Traditional PKWARE encryption
    /// - `Some("aes128")` - WinZip AE (AES 128)
    /// - `Some("aes192")` - WinZip AE (AES 192)
    /// - `Some("aes256")` - WinZip AE (AES 256)
    pub fn encryption_type(&self) -> Option<&'static str> {
        match &self.encryption {
            crypto::ZipEncryption::Null => None,
            crypto::ZipEncryption::Pk(_) => Some("pk"),
            crypto::ZipEncryption::Wz(enc) if enc.borrow().extra_fields.strength == 1 => {
                Some("aes128")
            }
            crypto::ZipEncryption::Wz(enc) if enc.borrow().extra_fields.strength == 2 => {
                Some("aes192")
            }
            crypto::ZipEncryption::Wz(enc) if enc.borrow().extra_fields.strength == 3 => {
                Some("aes256")
            }
            _ => unreachable!(),
        }
    }

    /// Checks the provided password as a decryption key
    pub fn set_password(&mut self, s: &str) -> bool {
        if !self.err.is_empty() {
            return false;
        }
        self.encryption.set_password(s.as_bytes())
    }

    /// Returns an [`EntryReader`] for this entry. An entry reader could be taken just once for
    /// each entry.
    ///
    /// Could return a error if there were errors while processing entry's metadata or if the
    /// reader has been already taken.
    pub fn take_reader(&mut self, input_limit: u64) -> Result<EntryReader, std::io::Error> {
        if !self.err.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                self.err.as_str(),
            ));
        }
        let lh = self.local_header.as_ref().unwrap();
        let uncompressed_size = lh.uncompressed_size;
        let compressed_size = self.csize.min(input_limit);
        let remaining_input_len: Rc<RefCell<u64>> = RefCell::new(compressed_size).into();
        let bounded_reader = CellRead {
            reader: self.reader.take().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "An entry reader for this entry has been already taken",
                )
            })?,
            todo: remaining_input_len.clone(),
        };
        let decrypting_reader: Box<dyn Read> = self.encryption.new_stream(bounded_reader);
        let decompressing_reader: Box<dyn Read> = match self.cmethod {
            0 => decrypting_reader,
            1 => Box::new(unshrink::UnShrinkStream::new(
                decrypting_reader,
                uncompressed_size,
            )?),
            2 => Box::new(expand::ExpandStream::<1, 512, _>::new(
                decrypting_reader,
                uncompressed_size,
            )?),
            3 => Box::new(expand::ExpandStream::<2, 1024, _>::new(
                decrypting_reader,
                uncompressed_size,
            )?),
            4 => Box::new(expand::ExpandStream::<3, 2048, _>::new(
                decrypting_reader,
                uncompressed_size,
            )?),
            5 => Box::new(expand::ExpandStream::<4, 4096, _>::new(
                decrypting_reader,
                uncompressed_size,
            )?),
            6 => Box::new(explode::ExplodeStream::new(
                decrypting_reader,
                uncompressed_size,
                explode::uses_large_window(lh.gp_flag),
                explode::uses_lit_tree(lh.gp_flag),
            )?),
            8 => Box::new(inflate::InflateStream::new(decrypting_reader)?),
            9 => Box::new(inflate64::Inflate64Stream::new(decrypting_reader)),
            12 => Box::new(bzip2::read::BzDecoder::new(decrypting_reader)),
            14 => Box::new(lzma::LzmaStream::new_lzma1(
                decrypting_reader,
                uncompressed_size,
                lzma::uses_term(lh.gp_flag),
            )?),
            20 | 93 => Box::new(zstd::Decoder::new(decrypting_reader)?),
            95 => Box::new(lzma::LzmaStream::new_xz(decrypting_reader)?),
            other => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Unsupported method {other}"),
                ));
            }
        };

        Ok(EntryReader {
            decompressor: decompressing_reader,
            expected_crc32: self.expected_crc32(),
            state: Rc::new(RefCell::new(EntryReaderState {
                crc32_hasher: crc32fast::Hasher::new(),
                encryption: self.encryption.clone(),
            })),
            remaining_input_len,
        })
    }

    // --- Convenience functions ---
    /// The entry central header
    pub fn get_central_header(&self) -> &CentralHeader {
        &self.central_header
    }

    /// The entry local header (if found)
    pub fn get_local_header(&self) -> Option<&LocalHeader> {
        self.local_header.as_ref()
    }

    /// Returns `u16` which represents compression method used for this entry.
    pub fn get_compression_method(&self) -> u16 {
        self.cmethod
    }

    /// Returns entry's compressed size in bytes.
    pub fn get_compressed_size(&self) -> u64 {
        self.csize
    }

    /// Returns entry's original (uncompressed) size in bytes.
    pub fn get_uncompressed_size(&self) -> u64 {
        self.local_header
            .as_ref()
            .map(|lh| lh.uncompressed_size)
            .unwrap_or_else(|| self.central_header.uncompressed_size)
    }

    /// Returns whether the compression method is supported by this crate
    pub fn is_compression_method_supported(&self) -> bool {
        [0, 1, 2, 3, 4, 5, 6, 8, 9, 12, 14, 20, 93, 95].contains(&self.cmethod)
    }

    /// Returns whether the entry's original (uncompressed) size - as returned by
    /// [`get_uncompressed_size()`](Self::get_uncompressed_size) is reliable
    pub fn is_uncompressed_size_reliable(&self) -> bool {
        [
            1, // Shrink
            2, 3, 4, 5,  // Reduce
            6,  // Implode
            14, // Lzma1
        ]
        .contains(&self.cmethod)
    }

    /// An expected CRC-32 checksum specified in file entry header.
    pub fn expected_crc32(&self) -> u32 {
        self.local_header
            .as_ref()
            .map(|lh| lh.crc32)
            .unwrap_or_else(|| self.central_header.crc32)
    }

    /// A lossy UTF-8 representation of the entry file name
    pub fn name(&self) -> std::borrow::Cow<'_, str> {
        self.local_header
            .as_ref()
            .map(|lh| lh.name())
            .unwrap_or_else(|| self.central_header.name())
    }

    /// An entry's apparent file type specified in the archive entry header.
    ///
    /// It is either `Text` or `Binary`.
    pub fn file_type(&self) -> FileType {
        match self.central_header.internal_attributes & 1 {
            0 => FileType::Binary,
            1 => FileType::Text,
            _ => unreachable!(),
        }
    }

    /// An `u8` representing operating system or file system of archive's origin.
    pub fn origin_os_or_fs(&self) -> u8 {
        (self.central_header.ver_made_by >> 8) as u8
    }

    /// An `u16` representing Unix file attributes.
    pub fn unix_file_attributes(&self) -> u16 {
        (self.central_header.external_attributes >> 16) as u16
    }

    /// An `u8` representing MS-DOS file attributes.
    pub fn msdos_file_attributes(&self) -> u8 {
        (self.central_header.external_attributes & 0xff) as u8
    }

    /// An `[u8; 3]` representing non-MSDOS file attributes.
    pub fn non_msdos_file_attributes(&self) -> [u8; 3] {
        let bytes = u32::to_le_bytes(self.central_header.external_attributes);
        [bytes[1], bytes[2], bytes[3]]
    }

    /// Returns entry's last modification time as a Unix timestamp.
    ///
    /// The function first tries to use mtime value stored in a local header. In case if local
    /// header is not available or local header's mtime value could not be parsed - falls back to a
    /// last modification time stored in a central header.
    pub fn timestamp(&self) -> Option<i64> {
        match self.local_header.as_ref() {
            Some(lh) => match lh.mtime {
                Some(mtime) => Some(mtime),
                None => self.central_header.mtime,
            },
            None => self.central_header.mtime,
        }
        .map(|mtime| mtime.assume_utc().unix_timestamp())
    }
}

/// An entity to contain mutable (and shared with the `Entry` in case of `encryption`) state of the
/// `EntryReader`.
struct EntryReaderState {
    /// An entry used to calculate CRC-32 checksum for all the data read via the `EntryReader`.
    crc32_hasher: crc32fast::Hasher,
    /// An interface to access encryption-related data and methods.
    encryption: crypto::ZipEncryption,
}

impl EntryReaderState {
    /// Obtain a CRC-32 checksum for the data that has been read via an `EntryReader` so far.
    fn computed_crc32(&self) -> u32 {
        self.crc32_hasher.finish() as u32
    }
}

/// A [`Read`] interface to the current entry
pub struct EntryReader<'a> {
    /// An implementation of the Read interface which provides decompressed data.
    decompressor: Box<dyn Read + 'a>,
    /// A mutable state of the `EntryReader`, contains a `crypto::ZipEncryption` shared with the
    /// `Entry`.
    state: Rc<RefCell<EntryReaderState>>,
    /// An expected value of CRC-32 checksum obtained from the archive entry headers.
    expected_crc32: u32,
    /// The remaining compressed (input) size
    remaining_input_len: Rc<RefCell<u64>>,
}

impl<'a> EntryReader<'a> {
    /// A computed CRC-32 checksum for a portion of [`Entry`]'s data that has been read via the
    /// `EntryReader`.
    ///
    /// To match to expected CRC-32 checksum the file has to be read in full via the reader.
    pub fn computed_crc32(&self) -> u32 {
        self.state.borrow().crc32_hasher.finish() as u32
    }

    /// Checks whether computed CRC-32 (or the Authentication code for WinZip encryption) matches
    /// to the reference value.
    ///
    /// This method has to be called after entry's content has been read in full via the entry
    /// reader to match to expected values.
    pub fn integrity_check_ok(&self) -> bool {
        let state = self.state.borrow();

        if let crypto::ZipEncryption::Wz(ref wz) = state.encryption {
            return wz.borrow_mut().check_authentication_code();
        }
        state.computed_crc32() == self.expected_crc32
    }

    /// Checks whether the compressed stream was fully consumed
    pub fn was_input_fully_consumed(&self) -> bool {
        *self.remaining_input_len.borrow() == 0
    }
}

impl Read for EntryReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let mut state = self.state.borrow_mut();
        let len = self.decompressor.read(buf)?;
        state.crc32_hasher.update(&buf[0..len]);
        Ok(len)
    }
}

/// A wrapper to hold a shared Read implementation.
///
/// It additionally limits the amount read() similarly to [`Take`]
struct CellRead<T> {
    reader: Rc<RefCell<T>>,
    todo: Rc<RefCell<u64>>,
}

impl<T: Read> Read for CellRead<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut todo = self.todo.borrow_mut();
        let avail = (*todo).min(buf.len().try_into().unwrap_or(*todo));
        if avail == 0 {
            return Ok(0);
        }
        let ret = self.reader.borrow_mut().read(&mut buf[0..(avail as usize)]); // safe bc min() above
        if let Ok(len) = ret {
            *todo = (*todo).saturating_sub(len as u64); // safe bc min() above
        }
        ret
    }
}

/// A (by name) random access interface to [`Zip`] entries
///
/// Caveats:
/// - This interface provides convenience at the expense of extra IO and memory usage
/// - Duplicate entries will not be detected (last one prevails)
pub struct ZipRandomAccess<R: Read + Seek> {
    pub zip: Zip<R>,
    toc: HashMap<String, CentralHeader>,
}

impl<R: Read + Seek> ZipRandomAccess<R> {
    /// Creates a new random access Zip processor
    pub fn new(r: R) -> Result<Self, std::io::Error> {
        let zip = Zip::new(r)?;
        let total_entries = zip.get_entries_total();
        zip.r
            .borrow_mut()
            .seek(std::io::SeekFrom::Start(zip.get_cd_offset_on_first_disk()))?;
        let mut toc: HashMap<String, CentralHeader> = HashMap::new();
        for _ in 0..total_entries {
            let central_header = CentralHeader::new(&zip.r)?;
            toc.insert(central_header.name().to_string(), central_header);
        }
        Ok(Self { zip, toc })
    }

    /// Returns an iterator over the zip entry names
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.toc.keys().map(|s| s.as_str())
    }

    /// Returns the zip [`Entry`] with the provided name
    pub fn get_entry_by_name(&self, name: &str) -> Option<Entry<R>> {
        self.toc
            .get(name)
            .map(|ch| Entry::new(&self.zip, ch.clone()))
    }
}
