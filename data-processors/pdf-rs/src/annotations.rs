use crate::{ChildType, PageWithIndex, PdfBackendError, Uris, config::Config};
use backend_utils::objects::BackendResultChild;
use pdfium_render::prelude::*;
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    ops::{Deref, DerefMut},
};
use tempfile::{Builder, NamedTempFile};
use tracing::warn;

/// A structure to hold counts of various types of annotations in a document.
#[derive(Debug, Default, Serialize)]
pub struct NumberOfAnnotations {
    /// Total number of visited annotations.
    total: usize,

    /// Number of annotations of Text type.
    text: usize,

    /// Number of annotations of Link type (not necessarily a link with URI)
    link: usize,

    /// Number of annotations of Popup type.
    popup: usize,

    /// Number of annotations of Widget type.
    widget: usize,

    /// Number of annotations of XFA Widget type.
    xfa_widget: usize,

    /// Number of other annotation types.
    other: usize,

    /// Number of annotations with unsupported type.
    unsupported: usize,

    /// Counter for annotations, where there were errors during examination.
    errors: usize,
}

/// A container for text extracted from annotations of Text type.
#[derive(Debug)]
pub struct AnnotationsText<'a> {
    /// A container for annotations text. Each entry holds text from a single annotation of "Text"
    /// type.
    inner: Vec<String>,

    /// Backend config.
    config: &'a Config,
}

impl<'a> AnnotationsText<'a> {
    /// Constructs a new `AnnotationsText` entity.
    pub fn new(config: &'a Config) -> Self {
        Self {
            config,
            inner: vec![],
        }
    }

    /// Attempts to save text extracted from annotations of Text type into a file and to construct
    /// corresponding `BackendResultChild` entry.
    pub fn consume(self) -> Result<BackendResultChild, PdfBackendError> {
        let output_file = if self.config.random_filenames {
            NamedTempFile::new_in(&self.config.output_path)
        } else {
            Builder::new()
                .prefix("text_annotations_")
                .suffix(".txt")
                .tempfile_in(&self.config.output_path)
        }?;

        fs::write(&output_file, self.inner.join("\n"))?;

        let output_file = output_file
            .into_temp_path()
            .keep()?
            .into_os_string()
            .into_string()
            .map_err(PdfBackendError::Utf8)?;

        Ok(BackendResultChild {
            path: Some(output_file),
            symbols: vec![],
            relation_metadata: match serde_json::to_value(ChildType::AnnotationsText {})? {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
            force_type: Some("Text".into()),
        })
    }
}

impl<'a> Deref for AnnotationsText<'a> {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> DerefMut for AnnotationsText<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// A structure to hold information extracted while processing PDF document annotations.
#[derive(Debug)]
pub struct Annotations<'a> {
    /// Text extracted from annotations of Text type.
    pub annotations_text: AnnotationsText<'a>,

    /// Uris extracted from annotations of Link type.
    pub uris: Uris,

    /// Counters of annotations of various types.
    pub number_of_annotations: NumberOfAnnotations,

    /// Symbols produced while processing annotations.
    pub symbols: HashSet<&'static str>,

    /// Issues faced while processing annotations.
    pub issues: HashSet<&'static str>,

    /// Maximum amount of annotations to visit/process.
    max_annotations: u32,
}

impl<'a> Annotations<'a> {
    /// Constructs a new `Annotations` instance.
    pub fn new(config: &'a Config) -> Self {
        Self {
            annotations_text: AnnotationsText::new(config),
            max_annotations: config.max_annotations,
            uris: Uris::default(),
            number_of_annotations: NumberOfAnnotations::default(),
            symbols: HashSet::new(),
            issues: HashSet::new(),
        }
    }

    /// Inspects/counts annotations on a page, accumulates URIs and text from annotations when
    /// available.
    ///
    /// Errors which might occur during extraction of URIs are logged/counted and then ignored, as
    /// in general annotations are viewed as a source of information of low value.
    pub fn append_from(&mut self, page: &PageWithIndex) {
        let number_below_limit =
            (self.max_annotations as usize).saturating_sub(self.number_of_annotations.total);
        page.annotations()
            .iter()
            .take(number_below_limit)
            .for_each(|annotation| {
                self.number_of_annotations.total += 1;
                match &annotation {
                    PdfPageAnnotation::Text(text) => {
                        self.number_of_annotations.text += 1;
                        if let Some(text) = text.contents() {
                            if !text.trim().is_empty() {
                                self.annotations_text.push(text);
                            }
                        }
                    }
                    PdfPageAnnotation::Link(link) => {
                        self.number_of_annotations.link += 1;
                        if let Ok(link) = link.link() {
                            if let Some(PdfAction::Uri(link)) = link.action() {
                                match link.uri() {
                                    Ok(uri) => {
                                        self.uris.insert(uri);
                                    }
                                    Err(e) => {
                                        self.number_of_annotations.errors += 1;
                                        self.issues.insert("ERROR_READING_ANNOTATION_LINK_URI");
                                        warn!(
                                            "failed to read an URI from \
                                            annotation's link: {e}"
                                        )
                                    }
                                }
                            }
                        } else {
                            self.number_of_annotations.errors += 1;
                            self.issues.insert("ERROR_READING_ANNOTATION_LINK");
                            warn!("annotation of link type doesn't have an associated link")
                        }
                    }
                    PdfPageAnnotation::Popup(_) => self.number_of_annotations.popup += 1,
                    PdfPageAnnotation::Widget(_) => self.number_of_annotations.widget += 1,
                    PdfPageAnnotation::XfaWidget(_) => self.number_of_annotations.xfa_widget += 1,
                    PdfPageAnnotation::Circle(_)
                    | PdfPageAnnotation::FreeText(_)
                    | PdfPageAnnotation::Highlight(_)
                    | PdfPageAnnotation::Ink(_)
                    | PdfPageAnnotation::Square(_)
                    | PdfPageAnnotation::Squiggly(_)
                    | PdfPageAnnotation::Stamp(_)
                    | PdfPageAnnotation::Strikeout(_)
                    | PdfPageAnnotation::Underline(_)
                    | PdfPageAnnotation::Redacted(_) => self.number_of_annotations.other += 1,
                    PdfPageAnnotation::Unsupported(_) => {
                        self.number_of_annotations.unsupported += 1;
                        self.issues.insert("HAS_UNSUPPORTED_ANNOTATION_TYPE");
                    }
                };
            });
        if self.max_annotations as usize == self.number_of_annotations.total {
            self.symbols.insert("MAX_ANNOTATIONS_REACHED");
            self.symbols.insert("LIMITS_REACHED");
        }
    }
}
