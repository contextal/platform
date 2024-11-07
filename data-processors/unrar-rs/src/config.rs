//! Facilities for reading runtime configuration values
use crate::RarBackendError;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;

/// Worker backend configuration.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// The path to the objects store.
    pub objects_path: String,

    /// Output path.
    pub output_path: String,

    /// Maximum number of files to extract.
    pub max_children: u32,

    /// Maximum number of archive entries (files and directories) to process.
    ///
    /// This is different from `max_children`, as for example entries of directory type don't cause
    /// creation of `BackendResultChild`.
    pub max_entries_to_process: u32,

    /// Maximum compressed archive entry size to extract.
    pub max_child_input_size: u64,

    /// Maximum decompressed archive entry size (according to archive headers) to extract.
    pub max_child_output_size: u64,

    /// Maximum overall compressed size to process.
    pub max_processed_size: u64,
}

impl Config {
    /// Constructs new `Config` from a `toml` file and environment variables.
    pub fn new() -> Result<Self, RarBackendError> {
        let config: Self = Figment::new()
            .merge(Toml::file("backend.toml"))
            .merge(Env::prefixed("BACKEND__").split("__"))
            .extract()?;

        macro_rules! check_upper_bound {
            ($parent:ident.$var:ident, $limit:expr) => {
                if $parent.$var > $limit {
                    Err(RarBackendError::ConfigParameterValue {
                        parameter: stringify!($var),
                        message: format!("value is too large (must be strictly below {})", $limit),
                    })?
                }
            };
        }

        check_upper_bound!(config.max_child_output_size, i64::MAX as u64);
        check_upper_bound!(config.max_child_input_size, i64::MAX as u64);
        check_upper_bound!(config.max_processed_size, i64::MAX as u64);

        Ok(config)
    }
}
