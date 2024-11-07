//! Lzma decompression
mod ffi;

use std::io::Read;

const BUFSIZ: usize = 32 * 1024;

/// LZMA1 and LZMA2/XZ streaming decompressor
pub struct LzmaStream<R: Read> {
    r: R,
    stream: ffi::lzma_stream,
    buffer: [u8; BUFSIZ],
    pos: usize,
    len: usize,
}

impl<R: Read> LzmaStream<R> {
    /// LZMA1 decoder (Zip variant only)
    pub fn new_lzma1(
        mut r: R,
        uncompressed_size: u64,
        has_term: bool,
    ) -> Result<Self, std::io::Error> {
        // The canonical LZMA1 header consists of:
        // - packed properties (1 byte for lc/lp/pb, 4 bytes for dict_size)
        // - the uncompressed length (8 bytes - u64_le)
        //
        // The zip LZMA1 header consists of:
        // - the version of the lzma library which generated the stream
        // - packed properties length (2 bytes) - this is always 5
        // - packed properties (1 byte for lc/lp/pb, 4 bytes for dict_size)
        let _version = ctxutils::io::rdu16le(&mut r)?;
        let props_len = ctxutils::io::rdu16le(&mut r)?;
        let mut packed_props: Vec<u8> = Vec::with_capacity(props_len.into());
        if (&mut r)
            .take(props_len.into())
            .read_to_end(&mut packed_props)?
            != usize::from(props_len)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Incomplete LZMA1 properties",
            ));
        }
        let mut lzma1_filter = ffi::lzma_filter {
            id: ffi::LZMA_FILTER_LZMA1EXT,
            options: std::ptr::null_mut(),
        };
        let res = unsafe {
            ffi::lzma_properties_decode(
                &mut lzma1_filter,
                std::ptr::null(),
                packed_props.as_slice().as_ptr(),
                props_len.into(),
            )
        };
        if res != ffi::LZMA_OK {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to decode LZMA1 properties (LZMA error: {})", res),
            ));
        }
        let options = unsafe { &mut *(lzma1_filter.options as *mut ffi::lzma_options_lzma) };
        if has_term {
            options.ext_flags = ffi::LZMA_LZMA1EXT_ALLOW_EOPM;
        }
        options.ext_size_low = (uncompressed_size & 0xffffffff) as u32;
        options.ext_size_high = (uncompressed_size >> 32) as u32;
        let term = ffi::lzma_filter {
            id: ffi::LZMA_VLI_UNKNOWN,
            options: std::ptr::null_mut(),
        };
        let mut filters = [lzma1_filter, term];
        let mut stream = Self::new_ffi_stream();
        let res = unsafe { ffi::lzma_raw_decoder(&mut stream, filters.as_slice().as_ptr()) };
        unsafe { ffi::lzma_filters_free(filters.as_mut_slice().as_mut_ptr(), std::ptr::null()) };
        if res != ffi::LZMA_OK {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to init LZMA1 stream decoder (LZMA error: {})", res),
            ));
        }
        Ok(Self {
            r,
            stream,
            buffer: [0u8; BUFSIZ],
            pos: 0,
            len: 0,
        })
    }

    /// XZ (LZMA2) decoder (canonical and Zip)
    pub fn new_xz(r: R) -> Result<Self, std::io::Error> {
        let mut stream = Self::new_ffi_stream();
        let res =
            unsafe { ffi::lzma_stream_decoder(&mut stream, ffi::UINT64_MAX, ffi::LZMA_FAIL_FAST) };
        if res != ffi::LZMA_OK {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to init XZ stream decoder (LZMA error: {})", res),
            ));
        }
        Ok(Self {
            r,
            stream,
            buffer: [0u8; BUFSIZ],
            pos: 0,
            len: 0,
        })
    }

    /// LZMA1/LZMA2/XZ/LZIP decoder (with automatic detection)
    pub fn new_auto(r: R) -> Result<Self, std::io::Error> {
        let mut stream = Self::new_ffi_stream();
        let res =
            unsafe { ffi::lzma_auto_decoder(&mut stream, ffi::UINT64_MAX, ffi::LZMA_FAIL_FAST) };
        if res != ffi::LZMA_OK {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Failed to init LZMA1/XZ stream decoder (LZMA error: {})",
                    res
                ),
            ));
        }
        Ok(Self {
            r,
            stream,
            buffer: [0u8; BUFSIZ],
            pos: 0,
            len: 0,
        })
    }

    /// Explicit version of STREAM_INIT
    fn new_ffi_stream() -> ffi::lzma_stream {
        let mut stream: ffi::lzma_stream = unsafe { std::mem::zeroed() };
        stream.next_in = std::ptr::null();
        stream.avail_in = 0;
        stream.total_in = 0;
        stream.next_out = std::ptr::null_mut();
        stream.avail_out = 0;
        stream.total_out = 0;
        stream.allocator = std::ptr::null();
        stream
    }
}

impl<R: Read> Drop for LzmaStream<R> {
    fn drop(&mut self) {
        unsafe { ffi::lzma_end(&mut self.stream) }
    }
}

impl<R: Read> Read for LzmaStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        if self.len == 0 {
            self.pos = 0;
            self.len = self.r.read(&mut self.buffer)?;
        }
        self.stream.next_in = self.buffer[self.pos..(self.pos + self.len)].as_ptr();
        self.stream.avail_in = self.len;
        self.stream.next_out = buf.as_mut_ptr();
        self.stream.avail_out = buf.len();
        let res = unsafe { ffi::lzma_code(&mut self.stream, ffi::LZMA_RUN) };
        self.pos += self.len - self.stream.avail_in;
        self.len = self.stream.avail_in;
        let got = buf.len() - self.stream.avail_out;
        match res {
            ffi::LZMA_OK | ffi::LZMA_STREAM_END => Ok(got),
            ffi::LZMA_MEM_ERROR => Err(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "Out of memory decoding the LZMA stream",
            )),
            ffi::LZMA_FORMAT_ERROR | ffi::LZMA_DATA_ERROR | ffi::LZMA_BUF_ERROR => {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Error decoding the LZMA data: the stream is probably corrupted",
                ))
            }
            error => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Unknown result ({}) received when decoding the LZMA stream",
                    error
                ),
            )),
        }
    }
}

pub fn uses_term(gp_flag: u16) -> bool {
    gp_flag & (1 << 1) != 0
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn lzma1() -> Result<(), std::io::Error> {
        let mut r = b"\x10\x04\x05\x00\x5d\x00\x00\x04\x00\x00\x30\x90\xc8\x24\x2e\x7c\x24\x5f\xff\xfb\x58\x40\x00".as_slice();
        let mut lzma = LzmaStream::new_lzma1(&mut r, 4, true)?;
        let mut out: Vec<u8> = Vec::new();
        std::io::copy(&mut lzma, &mut out)?;
        assert_eq!(out, b"aCaB");
        Ok(())
    }
}
