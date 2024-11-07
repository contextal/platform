//! Implode decompressor
use crate::utils::CircularBuffer;
use std::collections::VecDeque;
use std::io::Read;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn};

/// A stream decompressor for imploded entries
///
/// This is just a wrapper around [`ExplodeType`] to avoid leaking private types
/// ExplodeType furtherly wraps [`Explode`] in order to honor const generic
pub struct ExplodeStream<R: Read>(ExplodeType<R>);

impl<R: Read> ExplodeStream<R> {
    /// Creates the decompressor
    pub fn new(
        r: R,
        uncompressed_size: u64,
        large_window: bool,
        lit_tree: bool,
    ) -> Result<Self, std::io::Error> {
        Ok(Self(if large_window {
            ExplodeType::Explode8k(Explode::<8192, R>::new(r, uncompressed_size, lit_tree)?)
        } else {
            ExplodeType::Explode4k(Explode::<4096, R>::new(r, uncompressed_size, lit_tree)?)
        }))
    }
}

impl<R: Read> Read for ExplodeStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        match &mut self.0 {
            ExplodeType::Explode4k(ref mut e) => e.read(buf),
            ExplodeType::Explode8k(ref mut e) => e.read(buf),
        }
    }
}

/// Indicates the size of the dictionary (4K or 8K)
#[allow(clippy::large_enum_variant)]
enum ExplodeType<R: Read> {
    Explode4k(Explode<4096, R>),
    Explode8k(Explode<8192, R>),
}

/// Identifies one of the trees
enum LookupTree {
    Lits,
    Lens,
    Dists,
}

/// The size of the input buffer
const INBUFSIZ: usize = 8192;
/// An implode decompressor with a window size of either 4 or 8KiB
struct Explode<const WINDOW_SIZE: usize, R: Read> {
    r: R,
    lits: Option<Tree<256>>,
    lens: Tree<64>,
    dists: Tree<64>,
    input: CircularBuffer<INBUFSIZ>,
    output: VecDeque<u8>,
    window: CircularBuffer<WINDOW_SIZE>,
    todo: u64,
    bits: u8,
    nbits: u8,
}

impl<const WINDOW_SIZE: usize, R: Read> Explode<WINDOW_SIZE, R> {
    fn new(mut r: R, uncompressed_size: u64, lit_tree: bool) -> Result<Self, std::io::Error> {
        assert!(WINDOW_SIZE == 4 * 1024 || WINDOW_SIZE == 8 * 1024);
        let lits = if lit_tree {
            Some(Tree::<256>::new(&mut r)?)
        } else {
            None
        };
        let lens = Tree::<64>::new(&mut r)?;
        let dists = Tree::<64>::new(&mut r)?;
        Ok(Self {
            r,
            lits,
            lens,
            dists,
            input: CircularBuffer::new(),
            output: VecDeque::with_capacity(4 * 1024),
            window: CircularBuffer::new(),
            bits: 0,
            nbits: 0,
            todo: uncompressed_size,
        })
    }

    fn getbits<const N: u8>(&mut self) -> Result<u8, std::io::Error> {
        if N <= self.nbits {
            let ret = self.bits & ((1 << N) - 1);
            self.bits >>= N;
            self.nbits -= N;
            return Ok(ret);
        }
        if self.input.is_empty() {
            std::io::copy(&mut (&mut self.r).take(INBUFSIZ as u64), &mut self.input)?;
        }
        let have = N.min(self.nbits);
        let need = N - have;
        Ok(if need == 8 {
            // Implies have = 0 (8 is the highest bit count requested)
            // Requeired due to << going out of bounds on u8
            self.input
                .pop_front()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))?
        } else {
            let mut ret = self.bits & ((1 << have) - 1);
            self.bits = self
                .input
                .pop_front()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))?;
            ret |= (self.bits & ((1 << need) - 1)) << have;
            self.bits >>= need;
            self.nbits = 8 - need;
            ret
        })
    }

    fn table_lookup(&mut self, tree: LookupTree) -> Result<u8, std::io::Error> {
        let mut code = 0u16;
        for len in 0u16..16 {
            code |= u16::from(self.getbits::<1>()?) << (15 - len);
            if let Some(v) = match &tree {
                LookupTree::Lits => self.lits.as_ref().unwrap().lookup(len, code),
                LookupTree::Dists => self.dists.lookup(len, code),
                LookupTree::Lens => self.lens.lookup(len, code),
            } {
                return Ok(v);
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Code not found in tree",
        ))
    }

    fn decompress_byte(&mut self) -> Result<(), std::io::Error> {
        let is_literal = self.getbits::<1>()?;
        if is_literal == 1 {
            // Literal
            let lit = if self.lits.is_some() {
                self.table_lookup(LookupTree::Lits)?
            } else {
                self.getbits::<8>()?
            };
            self.output.push_back(lit);
            self.window.push_back(lit);
            self.todo -= 1;
        } else {
            // Distance/length encoded
            let dist = 1 + if WINDOW_SIZE == 8 * 1024 {
                usize::from(self.getbits::<7>()?)
                    | (usize::from(self.table_lookup(LookupTree::Dists)?) << 7)
            } else {
                usize::from(self.getbits::<6>()?)
                    | (usize::from(self.table_lookup(LookupTree::Dists)?) << 6)
            };
            let mut len = u64::from(self.table_lookup(LookupTree::Lens)?);
            if len == 63 {
                len += u64::from(self.getbits::<8>()?);
            }
            if self.lits.is_some() {
                len += 3;
            } else {
                len += 2;
            }
            len = len.min(self.todo);
            self.todo -= len;
            for _ in 0..len {
                let byte = if dist <= self.window.len() {
                    self.window[self.window.len() - dist]
                } else {
                    0u8
                };
                self.output.push_back(byte);
                self.window.push_back(byte);
            }
        }
        Ok(())
    }
}

impl<const WINDOW_SIZE: usize, R: Read> Read for Explode<WINDOW_SIZE, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        if self.output.is_empty() {
            while self.output.len() < buf.len() && self.todo > 0 {
                self.decompress_byte()?;
            }
        }
        self.output.read(buf)
    }
}

/// An implode Huffman tree
struct Tree<const SIZE: usize> {
    codes: [Vec<(u16, u8)>; 16],
}

impl<const SIZE: usize> Tree<SIZE> {
    fn new<R: Read>(r: &mut R) -> Result<Self, std::io::Error> {
        assert!(SIZE <= 256);
        // First byte indicates the count of packed lengths
        let cnt: u16 = u16::from(ctxutils::io::rdu8(r)?) + 1;
        let mut packed = Vec::with_capacity(usize::from(cnt));
        r.take(u64::from(cnt)).read_to_end(&mut packed)?;
        if packed.len() != usize::from(cnt) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "End of stream reached when reading implode tree",
            ));
        }
        // Unpack the lengths
        let mut lengths: Vec<u8> = Vec::with_capacity(SIZE);
        for p in packed {
            let length = (p & 0xf) + 1;
            let ncodes = (p >> 4) + 1;
            for _ in 0..ncodes {
                lengths.push(length);
            }
        }
        if lengths.len() != SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Expected a tree with {} codes but {} were found",
                    SIZE,
                    lengths.len()
                ),
            ));
        }
        // A parallel vector for stable sorting by length
        let mut ord: Vec<usize> = Vec::with_capacity(SIZE);
        for i in 0..SIZE {
            ord.push(i);
        }
        ord.sort_by(|a, b| lengths[*a].cmp(&lengths[*b]));
        // The tree is kept internally as an array of (reversed_code, value) pairs
        let mut codes: [Vec<(u16, u8)>; 16] = std::array::from_fn(|_| Vec::new());
        let mut code = 0u16;
        let mut codeinc = 0u16;
        let mut lastlen = 0u8;
        let mut i = SIZE;
        while i > 0 {
            i -= 1;
            code += codeinc;
            if lengths[ord[i]] != lastlen {
                lastlen = lengths[ord[i]];
                codeinc = 1 << ((16 - lastlen) as u16);
            }
            codes[lastlen as usize - 1].push((code, ord[i] as u8));
        }
        // For fast lookup the array is sorted by key - see lookup()
        for code in codes.iter_mut() {
            code.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        }
        Ok(Self { codes })
    }

    fn lookup(&self, len: u16, code: u16) -> Option<u8> {
        let len = len as usize;
        self.codes[len]
            .binary_search_by(|v| v.0.cmp(&code))
            .map(|found| self.codes[len][found].1)
            .ok()
    }
}

pub fn uses_large_window(gp_flag: u16) -> bool {
    gp_flag & (1 << 1) != 0
}

pub fn uses_lit_tree(gp_flag: u16) -> bool {
    gp_flag & (1 << 2) != 0
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn tree() {
        let t: Tree<8> = Tree::new(&mut ([0x02, 0x42, 0x01, 0x13].as_slice())).unwrap();
        assert_eq!(t.lookup(2, 0b1010000000000000), Some(0));
        assert_eq!(t.lookup(2, 0b1000000000000000), Some(1));
        assert_eq!(t.lookup(2, 0b0110000000000000), Some(2));
        assert_eq!(t.lookup(2, 0b0100000000000000), Some(3));
        assert_eq!(t.lookup(2, 0b0010000000000000), Some(4));
        assert_eq!(t.lookup(1, 0b1100000000000000), Some(5));
        assert_eq!(t.lookup(3, 0b0001000000000000), Some(6));
        assert_eq!(t.lookup(3, 0b0000000000000000), Some(7));
    }

    #[test]
    fn explode_4k() -> Result<(), std::io::Error> {
        fn explode_test(comp: &[u8], check: &[u8]) -> Result<(), std::io::Error> {
            let gp_flag = 0b000;
            let mut decomp: Vec<u8> = Vec::new();
            let mut stream = ExplodeStream::new(
                comp,
                check.len().try_into().unwrap(),
                uses_large_window(gp_flag),
                uses_lit_tree(gp_flag),
            )?;
            stream.read_to_end(&mut decomp)?;
            assert_eq!(decomp, check);
            Ok(())
        }

        explode_test(
            b"\x0d\x02\x01\x12\x23\x14\x15\x36\x37\x68\x89\x9a\xdb\x3c\x05\x06\
              \x12\x13\x44\xc5\xf6\x96\xf7\xc3\x86\x09\x0e\x7c\x85"
                .as_slice(),
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        )?;
        explode_test(
            b"\x0d\x02\x01\x12\x23\x14\x15\x36\x37\x68\x89\x9a\xdb\x3c\x05\x06\x12\x13\x44\xc5\
              \xf6\x96\xf7\xc3\x8a\x0d\x1b\x26\x38\xb8\x62\xc5\x04\x07\x57\x21\x50"
                .as_slice(),
            b"abaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbb\n",
        )?;
        explode_test(
            b"\x0d\x02\x01\x12\x23\x14\x15\x36\x37\x68\x89\x9a\xdb\x3c\x05\x06\x12\x13\x44\xc5\xf6\x96\xf7\xc3\
              \x8a\x1d\x4b\xb6\xac\x99\xe2\xd0\xac\x83\xb7\x1e\x50\xa8\xa1\x0d\xca\x46\xc1\x2a\x04".as_slice(),
            b"abcdefabcdefabcdefabcdeabcdefabcdefabcdefabcabcdefabcdefabcdefabcdeabcdefabcdefabcdefab\
              cdefabcdefabcdefabcdeabcdefabcdefabcdefabcabcdefabcdefabcdefabcdeabcdefabcdefababcdefab\
              cdefabcdefabcdeabcdefabcdefabcdefabcabcdefabcdefabcdefabcdeabcdefabcdefabcdefabcdefabcd\
              efabcdefabcdeabcdefabcdefabcdefabcabcdefabcdefabcdefabcdeabcdefabc\n"
        )?;
        Ok(())
    }

    #[test]
    fn explode_8k() -> Result<(), std::io::Error> {
        let gp_flag = 0b110;
        let mut decomp: Vec<u8> = Vec::new();
        let mut stream = ExplodeStream::new(
            [
                97, 10, 123, 7, 6, 27, 6, 187, 12, 75, 3, 9, 7, 11, 9, 11, 9, 7, 22, 7, 8, 6, 5, 6,
                7, 6, 5, 54, 7, 22, 23, 11, 10, 6, 8, 10, 11, 5, 6, 21, 4, 6, 23, 5, 10, 8, 5, 6,
                21, 6, 10, 37, 6, 8, 7, 24, 10, 7, 10, 8, 11, 7, 11, 4, 37, 4, 37, 4, 10, 6, 4, 5,
                20, 5, 9, 52, 7, 6, 23, 9, 26, 43, 252, 252, 252, 251, 251, 251, 12, 11, 44, 11,
                44, 11, 60, 11, 44, 43, 172, 12, 1, 34, 35, 20, 21, 54, 55, 104, 137, 154, 219, 60,
                5, 6, 18, 35, 20, 229, 246, 150, 247, 79, 179, 81, 191, 107, 213, 202, 21, 66, 9,
                103, 249, 216, 75, 33, 219, 214, 101, 142, 16, 31, 62, 92, 72, 115, 39, 57, 217,
                89, 153, 49, 50, 210, 163, 143, 83, 71, 195, 55, 37, 106, 148, 65, 136, 133, 217,
                79, 238, 117, 147, 66, 180, 211, 110, 37, 134, 17, 57, 244, 208, 66, 36, 98, 68,
                129, 163, 254, 130, 141, 250, 11, 68, 212, 95, 208, 71, 253, 5, 28, 212, 95, 208,
                66, 253, 5, 116, 212, 95, 80, 69, 253, 5, 36, 212, 95, 80, 136, 250, 11, 240, 80,
                127, 65, 14, 234, 47, 192, 68, 253, 5, 233, 168, 191, 0, 5, 245, 23, 36, 161, 254,
                2, 120, 212, 95, 16, 139, 250, 11, 28, 101, 213, 66, 0, 224, 226,
            ]
            .as_slice(),
            6125,
            uses_large_window(gp_flag),
            uses_lit_tree(gp_flag),
        )?;
        stream.read_to_end(&mut decomp)?;
        assert_eq!(
            &decomp[0..62],
            b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
        );
        for v in &decomp[62..6062] {
            assert_eq!(*v, b'_');
        }
        assert_eq!(
            &decomp[6062..],
            b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ\n"
        );
        Ok(())
    }
}
