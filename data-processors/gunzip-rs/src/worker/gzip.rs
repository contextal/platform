//! Gzip decompressor
//!
//! A tiny wrapper around system zlib (through the [libz-sys](https://crates.io/crates/libz-sys) crate)
//! required due to  the shortcomings and bugs of the [flate2](https://crates.io/crates/flate2) crate
//!
//! The main interface is [`Gunzip`]
use std::io::Read;

unsafe fn make_stream(wbits: i32) -> Result<Box<libz_sys::z_stream>, std::io::Error> {
    let arr: [u8; std::mem::size_of::<libz_sys::z_stream>()] =
        [0; std::mem::size_of::<libz_sys::z_stream>()];
    let mut z = Box::new(std::mem::transmute::<[u8; 112], libz_sys::z_stream>(arr));
    // Note: z_stream contains two nullable function pointers (zalloc and zfree) which
    // rustc will not tolerate
    // Array transmutation is used here in place of MayeUninit::zeroed() so the compiler
    // won't notice

    let zres = libz_sys::inflateInit2_(
        z.as_mut(),
        wbits + 16,
        "1.2.13".as_ptr() as _,
        std::mem::size_of::<libz_sys::z_stream>() as i32,
    );
    match zres {
        libz_sys::Z_OK => {}
        libz_sys::Z_MEM_ERROR => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "Not enough memory to init zlib state",
            ));
        }
        libz_sys::Z_VERSION_ERROR => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Internal error: zlib version error",
            ));
        }
        libz_sys::Z_STREAM_ERROR => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Internal error: invalid init parameter",
            ));
        }
        v => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Internal error: unknown init result ({})", v),
            ));
        }
    }
    Ok(z)
}

unsafe fn make_header() -> Box<libz_sys::gz_header> {
    let header: libz_sys::gz_header = std::mem::MaybeUninit::zeroed().assume_init();
    Box::new(header)
}

const MAX_WBITS: i32 = 15;
const INBUFLEN: u32 = 64 * 1024;

#[derive(Debug, PartialEq, PartialOrd)]
enum Phase {
    Begin,
    Header,
    Data,
}

/// Gzip decompressor interface
///
/// A wrapper for any [`Read`](std::io::Read) type that transparently
/// decompresses Gzip data
///
/// It additionally collects headers for each Gzip member - see [`members()`](Self::members)
pub struct Gunzip<R: Read> {
    z: Box<libz_sys::z_stream>,
    r: R,
    inbuf: [u8; INBUFLEN as usize],
    phase: Phase,
    header: Box<libz_sys::gz_header>,
    headers: Vec<GzipHeader>,
    n_members: usize,
    members_max: usize,
    input_size: usize,
    output_size: usize,
    has_tail: bool,
    // Note: these are just used for heap allocation, not as actual Vec's
    extra: Vec<u8>,
    name: Vec<u8>,
    comment: Vec<u8>,
    remaining_input_size: u64,
}

impl<R: Read> Gunzip<R> {
    /// Creates a new Gzip decompressor
    /// # Arguments
    ///
    /// * `r` - Any type implementing [`Read`](std::io::Read)
    /// * `extra_max` - The maximum amount of *Extra field* data to collect for
    ///   each header (the exceeding portion won't be available)
    /// * `name_max` - The maximum amount of bytes to collect for Gzip member names
    ///   (longer names will appear truncated)
    /// * `comment_max` - The maximum amount of bytes to collect for Gzip member comments
    ///   (longer comments will appear truncated)
    /// * `members_max` - The maximum amount of member headers to collect
    pub fn new(
        r: R,
        max_input_size: u64,
        extra_max: u32,
        name_max: u32,
        comment_max: u32,
        members_max: usize,
    ) -> Result<Self, std::io::Error> {
        isize::try_from(extra_max).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Parameter extra_max is too large",
            )
        })?;
        isize::try_from(name_max).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Parameter name_max is too large",
            )
        })?;
        isize::try_from(comment_max).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Parameter comment_max is too large",
            )
        })?;
        let z = unsafe { make_stream(MAX_WBITS + 16) }?;
        let mut header = unsafe { make_header() };
        header.extra_max = extra_max;
        header.name_max = name_max;
        header.comm_max = comment_max;
        Ok(Self {
            z,
            r,
            inbuf: [0u8; INBUFLEN as usize],
            phase: Phase::Begin,
            header,
            headers: Vec::with_capacity(1),
            n_members: 0,
            members_max,
            input_size: 0,
            output_size: 0,
            has_tail: false,
            extra: Vec::with_capacity(extra_max as usize),
            name: Vec::with_capacity(name_max as usize),
            comment: Vec::with_capacity(comment_max as usize),
            remaining_input_size: max_input_size,
        })
    }

    /// Inflates stream data into the provided buffer
    pub fn inflate(&mut self, outbuf: &mut [u8]) -> Result<usize, std::io::Error> {
        if self.has_tail {
            return Ok(0);
        }
        let outbuflen: u32 = outbuf.len().min(u32::MAX as usize) as u32;
        self.z.next_out = outbuf.as_mut_ptr();
        self.z.avail_out = outbuflen;
        while self.z.avail_out > 0 {
            if self.phase == Phase::Begin {
                self.header.extra = self.extra.as_mut_ptr();
                self.header.name = self.name.as_mut_ptr();
                self.header.comment = self.comment.as_mut_ptr();
                let zres =
                    unsafe { libz_sys::inflateGetHeader(self.z.as_mut(), self.header.as_mut()) };
                if zres != libz_sys::Z_OK {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Internal error: inflateGetHeader failed ({})", zres),
                    ));
                }
            }
            if self.z.avail_in == 0 {
                self.z.next_in = self.inbuf.as_mut_ptr();
                let to_read = self.inbuf.len().min(
                    self.remaining_input_size
                        .try_into()
                        .unwrap_or(self.inbuf.len()),
                );
                if to_read == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Input size limit exceeded",
                    ));
                }
                let len = match self.r.read(&mut self.inbuf[0..to_read]) {
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                    Ok(v) => v,
                };
                self.remaining_input_size -= len as u64; // safe bc min() above
                self.input_size += len;
                if len == 0 {
                    if self.phase == Phase::Begin {
                        break;
                    }
                    // FIXME: UnexpectedEof?
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "File appears to be truncated",
                    ));
                }
                self.z.avail_in = len as u32;
            }
            let flush = if self.phase <= Phase::Header {
                libz_sys::Z_BLOCK
            } else {
                libz_sys::Z_NO_FLUSH
            };
            let zres = unsafe { libz_sys::inflate(self.z.as_mut(), flush) };
            match zres {
                libz_sys::Z_OK | libz_sys::Z_STREAM_END => {}
                libz_sys::Z_NEED_DICT => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Dictionary required",
                    ))
                }
                libz_sys::Z_DATA_ERROR => {
                    if self.phase <= Phase::Header && self.n_members > 0 {
                        self.has_tail = true;
                        return Ok((outbuflen - self.z.avail_out) as usize);
                    }
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Data error: {:?}", unsafe {
                            std::ffi::CStr::from_ptr(self.z.msg)
                        }),
                    ));
                }
                libz_sys::Z_STREAM_ERROR => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Internal error: stream error",
                    ))
                }
                libz_sys::Z_MEM_ERROR => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::OutOfMemory,
                        "Not enough memory to inflate stream",
                    ))
                }
                libz_sys::Z_BUF_ERROR => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Internal error: buffer error",
                    ))
                }
                e => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Internal error: unexpected inflate result ({})", e),
                    ))
                }
            }
            if self.phase <= Phase::Header {
                self.phase = Phase::Header;
                if self.header.done == 0 {
                    continue;
                }
                self.n_members += 1;
                if self.n_members <= self.members_max {
                    let gzheader = GzipHeader::from(&*self.header);
                    self.headers.push(gzheader);
                }
                self.phase = Phase::Data;
            }
            if zres == libz_sys::Z_STREAM_END {
                let zres = unsafe { libz_sys::inflateReset(self.z.as_mut()) };
                if zres != libz_sys::Z_OK {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Internal error: failed to reset state",
                    ));
                }
                self.phase = Phase::Begin;
            }
        }
        let len = (outbuflen - self.z.avail_out) as usize;
        Ok(len)
    }

    /// Iterator over all the headers encountered so far
    pub fn members(&self) -> &Vec<GzipHeader> {
        &self.headers
    }

    /// The total number or members encountered so far
    pub fn total_members(&self) -> usize {
        self.n_members
    }

    /// The input and output size in bytes
    pub fn get_io_size(&self) -> (usize, usize) {
        (self.input_size, self.output_size)
    }

    /// Checks if the archive has "trailing garbage"
    pub fn has_trailing_garbage(&self) -> bool {
        self.has_tail
    }
}

impl<R: Read> Drop for Gunzip<R> {
    fn drop(&mut self) {
        unsafe {
            libz_sys::inflateEnd(self.z.as_mut());
        }
    }
}

impl<R: Read> Read for Gunzip<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let ret = self.inflate(buf);
        if let Ok(len) = ret {
            self.output_size += len;
        }
        ret
    }
}

#[derive(Debug)]
/// A Gzip member header
pub struct GzipHeader {
    /// Indicates that the file is probably ASCII text
    pub text: bool,
    /// This gives the most recent modification time of the original file
    pub time: std::time::SystemTime,
    /// Extra flags
    pub extra_flags: u8,
    /// Operating System
    ///
    /// * 0 - FAT filesystem (MS-DOS, OS/2, NT/Win32)
    /// * 1 - Amiga
    /// * 2 - VMS (or OpenVMS)
    /// * 3 - Unix
    /// * 4 - VM/CMS
    /// * 5 - Atari TOS
    /// * 6 - HPFS filesystem (OS/2, NT)
    /// * 7 - Macintosh
    /// * 8 - Z-System
    /// * 9 - CP/M
    /// * 10 - TOPS-20
    /// * 11 - NTFS filesystem (NT)
    /// * 12 - QDOS
    /// * 13 - Acorn RISCOS
    /// * 255 - unknown
    pub os: u8,
    /// Extra field (`None` indicates absence)
    pub extra: Option<Vec<u8>>,
    /// Original file name (`None` indicates absence)
    pub name: Option<String>,
    /// File comment (`None` indicates absence)
    pub comment: Option<String>,
}

unsafe fn ptr_to_latin1z(ptr: *const u8, max: u32) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let mut s = String::new();
    let max = max as usize;
    for len in 0..max {
        let c = *ptr.add(len);
        if c == 0 {
            break;
        }
        s.push(c as char);
    }
    Some(s)
}

impl From<&libz_sys::gz_header> for GzipHeader {
    fn from(header: &libz_sys::gz_header) -> Self {
        let text = header.text == 1;
        let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(header.time);
        let extra_flags = header.xflags as u8;
        let os = header.os as u8;
        let extra = if header.extra.is_null() {
            None
        } else {
            Some(if header.extra_max == 0 {
                Vec::new()
            } else {
                unsafe { std::slice::from_raw_parts(header.extra, header.extra_len as usize) }
                    .to_vec()
            })
        };
        let name = unsafe { ptr_to_latin1z(header.name, header.name_max) };
        let comment = unsafe { ptr_to_latin1z(header.comment, header.comm_max) };
        Self {
            text,
            time,
            extra_flags,
            os,
            extra,
            name,
            comment,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Write;

    const GZINPUT: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x09, 0xf2, 0xa9, 0x66, 0x64, 0x02, 0x00, 0x66, 0x69, 0x72, 0x73, 0x74,
        0x2e, 0x74, 0x78, 0x74, 0x00, 0x4b, 0xcb, 0x2c, 0x2a, 0x2e, 0x51, 0xc8, 0xc9, 0xcc, 0x4b,
        0xe5, 0x02, 0x00, 0x10, 0xfc, 0xef, 0xbd, 0x0b, 0x00, 0x00, 0x00, 0x1f, 0x8b, 0x08, 0x10,
        0xff, 0xa9, 0x66, 0x64, 0x04, 0x01, 0x61, 0x20, 0x63, 0x6f, 0x6d, 0x6d, 0x65, 0x6e, 0x74,
        0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1f, 0x8b, 0x08, 0x08,
        0x10, 0xaa, 0x66, 0x64, 0x00, 0x03, 0x6c, 0x61, 0x73, 0x74, 0x2e, 0x74, 0x78, 0x74, 0x00,
        0x2b, 0x4e, 0x4d, 0xce, 0xcf, 0x4b, 0x51, 0xc8, 0xc9, 0xcc, 0x4b, 0xe5, 0x02, 0x00, 0x4d,
        0x33, 0x46, 0xad, 0x0c, 0x00, 0x00, 0x00,
    ];

    const MAXIN: u64 = 1024;

    #[test]
    fn test_gunzip() -> Result<(), std::io::Error> {
        let mut gz = Gunzip::new(GZINPUT, MAXIN, 0, 256, 6, 3)?;
        let mut out = String::new();
        gz.read_to_string(&mut out)?;
        assert_eq!(out, "first line\nsecond line\n");
        assert_eq!(gz.headers.len(), 3);

        let hdr = &gz.headers[0];
        assert_eq!(hdr.name.as_ref().unwrap(), "first.txt");
        assert!(hdr.text);
        assert_eq!(hdr.extra_flags, 2);
        assert_eq!(hdr.os, 0);
        assert_eq!(hdr.extra, None);
        assert_eq!(hdr.comment, None);

        let hdr = &gz.headers[1];
        assert_eq!(hdr.name, None);
        assert!(!hdr.text);
        assert_eq!(hdr.extra_flags, 4);
        assert_eq!(hdr.os, 1);
        assert_eq!(hdr.comment.as_ref().unwrap(), "a comm");

        let hdr = &gz.headers[2];
        assert_eq!(hdr.name.as_ref().unwrap(), "last.txt");
        assert!(!hdr.text);

        assert_eq!(gz.input_size, GZINPUT.len());
        assert_eq!(gz.output_size, 23);

        Ok(())
    }

    struct LameReader<R: Read>(R);
    impl<R: Read> Read for LameReader<R> {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
            let len = buf.len().min(1);
            self.0.read(&mut buf[0..len])
        }
    }

    struct LameWriter<W: Write>(W);
    impl<W: Write> Write for LameWriter<W> {
        fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
            let len = buf.len().min(1);
            self.0.write(&buf[0..len])
        }
        fn flush(&mut self) -> Result<(), std::io::Error> {
            Ok(())
        }
    }

    #[test]
    fn test_one_byte_r() -> Result<(), std::io::Error> {
        let mut gz = LameReader(Gunzip::new(GZINPUT, MAXIN, 0, 256, 6, 0)?);
        let mut out: Vec<u8> = Vec::new();
        std::io::copy(&mut gz, &mut out)?;
        assert_eq!(String::from_utf8(out).unwrap(), "first line\nsecond line\n");
        Ok(())
    }

    #[test]
    fn test_one_byte_w() -> Result<(), std::io::Error> {
        let mut gz = Gunzip::new(GZINPUT, MAXIN, 0, 0, 0, 0)?;
        let mut buf = Vec::<u8>::new();
        let mut out = LameWriter(&mut buf);
        std::io::copy(&mut gz, &mut out)?;
        assert_eq!(String::from_utf8(buf).unwrap(), "first line\nsecond line\n");
        Ok(())
    }

    #[test]
    fn test_one_byte_rw() -> Result<(), std::io::Error> {
        let mut gz = LameReader(Gunzip::new(GZINPUT, MAXIN, 0, 0, 0, 0)?);
        let mut buf = Vec::<u8>::new();
        let mut out = LameWriter(&mut buf);
        std::io::copy(&mut gz, &mut out)?;
        assert_eq!(String::from_utf8(buf).unwrap(), "first line\nsecond line\n");
        Ok(())
    }

    #[test]
    fn test_header_crc() -> Result<(), std::io::Error> {
        let buf: &mut [u8] = &mut [
            0x1f, 0x8b, 0x08, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x25, 0x15, 0x4b, 0x4c,
            0x04, 0x02, 0x00, 0xb9, 0x93, 0xac, 0xee, 0x05, 0x00, 0x00, 0x00,
        ];
        {
            let mut gz = Gunzip::new(&*buf, MAXIN, 0, 0, 0, 0)?;
            let mut out = String::new();
            gz.read_to_string(&mut out)?;
            assert_eq!(out, "aaaaa");
        }

        buf[9] = 0;
        {
            let mut gz = Gunzip::new(&*buf, MAXIN, 0, 0, 0, 0)?;
            assert_eq!(
                std::io::copy(&mut gz, &mut std::io::sink())
                    .expect_err("crc mismatch undetected")
                    .kind(),
                std::io::ErrorKind::InvalidData
            );
        }
        Ok(())
    }

    #[test]
    fn test_data_crc() -> Result<(), std::io::Error> {
        let mut buf: &[u8] = &[
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x4b, 0x4c, 0x04, 0x02,
            0x00, 0xba, 0x0d, 0xba, 0x0d, 0x05, 0x00, 0x00, 0x00,
        ];
        let mut gz = Gunzip::new(&mut buf, MAXIN, 0, 0, 0, 0)?;
        assert_eq!(
            std::io::copy(&mut gz, &mut std::io::sink())
                .expect_err("crc mismatch undetected")
                .kind(),
            std::io::ErrorKind::InvalidData
        );
        Ok(())
    }

    #[test]
    fn test_data_len() -> Result<(), std::io::Error> {
        let mut buf: &[u8] = &[
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x4b, 0x4c, 0x04, 0x02,
            0x00, 0xb9, 0x93, 0xac, 0xee, 0x08, 0x00, 0x00, 0x00,
        ];
        let mut gz = Gunzip::new(&mut buf, MAXIN, 0, 0, 0, 0)?;
        assert_eq!(
            std::io::copy(&mut gz, &mut std::io::sink())
                .expect_err("data len mismatch undetected")
                .kind(),
            std::io::ErrorKind::InvalidData
        );
        Ok(())
    }

    #[test]
    fn test_trailing_garbage() -> Result<(), std::io::Error> {
        let mut buf: &[u8] = &[
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x4b, 0x4c, 0x4e, 0x4c,
            0x02, 0x00, 0x91, 0x60, 0x15, 0x37, 0x04, 0x00, 0x00, 0x00, b't', b'a', b'i', b'l',
        ];
        let mut gz = Gunzip::new(&mut buf, MAXIN, 0, 0, 0, 0)?;
        std::io::copy(&mut gz, &mut std::io::sink())?;
        assert!(gz.has_trailing_garbage());
        Ok(())
    }
}
