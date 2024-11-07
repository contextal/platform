//! Shrink decompressor
use crate::utils::CircularBuffer;
use std::collections::{BTreeSet, HashSet, VecDeque};
use std::io::Read;

const MIN_CODE_LEN: u8 = 9;
const MAX_CODE_LEN: u8 = 13;

/// A poison marker to invalidate the comparison with stale entries
/// without the need to undergo the more expensive free list lookup
const FREE_ENTRY: usize = usize::MAX;
/// A marker used to indicate an entry with no parent
/// These entries are static and don't undego the regular maintenance
const ROOT_ENTRY: usize = usize::MAX - 1;

/// A dynamic LZW dictionary
struct Dictionary {
    /// An array of entries indexed by code
    dict: Vec<(usize, u8)>,
    /// The same, for fast presence lookup
    tuples: HashSet<(usize, u8)>,
    /// A partial free-list of deleted entries (entries not yet allocated
    /// do NOT appear in here)
    freelist: BTreeSet<usize>,
    /// The maximum size of the dictionary
    max_len: usize,
}

impl Dictionary {
    fn new() -> Self {
        let max_len = 1 << (MIN_CODE_LEN + 1);
        let mut ret = Self {
            dict: Vec::with_capacity(max_len),
            tuples: HashSet::with_capacity(max_len),
            freelist: BTreeSet::new(),
            max_len,
        };
        for i in 0u8..=255 {
            ret.dict.push((ROOT_ENTRY, i));
        }
        ret.dict.push((ROOT_ENTRY, 0));
        ret
    }

    fn is_entry_freed(&self, code: usize) -> bool {
        self.freelist.contains(&code)
    }

    fn get(&self, code: usize) -> Result<Vec<u8>, std::io::Error> {
        let mut ret: VecDeque<u8> = VecDeque::new();
        let mut pos = code;
        loop {
            if let Some(entry) = self.dict.get(pos) {
                if self.is_entry_freed(entry.0) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "Invalid shrunk data: entry {} is freed (code: {})",
                            pos, code
                        ),
                    ));
                }
                ret.push_front(entry.1);
                if entry.0 == ROOT_ENTRY {
                    return Ok(ret.into());
                }
                pos = entry.0;
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "Invalid shrunk data: entry {} was not found (code: {})",
                        pos, code
                    ),
                ));
            }
        }
    }

    fn next_free_pos(&self) -> usize {
        self.freelist.first().copied().unwrap_or(self.dict.len())
    }

    fn push(&mut self, parent: usize, byte: u8) -> Result<(), std::io::Error> {
        let entry: (usize, u8) = (parent, byte);
        if self.tuples.contains(&entry) {
            return Ok(());
        }
        if let Some(pos) = self.freelist.pop_first() {
            self.dict[pos] = entry;
        } else {
            let pos = self.dict.len();
            if pos >= self.max_len {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid shrunk data: dictionary is full",
                ));
            }
            self.dict.push(entry);
        }
        self.tuples.insert(entry);
        Ok(())
    }

    fn bump_max_len(&mut self) {
        let new_max_len = self.max_len << 1;
        self.dict.reserve(new_max_len - self.max_len);
        self.max_len = new_max_len;
    }

    fn clear(&mut self) {
        let mut leaves: Vec<usize> = Vec::new();
        let mut parents: HashSet<usize> = HashSet::with_capacity(self.dict.len());
        for parent in self.dict.iter() {
            if parent.0 < ROOT_ENTRY {
                parents.insert(parent.0);
            }
        }
        for i in 257..self.dict.len() {
            if !self.is_entry_freed(i) && !parents.contains(&i) {
                leaves.push(i);
            }
        }
        for leave in leaves {
            let entry = &mut self.dict[leave];
            self.tuples.remove(entry);
            *entry = (FREE_ENTRY, 0);
            self.freelist.insert(leave);
        }
    }
}

/// The size of the input buffer
const INBUFSIZ: usize = 8192;

/// Streaming Shrink decompressor
pub struct UnShrinkStream<R: Read> {
    r: R,
    dict: Dictionary,
    input: CircularBuffer<INBUFSIZ>,
    output: VecDeque<u8>,
    todo: u64,
    last_pos: usize,
    bits: u16,
    nbits: u8,
    code_len: u8,
}

impl<R: Read> UnShrinkStream<R> {
    pub fn new(r: R, uncompressed_size: u64) -> Result<Self, std::io::Error> {
        let mut ret = Self {
            r,
            dict: Dictionary::new(),
            input: CircularBuffer::new(),
            output: VecDeque::with_capacity(8 * 1024),
            todo: uncompressed_size,
            last_pos: 0,
            bits: 0,
            nbits: 0,
            code_len: MIN_CODE_LEN,
        };
        if uncompressed_size > 0 {
            let last_pos = ret.getcode()?;
            if last_pos >= 256 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid shrunk data: bad first code ({})", last_pos),
                ));
            }
            let first_entry = ret.dict.get(last_pos)?;
            ret.output.push_back(first_entry[0]);
            ret.todo -= 1;
            ret.last_pos = last_pos;
        }
        Ok(ret)
    }

    fn getcode(&mut self) -> Result<usize, std::io::Error> {
        Ok(if self.nbits < self.code_len {
            let mut bits = u32::from(self.bits);
            let need = self.code_len - self.nbits;
            if need <= 8 {
                if self.input.is_empty() {
                    std::io::copy(&mut (&mut self.r).take(INBUFSIZ as u64), &mut self.input)?;
                }
                bits |= (ctxutils::io::rdu8(&mut self.input)? as u32) << self.nbits;
                self.nbits += 8;
            } else {
                if self.input.len() < 2 {
                    std::io::copy(
                        &mut (&mut self.r).take((INBUFSIZ - self.input.len()) as u64),
                        &mut self.input,
                    )?;
                }
                bits |= (ctxutils::io::rdu16le(&mut self.input)? as u32) << self.nbits;
                self.nbits += 16;
            }
            let ret = (bits & ((1 << self.code_len) - 1)) as u16;
            self.bits = (bits >> self.code_len) as u16;
            self.nbits -= self.code_len;
            usize::from(ret)
        } else {
            let ret = self.bits & ((1 << self.code_len) - 1);
            self.bits >>= self.code_len;
            self.nbits -= self.code_len;
            usize::from(ret)
        })
    }

    fn decompress(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let code = loop {
            let code = self.getcode()?;
            if code != 256 {
                break code;
            }
            let ctrl = self.getcode()?;
            if ctrl == 1 {
                self.code_len += 1;
                if self.code_len > MAX_CODE_LEN {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Invalid shrunk data: maximum code length exceeded",
                    ));
                }
                self.dict.bump_max_len();
            } else if ctrl == 2 {
                self.dict.clear();
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid shrunk data: unknown control subcode {}", ctrl),
                ));
            }
        };
        if code == self.dict.next_free_pos() {
            self.dict
                .push(self.last_pos, self.dict.get(self.last_pos)?[0])?;
        }
        let current = self.dict.get(code)?;
        self.dict.push(self.last_pos, current[0])?;
        self.last_pos = code;
        Ok(current)
    }
}

impl<R: Read> Read for UnShrinkStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        if self.output.is_empty() {
            while self.output.len() < buf.len() && self.todo > 0 {
                let chunk = self.decompress()?;
                let len = chunk.len();
                let len = len.min(usize::try_from(self.todo).unwrap_or(len));
                self.output.extend(&chunk.as_slice()[0..len]);
                self.todo -= len as u64;
            }
        }
        self.output.read(buf)
    }
}
