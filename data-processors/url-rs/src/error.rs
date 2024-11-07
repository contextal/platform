use thiserror::Error;

#[derive(Error, Debug)]
pub enum UrlBackendError {
    /// Configuration parameter value is out of bounds.
    #[error("config parameter {parameter:?} value is out of bounds: {message}")]
    ConfigParameterValue {
        parameter: &'static str,
        message: String,
    },

    /// Wrapper for [`Figment::Error`](https://docs.rs/figment/latest/figment/struct.Error.html)
    #[error("config deserialization: {0:?}")]
    ConfigDeserialization(#[from] figment::Error),

    /// Wrapper for [`serde_json::Error`](https://docs.rs/serde_json/latest/serde_json/struct.Error.html)
    #[error("json serialization/deserialization: {0:?}")]
    SerdeJson(#[from] serde_json::Error),

    /// Wrapper for [`std::io::Error`](https://doc.rust-lang.org/std/io/struct.Error.html)
    #[error("IO: {0:?}")]
    IO(#[from] std::io::Error),

    /// Wrapper for
    /// [`tokio::task::JoinError`](https://docs.rs/tokio/latest/tokio/task/struct.JoinError.html)
    #[error("task failed to execute to completion: {0}")]
    JoinError(#[from] tokio::task::JoinError),

    /// Wrapper for
    /// [`tempfile::PathPersistError`](https://docs.rs/tempfile/latest/tempfile/struct.PathPersistError.html)
    #[error("failed to persist a temporary file: {0}")]
    PathPersist(#[from] tempfile::PathPersistError),

    /// OsString contains valid Unicode data.
    #[error("invalid UTF-8 byte sequence: {0:?}")]
    Utf8(std::ffi::OsString),

    /// Failed to construct browser config with a builder.
    #[error("failed to construct borwser config: {0}")]
    BrowserConfigBuilder(String),

    /// Wrapper for
    /// [`chromiumoxide::error::CdpError`](https://docs.rs/chromiumoxide/latest/chromiumoxide/error/enum.CdpError.html)
    #[error("Chrome DevTools Protocol: {0}")]
    Cdp(#[from] chromiumoxide::error::CdpError),

    /// No URL has been provided.
    #[error("no URL has been provided")]
    NoUrl,

    /// More than one URL has been provided.
    #[error("more than one URL has been provided")]
    MoreThanOneUrl,

    /// Failed to get page HTML contents.
    #[error("failed to get page HTML contents: {0:?}")]
    HtmlContents(chromiumoxide::error::CdpError),

    /// Failed to capture a screenshot.
    #[error("failed to capture a screenshot: {0:?}")]
    Screenshot(chromiumoxide::error::CdpError),

    /// Failed to print to PDF.
    #[error("failed to print to PDF: {0:?}")]
    PrintToPdf(chromiumoxide::error::CdpError),

    /// Backend request timeout.
    #[error("backend request timeout")]
    BackendRequestTimeout,

    /// Browser page handler is not available. This could happen if page has been closed already.
    #[error("page handler is not available (page has been closed?)")]
    PageHandlerIsNotAvailable,
}
