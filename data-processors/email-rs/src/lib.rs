//! Email parser

#![warn(missing_docs)]
mod decode;
pub mod header;
mod line;

use header::{Header, TmpHeader};
use line::LineReader;
use std::io::{Read, Write};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};

const CR: u8 = b'\r';
const LF: u8 = b'\n';
const CRLF: &[u8] = &[CR, LF];
const WSP: &[u8] = &[b' ', b'\t'];

#[inline]
/// Removes a single CR, LF or CRLF from the end of the slice
fn without_eol(line: &[u8]) -> &[u8] {
    if line.ends_with(CRLF) {
        &line[0..(line.len() - 2)]
    } else if CRLF.contains(line.last().unwrap()) {
        &line[0..(line.len() - 1)]
    } else {
        line
    }
}

#[inline]
/// Removes whitespace from the beginning of the slice
fn trim_wsp_start(bytes: &[u8]) -> &[u8] {
    let mut ret = bytes;
    while let Some(v) = ret.first() {
        if WSP.contains(v) {
            ret = &ret[1..];
            continue;
        }
        break;
    }
    ret
}

#[inline]
/// Removes whitespace from the end of the slice
fn trim_wsp_end(bytes: &[u8]) -> &[u8] {
    let mut ret = bytes;
    while let Some(v) = ret.last() {
        if WSP.contains(v) {
            ret = &ret[0..(ret.len() - 1)];
            continue;
        }
        break;
    }
    ret
}

#[inline]
/// Removes whitespace from both sides of the slice
fn trim_wsp(bytes: &[u8]) -> &[u8] {
    trim_wsp_end(trim_wsp_start(bytes))
}

/// Enum representing the values of the `Content-Transfer-Encoding` header
pub enum TransferEncoding {
    /// The encoding was not recognized
    Unknown,
    /// 7bit
    SevenBit,
    /// 8bit
    EightBit,
    /// binary
    Binary,
    /// quoted-printable
    QuotedPrintable,
    /// base64
    Base64,
}

#[derive(Debug, Clone)]
/// A MIME part
///
/// Can be the whole message or a portion of it in case the mail has `Content-Type` set to `multipart/*`
pub struct Part {
    headers: Vec<Header>,
    boundary: Option<String>,
    child_of_digest: bool,
}

impl Part {
    fn new<R: Read>(
        r: &mut LineReader<R>,
        child_of_digest: bool,
        allow_empty: bool,
    ) -> Result<Self, std::io::Error> {
        let mut ret = Self {
            headers: Vec::new(),
            boundary: None,
            child_of_digest,
        };
        let mut current_header: Option<TmpHeader> = None;
        loop {
            let line = r.read_line_rtrim()?;
            if line.is_empty() {
                // Headers completed
                if let Some(hdr) = current_header.take() {
                    let hdr: Header = hdr.into();
                    debug!("Header complete: {:?}", hdr);
                    ret.headers.push(hdr);
                } else if !allow_empty {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Invalid mail (no headers)",
                    ));
                }
                break;
            }
            if let Some(ref mut hdr) = current_header.as_mut() {
                if WSP.contains(line.first().unwrap()) {
                    hdr.unfold(line);
                    debug!("Unfolding header: {}", hdr.name);
                    continue;
                }
                let hdr: Header = current_header.take().unwrap().into();
                debug!("Header complete: {:?}", hdr);
                ret.headers.push(hdr);
            }
            current_header = Some(TmpHeader::begin(line));
        }

        // Caching boundary to owned string for perf
        // A part is considered multipart if
        // 1. content-type is multipart/*
        // 2. a boundary parameter is present
        // 3. the parameter value is not empty
        ret.boundary = ret
            .get_header("content-type")
            .filter(|hdr| hdr.value.starts_with("multipart/"))
            .and_then(|hdr| hdr.get_param("boundary"))
            .filter(|bound| !bound.is_empty())
            .map(|bound| bound.to_string());
        debug!("Part begins: {:?}", ret);
        Ok(ret)
    }

    /// Checks if the line is a starting boundary
    fn is_child_start(&self, line: &[u8]) -> bool {
        self.boundary
            .as_ref()
            .map(|bound| line.starts_with(b"--") && &line[2..] == bound.as_bytes())
            .unwrap_or(false)
    }

    /// Checks if the line is a terminating boundary
    fn ends_here(&self, line: &[u8]) -> bool {
        self.boundary
            .as_ref()
            .map(|bound| {
                let blen = bound.len();
                blen + 4 == line.len()
                    && line.starts_with(b"--")
                    && line.ends_with(b"--")
                    && &line[2..(blen + 2)] == bound.as_bytes()
            })
            .unwrap_or(false)
    }

    /// Retrieves the first header matching `name`, if any
    pub fn get_header(&self, name: &str) -> Option<&Header> {
        self.headers.iter().find(|hdr| hdr.name == name)
    }

    /// Returns the part `Content-Type`
    pub fn content_type(&self) -> &str {
        self.get_header("content-type")
            .map(|hdr| hdr.value.as_str())
            .unwrap_or_else(|| {
                if self.child_of_digest {
                    "message/rfc822"
                } else {
                    "text/plain"
                }
            })
    }

    /// Returns whether the part is declared as containing text
    pub fn is_text(&self) -> bool {
        self.content_type().starts_with("text/")
    }

    /// Returns whether the part is declared as containing plaintext
    pub fn is_text_plain(&self) -> bool {
        self.content_type() == "text/plain"
    }

    /// Returns the part charset (as set in the `Content-Type` header)
    /// if the part contains text, `None` otherwise
    pub fn charset(&self) -> Option<&str> {
        if self.is_text() {
            Some(
                self.get_header("content-type")
                    .and_then(|hdr| hdr.get_param("charset"))
                    .unwrap_or("us-ascii"),
            )
        } else {
            None
        }
    }

    /// Returns whether the part is multipart
    ///
    /// Note: multipart parts (other than the message itself) are not exposed
    pub fn is_multipart(&self) -> bool {
        self.boundary.is_some()
    }

    /// Returns the value of the `Content-Disposition` header or the default
    pub fn content_disposition(&self) -> &str {
        if let Some(hdr) = self.get_header("content-disposition") {
            &hdr.value
        } else {
            "inline"
        }
    }

    /// Indicates if the part is inline or attached
    pub fn is_inline(&self) -> bool {
        self.content_disposition() != "attachment"
    }

    /// Returns the encoding of the part as set in the `Content-Transfer-Encoding` header
    pub fn transfer_encoding(&self) -> TransferEncoding {
        match self.content_transfer_encoding() {
            Some(cte) => match cte {
                "7bit" => TransferEncoding::SevenBit,
                "8bit" => TransferEncoding::EightBit,
                "binary" => TransferEncoding::Binary,
                "quoted-printable" => TransferEncoding::QuotedPrintable,
                "base64" => TransferEncoding::Base64,
                _ => TransferEncoding::Unknown,
            },
            _ => TransferEncoding::SevenBit,
        }
    }

    /// Returns the value of the `Content-Transfer-Encoding` header
    pub fn content_transfer_encoding(&self) -> Option<&str> {
        self.get_header("content-transfer-encoding")
            .map(|hdr| hdr.value.as_str())
    }

    /// Checks if the part contains **really** broken headers
    pub fn has_invalid_headers(&self) -> bool {
        self.headers.iter().any(|h| !h.valid)
    }

    /// Checks if the header indicated by name appears more than once
    pub fn has_duplicate_header(&self, name: &str) -> bool {
        self.headers.iter().filter(|hdr| hdr.name == name).count() > 1
    }

    /// Checks if any *resent* header is present
    pub fn is_resent(&self) -> bool {
        [
            "resent-date",
            "resent-from",
            "resent-sender",
            "resent-to",
            "resent-cc",
            "resent-bcc",
            "resent-msg-id",
        ]
        .iter()
        .any(|hdr_name| self.get_header(hdr_name).is_some())
    }

    /// Checks if any *list* header is present
    pub fn is_list(&self) -> bool {
        self.headers
            .iter()
            .any(|hdr| hdr.value.starts_with("list-"))
    }

    /// Returns the decoded value of the date header
    pub fn date(&self) -> Option<Result<time::OffsetDateTime, ()>> {
        self.get_header("date").map(|dt| {
            time::OffsetDateTime::parse(
                &dt.value_nocomments(),
                &time::format_description::well_known::Rfc2822,
            )
            .map_err(|_| ())
        })
    }

    /// Returns the names of this part, in MUA preference order
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.headers
            .iter()
            .filter(|hdr| hdr.name == "content-disposition")
            .flat_map(|hdr| &hdr.params)
            .filter_map(|(k, v)| {
                if k == "filename" {
                    Some(v.as_str())
                } else {
                    None
                }
            })
            .chain(
                self.headers
                    .iter()
                    .filter(|hdr| hdr.name == "content-type")
                    .flat_map(|hdr| &hdr.params)
                    .filter_map(|(k, v)| if k == "name" { Some(v.as_str()) } else { None }),
            )
    }

    /// Returns a summary of the flaws (normally tolerated deviation from the standard)
    /// encountered while processing the part headers
    ///
    /// The returned tuple indicates flaws in (respectively):
    /// * Any header name
    /// * Any header value
    /// * Any header value encoding (for RFC 2231 and RFC 2231)
    /// * Any parameter value (missing value)
    /// * Any error in quoting
    pub fn collect_header_flaws(&self) -> (bool, bool, bool, bool, bool) {
        self.headers
            .iter()
            .fold((false, false, false, false, false), |res, hdr| {
                (
                    res.0 | hdr.ugly_name,
                    res.1 | hdr.ugly_value,
                    res.2 | hdr.bad_encoding,
                    res.3 | hdr.naked_params,
                    res.4 | hdr.ugly_quotes,
                )
            })
    }

    /// Reports if the parts is an attachment and has an explicit `charset`
    ///
    /// This sometimes indicates malware disguised as text
    pub fn is_attachment_with_charset(&self) -> bool {
        !self.is_inline()
            && self
                .get_header("content-type")
                .and_then(|hdr| hdr.get_param("charset"))
                .is_some()
    }
}

/// The mail parser
pub struct Mail<R: Read> {
    r: LineReader<R>,
    message: Part,
    stack: Vec<Part>,
}

impl<R: Read> Mail<R> {
    /// Creates a new parser
    #[instrument(skip_all)]
    pub fn new(r: R) -> Result<Self, std::io::Error> {
        let mut r = LineReader::new(r);
        let message = Part::new(&mut r, false, false)?;
        let stack = vec![message.clone()];
        Ok(Self { r, message, stack })
    }

    /// Walks the MIME structure until a non multipart part is found or the parsing is completed
    #[instrument(skip_all)]
    fn load_next_part(&mut self) -> Result<(), std::io::Error> {
        loop {
            if self.stack.is_empty() {
                // Work complete
                break;
            }
            if !self.stack.last().unwrap().is_multipart() {
                // Not a multipart part
                break;
            }
            // We're either at the preamble or at the epilogue of a multipart part
            loop {
                // Consume preamble/epilogue
                let line = self.r.read_line_rtrim()?;
                if self.stack.last().unwrap().is_child_start(line) {
                    // A new child part starts
                    self.stack.push(Part::new(
                        &mut self.r,
                        self.stack
                            .last()
                            .map(|parent| parent.content_type() == "multipart/digest")
                            .unwrap_or(false),
                        true,
                    )?);
                    break;
                }
                if self.stack.last().unwrap().ends_here(line) {
                    // This multipart part is empty
                    self.stack.pop();
                    break;
                }
                debug!("[invisible] {}", String::from_utf8_lossy(line));
            }
        }
        Ok(())
    }

    /// Returns the message itself
    pub fn message(&self) -> &Part {
        &self.message
    }

    /// Returns the MIME part currently being processed (but not yet dumped nor skipped)
    ///
    /// If called right after [`new`](Self::new), this is the same as [`message`](Self::message)
    pub fn current_part(&mut self) -> Option<&Part> {
        self.load_next_part().ok().and_then(|_| self.stack.last())
    }

    #[instrument(skip_all)]
    /// Decodes the current MIME part and writes it out to the provided writer
    pub fn dump_current_part<W: Write>(
        &mut self,
        w: W,
        max_input_size: u64,
        max_output_size: u64,
    ) -> Result<Option<DumpedPart>, std::io::Error> {
        self.load_next_part()?;
        if self.stack.is_empty() {
            return Ok(None);
        }
        let mut read_bytes = 0u64;
        let mut written_bytes = 0u64;
        // The stack now is a chain of multipart parts and a single non multipart part
        let part = self.stack.pop().unwrap();
        let mut writer = Some(decode::BodyDecoder::new(w, &part));
        let mut has_ugly_qp = false;
        let mut has_ugly_b64 = false;
        let mut unsupported_charset = false;
        let mut has_text_decoder_errors = false;
        loop {
            let line = match self.r.read_line_rtrim() {
                Err(e)
                    if e.kind() == std::io::ErrorKind::UnexpectedEof && self.stack.is_empty() =>
                {
                    // EOF at the topmost level
                    break;
                }
                Err(e) => return Err(e),
                Ok(l) => l,
            };
            read_bytes += line.len() as u64; // cast safe bc line len is limited to MAX_LINE_LEN
            if read_bytes > max_input_size {
                debug!("Read limit exceeded, breaking out");
                break;
            }
            if let Some(parent) = self.stack.last() {
                if parent.is_child_start(line) {
                    // A new child part starts
                    debug!("Part ends (child begins)");
                    self.stack.push(Part::new(
                        &mut self.r,
                        self.stack
                            .last()
                            .map(|parent| parent.content_type() == "multipart/digest")
                            .unwrap_or(false),
                        true,
                    )?);
                    break;
                }
                if parent.ends_here(line) {
                    self.stack.pop();
                    debug!("Parent ends (boundary reached)");
                    break;
                }
            }
            if let Some(ref mut body_writer) = &mut writer {
                if line.is_empty() {
                    // Note: write_all() won't call write() on an empty line
                    // also `let _ =` makes clippy happy
                    let _ = body_writer.write(b"")?;
                } else {
                    body_writer.write_all(line)?;
                }
                if body_writer.written_bytes > max_output_size {
                    debug!("Write limit exceeded, switching to skip mode");
                    writer = None;
                }
            }
        }
        if let Some(ref mut writer) = &mut writer {
            writer.flush()?;
            (
                has_ugly_qp,
                has_ugly_b64,
                unsupported_charset,
                has_text_decoder_errors,
            ) = writer.collect_decoder_errors();
            written_bytes = writer.written_bytes;
        }
        Ok(Some(DumpedPart {
            part,
            has_ugly_qp,
            has_ugly_b64,
            unsupported_charset,
            has_text_decoder_errors,
            read_bytes,
            written_bytes,
        }))
    }
}

#[derive(Debug)]
/// The MIME part that was processed and dumped
pub struct DumpedPart {
    /// The MIME part
    pub part: Part,
    /// Indicates issues with the quoted-printable encoding
    pub has_ugly_qp: bool,
    /// Indicates issues with the base64 encoding
    pub has_ugly_b64: bool,
    /// Indicates that the indicated `charset` was bogus or not supported
    ///
    /// When `true` the part was dumped without UTF-8 conversion
    pub unsupported_charset: bool,
    /// Indicates that there were issues with the UTF-8 conversion
    ///
    /// The decoded text will likely contain replacement characters
    pub has_text_decoder_errors: bool,
    /// The amount of bytes read for this part
    pub read_bytes: u64,
    /// The amount of bytes written for this part
    pub written_bytes: u64,
}
