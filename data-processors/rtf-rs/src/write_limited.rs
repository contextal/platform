use std::io::Write;

/// A `Write` wrapper which limits the amount of data *actually* written
///
/// Writes in excess are reported as successful, but the data is discarded
pub struct LimitedWriter<W: Write> {
    w: W,
    limit: u64,
    current: u64,
}

impl<W: Write> LimitedWriter<W> {
    /// Creates a new writer
    pub fn new(w: W, limit: u64) -> Self {
        Self {
            w,
            limit,
            current: 0,
        }
    }

    /// Reports the amount of data written
    ///
    /// This is only meaningful if the limit was not reached
    pub fn written_size(&self) -> u64 {
        self.current
    }

    /// Reports whether the limit was hit
    pub fn limit_reached(&self) -> bool {
        self.current >= self.limit
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
        let avail = self
            .limit
            .saturating_sub(self.current)
            .saturating_sub(buf.len().try_into().unwrap_or(u64::MAX));
        if avail == 0 {
            self.current = self.limit;
            return Ok(buf.len());
        }
        let ret = self.w.write(buf);
        if let Ok(written) = ret {
            self.current += u64::try_from(written).unwrap();
        }
        ret
    }
    fn flush(&mut self) -> Result<(), std::io::Error> {
        self.w.flush()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn limits() -> Result<(), std::io::Error> {
        let inbuf = [0xffu8; 10];

        let mut r: &[u8] = &inbuf;
        let mut outbuf = [0u8; 10];
        let mut w = LimitedWriter::new(outbuf.as_mut_slice(), 100);
        assert_eq!(std::io::copy(&mut r, &mut w)?, 10);
        assert!(!w.limit_reached());
        assert_eq!(w.written_size(), 10);
        assert_eq!(&inbuf, &outbuf);

        let mut r: &[u8] = &inbuf;
        let mut outbuf = [0u8; 5];
        let mut w = LimitedWriter::new(outbuf.as_mut_slice(), 100);
        assert!(std::io::copy(&mut r, &mut w).is_err());

        let mut r: &[u8] = &inbuf;
        let mut outbuf = [0u8; 20];
        let mut w = LimitedWriter::new(outbuf.as_mut_slice(), 100);
        assert_eq!(std::io::copy(&mut r, &mut w)?, 10);
        assert!(!w.limit_reached());
        assert_eq!(w.written_size(), 10);
        assert_eq!(&inbuf, &outbuf[0..10]);

        let mut r: &[u8] = &inbuf;
        let mut outbuf = [0u8; 10];
        let mut w = LimitedWriter::new(outbuf.as_mut_slice(), 10);
        assert_eq!(std::io::copy(&mut r, &mut w)?, 10);
        assert!(w.limit_reached());
        assert_eq!(&outbuf, &[0u8; 10]);

        let mut r: &[u8] = &inbuf;
        let mut outbuf = [0u8; 10];
        let mut w = LimitedWriter::new(outbuf.as_mut_slice(), 5);
        assert_eq!(std::io::copy(&mut r, &mut w)?, 10);
        assert!(w.limit_reached());

        Ok(())
    }
}
