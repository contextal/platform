//! Facilities for reading runtime configuration values
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;
use shared::config::{BrokerConfig, ClamdServiceConfig};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

#[derive(Deserialize)]
/// Worker frontend configuration
pub struct Config {
    #[cfg(feature = "backend")]
    /// The object type served by this worker
    worker_type: String,
    /// Message broker configuration
    pub broker: BrokerConfig,
    /// The clamd symbols scanner
    pub clamd: ClamdServiceConfig,
    #[cfg(feature = "backend")]
    /// Object type detection server
    pub typedet: ClamdServiceConfig,
    #[cfg(feature = "backend")]
    /// The backend options
    pub backend: BackendConfig,
    /// The path to the objects store
    pub objects_path: String,
}

#[cfg(feature = "backend")]
#[derive(Deserialize)]
/// Backend configuration
pub struct BackendConfig {
    /// The path to the worker binary
    pub path: String,
    /// The optional arguments to pass to the worker
    pub args: Option<Vec<String>>,
    /// The port to connect to
    pub port: u16,
}

impl Config {
    /// Loads the configuration from a `frontend.toml` file and env
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Figment::new()
            .merge(Toml::file("frontend.toml"))
            .merge(Env::prefixed("FRONTEND__").split("__"))
            .extract::<Self>()
            .map_err(|err| {
                error!("Failed to validate configuration: {}", err);
                err.into()
            })
    }

    pub fn get_worker_type(&self) -> &str {
        #[cfg(feature = "backend")]
        {
            &self.worker_type
        }
        #[cfg(not(feature = "backend"))]
        {
            "UNKNOWN"
        }
    }
}
