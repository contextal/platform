use super::Frame;
use crate::Ole;
use crate::forms::controls::{
    child::{Tab, TabStrip},
    *,
};
use crate::forms::mask::*;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::{self, Read, Seek};

/// A MultiPage control
///
/// A `MultiPage` is a special kind of [`Frame`] that allows different sets of overlapping
/// children (i.e. `Page`s) which can be alternatively brought into view via a dedicated
/// [`TabStrip`]
///
/// Note: MultiPage is effectively a Frame with a rather complicated extra set of features
/// including an extra stream (the *"x" stream*), a rather strict children composition, etc.
///
/// This parser will formally verify the extra requisites and will report child pages as
/// [`Control::Page`] if everything is alright; failing that, children enumeration will
/// fall back to that of the [`Frame`] element
///
/// `Frame`-like enumeration is also always available via the [`frame`](Self::frame) field
pub struct MultiPage<'a, R: Read + Seek> {
    /// Info on the `Frame`-like component of the MultiPage control
    pub frame: Frame<'a, R>,
    /// Control version
    pub version: u16,
    /// Control ID
    pub id: i32,
    /// The control can receive the focus and respond to user-generated events
    pub enabled: bool,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,

    tabs_and_pages: Vec<(Tab, PageInfo)>,
}

impl<'a, R: Read + Seek> fmt::Debug for MultiPage<'a, R> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("MultiPage")
            .field("frame", &self.frame)
            .field("version", &self.version)
            .field("id", &self.id)
            .field("enabled", &self.enabled)
            .field("anomalies", &self.anomalies)
            .field("tabs_and_pages", &self.tabs_and_pages)
            .finish()
    }
}

impl<'a, R: 'a + Read + Seek> MultiPage<'a, R> {
    pub(crate) fn new(
        ole: &'a Ole<R>,
        control: &OleSiteConcreteControl,
        storage_name: &str,
    ) -> Result<Self, io::Error> {
        let mut ret = Self {
            frame: Frame::new(ole, control, storage_name)?,
            version: 0,
            id: 0,
            enabled: true,
            tabs_and_pages: Vec::new(),
            anomalies: Vec::new(),
        };
        if let Err(e) = ret.parse_pages() {
            ret.anomalies
                .push(format!("Error processing the pages info: {}", e));
        }
        Ok(ret)
    }

    fn parse_pages(&mut self) -> Result<(), io::Error> {
        let pi = &self.frame.pi;
        let stream_name = format!("{}/x", pi.path);
        let mut x_stream = pi
            .ole
            .get_stream_reader(&pi.ole.get_entry_by_name(&stream_name)?);

        // The following checks ensure that:
        // 1. sites starts with a tab
        // 2. there exists at least one page
        // 3. the remaining sites are all pages
        // 4. the number of tabs equal the number or pages
        // 5. the sites pages id are unique

        let ts: TabStrip = if let Some(Ok(Control::TabStrip(t))) = pi.get_child(0) {
            // [1]
            t
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "First child is not a TabStrip".to_string(),
            ));
        };
        if ts.tabs.is_empty() {
            // [2]
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Insufficient children".to_string(),
            ));
        }

        let mut site_ids: HashSet<i32> = pi
            .sites
            .iter()
            .filter(|s| !s.streamed && s.class_index == 7)
            .map(|s| s.id)
            .collect();
        if pi.sites.len() != site_ids.len() + 1 // [3,5]
            || site_ids.len() != ts.tabs.len()
        // [4]
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Children count mismatch".to_string(),
            ));
        }

        let npages = ts.tabs.len();
        let mut pages = Vec::<PageInfo>::with_capacity(npages);
        for i in 0..(npages + 1) {
            if i == 0 {
                // First page is bogus
                let sz: i64 = (rdu32le(&mut x_stream)? >> 16).into();
                x_stream.seek(io::SeekFrom::Current(sz))?;
            } else {
                pages.push(PageInfo::new(&mut x_stream, i)?);
            }
        }

        self.version = rdu16le(&mut x_stream)?;
        if self.version != 0x0200 {
            self.anomalies.push(format!(
                "Invalid MultiPageProperties version {} (should be {})",
                self.version, 0x200
            ));
        }

        let mut pagecount = -1i32;
        let mask: PropertyMask = [
            Unused,
            property_mask_bit!(pagecount),
            property_mask_bit!(self.id),
            property_mask_bit!(self.enabled),
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
        ];
        set_data_properties(&mut x_stream, &mask)?;

        match usize::try_from(pagecount) {
            Ok(v) if v == npages => (),
            _ => self.anomalies.push("Invalid/missing PageCount".to_string()),
        }

        // This is a best effort approach to retain the proper visual page order; failing this an
        // anomaly is logged and the site order is used. No point in hard failing
        let mut ordered_ids = HashMap::<i32, usize>::with_capacity(npages);
        for i in 0..npages {
            if let Ok(id) = rdi32le(&mut x_stream) {
                if site_ids.remove(&id) {
                    ordered_ids.insert(id, i);
                    continue;
                }
            }
            break;
        }
        if ordered_ids.len() == npages {
            pages.sort_by(|a, b| {
                ordered_ids[&pi.sites[a.site_index].id]
                    .cmp(&ordered_ids[&pi.sites[b.site_index].id])
            });
        }
        self.tabs_and_pages = ts.tabs.into_iter().zip(pages).collect();
        Ok(())
    }

    fn get_page_or_site(&self, n: usize) -> Option<Result<Control<R>, io::Error>> {
        if self.tabs_and_pages.is_empty() {
            return self.frame.get_child(n);
        }

        let (tab, page) = self.tabs_and_pages.get(n)?;
        let site = self.frame.get_child(page.site_index);
        if let Some(Ok(Control::Frame(frame))) = site {
            Some(Ok(Control::Page(Page::new(frame, page, tab))))
        } else {
            site
        }
    }
}

impl<'a, R: Read + Seek> From<MultiPage<'a, R>> for Frame<'a, R> {
    fn from(mp: MultiPage<'a, R>) -> Self {
        mp.frame
    }
}

/// A MultiPage Page
///
/// A `Page` is a convenience structure which does not exist in the specs
///
/// It is effectively a [`Frame`] with some extra control-specific characteristics
pub struct Page<'a, R: Read + Seek> {
    /// Info on the `Frame`-like component of the Page control
    pub frame: Frame<'a, R>,
    /// Control version
    pub version: u16,
    /// Specifies the effect displayed when the user switches between pages
    pub transition_effect: u32,
    /// Amount of time, in milliseconds, that the current page remains visible before switching to the new page
    pub transition_period: u32,
    /// The caption of the page
    pub caption: String,
    /// The tooltip of the page
    pub tooltip: String,
    /// The name of the page
    pub name: String,
    /// The tag of the page
    pub tag: String,
    /// The accelerator key
    pub accelerator: String,
    /// Whether the page is visible
    pub visible: bool,
    /// The page can receive the focus and respond to user-generated events
    pub enabled: bool,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl<'a, R: Read + Seek> fmt::Debug for Page<'a, R> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Page")
            .field("frame", &self.frame)
            .field("version", &self.version)
            .field("transition_effect", &self.transition_effect)
            .field("transition_period", &self.transition_period)
            .field("caption", &self.caption)
            .field("tooltip", &self.tooltip)
            .field("name", &self.name)
            .field("tag", &self.tag)
            .field("accelerator", &self.accelerator)
            .field("visible", &self.visible)
            .field("enabled", &self.enabled)
            .field("anomalies", &self.anomalies)
            .finish()
    }
}

impl<'a, R: 'a + Read + Seek> Page<'a, R> {
    fn new(frame: Frame<'a, R>, page_info: &PageInfo, tab_info: &Tab) -> Self {
        Self {
            frame,
            version: page_info.version,
            transition_effect: page_info.transition_effect,
            transition_period: page_info.transition_period,
            caption: tab_info.caption.clone(),
            tooltip: tab_info.tooltip.clone(),
            name: tab_info.name.clone(),
            tag: tab_info.tag.clone(),
            accelerator: tab_info.accelerator.clone(),
            visible: tab_info.visible,
            enabled: tab_info.enabled,
            anomalies: page_info.anomalies.clone(),
        }
    }
}

impl<'a, R: Read + Seek> From<Page<'a, R>> for Frame<'a, R> {
    fn from(pg: Page<'a, R>) -> Self {
        pg.frame
    }
}

#[derive(Debug)]
struct PageInfo {
    site_index: usize,
    version: u16,
    transition_effect: u32,
    transition_period: u32,
    anomalies: Vec<String>,
}

impl PageInfo {
    fn new<R: Read>(f: &mut R, idx: usize) -> Result<Self, io::Error> {
        let mut ret = Self {
            site_index: idx,
            version: rdu16le(f)?,
            transition_effect: 0,
            transition_period: 0,
            anomalies: Vec::new(),
        };

        if ret.version != 0x0200 {
            ret.anomalies.push(format!(
                "Invalid version {} (should be {})",
                ret.version, 0x200
            ));
        }

        let mask: PropertyMask = [
            Unused,
            property_mask_bit!(ret.transition_effect),
            property_mask_bit!(ret.transition_period),
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
        ];
        set_data_properties(f, &mask)?;

        Ok(ret)
    }
}

impl<'a, R: Read + Seek> ParentControl<'a, R> for MultiPage<'a, R> {
    fn pctrl_info(&'a self) -> &'a ParentControlInfo<'a, R> {
        &self.frame.pi
    }

    fn get_child(&'a self, n: usize) -> Option<Result<Control<'a, R>, io::Error>> {
        self.get_page_or_site(n)
    }
}

impl<'a, R: Read + Seek> ParentControl<'a, R> for Page<'a, R> {
    fn pctrl_info(&'a self) -> &'a ParentControlInfo<'a, R> {
        &self.frame.pi
    }
}

impl<'a, R: Read + Seek> ChildControl for MultiPage<'a, R> {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        self.frame.cctrl_info()
    }
}

impl<'a, R: Read + Seek> ChildControl for Page<'a, R> {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        self.frame.cctrl_info()
    }
}

// -------------------------- TESTS --------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    #[test]
    fn test_multipage() -> Result<(), io::Error> {
        let f: Vec<u8> = vec![
            0x00, 0x04, 0x24, 0x00, 0x48, 0x0c, 0x00, 0x0c, 0x0f, 0x00, 0x00, 0x00, 0x04, 0xc0,
            0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x7d, 0x00, 0x00, 0x62, 0x24, 0x00, 0x00,
            0x26, 0x17, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x70, 0x00, 0x00, 0x00, 0x00, 0x83, 0x01, 0x6f, 0x00, 0x00, 0x18, 0x00,
            0xe4, 0x01, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x94, 0x00, 0x00, 0x00, 0x02, 0x00,
            0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x24, 0x00,
            0xd5, 0x01, 0x00, 0x00, 0x05, 0x00, 0x00, 0x80, 0x09, 0x00, 0x00, 0x00, 0x23, 0x00,
            0x04, 0x00, 0x00, 0x00, 0x07, 0x00, 0x50, 0x61, 0x67, 0x65, 0x31, 0x61, 0x62, 0x34,
            0x35, 0x00, 0x00, 0x00, 0x2c, 0x02, 0x00, 0x00, 0x00, 0x00, 0x24, 0x00, 0xd5, 0x01,
            0x00, 0x00, 0x05, 0x00, 0x00, 0x80, 0x0a, 0x00, 0x00, 0x00, 0x21, 0x00, 0x04, 0x00,
            0x01, 0x00, 0x07, 0x00, 0x50, 0x61, 0x67, 0x65, 0x32, 0x00, 0x00, 0x00, 0x35, 0x00,
            0x00, 0x00, 0x2c, 0x02, 0x00, 0x00, 0x00, 0x02, 0x0c, 0x00, 0x19, 0x00, 0x00, 0x00,
            0xfc, 0x8f, 0x00, 0x00, 0xff, 0x01, 0x00, 0x00,
        ];
        let o: Vec<u8> = vec![
            0x00, 0x02, 0x6c, 0x00, 0x31, 0x80, 0xfa, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00,
            0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
            0x08, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x62, 0x24,
            0x00, 0x00, 0x26, 0x17, 0x00, 0x00, 0x05, 0x00, 0x00, 0x80, 0x50, 0x61, 0x67, 0x65,
            0x31, 0x6d, 0x65, 0x2e, 0x05, 0x00, 0x00, 0x80, 0x50, 0x61, 0x67, 0x65, 0x32, 0x6d,
            0x65, 0x2e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x80,
            0x54, 0x61, 0x62, 0x33, 0x04, 0x00, 0x00, 0x80, 0x54, 0x61, 0x62, 0x34, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x02, 0x18, 0x00, 0x35, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x80, 0xa5, 0x00,
            0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x54, 0x61, 0x68, 0x6f, 0x6d, 0x61, 0x62, 0x34,
            0x03, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
        ];
        #[rustfmt::skip]
        let x: Vec<u8> = vec![
            // Page 0 - bogus
            0x00, 0x02, // version
            0x04, 0x00, // cb
            0x00, 0x00, 0x00, 0x00, // mask
            // Page 1
            0x00, 0x02, // version
            0x0c, 0x00, // cb
            0x06, 0x00, 0x00, 0x00, // mask
            0x01, 0x00, 0x00, 0x00, // trans effect
            0xf4, 0x01, 0x00, 0x00, // trans period
            // Page 2
            0x00, 0x02, // version
            0x04, 0x00, // cb
            0x00, 0x00, 0x00, 0x00, // mask
            // MultiPage
            0x00, 0x02, // version
            0x0c, 0x00, // cb
            0x06, 0x00, 0x00, 0x00, // mask
            0x02, 0x00, 0x00, 0x00, // page count
            0x08, 0x00, 0x00, 0x00, // id
            // Page IDs
            0x09, 0x00, 0x00, 0x00, // First page id
            0x0a, 0x00, 0x00, 0x00, // Second page id
        ];

        let mut ole: Ole<Cursor<Vec<u8>>> = Ole::new();
        ole.push("Macros/MyForm/i07/f", Cursor::new(f));
        ole.push("Macros/MyForm/i07/o", Cursor::new(o));
        ole.push("Macros/MyForm/i07/x", Cursor::new(x));
        let mp = MultiPage::new(
            &ole,
            &OleSiteConcreteControl::default(),
            "Macros/MyForm/i07",
        )?;

        assert_eq!(mp.version, 0x200);
        assert_eq!(mp.id, 8);
        assert_eq!(mp.enabled, true);
        assert_eq!(mp.tabs_and_pages[0].0.caption, "Page1");
        assert_eq!(mp.tabs_and_pages[0].0.tooltip, "");
        assert_eq!(mp.tabs_and_pages[0].0.name, "Tab3");
        assert_eq!(mp.tabs_and_pages[0].0.tag, "");
        assert_eq!(mp.tabs_and_pages[0].0.accelerator, "");
        assert_eq!(mp.tabs_and_pages[0].0.visible, true);
        assert_eq!(mp.tabs_and_pages[0].0.enabled, true);
        assert_eq!(mp.tabs_and_pages[0].1.site_index, 1);
        assert_eq!(mp.tabs_and_pages[0].1.version, 0x200);
        assert_eq!(mp.tabs_and_pages[0].1.transition_effect, 1);
        assert_eq!(mp.tabs_and_pages[0].1.transition_period, 500);
        assert_eq!(mp.tabs_and_pages[1].0.caption, "Page2");
        assert_eq!(mp.tabs_and_pages[1].0.tooltip, "");
        assert_eq!(mp.tabs_and_pages[1].0.name, "Tab4");
        assert_eq!(mp.tabs_and_pages[1].0.tag, "");
        assert_eq!(mp.tabs_and_pages[1].0.accelerator, "");
        assert_eq!(mp.tabs_and_pages[1].0.visible, true);
        assert_eq!(mp.tabs_and_pages[1].0.enabled, true);
        assert_eq!(mp.tabs_and_pages[1].1.site_index, 2);
        assert_eq!(mp.tabs_and_pages[1].1.version, 0x200);
        assert_eq!(mp.tabs_and_pages[1].1.transition_effect, 0);
        assert_eq!(mp.tabs_and_pages[1].1.transition_period, 0);
        Ok(())
    }
}
