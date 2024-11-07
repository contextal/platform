//! Message properties
//!
//! Intended for internal use but publicly exposed for research purposes and low
//! level operations
pub mod tags;

use ctxole::Ole;
use ctxutils::io::*;
pub use ctxutils::win32::{filetime_to_datetime, GUID};
use std::borrow::Cow;
use std::fmt;
use std::io::{self, Read, Seek};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

#[derive(Debug)]
/// Property value stored inline
pub enum InlineValue {
    /// Boolean value
    Bool(bool),
    /// Integer value
    Int(i64),
    /// Floating point value
    Float(f64),
    /// Datetime value
    Time(Option<time::OffsetDateTime>),
    /// Error value
    Error(i32),
    /// Object value
    Object((u32, u32)),
}

impl InlineValue {
    /// Return the value as boolean
    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    /// Return the value as integer
    pub fn as_int(&self) -> Option<i64> {
        if let Self::Int(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    /// Return the value as float
    pub fn as_float(&self) -> Option<f64> {
        if let Self::Float(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    /// Return the value as datetime
    pub fn as_time(&self) -> Option<Option<&time::OffsetDateTime>> {
        if let Self::Time(v) = self {
            Some(v.as_ref())
        } else {
            None
        }
    }

    /// Return the value as error
    pub fn as_error(&self) -> Option<i32> {
        if let Self::Error(v) = self {
            Some(*v)
        } else {
            None
        }
    }
}

#[derive(Debug)]
/// Property value stored inside a stream
pub struct StreamedValue {
    /// Value id
    pub id: u16,
    /// Property type
    pub ptype: u16,
    /// Streamed value size
    pub size: u32,
}

impl StreamedValue {
    /// Return a reader for the streamed value
    pub fn get_stream<'a, R: Read + Seek>(
        &self,
        ole: &'a Ole<R>,
        base: &str,
    ) -> Result<impl Read + Seek + 'a, std::io::Error> {
        let path = format!("{}__substg1.0_{:04X}{:04X}", base, self.id, self.ptype);
        let stream = ole.get_stream_reader(&ole.get_entry_by_name(path.as_str())?);
        Ok(SeekTake::new(stream, u64::from(self.size)))
    }
}

#[derive(Debug)]
/// Property value (inline or streamed)
pub enum PropertyValue {
    /// Inline value
    Inline(InlineValue),
    /// String8 value
    String8(StreamedValue),
    /// String value
    String(StreamedValue),
    /// Binary value
    Binary(StreamedValue),
    /// GUID value
    GUID(StreamedValue),
}

#[derive(Debug)]
/// Message Property
pub struct Property {
    /// Property key
    pub key: Cow<'static, str>,
    /// Property flags
    pub flags: u32,
    /// Property value
    pub value: PropertyValue,
}

impl Property {
    fn from_bytes(
        buf: &[u8; 16],
        prop_map: &PropertyMap,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let ptype = u16::from_le_bytes(buf[0..2].try_into().unwrap());
        let id = u16::from_le_bytes(buf[2..4].try_into().unwrap());
        let flags = u32::from_le_bytes(buf[4..8].try_into().unwrap());
        let key = if id & 0x8000 == 0 {
            if let Some(tagname) = tags::get_tag_name(id, ptype) {
                Cow::from(tagname)
            } else {
                Cow::from(format!("Tag#{:04x}", id))
            }
        } else {
            let idx = usize::from(id - 0x8000);
            prop_map
                .0
                .get(idx)
                .map(|p| match &p.map_type {
                    PropertyNameType::Named(s) => Cow::from(s.clone()),
                    PropertyNameType::Numeric(v) => Cow::from(format!("#{:08x}", v)),
                })
                .ok_or_else(|| format!("Cannot map named item {}", idx))?
        };
        let value = match ptype {
            // --- INLINE ---
            0x000b /* PtypBoolean */ => PropertyValue::Inline(InlineValue::Bool(buf[8] != 0)),
            0x0002 /* PtypInteger16 */ => PropertyValue::Inline(InlineValue::Int(i64::from(i16::from_le_bytes(buf[8..10].try_into().unwrap())))),
            0x0003 /* PtypInteger32 */ => PropertyValue::Inline(InlineValue::Int(i64::from(i32::from_le_bytes(buf[8..12].try_into().unwrap())))),
            0x0014 /* PtypInteger64 */ => PropertyValue::Inline(InlineValue::Int(i64::from_le_bytes(buf[8..16].try_into().unwrap()))),
            0x0004 /* PtypFloating32 */ => PropertyValue::Inline(InlineValue::Float(f64::from(f32::from_le_bytes(buf[8..12].try_into().unwrap())))),
            0x0005 /* PtypFloating64 */ => PropertyValue::Inline(InlineValue::Float(f64::from_le_bytes(buf[8..16].try_into().unwrap()))),
            0x000a /* PtypErrorCode */ => PropertyValue::Inline(InlineValue::Error(i32::from_le_bytes(buf[8..12].try_into().unwrap()))),
            0x0007 /* PtypFloatingTime */ => {
                PropertyValue::Inline(InlineValue::Time(filetime_to_datetime(f64::from_le_bytes(buf[8..16].try_into().unwrap()) as u64)))
            }
            0x0040 /* PtypTime */ => {
                PropertyValue::Inline(InlineValue::Time(filetime_to_datetime(u64::from_le_bytes(buf[8..16].try_into().unwrap()))))
            }
            0x000d => {
                let nid = u32::from_le_bytes(buf[8..12].try_into().unwrap());
                let size = u32::from_le_bytes(buf[12..16].try_into().unwrap());
                PropertyValue::Inline(InlineValue::Object((nid, size)))
            }
            // --- EXTERNAL ---
            0x001e /* PtypString8 */ => {
                let size = u32::from_le_bytes(buf[8..12].try_into().unwrap())
                    .saturating_sub(1);
                PropertyValue::String8(StreamedValue {id, ptype, size })
            }
            0x001f /* PtypString */ => {
                let size = u32::from_le_bytes(buf[8..12].try_into().unwrap())
                    .saturating_sub(2);
                PropertyValue::String(StreamedValue {id, ptype, size })
            }
            0x0102 /* PtypBinary */ => {
                let size = u32::from_le_bytes(buf[8..12].try_into().unwrap());
                PropertyValue::Binary(StreamedValue {id, ptype, size })
            }
            0x0048 /* PtypGuid */ => {
                PropertyValue::GUID(StreamedValue {id, ptype, size: 16 })
            }
            _ => return Err(format!("Unsupported property type {:04x} (property id {:04x})", ptype, id).into())
        };
        Ok(Self { key, flags, value })
    }

    /// Return the property value as boolean
    pub fn as_bool(&self) -> Option<bool> {
        if let PropertyValue::Inline(v) = &self.value {
            v.as_bool()
        } else {
            None
        }
    }

    /// Return the property value as integer
    pub fn as_int(&self) -> Option<i64> {
        if let PropertyValue::Inline(v) = &self.value {
            v.as_int()
        } else {
            None
        }
    }

    /// Return the property value as float
    pub fn as_float(&self) -> Option<f64> {
        if let PropertyValue::Inline(v) = &self.value {
            v.as_float()
        } else {
            None
        }
    }

    /// Return the property value as datetime
    pub fn as_time(&self) -> Option<Option<&time::OffsetDateTime>> {
        if let PropertyValue::Inline(v) = &self.value {
            v.as_time()
        } else {
            None
        }
    }

    /// Return the property value as error
    pub fn as_error(&self) -> Option<i32> {
        if let PropertyValue::Inline(v) = &self.value {
            v.as_error()
        } else {
            None
        }
    }

    /// Read a GUID property value
    pub fn read_guid<'a, R: Read + Seek>(
        &self,
        ole: &'a Ole<R>,
        base: &str,
    ) -> Option<Result<GUID, std::io::Error>> {
        if let PropertyValue::GUID(sv) = &self.value {
            Some(
                sv.get_stream(ole, base)
                    .and_then(|mut stream| GUID::from_le_stream(&mut stream)),
            )
        } else {
            None
        }
    }

    /// Read a String or String8 property value
    pub fn read_string<'a, R: Read + Seek>(
        &self,
        ole: &'a Ole<R>,
        base: &str,
        cp: u16,
    ) -> Option<Result<String, std::io::Error>> {
        let mut s = String::new();
        match &self.value {
            PropertyValue::String(sv) => Some(
                sv.get_stream(ole, base)
                    .and_then(|stream| {
                        Ok(utf8dec_rs::UTF8DecReader::for_label("UTF-16LE", stream).unwrap())
                    })
                    .and_then(|mut stream| stream.read_to_string(&mut s))
                    .and_then(|_| Ok(s)),
            ),
            PropertyValue::String8(sv) => Some(
                sv.get_stream(ole, base)
                    .and_then(|stream| Ok(utf8dec_rs::UTF8DecReader::for_windows_cp(cp, stream)))
                    .and_then(|mut stream| stream.read_to_string(&mut s))
                    .and_then(|_| Ok(s)),
            ),
            _ => None,
        }
    }
}

impl PartialEq<str> for Property {
    fn eq(&self, other: &str) -> bool {
        self.key == other
    }
}

/// Properties
pub struct Properties<'o, O: Read + Seek> {
    ole: &'o Ole<O>,
    /// Base path
    pub base: String,
    /// Code page
    pub codepage: u16,
    /// Properties
    pub entries: Vec<Property>,
}

impl<'o, O: Read + Seek> Properties<'o, O> {
    /// Parse properties from a stream
    pub(crate) fn new<R: Read>(
        mut r: R,
        base: &str,
        prop_map: &PropertyMap,
        ole: &'o Ole<O>,
    ) -> Result<Self, io::Error> {
        let mut buf = [0u8; 16];
        let mut entries: Vec<Property> = Vec::new();
        loop {
            let read_res = r.read_exact(&mut buf);
            match read_res {
                Ok(_) => match Property::from_bytes(&buf, prop_map) {
                    Ok(p) => entries.push(p),
                    Err(e) => debug!("Failed to parse property: {e}"),
                },
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
        }
        let codepage = entries
            .iter()
            .find_map(|p| {
                if p == "MessageCodepage" {
                    p.as_int().map(|v| v as u16)
                } else {
                    None
                }
            })
            .unwrap_or(1252u16);
        Ok(Self {
            ole,
            base: base.to_string(),
            codepage,
            entries,
        })
    }

    /// Find property by name
    pub fn find(&self, name: &str) -> Option<&Property> {
        self.entries.iter().find(|p| *p == name)
    }

    /// Return the property value as boolean
    pub fn as_bool(&self, name: &str) -> Option<bool> {
        self.find(name).and_then(|v| v.as_bool())
    }

    /// Return the property value as integer
    pub fn as_int(&self, name: &str) -> Option<i64> {
        self.find(name).and_then(|v| v.as_int())
    }

    /// Return the property value as float
    pub fn as_float(&self, name: &str) -> Option<f64> {
        self.find(name).and_then(|v| v.as_float())
    }

    /// Return the property value as date time
    pub fn as_time(&self, name: &str) -> Option<Option<&time::OffsetDateTime>> {
        self.find(name).and_then(|v| v.as_time())
    }

    /// Return the property value as error
    pub fn as_error(&self, name: &str) -> Option<i32> {
        self.find(name).and_then(|v| v.as_error())
    }

    /// Read a GUID property value
    pub fn read_guid(&self, name: &str) -> Option<Result<GUID, std::io::Error>> {
        self.find(name)
            .and_then(|v| v.read_guid(self.ole, &self.base))
    }

    /// Read a String or String8 property value
    pub fn read_string(&self, name: &str) -> Option<Result<String, std::io::Error>> {
        self.find(name)
            .and_then(|v| v.read_string(self.ole, &self.base, self.codepage))
    }

    /// Return a reader for the streamed propery value
    pub fn get_stream(&self, name: &str) -> Option<Result<impl Read + Seek + 'o, std::io::Error>> {
        let p = self.find(name)?;
        let sv = match &p.value {
            PropertyValue::String8(sv) => sv,
            PropertyValue::String(sv) => sv,
            PropertyValue::Binary(sv) => sv,
            PropertyValue::GUID(sv) => sv,
            _ => return None,
        };
        Some(sv.get_stream(self.ole, &self.base))
    }
}

impl<'o, O: Read + Seek> fmt::Debug for Properties<'o, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.debug_struct("Properties")
            .field("base", &self.base)
            .field("codepage", &self.codepage)
            .field("entries", &self.entries)
            .finish()
    }
}

#[derive(Debug)]
/// A name mapper for a Property
pub struct PropertyNameMap {
    /// Property name or number
    pub map_type: PropertyNameType,
    /// Property index
    pub index: u16,
    /// Property [`GUID`]
    pub guid: Option<GUID>,
}

#[derive(Debug)]
/// Property name or number
pub enum PropertyNameType {
    /// Property name
    Named(String),
    /// Property number
    Numeric(u32),
}

#[derive(Debug)]
/// Global property map in a [`Msg`](super::Msg)
pub struct PropertyMap(pub Vec<PropertyNameMap>);

impl PropertyMap {
    /// Read the full map
    pub fn new<R: Read + Seek>(ole: &Ole<R>) -> Result<Self, io::Error> {
        let mut guid_stream = ole
            .get_stream_reader(&ole.get_entry_by_name("__nameid_version1.0/__substg1.0_00020102")?);
        let mut guids: Vec<GUID> = Vec::new();
        let mut buf = [0u8; 16];
        loop {
            let read_res = guid_stream.read_exact(&mut buf);
            match read_res {
                Ok(_) => {
                    guids.push(GUID::from_le_bytes(&buf).unwrap());
                }
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
        }
        let mut map: Vec<PropertyNameMap> = Vec::new();
        let mut string_stream = ole
            .get_stream_reader(&ole.get_entry_by_name("__nameid_version1.0/__substg1.0_00040102")?);
        let mut entry_stream = ole
            .get_stream_reader(&ole.get_entry_by_name("__nameid_version1.0/__substg1.0_00030102")?);
        loop {
            let read_res = entry_stream.read_exact(&mut buf[0..8]);
            match read_res {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
            let name_id_or_offset = u32::from_le_bytes(buf[0..4].try_into().unwrap());
            let guid_and_kind = u16::from_le_bytes(buf[4..6].try_into().unwrap());
            let prop_index = u16::from_le_bytes(buf[6..8].try_into().unwrap());
            let is_numeric = (guid_and_kind & 1) == 0;
            let guid_index = guid_and_kind >> 1;
            let guid = match guid_index {
                0 => None,
                1 => "00020328-0000-0000-C000-000000000046".parse().ok(), // PS_MAPI
                2 => "00020329-0000-0000-C000-000000000046".parse().ok(), // PS_PUBLIC_STRINGS
                idx => guids.get(usize::from(idx) - 3).cloned(),
            };
            let map_type = if is_numeric {
                PropertyNameType::Numeric(name_id_or_offset)
            } else {
                string_stream.seek(io::SeekFrom::Start(u64::from(name_id_or_offset)))?;
                let len = rdu32le(&mut string_stream)?;
                let mut rd = utf8dec_rs::UTF8DecReader::for_label(
                    "UTF-16LE",
                    (&mut string_stream).take(u64::from(len)),
                )
                .unwrap();
                let mut s = String::new();
                rd.read_to_string(&mut s)?;
                PropertyNameType::Named(s)
            };
            map.push(PropertyNameMap {
                map_type,
                index: prop_index,
                guid,
            });
        }
        Ok(PropertyMap(map))
    }
}
