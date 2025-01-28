//! Interface to the backend worker and related structures
//!
//! The worker is spawned and monitored: if it exits or dies, this program
//! will also exit; vice versa if this program exits or is killed, the
//! backend worker will also be killed
//!
//! The backend standard output and error is collected and logged through
//! this program
use crate::config::Config;
use serde::{Deserialize, Serialize};
use shared::object;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

/// The JSON struct passed to the backend
#[derive(Serialize)]
struct BackendRequest<'a> {
    object: &'a object::Info,
    symbols: &'a Vec<String>,
    relation_metadata: &'a object::Metadata,
}

/// The JSON result produced by the worker
#[derive(Debug, Deserialize)]
pub struct BackendResult {
    #[serde(flatten)]
    pub result: BackendResultKind,
}

/// The ok/error portion of the result
#[derive(Debug, Deserialize)]
#[allow(non_camel_case_types)]
pub enum BackendResultKind {
    error(String),
    ok(BackendResultOk),
}

/// The ok portion of the result
#[derive(Debug, Deserialize)]
pub struct BackendResultOk {
    pub symbols: Vec<String>,
    pub object_metadata: object::Metadata,
    pub children: Vec<BackendResultChild>,
}

/// A child object extracted by the backend
#[derive(Debug, Deserialize)]
pub struct BackendResultChild {
    pub path: Option<String>,
    pub force_type: Option<String>,
    pub symbols: Vec<String>,
    pub relation_metadata: object::Metadata,
}

#[cfg(feature = "backend")]
pub mod inner {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    /// A backend worker
    pub struct Backend {
        process: tokio::process::Child,
        addr: String,
    }

    impl Backend {
        /// Creates a new interface to the backend worker
        pub fn new(config: &Config) -> Result<Self, Box<dyn std::error::Error>> {
            // Spawn backend
            let mut command = tokio::process::Command::new(&config.backend.path);
            let command = command
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            if let Some(args) = &config.backend.args {
                command.args(args);
            }
            let mut process = command.spawn().map_err(|e| {
                error!("Failed to spawn backend process: {}", e);
                e
            })?;

            // Divert stdio
            let stdout = process.stdout.take().unwrap();
            let stderr = process.stderr.take().unwrap();
            tokio::spawn(async move {
                let mut reader = tokio::io::BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    debug!("Backend(out): {}", line);
                }
            });
            tokio::spawn(async move {
                let mut reader = tokio::io::BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    debug!("Backend(err): {}", line);
                }
            });
            info!(
                "Successfully spawned backend \"{}\" listening on port {}",
                config.backend.path, config.backend.port
            );
            Ok(Self {
                process,
                addr: format!("localhost:{}", config.backend.port),
            })
        }

        /// Sends a JSON job request to the backend; awaits and return the result
        pub async fn invoke(
            &self,
            object_descriptor: &object::Descriptor,
        ) -> Result<(BackendResult, f64), Box<dyn std::error::Error>> {
            let object = &object_descriptor.info;
            let symbols = &object_descriptor.symbols;
            let relation_metadata = &object_descriptor.relation_metadata;
            let max_recursion = object_descriptor.max_recursion;
            if object.recursion_level >= max_recursion {
                info!("Max recursion level ({max_recursion}) reached, backend not invoked");
                return Ok((
                    BackendResult {
                        result: BackendResultKind::ok(BackendResultOk {
                            symbols: vec!["TOODEEP".to_string()],
                            object_metadata: object::Metadata::new(),
                            children: Vec::new(),
                        }),
                    },
                    f64::NAN,
                ));
            }
            debug!(
                "Backend invoked at recursion level {}/{}",
                object.recursion_level, max_recursion
            );
            let start = std::time::Instant::now();
            let reply = async {
                let mut stream = tokio::net::TcpStream::connect(&self.addr).await?;
                let req = BackendRequest {
                    object,
                    symbols,
                    relation_metadata,
                };
                let req_json = serde_json::to_string(&req).unwrap();
                stream.write_all(req_json.as_bytes()).await?;
                stream.shutdown().await?;
                let reply = shared::utils::read_all(&mut stream).await?;
                Ok::<Vec<u8>, Box<dyn std::error::Error>>(reply)
            }
            .await
            .map_err(|e| {
                // Communication failure is a HARD error
                error!("Communication with the backend failed: {e}");
                e
            })?;
            // Note: there's a little extra work here to ensure preference to the 'error'
            // path in case the reply object is ambiguous and contains both the 'ok' and
            // 'error' keys
            let mut res: serde_json::Value = serde_json::from_slice(&reply).map_err(|e| {
                // This is a HARD error
                error!("Invalid JSON reply received from backend: {e}");
                e
            })?;
            // After this point the reply is valid JSON
            if res.get("error").is_some() && res.get("ok").is_some() {
                warn!("Backend returned 'error' and 'ok' and at the same time, assuming error");
                res.as_object_mut().unwrap().remove("ok");
            }
            let res: BackendResult = serde_json::from_value(res).map_err(|e| {
                // Backend returned an invalid reply
                // This is a HARD error
                error!("Invalid reply received from backend: {e}");
                e
            })?;
            debug!("Backend result received: {:#?}", res);
            match &res.result {
                BackendResultKind::ok(r) => {
                    info!(
                        "Backend produced {} symbols and {} children",
                        r.symbols.len(),
                        r.children.len(),
                    );
                }
                BackendResultKind::error(ref e) => {
                    info!("Backend failed: {e}")
                }
            }
            Ok((res, start.elapsed().as_secs_f64()))
        }

        /// Waits indefinitely for the backend to exit
        pub async fn wait(&mut self) {
            let status = self.process.wait().await;
            match status {
                Ok(exit_code) => error!("Backend has exited with {}", exit_code),
                Err(e) => error!("Failed to check backend status: {}", e),
            }
        }

        /// Checks if backend is working
        pub async fn is_working(&self) -> bool {
            let reply = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                let mut stream = tokio::net::TcpStream::connect(&self.addr).await?;
                stream.write_all(b"{}").await?;
                stream.shutdown().await?;
                let reply = shared::utils::read_all(&mut stream).await?;
                Ok::<Vec<u8>, Box<dyn std::error::Error>>(reply)
            })
            .await;
            reply.is_ok()
        }
    }
}

#[cfg(not(feature = "backend"))]
pub mod inner {
    use super::*;

    /// A backend worker
    pub struct Backend;

    impl Backend {
        /// Creates a new interface to the backend worker
        #[inline]
        pub fn new(_config: &Config) -> Result<Self, Box<dyn std::error::Error>> {
            Ok(Self)
        }

        /// Sends a JSON job request to the backend; awaits and return the result
        #[inline]
        pub async fn invoke(
            &self,
            _object_descriptor: &object::Descriptor,
        ) -> Result<(BackendResult, f64), Box<dyn std::error::Error>> {
            Ok((
                BackendResult {
                    result: BackendResultKind::ok(BackendResultOk {
                        symbols: Vec::new(),
                        object_metadata: object::Metadata::new(),
                        children: Vec::new(),
                    }),
                },
                f64::NAN,
            ))
        }

        /// Waits indefinitely for the backend to exit
        #[inline]
        pub async fn wait(&mut self) -> ! {
            loop {
                tokio::time::sleep(tokio::time::Duration::MAX).await;
            }
        }

        /// Checks if backend is working
        #[inline]
        pub async fn is_working(&self) -> bool {
            return true;
        }
    }
}

pub use inner::*;
