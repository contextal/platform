//! Binary line reader
use lazy_static::lazy_static;
use std::io::Read;

/// The maximum line length (set to 1000 per RFC 5322)
const MAX_LINE_LEN: usize = 1000;
/// The size of the internal buffer
const BUFSIZ: usize = 4096;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse4.2")]
#[inline]
/// Returns the position of the first CR or LF - SSE4.2 version (unsafe)
unsafe fn get_line_len_fast(line: &[u8]) -> Option<usize> {
    let line = &line[0..line.len().min(MAX_LINE_LEN)];
    #[cfg(target_arch = "x86")]
    use std::arch::x86::{
        __m128i, _mm_cmpestri, _mm_lddqu_si128, _mm_load_si128, _mm_set1_epi16, _mm_srli_si128,
    };
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::{
        __m128i, _mm_cmpestri, _mm_lddqu_si128, _mm_load_si128, _mm_set1_epi16, _mm_srli_si128,
    };

    if line.is_empty() {
        return None;
    }
    let needle = unsafe { _mm_set1_epi16(0x0d0a) };
    if line.len() == 16 {
        // Special case, unaligned but faster
        let haystack = unsafe { _mm_lddqu_si128(line.as_ptr() as _) };
        let idx = unsafe { _mm_cmpestri(needle, 2, haystack, 16, 0) };
        if idx < 16 {
            return Some(idx as usize);
        } else {
            return None;
        }
    }

    let (head, aligned, mut tail): (&[u8], &[__m128i], &[u8]) =
        unsafe { line.align_to::<__m128i>() };
    if !head.is_empty() {
        let len = head.len();
        // Line is misaligned
        // Align to the previous 16-byte bound, so it points to the garbage before head
        let hay_ptr = head.as_ptr() as usize & !15;
        // Load the data (aligned read)
        let mut haystack = unsafe { _mm_load_si128(hay_ptr as _) };
        // The haystack contain leading garbage which we don't want to match on, so it's rotated as needed
        // Note this is *a lot* faster than an unaligned read
        // The length of the leading garbage
        let garbage_len = head.as_ptr() as usize & 15;
        if garbage_len & 8 != 0 {
            haystack = unsafe { _mm_srli_si128(haystack, 8) };
        }
        if garbage_len & 4 != 0 {
            haystack = unsafe { _mm_srli_si128(haystack, 4) };
        }
        if garbage_len & 2 != 0 {
            haystack = unsafe { _mm_srli_si128(haystack, 2) };
        }
        if garbage_len & 1 != 0 {
            haystack = unsafe { _mm_srli_si128(haystack, 1) };
        }
        // The haystack now contains our head at the beginning, the garbage is out, zeroes are in
        let idx = unsafe { _mm_cmpestri(needle, 2, haystack, len as i32, 0) }; // FIXME compare less?
        if idx < len as i32 {
            return Some(idx as usize);
        }
    }

    // Nothing found in head, process the aligned blocks
    let mut pos = head.len();
    for chunk in aligned {
        // Load the data (aligned read)
        let haystack = unsafe { _mm_load_si128(chunk as _) };
        let idx = unsafe { _mm_cmpestri(needle, 2, haystack, 16, 0) };
        if idx < 16 {
            return Some(pos + idx as usize);
        }
        pos += 16;
    }

    // Nothing found again, parse tail
    // For inexplicable reasons this might might be > 16 bytes - see notes on align_to
    while !tail.is_empty() {
        let len = tail.len().min(16);
        let haystack = unsafe { _mm_load_si128(tail.as_ptr() as _) };
        let idx = unsafe { _mm_cmpestri(needle, 2, haystack, len as i32, 0) };
        if idx < len as i32 {
            return Some(pos + idx as usize);
        }
        pos += len;
        tail = &tail[len..];
    }

    None
}

#[inline]
/// Returns the position of the first CR or LF - regular version (safe)
fn get_line_len_slow(line: &[u8]) -> Option<usize> {
    line.iter()
        .take(MAX_LINE_LEN)
        .position(|&v| v == b'\n' || v == b'\r')
}

/// A buffered "mail line" reader, wraps any `Read`
///
/// Line breaks on CR, LF or CRLF
pub struct LineReader<R: Read> {
    r: R,
    buf: [u8; BUFSIZ],
    start: usize,
    end: usize,
    eof: bool,
}

impl<R: Read> LineReader<R> {
    /// Creates the line reader
    pub fn new(r: R) -> Self {
        Self {
            r,
            buf: [0; BUFSIZ],
            start: 0,
            end: 0,
            eof: false,
        }
    }

    fn fill_buf(&mut self) -> Result<(), std::io::Error> {
        if self.start > 0 {
            // memmove
            self.buf.copy_within(self.start..self.end, 0);
            self.end -= self.start;
            self.start = 0;
        }
        loop {
            let read = match self.r.read(&mut self.buf[self.end..]) {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            if read == 0 {
                self.eof = true;
            }
            self.end += read;
            break;
        }
        Ok(())
    }

    fn get_line_len(&self) -> Option<usize> {
        let line = &self.buf[self.start..self.end];
        lazy_static! {
            /// A wrapper which invokes either [`get_line_len_fast`] or
            ///  [`get_line_len_slow`] based on the CPU features availability
            static ref GLL: fn(&[u8]) -> Option<usize> = {
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                {
                    if is_x86_feature_detected!("sse4.2") {
                        return |l: &[u8]| unsafe { get_line_len_fast(l) };
                    }
                }
                get_line_len_slow
            };
        }
        if let Some(pos) = GLL(line) {
            if self.buf[self.start + pos] == b'\n' {
                // LF only
                return Some(pos + 1);
            }
            if self.start + pos + 1 >= self.end {
                // One more byte needed
            } else if self.buf[self.start + pos + 1] == b'\n' {
                // CRLF
                return Some(pos + 2);
            } else {
                // CR only
                return Some(pos + 1);
            }
        }
        None
    }

    /// Reads and returns a full mail line (with EOL)
    ///
    /// An empty slice is returned on EOF
    pub fn read_line(&mut self) -> Result<&[u8], std::io::Error> {
        loop {
            if let Some(pos) = self.get_line_len() {
                let start = self.start;
                self.start += pos;
                return Ok(&self.buf[start..(start + pos)]);
            } else if self.end - self.start >= MAX_LINE_LEN {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Line too long",
                ));
            }
            self.fill_buf()?;
            if self.eof {
                let start = self.start;
                self.start = self.end;
                return Ok(&self.buf[start..self.end]);
            }
        }
    }

    /// Reads and returns a full mail line (with EOL removed)
    ///
    /// Note: returns an empty slice on EOF or empty line, so it is not suitable
    /// to determine if EOF is reached
    pub fn read_line_rtrim(&mut self) -> Result<&[u8], std::io::Error> {
        let line = self.read_line()?;
        if line.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Truncated or otherwise invalid mail",
            ));
        }
        Ok(super::without_eol(line))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_readline() -> Result<(), std::io::Error> {
        let data: &[u8] = b"crlf\r\ncr\rlf\n4\n\n6";
        let mut r = LineReader::new(data);
        assert_eq!(r.read_line()?, b"crlf\r\n");
        assert_eq!(r.read_line()?, b"cr\r");
        assert_eq!(r.read_line()?, b"lf\n");
        assert_eq!(r.read_line()?, b"4\n");
        assert_eq!(r.read_line()?, b"\n");
        assert_eq!(r.read_line()?, b"6");
        assert_eq!(r.read_line()?, b"");
        assert_eq!(r.read_line()?, b"");
        Ok(())
    }

    struct LameReader<R: Read>(R);
    impl<R: Read> Read for LameReader<R> {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
            let len = buf.len().min(1);
            self.0.read(&mut buf[0..len])
        }
    }

    #[test]
    fn test_readline_lame() -> Result<(), std::io::Error> {
        let data: &[u8] = b"crlf\r\ncr\rlf\n4\n\n6";
        let mut r = LineReader::new(LameReader(data));
        assert_eq!(r.read_line()?, b"crlf\r\n");
        assert_eq!(r.read_line()?, b"cr\r");
        assert_eq!(r.read_line()?, b"lf\n");
        assert_eq!(r.read_line()?, b"4\n");
        assert_eq!(r.read_line()?, b"\n");
        assert_eq!(r.read_line()?, b"6");
        assert_eq!(r.read_line()?, b"");
        assert_eq!(r.read_line()?, b"");
        Ok(())
    }

    #[test]
    fn test_trailing_newline() -> Result<(), std::io::Error> {
        let data: &[u8] = b"line1\nline2\n";
        let mut r = LineReader::new(data);
        assert_eq!(r.read_line()?, b"line1\n");
        assert_eq!(r.read_line()?, b"line2\n");
        assert_eq!(r.read_line()?, b"");
        Ok(())
    }

    #[test]
    fn test_no_trailing_newline() -> Result<(), std::io::Error> {
        let data: &[u8] = b"line1\nline2";
        let mut r = LineReader::new(data);
        assert_eq!(r.read_line()?, b"line1\n");
        assert_eq!(r.read_line()?, b"line2");
        assert_eq!(r.read_line()?, b"");
        Ok(())
    }

    #[test]
    fn test_limits() -> Result<(), std::io::Error> {
        let data = [b'a'; MAX_LINE_LEN - 1].as_ref();
        let mut r = LineReader::new(data);
        assert!(r.read_line().is_ok());

        let data = [b'a'; MAX_LINE_LEN].as_ref();
        let mut r = LineReader::new(data);
        assert_eq!(
            r.read_line().unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );

        let data = [b'a'; MAX_LINE_LEN - 1].as_ref().chain(b"\ntail".as_ref());
        let mut r = LineReader::new(data);
        assert!(r.read_line().is_ok());
        assert_eq!(r.read_line()?, b"tail");
        Ok(())
    }

    #[test]
    fn test_refill() -> Result<(), std::io::Error> {
        fn test_size(len: usize) {
            let mut data: Vec<u8> = Vec::with_capacity(len + 4);
            while data.len() < len {
                let needed = (len - data.len() - 1).min(MAX_LINE_LEN - 1);
                for _ in 0..needed {
                    data.push(b'a');
                }
                data.push(b'\n');
            }
            data.extend_from_slice(b"last");
            let mut r = LineReader::new(data.as_slice());
            let mut is_last = false;
            loop {
                let line = r.read_line().unwrap();
                if line.is_empty() {
                    break;
                }
                is_last = line == b"last";
            }
            assert!(is_last, "refill with len {} failed", len);
        }

        for len in (BUFSIZ - 4)..(BUFSIZ + 4) {
            test_size(len);
        }
        Ok(())
    }

    #[test]
    fn test_get_line_len() {
        let gll = |l| {
            let pos = get_line_len_slow(l);
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            {
                if is_x86_feature_detected!("sse4.2") {
                    let fpos = unsafe { get_line_len_fast(l) };
                    assert_eq!(
                        fpos, pos,
                        "GLL mismatch fast {:?}, slow {:?} on line {:x?}",
                        fpos, pos, l
                    );
                }
            }
            pos
        };
        assert_eq!(gll(b""), None);
        assert_eq!(gll(b"\n"), Some(0));
        assert_eq!(gll(b"\r"), Some(0));
        assert_eq!(gll(b"\r\n"), Some(0));
        assert_eq!(gll(b"a\n"), Some(1));
        assert_eq!(gll(b"a\r"), Some(1));
        assert_eq!(gll(b"a\r\n"), Some(1));

        assert_eq!(gll(b"x"), None);
        assert_eq!(gll(b"\nx"), Some(0));
        assert_eq!(gll(b"\rx"), Some(0));
        assert_eq!(gll(b"\r\nx"), Some(0));
        assert_eq!(gll(b"a\nx"), Some(1));
        assert_eq!(gll(b"a\rx"), Some(1));
        assert_eq!(gll(b"a\r\nx"), Some(1));

        assert_eq!(gll(b"0123456789abcdef"), None);
        assert_eq!(gll(b"012345678\nabcdef"), Some(9));
        assert_eq!(gll(b"0123\n56789abcdef"), Some(4));
        assert_eq!(gll(b"0123456789\r\ncdef"), Some(10));
        assert_eq!(gll(b"0123456789abcdefABC\r\n"), Some(19));

        let buf = b"012345";
        for i in 0..buf.len() {
            assert_eq!(gll(&buf[i..]), None);
        }
        let buf = b"012345\n";
        for i in 0..buf.len() {
            assert_eq!(gll(&buf[i..]), Some(buf.len() - i - 1));
        }
        let buf = b"0123456789abcdefABCDEFGHIJKLMNOPQRTSUVWXYZ";
        for i in 0..buf.len() {
            assert_eq!(gll(&buf[i..]), None);
        }
        let buf = b"0123456789abcdefABCDEFGHIJKLMNOPQRTSUVWXYZ\n";
        for i in 0..buf.len() {
            assert_eq!(gll(&buf[i..]), Some(buf.len() - i - 1));
        }

        let buf: Vec<u8> = std::iter::repeat(0).take(MAX_LINE_LEN).collect();
        assert_eq!(gll(&buf), None);
        let mut buf: Vec<u8> = std::iter::repeat(0).take(MAX_LINE_LEN).collect();
        buf.push(b'\n');
        assert_eq!(gll(&buf), None);
    }
}
