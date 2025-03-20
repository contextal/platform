use crate::forms::controls::{ChildControl, OleSiteConcreteControl};
use crate::forms::{Font, Picture, mask::*};
use ctxutils::io::*;
use std::io::{self, Read, Seek};

#[derive(Debug)]
struct Morph {
    version: u16,
    back_color: u32,
    fore_color: u32,
    max_length: u32,
    border_style: u8,
    scroll_bars: u8,
    display_style: u8,
    mouse_pointer: u8,
    size: (i32, i32),
    password_char: u16,
    list_width: u32,
    bound_column: u16,
    text_column: i16,
    column_count: i16,
    list_rows: u16,
    column_info: u16,
    match_entry: u8,
    list_style: u8,
    drop_btn_when: u8,
    drop_btn_style: u8,
    multi_select: u8,
    value: String,
    caption: String,
    picture_position: u32,
    border_color: u32,
    special_effect: u32,
    mouse_icon: Option<Picture>,
    picture: Option<Picture>,
    accelerator: u16,
    group: String,
    font: Option<Font>,
    anomalies: Vec<String>,
    // VP
    enabled: bool,
    locked: bool,
    opaque: bool,
    column_heads: bool,
    integral_height: bool,
    match_required: bool,
    left_aligned: bool,
    editable: bool,
    ime_mode: u8,
    drag_behavior: bool,
    enter_key_behaviour: bool,
    enter_field_behaviour: bool,
    tab_key_behaviour: bool,
    word_wrap: bool,
    border_suppress: bool,
    selection_margin: bool,
    auto_word_select: bool,
    auto_size: bool,
    hide_selection: bool,
    auto_tab: bool,
    multi_line: bool,
}

impl Default for Morph {
    fn default() -> Self {
        Self {
            version: 0,
            back_color: 0x80000005,
            fore_color: 0x80000008,
            max_length: 0,
            border_style: 0,
            scroll_bars: 0,
            display_style: 1,
            mouse_pointer: 0,
            size: (0, 0),
            password_char: 0,
            list_width: 0,
            bound_column: 1,
            text_column: -1,
            column_count: 1,
            list_rows: 8,
            column_info: 0,
            match_entry: 2,
            list_style: 0,
            drop_btn_when: 0,
            drop_btn_style: 1,
            multi_select: 0,
            value: String::new(),
            caption: String::new(),
            picture_position: 0x00070001,
            border_color: 0x80000006,
            special_effect: 2,
            mouse_icon: None,
            picture: None,
            accelerator: 0,
            group: String::new(),
            font: None,
            anomalies: Vec::new(),
            // VP
            enabled: false,
            locked: false,
            opaque: false,
            column_heads: false,
            integral_height: false,
            match_required: false,
            left_aligned: false,
            editable: false,
            ime_mode: 0,
            drag_behavior: false,
            enter_key_behaviour: false,
            enter_field_behaviour: false,
            tab_key_behaviour: false,
            word_wrap: false,
            border_suppress: false,
            selection_margin: false,
            auto_word_select: false,
            auto_size: false,
            hide_selection: false,
            auto_tab: false,
            multi_line: false,
        }
    }
}

impl Morph {
    fn new<R: Read + Seek>(f: &mut R) -> Result<Self, io::Error> {
        let mut ret = Self {
            version: rdu16le(f)?,
            ..Self::default()
        };
        if ret.version != 0x0200 {
            ret.anomalies.push(format!(
                "Invalid version {} (should be {})",
                ret.version, 0x200
            ));
        }
        let size = rdu16le(f)?;
        if size < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Insuffient stream size",
            ));
        }
        let mask = rdu64le(f)?;
        let mut buf: Vec<u8> = vec![0u8; (size - 8).into()];
        f.read_exact(&mut buf)?;
        let mut cur = 0usize;
        let various_properties: u32;
        if mask & (1 << 0) != 0 {
            various_properties = u32::from_le_bytes(
                buf.get(cur..cur + 4)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "VariousProperties overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 4;
        } else {
            various_properties = 0x2c80081b;
        }
        ret.set_various_properties(various_properties);
        if mask & (1 << 1) != 0 {
            ret.back_color = u32::from_le_bytes(
                buf.get(cur..cur + 4)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "BackColor overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 4;
        }
        if mask & (1 << 2) != 0 {
            ret.fore_color = u32::from_le_bytes(
                buf.get(cur..cur + 4)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "ForeColor overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 4;
        }
        if mask & (1 << 3) != 0 {
            ret.max_length = u32::from_le_bytes(
                buf.get(cur..cur + 4)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "MaxLength overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 4;
        }
        if mask & (1 << 4) != 0 {
            ret.border_style = *buf.get(cur).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "BorderStyle overflow")
            })?;
            cur += 1;
        }
        if mask & (1 << 5) != 0 {
            ret.scroll_bars = *buf
                .get(cur)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "ScrollBars overflow"))?;
            cur += 1;
        }
        if mask & (1 << 6) != 0 {
            ret.display_style = *buf.get(cur).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "DisplayStyle overflow")
            })?;
            cur += 1;
        }
        if mask & (1 << 7) != 0 {
            ret.mouse_pointer = *buf.get(cur).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "MousePointer overflow")
            })?;
            cur += 1;
        }
        let has_size = mask & (1 << 8) != 0;
        if mask & (1 << 9) != 0 {
            cur = cur + (cur & 1);
            ret.password_char = u16::from_le_bytes(
                buf.get(cur..cur + 2)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "PasswordChar overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 2;
        }
        if mask & (1 << 10) != 0 {
            cur = cur + ((4 - (cur & 3)) & 3);
            ret.list_width = u32::from_le_bytes(
                buf.get(cur..cur + 4)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "ListWidth overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 4;
        }
        if mask & (1 << 11) != 0 {
            cur = cur + (cur & 1);
            ret.bound_column = u16::from_le_bytes(
                buf.get(cur..cur + 2)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "BoundColumn overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 2;
        }
        if mask & (1 << 12) != 0 {
            cur = cur + (cur & 1);
            ret.text_column = i16::from_le_bytes(
                buf.get(cur..cur + 2)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "TextColumn overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 2;
        }
        if mask & (1 << 13) != 0 {
            cur = cur + (cur & 1);
            ret.column_count = i16::from_le_bytes(
                buf.get(cur..cur + 2)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "ColumnCount overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 2;
        }
        if mask & (1 << 14) != 0 {
            cur = cur + (cur & 1);
            ret.list_rows = u16::from_le_bytes(
                buf.get(cur..cur + 2)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "ListRows overflow"))?
                    .try_into()
                    .unwrap(),
            );
            cur += 2;
        }
        if mask & (1 << 15) != 0 {
            cur = cur + (cur & 1);
            ret.column_info = u16::from_le_bytes(
                buf.get(cur..cur + 2)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "ColumnInfo overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 2;
        }
        if mask & (1 << 16) != 0 {
            ret.match_entry = *buf
                .get(cur)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "MatchEntry overflow"))?;
            cur += 1;
        }
        if mask & (1 << 17) != 0 {
            ret.list_style = *buf
                .get(cur)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "ListStyle overflow"))?;
            cur += 1;
        }
        if mask & (1 << 18) != 0 {
            ret.drop_btn_when = *buf.get(cur).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "ShowDropButtonWhen overflow")
            })?;
            cur += 1;
        }
        // (1<<19) is unused
        if mask & (1 << 20) != 0 {
            ret.drop_btn_style = *buf.get(cur).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "DropButtonStyle overflow")
            })?;
            cur += 1;
        }
        if mask & (1 << 21) != 0 {
            ret.multi_select = *buf.get(cur).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "MultiSelect overflow")
            })?;
            cur += 1;
        }
        let value: Option<u32>;
        if mask & (1 << 22) != 0 {
            cur = cur + ((4 - (cur & 3)) & 3);
            value = Some(u32::from_le_bytes(
                buf.get(cur..cur + 4)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Value overflow"))?
                    .try_into()
                    .unwrap(),
            ));
            cur += 4;
        } else {
            value = None;
        }
        let caption: Option<u32>;
        if mask & (1 << 23) != 0 {
            cur = cur + ((4 - (cur & 3)) & 3);
            caption = Some(u32::from_le_bytes(
                buf.get(cur..cur + 4)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Caption overflow"))?
                    .try_into()
                    .unwrap(),
            ));
            cur += 4;
        } else {
            caption = None;
        }
        if mask & (1 << 24) != 0 {
            cur = cur + ((4 - (cur & 3)) & 3);
            ret.picture_position = u32::from_le_bytes(
                buf.get(cur..cur + 4)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "PicturePosition overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 4;
        }
        if mask & (1 << 25) != 0 {
            cur = cur + ((4 - (cur & 3)) & 3);
            ret.border_color = u32::from_le_bytes(
                buf.get(cur..cur + 4)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "BorderColor overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 4;
        }
        if mask & (1 << 26) != 0 {
            cur = cur + ((4 - (cur & 3)) & 3);
            ret.special_effect = u32::from_le_bytes(
                buf.get(cur..cur + 4)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "SpecialEffect overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 4;
        }
        let mouse_icon: Option<u16>;
        if mask & (1 << 27) != 0 {
            cur = cur + (cur & 1);
            mouse_icon = Some(u16::from_le_bytes(
                buf.get(cur..cur + 2)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "MouseIcon overflow")
                    })?
                    .try_into()
                    .unwrap(),
            ));
            cur += 2;
        } else {
            mouse_icon = None;
        }
        let picture: Option<u16>;
        if mask & (1 << 28) != 0 {
            cur = cur + (cur & 1);
            picture = Some(u16::from_le_bytes(
                buf.get(cur..cur + 2)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Picture overflow"))?
                    .try_into()
                    .unwrap(),
            ));
            cur += 2;
        } else {
            picture = None;
        }
        if mask & (1 << 29) != 0 {
            cur = cur + (cur & 1);
            ret.accelerator = u16::from_le_bytes(
                buf.get(cur..cur + 2)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "Accelerator overflow")
                    })?
                    .try_into()
                    .unwrap(),
            );
            cur += 2;
        }
        // (1<<30) is unused, (1<<31) is reserved
        let group: Option<u32>;
        if mask & (1 << 32) != 0 {
            cur = cur + ((4 - (cur & 3)) & 3);
            group = Some(u32::from_le_bytes(
                buf.get(cur..cur + 4)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Group overflow"))?
                    .try_into()
                    .unwrap(),
            ));
            cur += 4;
        } else {
            group = None;
        }

        // ExtraData
        cur = cur + ((4 - (cur & 3)) & 3);
        let extra_data = buf
            .get(cur..)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "ExtraData overflow"))?;
        cur = 0;

        if has_size {
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
        if let Some(cob) = value {
            if let Some((s, slen)) = get_cob_string(cob, extra_data.get(cur..)) {
                ret.value = s;
                cur += slen;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property Value overflow",
                ));
            }
        }
        if let Some(cob) = caption {
            if let Some((s, slen)) = get_cob_string(cob, extra_data.get(cur..)) {
                ret.caption = s;
                cur += slen;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property Caption overflow",
                ));
            }
        }
        if let Some(cob) = group {
            if let Some((s, _slen)) = get_cob_string(cob, extra_data.get(cur..)) {
                ret.group = s;
                //cur += _slen;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra property Group overflow",
                ));
            }
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
        // Picture
        if let Some(pic) = picture {
            if pic != 0xffff {
                ret.anomalies.push(format!(
                    "Invalid Picture value {} (instead of {})",
                    pic, 0xffff
                ));
            }
            ret.picture = Some(Picture::new(f)?);
        }

        // TextProps
        ret.font = Some(Font::text_props(f)?);

        Ok(ret)
    }

    fn set_various_properties(&mut self, vp: u32) {
        self.enabled = vp & (1 << 1) != 0;
        self.locked = vp & (1 << 2) != 0;
        self.opaque = vp & (1 << 3) != 0;
        self.column_heads = vp & (1 << 10) != 0;
        self.integral_height = vp & (1 << 11) != 0;
        self.match_required = vp & (1 << 12) != 0;
        self.left_aligned = vp & (1 << 13) != 0;
        self.editable = vp & (1 << 14) != 0;
        self.ime_mode = ((vp >> 15) & 0b1111) as u8;
        self.drag_behavior = vp & (1 << 19) != 0;
        self.enter_key_behaviour = vp & (1 << 20) != 0;
        self.enter_field_behaviour = vp & (1 << 21) != 0;
        self.tab_key_behaviour = vp & (1 << 22) != 0;
        self.word_wrap = vp & (1 << 23) != 0;
        self.border_suppress = vp & (1 << 25) != 0;
        self.selection_margin = vp & (1 << 26) != 0;
        self.auto_word_select = vp & (1 << 27) != 0;
        self.auto_size = vp & (1 << 28) != 0;
        self.hide_selection = vp & (1 << 29) != 0;
        self.auto_tab = vp & (1 << 30) != 0;
        self.multi_line = vp & (1 << 31) != 0;
    }
}

/// A CheckBox control
#[derive(Debug)]
pub struct CheckBox {
    /// Site control properties
    pub control: OleSiteConcreteControl,
    /// The control version
    pub version: u16,
    /// The control can receive the focus and respond to user-generated events
    pub enabled: bool,
    /// Specifies whether data in the control is locked for editing
    pub locked: bool,
    /// Specifies whether the control is opaque or transparent
    pub opaque: bool,
    /// Specifies that the Caption is to the left of the control
    pub left_aligned: bool,
    /// The default run-time mode of the Input Method Editor (IME)
    pub ime_mode: u8,
    /// Specifies whether the contents of the control automatically wrap at the end of a line
    pub word_wrap: bool,
    /// Specifies whether the control automatically resizes to display its entire contents
    pub auto_size: bool,
    /// Background color
    pub back_color: u32,
    /// Foreground color
    pub fore_color: u32,
    /// Type of icon displayed as the mouse pointer of the control
    pub mouse_pointer: u8,
    /// Width and height of the control, in HIMETRIC units
    pub size: (i32, i32),
    /// Specifies whether the control permits multiple selections (0:single, 1:multi, 2:extended)
    pub multi_select: u8,
    /// The state of the control ("0":unchecked, "1":checked)
    pub value: String,
    /// The caption of the control
    pub caption: String,
    /// The location of the picture of a control relative to the caption of the control
    pub picture_position: u32,
    /// Specifies the visual appearance of the control
    pub special_effect: u32,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// The (optional) picture to display on the control
    pub picture: Option<Picture>,
    /// The accelerator key
    pub accelerator: u16,
    /// The group of mutually exclusive controls this control belongs to
    pub group: String,
    /// Font properties
    pub font: Option<Font>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl CheckBox {
    pub(crate) fn new<R: Read + Seek>(
        base: &OleSiteConcreteControl,
        f: &mut R,
    ) -> Result<Self, io::Error> {
        let mut morph = Morph::new(f)?;
        if morph.display_style != 4 {
            morph.anomalies.push(format!(
                "DisplayStyle should be 4 but it's set to {}",
                morph.display_style
            ));
        }
        if !morph.integral_height {
            morph
                .anomalies
                .push("IntegralHeight should be set but it's not".to_string());
        }
        if !morph.selection_margin {
            morph
                .anomalies
                .push("SelectionMargin should be set but it's not".to_string());
        }
        if !morph.auto_word_select {
            morph
                .anomalies
                .push("AutoWordSelect should be set but it's not".to_string());
        }
        if !morph.hide_selection {
            morph
                .anomalies
                .push("HideSelection should be set but it's not".to_string());
        }
        Ok(Self {
            control: base.clone(),
            version: morph.version,
            enabled: morph.enabled,
            locked: morph.locked,
            opaque: morph.opaque,
            left_aligned: morph.left_aligned,
            ime_mode: morph.ime_mode,
            word_wrap: morph.word_wrap,
            auto_size: morph.auto_size,
            back_color: morph.back_color,
            fore_color: morph.fore_color,
            mouse_pointer: morph.mouse_pointer,
            size: morph.size,
            multi_select: morph.multi_select,
            value: morph.value,
            caption: morph.caption,
            picture_position: morph.picture_position,
            special_effect: morph.special_effect,
            mouse_icon: morph.mouse_icon,
            picture: morph.picture,
            accelerator: morph.accelerator,
            group: morph.group,
            font: morph.font,
            anomalies: morph.anomalies,
        })
    }
}

impl ChildControl for CheckBox {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

/// A ComboBox control
#[derive(Debug)]
pub struct ComboBox {
    /// Site control properties
    pub control: OleSiteConcreteControl,
    /// The control version
    pub version: u16,
    /// The control can receive the focus and respond to user-generated events
    pub enabled: bool,
    /// Specifies whether data in the control is locked for editing
    pub locked: bool,
    /// Specifies whether the control is opaque or transparent
    pub opaque: bool,
    /// Specifies whether column headings are displayed
    pub column_heads: bool,
    /// Specifies whether the entered value must match an entry in the ListBox part of the control
    pub match_required: bool,
    /// Specifies whether the user can type into the control
    pub editable: bool,
    /// The default run-time mode of the Input Method Editor (IME)
    pub ime_mode: u8,
    /// Specifies whether dragging and dropping is enabled for the control
    pub drag_behavior: bool,
    /// Specifies if the selected text is unchanged when entering the control
    pub enter_field_behaviour: bool,
    /// Specifies whether the contents of the control automatically wrap at the end of a line
    pub word_wrap: bool,
    /// Specifies whether the user can select a line of text by clicking
    /// in the region to the left of the text
    pub selection_margin: bool,
    /// Automatically extends the selection to a whole word (rather than a single char)
    pub auto_word_select: bool,
    /// Specifies whether the control automatically resizes to display its entire contents
    pub auto_size: bool,
    /// Hide selection when the control is not focused
    pub hide_selection: bool,
    /// Specifies whether the focus automatically moves to the next control when the limit is reached
    pub auto_tab: bool,
    /// Background color
    pub back_color: u32,
    /// Foreground color
    pub fore_color: u32,
    /// The maximum number of characters that a user can enter
    pub max_length: u32,
    /// Type of border used by the control (0: none, 1: single)
    pub border_style: u8,
    /// Type of icon displayed as the mouse pointer of the control
    pub mouse_pointer: u8,
    /// Width and height of the control, in HIMETRIC units
    pub size: (i32, i32),
    /// Width, in HIMETRIC units, of the ListBox part of the control (0: same)
    pub list_width: u32,
    /// Number of the column from which the value is retrieved
    pub bound_column: u16,
    /// Number of the column from which the displayed text is retrieved
    pub text_column: i16,
    /// Number of columns to display
    pub column_count: i16,
    /// Maximum number of rows to display
    pub list_rows: u16,
    /// The last column with a non-default width
    pub column_info: u16,
    /// How to search for entries (0: first letter, 1: all, 2: do not search)
    pub match_entry: u8,
    /// Visual appearance of the list (0: plain, 1: with side buttons)
    pub list_style: u8,
    /// Specifies when to show the drop button (0: never, 1: when focused, 2: always)
    pub drop_btn_when: u8,
    /// The symbol to display on the drop down button
    pub drop_btn_style: u8,
    /// The value of the currently selected row
    pub value: String,
    /// The color of the border of the control
    pub border_color: u32,
    /// Specifies the visual appearance of the control
    pub special_effect: u32,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// Font properties
    pub font: Option<Font>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl ComboBox {
    pub(crate) fn new<R: Read + Seek>(
        base: &OleSiteConcreteControl,
        f: &mut R,
    ) -> Result<Self, io::Error> {
        let mut morph = Morph::new(f)?;
        if morph.display_style != 3 && morph.display_style != 7 {
            morph.anomalies.push(format!(
                "DisplayStyle should be 3 or 7 but it's set to {}",
                morph.display_style
            ));
        }
        if morph.display_style == 3 && !morph.editable {
            morph
                .anomalies
                .push("Editable should be set but it's not".to_string());
        }
        if !morph.word_wrap {
            morph
                .anomalies
                .push("WordWrap should be set but it's not".to_string());
        }

        skip_rg_column_info(f, morph.column_info)?;

        Ok(Self {
            control: base.clone(),
            version: morph.version,
            enabled: morph.enabled,
            locked: morph.locked,
            opaque: morph.opaque,
            column_heads: morph.column_heads,
            match_required: morph.match_required,
            editable: morph.editable,
            ime_mode: morph.ime_mode,
            drag_behavior: morph.drag_behavior,
            enter_field_behaviour: morph.enter_field_behaviour,
            word_wrap: morph.word_wrap,
            selection_margin: morph.selection_margin,
            auto_word_select: morph.auto_word_select,
            auto_size: morph.auto_size,
            hide_selection: morph.hide_selection,
            auto_tab: morph.auto_tab,
            back_color: morph.back_color,
            fore_color: morph.fore_color,
            max_length: morph.max_length,
            border_style: morph.border_style,
            mouse_pointer: morph.mouse_pointer,
            size: morph.size,
            list_width: morph.list_width,
            bound_column: morph.bound_column,
            text_column: morph.text_column,
            column_count: morph.column_count,
            list_rows: morph.list_rows,
            column_info: morph.column_info,
            match_entry: morph.match_entry,
            list_style: morph.list_style,
            drop_btn_when: morph.drop_btn_when,
            drop_btn_style: morph.drop_btn_style,
            value: morph.value,
            border_color: morph.border_color,
            special_effect: morph.special_effect,
            mouse_icon: morph.mouse_icon,
            font: morph.font,
            anomalies: morph.anomalies,
        })
    }
}

impl ChildControl for ComboBox {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

/// A ListBox control
#[derive(Debug)]
pub struct ListBox {
    /// Site control properties
    pub control: OleSiteConcreteControl,
    /// The control version
    pub version: u16,
    /// The control can receive the focus and respond to user-generated events
    pub enabled: bool,
    /// Specifies whether data in the control is locked for editing
    pub locked: bool,
    /// Specifies whether the control is opaque or transparent
    pub opaque: bool,
    /// Specifies whether column headings are displayed
    pub column_heads: bool,
    /// Whether the control shows only complete lines of text
    pub integral_height: bool,
    /// The default run-time mode of the Input Method Editor (IME)
    pub ime_mode: u8,
    /// Specifies whether the contents of the control automatically wrap at the end of a line
    pub word_wrap: bool,
    /// Specifies whether the user can select a line of text by clicking
    /// in the region to the left of the text
    pub selection_margin: bool,
    /// Automatically extends the selection to a whole word (rather than a single char)
    pub auto_word_select: bool,
    /// Hide selection when the control is not focused
    pub hide_selection: bool,
    /// Background color
    pub back_color: u32,
    /// Foreground color
    pub fore_color: u32,
    /// Type of border used by the control (0: none, 1: single)
    pub border_style: u8,
    /// Whether the control has vertical scroll bars (0: none, 1: horiz, 2: vert, 3: both)
    pub scroll_bars: u8,
    /// Type of icon displayed as the mouse pointer of the control
    pub mouse_pointer: u8,
    /// Width and height of the control, in HIMETRIC units
    pub size: (i32, i32),
    /// Width, in HIMETRIC units, of the ListBox part of the control (0: same)
    pub list_width: u32,
    /// Number of the column from which the value is retrieved
    pub bound_column: u16,
    /// Number of the column from which the displayed text is retrieved
    pub text_column: i16,
    /// Number of columns to display
    pub column_count: i16,
    /// The last column with a non-default width
    pub column_info: u16,
    /// How to search for entries (0: first letter, 1: all, 2: do not search)
    pub match_entry: u8,
    /// Visual appearance of the list (0: plain, 1: with side buttons)
    pub list_style: u8,
    /// The value of the currently selected row
    pub value: String,
    /// The color of the border of the control
    pub border_color: u32,
    /// Specifies the visual appearance of the control
    pub special_effect: u32,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// Font properties
    pub font: Option<Font>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl ListBox {
    pub(crate) fn new<R: Read + Seek>(
        base: &OleSiteConcreteControl,
        f: &mut R,
    ) -> Result<Self, io::Error> {
        let mut morph = Morph::new(f)?;
        if morph.display_style != 2 {
            morph.anomalies.push(format!(
                "DisplayStyle should be 2 but it's set to {}",
                morph.display_style
            ));
        }
        if !morph.opaque {
            morph
                .anomalies
                .push("BackStyle should be set but it's not".to_string());
        }
        if !morph.word_wrap {
            morph
                .anomalies
                .push("WordWrap should be set but it's not".to_string());
        }
        if !morph.selection_margin {
            morph
                .anomalies
                .push("SelectionMargin should be set but it's not".to_string());
        }
        if !morph.auto_word_select {
            morph
                .anomalies
                .push("AutoWordSelect should be set but it's not".to_string());
        }
        if !morph.hide_selection {
            morph
                .anomalies
                .push("HideSelection should be set but it's not".to_string());
        }

        skip_rg_column_info(f, morph.column_info)?;

        Ok(Self {
            control: base.clone(),
            version: morph.version,
            enabled: morph.enabled,
            locked: morph.locked,
            opaque: morph.opaque,
            column_heads: morph.column_heads,
            integral_height: morph.integral_height,
            ime_mode: morph.ime_mode,
            word_wrap: morph.word_wrap,
            selection_margin: morph.selection_margin,
            auto_word_select: morph.auto_word_select,
            hide_selection: morph.hide_selection,
            back_color: morph.back_color,
            fore_color: morph.fore_color,
            border_style: morph.border_style,
            scroll_bars: morph.scroll_bars,
            mouse_pointer: morph.mouse_pointer,
            size: morph.size,
            list_width: morph.list_width,
            bound_column: morph.bound_column,
            text_column: morph.text_column,
            column_count: morph.column_count,
            column_info: morph.column_info,
            match_entry: morph.match_entry,
            list_style: morph.list_style,
            value: morph.value,
            border_color: morph.border_color,
            special_effect: morph.special_effect,
            mouse_icon: morph.mouse_icon,
            font: morph.font,
            anomalies: morph.anomalies,
        })
    }
}

fn skip_rg_column_info<R: Read + Seek>(f: &mut R, cols: u16) -> Result<(), io::Error> {
    // Skip rgColumnInfo
    for _ in 0..cols {
        let size = rdu32le(f)? >> 16;
        f.seek(io::SeekFrom::Current(size.into()))?;
    }
    Ok(())
}

impl ChildControl for ListBox {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

/// An OptionButton control
#[derive(Debug)]
pub struct OptionButton {
    /// Site control properties
    pub control: OleSiteConcreteControl,
    /// The control version
    pub version: u16,
    /// The control can receive the focus and respond to user-generated events
    pub enabled: bool,
    /// Specifies whether data in the control is locked for editing
    pub locked: bool,
    /// Specifies whether the control is opaque or transparent
    pub opaque: bool,
    /// Specifies that the Caption is to the left of the control
    pub left_aligned: bool,
    /// The default run-time mode of the Input Method Editor (IME)
    pub ime_mode: u8,
    /// Specifies whether the contents of the control automatically wrap at the end of a line
    pub word_wrap: bool,
    /// Specifies whether the control automatically resizes to display its entire contents
    pub auto_size: bool,
    /// Background color
    pub back_color: u32,
    /// Foreground color
    pub fore_color: u32,
    /// Type of icon displayed as the mouse pointer of the control
    pub mouse_pointer: u8,
    /// Width and height of the control, in HIMETRIC units
    pub size: (i32, i32),
    /// Specifies whether the control permits multiple selections (0:single, 1:multi, 2:extended)
    pub multi_select: u8,
    /// The state of the control ("0":unchecked, "1":checked)
    pub value: String,
    /// The caption of the control
    pub caption: String,
    /// The location of the picture of a control relative to the caption of the control
    pub picture_position: u32,
    /// Specifies the visual appearance of the control
    pub special_effect: u32,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// The (optional) picture to display on the control
    pub picture: Option<Picture>,
    /// The accelerator key
    pub accelerator: u16,
    /// The group of mutually exclusive controls this control belongs to
    pub group: String,
    /// Font properties
    pub font: Option<Font>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl OptionButton {
    pub(crate) fn new<R: Read + Seek>(
        base: &OleSiteConcreteControl,
        f: &mut R,
    ) -> Result<Self, io::Error> {
        let mut morph = Morph::new(f)?;
        if morph.display_style != 5 {
            morph.anomalies.push(format!(
                "DisplayStyle should be 5 but it's set to {}",
                morph.display_style
            ));
        }
        if !morph.integral_height {
            morph
                .anomalies
                .push("IntegralHeight should be set but it's not".to_string());
        }
        if !morph.selection_margin {
            morph
                .anomalies
                .push("SelectionMargin should be set but it's not".to_string());
        }
        if !morph.auto_word_select {
            morph
                .anomalies
                .push("AutoWordSelect should be set but it's not".to_string());
        }
        if !morph.hide_selection {
            morph
                .anomalies
                .push("HideSelection should be set but it's not".to_string());
        }
        Ok(Self {
            control: base.clone(),
            version: morph.version,
            enabled: morph.enabled,
            locked: morph.locked,
            opaque: morph.opaque,
            left_aligned: morph.left_aligned,
            ime_mode: morph.ime_mode,
            word_wrap: morph.word_wrap,
            auto_size: morph.auto_size,
            back_color: morph.back_color,
            fore_color: morph.fore_color,
            mouse_pointer: morph.mouse_pointer,
            size: morph.size,
            multi_select: morph.multi_select,
            value: morph.value,
            caption: morph.caption,
            picture_position: morph.picture_position,
            special_effect: morph.special_effect,
            mouse_icon: morph.mouse_icon,
            picture: morph.picture,
            accelerator: morph.accelerator,
            group: morph.group,
            font: morph.font,
            anomalies: morph.anomalies,
        })
    }
}

impl ChildControl for OptionButton {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

/// A TextBox control
#[derive(Debug)]
pub struct TextBox {
    /// Site control properties
    pub control: OleSiteConcreteControl,
    /// The control version
    pub version: u16,
    /// The control can receive the focus and respond to user-generated events
    pub enabled: bool,
    /// Specifies whether data in the control is locked for editing
    pub locked: bool,
    /// Specifies whether the control is opaque or transparent
    pub opaque: bool,
    /// Whether the control shows only complete lines of text
    pub integral_height: bool,
    /// The default run-time mode of the Input Method Editor (IME)
    pub ime_mode: u8,
    /// Specifies whether dragging and dropping is enabled for the control
    pub drag_behavior: bool,
    /// Specifies if the ENTER key creates a newline (true) or if it moves the focus (false)
    pub enter_key_behaviour: bool,
    /// Specifies if the selected text is unchanged when entering the control
    pub enter_field_behaviour: bool,
    /// Specifies if the TAB key inputs a tab character (true) or if it moves the focus (false)
    pub tab_key_behaviour: bool,
    /// Specifies whether the contents of the control automatically wrap at the end of a line
    pub word_wrap: bool,
    /// Specifies whether the user can select a line of text by clicking
    /// in the region to the left of the text
    pub selection_margin: bool,
    /// Automatically extends the selection to a whole word (rather than a single char)
    pub auto_word_select: bool,
    /// Specifies whether the control automatically resizes to display its entire contents
    pub auto_size: bool,
    /// Hide selection when the control is not focused
    pub hide_selection: bool,
    /// Specifies whether the focus automatically moves to the next control when the limit is reached
    pub auto_tab: bool,
    /// Specifies whether the control can display more than one line of text
    pub multi_line: bool,
    /// Background color
    pub back_color: u32,
    /// Foreground color
    pub fore_color: u32,
    /// The maximum number of characters that a user can enter
    pub max_length: u32,
    /// Type of border used by the control (0: none, 1: single)
    pub border_style: u8,
    /// Whether the control has vertical scroll bars (0: none, 1: horiz, 2: vert, 3: both)
    pub scroll_bars: u8,
    /// Type of icon displayed as the mouse pointer of the control
    pub mouse_pointer: u8,
    /// Width and height of the control, in HIMETRIC units
    pub size: (i32, i32),
    /// Character to be displayed in place of the characters entered (0: don't mask)
    pub password_char: u16,
    /// The text in the control
    pub value: String,
    /// The color of the border of the control
    pub border_color: u32,
    /// Specifies the visual appearance of the control
    pub special_effect: u32,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// Font properties
    pub font: Option<Font>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl TextBox {
    pub(crate) fn new<R: Read + Seek>(
        base: &OleSiteConcreteControl,
        f: &mut R,
    ) -> Result<Self, io::Error> {
        let mut morph = Morph::new(f)?;
        if morph.display_style != 1 {
            morph.anomalies.push(format!(
                "DisplayStyle should be 1 but it's set to {}",
                morph.display_style
            ));
        }
        if !morph.editable {
            morph
                .anomalies
                .push("Editable should be set but it's not".to_string());
        }

        Ok(Self {
            control: base.clone(),
            version: morph.version,
            enabled: morph.enabled,
            locked: morph.locked,
            opaque: morph.opaque,
            integral_height: morph.integral_height,
            ime_mode: morph.ime_mode,
            drag_behavior: morph.drag_behavior,
            enter_key_behaviour: morph.enter_key_behaviour,
            enter_field_behaviour: morph.enter_field_behaviour,
            tab_key_behaviour: morph.tab_key_behaviour,
            word_wrap: morph.word_wrap,
            selection_margin: morph.selection_margin,
            auto_word_select: morph.auto_word_select,
            auto_size: morph.auto_size,
            hide_selection: morph.hide_selection,
            auto_tab: morph.auto_tab,
            multi_line: morph.multi_line,
            back_color: morph.back_color,
            fore_color: morph.fore_color,
            max_length: morph.max_length,
            border_style: morph.border_style,
            scroll_bars: morph.scroll_bars,
            mouse_pointer: morph.mouse_pointer,
            size: morph.size,
            password_char: morph.password_char,
            value: morph.value,
            border_color: morph.border_color,
            special_effect: morph.special_effect,
            mouse_icon: morph.mouse_icon,
            font: morph.font,
            anomalies: morph.anomalies,
        })
    }
}

impl ChildControl for TextBox {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

/// A ToggleButton control
#[derive(Debug)]
pub struct ToggleButton {
    /// Site control properties
    pub control: OleSiteConcreteControl,
    /// The control version
    pub version: u16,
    /// The control can receive the focus and respond to user-generated events
    pub enabled: bool,
    /// Specifies whether data in the control is locked for editing
    pub locked: bool,
    /// Specifies whether the control is opaque or transparent
    pub opaque: bool,
    /// The default run-time mode of the Input Method Editor (IME)
    pub ime_mode: u8,
    /// Specifies whether the contents of the control automatically wrap at the end of a line
    pub word_wrap: bool,
    /// Specifies whether the control automatically resizes to display its entire contents
    pub auto_size: bool,
    /// Background color
    pub back_color: u32,
    /// Foreground color
    pub fore_color: u32,
    /// Type of icon displayed as the mouse pointer of the control
    pub mouse_pointer: u8,
    /// Width and height of the control, in HIMETRIC units
    pub size: (i32, i32),
    /// Specifies whether the control permits multiple selections (0:single, 1:multi, 2:extended)
    pub multi_select: u8,
    /// The state of the control ("0":unchecked, "1":checked)
    pub value: String,
    /// The caption of the control
    pub caption: String,
    /// The location of the picture of a control relative to the caption of the control
    pub picture_position: u32,
    /// Specifies the visual appearance of the control
    pub special_effect: u32,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// The (optional) picture to display on the control
    pub picture: Option<Picture>,
    /// The accelerator key
    pub accelerator: u16,
    /// Font properties
    pub font: Option<Font>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl ToggleButton {
    pub(crate) fn new<R: Read + Seek>(
        base: &OleSiteConcreteControl,
        f: &mut R,
    ) -> Result<Self, io::Error> {
        let mut morph = Morph::new(f)?;
        if morph.display_style != 6 {
            morph.anomalies.push(format!(
                "DisplayStyle should be 6 but it's set to {}",
                morph.display_style
            ));
        }
        if !morph.integral_height {
            morph
                .anomalies
                .push("IntegralHeight should be set but it's not".to_string());
        }
        if !morph.selection_margin {
            morph
                .anomalies
                .push("SelectionMargin should be set but it's not".to_string());
        }
        if !morph.auto_word_select {
            morph
                .anomalies
                .push("AutoWordSelect should be set but it's not".to_string());
        }
        if !morph.hide_selection {
            morph
                .anomalies
                .push("HideSelection should be set but it's not".to_string());
        }
        Ok(Self {
            control: base.clone(),
            version: morph.version,
            enabled: morph.enabled,
            locked: morph.locked,
            opaque: morph.opaque,
            ime_mode: morph.ime_mode,
            word_wrap: morph.word_wrap,
            auto_size: morph.auto_size,
            back_color: morph.back_color,
            fore_color: morph.fore_color,
            mouse_pointer: morph.mouse_pointer,
            size: morph.size,
            multi_select: morph.multi_select,
            value: morph.value,
            caption: morph.caption,
            picture_position: morph.picture_position,
            special_effect: morph.special_effect,
            mouse_icon: morph.mouse_icon,
            picture: morph.picture,
            accelerator: morph.accelerator,
            font: morph.font,
            anomalies: morph.anomalies,
        })
    }
}

impl ChildControl for ToggleButton {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};
    #[test]
    fn test_morph_oddmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x34, 0x00, // cbImage
            0b10101010, 0b10101010, 0b10101010, 0b10101010, // Mask1
            0b10101010, 0b10101010, 0b10101010, 0b10101010, // Mask2

            0x11, 0x22, 0x33, 0x44, // back color
            0x34, 0x12, 0x20, 0x10, // max len
            0x03, // scroll bars
            0x63, // mouse pointer
            0x2a, 0x00, // passwd char
            0x20, 0x01, // bound column
            0x0a, 0x00, // col count
            0x03, 0x00, // col info
            0x01, // list style
            0x02, // multi select
            0x0a, 0x00, 0x00, 0x00, // caption
            0xc0, 0xe2, 0x2d, 0xb0, // border color
            0xff, 0xff, // mouse icon
            0xab, 0xac, // accel

            // ExtraData
            0x4d, 0x00, 0x6f, 0x00, 0x72, 0x00, 0x70, 0x00, 0x68, 0x00, // caption
            0x00, 0x00, // pad

            // StreamData
            // MouseIcon
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // mouse icon
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, // guid
            0x6c, 0x74, 0x00, 0x00, // icon preamble
            0x08, 0x00, 0x00, 0x00, // icon size
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // "icon"

            // TextProps
            0x00, 0x02, // Version
            0x04, 0x00, // cbImage
            0x00, 0x00, 0x00, 0x00, // Mask
        ];

        let mut f = Cursor::new(buf);
        let morph = Morph::new(&mut f)?;
        assert_eq!(morph.version, 0x200);
        assert_eq!(morph.back_color, 0x44332211);
        assert_eq!(morph.fore_color, 0x80000008);
        assert_eq!(morph.max_length, 0x10201234);
        assert_eq!(morph.border_style, 0);
        assert_eq!(morph.scroll_bars, 0x03);
        assert_eq!(morph.display_style, 1);
        assert_eq!(morph.mouse_pointer, 0x63);
        assert_eq!(morph.size, (0, 0));
        assert_eq!(morph.password_char, 0x2a);
        assert_eq!(morph.list_width, 0);
        assert_eq!(morph.bound_column, 0x120);
        assert_eq!(morph.text_column, -1);
        assert_eq!(morph.column_count, 0x0a);
        assert_eq!(morph.list_rows, 8);
        assert_eq!(morph.column_info, 3);
        assert_eq!(morph.match_entry, 2);
        assert_eq!(morph.list_style, 1);
        assert_eq!(morph.drop_btn_when, 0);
        assert_eq!(morph.drop_btn_style, 1);
        assert_eq!(morph.multi_select, 2);
        assert_eq!(morph.value, "");
        assert_eq!(morph.caption, "Morph");
        assert_eq!(morph.picture_position, 0x00070001);
        assert_eq!(morph.border_color, 0xb02de2c0);
        assert_eq!(morph.special_effect, 2);
        assert_eq!(morph.mouse_icon.unwrap().picture, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(morph.picture.is_none());
        assert_eq!(morph.accelerator, 0xacab);
        assert_eq!(morph.group, "");
        assert!(morph.font.is_some());
        assert_eq!(morph.enabled, true);
        assert_eq!(morph.locked, false);
        assert_eq!(morph.opaque, true);
        assert_eq!(morph.column_heads, false);
        assert_eq!(morph.integral_height, true);
        assert_eq!(morph.match_required, false);
        assert_eq!(morph.left_aligned, false);
        assert_eq!(morph.editable, false);
        assert_eq!(morph.ime_mode, 0);
        assert_eq!(morph.drag_behavior, false);
        assert_eq!(morph.enter_key_behaviour, false);
        assert_eq!(morph.enter_field_behaviour, false);
        assert_eq!(morph.tab_key_behaviour, false);
        assert_eq!(morph.word_wrap, true);
        assert_eq!(morph.border_suppress, false);
        assert_eq!(morph.selection_margin, true);
        assert_eq!(morph.auto_word_select, true);
        assert_eq!(morph.auto_size, false);
        assert_eq!(morph.hide_selection, true);
        assert_eq!(morph.auto_tab, false);
        assert_eq!(morph.multi_line, false);

        Ok(())
    }

    #[test]
    fn test_morph_evenmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x48, 0x00, // cbImage
            0b01010101, 0b01010101, 0b01010101, 0b01010101, // Mask1
            0b01010101, 0b01010101, 0b01010101, 0b01010101, // Mask2

            0xe4, 0xf7, 0x7f, 0xd3, // various properties
            0x11, 0x22, 0x33, 0x44, // fore color
            0x01, // border style
            0x05, // display style
            0x00, 0x00, // pad
            0x44, 0x33, 0x22, 0x11, // list width
            0x05, 0x00, // text column
            0x10, 0x00, // list rows
            0x00, // match entry
            0x02, // show drop btn when
            0x03, // drop btn style
            0x00, // pad
            0x01, 0x00, 0x00, 0x80, // value
            0x04, 0x00, 0x04, 0x00, // pic position
            0x00, 0x00, 0x00, 0x00, // special effect
            0xff, 0xff, // picture
            0x00, 0x00, // pad
            0x07, 0x00, 0x00, 0x80, // value

            // ExtraData
            0x01, 0x00, 0x00, 0x00, // size w
            0x02, 0x00, 0x00, 0x00, // size h
            0x31, // value
            0x00, 0x00, 0x00, // pad
            0x47, 0x72, 0x6f, 0x75, 0x70, 0x23, 0x31, // group
            0x00, // pad


            // StreamData
            // Picture
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // picture
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, // guid
            0x6c, 0x74, 0x00, 0x00, // picture preamble
            0x04, 0x00, 0x00, 0x00, // picture size
            0x0a, 0x0b, 0x0c, 0x0d, // "picture"

            // TextProps
            0x00, 0x02, // Version
            0x04, 0x00, // cbImage
            0x00, 0x00, 0x00, 0x00, // Mask
        ];
        let mut f = Cursor::new(buf);
        let morph = Morph::new(&mut f)?;
        assert_eq!(morph.version, 0x200);
        assert_eq!(morph.back_color, 0x80000005);
        assert_eq!(morph.fore_color, 0x44332211);
        assert_eq!(morph.max_length, 0);
        assert_eq!(morph.border_style, 1);
        assert_eq!(morph.scroll_bars, 0);
        assert_eq!(morph.display_style, 5);
        assert_eq!(morph.mouse_pointer, 0);
        assert_eq!(morph.size, (1, 2));
        assert_eq!(morph.password_char, 0);
        assert_eq!(morph.list_width, 0x11223344);
        assert_eq!(morph.bound_column, 1);
        assert_eq!(morph.text_column, 5);
        assert_eq!(morph.column_count, 1);
        assert_eq!(morph.list_rows, 16);
        assert_eq!(morph.column_info, 0);
        assert_eq!(morph.match_entry, 0);
        assert_eq!(morph.list_style, 0);
        assert_eq!(morph.drop_btn_when, 2);
        assert_eq!(morph.drop_btn_style, 3);
        assert_eq!(morph.multi_select, 0);
        assert_eq!(morph.value, "1");
        assert_eq!(morph.caption, "");
        assert_eq!(morph.picture_position, 0x00040004);
        assert_eq!(morph.border_color, 0x80000006);
        assert_eq!(morph.special_effect, 0);
        assert!(morph.mouse_icon.is_none());
        assert_eq!(morph.picture.unwrap().picture, [10, 11, 12, 13]);
        assert_eq!(morph.accelerator, 0);
        assert_eq!(morph.group, "Group#1");
        assert!(morph.font.is_some());
        assert_eq!(morph.enabled, false);
        assert_eq!(morph.locked, true);
        assert_eq!(morph.opaque, false);
        assert_eq!(morph.column_heads, true);
        assert_eq!(morph.integral_height, false);
        assert_eq!(morph.match_required, true);
        assert_eq!(morph.left_aligned, true);
        assert_eq!(morph.editable, true);
        assert_eq!(morph.ime_mode, 0b1111);
        assert_eq!(morph.drag_behavior, true);
        assert_eq!(morph.enter_key_behaviour, true);
        assert_eq!(morph.enter_field_behaviour, true);
        assert_eq!(morph.tab_key_behaviour, true);
        assert_eq!(morph.word_wrap, false);
        assert_eq!(morph.border_suppress, true);
        assert_eq!(morph.selection_margin, false);
        assert_eq!(morph.auto_word_select, false);
        assert_eq!(morph.auto_size, true);
        assert_eq!(morph.hide_selection, false);
        assert_eq!(morph.auto_tab, true);
        assert_eq!(morph.multi_line, true);
        Ok(())
    }

    #[test]
    fn test_cb() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: &[u8] = &[
            0x00,0x02, // ver
            0x3c,0x00, // size
            0x47,0x01,0xc0,0x80, // mask 1
            0x00,0x00,0x00,0x00, // mask 2

            // ExtraData
            0x19,0x08,0x80,0x2c, // vp
            0x0f,0x00,0x00,0x80, // back color
            0x12,0x00,0x00,0x80, // fore color
            0x04, // display style
            0x00,0x00,0x00, // pad
            0x01,0x00,0x00,0x80, // value (1 compressed)
            0x0e,0x00,0x00,0x80, // caption

            // StreamData
            0xe2,0x0e,0x00,0x00, // size w
            0x7b,0x02,0x00,0x00, // size h
            0x30, // value
            0x61,0x68,0x6f, // pad
            0x43,0x61,0x6e,0x27, 0x74,0x20,0x63,0x6c, 0x69,0x63,0x6b,0x20, 0x6d,0x65, // caption
            0x00,0x00, // pad

            // TextProps
            0x00,0x02, // ver
            0x1c,0x00, // size
            0x37,0x00,0x00,0x00, // mask
            0x06,0x00,0x00,0x80, // name
            0x00,0x20,0x00,0x40, // effects
            0xa5,0x00,0x00,0x00, // height
            0x00, // charset
            0x02, // pitch+family
            0x00,0x00, // pad
            0x54,0x61,0x68,0x6f,0x6d,0x61, // Tahoma
            0x00,0x00, // pad
        ];
        let mut f = Cursor::new(buf);
        let cb = CheckBox::new(&OleSiteConcreteControl::default(), &mut f)?;
        assert_eq!(cb.enabled, false);
        assert_eq!(cb.locked, false);
        assert_eq!(cb.opaque, true);
        assert_eq!(cb.left_aligned, false);
        assert_eq!(cb.ime_mode, 0);
        assert_eq!(cb.word_wrap, true);
        assert_eq!(cb.auto_size, false);
        assert_eq!(cb.back_color, 0x8000000f);
        assert_eq!(cb.fore_color, 0x80000012);
        assert_eq!(cb.mouse_pointer, 0);
        assert_eq!(cb.size, (0xee2, 0x27b));
        assert_eq!(cb.multi_select, 0);
        assert_eq!(cb.value, "0");
        assert_eq!(cb.caption, "Can't click me");
        assert_eq!(cb.picture_position, 0x00070001);
        assert_eq!(cb.special_effect, 2);
        assert!(cb.mouse_icon.is_none());
        assert!(cb.picture.is_none());
        assert_eq!(cb.accelerator, 0);
        assert!(cb.group.is_empty());
        assert_eq!(cb.font.as_ref().unwrap().name, "Tahoma");
        Ok(())
    }
}
