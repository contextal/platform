use crate::{PdfBackendError, Uris};
use pdfium_render::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use tracing::warn;

/// A structure to hold total number of bookmark in a document, number of bookmarks with URI and a
/// number of errors which happened during processing of bookmarks.
#[derive(Debug, Default, Serialize)]
pub struct NumberOfBookmarks {
    /// Counter for visited bookmarks.
    total: usize,

    /// Counter for bookmarks which contain URI.
    with_uris: usize,

    /// Counter for bookmarks, where there were errors during examination.
    errors: usize,
}

/// A container to hold PDF bookmarks count, set of unique URIs extracted from bookmarks, and
/// symbols added while processing bookmarks.
#[derive(Default, Debug)]
pub struct Bookmarks {
    /// A set of unique URIs extracted from bookmarks. Usually bookmarks don't have URIs.
    pub uris: Uris,

    /// Counter for bookmarks and errors.
    pub number_of_bookmarks: NumberOfBookmarks,

    /// Symbols produced while processing PDF bookmarks.
    pub symbols: HashSet<&'static str>,

    /// Issues faced while processing PDF bookmarks.
    pub issues: HashSet<&'static str>,

    /// Maximum amount of bookmarks to visit/process.
    max_bookmarks: u32,
}

impl Bookmarks {
    /// Constructs a new `Bookmarks` entity.
    pub fn new(max_bookmarks: u32) -> Self {
        Self {
            max_bookmarks,
            ..Self::default()
        }
    }

    /// Iterates over PDF document bookmarks, counts bookmarks and extracts/accumulates URIs if
    /// available.
    pub fn process(&mut self, document: &PdfDocument<'_>) -> Vec<Result<(), PdfBackendError>> {
        let ret = document
            .bookmarks()
            .iter()
            .take(self.max_bookmarks as usize)
            .map(|bookmark| {
                self.number_of_bookmarks.total += 1;

                if let Some(PdfAction::Uri(link)) = bookmark.action() {
                    self.number_of_bookmarks.with_uris += 1;
                    match link.uri() {
                        Ok(uri) => {
                            self.uris.insert(uri);
                        }
                        Err(e) => {
                            self.number_of_bookmarks.errors += 1;
                            self.issues.insert("ERROR_READING_BOOKMARK_LINK_URI");
                            warn!("failed to read an URI from from bookmark's link: {e}");
                            Err(e)?
                        }
                    }
                }

                Ok(())
            })
            .collect();

        if self.number_of_bookmarks.total as u32 == self.max_bookmarks {
            self.symbols.insert("MAX_BOOKMARKS_REACHED");
            self.symbols.insert("LIMITS_REACHED");
        }

        ret
    }
}
