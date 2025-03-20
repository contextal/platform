//! # Low level access to Ole Property Set Streams
//!
//! [`OlePS`] offers support for extracting features and data from *Simple Property Set Streams*
//!
//! The implementation is entirely based on
//! [\[MS-OLEPS\]](https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-oleps/bf7aeae8-c47a-4939-9f45-700158dac3bc).
//!
//! [`OlePS`] provides access to the stream metadata as well as to to its properties through
//! property iterators.
//!
//! Note: access to individual array elements is currently not available.
//!
//! # Examples
//! ```no_run
//! use std::io::{self,Read,Seek};
//! use ctxole::oleps::{OlePS};
//!
//! fn print_string_properties<R: Read + Seek>(mut f: R) -> Result<(), io::Error> {
//!     let oleps = OlePS::new(&mut f)?;
//!     for p in oleps.properties() {
//!         println!("{:x?}", p);
//!     }
//!     Ok(())
//! }
//! ```
//!
//! # Caveats
//!
//! `OlePS` will not honor the *PropertySet* version restrictions and will extract properties that
//! violate the version rules, if they are present

#[cfg(test)]
mod test;

use ctxutils::{
    io::*,
    win32::{GUID, filetime_to_datetime},
};
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::str::FromStr;
use std::{
    fmt::{Debug, Display},
    io::{Cursor, Error, ErrorKind, Read, Seek, SeekFrom},
};
use time::{Duration, OffsetDateTime, PrimitiveDateTime};

/// The specs neglect to state whether it's legal for multiple CodePage properties to appears
/// within a single set. It's empirically possible to demonstrate that Office disregards them and
/// that Windows Explorer instead fails to decode it entirely.
///
/// This implementation follows the Office behavior - i.e. ignores subsequent CodePage properties
/// beyond the first one.
///
/// This implementation defaults to CP1252 in case no such CodePage property appears.
type CodePage = u16;
type IndirectPropertyName = CodepageString;

#[inline]
fn into_usize<T: TryInto<usize>>(v: T) -> Result<usize, Error> {
    TryInto::<usize>::try_into(v)
        .map_err(|_| Error::new(ErrorKind::InvalidData, "Cannot convert value to usize"))
}

/// Wrapper around ANSI string
///
/// IMPORTANT NOTE:
///
/// Despite what stated in [MS-OLEPS], CodepageString is very likely NEVER padded
/// unless the CodePage is CP_WINUNICODE
/// The correct padding rules are probably NOT those presented in 2.5 (CodepageString)
/// but rather those described in 2.16 (DictionaryEntry)
/// However, due to MS combination of ineptitude, closeness and failure to understand,
/// implement and document their own formats, there is no clear and definite answer
///
/// With all this in mind, this implementation has opted to:
/// - honor the (bad) specifications (i.e. apply padding) in case the CodepageString
///   appears alone (VT_BSTR, VT_LPSTR) - as this has no practical ill effects
/// - purposedly violate the specifications (i.e. pack rather than pad) in case the
///   the CodepageString is part of a vector or an array - to mimic the real life behavior
///   of both Office and explorer.exe
pub struct CodepageString {
    /// Contains raw string data
    pub data: Vec<u8>,
    /// The string code page
    pub codepage: CodePage,
}

const CP_WINUNICODE: CodePage = 0x4b0;
impl CodepageString {
    fn new_noalign<R: Read>(reader: &mut R, codepage: CodePage, size: u32) -> Result<Self, Error> {
        let data = if codepage == CP_WINUNICODE {
            let us = UnicodeString::new_noalign(reader, size)?;
            us.data
        } else {
            let mut data = vec![0u8; into_usize(size)?];
            reader.read_exact(&mut data)?;
            if data.last() == Some(&0) {
                data.pop();
            }
            data
        };
        Ok(Self { data, codepage })
    }
    /// Indicates whether the string uses the CP_WINUNICODE (0x04B0) code page
    fn is_winunicode(&self) -> bool {
        self.codepage == CP_WINUNICODE
    }

    /// Decode text using codepage
    fn decode(&self) -> String {
        if self.is_winunicode() {
            utf8dec_rs::decode_utf16le_str(&self.data)
        } else {
            utf8dec_rs::decode_win_str(&self.data, self.codepage)
        }
    }
}

impl Display for CodepageString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.decode())
    }
}

impl Debug for CodepageString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let decoded = self.decode();
        f.debug_struct("CodepageString")
            .field("decoded", &decoded)
            .field("codepage", &self.codepage)
            .field("data", &self.data)
            .finish()
    }
}

impl Serialize for CodepageString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.decode().serialize(serializer)
    }
}

/// Wrapper around UTF-16 string
#[derive(Debug)]
pub struct UnicodeString {
    /// Contains raw string data
    pub data: Vec<u8>,
}

impl UnicodeString {
    fn new_noalign<R: Read>(reader: &mut R, size: u32) -> Result<Self, Error> {
        let mut data = vec![0u8; into_usize(size)?];
        reader.read_exact(&mut data)?;
        // This is technically invalid, but that's what Windows does
        data.truncate(data.len() & !1);
        if let Some(noterm) = data.strip_suffix(&[0, 0]) {
            data.truncate(noterm.len());
        }
        Ok(Self { data })
    }
}

impl Display for UnicodeString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", utf8dec_rs::decode_utf16le_str(&self.data))
    }
}

impl Serialize for UnicodeString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        utf8dec_rs::decode_utf16le_str(&self.data).serialize(serializer)
    }
}

/// Contains a 64-bit value representing the number of 100-nanosecond intervals since January 1, 1601 (UTC).
pub struct Filetime {
    date_time: u64,
}

impl Debug for Filetime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Filetime")
            .field("date_time", &self.date_time)
            .field("as_datetime", &self.as_datetime())
            .finish()
    }
}

impl Filetime {
    /// Converts Filetime to OffsetDateTime
    pub fn as_datetime(&self) -> Option<OffsetDateTime> {
        filetime_to_datetime(self.date_time)
    }

    fn as_duration(&self) -> Option<Duration> {
        let delta = self.date_time as i64;
        if delta < 0 {
            return None;
        }
        delta.checked_mul(100).map(Duration::nanoseconds)
    }
}

impl Display for Filetime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.date_time == 0 {
            write!(f, "Not set")
        } else if let Some(dt) = self.as_datetime() {
            write!(f, "{dt}")
        } else {
            write!(f, "Invalid")
        }
    }
}

impl Serialize for Filetime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

/// Represents the typed value of a property in a property set.
#[derive(Debug, Serialize)]
pub enum TypedPropertyValue {
    /// VT_EMPTY (0x0000)
    Empty,
    /// VT_NULL (0x0001)
    Null,
    /// VT_I2 (0x0002)
    I2(i16),
    /// VT_I4 (0x0003)
    I4(i32),
    /// VT_R4 (0x0004)
    R4(f32),
    /// VT_R8 (0x0005)
    R8(f64),
    /// VT_CY (0x0006)
    CY(Currency),
    /// VT_DATE (0x0007)
    Date(Date),
    /// VT_BSTR (0x0008)
    BStr(CodepageString),
    /// VT_ERROR (0x000A)
    Error(Hresult),
    /// VT_BOOL (0x000B)
    Bool(bool),
    /// VT_DECIMAL (0x000E)
    Decimal(Decimal),
    /// VT_I1 (0x0010)
    I1(i8),
    /// VT_UI1 (0x0011)
    UI1(u8),
    /// VT_UI2 (0x0012)
    UI2(u16),
    /// VT_UI4 (0x0013)
    UI4(u32),
    /// VT_I8 (0x0014)
    I8(i64),
    /// VT_UI8 (0x0015)
    UI8(u64),
    /// VT_INT (0x0016)
    Int(i32),
    /// VT_UINT (0x0017)
    UInt(u32),
    /// VT_LPSTR (0x001E)
    LPStr(CodepageString),
    /// VT_LPWSTR (0x001F)
    LPWStr(UnicodeString),
    /// VT_FILETIME (0x0040)
    Filetime(Filetime),
    /// VT_BLOB (0x0041)
    Blob(Blob),
    /// VT_STREAM (0x0042)
    Stream(IndirectPropertyName),
    /// VT_STORAGE (0x0043)
    Storage(IndirectPropertyName),
    /// VT_STREAMED_OBJECT (0x0044)
    StreamedObject(IndirectPropertyName),
    /// VT_STORED_OBJECT (0x0045)
    StoredObject(IndirectPropertyName),
    /// VT_BLOB_OBJECT (0x0046)
    BlobObject(Blob),
    /// VT_CF (0x0047)
    CF(ClipboardData),
    /// VT_CLSID (0x0048)
    Clsid(GUID),
    /// VT_VERSIONED_STREAM (0x0049)
    VersionedStream(VersionedStream),
    /// VT_VECTOR | VT_I2 (0x1002)
    VectorI2(MsVector<i16>),
    /// VT_VECTOR | VT_I4 (0x1003)
    VectorI4(MsVector<i32>),
    /// VT_VECTOR | VT_R4 (0x1004)
    VectorR4(MsVector<f32>),
    /// VT_VECTOR | VT_R8 (0x1005)
    VectorR8(MsVector<f64>),
    /// VT_VECTOR | VT_CY (0x1006)
    VectorCY(MsVector<Currency>),
    /// VT_VECTOR | VT_DATE (0x1007)
    VectorDate(MsVector<Date>),
    /// VT_VECTOR | VT_BSTR (0x1008)
    VectorBSTR(MsVector<CodepageString>),
    /// VT_VECTOR | VT_ERROR (0x100A)
    VectorError(MsVector<Hresult>),
    /// VT_VECTOR | VT_BOOL (0x100B)
    VectorBool(MsVector<bool>),
    /// VT_VECTOR | VT_VARIANT (0x100C)
    VectorVariant(Box<MsVector<TypedPropertyValue>>),
    /// VT_VECTOR | VT_I1 (0x1010)
    VectorI1(MsVector<i8>),
    /// VT_VECTOR | VT_UI1 (0x1011)
    VectorUI1(MsVector<u8>),
    /// VT_VECTOR | VT_UI2 (0x1012)
    VectorUI2(MsVector<u16>),
    /// VT_VECTOR | VT_UI4 (0x1013)
    VectorUI4(MsVector<u32>),
    /// VT_VECTOR | VT_I8 (0x1014)
    VectorI8(MsVector<i64>),
    /// VT_VECTOR | VT_UI8 (0x1015)
    VectorUI8(MsVector<u64>),
    /// VT_VECTOR | VT_LPSTR (0x101E)
    VectorLPSTR(MsVector<CodepageString>),
    /// VT_VECTOR | VT_LPWSTR (0x101F)
    VectorLPWSTR(MsVector<UnicodeString>),
    /// VT_VECTOR | VT_FILETIME (0x1040)
    VectorFiletime(MsVector<Filetime>),
    /// VT_VECTOR | VT_CF (0x1047)
    VectorCF(MsVector<ClipboardData>),
    /// VT_VECTOR | VT_CLSID (0x1048)
    VectorCLSID(MsVector<GUID>),
    /// VT_ARRAY | VT_I2 (0x2002)
    ArrayI2(MsArray<i16>),
    /// VT_ARRAY | VT_I4 (0x2003)
    ArrayI4(MsArray<i32>),
    /// VT_ARRAY | VT_R4 (0x2004)
    ArrayR4(MsArray<f32>),
    /// VT_ARRAY | VT_R8 (0x2005)
    ArrayR8(MsArray<f64>),
    /// VT_ARRAY | VT_CY (0x2006)
    ArrayCY(MsArray<Currency>),
    /// VT_ARRAY | VT_DATE (0x2007)
    ArrayDate(MsArray<Date>),
    /// VT_ARRAY | VT_BSTR (0x2008)
    ArrayBSTR(MsArray<CodepageString>),
    /// VT_ARRAY | VT_ERROR (0x200A)
    ArrayError(MsArray<Hresult>),
    /// VT_ARRAY | VT_BOOL (0x200B)
    ArrayBool(MsArray<bool>),
    /// VT_ARRAY | VT_VARIANT (0x200C)
    ArrayVariant(Box<MsArray<TypedPropertyValue>>),
    /// VT_ARRAY | VT_DECIMAL (0x200E)
    ArrayDecimal(MsArray<Decimal>),
    /// VT_ARRAY | VT_I1 (0x2010)
    ArrayI1(MsArray<i8>),
    /// VT_ARRAY | VT_UI1 (0x2011)
    ArrayUI1(MsArray<u8>),
    /// VT_ARRAY | VT_UI2 (0x2012)
    ArrayUI2(MsArray<u16>),
    /// VT_ARRAY | VT_UI4 (0x2013)
    ArrayUI4(MsArray<u32>),
    /// VT_ARRAY | VT_INT (0x2016)
    ArrayINT(MsArray<i32>),
    /// VT_ARRAY | VT_UINT (0x2017)
    ArrayUINT(MsArray<u32>),
}

/// Represents a Windows Runtime error.
#[derive(Debug)]
pub struct Hresult {
    /// An integer that describes an error.
    pub value: u32,
}

impl Serialize for Hresult {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let result = format!("0x{:08X}", self.value);
        result.serialize(serializer)
    }
}

/// A currency number stored as an 8-byte, scaled by 10,000 to give a fixed-point number with 15 digits to the left of the decimal point and 4 digits to the right.
#[derive(Debug)]
pub struct Currency {
    value: i64,
}

impl Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let integer = self.value / 10000;
        let modulo = self.value % 10000;
        let mut result = integer.to_string();
        if modulo > 0 {
            let fraction = format!(".{:04}", modulo);
            result.push_str(fraction.trim_end_matches('0'));
        }
        write!(f, "{}", result)
    }
}

impl Serialize for Currency {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let integer = self.value / 10000;
        let modulo = self.value % 10000;
        let mut result = integer.to_string();
        if modulo > 0 {
            let fraction = format!(".{:04}", modulo);
            result.push_str(fraction.trim_end_matches('0'));
        }
        result.serialize(serializer)
    }
}

/// DATE is a type that specifies date and time information.
/// It is represented as an 8-byte floating-point number:
/// * The date information is represented by whole-number increments, starting with December 30, 1899 midnight as time zero.
/// * The time information is represented by the fraction of a day since the preceding midnight.
#[derive(Debug)]
pub struct Date {
    value: f64,
}

impl Date {
    /// Converts Date to PrimitiveDateTime
    pub fn to_datetime(&self) -> Option<PrimitiveDateTime> {
        let date = time::Date::from_calendar_date(1995, time::Month::November, 16)
            .unwrap()
            .checked_add(Duration::days(self.value.trunc() as i64))?;
        let tm = (self.value.fract() * 24f64 * 60f64 * 60f64).round() as u64;
        let ss = (tm % 60) as u8;
        let mm = (tm / 60 % 60) as u8;
        let hh = ((tm / 60 / 60) % 24) as u8;
        Some(PrimitiveDateTime::new(
            date,
            time::Time::from_hms(hh, mm, ss).ok()?,
        ))
    }
}

impl Serialize for Date {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let result = self.to_datetime().unwrap();
        result.to_string().serialize(serializer)
    }
}

/// Represents a decimal data type that provides a sign and scale for a number (as in coordinates).
#[derive(PartialEq, Debug, Serialize)]
pub struct Decimal {
    value: i128,
    scale: u8,
}

/// The BLOB structure, derived from Binary Large Object, contains information about a block of data.
#[derive(Debug, Serialize)]
pub struct Blob {
    /// The size in bytes of the bytes field.
    pub size: u32,
    /// Vector of bytes
    pub bytes: Vec<u8>,
}

/// Represents clipboard data.
#[derive(Debug, Serialize)]
pub struct ClipboardData {
    /// The total size in bytes of the format and data fields.
    pub size: u32,
    /// An application-specific identifier for the format of the data in the data field.
    pub format: u32,
    /// Vector of bytes
    pub data: Vec<u8>,
}

/// The VersionedStream packet represents a stream with an application-specific version GUID.
#[derive(Debug, Serialize)]
pub struct VersionedStream {
    /// Guid
    pub version_guid: GUID,
    /// Represents the name of a stream
    pub stream_name: IndirectPropertyName,
}

/// One-dimensional array of the scalar type
#[derive(Debug, Serialize)]
pub struct MsVector<T: FromOlepsReader> {
    /// data
    pub data: Vec<T>,
}

/// Multi-dimensional array of the scalar type, with elements in row-major order
#[derive(Debug, Serialize)]
pub struct MsArray<T: FromOlepsReader> {
    /// Represents the type and dimensions of an array property type
    pub header: ArrayHeader,
    /// Represents the size and index offset of a dimension of an array property type.
    pub dimensions: Vec<ArrayDimension>,
    /// data
    pub data: Vec<T>,
}

/// Represents the size and index offset of a dimension of an array property type.
#[derive(Debug, Serialize)]
pub struct ArrayDimension {
    /// Size of the dimension
    pub size: u32,
    /// A signed integer representing the index offset of the dimension. For example, an array dimension that is to be accessed with a 0-based index would have the value zero, whereas an array dimension that is to be accessed with a 1-based index would have the value 0x00000001
    pub index_offset: i32,
}

/// Represents the type and dimensions of an array property type
#[derive(Debug, Serialize)]
pub struct ArrayHeader {
    /// Property type
    pub value_type: u32,
    /// An unsigned integer representing the number of dimensions in the array property. MUST be at least 1 and at most 31.
    pub num_dimensions: u32,
}

/// Helper trait
pub trait FromOlepsReader {
    /// Read Object from OlePS, propagate codepage to proper structs
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        codepage: CodePage,
        align: bool,
    ) -> Result<Self, Error>
    where
        Self: Sized;
}

impl FromOlepsReader for TypedPropertyValue {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        codepage: CodePage,
        align: bool,
    ) -> Result<Self, Error> {
        let value_type = rdu16le(reader)?;
        let padding = rdu16le(reader)?;
        if padding != 0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid padding {padding:x} found in property with type 0x{value_type:x}"),
            ));
        }
        use TypedPropertyValue as TPV;
        let value = match value_type {
            0x0000 => TPV::Empty,
            0x0001 => TPV::Null,
            0x0002 => TPV::I2(i16::from_oleps_reader(reader, codepage, align)?),
            0x0003 => TPV::I4(i32::from_oleps_reader(reader, codepage, align)?),
            0x0004 => TPV::R4(f32::from_oleps_reader(reader, codepage, align)?),
            0x0005 => TPV::R8(f64::from_oleps_reader(reader, codepage, align)?),
            0x0006 => TPV::CY(Currency::from_oleps_reader(reader, codepage, align)?),
            0x0007 => TPV::Date(Date::from_oleps_reader(reader, codepage, align)?),
            0x0008 => TPV::BStr(CodepageString::from_oleps_reader(reader, codepage, align)?),
            0x000A => TPV::Error(Hresult::from_oleps_reader(reader, codepage, align)?),
            0x000B => TPV::Bool(bool::from_oleps_reader(reader, codepage, align)?),
            0x000E => TPV::Decimal(Decimal::from_oleps_reader(reader, codepage, align)?),
            0x0010 => TPV::I1(i8::from_oleps_reader(reader, codepage, align)?),
            0x0011 => TPV::UI1(u8::from_oleps_reader(reader, codepage, align)?),
            0x0012 => TPV::UI2(u16::from_oleps_reader(reader, codepage, align)?),
            0x0013 => TPV::UI4(u32::from_oleps_reader(reader, codepage, align)?),
            0x0014 => TPV::I8(i64::from_oleps_reader(reader, codepage, align)?),
            0x0015 => TPV::UI8(u64::from_oleps_reader(reader, codepage, align)?),
            0x0016 => TPV::Int(i32::from_oleps_reader(reader, codepage, align)?),
            0x0017 => TPV::UInt(u32::from_oleps_reader(reader, codepage, align)?),
            0x001E => TPV::LPStr(CodepageString::from_oleps_reader(reader, codepage, align)?),
            0x001F => TPV::LPWStr(UnicodeString::from_oleps_reader(reader, codepage, align)?),
            0x0040 => TPV::Filetime(Filetime::from_oleps_reader(reader, codepage, align)?),
            0x0041 => TPV::Blob(Blob::from_oleps_reader(reader, codepage, align)?),
            0x0042 => TPV::Stream(IndirectPropertyName::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x0043 => TPV::Storage(IndirectPropertyName::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x0044 => TPV::StreamedObject(IndirectPropertyName::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x0045 => TPV::StoredObject(IndirectPropertyName::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x0046 => TPV::BlobObject(Blob::from_oleps_reader(reader, codepage, align)?),
            0x0047 => TPV::CF(ClipboardData::from_oleps_reader(reader, codepage, align)?),
            0x0048 => TPV::Clsid(GUID::from_oleps_reader(reader, codepage, align)?),
            0x0049 => {
                TPV::VersionedStream(VersionedStream::from_oleps_reader(reader, codepage, align)?)
            }
            0x1002 => TPV::VectorI2(MsVector::<i16>::from_oleps_reader(reader, codepage, align)?),
            0x1003 => TPV::VectorI4(MsVector::<i32>::from_oleps_reader(reader, codepage, align)?),
            0x1004 => TPV::VectorR4(MsVector::<f32>::from_oleps_reader(reader, codepage, align)?),
            0x1005 => TPV::VectorR8(MsVector::<f64>::from_oleps_reader(reader, codepage, align)?),
            0x1006 => TPV::VectorCY(MsVector::<Currency>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x1007 => TPV::VectorDate(MsVector::<Date>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x1008 => TPV::VectorBSTR(MsVector::<CodepageString>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x100A => TPV::VectorError(MsVector::<Hresult>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x100B => TPV::VectorBool(MsVector::<bool>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x100C => TPV::VectorVariant(Box::new(MsVector::<TPV>::new_variant(reader, codepage)?)),
            0x1010 => TPV::VectorI1(MsVector::<i8>::from_oleps_reader(reader, codepage, align)?),
            0x1011 => TPV::VectorUI1(MsVector::<u8>::from_oleps_reader(reader, codepage, align)?),
            0x1012 => TPV::VectorUI2(MsVector::<u16>::from_oleps_reader(reader, codepage, align)?),
            0x1013 => TPV::VectorUI4(MsVector::<u32>::from_oleps_reader(reader, codepage, align)?),
            0x1014 => TPV::VectorI8(MsVector::<i64>::from_oleps_reader(reader, codepage, align)?),
            0x1015 => TPV::VectorUI8(MsVector::<u64>::from_oleps_reader(reader, codepage, align)?),
            0x101E => TPV::VectorLPSTR(MsVector::<CodepageString>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x101F => TPV::VectorLPWSTR(MsVector::<UnicodeString>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x1040 => TPV::VectorFiletime(MsVector::<Filetime>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x1047 => TPV::VectorCF(MsVector::<ClipboardData>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x1048 => TPV::VectorCLSID(MsVector::<GUID>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x2002 => TPV::ArrayI2(MsArray::<i16>::from_oleps_reader(reader, codepage, align)?),
            0x2003 => TPV::ArrayI4(MsArray::<i32>::from_oleps_reader(reader, codepage, align)?),
            0x2004 => TPV::ArrayR4(MsArray::<f32>::from_oleps_reader(reader, codepage, align)?),
            0x2005 => TPV::ArrayR8(MsArray::<f64>::from_oleps_reader(reader, codepage, align)?),
            0x2006 => TPV::ArrayCY(MsArray::<Currency>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x2007 => TPV::ArrayDate(MsArray::<Date>::from_oleps_reader(reader, codepage, align)?),
            0x2008 => TPV::ArrayBSTR(MsArray::<CodepageString>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x200A => TPV::ArrayError(MsArray::<Hresult>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x200B => TPV::ArrayBool(MsArray::<bool>::from_oleps_reader(reader, codepage, align)?),
            0x200C => TPV::ArrayVariant(Box::new(MsArray::<TPV>::new_variant(reader, codepage)?)),
            0x200E => TPV::ArrayDecimal(MsArray::<Decimal>::from_oleps_reader(
                reader, codepage, align,
            )?),
            0x2010 => TPV::ArrayI1(MsArray::<i8>::from_oleps_reader(reader, codepage, align)?),
            0x2011 => TPV::ArrayUI1(MsArray::<u8>::from_oleps_reader(reader, codepage, align)?),
            0x2012 => TPV::ArrayUI2(MsArray::<u16>::from_oleps_reader(reader, codepage, align)?),
            0x2013 => TPV::ArrayUI4(MsArray::<u32>::from_oleps_reader(reader, codepage, align)?),
            0x2016 => TPV::ArrayINT(MsArray::<i32>::from_oleps_reader(reader, codepage, align)?),
            0x2017 => TPV::ArrayUINT(MsArray::<u32>::from_oleps_reader(reader, codepage, align)?),
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Unsupported type 0x{:04X}", value_type),
                ));
            }
        };

        Ok(value)
    }
}

impl FromOlepsReader for i8 {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        align: bool,
    ) -> Result<Self, Error> {
        let res = rdi8(reader)?;
        if align {
            reader.seek(SeekFrom::Current(3))?;
        }
        Ok(res)
    }
}

impl FromOlepsReader for i16 {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        align: bool,
    ) -> Result<Self, Error> {
        let res = rdi16le(reader)?;
        if align {
            reader.seek(SeekFrom::Current(2))?;
        }
        Ok(res)
    }
}

impl FromOlepsReader for i32 {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        rdi32le(reader)
    }
}

impl FromOlepsReader for i64 {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        rdi64le(reader)
    }
}

impl FromOlepsReader for u8 {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        align: bool,
    ) -> Result<Self, Error> {
        let res = rdu8(reader)?;
        if align {
            reader.seek(SeekFrom::Current(3))?;
        }
        Ok(res)
    }
}

impl FromOlepsReader for u16 {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        align: bool,
    ) -> Result<Self, Error> {
        let res = rdu16le(reader)?;
        if align {
            reader.seek(SeekFrom::Current(2))?;
        }
        Ok(res)
    }
}

impl FromOlepsReader for u32 {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        rdu32le(reader)
    }
}

impl FromOlepsReader for u64 {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        rdu64le(reader)
    }
}

impl FromOlepsReader for f32 {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        rdf32le(reader)
    }
}

impl FromOlepsReader for f64 {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        rdf64le(reader)
    }
}

impl FromOlepsReader for bool {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        align: bool,
    ) -> Result<Self, Error> {
        let data = rdu16le(reader)?;
        let res = match data {
            0xFFFF => true,
            0x0000 => false,
            _ => {
                // TODO: Warn about corrupted value
                true
            }
        };
        if align {
            reader.seek(SeekFrom::Current(2))?;
        }
        Ok(res)
    }
}

impl FromOlepsReader for Hresult {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        Ok(Hresult {
            value: rdu32le(reader)?,
        })
    }
}

impl FromOlepsReader for Currency {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        Ok(Currency {
            value: rdi64le(reader)?,
        })
    }
}

impl FromOlepsReader for Date {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        Ok(Date {
            value: rdf64le(reader)?,
        })
    }
}

impl FromOlepsReader for Filetime {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        Ok(Filetime {
            date_time: rdu64le(reader)?,
        })
    }
}

impl FromOlepsReader for Decimal {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        let _reserved = rdu16le(reader)?;
        let scale = rdu8(reader)?;
        let sign = rdu8(reader)?;
        let hi32 = rdu32le(reader)?;
        let lo64 = rdu64le(reader)?;

        let mut value = (i128::from(hi32) << 64) | i128::from(lo64);
        if sign == 0x80 {
            value *= -1;
        }

        Ok(Decimal { value, scale })
    }
}

impl FromOlepsReader for Blob {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        let size = rdu32le(reader)?;
        let mut bytes = vec![0u8; into_usize(size)?];
        reader.read_exact(&mut bytes)?;
        let padlen = ((size + 3) & !3) - size;
        reader.seek(SeekFrom::Current(i64::from(padlen)))?;
        Ok(Blob { bytes, size })
    }
}

impl FromOlepsReader for ClipboardData {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        let size = rdu32le(reader)?;
        let format = rdu32le(reader)?;
        let data_size = into_usize(size - 4)?;
        let mut data = vec![0u8; data_size];
        reader.read_exact(&mut data)?;
        let padlen = ((size + 3) & !3) - size;
        reader.seek(SeekFrom::Current(i64::from(padlen)))?;
        Ok(ClipboardData { size, format, data })
    }
}

impl FromOlepsReader for GUID {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        reader.read_guid()
    }
}

impl FromOlepsReader for VersionedStream {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        codepage: CodePage,
        align: bool,
    ) -> Result<Self, Error> {
        let version_guid = reader.read_guid()?;
        let stream_name = IndirectPropertyName::from_oleps_reader(reader, codepage, align)?;
        Ok(VersionedStream {
            version_guid,
            stream_name,
        })
    }
}

impl<T: FromOlepsReader> MsVector<T> {
    fn new_variant<R: Read + Seek>(
        reader: &mut R,
        codepage: CodePage,
    ) -> Result<MsVector<TypedPropertyValue>, Error> {
        let nitems = into_usize(rdu32le(reader)?)?;
        let mut vector = Vec::<TypedPropertyValue>::with_capacity(nitems);
        for _ in 0..nitems {
            let value_type = rdu16le(reader)?;
            let padding = rdu16le(reader)?;
            if padding != 0 {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Invalid padding {padding:x} found in property with type MsVector<0x{value_type:x}>"
                    ),
                ));
            }
            let value = match value_type {
                0x0002 => TypedPropertyValue::I2(i16::from_oleps_reader(reader, codepage, true)?),
                0x0003 => TypedPropertyValue::I4(i32::from_oleps_reader(reader, codepage, true)?),
                0x0004 => TypedPropertyValue::R4(f32::from_oleps_reader(reader, codepage, true)?),
                0x0005 => TypedPropertyValue::R8(f64::from_oleps_reader(reader, codepage, true)?),
                0x0006 => {
                    TypedPropertyValue::CY(Currency::from_oleps_reader(reader, codepage, true)?)
                }
                0x0007 => {
                    TypedPropertyValue::Date(Date::from_oleps_reader(reader, codepage, true)?)
                }
                0x0008 => TypedPropertyValue::BStr(CodepageString::from_oleps_reader(
                    reader, codepage, false,
                )?),
                0x000A => {
                    TypedPropertyValue::Error(Hresult::from_oleps_reader(reader, codepage, true)?)
                }
                0x000B => {
                    TypedPropertyValue::Bool(bool::from_oleps_reader(reader, codepage, true)?)
                }
                0x0010 => TypedPropertyValue::I1(i8::from_oleps_reader(reader, codepage, true)?),
                0x0011 => TypedPropertyValue::UI1(u8::from_oleps_reader(reader, codepage, true)?),
                0x0012 => TypedPropertyValue::UI2(u16::from_oleps_reader(reader, codepage, true)?),
                0x0013 => TypedPropertyValue::UI4(u32::from_oleps_reader(reader, codepage, true)?),
                0x0014 => TypedPropertyValue::I8(i64::from_oleps_reader(reader, codepage, true)?),
                0x0015 => TypedPropertyValue::UI8(u64::from_oleps_reader(reader, codepage, true)?),
                0x001E => TypedPropertyValue::LPStr(CodepageString::from_oleps_reader(
                    reader, codepage, false,
                )?),
                0x001F => TypedPropertyValue::LPWStr(UnicodeString::from_oleps_reader(
                    reader, codepage, true,
                )?),
                0x0040 => TypedPropertyValue::Filetime(Filetime::from_oleps_reader(
                    reader, codepage, true,
                )?),
                0x0047 => TypedPropertyValue::CF(ClipboardData::from_oleps_reader(
                    reader, codepage, true,
                )?),
                0x0048 => {
                    TypedPropertyValue::Clsid(GUID::from_oleps_reader(reader, codepage, true)?)
                }
                _ => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("Unsupported MsVector type 0x{:04X}", value_type),
                    ));
                }
            };
            vector.push(value);
        }
        Ok(MsVector::<TypedPropertyValue> { data: vector })
    }
}

impl<T: FromOlepsReader> FromOlepsReader for MsVector<T> {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        let nitems = into_usize(rdu32le(reader)?)?;
        let mut vector = Vec::<T>::with_capacity(nitems);
        for _ in 0..nitems {
            let value = T::from_oleps_reader(reader, codepage, false)?;
            vector.push(value);
        }
        Ok(MsVector::<T> { data: vector })
    }
}

impl<T: FromOlepsReader> MsArray<T> {
    fn common<R: Read>(reader: &mut R) -> Result<(Self, usize), Error> {
        let header = ArrayHeader {
            value_type: rdu32le(reader)?,
            num_dimensions: rdu32le(reader)?,
        };
        let num_dimensions = into_usize(header.num_dimensions)?;
        if !(1..=31).contains(&num_dimensions) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Invalid NumDimensions ({}) in MsArray",
                    header.num_dimensions
                ),
            ));
        }
        let mut dimensions = Vec::<ArrayDimension>::with_capacity(num_dimensions);
        let mut nitems: usize = if num_dimensions > 0 { 1 } else { 0 };
        for _ in 0..num_dimensions {
            let dimension = ArrayDimension {
                size: rdu32le(reader)?,
                index_offset: rdi32le(reader)?,
            };
            if let Some(res) = nitems.checked_mul(into_usize(dimension.size)?) {
                nitems = res;
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Data size overflow in MsArray",
                ));
            }
            dimensions.push(dimension);
        }
        Ok((
            MsArray::<T> {
                header,
                dimensions,
                data: Vec::new(),
            },
            nitems,
        ))
    }

    fn new_variant<R: Read + Seek>(
        reader: &mut R,
        codepage: CodePage,
    ) -> Result<MsArray<TypedPropertyValue>, Error> {
        let (ret, nitems) = Self::common(reader)?;
        let mut data = Vec::<TypedPropertyValue>::with_capacity(nitems);
        for _ in 0..nitems {
            let value_type = rdu16le(reader)?;
            let padding = rdu16le(reader)?;
            if padding != 0 {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Invalid padding {padding:x} found in property with type MsVector<0x{value_type:x}>"
                    ),
                ));
            }
            let value = match value_type {
                0x0002 => TypedPropertyValue::I2(i16::from_oleps_reader(reader, codepage, true)?),
                0x0003 => TypedPropertyValue::I4(i32::from_oleps_reader(reader, codepage, true)?),
                0x0004 => TypedPropertyValue::R4(f32::from_oleps_reader(reader, codepage, true)?),
                0x0005 => TypedPropertyValue::R8(f64::from_oleps_reader(reader, codepage, true)?),
                0x0006 => {
                    TypedPropertyValue::CY(Currency::from_oleps_reader(reader, codepage, true)?)
                }
                0x0007 => {
                    TypedPropertyValue::Date(Date::from_oleps_reader(reader, codepage, true)?)
                }
                0x0008 => TypedPropertyValue::BStr(CodepageString::from_oleps_reader(
                    reader, codepage, false,
                )?),
                0x000A => {
                    TypedPropertyValue::Error(Hresult::from_oleps_reader(reader, codepage, true)?)
                }
                0x000B => {
                    TypedPropertyValue::Bool(bool::from_oleps_reader(reader, codepage, true)?)
                }
                0x000E => {
                    TypedPropertyValue::Decimal(Decimal::from_oleps_reader(reader, codepage, true)?)
                }
                0x0010 => TypedPropertyValue::I1(i8::from_oleps_reader(reader, codepage, true)?),
                0x0011 => TypedPropertyValue::UI1(u8::from_oleps_reader(reader, codepage, true)?),
                0x0012 => TypedPropertyValue::UI2(u16::from_oleps_reader(reader, codepage, true)?),
                0x0013 => TypedPropertyValue::UI4(u32::from_oleps_reader(reader, codepage, true)?),
                0x0016 => TypedPropertyValue::Int(i32::from_oleps_reader(reader, codepage, true)?),
                0x0017 => TypedPropertyValue::UInt(u32::from_oleps_reader(reader, codepage, true)?),
                _ => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("Unsupported MsArray type 0x{:04X}", value_type),
                    ));
                }
            };
            data.push(value);
        }
        Ok(MsArray::<TypedPropertyValue> {
            header: ret.header,
            dimensions: ret.dimensions,
            data,
        })
    }
}

impl<T: FromOlepsReader> FromOlepsReader for MsArray<T> {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        let (ret, nitems) = Self::common(reader)?;
        let mut data = Vec::<T>::with_capacity(nitems);
        for _ in 0..nitems {
            let value = T::from_oleps_reader(reader, codepage, false)?;
            data.push(value);
        }
        Ok(Self { data, ..ret })
    }
}

impl FromOlepsReader for CodepageString {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        codepage: CodePage,
        align: bool,
    ) -> Result<Self, Error> {
        let size = rdu32le(reader)?;
        let res = CodepageString::new_noalign(reader, codepage, size)?;
        if align || codepage == CP_WINUNICODE {
            let padlen = ((size + 3) & !3) - size;
            reader.seek(SeekFrom::Current(i64::from(padlen)))?;
        }
        Ok(res)
    }
}

impl FromOlepsReader for UnicodeString {
    fn from_oleps_reader<R: Read + Seek>(
        reader: &mut R,
        _codepage: CodePage,
        _align: bool,
    ) -> Result<Self, Error> {
        let nchars = rdu32le(reader)?;
        let size = nchars
            .checked_mul(2)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "UnicodeString size overflow"))?;
        let res = UnicodeString::new_noalign(reader, size)?;
        // Realign in case length is odd
        let padlen = (nchars & 1) * 2;
        reader.seek(SeekFrom::Current(i64::from(padlen)))?;
        Ok(res)
    }
}

/// Represents a mapping between a property identifier and a property name.
#[derive(Debug)]
pub struct DictionaryEntry {
    /// Property identifier
    pub property_identifier: PropertyIdentifier,
    /// Property name
    pub name: CodepageString,
}

impl DictionaryEntry {
    fn new<R: Read + Seek>(reader: &mut R, codepage: CodePage) -> Result<Self, Error> {
        let property_identifier = PropertyIdentifier::new(reader)?;
        let nchars = rdu32le(reader)?;

        // Note: DictionaryEntry.Name is not a CodepageString because:
        // - size is in chars, not bytes
        // - it is not padded unless it's in CP_WINUNICODE
        let name = if codepage == CP_WINUNICODE {
            let size = nchars.checked_mul(2).ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "DictionaryEntry size overflow")
            })?;
            let s = CodepageString::new_noalign(reader, codepage, size)?;
            let padlen = (nchars & 1) * 2;
            reader.seek(SeekFrom::Current(i64::from(padlen)))?;
            s
        } else {
            CodepageString::new_noalign(reader, codepage, nchars)?
        };
        Ok(DictionaryEntry {
            property_identifier,
            name,
        })
    }
}

/// Represents all mappings between property identifiers and property names in a property set.
#[derive(Debug)]
pub struct Dictionary {
    /// Number of entries in the Dictionary
    pub num_entries: u32,
    /// List of entries
    pub entry: Vec<DictionaryEntry>,
}

impl Dictionary {
    fn new<R: Read + Seek>(reader: &mut R, codepage: CodePage) -> Result<Self, Error> {
        let start_offset = reader.stream_position()?;
        let num_entries = rdu32le(reader)?;
        let mut entry = Vec::<DictionaryEntry>::new();
        for _i in 0..num_entries {
            let dictionary_entry = DictionaryEntry::new(reader, codepage)?;
            entry.push(dictionary_entry);
        }
        let end_offset = reader.stream_position()?;
        let size = end_offset - start_offset;
        let padlen = ((size + 3) & !3) - size;
        reader.seek(SeekFrom::Current(padlen as i64))?;
        Ok(Dictionary { num_entries, entry })
    }
}

/// Represents the property identifier of a property in a property set
#[derive(PartialEq, Debug)]
pub enum PropertyIdentifier {
    /// 0x00000000: property identifier for the Dictionary property.
    Dictionary,
    /// 0x00000001: property identifier for the CodePage property.
    Codepage,
    /// 0x80000000: property identifier for the Locale property.
    Locale,
    /// 0x80000003: property identifier for the Behavior property.
    Behavior,
    /// 0x00000002 — 0x7FFFFFFF: Used to identify normal properties.
    Normal(u32),
    /// Fallback
    Invalid(u32),
}

impl PropertyIdentifier {
    fn new<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let value = rdu32le(reader)?;
        Ok(match value {
            0x00000000 => Self::Dictionary,
            0x00000001 => Self::Codepage,
            0x80000000 => Self::Locale,
            0x80000001 => Self::Behavior,
            0x00000002..=0x7FFFFFFF => Self::Normal(value),
            _ => Self::Invalid(value),
        })
    }
}

/// Represent a property identifier and the byte offset of the property in the PropertySet packet.
#[derive(Debug)]
pub struct PropertyIdentifierAndOffset {
    property_identifier: PropertyIdentifier,
    offset: u32,
}

impl PropertyIdentifierAndOffset {
    fn new<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let property_identifier = PropertyIdentifier::new(reader)?;
        let offset = rdu32le(reader)?;
        Ok(Self {
            property_identifier,
            offset,
        })
    }
}

/// A typed value associated with a property identifier and optionally a property name.
#[derive(Debug)]
pub enum Property {
    /// The TypedPropertyValue structure
    TypedPropertyValue(TypedPropertyValue),
    /// The Dictionary structure
    Dictionary(Dictionary),
    /// An improperly encoded element
    Invalid(String),
}

impl Property {
    fn as_vt_string(&self) -> Option<String> {
        match self {
            Self::TypedPropertyValue(TypedPropertyValue::LPStr(s)) => Some(s.to_string()),
            Self::TypedPropertyValue(TypedPropertyValue::LPWStr(s)) => Some(s.to_string()),
            _ => None,
        }
    }
}

/// The PropertySet packet represents a property set.
#[derive(Debug)]
pub struct PropertySet {
    /// total size in bytes of the PropertySet packet.
    pub size: u32,
    /// number of properties in the property set.
    pub num_properties: u32,
    /// array of properties identifiers and offsets
    pub property_identifier_and_offset: Vec<PropertyIdentifierAndOffset>,
    property: Vec<Property>,
    /// The code page in use for this set
    pub codepage: CodePage,
    /// Indicates that no code page was set (fallback CP1252 will be used)
    pub missing_cp: bool,
    /// Indicates multiple attempts to set the code page were encountered (and ignored)
    pub multiple_cps: bool,
}

impl PropertySet {
    fn new<R: Read + Seek>(reader: &mut R) -> Result<Self, Error> {
        let size = rdu32le(reader)?;
        let mut data = vec![0u8; into_usize(size)?];
        data[0..4].copy_from_slice(&size.to_le_bytes());
        reader.read_exact(&mut data[4..])?;
        let mut cursor = Cursor::new(data);
        cursor.seek(SeekFrom::Start(4))?;
        let num_properties = rdu32le(&mut cursor)?;

        // Read property types and offsets
        let mut property_identifier_and_offset = Vec::<PropertyIdentifierAndOffset>::new();
        for _ in 0..num_properties {
            let id_and_offset = PropertyIdentifierAndOffset::new(&mut cursor)?;
            property_identifier_and_offset.push(id_and_offset);
        }

        // Find and read the codepage for this stream or fallback to cp1252
        let mut maybe_cp: Option<CodePage> = None;
        let mut cp_iter = property_identifier_and_offset
            .iter()
            .filter(|pio| pio.property_identifier == PropertyIdentifier::Codepage);
        if let Some(cp_pio) = cp_iter.next() {
            cursor.seek(SeekFrom::Start(u64::from(cp_pio.offset)))?;
            let cp_prop = TypedPropertyValue::from_oleps_reader(&mut cursor, 0, true)?;
            if let TypedPropertyValue::I2(cp) = cp_prop {
                maybe_cp = Some(cp as u16);
            }
        }
        let multiple_cps = cp_iter.next().is_some();
        let codepage = maybe_cp.unwrap_or(1252);

        // Read the property values with the correct codepage now in place
        let mut property: Vec<Property> = Vec::with_capacity(property_identifier_and_offset.len());
        for pio in property_identifier_and_offset.iter() {
            cursor.seek(SeekFrom::Start(u64::from(pio.offset)))?;
            let p = match pio.property_identifier {
                PropertyIdentifier::Dictionary => match Dictionary::new(&mut cursor, codepage) {
                    Ok(dict) => Property::Dictionary(dict),
                    Err(e) => Property::Invalid(e.to_string()),
                },
                _ => match TypedPropertyValue::from_oleps_reader(&mut cursor, codepage, true) {
                    Ok(prop) => Property::TypedPropertyValue(prop),
                    Err(e) => Property::Invalid(e.to_string()),
                },
            };
            property.push(p);
        }

        Ok(PropertySet {
            size,
            num_properties,
            property_identifier_and_offset,
            property,
            codepage,
            missing_cp: maybe_cp.is_none(),
            multiple_cps,
        })
    }
}

/// The PropertySetStream packet specifies the stream format for simple property sets and the stream format for the CONTENTS stream in the Non-Simple Property Set Storage Format. A simple property set MUST be represented by a stream containing a PropertySetStream packet.
pub struct OlePS {
    /// MUST be set to 0xFFFE.
    pub byte_order: u16,
    /// Version number of the property set
    pub version: u16,
    /// An implementation-specific value that SHOULD be ignored
    pub system_identifier: u32,
    /// representing the associated CLSID of the property set
    pub clsid: GUID,
    /// number of property sets represented by this PropertySetStream structure
    pub num_property_sets: u32,
    /// A GUID that MUST be set to the FMTID of the property set represented by the field PropertySet 0.
    pub fmtid: Vec<GUID>,
    /// Offset in bytes from the beginning of this PropertySetStream structure to the beginning of the field PropertySet 0.
    pub offset: Vec<u32>,
    property_set: Vec<PropertySet>,
}

impl OlePS {
    /// Read PropertySetStream (OlePS) from reader
    pub fn new<R: Read + Seek>(reader: &mut R) -> Result<Self, Error> {
        let start_offset = reader.stream_position()?;
        let byte_order = rdu16le(reader)?;
        if byte_order != 0xFFFE {
            return Err(Error::new(ErrorKind::InvalidData, "Unsupported byte order"));
        }
        let version = rdu16le(reader)?;
        let system_identifier = rdu32le(reader)?;
        let clsid = reader.read_guid()?;
        let num_property_sets = rdu32le(reader)?;
        if ![1, 2].contains(&num_property_sets) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid NumPropertySets",
            ));
        }
        let mut fmtid = Vec::<GUID>::new();
        let mut offset = Vec::<u32>::new();
        let mut property_set = Vec::<PropertySet>::new();

        for _n in 0..num_property_sets {
            fmtid.push(reader.read_guid()?);
            offset.push(rdu32le(reader)?);
        }

        for pos in &offset {
            let property_set_offset = start_offset + u64::from(*pos);
            reader.seek(SeekFrom::Start(property_set_offset))?;
            let ps = PropertySet::new(reader)?;
            property_set.push(ps);
        }
        Ok(Self {
            byte_order,
            version,
            system_identifier,
            clsid,
            num_property_sets,
            fmtid,
            offset,
            property_set,
        })
    }

    /// Returns an iterator over all the properties in the stream
    pub fn properties(&self) -> impl Iterator<Item = (&PropertyIdentifier, &Property)> {
        self.property_set.iter().flat_map(|ps| {
            ps.property_identifier_and_offset
                .iter()
                .map(|pio| &pio.property_identifier)
                .zip(ps.property.iter())
        })
    }

    /// Returns an iterator over all the properties in the PropertySet 0
    pub fn properties_0(&self) -> impl Iterator<Item = (&PropertyIdentifier, &Property)> {
        self.property_set.iter().take(1).flat_map(|ps| {
            ps.property_identifier_and_offset
                .iter()
                .map(|pio| &pio.property_identifier)
                .zip(ps.property.iter())
        })
    }

    /// Returns an iterator over all the properties in the PropertySet 1
    pub fn properties_1(&self) -> impl Iterator<Item = (&PropertyIdentifier, &Property)> {
        self.property_set.iter().skip(1).take(1).flat_map(|ps| {
            ps.property_identifier_and_offset
                .iter()
                .map(|pio| &pio.property_identifier)
                .zip(ps.property.iter())
        })
    }
}

trait ReadVec {
    fn read_guid(&mut self) -> Result<GUID, Error>;
}

impl<R: Read> ReadVec for R {
    fn read_guid(&mut self) -> Result<GUID, Error> {
        GUID::from_le_stream(self)
    }
}

// From MS-OLEPS 2.21
// An implementation SHOULD enforce a limit on the total size of a PropertySetStream packet. This limit
// MUST be at least 262,144 bytes, and for maximum interoperability SHOULD<4> be 2,097,152 bytes.
const PSS_REC_MAX_LEN: u64 = 2_097_152;

/// SummaryInformation structure
#[derive(Debug, Default)]
pub struct SummaryInformation {
    /// Document title
    pub title: Option<String>,
    /// Document subject
    pub subject: Option<String>,
    /// Document author
    pub author: Option<String>,
    /// Document keywords
    pub keywords: Option<String>,
    /// Document comments
    pub comments: Option<String>,
    /// The name of the template on which the document is based
    pub template: Option<String>,
    /// The name of the author who last modified the document
    pub last_author: Option<String>,
    /// The revision number for the document
    pub revision: Option<String>,
    /// The total time that the document has been opened for edit
    pub edit_time: Option<Duration>,
    /// The date and time the document was last printed
    pub last_printed_dt: Option<OffsetDateTime>,
    /// The date and time the document was created
    pub created_dt: Option<OffsetDateTime>,
    /// The date and time the document was last saved
    pub last_saved_dt: Option<OffsetDateTime>,
    /// The page count of the document
    pub pages: Option<i32>,
    /// An estimate or an exact count of the words in the document
    pub words: Option<i32>,
    /// An estimate of the character count of the document
    pub chars: Option<i32>,
    /// Indicates whether the thumbnail image for the document is present
    pub has_thumbnail: bool,
    /// The name of the application that produced the document
    pub application_name: Option<String>,
    /// Indicates that the document must be password protected
    pub password_protected: bool,
    /// Opening the document read-only is recommended but not enforced
    pub readonly_recommend: bool,
    /// Indicates that the document is opened read-only
    pub readonly_enforced: bool,
    /// Specifies that the document is opened read-only except for annotations
    pub locked: bool,
    /// Invalid entries were encountered and ignored
    pub has_bad_entries: bool,
    /// Duplicate entries were encountered and ignored
    pub has_dups: bool,
    /// Valid entries were encountered and ignored due to improper typing
    pub has_bad_type: bool,
}

impl SummaryInformation {
    /// Parse a SummaryInformation structure from an Ole stream
    pub fn new<R: Read + Seek>(reader: &mut R) -> Result<Self, Error> {
        let pstrm = OlePS::new(&mut SeekTake::new(reader, PSS_REC_MAX_LEN))?;
        if pstrm.byte_order != 0xfffe {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid ByteOrder ({:04x})", pstrm.byte_order),
            ));
        }
        if pstrm.version != 0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid Version ({:04x})", pstrm.version),
            ));
        }
        if !pstrm.clsid.is_null() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid CLSID ({})", pstrm.clsid),
            ));
        }
        if pstrm.num_property_sets != 1 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Invalid number of property sets ({})",
                    pstrm.num_property_sets
                ),
            ));
        }
        if pstrm.fmtid[0] != GUID::from_str("F29F85E0-4FF9-1068-AB91-08002B27B3D9").unwrap() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid FMTID ({})", pstrm.clsid),
            ));
        }

        let mut ret = Self::default();
        for (identifier, property) in pstrm.properties() {
            match identifier {
                PropertyIdentifier::Dictionary => ret.has_bad_entries = true,
                PropertyIdentifier::Codepage => {}
                PropertyIdentifier::Locale => ret.has_bad_entries = true,
                PropertyIdentifier::Behavior => ret.has_bad_entries = true,
                PropertyIdentifier::Invalid(_) => ret.has_bad_entries = true,
                PropertyIdentifier::Normal(val) => match val {
                    0x00000002 /* PIDSI_TITLE */ => {
                        if ret.title.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.title = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000003 /* PIDSI_SUBJECT */ => {
                        if ret.subject.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.subject = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000004 /* PIDSI_AUTHOR */ => {
                        if ret.author.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.author = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000005 /* PIDSI_KEYWORDS */ => {
                        if ret.keywords.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.keywords = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000006 /* PIDSI_COMMENTS */ => {
                        if ret.comments.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.comments = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000007 /* PIDSI_TEMPLATE */ => {
                        if ret.template.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.template = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000008 /* PIDSI_LASTAUTHOR */ => {
                        if ret.last_author.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.last_author = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000009 /* PIDSI_REVNUMBER */ => {
                        if ret.revision.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.revision = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000000A /* PIDSI_EDITTIME */ => {
                        if ret.edit_time.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::Filetime(t)) => {
                                    ret.edit_time = t.as_duration();
                                    ret.has_bad_type |= ret.edit_time.is_none();
                                }
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000000B /* PIDSI_LASTPRINTED */ => {
                        if ret.last_printed_dt.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::Filetime(t)) => {
                                    ret.last_printed_dt = t.as_datetime();
                                    ret.has_bad_type |= ret.last_printed_dt.is_none();
                                }
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000000C /* PIDSI_CREATE_DTM */ => {
                        if ret.created_dt.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::Filetime(t)) => {
                                    ret.created_dt = t.as_datetime();
                                    ret.has_bad_type |= ret.created_dt.is_none();
                                }
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000000D /* PIDSI_LASTSAVE_DTM */ => {
                        if ret.last_saved_dt.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::Filetime(t)) => {
                                    ret.last_saved_dt = t.as_datetime();
                                    ret.has_bad_type |= ret.last_saved_dt.is_none();
                                }
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000000E /* PIDSI_PAGECOUNT */ => {
                        if ret.pages.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.pages = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000000F /* PIDSI_WORDCOUNT */ => {
                        if ret.words.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.words = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000010 /* PIDSI_CHARCOUNT */ => {
                        if ret.chars.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.chars = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000011 /* PIDSI_THUMBNAIL */ => {
                        match property {
                            Property::TypedPropertyValue(TypedPropertyValue::CF(_)) => ret.has_thumbnail = true,
                            _ => ret.has_bad_type = true,
                        }
                    }
                    0x00000012 /* PIDSI_APPNAME */ => {
                        if ret.application_name.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.application_name = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000013 /* PIDSI_DOC_SECURITY */ => {
                        match property {
                            Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => {
                                ret.password_protected |= (v & 1) != 0;
                                ret.readonly_recommend |= (v & 2) != 0;
                                ret.readonly_enforced |= (v & 4) != 0;
                                ret.locked |= (v & 8) != 0;
                            }
                            _ => ret.has_bad_type = true,
                        }
                    }
                    _ => ret.has_bad_entries = true,
                },
            }
        }
        Ok(ret)
    }
}

/// User Defined Property values
#[derive(Debug)]
pub enum UserDefinedProperty {
    /// A string
    String(String),
    /// An integer
    Int(i32),
    /// A floating point number
    Real(f64),
    /// A boolean
    Bool(bool),
    /// A date
    DateTime(OffsetDateTime),
    /// An unsupported type
    Undecoded,
}

/// DocumentSummaryInformation structure
#[derive(Debug, Default)]
pub struct DocumentSummaryInformation {
    /// Category name for the document
    pub category: Option<String>,
    /// Presentation format type of the document
    pub presentation_format: Option<String>,
    /// Estimate of the size of the document in bytes
    pub bytes: Option<i32>,
    /// Estimate of the number of text lines in the document
    pub lines: Option<i32>,
    /// Estimate or an exact count of the number of paragraphs in the document
    pub paragraphs: Option<i32>,
    /// Number of slides in the document
    pub slides: Option<i32>,
    /// Number of notes in the document
    pub notes: Option<i32>,
    /// Number of hidden slides in the document
    pub hidden_slides: Option<i32>,
    /// Number of multimedia clips in the document
    pub mmclips: Option<i32>,
    /// Scale - should be FALSE
    pub scale: Option<bool>,
    /// The manager associated with the document
    pub manager: Option<String>,
    /// The company associated with the document’s authoring
    pub company: Option<String>,
    /// Indicates if any of the values for the linked properties in the User Defined Property
    /// Set have changed outside of the application
    pub links_dirty: Option<bool>,
    /// Estimate of the number of characters in the document, including whitespace
    pub characters: Option<i32>,
    /// Indicates if the *_PID_HLINKS* property has changed outside of the application
    pub hyperlinks_changed: Option<bool>,
    /// The application version
    pub version: Option<crate::crypto::Version>,
    /// Specifies that the VBA project of the document is present and has a digital signature
    pub has_vba_signature: bool,
    /// The content type of the file
    pub content_type: Option<String>,
    /// The document status
    pub content_status: Option<String>,
    /// Language - should be absent
    pub language: Option<String>,
    /// Document version - should be absent
    pub docversion: Option<String>,
    /// Headings and parts
    pub headings_parts: Vec<(String, Vec<String>)>,
    /// User defined properties
    pub user_defined_properties: HashMap<String, UserDefinedProperty>,
    /// Invalid entries were encountered and ignored
    pub has_bad_entries: bool,
    /// Duplicate entries were encountered and ignored
    pub has_dups: bool,
    /// Valid entries were encountered and ignored due to improper typing
    pub has_bad_type: bool,
}

impl DocumentSummaryInformation {
    /// Parse a DocumentSummaryInformation structure from an Ole stream
    pub fn new<R: Read + Seek>(reader: &mut R) -> Result<Self, Error> {
        let pstrm = OlePS::new(&mut SeekTake::new(reader, PSS_REC_MAX_LEN))?;
        if pstrm.byte_order != 0xfffe {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid ByteOrder ({:04x})", pstrm.byte_order),
            ));
        }
        if pstrm.version != 0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid Version ({:04x})", pstrm.version),
            ));
        }
        if !pstrm.clsid.is_null() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid CLSID ({})", pstrm.clsid),
            ));
        }
        if pstrm.num_property_sets < 1 || pstrm.num_property_sets > 2 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Invalid number of property sets ({})",
                    pstrm.num_property_sets
                ),
            ));
        }
        if pstrm.fmtid[0] != GUID::from_str("D5CDD502-2E9C-101B-9397-08002B2CF9AE").unwrap() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid FMTID ({})", pstrm.clsid),
            ));
        }
        let mut ret = Self::default();
        let mut headings: Vec<(String, usize)> = Vec::new();
        let mut parts: Vec<String> = Vec::new();
        for (identifier, property) in pstrm.properties_0() {
            match identifier {
                PropertyIdentifier::Dictionary => ret.has_bad_entries = true,
                PropertyIdentifier::Codepage => {}
                PropertyIdentifier::Locale => ret.has_bad_entries = true,
                PropertyIdentifier::Behavior => ret.has_bad_entries = true,
                PropertyIdentifier::Invalid(_) => ret.has_bad_entries = true,
                PropertyIdentifier::Normal(val) => match val {
                    0x00000002 /* GKPIDDSI_CATEGORY */ => {
                        if ret.category.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.category = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000003 /* GKPIDDSI_PRESFORMAT */ => {
                        if ret.presentation_format.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.presentation_format = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000004 /* GKPIDDSI_BYTECOUNT */ => {
                        if ret.bytes.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.bytes = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000005 /* GKPIDDSI_LINECOUNT */ => {
                        if ret.lines.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.lines = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000006 /* GKPIDDSI_PARACOUNT */ => {
                        if ret.paragraphs.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.paragraphs = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000007 /* GKPIDDSI_SLIDECOUNT */ => {
                        if ret.slides.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.slides = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000008 /* GKPIDDSI_NOTECOUNT */ => {
                        if ret.notes.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.notes = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000009 /* GKPIDDSI_HIDDENCOUNT */ => {
                        if ret.hidden_slides.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.hidden_slides = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000000A /* GKPIDDSI_MMCLIPCOUNT */ => {
                        if ret.mmclips.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.mmclips = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000000B /* GKPIDDSI_SCALE */ => {
                        if ret.scale.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::Bool(v)) => ret.scale = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000000C /* GKPIDDSI_HEADINGPAIR */ => {
                        match property {
                            Property::TypedPropertyValue(TypedPropertyValue::VectorVariant(p)) => {
                                if headings.is_empty() {
                                    for headpair in p.data.as_slice().chunks_exact(2) {
                                        let s = match &headpair[0] {
                                            TypedPropertyValue::LPStr(s) => s.to_string(),
                                            TypedPropertyValue::LPWStr(s) => s.to_string(),
                                            _ => {
                                                ret.has_bad_type = true;
                                                continue;
                                            }
                                        };
                                        if let TypedPropertyValue::I4(i) = headpair[1] {
                                            if let Ok(i) = usize::try_from(i) {
                                                headings.push((s, i));
                                            } else {
                                                ret.has_bad_type = true;
                                            }
                                        }
                                    }
                                } else {
                                    ret.has_dups = true;
                                }
                            }
                            _ => ret.has_bad_type = true,
                        }
                    }
                    0x0000000D /* GKPIDDSI_DOCPARTS */ => {
                        if parts.is_empty() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::VectorLPSTR(s)) => {
                                    parts = s.data.iter().map(|s| s.to_string()).collect()
                                }
                                Property::TypedPropertyValue(TypedPropertyValue::VectorLPWSTR(s)) => {
                                    parts = s.data.iter().map(|s| s.to_string()).collect()
                                }
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000000E /* GKPIDDSI_MANAGER */ => {
                        if ret.manager.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.manager = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000000F /* GKPIDDSI_COMPANY */ => {
                        if ret.company.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.company = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000010 /* GKPIDDSI_LINKSDIRTY */ => {
                        if ret.links_dirty.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::Bool(v)) => ret.links_dirty = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000011 /* GKPIDDSI_CCHWITHSPACES */ => {
                        if ret.characters.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.characters = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000013 /* GKPIDDSI_SHAREDDOC */ => {}
                    0x00000014 /* GKPIDDSI_LINKBASE */ => {}
                    0x00000015 /* GKPIDDSI_HLINKS */ => {}
                    0x00000016 /* GKPIDDSI_HYPERLINKSCHANGED */ => {
                        if ret.hyperlinks_changed.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::Bool(v)) => ret.hyperlinks_changed = Some(*v),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000017 /* GKPIDDSI_VERSION */ => {
                        if ret.version.is_none() {
                            match property {
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => ret.version = Some(crate::crypto::Version {
                                    major: (*v >> 16) as u16,
                                    minor: (*v) as u16,
                                }),
                                _ => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x00000018 /* GKPIDDSI_DIGSIG */ => {
                        match property {
                            Property::TypedPropertyValue(TypedPropertyValue::Blob(_)) => ret.has_vba_signature = true,
                            _ => ret.has_bad_type = true,
                        }
                    }
                    0x0000001A /* GKPIDDSI_CONTENTTYPE */ => {
                        if ret.content_type.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.content_type = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000001B /* GKPIDDSI_CONTENTSTATUS */ => {
                        if ret.content_status.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.content_status = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000001C /* GKPIDDSI_CONTENTSTATUS */ => {
                        if ret.language.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.language = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    0x0000001D /* GKPIDDSI_DOCVERSION */ => {
                        if ret.docversion.is_none() {
                            match property.as_vt_string() {
                                Some(s) => ret.docversion = Some(s),
                                None => ret.has_bad_type = true,
                            }
                        } else {
                            ret.has_dups = true;
                        }
                    }
                    _ => ret.has_bad_entries = true,
                },
            }
        }
        if headings.iter().map(|(_, i)| *i).sum::<usize>() == parts.len() {
            for (s, i) in headings {
                let tail = parts.split_off(i);
                ret.headings_parts.push((s, parts));
                parts = tail;
            }
        } else {
            ret.has_bad_entries = true;
        }
        if pstrm.num_property_sets == 2
            && pstrm.fmtid[1] == GUID::from_str("D5CDD505-2E9C-101B-9397-08002B2CF9AE").unwrap()
        {
            let mut it = pstrm.properties_1().filter_map(|(pi, dict)| match dict {
                Property::Dictionary(dict) if matches!(pi, PropertyIdentifier::Dictionary) => {
                    Some(dict)
                }
                _ => None,
            });
            if let Some(dict) = it.next() {
                let mut idmap: HashMap<u32, String> = HashMap::with_capacity(dict.entry.len());
                for (id, name) in
                    dict.entry
                        .iter()
                        .filter_map(|entry| match entry.property_identifier {
                            PropertyIdentifier::Normal(v) => Some((v, entry.name.to_string())),
                            _ => None,
                        })
                {
                    idmap.insert(id, name);
                }
                if it.next().is_some() {
                    ret.has_dups = true;
                }
                let mut user_props: HashMap<String, UserDefinedProperty> =
                    HashMap::with_capacity(idmap.len());
                for (pi, prop) in pstrm.properties_1() {
                    if let PropertyIdentifier::Normal(v) = pi {
                        if let Some(key) = idmap.remove(v) {
                            let value = match prop {
                                Property::TypedPropertyValue(TypedPropertyValue::LPStr(s)) => {
                                    UserDefinedProperty::String(s.to_string())
                                }
                                Property::TypedPropertyValue(TypedPropertyValue::LPWStr(s)) => {
                                    UserDefinedProperty::String(s.to_string())
                                }
                                Property::TypedPropertyValue(TypedPropertyValue::I4(v)) => {
                                    UserDefinedProperty::Int(*v)
                                }
                                Property::TypedPropertyValue(TypedPropertyValue::R8(v)) => {
                                    UserDefinedProperty::Real(*v)
                                }
                                Property::TypedPropertyValue(TypedPropertyValue::Bool(v)) => {
                                    UserDefinedProperty::Bool(*v)
                                }
                                Property::TypedPropertyValue(TypedPropertyValue::Filetime(t)) => {
                                    if let Some(dt) = t.as_datetime() {
                                        UserDefinedProperty::DateTime(dt)
                                    } else {
                                        UserDefinedProperty::Undecoded
                                    }
                                }
                                Property::TypedPropertyValue(TypedPropertyValue::Blob(v))
                                    if key == "_PID_LINKBASE" =>
                                {
                                    // Exceptionally decoded (although blob) because very common
                                    let url = utf8dec_rs::decode_utf16le_str(&v.bytes)
                                        .trim_end_matches('\0')
                                        .to_string();
                                    UserDefinedProperty::String(url)
                                }
                                _ => UserDefinedProperty::Undecoded,
                            };
                            user_props.insert(key, value);
                        }
                    }
                }
                ret.user_defined_properties = user_props;
            }
        }

        Ok(ret)
    }
}
