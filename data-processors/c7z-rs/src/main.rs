mod config;

use backend_utils::objects::*;
use serde::Serialize;
use sevenz_rust::SevenZReader;
use std::io::{Error, ErrorKind, Read, Seek};
use std::path::PathBuf;
use std::str::FromStr;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

#[derive(Serialize)]
pub struct ArchiveInfo {
    num_of_files: usize,
    num_of_folders: usize,
    total_compressed_size: u64,
    total_uncompressed_size: u64,
}

#[derive(Serialize)]
pub struct EntryInfo {
    name: String,
    is_anti_item: bool,
    has_creation_date: bool,
    has_last_modified_date: bool,
    has_access_date: bool,
    creation_date: i64,
    last_modified_date: i64,
    access_date: i64,
    has_windows_attributes: bool,
    windows_attributes: u32,
    has_crc: bool,
    crc: u64,
    compressed_crc: u64,
    size: u64,
    compressed_size: u64,
    guessed_password: Option<String>,
}

fn extract_archive<R: Read + Seek>(
    sz: &mut SevenZReader<R>,
    config: &config::Config,
    limits_reached: &mut bool,
    password: Option<String>,
    is_last_password: bool,
    no_extract: bool,
) -> Result<Vec<BackendResultChild>, Error> {
    let mut children: Vec<BackendResultChild> = Vec::new();
    let mut remaining_total_size = config.max_processed_size + 1;

    let mut num = 0;
    let mut failed = 0;
    let ret = sz.for_each_entries(|entry, reader| {
        let mut guessed_password = None;
        let mut entry_symbols: Vec<String> = Vec::new();
        if num >= config.max_children {
            *limits_reached = true;
            return Ok(true);
        }
        if entry.size == 0 {
            return Ok(true);
        }
        num += 1;
        if password.is_some() {
            entry_symbols.push("ENCRYPTED".to_string());
        }
        let path = if entry.compressed_size > config.max_child_input_size {
            debug!("Entry exceeds max_child_input_size, skipping");
            *limits_reached = true;
            entry_symbols.push("TOOBIG".to_string());
            None
        } else if entry.size > config.max_child_output_size {
            debug!("Entry exceeds max_child_output_size, skipping");
            *limits_reached = true;
            entry_symbols.push("TOOBIG".to_string());
            None
        } else if remaining_total_size.saturating_sub(entry.size) == 0 {
            debug!("Entry exceeds max_processed_size, skipping");
            *limits_reached = true;
            entry_symbols.push("TOOBIG".to_string());
            None
        } else if no_extract {
            None
        } else {
            let output_file = tempfile::NamedTempFile::new_in(&config.output_path)
                .unwrap()
                .into_temp_path()
                .keep()
                .unwrap()
                .into_os_string()
                .into_string()
                .unwrap();

            let ret = sevenz_rust::default_entry_extract_fn(
                entry,
                reader,
                &PathBuf::from_str(&output_file).unwrap(),
            );
            match ret {
                Ok(true) => {
                    remaining_total_size = remaining_total_size.saturating_sub(entry.size);
                    if let Some(password) = &password {
                        guessed_password = Some(password.clone());
                        entry_symbols.push("DECRYPTED".to_string());
                    }
                    Some(output_file)
                }
                _ => {
                    failed += 1;
                    let _ = std::fs::remove_file(output_file);
                    None
                }
            }
        };
        let entry_info = EntryInfo {
            name: entry.name.clone(),
            is_anti_item: entry.is_anti_item,
            has_creation_date: entry.has_creation_date,
            has_last_modified_date: entry.has_last_modified_date,
            has_access_date: entry.has_access_date,
            creation_date: entry.creation_date.to_unix_time(),
            last_modified_date: entry.last_modified_date.to_unix_time(),
            access_date: entry.access_date.to_unix_time(),
            has_windows_attributes: entry.has_windows_attributes,
            windows_attributes: entry.windows_attributes,
            has_crc: entry.has_crc,
            crc: entry.crc,
            compressed_crc: entry.compressed_crc,
            size: entry.size,
            compressed_size: entry.compressed_size,
            guessed_password,
        };
        children.push(BackendResultChild {
            path,
            force_type: None,
            symbols: entry_symbols,
            relation_metadata: match serde_json::to_value(entry_info).unwrap() {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
        });
        Ok(true)
    });

    match ret {
        Ok(_) => {
            if password.is_some() && num == failed && !is_last_password {
                Err(Error::new(ErrorKind::Other, "Bad password or other error"))
            } else {
                Ok(children)
            }
        }
        Err(sevenz_rust::Error::PasswordRequired) => {
            Err(Error::new(ErrorKind::InvalidInput, "No password provided"))
        }
        Err(_) => {
            if num > 0 && failed < num {
                Ok(children)
            } else {
                Err(Error::new(ErrorKind::InvalidData, "Failed to extract data"))
            }
        }
    }
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, Error> {
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
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    info!("Parsing {}", input_name.display());
    let mut sz = match SevenZReader::open(&input_name, "".into()) {
        Ok(sz) => sz,
        Err(e) => {
            debug!("An IO error occurred: {e}");
            match e {
                sevenz_rust::Error::Io(serr, _) | sevenz_rust::Error::FileOpen(serr, _) => {
                    // Temporary IO error
                    return Err(serr);
                }
                e => {
                    // Permanent format error
                    return Ok(BackendResultKind::error(format!(
                        "Invalid 7zip archive: {e}"
                    )));
                }
            }
        }
    };

    let mut limits_reached = false;
    let children = match extract_archive(&mut sz, config, &mut limits_reached, None, false, false) {
        Ok(ch) => ch,
        Err(err) if err.kind() == ErrorKind::InvalidInput => {
            let mut ch = Err(Error::new(ErrorKind::InvalidData, "Failed to process data"));
            if possible_passwords.is_empty() {
                sz = SevenZReader::open(&input_name, "xxx".into()).unwrap();
                ch = extract_archive(
                    &mut sz,
                    config,
                    &mut limits_reached,
                    Some("xxx".into()),
                    true,
                    true,
                );
            } else {
                for (pass, is_last_pass) in possible_passwords
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (p, i == possible_passwords.len() - 1))
                {
                    sz = SevenZReader::open(&input_name, (*pass).into()).unwrap();
                    ch = extract_archive(
                        &mut sz,
                        config,
                        &mut limits_reached,
                        Some(String::from(*pass)),
                        is_last_pass,
                        false,
                    );
                    if ch.is_ok() {
                        break;
                    }
                }
            }
            match ch {
                Ok(ch) => ch,
                Err(_) => {
                    return Ok(BackendResultKind::error(
                        "Failed to process encrypted archive".to_string(),
                    ))
                }
            }
        }
        Err(_) => {
            return Ok(BackendResultKind::error(
                "Failed to extract archive".to_string(),
            ))
        }
    };

    let total_uncompressed_size: u64 = sz
        .archive()
        .files
        .iter()
        .filter(|e| e.has_stream())
        .map(|e| e.size())
        .sum();

    let total_compressed_size: u64 = sz
        .archive()
        .files
        .iter()
        .filter(|e| e.has_stream())
        .map(|e| e.compressed_size)
        .sum();

    let num_of_files = sz.archive().files.len();

    let num_of_folders = sz.archive().folders.len();

    let mut symbols: Vec<String> = Vec::new();
    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    let arch_info = ArchiveInfo {
        num_of_files,
        num_of_folders,
        total_compressed_size,
        total_uncompressed_size,
    };
    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(arch_info).unwrap() {
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

#[test]
fn regular_archive() {
    let config = config::Config::new().unwrap();
    let path = "tests/test_data/test.7z";
    let mut limits_reached = false;
    let mut sz = SevenZReader::open(&path, "".into()).unwrap();
    let ch = extract_archive(&mut sz, &config, &mut limits_reached, None, false, false).unwrap();

    assert_eq!(ch.len(), 1, "children number mismatch");
    assert!(ch[0].symbols.is_empty(), "symbols mismatch");
    assert_eq!(limits_reached, false, "limits_reached mismatch");

    if let Some(path) = &ch[0].path {
        std::fs::remove_file(path).unwrap();
    } else {
        panic!("No output file");
    }

    let m = &ch[0].relation_metadata;
    assert_eq!(m["has_crc"], true, "has_crc mismatch");
    assert_eq!(m["crc"], 3148199144 as u32, "crc mismatch");
    assert_eq!(
        m["guessed_password"],
        serde_json::Value::Null,
        "guessed_password mismatch"
    );
    assert_eq!(m["name"], "contextal.jpg", "name mismatch");
    assert_eq!(m["size"], 35996, "size mismatch");
    assert_eq!(m["compressed_size"], 32273, "compressed_size mismatch");
    assert_eq!(
        m["has_last_modified_date"], true,
        "has_last_modified_date mismatch"
    );
    assert_eq!(
        m["last_modified_date"], 1691602808,
        "last_modified_date mismatch"
    );
}

#[test]
fn successful_decryption() {
    let config = config::Config::new().unwrap();
    let path = "tests/test_data/test-pass1234.7z";
    let mut limits_reached = false;
    let mut sz = SevenZReader::open(&path, "1234".into()).unwrap();
    let ch = extract_archive(
        &mut sz,
        &config,
        &mut limits_reached,
        Some(String::from("1234")),
        false,
        false,
    )
    .unwrap();

    assert_eq!(ch.len(), 2, "children number mismatch");
    assert_eq!(
        ch[0].symbols,
        ["ENCRYPTED", "DECRYPTED"],
        "symbols mismatch"
    );
    assert_eq!(
        ch[1].symbols,
        ["ENCRYPTED", "DECRYPTED"],
        "symbols mismatch"
    );

    if let Some(path) = &ch[0].path {
        std::fs::remove_file(path).unwrap();
    } else {
        panic!("No output file");
    }

    let m = &ch[0].relation_metadata;
    assert_eq!(m["has_crc"], true, "has_crc mismatch");
    assert_eq!(m["crc"], 3148199144 as u32, "crc mismatch");
    assert_eq!(m["guessed_password"], "1234", "guessed_password mismatch");
    assert_eq!(m["name"], "contextal.jpg", "name mismatch");
    assert_eq!(m["size"], 35996, "size mismatch");
    assert_eq!(m["compressed_size"], 32288, "compressed_size mismatch");
    assert_eq!(
        m["has_last_modified_date"], true,
        "has_last_modified_date mismatch"
    );
    assert_eq!(
        m["last_modified_date"], 1691602808,
        "last_modified_date mismatch"
    );

    let m = &ch[1].relation_metadata;
    assert_eq!(m["has_crc"], true, "has_crc mismatch");
    assert_eq!(m["crc"], 3902844536 as u32, "crc mismatch");
    assert_eq!(m["guessed_password"], "1234", "guessed_password mismatch");
    assert_eq!(m["name"], "contextal.txt", "name mismatch");
    assert_eq!(m["size"], 311, "size mismatch");
    assert_eq!(m["compressed_size"], 240, "compressed_size mismatch");
    assert_eq!(
        m["has_last_modified_date"], true,
        "has_last_modified_date mismatch"
    );
    assert_eq!(
        m["last_modified_date"], 1703009804,
        "last_modified_date mismatch"
    );
}

#[test]
fn failed_decryption() {
    let config = config::Config::new().unwrap();
    let path = "tests/test_data/test-pass-solid.7z";
    let mut limits_reached = false;
    let mut sz = SevenZReader::open(&path, "wrong_pass".into()).unwrap();
    let ch = extract_archive(
        &mut sz,
        &config,
        &mut limits_reached,
        Some(String::from("wrong_pass")),
        true,
        false,
    )
    .unwrap();

    assert_eq!(ch.len(), 2, "children number mismatch");
    assert_eq!(ch[0].symbols, ["ENCRYPTED"], "symbols mismatch");
    assert_eq!(ch[1].symbols, ["ENCRYPTED"], "symbols mismatch");

    let m = &ch[0].relation_metadata;
    assert_eq!(m["has_crc"], true, "has_crc mismatch");
    assert_eq!(m["crc"], 3148199144 as u32, "crc mismatch");
    assert_eq!(
        m["guessed_password"],
        serde_json::Value::Null,
        "guessed_password mismatch"
    );
    assert_eq!(m["name"], "contextal.jpg", "name mismatch");
    assert_eq!(m["size"], 35996, "size mismatch");
    assert_eq!(m["compressed_size"], 32480, "compressed_size mismatch");
    assert_eq!(
        m["has_last_modified_date"], true,
        "has_last_modified_date mismatch"
    );
    assert_eq!(
        m["last_modified_date"], 1691602808,
        "last_modified_date mismatch"
    );

    let m = &ch[1].relation_metadata;
    assert_eq!(m["has_crc"], true, "has_crc mismatch");
    assert_eq!(m["crc"], 3902844536 as u32, "crc mismatch");
    assert_eq!(
        m["guessed_password"],
        serde_json::Value::Null,
        "guessed_password mismatch"
    );
    assert_eq!(m["name"], "contextal.txt", "name mismatch");
    assert_eq!(m["size"], 311, "size mismatch");
    assert_eq!(m["compressed_size"], 0, "compressed_size mismatch");
    assert_eq!(
        m["has_last_modified_date"], true,
        "has_last_modified_date mismatch"
    );
    assert_eq!(
        m["last_modified_date"], 1703009804,
        "last_modified_date mismatch"
    );
}
