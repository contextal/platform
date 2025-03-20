//! ClamAV (clamd) socket interface
use crate::config::ClamdServiceConfig;
use crate::{object, utils};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

const SCAN_COUNT: &str = "clam_scan_total";
const TYPEDET_COUNT: &str = "typedet_total";
const SCAN_TIME: &str = "clam_scan_time_seconds";
const TYPEDET_TIME: &str = "typedet_time_seconds";

/// A Clamd interface
#[derive(Clone)]
pub struct Clamd {
    addr: String,
    objects_path: String,
}

impl Clamd {
    /// Creates a new interface to Clamd
    pub fn new(config: &ClamdServiceConfig) -> Self {
        metrics::describe_counter!(SCAN_COUNT, "Total number of scanned objects");
        metrics::describe_histogram!(SCAN_TIME, metrics::Unit::Seconds, "Time to scan an object");
        Self {
            addr: format!("{}:{}", config.host, config.port),
            objects_path: config.objects_path.clone(),
        }
    }

    /// Retrieves Clamd scan result or results
    async fn get_result_common(
        &self,
        object_id: &str,
        allmatch: bool,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        debug!("Retrieving Clam symbols from {}...", self.addr);
        let mut stream = tokio::net::TcpStream::connect(&self.addr)
            .await
            .inspect_err(|e| warn!("Failed to connect to clamd at {}: {}", self.addr, e))?;
        let scan_cmd = if allmatch { "ALLMATCHSCAN" } else { "SCAN" };
        let obj_fname = format!("{}/{}", self.objects_path, object_id);
        let cmd = format!("z{} {}\0", scan_cmd, obj_fname);
        stream
            .write_all(cmd.as_bytes())
            .await
            .inspect_err(|e| warn!("Failed to send command to clamd at {}: {}", self.addr, e))?;
        let reply = utils::read_all(&mut stream)
            .await
            .inspect_err(|e| warn!("Failed to recv from clamd at {}: {}", self.addr, e))?;
        let reply = String::from_utf8(reply)
            .inspect_err(|e| warn!("Invalid reply from clamd at {}: {}", self.addr, e))?;
        let ret: Vec<String> = reply
            .as_str()
            .split('\0')
            .filter_map(|line| {
                let mut parts = line.split(' ');
                if parts.next_back() == Some("FOUND") {
                    parts
                        .next_back()
                        .map(|s| s.strip_suffix(".UNOFFICIAL").unwrap_or(s).to_string())
                } else {
                    None
                }
            })
            .collect();
        debug!("Clam symbols: {:?}", ret);
        Ok(ret)
    }

    /// Retrieves Clamd scan results as symbols
    pub async fn get_symbols(
        &self,
        object_id: &str,
    ) -> Result<(Vec<String>, f64), Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        let res = self.get_result_common(object_id, true).await?;
        metrics::counter!(SCAN_COUNT).increment(1);
        let scan_time = start.elapsed().as_secs_f64();
        metrics::histogram!(SCAN_TIME).record(scan_time);
        Ok((res, scan_time))
    }

    /// Retrieves a single Clamd scan result
    async fn get_symbol(
        &self,
        object_id: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        self.get_result_common(object_id, false)
            .await
            .map(|mut syms| syms.pop())
    }
}

/// A convenience `Clamd` wrapper interface that deals with object types
#[derive(Clone)]
pub struct Typedet {
    clamd: Clamd,
}

impl Typedet {
    /// Creates a new file type interface to Clamd
    pub fn new(config: &ClamdServiceConfig) -> Self {
        Self {
            clamd: Clamd::new(config),
        }
    }

    /// Updates the object type based on typedet results
    pub async fn set_ftype(
        &self,
        object: &mut object::Info,
        objects_path: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        let ftype = self
            .clamd
            .get_symbol(&object.object_id)
            .await
            .inspect_err(|e| {
                error!(
                    "Typedet failed to determine ftype for \"{}\": {}",
                    object.object_id, e
                )
            })?
            .unwrap_or_else(|| "UNKNOWN".to_string());
        debug!("Typedet for object \"{}\": {:?}", object.object_id, ftype);
        object.set_type(&ftype);
        Self::refine_object_type(object, objects_path).await?;
        metrics::counter!(TYPEDET_COUNT).increment(1);
        metrics::histogram!(TYPEDET_TIME).record(start.elapsed().as_secs_f64());
        Ok(())
    }

    async fn refine_object_type(
        object: &mut object::Info,
        objects_path: &str,
    ) -> Result<(), std::io::Error> {
        const CHUNK_SIZE: u64 = 512;
        const CHUNK_SIZE_BUF: usize = CHUNK_SIZE as usize; // safe
        const NCHUNKS: u64 = 4;
        if object.object_type != "Text" {
            return Ok(());
        }
        let obj_fname = format!("{}/{}", objects_path, object.object_id);
        let mut f = tokio::fs::File::open(&obj_fname).await.inspect_err(|e| {
            error!("Failed to open \"{}\" for text detection: {}", obj_fname, e)
        })?;
        let size = f
            .metadata()
            .await
            .inspect_err(|e| {
                error!(
                    "Failed to read metadata from \"{}\" for text detection: {}",
                    obj_fname, e
                )
            })?
            .len();
        if size == 0 {
            debug!(
                "Text type not confirmed for object \"{}\" (empty file)",
                object.object_id
            );
            object.set_type("UNKNOWN");
            return Ok(());
        }
        let buf_size = usize::try_from(size.min(CHUNK_SIZE * NCHUNKS)).unwrap(); // safe bc min
        let mut buf = vec![0u8; buf_size];
        if size <= CHUNK_SIZE * NCHUNKS {
            f.read_exact(&mut buf).await.inspect_err(|e| {
                error!(
                    "Failed to read from \"{}\" for text detection: {}",
                    obj_fname, e
                )
            })?;
        } else {
            let chunk_size = size / NCHUNKS;
            let mut cur_buf = buf.as_mut_slice();
            for i in 0u64..NCHUNKS {
                info!("Reading chunk {} @{}", i, chunk_size * i);
                f.seek(std::io::SeekFrom::Start(chunk_size * i))
                    .await
                    .inspect_err(|e| {
                        error!(
                            "Failed to seek chunk of \"{}\" for text detection: {}",
                            obj_fname, e
                        )
                    })?;
                f.read_exact(&mut cur_buf[0..CHUNK_SIZE_BUF])
                    .await
                    .inspect_err(|e| {
                        error!(
                            "Failed to read chunk from \"{}\" for text detection: {}",
                            obj_fname, e
                        )
                    })?;
                cur_buf = &mut cur_buf[CHUNK_SIZE_BUF..];
            }
        }
        if buf.starts_with(b"\xff\xfe") {
            debug!(
                "Text type confirmed for object \"{}\" (UTF-16LE bom)",
                object.object_id
            );
            return Ok(());
        }
        if buf.starts_with(b"\xfe\xff") {
            debug!(
                "Text type confirmed for object \"{}\" (UTF-16BE bom)",
                object.object_id
            );
            return Ok(());
        }
        if buf.starts_with(b"\xef\xbb\xbf") {
            debug!(
                "Text type confirmed for object \"{}\" (UTF-16BE bom)",
                object.object_id
            );
            return Ok(());
        }
        #[rustfmt::skip]
        const UTF8_LUT: [i8; 256] = [
            /* 0x00 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            /* 0x10 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            /* 0x20 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            /* 0x30 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            /* 0x40 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            /* 0x50 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            /* 0x60 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            /* 0x70 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            /* 0x80 */ -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2,
            /* 0x90 */ -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2,
            /* 0xa0 */ -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2,
            /* 0xb0 */ -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2,
            /* 0xc0 */ -1, -1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
            /* 0xd0 */ 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
            /* 0xe0 */ 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
            /* 0xf0 */ 3, 3, 3, 3, 3, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        ];
        let mut char_counts = [0u32; 256];
        let mut continuations = 0i8;
        let mut non_zero_chars = 0u32;
        let mut bad_utf8_chars = 0u32;
        for b in buf.into_iter() {
            char_counts[usize::from(b)] += 1;
            let code = UTF8_LUT[usize::from(b)];
            if continuations > 0 {
                if code != -2 {
                    bad_utf8_chars += 1;
                }
                continuations -= 1;
            } else if b == 0 {
                continue;
            } else if code < 0 {
                bad_utf8_chars += 1;
            } else {
                continuations = code;
            }
            non_zero_chars += 1;
        }
        let invalid_utf8_ratio = f64::from(bad_utf8_chars) / f64::from(non_zero_chars);
        if non_zero_chars > 0 && invalid_utf8_ratio < 0.05 {
            // Parses as UTF-8 (possibly with statistically insignificant errors)
            // U+0000 is ignored as although it's valid unicode as it doesn't convey any
            // significance to text and it's also vastly reppresented in binary files
            debug!(
                "Text type confirmed for object \"{}\" ({:.2}% invalid UTF-8)",
                object.object_id,
                invalid_utf8_ratio * 100.0
            );
            return Ok(());
        }
        let wsp_chars = char_counts[0x0a] + char_counts[32] + char_counts[0x09];
        let ctrl_chars: u32 = char_counts
            .into_iter()
            .take(32)
            .enumerate()
            .filter_map(|(i, c)| {
                if ![0, 9, 10, 11, 12, 13].contains(&i) {
                    // sum up counts for non printable chars in low ascii
                    Some(c)
                } else {
                    None
                }
            })
            .sum();
        let size = f64::from(size as u32); // Safe bc capped to 1024
        let wsp_ratio = f64::from(wsp_chars) / size;
        let ctrl_ratio = f64::from(ctrl_chars) / size;
        if wsp_ratio > 0.02 && ctrl_ratio < 0.001 {
            debug!(
                "Text type confirmed for object \"{}\" ({:.2}% whitespace, {:.2} control chars)",
                object.object_id,
                wsp_ratio * 100.0,
                ctrl_ratio * 100.0,
            );
            return Ok(());
        }
        debug!(
            "Text type not confirmed for object \"{}\" ({:.2}% invalid UTF-8, {:.2}% whitespace, {:.2} control chars)",
            object.object_id,
            invalid_utf8_ratio * 100.0,
            wsp_ratio * 100.0,
            ctrl_ratio * 100.0,
        );
        object.set_type("UNKNOWN");
        Ok(())
    }
}
