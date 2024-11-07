use crate::forms::controls::{ChildControl, OleSiteConcreteControl};
use crate::forms::{mask::*, Picture};
use ctxutils::io::*;
use std::io::{self, Read, Seek};

/// ScrollBar control
#[derive(Debug)]
pub struct ScrollBar {
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
    /// Type of icon displayed as the mouse pointer of the control
    pub mouse_pointer: u8,
    /// The minimum valid control value
    pub min_value: i32,
    /// The maximum valid control value
    pub max_value: i32,
    /// The control value
    pub value: i32,
    /// The amount by which the value changes when the user clicks either scroll arrow
    pub small_change: i32,
    /// The amount by which the value changes when the user clicks between the scroll box and scroll arrow
    pub large_change: i32,
    /// The control orientation (-1: auto, 0: vertical, 1: horizontal)
    pub orientation: i32,
    /// The size of the scroll box (-1: proporional, 0: fixed)
    pub proportional_thumb: i16,
    /// The delay, in milliseconds, between successive value-change events when a user clicks and holds down a button
    pub delay: u32,
    /// A custom mouse pointer image
    pub mouse_icon: Option<Picture>,
    /// Non fatal anomalies encountered while processing the control
    pub anomalies: Vec<String>,
}

impl Default for ScrollBar {
    fn default() -> Self {
        Self {
            control: OleSiteConcreteControl::default(),
            version: 0,
            fore_color: 0x80000012,
            back_color: 0x8000000f,
            enabled: true,
            ime_mode: 0,
            size: (0, 0),
            mouse_pointer: 0,
            min_value: 0,
            max_value: 0x00007fff,
            value: 0,
            small_change: 1,
            large_change: 1,
            orientation: -1,
            proportional_thumb: -1,
            delay: 50,
            mouse_icon: None,
            anomalies: Vec::new(),
        }
    }
}

impl ScrollBar {
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
        let mut _prev_enabled = 0u32;
        let mut _next_enabled = 0u32;
        let mut mouse_icon: Option<u16> = None;

        let mask: PropertyMask = [
            property_mask_bit!(ret.fore_color),
            property_mask_bit!(ret.back_color),
            property_mask_bit!(various_properties),
            property_mask_bit!(has_size),
            property_mask_bit!(ret.mouse_pointer),
            property_mask_bit!(ret.min_value),
            property_mask_bit!(ret.max_value),
            property_mask_bit!(ret.value),
            Unused,
            property_mask_bit!(_prev_enabled),
            property_mask_bit!(_next_enabled),
            property_mask_bit!(ret.small_change),
            property_mask_bit!(ret.large_change),
            property_mask_bit!(ret.orientation),
            property_mask_bit!(ret.proportional_thumb),
            property_mask_bit!(ret.delay),
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

impl ChildControl for ScrollBar {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.control
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};
    #[test]
    fn test_scrollbar_oddmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x28, 0x00, // cbImage
            0b10101010, 0b10101010, 0b10101010, 0b10101010, // Mask
            0x44, 0x33, 0x22, 0x11, // back color
            0xf6, 0xff, 0xff, 0xff, // min
            0x20, 0x00, 0x00, 0x00, // value
            0x00, 0x00, 0x00, 0x00, // prev xxx
            0x0a, 0x00, 0x00, 0x00, // small change
            0x01, 0x00, 0x00, 0x00, // orientation
            0x00, 0x01, 0x00, 0x00, // delay

            // ExtraData
            0x67, 0x45, 0x23, 0x01, // size w
            0x10, 0x32, 0x54, 0x76, // size h

            // StreamData is empty
        ];

        let mut f = Cursor::new(buf.as_slice());
        let btn = ScrollBar::new(&OleSiteConcreteControl::default(), &mut f)?;
        assert_eq!(btn.version, 0x200);
        assert_eq!(btn.fore_color, 0x80000012); // mask bit 0
        assert_eq!(btn.back_color, 0x11223344); // mask bit 1
        assert_eq!(btn.enabled, true); // mask bit 2
        assert_eq!(btn.ime_mode, 0); // mask bit 2
        assert_eq!(btn.size, (0x01234567, 0x76543210)); // mask bit 3
        assert_eq!(btn.mouse_pointer, 0); // mask bit 4
        assert_eq!(btn.min_value, -10); // mask bit 5
        assert_eq!(btn.max_value, 32767); // mask bit 6
        assert_eq!(btn.value, 0x20); // mask bit 7
        assert_eq!(btn.small_change, 10); // mask bit 11
        assert_eq!(btn.large_change, 1); // mask bit 12
        assert_eq!(btn.orientation, 1); // mask bit 13
        assert_eq!(btn.proportional_thumb, -1); // mask bit 14
        assert_eq!(btn.delay, 256); // mask bit 15
        assert!(btn.mouse_icon.is_none()); // mask bit 16

        Ok(())
    }

    #[test]
    fn test_scrollbar_evenmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x02, // Version
            0x20, 0x00, // cbImage
            0b01010101, 0b01010101, 0b01010101, 0b01010101, // Mask
            0x44, 0x33, 0x22, 0x11, // fore color
            0xe4, 0xff, 0xff, 0xff, // various properties
            0x0c, // mouse pointer
            0x00, 0x00, 0x00, // pad
            0x64, 0x00, 0x00, 0x00, // max
            0x00, 0x00, 0x00, 0x00, // next xxx
            0x0a, 0x00, 0x00, 0x00, // large change
            0x00, 0x00, // proportional
            0xff, 0xff, // mouse icon

            // ExtraData is empty

            // StreamData
            // Mouse icon
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // picture
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, // guid
            0x6c, 0x74, 0x00, 0x00, // picture preamble
            0x04, 0x00, 0x00, 0x00, // picture size
            0x01, 0x02, 0x03, 0x04, // "picture"
        ];

        let mut f = Cursor::new(buf.as_slice());
        let btn = ScrollBar::new(&OleSiteConcreteControl::default(), &mut f)?;
        assert_eq!(btn.version, 0x200);
        assert_eq!(btn.fore_color, 0x11223344); // mask bit 0
        assert_eq!(btn.back_color, 0x8000000f); // mask bit 1
        assert_eq!(btn.enabled, false); // mask bit 2
        assert_eq!(btn.ime_mode, 0x0f); // mask bit 2
        assert_eq!(btn.size, (0, 0)); // mask bit 3
        assert_eq!(btn.mouse_pointer, 0x0c); // mask bit 4
        assert_eq!(btn.min_value, 0); // mask bit 5
        assert_eq!(btn.max_value, 100); // mask bit 6
        assert_eq!(btn.value, 0); // mask bit 7
        assert_eq!(btn.small_change, 1); // mask bit 11
        assert_eq!(btn.large_change, 10); // mask bit 12
        assert_eq!(btn.orientation, -1); // mask bit 13
        assert_eq!(btn.proportional_thumb, 0); // mask bit 14
        assert_eq!(btn.delay, 50); // mask bit 15
        assert_eq!(btn.mouse_icon.unwrap().picture, [1, 2, 3, 4]); // mask bit 16

        Ok(())
    }
}
