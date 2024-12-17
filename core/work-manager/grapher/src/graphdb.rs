//! Graph database communication
//!
//! This module saves work results into GraphDB as nodes and relations

use shared::amqp::{JobResult, JobResultKind};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

const WORK_TOTAL_TIME: &str = "grapher_work_total_time_seconds";

/// The graph database connector
pub struct GraphDB {
    client: tokio_postgres::Client,
    failure_notifier: std::sync::Arc<tokio::sync::Notify>,
    node_stmt: tokio_postgres::Statement,
    rel_stmt: tokio_postgres::Statement,
}

impl GraphDB {
    /// Creates a new connection to the graph database
    pub async fn new(
        config: &shared::config::DBConfig,
        failure_notifier: std::sync::Arc<tokio::sync::Notify>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        metrics::describe_histogram!(
            WORK_TOTAL_TIME,
            metrics::Unit::Seconds,
            "Time to fully process a work request and generate the result graph"
        );
        let mut pgcfg = tokio_postgres::Config::new();
        pgcfg
            .application_name("grapher")
            .dbname(&config.dbname)
            .host(&config.host)
            .port(config.port)
            .user(&config.user)
            .password(&config.pass)
            .target_session_attrs(tokio_postgres::config::TargetSessionAttrs::ReadWrite);
        let (mut client, conn) = pgcfg.connect(tokio_postgres::NoTls).await.map_err(|e| {
            error!(
                "Failed to connect to graph database {} at {}:{}: {}",
                config.dbname, config.host, config.port, e
            );
            e
        })?;
        let failure_notifier_moved = failure_notifier.clone();
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                error!("Database connection error: {}", e);
                failure_notifier_moved.notify_one();
            }
        });

        if let Err(e) = apply_migrations("migrations", &mut client).await {
            error!("Database migration failed");
            error!(e);
            return Err(e);
        }

        let node_stmt = client
            .prepare(
                "INSERT INTO objects (
                  org, work_id, is_entry, object_id, object_type, object_subtype,
                  recursion_level, size, hashes, t, result, entropy
              ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10), $11, $12)
              RETURNING id",
            )
            .await
            .map_err(|e| {
                error!("Failed to prepare node insert statement: {}", e);
                e
            })?;

        let rel_stmt = client
            .prepare(
                "INSERT INTO rels (
                 parent, child, props
             ) VALUES ($1, $2, $3)",
            )
            .await
            .map_err(|e| {
                error!("Failed to prepare relationship insert statement: {}", e);
                e
            })?;

        debug!(
            "Connected to GraphDB {} at {}:{}",
            config.dbname, config.host, config.port
        );
        Ok(Self {
            client,
            failure_notifier,
            node_stmt,
            rel_stmt,
        })
    }

    /// Reports that a failure occurred
    pub fn notify_failure(&self) {
        self.failure_notifier.notify_one();
        debug!("Failure notification sent");
    }

    /// Turns a work result into a graph and saves it
    pub async fn save_result(
        &mut self,
        result: JobResult,
        work_id: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        async fn save_graph(
            txn: &tokio_postgres::Transaction<'_>,
            start_node: JobResult,
            parent_id: Option<i64>,
            work_id: &str,
            node_stmt: &tokio_postgres::Statement,
            rel_stmt: &tokio_postgres::Statement,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            async fn save_node(
                txn: &tokio_postgres::Transaction<'_>,
                node: JobResult,
                parent_id: Option<i64>,
                work_id: &str,
                node_stmt: &tokio_postgres::Statement,
                rel_stmt: &tokio_postgres::Statement,
            ) -> Result<(i64, Vec<JobResult>), Box<dyn std::error::Error + Send + Sync>>
            {
                let size = i64::try_from(node.info.size).inspect_err(|e| {
                    error!("Object size too large: {}", e);
                })?;
                let recursion_level =
                    i32::try_from(node.info.recursion_level).inspect_err(|e| {
                        error!("Rerecursion level too large: {}", e);
                    })?;
                // Remove and return children from OK results
                let mut result_json = serde_json::json!(&node.result);
                let children: Vec<JobResult> = if let JobResultKind::ok(mut okres) = node.result {
                    result_json["ok"]
                        .as_object_mut()
                        .unwrap()
                        .remove("children");
                    okres.children.drain(..).collect()
                } else {
                    Vec::new()
                };
                replace_nul(&mut result_json);
                let hashes_json = serde_json::json!(node.info.hashes);
                let row = txn
                    .query_one(
                        node_stmt,
                        &[
                            &node.info.org,            // org
                            &work_id,                  // work_id
                            &parent_id.is_none(),      // is_entry,
                            &node.info.object_id,      // object_id
                            &node.info.object_type,    // object_type
                            &node.info.object_subtype, // object_subtype
                            &recursion_level,          // recursion_level
                            &size,                     // size,
                            &hashes_json,              // hashes
                            &node.info.ctime,          // t
                            &result_json,              // result
                            &node.info.entropy,        // entropy
                        ],
                    )
                    .await
                    .map_err(|e| {
                        error!("Failed to insert node: {}", e);
                        e
                    })?;
                let obj_id: i64 = match row.try_get(0) {
                    Ok(id) => id,
                    Err(e) => {
                        error!("Failed to get node ID: {}", e);
                        return Err(e.into());
                    }
                };
                debug!(
                    "Created node \"{}\" with id: {}",
                    node.info.object_id, obj_id
                );
                let mut rel_metadata_json = serde_json::json!(node.relation_metadata);
                replace_nul(&mut rel_metadata_json);
                txn.execute(
                    rel_stmt,
                    &[
                        &parent_id,         // parent
                        &obj_id,            // child
                        &rel_metadata_json, // props
                    ],
                )
                .await
                .map_err(|e| {
                    error!(
                        "Failed to add relationship ({} -> {}): {}",
                        parent_id
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "NULL".to_string()),
                        obj_id,
                        e
                    );
                    e
                })?;
                Ok((obj_id, children))
            }
            let (obj_id, children) =
                save_node(txn, start_node, parent_id, work_id, node_stmt, rel_stmt).await?;
            for child in children {
                Box::pin(save_graph(
                    txn,
                    child,
                    Some(obj_id),
                    work_id,
                    node_stmt,
                    rel_stmt,
                ))
                .await?;
            }
            Ok(())
        }

        let start_time = result.info.work_creation_time();
        debug!("Creating graph for work \"{}\"", work_id);
        let txn = self.client.transaction().await.map_err(|e| {
            error!("Failed to start transaction: {}", e);
            e
        })?;
        if let Err(e) = Box::pin(save_graph(
            &txn,
            result,
            None,
            &work_id,
            &self.node_stmt,
            &self.rel_stmt,
        ))
        .await
        {
            error!("Failed to save work \"{}\": {}", work_id, e);
            txn.rollback().await.map_err(|e| {
                error!(
                    "Failed to rollback transaction for work \"{}\": {}",
                    work_id, e
                );
                e
            })?;
            Err(e)
        } else {
            txn.commit().await.map_err(|e| {
                error!(
                    "Failed to commit transaction for work \"{}\": {}",
                    work_id, e
                );
                e
            })?;
            let elapsed = start_time
                .elapsed()
                .unwrap_or_else(|_| std::time::Duration::from_secs(0));
            info!("Published graph for work \"{}\"", work_id);
            metrics::histogram!(WORK_TOTAL_TIME).record(elapsed.as_secs_f64());
            Ok(())
        }
    }
}

const NUL_REPLACEMENT: &str = "\u{f2b3}";
fn replace_nul(json: &mut serde_json::Value) {
    match json {
        serde_json::Value::String(s) => {
            *s = s.replace('\0', NUL_REPLACEMENT);
        }
        serde_json::Value::Array(ar) => {
            ar.iter_mut().for_each(replace_nul);
        }
        serde_json::Value::Object(map) => {
            // Note: iter_mut will not yield mutable keys, just mutable values
            // so i need to remove and add back
            let keys_to_replace: Vec<String> = map
                .keys()
                .filter(|k| k.contains('\0'))
                .map(|k| k.to_string())
                .collect();
            for mut k in keys_to_replace {
                let v = map.remove(&k).unwrap();
                k = k.replace('\0', NUL_REPLACEMENT);
                map.insert(k, v);
            }
            for (_, v) in map.iter_mut() {
                replace_nul(v);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

async fn apply_migrations<P: AsRef<std::path::Path>>(
    migrations_dir: P,
    db: &mut tokio_postgres::Client,
) -> Result<(), Box<dyn std::error::Error>> {
    let txn = db
        .transaction()
        .await
        .map_err(|e| format!("Failed to start migration transaction: {}", e))?;
    txn.execute("CREATE TABLE IF NOT EXISTS version(v int NOT NULL)", &[])
        .await
        .map_err(|e| format!("Failed to create version table: {}", e))?;
    txn.execute("LOCK TABLE version", &[])
        .await
        .map_err(|e| format!("Failed to lock version table: {}", e))?;
    txn.execute(
        "INSERT INTO VERSION SELECT 0 WHERE NOT EXISTS (SELECT 1 FROM version)",
        &[],
    )
    .await
    .map_err(|e| format!("Failed to initialize version table: {}", e))?;
    let current_version: i32 = txn
        .query_one("SELECT v FROM version", &[])
        .await
        .map_err(|e| format!("Failed to determine the current database version: {}", e))?
        .get(0);
    if current_version < 0 {
        return Err(format!("Invalid database version: {}", current_version).into());
    }
    if current_version > shared::DB_SCHEMA_VERSION {
        return Err(format!(
            "The database version is newer than ours: our version {}, current version {}",
            shared::DB_SCHEMA_VERSION,
            current_version
        )
        .into());
    }
    for migration in 0..shared::DB_SCHEMA_VERSION {
        if migration < current_version {
            debug!("Skipping migration {} (already present)", migration);
            continue;
        }
        let migration_file = migrations_dir
            .as_ref()
            .join(format!("{:06}.sql", migration));
        let migration_body = std::fs::read_to_string(&migration_file).map_err(|e| {
            format!(
                "Failed to find migration file {}: {}",
                migration_file.display(),
                e
            )
        })?;
        debug!("Applying migration {}...", migration);
        txn.batch_execute(&migration_body)
            .await
            .map_err(|e| format!("Failed to apply migration {}: {}", migration, e))?;
        txn.execute("UPDATE version SET v = $1", &[&(migration + 1)])
            .await
            .map_err(|e| {
                format!(
                    "Failed to bump database version to {}: {}",
                    migration + 1,
                    e
                )
            })?;
    }
    txn.commit()
        .await
        .map_err(|e| format!("Failed to commit database migrations: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_replace_nul() {
        let mut json = serde_json::json!({
            "string": "string",
            "number": 123,
            "bool": false,
            "string_with_nul": "string\0nul",
            "ar": [ "string", "string\0nul", 123 ],
            "obj": {
                "key": "value",
                "nul": "nul\0val",
                "ar":[ "another\0nul" ],
                "subobj": {
                    "subkey": "subval",
                    "subnul": "sub\0nul",
                    "nul\0subkey": 123,
                }
            },
            "nul\0key": "someval"
        });
        replace_nul(&mut json);

        let refjson = serde_json::json!({
            "string": "string",
            "number": 123,
            "bool": false,
            "string_with_nul": format!("string{NUL_REPLACEMENT}nul"),
            "ar": [ "string", format!("string{NUL_REPLACEMENT}nul"), 123 ],
            "obj": {
                "key": "value",
                "nul": format!("nul{NUL_REPLACEMENT}val"),
                "ar":[ format!("another{NUL_REPLACEMENT}nul") ],
                "subobj": {
                    "subkey": "subval",
                    "subnul": format!("sub{NUL_REPLACEMENT}nul"),
                    format!("nul{NUL_REPLACEMENT}subkey"): 123,
                }
            },
            format!("nul{NUL_REPLACEMENT}key"): "someval"
        });
        assert_eq!(json, refjson);
    }

    #[test]
    fn test_migration_files() -> Result<(), std::io::Error> {
        let project_dir = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR environment variable should be set");
        let migration_dir = std::path::Path::new(&project_dir).join("migrations");
        for migration in 0..shared::DB_SCHEMA_VERSION {
            let fname = migration_dir.join(format!("{:06}.sql", migration));
            if let Err(e) = std::fs::read_to_string(&fname) {
                panic!(
                    "Migration file {} is missing or not readable: {}",
                    fname.display(),
                    e
                );
            }
        }
        for migration in shared::DB_SCHEMA_VERSION..(shared::DB_SCHEMA_VERSION + 10) {
            let fname = migration_dir.join(format!("{:06}.sql", migration));
            assert!(
                !std::fs::exists(&fname)?,
                "Found migration file {} which is greater than the current version",
                fname.display(),
            );
        }
        Ok(())
    }
}
