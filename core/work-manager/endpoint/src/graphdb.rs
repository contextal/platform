//! Graph database communication
//!
//! This module interacts with the GraphDB

mod clam;

use shared::{
    amqp::{JobResult, JobResultKind},
    scene,
};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

pub enum SearchError {
    Rule(String),
    Query(String),
    Timeout,
    Internal,
}

#[derive(serde::Serialize)]
pub struct CountResult {
    count: i64,
}

pub enum ScenaryError {
    Invalid(&'static str),
    Signature(String),
    Database,
    Duplicate,
    NotFound,
    Internal,
}

#[derive(serde::Serialize)]
pub struct ScenarioDetails {
    id: i64,
    name: String,
    creator: String,
    description: String,
    #[serde(serialize_with = "shared::time_to_f64")]
    t: std::time::SystemTime,
    action: String,
}

/// The graph database connector
#[derive(Clone)]
pub struct GraphDB {
    read_pool: deadpool_postgres::Pool,
    write_pool: deadpool_postgres::Pool,
    search_timeout_ms: u32,
}

impl GraphDB {
    /// Creates a new connection to the graph database
    pub async fn new(config: &crate::config::Config) -> Result<Self, Box<dyn std::error::Error>> {
        let search_timeout_ms = config.get_search_timeout_ms();
        let read_config = &config.read_db;
        let mut pgcfg = tokio_postgres::Config::new();
        pgcfg
            .application_name("endpoint_ro")
            .dbname(&read_config.dbname)
            .host(&read_config.host)
            .port(read_config.port)
            .user(&read_config.user)
            .password(&read_config.pass)
            .target_session_attrs(tokio_postgres::config::TargetSessionAttrs::Any);
        let manager = deadpool_postgres::Manager::from_config(
            pgcfg,
            tokio_postgres::NoTls,
            deadpool_postgres::ManagerConfig {
                recycling_method: deadpool_postgres::RecyclingMethod::Fast,
            },
        );
        let read_pool = deadpool_postgres::Pool::builder(manager)
            .max_size(16)
            .build()
            .expect("Internal error: no runtime");
        debug!(
            "Connection pool created for GraphDB {} at {}:{}...",
            read_config.dbname, read_config.host, read_config.port
        );
        let client = read_pool.get().await.map_err(|e| {
            error!("Failed to get read-only connection from pool: {e}");
            e
        })?;
        let db_version: i32 = client
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

        let write_config = &config.write_db;
        let mut pgcfg = tokio_postgres::Config::new();
        pgcfg
            .application_name("endpoint_rw")
            .dbname(&write_config.dbname)
            .host(&write_config.host)
            .port(write_config.port)
            .user(&write_config.user)
            .password(&write_config.pass)
            .target_session_attrs(tokio_postgres::config::TargetSessionAttrs::ReadWrite);
        let manager = deadpool_postgres::Manager::from_config(
            pgcfg,
            tokio_postgres::NoTls,
            deadpool_postgres::ManagerConfig {
                recycling_method: deadpool_postgres::RecyclingMethod::Fast,
            },
        );
        let write_pool = deadpool_postgres::Pool::builder(manager)
            .max_size(4)
            .build()
            .expect("Internal error: no runtime");
        debug!(
            "Connection pool created for Scenarios DB {} at {}:{}...",
            write_config.dbname, write_config.host, write_config.port
        );
        let client = write_pool.get().await.map_err(|e| {
            error!("Failed to get read-write connection from pool: {e}");
            e
        })?;
        let db_version: i32 = client
            .query_one("SELECT v FROM version", &[])
            .await
            .map_err(|e| format!("Failed to query db version: {}", e))?
            .get(0);
        if db_version != shared::DB_SCHEMA_VERSION {
            error!(
                "Wrong database version (read-write): expected {}, found {}",
                shared::DB_SCHEMA_VERSION,
                db_version
            );
            return Err("Wrong database version".into());
        }
        Ok(Self {
            read_pool,
            write_pool,
            search_timeout_ms,
        })
    }

    pub async fn get_work_graph(
        &self,
        work_id: &str,
    ) -> Result<Option<JobResult>, Box<dyn std::error::Error>> {
        async fn walk(
            parent_row: tokio_postgres::Row,
            client: &deadpool_postgres::Object,
            stmt: &tokio_postgres::Statement,
        ) -> Result<JobResult, Box<dyn std::error::Error>> {
            let parent_id: i64 = parent_row.try_get("id")?;
            let mut parent = row2jobresult(&parent_row)?;
            match parent.result {
                JobResultKind::error(_) => {}
                JobResultKind::ok(ref mut p_result) => {
                    let rows = client.query(stmt, &[&parent_id]).await.map_err(|e| {
                        error!("Failed to execute get_chilren statement: {}", e);
                        e
                    })?;
                    for row in rows {
                        let child = Box::pin(walk(row, client, stmt)).await?;
                        p_result.children.push(child);
                    }
                }
            }
            Ok(parent)
        }

        let client = self.read_pool.get().await.map_err(|e| {
            error!("Failed to get read-only connection from pool: {e}");
            e
        })?;
        let get_parent_stmt = client
            .prepare_cached(
                "SELECT
                   objects.id, objects.org, objects.object_id,
                   objects.object_type, objects.object_subtype,
                   objects.recursion_level, objects.size, objects.hashes, objects.t,
                   objects.entropy, objects.result, rels.props AS relation_metadata
                 FROM objects
                 LEFT JOIN rels ON objects.id = rels.child
                 WHERE objects.work_id = $1 AND objects.is_entry",
            )
            .await
            .map_err(|e| {
                error!("Failed to prepare get_parent statement: {}", e);
                e
            })?;
        let get_children_stmt = client
            .prepare_cached(
                "SELECT
                   objects.id, objects.org, objects.object_id,
                   objects.object_type, objects.object_subtype,
                   objects.recursion_level, objects.size, objects.hashes, objects.t,
                   objects.entropy, objects.result, rels.props AS relation_metadata
                 FROM objects, rels
                 WHERE objects.id = rels.child AND rels.parent = $1
                 ORDER BY objects.id",
            )
            .await
            .map_err(|e| {
                error!("Failed to prepare get_children statement: {}", e);
                e
            })?;
        let row = client
            .query_opt(&get_parent_stmt, &[&work_id])
            .await
            .map_err(|e| {
                error!("Failed to execute get_parent statement: {}", e);
                e
            })?;
        let entry = if let Some(row) = row {
            Some(walk(row, &client, &get_children_stmt).await?)
        } else {
            None
        };
        Ok(entry)
    }

    pub async fn search(
        &self,
        q: &str,
        getobjects: bool,
        max_items: u32,
    ) -> Result<Vec<String>, SearchError> {
        let parsed = pgrules::parse_to_sql(q).map_err(|e| SearchError::Rule(e.to_string()))?;
        let query = format!(
            "SELECT {} {} LIMIT {}",
            if getobjects {
                "object_id"
            } else {
                "distinct work_id"
            },
            parsed,
            max_items
        );
        let mut client = self.read_pool.get().await.map_err(|e| {
            error!("Failed to get read-only connection from pool: {e}");
            SearchError::Internal
        })?;
        let txn = client.transaction().await.map_err(|e| {
            error!("Failed to start transaction: {e}");
            SearchError::Internal
        })?;
        txn.query(
            &format!("SET LOCAL statement_timeout = {}", self.search_timeout_ms),
            &[],
        )
        .await
        .map_err(|e| {
            error!("Failed to set transaction timeout: {e}");
            SearchError::Internal
        })?;
        let rows = txn.query(&query, &[]).await.map_err(|e| {
            match e.code() {
                Some(sqst)
                    if *sqst
                        == deadpool_postgres::tokio_postgres::error::SqlState::QUERY_CANCELED =>
                {
                    SearchError::Timeout
                }
                Some(sqst) if sqst.code().starts_with("42") => {
                    // Syntax error class (42XXX)
                    SearchError::Query(e.to_string())
                }
                _ => {
                    error!("Failed to execute search statement: {}", e);
                    SearchError::Internal
                }
            }
        })?;
        if let Err(e) = txn.commit().await {
            warn!("Failed to commit search transaction: {e}");
        }
        let mut items: Vec<String> = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(row.try_get(0).map_err(|_| SearchError::Internal)?);
        }
        Ok(items)
    }

    pub async fn count(&self, q: &str, getobjects: bool) -> Result<CountResult, SearchError> {
        let parsed = pgrules::parse_to_sql(q).map_err(|e| SearchError::Rule(e.to_string()))?;
        let query = format!(
            "SELECT count({}) {}",
            if getobjects { "*" } else { "distinct work_id" },
            parsed
        );
        let mut client = self.read_pool.get().await.map_err(|e| {
            error!("Failed to get read-only connection from pool: {e}");
            SearchError::Internal
        })?;
        let txn = client.transaction().await.map_err(|e| {
            error!("Failed to start transaction: {e}");
            SearchError::Internal
        })?;
        txn.query(
            &format!("SET LOCAL statement_timeout = {}", self.search_timeout_ms),
            &[],
        )
        .await
        .map_err(|e| {
            error!("Failed to set transaction timeout: {e}");
            SearchError::Internal
        })?;
        let count = txn
            .query_one(&query, &[])
            .await
            .and_then(|row| row.try_get::<_, i64>(0))
            .map_err(|e| match e.code() {
                Some(sqst) if sqst.code().starts_with("42") => {
                    // Syntax error class (42XXX)
                    SearchError::Query(e.to_string())
                }
                _ => {
                    error!("Failed to execute search statement: {}", e);
                    SearchError::Internal
                }
            })?;
        if let Err(e) = txn.commit().await {
            warn!("Failed to commit search transaction: {e}");
        }
        Ok(CountResult { count })
    }

    pub async fn add_scenario(
        &self,
        scenario: &scene::Scenario,
        replace_id: Option<i64>,
    ) -> Result<ScenarioDetails, ScenaryError> {
        if scenario.name.is_empty() {
            return Err(ScenaryError::Invalid("Invalid name"));
        }
        if let Some(max_ver) = scenario.max_ver {
            if max_ver < scenario.min_ver {
                return Err(ScenaryError::Invalid("Invalid version range"));
            }
        }
        if scenario.creator.is_empty() {
            return Err(ScenaryError::Invalid("Invalid creator"));
        }
        if scenario.action.is_empty() {
            return Err(ScenaryError::Invalid("Invalid action"));
        }
        if pgrules::parse_to_sql(&scenario.local_query).is_err() {
            return Err(ScenaryError::Invalid("Invalid local rule"));
        }
        clam::find_invalid_patttern(&scenario.local_query).await?;
        if let Some(context) = &scenario.context {
            if context.min_matches == 0 {
                return Err(ScenaryError::Invalid("Invalid min_matches"));
            }
            if pgrules::parse_to_sql(&context.global_query).is_err() {
                return Err(ScenaryError::Invalid("Invalid context rule"));
            }
            clam::find_invalid_patttern(&context.global_query).await?;
        }
        let mut client = self.write_pool.get().await.map_err(|e| {
            error!("Failed to get read-write connection from pool: {e}");
            ScenaryError::Database
        })?;
        let json_scenario = serde_json::to_value(scenario);
        if json_scenario.is_err() {
            return Err(ScenaryError::Invalid("Invalid scenario"));
        }
        let json_scenario = json_scenario.unwrap(); // checked above
        let txn = client.transaction().await.map_err(|e| {
            error!("Failed to start transaction: {}", e);
            ScenaryError::Database
        })?;
        if let Some(replace_id) = replace_id {
            let stmt = txn
                .prepare_cached("DELETE FROM scenarios WHERE id = $1")
                .await
                .map_err(|e| {
                    error!("Failed to prepare add_scenario statement: {}", e);
                    ScenaryError::Database
                })?;
            let deleted = txn.execute(&stmt, &[&replace_id]).await.map_err(|e| {
                error!("Failed to delete to-be-replaced scenario: {}", e);
                ScenaryError::Database
            })?;
            if deleted == 0 {
                return Err(ScenaryError::NotFound);
            }
        }
        let stmt = txn
            .prepare_cached("INSERT INTO scenarios (name, def) VALUES ($1, $2) RETURNING id, t")
            .await
            .map_err(|e| {
                error!("Failed to prepare add_scenario statement: {}", e);
                ScenaryError::Database
            })?;
        let row = txn
            .query_one(&stmt, &[&scenario.name, &json_scenario])
            .await
            .map_err(|e| match e.code() {
                Some(sqst)
                    if *sqst
                        == deadpool_postgres::tokio_postgres::error::SqlState::UNIQUE_VIOLATION =>
                {
                    ScenaryError::Duplicate
                }
                _ => {
                    error!("Failed to execute add_scenario statement: {}", e);
                    ScenaryError::Database
                }
            })?;
        txn.commit().await.map_err(|e| {
            error!("Failed to commit add_scenario transaction: {}", e);
            ScenaryError::Database
        })?;
        Ok(ScenarioDetails {
            id: row.try_get("id").map_err(|_| ScenaryError::Database)?,
            name: scenario.name.clone(),
            creator: scenario.creator.clone(),
            description: scenario.description.clone(),
            t: row.try_get("t").map_err(|_| ScenaryError::Database)?,
            action: scenario.action.clone(),
        })
    }

    pub async fn del_scenario(&self, id: i64) -> Result<bool, ()> {
        let client = self.write_pool.get().await.map_err(|e| {
            error!("Failed to get read-write connection from pool: {e}");
        })?;
        let stmt = client
            .prepare_cached("DELETE FROM scenarios WHERE id = $1")
            .await
            .map_err(|e| {
                error!("Failed to prepare del_scenario statement: {}", e);
            })?;
        let deleted = client.execute(&stmt, &[&id]).await.map_err(|e| {
            error!("Failed to execute del_scenario statement: {}", e);
        })?;
        Ok(deleted > 0)
    }

    pub async fn get_scenario(
        &self,
        id: i64,
    ) -> Result<Option<scene::Scenario>, Box<dyn std::error::Error>> {
        let client = self.read_pool.get().await.map_err(|e| {
            error!("Failed to get read-only connection from pool: {e}");
            e
        })?;
        let stmt = client
            .prepare_cached("SELECT def FROM scenarios WHERE id = $1")
            .await
            .map_err(|e| {
                error!("Failed to prepare get_scenario statement: {}", e);
                e
            })?;
        let row = client.query_opt(&stmt, &[&id]).await.map_err(|e| {
            error!("Failed to execute get_scenario statement: {}", e);
            e
        })?;
        if row.is_none() {
            return Ok(None);
        }
        let row = row.unwrap(); // checked above
        let json_scenario: serde_json::Value = row.try_get("def").map_err(|e| {
            error!("Unexpected result from get_scenario statement: {}", e);
            e
        })?;
        let scenario: scene::Scenario = serde_json::from_value(json_scenario).map_err(|e| {
            error!("Invalid scenario from get_scenario statement: {}", e);
            e
        })?;
        Ok(Some(scenario))
    }

    pub async fn list_scenarios(&self) -> Result<Vec<ScenarioDetails>, Box<dyn std::error::Error>> {
        let client = self.read_pool.get().await.map_err(|e| {
            error!("Failed to get read-only connection from pool: {e}");
            e
        })?;
        let stmt = client
            .prepare_cached(
                "SELECT
                    id,
                    name,
                    t,
                    def->>'creator' AS creator,
                    def->>'description' AS description,
                    def->>'action' AS action
                  FROM scenarios
                  ORDER BY name ASC",
            )
            .await
            .map_err(|e| {
                error!("Failed to prepare list_scenarios statement: {}", e);
                e
            })?;
        let rows = client.query(&stmt, &[]).await.map_err(|e| {
            error!("Failed to execute list_scenarios statement: {}", e);
            e
        })?;
        let mut res: Vec<ScenarioDetails> = Vec::with_capacity(rows.len());
        for row in rows {
            res.push(ScenarioDetails {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                creator: row.try_get("creator")?,
                description: row.try_get("description")?,
                t: row.try_get("t")?,
                action: row.try_get("action")?,
            })
        }
        Ok(res)
    }

    pub async fn get_work_actions(
        &self,
        work_id: &str,
        count: u32,
    ) -> Result<Vec<shared::scene::WorkActions>, Box<dyn std::error::Error>> {
        let client = self.read_pool.get().await.map_err(|e| {
            error!("Failed to get read-only connection from pool: {e}");
            e
        })?;
        let stmt = client
            .prepare_cached(
                "SELECT
                    t,
                    actions
                  FROM results
                  WHERE work_id = $1
                  ORDER BY t DESC
                  LIMIT $2",
            )
            .await
            .map_err(|e| {
                error!("Failed to prepare get_work_actions statement: {}", e);
                e
            })?;
        let count = i64::from(count);
        Ok(client
            .query(&stmt, &[&work_id, &count])
            .await
            .map_err(|e| {
                error!("Failed to execute get_work_actions statement: {}", e);
                e
            })?
            .into_iter()
            .filter_map(|row| {
                let t: std::time::SystemTime = row.try_get("t").ok()?;
                let actions_json: serde_json::Value = row.try_get("actions").ok()?;
                let actions: Vec<scene::WorkAction> = serde_json::from_value(actions_json).ok()?;
                Some(scene::WorkActions {
                    work_id: work_id.to_string(),
                    t,
                    actions,
                })
            })
            .collect())
    }
}

fn row2jobresult(row: &tokio_postgres::Row) -> Result<JobResult, Box<dyn std::error::Error>> {
    Ok(JobResult {
        info: row2info(row)?,
        relation_metadata: serde_json::from_value(
            row.try_get("relation_metadata")
                .unwrap_or_else(|_| serde_json::Value::Object(shared::object::Metadata::new())),
        )?,
        result: row2resultkind(row)?,
    })
}

fn row2info(row: &tokio_postgres::Row) -> Result<shared::object::Info, Box<dyn std::error::Error>> {
    Ok(shared::object::Info {
        org: row.try_get("org")?,
        object_id: row.try_get("object_id")?,
        object_type: row.try_get("object_type")?,
        object_subtype: row.try_get("object_subtype")?,
        recursion_level: row.try_get::<_, i32>("recursion_level")? as u32,
        size: row.try_get::<_, i64>("size")? as u64,
        hashes: serde_json::from_value(row.try_get("hashes")?)?,
        entropy: row.try_get("entropy")?,
        ctime: row
            .try_get::<_, std::time::SystemTime>("t")?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs_f64(),
    })
}

fn row2resultkind(row: &tokio_postgres::Row) -> Result<JobResultKind, Box<dyn std::error::Error>> {
    let mut res: serde_json::map::Map<String, serde_json::Value> =
        serde_json::from_value(row.try_get("result")?)?;
    if res.contains_key("ok") {
        res["ok"]["children"] = serde_json::Value::Array(Vec::new());
    }
    Ok(serde_json::from_value(serde_json::Value::Object(res))?)
}
