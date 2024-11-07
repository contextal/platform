mod config;

use backend_utils::objects::*;
use ctxole::Ole;
use ctxutils::io::{LimitedWriter, WriteLimitExceededError};
use msg_rs::*;
use serde::Serialize;
use std::io::{self, Read, Seek};
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

#[derive(Serialize)]
struct MessageRecipient {
    kind: Option<&'static str>,
    name: Option<String>,
    email: Option<String>,
}

impl<'o, O: Read + Seek> From<&Recipient<'o, O>> for MessageRecipient {
    fn from(rcpt: &Recipient<'o, O>) -> Self {
        Self {
            kind: match rcpt.kind() {
                Some(RecipientType::To) => Some("To"),
                Some(RecipientType::Cc) => Some("Cc"),
                Some(RecipientType::Bcc) => Some("Bcc"),
                _ => None,
            },
            name: rcpt.name().map(|s| s.to_string()),
            email: rcpt.email().map(|s| s.to_string()),
        }
    }
}

fn save_child<R: Read>(
    mut r: R,
    config: &config::Config,
    max_processed_size: &mut u64,
    limits_reached: &mut bool,
) -> Result<BackendResultChild, io::Error> {
    let tempf = tempfile::NamedTempFile::new_in(&config.output_path)?;
    debug!("Dumping child to {:?}", tempf);
    let mut w = LimitedWriter::new(
        tempf,
        (*max_processed_size).min(config.max_child_output_size),
    );
    match std::io::copy(&mut r, &mut w) {
        Ok(len) => {
            *max_processed_size -= len;
            Ok(BackendResultChild {
                path: Some(
                    w.into_inner()
                        .into_temp_path()
                        .keep()
                        .map_err(|e| {
                            io::Error::other(format!("Failed to preserve temporary file: {e}"))
                        })?
                        .into_os_string()
                        .into_string()
                        .map_err(|s| {
                            io::Error::other(format!("Failed to convert OsString {s:?} to String"))
                        })?,
                ),
                force_type: None,
                symbols: Vec::new(),
                relation_metadata: Metadata::new(),
            })
        }
        Err(ioerr) => {
            if ioerr
                .get_ref()
                .is_some_and(|e| e.is::<WriteLimitExceededError>())
            {
                // Limits
                *limits_reached = true;
                Ok(BackendResultChild {
                    path: None,
                    force_type: None,
                    symbols: vec!["TOOBIG".to_string()],
                    relation_metadata: Metadata::new(),
                })
            } else {
                match ioerr.kind() {
                    // Format error
                    io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof => {
                        Ok(BackendResultChild {
                            path: None,
                            force_type: None,
                            symbols: vec!["CORRUPTED".to_string()],
                            relation_metadata: Metadata::new(),
                        })
                    }
                    _ => {
                        // Transient IO error
                        Err(ioerr)
                    }
                }
            }
        }
    }
}

#[derive(Serialize)]
struct MessageMetadata {
    headers: Vec<(String, String)>,
    from: Option<String>,
    recipients: Vec<MessageRecipient>,
    n_attachments: usize,
    has_text_body: bool,
    has_rtf_body: bool,
    has_html_body: bool,
    is_embedded: bool,
    attachments_by_ref: Vec<String>,
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
    let max_children = usize::try_from(config.max_children).unwrap_or(usize::MAX);
    let mut input_file = std::fs::File::open(&input_name)?;
    let ole = Ole::new(&mut input_file)?;
    let msg = Msg::new(&ole)?;
    let msg = if let Some(serde_json::Value::String(base)) =
        request.relation_metadata.get("_msg_substream")
    {
        let embmsg = EmbMsg::new(&ole, base, &msg.property_map)?;
        GenericMessage::Embedded(embmsg)
    } else {
        GenericMessage::Main(msg)
    };

    let mut headers: Vec<(String, String)> = Vec::new();
    if let Some(header_iter) = msg.headers() {
        for (k, v) in &header_iter {
            let klc = k.to_lowercase();
            if [
                "bcc",
                "cc",
                "envelope-to",
                "from",
                "in-reply-to",
                "message-id",
                "reply-to",
                "return-path",
                "subject",
                "to",
            ]
            .contains(&klc.as_str())
            {
                headers.push((klc, v.to_string()));
            }
        }
    }
    let from = msg.sender_email().map(|email| {
        if let Some(name) = msg.sender_name() {
            format!("{name} <{email}>")
        } else {
            format!("<{email}>")
        }
    });
    let recipients: Vec<MessageRecipient> = msg.get_recipients().iter().map(|r| r.into()).collect();
    let mut children: Vec<BackendResultChild> = Vec::new();
    let mut max_size = config.max_processed_size;
    let mut limits_reached = false;
    let has_text_body = if let Some(Ok(stream)) = msg.plain_body() {
        let mut child = save_child(stream, config, &mut max_size, &mut limits_reached)?;
        if child.path.is_some() {
            child.force_type = Some("Text".to_string());
        }
        child.symbols.push("MSG_TEXT_BODY".to_string());
        children.push(child);
        true
    } else {
        false
    };
    let has_rtf_body = if let Some(Ok(mut stream)) = msg.rtf_body() {
        let mut child = save_child(&mut stream, config, &mut max_size, &mut limits_reached)?;
        if child.path.is_some() {
            child.symbols.push("MSG_RTF_BODY".to_string());
        }
        if !stream.has_valid_crc() {
            child.symbols.push("CRTF_CRC_MISMATCH".to_string());
        }
        children.push(child);
        true
    } else {
        false
    };
    let has_html_body = if let Some(Ok(stream)) = msg.html_body() {
        let mut child = save_child(stream, config, &mut max_size, &mut limits_reached)?;
        child.symbols.push("MSG_HTML_BODY".to_string());
        children.push(child);
        true
    } else {
        false
    };

    let mut n_attachments = 0usize;
    let mut attachments_by_ref: Vec<String> = Vec::new();
    let mut symbols = Vec::new();
    for attm in msg.get_attachments().iter() {
        if limits_reached || children.len() >= max_children {
            limits_reached = true;
            break;
        }
        match attm.properties.as_int("AttachMethod") {
            Some(0) => {
                // No attachment: just created, empty
            }
            Some(1) => {
                // Actual attachment: stream in AttachDataBinary prop
                let name = attm.name();
                let mime_type = attm.mime_type();
                debug!("Processing attachment \"{:?}\" ({:?})", name, mime_type);
                if let Some(Ok(stream)) = attm.get_binary_stream() {
                    let mut child = save_child(stream, config, &mut max_size, &mut limits_reached)?;
                    child
                        .relation_metadata
                        .insert("name".to_string(), name.into());
                    child
                        .relation_metadata
                        .insert("mime_type".to_string(), mime_type.into());
                    children.push(child);
                }
                n_attachments += 1;
            }
            Some(2) | Some(3) | Some(4) => {
                // 2 => Attach by reference
                // 3 => Attach by reference resolve
                // 4 => Attach by reference only
                if let Some(Ok(path)) = attm
                    .properties
                    .read_string("AttachLongPathname")
                    .or_else(|| attm.properties.read_string("AttachPathname"))
                {
                    attachments_by_ref.push(path)
                }
            }
            Some(5) => {
                // Embedded MSG: AttachDataObject contains the nested MSG
                // The Property Map is still inside the outermost MSG
                let name = attm.name();
                let mime_type = attm.mime_type();
                if attm
                    .get_embedded_message(&ole, msg.get_property_map())
                    .is_ok()
                {
                    let input_file_newfd = std::fs::File::open(&input_name)?;
                    let mut child =
                        save_child(input_file_newfd, config, &mut max_size, &mut limits_reached)?;
                    child.relation_metadata.insert(
                        "_msg_substream".to_string(),
                        attm.properties.base.as_str().into(),
                    );
                    child
                        .relation_metadata
                        .insert("name".to_string(), name.into());
                    child
                        .relation_metadata
                        .insert("mime_type".to_string(), mime_type.into());
                    children.push(child);
                }
            }
            Some(6) => {
                // Embedded OLE: application defined format
                // Note: could possibly be handled the same as 1
                // Maybe AttachDataObject instead of AttachDataBinary ?
            }
            Some(7) => {
                // Web reference
                // AttachmentProviderType indicates the web service API and AttachLongPathname
                // the path to the resource
            }
            Some(v) => {
                debug!("Unsupported / invalid AttachMethod {}", v);
            }
            _ => {}
        }
    }
    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }
    let object_metadata = match serde_json::to_value(MessageMetadata {
        headers,
        from,
        recipients,
        n_attachments,
        has_text_body,
        has_rtf_body,
        has_html_body,
        attachments_by_ref,
        is_embedded: matches!(msg, GenericMessage::Embedded(_)),
    })? {
        serde_json::Value::Object(v) => v,
        _ => unreachable!(),
    };
    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata,
        children,
    }))
}
