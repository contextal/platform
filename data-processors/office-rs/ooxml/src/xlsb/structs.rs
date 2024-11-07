use crate::OoxmlError;
use ctxutils::io::rdu32le;
use std::{fmt, io::Read, vec};

pub trait FromReader {
    fn from_reader<R: Read>(reader: &mut R) -> Result<Self, OoxmlError>
    where
        Self: std::marker::Sized;
}

#[derive(Debug)]
pub struct XLNullableWideString {
    pub data: Option<String>,
}

impl FromReader for XLNullableWideString {
    fn from_reader<R: Read>(reader: &mut R) -> Result<Self, OoxmlError>
    where
        Self: std::marker::Sized,
    {
        #[allow(non_snake_case)]
        let cchCharacters = rdu32le(reader)?;
        let data = if cchCharacters == 0xFFFFFFFF {
            None
        } else {
            let size = usize::try_from(cchCharacters)?
                .checked_mul(2)
                .ok_or("Invalid cchCharacters value")?;
            let mut bytes = vec![0u8; size];
            reader.read_exact(&mut bytes)?;
            Some(utf8dec_rs::decode_utf16le_str(&bytes))
        };
        Ok(Self { data })
    }
}

#[derive(Debug)]
pub struct XLWideString {
    pub data: String,
}

impl FromReader for XLWideString {
    fn from_reader<R: Read>(reader: &mut R) -> Result<Self, OoxmlError>
    where
        Self: std::marker::Sized,
    {
        #[allow(non_snake_case)]
        let cchCharacters = rdu32le(reader)?;
        let size = usize::try_from(cchCharacters)?
            .checked_mul(2)
            .ok_or("Invalid cchCharacters value")?;
        let mut bytes = vec![0u8; size];
        reader.read_exact(&mut bytes)?;
        let data = utf8dec_rs::decode_utf16le_str(&bytes);
        Ok(Self { data })
    }
}

/// Numeric value
#[derive(Debug)]
pub struct RkNumber {
    f_x100: bool,
    num: RkNumberValue,
}

impl fmt::Display for RkNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.num {
            RkNumberValue::Int(int) => {
                if self.f_x100 {
                    let float = f64::from(int) / 100.0;
                    write!(f, "{}", float)
                } else {
                    write!(f, "{}", int)
                }
            }
            RkNumberValue::Float(mut float) => {
                if self.f_x100 {
                    float /= 100.0;
                }
                write!(f, "{}", float)
            }
        }
    }
}

impl FromReader for RkNumber {
    fn from_reader<R: Read>(reader: &mut R) -> Result<Self, OoxmlError>
    where
        Self: std::marker::Sized,
    {
        let misc = rdu32le(reader)?;
        let f_x100 = misc & 1 != 0;
        let f_int = misc & (1 << 1) != 0;
        let num = RkNumberValue::from30bits(misc >> 2, f_int);
        Ok(RkNumber { f_x100, num })
    }
}

#[derive(Debug)]
enum RkNumberValue {
    Int(i32),
    Float(f64),
}

impl RkNumberValue {
    fn from30bits(num: u32, is_integer: bool) -> Self {
        if is_integer {
            let int = if num & (1 << 29) != 0 {
                num | 0xC0000000
            } else {
                num
            } as i32;
            RkNumberValue::Int(int)
        } else {
            let num = u64::from(num) << 34;
            let bytes = num.to_le_bytes();
            let float = f64::from_le_bytes(bytes);
            RkNumberValue::Float(float)
        }
    }
}

pub(crate) struct ObjectParsedFormula {
    _cce: u32,
    _rgce: Vec<u8>,
    _cb: u32,
    _rgcb: Vec<u8>,
}

impl FromReader for ObjectParsedFormula {
    fn from_reader<R: Read>(reader: &mut R) -> Result<Self, OoxmlError>
    where
        Self: std::marker::Sized,
    {
        let cce = rdu32le(reader)?;
        let mut rgce = vec![0u8; usize::try_from(cce)?];
        reader.read_exact(&mut rgce)?;
        let cb = rdu32le(reader)?;
        let mut rgcb = vec![0u8; usize::try_from(cb)?];
        reader.read_exact(&mut rgcb)?;
        Ok(ObjectParsedFormula {
            _cb: cb,
            _cce: cce,
            _rgcb: rgcb,
            _rgce: rgce,
        })
    }
}
