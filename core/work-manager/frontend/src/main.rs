//! Job processor
//!
//! Retrieves jobs of the configured type, have them processed by the backend,
//! publishes the result
mod config;
mod wrkmgr;
use metrics_exporter_prometheus::PrometheusBuilder;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config = crate::config::Config::new()?;
    PrometheusBuilder::new().install().map_err(|e| {
        error!("Failed to setup the prometheus builder: {}", e);
        e
    })?;
    let manager = wrkmgr::WorkManager::new(config).await?;
    if manager.manage_jobs().await {
        Ok(())
    } else {
        Err("Exiting due to error condition".into())
    }
}
