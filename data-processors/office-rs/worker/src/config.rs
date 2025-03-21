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
    /// Output path
    pub output_path: String,
    /// Maximum number of generated childred
    pub max_children: u32,
    /// Overal size of generated children
    pub max_processed_size: u64,
    /// Single child size limit
    pub max_child_output_size: u64,
    /// Single sheet size limit
    pub sheet_size_limit: u64,
    /// Maximum shared strings xml file size allowed to be cached in memory
    pub shared_strings_cache_limit: u64,
    /// Whether to create URL children
    pub create_domain_children: bool,
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
