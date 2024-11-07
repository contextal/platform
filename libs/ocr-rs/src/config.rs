//! Facilities for reading runtime configuration values
use anyhow::Context;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;

/// Worker backend configuration
#[derive(Debug, Deserialize)]
pub struct Config {
    /// The hostname to bind to
    #[serde(default = "Config::default_host")]
    pub host: String,
    /// The port to bind to
    #[serde(default = "Config::default_port")]
    pub port: u16,
    /// Maximum allowed image size in pixels (result of `width` * `height` multiplication)
    pub max_input_size_pixels: u32,
    /// Maximum allowed image size in bytes
    pub max_input_size_bytes: u32,
    /// The path to the objects store
    pub objects_path: String,
    /// Output path
    pub output_path: String,
}

impl Config {
    /// Constructs `Config` from a `toml` file and environment variables
    pub fn new() -> anyhow::Result<Self> {
        let config: Self = Figment::new()
            .merge(Toml::file("backend.toml"))
            .merge(Env::prefixed("BACKEND__").split("__"))
            .extract()
            .context("failed to deserialize a configuration")?;

        macro_rules! disallow_max_value_of_type {
            ($type:ty; $parent:ident.$var:ident) => {
                if $parent.$var == <$type>::MAX {
                    Err(anyhow::anyhow!(
                        "Value of `{}` is too large (must be strictly below {})",
                        stringify!($var),
                        <$type>::MAX
                    ))
                    .with_context(|| format!("value of `{}` is too large", stringify!($var)))?
                }
            };
        }

        disallow_max_value_of_type!(u32; config.max_input_size_pixels);
        disallow_max_value_of_type!(u32; config.max_input_size_bytes);

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
