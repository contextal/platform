use crate::{
    config::Config,
    error::UrlBackendError,
    http_response::{HttpResponse, InterruptionReason},
    ChildType, FileDownload, Guid, Url,
};
use backend_utils::objects::BackendResultChild;
use base64::Engine;
use chromiumoxide::{
    cdp::browser_protocol::{
        browser::{DownloadProgressState, EventDownloadProgress, EventDownloadWillBegin},
        fetch::{
            self, ContinueRequestParams, EventRequestPaused, FailRequestParams,
            FulfillRequestParams, HeaderEntry, TakeResponseBodyAsStreamParams,
            TakeResponseBodyAsStreamReturns,
        },
        io::ReadParams,
        network::{
            ErrorReason, EventLoadingFailed, EventLoadingFinished, EventRequestWillBeSent,
            EventResponseReceived, GetResponseBodyParams, GetResponseBodyReturns,
        },
        page::{CaptureScreenshotFormat, NavigateParams, NavigateReturns, PrintToPdfParams},
    },
    error::{CdpError, ChannelError},
    page::ScreenshotParams,
    types::CommandResponse,
};
use futures::StreamExt;
use std::{
    collections::{hash_map::Entry, HashMap},
    mem::swap,
    sync::Arc,
    time::Duration,
};
use tempfile::NamedTempFile;
use tokio::{
    fs, pin, select,
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        Mutex, RwLock,
    },
    task::JoinHandle,
    time::Instant,
};
use tracing::{debug, error, info, trace, warn};

/// A wrapper around browser Page which is intended to navigate to a single URL and collect
/// information about issued HTTP requests and received responses.
pub struct Page {
    /// Handler for a browser page.
    inner: Arc<RwLock<Option<chromiumoxide::Page>>>,

    /// Holds an instant of time when the page has been created.
    start: Instant,

    /// Backend config.
    config: Arc<Config>,

    /// Holds information about HTTP responses, including response bodies.
    responses: Arc<Mutex<HashMap<fetch::RequestId, HttpResponse>>>,

    /// Holds information about a file download which might be started by the browser as a result
    /// of navigation to a given URL.
    download_slot: Arc<Mutex<Option<FileDownload>>>,

    /// Postponed reports about failed requests, which are collected for later application.
    ///
    /// At the moment when any of these events fired there were no corresponding request (event
    /// about creation of a request) to correlate-them-with/apply-them-to.
    postponed_failures: Arc<Mutex<HashMap<fetch::RequestId, Arc<EventLoadingFailed>>>>,

    /// Sender side of a channel, which is used to signal about changes in amount of requests which
    /// are being served/processed at the moment. This channel is used to detect intervals when
    /// there are no network activity.
    inflight_requests_tx: UnboundedSender<i8>,

    /// Receiver side of a channel, which is used to signal about changes is amount of requests
    /// which are being served/processed at the moment. This channel is used to detect intervals
    /// when there are no network activity.
    inflight_requests_rx: UnboundedReceiver<i8>,

    /// A collection of swapned event handlers which are necessary to await/join at the end of Page
    /// life cycle.
    task_handles: Vec<JoinHandle<()>>,
}

impl Page {
    /// Attempts to create a new Page instance and setup all the necessary event handlers.
    pub async fn try_new(
        page: chromiumoxide::Page,
        config: Arc<Config>,
    ) -> Result<Self, UrlBackendError> {
        let (inflight_requests_tx, inflight_requests_rx) = unbounded_channel::<i8>();

        let mut this = Self {
            inner: Arc::new(RwLock::new(Some(page))),
            start: Instant::now(),
            config,
            responses: Arc::new(Mutex::new(HashMap::new())),
            download_slot: Arc::new(Mutex::new(None)),
            postponed_failures: Arc::new(Mutex::new(HashMap::new())),
            inflight_requests_tx,
            inflight_requests_rx,
            task_handles: vec![],
        };

        this.task_handles.extend([
            this.register_request_will_be_sent().await?,
            this.register_download_will_begin().await?,
            this.register_download_progress().await?,
            this.register_response_received().await?,
            this.register_loading_finished().await?,
            this.register_loading_failed().await?,
            this.register_intercept().await?,
        ]);

        Ok(this)
    }

    /// Attempts to close a browser page and join spawned task handles.
    ///
    /// Returns HTTP responses collected during page life cycle and a contents of the file-download
    /// slot.
    pub async fn close_and_consume(
        self,
    ) -> Result<
        (
            HashMap<fetch::RequestId, HttpResponse>,
            Option<FileDownload>,
        ),
        UrlBackendError,
    > {
        trace!("closing browser page...");
        match self.inner.write().await.take() {
            Some(page) => match page.close().await {
                Ok(_) => trace!("page has been closed"),
                Err(e) => error!("failed to close a browser page: {e}"),
            },
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };

        for (i, handle) in self.task_handles.into_iter().enumerate() {
            trace!("joining page event handle #{i} ...");
            if let Err(e) = handle.await {
                warn!("failed to join a task handle: {e}")
            }
        }
        debug!("all page handles have been joined");

        let download_slot = self.download_slot.lock().await.take();

        let mut responses = HashMap::new();
        swap(&mut *self.responses.lock().await, &mut responses);

        Ok((responses, download_slot))
    }

    /// Applies postponed events about failed HTTP responses to matching entries in collected HTTP
    /// responses.
    async fn apply_postponsed_failures(&self) {
        for (request, loading_failed) in self.postponed_failures.lock().await.drain() {
            match self.responses.lock().await.get_mut(&request) {
                Some(response) => {
                    info!(
                        "request to {:?} has been marked as 'failed' based on postponed event",
                        response.url
                    );
                    response.error_text = Some(loading_failed.error_text.clone());
                    response.canceled = loading_failed.canceled.unwrap_or(false);
                    response.blocked_reason = loading_failed.blocked_reason.clone();
                }
                None => error!(
                    "request id {request:?} hasn't been found among requests/responses to apply \
                    postponed event",
                ),
            }
        }
    }

    /// Waits until network settles in idle state (when there are no incomplete network requests
    /// for some time) or until backend timeout interval passes.
    ///
    /// At the end applies postponed request-has-failed events (if there are any) to corresponding
    /// entries in collected responses.
    pub async fn wait_for_navigation(&mut self) {
        let idle_network_settled = tokio::time::sleep(Duration::from_millis(
            self.config.chrome_request_timeout_msec as _,
        ));
        pin!(idle_network_settled);
        let backend_timeout = tokio::time::sleep(
            Duration::from_millis(self.config.chrome_request_timeout_msec as _)
                .saturating_sub(self.start.elapsed()),
        );
        pin!(backend_timeout);
        let mut requests_in_flight = 0i32;
        loop {
            select! {
                _ = &mut backend_timeout => {
                    warn!("backend request timeout");
                    break
                },
                _ = &mut idle_network_settled => {
                    info!("network has settled in idle state");
                    break
                },
                message = self.inflight_requests_rx.recv() => {
                    if let Some(delta) = message {
                        requests_in_flight = requests_in_flight.saturating_add(delta as _);
                        // trace!("number of in-flight requests: {requests_in_flight}");
                        if requests_in_flight == 0 {
                            trace!("waiting for network to settle in idle state...");
                            idle_network_settled.as_mut().reset(
                                Instant::now()
                                    + Duration::from_millis(
                                        self.config.idle_network_settle_time_msec as _,
                                    ),
                            );
                        } else {
                            idle_network_settled.as_mut().reset(
                                Instant::now()
                                    + Duration::from_millis(
                                        self.config.chrome_request_timeout_msec as _,
                                    ),
                            );
                        }
                    }
                }
            }
        }

        self.apply_postponsed_failures().await;
    }

    /// Attempts to navigate to a given URL.
    pub async fn navigate_to(&self, url: Url) -> Result<(), UrlBackendError> {
        // Spawn navigate-to-URL command as a separate task to prevent it from blocking execution
        // flow (which could happen in case of direct use of `goto`).
        //
        // And also do it in a manner which doesn't keep the page handler in a read-locked state.
        // As otherwise it would make it impossible to close the page in a reasonable way.
        let navigate_to = match self.inner.read().await.as_ref() {
            Some(page) => page.command_future(NavigateParams::new(url.as_str()))?,
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };
        tokio::spawn(async move {
            match navigate_to.await {
                Ok(CommandResponse {
                    result: NavigateReturns { error_text, .. },
                    ..
                }) => match error_text {
                    Some(text) => warn!("navigate-to {url:?} is complete, error_text: {text}"),
                    None => info!("navigate-to {url:?} is complete"),
                },
                Err(CdpError::ChannelSendError(ChannelError::Canceled(_))) => {
                    info!("navigate-to {url:?} has been canceled")
                }
                Err(e) => warn!("navigate-to {url:?} has failed: {e}"),
            };
        });

        Ok(())
    }

    /// Attempts to register a `RequestPaused` event handler, which is used to inspect
    /// requests/responses and to abort responses which go over content-length and response body
    /// size limits.
    async fn register_intercept(&self) -> Result<JoinHandle<()>, UrlBackendError> {
        let mut event_stream = match self.inner.read().await.as_ref() {
            Some(page) => page.event_listener::<EventRequestPaused>().await?,
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };
        let config = self.config.clone();
        let page = self.inner.clone();
        let responses = self.responses.clone();
        let inflight_requests_tx = self.inflight_requests_tx.clone();
        let handle = tokio::task::spawn(async move {
            while let Some(event) = event_stream.next().await {
                let page = page.clone();
                let config = config.clone();
                let responses = responses.clone();
                let inflight_requests_tx = inflight_requests_tx.clone();
                tokio::task::spawn(async move {
                    let network_id = match event.network_id {
                        Some(ref id) => id.clone().into(),
                        None => {
                            warn!(
                                "intercepted: {:?}, no network id => skipping the event",
                                event.request.url
                            );
                            return;
                        }
                    };

                    match event.response_status_code {
                        None => {
                            trace!(
                                "intercepted: {:?}, no response headers yet => continue",
                                event.request.url,
                            );
                            responses
                                .lock()
                                .await
                                .entry(network_id)
                                .and_modify(
                                    |HttpResponse {
                                         url,
                                         redirected_from,
                                         resource_type,
                                         ..
                                     }| {
                                        if config
                                            .excluded_resource_types
                                            .contains(&event.resource_type)
                                        {
                                            return;
                                        }

                                        // The same request might appear at this stage more than
                                        // once, and this is not necessary because of HTTP
                                        // redirect.
                                        if url.0 != event.request.url {
                                            trace!(
                                                "request redirected: {:?} -> {:?}",
                                                url.0,
                                                event.request.url
                                            );
                                            let mut placeholder = Url(event.request.url.clone());
                                            swap(url, &mut placeholder);
                                            redirected_from.push(placeholder);
                                        }
                                        *resource_type = Some(event.resource_type.clone());
                                    },
                                )
                                .or_insert_with(|| {
                                    if let Err(e) = inflight_requests_tx.send(1) {
                                        warn!("failed to signal start-of-request: {e}");
                                    }
                                    HttpResponse {
                                        url: Url(event.request.url.clone()),
                                        redirected_from: vec![],
                                        canceled: false,
                                        remote_ip_address: None,
                                        resource_type: Some(event.resource_type.clone()),
                                        status_code: None,
                                        status_text: None,
                                        mime_type: None,
                                        body: None,
                                        error_text: None,
                                        config: config.clone(),
                                        interruption_reason: None,
                                        network_id: event
                                            .network_id
                                            .as_ref()
                                            .map(|id| id.clone().into()),
                                        blocked_reason: None,
                                    }
                                });
                            if let Err(e) = match page.read().await.as_ref() {
                                Some(page) => {
                                    page.execute(ContinueRequestParams::new(
                                        event.request_id.clone(),
                                    ))
                                    .await
                                }
                                None => {
                                    warn!("page has been closed already, nothing makes sense");
                                    return;
                                }
                            } {
                                warn!("failed to continue a request: {e}")
                            }
                        }
                        Some(code) if [301, 302, 303, 307, 308].contains(&code) => {
                            let location = event
                                .response_headers
                                .as_ref()
                                .expect("headers are always available at this stage")
                                .iter()
                                .find(|header| header.name.to_lowercase() == "location")
                                .map(|v| v.value.clone());
                            trace!(
                                "intercepted: {:?} ({code}) redirected to {:?} => continue",
                                event.request.url,
                                location.unwrap_or_else(|| "<None>".into())
                            );
                            if let Err(e) = match page.read().await.as_ref() {
                                Some(page) => {
                                    page.execute(ContinueRequestParams::new(
                                        event.request_id.clone(),
                                    ))
                                    .await
                                }
                                None => {
                                    warn!("page has been closed already, nothing makes sense");
                                    return;
                                }
                            } {
                                warn!("failed to continue a request: {e}")
                            }
                        }
                        Some(code) => {
                            trace!(
                                "intercepted: {:?} ({code}) => inspecting",
                                event.request.url,
                            );
                            match responses.lock().await.entry(network_id.clone()) {
                                Entry::Occupied(mut entry) => {
                                    let response = entry.get_mut();
                                    response.status_code = event.response_status_code;
                                    response.status_text = event.response_status_text.clone();
                                    response.mime_type =
                                        event.response_headers.as_ref().and_then(|headers| {
                                            headers
                                                .iter()
                                                .find(|HeaderEntry { name, .. }| {
                                                    name.to_lowercase() == "content-type"
                                                })
                                                .and_then(|HeaderEntry { value, .. }| {
                                                    value.split(';').next().map(String::from)
                                                })
                                        });
                                }
                                Entry::Vacant(_) => {
                                    error!(
                                        "received headers for unaccounted request id: {:?} - this \
                                        should not happen",
                                        event.request_id
                                    );
                                }
                            };

                            let content_length = event
                                .response_headers
                                .as_ref()
                                .expect("headers are always available at this stage")
                                .iter()
                                .find(|header| header.name.to_lowercase() == "content-length")
                                .and_then(|v| match v.value.parse::<u64>() {
                                    Ok(v) => Some(v),
                                    Err(e) => {
                                        warn!("failed to parse content-length header: {e}");
                                        None
                                    }
                                });

                            match content_length {
                                Some(value) if value > config.max_response_content_length => {
                                    info!(
                                        "content-length ({value}) is over the limit ({}) => \
                                        interrupting",
                                        config.max_response_content_length
                                    );
                                    responses.lock().await.entry(network_id).and_modify(
                                        |response| {
                                            response.interruption_reason =
                                                Some(InterruptionReason::ContentLength)
                                        },
                                    );
                                    if let Err(e) = match page.read().await.as_ref() {
                                        Some(page) => {
                                            page.execute(FailRequestParams::new(
                                                event.request_id.clone(),
                                                ErrorReason::BlockedByClient,
                                            ))
                                            .await
                                        }
                                        None => {
                                            warn!(
                                                "page has been closed already, nothing makes sense"
                                            );
                                            return;
                                        }
                                    } {
                                        warn!("failed to fail a request: {e}")
                                    }

                                    return;
                                }
                                Some(value) => {
                                    trace!("content-length ({value}) is within the limit")
                                }
                                None => trace!("content-length is not available"),
                            }

                            let command_response = match page.read().await.as_ref() {
                                Some(page) => {
                                    page.execute(TakeResponseBodyAsStreamParams::new(
                                        event.request_id.clone(),
                                    ))
                                    .await
                                }
                                None => {
                                    warn!("page has been closed already, nothing makes sense");
                                    return;
                                }
                            };
                            let stream = match command_response {
                                Ok(CommandResponse {
                                    result: TakeResponseBodyAsStreamReturns { stream },
                                    ..
                                }) => {
                                    trace!("response body stream has been taken for inspection");
                                    stream
                                }
                                Err(e) => {
                                    warn!("failed to take response body stream: {e}");
                                    let command_response = match page.read().await.as_ref() {
                                        Some(page) => {
                                            page.execute(ContinueRequestParams::new(
                                                event.request_id.clone(),
                                            ))
                                            .await
                                        }
                                        None => {
                                            warn!(
                                                "page has been closed already, nothing makes sense"
                                            );
                                            return;
                                        }
                                    };
                                    match command_response {
                                        Ok(_) => trace!("continue request has been issued"),
                                        Err(e) => warn!("failed to continue a request: {e}"),
                                    }
                                    return;
                                }
                            };

                            let max_response_data_length = config
                                .max_response_data_length
                                .min(config.max_child_output_size)
                                .try_into()
                                .unwrap_or(i64::MAX);

                            let mut eof = false;
                            let mut body = String::new();
                            let mut base64_encoded = None;
                            while !eof {
                                let read_future = match page.read().await.as_ref() {
                                    Some(page) => match page.command_future(ReadParams {
                                        size: Some(max_response_data_length.saturating_add(1)),
                                        ..ReadParams::new(stream.clone())
                                    }) {
                                        Ok(v) => v,
                                        Err(e) => {
                                            error!("failed to construct a read future: {e}");
                                            return;
                                        }
                                    },
                                    None => {
                                        warn!("page has been closed already, nothing makes sense");
                                        return;
                                    }
                                };
                                match read_future.await {
                                    Ok(read) => {
                                        let read_len = match read.data.len() {
                                            0 => 0,
                                            _ => {
                                                // ReadReturns' `base64_encoded` is meaningful only
                                                // if read has produced any data:
                                                if base64_encoded.is_none() {
                                                    base64_encoded = read.base64_encoded;
                                                }

                                                match read.base64_encoded {
                                                    Some(true) => {
                                                        let padding = match read.data.as_bytes() {
                                                            [.., b'=', b'='] => 2,
                                                            [.., b'='] => 1,
                                                            _ => 0,
                                                        };
                                                        read.data.len() * 3 / 4 - padding
                                                    }
                                                    _ => read.data.len(),
                                                }
                                            }
                                        };
                                        eof = read.eof;
                                        trace!("IO::Read provided {read_len} bytes, EOF: {eof}");

                                        body.push_str(&read.data);
                                        let response_len = match base64_encoded {
                                            Some(true) => {
                                                let padding = match body.as_bytes() {
                                                    [.., b'=', b'='] => 2,
                                                    [.., b'='] => 1,
                                                    _ => 0,
                                                };
                                                body.len() * 3 / 4 - padding
                                            }
                                            _ => body.len(),
                                        };
                                        if response_len > max_response_data_length as _ {
                                            info!(
                                                "received data size ({response_len}) is over the \
                                                limit => interrupting",
                                            );
                                            responses.lock().await.entry(network_id).and_modify(
                                                |response| {
                                                    response.interruption_reason =
                                                        Some(InterruptionReason::ResponseSize)
                                                },
                                            );
                                            if let Err(e) = match page.read().await.as_ref() {
                                                Some(page) => {
                                                    page.execute(FailRequestParams::new(
                                                        event.request_id.clone(),
                                                        ErrorReason::Aborted,
                                                    ))
                                                    .await
                                                }
                                                None => {
                                                    warn!(
                                                        "page has been closed already, \
                                                        nothing makes sense"
                                                    );
                                                    return;
                                                }
                                            } {
                                                warn!("failed to interrupt a request: {e}")
                                            }

                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        // With error at this stage it is not possible to continue
                                        // the request/response (as body stream has been taken),
                                        // and it is not possible to fulfill the request (as the
                                        // body could not be read in full).
                                        // So the only option is to fail/interrupt the request.
                                        warn!("failed to perform IO::Read: {e} => interrupting");
                                        if let Err(e) = match page.read().await.as_ref() {
                                            Some(page) => {
                                                page.execute(FailRequestParams::new(
                                                    event.request_id.clone(),
                                                    ErrorReason::Failed,
                                                ))
                                                .await
                                            }
                                            None => {
                                                warn!(
                                                    "page has been closed already, \
                                                    nothing makes sense"
                                                );
                                                return;
                                            }
                                        } {
                                            warn!("failed to interrupt a request: {e}")
                                        }

                                        return;
                                    }
                                }
                            }

                            trace!("full body received -> fulfilling a request");
                            if let Err(e) = match page.read().await.as_ref() {
                                Some(page) => {
                                    page.execute(FulfillRequestParams {
                                        response_headers: event.response_headers.clone(),
                                        body: Some(
                                            match base64_encoded {
                                                Some(true) => body,
                                                _ => base64::prelude::BASE64_STANDARD.encode(&body),
                                            }
                                            .into(),
                                        ),
                                        ..FulfillRequestParams::new(
                                            event.request_id.clone(),
                                            event.response_status_code.unwrap_or(200),
                                        )
                                    })
                                    .await
                                }
                                None => {
                                    warn!("page has been closed already, nothing makes sense");
                                    return;
                                }
                            } {
                                warn!("failed to fulfill a request: {e}")
                            }
                        }
                    }
                });
            }
        });

        Ok(handle)
    }

    /// Attempts to register a `LoadingFailed` event handler, which is used to account
    /// unsuccessfull/interrupted requests/responses.
    async fn register_loading_failed(&self) -> Result<JoinHandle<()>, UrlBackendError> {
        let mut event_stream = match self.inner.read().await.as_ref() {
            Some(page) => page.event_listener::<EventLoadingFailed>().await?,
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };
        let responses = self.responses.clone();
        let inflight_requests_tx = self.inflight_requests_tx.clone();
        let postponed_failures = self.postponed_failures.clone();
        let handle = tokio::task::spawn(async move {
            while let Some(event) = event_stream.next().await {
                if let Err(e) = inflight_requests_tx.send(-1) {
                    warn!("failed to signal end-of-request: {e}");
                }
                let request_id = fetch::RequestId::from(event.request_id.clone());
                match responses.lock().await.get_mut(&request_id) {
                    Some(response) => {
                        match response.interruption_reason {
                            Some(_) => info!("request has been interrupted: {}", response.url.0),
                            None => warn!("loading failed: {}", response.url.0),
                        }
                        response.error_text = Some(event.error_text.clone());
                        response.canceled = event.canceled.unwrap_or(false);
                        response.blocked_reason = event.blocked_reason.clone();
                    }
                    None => {
                        info!(
                            "loading-failed event for unaccounted request id {:?} => postponing \
                            the event for later stage",
                            event.request_id
                        );
                        match postponed_failures.lock().await.entry(request_id.clone()) {
                            Entry::Occupied(mut entry) => {
                                warn!(
                                    "loading-failed event for request id {request_id:?} is already \
                                    present",
                                );
                                entry.insert(event);
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(event);
                            }
                        }
                    }
                };
            }
        });

        Ok(handle)
    }

    /// Attempts to register a `LoadingFinished` event handler, which is used to account
    /// successful/accomplished requests/responses.
    async fn register_loading_finished(&self) -> Result<JoinHandle<()>, UrlBackendError> {
        let mut event_stream = match self.inner.read().await.as_ref() {
            Some(page) => page.event_listener::<EventLoadingFinished>().await?,
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };
        let responses = self.responses.clone();
        let page = self.inner.clone();
        let inflight_requests_tx = self.inflight_requests_tx.clone();
        let handle = tokio::task::spawn(async move {
            while let Some(event) = event_stream.next().await {
                let request_id = fetch::RequestId::from(event.request_id.clone());
                match responses.lock().await.get(&request_id) {
                    Some(v) => {
                        info!("loading finished: {}", v.url.0);
                        if let Err(e) = inflight_requests_tx.send(-1) {
                            warn!("failed to signal end-of-request: {e}");
                        }
                    }
                    None => {
                        warn!(
                            "loading finished for unaccounted request {:?}",
                            event.request_id
                        );
                    }
                };

                let command_response = match page.read().await.as_ref() {
                    Some(page) => {
                        page.execute(GetResponseBodyParams::new(event.request_id.clone()))
                            .await
                    }
                    None => {
                        warn!("page has been closed already, nothing makes sense");
                        return;
                    }
                };
                match command_response {
                    Ok(CommandResponse {
                        result:
                            GetResponseBodyReturns {
                                body,
                                base64_encoded,
                            },
                        ..
                    }) => {
                        match responses.lock().await.get_mut(&request_id) {
                            Some(v) => {
                                v.body = Some(match base64_encoded {
                                    true => {
                                        match base64::prelude::BASE64_STANDARD.decode(&body) {
                                            Ok(v) => v,
                                            Err(e) => {
                                                warn!("base64 decode has failed, storing body as-is: {e}");
                                                body.into()
                                            }
                                        }
                                    }
                                    false => body.into(),
                                })
                            }
                            None => {
                                warn!("skipping body for unaccounted request: {:?}", request_id)
                            }
                        };
                    }
                    Err(e) => {
                        error!("failed to get finished response body: {e}")
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Attempts to register a `ResponseReceived` event handler, which is used to get remote IP
    /// addresses when possible.
    async fn register_response_received(&self) -> Result<JoinHandle<()>, UrlBackendError> {
        let mut event_stream = match self.inner.read().await.as_ref() {
            Some(page) => page.event_listener::<EventResponseReceived>().await?,
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };
        let responses = self.responses.clone();
        let handle = tokio::task::spawn(async move {
            while let Some(event) = event_stream.next().await {
                if let Some(ref ip) = event.response.remote_ip_address {
                    if !ip.is_empty() {
                        let request_id = fetch::RequestId::from(event.request_id.clone());
                        match responses.lock().await.get_mut(&request_id) {
                            Some(v) => {
                                if v.remote_ip_address.is_none() {
                                    v.remote_ip_address = Some(ip.clone())
                                }
                            }
                            None => error!("received a response on unaccounted request id"),
                        }
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Attempts to register a `DownloadWillBegin` event handler, which is used to account for
    /// file-download when it is started.
    async fn register_download_will_begin(&self) -> Result<JoinHandle<()>, UrlBackendError> {
        let mut event_stream = match self.inner.read().await.as_ref() {
            Some(page) => page.event_listener::<EventDownloadWillBegin>().await?,
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };
        let download_slot = self.download_slot.clone();
        let inflight_requests_tx = self.inflight_requests_tx.clone();
        let config = self.config.clone();
        let handle = tokio::task::spawn(async move {
            while let Some(event) = event_stream.next().await {
                trace!(
                    "DownloadWillBegin -> url = {:?}: {:?} => {:?}",
                    event.url,
                    event.suggested_filename,
                    event.guid
                );
                let mut download_slot = download_slot.lock().await;
                if let Some(ref download) = *download_slot {
                    error!(
                        "download slot has been already taken by \
                        {download:?} - this is not expected to happen"
                    );

                    return;
                };

                *download_slot = Some(FileDownload {
                    url: Url(event.url.clone()),
                    guid: Guid(event.guid.clone()),
                    suggested_filename: event.suggested_filename.clone(),
                    state: DownloadProgressState::InProgress,
                    config: config.clone(),
                });

                if let Err(e) = inflight_requests_tx.send(1) {
                    warn!("failed to signal start-of-request (download): {e}");
                }
            }
        });

        Ok(handle)
    }

    /// Attempts to register a `DownloadProgress` event handler, which is used to watch
    /// when file-download completes or gets canceled.
    async fn register_download_progress(&self) -> Result<JoinHandle<()>, UrlBackendError> {
        let mut event_stream = match self.inner.read().await.as_ref() {
            Some(page) => page.event_listener::<EventDownloadProgress>().await?,
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };
        let download_slot = self.download_slot.clone();
        let inflight_requests_tx = self.inflight_requests_tx.clone();
        let handle = tokio::task::spawn(async move {
            while let Some(event) = event_stream.next().await {
                trace!(
                    "DownloadProgress -> guid {:?} {:?}: [{}/{}]",
                    event.guid,
                    event.state,
                    event.received_bytes,
                    event.total_bytes
                );

                if matches!(
                    event.state,
                    DownloadProgressState::Completed | DownloadProgressState::Canceled
                ) {
                    match &mut *download_slot.lock().await {
                        Some(ref mut v) if *v.guid == event.guid => v.state = event.state.clone(),
                        Some(_) | None => {
                            error!(
                                "download guid {:?} has not been accounted - \
                                this should never happen",
                                event.guid
                            );
                            return;
                        }
                    };
                    if let Err(e) = inflight_requests_tx.send(-1) {
                        warn!("failed to signal end-of-request (download): {e}");
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Attempts to register a `RequestWillBeSent` event handler, which is used to account/register
    /// outgoing network requests.
    async fn register_request_will_be_sent(&self) -> Result<JoinHandle<()>, UrlBackendError> {
        let mut event_stream = match self.inner.read().await.as_ref() {
            Some(page) => page.event_listener::<EventRequestWillBeSent>().await?,
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };
        let responses = self.responses.clone();
        let config = self.config.clone();
        let inflight_requests_tx = self.inflight_requests_tx.clone();
        let handle = tokio::task::spawn(async move {
            while let Some(event) = event_stream.next().await {
                if let Some(rtype) = &event.r#type {
                    if config.excluded_resource_types.contains(rtype) {
                        continue;
                    }
                }
                trace!("NetworkRequestWillBeSent {:?}", event.request.url);
                let request_id = fetch::RequestId::from(event.request_id.clone());
                responses
                    .lock()
                    .await
                    .entry(request_id.clone())
                    .or_insert_with(|| {
                        if let Err(e) = inflight_requests_tx.send(1) {
                            warn!("failed to signal start-of-request: {e}");
                        }
                        HttpResponse {
                            url: Url(event.request.url.clone()),
                            redirected_from: vec![],
                            canceled: false,
                            remote_ip_address: None,
                            resource_type: event.r#type.clone(),
                            status_code: None,
                            status_text: None,
                            mime_type: None,
                            body: None,
                            error_text: None,
                            config: config.clone(),
                            interruption_reason: None,
                            network_id: Some(request_id),
                            blocked_reason: None,
                        }
                    });
            }
        });

        Ok(handle)
    }

    /// Attempts to produce a PDF document from browser's Page content and produce a
    /// `BackendResultChild` of appropriate type.
    pub async fn print_to_pdf(&self) -> Result<BackendResultChild, UrlBackendError> {
        let (print_to_pdf, url) = match self.inner.read().await.as_ref() {
            Some(page) => (
                page.pdf(PrintToPdfParams::builder().build()).await,
                page.url().await,
            ),
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };
        match print_to_pdf {
            Ok(pdf_data) => {
                let output_file = {
                    let config = self.config.clone();
                    tokio::task::spawn_blocking(move || {
                        if config.random_filenames {
                            NamedTempFile::new_in(&config.output_path)
                        } else {
                            tempfile::Builder::new()
                                .prefix("print_to_pdf_")
                                .suffix(".pdf")
                                .tempfile_in(&config.output_path)
                        }
                    })
                    .await??
                };

                if u64::try_from(pdf_data.len()).map(|len| len <= self.config.max_child_output_size)
                    != Ok(true)
                {
                    return Ok(BackendResultChild {
                        path: None,
                        symbols: vec!["TOOBIG".to_string()],
                        relation_metadata: match serde_json::to_value(ChildType::PrintToPdf {
                            url: url?.map(Url),
                        })? {
                            serde_json::Value::Object(v) => v,
                            _ => unreachable!(),
                        },
                        force_type: None,
                    });
                }

                fs::write(&output_file, pdf_data).await?;

                let output_file = output_file
                    .into_temp_path()
                    .keep()?
                    .into_os_string()
                    .into_string()
                    .map_err(UrlBackendError::Utf8)?;

                Ok(BackendResultChild {
                    path: Some(output_file),
                    symbols: vec![],
                    relation_metadata: match serde_json::to_value(ChildType::PrintToPdf {
                        url: url?.map(Url),
                    })? {
                        serde_json::Value::Object(v) => v,
                        _ => unreachable!(),
                    },
                    force_type: None,
                })
            }
            Err(e) => Err(UrlBackendError::PrintToPdf(e)),
        }
    }

    /// Attempts to get a browser Page HTML document and produce a `BackendResultChild` of
    /// appropriate type.
    ///
    /// The document content might differ from HTML body provided by a web server, as HTML page
    /// content is often a subject for modification by accompanying JavaScript code.
    pub async fn get_content(&self) -> Result<BackendResultChild, UrlBackendError> {
        let (content, title, url) = match self.inner.read().await.as_ref() {
            Some(page) => (
                page.content().await,
                page.get_title().await,
                page.url().await,
            ),
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };
        match content {
            Ok(page_html_content) => {
                if u64::try_from(page_html_content.bytes().len())
                    .map(|len| len <= self.config.max_child_output_size)
                    != Ok(true)
                {
                    return Ok(BackendResultChild {
                        path: None,
                        symbols: vec!["TOOBIG".to_string()],
                        relation_metadata: match serde_json::to_value(ChildType::PageHtmlContent {
                            url: url?.map(Url),
                            title: title?,
                        })? {
                            serde_json::Value::Object(v) => v,
                            _ => unreachable!(),
                        },
                        force_type: None,
                    });
                }
                let output_file = {
                    let config = self.config.clone();
                    tokio::task::spawn_blocking(move || {
                        if config.random_filenames {
                            NamedTempFile::new_in(&config.output_path)
                        } else {
                            tempfile::Builder::new()
                                .prefix("page_content_")
                                .suffix(".html")
                                .tempfile_in(&config.output_path)
                        }
                    })
                    .await??
                };
                fs::write(&output_file, page_html_content).await?;

                let output_file = output_file
                    .into_temp_path()
                    .keep()?
                    .into_os_string()
                    .into_string()
                    .map_err(UrlBackendError::Utf8)?;

                Ok(BackendResultChild {
                    path: Some(output_file),
                    symbols: vec![],
                    relation_metadata: match serde_json::to_value(ChildType::PageHtmlContent {
                        url: url?.map(Url),
                        title: title?,
                    })? {
                        serde_json::Value::Object(v) => v,
                        _ => unreachable!(),
                    },
                    force_type: None,
                })
            }
            Err(e) => Err(UrlBackendError::HtmlContents(e)),
        }
    }

    /// Attempts to take a screenshot of a browser page content and produce a `BackendResultChild`
    /// of appropriate type.
    pub async fn capture_screenshot(&self) -> Result<BackendResultChild, UrlBackendError> {
        let (screenshot, url) = match self.inner.read().await.as_ref() {
            Some(page) => (
                page.screenshot(
                    ScreenshotParams::builder()
                        .format(CaptureScreenshotFormat::Jpeg)
                        .quality(90)
                        .full_page(true)
                        .build(),
                )
                .await,
                page.url().await,
            ),
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        };
        match screenshot {
            Ok(image_data) => {
                if u64::try_from(image_data.len())
                    .map(|len| len <= self.config.max_child_output_size)
                    != Ok(true)
                {
                    return Ok(BackendResultChild {
                        path: None,
                        symbols: vec!["TOOBIG".to_string()],
                        relation_metadata: match serde_json::to_value(ChildType::Screenshot {
                            url: url?.map(Url),
                        })? {
                            serde_json::Value::Object(v) => v,
                            _ => unreachable!(),
                        },
                        force_type: None,
                    });
                }
                let output_file = {
                    let config = self.config.clone();
                    tokio::task::spawn_blocking(move || {
                        if config.random_filenames {
                            NamedTempFile::new_in(&config.output_path)
                        } else {
                            tempfile::Builder::new()
                                .prefix("screenshot_")
                                .suffix(".jpg")
                                .tempfile_in(&config.output_path)
                        }
                    })
                    .await??
                };
                fs::write(&output_file, image_data).await?;

                let output_file = output_file
                    .into_temp_path()
                    .keep()?
                    .into_os_string()
                    .into_string()
                    .map_err(UrlBackendError::Utf8)?;

                Ok(BackendResultChild {
                    path: Some(output_file),
                    symbols: vec![],
                    relation_metadata: match serde_json::to_value(ChildType::Screenshot {
                        url: url?.map(Url),
                    })? {
                        serde_json::Value::Object(v) => v,
                        _ => unreachable!(),
                    },
                    force_type: None,
                })
            }
            Err(e) => Err(UrlBackendError::Screenshot(e)),
        }
    }

    /// Attempts to provide an address bar URL.
    pub async fn url(&self) -> Result<Option<String>, UrlBackendError> {
        match self.inner.read().await.as_ref() {
            Some(page) => page.url().await.map_err(|e| e.into()),
            None => Err(UrlBackendError::PageHandlerIsNotAvailable)?,
        }
    }
}
