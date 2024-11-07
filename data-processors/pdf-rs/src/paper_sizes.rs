use crate::PageWithIndex;
use pdfium_render::prelude::*;
use serde::Serialize;
use std::collections::{BTreeSet, HashSet};

/// A structure to hold a set of unique page sizes.
///
/// Page width and height are measured in "points", which are device-independent unit equal to 1/72
/// of inch, or about 0.358 mm.
#[derive(Default, Debug)]
pub struct PaperSizes(BTreeSet<(PdfPoints, PdfPoints)>);

/// A structure to hold page size in millimeters and page's standard size name, if there is a
/// corresponding standard.
#[derive(Debug, Serialize, PartialEq, Eq, Hash)]
pub struct PaperSizeMillimeters {
    /// Paper width in millimeters.
    width: u32,

    /// Pager height in millimeters.
    height: u32,

    /// Optional page size standard name, like "A4", "USLetterAnsiA" and others.
    standard_name: Option<String>,
}

impl PaperSizes {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends dimensions of a provided page into a set.
    pub fn append_from(&mut self, page: &PageWithIndex) {
        let paper_size = page.paper_size();
        self.0.insert((paper_size.width(), paper_size.height()));
    }
}

impl From<PaperSizes> for Vec<PaperSizeMillimeters> {
    /// Converts set of page dimensions from `PdfPoints` to millimeters, rounding dimensions to
    /// closest integer values.
    /// Adds a standard name for page dimensions (like "A4", "USLetterAnsiA", etc), if such
    /// standard name exists.
    fn from(paper_sizes: PaperSizes) -> Self {
        paper_sizes
            .0
            .into_iter()
            .map(|(width, height)| {
                let width = width.to_mm().round() as u32;
                let height = height.to_mm().round() as u32;
                PaperSizeMillimeters {
                    width,
                    height,
                    standard_name: PdfPagePaperStandardSize::from_mm_dimensions(width, height)
                        .map(|v| format!("{v:?}")),
                }
            })
            .collect::<HashSet<PaperSizeMillimeters>>()
            .into_iter()
            .collect()
    }
}
