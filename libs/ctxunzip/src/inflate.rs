//! Deflate decompressor (via zlib)
use std::io::Read;

unsafe fn make_zstream() -> Result<Box<libz_sys::z_stream>, std::io::Error> {
    let arr: [u8; std::mem::size_of::<libz_sys::z_stream>()] =
        [0; std::mem::size_of::<libz_sys::z_stream>()];
    let mut z: Box<libz_sys::z_stream> = Box::new(std::mem::transmute(arr));
    // Note: z_stream contains two nullable function pointers (zalloc and zfree) which
    // rustc will not tolerate
    // Array transmutation is used here in place of MayeUninit::zeroed() so the compiler
    // won't notice

    let zres = libz_sys::inflateInit2_(
        z.as_mut(),
        -15,
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

/// Input buffer size
const BUFSIZ: usize = 8 * 1024;

/// DEFLATE streaming decompressor
///
/// Use the `Read` trait
pub struct InflateStream<R: Read> {
    /// The wrapped `Read`er
    r: R,
    /// The z stream
    z: Box<libz_sys::z_stream>,
    /// Input buffer
    input: [u8; BUFSIZ],
    /// EOF flag
    eof: bool,
}

impl<R: Read> InflateStream<R> {
    /// Creates the decompressor
    pub fn new(r: R) -> Result<Self, std::io::Error> {
        Ok(Self {
            r,
            input: [0u8; BUFSIZ],
            z: unsafe { make_zstream()? },
            eof: false,
        })
    }

    /// Inflates stream data into the provided buffer
    pub fn inflate(&mut self, outbuf: &mut [u8]) -> Result<usize, std::io::Error> {
        if self.eof {
            return Ok(0);
        }
        let outbuflen: u32 = outbuf.len().min(u32::MAX as usize) as u32;
        self.z.next_out = outbuf.as_mut_ptr();
        self.z.avail_out = outbuflen;
        while self.z.avail_out > 0 {
            if self.z.avail_in == 0 {
                self.z.next_in = self.input.as_mut_ptr();
                let len = match self.r.read(&mut self.input) {
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                    Ok(v) => v,
                };
                if len == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Truncated Deflate stream",
                    ));
                }
                self.z.avail_in = len as u32;
            }
            let zres = unsafe { libz_sys::inflate(self.z.as_mut(), libz_sys::Z_NO_FLUSH) };
            match zres {
                libz_sys::Z_OK | libz_sys::Z_STREAM_END => {}
                libz_sys::Z_NEED_DICT => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Deflate stream requires dictionary",
                    ))
                }
                libz_sys::Z_DATA_ERROR => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Zlib data error: {:?}", unsafe {
                            std::ffi::CStr::from_ptr(self.z.msg)
                        }),
                    ));
                }
                libz_sys::Z_STREAM_ERROR => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Zlib internal error: Deflate stream error",
                    ))
                }
                libz_sys::Z_MEM_ERROR => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::OutOfMemory,
                        "Not enough memory to inflate Deflate stream",
                    ))
                }
                libz_sys::Z_BUF_ERROR => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Zlib internal error: buffer error when inflating the stream",
                    ))
                }
                e => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Zlib internal error: unexpected inflate result ({})", e),
                    ))
                }
            }
            if zres == libz_sys::Z_STREAM_END {
                let zres = unsafe { libz_sys::inflateReset(self.z.as_mut()) };
                if zres != libz_sys::Z_OK {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Zlib internal error: failed to reset state",
                    ));
                }
                self.eof = true;
                break;
            }
        }
        let len = (outbuflen - self.z.avail_out) as usize;
        Ok(len)
    }
}

impl<R: Read> Read for InflateStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        self.inflate(buf)
    }
}
