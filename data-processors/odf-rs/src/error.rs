use std::{io, num::TryFromIntError};

use ctxutils::io::WriteLimitExceededError;

/// Wrapper around std::io::Error
#[derive(Debug)]
pub struct OdfError(io::Error);

impl OdfError {
    /// Returns true if error is related with invalid data format
    pub fn is_data_error(&self) -> bool {
        [io::ErrorKind::InvalidData, io::ErrorKind::UnexpectedEof].contains(&self.0.kind())
    }

    /// Convert to std::io::Error
    #[allow(dead_code)]
    pub fn into_inner(self) -> io::Error {
        self.0
    }

    /// Returns true if error was raised by Limited Writer
    pub fn is_write_limit_error(&self) -> bool {
        self.0
            .get_ref()
            .is_some_and(|e| e.is::<WriteLimitExceededError>())
    }
}

impl std::fmt::Display for OdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for OdfError {}

impl From<io::Error> for OdfError {
    fn from(e: io::Error) -> Self {
        Self(e)
    }
}

impl From<quick_xml::Error> for OdfError {
    fn from(e: quick_xml::Error) -> Self {
        if let quick_xml::Error::Io(ioerr) = e {
            io::Error::new(ioerr.kind(), ioerr.clone()).into()
        } else {
            io::Error::new(io::ErrorKind::InvalidData, e).into()
        }
    }
}

impl From<quick_xml::DeError> for OdfError {
    fn from(e: quick_xml::DeError) -> Self {
        if let quick_xml::DeError::InvalidXml(quick_xml::Error::Io(ioerr)) = e {
            io::Error::new(ioerr.kind(), ioerr).into()
        } else {
            io::Error::new(io::ErrorKind::InvalidData, e).into()
        }
    }
}

impl From<std::num::ParseIntError> for OdfError {
    fn from(e: std::num::ParseIntError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e).into()
    }
}

impl From<std::num::ParseFloatError> for OdfError {
    fn from(e: std::num::ParseFloatError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e).into()
    }
}

impl From<std::str::ParseBoolError> for OdfError {
    fn from(e: std::str::ParseBoolError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e).into()
    }
}

// impl From<time::error::Parse> for OoxmlError {
//     fn from(e: time::error::Parse) -> Self {
//         io::Error::new(io::ErrorKind::InvalidData, e).into()
//     }
// }

impl From<&str> for OdfError {
    fn from(s: &str) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s).into()
    }
}

impl From<String> for OdfError {
    fn from(s: String) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s).into()
    }
}

impl From<&String> for OdfError {
    fn from(s: &String) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s.to_string()).into()
    }
}

impl From<TryFromIntError> for OdfError {
    fn from(value: TryFromIntError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, value).into()
    }
}

impl From<quick_xml::events::attributes::AttrError> for OdfError {
    fn from(value: quick_xml::events::attributes::AttrError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, value).into()
    }
}
