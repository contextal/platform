use crate::{BackendResultSymbols, ChildType, Url, config::Config, error::UrlBackendError};
use backend_utils::objects::BackendResultChild;
use chromiumoxide::cdp::browser_protocol::{
    fetch,
    network::{BlockedReason, ResourceType},
};
use data_url;
use serde::Serialize;
use std::io::{Seek, SeekFrom};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::fs;

/// The reason of backend-initiated request/response interruption.
#[derive(Debug, Clone, Copy)]
pub enum InterruptionReason {
    /// Content-length HTTP header is available and the value is higher than the specified limit.
    ContentLength,

    /// Response body size is larger that the specified limit.
    ResponseSize,
}

/// Intercepted HTTP response.
#[derive(Debug, Serialize)]
pub struct HttpResponse {
    /// HTTP request URL.
    pub url: Url,

    /// Optional preceding redirecting URLs in chronological order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub redirected_from: Vec<Url>,

    /// Remote IP address as reported by the browser.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_ip_address: Option<String>,

    /// Resource type assigned to an object by the browser.
    pub resource_type: Option<ResourceType>,

    /// HTTP status code in the reply.
    pub status_code: Option<i64>,

    /// HTTP status message.
    pub status_text: Option<String>,

    /// URL resource mime type as reported by the web server.
    pub mime_type: Option<String>,

    /// A place to keep HTTP response body.
    #[serde(skip_serializing)]
    pub body: Option<Vec<u8>>,

    /// An optional error string reported by the browser.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_text: Option<String>,

    /// True if the request/response has been canceled (either browser decided to cancel, or we
    /// asked browser to cancel the request).
    #[serde(skip_serializing)]
    pub canceled: bool,

    /// Backend config. Necessary for construction of `BackendResultChild`.
    #[serde(skip_serializing)]
    pub config: Arc<Config>,

    /// The reason why browser blocked the request, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<BlockedReason>,

    /// The reason of interruption, if the request/response has been interrupted because of backend
    /// limits.
    #[serde(skip_serializing)]
    pub interruption_reason: Option<InterruptionReason>,

    /// Network identifier assigned to the request/response by the browser.
    #[serde(skip_serializing)]
    pub network_id: Option<fetch::RequestId>,
}

#[derive(Debug, Serialize)]
pub struct DataUrl {
    /// Resource type assigned to an object by the browser.
    pub resource_type: Option<ResourceType>,

    /// Mime type from data URL
    pub mime_type: Option<String>,
}

impl HttpResponse {
    pub async fn consume(
        self,
    ) -> Result<(BackendResultChild, BackendResultSymbols), UrlBackendError> {
        let mut symbols = BackendResultSymbols::new();
        let mut child_symbols = vec![];

        let path = match self.body {
            Some(ref body)
                if u64::try_from(body.len())
                    .map(|len| len <= self.config.max_child_output_size)
                    != Ok(true) =>
            {
                symbols.insert("LIMITS_REACHED");
                child_symbols.push("TOOBIG".to_string());
                None
            }
            Some(ref body) => {
                let mut output_file = {
                    let config = self.config.clone();
                    if self.config.random_filenames {
                        tokio::task::spawn_blocking(move || {
                            NamedTempFile::new_in(&config.output_path)
                        })
                    } else {
                        let filename = {
                            let without_args = match self.url.find('?') {
                                Some(v) => &self.url[..v],
                                None => &self.url,
                            }
                            .trim_end_matches('/');
                            match without_args.rfind('/') {
                                Some(v) => {
                                    const MAX_FILENAME_SUFFIX_LEN: usize = 32;
                                    let name: String = without_args[(v + 1)..]
                                        .chars()
                                        .filter(|&v| {
                                            v.is_ascii_alphanumeric() || v == '.' || v == '_'
                                        })
                                        .take(MAX_FILENAME_SUFFIX_LEN)
                                        .collect();
                                    match name.is_empty() {
                                        true => "noname".into(),
                                        false => name,
                                    }
                                }
                                None => "noname".into(),
                            }
                        };
                        let prefix = format!(
                            "request_{:?}_",
                            self.resource_type.as_ref().unwrap_or(&ResourceType::Other)
                        )
                        .to_lowercase();
                        tokio::task::spawn_blocking(move || {
                            tempfile::Builder::new()
                                .prefix(&prefix)
                                .suffix(&format!("_{filename}"))
                                .tempfile_in(&config.output_path)
                        })
                    }
                }
                .await??;
                fs::write(&output_file, body).await?;

                if self.resource_type != Some(ResourceType::Image)
                    && output_file.seek(SeekFrom::End(0))? < 16
                {
                    child_symbols.push("TOOSMALL".into());
                    None
                } else {
                    Some(
                        output_file
                            .into_temp_path()
                            .keep()?
                            .into_os_string()
                            .into_string()
                            .map_err(UrlBackendError::Utf8)?,
                    )
                }
            }
            None => {
                match self.interruption_reason {
                    Some(reason) => {
                        symbols.insert("LIMITS_REACHED");
                        match reason {
                            InterruptionReason::ResponseSize
                            | InterruptionReason::ContentLength => {
                                child_symbols.push("TOOBIG".into())
                            }
                        }
                    }
                    None => match self.error_text {
                        Some(_) => child_symbols.push("FETCH_ERROR".into()),
                        None => {
                            symbols.insert("LIMITS_REACHED");
                            child_symbols.push("TIMEOUT".into());
                        }
                    },
                }

                None
            }
        };

        let relation_metadata = if self.url.starts_with("data:") {
            let mime_type = if let Ok(du) = data_url::DataUrl::process(&self.url) {
                Some(format!(
                    "{}/{}",
                    du.mime_type().type_,
                    du.mime_type().subtype
                ))
            } else {
                None
            };
            let dataurl = DataUrl {
                resource_type: self.resource_type,
                mime_type,
            };
            match serde_json::to_value(ChildType::DataUrl(dataurl))? {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            }
        } else {
            match serde_json::to_value(ChildType::HttpResponse(self))? {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            }
        };

        Ok((
            BackendResultChild {
                path,
                symbols: child_symbols,
                relation_metadata,
                force_type: None,
            },
            symbols,
        ))
    }
}
