mod config;

use backend_utils::objects::*;
use cab::CompressionType;
use serde::Serialize;
use std::fs::File;
use std::io::Error;
use std::path::PathBuf;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

#[derive(Serialize)]
struct CabInfo {
    number_of_files: u32,
    number_of_folders: u32,
}

#[derive(Serialize)]
struct FileInfo<'a> {
    name: &'a str,
    uncompressed_size: u32,
    compression_type: &'a str,
    is_read_only: bool,
    is_hidden: bool,
    is_system: bool,
    is_archive: bool,
    is_exec: bool,
    is_name_utf: bool,
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    let mut children: Vec<BackendResultChild> = Vec::new();
    let mut symbols: Vec<String> = Vec::new();
    let mut remaining_total_size = config.max_processed_size + 1;
    let mut limits_reached = false;
    let cab_file = File::open(&input_name)?;
    let cab_file_reader = File::open(&input_name)?;

    info!("Parsing {}", input_name.display());

    let cab = match cab::Cabinet::new(&cab_file) {
        Ok(p) => p,
        Err(e) => {
            return Ok(BackendResultKind::error(format!(
                "Failed to open CAB file: {e}"
            )));
        }
    };
    let mut cab_reader = cab::Cabinet::new(&cab_file_reader)?;

    let mut number_of_folders = 0;
    let mut number_of_files = 0;
    for folder in cab.folder_entries() {
        number_of_folders += 1;
        for file in folder.file_entries() {
            if children.len() + 1 >= config.max_children as usize {
                limits_reached = true;
                break;
            }
            let mut child_symbols: Vec<String> = Vec::new();
            let mut path = None;
            number_of_files += 1;

            if let Ok(mut reader) = cab_reader
                .read_file(file.name())
                .map_err(|e| warn!("Failed to extract file: {}", e))
            {
                let size = file.uncompressed_size() as u64;

                path = if size > config.max_child_output_size {
                    debug!("File exceeds max_child_output_size, skipping");
                    limits_reached = true;
                    child_symbols.push("TOOBIG".to_string());
                    None
                } else if remaining_total_size.saturating_sub(size) == 0 {
                    debug!("File exceeds max_processed_size, skipping");
                    limits_reached = true;
                    child_symbols.push("TOOBIG".to_string());
                    None
                } else {
                    let mut output_file = tempfile::NamedTempFile::new_in(&config.output_path)?;
                    if std::io::copy(&mut reader, &mut output_file)
                        .map_err(|e| warn!("Failed to extract file: {}", e))
                        .is_ok()
                    {
                        remaining_total_size = remaining_total_size.saturating_sub(size);
                        Some(
                            output_file
                                .into_temp_path()
                                .keep()
                                .unwrap()
                                .into_os_string()
                                .into_string()
                                .unwrap(),
                        )
                    } else {
                        None
                    }
                }
            }

            let compression_type = match folder.compression_type() {
                CompressionType::None => "None",
                CompressionType::MsZip => "MSZIP",
                CompressionType::Quantum(_, _) => "Quantum",
                CompressionType::Lzx(_) => "LZX",
            };

            let file_info = FileInfo {
                name: file.name(),
                uncompressed_size: file.uncompressed_size(),
                compression_type,
                is_read_only: file.is_read_only(),
                is_hidden: file.is_hidden(),
                is_system: file.is_system(),
                is_archive: file.is_archive(),
                is_exec: file.is_exec(),
                is_name_utf: file.is_name_utf(),
            };

            children.push(BackendResultChild {
                path,
                force_type: None,
                symbols: child_symbols,
                relation_metadata: match serde_json::to_value(file_info).unwrap() {
                    serde_json::Value::Object(v) => v,
                    _ => unreachable!(),
                },
            });
        }
        if limits_reached {
            break;
        }
    }

    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    let cab_info = CabInfo {
        number_of_files,
        number_of_folders,
    };
    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(cab_info).unwrap() {
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

#[cfg(test)]
mod test;
