//! *CompressedContainer* decompressor
//!
//! Intended for internal crate use but exported for convenience and low level operations
//!
//! See [`CompressContainerReader`]
use ctxutils::{cmp::*, io::*};
use std::io::{self, Read};

/// A decompressor for *CompressedContainer* streams
///
/// Create the decompressor via [`new()`](CompressContainerReader::new);
/// use the [`Read`] trait to get the decompressed output
pub struct CompressContainerReader<R: Read> {
    f: R,
    avail_in: u64,
    start: usize,
    end: usize,
    buf: [u8; 4096],
}

impl<R: Read> CompressContainerReader<R> {
    /// Creates a `CompressContainerReader`
    pub fn new(mut f: R, size: u64) -> Result<Self, io::Error> {
        if size < 1 || rdu8(&mut f)? != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid signature in CompressedContainer",
            ));
        }
        Ok(Self {
            f,
            avail_in: size - 1,
            start: 0,
            end: 0,
            buf: [0u8; 4096],
        })
    }

    fn get_token_len_off(&mut self) -> Result<(usize, usize), io::Error> {
        let token = rdu16le(&mut self.f)?;
        let tparts = match self.end {
            1..=16 => ((token & 0xfff), (token >> 12)),
            17..=32 => ((token & 0x7ff), (token >> 11)),
            33..=64 => ((token & 0x3ff), (token >> 10)),
            65..=128 => ((token & 0x1ff), (token >> 9)),
            129..=256 => ((token & 0xff), (token >> 8)),
            257..=512 => ((token & 0x7f), (token >> 7)),
            513..=1024 => ((token & 0x3f), (token >> 6)),
            1025..=2048 => ((token & 0x1f), (token >> 5)),
            2049..=4096 => ((token & 0x0f), (token >> 4)),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid token encoding",
                ));
            }
        };
        let len = usize::from(tparts.0) + 3;
        let off = usize::from(tparts.1) + 1;
        if off > self.end {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CopyToken underflow",
            ));
        }
        if self.buf.get(self.end..(self.end + len)).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CopyToken overflow",
            ));
        }
        Ok((len, off))
    }

    fn read_chunk(&mut self) -> Result<(), io::Error> {
        if self.avail_in < 2 {
            return Ok(());
        }
        self.avail_in -= 2;
        let chunk_hdr = rdu16le(&mut self.f)?;
        let mut chunk_size = umin(usize::from((chunk_hdr & 0xfff) + 1), self.avail_in);
        let chunk_sig = (chunk_hdr >> 12) & 0b111;
        if chunk_sig != 0b011 {
            // Office ignores the signature
        }
        let chunk_is_compressed = (chunk_hdr & 0x8000) != 0;

        self.avail_in -= chunk_size as u64; // Cast is safe due to umin above
        self.start = 0;
        self.end = 0;

        if !chunk_is_compressed {
            self.end = chunk_size;
            self.f.read_exact(&mut self.buf[0..chunk_size])?;
            return Ok(());
        }

        while chunk_size > 0 {
            let flag_byte = rdu8(&mut self.f)?;
            chunk_size -= 1;
            for i in 0..8 {
                if flag_byte & (1 << i) == 0 {
                    // LiteralToken
                    if chunk_size < 1 {
                        break;
                    }
                    if let Some(p) = self.buf.get_mut(self.end) {
                        *p = rdu8(&mut self.f)?;
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "LiteralToken overflow",
                        ));
                    }
                    self.end += 1;
                    chunk_size -= 1;
                } else {
                    // CopyToken
                    if chunk_size < 2 {
                        break;
                    }
                    let (len, off) = self.get_token_len_off()?;
                    chunk_size -= 2;
                    for _ in 0..len {
                        self.buf[self.end] = self.buf[self.end - off];
                        self.end += 1;
                    }
                }
            }
        }
        Ok(())
    }
}

impl<R: Read> Read for CompressContainerReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        if self.start == self.end {
            self.read_chunk()?;
        }
        let size = umin(self.end - self.start, buf.len());
        buf[0..size].copy_from_slice(&self.buf[self.start..(self.start + size)]);
        self.start += size;
        Ok(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    #[test]
    fn test_compress_container_reader() -> Result<(), Box<dyn Error>> {
        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x01, // CompressedStream signature
            0x19, 0xB0, // ChunkHeader (compressed=true, size=(25+1)=26)
            0x00, // all LiteralToken's
            0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, // [a-h]
            0x00, // all LiteralToken's
            0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x70, // [i-p]
            0x00, // all LiteralToken's
            0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x2E, // [q-v].
        ];
        let mut f = CompressContainerReader::new(buf.as_slice(), buf.len() as u64)?;
        let mut out: Vec<u8> = Vec::new();
        f.read_to_end(&mut out)?;
        assert_eq!(String::from_utf8(out)?, "abcdefghijklmnopqrstuv.");

        #[rustfmt::skip]
        let buf: Vec<u8> = vec![
            0x01, // CompressedStream signature
            0x2F, 0xB0, // ChunkHeader (compressed=true, size=(47+1)=48)
            0x00, // l,l,l,l,l,l,l,l
            0x23, 0x61, 0x61, 0x61, 0x62, 0x63, 0x64, 0x65, 0x82, // l,c,l,l,l,l,l,c
            0x66, // LiteralToken
            0x00, 0x70, // CopyToken (len=(0+3)=3, off=(7+3)=10)
            0x61, 0x67, 0x68, 0x69, 0x6A, // LiteralToken's
            0x01, 0x38, // CopyToken (len=(1+3)=4, off=(7+3)=10)
            0x08, // l,l,l,c,l,l,l,l
            0x61, 0x6B, 0x6C, // LiteralToken's
            0x00, 0x30, // CopyToken (len=(0+3)=3, off=(6+3)=9)
            0x6D, 0x6E, 0x6F, 0x70, // LiteralToken's
            0x06, // l,c,c,l,l,l,l,l
            0x71, // LiteralToken
            0x02, 0x70, // CopyToken (len=(5+3)=8, off=(14+3)=17)
            0x04, 0x10, // CopyToken (len=(4+3)=7, off=(2+3)=5)
            0x72, 0x73, 0x74, 0x75, 0x76, // LiteralToken's
            0x10, // l,l,l,l,c,l,l,l
            0x77, 0x78, 0x79, 0x7A, 0x00, 0x3C, // CopyToken (len=(0+3)=3, off=(15+3)=18)
        ];
        let mut f = CompressContainerReader::new(buf.as_slice(), buf.len() as u64)?;
        let mut out: Vec<u8> = Vec::new();
        f.read_to_end(&mut out)?;
        assert_eq!(
            String::from_utf8(out)?,
            "#aaabcdefaaaaghijaaaaaklaaamnopqaaaaaaaaaaaarstuvwxyzaaa"
        );

        let buf: Vec<u8> = vec![0x01, 0x03, 0xB0, 0x02, 0x61, 0x45, 0x00];
        let mut f = CompressContainerReader::new(buf.as_slice(), buf.len() as u64)?;
        let mut out: Vec<u8> = Vec::new();
        f.read_to_end(&mut out)?;
        assert_eq!(
            String::from_utf8(out)?,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );

        Ok(())
    }
}
