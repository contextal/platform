use crate::{PageWithIndex, PdfBackendError};
use pdfium_render::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use tracing::{info, trace};

/// A structure to hold SHA256 checksums of embedded thumbnail images of document pages.
///
/// Thumbnails are optional, and most documents contain no embedded thumbnails.
#[derive(Default, Debug)]
pub struct Thumbnails {
    /// SHA256 checksums of embedded thumbnail images.
    pub hashes: Vec<String>,

    /// Symbols added while processing embedded thumbnails.
    pub issues: HashSet<&'static str>,
}

impl Thumbnails {
    /// Constructs a new `Thumbnails` instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads a page thumbnail image, if it is available, calculates its SHA256 checksum and
    /// appends it to the list.
    pub fn append_from(&mut self, page: &PageWithIndex) -> Result<(), PdfBackendError> {
        match page.embedded_thumbnail() {
            Ok(thumbnail) => {
                let bytes = thumbnail.as_raw_bytes();
                let stride = bytes.len() / thumbnail.height() as usize;
                let format = thumbnail.format().inspect_err(|_| {
                    self.issues.insert("FAILED_TO_DETERMINE_THUMBNAIL_FORMAT");
                    info!("failed to determine format of an embedded thumbnail");
                })?;

                let bytes_per_pixel = match format {
                    PdfBitmapFormat::Gray => 1,
                    PdfBitmapFormat::BGR => 3,
                    PdfBitmapFormat::BGRx | PdfBitmapFormat::BGRA => 4,
                    #[allow(deprecated)]
                    PdfBitmapFormat::BRGx => 4,
                };
                let mut hasher = Sha256::new();

                // Each bitmap scanline occupies a `stride` bytes, but only `width * bytes_per_pixel`
                // bytes are actually represent an image pixels. The rest, if any, are padding bytes,
                // which should be ignored during hashing.
                bytes.chunks_exact(stride).for_each(|scanline| {
                    hasher.update(&scanline[..(thumbnail.width() * bytes_per_pixel) as usize])
                });
                self.hashes.push(format!("{:x}", hasher.finalize()));
            }
            Err(PdfiumError::PageMissingEmbeddedThumbnail) => {
                trace!("page has no embedded thumbnail")
            }
            Err(e) => {
                self.issues.insert("ERROR_READING_THUMBNAIL");
                Err(e)?
            }
        }

        Ok(())
    }
}
