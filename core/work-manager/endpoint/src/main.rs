//! The API endpoint
//!
//! Retrieves work requests, saves the stream into an object and
//! initiates the work processing
//!
//! For the API see the [httpd] module
mod amqp;
mod config;
mod graphdb;
mod httpd;

use actix_web::{
    dev::Service,
    {web, App, HttpServer},
};
use metrics_exporter_prometheus::PrometheusBuilder;
use shared::clamd;
use std::net::{Ipv4Addr, SocketAddr};
use tokio::sync::mpsc;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};
use tracing_subscriber::prelude::*;

const REQUEST_COUNT: &str = "endpoint_requests_total";
const RESPONSE_TIME: &str = "endpoint_response_time_seconds";
const WORK_COUNT: &str = "endpoint_work_requests_total";

/// A single thread dedicated to posting job request to the broker
///
/// AMQP channels shall and can not be shared/cloned, making it difficult
/// to publish requests directly from within the HTTP request handlers
///
/// This function receives the object to process, posts the request and
/// returns its id via tokio channels
async fn post_requests(
    broker: amqp::Broker,
    mut rx: mpsc::Receiver<httpd::BrokerAction>,
) -> Result<(), std::io::Error> {
    info!("Request publisher started");
    while let Some(act) = rx.recv().await {
        match act {
            httpd::BrokerAction::Job(req) => {
                let object_id = req.object.object_id.clone();
                if let Ok(id) = broker
                    .publish_job_request(
                        req.object,
                        req.ttl,
                        req.max_recursion,
                        req.relation_metadata,
                    )
                    .await
                {
                    info!("Published request for \"{}\" (id: \"{}\")", object_id, id);
                    metrics::counter!(WORK_COUNT).increment(1);
                    if let Err(e) = req.reply_tx.send(id) {
                        error!("Failed to report request id to httpd: {}", e);
                        break;
                    }
                } else {
                    error!("Failed to publish job request");
                    break;
                }
            }
            httpd::BrokerAction::Reload => {
                if broker.request_reload_scenarios().await.is_err() {
                    error!("Failed to publish scenario reload request");
                    break;
                }
            }
            httpd::BrokerAction::ApplyScenarios(work_ids) => {
                if broker.request_apply_scenarios(work_ids).await.is_err() {
                    error!("Failed to publish apply scenarios request");
                    break;
                }
            }
        }
    }
    broker.close().await;
    info!("Request publisher exited");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = crate::config::Config::new()?;
    let prom = web::Data::new(PrometheusBuilder::new().install_recorder().map_err(|e| {
        error!("Failed to setup the prometheus builder: {}", e);
        e
    })?);
    let graphdb = web::Data::new(graphdb::GraphDB::new(&config).await.map_err(|e| {
        error!("Failed to setup the database pool: {}", e);
        e
    })?);
    metrics::describe_counter!(REQUEST_COUNT, "Total number of HTTP requests");
    metrics::describe_histogram!(RESPONSE_TIME, metrics::Unit::Seconds, "HTTP response time");
    metrics::describe_counter!(WORK_COUNT, "Number of work requests published");

    let clamd = web::Data::new(clamd::Typedet::new(&config.typedet));
    let broker = amqp::Broker::new(&config.broker).await?;
    let (tx, rx) = mpsc::channel::<httpd::BrokerAction>(100);
    let data_tx = web::Data::new(tx.downgrade());
    let max_submit_size = config.get_max_submit_size();
    let limits = web::Data::new(httpd::Limits {
        max_work_results: config.get_max_work_results(),
        max_search_results: config.get_max_search_results(),
        max_action_results: config.get_max_action_results(),
    });
    let is_reprocess_enabled = web::Data::new(config.is_reprocess_enabled());
    let objects_path = web::Data::new(config.objects_path);

    let server = HttpServer::new(move || {
        App::new()
            .wrap_fn(|req, srv| {
                // All requests are wrapped for prometheus accounting
                let path = req.path();
                let endpoint = req
                    .resource_map()
                    .match_name(path)
                    .unwrap_or("<invalid>")
                    .to_string();
                let fut = srv.call(req);
                let start = std::time::Instant::now();
                async move {
                    let res = fut.await?;
                    metrics::counter!(
                        REQUEST_COUNT,
                        "endpoint" => endpoint.clone(),
                        "status" => res.status().as_str().to_string(),
                    )
                    .increment(1);
                    metrics::histogram!(
                        RESPONSE_TIME,
                        "endpoint" => endpoint,
                    )
                    .record(start.elapsed().as_secs_f64());
                    Ok(res)
                }
            })
            .configure(httpd::app_setup)
            .app_data(data_tx.clone())
            .app_data(clamd.clone())
            .app_data(prom.clone())
            .app_data(objects_path.clone())
            .app_data(graphdb.clone())
            .app_data(
                actix_multipart::form::MultipartFormConfig::default().total_limit(max_submit_size),
            )
            .app_data(limits.clone())
            .app_data(is_reprocess_enabled.clone())
    })
    .bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, config.port)));
    let server = match server {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to create HTTP server: {}", e);
            broker.close().await;
            return Err(e.into());
        }
    };
    let mut jset = tokio::task::JoinSet::new();
    jset.spawn(post_requests(broker, rx));
    jset.spawn(server.run());

    // Await termination of either future
    jset.join_next().await;

    // Close the publisher channel - the publish future will exit
    // if it's still alive
    drop(tx);

    // Allow for graceful shutdown
    tokio::select!(
        _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
        _ = jset.join_next() => {}
    );

    // Kill everything anyway
    jset.shutdown().await;

    Ok(())
}
