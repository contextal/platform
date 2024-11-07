//! Record conversion structs and traits
use crate::{ExcelError, RecordStream, RecordType};
use std::io::{Read, Seek};

/// Stores list of detected problems during parsing
pub type Anomalies = Vec<String>;

pub(crate) trait FromRecordStream {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized;
}

/// Helper trait used to identify Records
pub trait Record: std::fmt::Debug {
    /// Returns RecordType
    fn record_type() -> RecordType
    where
        Self: Sized;
}

impl FromRecordStream for u8 {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        let value = stream.rdu8()?;
        Ok(value)
    }
}

impl FromRecordStream for u16 {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        let value = stream.rdu16()?;
        Ok(value)
    }
}

impl FromRecordStream for u32 {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        let value = stream.rdu32()?;
        Ok(value)
    }
}

impl<T: FromRecordStream + Sized + Default + Copy, const SIZE: usize> FromRecordStream
    for [T; SIZE]
{
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        let mut result = [T::default(); SIZE];
        for item in &mut result {
            *item = T::from_record(stream, anomalies)?;
        }
        Ok(result)
    }
}

// pub(crate) trait FromRecordVec<Number> {
//     fn from_record<R: Read + Seek>(
//         stream: &mut RecordStream<R>,
//         anomalies: &mut Anomalies,
//         argument: &Number,
//     ) -> Result<Self, ExcelError>
//     where
//         Self: Sized,
//         usize: TryFrom<Number>;
// }

// impl<T: FromRecordStream, Number: Copy + std::fmt::Display> FromRecordVec<Number> for Vec<T> {
//     fn from_record<R: Read + Seek>(
//         stream: &mut RecordStream<R>,
//         anomalies: &mut Anomalies,
//         argument: &Number,
//     ) -> Result<Self, ExcelError>
//     where
//         Self: Sized,
//         usize: TryFrom<Number>,
//     {
//         let mut result = Vec::<T>::new();
//         let size = usize::try_from(*argument).map_err(|_| {
//             std::io::Error::new(
//                 std::io::ErrorKind::InvalidInput,
//                 format!("Unable to cast {} to vector size", *argument),
//             )
//         })?;
//         result.reserve(size);
//         for _ in 0..size {
//             result.push(T::from_record(stream, anomalies)?);
//         }
//         Ok(result)
//     }
// }

impl FromRecordStream for f64 {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        let mut data = [0u8; 8];
        stream.read_exact(&mut data)?;
        Ok(f64::from_le_bytes(data))
    }
}
