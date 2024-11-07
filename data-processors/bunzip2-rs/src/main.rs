//! Email backend
mod config;

use backend_utils::objects::*;
use std::io::Read;
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

/// Parse the mail object
///
/// # Returns #
/// * Err for transient retriable errors (I/O, memory allocation, etc)
/// * Ok(BackendResultKind::ok) for success
/// * Ok(BackendReusultKind::error) for permanent errors (e.g. invalid file)
#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, Box<dyn std::error::Error>> {
    let input_path = PathBuf::from(&config.objects_path).join(&request.object.object_id);
    let input_path = input_path.to_str().ok_or("Invalid path")?;
    debug!("Processing request {:?}", request);
    let input_file = std::fs::File::open(input_path)?.take(config.max_child_input_size + 1);
    let mut output_file = tempfile::NamedTempFile::new_in(&config.output_path)?;
    let mut bz = bzip2::read::MultiBzDecoder::new(input_file);
    let mut outf =
        ctxutils::io::LimitedWriter::new(output_file.as_file_mut(), config.max_child_output_size);
    let mut overlimits = false;
    let output_size = match std::io::copy(&mut bz, &mut outf) {
        Err(e) => {
            match e.kind() {
                // Not documented, see source code in the bzip2 crate
                std::io::ErrorKind::InvalidInput | std::io::ErrorKind::UnexpectedEof => {
                    return Ok(BackendResultKind::error(format!("Invalid data ({})", e)))
                }
                std::io::ErrorKind::Other => {
                    debug!("Limit exceeded: {e}");
                    overlimits = true;
                }
                _ => return Err(e.into()),
            }
            0u64
        }
        Ok(v) => v,
    };
    let mut object_symbols: Vec<String> = Vec::new();
    let mut child_symbols: Vec<String> = Vec::new();
    let mut child_relation_metadata = Metadata::new();
    if let Some(serde_json::Value::String(name)) = request.relation_metadata.get("name") {
        let name = if name.ends_with(".bzip2") {
            Some(name[0..(name.len() - 6)].to_string())
        } else if name.ends_with(".bz2") {
            Some(name[0..(name.len() - 4)].to_string())
        } else if name.ends_with(".bz") {
            Some(name[0..(name.len() - 3)].to_string())
        } else {
            None
        };
        if let Some(name) = name {
            child_relation_metadata.insert("name".to_string(), name.into());
        };
    }
    let take = bz.into_inner();
    let remaining_input_size = take.limit();
    let path = if overlimits || remaining_input_size == 0 {
        object_symbols.push("LIMITS_REACHED".to_string());
        child_symbols.push("TOOBIG".to_string());
        None
    } else {
        let input_size = config.max_child_input_size + 1 - remaining_input_size;
        child_relation_metadata.insert(
            "input_size".to_string(),
            serde_json::Value::Number(input_size.into()),
        );
        child_relation_metadata.insert(
            "output_size".to_string(),
            serde_json::Value::Number(output_size.into()),
        );
        let comp_ratio = (output_size as f64) / (input_size as f64);
        child_relation_metadata.insert(
            "compression_factor".to_string(),
            if comp_ratio.is_nan() {
                serde_json::Value::Null
            } else {
                serde_json::Value::Number(serde_json::Number::from_f64(comp_ratio).unwrap())
            },
        );
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
    let child = BackendResultChild {
        path,
        force_type: None,
        symbols: child_symbols,
        relation_metadata: child_relation_metadata,
    };
    Ok(BackendResultKind::ok(BackendResultOk {
        symbols: object_symbols,
        object_metadata: Metadata::new(),
        children: vec![child],
    }))
}
