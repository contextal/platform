use crate::{config::Config, rendered_page::RenderedPage, ChildType, PdfBackendError};
use backend_utils::objects::BackendResultChild;
use ocr_rs::TessBaseApi;
use std::fs;
use tempfile::{Builder, NamedTempFile};
use tracing::info;

/// A container to hold text produced by optical character recognition of rendered document pages.
#[derive(Debug)]
pub struct OcrText<'a> {
    /// Text from pages where OCR results were not empty.
    inner: Vec<String>,

    /// Backend config.
    config: &'a Config,
}

impl<'a> OcrText<'a> {
    /// Constructs new `OcrText` instance.
    pub fn new(config: &'a Config) -> Self {
        Self {
            inner: vec![],
            config,
        }
    }

    /// Returns true if no text has been accumulated so far.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Performs OCR of a given rendered page and accumulates the text, if any text has been
    /// recognized.
    pub fn append_from(
        &mut self,
        rendered: RenderedPage,
        tesseract: &TessBaseApi,
    ) -> Result<(), PdfBackendError> {
        tesseract
            .set_rgba_image(&rendered.inner.into_rgba8())
            .inspect_err(|_| {
                info!("failed to pass an image to Tesseract");
            })?;

        tesseract.recognize().inspect_err(|_| {
            info!("text recognition step has failed");
        })?;

        let text = tesseract.get_text().inspect_err(|_| {
            info!("failed to obtain recognized text");
        })?;

        if !text.trim().is_empty() {
            self.inner.push(text);
        }

        Ok(())
    }

    /// Attempts to save OCR-produced text accumulated from pages into a file and construct
    /// `BackendResultChild` entry.
    pub fn consume(self) -> Result<BackendResultChild, PdfBackendError> {
        let output_file = if self.config.random_filenames {
            NamedTempFile::new_in(&self.config.output_path)
        } else {
            Builder::new()
                .prefix("text_ocr_")
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
            symbols: vec!["OCR".into()],
            relation_metadata: match serde_json::to_value(ChildType::DocumentText {})? {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
            force_type: Some("Text".into()),
        })
    }
}
