//! Mail data decoders
//!
//! This module contains decoders for different kinds of encodings used in emails
use lazy_static::lazy_static;
use regex::bytes::{Captures as BinCaptures, Regex as BinRegex};
use std::borrow::Cow;
use std::io::Write;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};

static QLUT: &[u8] = &[
    0x00, 0x01, 0x02, 0x03, 0x04, 0x5, 0x06, 0x07, 0x08, 0x09, // 0-9
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // :;<=>?@
    0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, // A-F
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // G-P
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // Q-Z
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // [\]^_`
    0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, // a-f
];

/// Decodes a RFC 2047 header value Q-encoded portion (similar to *quoted-printable*)
///
/// Note: RFC 2047 is very strict about Q, however MUAs are forgiving; this decoder
/// tolerates invalid encodings but flags them
pub fn decode_q(enc: &[u8]) -> (Cow<'_, [u8]>, bool) {
    lazy_static! {
        static ref RE: BinRegex = BinRegex::new(r"(?-u)(_)|(?:=([0-9A-Fa-f][0-9A-Fa-f]))").unwrap();
    }
    let mut ugly = enc.contains(&b' ');
    let ret = RE.replace_all(enc, |caps: &BinCaptures| {
        if caps.get(2).is_none() {
            [b' ']
        } else {
            let hi = QLUT[(caps[2][0] - b'0') as usize];
            let lo = QLUT[(caps[2][1] - b'0') as usize];
            ugly |= ((hi | lo) & 0x10) != 0;
            [((hi & 0xf) << 4) | (lo & 0xf)]
        }
    });
    (ret, ugly)
}

/// Decodes a *quoted-printable* encoded MIME part body
///
/// This is an intentionally lax parser
pub fn decode_quoted_printable(enc: &[u8]) -> (Cow<'_, [u8]>, bool) {
    lazy_static! {
        static ref RE: BinRegex = BinRegex::new(r"(?-u)=([0-9A-Fa-f][0-9A-Fa-f])").unwrap();
    }
    let mut ugly = false;
    let ret = RE.replace_all(enc, |caps: &BinCaptures| {
        let hi = QLUT[(caps[1][0] - b'0') as usize];
        let lo = QLUT[(caps[1][1] - b'0') as usize];
        ugly |= ((hi | lo) & 0x10) != 0;
        [((hi & 0xf) << 4) | (lo & 0xf)]
    });
    (ret, ugly)
}

#[rustfmt::skip]
static B64LUT: &[u8] = &[
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 0-15
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 16-31
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,  62, 255, 255, 255,  63, // 31-47
     52,  53,  54,  55,  56,  57,  58,  59,  60,  61, 255, 255, 255,  64, 255, 255, // 48-63
    255,   0,   1,   2,   3,   4,   5,   6,   7,   8,   9,  10,  11,  12,  13,  14, // 64-79
     15,  16,  17,  18,  19,  20,  21,  22,  23,  24,  25, 255, 255, 255, 255, 255, // 80-95
    255,  26,  27,  28,  29,  30,  31,  32,  33,  34,  35,  36,  37,  38,  39,  40, // 96-111
     41,  42,  43,  44,  45,  46,  47,  48,  49,  50,  51, 255, 255, 255, 255, 255, // 112-127

    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 128-143
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 144-159
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 160-175
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 176-191
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 192-207
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 208-223
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 224-239
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 240-255
];

fn decode_base64_chunk(chunk: &[u8], dec: &mut Vec<u8>) -> Option<bool> {
    let mut has_padding = false;
    let b1 = B64LUT[chunk[0] as usize];
    let b2 = B64LUT[chunk[1] as usize];
    let b3 = B64LUT[chunk[2] as usize];
    let b4 = B64LUT[chunk[3] as usize];
    if (b1 | b2 | b3 | b4) & 0b1000_0000 != 0 {
        return None;
    }
    if (b1 | b2) & (1 << 6) != 0 {
        return None;
    }
    let out: [u8; 3] = [(b1 << 2) | (b2 >> 4), (b2 << 4) | (b3 >> 2), (b3 << 6) | b4];
    if b4 & 0b0100_0000 != 0 {
        has_padding = true;
        if b3 & 0b0100_0000 != 0 {
            dec.push(out[0]);
        } else {
            dec.extend_from_slice(&out[0..2]);
        }
    } else if b3 & (1 << 6) != 0 {
        return None;
    } else {
        dec.extend_from_slice(&out);
    }
    Some(has_padding)
}

/// Decodes a RFC 2047 header value B-encoded portion (i.e. *base64*)
///
/// Note: this parser is strict
pub fn decode_b(enc: &str) -> Option<Vec<u8>> {
    // A rather picky decoder, except for padding
    let mut ret: Vec<u8> = Vec::with_capacity(enc.len() / 4 * 3);
    let mut chunks = enc.as_bytes().chunks_exact(4);
    let mut padding_seen = false;
    for chunk in chunks.by_ref() {
        if padding_seen {
            return None;
        }
        padding_seen = decode_base64_chunk(chunk, &mut ret)?;
    }
    let reminder = chunks.remainder();
    if !reminder.is_empty() {
        if padding_seen || reminder.len() < 2 {
            return None;
        }
        let chunk: [u8; 4] = if reminder.len() == 2 {
            [reminder[0], reminder[1], b'=', b'=']
        } else {
            [reminder[0], reminder[1], reminder[2], b'=']
        };
        decode_base64_chunk(&chunk, &mut ret)?;
    }
    Some(ret)
}

/// Decodes a *base64* encoded MIME part body
///
/// This is an intentionally **extremely lax** parser: any char not in the
/// alphabet is silently discarded - this is in line with most MUAs
pub fn decode_base64(enc: &[u8], prev: &[u8]) -> (Vec<u8>, Vec<u8>, bool) {
    // A very forgiving decoder, which skips unacceptable chars
    assert!(prev.len() < 4);
    let mut ret: Vec<u8> = Vec::with_capacity((enc.len() / 4 + 1) * 3);
    let mut chunk = [0u8; 4];
    let mut pos = prev.len();
    let mut ugly = false;
    if pos > 0 {
        chunk[0..pos].copy_from_slice(prev);
    }
    for c in enc {
        if B64LUT[*c as usize] & 0b1000_0000 == 0 {
            chunk[pos] = *c;
            pos += 1;
            if pos == 4 {
                if decode_base64_chunk(&chunk, &mut ret).is_none() {
                    ugly = true;
                }
                pos = 0;
            }
        } else {
            ugly = true;
        }
    }
    (ret, chunk[0..pos].to_vec(), ugly)
}

enum BodyEnc {
    B64,
    QP,
    PassThru,
}

/// Decoder for MIME part bodies (`Write` wrapper)
///
/// The following actions are performed on the body
/// 1. Decoding from *quoted-printable* or *base64* into binary - if encoded
/// 2. Text conversion from any (supported) charset to to UTF-8 - if inline text
pub struct BodyDecoder<W: Write> {
    w: TextDecoder<W>,
    chunk: [u8; 4],
    chunk_cnt: usize,
    enc: BodyEnc,
    ugly_qp: bool,
    ugly_b64: bool,
    pub written_bytes: u64,
}

impl<W: Write> BodyDecoder<W> {
    /// Creates a new body decoder
    pub fn new(w: W, part: &super::Part) -> Self {
        let enc = match part.transfer_encoding() {
            super::TransferEncoding::Base64 => BodyEnc::B64,
            super::TransferEncoding::QuotedPrintable => BodyEnc::QP,
            _ => BodyEnc::PassThru,
        };
        Self {
            w: TextDecoder::new(w, part),
            chunk: [0u8; 4],
            chunk_cnt: 0,
            enc,
            ugly_qp: false,
            ugly_b64: false,
            written_bytes: 0,
        }
    }

    pub fn collect_decoder_errors(&self) -> (bool, bool, bool, bool) {
        (
            self.ugly_qp,
            self.ugly_b64,
            self.w.unsupported_charset,
            self.w.decoder_error,
        )
    }
}

impl<W: Write> Write for BodyDecoder<W> {
    fn write(&mut self, line: &[u8]) -> Result<usize, std::io::Error> {
        match self.enc {
            BodyEnc::QP => {
                let mut qp = super::trim_wsp_end(line);
                let add_eol = if qp.ends_with(b"=") {
                    qp = &qp[0..(qp.len() - 1)];
                    false
                } else {
                    true
                };
                let (dec, ugly) = decode_quoted_printable(qp);
                self.ugly_qp |= ugly;
                self.w.write_all(&dec)?;
                self.written_bytes += dec.len() as u64; // safe bc line len is limited to MAX_LINE_LEN
                if add_eol {
                    self.w.write_all(b"\n")?;
                    self.written_bytes += 1;
                }
            }
            BodyEnc::B64 => {
                let (dec, reminder, ugly) = decode_base64(line, &self.chunk[0..self.chunk_cnt]);
                self.ugly_b64 |= ugly;
                assert!(reminder.len() < 4);
                self.chunk_cnt = reminder.len();
                self.chunk[0..self.chunk_cnt].copy_from_slice(&reminder);
                self.w.write_all(&dec)?;
                self.written_bytes += dec.len() as u64; // safe bc line len is limited to MAX_LINE_LEN
            }
            BodyEnc::PassThru => {
                self.w.write_all(line)?;
                self.w.write_all(b"\n")?; // FIXME only for text?
                self.written_bytes += line.len() as u64 + 1; // safe bc line len is limited to MAX_LINE_LEN
            }
        }
        Ok(line.len())
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        if matches!(self.enc, BodyEnc::B64) && self.chunk_cnt >= 2 {
            assert!(self.chunk_cnt < 4);
            self.chunk[3] = b'=';
            if self.chunk_cnt == 2 {
                self.chunk[2] = b'=';
            }
            let mut dec: Vec<u8> = Vec::with_capacity(2);
            if decode_base64_chunk(&self.chunk, &mut dec).is_none() {
                self.ugly_b64 = true;
            }
            self.w.write_all(&dec)?;
        }
        self.w.flush()
    }
}

/// To-UTF-8 text decoder
struct TextDecoder<W: Write> {
    w: W,
    decoder: Option<utf8dec_rs::UTF8Decoder>,
    buf: [u8; 1024],
    unsupported_charset: bool,
    decoder_error: bool,
}

impl<W: Write> TextDecoder<W> {
    fn new(w: W, part: &super::Part) -> Self {
        let mut unsupported_charset = false;
        let decoder = if part.is_inline() && part.is_text_plain() {
            let decoder = part.charset().and_then(utf8dec_rs::UTF8Decoder::for_label);
            unsupported_charset = decoder.is_none();
            decoder
        } else {
            // Note: some malware ships attachments marked as text/plain with
            // a bogus charset (e.g. UTF-16LE) which we'd corrupt here
            None
        };
        Self {
            w,
            decoder,
            buf: [0u8; 1024],
            unsupported_charset,
            decoder_error: false,
        }
    }
}

impl<W: Write> Write for TextDecoder<W> {
    fn write(&mut self, src: &[u8]) -> Result<usize, std::io::Error> {
        if let Some(ref mut decoder) = &mut self.decoder {
            let mut src = src;
            while !src.is_empty() {
                let (inlen, outlen) = decoder.decode(src, &mut self.buf);
                self.w.write_all(&self.buf[0..outlen])?;
                src = &src[inlen..];
            }
        } else {
            self.w.write_all(src)?;
        }
        Ok(src.len())
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        if let Some(ref mut decoder) = &mut self.decoder {
            let outlen = decoder.flush(&mut self.buf).unwrap();
            self.w.write_all(&self.buf[0..outlen])?;
            if decoder.has_repl {
                warn!("Malformed text using decoder \"{}\"", decoder.charset);
                self.decoder_error = true;
            }
        }
        self.w.flush()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_decode_base64_chunk() {
        let mut out = Vec::<u8>::new();
        assert_eq!(
            decode_base64_chunk(b"\x30\x4c\x55\x3d", &mut out),
            Some(true)
        );
        assert_eq!(out, [0xd0, 0xb5]);
    }

    #[test]
    fn test_decode_b() {
        assert_eq!(decode_b("").unwrap(), b"");
        assert!(decode_b("?AAA").is_none());
        assert!(decode_b("A?AA").is_none());
        assert!(decode_b("AA?A").is_none());
        assert!(decode_b("AAA?").is_none());
        assert!(decode_b("A").is_none());
        assert_eq!(
            decode_b("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopwrstuvwxyz0123456789+/").unwrap(),
            &[
                0, 16, 131, 16, 81, 135, 32, 146, 139, 48, 211, 143, 65, 20, 147, 81, 85, 151, 97,
                150, 155, 113, 215, 159, 130, 24, 163, 146, 89, 167, 162, 156, 43, 178, 219, 175,
                195, 28, 179, 211, 93, 183, 227, 158, 187, 243, 223, 191
            ]
        );
        assert_eq!(decode_b("VarM").unwrap(), &[85, 170, 204]);
        assert_eq!(decode_b("MVar").unwrap(), &[49, 86, 171]);
        assert_eq!(decode_b("rMVa").unwrap(), &[172, 197, 90]);
        assert_eq!(decode_b("arMV").unwrap(), &[106, 179, 21]);
        assert!(decode_b("aCaB4===").is_none());
        assert!(decode_b("aCaB42==").is_some());
        assert!(decode_b("aCaB423=").is_some());
    }

    #[test]
    fn test_decode_base64() {
        assert_eq!(decode_base64(b"", &[]), (vec![], vec![], false));
        assert_eq!(decode_base64(b"A", &[]), (vec![], vec![b'A'], false));
        assert_eq!(decode_base64(b"~A~~", &[]), (vec![], vec![b'A'], true));
        assert_eq!(decode_base64(b"~a~?C", &[]), (vec![], b"aC".to_vec(), true));
        assert_eq!(
            decode_base64(b"~a~?C,a", &[]),
            (vec![], b"aCa".to_vec(), true)
        );
        assert_eq!(
            decode_base64(b"~a~?C,aB", &[]),
            (vec![104, 38, 129], vec![], true)
        );
        assert_eq!(
            decode_base64(b"CaB", b"a"),
            (vec![104, 38, 129], vec![], false)
        );
        assert_eq!(
            decode_base64(b"CaBx", b"a"),
            (vec![104, 38, 129], b"x".to_vec(), false)
        );
        assert_eq!(
            decode_base64(b"aBxx", b"aC"),
            (vec![104, 38, 129], b"xx".to_vec(), false)
        );
        assert_eq!(
            decode_base64(b"Bxxx", b"aCa"),
            (vec![104, 38, 129], b"xxx".to_vec(), false)
        );
        assert_eq!(
            decode_base64(b"YQ==Yg==Yw==", &[]),
            (b"abc".to_vec(), vec![], false)
        )
    }

    #[test]
    fn test_decode_q() {
        assert_eq!(decode_q(b""), (Cow::from(b"".as_slice()), false));
        assert_eq!(decode_q(b"asd"), (Cow::from(b"asd".as_slice()), false));
        assert_eq!(
            decode_q(b"=31=3337"),
            (Cow::from(b"1337".as_slice()), false)
        );
        assert_eq!(decode_q(b"=2E"), (Cow::from(b".".as_slice()), false));
        assert_eq!(decode_q(b"=2e"), (Cow::from(b".".as_slice()), true));
        assert_eq!(decode_q(b"=20"), (Cow::from(b" ".as_slice()), false));
        assert_eq!(decode_q(b"_"), (Cow::from(b" ".as_slice()), false));
        assert_eq!(decode_q(b" "), (Cow::from(b" ".as_slice()), true));
    }

    #[test]
    fn test_decode_quoted_printable() {
        assert_eq!(
            decode_quoted_printable(b""),
            (Cow::from(b"".as_slice()), false)
        );
        assert_eq!(
            decode_quoted_printable(b"asd"),
            (Cow::from(b"asd".as_slice()), false)
        );
        assert_eq!(
            decode_quoted_printable(b"=31=3337"),
            (Cow::from(b"1337".as_slice()), false)
        );
        assert_eq!(
            decode_quoted_printable(b"=2E"),
            (Cow::from(b".".as_slice()), false)
        );
        assert_eq!(
            decode_quoted_printable(b"=2e"),
            (Cow::from(b".".as_slice()), true)
        );
        assert_eq!(
            decode_quoted_printable(b"=20"),
            (Cow::from(b" ".as_slice()), false)
        );
        assert_eq!(
            decode_quoted_printable(b"_"),
            (Cow::from(b"_".as_slice()), false)
        );
        assert_eq!(
            decode_quoted_printable(b" "),
            (Cow::from(b" ".as_slice()), false)
        );
    }
}
