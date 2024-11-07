mod config;

use aho_corasick::AhoCorasick;
use backend_utils::objects::*;
use elf_rs::ELF;
use serde::Serialize;
use std::io::{Read, Seek};
use std::path::PathBuf;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

#[derive(Serialize)]
struct SXFInfo {
    /// Size of the executable stub
    pub stub_size: usize,
}

fn process_sfx<R: Read + Seek>(
    input_file: &mut R,
    config: &config::Config,
) -> Result<Vec<BackendResultChild>, std::io::Error> {
    let mut children: Vec<BackendResultChild> = Vec::new();
    let sigs: Vec<&[u8]> = vec![
        b"Rar!\x1a\x07",       // Rar
        b"PK\x03\x04",         // Zip
        b"7z\xbc\xaf\x27\x1c", // 7-Zip
    ];
    let ac = AhoCorasick::new(sigs).unwrap();
    input_file.seek(std::io::SeekFrom::Start(0))?;

    let mut f = input_file.take(524288);
    let mut data: Vec<u8> = Vec::new();
    f.read_to_end(&mut data)?;

    if let Some(arch) = ac.find_iter(&data).next() {
        input_file.seek(std::io::SeekFrom::Start(arch.start() as u64))?;
        let mut output_file = tempfile::NamedTempFile::new_in(&config.output_path)?;
        std::io::copy(input_file, &mut output_file).map_err(|e| {
            warn!("Failed to extract embedded archive: {}", e);
            e
        })?;

        let sfx_info = SXFInfo {
            stub_size: arch.start(),
        };

        children.push(BackendResultChild {
            path: Some(
                output_file
                    .into_temp_path()
                    .keep()
                    .unwrap()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
            ),
            force_type: match arch.pattern().as_u32() {
                0 => Some("Rar".to_string()),
                1 => Some("Zip".to_string()),
                2 => Some("7Z".to_string()),
                _ => None,
            },
            symbols: vec!["SFX".to_string()],
            relation_metadata: match serde_json::to_value(sfx_info).unwrap() {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
        });
    }

    Ok(children)
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
    let mut input_file = std::fs::File::open(input_name)?;
    match ELF::new(&input_file) {
        Ok(p) => {
            let children: Vec<BackendResultChild> = process_sfx(&mut input_file, config)?;
            Ok(BackendResultKind::ok(BackendResultOk {
                symbols: match p.issues.is_empty() {
                    true => vec![],
                    false => vec!["ISSUES".to_string()],
                },
                object_metadata: match serde_json::to_value(p).unwrap() {
                    serde_json::Value::Object(v) => v,
                    _ => unreachable!(),
                },
                children,
            }))
        }
        Err(e) => Ok(BackendResultKind::error(format!(
            "Error parsing ELF file: {}",
            e
        ))),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    if std::env::args().len() == 1 {
        let config = config::Config::new()?;
        backend_utils::work_loop!(config.host.as_deref(), config.port, |request| {
            process_request(request, &config)
        })?;
        unreachable!()
    }

    for arg in std::env::args().skip(1) {
        let f = match std::fs::File::open(&arg) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("{}: ERROR: Can't open file: {}", arg, e);
                continue;
            }
        };
        let elf = match ELF::new(f) {
            Ok(elf) => elf,
            Err(err) => {
                eprintln!("{}: ERROR: {}", arg, err);
                continue;
            }
        };

        let eh = &elf.elf_header;
        info!("--- ELF HEADER ---");
        info!("Class: {}", eh.ei_class);
        info!("Endianness: {}", eh.ei_data);
        info!("OS ABI: {}", eh.ei_osabi);
        info!("Type: {}", eh.e_typestr);
        info!("Machine: {}", eh.e_machinestr);
        info!("Entry point: {:#x}", eh.e_entry);
        info!("PH offset: {}", eh.e_phoff);
        info!("SH offset: {}", eh.e_shoff);
        info!("PH entries: {}", eh.e_phnum);
        info!("SH entries: {}", eh.e_shnum);
        info!("SH size: {}", eh.e_shentsize);

        for (phnum, ph) in elf.program_headers.iter().enumerate() {
            info!("--- PROGRAM HEADER #{} ---", phnum);
            info!("Type: {}", ph.p_typestr);
            info!("Offset: {:#x}", ph.p_offset);
            info!("VirtAddr: {:#x}", ph.p_vaddr);
            info!("PhysAddr: {:#x}", ph.p_paddr);
            info!("FileSiz: {:#x}", ph.p_filesz);
            info!("MemSiz: {:#x}", ph.p_memsz);
            info!("Flags: {:?}", ph.p_flagsvec);
            info!("Align: {:#x}", ph.p_align);
        }

        for (shnum, sh) in elf.section_headers.iter().enumerate() {
            info!("--- SECTION HEADER #{} ---", shnum);
            info!("Name: {}", sh.sh_namestr);
            info!("Type: {}", sh.sh_typestr);
            info!("Flags: {:?}", sh.sh_flagsvec);
            info!("Addr: {:#x}", sh.sh_addr);
            info!("Offset: {:#x}", sh.sh_offset);
            info!("Size: {}", sh.sh_size);
            info!("Align: {:#x}", sh.sh_addralign);
        }

        if elf.issues.is_empty() {
            println!("{}: OK", arg);
        } else {
            println!("{}: ISSUES: {:?}", arg, elf.issues);
        }
    }
    Ok(())
}
