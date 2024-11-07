//! Unzip backend
use backend_utils::objects::*;
use ctxunzip::{FileType, Zip};
use scopeguard::ScopeGuard;
use serde::Serialize;
use std::{fs, path::PathBuf};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn};
use tracing_subscriber::prelude::*;

mod config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config = config::Config::new()?;
    backend_utils::work_loop!(Some(&config.host), Some(config.port), |request| {
        process_request(request, &config)
    })?;
    unreachable!()
}

/// A structure to hold metadata attributes of a Zip archive.
#[derive(Default, Serialize)]
struct ZipMetadata {
    /// Number of entries in the archive according to Zip archive headers.
    num_entries: u64,
    /// Archive comment (entries in the archive could have their own per-entry comments).
    archive_comment: String,
    /// True if archive have encrypted entries, false otherwise.
    has_encrypted_entries: bool,
    /// Whether the archive is Zip64
    zip64: bool,
}

/// A structure to represent properties and attributes of a single Zip archive entry.
#[derive(Debug, Serialize, Default)]
struct ZipEntry {
    /// Entry file name.
    name: String,
    /// True if apparent file type flag corresponds to "text" value. False otherwise.
    is_text: bool,
    /// Optional Unix timestamp representing entry's modification time. Could be None if original
    /// data/time could not be parsed to valid time.
    timestamp: Option<i64>,
    /// True if the entry is encrypted. False otherwise.
    encrypted: bool,
    /// Encryption type
    encryption_type: Option<&'static str>,
    /// A string containing entry's comment (if any).
    file_comment: String,
    /// True if file names in local and central headers don't match to one another.
    names_mismatch: bool,
    /// Expected CRC-32 value, as provided in archive's entry header.
    crc32: u32,
    /// A value which represents a compression method.
    compression_method: u16,
    /// Entry's compressed size as specified in archive entry headers.
    compressed_size: u64,
    /// Entry's uncompressed size as specified in archive entry headers.
    uncompressed_size: u64,
    /// The compression factor
    compression_factor: Option<f64>,
    /// A value which represents Unix file attributes.
    unix_file_attributes: u16,
    /// A value which represents MS-DOS file attributes.
    msdos_file_attributes: u8,
    /// A value which represents operating system or file system which/where the original archive
    /// has been created.
    origin_os_or_fs: u8,
    /// True if local and central headers of the entry don't match to one another. See comparison
    /// implementation to find which fields matter and which don't.
    local_and_central_headers_match: bool,
    /// The value of the "guessed" password, if entry is encrypted and we managed to
    /// guess/bruteforce the password. None otherwise.
    guessed_password: Option<String>,
    /// Critical error encountered when parsing this entry
    error: Option<String>,
}

/// Parse a Zip object and its entries.
///
/// # Returns #
/// * Err for transient retriable errors (I/O, memory allocation, etc)
/// * Ok(BackendResultKind::ok) for success
/// * Ok(BackendResultKind::error) for permanent errors (e.g. invalid zip file)
#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, Box<dyn std::error::Error>> {
    let mut possible_passwords: Vec<&str> = Vec::new();
    if let Some(serde_json::Value::Object(glob)) = request.relation_metadata.get("_global") {
        if let Some(serde_json::Value::Array(passwords)) = glob.get("possible_passwords") {
            for pwd in passwords {
                if let serde_json::Value::String(s) = pwd {
                    possible_passwords.push(s);
                }
            }
        }
    }
    let input_path = PathBuf::from(&config.objects_path).join(&request.object.object_id);
    let input_file = fs::File::open(&input_path)
        .map_err(|e| format!("failed to open a {input_path:?} for processing: {e}"))?;
    let input_path = input_path.display();

    info!("Parsing {input_path}");
    let zip = match Zip::new(input_file) {
        Ok(v) => v,
        Err(e) => match e.kind() {
            std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
                return Ok(BackendResultKind::error(format!("invalid Zip file: {e}")))
            }
            _ => return Err(e.into()),
        },
    };

    let archive_comment = to_string_lossy(zip.comment());
    let num_entries = zip.get_entries_this_disk();

    let mut files_for_cleanup = scopeguard::guard(vec![], |files| {
        files.into_iter().for_each(|file| {
            let _ = fs::remove_file(file);
        })
    });

    let mut limits_reached = false;
    let zip64 = zip.is_zip64();
    let mut remaining_processed_size = config.max_processed_size;
    let mut has_encrypted_entries = false;

    let mut children: Vec<BackendResultChild> = Vec::new();
    let mut idx = 0u32;
    for entry in zip.into_iter() {
        idx += 1;
        if idx > config.max_children {
            debug!("Limit on total number of entries is exceeded");
            limits_reached = true;
            break;
        }
        if remaining_processed_size == 0 {
            debug!("Limit on processed size is reached");
            limits_reached = true;
            break;
        }
        let mut entry = match entry {
            Ok(entry) => entry,
            Err(e) => match e.kind() {
                std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
                    return Ok(BackendResultKind::error(format!(
                        "Invalid Zip file entry {idx}: {e}"
                    )))
                }
                _ => return Err(format!("Failed to read entry #{idx}: {e}").into()),
            },
        };

        let entry_name = entry.name().to_string();
        if entry_name.ends_with('/') && entry.get_compressed_size() == 0 {
            debug!("Directory {entry_name} skipped");
            continue;
        }
        let mut rel_meta = ZipEntry {
            name: entry_name.clone(),
            is_text: entry.file_type() == FileType::Text,
            timestamp: entry.timestamp(),
            encrypted: entry.is_encrypted(),
            encryption_type: entry.encryption_type(),
            file_comment: to_string_lossy(&entry.get_central_header().comment),
            crc32: entry.expected_crc32(),
            compressed_size: entry.get_compressed_size(),
            uncompressed_size: entry.get_uncompressed_size(),
            compression_method: entry.get_compression_method(),
            origin_os_or_fs: entry.origin_os_or_fs(),
            unix_file_attributes: entry.unix_file_attributes(),
            msdos_file_attributes: entry.msdos_file_attributes(),
            local_and_central_headers_match: match entry.get_local_header() {
                Some(local_header) => entry.get_central_header() == local_header,
                None => false,
            },
            ..ZipEntry::default()
        };

        let comp_ratio = (rel_meta.uncompressed_size as f64) / (rel_meta.compressed_size as f64);
        rel_meta.compression_factor = if comp_ratio.is_nan() {
            None
        } else {
            Some(comp_ratio)
        };
        has_encrypted_entries |= rel_meta.encrypted;
        let mut entry_symbols: Vec<String> = Vec::new();
        let mut skip = false;
        // IMPORTANT!
        // Skipping overlimit entries is challenging in Zip
        //
        // Some compressors are self terminating, therfore the declared compressed size is
        // more an absolute max rather than an exact quantity - i.e. the compressed size
        // can be declared much larger than it actually is and the archive will unzip
        // correctly up until the termination flag is encountered
        // Similarly the uncompressed size is only reliable for those methods which are not
        // self terminated and which will keep decompressing until it's reached
        // In the other cases, decompressors may or may not check that the output size is
        // consistent and if they do, they may just warn or not even warn.
        //
        // This limits our skipping abilities by a far margine
        //
        // TL;DR:
        // - compressed_size cannot be relied upon for skipping
        // - uncompressed_size is only usable with selected compression methods
        if let Some(e) = entry.get_error() {
            debug!("Skipping bad entry #{idx} \"{entry_name}\": {e}");
            entry_symbols.push("ZIP_BAD".to_string());
            rel_meta.error = Some(e.to_string());
            skip = true;
        }
        if entry.is_uncompressed_size_reliable()
            && rel_meta.uncompressed_size > config.max_child_output_size
        {
            debug!("Skipping entry #{idx} \"{entry_name}\" - output size exceeded");
            entry_symbols.push("TOOBIG".to_string());
            limits_reached = true;
            skip = true;
        }
        if rel_meta.encrypted {
            entry_symbols.push("ENCRYPTED".to_string());
            if let Some((i, password)) = possible_passwords
                .iter()
                .enumerate()
                .find(|(_, guess)| entry.set_password(guess))
            {
                let guessed = String::from(*password);
                debug!(r#"Entry #{idx} "{entry_name}": found possible password "{guessed}""#);
                possible_passwords.swap(0, i);
                rel_meta.guessed_password = Some(guessed);
                entry_symbols.push("DECRYPTED".to_string());
            } else {
                debug!("Skipping  - processed size exceeded");
                skip = true;
            }
        }
        if !entry.is_compression_method_supported() {
            debug!(
                "Skipping entry #{idx} \"{entry_name}\" - unsupported method {}",
                rel_meta.compression_method
            );
            entry_symbols.push("ZIP_UNSUP".to_string());
            skip = true;
        }
        let reader = entry.take_reader(config.max_child_input_size);
        if let Err(ref e) = reader {
            debug!("Skipping entry #{idx} \"{entry_name}\" - failed to get reader: {e}");
            entry_symbols.push("ZIP_BAD".to_string());
            skip = true;
        }
        if skip {
            children.push(BackendResultChild {
                path: None,
                force_type: None,
                symbols: entry_symbols,
                relation_metadata: match serde_json::to_value(rel_meta)
                    .map_err(|e| format!("failed to serialize ZipEntry: {e}"))?
                {
                    serde_json::Value::Object(v) => v,
                    _ => unreachable!(),
                },
            });
            continue;
        }
        let mut reader = reader.unwrap();
        let mut output_file = tempfile::NamedTempFile::new_in(&config.output_path)
            .map_err(|e| format!("failed to create a temporary file: {e}"))?;
        let mut writer = ctxutils::io::LimitedWriter::new(
            output_file.as_file_mut(),
            config.max_child_output_size,
        );
        let path = match std::io::copy(&mut reader, &mut writer) {
            Ok(len) => {
                remaining_processed_size = remaining_processed_size.saturating_sub(len);
                if len != rel_meta.uncompressed_size {
                    entry_symbols.push("ZIP_DSIZE_MISMATCH".to_string());
                }
                if reader.integrity_check_ok() {
                    if !reader.was_input_fully_consumed() {
                        entry_symbols.push("ZIP_CSIZE_MISMATCH".to_string());
                    }
                    let output_file = output_file
                        .into_temp_path()
                        .keep()
                        .map_err(|e| format!("failed to preserve a temporary file: {e}"))?
                        .into_os_string()
                        .into_string()
                        .map_err(|s| format!("failed to convert OsString {s:?} to String"))?;
                    files_for_cleanup.push(output_file.clone());
                    Some(output_file)
                } else {
                    entry_symbols.push("CORRUPTED".to_string());
                    None
                }
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
                    entry_symbols.push("CORRUPTED".to_string());
                    None
                }
                std::io::ErrorKind::Other => {
                    debug!("Skipping entry #{idx} \"{entry_name}\" - output size exceeded");
                    entry_symbols.push("TOOBIG".to_string());
                    limits_reached = true;
                    None
                }
                _ => {
                    warn!("Error occurred processing entry #{idx} \"{entry_name}\": {e}");
                    return Err(e.into());
                }
            },
        };
        children.push(BackendResultChild {
            path,
            force_type: None,
            symbols: entry_symbols,
            relation_metadata: match serde_json::to_value(rel_meta)
                .map_err(|e| format!("failed to serialize ZipEntry: {e}"))?
            {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
        });
    }

    let metadata = ZipMetadata {
        archive_comment,
        num_entries,
        has_encrypted_entries,
        zip64,
    };

    // Global features
    let mut symbols = vec![];
    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    let result = BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(metadata)
            .map_err(|e| format!("failed to serialize ZipMetadata: {e}"))?
        {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        },
        children,
    });
    ScopeGuard::into_inner(files_for_cleanup); // disarm garbage collection

    Ok(result)
}

fn to_string_lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

#[cfg(test)]
mod tests {
    mod main;
}
