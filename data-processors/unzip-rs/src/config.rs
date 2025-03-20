//! Facilities for reading runtime configuration values
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn};

/// Worker backend configuration
#[derive(Debug, Deserialize)]
pub struct Config {
    /// The hostname to bind to
    #[serde(default = "Config::default_host")]
    pub host: String,
    /// The port to bind to
    #[serde(default = "Config::default_port")]
    pub port: u16,
    /// Maximum number of parts to extract (processing halts if reached)
    pub max_children: u32,
    /// Overall compressed size limit (processing halts if reached)
    pub max_processed_size: u64,
    /// Single object compressed limit (the part is skipped if size is exceeded)
    pub max_child_input_size: u64,
    /// Single object decompressed limit (the part is skipped if size is exceeded)
    pub max_child_output_size: u64,
    /// The path to the objects store
    pub objects_path: String,
    /// Output path
    pub output_path: String,
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
        if config.max_child_input_size > i64::MAX as u64 {
            error!(
                "Value of max_child_input_size too large (must be strictly < {})",
                i64::MAX
            );
            return Err("Value of max_child_input_size too large".into());
        }

        macro_rules! disallow_max_value_of_type {
            ($type:ty; $parent:ident.$var:ident) => {
                if $parent.$var > <$type>::MAX.try_into().unwrap() {
                    return Err(format!(
                        "Value of `{}` is too large (must be less or equal than {})",
                        stringify!($var),
                        <$type>::MAX
                    )
                    .into());
                }
            };
        }

        disallow_max_value_of_type!(i64; config.max_processed_size);
        disallow_max_value_of_type!(i64; config.max_child_input_size);
        disallow_max_value_of_type!(i64; config.max_child_output_size);

        Ok(config)
    }

    /// Returns a default host value
    fn default_host() -> String {
        "127.0.0.1".to_string()
    }

    /// Returns a default port value
    fn default_port() -> u16 {
        44203
    }
}
