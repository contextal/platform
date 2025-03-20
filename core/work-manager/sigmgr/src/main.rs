mod amqp;
mod config;
mod graphdb;
mod watch;

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

    // Run freshclam in one off mode at startup
    let disable_freshclam = config.disable_freshclam.unwrap_or(false);
    if !disable_freshclam {
        debug!("Executing preliminary database update...");
        match std::process::Command::new("freshclam").status() {
            Ok(st) if st.success() => debug!("Successfully updated the upstream ClamAV database"),
            Ok(st) => warn!(
                "Freshclam failed with exit code {}",
                st.code().unwrap_or(-127)
            ),
            Err(e) => warn!("Failed to execute freshclam: {}", e),
        }
    }

    // Connect to the graphdb and write the scenario rules
    let graphdb = graphdb::GraphDB::new(&config).await?;
    graphdb.deploy_clam_dbs().await?;

    // Start freshclam in daemon mode
    let mut freshclam = watch::Freshclam::new(disable_freshclam)?;

    // Start clamd
    let reload_notice = Arc::new(tokio::sync::Notify::new());
    let mut clamd = watch::Clamd::new(reload_notice.clone()).await?;

    // Connect to broker
    let newrules_notice = Arc::new(tokio::sync::Notify::new());
    let broker = amqp::Broker::new(&config.broker, newrules_notice.clone()).await?;

    // Signal handlers
    let mut sigtask = tokio::spawn(async move {
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

    let clamd_watch = clamd.watch();
    let freshclam_watch = freshclam.watch();
    tokio::pin!(clamd_watch);
    tokio::pin!(freshclam_watch);
    let mut rng = rand::rng();
    let mut interval = make_interval(&mut rng).await;
    let mut newrules_arrived = false;

    info!("Sigmgr watch loop started");
    let ret = loop {
        tokio::select!(
            _ = &mut clamd_watch => break Err("Clamd error".into()),
            _ = &mut freshclam_watch => break Err("Freshclam error".into()),
            _ = newrules_notice.notified() => newrules_arrived = true,
            _ = interval.tick() => {
                if newrules_arrived {
                    debug!("New rules arrived");
                    if let Err(e) = graphdb.deploy_clam_dbs().await {
                        break Err(e);
                    }
                    reload_notice.notify_one();
                    newrules_arrived = false;
                    interval = make_interval(&mut rng).await;
                    interval.reset();
                }
            }
            _ = &mut sigtask => {
                info!("Signal caught, exiting");
                break Ok(())
            }
        );
    };
    broker.close().await;
    ret
}

async fn make_interval(rng: &mut rand::rngs::ThreadRng) -> tokio::time::Interval {
    use rand::Rng;
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(
        rng.random_range(0..60_000),
    ));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    debug!("Reload check set to {}", interval.period().as_millis());
    interval
}
