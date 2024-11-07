mod amqp;
mod config;
mod graph;

use metrics_exporter_prometheus::PrometheusBuilder;
use std::sync::Arc;
use tokio::signal::unix;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = config::Config::new()?;
    PrometheusBuilder::new().install().map_err(|e| {
        error!("Failed to setup the prometheus builder: {}", e);
        e
    })?;

    let failure_notice = Arc::new(tokio::sync::Notify::new());
    let sigtask = tokio::spawn(async move {
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

    let graphdb = graph::GraphDB::new(&config.write_db, failure_notice.clone()).await?;
    let broker = amqp::Broker::new(&config.broker, graphdb).await?;

    info!("Grapher started");
    let ret = tokio::select!(
        _ = failure_notice.notified() => Err("Exiting due to error condition".into()),
        _ = sigtask => {
            info!("Signal caught, exiting");
            Ok(())
        }
    );
    broker.close().await;
    ret
}
