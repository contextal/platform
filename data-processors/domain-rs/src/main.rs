mod config;

use backend_utils::objects::*;
use domain_rs::{DomainInfo, DomainQuery};
use serde::Serialize;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use tokio::runtime::Runtime;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

#[derive(Serialize, Default)]
struct DomainMeta<'a> {
    creation_date: Option<String>,
    update_date: Option<String>,
    expiration_date: Option<String>,
    age_days: Option<i64>,
    name_servers: Option<&'a [String]>,

    registrar: Option<&'a str>,
    status: Option<&'a str>,
    registrant_name: Option<&'a str>,
    registrant_org: Option<&'a str>,
    admin_name: Option<&'a str>,
    admin_org: Option<&'a str>,
    tech_name: Option<&'a str>,
    tech_org: Option<&'a str>,
}

impl<'a> From<&'a DomainInfo> for DomainMeta<'a> {
    fn from(di: &'a DomainInfo) -> Self {
        Self {
            creation_date: di.created.map(|d| d.to_string()),
            update_date: di.updated.map(|d| d.to_string()),
            expiration_date: di.expiry.map(|d| d.to_string()),
            age_days: di.created.map(|dt| {
                (time::OffsetDateTime::now_utc() - dt.midnight().assume_utc()).whole_days()
            }),
            name_servers: di.nss.as_deref(),
            registrar: di.registrar.as_deref(),
            status: di.status.as_deref(),
            registrant_name: di.registrant_name.as_deref(),
            registrant_org: di.registrant_org.as_deref(),
            admin_name: di.admin_name.as_deref(),
            admin_org: di.admin_org.as_deref(),
            tech_name: di.tech_name.as_deref(),
            tech_org: di.tech_org.as_deref(),
        }
    }
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
async fn process_domain(
    request: &BackendRequest,
    config: &config::Config,
    dq: &RefCell<DomainQuery>,
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
    let mut symbols = vec![];
    let mut dq_mut = dq.borrow_mut();
    let domain_meta: DomainMeta = if let Ok(domain_data) = dq_mut
        .query(
            domain,
            &std::time::Duration::from_secs(u64::from(config.query_timeout_secs.unwrap_or(8))),
        )
        .await
    {
        if let Some(di) = domain_data {
            di.into()
        } else {
            symbols.push("NO_DATA".into());
            DomainMeta::default()
        }
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Couldn't query WHOIS database",
        ));
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

async fn process_request(
    request: &BackendRequest,
    config: &config::Config,
    dq: &RefCell<DomainQuery>,
) -> Result<BackendResultKind, std::io::Error> {
    match process_domain(request, config, dq).await {
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
    let dq = RefCell::new(DomainQuery::new());
    let runtime = Runtime::new()?;

    backend_utils::work_loop!(config.host.as_deref(), config.port, |request| {
        runtime.block_on(async { process_request(request, &config, &dq).await })
    })?;

    Ok(())
}
