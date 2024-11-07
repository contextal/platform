//! I/O utilities
use crate::cmp::Unsigned;
use std::io::{self, Read, Seek, Write};

#[derive(Debug)]
/// Error indicatring excess writing to a [`LimitedWriter`]
pub struct WriteLimitExceededError;
impl std::fmt::Display for WriteLimitExceededError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "Write limit exceeded")
    }
}
impl std::error::Error for WriteLimitExceededError {}

/// A `Write` wrapper which limits the amount of data written
///
/// Writes in excess produce an [`std::io::Error`] of type [`std::io::ErrorKind::Other`]
/// with the error set to [`WriteLimitExceededError`]
pub struct LimitedWriter<W: Write> {
    w: W,
    available: u64,
    written: u64,
}

impl<W: Write> LimitedWriter<W> {
    /// Creates a new writer
    pub fn new(w: W, limit: u64) -> Self {
        Self {
            w,
            available: limit,
            written: 0,
        }
    }

    /// Returns number of written bytes
    pub fn written(&self) -> u64 {
        self.written
    }

    /// Unwraps this LimitedWriter, returning the underlying writer.
    pub fn into_inner(self) -> W {
        self.w
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
        if self.available == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                WriteLimitExceededError,
            ));
        }
        let write_len = buf
            .len()
            .min(self.available.try_into().unwrap_or(buf.len()));
        let ret = self.w.write(&buf[0..write_len]);
        if let Ok(written) = ret {
            let written = u64::try_from(written).unwrap();
            self.available -= written; // safe bc min()
            self.written += written;
        }
        ret
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        self.w.flush()
    }
}

/// Single byte `u8` reader
#[inline]
pub fn rdu8<R: Read>(r: &mut R) -> Result<u8, std::io::Error> {
    let mut buf = [0u8; 1];
    r.read_exact(&mut buf)?;
    Ok(buf[0])
}

#[inline]
/// Single byte `i8` reader
pub fn rdi8<R: Read>(f: &mut R) -> Result<i8, std::io::Error> {
    let mut buf = [0u8; 1];
    f.read_exact(&mut buf)?;
    Ok(buf[0] as i8)
}

/// Little endian `u16` reader
#[inline]
pub fn rdu16le<R: Read>(r: &mut R) -> Result<u16, std::io::Error> {
    let mut buf = [0u8; 2];
    r.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

#[inline]
/// Little endian `i16` reader
pub fn rdi16le<R: Read>(f: &mut R) -> Result<i16, std::io::Error> {
    let mut buf = [0u8; 2];
    f.read_exact(&mut buf)?;
    Ok(i16::from_le_bytes(buf))
}

/// Big endian `u16` reader
#[inline]
pub fn rdu16be<R: Read>(r: &mut R) -> Result<u16, std::io::Error> {
    let mut buf = [0u8; 2];
    r.read_exact(&mut buf)?;
    Ok(u16::from_be_bytes(buf))
}

/// Little endian `u32` reader
#[inline]
pub fn rdu32le<R: Read>(r: &mut R) -> Result<u32, std::io::Error> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

#[inline]
/// Little endian `i32` reader
pub fn rdi32le<R: Read>(f: &mut R) -> Result<i32, std::io::Error> {
    let mut buf = [0u8; 4];
    f.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

/// Big endian `u32` reader
#[inline]
pub fn rdu32be<R: Read>(r: &mut R) -> Result<u32, std::io::Error> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
}

/// Little endian `u64` reader
#[inline]
pub fn rdu64le<R: Read>(r: &mut R) -> Result<u64, std::io::Error> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

#[inline]
/// Little endian `i64` reader
pub fn rdi64le<R: Read>(f: &mut R) -> Result<i64, std::io::Error> {
    let mut buf = [0u8; 8];
    f.read_exact(&mut buf)?;
    Ok(i64::from_le_bytes(buf))
}

/// Big endian `u64` reader
#[inline]
pub fn rdu64be<R: Read>(r: &mut R) -> Result<u64, std::io::Error> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(u64::from_be_bytes(buf))
}

#[inline]
/// Little endian `f32` reader
pub fn rdf32le<R: Read>(f: &mut R) -> Result<f32, std::io::Error> {
    let mut buf = [0u8; 4];
    f.read_exact(&mut buf)?;
    Ok(f32::from_le_bytes(buf))
}

#[inline]
/// Little endian `f64` reader
pub fn rdf64le<R: Read>(f: &mut R) -> Result<f64, std::io::Error> {
    let mut buf = [0u8; 8];
    f.read_exact(&mut buf)?;
    Ok(f64::from_le_bytes(buf))
}

/// Returns the sum of the values the slice or None in case of overflow
#[inline]
pub fn checked_sum(values: &[u32]) -> Option<u32> {
    let mut ret = 0u32;
    for v in values {
        ret = ret.checked_add(*v)?;
    }
    Some(ret)
}

/// A [`Take`](io::Take) that can [`Seek`]
///
/// Adapter which limits the bytes read from an underlying reader, seekable
///
/// Standard Seek semantics are honored
///
/// Changing the stream position of the underlying object outside of this scope
/// yields undefined results
pub struct SeekTake<R: Read + Seek> {
    r: R,
    offset: u64,
    limit: u64,
}

impl<R: Read + Seek> SeekTake<R> {
    /// Creates a new SeekTake
    pub fn new(r: R, limit: u64) -> Self {
        Self {
            r,
            offset: 0,
            limit,
        }
    }

    /// Consumes the struct, returning the underlying value
    pub fn into_inner(self) -> R {
        self.r
    }

    /// Returns the number of bytes that can be read before this instance will return EOF
    pub fn limit(&self) -> u64 {
        self.limit.saturating_sub(self.offset)
    }
}

impl<R: Read + Seek> Read for SeekTake<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let maxlen = buf.len().umin(self.limit());
        let res = R::read(&mut self.r, &mut buf[0..maxlen]);
        if let Ok(done) = res {
            // unwrap is safe bc umin
            // sum is safe because the underlaying R is also u64 sized
            self.offset += u64::try_from(done).unwrap();
        }
        res
    }
}

impl<R: Read + Seek> Seek for SeekTake<R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        fn apply_diff(offset: u64, diff: i64) -> io::Result<u64> {
            if diff >= 0 {
                offset
                    .checked_add(diff as u64)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Seek overflow"))
            } else {
                offset
                    .checked_sub((-diff) as u64)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Seek underflow"))
            }
        }

        let (diff, newoff) = match pos {
            io::SeekFrom::Start(abspos) => (abspos.wrapping_sub(self.offset) as i64, abspos),
            io::SeekFrom::Current(diff) => (diff, apply_diff(self.offset, diff)?),
            io::SeekFrom::End(diff) => {
                let newoff = apply_diff(self.limit, diff)?;
                (newoff.wrapping_sub(self.offset) as i64, newoff)
            }
        };
        self.r.seek(io::SeekFrom::Current(diff))?;
        self.offset = newoff;
        Ok(self.offset)
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        Ok(self.offset)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn limits() -> Result<(), std::io::Error> {
        let inbuf = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

        let mut r: &[u8] = &inbuf;
        let mut outbuf = [0u8; 20];
        let mut w = LimitedWriter::new(outbuf.as_mut_slice(), 100);
        assert_eq!(std::io::copy(&mut r, &mut w)?, 10);
        assert_eq!(&inbuf, &outbuf[0..10]);

        let mut r: &[u8] = &inbuf;
        let mut outbuf = [0u8; 10];
        let mut w = LimitedWriter::new(outbuf.as_mut_slice(), 100);
        assert_eq!(std::io::copy(&mut r, &mut w)?, 10);
        assert_eq!(&inbuf, &outbuf);

        let mut r: &[u8] = &inbuf;
        let mut outbuf = [0u8; 5];
        let mut w = LimitedWriter::new(outbuf.as_mut_slice(), 100);
        let e = std::io::copy(&mut r, &mut w).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::WriteZero);

        let mut r: &[u8] = &inbuf;
        let mut outbuf = [0u8; 10];
        let mut w = LimitedWriter::new(outbuf.as_mut_slice(), 10);
        assert_eq!(std::io::copy(&mut r, &mut w)?, 10);
        assert_eq!(&inbuf, &outbuf[0..10]);

        let mut r: &[u8] = &inbuf;
        let mut outbuf = [0u8; 10];
        let mut w = LimitedWriter::new(outbuf.as_mut_slice(), 5);
        let e = std::io::copy(&mut r, &mut w).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::Other);
        let _limit_err = WriteLimitExceededError;
        assert!(matches!(e.into_inner().unwrap(), _limit_err));

        let mut outbuf = [0u8; 10];
        let mut w = LimitedWriter::new(outbuf.as_mut_slice(), 10);
        assert_eq!(w.write(&inbuf[0..6])?, 6);
        assert_eq!(w.write(&inbuf[6..])?, 4);
        assert_eq!(&inbuf, &outbuf);

        Ok(())
    }

    #[test]
    fn intread() -> Result<(), std::io::Error> {
        let buf = &mut b"\
        \x00\
        \xd6\
        \x01\x02\
        \xc7\xcf\
        \x03\x04\
        \x05\x06\x07\x08\
        \x4f\x97\x21\xc5\
        \xa0\xb1\xc2\xd3\
        \x2b\x52\x9a\x44\
        \xef\xbe\xfe\xca\xce\xfa\xed\xfe\
        \xeb\x7e\x16\x82\x0b\xef\xdd\xee\
        \x13\x37\xc0\xde\xba\xdb\xab\x1e\
        \xe0\x0f\xfd\x84\x45\x4a\x93\xc0\
        "
        .as_slice();
        assert_eq!(rdu8(buf)?, 0);
        assert_eq!(rdi8(buf)?, -42);
        assert_eq!(rdu16le(buf)?, 0x0201);
        assert_eq!(rdi16le(buf)?, -12345);
        assert_eq!(rdu16be(buf)?, 0x0304);
        assert_eq!(rdu32le(buf)?, 0x08070605);
        assert_eq!(rdi32le(buf)?, -987654321);
        assert_eq!(rdu32be(buf)?, 0xa0b1c2d3);
        assert_eq!(rdf32le(buf)?, 1234.5678);
        assert_eq!(rdu64le(buf)?, 0xfeedfacecafebeef);
        assert_eq!(rdi64le(buf)?, -1234567890123456789);
        assert_eq!(rdu64be(buf)?, 0x1337c0debadbab1e);
        assert_eq!(rdf64le(buf)?, -1234.56789012345678);
        assert_eq!(buf.len(), 0);
        Ok(())
    }

    fn readbyte<R: Read>(r: &mut R) -> io::Result<Option<u8>> {
        let mut buf = [0u8];
        match r.read_exact(&mut buf) {
            Ok(_) => Ok(Some(buf[0])),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e),
        }
    }

    #[test]
    fn seek_take() -> io::Result<()> {
        let buf = &[0, 1, 2, 3, 4];
        let mut cur = io::Cursor::new(&buf);
        assert_eq!(readbyte(&mut cur)?, Some(0));
        let mut rs = SeekTake::new(&mut cur, 3);
        assert_eq!(readbyte(&mut rs)?, Some(1));
        assert_eq!(readbyte(&mut rs)?, Some(2));
        assert_eq!(readbyte(&mut rs)?, Some(3));
        assert_eq!(readbyte(&mut rs)?, None);
        assert_eq!(rs.seek(io::SeekFrom::Start(1))?, 1);
        assert_eq!(readbyte(&mut rs)?, Some(2));
        assert!(rs.seek(io::SeekFrom::Current(-3)).is_err());
        assert_eq!(rs.seek(io::SeekFrom::Current(-2))?, 0);
        assert_eq!(readbyte(&mut rs)?, Some(1));
        assert!(rs.seek(io::SeekFrom::Current(-4)).is_err());
        assert_eq!(rs.seek(io::SeekFrom::End(-3))?, 0);
        assert_eq!(readbyte(&mut rs)?, Some(1));
        assert_eq!(rs.seek(io::SeekFrom::Current(1))?, 2);
        assert_eq!(readbyte(&mut rs)?, Some(3));
        assert_eq!(rs.seek(io::SeekFrom::End(3))?, 6);
        assert_eq!(readbyte(&mut rs)?, None);
        assert_eq!(rs.seek(io::SeekFrom::Current(-4))?, 2);
        assert_eq!(readbyte(&mut rs)?, Some(3));
        Ok(())
    }
}
