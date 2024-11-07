use ctxunzip::ZipRandomAccess;
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs;
use std::{
    cell::RefCell,
    fs::File,
    io::{self, BufRead, BufReader, Read, Seek},
    path,
};
use tempfile::TempDir;
use tracing::debug;
use utf8dec_rs::UTF8DecReader;

pub(crate) struct Archive<R: Read + Seek> {
    tempdir: TempDir,
    archive: ZipRandomAccess<R>,
    cache: RefCell<HashMap<String, usize>>,
}

pub struct Entry {
    file: BufReader<File>,
}

impl Read for Entry {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        self.file.read(buf)
    }
}

impl BufRead for Entry {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.file.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.file.consume(amt)
    }

    #[cfg(feature = "buf_read_has_data_left")]
    fn has_data_left(&mut self) -> io::Result<bool> {
        self.file.has_data_left()
    }

    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.file.read_until(byte, buf)
    }

    #[cfg(feature = "bufread_skip_until")]
    fn skip_until(&mut self, byte: u8) -> io::Result<usize> {
        self.file.skip_until(byte)
    }

    fn read_line(&mut self, buf: &mut String) -> io::Result<usize> {
        self.file.read_line(buf)
    }
}

impl Seek for Entry {
    fn seek(&mut self, pos: std::io::SeekFrom) -> Result<u64, io::Error> {
        self.file.seek(pos)
    }
}

impl<R: Read + Seek> Archive<R> {
    pub(crate) fn new(r: R) -> Result<Archive<R>, io::Error> {
        Ok(Archive {
            tempdir: TempDir::new()?,
            archive: ZipRandomAccess::new(r)?,
            cache: RefCell::new(HashMap::new()),
        })
    }

    fn get_cache_path(&self, nentry: usize) -> path::PathBuf {
        self.tempdir.path().join(nentry.to_string())
    }

    pub(crate) fn find_entry(&self, path: &str, convert_to_utf8: bool) -> Result<Entry, io::Error> {
        debug!("find_entry({path}, {convert_to_utf8})");
        let nentry = self.cache.borrow().get(path).copied();
        let nentry = if let Some(nentry) = nentry {
            nentry
        } else {
            let nentry = self.cache.borrow().len();
            let temp_path = self.get_cache_path(nentry);
            let mut zip_entry = self.archive.get_entry_by_name(path).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("ZIP archive does not contain entry {path}"),
                )
            })?;
            let mut file = File::create(temp_path)?;
            std::io::copy(&mut zip_entry.take_reader(1073741824u64)?, &mut file)?;
            self.cache.borrow_mut().insert(path.to_string(), nentry);
            nentry
        };

        let mut path = self.get_cache_path(nentry);
        if convert_to_utf8 {
            let mut path_converted = path.clone();
            path_converted.set_extension("converted");

            if !path_converted.exists() {
                let mut file = File::open(&path)?;
                let mut bufer = [0u8; 30];
                file.read_exact(&mut bufer)?;
                let pair = detect_codepage(&bufer);
                if let Some((codepage, offset)) = pair {
                    file.seek(io::SeekFrom::Start(offset))?;
                    let mut reader =
                        UTF8DecReader::for_label(codepage, file).ok_or(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Unable to create UTF8DecReader",
                        ))?;
                    let mut writer = File::create_new(&path_converted)?;
                    io::copy(&mut reader, &mut writer)?;
                    writer.flush()?;
                } else {
                    // File is already UTF8 or not Unicode
                    // create link to avoid future checks
                    fs::symlink(&path, &path_converted)?;
                }
            }
            path = path_converted;
        }

        Ok(Entry {
            file: BufReader::new(File::open(path)?),
        })
    }

    #[allow(dead_code)]
    pub(crate) fn contains(&self, path: &str) -> bool {
        self.archive.names().any(|f| f == path)
    }
}

const HEADER_UTF16LE: &[u8] = &[
    0x3c, 0x00, 0x3f, 0x00, 0x78, 0x00, 0x6d, 0x00, 0x6c, 0x00, 0x20, 0x00,
];
const HEADER_UTF16BE: &[u8] = &[
    0x00, 0x3c, 0x00, 0x3f, 0x00, 0x78, 0x00, 0x6d, 0x00, 0x6c, 0x00, 0x20,
];
const HEADER_UTF32LE: &[u8] = &[
    0x3c, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x00, 0x78, 0x00, 0x00, 0x00, 0x6d, 0x00, 0x00, 0x00,
    0x6c, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00,
];
const HEADER_UTF32BE: &[u8] = &[
    0x00, 0x00, 0x00, 0x3c, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x00, 0x78, 0x00, 0x00, 0x00, 0x6d,
    0x00, 0x00, 0x00, 0x6c, 0x00, 0x00, 0x00, 0x20,
];
const BOM_UTF32BE: &[u8] = &[0x00, 0x00, 0xfe, 0xff];
const BOM_UTF32LE: &[u8] = &[0xff, 0xfe, 0x00, 0x00];
const BOM_UTF16BE: &[u8] = &[0xfe, 0xff];
const BOM_UTF16LE: &[u8] = &[0xff, 0xfe];
const BOM_UTF8: &[u8] = &[0xef, 0xbb, 0xbf];

/// Detect unicode encoding other than UTF8 and number of bytes to skip (BOM size)
fn detect_codepage(data: &[u8]) -> Option<(&str, u64)> {
    // Check BOM
    if data.starts_with(BOM_UTF8) {
        return None;
    }
    if data.starts_with(BOM_UTF32BE) {
        return Some(("UTF32BE", 4));
    }
    if data.starts_with(BOM_UTF32LE) {
        return Some(("UTF32LE", 4));
    }
    if data.starts_with(BOM_UTF16BE) {
        return Some(("UTF16BE", 2));
    }
    if data.starts_with(BOM_UTF16LE) {
        return Some(("UTF16LE", 2));
    }
    // Check encoded slice
    if data.starts_with(HEADER_UTF32BE) {
        return Some(("UTF32BE", 0));
    }
    if data.starts_with(HEADER_UTF32LE) {
        return Some(("UTF32LE", 0));
    }
    if data.starts_with(HEADER_UTF16BE) {
        return Some(("UTF16BE", 0));
    }
    if data.starts_with(HEADER_UTF16LE) {
        return Some(("UTF16LE", 0));
    }
    None
}
