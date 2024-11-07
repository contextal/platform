//! # Office controls and shared structs
//! This module contains support structures for both the [parent] and [child]
//! controls

pub mod child;
pub mod parent;

use super::{mask::*, Font, Picture};
use crate::Ole;
use child::*;
use ctxutils::io::*;
use parent::Frame;
use parent::{MultiPage, Page};
use std::fmt;
use std::io::{self, Read, Seek};

// -------------------------- SHARED DATA STRUCTS --------------------------

/// Common data for all *Parent Controls* (as found in the *"f" stream*)
pub struct ParentControlInfo<'a, R: Read + Seek> {
    ole: &'a Ole<R>,
    path: String,
    /// The control name
    pub name: String,
    /// Control version (`Version`)
    pub version: u16,
    /// Background color (`BackColor`)
    pub back_color: u32,
    /// Foreground color (`ForeColor`)
    pub fore_color: u32,
    /// The largest ID that has been used by an embedded control inside this
    /// control (`NextAvailableID`)
    pub next_available_id: u32,
    /// Type of border used (`BorderStyle`)
    pub border_style: u8,
    /// Type of icon displayed as the mouse pointer (`MousePointer`)
    pub mouse_pointer: u8,
    /// Specifies whether this control has vertical or horizontal scroll bars and
    /// when to display them (`ScrollBars`)
    pub scroll_bars: u8,
    /// The number of control groups on this control (`GroupCnt`)
    ///
    /// Note: MS-OFORMS is ambivalent about the sign of this property
    pub group_count: u32,
    /// Specifies the behavior of the TAB key in the last control of
    /// this control (`Cycle`)
    pub cycle: u8,
    /// Specifies the visual appearance of the control (`SpecialEffect`)
    pub special_effect: u8,
    /// Color of the border (`BorderColor`)
    pub border_color: u32,
    /// The magnification in % (`Zoom`)
    ///
    /// Note: MS-OFORMS is ambivalent about the sign of this property
    pub zoom: u32,
    /// Specifies the alignment of the picture (`PictureAlignment`)
    pub picture_alignment: u8,
    /// Specifies whether the picture is tiled (`PictureTiling`)
    pub picture_tiling: bool,
    /// Specifies how to display the picture (`PictureSizeMode`): 0=Clip, 1=Stretch, 2=Zoom
    pub picture_size_mode: u8,
    /// The number of times the dynamic type information of this control has
    /// changed (`ShapeCookie`)
    pub shape_cookie: u32,
    /// The number of pixels in a buffer into which this control can be
    /// drawn (`DrawBuffer`)
    pub draw_buffer: u32,
    /// Physical size of this control, in `HIMETRIC` units (`DisplayedSize`)
    pub displayed_size: (i32, i32),
    /// Scrollable size of this control, in `HIMETRIC` units (`LogicalSize`)
    pub logical_size: (i32, i32),
    /// The coordinates of the first visible point of this control that
    /// is visible (`ScrollPosition`)
    pub scroll_position: (i32, i32),
    /// The caption of the frame (for the top level form title see
    /// [`DesignerInfo::caption`](parent::DesignerInfo::caption))
    pub caption: String,
    /// A custom mouse pointer image (`MouseIcon`)
    pub mouse_icon: Option<Picture>,
    /// The (optional) picture to display on this control (`Picture`)
    pub picture: Option<Picture>,
    /// The font to use for this form text (`Font`)
    pub font: Option<Font>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,

    sites: Vec<OleSiteConcreteControl>,
}

impl<'a, R: Read + Seek> fmt::Debug for ParentControlInfo<'a, R> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("ParentControlInfo")
            .field("path", &self.path)
            .field("name", &self.name)
            .field("version", &self.version)
            .field("back_color", &self.back_color)
            .field("fore_color", &self.fore_color)
            .field("next_available_id", &self.next_available_id)
            .field("border_style", &self.border_style)
            .field("mouse_pointer", &self.mouse_pointer)
            .field("scroll_bars", &self.scroll_bars)
            .field("group_count", &self.group_count)
            .field("cycle", &self.cycle)
            .field("special_effect", &self.special_effect)
            .field("border_color", &self.border_color)
            .field("zoom", &self.zoom)
            .field("picture_alignment", &self.picture_alignment)
            .field("picture_tiling", &self.picture_tiling)
            .field("picture_size_mode", &self.picture_size_mode)
            .field("shape_cookie", &self.shape_cookie)
            .field("draw_buffer", &self.draw_buffer)
            .field("displayed_size", &self.displayed_size)
            .field("logical_size", &self.logical_size)
            .field("scroll_position", &self.scroll_position)
            .field("caption", &self.caption)
            .field("mouse_icon", &self.mouse_icon)
            .field("picture", &self.picture)
            .field("font", &self.font)
            .field("anomalies", &self.anomalies)
            .field("sites", &self.sites)
            .finish_non_exhaustive()
    }
}

impl<'a, R: 'a + Read + Seek> ParentControlInfo<'a, R> {
    fn new(ole: &'a Ole<R>, name: &str, storage_name: &str) -> Result<Self, io::Error> {
        let mut ret = Self {
            ole,
            name: name.to_string(),
            path: storage_name.to_string(),
            version: 0,
            back_color: 0x8000000f,
            fore_color: 0x80000012,
            next_available_id: 0,
            border_style: 0,
            mouse_pointer: 0,
            scroll_bars: 0x0c,
            group_count: 0,
            cycle: 0,
            special_effect: 0,
            border_color: 0x80000012,
            zoom: 100,
            picture_alignment: 0x02,
            picture_tiling: false,
            picture_size_mode: 0,
            shape_cookie: 0,
            draw_buffer: 16000, // This must be persisted
            displayed_size: (4000, 3000),
            logical_size: (4000, 3000),
            scroll_position: (0, 0),
            caption: String::new(),
            mouse_icon: None,
            picture: None,
            font: None,
            anomalies: Vec::new(),
            sites: Vec::new(),
        };

        // Parse DataBlock, ExtraData, StreamData and SiteData
        ret.load_data()?;

        // There's nothing interesting in DesignExData
        Ok(ret)
    }

    fn load_data(&mut self) -> Result<(), io::Error> {
        let stream_name = format!("{}/f", self.path);
        let mut f_stream = self
            .ole
            .get_stream_reader(&self.ole.get_entry_by_name(&stream_name)?);
        self.version = rdu16le(&mut f_stream)?;
        if self.version != 0x0400 {
            // This is not actually a version but a size: changing this slightly doesn't cause
            // any visible effect but larger bumps make Office report an OOM
            self.anomalies.push(format!(
                "Invalid version {} (should be {})",
                self.version, 0x400
            ));
        }

        // helpers
        let mut boolean_properties: u32 = 0x00000004;
        let mut has_displayed_size: Option<()> = None;
        let mut has_logical_size: Option<()> = None;
        let mut has_scroll_position: Option<()> = None;
        let mut caption: Option<u32> = None;
        let mut mouse_icon: Option<u16> = None;
        let mut font: Option<u16> = None;
        let mut picture: Option<u16> = None;

        let mask: PropertyMask = [
            Unused, // Unused1
            property_mask_bit!(self.back_color),
            property_mask_bit!(self.fore_color),
            property_mask_bit!(self.next_available_id),
            Unused, // Unused2
            Unused, // Unused2
            property_mask_bit!(boolean_properties),
            property_mask_bit!(self.border_style),
            property_mask_bit!(self.mouse_pointer),
            property_mask_bit!(self.scroll_bars),
            property_mask_bit!(has_displayed_size),
            property_mask_bit!(has_logical_size),
            property_mask_bit!(has_scroll_position),
            property_mask_bit!(self.group_count),
            Unused, // Reserved
            property_mask_bit!(mouse_icon),
            property_mask_bit!(self.cycle),
            property_mask_bit!(self.special_effect),
            property_mask_bit!(self.border_color),
            property_mask_bit!(caption),
            property_mask_bit!(font),
            property_mask_bit!(picture),
            property_mask_bit!(self.zoom),
            property_mask_bit!(self.picture_alignment),
            property_mask_bit!(self.picture_tiling),
            property_mask_bit!(self.picture_size_mode),
            property_mask_bit!(self.shape_cookie),
            property_mask_bit!(self.draw_buffer),
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
        ];

        let extra_data = set_data_properties(&mut f_stream, &mask)?;
        // Note: enabled is NOT persisted here but only inside VBFrame; this one
        // is in fact completely ignored by Office (as per the specs)
        // self.enabled = boolean_properties & (1 << 2) != 0;
        let _has_desink = (boolean_properties & (1 << 14)) != 0;
        let has_clstbl = (boolean_properties & (1 << 15)) == 0;

        // ExtraData
        let mut cur = 0usize;
        if has_displayed_size.is_some() {
            let w = extra_data.get(cur..cur + 4);
            cur += 4;
            let h = extra_data.get(cur..cur + 4);
            cur += 4;
            if let (Some(w), Some(h)) = (w, h) {
                self.displayed_size = (
                    i32::from_le_bytes(w.try_into().unwrap()),
                    i32::from_le_bytes(h.try_into().unwrap()),
                );
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property DisplayedSize overflow",
                ));
            }
        }
        if has_logical_size.is_some() {
            let w = extra_data.get(cur..cur + 4);
            cur += 4;
            let h = extra_data.get(cur..cur + 4);
            cur += 4;
            if let (Some(w), Some(h)) = (w, h) {
                self.logical_size = (
                    i32::from_le_bytes(w.try_into().unwrap()),
                    i32::from_le_bytes(h.try_into().unwrap()),
                );
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property LogicalSize overflow",
                ));
            }
        }
        if has_scroll_position.is_some() {
            let t = extra_data.get(cur..cur + 4);
            cur += 4;
            let l = extra_data.get(cur..cur + 4);
            cur += 4;
            if let (Some(t), Some(l)) = (t, l) {
                self.scroll_position = (
                    i32::from_le_bytes(t.try_into().unwrap()),
                    i32::from_le_bytes(l.try_into().unwrap()),
                );
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property ScrollPosition overflow",
                ));
            }
        }
        // NOTE: According to the specs Form captions should not be persisted in Data/ExtraData
        // however Office does happily apply whatever caption is set in here to a usually hidden
        // frame inside the form
        if let Some(cob) = caption {
            if let Some((s, _slen)) = get_cob_string(cob, extra_data.get(cur..)) {
                self.caption = s;
                //cur += _slen;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property Caption overflow",
                ));
            }
        }

        // StreamData
        // MouseIcon
        if let Some(icon) = mouse_icon {
            if icon != 0xffff {
                self.anomalies.push(format!(
                    "Invalid MouseIcon value {} (instead of {})",
                    icon, 0xffff
                ));
            }
            if self.mouse_pointer != 0x63 {
                self.anomalies.push(
                    "Found custom mouse icon although this control is using a regular pointer"
                        .to_string(),
                );
            }
            self.mouse_icon = Some(Picture::new(&mut f_stream)?);
        }
        // Font
        if let Some(font) = font {
            if font != 0xffff {
                self.anomalies.push(format!(
                    "Invalid Font value {} (instead of {})",
                    font, 0xffff
                ));
            }
            self.font = Some(Font::guid_and_font(&mut f_stream)?);
        }
        // Picture
        if let Some(pic) = picture {
            if pic != 0xffff {
                self.anomalies.push(format!(
                    "Invalid Picture value {} (instead of {})",
                    pic, 0xffff
                ));
            }
            self.picture = Some(Picture::new(&mut f_stream)?);
        }

        // SiteData
        self.load_sites(&mut f_stream, has_clstbl)?;
        Ok(())
    }

    fn load_sites<T: Read + Seek>(
        &mut self,
        f: &mut T,
        has_clstable: bool,
    ) -> Result<(), io::Error> {
        fn skip<R: Read>(reader: R, bytes_to_skip: u64) -> Result<(), io::Error> {
            let mut skip_r = reader.take(bytes_to_skip);
            io::copy(&mut skip_r, &mut io::sink())?;
            Ok(())
        }
        // Skip the ClassTable
        if has_clstable {
            let nclasses = rdu16le(f)?;
            for _ in 0..nclasses {
                let size: i64 = (rdu32le(f)? >> 16).into(); // 16bit version + 16bit size
                f.seek(io::SeekFrom::Current(size))?;
            }
        }

        let nsites = rdu32le(f)?; // CountOfSites
        let size = rdu32le(f)?; // CountOfBytes
        let mut limited_f = f.take(size.into());

        // SiteDepthsAndTypes
        let mut nsites_listed = 0u32;
        let mut align = 0u8;
        while nsites_listed < nsites {
            let count = ((rdu16le(&mut limited_f)?) >> 8) as u8; // 8 bit depth + 8 bit TypeOrCount
            match nsites_listed.checked_add(if (count & 0x80) == 0 {
                align = align.overflowing_add(2).0;
                1
            } else {
                skip(&mut limited_f, 1)?; // Skip type
                align = align.overflowing_add(3).0;
                u32::from(count & !0x80)
            }) {
                Some(v) => nsites_listed = v,
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "SiteDepthsAndTypes overflow",
                    ));
                }
            }
        }
        if nsites_listed != nsites {
            // Note: Office will panic here with random memory or &HXXXX errors
            self.anomalies.push(format!(
                "CountOfSites reports {} sites, but {} were found in SiteDepthsAndTypes",
                nsites, nsites_listed
            ));
        }
        align = (4 - (align & 3)) & 3;
        skip(&mut limited_f, align.into())?;
        for _ in 0..nsites {
            self.sites
                .push(OleSiteConcreteControl::new(&mut limited_f)?);
        }
        Ok(())
    }

    fn get_streamed_child(
        &self,
        child: &OleSiteConcreteControl,
        offset: u64,
    ) -> Result<Control<R>, io::Error> {
        let path = self.ole.get_entry_by_name(&format!("{}/o", self.path))?;
        let mut o_stream = self.ole.get_stream_reader(&path);
        o_stream.seek(io::SeekFrom::Start(offset))?;
        Ok(match child.class_index {
            12 => Control::Image(Image::new(child, &mut o_stream)?),
            15 => {
                // Note this is technically defined as MorphData (whatever that means)
                // Office shows an error message stating that the control is not available
                Control::UnknownType(15)
            }
            16 => Control::SpinButton(SpinButton::new(child, &mut o_stream)?),
            17 => Control::CommandButton(CommandButton::new(child, &mut o_stream)?),
            18 => Control::TabStrip(TabStrip::new(child, &mut o_stream)?),
            21 => Control::Label(Label::new(child, &mut o_stream)?),
            23 => Control::TextBox(TextBox::new(child, &mut o_stream)?),
            24 => Control::ListBox(ListBox::new(child, &mut o_stream)?),
            25 => Control::ComboBox(ComboBox::new(child, &mut o_stream)?),
            26 => Control::CheckBox(CheckBox::new(child, &mut o_stream)?),
            27 => Control::OptionButton(OptionButton::new(child, &mut o_stream)?),
            28 => Control::ToggleButton(ToggleButton::new(child, &mut o_stream)?),
            47 => Control::ScrollBar(ScrollBar::new(child, &mut o_stream)?),
            t => Control::UnknownType(t),
        })
    }

    fn get_stored_child(&self, child: &OleSiteConcreteControl) -> Result<Control<R>, io::Error> {
        let path = format!("{}/i{:02}", self.path, child.id);
        Ok(match child.class_index {
            7 | 14 => Control::Frame(Frame::new(self.ole, child, &path)?),
            57 => Control::MultiPage(MultiPage::new(self.ole, child, &path)?),
            t => Control::UnknownType(t),
        })
    }

    fn get_child(&self, nchild: usize) -> Option<Result<Control<R>, io::Error>> {
        // Notes:
        // 1. Albeit briefly mentioned in [MS-OFORMS] 2.1.1.3, all the attempts to store non-parent
        //    children in a storage have failed to work properly
        //    Office tend to fail with a "File not found", hinting that it should actually be possible
        // 2. Parent controls, OTOH, most definitely cannot be streamed
        //    Office fails on these with an "Interface not supported" error
        let child = self.sites.get(nchild)?;
        Some(if child.streamed {
            // Note: Office does not account for ObjectStreamSize in non streamed objects
            // they are therefore skipped in the offset calculation below
            let offset: u64 = self
                .sites
                .iter()
                .take(nchild)
                .filter(|s| s.streamed)
                .map(|s| u64::from(s.stream_size))
                .sum();
            self.get_streamed_child(child, offset)
        } else {
            self.get_stored_child(child)
        })
    }
}

/// Common data for *Child Controls* (as found in the *"f" stream*)
#[derive(Debug, Clone)]
pub struct OleSiteConcreteControl {
    /// Version of the control
    pub version: u16,
    /// Name of the control
    pub name: String,
    /// Tag associated to the control
    pub tag: String,
    /// ID of the control
    pub id: i32,
    /// Help context of the control
    pub help_ctx: i32,
    /// Whether the control can receive focus while the user is navigating controls using the TAB key
    pub tab_stop: bool,
    /// Whether the control is displayed
    pub visible: bool,
    /// Whether the control is the default option on the parent
    pub default: bool,
    /// Whether the control is the cancel option on the form
    pub cancel: bool,
    /// Whether the control data is stored in a shared stream or a dedicated storage
    pub streamed: bool,
    /// Whether the control automatically resizes to display its entire contents
    pub auto_size: bool,
    /// Whether to preserve the height of a control when resizing (only for [`ListBox`] controls)
    pub preserve_height: bool,
    /// Whether to adjust the size of a control when the size of its parent changes
    pub fit_to_parent: bool,
    /// Whether to select the first child of a container control when the
    /// container control is the next control to which the user is navigating
    pub select_child: bool,
    /// Whether child controls are promoted to child objects of the parent of this control
    pub promote_controls: bool,
    /// Size, in bytes, of the control - if [`streamed`](Self::streamed)
    pub stream_size: u32,
    /// Tab order index of the control - negative values mean no index
    pub tab_index: i16,
    /// The control class type
    pub class_index: u16,
    /// Location (in `HIMETRICS`) of the top-left corner of the control relative to the top-left corner of the parent
    pub position: (i32, i32),
    /// Group ID the control belongs to - 0: no group
    pub group: u16,
    /// Control tooltip
    pub tooltip: String,
    /// License key of a control
    pub licence_key: String,
    /// The *worksheet cell* that determines the `Value` of the control: "": no source
    pub ctrl_source: String,
    /// The source for the list of values in [`ComboBox`] and [`ListBox`] controls
    pub row_source: String,
}

impl Default for OleSiteConcreteControl {
    fn default() -> Self {
        Self {
            version: 0,
            name: String::new(),
            tag: String::new(),
            id: 0,
            help_ctx: 0,
            tab_stop: true,
            visible: true,
            default: false,
            cancel: false,
            streamed: true,
            auto_size: true,
            preserve_height: false,
            fit_to_parent: false,
            select_child: false,
            promote_controls: false,
            stream_size: 0,
            tab_index: -1,
            class_index: 0x7fff,
            position: (0, 0),
            group: 0,
            tooltip: String::new(),
            licence_key: String::new(),
            ctrl_source: String::new(),
            row_source: String::new(),
        }
    }
}

impl OleSiteConcreteControl {
    fn new<R: Read>(f: &mut R) -> Result<Self, io::Error> {
        let version = rdu16le(f)?;
        if version != 0 {
            // Note: Office ignores the version field
        }
        let mut ret = Self {
            version,
            ..Self::default()
        };
        let mut name: Option<u32> = None;
        let mut tag: Option<u32> = None;
        let mut bitflags: Option<u32> = None;
        let mut has_position: Option<()> = None;
        let mut tooltip: Option<u32> = None;
        let mut licence_key: Option<u32> = None;
        let mut ctrl_source: Option<u32> = None;
        let mut row_source: Option<u32> = None;

        let mask: PropertyMask = [
            property_mask_bit!(name),
            property_mask_bit!(tag),
            property_mask_bit!(ret.id),
            property_mask_bit!(ret.help_ctx),
            property_mask_bit!(bitflags),
            property_mask_bit!(ret.stream_size),
            property_mask_bit!(ret.tab_index),
            property_mask_bit!(ret.class_index),
            property_mask_bit!(has_position),
            property_mask_bit!(ret.group),
            Unused, // Unused1
            property_mask_bit!(tooltip),
            property_mask_bit!(licence_key),
            property_mask_bit!(ctrl_source),
            property_mask_bit!(row_source),
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
            Unused, // Unused2
        ];
        let extra_data = set_data_properties(f, &mask)?;
        if let Some(bitflags) = bitflags {
            ret.set_bitflags(bitflags);
        }
        let mut cur = 0usize;
        if let Some(cob) = name {
            if let Some((s, slen)) = get_cob_string(cob, extra_data.get(cur..)) {
                ret.name = s;
                cur += slen;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Control name overflow",
                ));
            }
        }
        if let Some(cob) = tag {
            if let Some((s, slen)) = get_cob_string(cob, extra_data.get(cur..)) {
                ret.tag = s;
                cur += slen;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Control tag overflow",
                ));
            }
        }
        cur += (4 - (cur & 3)) & 3;
        if has_position.is_some() {
            let w = extra_data.get(cur..cur + 4);
            cur += 4;
            let h = extra_data.get(cur..cur + 4);
            cur += 4;
            if let (Some(w), Some(h)) = (w, h) {
                ret.position = (
                    i32::from_le_bytes(w.try_into().unwrap()),
                    i32::from_le_bytes(h.try_into().unwrap()),
                );
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Control position overflow",
                ));
            }
        }
        if let Some(cob) = tooltip {
            if let Some((s, slen)) = get_cob_string(cob, extra_data.get(cur..)) {
                ret.tooltip = s;
                cur += slen;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Control tooltip overflow",
                ));
            }
        }
        if let Some(cob) = licence_key {
            if let Some((s, slen)) = get_cob_string(cob, extra_data.get(cur..)) {
                ret.licence_key = s;
                cur += slen;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Control licence key overflow",
                ));
            }
        }
        if let Some(cob) = ctrl_source {
            if let Some((s, slen)) = get_cob_string(cob, extra_data.get(cur..)) {
                ret.ctrl_source = s;
                cur += slen;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Control source overflow",
                ));
            }
        }
        if let Some(cob) = row_source {
            if let Some((s, _slen)) = get_cob_string(cob, extra_data.get(cur..)) {
                ret.row_source = s;
                //cur += _slen;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Control row source overflow",
                ));
            }
        }
        Ok(ret)
    }

    fn set_bitflags(&mut self, bitflags: u32) {
        self.tab_stop = bitflags & (1 << 0) != 0;
        self.visible = bitflags & (1 << 1) != 0;
        self.default = bitflags & (1 << 2) != 0;
        self.cancel = bitflags & (1 << 3) != 0;
        self.streamed = bitflags & (1 << 4) != 0;
        self.auto_size = bitflags & (1 << 5) != 0;
        self.preserve_height = bitflags & (1 << 8) != 0;
        self.fit_to_parent = bitflags & (1 << 9) != 0;
        self.select_child = bitflags & (1 << 13) != 0;
        self.promote_controls = bitflags & (1 << 18) != 0;
    }
}

// -------------------------- ITERATION --------------------------

/// Enumeration of all possible *child controls*
pub enum Control<'a, R: Read + Seek> {
    /// A `Frame` control
    Frame(Frame<'a, R>),
    /// A `MultiPage` control
    MultiPage(MultiPage<'a, R>),
    /// A `Page` control
    Page(Page<'a, R>),
    /// A `CommandButton` control
    CommandButton(CommandButton),
    /// A `SpinBox` control
    SpinButton(SpinButton),
    /// An `Image` control
    Image(Image),
    /// A `Label` control
    Label(Label),
    /// A `CheckBox` control
    CheckBox(CheckBox),
    /// A `ComboBox` control
    ComboBox(ComboBox),
    /// A `ListBox` control
    ListBox(ListBox),
    /// An `OptionButton` control
    OptionButton(OptionButton),
    /// A `TestBox` control
    TextBox(TextBox),
    /// A `ToggleButton` control
    ToggleButton(ToggleButton),
    /// A `ScrollBar` control
    ScrollBar(ScrollBar),
    /// A `TabStrip` control
    TabStrip(TabStrip),
    /// A control whose (indicated) type is invalid
    UnknownType(u16),
}

/// A trait indicating a *parent control*
///
/// It provides internal enumeration of child controls (as found in the *"o" stream*)
/// via the [`children`](Self::children) method
///
/// It additionally offers a generic interface to retrieve parent control data
pub trait ParentControl<'a, R: 'a + Read + Seek> {
    /// Returns parent control info
    fn pctrl_info(&'a self) -> &'a ParentControlInfo<'a, R>;

    /// Retrieves a child control by its index
    fn get_child(&'a self, n: usize) -> Option<Result<Control<'a, R>, io::Error>> {
        self.pctrl_info().get_child(n)
    }

    /// Returns a child control iterator
    fn children(&'a self) -> ChildIterator<'a, R>
    where
        Self: Sized,
    {
        ChildIterator {
            parent: self,
            cur: 0usize,
        }
    }

    /// The control name
    fn get_name(&'a self) -> &'a str {
        &self.pctrl_info().name
    }
    /// Control version (`Version`)
    fn get_version(&'a self) -> u16 {
        self.pctrl_info().version
    }
    /// Background color (`BackColor`)
    fn get_back_color(&'a self) -> u32 {
        self.pctrl_info().back_color
    }
    /// Foreground color (`ForeColor`)
    fn get_fore_color(&'a self) -> u32 {
        self.pctrl_info().fore_color
    }
    /// The largest ID that has been used by an embedded control inside this
    /// control (`NextAvailableID`)
    fn get_next_available_id(&'a self) -> u32 {
        self.pctrl_info().next_available_id
    }
    /// Type of border used (`BorderStyle`)
    fn get_border_style(&'a self) -> u8 {
        self.pctrl_info().border_style
    }
    /// Type of icon displayed as the mouse pointer (`MousePointer`)
    fn get_mouse_pointer(&'a self) -> u8 {
        self.pctrl_info().mouse_pointer
    }
    /// Specifies whether this control has vertical or horizontal scroll bars and
    /// when to display them (`ScrollBars`)
    fn get_scroll_bars(&'a self) -> u8 {
        self.pctrl_info().scroll_bars
    }
    /// The number of control groups on this control (`GroupCnt`)
    ///
    /// Note: MS-OFORMS is ambivalent about the sign of this property
    fn get_group_count(&'a self) -> u32 {
        self.pctrl_info().group_count
    }
    /// Specifies the behavior of the TAB key in the last control of
    /// this control (`Cycle`)
    fn get_cycle(&'a self) -> u8 {
        self.pctrl_info().cycle
    }
    /// Specifies the visual appearance of the control (`SpecialEffect`)
    fn get_special_effect(&'a self) -> u8 {
        self.pctrl_info().special_effect
    }
    /// Color of the border (`BorderColor`)
    fn get_border_color(&'a self) -> u32 {
        self.pctrl_info().border_color
    }
    /// The magnification in % (`Zoom`)
    ///
    /// Note: MS-OFORMS is ambivalent about the sign of this property
    fn get_zoom(&'a self) -> u32 {
        self.pctrl_info().zoom
    }
    /// Specifies the alignment of the picture (`PictureAlignment`)
    fn get_picture_alignment(&'a self) -> u8 {
        self.pctrl_info().picture_alignment
    }
    /// Specifies whether the picture is tiled (`PictureTiling`)
    fn get_picture_tiling(&'a self) -> bool {
        self.pctrl_info().picture_tiling
    }
    /// Specifies how to display the picture (`PictureSizeMode`): 0=Clip, 1=Stretch, 2=Zoom
    fn get_picture_size_mode(&'a self) -> u8 {
        self.pctrl_info().picture_size_mode
    }
    /// The number of times the dynamic type information of this control has
    /// changed (`ShapeCookie`)
    fn get_shape_cookie(&'a self) -> u32 {
        self.pctrl_info().shape_cookie
    }
    /// The number of pixels in a buffer into which this control can be
    /// drawn (`DrawBuffer`)
    fn get_draw_buffer(&'a self) -> u32 {
        self.pctrl_info().draw_buffer
    }
    /// Physical size of this control, in `HIMETRIC` units (`DisplayedSize`)
    fn get_displayed_size(&'a self) -> (i32, i32) {
        self.pctrl_info().displayed_size
    }
    /// Scrollable size of this control, in `HIMETRIC` units (`LogicalSize`)
    fn get_logical_size(&'a self) -> (i32, i32) {
        self.pctrl_info().logical_size
    }
    /// The coordinates of the first visible point of this control that
    /// is visible (`ScrollPosition`)
    fn get_scroll_position(&'a self) -> (i32, i32) {
        self.pctrl_info().scroll_position
    }
    /// The caption of the frame (for the top level form title see
    /// [`DesignerInfo::caption`](parent::DesignerInfo::caption))
    fn get_caption(&'a self) -> &'a str {
        &self.pctrl_info().caption
    }
    /// A custom mouse pointer image (`MouseIcon`)
    fn get_mouse_icon(&'a self) -> Option<&Picture> {
        self.pctrl_info().mouse_icon.as_ref()
    }
    /// The (optional) picture to display on this control (`Picture`)
    fn get_picture(&'a self) -> Option<&Picture> {
        self.pctrl_info().picture.as_ref()
    }
    /// The font to use for this form text (`Font`)
    fn get_font(&'a self) -> Option<&Font> {
        self.pctrl_info().font.as_ref()
    }
    /// Non fatal anomalies encountered while processing the control
    fn get_anomalies(&'a self) -> &[String] {
        self.pctrl_info().anomalies.as_ref()
    }
}

/// Child control iterator
pub struct ChildIterator<'a, R: Read + Seek> {
    parent: &'a dyn ParentControl<'a, R>,
    cur: usize,
}

impl<'a, R: Read + Seek> Iterator for ChildIterator<'a, R> {
    type Item = Result<Control<'a, R>, io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let ret = self.parent.get_child(self.cur);
        self.cur += 1;
        ret
    }
}

/// A trait indicating a *child control*
///
/// It offers a generic interface to retrieve common child control data
pub trait ChildControl {
    /// Returns parent control info
    fn cctrl_info(&self) -> &OleSiteConcreteControl;

    /// Version of the control
    fn get_version(&self) -> u16 {
        self.cctrl_info().version
    }
    /// Name of the control
    fn get_name(&self) -> &str {
        &self.cctrl_info().name
    }
    /// Tag associated to the control
    fn get_tag(&self) -> &str {
        &self.cctrl_info().tag
    }
    /// ID of the control
    fn get_id(&self) -> i32 {
        self.cctrl_info().id
    }
    /// Help context of the control
    fn get_help_ctx(&self) -> i32 {
        self.cctrl_info().help_ctx
    }
    /// Whether the control can receive focus while the user is navigating controls using the TAB key
    fn get_tab_stop(&self) -> bool {
        self.cctrl_info().tab_stop
    }
    /// Whether the control is displayed
    fn get_visible(&self) -> bool {
        self.cctrl_info().visible
    }
    /// Whether the control is the default option on the parent
    fn get_default(&self) -> bool {
        self.cctrl_info().default
    }
    /// Whether the control is the cancel option on the form
    fn get_cancel(&self) -> bool {
        self.cctrl_info().cancel
    }
    /// Weather the control data is stored in a shared stream or a dedicated storage
    fn get_streamed(&self) -> bool {
        self.cctrl_info().streamed
    }
    /// Whether the control automatically resizes to display its entire contents
    fn get_auto_size(&self) -> bool {
        self.cctrl_info().auto_size
    }
    /// Whether to preserve the height of a control when resizing (only for [`ListBox`] controls)
    fn get_preserve_height(&self) -> bool {
        self.cctrl_info().preserve_height
    }
    /// Whether to adjust the size of a control when the size of its parent changes
    fn get_fit_to_parent(&self) -> bool {
        self.cctrl_info().fit_to_parent
    }
    /// Whether to select the first child of a container control when the
    /// container control is the next control to which the user is navigating
    fn get_select_child(&self) -> bool {
        self.cctrl_info().select_child
    }
    /// Whether child controls are promoted to child objects of the parent of this control
    fn get_promote_controls(&self) -> bool {
        self.cctrl_info().promote_controls
    }
    /// Size, in bytes, of the control - if [`streamed`](Self::get_streamed)
    fn get_stream_size(&self) -> u32 {
        self.cctrl_info().stream_size
    }
    /// Tab order index of the control - negative values mean no index
    fn get_tab_index(&self) -> i16 {
        self.cctrl_info().tab_index
    }
    /// The control class type
    fn get_class_index(&self) -> u16 {
        self.cctrl_info().class_index
    }
    /// Location (in `HIMETRICS`) of the top-left corner of the control relative to the top-left corner of the parent
    fn get_position(&self) -> (i32, i32) {
        self.cctrl_info().position
    }
    /// Group ID the control belongs to - 0: no group
    fn get_group(&self) -> u16 {
        self.cctrl_info().group
    }
    /// Control tooltip
    fn get_tooltip(&self) -> &str {
        &self.cctrl_info().tooltip
    }
    /// License key of a control
    fn get_licence_key(&self) -> &str {
        &self.cctrl_info().licence_key
    }
    /// The *worksheet cell* that determines the `Value` of the control: "": no source
    fn get_ctrl_source(&self) -> &str {
        &self.cctrl_info().ctrl_source
    }
    /// The source for the list of values in [`ComboBox`] and [`ListBox`] controls
    fn get_row_source(&self) -> &str {
        &self.cctrl_info().row_source
    }
}

// -------------------------- TESTS --------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};
    #[test]
    fn test_site_oddmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x00, // version
            0x2c, 0x00, // cbsite
            0b10101010, 0b10101010, 0b10101010, 0b10101010, // mask
            0x05, 0x00, 0x00, 0x80, // tag
            0x20, 0x30, 0x40, 0x50, // help_ctx
            0x04, 0x03, 0x02, 0x01, // stream size
            0x0c, 0x00, // class_index
            0x42, 0x00, // group id
            0x03, 0x00, 0x00, 0x80, // tooltip
            0x04, 0x00, 0x00, 0x80, // control source

            // ExtraData
            0x43, 0x68, 0x69, 0x6c, 0x64, // "Child"
            0x00, 0x00, 0x00, // pad
            0x54, 0x69, 0x70, // "Tip"
            0x00, // pad
            0x42, 0x6c, 0x61, 0x68, // "Blah"

            // StreamData is empty
        ];

        let mut f = Cursor::new(buf.as_slice());
        let site = OleSiteConcreteControl::new(&mut f)?;
        assert_eq!(site.version, 0);
        assert_eq!(site.name, "");
        assert_eq!(site.tag, "Child");
        assert_eq!(site.id, 0);
        assert_eq!(site.help_ctx, 0x50403020);
        assert_eq!(site.tab_stop, true);
        assert_eq!(site.visible, true);
        assert_eq!(site.default, false);
        assert_eq!(site.cancel, false);
        assert_eq!(site.streamed, true);
        assert_eq!(site.auto_size, true);
        assert_eq!(site.preserve_height, false);
        assert_eq!(site.fit_to_parent, false);
        assert_eq!(site.select_child, false);
        assert_eq!(site.promote_controls, false);
        assert_eq!(site.stream_size, 0x01020304);
        assert_eq!(site.tab_index, -1);
        assert_eq!(site.class_index, 0x0c);
        assert_eq!(site.position, (0, 0));
        assert_eq!(site.group, 0x42);
        assert_eq!(site.tooltip, "Tip");
        assert_eq!(site.licence_key, "");
        assert_eq!(site.ctrl_source, "Blah");
        assert_eq!(site.row_source, "");
        Ok(())
    }

    #[test]
    fn test_site_evenmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x00, // version
            0x3c, 0x00, // cbsite
            0b01010101, 0b01010101, 0b01010101, 0b01010101, // mask
            0x06, 0x00, 0x00, 0x80, // name
            0x02, 0x03, 0x04, 0x05, // ID
            0xcc, 0xff, 0xff, 0xff, // bitflags
            0x01, 0x00, // tab_index
            0x42, 0x00, // group id
            0x0a, 0x00, 0x00, 0x80, // licence
            0x03, 0x00, 0x00, 0x80, // row source

            // ExtraData
            0x4c, 0x61, 0x62, 0x65, 0x6c, 0x31, // "Label1"
            0x00, 0x00, // pad
            0x67, 0x45, 0x23, 0x01, // size x
            0x10, 0x32, 0x54, 0x76, // size y
            0x50, 0x72, 0x6f, 0x64, // ProductKey
            0x75, 0x63, 0x74, 0x4b,
            0x65, 0x79,
            0x00, 0x00, // pad
            0x52, 0x6f, 0x77, // "Row"
            0x68, // pad

            // StreamData is empty
        ];

        let mut f = Cursor::new(buf.as_slice());
        let site = OleSiteConcreteControl::new(&mut f)?;
        assert_eq!(site.version, 0);
        assert_eq!(site.name, "Label1");
        assert_eq!(site.tag, "");
        assert_eq!(site.id, 0x05040302);
        assert_eq!(site.help_ctx, 0);
        assert_eq!(site.tab_stop, false);
        assert_eq!(site.visible, false);
        assert_eq!(site.default, true);
        assert_eq!(site.cancel, true);
        assert_eq!(site.streamed, false);
        assert_eq!(site.auto_size, false);
        assert_eq!(site.preserve_height, true);
        assert_eq!(site.fit_to_parent, true);
        assert_eq!(site.select_child, true);
        assert_eq!(site.promote_controls, true);
        assert_eq!(site.stream_size, 0);
        assert_eq!(site.tab_index, 1);
        assert_eq!(site.class_index, 0x7fff);
        assert_eq!(site.position, (0x01234567, 0x76543210));
        assert_eq!(site.group, 0);
        assert_eq!(site.tooltip, "");
        assert_eq!(site.licence_key, "ProductKey");
        assert_eq!(site.ctrl_source, "");
        assert_eq!(site.row_source, "Row");
        Ok(())
    }

    #[test]
    fn test_parent_control_evenmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x04, // version
            0x30, 0x00, // cbsite
            0b01010101, 0b01010101, 0b01010101, 0b01010101, // mask
            0x11, 0x22, 0x33, 0x44, // fore color
            0x04, 0x80, 0x00, 0x00, // various props
            0x02, // mouse pointer
            0x0a, // cycle
            0x00, 0x00, // pad
            0xaa, 0xbb, 0xcc, 0xdd, // border color
            0xff, 0xff, // font
            0x00, 0x00, // pad
            0xc8, 0x00, 0x00, 0x00, // zoom
            0x0d, 0xd0, 0xad, 0xde, // cookie

            // ExtraData
            0xe8, 0x03, 0x00, 0x00, // disp w
            0xf4, 0x01, 0x00, 0x00, // disp h
            0x2c, 0x01, 0x00, 0x00, // scroll x
            0x64, 0x00, 0x00, 0x00, // scroll y

            // StreamData
            // Font
            0x03, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // clsid
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51,
            0x01, // version
            0x00, 0x00, // charset
            0x00, // flags
            0x01, 0x00, // weight
            0x01, 0x00, 0x00, 0x00, // height
            0x03, 0x41, 0x73, 0x64,

            // Sites
            // no CountOfSiteClassInfo
            0x00, 0x00, 0x00, 0x00, // nsites
            0x00, 0x00, 0x00, 0x00, // nbytes
        ];

        let mut ole: Ole<Cursor<Vec<u8>>> = Ole::new();
        ole.push("Macros/MyForm/i05/f", Cursor::new(buf));
        let pc = ParentControlInfo::new(&ole, "MyFrame", "Macros/MyForm/i05")?;
        assert_eq!(pc.path, "Macros/MyForm/i05");
        assert_eq!(pc.version, 0x400);
        assert_eq!(pc.name, "MyFrame");
        assert_eq!(pc.back_color, 0x8000000f); // mask bit 1
        assert_eq!(pc.fore_color, 0x44332211); // mask bit 2
        assert_eq!(pc.next_available_id, 0); // mask bit 3
        assert_eq!(pc.border_style, 0); // mask bit 7
        assert_eq!(pc.mouse_pointer, 2); // mask bit 8
        assert_eq!(pc.scroll_bars, 0xc); // mask bit 9
        assert_eq!(pc.displayed_size, (1000, 500)); // mask bit 10
        assert_eq!(pc.logical_size, (4000, 3000)); // mask bit 11
        assert_eq!(pc.scroll_position, (300, 100)); // mask bit 12
        assert_eq!(pc.group_count, 0); // mask bit 13
        assert!(pc.mouse_icon.is_none()); // mask bit 15
        assert_eq!(pc.cycle, 10); // mask bit 16
        assert_eq!(pc.special_effect, 0); // mask bit 17
        assert_eq!(pc.border_color, 0xddccbbaa); // mask bit 18
        assert_eq!(pc.caption, ""); // mask bit 19
        assert_eq!(pc.font.unwrap().name, "Asd"); // mask bit 20
        assert!(pc.picture.is_none()); // mask bit 21
        assert_eq!(pc.zoom, 200); // mask bit 22
        assert_eq!(pc.picture_alignment, 2); // mask bit 23
        assert_eq!(pc.picture_tiling, true); // mask bit 24
        assert_eq!(pc.picture_size_mode, 0); // mask bit 25
        assert_eq!(pc.shape_cookie, 0xdeadd00d); // mask bit 26
        assert_eq!(pc.draw_buffer, 16000); // mask bit 27
        Ok(())
    }
}
