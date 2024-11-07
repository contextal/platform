//! Tar backend
mod config;

use backend_utils::objects::*;
use serde::Serialize;
use std::path::PathBuf;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

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

#[derive(Default, Serialize)]
struct TarMetadata {
    has_devs: bool,
    has_links: bool,
    is_gnu: bool,
    is_ustar: bool,
}

#[derive(Debug, Serialize)]
struct TarEntry {
    name: String,
    mode: Option<String>,
    uid: Option<u64>,
    gid: Option<u64>,
    user: Option<String>,
    group: Option<String>,
    time: Option<u64>,
}

/// Parse the tarball
///
/// # Returns #
/// * Err for transient retriable errors (I/O, memory allocation, etc)
/// * Ok(BackendResultKind::ok) for success
/// * Ok(BackendReusultKind::error) for permanent errors (e.g. invalid file)
#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, std::io::Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    info!("Parsing {}", input_name.display());
    let input_file = std::fs::File::open(input_name)?;
    let mut tarball = tar::Archive::new(input_file);
    tarball.set_ignore_zeros(true);
    let mut symbols: Vec<String> = Vec::new();
    let mut metadata = TarMetadata::default();
    let mut children: Vec<BackendResultChild> = Vec::new();
    let mut limits_reached = false;
    let mut remaining_total_size = config.max_processed_size + 1;

    // Message parts processing
    let mut ord = 0u64;
    let mut is_valid_tar = false;
    // Note: note entries() and entries_with_seek() would only fail if the
    // tarball is not at the beginning, therfore unwrap is safe
    for maybe_entry in tarball.entries_with_seek().unwrap() {
        if ord >= config.max_children.into() {
            limits_reached = true;
            break;
        }
        let mut entry = match maybe_entry {
            Ok(e) => e,
            Err(e) => {
                warn!("Error grabbing entry {}: {}", ord, e);
                continue;
            }
        };
        is_valid_tar = true;
        let header = entry.header();
        metadata.is_gnu |= header.as_gnu().is_some();
        metadata.is_ustar |= header.as_ustar().is_some();
        let entry_type = header.entry_type();
        match entry_type {
            tar::EntryType::Regular | tar::EntryType::Continuous | tar::EntryType::GNUSparse => {}
            tar::EntryType::Block | tar::EntryType::Char => {
                metadata.has_devs = true;
                debug!("Skipping type {:?}", entry_type);
                continue;
            }
            tar::EntryType::Link | tar::EntryType::Symlink => {
                metadata.has_links = true;
                debug!("Skipping type {:?}", entry_type);
                continue;
            }
            _ => {
                debug!("Skipping type {:?}", entry_type);
                continue;
            }
        }
        ord += 1;
        let header = entry.header();
        let ent = TarEntry {
            name: to_string_lossy(&entry.path_bytes()),
            mode: header.mode().ok().map(|m| format!("{:o}", m)),
            uid: header.uid().ok(),
            gid: header.gid().ok(),
            user: header.username_bytes().map(to_string_lossy),
            group: header.groupname_bytes().map(to_string_lossy),
            time: header.mtime().ok(),
        };
        let mut entry_symbols: Vec<String> = Vec::new();
        let path = if entry.size() > config.max_child_output_size {
            debug!("Part exceeds max_child_output_size, skipping");
            limits_reached = true;
            entry_symbols.push("TOOBIG".to_string());
            None
        } else if remaining_total_size.saturating_sub(entry.size()) == 0 {
            debug!("Part exceeds max_processed_size, skipping");
            limits_reached = true;
            entry_symbols.push("TOOBIG".to_string());
            None
        } else {
            let mut output_file = tempfile::NamedTempFile::new_in(&config.output_path)?;
            std::io::copy(&mut entry, &mut output_file).map_err(|e| {
                warn!("Failed to copy entry {}: {}", ord, e);
                e
            })?;
            remaining_total_size = remaining_total_size.saturating_sub(entry.size());
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
        debug!("Details of the extracted file: {:#?}", ent);
        children.push(BackendResultChild {
            path,
            force_type: None,
            symbols: entry_symbols,
            relation_metadata: match serde_json::to_value(ent).unwrap() {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
        });
    }

    // Global features
    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }
    Ok(if !is_valid_tar {
        BackendResultKind::error("Invalid TAR file".to_string())
    } else {
        BackendResultKind::ok(BackendResultOk {
            symbols,
            object_metadata: match serde_json::to_value(metadata).unwrap() {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
            children,
        })
    })
}

fn to_string_lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}
