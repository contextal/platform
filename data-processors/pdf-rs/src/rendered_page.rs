use crate::{config::Config, ChildType, PageWithIndex, PdfBackendError};
use backend_utils::objects::BackendResultChild;
use image::DynamicImage;
use pdfium_render::prelude::*;
use tempfile::{Builder, NamedTempFile};
use tracing::warn;

/// A container for a rendered PDF page.
#[derive(Debug)]
pub struct RenderedPage<'a> {
    /// A rendered PDF page.
    pub inner: DynamicImage,

    /// Zero-based PDF page index.
    pub index: u32,

    /// Backend config.
    config: &'a Config,
}

impl<'a> RenderedPage<'a> {
    /// Attempts to save a rendered PDF page as an image file and to construct a corresponding
    /// `BackendResultChild` entry.
    pub fn save(&self) -> Result<BackendResultChild, PdfBackendError> {
        let output_file = if self.config.random_filenames {
            NamedTempFile::new_in(&self.config.output_path)
        } else {
            Builder::new()
                .prefix(&format!("render_{:03}_", self.index))
                .suffix(&format!(
                    ".{}",
                    self.config.output_image_format.extensions_str()[0]
                ))
                .tempfile_in(&self.config.output_path)
        }?;

        self.inner
            .save_with_format(&output_file, *self.config.output_image_format)?;

        let output_file = output_file
            .into_temp_path()
            .keep()?
            .into_os_string()
            .into_string()
            .map_err(PdfBackendError::Utf8)?;

        Ok(BackendResultChild {
            path: Some(output_file),
            symbols: vec![],
            relation_metadata: match serde_json::to_value(ChildType::RenderedPage {
                page_index: self.index,
            })? {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
            force_type: None,
        })
    }
}

impl<'a> TryFrom<(&PageWithIndex<'_>, &PdfRenderConfig, &'a Config)> for RenderedPage<'a> {
    type Error = PdfBackendError;

    /// Produces a rendered PDF page image from a given page, render config and backend config.
    fn try_from(
        (page, render_config, config): (&PageWithIndex<'_>, &PdfRenderConfig, &'a Config),
    ) -> Result<Self, Self::Error> {
        let rendered = page
            .render_with_config(render_config)
            .map_err(|e| {
                warn!("failed to render a page with index {}: {e}", page.index);
                e
            })?
            .as_image();

        Ok(Self {
            inner: rendered,
            index: page.index,
            config,
        })
    }
}
