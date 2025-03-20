use crate::forms::controls::{ChildControl, OleSiteConcreteControl};
use crate::forms::{Font, Picture, mask::*};
use ctxutils::io::*;
use std::io::{self, Read, Seek};

/// CommandButton control
#[derive(Debug)]
pub struct CommandButton {
    /// Site control properties
    pub control: OleSiteConcreteControl,
    /// The control version
    pub version: u16,
    /// Foreground color
    pub fore_color: u32,
    /// Background color
    pub back_color: u32,
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
    /// The caption of the control
    pub caption: String,
    /// The location of the picture of a control relative to the caption of the control
    pub picture_position: u32,
    /// Width and height of the control, in HIMETRIC units
    pub size: (i32, i32),
    /// Type of icon displayed as the mouse pointer of the control
    pub mouse_pointer: u8,
    /// The (optional) picture to display on the form (`Picture`)
    pub picture: Option<Picture>,
    /// The accelerator key
    pub accelerator: u16,
    /// Specifies whether the control takes the focus when clicked
    pub focus_on_click: bool,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// Font properties
    pub font: Option<Font>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl Default for CommandButton {
    fn default() -> Self {
        Self {
            control: OleSiteConcreteControl::default(),
            version: 0,
            fore_color: 0x80000012,
            back_color: 0x8000000f,
            enabled: true,
            locked: false,
            opaque: false,
            ime_mode: 0,
            word_wrap: false,
            auto_size: false,
            caption: String::new(),
            picture_position: 0x00070001,
            size: (0, 0),
            mouse_pointer: 0,
            picture: None,
            accelerator: 0,
            focus_on_click: true,
            mouse_icon: None,
            font: None,
            anomalies: Vec::new(),
        }
    }
}

impl CommandButton {
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

        let mut various_properties: u32 = 0x0000001b;
        let mut caption: Option<u32> = None;
        let mut has_size: Option<()> = None;
        let mut picture: Option<u16> = None;
        let mut mouse_icon: Option<u16> = None;

        let mask: PropertyMask = [
            property_mask_bit!(ret.fore_color),
            property_mask_bit!(ret.back_color),
            property_mask_bit!(various_properties),
            property_mask_bit!(caption),
            property_mask_bit!(ret.picture_position),
            property_mask_bit!(has_size),
            property_mask_bit!(ret.mouse_pointer),
            property_mask_bit!(picture),
            property_mask_bit!(ret.accelerator),
            property_mask_bit!(ret.focus_on_click),
            property_mask_bit!(mouse_icon),
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

        let extra_data = set_data_properties(f, &mask)?;
        ret.set_various_properties(various_properties);

        // ExtraData
        let mut cur = 0usize;
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
        if has_size.is_some() {
            let w = extra_data.get(cur..cur + 4);
            cur += 4;
            let h = extra_data.get(cur..cur + 4);
            //cur += 4;
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

        // StreamData
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
        ret.font = Some(Font::text_props(f)?);
        Ok(ret)
    }

    fn set_various_properties(&mut self, vp: u32) {
        self.enabled = (vp & (1 << 1)) != 0;
        self.locked = (vp & (1 << 2)) != 0;
        self.opaque = (vp & (1 << 3)) != 0;
        self.ime_mode = ((vp >> 15) & 0b1111) as u8;
        self.word_wrap = (vp & (1 << 23)) != 0;
        self.auto_size = (vp & (1 << 28)) != 0;
    }
}

impl ChildControl for CommandButton {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

/// SpinButton control
#[derive(Debug)]
pub struct SpinButton {
    /// Site control properties
    pub control: OleSiteConcreteControl,
    /// The control version
    pub version: u16,
    /// Foreground color
    pub fore_color: u32,
    /// Background color
    pub back_color: u32,
    /// The control can receive the focus and respond to user-generated events
    pub enabled: bool,
    /// The default run-time mode of the Input Method Editor (IME)
    pub ime_mode: u8,
    /// Width and height of the control, in HIMETRIC units
    pub size: (i32, i32),
    /// The minimum valid control value
    pub min_value: i32,
    /// The maximum valid control value
    pub max_value: i32,
    /// The control value
    pub value: i32,
    /// The amount by which the value changes when the user clicks either scroll arrow
    pub small_change: i32,
    /// The control orientation (-1: auto, 0: vertical, 1: horizontal)
    pub orientation: i32,
    /// The delay, in milliseconds, between successive value-change events when a user clicks and holds down a button
    pub delay: u32,
    /// Type of icon displayed as the mouse pointer of the control
    pub mouse_pointer: u8,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl Default for SpinButton {
    fn default() -> Self {
        Self {
            control: OleSiteConcreteControl::default(),
            version: 0,
            fore_color: 0x80000012,
            back_color: 0x8000000f,
            enabled: true,
            ime_mode: 0,
            size: (0, 0),
            min_value: 0,
            max_value: 100,
            value: 0,
            small_change: 1,
            orientation: -1,
            delay: 50,
            mouse_pointer: 0,
            mouse_icon: None,
            anomalies: Vec::new(),
        }
    }
}

impl SpinButton {
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

        let mut various_properties: u32 = 0x0080001b;
        let mut has_size: Option<()> = None;
        let mut _prev_enabled = 0u32;
        let mut _next_enabled = 0u32;
        let mut mouse_icon: Option<u16> = None;

        let mask: PropertyMask = [
            property_mask_bit!(ret.fore_color),
            property_mask_bit!(ret.back_color),
            property_mask_bit!(various_properties),
            property_mask_bit!(has_size),
            Unused,
            property_mask_bit!(ret.min_value),
            property_mask_bit!(ret.max_value),
            property_mask_bit!(ret.value),
            property_mask_bit!(_prev_enabled),
            property_mask_bit!(_next_enabled),
            property_mask_bit!(ret.small_change),
            property_mask_bit!(ret.orientation),
            property_mask_bit!(ret.delay),
            property_mask_bit!(mouse_icon),
            property_mask_bit!(ret.mouse_pointer),
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

        let extra_data = set_data_properties(f, &mask)?;
        ret.set_various_properties(various_properties);

        // ExtraData
        if has_size.is_some() {
            let w = extra_data.get(0..4);
            let h = extra_data.get(4..8);
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
}

impl ChildControl for SpinButton {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};
    #[test]
    fn test_commandbutton_oddmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x20, 0x00, // cbImage
            0b10101010, 0b10101010, 0b10101010, 0b10101010, // Mask
            0x44, 0x33, 0x22, 0x11, // back color
            0x08, 0x00, 0x00, 0x00, // caption
            0xff, 0xff, // picture
            0x00, 0x00, //padding

            // ExtraData
            0x30, 0x00, 0x31, 0x00, 0x32, 0x00, 0x33, 0x00, // caption
            0x67, 0x45, 0x23, 0x01, // size w
            0x10, 0x32, 0x54, 0x76, // size h

            // StreamData
            // Picture
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // picture
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51,  // guid
            0x6c, 0x74, 0x00, 0x00, // picture preamble
            0x04, 0x00, 0x00, 0x00, // picture size
            0x10, 0x20, 0x30, 0x40, // "picture"

            // TextProps
            0x00, 0x02, // Version
            0x0d, 0x00, // cbImage
            0x01, 0x00, 0x00, 0x00, // Mask
            0x05, 0x00, 0x00, 0x80, // Font Name
            0x41, 0x72, 0x69, 0x61, 0x6c, // Arial

        ];

        let mut f = Cursor::new(buf.as_slice());
        let btn = CommandButton::new(&OleSiteConcreteControl::default(), &mut f)?;
        assert_eq!(btn.version, 0x200);
        assert_eq!(btn.fore_color, 0x80000012); // mask bit 0
        assert_eq!(btn.back_color, 0x11223344); // mask bit 1
        assert_eq!(btn.enabled, true); // mask bit 2
        assert_eq!(btn.locked, false); // mask bit 2
        assert_eq!(btn.opaque, true); // mask bit 2
        assert_eq!(btn.ime_mode, 0); // mask bit 2
        assert_eq!(btn.word_wrap, false); // mask bit 2
        assert_eq!(btn.auto_size, false); // mask bit 2
        assert_eq!(btn.caption, "0123"); // mask bit 3
        assert_eq!(btn.picture_position, 0x00070001); // mask bit 4
        assert_eq!(btn.size, (0x01234567, 0x76543210)); // mask bit 5
        assert_eq!(btn.mouse_pointer, 0); // mask bit 6
        assert_eq!(btn.picture.unwrap().picture, [0x10, 0x20, 0x30, 0x40]); // mask bit 7
        assert_eq!(btn.accelerator, 0); // mask bit 8
        assert_eq!(btn.focus_on_click, false); // mask bit 9
        assert!(btn.mouse_icon.is_none()); // mask bit 10
        assert_eq!(btn.font.unwrap().name, "Arial");

        Ok(())
    }

    #[test]
    fn test_commandbutton_evenmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x18, 0x00, // cbImage
            0b01010101, 0b01010101, 0b01010101, 0b01010101, // Mask
            0x44, 0x33, 0x22, 0x11, // fore color
            0b00000100, 0b10000000, 0b10000111, 0b00010000, // various properties
            0x08, 0x00, 0x06, 0x00, // picture position
            0x63, // mouse pointer
            0x00, // padding
            0x37, 0x13, // accelerator
            0xff, 0xff, // mouse icon
            0x00, 0x00, // padding

            // ExtraData is empty

            // StreamData
            // Mouse icon
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // picture
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, // guid
            0x6c, 0x74, 0x00, 0x00, // picture preamble
            0x02, 0x00, 0x00, 0x00, // picture size
            0xaa, 0x55, // "picture"

            // TextProps
            0x00, 0x02, // Version
            0x0d, 0x00, // cbImage
            0x01, 0x00, 0x00, 0x00, // Mask
            0x05, 0x00, 0x00, 0x80, // Font Name
            0x41, 0x72, 0x69, 0x61, 0x6c, // Arial

        ];

        let mut f = Cursor::new(buf.as_slice());
        let btn = CommandButton::new(&OleSiteConcreteControl::default(), &mut f)?;
        assert_eq!(btn.version, 0x200);
        assert_eq!(btn.fore_color, 0x11223344); // mask bit 0
        assert_eq!(btn.back_color, 0x8000000f); // mask bit 1
        assert_eq!(btn.enabled, false); // mask bit 2
        assert_eq!(btn.locked, true); // mask bit 2
        assert_eq!(btn.opaque, false); // mask bit 2
        assert_eq!(btn.ime_mode, 0b1111); // mask bit 2
        assert_eq!(btn.word_wrap, true); // mask bit 2
        assert_eq!(btn.auto_size, true); // mask bit 2
        assert!(btn.caption.is_empty()); // mask bit 3
        assert_eq!(btn.picture_position, 0x00060008); // mask bit 4
        assert_eq!(btn.size, (0, 0)); // mask bit 5
        assert_eq!(btn.mouse_pointer, 0x63); // mask bit 6
        assert!(btn.picture.is_none()); // mask bit 7
        assert_eq!(btn.accelerator, 0x1337); // mask bit 8
        assert_eq!(btn.focus_on_click, true); // mask bit 9
        assert_eq!(btn.mouse_icon.unwrap().picture, [0xaa, 0x55]); // mask bit 10
        assert_eq!(btn.font.unwrap().name, "Arial");

        Ok(())
    }

    #[test]
    fn test_spinbutton_oddmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x24, 0x00, // cbImage
            0b10101010, 0b10101010, 0b10101010, 0b10101010, // Mask
            0x44, 0x33, 0x22, 0x11, // back color
            0xf6, 0xff, 0xff, 0xff, // min
            0x20, 0x00, 0x00, 0x00, // value
            0x00, 0x00, 0x00, 0x00, // prev xxx
            0x01, 0x00, 0x00, 0x00, // orientation
            0xff, 0xff, // mouse
            0x00, 0x00, // padding

            // ExtraData
            0x67, 0x45, 0x23, 0x01, // size w
            0x10, 0x32, 0x54, 0x76, // size h

            // StreamData
            // Mouse icon
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // picture
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, // guid
            0x6c, 0x74, 0x00, 0x00, // picture preamble
            0x04, 0x00, 0x00, 0x00, // picture size
            0xc0, 0x01, 0xba, 0xbe, // "picture"

        ];

        let mut f = Cursor::new(buf.as_slice());
        let btn = SpinButton::new(&OleSiteConcreteControl::default(), &mut f)?;
        assert_eq!(btn.version, 0x200);
        assert_eq!(btn.fore_color, 0x80000012); // mask bit 0
        assert_eq!(btn.back_color, 0x11223344); // mask bit 1
        assert_eq!(btn.enabled, true); // mask bit 2
        assert_eq!(btn.ime_mode, 0); // mask bit 2
        assert_eq!(btn.size, (0x01234567, 0x76543210)); // mask bit 3
        assert_eq!(btn.min_value, -10); // mask bit 5
        assert_eq!(btn.max_value, 100); // mask bit 6
        assert_eq!(btn.value, 0x20); // mask bit 7
        assert_eq!(btn.small_change, 1); // mask bit 10
        assert_eq!(btn.orientation, 1); // mask bit 11
        assert_eq!(btn.delay, 50); // mask bit 12
        assert_eq!(btn.mouse_icon.unwrap().picture, [0xc0, 0x01, 0xba, 0xbe]); // mask bit 13
        assert_eq!(btn.mouse_pointer, 0); // mask bit 14

        Ok(())
    }

    #[test]
    fn test_spinbutton_evenmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x20, 0x00, // cbImage
            0b01010101, 0b01010101, 0b01010101, 0b01010101, // Mask
            0x44, 0x33, 0x22, 0x11, // fore color
            0b00000000, 0b10000000, 0b00000111, 0b00000000, // various properties
            0x78, 0x56, 0x34, 0x12, // max
            0x00, 0x00, 0x00, 0x00, // prev xxx
            0x23, 0x01, 0x00, 0x00, // small change
            0xe8, 0x03, 0x00, 0x00, // delay
            0x0f, // mouse pointer
            0x00, 0x00, 0x00, // padding

            // ExtraData is empty

            // StreamData is empty
        ];

        let mut f = Cursor::new(buf.as_slice());
        let btn = SpinButton::new(&OleSiteConcreteControl::default(), &mut f)?;
        assert_eq!(btn.version, 0x200);
        assert_eq!(btn.fore_color, 0x11223344); // mask bit 0
        assert_eq!(btn.back_color, 0x8000000f); // mask bit 1
        assert_eq!(btn.enabled, false); // mask bit 2
        assert_eq!(btn.ime_mode, 0b1111); // mask bit 2
        assert_eq!(btn.size, (0, 0)); // mask bit 3
        assert_eq!(btn.min_value, 0); // mask bit 5
        assert_eq!(btn.max_value, 0x12345678); // mask bit 6
        assert_eq!(btn.value, 0); // mask bit 7
        assert_eq!(btn.small_change, 0x123); // mask bit 10
        assert_eq!(btn.orientation, -1); // mask bit 11
        assert_eq!(btn.delay, 1000); // mask bit 12
        assert!(btn.mouse_icon.is_none()); // mask bit 13
        assert_eq!(btn.mouse_pointer, 0x0f); // mask bit 14

        Ok(())
    }
}
