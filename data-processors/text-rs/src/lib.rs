use thiserror::Error;

pub mod config;

#[derive(Error, Debug)]
pub enum TextBackendError {
    /// Configuration parameter value is out of bounds.
    #[error("config parameter `{parameter}` value is out of bounds: {message}")]
    ConfigParameterValue {
        parameter: &'static str,
        message: String,
    },

    /// Wrapper for [`Figment::Error`](https://docs.rs/figment/latest/figment/struct.Error.html)
    #[error("config deserialization: {0:?}")]
    ConfigDeserialization(#[from] figment::Error),

    /// Wrapper for [`serde_json::Error`](https://docs.rs/serde_json/latest/serde_json/struct.Error.html)
    #[error("json serialization/deserialization error: {0:?}")]
    SerdeJson(#[from] serde_json::Error),

    /// Wrapper for [`std::io::Error`](https://doc.rust-lang.org/std/io/struct.Error.html)
    #[error("IO error: {0:?}")]
    IO(#[from] std::io::Error),

    /// Guesslang model not found.
    #[error("guesslang machine learning model is not found in any of {locations:?} locations")]
    GuesslangModelNotFound { locations: Vec<&'static str> },

    /// Wrapper for [`tensorflow::Status`]
    #[error("TensorFlow: {0:?}")]
    TensorFlow(#[from] tensorflow::Status),
}
