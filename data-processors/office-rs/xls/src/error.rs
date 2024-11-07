use std::{io, num::TryFromIntError};

/// Wrapper around [`io::Error`]
#[derive(Debug)]
pub struct ExcelError(io::Error);

impl ExcelError {
    /// Returns true if error is related with invalid data format
    pub fn is_data_error(&self) -> bool {
        [io::ErrorKind::InvalidData, io::ErrorKind::UnexpectedEof].contains(&self.0.kind())
    }

    /// Convert to std::io::Error
    pub fn to_io_error(self) -> io::Error {
        self.0
    }
}

impl std::fmt::Display for ExcelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for ExcelError {}

impl From<io::Error> for ExcelError {
    fn from(e: io::Error) -> Self {
        Self(e)
    }
}

impl From<std::num::ParseIntError> for ExcelError {
    fn from(e: std::num::ParseIntError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e).into()
    }
}

impl From<std::num::ParseFloatError> for ExcelError {
    fn from(e: std::num::ParseFloatError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e).into()
    }
}

impl From<std::str::ParseBoolError> for ExcelError {
    fn from(e: std::str::ParseBoolError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e).into()
    }
}

impl From<&str> for ExcelError {
    fn from(s: &str) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s).into()
    }
}

impl From<String> for ExcelError {
    fn from(s: String) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s).into()
    }
}

impl From<&String> for ExcelError {
    fn from(s: &String) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s.to_string()).into()
    }
}

impl From<TryFromIntError> for ExcelError {
    fn from(e: TryFromIntError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e.to_string()).into()
    }
}
