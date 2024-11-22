use crate::{config::Config, error::UrlBackendError, page::Page, Url};
use backend_utils::objects::BackendRequest;
use chromiumoxide::{
    cdp::browser_protocol::{
        browser::{SetDownloadBehaviorBehavior, SetDownloadBehaviorParams},
        fetch::{self, RequestPattern, RequestStage},
        network::SetUserAgentOverrideParams,
    },
    Browser, BrowserConfig,
};
use futures::{lock::Mutex, StreamExt};
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use std::{env, sync::Arc};
use tokio::task::{self, JoinHandle};
use tracing::{debug, info, trace, warn};

/// A container to keep initialized entities, timers and counters used by the backend during its
/// life time.
pub struct Backend {
    /// Backend configuration.
    pub config: Arc<Config>,

    /// Headless Chromium/Chrome instance handler.
    browser: Mutex<chromiumoxide::Browser>,

    /// Browser's event handle (not generally used by the backend, part of `chromiumoxide`
    /// interface).
    browser_handle: Cell<JoinHandle<()>>,

    /// A moment in time when current browser instance has been started.
    creation_time: Cell<Instant>,

    /// Number of backend requests processed (new-page requests) by current browser instance.
    new_pages_requested: Cell<usize>,
}

impl Backend {
    /// Attempts to create new `Backend` instance.
    pub async fn new(config: Config) -> Result<Self, UrlBackendError> {
        let (browser, browser_handle) = Self::new_browser(&config).await?;
        Ok(Self {
            browser: Mutex::new(browser),
            browser_handle: Cell::new(browser_handle),
            config: Arc::new(config),
            creation_time: Cell::new(Instant::now()),
            new_pages_requested: Cell::new(0),
        })
    }

    /// Attempts to start a new Chromium/Chrome browser instance.
    async fn new_browser(config: &Config) -> Result<(Browser, JoinHandle<()>), UrlBackendError> {
        let browser_config = {
            let mut config_builder = BrowserConfig::builder();

            if let Some(ref proxy) = config.proxy {
                config_builder = config_builder.arg(format!("--proxy-server={proxy}"));
            }

            if env::var("RUNNING_IN_DOCKER")
                .as_deref()
                .map(str::to_lowercase)
                .as_deref()
                == Ok("true")
            {
                // This is insecure, but this allows to start Chromium/Chrome in a Docker
                // container:
                config_builder = config_builder.arg("--no-sandbox");
            }

            config_builder
                .enable_request_intercept()
                .disable_cache()
                .window_size(config.window_size.0, config.window_size.1)
                .request_timeout(Duration::from_millis(
                    config.chrome_request_timeout_msec as _,
                ))
                .user_data_dir(config.output_path.clone())
                .build()
                .map_err(UrlBackendError::BrowserConfigBuilder)?
        };
        let (browser, mut browser_handler) = Browser::launch(browser_config).await?;

        let browser_handle = task::spawn(async move {
            while let Some(h) = browser_handler.next().await {
                if let Err(e) = h {
                    warn!("browser handler: {e}");
                    break;
                }
            }
        });
        Ok((browser, browser_handle))
    }

    /// Attempts to create a browser `Page` and set it up according to the backend config.
    ///
    /// If browser instance is
    /// - not reachable-and-responding, or
    /// - it is already running for too long, or
    /// - too many backend requests have been already processed,
    /// then the browser instance will be restarted before creating a new `Page`.
    pub async fn new_page(&self) -> Result<Page, UrlBackendError> {
        let is_it_time_to_recycle = self.new_pages_requested.get()
            >= self.config.max_backend_requests_per_instance as _
            || Instant::now().saturating_duration_since(self.creation_time.get())
                >= Duration::from_secs(self.config.max_instance_lifetime_seconds as _);

        let reachable_and_responding = match is_it_time_to_recycle {
            true => None,
            false => {
                // Try to open (and then close) an empty page to ensure the browser is reachable
                // and it responds to commands:
                Some(
                    match self.browser.lock().await.new_page("about:blank").await {
                        Ok(page) => match page.close().await {
                            Ok(_) => true,
                            Err(e) => {
                                warn!("browser instance doesn't look good on page close: {e}");
                                false
                            }
                        },
                        Err(e) => {
                            warn!("browser instance doesn't look good on page open: {e}");
                            false
                        }
                    },
                )
            }
        };

        if is_it_time_to_recycle || reachable_and_responding == Some(false) {
            info!("restarting a browser instance...");
            let mut browser = self.browser.lock().await;
            match browser.close().await {
                Ok(_) => {
                    trace!("browser has been closed successfully");
                    match browser.wait().await {
                        Ok(exit_status) => trace!("browser exit status: {exit_status:?}"),
                        Err(e) => {
                            warn!("browser failed to exit: {e}");
                            match browser.kill().await {
                                Some(Ok(_)) => trace!("browser instance has been killed"),
                                Some(Err(e)) => warn!("failed to kill a browser instance: {e}"),
                                None => trace!("no browser instance seems to be running"),
                            }
                        }
                    };
                }
                Err(e) => {
                    warn!("failed to close a browser: {e}");
                    match browser.kill().await {
                        Some(Ok(_)) => trace!("browser instance has been killed"),
                        Some(Err(e)) => warn!("failed to kill a browser instance: {e}"),
                        None => trace!("no browser instance seems to be running"),
                    }
                }
            }
            let (new_browser, new_browser_handle) = Self::new_browser(&self.config).await?;

            let handle_placeholder = Cell::new(new_browser_handle);
            self.browser_handle.swap(&handle_placeholder);
            let old_browser_handle = handle_placeholder.into_inner();
            if let Err(e) = old_browser_handle.await {
                warn!("failed to join a decomissioned browser instance event handle: {e}")
            }

            debug!("restarted a browser instance");
            *browser = new_browser;
            self.new_pages_requested.set(0);
            self.creation_time.set(Instant::now());
        }

        self.new_pages_requested
            .set(self.new_pages_requested.get().saturating_add(1));

        let page = self.browser.lock().await.new_page("about:blank").await?;

        page.enable_stealth_mode().await?;

        page.set_user_agent(SetUserAgentOverrideParams {
            accept_language: self.config.accept_language.clone(),
            ..SetUserAgentOverrideParams::new(&self.config.user_agent)
        })
        .await?;

        page.execute(
            fetch::EnableParams::builder()
                .pattern(
                    RequestPattern::builder()
                        .request_stage(RequestStage::Request)
                        .build(),
                )
                .pattern(
                    RequestPattern::builder()
                        .request_stage(RequestStage::Response)
                        .build(),
                )
                .build(),
        )
        .await?;

        page.execute(SetDownloadBehaviorParams {
            behavior: SetDownloadBehaviorBehavior::AllowAndName,
            browser_context_id: None,
            download_path: Some(self.config.output_path.clone()),
            events_enabled: Some(true),
        })
        .await?;

        Page::try_new(page, self.config.clone()).await
    }

    /// Attempts to read an URL from a file specified by backend request object ID.
    ///
    /// Given file is expected to contain a single URL, empty lines are ignored.
    pub fn read_url(&self, request: &BackendRequest) -> Result<Url, UrlBackendError> {
        let request_text = fs::read_to_string(
            PathBuf::from(&self.config.objects_path).join(&request.object.object_id),
        )?;

        let mut lines = request_text.lines().filter(|line| !line.is_empty());
        let url = lines.next().ok_or(UrlBackendError::NoUrl)?;
        if lines.next().is_some() {
            return Err(UrlBackendError::MoreThanOneUrl);
        }

        Ok(url.into())
    }
}
