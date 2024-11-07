use backend_date_time::BackendDateTime;
use pdfium_render::prelude::*;
use serde::Serialize;
use std::{collections::HashSet, ops::Deref};
use thiserror::Error;

pub mod annotations;
pub mod attachments;
pub mod backend_date_time;
pub mod bookmarks;
pub mod builtin_metadata;
pub mod config;
pub mod document_text;
pub mod fonts;
pub mod links;
pub mod objects;
pub mod ocr_text;
pub mod paper_sizes;
pub mod rendered_page;
pub mod signatures;
pub mod thumbnails;

#[derive(Error, Debug)]
pub enum PdfBackendError {
    /// OsString contains valid Unicode data.
    #[error("invalid UTF-8 byte sequence: {0:?}")]
    Utf8(std::ffi::OsString),

    /// Configuration parameter value is out of bounds.
    #[error("config parameter `{parameter}` value is out of bounds: {message}")]
    ConfigParameterValue { parameter: String, message: String },

    /// Wrapper for [`Figment::Error`](https://docs.rs/figment/latest/figment/struct.Error.html)
    #[error("config deserialization: {0}")]
    ConfigDeserialization(#[from] figment::Error),

    /// Wrapper for [`time::error::ComponentRange`](https://docs.rs/time/latest/time/error/struct.ComponentRange.html)
    #[error("time component is out of range: {0}")]
    TimeRange(#[from] time::error::ComponentRange),

    /// Wrapper for [`nom::Err<nom::error::Error<&'static str>>`](https://docs.rs/nom/latest/nom/enum.Err.html)
    #[error("unable to parse: {0}")]
    Parse(#[from] nom::Err<nom::error::Error<String>>),

    /// Wrapper for [`std::io::Error`](https://doc.rust-lang.org/std/io/struct.Error.html)
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),

    /// Wrapper for [`tempfile::PathPersistError`](https://docs.rs/tempfile/latest/tempfile/struct.PathPersistError.html)
    #[error("failed to persist a temporary file: {0}")]
    PathPersist(#[from] tempfile::PathPersistError),

    /// Wrapper for [`image::error::ImageError`](https://docs.rs/image/latest/image/error/enum.ImageError.html)
    #[error("image crate error: {0}")]
    Image(#[from] image::error::ImageError),

    /// Wrapper for [`pdfium_render::prelude::PdfiumError`](https://docs.rs/pdfium-render/latest/pdfium_render/error/enum.PdfiumError.html)
    #[error("Pdfium library: {0}")]
    Pdfium(#[from] pdfium_render::prelude::PdfiumError),

    /// Wrapper for [`serde_json::Error`](https://docs.rs/serde_json/latest/serde_json/struct.Error.html)
    #[error("json serialization/deserialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    /// Wrapper for `ocr_rs::RawApiError`
    #[error("error from OCR crate: {0}")]
    Ocr(#[from] ocr_rs::RawApiError),
}

impl PdfBackendError {
    /// Returns true is the error variant is considered transient, i.e. a error kind which could go
    /// away on retry.
    pub fn is_transient(&self) -> bool {
        match self {
            PdfBackendError::IO(_) | PdfBackendError::PathPersist(_) | PdfBackendError::Ocr(_) => {
                true
            }
            PdfBackendError::ConfigParameterValue { .. }
            | PdfBackendError::ConfigDeserialization(_)
            | PdfBackendError::Utf8(_)
            | PdfBackendError::TimeRange(_)
            | PdfBackendError::Parse(_)
            | PdfBackendError::Image(_)
            | PdfBackendError::Pdfium(_)
            | PdfBackendError::SerdeJson(_) => false,
        }
    }

    /// Returns true is the error variant is considered nontransient, i.e. a error kind which is not
    /// expected to go away on retry.
    pub fn is_nontransient(&self) -> bool {
        !self.is_transient()
    }
}

/// Types of child objects which could be produced by the backend.
#[derive(Debug, Serialize)]
pub enum ChildType {
    /// Text from a PDF document, either extracted from text objects, or obtained by performing
    /// OCR.
    /// Single document can have multiple children of this type if OCR is enforced.
    DocumentText {},

    /// Text from document annotations.
    AnnotationsText {},

    /// Rendered page.
    RenderedPage {
        /// Zero-based page index.
        page_index: u32,
    },

    /// Image object.
    Image {
        /// Zero-based page index.
        page_index: u32,

        /// Zero-based "zero-level" object index.
        /// Indexes of multiple images within the same parent Form XObject (i.e. "container"
        /// object) will be the same.
        object_index: u32,
    },

    /// PDF attachment.
    Attachment {
        /// Attachment name (often it is a file name).
        name: String,

        /// Zero-based index of attachment.
        index: u16,
    },

    /// PDF document cryptographic signature.
    Signature {
        /// Optional reason for the signing.
        reason: Option<String>,

        /// Optional date of the signing.
        signing_date: Option<BackendDateTime>,

        /// Zero-based index of the document signature.
        index: u16,
    },
}

/// Convenience structure to hold a PDF page together with its zero-based index in a document.
pub struct PageWithIndex<'a> {
    inner: PdfPage<'a>,
    index: u32,
}

impl<'a> From<(PdfPage<'a>, u32)> for PageWithIndex<'a> {
    fn from((inner, index): (PdfPage<'a>, u32)) -> Self {
        Self { inner, index }
    }
}

impl<'a> Deref for PageWithIndex<'a> {
    type Target = PdfPage<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// A container to hold a set of unique URIs.
#[derive(Default, Debug)]
pub struct Uris(HashSet<String>);

impl Uris {
    pub fn new() -> Self {
        Self(Default::default())
    }

    /// Adds an URI to a set.
    pub fn insert(&mut self, value: String) -> bool {
        self.0.insert(value)
    }

    /// Extends an URIs set with contents of another URIs set.
    pub fn extend(mut self, other: Self) -> Self {
        self.0.extend(other.0);
        self
    }
}

impl From<Uris> for Vec<String> {
    fn from(uris: Uris) -> Self {
        let mut res: Vec<_> = uris.0.into_iter().collect();
        res.sort();
        res
    }
}

/// A wrapper for a standard version of PDF document.
pub struct PdfDocumentVersionWrapper(pub PdfDocumentVersion);

impl From<PdfDocumentVersionWrapper> for String {
    fn from(val: PdfDocumentVersionWrapper) -> Self {
        match val.0 {
            PdfDocumentVersion::Unset => unreachable!(),
            PdfDocumentVersion::Pdf1_0 => "1.0".to_string(),
            PdfDocumentVersion::Pdf1_1 => "1.1".to_string(),
            PdfDocumentVersion::Pdf1_2 => "1.2".to_string(),
            PdfDocumentVersion::Pdf1_3 => "1.3".to_string(),
            PdfDocumentVersion::Pdf1_4 => "1.4".to_string(),
            PdfDocumentVersion::Pdf1_5 => "1.5".to_string(),
            PdfDocumentVersion::Pdf1_6 => "1.6".to_string(),
            PdfDocumentVersion::Pdf1_7 => "1.7".to_string(),
            PdfDocumentVersion::Pdf2_0 => "2.0".to_string(),
            PdfDocumentVersion::Other(version) => {
                format!(
                    "{major}.{minor}",
                    major = version / 10,
                    minor = version % 10
                )
            }
        }
    }
}

/// A wrapper for document form type.
pub struct PdfFormTypeWrapper(pub PdfFormType);

impl From<Option<&PdfForm<'_>>> for PdfFormTypeWrapper {
    fn from(form: Option<&PdfForm<'_>>) -> Self {
        let kind = match form {
            Some(v) => v.form_type(),
            None => PdfFormType::None,
        };
        Self(kind)
    }
}

impl From<PdfFormTypeWrapper> for String {
    fn from(val: PdfFormTypeWrapper) -> Self {
        match val.0 {
            PdfFormType::None => "None",
            PdfFormType::Acrobat => "Acrobat",
            PdfFormType::XfaFull => "XFA full",
            PdfFormType::XfaForeground => "XFA foreground",
        }
        .to_string()
    }
}
