use super::mask::*;
use ctxutils::{io::*, win32::GUID};
use std::io::{self, Read};
use std::str::FromStr;

/// A Font definition
#[derive(Debug, Default)]
pub struct Font {
    /// Name of the font
    pub name: String,
    /// Character set
    pub charset: u16,
    /// Bold effect is applied
    pub bold: bool,
    /// Italic effect is applied
    pub italic: bool,
    /// Underline effect is applied
    pub underline: bool,
    /// Strikeout effect is applied
    pub strikeout: bool,
    /// Disabled effect is applied
    pub disabled: bool,
    /// AutoColor effect is applied
    pub auto_color: bool,
    /// The weight of the text (0 - 1000)
    pub weight: u16,
    /// The height in twips of the text
    pub height: u32,
    /// Font pitch - see table
    ///
    /// | Value | Meaning
    /// |-------|------------------------------------------|
    /// |   0   | Default                                  |
    /// |   1   | All characters have the same fixed width |
    /// |   2   | Characters have varying widths           |
    ///
    pub pitch: u8,
    /// The font family - see table
    ///
    /// | Value | Meaning
    /// |-------|----------------------|
    /// |   0   | Don't care           |
    /// |   1   | Roman                |
    /// |   2   | Swiss                |
    /// |   3   | Modern (monospace)   |
    /// |   4   | Script (handwriting) |
    /// |   5   | Decorative           |
    ///
    pub family: u8,
    /// The paragraph alignment:
    ///
    /// | Value | Meaning
    /// |-------|---------------|
    /// |   0   | Unspecified   |
    /// |   1   | Left aligned  |
    /// |   2   | Right aligned |
    /// |   3   | Centered      |
    ///
    pub align: u8,
    /// Non fatal anomalies encountered while processing the font definition
    pub anomalies: Vec<String>,
}

impl Font {
    pub(crate) fn guid_and_font<R: Read>(f: &mut R) -> Result<Self, io::Error> {
        let clsid = GUID::from_le_stream(f)?;
        if clsid == GUID::from_str("0BE35203-8F91-11CE-9DE3-00AA004BB851").unwrap() {
            Self::std_font(f)
        } else if clsid == GUID::from_str("AFC20920-DA4E-11CE-B943-00AA006887B4").unwrap() {
            Self::text_props(f)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid FontGUID {}", clsid),
            ))
        }
    }

    pub(crate) fn std_font<R: Read>(f: &mut R) -> Result<Self, io::Error> {
        let mut ret = Self::default();
        let ver = rdu8(f)?;
        if ver != 1 {
            ret.anomalies
                .push(format!("Invalid StdFont version: {} (should be 1)", ver));
        }
        ret.charset = rdu16le(f)?;
        let flags = rdu8(f)?;
        ret.bold = (flags & 1) != 0;
        ret.italic = (flags & 2) != 0;
        ret.underline = (flags & 4) != 0;
        ret.strikeout = (flags & 8) != 0;
        ret.weight = rdu16le(f)?;
        ret.height = rdu32le(f)?;
        let face_len = rdu8(f)?;
        if face_len > 32 {
            ret.anomalies
                .push("StdFont FaceLen size exceeds 32 bytes".to_string());
        }
        let mut name = vec![0u8; face_len.into()];
        f.read_exact(&mut name)?;
        ret.name = String::from_utf8_lossy(&name).to_string();
        Ok(ret)
    }

    pub(crate) fn text_props<R: Read>(f: &mut R) -> Result<Self, io::Error> {
        let ver = rdu16le(f)?;
        let mut ret = Self {
            height: 160,
            weight: 400,
            align: 1,
            ..Self::default()
        };
        if ver != 0x0200 {
            ret.anomalies.push(format!(
                "Invalid TextProps version: {} (should be 0x200)",
                ver
            ));
        }
        let mut name: Option<u32> = None;
        let mut effects: u32 = 0;
        let mut charset: u8 = 1;
        let mut pitch_family: u8 = 0;
        let mask: PropertyMask = [
            property_mask_bit!(name),
            property_mask_bit!(effects),
            property_mask_bit!(ret.height),
            Unused, // UnusedBits1
            property_mask_bit!(charset),
            property_mask_bit!(pitch_family),
            property_mask_bit!(ret.align),
            property_mask_bit!(ret.weight),
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
            Unused, // UnusedBits2
        ];

        let buf = set_data_properties(f, &mask)?;
        if let Some(cob) = name {
            if let Some((s, _)) = get_cob_string(cob, Some(&buf)) {
                ret.name = s;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Font name overflow",
                ));
            }
        } else {
            ret.name = String::from("MS Sans Serif");
        }
        ret.bold = (effects & (1 << 0)) != 0;
        ret.italic = (effects & (1 << 1)) != 0;
        ret.underline = (effects & (1 << 2)) != 0;
        ret.strikeout = (effects & (1 << 3)) != 0;
        ret.disabled = (effects & (1 << 13)) != 0;
        ret.auto_color = (effects & (1 << 30)) != 0;
        ret.charset = charset.into();
        ret.pitch = pitch_family & 0xf;
        ret.family = (pitch_family & 0xf0) >> 4;
        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};
    #[test]
    fn test_std_font() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x03, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // guid
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, //
            0x01, // version
            0x01, 0x00, // charset
            0x05, // bold + underline
            0xaa, 0x55, // weight
            0x04, 0x03, 0x02, 0x01, // height
            0x0d, // face len
            0x43, 0x6f, 0x6d, 0x69, 0x63, 0x20, 0x53, 0x61, // comic sans
            0x6e, 0x73, 0x20, 0x4d, 0x53,
        ];

        let mut f = Cursor::new(buf.as_slice());
        let font = Font::guid_and_font(&mut f)?;
        assert_eq!(font.name, "Comic Sans MS");
        assert_eq!(font.charset, 1);
        assert_eq!(font.bold, true);
        assert_eq!(font.italic, false);
        assert_eq!(font.underline, true);
        assert_eq!(font.strikeout, false);
        assert_eq!(font.disabled, false);
        assert_eq!(font.auto_color, false);
        assert_eq!(font.weight, 0x55aa);
        assert_eq!(font.height, 0x01020304);
        assert_eq!(font.pitch, 0);
        assert_eq!(font.family, 0);
        assert_eq!(font.align, 0);
        Ok(())
    }

    #[test]
    fn test_textprops_oddmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x20, 0x09, 0xc2, 0xaf, 0x4e, 0xda, 0xce, 0x11, // guid
            0xb9, 0x43, 0x00, 0xaa, 0x00, 0x68, 0x87, 0xb4, //
            0x00, 0x02, // ver
            0x0c, 0x00, // cb
            0b10101010, 0b10101010, 0b10101010, 0b10101010, // Mask
            0x0f, 0x20, 0x00, 0x40, // effects
            0x23, // pitch + fam
            0x00, // pad
            0x37, 0x13, // weigth

            // ExtraData is empty
            // StreamData is empty

        ];
        let mut f = Cursor::new(buf.as_slice());
        let font = Font::guid_and_font(&mut f)?;
        assert_eq!(font.name, "MS Sans Serif");
        assert_eq!(font.bold, true);
        assert_eq!(font.italic, true);
        assert_eq!(font.underline, true);
        assert_eq!(font.strikeout, true);
        assert_eq!(font.disabled, true);
        assert_eq!(font.auto_color, true);
        assert_eq!(font.height, 160);
        assert_eq!(font.charset, 1);
        assert_eq!(font.pitch, 3);
        assert_eq!(font.family, 2);
        assert_eq!(font.align, 1);
        assert_eq!(font.weight, 0x1337);
        Ok(())
    }

    #[test]
    fn test_textprops_evenmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x20, 0x09, 0xc2, 0xaf, 0x4e, 0xda, 0xce, 0x11, // guid
            0xb9, 0x43, 0x00, 0xaa, 0x00, 0x68, 0x87, 0xb4, //
            0x00, 0x02, // ver
            0x20, 0x00, // cb
            0b01010101, 0b01010101, 0b01010101, 0b01010101, // Mask
            0x0d, 0x00, 0x00, 0x80, // name
            0x04, 0x03, 0x02, 0x01, // height
            0x01, // charset
            0x03, // para align
            0x00, 0x00, // pad

            // ExtraData
            0x43, 0x6f, 0x6d, 0x69, 0x63, 0x20, 0x53, 0x61, // comic sans
            0x6e, 0x73, 0x20, 0x4d, 0x53,
            0x00, 0x00, 0x00, // pad

            // StreamData is empty

        ];
        let mut f = Cursor::new(buf.as_slice());
        let font = Font::guid_and_font(&mut f)?;
        assert_eq!(font.name, "Comic Sans MS");
        assert_eq!(font.bold, false);
        assert_eq!(font.italic, false);
        assert_eq!(font.underline, false);
        assert_eq!(font.strikeout, false);
        assert_eq!(font.disabled, false);
        assert_eq!(font.auto_color, false);
        assert_eq!(font.height, 0x01020304);
        assert_eq!(font.charset, 1);
        assert_eq!(font.pitch, 0);
        assert_eq!(font.family, 0);
        assert_eq!(font.align, 3);
        assert_eq!(font.weight, 400);
        Ok(())
    }
}
