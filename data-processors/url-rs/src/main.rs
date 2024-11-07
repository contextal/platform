//! URL backend
use backend_utils::objects::{
    BackendRequest, BackendResultChild, BackendResultKind, BackendResultOk,
};
use regex::Regex;
use scopeguard::ScopeGuard;
use serde::Serialize;
use std::{collections::HashSet, env};
use tokio::{
    fs,
    runtime::{Handle, Runtime},
};
use tracing::{error, trace};
use tracing_subscriber::prelude::*;
use url_rs::{
    backend_state::Backend, config::Config, error::UrlBackendError, BackendResultSymbols,
};

fn main() -> Result<(), UrlBackendError> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::new()?;
    let runtime = Runtime::new()?;
    let backend_state = runtime.block_on(async { Backend::new(config).await })?;

    backend_utils::work_loop!(None, None, |request| {
        runtime.block_on(async { process_request(request, &backend_state).await })
    })?;

    Ok(())
}

/// Attributes for an URL from a backend request.
#[derive(Debug, Serialize)]
struct UrlMetadata {}

async fn process_request(
    request: &BackendRequest,
    backend: &Backend,
) -> Result<BackendResultKind, UrlBackendError> {
    let url = match backend.read_url(request) {
        Ok(v) => v,
        Err(e) => {
            let message = format!("failed to read an URL from request: {e}");
            error!(message);
            return Ok(BackendResultKind::error(message));
        }
    };

    let mut symbols: HashSet<&str> = HashSet::new();
    let metadata = UrlMetadata {};

    let mut page = match backend.new_page().await {
        Ok(v) => v,
        Err(e) => {
            let message = format!("failed to setup a new page: {e}");
            error!(message);
            return Ok(BackendResultKind::error(message));
        }
    };

    if let Err(e) = page.navigate_to(url).await {
        let message = format!("failed to navigate: {e}");
        error!(message);
        return Ok(BackendResultKind::error(message));
    }
    page.wait_for_navigation().await;
    let max_children = backend.config.max_children;

    let mut children = scopeguard::guard(Vec::<BackendResultChild>::new(), |children| {
        Handle::current().spawn(async {
            for file in children.into_iter().filter_map(|child| child.path) {
                let _ = fs::remove_file(file).await;
            }
        });
    });

    let current_url = match page.url().await {
        Ok(v) => v,
        Err(e) => {
            let message = format!("failed to obtain page URL: {e}");
            error!(message);
            return Ok(BackendResultKind::error(message));
        }
    };

    if current_url.as_deref() == Some("about:blank") || current_url.is_none() {
        trace!("skipping child creation for page visuals-and-content")
    } else {
        if backend.config.perform_print_to_pdf && children.len() < max_children {
            match page.print_to_pdf().await {
                Ok(child) => children.push(child),
                Err(e) => {
                    let message = e.to_string();
                    error!(message);
                    return Ok(BackendResultKind::error(message));
                }
            }
        }

        if children.len() < max_children {
            match page.get_content().await {
                Ok(child) => children.push(child),
                Err(e) => {
                    let message = e.to_string();
                    error!(message);
                    return Ok(BackendResultKind::error(message));
                }
            };
        }

        if backend.config.take_screenshot && children.len() < max_children {
            match page.capture_screenshot().await {
                Ok(child) => children.push(child),
                Err(e) => {
                    let message = e.to_string();
                    error!(message);
                    return Ok(BackendResultKind::error(message));
                }
            }
        }
    }

    let (responses, download_slot) = match page.close_and_consume().await {
        Ok(v) => v,
        Err(e) => {
            let message = e.to_string();
            error!(message);
            return Ok(BackendResultKind::error(message));
        }
    };

    let re_ignored_urls = Regex::new(
        r#"(?xi)
        ^(chrome-error://|background-color:)
        "#,
    )
    .expect("invalid ignored URLs regex");

    for (_, response) in responses {
        if let Some(rtype) = &response.resource_type {
            if backend.config.excluded_resource_types.contains(rtype) {
                continue;
            }
        }
        // When Chromium/Chrome starts a file-download it fires `LoadingFailed` event (probably to
        // signal some "virtual" cancellation of original HTTP request).
        // To avoid creation a misleading child for this "virtually-canceled" HTTP request it makes
        // sense to just skip matching response:
        if let Some(download) = &download_slot {
            if response.canceled
                && response.error_text.as_deref() == Some("net::ERR_ABORTED")
                && response.url == download.url
            {
                trace!("skipping original HTTP request which initiated a file-download");
                continue;
            }
        }

        if !backend.config.save_original_response
            && Some(response.url.0.as_str()) == current_url.as_deref()
        {
            trace!("skipping original HTTP response for {current_url:?}");
            continue;
        }

        if re_ignored_urls.is_match(response.url.0.as_str()) {
            trace!("skipping ignored url {:?}", response.url.0.as_str());
            continue;
        }

        match response.consume().await {
            Ok((child, BackendResultSymbols(response_symbols))) => {
                if children.len() < max_children {
                    children.push(child);
                }
                symbols.extend(response_symbols);
            }
            Err(e) => {
                error!("{}", e.to_string());
                return Err(e);
            }
        }
    }

    if let Some(download) = download_slot {
        if children.len() < max_children {
            match download.consume().await {
                Ok((child, BackendResultSymbols(download_symbols))) => {
                    children.push(child);
                    symbols.extend(download_symbols);
                }
                Err(e) => {
                    let message = e.to_string();
                    error!(message);
                    return Ok(BackendResultKind::error(message));
                }
            }
        }
    }

    if !symbols.contains("LIMITS_REACHED") {
        for child in children.iter() {
            if child.symbols.contains(&"TOOBIG".to_string()) {
                symbols.insert("LIMITS_REACHED");
                break;
            }
        }
    }

    let mut symbols: Vec<String> = symbols.into_iter().map(String::from).collect();
    symbols.sort();

    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(metadata)? {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        },
        children: ScopeGuard::into_inner(children),
    }))
}
