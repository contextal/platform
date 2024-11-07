use ctxutils::{cmp::*, io::*, win32::GUID};
use std::fmt;
use std::io::{self, Read, Seek};
use std::str::FromStr;

/// An image of sort (bitmap, gif, jpeg, wmf, emf, icon)
pub struct Picture {
    /// The StdPicture CLSID: `{0BE35204-8F91-11CE-9DE3-00AA004BB851}`
    pub clsid: GUID,
    /// The raw image data (truncated to the first 2MiB)
    ///
    /// Note: the Debug output is truncated to the first 64 bytes
    pub picture: Vec<u8>,
    /// Non fatal anomalies encountered while processing the picture data
    pub anomalies: Vec<String>,
}

impl Picture {
    pub(crate) fn new<R: Read + Seek>(f: &mut R) -> Result<Self, io::Error> {
        let clsid = GUID::from_le_stream(f)?;
        let mut anomalies: Vec<String> = Vec::new();
        if clsid != GUID::from_str("0BE35204-8F91-11CE-9DE3-00AA004BB851").unwrap() {
            // Office fails with an unexpected GUID
            anomalies.push("GuidAndPicture contains an invalid GUID".to_string());
        }
        if rdu32le(f)? != 0x0000746c {
            // Office shows a "Picture error" message; some of the controls may still be reachable
            anomalies.push("Invalid StdPicture preamble".to_string());
        }
        let total_size: u64 = rdu32le(f)?.into();
        let read_size = umin(umin(total_size, 2 * 1024 * 1024u64), usize::MAX);
        let mut picture: Vec<u8> = vec![0u8; read_size as usize]; // Safe due to umin
        f.read_exact(&mut picture)?;
        if read_size < total_size {
            f.seek(io::SeekFrom::Current((total_size - read_size) as i64))?; // Safe due to umin
        }
        Ok(Self {
            clsid,
            picture,
            anomalies,
        })
    }
}

impl fmt::Debug for Picture {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Picture")
            .field("clsid", &self.clsid)
            .field("picture", &&self.picture[0..self.picture.len().min(64)])
            .finish()
    }
}
