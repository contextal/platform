use crate::forms::controls::{ChildControl, OleSiteConcreteControl};
use crate::forms::{Picture, mask::*};
use ctxutils::io::*;
use std::io::{self, Read, Seek};

/// An Image control
#[derive(Debug)]
pub struct Image {
    /// Site control properties
    pub control: OleSiteConcreteControl,
    /// The control version
    pub version: u16,
    /// Specifies whether the control automatically resizes to display its entire contents
    pub auto_size: bool,
    /// Color of the control border
    pub border_color: u32,
    /// Background color
    pub back_color: u32,
    /// Type of border used by the control
    pub border_style: u8,
    /// Type of icon displayed as the mouse pointer of the control
    pub mouse_pointer: u8,
    /// Specifies how to display the picture (0: crop, 1: stretch, 2: zoom)
    pub picture_size_mode: u8,
    /// Specifies the visual appearance of the control
    pub special_effect: u8,
    /// Width and height of the control, in HIMETRIC units
    pub size: (i32, i32),
    /// The (optional) picture to display on the control
    pub picture: Option<Picture>,
    /// The alignment of the picture in the contol
    pub picture_alignment: u8,
    /// Specifies whether the picture is tiled across the background
    pub tiled: bool,
    /// The control can receive the focus and respond to user-generated events
    pub enabled: bool,
    /// Specifies whether the control is opaque or transparent
    pub opaque: bool,
    /// The default run-time mode of the Input Method Editor (IME)
    pub ime_mode: u8,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            control: OleSiteConcreteControl::default(),
            version: 0,
            auto_size: false,
            border_color: 0x80000006,
            back_color: 0x8000000f,
            border_style: 1,
            mouse_pointer: 0,
            picture_size_mode: 0,
            special_effect: 0,
            size: (0, 0),
            picture: None,
            picture_alignment: 2,
            tiled: false,
            enabled: true,
            opaque: true,
            ime_mode: 0,
            mouse_icon: None,
            anomalies: Vec::new(),
        }
    }
}

impl Image {
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
        let mut has_size: Option<()> = None;
        let mut picture: Option<u16> = None;
        let mut mouse_icon: Option<u16> = None;

        let mask: PropertyMask = [
            Unused,
            Unused,
            property_mask_bit!(ret.auto_size),
            property_mask_bit!(ret.border_color),
            property_mask_bit!(ret.back_color),
            property_mask_bit!(ret.border_style),
            property_mask_bit!(ret.mouse_pointer),
            property_mask_bit!(ret.picture_size_mode),
            property_mask_bit!(ret.special_effect),
            property_mask_bit!(has_size),
            property_mask_bit!(picture),
            property_mask_bit!(ret.picture_alignment),
            property_mask_bit!(ret.tiled),
            property_mask_bit!(various_properties),
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
        ];

        let extra_data = set_data_properties(f, &mask)?;
        ret.set_vatious_properties(various_properties);

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

        Ok(ret)
    }

    fn set_vatious_properties(&mut self, vp: u32) {
        self.enabled = (vp & (1 << 1)) != 0;
        self.opaque = (vp & (1 << 3)) != 0;
        self.ime_mode = ((vp >> 15) & 0b1111) as u8;
    }
}

impl ChildControl for Image {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};
    #[test]
    fn test_image_oddmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x18, 0x00, // cbImage
            0b10101010, 0b10101010, 0b10101010, 0b10101010, // Mask
            0x12, 0x34, 0x56, 0x78, // border color
            0x00, // border style
            0x03, // picture size mode
            0x04, // picture alignment
            0,    // padding
            0b00010001, 0b10000000, 0b00000111, 0, // various properties
            // ExtraData
            0x0b, 0x0a, 0x0c, 0x0a, // Size w
            0x07, 0x03, 0x03, 0x01, // Size h
        ];

        let mut f = Cursor::new(buf.as_slice());
        let image = Image::new(&OleSiteConcreteControl::default(), &mut f)?;

        assert_eq!(image.version, 0x200);
        assert_eq!(image.auto_size, false); // mask bit 2
        assert_eq!(image.border_color, 0x78563412); // mask bit 3
        assert_eq!(image.back_color, 0x8000000f); // mask bit 4
        assert_eq!(image.border_style, 0); // mask bit 5
        assert_eq!(image.mouse_pointer, 0); // mask bit 6
        assert_eq!(image.picture_size_mode, 3); // mask bit 7
        assert_eq!(image.special_effect, 0); // mask bit 8
        assert_eq!(image.size, (0x0a0c0a0b, 0x01030307)); // mask bit 9
        assert!(image.picture.is_none()); // mask bit 10
        assert_eq!(image.picture_alignment, 4); // mask bit 11
        assert_eq!(image.tiled, false); // mask bit 12
        assert_eq!(image.enabled, false); // mask bit 13
        assert_eq!(image.opaque, false); // mask bit 13
        assert_eq!(image.ime_mode, 0b1111); // mask bit 13
        assert!(image.mouse_icon.is_none()); // mask bit 14
        Ok(())
    }

    #[test]
    fn test_image_evenmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x10, 0x00, // cbImage
            0b0101_0101, 0b01010101, 0b01010101, 0b01010101, // Mask
            0x12, 0x34, 0x56, 0x78, // back color
            0x0b, // mouse pointer
            0x02, // special effect
            0xff, 0xff, // picture
            0xff, 0xff, // mouse icon
            0x00, 0x00, // padding
            // ExtraData is empty
            // StreamData
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // picture
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, // guid
            0x6c, 0x74, 0x00, 0x00, // picture preamble
            0x04, 0x00, 0x00, 0x00, // picture size
            0x01, 0x02, 0x03, 0x04, // "picture"
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // mouse icon
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, // guid
            0x6c, 0x74, 0x00, 0x00, // icon preamble
            0x02, 0x00, 0x00, 0x00, // icon size
            0xaa, 0x55,  // "icon"
        ];

        let mut f = Cursor::new(buf.as_slice());
        let image = Image::new(&OleSiteConcreteControl::default(), &mut f)?;

        assert_eq!(image.version, 0x200);
        assert_eq!(image.auto_size, true); // mask bit 2
        assert_eq!(image.border_color, 0x80000006); // mask bit 3
        assert_eq!(image.back_color, 0x78563412); // mask bit 4
        assert_eq!(image.border_style, 1); // mask bit 5
        assert_eq!(image.mouse_pointer, 0x0b); // mask bit 6
        assert_eq!(image.picture_size_mode, 0); // mask bit 7
        assert_eq!(image.special_effect, 2); // mask bit 8
        assert_eq!(image.size, (0, 0)); // mask bit 9
        assert_eq!(image.picture.unwrap().picture, [1, 2, 3, 4]); // mask bit 10
        assert_eq!(image.picture_alignment, 2); // mask bit 11
        assert_eq!(image.tiled, true); // mask bit 12
        assert_eq!(image.enabled, true); // mask bit 13
        assert_eq!(image.opaque, true); // mask bit 13
        assert_eq!(image.ime_mode, 0); // mask bit 13
        assert_eq!(image.mouse_icon.unwrap().picture, [0xaa, 0x55]); // mask bit 14
        Ok(())
    }
}
