//! Facilities for reading runtime configuration values
use crate::TextBackendError;
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;
use tracing::trace;

/// Worker backend configuration.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// The path to the objects store.
    pub objects_path: String,

    /// Output path.
    pub output_path: String,

    /// Maximum allowed input file size in bytes.
    pub max_processed_size: u64,

    /// Maximum bumber of children to create
    pub max_children: u32,

    /// Maximum number_of_characters / number_of_whitespaces ratio to consider
    /// for running the natural language detection.
    pub natural_language_max_char_whitespace_ratio: f64,

    /// Minimum natural language confidence level to report. From 0.0 to 1.0.
    pub natural_language_min_confidence_level: f64,

    /// Whether to create URL children (currently only for OCR'd text)
    pub create_url_children: bool,

    /// Whether to create Domain children
    pub create_domain_children: bool,
}

impl Config {
    /// Constructs `Config` from a `toml` file and environment variables
    pub fn new() -> Result<Self, TextBackendError> {
        let config: Self = Figment::new()
            .merge(Toml::file("backend.toml"))
            .merge(Env::prefixed("BACKEND__").split("__"))
            .extract()?;

        macro_rules! disallow_value_below {
            ($parent:ident.$var:ident, $limit:expr) => {
                if $parent.$var < $limit {
                    Err(TextBackendError::ConfigParameterValue {
                        parameter: stringify!($var),
                        message: format!(
                            "parameter value should be equal or larger than {}",
                            $limit
                        ),
                    })?
                }
            };
        }
        disallow_value_below!(config.natural_language_max_char_whitespace_ratio, 0.0);
        disallow_value_below!(config.natural_language_min_confidence_level, 0.0);

        macro_rules! disallow_value_above {
            ($parent:ident.$var:ident, $limit:expr) => {
                if $parent.$var > $limit as _ {
                    Err(TextBackendError::ConfigParameterValue {
                        parameter: stringify!($var),
                        message: format!("parameter value should be equal or less than {}", $limit),
                    })?
                }
            };
        }
        disallow_value_above!(config.max_processed_size, u64::MAX - 1);
        disallow_value_above!(config.natural_language_min_confidence_level, 1.0);

        trace!("final config: {config:#?}");

        Ok(config)
    }
}
