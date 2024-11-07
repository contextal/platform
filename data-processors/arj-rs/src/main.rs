mod config;

use backend_utils::objects::*;
use serde::Serialize;
use std::{
    fs::File,
    io::{Error, Write},
    path::PathBuf,
};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;
use unarj_rs::{
    arj_archive::ArjArchieve, local_file_header::CompressionMethod, main_header::HostOS,
};

#[derive(Serialize)]
struct ArchiveInfo<'a> {
    name: &'a str,
    comment: Option<&'a str>,
    host_os: String,
}

#[derive(Serialize)]
struct FileEntry {
    name: String,
    compressed_size: u32,
    original_size: u32,
    original_crc32: u32,
    compression_method: String,
    arj_flags: u8,
    archiver_version_number: u8,
    min_version_to_extract: u8,
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

    info!("Parsing {}", input_name.display());

    let input_file = File::open(&input_name)?;
    let mut arj = match ArjArchieve::new(&input_file) {
        Ok(a) => a,
        Err(e) => {
            return Ok(BackendResultKind::error(format!(
                "Failed to open ARJ file: {e}"
            )));
        }
    };

    while let Ok(Some(entry)) = arj.get_next_entry() {
        if children.len() + 1 >= config.max_children as usize {
            limits_reached = true;
            break;
        }

        let mut child_symbols: Vec<String> = Vec::new();

        let path = if entry.original_size as u64 > config.max_child_output_size
            || entry.compressed_size as u64 > config.max_child_input_size
            || remaining_total_size.saturating_sub(entry.original_size as u64) == 0
        {
            limits_reached = true;
            child_symbols.push("TOOBIG".to_string());
            None
        } else if let Ok(buffer) = arj.read(&entry) {
            let mut output_file = tempfile::NamedTempFile::new_in(&config.output_path)?;
            output_file.write_all(&buffer)?;
            remaining_total_size = remaining_total_size.saturating_sub(buffer.len() as u64);
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
            child_symbols.push("CORRUPTED".to_string());
            None
        };

        let compression_method = match entry.compression_method {
            CompressionMethod::Stored => String::from("Stored"),
            CompressionMethod::CompressedMost => String::from("CompressedMost"),
            CompressionMethod::Compressed => String::from("Compressed"),
            CompressionMethod::CompressedFaster => String::from("CompressedFaster"),
            CompressionMethod::CompressedFastest => String::from("CompressedFastest"),
            CompressionMethod::NoDataNoCrc => String::from("NoDataNoCrc"),
            CompressionMethod::NoData => String::from("NoData"),
            _ => String::from("UNKNOWN"),
        };

        let file_entry = FileEntry {
            name: entry.name.clone(),
            compressed_size: entry.compressed_size,
            original_size: entry.original_size,
            original_crc32: entry.original_crc32,
            compression_method,
            arj_flags: entry.arj_flags,
            archiver_version_number: entry.archiver_version_number,
            min_version_to_extract: entry.min_version_to_extract,
        };

        children.push(BackendResultChild {
            path,
            force_type: None,
            symbols: child_symbols,
            relation_metadata: match serde_json::to_value(file_entry).unwrap() {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
        });
    }

    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    let host_os = match arj.get_host_os() {
        HostOS::MsDos => String::from("MS-DOS"),
        HostOS::PrimOS => String::from("PrimOS"),
        HostOS::Unix => String::from("UNIX"),
        HostOS::Amiga => String::from("Amiga"),
        HostOS::MacOs => String::from("MacOS"),
        HostOS::OS2 => String::from("OS/2"),
        HostOS::AppleGS => String::from("AppleGS"),
        HostOS::AtariST => String::from("AtariST"),
        HostOS::NeXT => String::from("NeXT"),
        HostOS::VaxVMS => String::from("VaxVMS"),
        HostOS::Win95 => String::from("Win95"),
        HostOS::Win32 => String::from("Win32"),
        _ => String::from("UNKNOWN"),
    };

    let comment = if arj.get_comment().is_empty() {
        None
    } else {
        Some(arj.get_comment())
    };

    let arj_info = ArchiveInfo {
        name: arj.get_name(),
        comment,
        host_os,
    };
    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(arj_info).unwrap() {
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
