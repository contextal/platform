//! Job management
mod amqp;
mod backend;
mod metrics;

use crate::config::Config;
use amqp::TimeRemaining;
use backend::BackendResultKind;
use futures::prelude::*;
use shared::{clamd, object};
use tokio::signal::unix;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

/// The work manager
pub struct WorkManager {
    #[cfg(feature = "backend")]
    config: Config,
    clamd: clamd::Clamd,
    #[cfg(feature = "backend")]
    typedet: clamd::Typedet,
    broker: amqp::Broker,
    backend: backend::Backend,
    check_backend: bool,
}

enum ProcessResult<T> {
    Success(T),
    Requeue,
    Timeout,
    Exit,
}
use ProcessResult::*;

const PERF_META_KEY: &str = "_perf";

fn sanitize_backend_symbols(symbols: &mut [String]) {
    for sym in symbols.iter_mut() {
        let sanitized_sym = sym
            .to_ascii_uppercase()
            .replace(|c: char| !(c.is_ascii_alphanumeric() || c == '_'), "_");
        if sanitized_sym != *sym {
            warn!("Symbol \"{}\" sanitized to \"{}\"", sym, sanitized_sym);
            *sym = sanitized_sym;
        }
    }
}

/// The work manager
impl WorkManager {
    /// Creates a new work manager, spawning the backend and connecting to the broker
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        metrics::init_metrics();
        Ok(Self {
            clamd: clamd::Clamd::new(&config.clamd),
            #[cfg(feature = "backend")]
            typedet: clamd::Typedet::new(&config.typedet),
            backend: backend::Backend::new(&config)?,
            check_backend: false,
            broker: amqp::Broker::new(&config).await?,
            #[cfg(feature = "backend")]
            config,
        })
    }

    /// The job manager main loop
    ///
    /// The process is roughly:
    /// 1. get the next job
    ///
    /// 2. [in parallel]
    ///
    ///   * retrieve ClamAV symbols for the object
    ///   * invoke the backend on the object
    ///
    /// 3. turn backend produced children into objects
    ///
    /// 4. post the job result
    ///
    /// Additionally it asynchronously ensures that:
    /// - the backend stays alive
    /// - the broker stays connected
    /// - the child object job results are retrieved
    pub async fn manage_jobs(mut self) -> bool {
        let mut sigtask: tokio::task::JoinHandle<_> = tokio::spawn(async move {
            let mut sigint = match unix::signal(unix::SignalKind::interrupt()) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to setup SIGINT handler: {}", e);
                    return;
                }
            };
            let mut sigterm = match unix::signal(unix::SignalKind::terminate()) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to setup SIGTERM handler: {}", e);
                    return;
                }
            };
            let mut sigquit = match unix::signal(unix::SignalKind::quit()) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to setup SIGQUIT handler: {}", e);
                    return;
                }
            };
            tokio::select!(
                _ = sigint.recv() => debug!("SIGINT received"),
                _ = sigterm.recv() => debug!("SIGTERM received"),
                _ = sigquit.recv() => debug!("SIGQUIT received"),
            )
        });

        // Job reception loop
        debug!("Entering job management loop");
        let mut signal_received = false;
        loop {
            // Break out if a signal was received
            if sigtask.is_finished() {
                signal_received = true;
                break;
            }

            // Recheck backend availability if we suspect it could be stuck
            if self.check_backend && !self.backend.is_working().await {
                error!("Backend has stopped working");
                break;
            } else {
                self.check_backend = false;
            }

            // Get the first available request or break out on error
            let job_request = tokio::select!(
                _ = &mut sigtask => {
                    signal_received = true;
                    break;
                },
                v = self.get_next_request() => match v {
                    Some(req) => req,
                    None => break,
                }
            );
            metrics::job_received();

            // Publish an error result right away if the job has exceeded the maximum retry count
            if job_request.delivery_count >= 10 {
                // FIXME: tune this, or stuff in config or shared
                warn!(
                    "Job request received for object \"{}\" at recursion level {} has exceeded maximum retry count, giving up",
                    job_request.object.info.object_id, job_request.object.info.recursion_level
                );
                if self.post_max_retries(job_request).await.is_ok() {
                    continue;
                }
                break;
            }

            let ttl = job_request.time_remaining();
            // Publish an error result right away if the job has timed out
            // This is a shoot in the dark: the job may have expired a while
            // ago and the requester may have given up already
            if ttl.is_err() {
                warn!(
                    "Expired job request received for object \"{}\" at recursion level {}/{}",
                    job_request.object.info.object_id,
                    job_request.object.info.recursion_level,
                    job_request.object.max_recursion,
                );
                if self.post_timed_out(job_request).await.is_ok() {
                    continue;
                }
                break;
            }
            let ttl = ttl.unwrap();
            if !self.run_job(job_request, ttl).await {
                break;
            }
        }

        if signal_received {
            info!("Signal caught, exiting");
        } else {
            // A fatal issue occurred: exit as cleanly as possible so the broker knows
            error!("Exiting due to error condition");
        }
        self.broker.close().await;
        signal_received
    }

    #[tracing::instrument(level=tracing::Level::ERROR, skip_all, fields(object_id = %job_request.object.info.object_id))]
    async fn run_job(&mut self, job_request: amqp::JobRequest, ttl: std::time::Duration) -> bool {
        info!(
            "Job request received at recursion level {}/{} (TTL: {}ms)",
            job_request.object.info.recursion_level,
            job_request.object.max_recursion,
            ttl.as_millis()
        );

        // Pass the object to clam and to the backend
        let backend_res = match self.call_clamd_and_backend(&job_request, &ttl).await {
            Success(v) => v,
            Timeout => return self.post_timed_out(job_request).await.is_ok(),
            Requeue => return self.broker.reject_job_request(&job_request).await.is_ok(),
            Exit => {
                self.broker.reject_job_request(&job_request).await.ok();
                return false;
            }
        };

        let pending = {
            // Process children if backend succeeded
            match backend_res.result {
                // Backend returned an ok result with possible children
                BackendResultKind::ok(res) => {
                    // Turn backend children into objects
                    #[cfg(feature = "backend")]
                    let pending_children: Vec<amqp::PendingChildKind> = {
                        let child_objects = match self
                            .children_to_objects(&res.children, &job_request)
                            .await
                        {
                            Success(children) => children,
                            Timeout => {
                                warn!("Job expired while processing its children");
                                return self.post_timed_out(job_request).await.is_ok();
                            }
                            Requeue => {
                                return self.broker.reject_job_request(&job_request).await.is_ok();
                            }
                            Exit => {
                                self.broker.reject_job_request(&job_request).await.ok();
                                return false;
                            }
                        };
                        let global_relmeta = job_request
                            .object
                            .relation_metadata
                            .get(shared::META_KEY_GLOBAL);
                        child_objects
                            .into_iter()
                            .zip(res.children.into_iter())
                            .map(|(o, mut c)| {
                                // Merge global relation_metadata into child relation_metadata
                                if let Some(global_meta) = global_relmeta {
                                    c.relation_metadata.insert(
                                        shared::META_KEY_GLOBAL.to_string(),
                                        global_meta.clone(),
                                    );
                                }
                                sanitize_backend_symbols(&mut c.symbols);
                                object::sanitize_meta_keys(&mut c.relation_metadata);
                                amqp::PendingChildKind::new(
                                    o,
                                    c.symbols,
                                    c.relation_metadata,
                                    job_request.object.max_recursion,
                                )
                            })
                            .collect()
                    };
                    #[cfg(not(feature = "backend"))]
                    let pending_children: Vec<amqp::PendingChildKind> = Vec::new();
                    amqp::PendingResult::new_ok(
                        job_request,
                        res.symbols,
                        res.object_metadata,
                        pending_children,
                    )
                }
                // Backend returned an error result
                BackendResultKind::error(err) => amqp::PendingResult::new_err(job_request, err),
            }
        };
        // Mark the job results for publication
        self.broker
            .publish_result_when_complete(pending)
            .await
            .is_ok()
    }

    /// Posts an error result for this request
    async fn post_error_result(
        &mut self,
        job_request: amqp::JobRequest,
        error: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.broker
            .publish_result_when_complete(amqp::PendingResult::new_err(
                job_request,
                error.to_string(),
            ))
            .await
    }

    /// Posts a "Time out" result for this request
    async fn post_timed_out(
        &mut self,
        job_request: amqp::JobRequest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        metrics::job_timed_out();
        self.post_error_result(job_request, "Time out").await
    }

    /// Posts a "Max retries" result for this request
    async fn post_max_retries(
        &mut self,
        job_request: amqp::JobRequest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        metrics::job_max_retries();
        self.post_error_result(job_request, "Max retries").await
    }

    /// Retrieves the next request or None in case the broker or the backend failed
    ///
    /// Note: This also asynchronously processes the result queue (via broker.get_request)
    async fn get_next_request(&mut self) -> Option<amqp::JobRequest> {
        tokio::select! {
            // Get the first available message (job request received or broker lost)
            jobreq = self.broker.get_request() => jobreq,
            // while also checking that the backend stays alive
            _ = self.backend.wait() => {
                // Only returns on error: backend exited or got killed
                None
            }
        }
    }

    /// Retrieves the next request or None in case the broker or the backend failed
    ///
    /// Note: This also asynchronously processes the result queue
    async fn call_clamd_and_backend(
        &mut self,
        job_request: &amqp::JobRequest,
        ttl: &std::time::Duration,
    ) -> ProcessResult<backend::BackendResult> {
        let obj = &job_request.object;
        let (clamd_res, backend_res) = tokio::select!(
            // Await both clamd and the backend
            workres = futures::future::join(
                // NOTE: clamd and the backend are invoked in parallel for perf
                // this means the backend will not see clam-generated symbols
                tokio::time::timeout(*ttl, self.clamd.get_symbols(&obj.info.object_id)),
                tokio::time::timeout(*ttl, self.backend.invoke(obj))
            ) => workres,
            // ...unless the broker has fatal issues
            _ = self.broker.process_child_queue() => {
                // Note: process_child_queue never exits except on serious errors
                return Exit; // HARD failure
            }
        );

        // Something timed out
        if clamd_res.is_err() || backend_res.is_err() {
            warn!(
                "Job expired while processing it (clamd {}, backend {})",
                if clamd_res.is_err() { "TimeOut" } else { "OK" },
                if backend_res.is_err() {
                    "TimeOut"
                } else {
                    "OK"
                },
            );
            self.check_backend = backend_res.is_err();
            return Timeout; // SOFT failure
        }

        // Unwrap the timeouts
        let clamd_res = clamd_res.unwrap();
        let backend_res = backend_res.unwrap();

        // Handle failures in backend
        if backend_res.is_err() {
            // SOFT failure
            self.check_backend = backend_res.is_err();
            return Requeue;
        }
        let (mut backend_res, backend_time) = backend_res.unwrap();

        // If backend succeeded merge together:
        // - existing symbols (set by the parent)
        // - backend generated symbols
        // - clam symbols (with prefix) if backend succeeded
        if let BackendResultKind::ok(res) = &mut backend_res.result {
            let mut perf_meta = object::Metadata::new();
            perf_meta.insert("time_backend".into(), serde_json::Value::from(backend_time));
            sanitize_backend_symbols(&mut res.symbols);
            debug!("Merged {} exising symbols", obj.symbols.len());
            for sym in obj.symbols.iter() {
                res.symbols.push(sym.to_string())
            }
            match clamd_res {
                Err(e) => {
                    warn!("Clamd failed: {e}");
                    res.symbols.push("AV_SCAN_INCOMPLETE".to_string());
                }
                Ok((clamd_symbols, scan_time)) => {
                    if !clamd_symbols.is_empty() {
                        debug!("Merged {} symbols from clamd", clamd_symbols.len());
                        let mut infected = false;
                        for sym in clamd_symbols {
                            if sym.starts_with("ContexQL.") {
                                res.symbols.push(sym);
                            } else {
                                infected = true;
                                res.symbols.push(format!("INFECTED-CLAM-{}", sym));
                            }
                        }
                        if infected {
                            res.symbols.push("INFECTED".to_string());
                        }
                    }
                    perf_meta.insert("time_clamd".into(), serde_json::Value::from(scan_time));
                }
            }
            res.object_metadata
                .insert(PERF_META_KEY.into(), perf_meta.into());
            object::sanitize_meta_keys(&mut res.object_metadata);
            res.symbols.sort_unstable();
            res.symbols.dedup();
        }

        Success(backend_res)
    }

    // Turns backend children into objects via atomic copy+move and removes the tempfiles
    //
    // Note: this is serialized because hashing is CPU heavy
    #[cfg(feature = "backend")]
    async fn children_to_objects(
        &mut self,
        children: &[backend::BackendResultChild],
        job_request: &amqp::JobRequest,
    ) -> ProcessResult<Vec<shared::object::Info>> {
        let recursion_level = job_request.object.info.recursion_level + 1;
        let ctime = job_request.object.info.ctime;
        let mut objects: Vec<shared::object::Info> = Vec::with_capacity(children.len());

        // Atomically turn children into objects
        for c in children.iter() {
            if let Some(ref path) = c.path {
                // FIXME: process_children has little chance of being executed, should i spawn()?
                tokio::select! {
                    obj = shared::object::Info::new_from_file(&job_request.object.info.org, path, &self.config.objects_path, recursion_level, ctime) => match obj {
                        Ok(mut object) => {
                            if let Some(forced) = &c.force_type {
                                object.set_type(forced);
                            }
                            objects.push(object)
                        },
                        Err(_) => break, // HARD fail, rejection below
                    },
                    _ = self.broker.process_child_queue() => {
                        // Note: process_child_queue never exits except on serious errors
                        break; // HARD fail, rejection below
                    }
                };
            } else {
                objects.push(shared::object::Info::new_failed(
                    &job_request.object.info.org,
                    recursion_level,
                    ctime,
                ));
            }
        }
        // Remove backend temp files
        for c in children.iter() {
            if let Some(ref path) = c.path {
                std::fs::remove_file(path).ok();
            }
        }
        // Return failure if any error occurred above
        if objects.len() != children.len() {
            warn!("Child object creation failed");
            return Exit; // HARD fail
        }

        let ttl = job_request
            .time_remaining()
            .unwrap_or(std::time::Duration::ZERO);

        const MAXIMUM_CONCURRENCY: usize = 8;

        let nobj = objects.len();
        let nskip = objects.iter().filter(|o| o.is_skipped()).count();
        let nempty = objects.iter().filter(|o| o.is_empty()).count();
        let nforced = objects
            .iter()
            .filter(|o| !o.is_skipped() && !o.is_empty() && !o.object_type.is_empty())
            .count();
        let n2ftype = objects.iter().filter(|o| o.object_type.is_empty()).count();
        debug!(
            "Child objects:\n\
             \ttotal: {nobj}\n\
             \tskipped: {nskip}\n\
             \tempty: {nempty}\n\
             \tforced: {nforced}\n\
             \tto-ftype: {n2ftype}"
        );
        if n2ftype == 0 {
            return Success(objects);
        }
        let stream_of_futures = futures::stream::iter(
            objects
                .iter_mut()
                .filter(|o| o.object_type.is_empty())
                .map(|o| self.typedet.set_ftype(o, &self.config.objects_path)),
        )
        .buffer_unordered(MAXIMUM_CONCURRENCY)
        .take_while(|res| future::ready(res.is_ok()));

        tokio::select!(
            // Get all the ftypes, processing no more than MAXIMUM_CONCURRENCY futures at the same
            // time.
            chld_types = tokio::time::timeout(ttl, stream_of_futures.collect::<Vec<_>>()) => {
                match chld_types {
                    Err(_) => {
                        debug!("Type detection timed out");
                        Timeout
                    }
                    Ok(results) => {
                        if results.len() != n2ftype {
                            // Ftype failed for some objects
                            Requeue
                        } else {
                            // All children succeeded
                            debug!("All children types were set");
                            Success(objects)
                        }
                    }
                }
            }
            // ...while receiving child results
            _ = self.broker.process_child_queue() => {
                // Note: process_child_queue never exits except on serious errors
                Exit
            }
        )
    }
}
