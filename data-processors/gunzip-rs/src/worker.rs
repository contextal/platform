//! The worker

mod gzip;

use crate::config::Config;
use backend_utils::objects::*;
use serde::Serialize;
use std::fs::File;
use std::path::PathBuf;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};

#[derive(Serialize)]
struct MetaMember<'a> {
    is_text: bool,
    ts: f64,
    extra_flags: u8,
    os: u8,
    has_extra: bool,
    name: Option<&'a str>,
    has_comment: bool,
}

#[derive(Serialize)]
struct ObjectMeta<'a> {
    has_extra: bool,
    has_name: bool,
    has_comment: bool,
    members: Vec<MetaMember<'a>>,
    total_members: usize,
}

impl<'a> From<&'a gzip::GzipHeader> for MetaMember<'a> {
    fn from(hdr: &'a gzip::GzipHeader) -> Self {
        Self {
            is_text: hdr.text,
            ts: hdr
                .time
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64(),
            extra_flags: hdr.extra_flags,
            os: hdr.os,
            has_extra: hdr.extra.is_some(),
            name: hdr.name.as_deref(),
            has_comment: hdr.comment.is_some(),
        }
    }
}

/// Inflates the gzip object
///
/// # Returns #
/// * Err for transient retriable errors (I/O, memory allocation, etc)
/// * Ok(BackendResultKind::ok) for success
/// * Ok(BackendReusultKind::error) for permanent errors (e.g. invalid gzip file)
#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
pub fn process_request(
    request: &BackendRequest,
    config: &Config,
) -> Result<BackendResultKind, std::io::Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    debug!("Processing request {:?}", request);
    let input_file = File::open(input_name)?;
    let mut output_file = tempfile::NamedTempFile::new_in(&config.output_path)?;
    let mut gz = gzip::Gunzip::new(
        input_file,
        config.max_child_input_size,
        0,
        1024,
        64 * 1024,
        config.max_headers,
    )?;
    let mut outf =
        ctxutils::io::LimitedWriter::new(output_file.as_file_mut(), config.max_child_output_size);

    let mut overlimits = false;
    if let Err(e) = std::io::copy(&mut gz, &mut outf) {
        match e.kind() {
            std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
                return Ok(BackendResultKind::error(format!("Invalid data ({})", e)))
            }
            std::io::ErrorKind::Other => {
                debug!("Limit exceeded: {e}");
                overlimits = true;
            }
            _ => return Err(e),
        }
    }

    // Success
    let mut symbols: Vec<String> = Vec::new();
    let members: Vec<MetaMember<'_>> = gz.members().iter().map(|hdr| hdr.into()).collect();
    let object_metadata = ObjectMeta {
        has_extra: members.iter().any(|hdr| hdr.has_extra),
        has_name: members.iter().any(|hdr| hdr.name.is_some()),
        has_comment: members.iter().any(|hdr| hdr.has_comment),
        members,
        total_members: gz.total_members(),
    };
    if object_metadata.total_members > 1 {
        symbols.push("GZIP_MULTI_MEMBER".to_string());
    }
    if gz.has_trailing_garbage() {
        symbols.push("GZIP_TRAILING_GARBAGE".to_string());
    }
    let mut child_relation_metadata = Metadata::new();
    let mut names: Vec<String> = Vec::new();
    if let Some(serde_json::Value::String(name)) = request.relation_metadata.get("name") {
        let name = if name.ends_with(".gz") {
            name[0..(name.len() - 3)].to_string()
        } else {
            name.to_string()
        };
        names.push(name.clone());
    }
    for member in object_metadata.members.iter() {
        if let Some(name) = member.name {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
    }
    child_relation_metadata.insert(
        "name".to_string(),
        names.first().map(|n| n.to_string()).into(),
    );
    child_relation_metadata.insert("names".to_string(), names.into());
    let (insz, outsz) = gz.get_io_size();
    child_relation_metadata.insert(
        "input_size".to_string(),
        serde_json::Value::Number(insz.into()),
    );
    child_relation_metadata.insert(
        "output_size".to_string(),
        serde_json::Value::Number(outsz.into()),
    );
    let comp_ratio = (outsz as f64) / (insz as f64);
    child_relation_metadata.insert(
        "compression_factor".to_string(),
        if comp_ratio.is_nan() {
            serde_json::Value::Null
        } else {
            serde_json::Value::Number(serde_json::Number::from_f64(comp_ratio).unwrap())
        },
    );
    let mut child_symbols: Vec<String> = Vec::new();
    let path: Option<String> = if overlimits {
        symbols.push("LIMITS_REACHED".to_string());
        child_symbols.push("TOOBIG".to_string());
        None
    } else {
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
    let object_metadata = match serde_json::to_value(object_metadata).unwrap() {
        serde_json::Value::Object(map) => map,
        _ => unreachable!(),
    };
    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata,
        children: vec![child],
    }))
}
