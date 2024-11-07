//! Facilities for reading runtime configuration values
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;
use shared::config::{BrokerConfig, ClamdServiceConfig, DBConfig};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

#[derive(Deserialize)]
/// API endpoint configuration
pub struct Config {
    /// HTTPD listening port
    pub port: u16,
    /// Message broker configuration
    pub broker: BrokerConfig,
    /// Object type detection server
    pub typedet: ClamdServiceConfig,
    /// The path to the objects store
    pub objects_path: String,
    /// Read only DB configuration
    pub read_db: DBConfig,
    /// Read/Write DB configuration
    pub write_db: DBConfig,
    max_submit_size: Option<usize>,
    max_work_results: Option<usize>,
    max_search_results: Option<u32>,
    max_action_results: Option<u32>,
    search_timeout_ms: Option<u32>,
    enable_reprocess: Option<bool>,
}

impl Config {
    /// Loads the configuration from the `endpoint.toml` file and the env
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Figment::new()
            .merge(Toml::file("endpoint.toml"))
            .merge(Env::prefixed("ENDPOINT__").split("__"))
            .extract::<Self>()
            .map_err(|err| {
                error!("Failed to validate configuration: {}", err);
                err.into()
            })
    }

    /// Maximum size of sumbitted object (default 200MiB)
    pub fn get_max_submit_size(&self) -> usize {
        self.max_submit_size.unwrap_or(200 * 1024 * 1024)
    }

    /// Maximum work results per query (default 100)
    pub fn get_max_work_results(&self) -> usize {
        self.max_work_results.unwrap_or(100)
    }

    /// Max search results (default 1000)
    pub fn get_max_search_results(&self) -> u32 {
        self.max_search_results.unwrap_or(1000)
    }

    /// Query time out in ms (default 60000)
    pub fn get_search_timeout_ms(&self) -> u32 {
        self.search_timeout_ms.unwrap_or(60000)
    }

    /// Query time out in ms (default 60000)
    pub fn is_reprocess_enabled(&self) -> bool {
        self.enable_reprocess.unwrap_or(false)
    }

    /// Max action results (default 100)
    pub fn get_max_action_results(&self) -> u32 {
        self.max_action_results.unwrap_or(100)
    }
}
