//! Email backend
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
struct MessageMetadata<'a> {
    headers: Vec<Header<'a>>,
    date_ts: Option<i64>,
    multipart: bool,
    mime_type: &'a str,
    charset: Option<&'a str>,
    n_attachments: u32,
    has_text_body: bool,
    has_html_body: bool,
    hdrs_health: HeaderHealth,
    // FIXME: add a DKIM validity check
}

#[derive(Serialize)]
struct Header<'a> {
    name: &'a str,
    value: std::borrow::Cow<'a, str>,
    dup: bool,
}

#[derive(Default, Serialize)]
struct Part {
    ord: u64,
    disposition: String,
    inline: bool,
    mime_type: String,
    transfer_encoding: Option<String>,
    transfer_encoding_errors: bool,
    text_decoder: Option<TextDecoder>,
    name: Option<String>,
    names: Vec<String>,
    hdrs_health: HeaderHealth,
}

#[derive(Serialize)]
struct TextDecoder {
    charset: String,
    supported: bool,
    replacement: bool,
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
) -> Result<BackendResultKind, std::io::Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    info!("Parsing {}", input_name.display());
    let input_file = std::fs::File::open(input_name)?;
    let mut mail = match email_rs::Mail::new(input_file) {
        Ok(v) => v,
        Err(e)
            if e.kind() == std::io::ErrorKind::InvalidData
                || e.kind() == std::io::ErrorKind::UnexpectedEof =>
        {
            return Ok(BackendResultKind::error(format!("Invalid data ({})", e)))
        }
        Err(e) => return Err(e),
    };
    debug!("Message {:?}", mail.message());

    let mut metadata = MessageMetadata::default();
    let mut children: Vec<BackendResultChild> = Vec::new();
    let mut limits_reached = false;
    let mut processed_size = 0u64;

    // Message parts processing
    for ord in 0u64.. {
        if ord >= config.max_children.into() {
            debug!("Max children reached, breaking out");
            limits_reached = true;
            break;
        }
        if processed_size > config.max_processed_size {
            debug!("Max processed size reached, breaking out");
            limits_reached = true;
            break;
        }
        let mut output_file = tempfile::NamedTempFile::new_in(&config.output_path)?;
        let dumped_part = match mail.dump_current_part(
            &mut output_file,
            config.max_child_input_size,
            config.max_child_output_size,
        ) {
            Ok(v) => v,
            Err(e) => match e.kind() {
                std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
                    return Ok(BackendResultKind::error(format!("Invalid data ({})", e)))
                }
                _ => return Err(e),
            },
        };

        let dumped_part = match dumped_part {
            Some(v) => v,
            None => break, // No more parts
        };

        // Part dumped
        let mut part_meta = Part {
            ord,
            disposition: dumped_part.part.content_disposition().to_string(),
            inline: dumped_part.part.is_inline(),
            mime_type: dumped_part.part.content_type().to_string(),
            transfer_encoding: dumped_part
                .part
                .content_transfer_encoding()
                .map(|cte| cte.to_string()),
            transfer_encoding_errors: match dumped_part.part.transfer_encoding() {
                email_rs::TransferEncoding::QuotedPrintable => dumped_part.has_ugly_qp,
                email_rs::TransferEncoding::Base64 => dumped_part.has_ugly_b64,
                _ => false,
            },
            text_decoder: dumped_part.part.charset().map(|cset| TextDecoder {
                charset: cset.to_string(),
                supported: !dumped_part.unsupported_charset,
                replacement: dumped_part.has_text_decoder_errors,
            }),
            ..Part::default()
        };
        let mut names: Vec<String> = Vec::new();
        for name in dumped_part.part.names() {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        part_meta.name = names.first().cloned();
        part_meta.names = names;

        if !part_meta.inline {
            metadata.n_attachments += 1;
        } else {
            metadata.has_text_body |= part_meta.mime_type == "text/plain";
            metadata.has_html_body |= part_meta.mime_type == "text/html";
        }

        let (mut part_symbols, hhealth) = get_symbols(&dumped_part.part);
        part_meta.hdrs_health = hhealth;
        if dumped_part.has_ugly_qp || dumped_part.has_ugly_b64 {
            part_symbols.push("INVALID_BODY_ENC".to_string());
        }

        let path = if dumped_part.read_bytes > config.max_child_input_size
            || dumped_part.written_bytes > config.max_child_output_size
        {
            limits_reached = true;
            part_symbols.push("TOOBIG".to_string());
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
        children.push(BackendResultChild {
            path,
            force_type: None,
            symbols: part_symbols,
            relation_metadata: match serde_json::to_value(part_meta).unwrap() {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
        });

        processed_size += dumped_part.read_bytes.max(dumped_part.written_bytes);
    }

    // Global message features
    let message = mail.message();
    let (mut symbols, hhealth) = get_symbols(message);
    metadata.hdrs_health = hhealth;
    for (hdr_name, unstructured, mandatory) in [
        ("bcc", false, false),
        ("cc", false, false),
        ("envelope-to", false, false),
        ("from", false, true),
        ("in-reply-to", false, false),
        ("message-id", false, true),
        ("reply-to", false, false),
        ("return-path", false, false),
        ("subject", true, true),
        ("to", false, true),
    ] {
        if let Some(hdr) = message.get_header(hdr_name) {
            let dup = message.has_duplicate_header(hdr_name);
            if dup {
                symbols.push(format!("DUP_{}", hdr_name.replace('-', "_").to_uppercase()));
            }
            metadata.headers.push(Header {
                name: hdr_name,
                value: if unstructured {
                    std::borrow::Cow::from(&hdr.value)
                } else {
                    std::borrow::Cow::from(hdr.value_nocomments())
                },
                dup,
            });
        } else if mandatory {
            symbols.push(format!(
                "MISSING_{}",
                hdr_name.replace('-', "_").to_uppercase()
            ));
        }
    }
    if message.is_resent() {
        symbols.push("RESENT".to_string());
    }
    if let Some(dt) = message.date() {
        if let Ok(dt) = dt {
            metadata.date_ts = Some(dt.unix_timestamp());
        } else {
            symbols.push("INVALID_DATE".to_string());
        }
    } else {
        symbols.push("MISSING_DATE".to_string());
    }
    if message.is_list() {
        symbols.push("FROM_LIST".to_string());
    }
    metadata.multipart = message.is_multipart();
    match message.get_header("mime-version") {
        Some(v) if v.value == "1.0" => {}
        Some(_) => symbols.push("INVALID_MIME_VER".to_string()),
        None if metadata.multipart => symbols.push("MISSING_MIME_VER".to_string()),
        None => {}
    }
    metadata.mime_type = message.content_type();
    metadata.charset = message.charset();
    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(metadata).unwrap() {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        },
        children,
    }))
}

#[derive(Default, Serialize)]
struct HeaderHealth {
    bad_name: bool,
    bad_value: bool,
    bad_value_encoding: bool,
    bad_value_params: bool,
    bad_value_quoting: bool,
}

fn get_symbols(part: &email_rs::Part) -> (Vec<String>, HeaderHealth) {
    let mut symbols: Vec<String> = Vec::new();
    let (bad_name, bad_value, bad_value_encoding, bad_value_params, bad_value_quoting) =
        part.collect_header_flaws();
    let flaws = HeaderHealth {
        bad_name,
        bad_value,
        bad_value_encoding,
        bad_value_params,
        bad_value_quoting,
    };
    if part.has_invalid_headers() {
        symbols.push("INVALID_HEADERS".to_string());
    }
    if part.is_attachment_with_charset() {
        symbols.push("CHARSET_ATTM".to_string());
    }
    (symbols, flaws)
}
