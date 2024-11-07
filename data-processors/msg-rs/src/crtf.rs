//! Compressed RTF decompressor
//!
//! Intended for internal use but publicly exposed for research purposes and low
//! level operations
use ctxutils::{cmp::*, io::*};
use std::io::{self, Read};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};

static DICT_INIT: &[u8; 207] =
    b"{\\rtf1\\ansi\\mac\\deff0\\deftab720{\\fonttbl;}{\\f0\\fnil \\froman \\fswiss \\fmodern \\fscript \\fdecor MS Sans SerifSymbolArialTimes New RomanCourier{\\colortbl\\red0\\green0\\blue0\r\n\\par \\pard\\plain\\f0\\fs20\\b\\i\\u\\tab\\tx";

struct Dict {
    buf: [u8; 4096],
    wr: usize,
    rd: usize,
    sz: usize,
    control: u8,
    control_bits: u8,
    left: [u8; 18],
    leftsz: usize,
}

impl Dict {
    fn next_token(&mut self) -> Option<bool> {
        if self.control_bits == 0 {
            None
        } else {
            let res = self.control & 1 != 0;
            self.control >>= 1;
            self.control_bits -= 1;
            Some(res)
        }
    }

    fn set_control(&mut self, control: u8) -> bool {
        assert_eq!(self.control_bits, 0);
        self.control = control;
        self.control_bits = 8;
        self.next_token().unwrap()
    }

    fn push_byte(&mut self, byte: u8) {
        self.buf[self.wr] = byte;
        self.wr = (self.wr + 1) % 4096;
        self.sz = (self.sz + 1).min(4096);
    }

    fn set_offset(&mut self, off: usize) -> Result<(), io::Error> {
        if self.sz <= off {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid compressed stream",
            ))
        } else {
            self.rd = off;
            Ok(())
        }
    }

    fn pop_byte(&mut self) -> u8 {
        let res = self.buf[self.rd];
        self.push_byte(res);
        self.rd = (self.rd + 1) % 4096;
        res
    }
}

impl Default for Dict {
    fn default() -> Self {
        let mut buf = [0u8; 4096];
        buf[0..207].copy_from_slice(DICT_INIT);
        Self {
            buf,
            wr: 207,
            rd: 0,
            sz: 207,
            control: 0,
            control_bits: 0,
            left: [0u8; 18],
            leftsz: 0,
        }
    }
}

const CRC_LUT: [u32; 256] = [
    0x00000000, 0x77073096, 0xee0e612c, 0x990951ba, 0x076dc419, 0x706af48f, 0xe963a535, 0x9e6495a3,
    0x0edb8832, 0x79dcb8a4, 0xe0d5e91e, 0x97d2d988, 0x09b64c2b, 0x7eb17cbd, 0xe7b82d07, 0x90bf1d91,
    0x1db71064, 0x6ab020f2, 0xf3b97148, 0x84be41de, 0x1adad47d, 0x6ddde4eb, 0xf4d4b551, 0x83d385c7,
    0x136c9856, 0x646ba8c0, 0xfd62f97a, 0x8a65c9ec, 0x14015c4f, 0x63066cd9, 0xfa0f3d63, 0x8d080df5,
    0x3b6e20c8, 0x4c69105e, 0xd56041e4, 0xa2677172, 0x3c03e4d1, 0x4b04d447, 0xd20d85fd, 0xa50ab56b,
    0x35b5a8fa, 0x42b2986c, 0xdbbbc9d6, 0xacbcf940, 0x32d86ce3, 0x45df5c75, 0xdcd60dcf, 0xabd13d59,
    0x26d930ac, 0x51de003a, 0xc8d75180, 0xbfd06116, 0x21b4f4b5, 0x56b3c423, 0xcfba9599, 0xb8bda50f,
    0x2802b89e, 0x5f058808, 0xc60cd9b2, 0xb10be924, 0x2f6f7c87, 0x58684c11, 0xc1611dab, 0xb6662d3d,
    0x76dc4190, 0x01db7106, 0x98d220bc, 0xefd5102a, 0x71b18589, 0x06b6b51f, 0x9fbfe4a5, 0xe8b8d433,
    0x7807c9a2, 0x0f00f934, 0x9609a88e, 0xe10e9818, 0x7f6a0dbb, 0x086d3d2d, 0x91646c97, 0xe6635c01,
    0x6b6b51f4, 0x1c6c6162, 0x856530d8, 0xf262004e, 0x6c0695ed, 0x1b01a57b, 0x8208f4c1, 0xf50fc457,
    0x65b0d9c6, 0x12b7e950, 0x8bbeb8ea, 0xfcb9887c, 0x62dd1ddf, 0x15da2d49, 0x8cd37cf3, 0xfbd44c65,
    0x4db26158, 0x3ab551ce, 0xa3bc0074, 0xd4bb30e2, 0x4adfa541, 0x3dd895d7, 0xa4d1c46d, 0xd3d6f4fb,
    0x4369e96a, 0x346ed9fc, 0xad678846, 0xda60b8d0, 0x44042d73, 0x33031de5, 0xaa0a4c5f, 0xdd0d7cc9,
    0x5005713c, 0x270241aa, 0xbe0b1010, 0xc90c2086, 0x5768b525, 0x206f85b3, 0xb966d409, 0xce61e49f,
    0x5edef90e, 0x29d9c998, 0xb0d09822, 0xc7d7a8b4, 0x59b33d17, 0x2eb40d81, 0xb7bd5c3b, 0xc0ba6cad,
    0xedb88320, 0x9abfb3b6, 0x03b6e20c, 0x74b1d29a, 0xead54739, 0x9dd277af, 0x04db2615, 0x73dc1683,
    0xe3630b12, 0x94643b84, 0x0d6d6a3e, 0x7a6a5aa8, 0xe40ecf0b, 0x9309ff9d, 0x0a00ae27, 0x7d079eb1,
    0xf00f9344, 0x8708a3d2, 0x1e01f268, 0x6906c2fe, 0xf762575d, 0x806567cb, 0x196c3671, 0x6e6b06e7,
    0xfed41b76, 0x89d32be0, 0x10da7a5a, 0x67dd4acc, 0xf9b9df6f, 0x8ebeeff9, 0x17b7be43, 0x60b08ed5,
    0xd6d6a3e8, 0xa1d1937e, 0x38d8c2c4, 0x4fdff252, 0xd1bb67f1, 0xa6bc5767, 0x3fb506dd, 0x48b2364b,
    0xd80d2bda, 0xaf0a1b4c, 0x36034af6, 0x41047a60, 0xdf60efc3, 0xa867df55, 0x316e8eef, 0x4669be79,
    0xcb61b38c, 0xbc66831a, 0x256fd2a0, 0x5268e236, 0xcc0c7795, 0xbb0b4703, 0x220216b9, 0x5505262f,
    0xc5ba3bbe, 0xb2bd0b28, 0x2bb45a92, 0x5cb36a04, 0xc2d7ffa7, 0xb5d0cf31, 0x2cd99e8b, 0x5bdeae1d,
    0x9b64c2b0, 0xec63f226, 0x756aa39c, 0x026d930a, 0x9c0906a9, 0xeb0e363f, 0x72076785, 0x05005713,
    0x95bf4a82, 0xe2b87a14, 0x7bb12bae, 0x0cb61b38, 0x92d28e9b, 0xe5d5be0d, 0x7cdcefb7, 0x0bdbdf21,
    0x86d3d2d4, 0xf1d4e242, 0x68ddb3f8, 0x1fda836e, 0x81be16cd, 0xf6b9265b, 0x6fb077e1, 0x18b74777,
    0x88085ae6, 0xff0f6a70, 0x66063bca, 0x11010b5c, 0x8f659eff, 0xf862ae69, 0x616bffd3, 0x166ccf45,
    0xa00ae278, 0xd70dd2ee, 0x4e048354, 0x3903b3c2, 0xa7672661, 0xd06016f7, 0x4969474d, 0x3e6e77db,
    0xaed16a4a, 0xd9d65adc, 0x40df0b66, 0x37d83bf0, 0xa9bcae53, 0xdebb9ec5, 0x47b2cf7f, 0x30b5ffe9,
    0xbdbdf21c, 0xcabac28a, 0x53b39330, 0x24b4a3a6, 0xbad03605, 0xcdd70693, 0x54de5729, 0x23d967bf,
    0xb3667a2e, 0xc4614ab8, 0x5d681b02, 0x2a6f2b94, 0xb40bbe37, 0xc30c8ea1, 0x5a05df1b, 0x2d02ef8d,
];

/// A simple stream reader which implements [MS-OXRTFCP] checksumming
pub struct CrcReader<R: Read> {
    stream: R,
    crc: u32,
}

impl<R: Read> Read for CrcReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let res = self.stream.read(buf);
        if let Ok(sz) = res {
            let mut crc = self.crc;
            for b in &buf[0..sz] {
                crc = CRC_LUT[(crc as u8 ^ *b) as usize] ^ (crc >> 8);
            }
            self.crc = crc;
        }
        res
    }
}

struct Compressed<R: Read> {
    stream: CrcReader<R>,
    compsz: u32,
    rawsz: u32,
    dict: Dict,
}

impl<R: Read> Read for Compressed<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let mut avail_out = buf.len().umin(self.rawsz);
        let mut out_pos = 0usize;
        while avail_out > 0 && self.compsz > 0 {
            if self.dict.leftsz > 0 {
                self.dict.leftsz -= 1;
                buf[out_pos] = self.dict.left[self.dict.leftsz];
                out_pos += 1;
                avail_out -= 1;
                continue;
            }
            let control = if let Some(t) = self.dict.next_token() {
                t
            } else {
                if self.compsz < 1 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Truncated CompressedRtf stream",
                    ));
                }
                let res = self.dict.set_control(rdu8(&mut self.stream)?);
                self.compsz -= 1;
                res
            };
            if control {
                // Copy from dict
                if self.compsz < 2 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Truncated CompressedRtf stream",
                    ));
                }
                self.compsz -= 2;
                let dictref = rdu16be(&mut self.stream)?;
                let off = usize::from(dictref >> 4);
                if off == self.dict.wr {
                    // complete
                    if self.compsz != 12 {
                        warn!("Compsz {} does not match header size", self.compsz);
                    }
                    self.compsz = 0;
                    break;
                }
                let len = usize::from(dictref & 0b1111) + 2;
                self.dict.set_offset(off)?;
                let copy_len = len.min(avail_out);
                for _ in 0..copy_len {
                    buf[out_pos] = self.dict.pop_byte();
                    out_pos += 1;
                }
                avail_out -= copy_len;
                self.dict.leftsz = len - copy_len;
                for i in (0..self.dict.leftsz).rev() {
                    self.dict.left[i] = self.dict.pop_byte();
                }
            } else {
                // Literal
                let lit = rdu8(&mut self.stream)?;
                self.compsz -= 1;
                buf[out_pos] = lit;
                out_pos += 1;
                self.dict.push_byte(lit);
                avail_out -= 1;
            }
        }
        Ok(out_pos)
    }
}

enum RtfStream<R: Read> {
    Compressed(Compressed<R>),
    Uncompressed(R),
}

/// A CompressedRtf decompressing reader
pub struct CompressedRtf<R: Read> {
    stream: RtfStream<R>,
    ref_crc: u32,
}

impl<R: Read> CompressedRtf<R> {
    /// Creates a new decompressing reader
    pub fn new(mut r: R) -> Result<Self, io::Error> {
        let compsz = rdu32le(&mut r)?;
        let rawsz = rdu32le(&mut r)?;
        let comp_type = rdu32le(&mut r)?;
        let ref_crc = rdu32le(&mut r)?;
        let stream = if comp_type == 0x75465a4c {
            RtfStream::Compressed(Compressed {
                stream: CrcReader { stream: r, crc: 0 },
                compsz,
                rawsz,
                dict: Dict::default(),
            })
        } else if comp_type == 0x414c454d {
            RtfStream::Uncompressed(r)
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid compression type value",
            ));
        };
        Ok(Self { stream, ref_crc })
    }

    /// Validates the stream crc
    pub fn has_valid_crc(&self) -> bool {
        match &self.stream {
            RtfStream::Compressed(stream) => stream.stream.crc == self.ref_crc,
            RtfStream::Uncompressed(_) => self.ref_crc == 0,
        }
    }
}

impl<R: Read> Read for CompressedRtf<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        match self.stream {
            RtfStream::Compressed(ref mut stream) => stream.read(buf),
            RtfStream::Uncompressed(ref mut stream) => {
                // GO FIGURE:
                // > When the COMPTYPE field is set to UNCOMPRESSED, the reader SHOULD read all bytes
                // > until the end of the stream is reached, regardless of the value of the RAWSIZE field. Or,
                // > the reader MAY read the number of bytes specified by the RAWSIZE field from the input
                // > (the Header field) and write them to the output.
                stream.read(buf)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_crc() -> Result<(), std::io::Error> {
        let buf = b"\x03\x00\x0a\x00\x72\x63\x70\x67\x31\x32\x35\x42\x32\x0a\xf3\x20\x68\x65\x6c\x09\x00\x20\x62\x77\x05\xb0\x6c\x64\x7d\x0a\x80\x0f\xa0";
        let mut crc = CrcReader {
            stream: buf.as_slice(),
            crc: 0,
        };
        std::io::copy(&mut crc, &mut std::io::sink())?;
        assert_eq!(crc.crc, 0xA7C7C5F1);
        Ok(())
    }

    fn assert_decomp(comp: &[u8], reference: &[u8]) -> Result<(), std::io::Error> {
        let mut r = CompressedRtf::new(comp)?;
        let mut decomp: Vec<u8> = Vec::new();
        r.read_to_end(&mut decomp)?;
        assert_eq!(&decomp, reference);
        Ok(())
    }

    #[test]
    fn test_decomp_1() -> Result<(), std::io::Error> {
        assert_decomp(
            b"\x2d\x00\x00\x00\x2b\x00\x00\x00\x4c\x5a\x46\x75\xf1\xc5\xc7\xa7\x03\x00\x0a\x00\x72\x63\x70\x67\x31\x32\x35\x42\x32\x0a\xf3\x20\x68\x65\x6c\x09\x00\x20\x62\x77\x05\xb0\x6c\x64\x7d\x0a\x80\x0f\xa0",
            b"{\\rtf1\\ansi\\ansicpg1252\\pard hello world}\r\n"
        )
    }

    #[test]
    fn test_decomp_2() -> Result<(), std::io::Error> {
        assert_decomp(
            b"\x1a\x00\x00\x00\x1c\x00\x00\x00\x4c\x5a\x46\x75\xe2\xd4\x4b\x51\x41\x00\x04\x20\x57\x58\x59\x5a\x0d\x6e\x7d\x01\x0e\xb0",
            b"{\\rtf1 WXYZWXYZWXYZWXYZWXYZ}"
        )
    }
}
