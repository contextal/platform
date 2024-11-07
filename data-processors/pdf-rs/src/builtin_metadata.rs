use std::collections::HashSet;

use crate::backend_date_time::BackendDateTime;
use pdfium_render::prelude::*;
use serde::Serialize;

/// A structure to hold standard PDF metadata fields.
#[derive(Debug, Default, Serialize)]
pub struct BuiltinMetadata {
    /// PDF document's title.
    title: Option<String>,

    /// The name of the person who created the document.
    author: Option<String>,

    /// The subject of the document.
    subject: Option<String>,

    /// Keywords associated with the document.
    keywords: Option<String>,

    /// If the document was converted to PDF from another format, the name of the application that
    /// created the original document from which it was converted.
    creator: Option<String>,

    /// If the document was converted to PDF from another format, the name of the application that
    /// converted it to PDF.
    producer: Option<String>,

    /// The date and time the document was created.
    pub creation_date: Option<BackendDateTime>,

    /// The date and time the document was most recently modified.
    pub modification_date: Option<BackendDateTime>,
}

/// A container to hold `BuiltinMetadata` and related issues.
pub struct BuiltinMetadataContainer {
    /// A structure which represents PDF metadata (structure for serialization).
    pub builtin_metadata: BuiltinMetadata,

    /// Issues faced while processing PDF metadata.
    pub issues: HashSet<&'static str>,
}

impl From<&PdfMetadata<'_>> for BuiltinMetadataContainer {
    fn from(pdf_metadata: &PdfMetadata) -> Self {
        let mut builtin_metadata = BuiltinMetadata::default();
        pdf_metadata.iter().for_each(|tag| match tag.tag_type() {
            PdfDocumentMetadataTagType::Title => {
                builtin_metadata.title = Some(tag.value().to_string())
            }
            PdfDocumentMetadataTagType::Author => {
                builtin_metadata.author = Some(tag.value().to_string())
            }
            PdfDocumentMetadataTagType::Subject => {
                builtin_metadata.subject = Some(tag.value().to_string())
            }
            PdfDocumentMetadataTagType::Keywords => {
                builtin_metadata.keywords = Some(tag.value().to_string())
            }
            PdfDocumentMetadataTagType::Creator => {
                builtin_metadata.creator = Some(tag.value().to_string())
            }
            PdfDocumentMetadataTagType::Producer => {
                builtin_metadata.producer = Some(tag.value().to_string())
            }
            PdfDocumentMetadataTagType::CreationDate => {
                builtin_metadata.creation_date = Some(tag.value().into())
            }
            PdfDocumentMetadataTagType::ModificationDate => {
                builtin_metadata.modification_date = Some(tag.value().into())
            }
        });

        let mut issues = HashSet::new();
        if let Some(date) = &builtin_metadata.creation_date {
            if date.parsed.is_none() {
                issues.insert("INVALID_CREATION_DATE");
            }
        }
        if let Some(date) = &builtin_metadata.modification_date {
            if date.parsed.is_none() {
                issues.insert("INVALID_MODIFICATION_DATE");
            }
        }

        Self {
            builtin_metadata,
            issues,
        }
    }
}
