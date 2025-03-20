use crate::{BackendResultSymbols, ChildType, Guid, Url, config::Config, error::UrlBackendError};
use backend_utils::objects::BackendResultChild;
use chromiumoxide::cdp::browser_protocol::browser::DownloadProgressState;
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tempfile::NamedTempFile;
use tokio::fs;
use tracing::warn;

/// An entity from browser "Downloads" menu.
#[derive(Debug, Serialize)]
pub struct FileDownload {
    /// Download source URL.
    pub url: Url,

    /// Filename suggested by the browser.
    pub suggested_filename: String,

    /// Globally unique identifier assigned to a "download" instace. Depending on download
    /// directory settings GUID might be used as a download destination filename.
    #[serde(skip_serializing)]
    pub guid: Guid,

    /// Current download state, like "InProgress", "Canceled", "Completed".
    #[serde(skip_serializing)]
    pub state: DownloadProgressState,

    /// Backend config. Necessary for construction of BackendResultChild.
    #[serde(skip_serializing)]
    pub config: Arc<Config>,
}

impl FileDownload {
    pub async fn consume(
        self,
    ) -> Result<(BackendResultChild, BackendResultSymbols), UrlBackendError> {
        let symbols = BackendResultSymbols::new();
        let mut childen_symbols = vec![];
        let final_location_used_by_browser =
            PathBuf::from(&self.config.output_path).join(&*self.guid);

        let path = match self.state {
            DownloadProgressState::Canceled => None,
            DownloadProgressState::InProgress => {
                warn!("incomplete download has not been canceled yet");
                childen_symbols.push("FETCH_INCOMPLETE".into());

                let temporary_location_used_by_browser = PathBuf::from(&self.config.output_path)
                    .join(format!("{}.crdownload", &*self.guid));
                let _ = fs::remove_file(final_location_used_by_browser).await;
                let _ = fs::remove_file(temporary_location_used_by_browser).await;

                None
            }
            DownloadProgressState::Completed => {
                let output_file = {
                    let config = self.config.clone();
                    let suggested_filename = self.suggested_filename.clone();
                    tokio::task::spawn_blocking(move || {
                        if config.random_filenames {
                            NamedTempFile::new_in(&config.output_path)
                        } else {
                            const MAX_FILENAME_SUFFIX_LEN: usize = 32;
                            let suffix: String = suggested_filename
                                .chars()
                                .filter(|&v| v.is_ascii_alphanumeric() || v == '.' || v == '_')
                                .take(MAX_FILENAME_SUFFIX_LEN)
                                .collect();

                            tempfile::Builder::new()
                                .prefix("download_")
                                .suffix(&format!("_{suffix}"))
                                .tempfile_in(&config.output_path)
                        }
                    })
                    .await??
                };
                fs::rename(&final_location_used_by_browser, &output_file).await?;

                Some(
                    output_file
                        .into_temp_path()
                        .keep()?
                        .into_os_string()
                        .into_string()
                        .map_err(UrlBackendError::Utf8)?,
                )
            }
        };

        Ok((
            BackendResultChild {
                path,
                symbols: childen_symbols,
                relation_metadata: match serde_json::to_value(ChildType::FileDownload(self))? {
                    serde_json::Value::Object(v) => v,
                    _ => unreachable!(),
                },
                force_type: None,
            },
            symbols,
        ))
    }
}
