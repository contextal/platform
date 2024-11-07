//! Facilities for reading runtime configuration values
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;
use shared::config::{BrokerConfig, DBConfig};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

#[derive(Deserialize)]
/// Grapher configuration
pub struct Config {
    /// Message broker configuration
    pub broker: BrokerConfig,
    /// GraphDB configuration
    pub write_db: DBConfig,
}

impl Config {
    /// Loads the configuration from `grapher.toml` and env
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Figment::new()
            .merge(Toml::file("grapher.toml"))
            .merge(Env::prefixed("GRAPHER__").split("__"))
            .extract::<Self>()
            .map_err(|err| {
                error!("Failed to validate configuration: {}", err);
                err.into()
            })
    }
}
