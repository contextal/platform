//! Graph database communication
//!
//! This module interacts with the GraphDB

use futures::{pin_mut, stream::TryStreamExt};
use std::os::unix::fs::PermissionsExt;
use tokio::io::AsyncWriteExt;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

/// The graph database connector
pub struct GraphDB {
    read_client: tokio_postgres::Client,
}

impl GraphDB {
    /// Creates a new connection to the graph database
    pub async fn new(config: &crate::config::Config) -> Result<Self, Box<dyn std::error::Error>> {
        // Read side
        let read_config = &config.read_db;
        let mut pgcfg = tokio_postgres::Config::new();
        pgcfg
            .application_name("sigmgr")
            .dbname(&read_config.dbname)
            .host(&read_config.host)
            .port(read_config.port)
            .user(&read_config.user)
            .password(&read_config.pass)
            .target_session_attrs(tokio_postgres::config::TargetSessionAttrs::Any);
        let (read_client, read_conn) = pgcfg.connect(tokio_postgres::NoTls).await.map_err(|e| {
            error!(
                "Failed to connect to graph database {} at {}:{}: {}",
                read_config.dbname, read_config.host, read_config.port, e
            );
            e
        })?;
        tokio::spawn(async move {
            if let Err(e) = read_conn.await {
                error!("Read connection error: {}", e);
            }
        });
        let db_version: i32 = read_client
            .query_one("SELECT v FROM version", &[])
            .await
            .map_err(|e| format!("Failed to query db version: {}", e))?
            .get(0);
        if db_version != shared::DB_SCHEMA_VERSION {
            error!(
                "Wrong database version (read-only): expected {}, found {}",
                shared::DB_SCHEMA_VERSION,
                db_version
            );
            return Err("Wrong database version".into());
        }
        debug!(
            "Connected to GraphDB {} at {}:{} in read-only mode",
            read_config.dbname, read_config.host, read_config.port
        );
        Ok(Self { read_client })
    }

    pub async fn deploy_clam_dbs(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Load signatures from scenarios
        let mut signatures: Vec<String> = Vec::new();
        let scn_it = self
            .read_client
            .query_raw("SELECT id, def FROM scenarios", &[] as &[&str])
            .await?;
        pin_mut!(scn_it);
        while let Some(row) = scn_it.try_next().await? {
            let id: i64 = row.try_get("id")?;
            let json_scenario: serde_json::Value = row.try_get("def")?;
            let scenario: shared::scene::Scenario = match serde_json::from_value(json_scenario) {
                Ok(v) => v,
                Err(e) => {
                    warn!("Skipping invalid scenario (id {}): {}", id, e);
                    continue;
                }
            };
            if let Some(max_ver) = scenario.max_ver {
                if shared::SCN_VERSION > max_ver {
                    warn!(
                        "Scenario {} (id {}) skipped due to unsatisfied maximum version requirements ({} > {})",
                        scenario.name, id, shared::SCN_VERSION, max_ver
                    );
                }
                continue;
            }
            debug!("Processing scenario {} (id {})", scenario.name, id);
            let rule_sigs = pgrules::parse_and_extract_clam_signatures(&scenario.local_query);
            if let Err(e) = rule_sigs {
                warn!(
                    "Scenario {} (id {}) skipped due to invalid local rule ({}): {}",
                    scenario.name, id, scenario.local_query, e
                );
                continue;
            }
            signatures.append(&mut rule_sigs.unwrap());
            if let Some(ctx) = scenario.context {
                match pgrules::parse_and_extract_clam_signatures(&ctx.global_query) {
                    Ok(mut rule_sigs) => signatures.append(&mut rule_sigs),
                    Err(e) => {
                        warn!(
                            "Scenario {} (id {}) skipped due to invalid rule ({}): {}",
                            scenario.name, id, scenario.local_query, e
                        );
                    }
                }
            }
        }
        signatures.sort_unstable();
        signatures.dedup();

        // Save signatures to ClamAV database
        let target_dir = std::path::Path::new("/var/lib/clamav");
        let target_file = target_dir.join("ctxtal.ndb");
        if signatures.is_empty() {
            // Clamd will not load an empty db file
            tokio::fs::remove_file(target_file).await.or_else(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    error!("Failed to remove old ClamAV database: {}", e);
                    Err(e.into())
                }
            })
        } else {
            let (tmp_name, mut f) = shared::utils::mktemp(target_dir, None).await?;
            match async {
                for sig in &signatures {
                    f.write_all(format!("{}\n", sig).as_bytes()).await?;
                }
                f.flush().await?;
                let mut perms = f.metadata().await?.permissions();
                perms.set_mode(0o644);
                f.set_permissions(perms).await?;
                tokio::fs::rename(tmp_name, &target_file).await?;
                debug!("{} scenarios signature(s) deployed", signatures.len());
                Ok(())
            }
            .await
            {
                Err(e) => {
                    error!("Failed to create new ClamAV database: {}", e);
                    tokio::fs::remove_file(&target_file).await.ok();
                    Err(e)
                }
                Ok(()) => Ok(()),
            }
        }
    }
}
