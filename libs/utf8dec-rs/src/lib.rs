//! A pragmatic text decoder based on iconv
//!
//! Notes:
//! * a transaprent iconv descriptor cache is added to support specific workloads (VBA)
//! * iconv was tested to be slightly faster than encoding_rs (~1.5%)
//! * iconv supports UTF-7 and a ton of ancient charsets which we must support
//!   but encodings_rs does not (want to) care about

#![warn(missing_docs)]
mod ffi;

use std::io::{Read, Write};
use tracing::warn;

/// Buffer size for reader and writer
const BUFSIZ: usize = 4096;
/// Maximum amount of cached iconv descriptors
const MAXCACHE: usize = 32;

/// Result of the conversion: i.e. why the conversion stopped
enum ConvResult {
    /// Input is fully consumed
    InputEmpty,
    /// Output buffer is full
    OutputFull,
    /// Invalid sequence encountered
    InvalidSeq,
    /// The input contains only a fragment of a character
    IncompleteSeq,
}

impl From<usize> for ConvResult {
    fn from(val: usize) -> Self {
        if val as isize != -1 {
            ConvResult::InputEmpty
        } else {
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap();
            match errno {
                libc::E2BIG => ConvResult::OutputFull,
                libc::EILSEQ => ConvResult::InvalidSeq,
                libc::EINVAL => ConvResult::IncompleteSeq,
                _ => unreachable!(),
            }
        }
    }
}

/// Wrapper to iconv() in normal mode
fn iconv_conv(conv: ffi::iconv_t, src: &[u8], dst: &mut [u8]) -> (ConvResult, usize, usize) {
    let mut src_ptr = src.as_ptr() as *const ::std::os::raw::c_char;
    let mut src_size = src.len();
    let mut dst_ptr = dst.as_mut_ptr() as *mut ::std::os::raw::c_char;
    let mut dst_size = dst.len();
    let res = unsafe {
        ffi::iconv(
            conv,
            &mut src_ptr,
            &mut src_size,
            &mut dst_ptr,
            &mut dst_size,
        )
    };
    let consumed = src.len() - src_size;
    let produced = dst.len() - dst_size;
    let ret: ConvResult = res.into();
    (ret, consumed, produced)
}

/// Wrapper to iconv() in flush mode
fn iconv_flush(conv: ffi::iconv_t, dst: &mut [u8]) -> (ConvResult, usize) {
    let mut dst_ptr = dst.as_mut_ptr() as *mut ::std::os::raw::c_char;
    let mut dst_size = dst.len();
    let res = unsafe {
        ffi::iconv(
            conv,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut dst_ptr,
            &mut dst_size,
        )
    };
    let produced = dst.len() - dst_size;
    let ret: ConvResult = res.into();
    (ret, produced)
}

/// Wrapper to iconv() in reset mode
fn iconv_reset(conv: ffi::iconv_t) {
    unsafe {
        ffi::iconv(
            conv,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
}

static DECODER_CACHE: std::sync::Mutex<Vec<UTF8Decoder>> = std::sync::Mutex::new(Vec::new());

/// Wrappage due to *void_ptr not being Send
struct IconvWrap(ffi::iconv_t);
unsafe impl Send for IconvWrap {}

/// A decoder from any (iconv supported) charset to UTF-8
///
/// This is NOT a validator, but a best effort, possibly lossy, text decoder
///
/// Invalid input sequences are marked with � (U+FFFD)
pub struct UTF8Decoder {
    /// The source charset actually used
    pub charset: std::borrow::Cow<'static, str>,
    /// Iconv handle
    cd: IconvWrap,
    /// Indicates some replacement occurred
    pub has_repl: bool,
    /// Buffer for partial character sequences
    partial: [u8; 8],
    /// The size of the partial character sequence
    partial_len: usize,
    /// Used for squashing subsequent replacement chars
    last_was_repl: bool,
}

impl UTF8Decoder {
    /// Creates a new decoder for charset label
    ///
    /// Note: a pragmatic approach is employed here and often misused encodings are mapped
    /// to their more widely used superset (alias).
    pub fn for_label(label: &str) -> Option<Self> {
        if label.is_empty() {
            // Needed because iconv supports the "" encoding as an alias for UTF-8
            return None;
        }
        let upc_label = label.to_uppercase();
        ENCMAP
            .binary_search_by_key(&upc_label.as_str(), |&(k, _)| k)
            .ok()
            .and_then(|i| Self::for_label_noalias(ENCMAP[i].1.into()))
            .or_else(|| Self::for_label_noalias(upc_label.into()))
    }

    /// Creates a new decoder for charset label (without unaliasing)
    fn for_label_noalias(charset: std::borrow::Cow<'static, str>) -> Option<Self> {
        if let Ok(mut cache) = DECODER_CACHE.try_lock() {
            if let Some(pos) = cache.iter().position(|dec| dec.charset == charset) {
                let mut cached_dec = cache.remove(pos);
                cached_dec.reset();
                return Some(cached_dec);
            }
        }
        let from = std::ffi::CString::new(charset.as_ref()).ok()?;
        let to = std::ffi::CString::new("UTF-8").ok()?;
        let cd = unsafe { ffi::iconv_open(to.as_ptr(), from.as_ptr()) };
        if cd as isize == -1 {
            None
        } else {
            Some(Self {
                charset,
                cd: IconvWrap(cd),
                has_repl: false,
                partial: [0u8; 8],
                partial_len: 0,
                last_was_repl: false,
            })
        }
    }

    /// Creates a new decoder for Windows code page
    ///
    /// Note: unaliasing applies, unmappable code pages are treated as WINDOWS-1252
    pub fn for_windows_cp(cp: u16) -> Self {
        let label = CPMAP
            .binary_search_by_key(&cp, |&(k, _)| k)
            .map(|i| CPMAP[i].1)
            .unwrap_or("WINDOWS-1252");
        Self::for_label(label).expect("Internal error: cannot create windows codepage decoder")
    }

    /// Decodes a chunk of data (streaming mode)
    pub fn decode(&mut self, src: &[u8], dst: &mut [u8]) -> (usize, usize) {
        let mut consumed = 0usize;
        let mut produced = 0usize;

        // Handle previous partial sequences
        if self.partial_len > 0 {
            let borrow = (self.partial.len() - self.partial_len).min(src.len());
            let in_now = self.partial_len + borrow;
            self.partial[self.partial_len..in_now].copy_from_slice(&src[0..borrow]);
            let (res, c, p) = iconv_conv(self.cd.0, &self.partial[0..in_now], dst);
            produced = p;
            if p > 0 {
                self.last_was_repl = false;
            }
            if c > 0 {
                // If something was consumed from partial:
                // * any error is irrelevant (will be reissued and handled later)
                // * partial has exhausted its task and we drop it
                // * iconv guarantees that c > partial_len
                // * however we may have broken the guarantee when handling
                //   InvalidSeq on the previous loop
                if self.partial_len > c {
                    // We've only consumed from within partial (no borrow)
                    // But we did not consume everything
                    // So we shift the tail to the start and leave the rest
                    // to be handled in the next loop
                    self.partial.copy_within(c..self.partial_len, 0);
                    self.partial_len -= c;
                    return (0, produced);
                }
                consumed = c - self.partial_len;
                self.partial_len = 0;
            } else {
                // Nothing was consumed, time to check the result
                match res {
                    ConvResult::InvalidSeq if dst.len() >= 3 || self.last_was_repl => {
                        if !self.last_was_repl {
                            // Stuff a replacement char (we have room for it)
                            dst[0] = 0xef;
                            dst[1] = 0xbf;
                            dst[2] = 0xbd;
                            produced += 3;
                            self.last_was_repl = true;
                        }
                        self.has_repl = true;
                        // Drop the first byte from partial and shift everything
                        self.partial.copy_within(1..in_now, 0);
                        self.partial_len = in_now - 1;
                    }
                    _ => {
                        // Either we need more input or we're out of output space
                        self.partial_len = in_now;
                    }
                }
                return (borrow, produced);
            }
        }

        // Handle the actual input
        let (res, c, p) = iconv_conv(self.cd.0, &src[consumed..], &mut dst[produced..]);
        consumed += c;
        produced += p;
        if p > 0 {
            self.last_was_repl = false;
        }
        match res {
            ConvResult::IncompleteSeq => {
                // Incomplete sequence at the end of input, move it to partial
                let avail = self.partial.len().min(src[consumed..].len());
                self.partial[0..avail].copy_from_slice(&src[consumed..(consumed + avail)]);
                self.partial_len = avail;
                consumed += avail;
            }
            ConvResult::InvalidSeq if dst[produced..].len() >= 3 || self.last_was_repl => {
                if !self.last_was_repl {
                    // Stuff a replacement char (we have room for it)
                    dst[produced] = 0xef;
                    dst[produced + 1] = 0xbf;
                    dst[produced + 2] = 0xbd;
                    produced += 3;
                    self.last_was_repl = true;
                }
                self.has_repl = true;
                // Drop the first char from input
                consumed += 1;
            }
            _ => {}
        }
        (consumed, produced)
    }

    /// Flushes the internal state (streaming mode)
    ///
    /// Note: may fail (and return `None`) if the output buffer is too small
    pub fn flush(&mut self, dst: &mut [u8]) -> Option<usize> {
        let mut produced = 0usize;
        let mut partial_at = 0usize;
        while partial_at < self.partial_len {
            let (res, c, p) = iconv_conv(
                self.cd.0,
                &self.partial[partial_at..self.partial_len],
                &mut dst[produced..],
            );
            partial_at += c;
            produced += p;
            match res {
                ConvResult::InputEmpty => break,
                ConvResult::OutputFull => return None,
                ConvResult::InvalidSeq | ConvResult::IncompleteSeq => {
                    if !self.last_was_repl {
                        if let Some(out) = dst.get_mut(produced..(produced + 3)) {
                            // Stuff a replacement char (we have room for it)
                            out[0] = 0xef;
                            out[1] = 0xbf;
                            out[2] = 0xbd;
                        } else {
                            return None;
                        }
                        produced += 3;
                    }
                    self.last_was_repl = true;
                    self.has_repl = true;
                    if matches!(res, ConvResult::InvalidSeq) {
                        partial_at += 1;
                    } else {
                        break;
                    }
                }
            }
        }
        self.partial_len = 0;

        // Note: iconv_flush may only produce output when converting to encodings with a
        // shift state like JIS, never for UTF-8.
        // It's invoked here just as a best practice, the result is ignored.
        let (_, p) = iconv_flush(self.cd.0, &mut dst[produced..]);
        produced += p;
        Some(produced)
    }

    /// Resets the internal state
    pub fn reset(&mut self) {
        self.partial_len = 0;
        self.has_repl = false;
        iconv_reset(self.cd.0);
    }

    /// Decodes a full buffer into a string (non-streaming-mode)
    pub fn decode_to_string(&mut self, src: &[u8]) -> String {
        let mut ret = String::new();
        let mut buf = [0u8; 1024];
        self.reset();
        let mut src = src;
        while !src.is_empty() {
            let (c, p) = self.decode(src, &mut buf);
            let s = std::str::from_utf8(&buf[0..p]).unwrap();
            ret.push_str(s);
            src = &src[c..];
        }
        let p = self.flush(&mut buf).unwrap();
        let s = std::str::from_utf8(&buf[0..p]).unwrap();
        ret.push_str(s);
        ret
    }
}

impl Drop for UTF8Decoder {
    fn drop(&mut self) {
        if let Ok(mut cache) = DECODER_CACHE.try_lock() {
            if cache.len() < MAXCACHE {
                cache.push(Self {
                    charset: self.charset.clone(),
                    cd: IconvWrap(self.cd.0),
                    has_repl: false,
                    partial: [0u8; 8],
                    partial_len: 0,
                    last_was_repl: false,
                });
                return;
            }
        }
        if unsafe { ffi::iconv_close(self.cd.0) } != 0 {
            warn!(
                "iconv_close returned error: {}",
                std::io::Error::last_os_error()
            );
        }
    }
}

/// A lossy Windows string to [`String`] decoder
pub fn decode_win_str(win_str: &[u8], codepage: u16) -> String {
    UTF8Decoder::for_windows_cp(codepage).decode_to_string(win_str)
}

/// A lossy UTF8-16-le string to [`String`] decoder (akin to
/// [`String::from_utf16le_lossy()`] in nightly)
pub fn decode_utf16le_str(utf16le_str: &[u8]) -> String {
    UTF8Decoder::for_label("UTF-16LE")
        .expect("Internal error: cannot decode UTF-16LE")
        .decode_to_string(utf16le_str)
}

/// UTF-8 decoder which implements [`Write`]
pub struct UTF8DecWriter<W: Write> {
    w: W,
    /// The decoder in use on this writer
    pub decoder: UTF8Decoder,
    buf: [u8; BUFSIZ],
}

impl<W: Write> UTF8DecWriter<W> {
    /// Creates a new UTF-8 writer
    pub fn new(decoder: UTF8Decoder, writer: W) -> Self {
        Self {
            w: writer,
            decoder,
            buf: [0u8; BUFSIZ],
        }
    }

    /// Wapper around [`UTF8Decoder::for_label`]
    pub fn for_label(label: &str, writer: W) -> Option<Self> {
        Some(Self {
            w: writer,
            decoder: UTF8Decoder::for_label(label)?,
            buf: [0u8; BUFSIZ],
        })
    }

    /// Wapper around [`UTF8Decoder::for_windows_cp`]
    pub fn for_windows_cp(cp: u16, writer: W) -> Self {
        Self::new(UTF8Decoder::for_windows_cp(cp), writer)
    }
}

impl<W: Write> Write for UTF8DecWriter<W> {
    fn write(&mut self, src: &[u8]) -> Result<usize, std::io::Error> {
        let mut cur = src;
        while !cur.is_empty() {
            let (inlen, outlen) = self.decoder.decode(cur, &mut self.buf);
            self.w.write_all(&self.buf[0..outlen])?;
            cur = &cur[inlen..];
        }
        Ok(src.len())
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        self.decoder
            .flush(&mut self.buf)
            .expect("Internal error: flush failed");
        Ok(())
    }
}

/// UTF-8 decoder which implements [`Read`]
pub struct UTF8DecReader<R: Read> {
    r: R,
    /// The decoder in use on this reader
    pub decoder: UTF8Decoder,
    buf: [u8; BUFSIZ],
    start: usize,
    end: usize,
    eof: bool,
}

impl<R: Read> UTF8DecReader<R> {
    /// Creates a new UTF-8 reader
    pub fn new(decoder: UTF8Decoder, reader: R) -> Self {
        Self {
            r: reader,
            decoder,
            buf: [0u8; BUFSIZ],
            start: 0,
            end: 0,
            eof: false,
        }
    }

    /// Wapper around [`UTF8Decoder::for_label`]
    pub fn for_label(label: &str, reader: R) -> Option<Self> {
        Some(Self {
            r: reader,
            decoder: UTF8Decoder::for_label(label)?,
            buf: [0u8; BUFSIZ],
            start: 0,
            end: 0,
            eof: false,
        })
    }

    /// Wapper around [`UTF8Decoder::for_windows_cp`]
    pub fn for_windows_cp(cp: u16, reader: R) -> Self {
        Self::new(UTF8Decoder::for_windows_cp(cp), reader)
    }
}

impl<R: Read> Read for UTF8DecReader<R> {
    fn read(&mut self, dst: &mut [u8]) -> Result<usize, std::io::Error> {
        let mut outlen = 0usize;
        if !self.eof {
            // Have input
            loop {
                // Read chunk if needed
                if self.start >= self.end {
                    self.start = 0;
                    self.end = self.r.read(&mut self.buf)?;
                    if self.end == 0 {
                        // EOF
                        self.eof = true;
                        self.end = self
                            .decoder
                            .flush(&mut self.buf)
                            .expect("Internal error: flush failed");
                        break;
                    }
                }
                // Decode chunk
                let (c, p) = self
                    .decoder
                    .decode(&self.buf[self.start..self.end], &mut dst[outlen..]);
                self.start += c;
                outlen += p;
                if outlen == dst.len() || p == 0 {
                    // Dst is full
                    return Ok(outlen);
                }
            }
        }
        let tailsize = (self.end - self.start).min(dst[outlen..].len());
        dst[outlen..(outlen + tailsize)]
            .copy_from_slice(&self.buf[self.start..(self.start + tailsize)]);
        self.start += self.start;
        outlen += tailsize;
        Ok(outlen)
    }
}

/// Charset alias mapping
// NOTE: this (k, v) slice MUST be sorted by k!
static ENCMAP: &[(&str, &str)] = &[
    ("866", "IBM866"),
    ("ANSI_X3.4-1968", "WINDOWS-1252"),
    ("ARABIC", "ISO-8859-6"),
    ("ASCII", "WINDOWS-1252"),
    ("ASMO-708", "ISO-8859-6"),
    ("BIG5-HKSCS", "BIG5"),
    ("CHINESE", "GBK"),
    ("CN-BIG5", "BIG5"),
    ("CP1250", "WINDOWS-1250"),
    ("CP1251", "WINDOWS-1251"),
    ("CP1252", "WINDOWS-1252"),
    ("CP1253", "WINDOWS-1253"),
    ("CP1254", "WINDOWS-1254"),
    ("CP1255", "WINDOWS-1255"),
    ("CP1256", "WINDOWS-1256"),
    ("CP1257", "WINDOWS-1257"),
    ("CP1258", "WINDOWS-1258"),
    ("CP819", "WINDOWS-1252"),
    ("CP866", "IBM866"),
    ("CSBIG5", "BIG5"),
    ("CSEUCKR", "EUC-KR"),
    ("CSEUCPKDFMTJAPANESE", "EUC-JP"),
    ("CSGB2312", "GBK"),
    ("CSIBM866", "IBM866"),
    ("CSISO2022JP", "ISO-2022-JP"),
    ("CSISO2022KR", "ISO-2022-KR"),
    ("CSISO58GB231280", "GBK"),
    ("CSISO88596E", "ISO-8859-6"),
    ("CSISO88596I", "ISO-8859-6"),
    ("CSISO88598E", "ISO-8859-8"),
    ("CSISO88598I", "ISO-8859-8"),
    ("CSISOLATIN1", "WINDOWS-1252"),
    ("CSISOLATIN2", "ISO-8859-2"),
    ("CSISOLATIN3", "ISO-8859-3"),
    ("CSISOLATIN4", "ISO-8859-4"),
    ("CSISOLATIN5", "WINDOWS-1254"),
    ("CSISOLATIN6", "ISO-8859-10"),
    ("CSISOLATIN9", "ISO-8859-15"),
    ("CSISOLATINARABIC", "ISO-8859-6"),
    ("CSISOLATINCYRILLIC", "ISO-8859-5"),
    ("CSISOLATINGREEK", "ISO-8859-7"),
    ("CSISOLATINHEBREW", "ISO-8859-8"),
    ("CSKOI8R", "KOI8-R"),
    ("CSKSC56011987", "EUC-KR"),
    ("CSMACINTOSH", "MACINTOSH"),
    ("CSSHIFTJIS", "SHIFT_JIS"),
    ("CSUNICODE", "UTF-16LE"),
    ("CSUNICODE11UTF7", "UTF-7"),
    ("CYRILLIC", "ISO-8859-5"),
    ("DOS-874", "WINDOWS-874"),
    ("ECMA-114", "ISO-8859-6"),
    ("ECMA-118", "ISO-8859-7"),
    ("ELOT_928", "ISO-8859-7"),
    ("GB2312", "GBK"),
    ("GB_2312", "GBK"),
    ("GB_2312-80", "GBK"),
    ("GREEK", "ISO-8859-7"),
    ("GREEK8", "ISO-8859-7"),
    ("HEBREW", "ISO-8859-8"),
    ("IBM819", "WINDOWS-1252"),
    ("ISO-10646-UCS-2", "UTF-16LE"),
    ("ISO-8859-1", "WINDOWS-1252"),
    ("ISO-8859-11", "WINDOWS-874"),
    ("ISO-8859-6-E", "ISO-8859-6"),
    ("ISO-8859-6-I", "ISO-8859-6"),
    ("ISO-8859-8-E", "ISO-8859-8"),
    ("ISO-8859-8-I", "ISO-8859-8"),
    ("ISO-8859-9", "WINDOWS-1254"),
    ("ISO-IR-100", "WINDOWS-1252"),
    ("ISO-IR-101", "ISO-8859-2"),
    ("ISO-IR-109", "ISO-8859-3"),
    ("ISO-IR-110", "ISO-8859-4"),
    ("ISO-IR-126", "ISO-8859-7"),
    ("ISO-IR-127", "ISO-8859-6"),
    ("ISO-IR-138", "ISO-8859-8"),
    ("ISO-IR-144", "ISO-8859-5"),
    ("ISO-IR-148", "WINDOWS-1254"),
    ("ISO-IR-149", "EUC-KR"),
    ("ISO-IR-157", "ISO-8859-10"),
    ("ISO-IR-58", "GBK"),
    ("ISO8859-1", "WINDOWS-1252"),
    ("ISO8859-10", "ISO-8859-10"),
    ("ISO8859-11", "WINDOWS-874"),
    ("ISO8859-13", "ISO-8859-13"),
    ("ISO8859-14", "ISO-8859-14"),
    ("ISO8859-15", "ISO-8859-15"),
    ("ISO8859-2", "ISO-8859-2"),
    ("ISO8859-3", "ISO-8859-3"),
    ("ISO8859-4", "ISO-8859-4"),
    ("ISO8859-5", "ISO-8859-5"),
    ("ISO8859-6", "ISO-8859-6"),
    ("ISO8859-7", "ISO-8859-7"),
    ("ISO8859-8", "ISO-8859-8"),
    ("ISO8859-9", "WINDOWS-1254"),
    ("ISO88591", "WINDOWS-1252"),
    ("ISO885910", "ISO-8859-10"),
    ("ISO885911", "WINDOWS-874"),
    ("ISO885913", "ISO-8859-13"),
    ("ISO885914", "ISO-8859-14"),
    ("ISO885915", "ISO-8859-15"),
    ("ISO88592", "ISO-8859-2"),
    ("ISO88593", "ISO-8859-3"),
    ("ISO88594", "ISO-8859-4"),
    ("ISO88595", "ISO-8859-5"),
    ("ISO88596", "ISO-8859-6"),
    ("ISO88597", "ISO-8859-7"),
    ("ISO88598", "ISO-8859-8"),
    ("ISO88599", "WINDOWS-1254"),
    ("ISO_8859-15", "ISO-8859-15"),
    ("ISO_8859-1:1987", "WINDOWS-1252"),
    ("ISO_8859-2", "ISO-8859-2"),
    ("ISO_8859-2:1987", "ISO-8859-2"),
    ("ISO_8859-3", "ISO-8859-3"),
    ("ISO_8859-3:1988", "ISO-8859-3"),
    ("ISO_8859-4", "ISO-8859-4"),
    ("ISO_8859-4:1988", "ISO-8859-4"),
    ("ISO_8859-5", "ISO-8859-5"),
    ("ISO_8859-5:1988", "ISO-8859-5"),
    ("ISO_8859-6", "ISO-8859-6"),
    ("ISO_8859-6:1987", "ISO-8859-6"),
    ("ISO_8859-7", "ISO-8859-7"),
    ("ISO_8859-7:1987", "ISO-8859-7"),
    ("ISO_8859-8", "ISO-8859-8"),
    ("ISO_8859-8:1988", "ISO-8859-8"),
    ("ISO_8859-9", "WINDOWS-1254"),
    ("ISO_8859-9:1989", "WINDOWS-1254"),
    ("KOI", "KOI8-R"),
    ("KOI8", "KOI8-R"),
    ("KOI8-RU", "KOI8-U"),
    ("KOI8_R", "KOI8-R"),
    ("KOREAN", "EUC-KR"),
    ("KSC5601", "EUC-KR"),
    ("KSC_5601", "EUC-KR"),
    ("KS_C_5601-1987", "EUC-KR"),
    ("KS_C_5601-1989", "EUC-KR"),
    ("L1", "WINDOWS-1252"),
    ("L2", "ISO-8859-2"),
    ("L3", "ISO-8859-3"),
    ("L4", "ISO-8859-4"),
    ("L5", "WINDOWS-1254"),
    ("L6", "ISO-8859-10"),
    ("L9", "ISO-8859-15"),
    ("LATIN1", "WINDOWS-1252"),
    ("LATIN2", "ISO-8859-2"),
    ("LATIN3", "ISO-8859-3"),
    ("LATIN4", "ISO-8859-4"),
    ("LATIN5", "WINDOWS-1254"),
    ("LATIN6", "ISO-8859-10"),
    ("LOGICAL", "ISO-8859-8"),
    ("MAC", "MACINTOSH"),
    ("MS932", "SHIFT_JIS"),
    ("MS_KANJI", "SHIFT_JIS"),
    ("SHIFT-JIS", "SHIFT_JIS"),
    ("SJIS", "SHIFT_JIS"),
    ("SUN_EU_GREEK", "ISO-8859-7"),
    ("TIS-620", "WINDOWS-874"),
    ("UCS-2", "UTF-16LE"),
    ("UNICODE", "UTF-16LE"),
    ("UNICODE-1-1-UTF-7", "UTF-7"),
    ("UNICODE-1-1-UTF-8", "UTF-8"),
    ("UNICODE11UTF8", "UTF-8"),
    ("UNICODE20UTF8", "UTF-8"),
    ("UNICODEFEFF", "UTF-16LE"),
    ("UNICODEFFFE", "UTF-16BE"),
    ("US ASCII", "WINDOWS-1252"),
    ("US-ASCII", "WINDOWS-1252"),
    ("USASCII", "WINDOWS-1252"),
    ("US_ASCII", "WINDOWS-1252"),
    ("UTF-16", "UTF-16LE"),
    ("UTF7", "UTF-7"),
    ("UTF8", "UTF-8"),
    ("VISUAL", "ISO-8859-8"),
    ("WINDOWS-31J", "SHIFT_JIS"),
    ("WINDOWS-949", "EUC-KR"),
    ("X-CP1250", "WINDOWS-1250"),
    ("X-CP1251", "WINDOWS-1251"),
    ("X-CP1252", "WINDOWS-1252"),
    ("X-CP1253", "WINDOWS-1253"),
    ("X-CP1254", "WINDOWS-1254"),
    ("X-CP1255", "WINDOWS-1255"),
    ("X-CP1256", "WINDOWS-1256"),
    ("X-CP1257", "WINDOWS-1257"),
    ("X-CP1258", "WINDOWS-1258"),
    ("X-EUC-JP", "EUC-JP"),
    ("X-GBK", "GBK"),
    ("X-MAC-CYRILLIC", "MAC-CYRILLIC"),
    ("X-MAC-ROMAN", "MACINTOSH"),
    ("X-MAC-UKRAINIAN", "MAC-CYRILLIC"),
    ("X-SJIS", "SHIFT_JIS"),
    ("X-UNICODE20UTF8", "UTF-8"),
    ("X-X-BIG5", "BIG5"),
];

static CPMAP: &[(u16, &str)] = &[
    (437, "CP437"),
    (708, "ASMO-708"),
    (737, "CP737"),
    (775, "CP775"),
    (850, "CP850"),
    (852, "CP852"),
    (855, "CP855"),
    (857, "CP857"),
    (858, "CP858"),
    (860, "CP860"),
    (861, "CP861"),
    (862, "CP862"),
    (863, "CP863"),
    (864, "CP864"),
    (865, "CP865"),
    (866, "CP866"),
    (869, "CP869"),
    (874, "CP874"),
    (932, "CP932"),
    (936, "CP936"),
    (949, "CP949"),
    (950, "CP950"),
    (1250, "CP1250"),
    (1251, "CP1251"),
    (1252, "CP1252"),
    (1253, "CP1253"),
    (1254, "CP1254"),
    (1255, "CP1255"),
    (1256, "CP1256"),
    (1257, "CP1257"),
    (1258, "CP1258"),
    (1361, "CP1361"),
    (10000, "MACINTOSH"),
    (65000, "UTF-7"),
    (65001, "UTF-8"),
];

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn encmap() {
        for (k, _) in ENCMAP.iter() {
            assert_eq!(*k, k.to_ascii_uppercase());
        }
        for (k, v) in ENCMAP.iter() {
            assert_ne!(k, v);
        }
        let unsorted = ENCMAP.iter().map(|(k, _)| *k).collect::<Vec<&str>>();
        let mut sorted = unsorted.clone();
        sorted.sort();
        assert_eq!(unsorted, sorted, "ENCMAP is not sorted");
        let with_dups = sorted.clone();
        sorted.dedup();
        assert_eq!(sorted, with_dups, "ENCMAP has duplicate keys");
        for (_, v) in ENCMAP.iter() {
            assert!(UTF8Decoder::for_label_noalias((*v).into()).is_some());
        }
    }

    #[test]
    fn for_label() {
        assert!(UTF8Decoder::for_label("asd").is_none());
        assert!(UTF8Decoder::for_label("us-ascii").is_some());
        let conv = UTF8Decoder::for_label("LaTiN5").unwrap();
        assert_eq!(conv.charset, "WINDOWS-1254");
    }

    #[test]
    fn for_windows_cp() {
        for (k, v) in CPMAP.iter() {
            assert_eq!(*v, v.to_ascii_uppercase());
            UTF8Decoder::for_windows_cp(*k);
        }
        assert!(UTF8Decoder::for_label("WINDOWS-1252").is_some());
    }

    #[test]
    fn streaming_conversion() {
        let mut buf = [0u8; 4];
        let mut iconv = UTF8Decoder::for_label("Windows-1252").unwrap();
        let (c, p) = iconv.decode(b"aCaB", &mut buf);
        assert_eq!(c, 4);
        assert_eq!(p, 4);
        assert_eq!(&buf, b"aCaB");
        assert!(!iconv.has_repl);
        assert_eq!(iconv.partial_len, 0);
        let (c, p) = iconv.decode(b"1337!!", &mut buf);
        assert_eq!(c, 4);
        assert_eq!(p, 4);
        assert_eq!(&buf, b"1337");
        assert!(!iconv.has_repl);
        assert_eq!(iconv.partial_len, 0);
        let (c, p) = iconv.decode(b"XY", &mut buf);
        assert_eq!(c, 2);
        assert_eq!(p, 2);
        assert_eq!(&buf[0..2], b"XY");
        assert!(!iconv.has_repl);
        assert_eq!(iconv.partial_len, 0);
        let (c, p) = iconv.decode(b"a\x81", &mut buf);
        assert_eq!(c, 2);
        assert_eq!(p, 4);
        assert_eq!(iconv.partial_len, 0);
        assert_eq!(&buf, b"a\xef\xbf\xbd");
        assert!(iconv.has_repl);
        iconv.reset();
        assert_eq!(iconv.partial_len, 0);
        assert!(!iconv.has_repl);
    }

    #[test]
    fn partial_conversion() {
        let mut buf = [0u8; 4];
        let mut iconv = UTF8Decoder::for_label("utf-16le").unwrap();
        let (c, p) = iconv.decode(b"a", &mut buf);
        assert_eq!(c, 1);
        assert_eq!(p, 0);
        assert!(!iconv.has_repl);
        assert_eq!(iconv.partial_len, 1);
        let (c, p) = iconv.decode(b"\0C\0a\0b\0", &mut buf);
        assert_eq!(c, 7);
        assert_eq!(p, 4);
        assert!(!iconv.has_repl);
        assert_eq!(iconv.partial_len, 0);
        let (c, p) = iconv.decode(b"a", &mut buf);
        assert_eq!(c, 1);
        assert_eq!(p, 0);
        assert!(!iconv.has_repl);
        assert_eq!(iconv.partial_len, 1);
        let mut smallbuf = [0u8; 2];
        let p = iconv.flush(&mut smallbuf);
        assert!(p.is_none());
        let p = iconv.flush(&mut buf).unwrap();
        assert_eq!(p, 3);
        assert!(iconv.has_repl);
        assert_eq!(iconv.partial_len, 0);
        assert_eq!(&buf[0..3], b"\xef\xbf\xbd");
        iconv.reset();
        let (c, p) = iconv.decode(b"a", &mut buf);
        assert_eq!(c, 1);
        assert_eq!(p, 0);
        assert!(!iconv.has_repl);
        assert_eq!(iconv.partial_len, 1);
        iconv.reset();
        let (c, p) = iconv.decode(b"a\0C\0a\0B\0", &mut buf);
        assert_eq!(c, 8);
        assert_eq!(p, 4);
        assert!(!iconv.has_repl);
        assert_eq!(iconv.partial_len, 0);
        assert_eq!(&buf, b"aCaB");
    }

    #[test]
    fn partial_with_repl() {
        let mut buf = [0u8; 4];
        let mut iconv = UTF8Decoder::for_label("utf-8").unwrap();
        // Incomplete sequence
        // Output: nothing
        let (c, p) = iconv.decode(b"\xe2\x82", &mut buf);
        assert_eq!(c, 2);
        assert_eq!(p, 0);
        assert!(!iconv.has_repl);
        assert_eq!(iconv.partial_len, 2);
        // In now becomes a bad sequence - the inital e2 is consumed
        // Output: REPLACEMENT
        let (c, p) = iconv.decode(b"A", &mut buf);
        assert_eq!(c, 1);
        assert_eq!(p, 3);
        assert!(iconv.has_repl);
        assert_eq!(iconv.partial_len, 2);
        assert_eq!(&buf[0..3], b"\xef\xbf\xbd");
        // Still a bad sequence - the inital e2 is consumed
        // Output: nothing (REPLACEMENT is squashed)
        let (c, p) = iconv.decode(b"\xac", &mut buf);
        assert_eq!(c, 1);
        assert_eq!(p, 0);
        assert_eq!(iconv.partial_len, 2);
        // This eats the 82 leading to the A
        // Output: A
        let (c, p) = iconv.decode(b"\xe2", &mut buf);
        assert_eq!(c, 0); // Nothing consumed since input is from partial
        assert_eq!(p, 1);
        assert_eq!(iconv.partial_len, 1);
        assert_eq!(&buf[0..1], b"A");
        // This eats the 82 leading to the A
        // Output: REPLACEMENT
        let (c, p) = iconv.decode(b"\xe2", &mut buf);
        assert_eq!(c, 1);
        assert_eq!(p, 3);
        assert_eq!(iconv.partial_len, 1);
        assert_eq!(&buf[0..3], b"\xef\xbf\xbd");
        // Incomplete sequence
        // Output: Nothing
        let (c, p) = iconv.decode(b"\x82", &mut buf);
        assert_eq!(c, 1);
        assert_eq!(p, 0);
        assert_eq!(iconv.partial_len, 2);
        // Sequence now complete
        // Output: euro sign
        let (c, p) = iconv.decode(b"\xac", &mut buf);
        assert_eq!(c, 1);
        assert_eq!(p, 3);
        assert_eq!(iconv.partial_len, 0);
        assert_eq!(&buf[0..3], b"\xe2\x82\xac");
    }

    #[test]
    fn to_string() {
        let reference = "بسم الله الرحمن الرحيم";
        let mut iconv = UTF8Decoder::for_label("iso-8859-6").unwrap();
        let s = iconv.decode_to_string(
            b"\xc8\xd3\xe5\x20\xc7\xe4\xe4\xe7\x20\xc7\xe4\xd1\xcd\xe5\xe6\x20\xc7\xe4\xd1\xcd\xea\xe5",
        );
        assert_eq!(s, reference);
        let mut iconv = UTF8Decoder::for_label("Windows-1256").unwrap();
        let s = iconv.decode_to_string(
            b"\xc8\xd3\xe3\x20\xc7\xe1\xe1\xe5\x20\xc7\xe1\xd1\xcd\xe3\xe4\x20\xc7\xe1\xd1\xcd\xed\xe3",
        );
        assert_eq!(s, reference);
    }

    #[test]
    fn decwriter() {
        let mut w: Vec<u8> = Vec::new();
        let mut dec = UTF8DecWriter::for_label("LATIN1", &mut w).unwrap();
        dec.write_all(b"\xe0\xe8\xec\xf2\xf9")
            .expect("Write failed");
        assert_eq!(String::from_utf8(w).expect("Failed to decode"), "àèìòù");
    }

    #[test]
    fn decreader() {
        let r = b"\xe0\xe8\xec\xf2\xf9";
        let mut dec = UTF8DecReader::for_label("LATIN1", r.as_slice()).unwrap();
        let mut s = String::new();
        dec.read_to_string(&mut s).expect("Failed to decode");
        assert_eq!(s, "àèìòù");
    }
}
