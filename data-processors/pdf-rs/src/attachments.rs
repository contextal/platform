use crate::{config::Config, ChildType, PdfBackendError};
use backend_utils::objects::BackendResultChild;
use pdfium_render::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use tempfile::{Builder, NamedTempFile};
use tracing::warn;

/// A structure to hold a number of attachments and a number of errors which happened during
/// processing of attachment.
#[derive(Default, Debug, Serialize)]
pub struct NumberOfAttachments {
    /// Counter for visited attachments.
    total: usize,

    /// Counter for attachments, where there were processing/extracting errors.
    errors: usize,
}

/// A container to hold PDF attachment count and symbols added while accessing/processing the
/// attachments.
#[derive(Debug)]
pub struct Attachments<'a> {
    /// Counters for attachments and errors.
    pub number_of_attachments: NumberOfAttachments,

    /// Symbols produced while processing PDF attachments.
    pub symbols: HashSet<&'static str>,

    /// Backend config.
    config: &'a Config,
}

impl<'a> Attachments<'a> {
    /// Constructs a new `Attachments` entity.
    pub fn new(config: &'a Config) -> Self {
        Self {
            number_of_attachments: NumberOfAttachments::default(),
            symbols: HashSet::new(),
            config,
        }
    }

    /// Saves PDF attachments into files and constructs a `BackendResultChild` for each attachment.
    pub fn process(
        &mut self,
        document: &PdfDocument,
    ) -> Vec<Result<BackendResultChild, PdfBackendError>> {
        (0..self.config.max_attachments)
            .zip(document.attachments().iter())
            .map(|(index, attachment)| {
                self.number_of_attachments.total += 1;
                if self.number_of_attachments.total == self.config.max_attachments as usize {
                    warn!(
                        "maximum number of attachments ({}) has been reached",
                        self.config.max_attachments
                    );
                    self.symbols.insert("MAX_ATTACHMENTS_REACHED");
                    self.symbols.insert("LIMITS_REACHED");
                }

                self.save_attachment(attachment, index).map_err(|e| {
                    self.number_of_attachments.errors += 1;
                    warn!("failed to save an attachment: {e}");
                    e
                })
            })
            .collect()
    }

    /// Attempts to save a PDF attachment info a file and construct a `BackendResultChild` entry.
    fn save_attachment(
        &mut self,
        attachment: PdfAttachment<'_>,
        index: u16,
    ) -> Result<BackendResultChild, PdfBackendError> {
        let mut child_symbols = vec![];
        let path = if attachment.len() > self.config.max_attachment_size {
            warn!(
                "attachment size ({}) is larger than the limit ({})",
                attachment.len(),
                self.config.max_attachment_size
            );
            child_symbols.push("TOOBIG".into());
            self.symbols.insert("LIMITS_REACHED");

            None
        } else {
            let output_file = if self.config.random_filenames {
                NamedTempFile::new_in(&self.config.output_path)
            } else {
                Builder::new()
                    .prefix(&format!("attachment_{index:02}_"))
                    .suffix(&format!("_{}", attachment.name()))
                    .tempfile_in(&self.config.output_path)
            }?;

            attachment.save_to_file(&output_file)?;

            let output_file = output_file
                .into_temp_path()
                .keep()?
                .into_os_string()
                .into_string()
                .map_err(PdfBackendError::Utf8)?;

            Some(output_file)
        };

        Ok(BackendResultChild {
            path,
            symbols: child_symbols,
            relation_metadata: match serde_json::to_value(ChildType::Attachment {
                name: attachment.name(),
                index,
            })? {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
            force_type: None,
        })
    }
}
