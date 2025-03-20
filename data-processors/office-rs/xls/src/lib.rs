//! # Excel Binary File Format parser (.xls)
//!
//! This module provides a parser for the *Excel Binary File Format* (.xls)
//!
//! Excel Binary files are *Compound File Binary Format* structures: check the [ole] crate for details
//!
//! The main interface documentation and code examples are under the [`Xls`] struct
//!
//! The implementation was written from scratch based entirely on
//! [\[MS-XLS\]](https://docs.microsoft.com/en-us/openspecs/office_file_formats/ms-xls/cd03cb5f-ca02-4934-a391-bb674cb8aa06)
//!

#![warn(missing_docs)]

mod error;
pub mod macrosheet;
pub mod records;
pub mod structures;
pub mod workbook;
pub mod worksheet;
pub use error::ExcelError;

use crate::structures::BofType;
use ctxole::{Encryption, NoValidPasswordError, Ole, OleEntry, crypto};
use ctxutils::io::*;
use records::{
    from_record::{Anomalies, FromRecordStream, Record},
    *,
};
use std::{
    fmt::{self, Debug},
    io::{self, Read, Seek},
};
use tracing::debug;
use vba::{Vba, VbaDocument};
use workbook::Workbook;

/// Default encryption/obfuscation password
pub const XLS_DEFAULT_PASSWORD: &str = "VelvetSweatshop";

/// Interface to Xls structures
pub struct Xls<'a, R: Read + Seek> {
    substreams: Vec<Substream<'a, R>>,
    /// The *Workbook*
    pub workbook: Workbook<'a, R>,
    encryption: Option<Encryption>,
}

fn decryption_init<R: Read + Seek>(
    r: &mut R,
    size: u64,
    passwords: &[&str],
) -> Result<Option<(crypto::LegacyKey, Encryption)>, ExcelError> {
    // Note: cannot use RecordStream as it's not Seek
    let mut avail = size;
    while avail >= 4 {
        let ty = rdu16le(r)?;
        let sz = rdu16le(r)?;
        avail = avail.saturating_sub(u64::from(sz)).saturating_sub(4);
        let rt = RecordType::new(ty);
        match rt {
            RecordType::FilePass => {
                let enc_type = rdu16le(r)?;
                match enc_type {
                    0 => {
                        // Xor obfuscation
                        let _key = rdu16le(r)?;
                        let verifier = rdu16le(r)?;
                        for pass in passwords {
                            let encryption_key = crypto::XorKey::method1(pass, verifier);
                            if let Some(encryption_key) = encryption_key {
                                debug!("Found correct XOR obfuscation password {pass}");
                                let encryption = Encryption {
                                    algorithm: "XOR obfuscation".to_string(),
                                    password: pass.to_owned().to_owned(),
                                };
                                return Ok(Some((encryption_key, encryption)));
                            }
                        }
                        return Err(NoValidPasswordError::new_io_error("XOR obfuscation").into());
                    }
                    1 => {
                        // Legacy encryption
                        let crypto_version = crypto::Version::new(r)?;
                        if crypto_version.minor == 1 && crypto_version.major == 1 {
                            // Office binary document RC4 encryption
                            let encr = crypto::BinaryRc4Encryption::new(r)?;
                            for pass in passwords {
                                let encryption_key = encr.get_key(pass, 1024);
                                if let Some(encryption_key) = encryption_key {
                                    debug!(
                                        "Found correct Office Binary Document Rc4 password {pass}"
                                    );
                                    let encryption = Encryption {
                                        algorithm: "RC4".to_string(),
                                        password: pass.to_owned().to_owned(),
                                    };
                                    return Ok(Some((encryption_key, encryption)));
                                }
                            }
                            return Err(NoValidPasswordError::new_io_error(
                                "Office binary document RC4 encryption",
                            )
                            .into());
                        } else if crypto_version.minor == 2
                            && [2, 3, 4].contains(&crypto_version.major)
                        {
                            // RC4 CryptoAPI Encryption
                            // FIXME: this has got a separate stream !!!!
                            let encr = crypto::Rc4CryptoApiEncryption::new(r)?;
                            for pass in passwords {
                                let encryption_key = encr.get_key(pass, 1024);
                                if let Some(encryption_key) = encryption_key {
                                    debug!(
                                        "Found correct Office Binary Document Rc4 CryptoApi password {pass}"
                                    );
                                    let encryption = Encryption {
                                        algorithm: "RC4 CryptoApi".to_string(),
                                        password: pass.to_owned().to_owned(),
                                    };
                                    return Ok(Some((encryption_key, encryption)));
                                }
                            }
                            return Err(NoValidPasswordError::new_io_error(
                                "Office binary document RC4 CryptoApi encryption",
                            )
                            .into());
                        } else {
                            return Err(format!(
                                "Unsupported/Invalid EncryptionInfo version ({crypto_version})"
                            )
                            .into());
                        }
                    }
                    v => return Err(format!("Invalid FilePass.wEncryptionType {}", v).into()),
                }
            }
            RecordType::EOF => break,
            _ => {
                r.seek(io::SeekFrom::Current(i64::from(sz)))?;
            }
        }
    }
    Ok(None)
}

impl<'a, R: Read + Seek> Xls<'a, R> {
    /// Process the Xls data and provide an interface to the *Workbook*
    pub fn new(ole: &'a Ole<R>, passwords: &[&str]) -> Result<Self, ExcelError> {
        debug!("Xls::new()");
        let entry = ole.get_entry_by_name("Workbook")?;
        let mut ole_stream = ole.get_stream_reader(&entry);
        let (encryption_key, encryption) =
            if let Some(pair) = decryption_init(&mut ole_stream, entry.size, passwords)? {
                (Some(pair.0), Some(pair.1))
            } else {
                (None, None)
            };
        debug!("Xls encryption key: {:?}", encryption_key);
        ole_stream.rewind()?;
        let mut substreams = Vec::<Substream<R>>::new();
        let mut wb = if let Some(ref key) = encryption_key {
            crypto::LegacyDecryptor::new(ole_stream, key, 0)
        } else {
            crypto::LegacyDecryptor::new_no_op(ole_stream)
        };
        let mut record_stream = RecordStream::new(&mut wb)?;
        let mut anomalies = Anomalies::new();
        loop {
            if record_stream.ty == RecordType::BOF {
                let offset = record_stream.offset;
                let bof = Bof::from_record(&mut record_stream, &mut anomalies)?;
                let substream_type = match bof.dt {
                    BofType::Workbook => SubstreamType::Workbook,
                    BofType::ChartSheet => SubstreamType::Chartsheet,
                    BofType::MacroSheet => SubstreamType::Macrosheet,
                    BofType::Invalid(v) => SubstreamType::Invalid(v),
                    BofType::DialogOrWorksheet => {
                        loop {
                            if record_stream.ty == RecordType::WsBool || !record_stream.next()? {
                                break;
                            }
                        }
                        let wsbool = WsBool::from_record(&mut record_stream, &mut anomalies)?;
                        record_stream.seek(offset)?;
                        if wsbool.f_dialog {
                            SubstreamType::Dialogsheet
                        } else {
                            SubstreamType::Worksheet
                        }
                    }
                };
                substreams.push(Substream::new(
                    ole,
                    &entry,
                    &encryption_key,
                    offset,
                    substream_type,
                )?);
            }
            if !record_stream.can_read_next() || !record_stream.next()? {
                break;
            }
        }
        //debug_record_stream(&mut record_stream)?;
        let workbook = Workbook::new(ole, entry, encryption_key)?;
        Ok(Self {
            substreams,
            workbook,
            encryption,
        })
    }

    /// Returns encryption algorithm and password (if any)
    pub fn encryption(&self) -> Option<&Encryption> {
        self.encryption.as_ref()
    }

    /// Return list of detected substreams.
    pub fn substreams(&mut self) -> Vec<&'a mut Substream<R>> {
        let mut result = Vec::<&mut Substream<R>>::new();
        for substream in &mut self.substreams {
            result.push(substream);
        }
        result
    }
}

impl<'a, R: Read + Seek> VbaDocument<'a, R> for &'a Xls<'_, R> {
    fn vba(self) -> Option<Result<Vba<'a, R>, io::Error>> {
        match Vba::new(self.workbook.ole, "_VBA_PROJECT_CUR") {
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            res => Some(res),
        }
    }
}

#[allow(dead_code)]
fn debug_record_stream<R: Read + Seek>(stream: &mut RecordStream<R>) -> Result<(), ExcelError> {
    const IGNORED_RECORDS: &[RecordType] = &[
        RecordType::Style,
        RecordType::Format,
        RecordType::Font,
        RecordType::XF,
        RecordType::XFExt,
        RecordType::Header,
        RecordType::Footer,
        RecordType::HCenter,
        RecordType::VCenter,
    ];
    let offset = stream.offset;
    let mut depth = 0;
    println!("RECORD LIST:");
    loop {
        if IGNORED_RECORDS.contains(&stream.ty) {
            if !stream.next()? {
                break;
            }
            continue;
        }
        let offset = stream.offset;
        if stream.ty == RecordType::EOF {
            depth -= 1;
        }
        println!("{depth}:{:?} ({offset})", stream.ty);
        let mut bytes = vec![0u8; stream.sz];
        match stream.ty {
            RecordType::BOF
            | RecordType::FilePass
            | RecordType::UsrExcl
            | RecordType::FileLock
            | RecordType::InterfaceHdr
            | RecordType::RRDInfo
            | RecordType::RRDHead => {
                // Not encrypted
                stream.get_ndr().read_exact(&mut bytes)?;
            }
            RecordType::BoundSheet8 => {
                // Not encrypted (lb_ply_pos)
                stream.get_ndr().read_exact(&mut bytes[0..4])?;
                // Encrypted (everything else)
                stream.read_exact(&mut bytes[4..])?;
            }
            _ => {
                stream.read_exact(&mut bytes)?;
            }
        }
        hexdump::hexdump(&bytes);
        stream.seek(stream.offset)?;
        if stream.ty == RecordType::BOF {
            depth += 1;
        }
        if !stream.can_read_next() || !stream.next_raw()? {
            break;
        }
    }
    stream.seek(offset)?;
    println!("RECORD DETAILS:");
    loop {
        let mut anomalies = Anomalies::new();
        let offset = stream.offset;
        match stream.ty {
            RecordType::BOF => {
                println!("{depth}:{:?} ({offset})", stream.ty);
                depth += 1;
                let r = Bof::from_record(stream, &mut anomalies);
                println!("{r:#?}");
            }
            RecordType::EOF => {
                depth -= 1;
                println!("{depth}:{:?} ({offset})", stream.ty);
            }
            RecordType::WsBool => {
                println!("{depth}:{:?} ({offset})", stream.ty);
                let r = WsBool::from_record(stream, &mut anomalies);
                println!("{r:#?}");
            }
            RecordType::RK => {
                println!("{depth}:{:?} ({offset})", stream.ty);
                let r = RK::from_record(stream, &mut anomalies);
                println!("{r:#?}");
            }
            RecordType::MulRk => {
                println!("{depth}:{:?} ({offset})", stream.ty);
                let r = MulRk::from_record(stream, &mut anomalies);
                println!("{r:#?}");
            }
            RecordType::BoolErr => {
                println!("{depth}:{:?} ({offset})", stream.ty);
                let r = BoolErr::from_record(stream, &mut anomalies);
                println!("{r:#?}");
            }
            RecordType::Number => {
                println!("{depth}:{:?} ({offset})", stream.ty);
                let r = Number::from_record(stream, &mut anomalies);
                println!("{r:#?}");
            }
            RecordType::LabelSst => {
                println!("{depth}:{:?} ({offset})", stream.ty);
                let r = LabelSst::from_record(stream, &mut anomalies);
                println!("{r:#?}");
            }
            RecordType::Formula => {
                println!("{depth}:{:?} ({offset})", stream.ty);
                let r = Formula::from_record(stream, &mut anomalies);
                println!("{r:#?}");
            }
            RecordType::SST => {
                println!("{depth}:{:?} ({offset})", stream.ty);
                let r = SST::from_record(stream, &mut anomalies);
                println!("{r:#?}");
            }
            RecordType::BoundSheet8 => {
                println!("{depth}:{:#?} ({offset})", stream.ty);
                let r = BoundSheet8::from_record(stream, &mut anomalies);
                println!("{r:#?}");
            }
            RecordType::Obj => {
                println!("{depth}:{:?} ({offset})", stream.ty);
                let r = Obj::from_record(stream, &mut anomalies);
                println!("{r:#?}");
            }
            _ => {}
        }

        if !stream.can_read_next() {
            break;
        }

        if stream.offset == offset && !stream.next()? {
            break;
        }
    }
    stream.seek(offset)?;
    Ok(())
}

impl<R: Read + Seek> fmt::Debug for Xls<'_, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Xls")
            .field("workbook", &self.workbook)
            .finish()
    }
}

struct RecordStream<'a, R: Read + Seek> {
    f: &'a mut crypto::LegacyDecryptor<R>,
    offset: u64,
    ole_stream_size: u64,
    ty: RecordType,
    sz: usize,
    conty: Option<RecordType>,
    pos: usize,
}

impl<R: Read + Seek> fmt::Debug for RecordStream<'_, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Record")
            .field("ty", &self.ty)
            .field("sz", &self.sz)
            .field("conty", &self.conty)
            .field("pos", &self.pos)
            .field("offset", &self.offset)
            .finish()
    }
}

fn continuation_for(ty: &RecordType) -> Option<RecordType> {
    // FIXME: add all continuable types and their continuation type
    let map = [(
        RecordType::Continue,
        [
            RecordType::BkHim,
            RecordType::ExternSheet,
            RecordType::MsoDrawing,
            RecordType::Obj,
            RecordType::PhoneticInfo,
            RecordType::Pls,
            RecordType::RRSort,
            RecordType::SCENARIO,
            RecordType::SST,
            RecordType::String,
            RecordType::SupBook,
            RecordType::SxItm,
            RecordType::SxIvd,
            RecordType::SXLI,
            RecordType::SXPI,
            RecordType::SXTH,
            RecordType::SXVDTEx,
            RecordType::TxO,
        ],
    )];

    for entry in &map {
        let types = &entry.1;
        let continuation = &entry.0;
        if types.contains(ty) {
            return Some(*continuation);
        }
    }
    None
}

impl<'a, R: Read + Seek> RecordStream<'a, R> {
    fn new(f: &'a mut crypto::LegacyDecryptor<R>) -> Result<Self, io::Error> {
        let offset = f.stream_position()?;
        let ole_stream_size = f.seek(io::SeekFrom::End(0))?;
        f.seek(io::SeekFrom::Start(offset))?;

        let ty = RecordType::new(rdu16le(f.as_inner())?);
        let sz = rdu16le(f.as_inner())?;
        // FIXME: add all continuable types and their continuation type
        let conty = continuation_for(&ty);
        Ok(Self {
            f,
            ole_stream_size,
            offset,
            ty,
            sz: sz.into(),
            conty,
            pos: 0,
        })
    }

    fn get_ndr<'b>(&'b mut self) -> NonDecryptingRecordReader<'b, 'a, R> {
        NonDecryptingRecordReader(self)
    }

    fn sink(&mut self, len: u16) -> Result<(), ExcelError> {
        debug!(
            "sink() offset={} sz={}, pos={} len={}",
            self.offset, self.sz, self.pos, len
        );
        let mut todo: usize = len.into();
        while todo > 0 {
            let avail = self.sz - self.pos;
            let done = avail.min(todo);
            self.pos += done;
            todo -= done;
            self.f
                .seek(io::SeekFrom::Current(done.try_into().unwrap()))?; // Safe
            if todo == 0 || self.conty.is_none() {
                break;
            }

            let ty = rdu16le(self.f.as_inner())?;
            if ty == 0 {
                break;
            }
            let ty = RecordType::new(ty);
            if Some(ty) != self.conty {
                self.f.seek(io::SeekFrom::Current(-2))?;
                break;
            }
            let size = rdu16le(self.f.as_inner())?;
            self.sz += usize::from(size);

            //debug!("Record: {ty:?}, size: {size}");
        }
        if todo > 0 {
            Err(format!("Record overflow occurred when sinking {} bytes", len).into())
        } else {
            Ok(())
        }
    }

    fn next(&mut self) -> Result<bool, io::Error> {
        let to_skip = i64::try_from(self.sz - self.pos).unwrap(); // Safe: max diff is u16
        self.f.seek(io::SeekFrom::Current(to_skip))?;
        if let Some(conty) = &self.conty {
            loop {
                let ty = rdu16le(self.f.as_inner())?;
                if ty == 0 {
                    let offset = self.f.seek(io::SeekFrom::Current(-2))?;
                    self.ole_stream_size = offset;
                    return Ok(false);
                }
                let ty = RecordType::new(ty);
                if ty != *conty {
                    self.f.seek(io::SeekFrom::Current(-2))?;
                    break;
                }
                let sz = rdu16le(self.f.as_inner())?;
                self.f.seek(io::SeekFrom::Current(sz.into()))?;
            }
        }

        let offset = self.f.stream_position()?;
        let ty = rdu16le(self.f.as_inner())?;
        if ty == 0 {
            self.ole_stream_size = offset;
            self.f.seek(io::SeekFrom::Current(-2))?;
            return Ok(false);
        }
        let ty = RecordType::new(ty);
        let sz = rdu16le(self.f.as_inner())?;

        let conty = continuation_for(&ty);
        self.ty = ty;
        self.sz = sz.into();
        self.conty = conty;
        self.pos = 0;
        self.offset = offset;
        Ok(true)
    }

    fn next_raw(&mut self) -> Result<bool, io::Error> {
        let to_skip = i64::try_from(self.sz - self.pos).unwrap(); // Safe: max diff is u16
        let offset = self.f.seek(io::SeekFrom::Current(to_skip))?;

        let ty = rdu16le(self.f.as_inner())?;
        if ty == 0 {
            self.ole_stream_size = offset;
            self.f.seek(io::SeekFrom::Current(-2))?;
            return Ok(false);
        }
        let ty = RecordType::new(ty);
        let sz = rdu16le(self.f.as_inner())?;

        let conty = continuation_for(&ty);
        self.ty = ty;
        self.sz = sz.into();
        self.conty = conty;
        self.pos = 0;
        self.offset = offset;
        Ok(true)
    }

    fn can_read_next(&mut self) -> bool {
        self.offset + (self.sz as u64/* safe: sz is a u16 */) + 8 < self.ole_stream_size
    }

    fn seek(&mut self, offset: u64) -> Result<bool, io::Error> {
        self.f.seek(io::SeekFrom::Start(offset))?;
        let ty = rdu16le(self.f.as_inner())?;
        if ty == 0 {
            self.f.seek(io::SeekFrom::Current(-2))?;
            return Ok(false);
        }
        let ty = RecordType::new(ty);
        let sz = rdu16le(self.f.as_inner())?;
        let conty = continuation_for(&ty);
        self.ty = ty;
        self.sz = sz.into();
        self.conty = conty;
        self.pos = 0;
        self.offset = offset;
        Ok(true)
    }

    fn skip_subtream(&mut self) -> Result<(), io::Error> {
        let mut depth = 1;
        while depth > 0 {
            if !self.next()? {
                break;
            }
            match self.ty {
                RecordType::BOF => depth += 1,
                RecordType::EOF => depth -= 1,
                _ => {}
            };
        }
        Ok(())
    }

    fn rdu8(&mut self) -> Result<u8, io::Error> {
        rdu8(self)
    }

    fn rdu16(&mut self) -> Result<u16, io::Error> {
        rdu16le(self)
    }

    fn rdi16(&mut self) -> Result<i16, io::Error> {
        rdi16le(self)
    }

    fn rdu32(&mut self) -> Result<u32, io::Error> {
        rdu32le(self)
    }

    fn rdi32(&mut self) -> Result<i32, io::Error> {
        rdi32le(self)
    }

    fn available(&self) -> usize {
        self.sz - self.pos
    }

    fn read_byte(&mut self) -> Result<u8, io::Error> {
        while self.conty.is_some() && self.pos >= self.sz {
            let ty = RecordType::new(rdu16le(self.f.as_inner())?);
            if self.conty != Some(ty) {
                self.f.seek(io::SeekFrom::Current(-2))?;
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Unexpected end of record",
                ));
            }
            self.sz += usize::from(rdu16le(self.f.as_inner())?);
        }
        if self.pos >= self.sz {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Unexpected end of record",
            ));
        }
        let mut buf = [0u8];
        let rdsz = self.f.read(&mut buf)?;
        if rdsz == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Unexpected end of record",
            ));
        }
        self.pos += rdsz;
        Ok(buf[0])
    }
}

impl<'a, R: Read + Seek> Read for RecordStream<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        debug!(
            "read() offset={} ty={:?} sz={}, pos={} len={}",
            self.offset,
            self.ty,
            self.sz,
            self.pos,
            buf.len(),
        );
        while self.conty.is_some() && self.pos >= self.sz {
            let ty = RecordType::new(rdu16le(self.f.as_inner())?);
            if self.conty != Some(ty) {
                self.f.seek(io::SeekFrom::Current(-2))?;
                return Ok(0);
            }
            self.sz += usize::from(rdu16le(self.f.as_inner())?);
        }

        if self.pos >= self.sz {
            return Ok(0);
        }
        if let Some(crypto::LegacyKey::XorObfuscation(ref mut k)) = self.f.key {
            k.set_method1_cypher_size(self.sz);
        }
        let buflen = buf.len();
        let slice = &mut buf[0..buflen.min(self.sz - self.pos)];
        let rdsz = self.f.read(slice)?;
        self.pos += rdsz;
        //debug!("SLICE: {:02x?}", slice);
        Ok(rdsz)
    }
}

struct NonDecryptingRecordReader<'b, 'a, R: Read + Seek>(&'b mut RecordStream<'a, R>);

impl<'b, 'a, R: Read + Seek> Read for NonDecryptingRecordReader<'b, 'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        debug!(
            "non_decrypting_read() offset={} ty={:?} sz={}, pos={} len={}",
            self.0.offset,
            self.0.ty,
            self.0.sz,
            self.0.pos,
            buf.len(),
        );
        while self.0.conty.is_some() && self.0.pos >= self.0.sz {
            let ty = RecordType::new(rdu16le(self.0.f.as_inner())?);
            if self.0.conty != Some(ty) {
                self.0.f.seek(io::SeekFrom::Current(-2))?;
                return Ok(0);
            }
            self.0.sz += usize::from(rdu16le(self.0.f.as_inner())?);
        }

        if self.0.pos >= self.0.sz {
            return Ok(0);
        }
        let buflen = buf.len();
        let slice = &mut buf[0..buflen.min(self.0.sz - self.0.pos)];
        let rdsz = self.0.f.as_inner().read(slice)?;
        self.0.pos += rdsz;
        //debug!("NDSLICE: {:02x?}", slice);
        Ok(rdsz)
    }
}

/// Location of embedded objects
#[derive(Debug)]
pub enum DataLocation {
    /// Zero-based offset of this object’s data within the control stream (Ctls)
    ControlStream {
        /// Stream offset
        offset: u32,
        /// Data length
        size: u32,
    },
    /// Name of MDB storage in OLE file
    EmbeddingStorage {
        /// Storage name
        storage: String,
    },
}

/// Type of substream in Workbook stream
#[derive(Debug, Clone)]
pub enum SubstreamType {
    /// Workbook substream
    Workbook,
    /// Worksheet substream
    Worksheet,
    /// Chartsheet substream
    Chartsheet,
    /// Dialogsheet substream
    Dialogsheet,
    /// Macrosheet substream
    Macrosheet,
    /// Invalid/Unsupported substream type
    Invalid(u16),
}

impl SubstreamType {
    /// Returns substream type name
    pub fn name(&self) -> &str {
        match &self {
            SubstreamType::Workbook => "workbook",
            SubstreamType::Worksheet => "worksheet",
            SubstreamType::Chartsheet => "charsheet",
            SubstreamType::Dialogsheet => "dialogsheet",
            SubstreamType::Macrosheet => "macrosheet",
            SubstreamType::Invalid(_) => "invalid",
        }
    }
}

/// Interface to handle raw substream
pub struct Substream<'a, R: Read + Seek> {
    stream: crypto::LegacyDecryptor<ctxole::OleStreamReader<'a, R>>,
    bof_offset: u64,
    /// Type of substrem
    pub substream_type: SubstreamType,
}

impl<'a, R: Read + Seek> Substream<'a, R> {
    fn new(
        ole: &'a Ole<R>,
        entry: &OleEntry,
        encryption_key: &Option<crypto::LegacyKey>,
        bof_offset: u64,
        substream_type: SubstreamType,
    ) -> Result<Self, ExcelError> {
        let ole_stream = ole.get_stream_reader(entry);
        let stream = if let Some(key) = &encryption_key {
            crypto::LegacyDecryptor::new(ole_stream, key, 0)
        } else {
            crypto::LegacyDecryptor::new_no_op(ole_stream)
        };
        Ok(Self {
            stream,
            bof_offset,
            substream_type,
        })
    }

    /// Get iterator over substream records
    pub fn get_iterator<'b>(
        &'b mut self,
    ) -> Result<RecordsIterator<'b, ctxole::OleStreamReader<'a, R>>, ExcelError> {
        // you can wrap this into an iterator
        let mut rs = RecordStream::new(&mut self.stream)?;
        rs.seek(self.bof_offset)?;
        Ok(RecordsIterator { rs, first: true })
    }
}

impl<R: Read + Seek> Debug for Substream<'_, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Substream")
            .field("substream_type", &self.substream_type)
            .field("bof_offset", &self.bof_offset)
            .finish()
    }
}

/// Iterator over records in substream
pub struct RecordsIterator<'a, R: Read + Seek> {
    rs: RecordStream<'a, R>,
    first: bool,
}

impl<'a, R: Read + Seek> Iterator for RecordsIterator<'a, R> {
    type Item = Result<Box<dyn Record>, ExcelError>;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        if !self.rs.can_read_next() {
            return None;
        }
        if self.rs.ty == RecordType::EOF {
            return None;
        }
        if !self.first {
            match self.rs.next() {
                Ok(true) => {}
                Ok(false) => return None,
                Err(e) => return Some(Err(e.into())),
            }
        }
        self.first = false;
        Some(load_dyn_record(&mut self.rs))
    }
}
