mod config;

use backend_utils::objects::*;
use serde::Serialize;
use std::io::{Error, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::SystemTime;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

#[derive(Serialize)]
struct PackageInfo<'a> {
    is_signed: bool,
    codepage_id: i32,
    codepage_name: &'a str,
    title: Option<&'a str>,
    subject: Option<&'a str>,
    author: Option<&'a str>,
    generated_by: Option<&'a str>,
    uuid: Option<String>,
    arch: Option<&'a str>,
    languages: Option<Vec<&'a str>>,
    comments: Option<Vec<&'a str>>,
    timestamp: Option<SystemTime>,
}

#[derive(Serialize)]
struct BinaryStream {
    name: String,
    stream_length: Option<u64>,
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

    let mut pkg = match msi::open(&input_name) {
        Ok(p) => p,
        Err(e) => {
            return Ok(BackendResultKind::error(format!(
                "Failed to open MSI file: {e}"
            )));
        }
    };

    // handle streams
    let stream_names: Vec<_> = pkg.streams().collect();
    for stream_name in stream_names {
        if children.len() + 1 >= config.max_children as usize {
            limits_reached = true;
            break;
        }
        let mut child_symbols: Vec<String> = Vec::new();
        let mut path = None;
        let mut stream_length = None;

        if let Ok(mut stream) = pkg.read_stream(stream_name.as_str()) {
            debug!("Accessing {stream_name}");
            let length = stream.seek(SeekFrom::End(0))?;
            stream_length = Some(length);
            stream.seek(SeekFrom::Start(0))?;

            path = if length > config.max_child_output_size || length > config.max_child_input_size
            {
                debug!("Stream exceeds max_child_input/output_size, skipping");
                limits_reached = true;
                child_symbols.push("TOOBIG".to_string());
                None
            } else if remaining_total_size.saturating_sub(length) == 0 {
                debug!("Stream exceeds max_processed_size, skipping");
                limits_reached = true;
                child_symbols.push("TOOBIG".to_string());
                None
            } else {
                let mut output_file = tempfile::NamedTempFile::new_in(&config.output_path)?;
                if std::io::copy(&mut stream, &mut output_file)
                    .map_err(|e| warn!("Failed to extract stream: {}", e))
                    .is_ok()
                {
                    remaining_total_size = remaining_total_size.saturating_sub(length);
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
        };

        let bs = BinaryStream {
            name: stream_name.clone(),
            stream_length,
        };

        children.push(BackendResultChild {
            path,
            force_type: None,
            symbols: child_symbols,
            relation_metadata: match serde_json::to_value(bs).unwrap() {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
        });
    }

    let summary = pkg.summary_info();
    let codepage = summary.codepage();

    let mut uuid = None;
    if let Some(u) = summary.uuid() {
        uuid = Some(u.hyphenated().to_string());
    }

    let mut languages = None;
    let langs = summary.languages();
    if !langs.is_empty() {
        languages = Some(langs.iter().map(msi::Language::tag).collect());
    }

    let mut comments = None;
    if let Some(comms) = summary.comments() {
        comments = Some(comms.lines().collect());
    }

    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    let pkg_info = PackageInfo {
        is_signed: pkg.has_digital_signature(),
        codepage_id: codepage.id(),
        codepage_name: codepage.name(),
        title: summary.title(),
        subject: summary.subject(),
        author: summary.author(),
        generated_by: summary.creating_application(),
        uuid,
        arch: summary.arch(),
        languages,
        comments,
        timestamp: summary.creation_time(),
    };
    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(pkg_info).unwrap() {
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
