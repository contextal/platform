use crate::{ChildType, PageWithIndex, PdfBackendError, config::Config};
use backend_utils::objects::BackendResultChild;
use pdfium_render::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use tempfile::{Builder, NamedTempFile};
use tracing::{info, trace, warn};

/// A structure to hold counts of various types of visited/processed PDF objects, and a counter for
/// objects with errors.
#[derive(Debug, Default, Serialize)]
pub struct NumberOfObjects {
    /// Total number of visited/processed objects in a document.
    pub total: usize,

    /// Number of text objects in a document.
    texts: usize,

    /// Number of image objects in a document.
    images: usize,

    /// Number of unsupported objects in a document.
    unsupported: usize,

    /// Number of vector path (lines and shapes) objects in the document.
    vector_paths: usize,

    /// Number of shading objects in the document (geometric shape whose color is an arbitrary
    /// function of position within the shape).
    shadings: usize,

    /// Number of Form XObjects (objects of "container" type) in the document.
    form_xobjects: usize,

    /// Counter for objects, where there were errors during processing/extracting.
    errors: usize,
}

/// A container to hold PDF objects counts and symbols added while accessing/processing PDF
/// objects.
#[derive(Debug)]
pub struct Objects<'a> {
    /// Counters for objects of various types and counter for errors.
    pub number_of_objects: NumberOfObjects,

    /// Symbols produced while processing document objects.
    pub symbols: HashSet<&'static str>,

    /// Issues faced while processing document objects.
    pub issues: HashSet<&'static str>,

    /// Backend config.
    config: &'a Config,
}

impl<'a> Objects<'a> {
    /// Constructs a new `Objects` instance.
    pub fn new(config: &'a Config) -> Self {
        Self {
            number_of_objects: NumberOfObjects::default(),
            symbols: HashSet::new(),
            issues: HashSet::new(),
            config,
        }
    }

    /// Processes objects from PDF page, including nested ones in case of objects of a "container"
    /// type.
    pub fn append_from(
        &mut self,
        page: &PageWithIndex,
        document: &PdfDocument,
        save_images: bool,
    ) -> Vec<Result<Option<BackendResultChild>, PdfBackendError>> {
        let res: Vec<_> = page
            .objects()
            .iter()
            .zip(0..)
            .take(
                // With this `take` we still could get more objects than we need as Form XObjects
                // are containers, which could have more than a single object inside.
                // So there is one more check for the same limit downstream.
                //
                // It would be nice to implement a (self-referencing?) iterator over
                // `PdfPageObjectsIterator` which unpacks `XObjectForm` as it goes, but this is not
                // trivial.
                self.config
                    .max_objects
                    .saturating_sub(self.number_of_objects.total as u32) as usize,
            )
            .flat_map(|(object, object_index)| {
                self.append_with_max_depth(
                    object,
                    object_index,
                    page.index,
                    document,
                    self.config.max_object_depth,
                    save_images,
                )
            })
            .collect();

        if self.number_of_objects.total == self.config.max_objects as usize
            && self.symbols.insert("MAX_OBJECTS_REACHED")
            && self.symbols.insert("LIMITS_REACHED")
        {
            warn!(
                "max objects number ({}) has been reached",
                self.config.max_objects
            );
        }

        res
    }

    /// Recursively processes a PDF object while going no more than `max_depth` levels deep in case
    /// of "container" type objects.
    ///
    /// As object of container type could hold more than a single objects, so the return type is a
    /// vector of results.
    ///
    /// The depth of Form XObject traversal/recursion is limited by `max_object_depth` config
    /// parameter.
    fn append_with_max_depth(
        &mut self,
        object: PdfPageObject<'_>,
        object_index: u32,
        page_index: u32,
        document: &PdfDocument,
        max_depth: u8,
        save_images: bool,
    ) -> Vec<Result<Option<BackendResultChild>, PdfBackendError>> {
        if self.number_of_objects.total == self.config.max_objects as usize {
            return vec![Ok(None)];
        }
        self.number_of_objects.total += 1;

        match &object {
            PdfPageObject::Text(_) => self.number_of_objects.texts += 1,
            PdfPageObject::Image(image_object) => {
                self.number_of_objects.images += 1;
                if save_images {
                    return vec![
                        self.save_image_object(document, image_object, page_index, object_index)
                            .map_err(|e| {
                                self.number_of_objects.errors += 1;
                                warn!("failed to save an image object: {e}");
                                e
                            }),
                    ];
                }
            }
            PdfPageObject::XObjectForm(xobjectform) => {
                self.number_of_objects.form_xobjects += 1;
                if max_depth > 0 {
                    return xobjectform
                        .iter()
                        .flat_map(|object| {
                            self.append_with_max_depth(
                                object,
                                object_index,
                                page_index,
                                document,
                                max_depth - 1,
                                save_images,
                            )
                        })
                        .collect();
                } else {
                    warn!(
                        "Maximum object depth ({}) has reached",
                        self.config.max_object_depth
                    );
                    self.symbols.insert("MAX_OBJECT_DEPTH_REACHED");
                    self.symbols.insert("LIMITS_REACHED");
                }
            }
            PdfPageObject::Shading(_) => {
                self.number_of_objects.shadings += 1;
                trace!("skipping PdfPageObject::Shading")
            }
            PdfPageObject::Path(_) => {
                self.number_of_objects.vector_paths += 1;
                trace!("skipping PdfPageObject::Path")
            }
            PdfPageObject::Unsupported(_) => {
                self.number_of_objects.unsupported += 1;
                self.issues.insert("HAS_UNSUPPORTED_OBJECT");
                info!("skipping PdfPageObject::Unsupported")
            }
        }

        vec![Ok(None)]
    }

    /// Attempts to save a PDF object of Image type info a file and construct corresponding
    /// `BackendResultChild` entry.
    fn save_image_object(
        &mut self,
        document: &PdfDocument,
        image_object: &PdfPageImageObject<'_>,
        page_index: u32,
        object_index: u32,
    ) -> Result<Option<BackendResultChild>, PdfBackendError> {
        let mut child_symbols = vec![];
        let (image, name_suffix) = match image_object.get_processed_image(document) {
            Ok(v) => (v, "processed"),
            Err(PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::Unknown)) => {
                info!("libpdfium failed to provide a processed image - falling back to raw");
                child_symbols.push("FALLBACK_TO_RAW_IMAGE".to_string());
                match image_object.get_raw_image() {
                    Ok(v) => (v, "fallback_to_raw"),
                    Err(e) => {
                        self.issues.insert("FAILED_TO_EXTRACT_IMAGE_OBJECT");
                        warn!("failed to extract a raw image");
                        return Err(e.into());
                    }
                }
            }
            Err(e) => {
                self.issues.insert("FAILED_TO_EXTRACT_IMAGE_OBJECT");
                warn!("failed to extract a processed image");
                return Err(e.into());
            }
        };

        let output_file = if self.config.random_filenames {
            NamedTempFile::new_in(&self.config.output_path)
        } else {
            Builder::new()
                .prefix(&format!(
                    "image_{name_suffix}_{page_index:03}_{object_index:02}_"
                ))
                .suffix(&format!(
                    ".{}",
                    self.config.output_image_format.extensions_str()[0]
                ))
                .tempfile_in(&self.config.output_path)
        }?;

        image.save_with_format(&output_file, *self.config.output_image_format)?;

        let output_file = output_file
            .into_temp_path()
            .keep()?
            .into_os_string()
            .into_string()
            .map_err(PdfBackendError::Utf8)?;

        Ok(Some(BackendResultChild {
            path: Some(output_file),
            symbols: child_symbols,
            relation_metadata: match serde_json::to_value(ChildType::Image {
                page_index,
                object_index,
            })? {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
            force_type: None,
        }))
    }

    /// Processes objects from PDF page, including nested ones in case of objects of a "container"
    /// type.
    pub fn count_images(&self, page: &PageWithIndex) -> usize {
        let mut res = 0;
        let iter = page.objects().iter().take(
            self.config
                .max_objects
                .saturating_sub(self.number_of_objects.total as u32) as usize,
        );
        for object in iter {
            count_images_inner(object, self.config.max_object_depth, &mut res);
        }
        res
    }
}

fn count_images_inner(object: PdfPageObject<'_>, max_depth: u8, res: &mut usize) {
    match &object {
        PdfPageObject::Image(_) => {
            *res += 1;
        }
        PdfPageObject::XObjectForm(xobjectform) => {
            if max_depth > 0 {
                for object in xobjectform.iter() {
                    count_images_inner(object, max_depth - 1, res);
                }
            }
        }
        _ => {}
    }
}
