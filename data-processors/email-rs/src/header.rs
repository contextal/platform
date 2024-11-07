//! Mail header parsers and utility functions
use lazy_static::lazy_static;
use regex::{Captures, Regex};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};

#[derive(Debug, Default)]
/// A mail header currently being parsed
pub(crate) struct TmpHeader {
    /// The header name
    pub name: String,
    value: Vec<u8>,
    valid: bool,
    ugly_name: bool,
}

impl TmpHeader {
    /// Creates a temporary header
    pub fn begin(line: &[u8]) -> Self {
        let mut ret = Self {
            name: String::new(),
            value: Vec::new(),
            valid: true,
            ugly_name: false,
        };
        match line.iter().position(|v| *v == b':') {
            Some(pos) => {
                let (name, value) = line.split_at(pos);
                ret.init_name(name);
                let (_, value) = value.split_at(1);
                ret.update_value(value);
            }
            None => ret.valid = false,
        }
        ret
    }

    /// Updates the header with the followup line (*unfolding*)
    pub fn unfold(&mut self, line: &[u8]) {
        // FIXME: set max value len
        if self.valid {
            self.update_value(line);
        }
    }

    fn init_name(&mut self, name: &[u8]) {
        let name = super::trim_wsp_end(name);
        self.name = name
            .iter()
            .map(|c| {
                if (33..=126).contains(c) {
                    (*c as char).to_ascii_lowercase()
                } else {
                    self.ugly_name = true;
                    char::REPLACEMENT_CHARACTER
                }
            })
            .collect();
    }

    fn update_value(&mut self, value: &[u8]) {
        let newvalue = super::trim_wsp(value);
        if !newvalue.is_empty() {
            if !self.value.is_empty() {
                self.value.push(b' ');
            }
            self.value.extend_from_slice(newvalue);
        }
    }

    fn body_unstructured(&self) -> (String, bool, bool) {
        assert!(self.valid);
        let mut ugly_value = false;

        // Step 1: turn the byte array into a string with replacement
        let body: String = self
            .value
            .iter()
            .map(|c| {
                if (1..=126).contains(c) {
                    *c as char
                } else {
                    ugly_value = true;
                    char::REPLACEMENT_CHARACTER
                }
            })
            .collect();

        // Step 2: handle RFC 2047 encoded tokens
        let (body, bad_encoding) = decode_rfc2047(&body);

        // Step 3: lowercase everything
        let body = body.to_lowercase();

        // Step 4: pack whitespace
        let body: String = body.split_whitespace().collect::<Vec<&str>>().join(" ");

        (body, ugly_value, bad_encoding)
    }

    fn body_structured(&self) -> (String, Vec<(String, String)>, bool, bool, bool, bool) {
        assert!(self.valid);
        let mut ugly_value = false;

        // Step 1: turn the byte array into a string with replacement
        let body: String = self
            .value
            .iter()
            .map(|c| {
                if (0..=126).contains(c) {
                    *c as char
                } else {
                    ugly_value = true;
                    char::REPLACEMENT_CHARACTER
                }
            })
            .collect();

        // Step 2: split value and the rest apart
        let (value, mut remaining) = if let Some((value, params)) = body.split_once(';') {
            (value.to_string(), params)
        } else {
            (body, "")
        };

        // Step 3: split param/value pairs (handling quoted-string values in the process)
        let mut naked_params = false;
        let mut ugly_quotes = false;
        let mut params: Vec<(String, String)> = Vec::new();
        while !remaining.is_empty() {
            let (attr, mut rem) = remaining.split_once('=').unwrap_or_else(|| {
                naked_params = true;
                (remaining, "")
            });

            // Params without value are illegal, but generally disregarded by MUAs
            let mut attr = attr.to_lowercase();
            while let Some((naked, rem)) = attr.split_once(';') {
                naked_params = true;
                params.push((naked.trim().to_string(), "".to_string()));
                attr = rem.to_string();
            }

            rem = rem.trim_start();
            let mut val: String = if rem.starts_with('"') {
                // A very tolerant quoted-string decoder
                let mut last_was_backslash = false;
                let mut val = String::new();
                let mut chars = rem[1..].chars();
                for c in chars.by_ref() {
                    if c == '\\' {
                        if !last_was_backslash {
                            last_was_backslash = true;
                            continue;
                        }
                    } else if c == '"' && !last_was_backslash {
                        break;
                    }
                    last_was_backslash = false;
                    val.push(c);
                }
                rem = chars.as_str().trim_start();
                if !(rem.is_empty() || rem.starts_with(';')) {
                    // The quoted-string has an unquoted tail
                    ugly_quotes = true
                }
                val
            } else {
                String::new()
            };

            // Token values and quoted-string tails
            // Note some MUAs discard the tails, some keep them /shrug
            let (token, rem) = rem.split_once(';').unwrap_or((rem, ""));
            remaining = rem;
            val.push_str(token);

            params.push((attr.trim().to_string(), val));
        }

        // Step 4: merge RFC 2231 headers - TODO (maybe)

        // Step 5: handle RFC 2231 and RFC 2047 encoded tokens (except boundaries)
        let mut bad_encoding = false;
        for (k, v) in params
            .iter_mut()
            .filter(|(k, _)| self.name != "content-type" || k != "boundary")
        {
            if k.ends_with('*') {
                let (dec, ugly) = decode_rfc2231(v);
                bad_encoding |= ugly;
                *v = dec;
            } else if (self.name == "content-type" && k == "name")
                || (self.name == "content-disposition" && k == "filename")
            {
                // RFC 2047 specifically forbids this, but then Outlook happened
                let (dec, ugly) = decode_rfc2047(v);
                bad_encoding |= ugly;
                *v = dec;
            }
        }

        // Step 5: lowercase everything and pack whitespace (except boundaries)
        let value = value
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join(" ");
        for (_, v) in params
            .iter_mut()
            .filter(|(k, _)| self.name != "content-type" || k != "boundary")
        {
            *v = v
                .to_lowercase()
                .split_whitespace()
                .collect::<Vec<&str>>()
                .join(" ");
        }

        (
            value,
            params,
            ugly_value,
            bad_encoding,
            naked_params,
            ugly_quotes,
        )
    }
}

/// Decodes RFC 2047 encoded header values
fn decode_rfc2047(enc: &str) -> (String, bool) {
    // REMINDER: STOP! Don't even try to make this return Cow!
    // It's NOT POSSIBLE due to the replace_all lifetimes
    // See - https://github.com/rust-lang/regex/issues/777
    lazy_static! {
        static ref RE: Regex =
            Regex::new(r"=\?([^?*]+)(?:\*[^?]*)?\?(.)\?([^?]{0,128})\?=").unwrap();
    }
    let mut ugly = false;
    let res = RE.replace_all(enc, |caps: &Captures| {
        let mut decoder = match utf8dec_rs::UTF8Decoder::for_label(&caps[1]) {
            Some(v) => v,
            None => {
                warn!("Invalid RFC2047 encoding \"{}\"", &caps[1]);
                ugly = true;
                return caps[0].to_string();
            }
        };
        let decoded = match caps[2].as_bytes()[0] {
            b'q' | b'Q' => {
                let deq = super::decode::decode_q(caps[3].as_bytes());
                ugly |= deq.1;
                deq.0
            }
            b'b' | b'B' => {
                if let Some(dec) = super::decode::decode_b(&caps[3]) {
                    std::borrow::Cow::from(dec)
                } else {
                    ugly = true;
                    return caps[0].to_string();
                }
            }
            c => {
                warn!("Invalid RFC2047 type \"{}\"", c);
                ugly = true;
                return caps[0].to_string();
            }
        };
        let decoded = decoder.decode_to_string(&decoded);
        ugly |= decoder.has_repl;
        decoded
    });
    (res.to_string(), ugly)
}

/// Decodes RFC 2231 encoded header values
fn decode_rfc2231(enc: &str) -> (String, bool) {
    // REMINDER: STOP! Don't even try to make this return Cow!
    // See notes on decode_rfc2047
    lazy_static! {
        static ref RE: Regex = Regex::new(r"^([^']*)'[^*']*'(.*)").unwrap();
    }
    if let Some(caps) = RE.captures(enc) {
        let cset = if caps[1].is_empty() {
            "us-ascii"
        } else {
            &caps[1]
        };
        let mut decoder = match utf8dec_rs::UTF8Decoder::for_label(cset) {
            Some(v) => v,
            None => {
                warn!("Invalid RFC 2231 encoding {}", cset);
                return (enc.to_string(), true);
            }
        };
        let decoded = urlencoding::decode_binary(caps[2].as_bytes());
        let decoded = decoder.decode_to_string(&decoded);
        return (decoded, decoder.has_repl);
    }
    (enc.to_string(), false)
}

#[derive(Debug, Default, Clone)]
/// A complete mail headers
pub struct Header {
    /// The header name (lowercased)
    pub name: String,
    /// The header value (lowercased and with whitespace normalized)
    pub value: String,
    /// The parameters following the values as a key/value list
    pub params: Vec<(String, String)>,
    /// Indicates if the header is valid
    ///
    /// When false, none of the other fields have sense
    pub valid: bool,
    /// Indicates flaws in the header name (e.g. invalid characters)
    pub ugly_name: bool,
    /// Indicates flaws in the header value
    pub ugly_value: bool,
    /// Indicates flaws in the header value encoding
    pub bad_encoding: bool,
    /// Indicates that some parameter lacks its value
    pub naked_params: bool,
    /// Indicates that some parameter value has inconsistent quoting
    pub ugly_quotes: bool,
}

impl Header {
    /// Returns the first parameter matching `name`
    pub fn get_param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|p| p.0 == name)
            .map(|p| p.1.as_str())
    }

    /// Returns the value with all comments stripped
    pub fn value_nocomments(&self) -> String {
        let mut ret = String::new();
        let mut nopen = 0;
        let mut is_esc = false;
        let mut is_wsp = false;
        for c in self.value.chars() {
            if c == '(' && !is_esc {
                nopen += 1;
                continue;
            }
            if nopen == 0 {
                if c.is_whitespace() {
                    if !ret.is_empty() {
                        is_wsp = true;
                    }
                } else {
                    if is_wsp {
                        ret.push(' ');
                    }
                    ret.push(c);
                    is_wsp = false;
                }
                continue;
            }
            if !is_esc && c == ')' {
                nopen -= 1;
                is_esc = false;
            }
            is_esc = !is_esc && c == '\\';
        }
        ret
    }
}

impl From<TmpHeader> for Header {
    fn from(tmp: TmpHeader) -> Self {
        if !tmp.valid {
            return Self::default();
        }
        let (value, mut params, ugly_value, bad_encoding, naked_params, ugly_quotes) =
            if ["content-type", "content-disposition"].contains(&tmp.name.as_str()) {
                tmp.body_structured()
            } else {
                let (value, ugly_value, bad_encoding) = tmp.body_unstructured();
                (value, Vec::new(), ugly_value, bad_encoding, false, false)
            };

        // Handle RFC 2231 params: move to top and strip the trailing '*'
        if params.iter().any(|(k, _)| k.ends_with('*')) {
            params.sort_by(|a, b| {
                if a.0.ends_with('*') {
                    if b.0.ends_with('*') {
                        std::cmp::Ordering::Equal
                    } else {
                        std::cmp::Ordering::Less
                    }
                } else {
                    std::cmp::Ordering::Greater
                }
            });
            for (k, _) in params.iter_mut().filter(|(k, _)| k.ends_with('*')) {
                (*k).pop();
            }
        }

        Self {
            name: tmp.name,
            value,
            params,
            valid: true,
            ugly_name: tmp.ugly_name,
            ugly_value,
            bad_encoding,
            naked_params,
            ugly_quotes,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_rfc2047() {
        let enc = "Te=?us-ascii*en?q?sting_?==?IsO-8859-1?q?vowels:_=e0=E8=eC=f2=F9?=";
        assert_eq!(
            decode_rfc2047(enc),
            ("Testing vowels: àèìòù".to_string(), true)
        );
        let enc = "Te=?us-ASCII?B?c3Rpbmcg?==?iSo-8859-1*spanish?b?Y2/x52/RYW50cw==?=";
        assert_eq!(
            decode_rfc2047(enc),
            ("Testing coñçoÑants".to_string(), false)
        );
        let enc = "=?invalid?q?asd?=";
        assert_eq!(decode_rfc2047(enc), (enc.to_string(), true));
        let enc = "=?us-ascii?F?invalid?=";
        assert_eq!(decode_rfc2047(enc), (enc.to_string(), true));
    }

    #[test]
    fn test_rfc2231() {
        let enc = "utf-8'en'Testing%20symbols: %E2%82%aC %c2%a5";
        assert_eq!(
            decode_rfc2231(enc),
            ("Testing symbols: € ¥".to_string(), false)
        );
        let enc = "us-ascii''Testing%20nolang";
        assert_eq!(decode_rfc2231(enc), ("Testing nolang".to_string(), false));
        let enc = "'en'Testing%20noenc";
        assert_eq!(decode_rfc2231(enc), ("Testing noenc".to_string(), false));
        let enc = "invalid'en'Testing%20invenc";
        assert_eq!(decode_rfc2231(enc), (enc.to_string(), true));
        let enc = "''Testing%20allmissing";
        assert_eq!(
            decode_rfc2231(enc),
            ("Testing allmissing".to_string(), false)
        );

        // FIXME: this should succeed - see ticket #12
        //let enc = "us-ascii'en'invalid_enc: %a3";
        //assert_eq!(decode_rfc2231(enc), ("invalid_enc: �".to_string(), false));
    }

    #[test]
    fn test_hdr_subject() {
        let mut header = TmpHeader::begin(b"Subject : H=?us-ascii?q?eLlo_ _ ?= \t");
        header.unfold(b" \t  world=?us-ascii?b?ISEh?=");
        let header: Header = header.into();
        assert_eq!(header.name, "subject");
        assert_eq!(header.value, "hello world!!!");
        assert!(header.params.is_empty());
    }

    #[test]
    fn test_hdr_to() {
        let header = TmpHeader::begin(b"To: \"=?US-ASCII*EN?Q?Keith_Moore?=\" <moore@cs.uTK.edU>");
        let header: Header = header.into();
        assert_eq!(header.name, "to");
        assert_eq!(header.value, "\"keith moore\" <moore@cs.utk.edu>");
        assert!(header.params.is_empty());
    }

    #[test]
    fn test_hdr_cd() {
        let mut header = TmpHeader::begin(b"CoNTeNT-dispOSITION : ");
        header.unfold(b"    aTTaChmEnt   ;invalid-param;");
        header.unfold(b"   ");
        header.unfold(b"\tfileName = \"=?uTF-8?B?wqnFgiHCosS4wrXigqwuZXhl?=\"   ; ");
        header.unfold(b" title*=us-ascii''Click%20Here");
        let header: Header = header.into();
        assert_eq!(header.params[0].0, "title");
        assert_eq!(header.params[0].1, "click here");
        assert_eq!(header.name, "content-disposition");
        assert_eq!(header.value, "attachment");
        assert_eq!(header.params.len(), 3);
        assert_eq!(header.params[1].0, "invalid-param");
        assert_eq!(header.params[1].1, "");
        assert_eq!(header.params[2].0, "filename");
        assert_eq!(header.params[2].1, "©ł!¢ĸµ€.exe");
    }

    #[test]
    fn test_ugliness() {
        let header: Header = TmpHeader::begin(b"Invalid ...").into();
        assert!(!header.valid);
        let header: Header = TmpHeader::begin(b"Val!d: but strange").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        let header: Header = TmpHeader::begin(b"Va lid: but ugly").into();
        assert!(header.valid);
        assert!(header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        let header: Header = TmpHeader::begin(b"Valid: but ugly\x00value").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(header.ugly_value);
        assert!(!header.bad_encoding);
        let header: Header = TmpHeader::begin(b"Valid-no-value:").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        let header: Header = TmpHeader::begin(b"Subject: =?us-ascii?B?B,AD_QP?=").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(header.bad_encoding);
        let header: Header = TmpHeader::begin(b"Subject: =?us-ascii?B?aGVsbG8=?=").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        let header: Header = TmpHeader::begin(b"Subject: =?us-ascii?q?with space?=").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(header.bad_encoding);
        let header: Header = TmpHeader::begin(b"Subject: =?us-ascii?Q?=30=31=32?=").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        let header: Header = TmpHeader::begin(b"Subject: =?us-ascii?Q?=3A?=").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        let header: Header = TmpHeader::begin(b"Subject: =?us-ascii?Q?=3a?=").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(header.bad_encoding);

        let header: Header = TmpHeader::begin(b"content-type: text/plain; param=value").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        assert!(!header.naked_params);
        assert!(!header.ugly_quotes);
        let header: Header = TmpHeader::begin(b"content-type: text/plain; param=\"value\"").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        assert!(!header.naked_params);
        assert!(!header.ugly_quotes);
        let header: Header = TmpHeader::begin(b"content-type: text/plain; param=\"va\"lue").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        assert!(!header.naked_params);
        assert!(header.ugly_quotes);
        let header: Header = TmpHeader::begin(b"content-type: text/plain; naked").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        assert!(header.naked_params);
        assert!(!header.ugly_quotes);
        let header: Header = TmpHeader::begin(b"content-type: text/plain; naked; k1=v1").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        assert!(header.naked_params);
        assert!(!header.ugly_quotes);
        let header: Header = TmpHeader::begin(b"content-type: text/plain; k1=v1; naked").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        assert!(header.naked_params);
        assert!(!header.ugly_quotes);
        let header: Header =
            TmpHeader::begin(b"content-type: text/plain; k1=v1; naked; k2=v2").into();
        assert!(header.valid);
        assert!(!header.ugly_name);
        assert!(!header.ugly_value);
        assert!(!header.bad_encoding);
        assert!(header.naked_params);
        assert!(!header.ugly_quotes);
    }

    #[test]
    fn test_nocomment() {
        let mut hdr = Header::default();
        hdr.value = String::from("no comments");
        assert_eq!(hdr.value_nocomments(), "no comments");
        hdr.value = String::from("some (stripped) comments");
        assert_eq!(hdr.value_nocomments(), "some comments");
        hdr.value = String::from("nested (level1 (level2)) comments");
        assert_eq!(hdr.value_nocomments(), "nested comments");
        hdr.value = String::from(r"escaped (comment with \(\)\ \\) comments");
        assert_eq!(hdr.value_nocomments(), "escaped comments");
        hdr.value =
            String::from(r"escaped+nested (comment with \(\)\ \\ (and another \()) comments");
        assert_eq!(hdr.value_nocomments(), "escaped+nested comments");
        hdr.value = String::from(r"unterminated (comment ");
        assert_eq!(hdr.value_nocomments(), "unterminated");
        hdr.value = String::from(r"nested unterminated ((((comment ");
        assert_eq!(hdr.value_nocomments(), "nested unterminated");
        hdr.value = String::from(r"unbalanced ((())))) comment ");
        assert_eq!(hdr.value_nocomments(), "unbalanced )) comment");
        hdr.value = String::from(r"a \(strange) case");
        assert_eq!(hdr.value_nocomments(), r"a \ case");
    }
}
