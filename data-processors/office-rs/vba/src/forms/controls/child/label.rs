use crate::forms::controls::{ChildControl, OleSiteConcreteControl};
use crate::forms::{mask::*, Font, Picture};
use ctxutils::io::*;
use std::io::{self, Read, Seek};

/// A Label control
#[derive(Debug)]
pub struct Label {
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
    /// Color of the control border
    pub border_color: u32,
    /// Type of border used by the control
    pub border_style: u8,
    /// Specifies the visual appearance of the control
    pub special_effect: u16,
    /// The (optional) picture to display on the control
    pub picture: Option<Picture>,
    /// The accelerator key
    pub accelerator: u16,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// Font properties
    pub font: Option<Font>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl Default for Label {
    fn default() -> Self {
        Self {
            control: OleSiteConcreteControl::default(),
            version: 0,
            fore_color: 0x80000012,
            back_color: 0x8000000f,
            enabled: true,
            opaque: true,
            ime_mode: 0,
            word_wrap: true,
            auto_size: false,
            caption: String::new(),
            picture_position: 0x00070001,
            size: (0, 0),
            mouse_pointer: 0,
            border_color: 0x80000006,
            border_style: 0,
            special_effect: 0,
            picture: None,
            accelerator: 0,
            mouse_icon: None,
            font: None,
            anomalies: Vec::new(),
        }
    }
}

impl Label {
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
            property_mask_bit!(ret.border_color),
            property_mask_bit!(ret.border_style),
            property_mask_bit!(ret.special_effect),
            property_mask_bit!(picture),
            property_mask_bit!(ret.accelerator),
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
        ];

        let extra_data = set_data_properties(f, &mask)?;
        ret.set_vatious_properties(various_properties);

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

    fn set_vatious_properties(&mut self, vp: u32) {
        self.enabled = (vp & (1 << 1)) != 0;
        self.opaque = (vp & (1 << 3)) != 0;
        self.ime_mode = ((vp >> 15) & 0b1111) as u8;
        self.word_wrap = (vp & (1 << 23)) != 0;
        self.auto_size = (vp & (1 << 28)) != 0;
    }
}

impl ChildControl for Label {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};
    #[test]
    fn test_label_oddmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x20, 0x00, // cbImage
            0b10101010, 0b10101010, 0b10101010, 0b10101010, // Mask
            0x11, 0x22, 0x33, 0x44, // back color
            0x04, 0x00, 0x00, 0x80, // caption
            0xdd, 0xcc, 0xbb, 0xaa, // border color
            0x06, 0x00, // special effect
            0x37, 0x13, // accelerator

            // ExtraData
            0x30, 0x31, 0x32, 0x33, // caption
            0x01, 0x02, 0x03, 0x04, // size w
            0x40, 0x30, 0x20, 0x10, // size h

            // StreamData is empty
            // TextProps
            0x00, 0x02, // Version
            0x04, 0x00, // cbImage
            0x00, 0x00, 0x00, 0x00, // Mask
        ];

        let mut f = Cursor::new(buf.as_slice());
        let label = Label::new(&OleSiteConcreteControl::default(), &mut f)?;

        assert_eq!(label.version, 0x200);
        assert_eq!(label.fore_color, 0x80000012); // mask bit 0
        assert_eq!(label.back_color, 0x44332211); // mask bit 1
        assert_eq!(label.enabled, true); // mask bit 2
        assert_eq!(label.opaque, true); // mask bit 2
        assert_eq!(label.ime_mode, 0); // mask bit 2
        assert_eq!(label.word_wrap, true); // mask bit 2
        assert_eq!(label.auto_size, false); // mask bit 2
        assert_eq!(label.caption, "0123"); // mask bit 3
        assert_eq!(label.picture_position, 0x00070001); // mask bit 4
        assert_eq!(label.size, (0x04030201, 0x10203040)); // mask bit 5
        assert_eq!(label.mouse_pointer, 0); // mask bit 6
        assert_eq!(label.border_color, 0xaabbccdd); // mask bit 7
        assert_eq!(label.border_style, 0); // mask bit 8
        assert_eq!(label.special_effect, 6); // mask bit 9
        assert!(label.picture.is_none()); // mask bit 10
        assert_eq!(label.accelerator, 0x1337); // mask bit 11
        assert!(label.mouse_icon.is_none()); // mask bit 12

        Ok(())
    }

    #[test]
    fn test_label_evenmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x18, 0x00, // cbImage
            0b01010101, 0b01010101, 0b01010101, 0b01010101, // Mask
            0x11, 0x22, 0x33, 0x44, // fore color
            0b00000000, 0b10000000, 0b00000111, 0b00010000, // various properties
            0x05, 0x00, 0x03, 0x00, // picture position
            0x63, // mouse pointer
            0x01, // border style
            0xff, 0xff, // picture
            0xff, 0xff, // mouse icon
            0x00, 0x00, // padding

            // ExtraData is empty

            // StreamData
            // Picture
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // picture
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, // guid
            0x6c, 0x74, 0x00, 0x00, // picture preamble
            0x04, 0x00, 0x00, 0x00, // picture size
            0xa0, 0xb1, 0xc2, 0xd3, // "picture"
            // Mouse Icon
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // mouse icon
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, // guid
            0x6c, 0x74, 0x00, 0x00, // icon preamble
            0x03, 0x00, 0x00, 0x00, // icon size
            0x0a, 0x0b, 0x0c, // "icon"

            // TextProps
            0x00, 0x02, // Version
            0x04, 0x00, // cbImage
            0x00, 0x00, 0x00, 0x00, // Mask

        ];

        let mut f = Cursor::new(buf.as_slice());
        let label = Label::new(&OleSiteConcreteControl::default(), &mut f)?;

        assert_eq!(label.version, 0x200);
        assert_eq!(label.fore_color, 0x44332211); // mask bit 0
        assert_eq!(label.back_color, 0x8000000f); // mask bit 1
        assert_eq!(label.enabled, false); // mask bit 2
        assert_eq!(label.opaque, false); // mask bit 2
        assert_eq!(label.ime_mode, 0xf); // mask bit 2
        assert_eq!(label.word_wrap, false); // mask bit 2
        assert_eq!(label.auto_size, true); // mask bit 2
        assert!(label.caption.is_empty()); // mask bit 3
        assert_eq!(label.picture_position, 0x00030005); // mask bit 4
        assert_eq!(label.size, (0, 0)); // mask bit 5
        assert_eq!(label.mouse_pointer, 0x63); // mask bit 6
        assert_eq!(label.border_color, 0x80000006); // mask bit 7
        assert_eq!(label.border_style, 1); // mask bit 8
        assert_eq!(label.special_effect, 0); // mask bit 9
        assert_eq!(label.picture.unwrap().picture, [0xa0, 0xb1, 0xc2, 0xd3]); // mask bit 10
        assert_eq!(label.accelerator, 0); // mask bit 11
        assert_eq!(label.mouse_icon.unwrap().picture, [0x0a, 0x0b, 0x0c]); // mask bit 12

        Ok(())
    }
}
