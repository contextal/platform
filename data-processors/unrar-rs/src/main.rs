//! Unrar backend
use backend_utils::objects::{
    BackendRequest, BackendResultChild, BackendResultKind, BackendResultOk,
};
use scopeguard::ScopeGuard;
use serde::Serialize;
use std::{
    cell::Cell,
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use time::PrimitiveDateTime;
use tracing::{error, info, warn};
use tracing_subscriber::prelude::*;
use unrar_rs::{
    config::Config, ArchiveHandler, Blake2spHash, CompressionMethod, Crc32, FileAttributes,
    HashType, MsDosDateTime, OperatingSystem, RarBackendError, RedirType, WindowsFiletime,
};

fn main() -> Result<(), RarBackendError> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::new()?;

    backend_utils::work_loop!(None, None, |request| { process_request(request, &config) })?;

    Ok(())
}

/// RAR archive attributes.
#[derive(Debug, Serialize)]
struct ArchiveMetadata {
    /// True if it is a solid archive.
    is_solid: bool,

    /// True if the archive has "locked for modifications" attribute.
    is_locked: bool,

    /// True if the archive has encrypted headers.
    has_encrypted_headers: bool,

    /// True if the archive has a comment.
    has_comment: bool,

    /// Archive comment.
    comment: Option<String>,

    /// True if the archive has a recovery record.
    has_recovery_record: bool,

    /// True if the archive follows new volume naming scheme, i.e. `volname.partN.rar`.
    is_new_numbering_scheme: bool,

    /// True is the archive-file is a part of multi-volume archive.
    is_multivolume_part: bool,

    /// True is the archive-file is a first volume of multi-volume archive.
    is_multivolume_start: bool,

    /// True if archive authenticity information is present (this RAR flag is obsolete).
    is_signed: bool,

    /// A list of directories from the archive.
    directories: Vec<String>,

    /// First matched password
    password: Option<String>,
}

/// File attributes from a RAR archive.
#[derive(Debug, Serialize)]
struct File {
    /// File's path and name within the archive.
    name: String,

    /// Filesystem file's attributes.
    ///
    /// The meaning of attributes depends on the operating system which added the file to an
    /// archive.
    attributes: FileAttributes,

    /// Unpacked file CRC32 checksum according to archive entry headers.
    expected_crc32: Crc32,

    /// File's modification timestamp according to archive entry headers.
    filetime: MsDosDateTime,

    /// File's compression method.
    compression_method: CompressionMethod,

    /// True is the file is encrypted withing the archive. False otherwise.
    is_encrypted: bool,

    /// Compressed file size in bytes.
    compressed_size: u64,

    /// Uncompressed file size in bytes. In some cases it might not be available (think of a file
    /// compressed from stdin and split into multiple volumes; when the uncompressed size is known
    /// all the preceding volumes are already closed).
    uncompressed_size: Option<u64>,

    /// Compress ratio, if applicable.
    compress_ratio: Option<f32>,

    /// Operating system used to put a file into an archive.
    creation_os: OperatingSystem,

    /// RAR version required to extract the file.
    version_to_extract: String,

    /// Size of file compression dictionary in bytes.
    dict_size: u64,

    /// Type of hash function used to verify file data integrity.
    ///
    /// The value is expected to be one of
    /// - no checksum or unknown hash function type (yes, this looks odd),
    /// - CRC32, or
    /// - BLAKE2sp.
    hash_type: HashType,

    /// This attribute contains 32 bytes of file data BLAKE2sp hash, if the file's hash is of
    /// Blake2sp type.
    blake2_hash: Option<Blake2spHash>,

    /// Type of file system redirection.
    ///
    /// It is expected to be one of the following options:
    /// - No redirection, usual file,
    /// - Unix symbolic link,
    /// - Windows symbolic link,
    /// - Windows junction,
    /// - Hard link,
    /// - File reference saved with `-oi` CLI switch.
    redir_type: RedirType,

    /// File system redirection target name, such as target of symbolic link or file reference.
    ///
    /// It is returned as-is and its value might be not immediately applicable for further use. For
    /// example, you may need to remove '??' or 'UNC' prefixes for Windows junctions or prepend the
    /// extraction destination path.
    redir_name: Option<String>,

    /// Optional file modification time.
    mtime: Option<WindowsFiletime>,

    /// Optional file creation (?) time.
    ctime: Option<WindowsFiletime>,

    /// Optional file last access time.
    atime: Option<WindowsFiletime>,
}

enum Password<'a> {
    None,
    Invalid,
    Possible(&'a str, std::vec::IntoIter<&'a str>),
    Matched(&'a str),
}

fn process_request(
    request: &BackendRequest,
    config: &Config,
) -> Result<BackendResultKind, RarBackendError> {
    let input_path = PathBuf::from(&config.objects_path).join(&request.object.object_id);

    let mut possible_passwords = Vec::new();
    if let Some(serde_json::Value::Object(glob)) = request.relation_metadata.get("_global") {
        if let Some(serde_json::Value::Array(passwords)) = glob.get("possible_passwords") {
            for pwd in passwords {
                if let serde_json::Value::String(s) = pwd {
                    possible_passwords.push(s.as_str());
                }
            }
        }
    }
    let mut password_it = possible_passwords.into_iter();
    let mut password = if let Some(password) = password_it.next() {
        Password::Possible(password, password_it)
    } else {
        Password::None
    };

    loop {
        let (symbols, mut metadata, children) =
            match process_archive(&mut password, &input_path, config) {
                Ok((s, m, c)) => (s, m, c),
                Err(e) => {
                    match e {
                        RarBackendError::InvalidPassword | RarBackendError::MissingPassword => {
                            if let Password::Possible(_, mut remaining) = password {
                                if let Some(s) = remaining.next() {
                                    password = Password::Possible(s, remaining);
                                    continue;
                                }
                            }
                        }
                        _ => {}
                    }
                    let message = format!("failed to open an archive: {e}");
                    error!(message);
                    return Ok(BackendResultKind::error(message));
                }
            };

        if let Password::Matched(s) = password {
            metadata.password = Some(s.to_string());
        }

        return Ok(BackendResultKind::ok(BackendResultOk {
            symbols,
            object_metadata: match serde_json::to_value(metadata)? {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
            children,
        }));
    }
}

fn process_archive(
    password: &mut Password,
    input_path: &Path,
    config: &Config,
) -> Result<(Vec<String>, ArchiveMetadata, Vec<BackendResultChild>), RarBackendError> {
    let current_password = match *password {
        Password::None => None,
        Password::Matched(s) => Some(s),
        Password::Possible(s, _) => Some(s),
        Password::Invalid => unreachable!(),
    };
    let archive = ArchiveHandler::new(input_path, current_password, config)?;

    if archive.has_encrypted_headers() {
        if let Password::Possible(s, _) = *password {
            *password = Password::Matched(s);
        }
    }

    let mut symbols: HashSet<&str> = HashSet::new();
    let comment = match archive.comment() {
        Ok(v) => v,
        Err(RarBackendError::ArchiveCommentTruncated(comment)) => {
            warn!("comment truncated");
            symbols.insert("COMMENT_TRUNCATED");
            Some(comment)
        }
        Err(e) => {
            warn!("failed to access archive comment: {e}");
            symbols.insert("COMMENT_EXTRACTION_FAILED");
            None
        }
    };

    let mut metadata = ArchiveMetadata {
        has_comment: archive.has_comment(),
        has_encrypted_headers: archive.has_encrypted_headers(),
        has_recovery_record: archive.has_recovery_record(),
        is_locked: archive.is_locked(),
        is_multivolume_part: archive.is_multivolume_part(),
        is_multivolume_start: archive.is_multivolume_start(),
        is_new_numbering_scheme: archive.is_new_numbering_scheme(),
        is_signed: archive.is_signed(),
        is_solid: archive.is_solid(),
        comment,
        directories: vec![],
        password: None,
    };

    let mut allowed_entries_left = config.max_entries_to_process;
    let allowed_compressed_size_left = Cell::new(config.max_processed_size);

    let mut children = scopeguard::guard(Vec::<BackendResultChild>::new(), |children| {
        children
            .into_iter()
            .filter_map(|child| child.path)
            .for_each(|file| {
                let _ = fs::remove_file(file);
            });
    });

    archive
        .into_iter()
        .take_while(|result| {
            match result {
                Ok(_) => return true,
                Err(RarBackendError::VolumeOpen) => {
                    info!("archive continues on the next volume and this is not supported yet");
                }
                Err(e) => {
                    warn!("unexpected error while iterating over archive entries: {e:?}");
                }
            }
            false
        })
        .map(|result| result.expect("unreachable"))
        .take_while(|_| {
            if allowed_entries_left == 0 {
                return false;
            }
            allowed_entries_left -= 1;
            true
        })
        .take_while(|_| allowed_compressed_size_left.get() != 0)
        .map(|mut entry| {
            if entry.is_directory() {
                metadata.directories.push(entry.filename());

                Ok(None)
            } else {
                let mut child_symbols = vec![];

                let filetime = {
                    let raw_filetime = entry.file_time();
                    match raw_filetime.try_into() {
                        Ok::<PrimitiveDateTime, _>(v) => MsDosDateTime::Parsed(v.assume_utc()),
                        Err(e) => {
                            warn!("failed to interpret file time: {e}");
                            child_symbols.push("INVALID_FILETIME".into());
                            MsDosDateTime::Raw(raw_filetime)
                        }
                    }
                };

                let mtime = entry.mtime_raw().map(|raw| match raw.try_into() {
                    Ok(v) => WindowsFiletime::Parsed(v),
                    Err(e) => {
                        warn!("failed to interpret file mtime: {e}");
                        child_symbols.push("INVALID_MTIME".into());
                        WindowsFiletime::Raw(raw)
                    }
                });

                let atime = entry.atime_raw().map(|raw| match raw.try_into() {
                    Ok(v) => WindowsFiletime::Parsed(v),
                    Err(e) => {
                        warn!("failed to interpret file atime: {e}");
                        child_symbols.push("INVALID_ATIME".into());
                        WindowsFiletime::Raw(raw)
                    }
                });

                let ctime = entry.ctime_raw().map(|raw| match raw.try_into() {
                    Ok(v) => WindowsFiletime::Parsed(v),
                    Err(e) => {
                        warn!("failed to interpret file ctime: {e}");
                        child_symbols.push("INVALID_CTIME".into());
                        WindowsFiletime::Raw(raw)
                    }
                });

                let extraction_path = {
                    if entry.uncompressed_size() > Some(config.max_child_output_size) {
                        info!(
                            "decompressed file size ({}) exceeds the entry limit ({})",
                            entry.uncompressed_size().expect("at this stage it is Some"),
                            config.max_child_output_size
                        );
                        symbols.insert("LIMITS_REACHED");
                        child_symbols.push("TOOBIG".into());
                        None
                    } else if entry.compressed_size() > config.max_child_input_size {
                        info!(
                            "compressed file size ({}) exceeds the entry limit ({})",
                            entry.compressed_size(),
                            config.max_child_input_size
                        );
                        symbols.insert("LIMITS_REACHED");
                        child_symbols.push("TOOBIG".into());
                        None
                    } else if entry.is_encrypted() && matches!(&password, Password::None) {
                        info!("no password has been provided for an encrypted entry");
                        None
                    } else {
                        let output_file = NamedTempFile::new_in(&config.output_path)?;
                        allowed_compressed_size_left.set(
                            allowed_compressed_size_left
                                .get()
                                .saturating_sub(entry.compressed_size()),
                        );
                        let output_file = match entry.extract_to_file(output_file.as_ref()) {
                            Ok(_) => {
                                if entry.is_encrypted() {
                                    child_symbols.push("DECRYPTED".into());
                                }
                                if let Password::Possible(s, _) = *password {
                                    *password = Password::Matched(s);
                                }
                                Some(output_file)
                            }
                            Err(e @ RarBackendError::ChecksumMismatchWhileExtracting { .. }) => {
                                info!("{e}");
                                child_symbols.push("CHECKSUM_MISMATCH".into());
                                symbols.insert("HAS_CHECKSUM_INCONSISTENCY");
                                Some(output_file)
                            }
                            Err(e @ RarBackendError::VolumeOpenWhileExtracting { .. }) => {
                                warn!("{e}");
                                Some(output_file)
                            }
                            Err(e @ RarBackendError::InvalidPassword) => {
                                if let Password::Possible(_, remaining) = password {
                                    if remaining.len() == 0 {
                                        *password = Password::Invalid;
                                    } else {
                                        return Err(e);
                                    };
                                }
                                warn!("failed to extract an entry: {e}");
                                child_symbols.push("INVALID_PASSWORD".into());
                                None
                            }
                            Err(RarBackendError::UnknownWhenExtracting {
                                extracted_so_far,
                                entry,
                            }) => {
                                // Originating error code is not documented in `libunrar`.
                                //
                                // It seems one of the reasons when it could be triggered is when
                                // entry extraction is interrupted from a callback function, which
                                // at the moment happens only when uncompressed entry size grows
                                // over the specified limit (what in its turn could happen when
                                // uncompressed size is not available in entry metadata, or,
                                // hypothetically, when uncompressed size is forged in archive
                                // entry header and `libunrar` didn't notice / didn't stop
                                // extraction by itself when it reached such forged size).
                                //
                                // To verify whether going beyond the uncompressed entry size limit
                                // and interruption from a callback function is actually the case,
                                // we check size of partially extracted data, and if it is larger
                                // than the limit we add appropriate symbols.
                                warn!(
                                    "unknown error (might be a safety belt for uncompressed \
                                    size limit) while extracting an entry: {entry:?}"
                                );

                                if extracted_so_far > config.max_child_output_size {
                                    info!(
                                        "partially decompressed file size ({extracted_so_far}) \
                                        exceeds the entry limit ({})",
                                        config.max_child_output_size
                                    );
                                    symbols.insert("LIMITS_REACHED");
                                    child_symbols.push("TOOBIG".into());
                                }
                                None
                            }
                            Err(e) => {
                                warn!("failed to extract an entry: {e}");
                                None
                            }
                        };
                        match output_file {
                            Some(output_file) => Some(
                                output_file
                                    .into_temp_path()
                                    .keep()?
                                    .into_os_string()
                                    .into_string()
                                    .map_err(RarBackendError::Utf8)?,
                            ),
                            None => None,
                        }
                    }
                };

                if entry.is_encrypted() {
                    child_symbols.push("ENCRYPTED".into());
                }
                if entry.is_solid() {
                    child_symbols.push("SOLID".into());
                }
                if entry.is_split_between_volumes() {
                    child_symbols.push("PARTIAL_DATA".into());
                }

                children.push(BackendResultChild {
                    path: extraction_path,
                    symbols: child_symbols,
                    relation_metadata: match serde_json::to_value(File {
                        name: entry.filename(),
                        attributes: entry.file_attr(),
                        expected_crc32: entry.file_crc32(),
                        compression_method: entry.compression_method(),
                        filetime,
                        is_encrypted: entry.is_encrypted(),
                        uncompressed_size: entry.uncompressed_size(),
                        compressed_size: entry.compressed_size(),
                        creation_os: entry.creation_os(),
                        version_to_extract: entry.version_to_extract(),
                        dict_size: entry.dict_size(),
                        hash_type: entry.hash_type(),
                        blake2_hash: entry.blake2_hash(),
                        redir_type: entry.redir_type(),
                        redir_name: entry.redir_name(),
                        mtime,
                        ctime,
                        atime,
                        compress_ratio: entry.compress_ratio(),
                    })? {
                        serde_json::Value::Object(v) => v,
                        _ => unreachable!(),
                    },
                    force_type: None,
                });

                Ok(Some(()))
            }
        })
        .filter_map(|v| match v {
            Ok(None) => None,
            Ok(Some(v)) => Some(Ok(v)),
            Err(e) => Some(Err(e)),
        })
        .take(config.max_children as _)
        .collect::<Result<Vec<()>, RarBackendError>>()?;

    if allowed_entries_left == 0 {
        info!("Limit on number of archive entries to process has been reached");
        symbols.insert("LIMITS_REACHED");
    }

    if children.len() == config.max_children as usize {
        info!("Limit on total number of produced `BackendResultChild` objects has been reached");
        symbols.insert("LIMITS_REACHED");
    }

    if allowed_compressed_size_left.get() == 0 {
        info!("Limit on overall compressed size has been reached");
        symbols.insert("LIMITS_REACHED");
    }

    let mut symbols: Vec<String> = symbols.into_iter().map(String::from).collect();
    symbols.sort();

    metadata.directories.sort();
    Ok((symbols, metadata, ScopeGuard::into_inner(children)))
}

#[cfg(test)]
mod test;
