//! Facilities for reading runtime configuration values
use crate::error::UrlBackendError;
use chromiumoxide::cdp::browser_protocol::network::ResourceType;
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

    /// User agent string to use while performing HTTP requests.
    pub user_agent: String,

    /// Optional accept language string to use while performing HTTP requests.
    pub accept_language: Option<String>,

    /// Browser window size.
    pub window_size: (u32, u32),

    /// Chromium's request timeout in milliseconds.
    /// The duration after a request with no response should time out.
    pub chrome_request_timeout_msec: u32,

    /// Whether to use totally random file names for child objects (default), or add suffixes,
    /// indexes and prefixes to produced filenames to make debugging/development more convenient.
    #[serde(default = "Config::default_random_filenames")]
    pub random_filenames: bool,

    /// Optional proxy server.
    pub proxy: Option<String>,

    /// Maximum number of backend requests to process before recycling the browser.
    pub max_backend_requests_per_instance: u32,

    /// Maximum interval of time in seconds for browser instance to run before being recycled by
    /// the backend.
    pub max_instance_lifetime_seconds: u32,

    /// An interval of time in milliseconds which must pass with no-requests-in-progress state
    /// before considering that page is fully loaded. Or in other words to workaround "network is
    /// idle -> busy -> idle" flops which might cause a navigation request to be prematurely
    /// considered as fully loaded.
    ///
    /// The higher this interval the higher probability that all elements on the page are fully
    /// loaded. On the other hand every backend request would have additional delay to ensure that
    /// there are no more network requests to fulfill before considering the page as fully loaded.
    pub idle_network_settle_time_msec: u32,

    /// Specifies whether to take a screenshot and produce corresponding artifact after navigated
    /// to URL from a request.
    pub take_screenshot: bool,

    /// Specifies whether to perform print-to-PDF and produce corresponding artifact after
    /// navigated to URL from a request.
    pub perform_print_to_pdf: bool,

    /// Specifies whether to save original HTTP-response-document and produce
    /// corresponding artifact.
    ///
    /// HTML document in the browser window is often differs from original HTML document sent by
    /// the web server in a reply to original HTTP request. The reason for this is usually
    /// JavaScript code executed by a browser, which adds, modifies, updates original HTML
    /// document.
    /// From the analysis perspective actual HTML page rendered in a browser window is much more
    /// valuable comparing to original HTML page, as it represents what web-site-user actually
    /// sees.
    pub save_original_response: bool,

    /// Maximum HTTP response body size in bytes (i.e. HTTP content-length) to allow browser to
    /// fetch. HTTP response body could be compressed, so the limit applies to the body before
    /// decompression and decompressed body size could be larger then the specified limit.
    /// While this parameter allows to filter some responses before response body download begins,
    /// not all responses are subject for this limit, as content-length HTTP header is optional.
    pub max_response_content_length: u64,

    /// Maximum allowed HTTP response data size in bytes (after HTTP transfer-encoding
    /// decompression).
    /// This limit is applied when response body is being received. When received response body
    /// grows over the specified limit the response is interrupted.
    pub max_response_data_length: u64,

    /// An optional list of resource types, which shouldn't be further processed
    #[serde(default = "Vec::new")]
    pub excluded_resource_types: Vec<ResourceType>,

    /// Maximum number of children objects to create (processing halts if reached)
    pub max_children: usize,

    /// Single object limit (the part is skipped if size is exceeded)
    pub max_child_output_size: u64,
}

impl Config {
    /// Constructs `Config` from a `toml` file and environment variables
    pub fn new() -> Result<Self, UrlBackendError> {
        let config: Self = Figment::new()
            .merge(Toml::file("backend.toml"))
            .merge(Env::prefixed("BACKEND__").split("__"))
            .extract()?;

        macro_rules! check_lower_bound {
            ($parent:ident.$var:ident, $limit:expr) => {
                if $parent.$var < $limit {
                    Err(UrlBackendError::ConfigParameterValue {
                        parameter: stringify!($var),
                        message: format!(
                            "parameter value should be equal or larger than {}",
                            $limit
                        ),
                    })?
                }
            };
        }
        check_lower_bound!(config.max_instance_lifetime_seconds, 1);
        check_lower_bound!(config.max_backend_requests_per_instance, 1);

        macro_rules! check_higher_bound {
            ($parent:ident.$var:ident, $limit:expr) => {
                if $parent.$var >= $limit as _ {
                    Err(UrlBackendError::ConfigParameterValue {
                        parameter: stringify!($var),
                        message: format!("parameter value should be less than {}", $limit),
                    })?
                }
            };
        }
        check_higher_bound!(config.max_response_content_length, i64::MAX);
        check_higher_bound!(config.max_response_data_length, i64::MAX);

        trace!("final config: {config:#?}");

        Ok(config)
    }

    /// Returns a default value for `random_filenames` parameter
    fn default_random_filenames() -> bool {
        true
    }
}
