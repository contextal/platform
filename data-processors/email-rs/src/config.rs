//! Facilities for reading runtime configuration values
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};

#[derive(Deserialize)]
/// Worker backend configuration
pub struct Config {
    /// The hostname to bind to
    pub host: Option<String>,
    /// The port to bind to
    pub port: Option<u16>,
    /// Single object input limit (the part is skipped if size is exceeded)
    pub max_child_input_size: u64,
    /// Single object output limit (the part is skipped if size is exceeded)
    pub max_child_output_size: u64,
    /// Maximum number of parts to extract (processing halts if reached)
    pub max_children: u32,
    /// Overall size limit (processing halts if reached)
    pub max_processed_size: u64,
    /// The path to the objects store
    pub objects_path: String,
    /// Output path
    pub output_path: String,
}

impl Config {
    /// Loads the configuration from a `toml` file and environment
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config: Self = Figment::new()
            .merge(Toml::file("backend.toml"))
            .merge(Env::prefixed("BACKEND__").split("__"))
            .extract()
            .map_err(|err| {
                error!("Failed to validate configuration: {}", err);
                err
            })?;
        if config.max_child_input_size > i64::MAX as u64 {
            error!(
                "Value of max_child_input_size too large (must be strictly < {})",
                i64::MAX
            );
            return Err("Value of max_child_input_size too large".into());
        }
        if config.max_child_output_size > i64::MAX as u64 {
            error!(
                "Value of max_child_output_size too large (must be strictly < {})",
                i64::MAX
            );
            return Err("Value of max_child_output_size too large".into());
        }
        if config.max_processed_size > i64::MAX as u64 {
            error!(
                "Value of max_processed_size too large (must be strictly < {})",
                i64::MAX
            );
            return Err("Value of max_processed_size too large".into());
        }
        Ok(config)
    }
}
