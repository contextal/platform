mod config;

use aho_corasick::AhoCorasick;
use backend_utils::objects::*;
use pe_rs::PE;
use serde::Serialize;
use std::{
    fs::File,
    io::{Read, Seek},
    path::PathBuf,
};
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
    let mut input_file = File::open(input_name)?;
    match PE::new(&input_file) {
        Ok(p) => {
            let children = process_sfx(&mut input_file, config)?;

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
            "Error parsing PE file: {}",
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
        let f = match File::open(&arg) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("{}: ERROR: Can't open file: {}", arg, e);
                continue;
            }
        };
        let pe = match PE::new(f) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{}: ERROR: {}", arg, e);
                continue;
            }
        };

        let ph = &pe.pe_header;
        info!("--- PE FILE HEADER ---");
        info!("Machine: {:#x} ({})", ph.Machine, ph.MachineStr);
        info!("NumberOfSections: {}", ph.NumberOfSections);
        info!("TimeDateStamp: {}", ph.TimeDateStampString);
        info!("PointerToSymbolTable: {:#x}", ph.PointerToSymbolTable);
        info!("NumberOfSymbols: {}", ph.NumberOfSymbols);
        info!("SizeOfOptionalHeader: {}", ph.SizeOfOptionalHeader);
        info!("Characteristics: {:?}", ph.CharacteristicsSymbols);

        let oh = &pe.optional_header;
        info!("--- IMAGE OPTIONAL HEADER ---");
        info!("Magic: {:#x} ({})", oh.Magic, oh.MagicStr);
        info!("Subsystem: {}", oh.SubsystemStr);
        info!("SizeOfHeaders: {:#x}", oh.SizeOfHeaders);
        info!("NumberOfRvaAndSizes: {}", oh.NumberOfRvaAndSizes);

        let mut snum = 1;
        for sec in &pe.section_headers {
            info!("--- SECTION {} HEADER ---", snum);
            info!("Name: {}", sec.Name);
            info!("VirtualSize: {:#x}", sec.VirtualSize);
            info!("VirtualAddress: {:#x}", sec.VirtualAddress);
            info!("SizeOfRawData: {}", sec.SizeOfRawData);
            info!("PointerToRawData: {}", sec.PointerToRawData);
            info!("Characteristics: {:?}", sec.CharacteristicsSymbols);
            snum += 1;
        }

        if pe.issues.is_empty() {
            println!("{}: OK", arg);
        } else {
            println!("{}: ISSUES: {:?}", arg, pe.issues);
        }
    }
    Ok(())
}
