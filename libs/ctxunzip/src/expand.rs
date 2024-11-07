//! Reduce decompressor
use crate::utils::CircularBuffer;
use std::collections::VecDeque;
use std::io::Read;

/// Size of the input buffer
const INBUFSIZ: usize = 8 * 1024;
/// The distance / length marker
const DLE: u8 = 0x90;

#[derive(Default)]
/// A set of bytes likely to follow a given byte
struct FollowerSet {
    size: usize,
    set: [u8; 32],
    bits_needed: u8,
}

/// Reduce decompressor
struct Expand<R: Read> {
    r: R,
    input: CircularBuffer<INBUFSIZ>,
    followers: [FollowerSet; 256],
    bits: u8,
    nbits: u8,
    last: u8,
}

impl<R: Read> Expand<R> {
    fn new(r: R) -> Result<Self, std::io::Error> {
        let mut ret = Self {
            r,
            input: CircularBuffer::new(),
            followers: std::array::from_fn(|_| FollowerSet::default()),
            bits: 0,
            nbits: 0,
            last: 0,
        };
        for i in (0usize..=255).rev() {
            let size = usize::from(ret.getbits(6)?);
            for j in 0..size {
                let follower = ret.getbits(8)?;
                ret.followers[i].set[j] = follower;
            }
            ret.followers[i].size = size;
            if size == 0 {
                ret.followers[i].bits_needed = 0;
            } else if size <= 2 {
                ret.followers[i].bits_needed = 1;
            } else if size <= 4 {
                ret.followers[i].bits_needed = 2;
            } else if size <= 8 {
                ret.followers[i].bits_needed = 3;
            } else if size <= 16 {
                ret.followers[i].bits_needed = 4;
            } else {
                ret.followers[i].bits_needed = 5;
            }
        }
        Ok(ret)
    }

    fn getbits(&mut self, n: u8) -> Result<u8, std::io::Error> {
        assert!((1..=8).contains(&n));
        Ok(if self.nbits < n {
            if self.input.is_empty() {
                std::io::copy(&mut (&mut self.r).take(INBUFSIZ as u64), &mut self.input)?;
            }
            let bits = u16::from(self.bits)
                | (u16::from(
                    self.input
                        .pop_front()
                        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))?,
                ) << self.nbits);
            let ret = bits & ((1 << n) - 1);
            self.bits = (bits >> n) as u8;
            self.nbits += 8 - n;
            ret as u8
        } else {
            let ret = self.bits & ((1 << n) - 1);
            self.bits >>= n;
            self.nbits -= n;
            ret
        })
    }

    fn getbyte(&mut self) -> Result<u8, std::io::Error> {
        let ret = if self.followers[usize::from(self.last)].size == 0 {
            self.getbits(8)?
        } else {
            let lit = self.getbits(1)?;
            if lit == 1 {
                self.getbits(8)?
            } else {
                let which = self.getbits(self.followers[usize::from(self.last)].bits_needed)?;
                self.followers[usize::from(self.last)].set[usize::from(which)]
            }
        };
        self.last = ret;
        Ok(ret)
    }
}

/// Streaming Reduce decompressor
///
/// At the time of writing Rust does not allow const expressions with const generics,
/// hence both BITS and WINSIZ are declared
pub struct ExpandStream<const BITS: usize, const WINSIZ: usize, R: Read> {
    expand: Expand<R>,
    output: VecDeque<u8>,
    window: CircularBuffer<WINSIZ>,
    todo: u64,
}

impl<const BITS: usize, const WINSIZ: usize, R: Read> ExpandStream<BITS, WINSIZ, R> {
    /// Creates a new decompressor
    pub fn new(r: R, uncompressed_size: u64) -> Result<Self, std::io::Error> {
        assert!([512usize, 1024, 2048, 4096].contains(&WINSIZ));
        assert!(512 << (BITS - 1) == WINSIZ);
        Ok(Self {
            expand: Expand::new(r)?,
            output: VecDeque::with_capacity(4 * 1024),
            window: CircularBuffer::new(),
            todo: uncompressed_size,
        })
    }
}

impl<const BITS: usize, const WINSIZ: usize, R: Read> Read for ExpandStream<BITS, WINSIZ, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        if self.output.is_empty() {
            while self.output.len() < buf.len() && self.todo > 0 {
                let c = self.expand.getbyte()?;
                if c != DLE {
                    self.output.push_back(c);
                    self.window.push_back(c);
                    self.todo -= 1;
                    continue;
                }
                let c = self.expand.getbyte()?;
                if c == 0 {
                    self.output.push_back(DLE);
                    self.window.push_back(DLE);
                    self.todo -= 1;
                    continue;
                }
                let c = usize::from(c);
                let len_mask = (1 << (8 - BITS)) - 1;
                let dist_mask = !len_mask;
                let mut len = c & len_mask;
                if len & len_mask == len_mask {
                    len += usize::from(self.expand.getbyte()?);
                }
                len += 3;
                let dist = (((c & dist_mask) << BITS) | usize::from(self.expand.getbyte()?)) + 1;
                let len = self.todo.min(len as u64);
                self.todo -= len;
                for _ in 0..len {
                    let c = if self.window.len() < dist {
                        0u8
                    } else {
                        self.window[self.window.len() - dist]
                    };
                    self.output.push_back(c);
                    self.window.push_back(c);
                }
            }
        }
        self.output.read(buf)
    }
}
