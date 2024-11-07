use std::{io, num::TryFromIntError};

/// Wrapper around std::io::Error
#[derive(Debug)]
pub struct OoxmlError(io::Error);

impl OoxmlError {
    /// Returns true if error is related with invalid data format
    pub fn is_data_error(&self) -> bool {
        [io::ErrorKind::InvalidData, io::ErrorKind::UnexpectedEof].contains(&self.0.kind())
    }

    /// Convert to std::io::Error
    pub fn to_io_error(self) -> io::Error {
        self.0
    }
}

impl std::fmt::Display for OoxmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for OoxmlError {}

impl From<io::Error> for OoxmlError {
    fn from(e: io::Error) -> Self {
        Self(e)
    }
}

impl From<quick_xml::Error> for OoxmlError {
    fn from(e: quick_xml::Error) -> Self {
        if let quick_xml::Error::Io(ioerr) = e {
            io::Error::new(ioerr.kind(), ioerr.clone()).into()
        } else {
            io::Error::new(io::ErrorKind::InvalidData, e).into()
        }
    }
}

impl From<quick_xml::DeError> for OoxmlError {
    fn from(e: quick_xml::DeError) -> Self {
        if let quick_xml::DeError::InvalidXml(quick_xml::Error::Io(ioerr)) = e {
            io::Error::new(ioerr.kind(), ioerr).into()
        } else {
            io::Error::new(io::ErrorKind::InvalidData, e).into()
        }
    }
}

impl From<std::num::ParseIntError> for OoxmlError {
    fn from(e: std::num::ParseIntError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e).into()
    }
}

impl From<std::num::ParseFloatError> for OoxmlError {
    fn from(e: std::num::ParseFloatError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e).into()
    }
}

impl From<std::str::ParseBoolError> for OoxmlError {
    fn from(e: std::str::ParseBoolError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e).into()
    }
}

impl From<time::error::Parse> for OoxmlError {
    fn from(e: time::error::Parse) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e).into()
    }
}

impl From<&str> for OoxmlError {
    fn from(s: &str) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s).into()
    }
}

impl From<String> for OoxmlError {
    fn from(s: String) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s).into()
    }
}

impl From<&String> for OoxmlError {
    fn from(s: &String) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s.to_string()).into()
    }
}

impl From<TryFromIntError> for OoxmlError {
    fn from(value: TryFromIntError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, value).into()
    }
}
