mod config;
mod write_limited;

use backend_utils::objects::*;
use chardetng::EncodingDetector;
use serde::Serialize;
use std::fs::File;
use std::io::{Error, Read, Seek, SeekFrom};
use std::path::PathBuf;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

mod rtf_control;
mod rtftotext;

#[derive(Serialize)]
struct RtfMeta {
    encoding: String,
}

fn process_rtf(
    input_name: &PathBuf,
    output_path: Option<&str>,
    max_processed_size: u64,
) -> Result<BackendResultKind, Error> {
    let mut children: Vec<BackendResultChild> = Vec::new();
    let mut limits_reached = false;

    let mut f = File::open(input_name)?;
    if f.metadata().unwrap().len() > max_processed_size {
        limits_reached = true;
    }
    let mut handle = f.take(max_processed_size);
    let mut data = String::new();

    let encoding;
    let decoded = match handle.read_to_string(&mut data) {
        Ok(_) => {
            encoding = String::from("utf-8");
            data
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::InvalidData {
                return Err(e);
            }
            let mut buffer = Vec::new();
            f = handle.into_inner();
            f.seek(SeekFrom::Start(0))?;
            handle = f.take(max_processed_size);
            handle.read_to_end(&mut buffer)?;

            let mut detector = EncodingDetector::new();
            detector.feed(&buffer, true);
            let (enc, score) = detector.guess_assess(None, false);
            if !score {
                return Ok(BackendResultKind::error(
                    "Failed to recognize character encoding".to_string(),
                ));
            }
            let (cow, _, _) = enc.decode(&buffer);
            encoding = enc.name().to_string();
            cow.to_string()
        }
    };

    let tokens = match rtftotext::tokenize(decoded.as_bytes().to_vec()) {
        Ok(t) => t,
        Err(_) => return Ok(BackendResultKind::error("Failed to parse RTF".to_string())),
    };

    if let Some(path) = output_path {
        let mut output_text_file = tempfile::NamedTempFile::new_in(path)?;
        let mut text_writer =
            write_limited::LimitedWriter::new(&mut output_text_file, max_processed_size);

        if rtftotext::write_plaintext(&tokens, &mut text_writer).is_err() {
            return Ok(BackendResultKind::error(
                "Failed to write extracted text".to_string(),
            ));
        };

        if text_writer.written_size() > 0 {
            debug!("Wrote {} bytes to {}", text_writer.written_size(), path);
            let mut text_symbols: Vec<String> = Vec::new();
            if text_writer.limit_reached() {
                text_symbols.push("TOOBIG".to_string());
                limits_reached = true;
            }
            children.push(BackendResultChild {
                path: Some(
                    output_text_file
                        .into_temp_path()
                        .keep()
                        .unwrap()
                        .into_os_string()
                        .into_string()
                        .unwrap(),
                ),
                force_type: Some("Text".to_string()),
                symbols: text_symbols,
                relation_metadata: Metadata::new(),
            });
        }
    }

    let mut symbols: Vec<String> = Vec::new();
    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    let rtf_meta = RtfMeta { encoding };
    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(rtf_meta).unwrap() {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        },
        children,
    }))
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    info!("Parsing {}", input_name.display());
    process_rtf(
        &input_name,
        Some(&config.output_path),
        config.max_processed_size,
    )
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
