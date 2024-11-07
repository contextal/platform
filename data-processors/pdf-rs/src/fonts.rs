use crate::PageWithIndex;
use std::collections::HashSet;

/// A structure to hold unique font names appearing in a PDF document.
#[derive(Default, Debug)]
pub struct Fonts {
    /// Unique font names.
    pub names: HashSet<String>,

    /// A place for symbols added while processing fonts.
    pub symbols: HashSet<&'static str>,

    /// Maximum number of fonts to process on a single page.
    max_fonts_per_page: u16,
}

impl Fonts {
    /// Construct new `Fonts` entity.
    pub fn new(max_fonts_per_page: u16) -> Self {
        Self {
            max_fonts_per_page,
            ..Self::default()
        }
    }

    /// Append font names used on a page. The maximum amount of fonts to process is limited by
    /// `max_fonts_per_page` configuration parameter.
    pub fn append_from(&mut self, page: &PageWithIndex) {
        let count = page
            .fonts()
            .iter()
            .take(self.max_fonts_per_page as usize)
            .map(|font| self.names.insert(font.family()))
            .count();

        if count == self.max_fonts_per_page as usize {
            self.symbols.insert("MAX_FONTS_PER_PAGE_REACHED");
            self.symbols.insert("LIMITS_REACHED");
        }
    }
}
