//! Facilities for reading runtime configuration values
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;
use tracing::*;

#[derive(Deserialize)]
/// Worker backend configuration
pub struct Config {
    /// The hostname to bind to
    pub host: Option<String>,
    /// The port to bind to
    pub port: Option<u16>,
    /// The path to the objects store
    pub objects_path: String,
}

impl Config {
    /// Loads the configuration from a `toml` file
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config: Self = Figment::new()
            .merge(Toml::file("backend.toml"))
            .merge(Env::prefixed("BACKEND__").split("__"))
            .extract()
            .map_err(|err| {
                error!("Failed to validate configuration: {}", err);
                err
            })?;
        Ok(config)
    }
}
