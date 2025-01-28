use crate::forms::controls::{ParentControl, ParentControlInfo};
use crate::Ole;
use ctxutils::win32::GUID;
use regex::Regex;
use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Seek};
use std::str::FromStr;

/// A top-level form
///
/// A `UserForm` is a *parent control* that sits at the top level of the hierarchy (i.e.
/// it has no parent)
pub struct UserForm<'a, R: Read + Seek> {
    /// Info related to the top-level placement (e.g. positioning, title, etc.)
    pub di: DesignerInfo,
    /// Info common to all parent controls (e.g. visual aspects, child controls, etc.)
    pub pi: ParentControlInfo<'a, R>,
}

impl<'a, R: Read + Seek> fmt::Debug for UserForm<'a, R> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Form")
            .field("di", &self.di)
            .field("pi", &self.pi)
            .finish()
    }
}

impl<'a, R: 'a + Read + Seek> UserForm<'a, R> {
    /// Returns the definition and related values of a top level form
    ///
    /// Typically `UserForm`s are obtained via [`Vba::forms()`][crate::Vba::forms], however
    /// this `fn` is also provided for the case when manual access is needed
    pub fn new(ole: &'a Ole<R>, name: &str, storage_name: &str) -> Result<Self, io::Error> {
        Ok(Self {
            pi: ParentControlInfo::new(ole, name, storage_name)?,
            di: DesignerInfo::new(ole, storage_name),
        })
    }
}

impl<'a, R: Read + Seek> ParentControl<'a, R> for UserForm<'a, R> {
    fn pctrl_info(&'a self) -> &'a ParentControlInfo<'a, R> {
        &self.pi
    }
}

/// Info for top-level Forms (as found in the *"vbFrame" stream*)
///
/// Note: Office handles failures here with a different degree of severity
/// However, since designer issues can be safely ignored without preventing
/// further processing, these fields are extracted whenever feasible and all
/// errors are simply logged here as [`anomalies`](Self::anomalies)
#[derive(Debug)]
pub struct DesignerInfo {
    /// The module name declared in the designer
    pub module_name: String,
    /// CLSID (`DesignerCLSID`)
    pub clsid: GUID,
    /// The form title (`DesignerCaption`)
    pub caption: String,
    /// Left edge position in `twips` (`DesignerLeft`)
    pub left: Option<f64>,
    /// Top edge position in `twips` (`DesignerTop`)
    pub top: Option<f64>,
    /// Width in `twips` (`DesignerWidth`)
    pub width: Option<f64>,
    /// Height in `twips` (`DesignerHeight`)
    pub height: Option<f64>,
    /// Enabled (`DesignerEnabled`)
    pub enabled: bool,
    /// Help topic identifier (`DesignerHelpContextId`)
    pub help_context_id: i32,
    /// Right and left coordinates are reversed (`DesignerRTL`)
    pub rtl: bool,
    /// Window is modal (`DesignerShowModal`)
    pub modal: bool,
    /// Startup position (`DesignerStartupPosition`)
    ///
    /// | Value | Meaning                                         |
    /// |-------|-------------------------------------------------|
    /// |   0   | Top and Left are relative to the Desktop window |
    /// |   1   | Centered on the parent window                   |
    /// |   2   | Centered on the Desktop window                  |
    /// |   3   | Placed in the upper left corner                 |
    ///
    pub start_position: i32,
    /// User defined data (`DesignerTag`)
    pub tag: String,
    /// Number of times the Form was changed and saved (`DesignerTypeInfoVer`)
    pub type_info_ver: i32,
    /// Visibility (`DesignerVisible`)
    pub visible: bool,
    /// Whether a help button is shown (`DesignerWhatsThisButton`)
    pub help_btn: bool,
    /// Whether to use the associated help topic (`DesignerWhatsThisHelp`) - see [`help_context_id`](Self::help_context_id)
    pub help_topic: bool,
    /// Non fatal anomalies encountered while processing the `vbFrame` stream
    pub anomalies: Vec<String>,
}

impl DesignerInfo {
    /// This function processes the *"VBFrame" stream* and returns its defining values
    fn new<R: Read + Seek>(ole: &Ole<R>, storage_name: &str) -> Self {
        let mut ret = Self {
            // Note: Most VBFrame properties (`DesignerProperties`) don't have an assigned default
            module_name: String::new(),
            clsid: GUID::null(),
            caption: String::new(),
            left: None,
            top: None,
            width: None,
            height: None,
            enabled: true,
            help_context_id: 0,
            rtl: false,
            modal: true,
            start_position: 0,
            tag: String::new(),
            type_info_ver: 0,
            visible: true,
            help_btn: false,
            help_topic: false,
            anomalies: Vec::new(),
        };

        // Note: Office products are very picky about the overall structure but not
        // excessively bothered by the actual property values, type, order, etc
        let stream_name = format!("{}/\u{3}VBFrame", storage_name);
        let entry = match ole.get_entry_by_name(&stream_name) {
            Ok(v) => v,
            Err(_) => {
                ret.anomalies
                    .push("Invalid DesignerInfo VERSION".to_string());
                return ret;
            }
        };
        let mut lines = BufReader::new(ole.get_stream_reader(&entry)).lines();
        match lines.next() {
            Some(Ok(s)) if s == "VERSION 5.00" => {}
            _ => {
                ret.anomalies
                    .push("Invalid DesignerInfo VERSION".to_string());
                return ret;
            }
        }
        let line = match lines.next() {
            Some(Ok(v)) => v,
            _ => {
                ret.anomalies.push("Invalid DesignerInfo Begin".to_string());
                return ret;
            }
        };
        let re = Regex::new(r"^Begin\s\{([0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12})\}\s([^\s]+).*$").unwrap();
        match re.captures(&line) {
            Some(caps) => {
                ret.clsid = GUID::from_str(&caps[1]).unwrap();
                ret.module_name = caps[2].trim_end().to_string();
            }
            _ => {
                ret.anomalies.push("Invalid DesignerInfo Begin".to_string());
                return ret;
            }
        }
        let re = Regex::new(r"^\s*([^\s]+)\s*=\s*(.*)$").unwrap();
        for line in lines.map_while(Result::ok) {
            if line == "End" {
                return ret;
            }
            let caps = match re.captures(&line) {
                Some(v) => v,
                None => continue,
            };
            match caps[1].as_ref() {
                "Caption" => {
                    if let Some(s) = Self::get_str_value(&caps[2]) {
                        ret.caption = s;
                    }
                }
                "ClientHeight" => ret.height = Self::get_float_value(&caps[2]),
                "ClientLeft" => ret.left = Self::get_float_value(&caps[2]),
                "ClientTop" => ret.top = Self::get_float_value(&caps[2]),
                "ClientWidth" => ret.width = Self::get_float_value(&caps[2]),
                "Enabled" => ret.enabled = Self::get_bool_value(&caps[2]).unwrap_or(ret.enabled),
                "HelpContextID" => {
                    ret.help_context_id =
                        Self::get_int_value(&caps[2]).unwrap_or(ret.help_context_id)
                }
                "RightToLeft" => ret.rtl = Self::get_bool_value(&caps[2]).unwrap_or(ret.rtl),
                "ShowModal" => ret.modal = Self::get_bool_value(&caps[2]).unwrap_or(ret.modal),
                "StartUpPosition" => {
                    ret.start_position = Self::get_int_value(&caps[2]).unwrap_or(ret.start_position)
                }
                "Tag" => {
                    if let Some(s) = Self::get_str_value(&caps[2]) {
                        ret.tag = s;
                    }
                }
                "TypeInfoVer" => {
                    ret.type_info_ver = Self::get_int_value(&caps[2]).unwrap_or(ret.type_info_ver)
                }
                "Visible" => ret.visible = Self::get_bool_value(&caps[2]).unwrap_or(ret.visible),
                "WhatsThisButton" => {
                    ret.help_btn = Self::get_bool_value(&caps[2]).unwrap_or(ret.help_btn)
                }
                "WhatsThisHelp" => {
                    ret.help_topic = Self::get_bool_value(&caps[2]).unwrap_or(ret.help_topic)
                }
                s => {
                    // Note: Office merely pops up an error MessageBox here with no
                    // practical adverse effects once dismissed with a "No"
                    ret.anomalies
                        .push(format!("Invalid DesignerInfo content: \"{}\"", s));
                }
            }
        }
        ret.anomalies
            .push("DesignerInfo is not properly terminated".to_string());
        ret
    }

    fn get_str_value(s: &str) -> Option<String> {
        if !s.starts_with('"') {
            return None;
        }
        let mut in_string = true;
        let mut in_quote = false;
        let mut ret = String::new();
        for c in s[1..].chars() {
            if in_string {
                if c == '"' {
                    if in_quote {
                        ret.push('"');
                        in_quote = false;
                        continue;
                    } else {
                        in_quote = true;
                        continue;
                    }
                } else {
                    if in_quote {
                        in_string = false;
                    } else {
                        ret.push(c);
                        continue;
                    }
                }
            }
            if !in_string {
                if c == '\'' {
                    break;
                } else if !c.is_whitespace() {
                    return None;
                }
            }
        }
        if !in_string || in_quote {
            Some(ret)
        } else {
            None
        }
    }

    fn get_float_value(s: &str) -> Option<f64> {
        f64::from_str(s.find('\'').map(|c| &s[0..c]).unwrap_or(s).trim_end()).ok()
    }

    fn get_bool_value(s: &str) -> Option<bool> {
        // Technically this should only allow 0=False or -1=True
        // Practical evidence on the other hand, shows that
        // * 0 is interpreted as False
        // * any other number is interpreted as True
        // * anything else is rejected
        match Self::get_int_value(s) {
            Some(0) => Some(false),
            Some(_) => Some(true),
            _ => None,
        }
    }

    fn get_int_value(s: &str) -> Option<i32> {
        s.find('\'')
            .map(|c| &s[0..c])
            .unwrap_or(s)
            .trim_end()
            .parse::<i32>()
            .ok()
    }
}

// -------------------------- TESTS --------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    #[test]
    fn test_designer_get_str_value() {
        assert_eq!(DesignerInfo::get_str_value(""), None);
        assert_eq!(DesignerInfo::get_str_value("\""), None);
        assert_eq!(DesignerInfo::get_str_value("\"a"), None);
        assert_eq!(DesignerInfo::get_str_value("a\""), None);
        assert_eq!(DesignerInfo::get_str_value("\"\""), Some("".to_owned()));
        assert_eq!(DesignerInfo::get_str_value("\"a\""), Some("a".to_owned()));
        assert_eq!(DesignerInfo::get_str_value("\"a\"\""), None);
        assert_eq!(
            DesignerInfo::get_str_value("\"a\"\"\""),
            Some("a\"".to_owned())
        );
        assert_eq!(
            DesignerInfo::get_str_value("\" a\t\""),
            Some(" a\t".to_owned())
        );
        assert_eq!(
            DesignerInfo::get_str_value("\"a'b\""),
            Some("a'b".to_owned())
        );
        assert_eq!(
            DesignerInfo::get_str_value("\"a b\" \t"),
            Some("a b".to_owned())
        );
        assert_eq!(
            DesignerInfo::get_str_value("\"a b\"\"\" \t"),
            Some("a b\"".to_owned())
        );
        assert_eq!(DesignerInfo::get_str_value("\"ab\" c"), None);
        assert_eq!(DesignerInfo::get_str_value("\"ab\" c '"), None);
        assert_eq!(
            DesignerInfo::get_str_value("\"a b\" '"),
            Some("a b".to_owned())
        );
        assert_eq!(
            DesignerInfo::get_str_value("\"a b\" 'c"),
            Some("a b".to_owned())
        );
        assert_eq!(
            DesignerInfo::get_str_value("\"a b\" ''\""),
            Some("a b".to_owned())
        );
    }

    #[test]
    fn test_designer_get_float_value() {
        assert_eq!(DesignerInfo::get_float_value(""), None);
        assert_eq!(DesignerInfo::get_float_value("1a"), None);
        assert_eq!(DesignerInfo::get_float_value("a1"), None);
        assert_eq!(DesignerInfo::get_float_value("0"), Some(0f64));
        assert_eq!(DesignerInfo::get_float_value("+0"), Some(0f64));
        assert_eq!(DesignerInfo::get_float_value("-0"), Some(0f64));
        assert_eq!(DesignerInfo::get_float_value("10"), Some(10f64));
        assert_eq!(DesignerInfo::get_float_value("10' 42"), Some(10f64));
        assert_eq!(DesignerInfo::get_float_value("+10"), Some(10f64));
        assert_eq!(DesignerInfo::get_float_value("-10"), Some(-10f64));
        assert_eq!(
            DesignerInfo::get_float_value("-10    ' .007 "),
            Some(-10f64)
        );
        assert_eq!(DesignerInfo::get_float_value("7."), Some(7f64));
        assert_eq!(DesignerInfo::get_float_value("+7."), Some(7f64));
        assert_eq!(DesignerInfo::get_float_value("-7."), Some(-7f64));
        assert_eq!(DesignerInfo::get_float_value("12.34"), Some(12.34));
        assert_eq!(DesignerInfo::get_float_value("+12.34"), Some(12.34));
        assert_eq!(DesignerInfo::get_float_value("-12.34"), Some(-12.34));
        assert_eq!(DesignerInfo::get_float_value(".123"), Some(0.123));
        assert_eq!(DesignerInfo::get_float_value("+.123"), Some(0.123));
        assert_eq!(DesignerInfo::get_float_value("-.123"), Some(-0.123));
        assert_eq!(DesignerInfo::get_float_value("1.2e+3"), Some(1200f64));
        assert_eq!(DesignerInfo::get_float_value("+1.2e+3"), Some(1200f64));
        assert_eq!(DesignerInfo::get_float_value("-1.2e+3"), Some(-1200f64));
        assert_eq!(DesignerInfo::get_float_value("3.2e-1"), Some(0.32f64));
        assert_eq!(DesignerInfo::get_float_value("+3.2e-1"), Some(0.32f64));
        assert_eq!(DesignerInfo::get_float_value("-3.2e-1"), Some(-0.32f64));
        assert_eq!(DesignerInfo::get_float_value("3.2e1"), Some(32f64));
        assert_eq!(DesignerInfo::get_float_value("+3.2e1"), Some(32f64));
        assert_eq!(DesignerInfo::get_float_value("-3.2e1"), Some(-32f64));
    }

    #[test]
    fn test_designer_get_bool_value() {
        assert_eq!(DesignerInfo::get_bool_value("0"), Some(false));
        assert_eq!(DesignerInfo::get_bool_value("-1"), Some(true));
        assert_eq!(DesignerInfo::get_bool_value("-1'asd"), Some(true));
        assert_eq!(DesignerInfo::get_bool_value("-1' 42"), Some(true));
        assert_eq!(DesignerInfo::get_bool_value(""), None);
        assert_eq!(DesignerInfo::get_bool_value("1"), Some(true));
        assert_eq!(DesignerInfo::get_bool_value("true"), None);
    }

    #[test]
    fn test_designer_get_int_value() {
        assert_eq!(DesignerInfo::get_int_value("10"), Some(10));
        assert_eq!(DesignerInfo::get_int_value("+10"), Some(10));
        assert_eq!(DesignerInfo::get_int_value("+10 ' hello"), Some(10));
        assert_eq!(DesignerInfo::get_int_value("-10"), Some(-10));
        assert_eq!(DesignerInfo::get_int_value("-10' +10"), Some(-10));
        assert_eq!(DesignerInfo::get_int_value(""), None);
        assert_eq!(DesignerInfo::get_int_value("12f"), None);
        assert_eq!(
            DesignerInfo::get_int_value("-2147483648"),
            Some(-2147483648)
        );
        assert_eq!(DesignerInfo::get_int_value("2147483647"), Some(2147483647));
        assert_eq!(DesignerInfo::get_int_value("-2147483649"), None);
        assert_eq!(DesignerInfo::get_int_value("2147483648"), None);
    }

    #[test]
    fn test_designer() {
        let vbframe = "VERSION 5.00\r\n\
                       Begin {12345678-ABCD-EF01-2345-6789ABCDEF12} SomeModule\r\n\
                       Caption          =     \"Window Title\"  'this is the title \r\n\
                       ClientHeight     =     1234 \r\n\
                       ClientLeft       =     345 'the left placement\r\n\
                       ClientTop        =     678 'the top placement\r\n\
                       ClientWidth      =     910  \r\n\
                       Enabled          =     -1  'TRUE\r\n\
                       ShowModal        =     0   'FALSE\r\n\
                       StartUpPosition  =     2   'Center\r\n\
                       Tag              =     \"Some Tag\"\r\n\
                       TypeInfoVer      =     12\r\n\
                       End\r\n";
        let mut ole: Ole<Cursor<Vec<u8>>> = Ole::new();
        ole.push(
            "Macros/MyForm/\u{3}VBFrame",
            Cursor::new(Vec::from(vbframe.as_bytes())),
        );
        let di = DesignerInfo::new(&ole, "Macros/MyForm");
        assert_eq!(di.clsid.to_string(), "12345678-abcd-ef01-2345-6789abcdef12");
        assert_eq!(di.caption, "Window Title");
        assert_eq!(di.left, Some(345f64));
        assert_eq!(di.top, Some(678f64));
        assert_eq!(di.width, Some(910f64));
        assert_eq!(di.height, Some(1234f64));
        assert_eq!(di.enabled, true);
        assert_eq!(di.help_context_id, 0);
        assert_eq!(di.rtl, false);
        assert_eq!(di.modal, false);
        assert_eq!(di.start_position, 2);
        assert_eq!(di.tag, "Some Tag");
        assert_eq!(di.type_info_ver, 12);
        assert_eq!(di.visible, true);
        assert_eq!(di.help_btn, false);
        assert_eq!(di.help_topic, false);
    }

    #[test]
    fn test_parent_control_oddmask() -> Result<(), io::Error> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x00, 0x04, // version
            0x3c, 0x00, // cbsite
            0b10101010, 0b10101010, 0b10101010, 0b10101010, // mask
            0x11, 0x22, 0x33, 0x44, // back color
            0x78, 0x56, 0x34, 0x12, // ID
            0xff, // border style
            0x03, // scroll bars
            0x00, 0x00, // pad
            0x01, 0x03, 0x02, 0x04, // group count
            0xff, 0xff, // mouse icon
            0x42, // sepcial effect
            0x00, // pad
            0x0d, 0x00, 0x00, 0x80, // caption
            0xff, 0xff, // pic
            0x04, // pic align
            0x03, // pic size mode
            0x40, 0x42, 0x0f, 0x00, // draw buffer

            // ExtraData
            0xe8, 0x03, 0x00, 0x00, // logical w
            0xf4, 0x01, 0x00, 0x00, // logical h
            0x46, 0x72, 0x61, 0x6d,  0x65, 0x20, 0x43, 0x61, // "Frame Caption"
            0x70, 0x74, 0x69, 0x6f,  0x6e, 0x00, 0x00, 0x00,

            // StreamData
            // Mouse Icon
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // mouse icon
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, // guid
            0x6c, 0x74, 0x00, 0x00, // icon preamble
            0x03, 0x00, 0x00, 0x00, // icon size
            0x01, 0x02, 0x03, // "icon"
            // Picture
            0x04, 0x52, 0xe3, 0x0b, 0x91, 0x8f, 0xce, 0x11, // picture
            0x9d, 0xe3, 0x00, 0xaa, 0x00, 0x4b, 0xb8, 0x51, // guid
            0x6c, 0x74, 0x00, 0x00, // picture preamble
            0x04, 0x00, 0x00, 0x00, // picture size
            0xa0, 0xb1, 0xc2, 0xd3, // "picture"

            // Sites
            0x00, 0x00, // CountOfSiteClassInfo
            0x00, 0x00, 0x00, 0x00, // nsites
            0x00, 0x00, 0x00, 0x00, // nbytes
        ];

        let mut ole: Ole<Cursor<Vec<u8>>> = Ole::new();
        ole.push("Macros/MyForm/i05/f", Cursor::new(buf));
        let pc = ParentControlInfo::new(&ole, "MyFrame", "Macros/MyForm/i05")?;
        assert_eq!(pc.path, "Macros/MyForm/i05");
        assert_eq!(pc.version, 0x400);
        assert_eq!(pc.name, "MyFrame");
        assert_eq!(pc.back_color, 0x44332211); // mask bit 1
        assert_eq!(pc.fore_color, 0x80000012); // mask bit 2
        assert_eq!(pc.next_available_id, 0x12345678); // mask bit 3
        assert_eq!(pc.border_style, 0xff); // mask bit 7
        assert_eq!(pc.mouse_pointer, 0); // mask bit 8
        assert_eq!(pc.scroll_bars, 0x3); // mask bit 9
        assert_eq!(pc.displayed_size, (4000, 3000)); // mask bit 10
        assert_eq!(pc.logical_size, (1000, 500)); // mask bit 11
        assert_eq!(pc.scroll_position, (0, 0)); // mask bit 12
        assert_eq!(pc.group_count, 0x04020301); // mask bit 13
        assert_eq!(pc.mouse_icon.unwrap().picture, [1, 2, 3]); // mask bit 15
        assert_eq!(pc.cycle, 0); // mask bit 16
        assert_eq!(pc.special_effect, 0x42); // mask bit 17
        assert_eq!(pc.border_color, 0x80000012); // mask bit 18
        assert_eq!(pc.caption, "Frame Caption"); // mask bit 19
        assert!(pc.font.is_none()); // mask bit 20
        assert_eq!(pc.picture.unwrap().picture, [0xa0, 0xb1, 0xc2, 0xd3]); // mask bit 21
        assert_eq!(pc.zoom, 100); // mask bit 22
        assert_eq!(pc.picture_alignment, 4); // mask bit 23
        assert_eq!(pc.picture_tiling, false); // mask bit 24
        assert_eq!(pc.picture_size_mode, 3); // mask bit 25
        assert_eq!(pc.shape_cookie, 0); // mask bit 26
        assert_eq!(pc.draw_buffer, 1000000); // mask bit 27
        Ok(())
    }
}
