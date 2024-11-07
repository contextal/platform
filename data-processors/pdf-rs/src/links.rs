use crate::{PageWithIndex, Uris};
use pdfium_render::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use tracing::warn;

/// A structure to hold counts of various links in a document.
#[derive(Debug, Default, Serialize)]
pub struct NumberOfLinks {
    /// Total number of visited links.
    total: usize,

    /// Number of links with action of URI type.
    with_action_uri: usize,

    /// Number of links with action with a destination in the current document.
    with_action_local: usize,

    /// Number of links with action with a destination in another document.
    with_action_remote: usize,

    /// Number of links with action with a lanuch-an-application type.
    with_action_launch: usize,

    /// Number of links with action with a destination in an embedded file.
    with_action_embedded: usize,

    /// Number of links with action with an unsupported type.
    with_action_unsupported: usize,

    /// Counter for links, where there were errors during examination.
    errors: usize,
}

/// A structure to hold unique URIs found in links, couters of various link types and symbols
/// produced while processing document links.
#[derive(Default, Debug)]
pub struct Links {
    /// Unique set of URIs.
    pub uris: Uris,

    /// Couters of various link types and a error counter.
    pub number_of_links: NumberOfLinks,

    /// Symbols produced while processing links.
    pub symbols: HashSet<&'static str>,

    /// Issues faced while processing links.
    pub issues: HashSet<&'static str>,

    /// Maximum number of links to visit/process.
    max_links: u32,
}

impl Links {
    /// Constructs new `Links` instance.
    pub fn new(max_links: u32) -> Self {
        Self {
            max_links,
            ..Self::default()
        }
    }

    /// Counts different types of links on a given page and extracts URIs when available.
    ///
    /// Errors which might occur during extraction of URIs are logged/counted and then ignored.
    pub fn append_from(&mut self, page: &PageWithIndex) {
        let number_below_limit =
            (self.max_links as usize).saturating_sub(self.number_of_links.total);

        page.links()
            .iter()
            .take(number_below_limit)
            .for_each(|link| {
                self.number_of_links.total += 1;
                if let Some(action) = link.action() {
                    match action {
                        PdfAction::LocalDestination(_) => {
                            self.number_of_links.with_action_local += 1
                        }
                        PdfAction::RemoteDestination(_) => {
                            self.number_of_links.with_action_remote += 1
                        }
                        PdfAction::EmbeddedDestination(_) => {
                            self.number_of_links.with_action_embedded += 1
                        }
                        PdfAction::Launch(_) => self.number_of_links.with_action_launch += 1,
                        PdfAction::Uri(link) => {
                            self.number_of_links.with_action_uri += 1;
                            match link.uri() {
                                Ok(uri) => {
                                    self.uris.insert(uri);
                                }
                                Err(e) => {
                                    self.number_of_links.errors += 1;
                                    self.issues.insert("ERROR_READING_LINK_URI");
                                    warn!("failed to read an URI from from PdfActionUri: {e}");
                                }
                            }
                        }
                        PdfAction::Unsupported(_) => {
                            self.number_of_links.with_action_unsupported += 1
                        }
                    };
                }
            });

        if self.number_of_links.total == self.max_links as usize {
            self.symbols.insert("MAX_LINKS_REACHED");
            self.symbols.insert("LIMITS_REACHED");
        }
    }
}
