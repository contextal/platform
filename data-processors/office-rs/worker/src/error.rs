use ctxole::NoValidPasswordError;
use ctxutils::io::WriteLimitExceededError;
use ooxml::OoxmlError;
use std::{io, num::TryFromIntError};
use xls::ExcelError;

/// Wrapper around std::io::Error
#[derive(Debug)]
pub struct OfficeError(io::Error);

impl OfficeError {
    /// Returns true if error is related with invalid data format
    pub fn is_data_error(&self) -> bool {
        [io::ErrorKind::InvalidData, io::ErrorKind::UnexpectedEof].contains(&self.0.kind())
    }

    /// Returns true if inner error is NoPasswordError
    pub fn is_no_valid_password_error(&self) -> bool {
        self.0
            .get_ref()
            .is_some_and(|e| e.is::<NoValidPasswordError>())
    }

    /// Returns algorithm if inner error is NoPasswordError
    pub fn get_no_valid_password_error_algorithm(&self) -> Option<String> {
        Some(
            self.0
                .get_ref()?
                .downcast_ref::<NoValidPasswordError>()?
                .algorithm(),
        )
    }

    /// Returns true if error was raised by Limited Writer
    pub fn is_write_limit_error(&self) -> bool {
        self.0
            .get_ref()
            .is_some_and(|e| e.is::<WriteLimitExceededError>())
    }
}

impl std::fmt::Display for OfficeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for OfficeError {}

impl From<io::Error> for OfficeError {
    fn from(e: io::Error) -> Self {
        Self(e)
    }
}

impl From<&str> for OfficeError {
    fn from(s: &str) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s).into()
    }
}

impl From<String> for OfficeError {
    fn from(s: String) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s).into()
    }
}

impl From<&String> for OfficeError {
    fn from(s: &String) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, s.to_string()).into()
    }
}

impl From<TryFromIntError> for OfficeError {
    fn from(value: TryFromIntError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, value.to_string()).into()
    }
}

impl From<OoxmlError> for OfficeError {
    fn from(value: OoxmlError) -> Self {
        value.to_io_error().into()
    }
}

impl From<ExcelError> for OfficeError {
    fn from(value: ExcelError) -> Self {
        value.to_io_error().into()
    }
}
