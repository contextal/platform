//! A pure safe rust implementation of inflate64
//!
//! Losely based of puff.c / infback9 by Mark Adler
//!
//! See [`Inflate64Stream`]
use crate::utils::{CircularBuffer, TwinWriter};
use std::collections::VecDeque;
use std::io::Read;

/// Maximum number of bits per Huffman code
const MAXBITS: usize = 15;
/// Maximum number of codes in *litlen* dynamic trees
const MAXLCODES: usize = 286;
/// Maximum number of codes in distance trees (fixed and dynamic)
const MAXDCODES: usize = 32;
/// Number of codes in *litlen* the static tree
const FIXLCODES: usize = 288;
/// Size of the input buffer
const INBUFSIZ: usize = 8 * 1024;
/// Size for the sliding window
const BUFSIZ: usize = 64 * 1024;

/// DEFLATE streaming decompressor
///
/// Use the `Read` trait
pub struct Inflate64Stream<R: Read> {
    /// The wrapped `Read`er
    r: R,
    /// Bitstream
    bits: usize,
    /// Number of bits in the bitstream
    nbits: usize,
    /// Input buffer
    input: CircularBuffer<INBUFSIZ>,
    /// Output buffer
    output: VecDeque<u8>,
    /// Sliding window (last 32KB of output)
    window: CircularBuffer<BUFSIZ>,
    /// EOF flag
    eof: bool,
}

impl<R: Read> Inflate64Stream<R> {
    /// Creates the decompressor
    pub fn new(r: R) -> Self {
        Self {
            r,
            bits: 0,
            nbits: 0,
            input: CircularBuffer::new(),
            output: VecDeque::with_capacity(4 * 1024),
            window: CircularBuffer::new(),
            eof: false,
        }
    }

    /// Fills the buffer from the stream if empty and returns the first byte
    fn read_byte(&mut self) -> Result<u8, std::io::Error> {
        if self.input.is_empty() && !self.eof {
            self.eof =
                std::io::copy(&mut (&mut self.r).take(INBUFSIZ as u64), &mut self.input)? == 0;
        }
        self.input
            .pop_front()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))
    }

    /// Returns the requested number of bits consuming the stream
    fn getbits(&mut self, n: usize) -> Result<usize, std::io::Error> {
        while self.nbits < n {
            let next = self.read_byte()?;
            self.bits |= usize::from(next) << self.nbits;
            self.nbits += 8;
        }
        let ret = self.bits & ((1 << n) - 1);
        self.bits >>= n;
        self.nbits -= n;
        Ok(ret)
    }

    /// Drops any bits in the bitstream
    fn dropbits(&mut self) {
        self.nbits = 0;
        self.bits = 0;
    }

    /// Appends one byte to the output buffer and to the window
    fn push_output_byte(&mut self, b: u8) {
        self.output.push_back(b);
        self.window.push_back(b);
    }

    /// Handles stored blocks (type 0)
    fn stored(&mut self) -> Result<(), std::io::Error> {
        self.dropbits();
        let len: u16 = (self.read_byte()? as u16) | ((self.read_byte()? as u16) << 8);
        let not_len: u16 = (self.read_byte()? as u16) | ((self.read_byte()? as u16) << 8);
        if len != !not_len {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Stored len mismatch",
            ));
        }
        let mut len: u64 = len.into();
        if len > BUFSIZ as u64 {
            len -= BUFSIZ as u64;
            // Grab from the internal buffer first...
            len -= std::io::copy(&mut (&mut self.input).take(len), &mut self.output)?;
            // Then straight from r
            if len > 0 {
                std::io::copy(&mut (&mut self.r).take(len), &mut self.output)?;
            }
            len = BUFSIZ as u64;
        }
        let mut tw = TwinWriter {
            a: &mut self.output,
            b: &mut self.window,
        };
        // Grab from the internal buffer first...
        len -= std::io::copy(&mut (&mut self.input).take(len), &mut tw)?;
        // Then straight from r
        if len > 0 {
            len -= std::io::copy(&mut (&mut self.r).take(len), &mut tw)?;
        }
        if len > 0 {
            Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))
        } else {
            Ok(())
        }
    }

    /// Huffman decoder (slow version)
    ///
    /// Canonical traversing of canonical Huffman trees, one bit at a time
    fn huffman_decode_slow(&mut self, huf: &Huffman) -> Result<u16, std::io::Error> {
        let mut code = 0usize;
        for slot in 1..=MAXBITS {
            code |= self.getbits(1)?;
            if let Some(symbol) = huf.get(slot, code) {
                return Ok(symbol);
            }
            code -= huf.table[slot].len();
            code <<= 1;
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Code not found",
        ))
    }

    /// Huffman decoder (faster version)
    ///
    /// The normal flow would just call [`getbits(1)`] (up till until a symbol is found - see
    /// [`huffman_decode_slow()`](Self::huffman_decode_slow). However that's extremely slow.
    ///
    /// Instead this function reaches directly into the input buffer data. If enough bits are
    /// available (up to [`MAXBITS`] may be needed), they are retrieved but not consumed. I.e.
    /// one or two extra bytes are copied locally and accumulated into a local bitstream.
    ///
    /// The tree is then traversed using the local bits and the number of bits actually utilized
    /// is finally consumed
    ///
    /// Notes:
    ///
    /// * In case of error the state is left in inconsistent state (but that's fine)
    ///
    /// * In case the buffer lacks enough bits, the code falls back to the slow version (that's quite rare)
    fn huffman_decode(&mut self, huf: &Huffman) -> Result<u16, std::io::Error> {
        let mut sbits = self.bits;
        if self.input.len() * 8 < MAXBITS - self.nbits {
            // Reading is required, taking the slow path
            return self.huffman_decode_slow(huf);
        }
        sbits |= usize::from(self.input[0]) << (self.nbits);
        if self.nbits + 8 < MAXBITS {
            sbits |= usize::from(self.input[1]) << (self.nbits + 8);
        }
        let mut code = 0usize;
        for slot in 1..=MAXBITS {
            code |= sbits & 1;
            sbits >>= 1;
            if let Some(symbol) = huf.get(slot, code) {
                // This is a faster equivalent to self.getbits(slot): it consumes (a portion) of the bits
                // either from the bitstream alone or from the byte(s) peeked above too
                if slot <= self.nbits {
                    // Only the bitstream was consumed
                    self.nbits -= slot;
                    self.bits >>= slot;
                } else {
                    // Some extra bytes were consumed too
                    let mut borrowed = slot - self.nbits;
                    if borrowed > 8 {
                        self.input.discard(2);
                        borrowed -= 8;
                    } else {
                        self.input.discard(1);
                    }
                    self.nbits = 8 - borrowed;
                    self.bits = sbits & ((1 << self.nbits) - 1);
                }
                return Ok(symbol);
            }
            code -= huf.table[slot].len();
            code <<= 1;
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Code not found",
        ))
    }

    /// Common decompressor for fixed and dynamic blocks (types 1 and 2)
    fn compressed_common(
        &mut self,
        huf_litlen: &Huffman,
        huf_dist: &Huffman,
    ) -> Result<(), std::io::Error> {
        fn get_or_err<T: Copy>(buf: &[T], n: usize) -> Result<T, std::io::Error> {
            buf.get(n).copied().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid symbol mapping")
            })
        }
        // LitLens codes:
        // 0..=255   -> literal
        // 256       -> end of block
        // 257..=285 -> lengths (either complete or with extra bits)
        const LENS: [usize; 29] = [
            // Lengths
            3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99,
            115, 131, 163, 195, 227, 3,
        ];
        const LEXT: [usize; 29] = [
            // Extra bits for lengths
            0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 16,
        ];
        // Dists have a base offset and are either complete or have extra bits
        const DISTS: [usize; 32] = [
            // Base offsets for dists
            1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025,
            1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577, 32769, 49153,
        ];
        const DEXT: [usize; 32] = [
            // Extra bits for dists
            0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12,
            12, 13, 13, 14, 14,
        ];

        loop {
            let symbol = self.huffman_decode(huf_litlen)?;
            if symbol == 256 {
                break;
            }
            if symbol < 256 {
                // Literal
                self.push_output_byte(symbol as u8);
                continue;
            }

            // Length
            let symbol = usize::from(symbol - 257);
            let len = get_or_err(&LENS, symbol)? + self.getbits(get_or_err(&LEXT, symbol)?)?;
            // Distance
            let symbol = usize::from(self.huffman_decode(huf_dist)?);
            let dist = DISTS[symbol] + self.getbits(DEXT[symbol])?;
            if dist > self.window.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Distance underflow",
                ));
            }
            for _ in 0..len {
                let v = self.window[self.window.len() - dist];
                self.push_output_byte(v);
            }
        }
        Ok(())
    }

    /// Decompressor for fixed blocks (type 1)
    fn fixed(&mut self) -> Result<(), std::io::Error> {
        self.compressed_common(&FIXED_LITLEN, &FIXED_DIST)
    }

    /// Decompressor for dynamic blocks (type 2)
    fn dynamic(&mut self) -> Result<(), std::io::Error> {
        const ORDER: [usize; 19] = [
            // permutation of code length codes
            16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
        ];
        let nlen = self.getbits(5)? + 257;
        let ndist = self.getbits(5)? + 1;
        let ncode = self.getbits(4)? + 4;
        if nlen > MAXLCODES || ndist > MAXDCODES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Dynamic counts overflow",
            ));
        }
        let mut lens = [0u8; 19];
        for i in 0..ncode {
            lens[ORDER[i]] = self.getbits(3)? as u8;
        }
        for i in ncode..19 {
            lens[ORDER[i]] = 0;
        }
        // litlens and dists are huffman trees wrapped in a huffman tree
        // the outer envelope is unwrapped here
        let huf_litlen_dist = Huffman::build(&lens).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Huffman envelope build failure",
            )
        })?;
        if !huf_litlen_dist.is_complete {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Huffman envelope is not complete",
            ));
        }

        // in fact they're nor merely wrapped but compressed too
        // they're uncompressed here (both litlen and dists in a single step)
        let mut lens = [0u8; MAXLCODES + MAXDCODES];
        let mut i = 0usize;
        while i < nlen + ndist {
            let symbol = self.huffman_decode(&huf_litlen_dist)?;
            let (value, times) = match symbol {
                0..=15 => {
                    // no repeatition
                    (symbol as u8, 1usize)
                }
                16 => {
                    // repeat 3 to 6 times
                    if i == 0 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Dynamic single byte underflow",
                        ));
                    }
                    (lens[i - 1], 3 + self.getbits(2)?)
                }
                17 => {
                    // 0 repeated 3 to 10 times
                    (0, 3 + self.getbits(3)?)
                }
                _ => {
                    // 0 repeated 11 to 138
                    (0, 11 + self.getbits(7)?)
                }
            };
            let range = lens.get_mut(i..(i + times)).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Dynamic fill range overflow",
                )
            })?;
            range.fill(value);
            i += times;
        }

        if lens[256] == 0 {
            // check for end of block presence
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Dynamic tree with no end",
            ));
        }

        let litlen = Huffman::build(&lens[0..nlen]).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Huffman litlen build failure",
            )
        })?;
        if !litlen.is_complete && litlen.nlen1() != nlen {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Dynamic litlen is invalid",
            ));
        }
        let dist = Huffman::build(&lens[nlen..(nlen + ndist)]).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Huffman dist build failure",
            )
        })?;
        if !dist.is_complete && dist.nlen1() != ndist {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Dynamic dist is invalid",
            ));
        }
        self.compressed_common(&litlen, &dist)
    }

    /// Decompresses the next block
    fn inflate_block(&mut self) -> Result<bool, std::io::Error> {
        self.output.clear();
        let last_block = self.getbits(1)? == 1;
        let block_type = self.getbits(2)?;
        match block_type {
            0 => self.stored(),
            1 => self.fixed(),
            2 => self.dynamic(),
            3 => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid block type",
            )),
            _ => unreachable!(),
        }?;
        Ok(last_block)
    }
}

impl<R: Read> Read for Inflate64Stream<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        while self.output.is_empty() && !self.eof {
            // Sometimes an empty block is present which is not the last block
            // This loop avoids read() returning 0 on such blocks which would
            // make the caller incorrectly assume the stream is exhausted
            self.eof = self.inflate_block()?;
        }
        self.output.read(buf)
    }
}

/// Canonical Huffman tree in a relatively compact form
///
/// Symbols are arranged in separate vectors, one per code length (named slots below)
#[derive(Default, Debug)]
struct Huffman {
    table: [Vec<u16>; MAXBITS + 1],
    is_complete: bool,
}

impl Huffman {
    /// Builds a Huffman tree from the array of lengths
    ///
    /// Fails if any length is oversubscribed
    fn build(slot_map: &[u8]) -> Option<Self> {
        let mut ret = Self::default();

        // Stable (!) sort by slot
        for (symbol, slot) in slot_map.iter().enumerate() {
            ret.table[usize::from(*slot)].push(symbol as u16);
        }
        // Available slots in a Huffman tree at each depth is the pow2() of the depth
        // minus the used slots above that depth
        let mut available = 1usize;
        for slot in 1..=MAXBITS {
            available <<= 1;
            let used = ret.table[slot].len();
            // Return None if this slot is over subscribed
            available = available.checked_sub(used)?;
        }
        ret.is_complete = available == 0;
        Some(ret)
    }

    /// Returns the nth entry of the given slot, if any
    fn get(&self, slot: usize, code: usize) -> Option<u16> {
        self.table[slot].get(code).copied()
    }

    /// Used for checking for incomplete codes that have more than one symbol
    fn nlen1(&self) -> usize {
        self.table[0].len() + self.table[1].len()
    }
}

lazy_static::lazy_static! {
    /// Literals-lengths used in fixed block types
    static ref FIXED_LITLEN: Huffman = {
        let mut lens = [8u8; FIXLCODES];
        lens[144..256].fill(9);
        lens[256..280].fill(7);
        Huffman::build(&lens).unwrap()
    };

    /// Distances used in fixed block types
    static ref FIXED_DIST: Huffman = {
        Huffman::build(&[5u8; MAXDCODES]).unwrap()
    };
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_getbits() {
        let mut z = Inflate64Stream::new([0xff, 0x0, 0x55].as_ref());
        assert_eq!(z.getbits(0).unwrap(), 0);
        assert_eq!(z.getbits(4).unwrap(), 0xf);
        assert_eq!(z.getbits(8).unwrap(), 0xf);
        assert!(z.getbits(13).is_err());
    }

    #[test]
    fn huffman_fixed_litlen() {
        assert!(FIXED_LITLEN.table[0].is_empty());
        assert!(FIXED_LITLEN.table[1].is_empty());
        assert!(FIXED_LITLEN.table[2].is_empty());
        assert!(FIXED_LITLEN.table[3].is_empty());
        assert!(FIXED_LITLEN.table[4].is_empty());
        assert!(FIXED_LITLEN.table[5].is_empty());
        assert!(FIXED_LITLEN.table[6].is_empty());
        assert!(FIXED_LITLEN.table[7]
            .iter()
            .map(|v| *v)
            .eq((256u16..280).into_iter()));
        assert!(FIXED_LITLEN.table[8][0..144]
            .iter()
            .map(|v| *v)
            .eq((0u16..144).into_iter()));
        assert!(FIXED_LITLEN.table[8][144..]
            .iter()
            .map(|v| *v)
            .eq((280u16..288).into_iter()));
        assert!(FIXED_LITLEN.table[9]
            .iter()
            .map(|v| *v)
            .eq((144u16..256).into_iter()));
        assert!(FIXED_LITLEN.table[10].is_empty());
        assert!(FIXED_LITLEN.table[11].is_empty());
        assert!(FIXED_LITLEN.table[12].is_empty());
        assert!(FIXED_LITLEN.table[13].is_empty());
        assert!(FIXED_LITLEN.table[14].is_empty());
        assert!(FIXED_LITLEN.table[15].is_empty());
        assert!(FIXED_LITLEN.is_complete);
    }

    #[test]
    fn huffman_fixed_distance() {
        assert!(FIXED_DIST.table[0].is_empty());
        assert!(FIXED_DIST.table[1].is_empty());
        assert!(FIXED_DIST.table[2].is_empty());
        assert!(FIXED_DIST.table[3].is_empty());
        assert!(FIXED_DIST.table[4].is_empty());
        assert!(FIXED_DIST.table[5]
            .iter()
            .map(|v| *v)
            .eq((0u16..32).into_iter()));
        assert!(FIXED_DIST.table[6].is_empty());
        assert!(FIXED_DIST.table[7].is_empty());
        assert!(FIXED_DIST.table[8].is_empty());
        assert!(FIXED_DIST.table[9].is_empty());
        assert!(FIXED_DIST.table[10].is_empty());
        assert!(FIXED_DIST.table[11].is_empty());
        assert!(FIXED_DIST.table[12].is_empty());
        assert!(FIXED_DIST.table[13].is_empty());
        assert!(FIXED_DIST.table[14].is_empty());
        assert!(FIXED_DIST.table[15].is_empty());
        assert!(FIXED_DIST.is_complete);
    }

    #[test]
    fn inflate() {
        let mut inf = Inflate64Stream::new(b"\x01\x04\x00\x13\x37aCaB".as_ref());
        assert!(inf.inflate_block().is_err(), "badstored1 error");

        let mut inf = Inflate64Stream::new(b"\x01\x05\x00\xfa\xffaCaB".as_ref());
        assert!(inf.inflate_block().is_err(), "badstored2 error");

        let mut inf = Inflate64Stream::new(b"\x01\x04\x00\xfb\xffaCaB".as_ref());
        assert!(inf.inflate_block().is_ok(), "storedok error");
        assert_eq!(&inf.output, b"aCaB", "storedok mismatch");

        let mut inf = Inflate64Stream::new(b"\x00\x04\x00\xfb\xffaCaB".as_ref());
        assert!(inf.inflate_block().is_ok(), "eof1");
        assert!(inf.inflate_block().is_err(), "eof1");

        let mut inf = Inflate64Stream::new(b"\x03".as_ref());
        assert!(inf.inflate_block().is_err(), "badfixed error");

        let mut inf = Inflate64Stream::new(b"KtNt\x02\x00".as_ref());
        assert!(inf.inflate_block().is_ok(), "fixedok error");
        assert_eq!(&inf.output, b"aCaB", "fixedok mismatch");

        let mut inf = Inflate64Stream::new(b"\x07aCaB".as_ref());
        assert!(inf.inflate_block().is_err(), "badtype error");

        // From the zlib coverage suite
        let tests: [&[u8]; 17] = [
            b"\x04",
            b"\x00",
            b"\x00\x00\x00\x00\x00",
            b"\x00\x01\x00\xfe\xff",
            b"\x02\x7e\xff\xff",
            b"\x02",
            b"\x04\x80\x49\x92\x24\x49\x92\x24\x0f\xb4\xff\xff\xc3\x04",
            b"\x04\x80\x49\x92\x24\x49\x92\x24\x71\xff\xff\x93\x11\x00",
            b"\x04\xc0\x81\x08\x00\x00\x00\x00\x20\x7f\xeb\x0b\x00\x00",
            b"\x1a\x07",
            b"\x0c\xc0\x81\x00\x00\x00\x00\x00\x90\xff\x6b\x04",
            b"\xfc\x00\x00",
            b"\x04\x00\xfe\xff",
            b"\x04\x00\x24\x49",
            b"\x04\x80\x49\x92\x24\x49\x92\x24\x0f\xb4\xff\xff\xc3\x84",
            b"\x04\x00\x24\xe9\xff\xff",
            b"\x04\x00\x24\xe9\xff\x6d",
        ];
        for (test, bytes) in tests.into_iter().enumerate() {
            let mut inf = Inflate64Stream::new(bytes);
            assert!(inf.inflate_block().is_err(), "zlib #{} error", test);
        }
    }
}
