use crate::{ChildType, PageWithIndex, PdfBackendError, config::Config};
use backend_utils::objects::BackendResultChild;
use std::{collections::HashSet, fs};
use tempfile::{Builder, NamedTempFile};

/// A structure to hold a PDF document text.
///
/// Often PDFs contain no text objects.
#[derive(Debug)]
pub struct DocumentText<'a> {
    /// Text from non-empty pages.
    inner: Vec<String>,

    /// Issues faced while processing document text.
    pub issues: HashSet<&'static str>,

    /// Backend config.
    config: &'a Config,
}

impl<'a> DocumentText<'a> {
    /// Constructs a new `DocumentText` instance.
    pub fn new(config: &'a Config) -> Self {
        Self {
            inner: vec![],
            issues: HashSet::new(),
            config,
        }
    }

    /// Returns true if no text has been accumulated so far.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Appends text from a page.
    pub fn append_from(&mut self, page: &PageWithIndex) -> Result<(), PdfBackendError> {
        let text = page
            .inner
            .text()
            .inspect_err(|_| {
                self.issues.insert("ERROR_OBTAINING_PAGE_TEXT");
            })?
            .to_string();

        if !text.trim().is_empty() {
            self.inner.push(text)
        }

        Ok(())
    }

    /// Attempts to save text accumulated from PDF pages into file and to construct corresponding
    /// `BackendResultChild` entry.
    pub fn consume(self) -> Result<BackendResultChild, PdfBackendError> {
        let output_file = if self.config.random_filenames {
            NamedTempFile::new_in(&self.config.output_path)
        } else {
            Builder::new()
                .prefix("text_")
                .suffix(".txt")
                .tempfile_in(&self.config.output_path)
        }?;

        fs::write(&output_file, self.inner.into_iter().collect::<String>())?;

        let output_file = output_file
            .into_temp_path()
            .keep()?
            .into_os_string()
            .into_string()
            .map_err(PdfBackendError::Utf8)?;

        Ok(BackendResultChild {
            path: Some(output_file),
            symbols: vec![],
            relation_metadata: match serde_json::to_value(ChildType::DocumentText {})? {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
            force_type: Some("Text".into()),
        })
    }
}
