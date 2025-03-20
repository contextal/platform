mod config;

use backend_utils::objects::*;
use serde::Serialize;
use std::io::{Read, Seek, Write};
use std::path::PathBuf;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

const FAT_MAGIC: u32 = 0xcafebabe;
const FAT_CIGAM: u32 = 0xbebafeca;

fn rdu32<R: Read>(r: &mut R, le: bool) -> Result<u32, std::io::Error> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    match le {
        true => Ok(u32::from_le_bytes(buf)),
        false => Ok(u32::from_be_bytes(buf)),
    }
}

/// Universal Binary header
#[derive(Serialize)]
pub struct FatHeader {
    /// Magic number
    pub magic: u32,
    /// Number of structs that follow
    pub nfat_arch: u32,
}

/// Architecture header
#[derive(Serialize, Debug)]
pub struct FatArch {
    /// CPU subtype
    pub cputype: u32,
    /// Description of CPU type (not an official field)
    pub cputypestr: &'static str,
    /// CPU subtype
    pub cpusubtype: u32,
    /// File offset to the object file
    pub offset: u32,
    /// Size of the object file
    pub size: u32,
    /// Alignment as a power of 2
    pub align: u32,
}

fn arch_cputype(cputype: u32) -> &'static str {
    match cputype {
        1 => "VAX",
        6 => "MC680x0",
        7 => "I386",
        8 => "MIPS",
        10 => "MC98000",
        11 => "HPPA",
        12 => "ARM",
        13 => "MC88000",
        14 => "SPARC",
        15 => "I860",
        16 => "ALPHA",
        18 => "POWERPC",
        0x01000012 => "POWERPC64",
        0x01000007 => "X86_64",
        0x0100000c => "ARM64",
        0x0200000c => "ARM64_32",
        0xffffffff => "ANY",
        _ => "*** UNKNOWN ***",
    }
}

impl FatArch {
    fn new<R: Read>(mut r: R, le: bool) -> Result<Self, std::io::Error> {
        let cputype = rdu32(&mut r, le)?;
        let cpusubtype = rdu32(&mut r, le)?;
        let offset = rdu32(&mut r, le)?;
        let size = rdu32(&mut r, le)?;
        let align = rdu32(&mut r, le)?;

        Ok(Self {
            cputype,
            cputypestr: arch_cputype(cputype),
            cpusubtype,
            offset,
            size,
            align,
        })
    }

    fn copy<R: Read + Seek, W: Write>(
        &self,
        from: &mut R,
        to: &mut W,
    ) -> Result<(), std::io::Error> {
        let oldpos = from.stream_position()?;
        from.seek(std::io::SeekFrom::Start(self.offset as u64))?;
        {
            let mut take = from.take(self.size as u64);
            std::io::copy(&mut take, to).map_err(|e| {
                warn!("Failed to extract Mach-O file: {}", e);
                e
            })?;
        }
        from.seek(std::io::SeekFrom::Start(oldpos))?;
        Ok(())
    }
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, std::io::Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    info!("Parsing {}", input_name.display());
    let mut r = std::fs::File::open(input_name)?;
    let mut symbols: Vec<String> = Vec::new();
    let mut children: Vec<BackendResultChild> = Vec::new();
    let mut remaining_total_size = config.max_processed_size + 1;
    let mut limits_reached = false;
    let mut le = true; // we just use the flag to determine the right conversion order, not for the actual endianness
    let magic = rdu32(&mut r, le)?;
    match magic {
        FAT_MAGIC => (),
        FAT_CIGAM => {
            le = false;
        }
        _ => {
            return Ok(BackendResultKind::error(
                "Not a Universal Binary".to_string(),
            ));
        }
    }

    let nfat_arch = rdu32(&mut r, le)?;
    for i in 0..nfat_arch {
        if i >= config.max_children {
            limits_reached = true;
            break;
        }
        let arch = FatArch::new(&mut r, le)?;
        let mut arch_symbols: Vec<String> = Vec::new();
        let path = if arch.size as u64 > config.max_child_output_size {
            debug!("Mach-O exceeds max_child_output_size, skipping");
            limits_reached = true;
            arch_symbols.push("TOOBIG".to_string());
            None
        } else if remaining_total_size.saturating_sub(arch.size.into()) == 0 {
            debug!("Mach-O exceeds max_processed_size, skipping");
            limits_reached = true;
            arch_symbols.push("TOOBIG".to_string());
            None
        } else {
            let mut output_file = tempfile::NamedTempFile::new_in(&config.output_path)?;
            arch.copy(&mut r, &mut output_file).map_err(|e| {
                warn!("Failed to extract Mach-O file: {}", e);
                e
            })?;
            remaining_total_size = remaining_total_size.saturating_sub(arch.size.into());
            Some(
                output_file
                    .into_temp_path()
                    .keep()
                    .unwrap()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
            )
        };
        debug!("Details of the extracted file: {:#?}", arch);
        children.push(BackendResultChild {
            path,
            force_type: Some("MachO".to_string()),
            symbols: arch_symbols,
            relation_metadata: match serde_json::to_value(arch).unwrap() {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
        });
    }

    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    let fat_header = FatHeader { magic, nfat_arch };
    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(fat_header).unwrap() {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        },
        children,
    }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = config::Config::new()?;
    backend_utils::work_loop!(config.host.as_deref(), config.port, |request| {
        process_request(request, &config)
    })?;
    unreachable!()
}
