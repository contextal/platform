#![warn(missing_docs)]
//! # backend-utils #
//!
//! Common facilities and data structures for backend development
//!
//! # Backend main() function example #
//! ```rust,ignore
//! use tracing_subscriber::prelude::*;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Enable env based logging
//!     tracing_subscriber::registry()
//!         .with(tracing_subscriber::fmt::layer())
//!         .with(tracing_subscriber::EnvFilter::from_default_env())
//!         .init();
//!     // Grab your config
//!     let config = Config::get_my_config()?;
//!     // Enter the work request loop
//!     backend_utils::work_loop!(None, None, |request| do_work(&config, request))?;
//!     unreachable!()
//! }
//! ```

pub mod objects;
pub mod tcpserver;

use std::fmt::{Debug, Display};
use tracing::{debug, warn};

/// A convenience macro which accepts optional `host`, optional `port` and `Fn`, adds backend crate
/// version (extracted from environment variable at compile time) and passes it all to
/// [`work_loop_with_backend_version`].
#[macro_export]
macro_rules! work_loop {
    ($host:expr, $port:expr, $fn:expr) => {{
        // As macro code is generated at the call-site the environment variable would be of a
        // caller, i.e. backend-crate build process.
        //
        // If environment variable `CARGO_PKG_VERSION` is not available this will panic at compile
        // time. And this could only happen if something other than Cargo (e.g. direct invocation
        // of `rustc`) is used as a build tool.
        let backend_version = env!("CARGO_PKG_VERSION");
        $crate::work_loop_with_backend_version($host, $port, backend_version, $fn)
    }};
}

/// The main backend<->frontend communication handler
///
/// Convenience function that:
/// - creates a listening TCP socket on the specified host/port
/// - enters the backend (infinite) loop:
///   - receives work requests from the frontend
///   - dispatches the request over to the specified work_fn
///   - receives the result
///   - appends `backend_version` argument value to `object_metadata` if the result is of
///     `BackendResultKind::ok(BackendResultOk)` kind
///   - transmits back the result to the frontend
///
/// # Parameters #
/// - `host`: the frontend host to connect to (address or name)
/// - `port`: the frontend TCP port to connect to
/// - `backend_version`: the backend crate version (see [`work_loop!`] macro which can
///   extract/provide this value by itself)
/// - `work_fn`: the worker function
///
/// # Notes #
/// - For production use pass `None` for both `host` and `port`
/// - The worker function takes a single parameter (the work request); if you need
///   to pass extra parameters to it, wrap it in a closure. See the example in the
///   [crate level documentation](crate)
pub fn work_loop_with_backend_version<W, E>(
    host: Option<&str>,
    port: Option<u16>,
    backend_version: &str,
    work_fn: W,
) -> Result<(), std::io::Error>
where
    W: Fn(&objects::BackendRequest) -> Result<objects::BackendResultKind, E>,
    E: Display + Debug,
{
    let host = host.unwrap_or("127.0.0.1");
    let port = port.unwrap_or(44203);
    let mut server = tcpserver::TcpServer::new(host, port)?;
    loop {
        let request = server.get_job_request();
        debug!("Job request received: {request:?}");

        let result = work_fn(&request);
        debug!("Job result generated: {result:?}");

        match result {
            Ok(mut result) => {
                if let objects::BackendResultKind::ok(ref mut v) = result {
                    // Use an underscore key prefix to indicate flow-injected/non-ordinary kind of
                    // key/value pair.
                    v.object_metadata
                        .insert("_backend_version".into(), backend_version.into());
                }

                match server.send_job_result(&result) {
                    Ok(_) => debug!("Job result sent to frontend"),
                    Err(e) => warn!("Failed to send job result to frontend: {e}"),
                }
            }
            Err(e) => {
                debug!("Job failure sent to frontend: {e}");
                server.send_job_failure();
            }
        }
    }
}
