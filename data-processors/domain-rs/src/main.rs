mod config;
mod whos_timeout;

use backend_utils::objects::*;
use chrono::Utc;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

#[derive(Serialize)]
struct DomainMeta {
    creation_date: Option<String>,
    expiration_date: Option<String>,
    age_days: Option<i64>,
    name_servers: Option<Vec<String>>,
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_domain(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, std::io::Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    let request_text: String = fs::read_to_string(input_name)?;
    let mut lines = request_text.lines().filter(|line: &&str| !line.is_empty());
    let domain: &str = lines.next().ok_or(std::io::ErrorKind::InvalidData)?;
    if lines.next().is_some() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Multiple lines found in input data",
        ));
    }

    info!("Checking {}", domain);
    let mut creation_date = None;
    let mut expiration_date = None;
    let mut age_days = None;
    let mut name_servers = None;

    if let Ok(domain_data) = whos_timeout::domain(
        domain,
        &std::time::Duration::from_secs(u64::from(config.query_timeout_secs.unwrap_or(15))),
    ) {
        if let Some(data) = domain_data {
            creation_date = data.created.map(|dt| dt.to_string());
            expiration_date = data.expiry.map(|dt| dt.to_string());
            if let Some(creation) = data.created {
                age_days = Some((Utc::now() - creation).num_days());
            }
            name_servers = Some(data.name_servers);
        }
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Couldn't query WHOIS database",
        ));
    }

    let mut symbols = vec![];
    if creation_date.is_none() && expiration_date.is_none() && name_servers.is_none() {
        symbols.push("NO_DATA".into());
    }

    let domain_meta = DomainMeta {
        creation_date,
        expiration_date,
        age_days,
        name_servers,
    };
    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(domain_meta).unwrap() {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        },
        children: Vec::new(),
    }))
}

fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, std::io::Error> {
    match process_domain(request, config) {
        Ok(d) => Ok(d),
        Err(e) => Ok(BackendResultKind::error(format!(
            "Error processing domain: {}",
            e
        ))),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = config::Config::new()?;
    backend_utils::work_loop!(config.host.as_deref(), config.port, |request| {
        process_request(request, &config)
    })?;
    unreachable!()
}
