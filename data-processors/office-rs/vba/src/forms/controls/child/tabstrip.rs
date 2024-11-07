use crate::forms::controls::{ChildControl, OleSiteConcreteControl};
use crate::forms::{mask::*, Font, Picture};
use ctxutils::io::*;
use std::io::{self, Read, Seek};

/// TabStrip control
#[derive(Debug)]
pub struct TabStrip {
    /// Site control properties
    pub control: OleSiteConcreteControl,
    /// The control version
    pub version: u16,
    /// The index of the selected tab
    pub list_index: i32,
    /// Background color
    pub back_color: u32,
    /// Foreground color
    pub fore_color: u32,
    /// Width and height of the control, in HIMETRIC units
    pub size: (i32, i32),
    /// Type of icon displayed as the mouse pointer of the control
    pub mouse_pointer: u8,
    /// Position of the tabs of a form (0: top, 1: bottom, 2: left, 3: right)
    pub orientation: u32,
    /// Sisplay style of the tabs (0: tabs, 1: buttons)
    pub style: u32,
    /// Whether the tabs can be displayed in more than one row
    pub multi_row: bool,
    /// Tab width, in HIMETRIC units
    pub fixed_width: u32,
    /// Tab height, in HIMETRIC units
    pub fixed_height: u32,
    /// Whether to display the tooltips
    pub show_tooltips: bool,
    /// The control can receive the focus and respond to user-generated events
    pub enabled: bool,
    /// The default run-time mode of the Input Method Editor (IME)
    pub ime_mode: u8,
    /// The number of tabs that have been inserted since the control was created
    pub tabs_allocated: u32,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// Font properties
    pub font: Option<Font>,
    /// The Tabs in the control
    pub tabs: Vec<Tab>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl Default for TabStrip {
    fn default() -> Self {
        Self {
            control: OleSiteConcreteControl::default(),
            version: 0,
            list_index: -1,
            back_color: 0x8000000f,
            fore_color: 0x80000012,
            size: (0, 0),
            mouse_pointer: 0,
            orientation: 0,
            style: 0,
            multi_row: false,
            fixed_width: 0,
            fixed_height: 0,
            show_tooltips: true,
            enabled: true,
            ime_mode: 0,
            tabs_allocated: 0,
            mouse_icon: None,
            font: None,
            tabs: Vec::new(),
            anomalies: Vec::new(),
        }
    }
}

impl TabStrip {
    pub(crate) fn new<R: Read + Seek>(
        base: &OleSiteConcreteControl,
        f: &mut R,
    ) -> Result<Self, io::Error> {
        let mut ret = Self {
            control: base.clone(),
            ..Self::default()
        };
        ret.version = rdu16le(f)?;
        if ret.version != 0x0200 {
            ret.anomalies.push(format!(
                "Invalid version {} (should be {})",
                ret.version, 0x200
            ));
        }

        let mut has_size: Option<()> = None;
        let mut items_size = 0u32;
        let mut tips_size = 0u32;
        let mut names_size = 0u32;
        let mut various_properties: u32 = 0x0000001b;
        let mut is_new_version = false;
        let mut tags_size = 0u32;
        let mut tab_data = 0u32;
        let mut accels_size = 0u32;
        let mut mouse_icon: Option<u16> = None;

        let mask: PropertyMask = [
            property_mask_bit!(ret.list_index),
            property_mask_bit!(ret.back_color),
            property_mask_bit!(ret.fore_color),
            Unused,
            property_mask_bit!(has_size),
            property_mask_bit!(items_size),
            property_mask_bit!(ret.mouse_pointer),
            Unused,
            property_mask_bit!(ret.orientation),
            property_mask_bit!(ret.style),
            property_mask_bit!(ret.multi_row),
            property_mask_bit!(ret.fixed_width),
            property_mask_bit!(ret.fixed_height),
            property_mask_bit!(ret.show_tooltips),
            Unused,
            property_mask_bit!(tips_size),
            Unused,
            property_mask_bit!(names_size),
            property_mask_bit!(various_properties),
            property_mask_bit!(is_new_version),
            property_mask_bit!(ret.tabs_allocated),
            property_mask_bit!(tags_size),
            property_mask_bit!(tab_data),
            property_mask_bit!(accels_size),
            property_mask_bit!(mouse_icon),
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
            Unused,
        ];

        let extra_data = set_data_properties(f, &mask)?;
        ret.set_various_properties(various_properties);

        // ExtraData
        let mut cur = 0usize;
        if has_size.is_some() {
            let w = extra_data.get(cur..cur + 4);
            cur += 4;
            let h = extra_data.get(cur..cur + 4);
            cur += 4;
            if let (Some(w), Some(h)) = (w, h) {
                ret.size = (
                    i32::from_le_bytes(w.try_into().unwrap()),
                    i32::from_le_bytes(h.try_into().unwrap()),
                );
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property Size overflow",
                ));
            }
        } else {
            ret.anomalies
                .push("The control Size property should be persisted but it's not".to_string());
        }

        // Items
        if let Ok(size) = usize::try_from(items_size) {
            ret.set_tabs(extra_data.get(cur..(cur + size)))?;
            cur += size;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Extra property Tab Caption integer conversion failed",
            ));
        }

        // TipStrings
        if let Ok(size) = usize::try_from(tips_size) {
            ret.set_tab_tips(extra_data.get(cur..(cur + size)))?;
            cur += size;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Extra property Tab Tooltip integer conversion failed",
            ));
        }

        // TabNames
        if let Ok(size) = usize::try_from(names_size) {
            ret.set_tab_names(extra_data.get(cur..(cur + size)))?;
            cur += size;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Extra property Tab Name integer conversion failed",
            ));
        }

        // Tags
        if let Ok(size) = usize::try_from(tags_size) {
            ret.set_tab_tags(extra_data.get(cur..(cur + size)))?;
            cur += size;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Extra property Tab Tag integer conversion failed",
            ));
        }

        // Accelerators
        if let Ok(size) = usize::try_from(accels_size) {
            ret.set_tab_accels(extra_data.get(cur..(cur + size)))?;
            //cur += size;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Extra property Tab Name integer conversion failed",
            ));
        }

        // StreamData
        // MouseIcon
        if let Some(icon) = mouse_icon {
            if icon != 0xffff {
                ret.anomalies.push(format!(
                    "Invalid MouseIcon value {} (instead of {})",
                    icon, 0xffff
                ));
            }
            if ret.mouse_pointer != 0x63 {
                ret.anomalies.push(
                    "Found custom mouse icon although this control is using a regular pointer"
                        .to_string(),
                );
            }
            ret.mouse_icon = Some(Picture::new(f)?);
        }

        // TextProps
        if is_new_version {
            ret.font = Some(Font::text_props(f)?);
        }

        // TabData
        for i in 0..tab_data {
            let flags = rdu32le(f)?;
            if let Ok(i) = usize::try_from(i) {
                if let Some(tab) = ret.tabs.get_mut(i) {
                    tab.visible = (flags & 1) != 0;
                    tab.enabled = (flags & 2) != 0;
                }
            }
        }

        Ok(ret)
    }

    fn set_various_properties(&mut self, vp: u32) {
        self.enabled = (vp & (1 << 1)) != 0;
        self.ime_mode = ((vp >> 15) & 0b1111) as u8;
        if (vp & (1 << 3)) == 0 {
            self.anomalies
                .push("The control should be opaque but it's not".to_string());
        }
    }

    // Note: Office relies on this first array to determine the number of tabs;
    // TabsAllocated is merely used as a hint to name newly added tabs
    fn set_tabs(&mut self, buf: Option<&[u8]>) -> Result<(), io::Error> {
        if let Some(buf) = buf {
            let mut size = buf.len();
            let mut cur = 0usize;
            loop {
                if size == 0 {
                    return Ok(());
                }
                if let Some((s, slen)) = get_array_string(buf.get(cur..)) {
                    cur += slen;
                    if size >= slen {
                        size -= slen;
                        self.tabs.push(Tab {
                            caption: s,
                            ..Tab::default()
                        });
                        continue;
                    }
                }
                break;
            }
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Extra property Tab Caption overflow",
        ))
    }

    fn parse_string_array(&mut self, buf: Option<&[u8]>, cb: fn(&mut Tab, String)) -> Option<()> {
        let buf = buf?;
        let mut len = buf.len();
        let mut i = 0usize;
        let mut cur = 0usize;
        loop {
            if len == 0 {
                return Some(());
            }
            if let Some((s, slen)) = get_array_string(buf.get(cur..)) {
                cur += slen;
                if len >= slen {
                    len -= slen;
                    if let Some(tab) = self.tabs.get_mut(i) {
                        cb(tab, s);
                    }
                    i += 1;
                    continue;
                }
            }
            break;
        }
        None
    }

    fn set_tab_tips(&mut self, buf: Option<&[u8]>) -> Result<(), io::Error> {
        self.parse_string_array(buf, |t, s| t.tooltip = s)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property Tab Tooltip overflow",
                )
            })
    }

    fn set_tab_names(&mut self, buf: Option<&[u8]>) -> Result<(), io::Error> {
        self.parse_string_array(buf, |t, s| t.name = s)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property Tab Name overflow",
                )
            })
    }

    fn set_tab_tags(&mut self, buf: Option<&[u8]>) -> Result<(), io::Error> {
        self.parse_string_array(buf, |t, s| t.tag = s)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property Tab Tag overflow",
                )
            })
    }

    fn set_tab_accels(&mut self, buf: Option<&[u8]>) -> Result<(), io::Error> {
        self.parse_string_array(buf, |t, s| t.accelerator = s)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property Tab Accelerator overflow",
                )
            })
    }
}

impl ChildControl for TabStrip {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

/// A tab page
#[derive(Debug, Default)]
pub struct Tab {
    /// The caption of the tab
    pub caption: String,
    /// The tooltip of the tab
    pub tooltip: String,
    /// The name of the tab
    pub name: String,
    /// The tag of the tab
    pub tag: String,
    /// The accelerator key
    pub accelerator: String,
    /// Whether the tab is visible
    pub visible: bool,
    /// The tab can receive the focus and respond to user-generated events
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};
    #[test]
    fn test_tab() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // version
            0x70, 0x00, // size
            0x31, 0x80, 0xfa, 0x00, // mask
            0x01, 0x00, 0x00, 0x00, // list index
            0x10, 0x00, 0x00, 0x00, // items size
            0x0c, 0x00, 0x00, 0x00, // tips size
            0x10, 0x00, 0x00, 0x00, // names size
            0x02, 0x00, 0x00, 0x00, // tabs allocated
            0x0c, 0x00, 0x00, 0x00, // tags size
            0x02, 0x00, 0x00, 0x00, // tab data
            0x0c, 0x00, 0x00, 0x00, // accels size

            // ExtraData
            0xdc, 0x26, 0x00, 0x00, // size w
            0x30, 0x12, 0x00, 0x00, // size h
            0x04, 0x00, 0x00, 0x80, 0x54, 0x61, 0x62, 0x31, // caption Tab1
            0x04, 0x00, 0x00, 0x80, 0x54, 0x61, 0x62, 0x32, // caption Tab2
            0x04, 0x00, 0x00, 0x80, 0x54, 0x69, 0x70, 0x31, // tip "Tip1"
            0x00, 0x00, 0x00, 0x00, // tip ""
            0x04, 0x00, 0x00, 0x80, 0x54, 0x61, 0x62, 0x31, // name Tab1
            0x04, 0x00, 0x00, 0x80, 0x54, 0x61, 0x62, 0x32, // name Tab2
            0x00, 0x00, 0x00, 0x00, // tag ""
            0x04, 0x00, 0x00, 0x80, 0x54, 0x61, 0x67, 0x32, // tag "Tag2"
            0x01, 0x00, 0x00, 0x00, 0x31, 0x00, 0x00, 0x00, // accel "1"
            0x00, 0x00, 0x00, 0x00, // accel ""

            // TextProp
            0x00, 0x02,
            0x18, 0x00,
            0x35, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x80,
            0xa5, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x54, 0x61, 0x68, 0x6f, 0x6d, 0x61, 0x62, 0x32,

            // TabData
            0x01, 0x00, 0x00, 0x00,
            0x02, 0x00, 0x00, 0x00,
        ];

        let mut f = Cursor::new(buf.as_slice());
        let btn = TabStrip::new(&OleSiteConcreteControl::default(), &mut f)?;
        assert_eq!(btn.version, 0x200);
        assert_eq!(btn.list_index, 1);
        assert_eq!(btn.back_color, 0x8000000f);
        assert_eq!(btn.fore_color, 0x80000012);
        assert_eq!(btn.size, (0x26dc, 0x1230));
        assert_eq!(btn.mouse_pointer, 0);
        assert_eq!(btn.orientation, 0);
        assert_eq!(btn.style, 0);
        assert_eq!(btn.multi_row, false);
        assert_eq!(btn.fixed_width, 0);
        assert_eq!(btn.fixed_height, 0);
        assert_eq!(btn.show_tooltips, true);
        assert_eq!(btn.enabled, true);
        assert_eq!(btn.ime_mode, 0);
        assert_eq!(btn.tabs_allocated, 2);
        assert!(btn.mouse_icon.is_none());
        assert_eq!(btn.font.unwrap().name, "Tahoma");
        assert_eq!(btn.tabs[0].caption, "Tab1");
        assert_eq!(btn.tabs[0].tooltip, "Tip1");
        assert_eq!(btn.tabs[0].name, "Tab1");
        assert_eq!(btn.tabs[0].tag, "");
        assert_eq!(btn.tabs[0].accelerator, "1");
        assert_eq!(btn.tabs[0].visible, true);
        assert_eq!(btn.tabs[0].enabled, false);
        assert_eq!(btn.tabs[1].caption, "Tab2");
        assert_eq!(btn.tabs[1].tooltip, "");
        assert_eq!(btn.tabs[1].name, "Tab2");
        assert_eq!(btn.tabs[1].tag, "Tag2");
        assert_eq!(btn.tabs[1].accelerator, "");
        assert_eq!(btn.tabs[1].visible, false);
        assert_eq!(btn.tabs[1].enabled, true);

        Ok(())
    }
}
