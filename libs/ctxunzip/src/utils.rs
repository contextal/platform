//! Shared utility code

use std::io::{Read, Write};
use std::ops::Index;

// WARNING: you might be tempted to #[inline] random things in here
// We've been there before. Before you touch anything DO TEST the
// performance benefits of your changes.

/// A circular buffer backed by an array whose size is exactly `SIZE` bytes
///
/// Writes in excess will roll over existing data
///
/// This is primarily intended for sliding windows, however, since it's slightly
/// faster than `VecDeque`, it can also be emplyed as an input buffer.
/// In this case, care must be taken in order to avoid overlapping writes
pub struct CircularBuffer<const SIZE: usize> {
    /// Data buffer
    data: [u8; SIZE],
    /// Start offset
    start: usize,
    /// Buffer size
    size: usize,
}

impl<const SIZE: usize> CircularBuffer<SIZE> {
    /// Creates a new CircularBuffer
    pub fn new() -> Self {
        Self {
            data: [0u8; SIZE],
            start: 0,
            size: 0,
        }
    }

    /// Pops the first byte from the buffer
    pub fn pop_front(&mut self) -> Option<u8> {
        if self.is_empty() {
            None
        } else {
            let ret = self.data[self.start];
            self.start = (self.start + 1) % SIZE;
            self.size -= 1;
            Some(ret)
        }
    }

    /// Appends one byte to the buffer
    pub fn push_back(&mut self, b: u8) {
        if self.is_empty() {
            // Empty
            self.start = 0;
            self.data[0] = b;
            self.size = 1;
        } else if self.size < SIZE {
            // Have room
            let end = (self.start + self.size) % SIZE;
            self.data[end] = b;
            self.size += 1;
        } else {
            // Already full
            self.data[self.start] = b;
            self.start = (self.start + 1) % SIZE;
        }
    }

    /// Returns whether the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Returns the number of bytes in the buffer
    pub fn len(&self) -> usize {
        self.size
    }

    /// Discards the indicated number of bytes from the beginning of the buffer
    pub fn discard(&mut self, n: usize) {
        let n = n.min(self.size);
        self.start = (self.start + n) % SIZE;
        self.size -= n;
    }
}

impl<const SIZE: usize> Index<usize> for CircularBuffer<SIZE> {
    type Output = u8;
    fn index(&self, index: usize) -> &Self::Output {
        &self.data[(self.start + index) % SIZE]
    }
}

impl<const SIZE: usize> Read for CircularBuffer<SIZE> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        // This only reads from the non wrapping chunk
        // So the len is the least of:
        let len = self
            .size // the current size
            .min(SIZE - self.start) // the distance before wrapping around
            .min(buf.len()); // the output buffer length
        buf[0..len].copy_from_slice(&self.data[self.start..(self.start + len)]);
        // Advance start (possibly wrapping it around)
        self.start = (self.start + len) % SIZE;
        // Reduce size
        self.size -= len;
        Ok(len)
    }
}

impl<const SIZE: usize> Write for CircularBuffer<SIZE> {
    fn write(&mut self, src: &[u8]) -> Result<usize, std::io::Error> {
        // This writes at most SIZE bytes
        // The excess data is is ignored and only the tail is actually written
        let src = if src.len() > SIZE {
            // Since we're writing the whole buffer we can just
            // start from the beginning
            self.start = 0;
            self.size = 0;
            &src[(src.len() - SIZE)..src.len()]
        } else {
            src
        };
        // Find the current end (including wraparound
        let end = (self.start + self.size) % SIZE;
        // Slice 1 is from end to end of buffer
        let len_chunk1 = (SIZE - end).min(src.len());
        self.data[end..(end + len_chunk1)].copy_from_slice(&src[0..len_chunk1]);
        // Slice 2 is from 0 to end
        // Since src is already restricted aboved, it's just the src tail
        let len_chunk2 = src.len() - len_chunk1;
        self.data[0..len_chunk2].copy_from_slice(&src[len_chunk1..(len_chunk1 + len_chunk2)]);
        self.size += src.len();
        // In case of wrap around it means we have overwritten the start of the buffer
        // So the size is capped and the start is updated
        if self.size >= SIZE {
            self.start = (self.start + self.size) % SIZE;
            self.size = SIZE;
        }
        Ok(src.len())
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

/// Support struct to write to two outputs at once
pub struct TwinWriter<A: Write, B: Write> {
    pub a: A,
    pub b: B,
}

impl<A: Write, B: Write> Write for TwinWriter<A, B> {
    fn write(&mut self, src: &[u8]) -> Result<usize, std::io::Error> {
        let n = self.a.write(src)?;
        self.b.write_all(&src[0..n])?;
        Ok(n)
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        self.a.flush()?;
        self.b.flush()
    }
}
