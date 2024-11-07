use super::{
    record_type::RecordType,
    structs::{FromReader, ObjectParsedFormula},
};
use crate::{
    xlsb::structs::{XLNullableWideString, XLWideString},
    xlsx::SheetInfo,
    OoxmlError, Relationship, RelationshipType, SheetType,
};
use ctxutils::io::{rdu16le, rdu32le, rdu8};
use std::io::{self, Read, Seek};
use tracing::warn;

#[derive(Debug)]
pub struct RecordHeader {
    pub record_type: RecordType,
    pub data_size: usize,
    pub offset: u64,
    pub data_offset: u64,
}

pub trait Record {
    fn record_type() -> RecordType;
}

pub struct RecordReader<R: Read + Seek> {
    reader: R,
    position: u64,
    size: u64,
}

impl<R: Read + Seek> RecordReader<R> {
    pub fn new(mut reader: R) -> Result<Self, OoxmlError> {
        let position = reader.stream_position()?;
        let size = reader.seek(std::io::SeekFrom::End(0))?;
        reader.seek(std::io::SeekFrom::Start(position))?;
        Ok(Self {
            position,
            size,
            reader,
        })
    }
    pub fn next_record_header(&mut self) -> Result<Option<RecordHeader>, OoxmlError> {
        if self.position >= self.size {
            return Ok(None);
        }
        if self.position != self.reader.stream_position()? {
            self.reader.seek(std::io::SeekFrom::Start(self.position))?;
        }
        let offset = self.position;
        let mut record_type = u16::from(rdu8(&mut self.reader)?);
        if record_type & 0x80 != 0 {
            record_type &= 0x7F;
            let byte = u16::from(rdu8(&mut self.reader)?);
            record_type |= byte << 7;
        }
        let record_type = RecordType::from_u16(record_type);
        let mut data_size: u32 = 0;
        for i in 0..4 {
            let byte = u32::from(rdu8(&mut self.reader)?);
            data_size |= (byte & 0x7F) << (i * 7);
            if byte & 0x80 == 0 {
                break;
            }
        }
        let data_offset = self.reader.stream_position()?;
        self.position = self
            .reader
            .seek(std::io::SeekFrom::Current(i64::from(data_size)))?;
        let data_size = usize::try_from(data_size)?;
        Ok(Some(RecordHeader {
            data_offset,
            offset,
            data_size,
            record_type,
        }))
    }
    pub fn next_record<Rec: Record + FromReader>(&mut self) -> Result<Option<Rec>, OoxmlError> {
        let header = match self.next_record_header()? {
            Some(header) => header,
            None => return Ok(None),
        };
        let record = self.read_record::<Rec>(&header)?;
        Ok(Some(record))
    }
    pub fn read_record_data(
        &mut self,
        record: &RecordHeader,
    ) -> Result<OffsetReader<R>, OoxmlError> {
        Ok(OffsetReader {
            reader: &mut self.reader,
            offset: record.data_offset,
            size: record.data_size,
        })
    }
    pub fn read_raw_data(
        &mut self,
        data_offset: u64,
        data_size: usize,
    ) -> Result<OffsetReader<R>, OoxmlError> {
        Ok(OffsetReader {
            reader: &mut self.reader,
            offset: data_offset,
            size: data_size,
        })
    }
    pub fn read_record<Rec: Record + FromReader>(
        &mut self,
        record: &RecordHeader,
    ) -> Result<Rec, OoxmlError> {
        if Rec::record_type() != record.record_type {
            return Err(format!(
                "Invalid record type, expecting {:?}, found {:?}",
                Rec::record_type(),
                record.record_type
            )
            .into());
        }
        let mut reader = self.read_record_data(record)?;
        Rec::from_reader(&mut reader)
    }
    pub fn seek(&mut self, offset: u64) -> Result<(), OoxmlError> {
        self.position = self.reader.seek(std::io::SeekFrom::Start(offset))?;
        Ok(())
    }
    // pub fn print_structure(&mut self) -> Result<(), OoxmlError> {
    //     let position = self.position;
    //     self.seek(0)?;
    //     let mut depth: usize = 0;
    //     while let Some(record_header) = self.next_record_header()? {
    //         let rt = format!("{:?}", record_header.record_type);
    //         if rt.contains("End") {
    //             depth = depth.saturating_sub(1);
    //         }
    //         println!(
    //             "{:spaces$}{rt} (offset={}, data_size={})",
    //             "",
    //             record_header.offset,
    //             record_header.data_size,
    //             spaces = depth * 2
    //         );
    //         if rt.contains("Begin") {
    //             depth += 1;
    //         }
    //     }
    //     self.seek(position)?;
    //     Ok(())
    // }
}

#[derive(Debug)]
pub struct BundleSh {
    pub state: u32,
    pub id: u32,
    pub relid: Option<String>,
    pub name: String,
}

impl BundleSh {
    pub fn convert_to_sheet_info(self, relationships: &[Relationship]) -> Option<SheetInfo> {
        let relid = self.relid?;
        let relationship = relationships.iter().find(|r| r.id == relid)?;
        let path = match &relationship.target {
            crate::relationship::TargetMode::Internal(path) => path.to_string(),
            crate::relationship::TargetMode::External(_) => {
                warn!("Invalid relationship target: {:?}", relationship.target);
                return None;
            }
        };
        let sheet_type = match &relationship.rel_type {
            RelationshipType::Worksheet => SheetType::Worksheet,
            RelationshipType::Macrosheet => SheetType::Macrosheet,
            RelationshipType::Dialogsheet => SheetType::Dialogsheet,
            RelationshipType::Chartsheet => SheetType::Chartsheet,
            rel_type => {
                warn!("Invalid relationship type: {:?}", rel_type);
                return None;
            }
        };

        let id = self.id.to_string();
        let name = self.name;
        let state = match self.state {
            0x00000000 => "visible".to_string(),
            0x00000001 => "hidden".to_string(),
            0x00000002 => "veryHidden".to_string(),
            invalid => format!("Invalid({invalid})"),
        };
        Some(SheetInfo {
            id,
            name,
            path,
            state,
            sheet_type,
        })
    }
}

impl Record for BundleSh {
    fn record_type() -> RecordType {
        RecordType::BundleSh
    }
}

impl FromReader for BundleSh {
    fn from_reader<R: Read>(reader: &mut R) -> Result<Self, OoxmlError>
    where
        Self: std::marker::Sized,
    {
        let state = rdu32le(reader)?;
        let id = rdu32le(reader)?;
        let relid = XLNullableWideString::from_reader(reader)?.data;
        let name = XLWideString::from_reader(reader)?.data;
        Ok(Self {
            state,
            id,
            relid,
            name,
        })
    }
}

pub struct BeginSst {
    pub _total: u32,
    pub _unique: u32,
}

impl Record for BeginSst {
    fn record_type() -> RecordType {
        RecordType::BeginSst
    }
}

impl FromReader for BeginSst {
    fn from_reader<R: Read>(reader: &mut R) -> Result<Self, OoxmlError>
    where
        Self: std::marker::Sized,
    {
        let total = rdu32le(reader)?;
        let unique = rdu32le(reader)?;
        Ok(Self {
            _total: total,
            _unique: unique,
        })
    }
}

pub struct SSTItem {
    pub value: String,
}

impl Record for SSTItem {
    fn record_type() -> RecordType {
        RecordType::SSTItem
    }
}

impl FromReader for SSTItem {
    fn from_reader<R: Read>(reader: &mut R) -> Result<Self, OoxmlError>
    where
        Self: std::marker::Sized,
    {
        _ = rdu8(reader)?;
        let str = XLWideString::from_reader(reader)?;
        let value = str.data;
        Ok(Self { value })
    }
}

pub struct OleObject {
    pub str_rel_id: XLNullableWideString,
}

impl Record for OleObject {
    fn record_type() -> RecordType {
        RecordType::OleObject
    }
}

impl FromReader for OleObject {
    fn from_reader<R: Read>(reader: &mut R) -> Result<Self, OoxmlError>
    where
        Self: std::marker::Sized,
    {
        let _dw_aspect = rdu32le(reader)?;
        let _dw_ole_update = rdu32le(reader)?;
        let _shape_id = rdu32le(reader)?;
        let misc = rdu16le(reader)?;
        let f_linked = misc & 1 != 0;
        //let f_auto_load = misc & 2 != 0;
        let _str_prog_id = XLWideString::from_reader(reader)?;
        let _link = if f_linked {
            Some(ObjectParsedFormula::from_reader(reader)?)
        } else {
            None
        };
        let str_rel_id = XLNullableWideString::from_reader(reader)?;
        Ok(OleObject { str_rel_id })
    }
}
pub struct OffsetReader<'a, R: Read + Seek> {
    reader: &'a mut R,
    offset: u64,
    size: usize,
}

impl<'a, R: Read + Seek> Read for OffsetReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let to_read = self.size.min(buf.len());
        if to_read == 0 {
            return Ok(0);
        }
        self.reader.seek(std::io::SeekFrom::Start(self.offset))?;
        let slice = &mut buf[0..to_read];
        let br = self.reader.read(slice)?;
        self.offset = self.offset.saturating_add(
            br.try_into()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
        );
        self.size -= br; //safe
        Ok(br)
    }
}
