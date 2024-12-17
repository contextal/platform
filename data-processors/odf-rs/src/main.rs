mod archive;
mod config;
mod error;
mod manifest;
mod meta;
mod odf;
mod xml;

#[cfg(test)]
mod tests;

use backend_utils::objects::{
    BackendRequest, BackendResultChild, BackendResultKind, BackendResultOk, Metadata,
};
use config::Config;
use ctxutils::io::LimitedWriter;
use error::OdfError;
use manifest::{EncryptedKey, FileEntry};
use meta::UserProperty;
use odf::{DocumentType, Odf, ProcessingSummary};
use scopeguard::ScopeGuard;
use serde::Serialize;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::PathBuf,
};
use tempfile::{NamedTempFile, TempPath};
use tracing::{instrument, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use url::Url;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config = Config::new()?;
    backend_utils::work_loop!(config.host.as_deref(), config.port, |request| {
        process_request(request, &config)
    })?;
    unreachable!()
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &Config,
) -> Result<BackendResultKind, Box<dyn std::error::Error>> {
    let input_path = PathBuf::from(&config.objects_path).join(&request.object.object_id);
    let input_path = input_path.to_str().ok_or("Invalid path")?;

    let mut files_for_cleanup = scopeguard::guard(vec![], |files| {
        files.into_iter().for_each(|file| {
            let _ = fs::remove_file(file);
        })
    });

    let processing_result = match process_file(input_path, config) {
        Ok(result) => result,
        Err(e) => {
            if e.is_data_error() {
                return Ok(BackendResultKind::error(format!("Invalid ODF file: {e}")));
            } else {
                return Err(e.into());
            }
        }
    };

    let mut symbols = processing_result.symbols;

    let mut children = Vec::<BackendResultChild>::new();
    for child in processing_result.children {
        let path = match child.file {
            Some(f) => Some(
                f.keep()
                    .map_err(|e| format!("failed to preserve a temporary file: {e}"))?
                    .into_os_string()
                    .into_string()
                    .map_err(|s| format!("failed to convert OsString {s:?} to String"))?,
            ),
            None => None,
        };

        if let Some(path) = &path {
            files_for_cleanup.push(path.clone());
        }

        children.push(BackendResultChild {
            path,
            force_type: child.enforced_type,
            symbols: child.symbols,
            relation_metadata: child.relation_metadata,
        });
    }

    let mut unique_hosts = Vec::<String>::new();
    let mut unique_domains = Vec::<String>::new();
    for input in &processing_result.metadata.hyperlinks {
        if let Ok(url) = Url::parse(input) {
            if let Some(host) = url.host_str() {
                unique_hosts.push(host.to_string());
                if let Ok(domain) = addr::parse_domain_name(host) {
                    if let Some(root) = domain.root() {
                        unique_domains.push(root.to_string());
                    }
                }
            }
        }
    }
    unique_hosts.sort_unstable();
    unique_hosts.dedup();
    unique_domains.sort_unstable();
    unique_domains.dedup();
    let mut limits_reached = processing_result.limits_reached;
    if config.create_domain_children {
        for domain in unique_domains.iter() {
            if children.len() >= config.max_children as usize {
                limits_reached = true;
                break;
            }
            let mut domain_file = tempfile::NamedTempFile::new_in(&config.output_path)?;
            if domain_file.write_all(&domain.clone().into_bytes()).is_ok() {
                children.push(BackendResultChild {
                    path: Some(
                        domain_file
                            .into_temp_path()
                            .keep()
                            .unwrap()
                            .into_os_string()
                            .into_string()
                            .unwrap(),
                    ),
                    force_type: Some("Domain".to_string()),
                    symbols: vec![],
                    relation_metadata: match serde_json::to_value(DomainMetadata {
                        name: domain.clone(),
                    })? {
                        serde_json::Value::Object(v) => v,
                        _ => unreachable!(),
                    },
                });
            }
        }
    }

    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    let mut object_metadata = match serde_json::to_value(processing_result.metadata)
        .map_err(|e| format!("failed to serialize Metadata: {e}"))?
    {
        serde_json::Value::Object(v) => v,
        _ => unreachable!(),
    };
    if !unique_hosts.is_empty() {
        object_metadata.insert(
            "unique_hosts".to_string(),
            serde_json::to_value(unique_hosts)?,
        );
    }
    if !unique_domains.is_empty() {
        object_metadata.insert(
            "unique_domains".to_string(),
            serde_json::to_value(unique_domains)?,
        );
    }

    let result = BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata,
        children,
    });

    ScopeGuard::into_inner(files_for_cleanup); // disarm garbage collection

    Ok(result)
}

#[derive(Debug, Serialize)]
struct OdfMetadata {
    manifest_version: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    properties: HashMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    user_properties: Vec<UserProperty>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hyperlinks: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    file_entry: Vec<FileEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    encrypted_key: Vec<EncryptedKey>,
}

#[derive(Debug)]
struct Child {
    file: Option<TempPath>,
    enforced_type: Option<String>,
    symbols: Vec<String>,
    relation_metadata: Metadata,
}
#[derive(Debug)]
struct ProcessingResult {
    symbols: Vec<String>,
    children: Vec<Child>,
    limits_reached: bool,
    metadata: OdfMetadata,
}

fn process_file(path: &str, config: &Config) -> Result<ProcessingResult, OdfError> {
    let file = File::open(path)?;
    let odf = Odf::new(file)?;
    let mut symbols = Vec::new();
    match odf.document_type {
        DocumentType::Text => symbols.push("ODT".to_string()),
        DocumentType::Spreadsheet => symbols.push("ODS".to_string()),
    }

    let mut processing_summary = ProcessingSummary::default();
    let mut writer = LimitedWriter::new(
        NamedTempFile::new_in(&config.output_path)?,
        config.max_processed_size.min(config.max_processed_size),
    );
    let mut children = Vec::<Child>::new();
    let mut limits_reached = false;

    let r = odf.process(&mut writer, &mut processing_summary);
    let limit_reached = if let Err(error) = r {
        if error.is_write_limit_error() {
            true
        } else {
            return Err(error);
        }
    } else {
        false
    };
    let mut remaining_processed_size = config.max_processed_size.saturating_sub(writer.written());

    let mut child_symbols = Vec::<String>::new();
    let mut enforced_type = None;
    let relation_metadata = Metadata::new();

    let file = if limit_reached {
        limits_reached = true;
        child_symbols.push("TOOBIG".to_string());
        None
    } else {
        writer.flush()?;
        enforced_type = Some("Text".to_string());
        Some(writer.into_inner().into_temp_path())
    };
    children.push(Child {
        enforced_type,
        file,
        symbols: child_symbols,
        relation_metadata,
    });
    let hyperlinks = processing_summary.hyperlinks;

    for image in processing_summary.images {
        if children.len() >= usize::try_from(config.max_children)? {
            limits_reached = true;
            break;
        }

        let mut symbols = Vec::<String>::new();
        let mut relation_metadata = Metadata::new();
        relation_metadata.insert(
            "name".to_string(),
            serde_json::Value::String(image.to_string()),
        );

        let image_path = match normalize_path("content.xml", &image) {
            Ok(path) => path,
            Err(err) => {
                warn!("Unable to normalize path '{image}': {err}");
                continue;
            }
        };
        let limit = std::cmp::min(remaining_processed_size, config.max_child_output_size);
        let mut writer = LimitedWriter::new(NamedTempFile::new_in(&config.output_path)?, limit);
        let r = odf.extract_file_to_writer(&image_path, &mut writer);
        remaining_processed_size = remaining_processed_size.saturating_sub(writer.written());

        if matches!(r, Ok(false)) {
            // File not found in archive
            symbols.push("NOT_FOUND".to_string());
            children.push(Child {
                enforced_type: None,
                file: None,
                symbols,
                relation_metadata,
            });
            continue;
        }

        relation_metadata.insert("request_ocr".to_string(), serde_json::Value::Bool(true));

        let limit_reached = if let Err(error) = r {
            if error.is_write_limit_error() {
                true
            } else {
                return Err(error);
            }
        } else {
            false
        };

        let file = if limit_reached {
            limits_reached = true;
            symbols.push("TOOBIG".to_string());
            None
        } else {
            writer.flush()?;
            Some(writer.into_inner().into_temp_path())
        };

        children.push(Child {
            enforced_type: None,
            file,
            symbols,
            relation_metadata,
        });
    }

    let manifest_version = if odf.manifest.manifest.is_some() {
        odf.manifest.manifest
    } else if odf.manifest.version.is_some() {
        odf.manifest.version
    } else {
        None
    };

    Ok(ProcessingResult {
        symbols,
        children,
        limits_reached,
        metadata: OdfMetadata {
            manifest_version,
            properties: odf.properties,
            user_properties: odf.user_properties,
            hyperlinks,
            file_entry: odf.manifest.file_entry.unwrap_or_default(),
            encrypted_key: odf.manifest.encrypted_key.unwrap_or_default(),
        },
    })
}

fn normalize_path(current_file: &str, new_file: &str) -> Result<String, OdfError> {
    if current_file.starts_with('/') || current_file.ends_with('/') {
        unreachable!("current_file cannot start or end with slash");
    }
    if let Some(path) = new_file.strip_prefix('/') {
        return Ok(path.to_string());
    }
    let parent = if let Some(index) = current_file.rfind('/') {
        &current_file[0..index]
    } else {
        ""
    };
    let path = if parent.is_empty() {
        new_file.to_string()
    } else {
        [parent, new_file].join("/")
    };
    let parts: Vec<&str> = path.split('/').collect();
    let mut result_parts = Vec::<&str>::new();
    for part in parts {
        match part {
            "." => {}
            ".." => {
                if result_parts.pop().is_none() {
                    return Err("Path traversal detected".into());
                }
            }
            part => result_parts.push(part),
        }
    }
    Ok(result_parts.join("/"))
}

#[derive(Serialize)]
struct DomainMetadata {
    name: String,
}
