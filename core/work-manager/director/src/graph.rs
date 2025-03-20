//! Graph database communication
//!
//! This module interacts with the GraphDB

use futures::{pin_mut, stream::TryStreamExt};
use pgrules::Interval;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

const SCENARIOS_COUNT: &str = "director_scenarios_total";
const WORKS_COUNT: &str = "director_works_total";
const PROCESSING_TIME: &str = "director_process_time_seconds";

/// The graph database connector
pub struct GraphDB {
    read_client: tokio_postgres::Client,
    write_client: tokio_postgres::Client,
    scenarios: Vec<(i64, tokio_postgres::Statement)>,
    get_before: tokio_postgres::Statement,
    get_before_count: tokio_postgres::Statement,
    get_after: tokio_postgres::Statement,
    get_after_count: tokio_postgres::Statement,
    save_actions: tokio_postgres::Statement,
}

async fn portal_next(
    txn: &mut tokio_postgres::Transaction<'_>,
    portal: &tokio_postgres::Portal,
) -> Result<Option<tokio_postgres::row::Row>, tokio_postgres::error::Error> {
    txn.query_portal(portal, 1).await.map(|mut rows| rows.pop())
}

impl GraphDB {
    /// Creates a new connection to the graph database
    pub async fn new(config: &crate::config::Config) -> Result<Self, Box<dyn std::error::Error>> {
        // Describe metrics
        metrics::describe_gauge!(SCENARIOS_COUNT, "Total number of scenarios");
        metrics::describe_counter!(WORKS_COUNT, "Total number of processed works");
        metrics::describe_histogram!(
            PROCESSING_TIME,
            metrics::Unit::Seconds,
            "Work processing time"
        );

        // Read side
        let read_config = &config.read_db;
        let mut pgcfg = tokio_postgres::Config::new();
        pgcfg
            .application_name("director_ro")
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
        let qtypes = &[
            tokio_postgres::types::Type::TEXT,
            tokio_postgres::types::Type::INTERVAL,
        ];
        let get_before = read_client
            .prepare_typed(
                "
            WITH ref AS (SELECT t FROM objects WHERE is_entry AND work_id = $1 LIMIT 1)
            SELECT work_id, ref.t - objects.t dt
            FROM objects, ref
            WHERE
                objects.t >= ref.t - $2 AND
                objects.t <= ref.t AND
                work_id != $1 AND
                is_entry
            ORDER BY objects.t DESC",
                qtypes,
            )
            .await?;
        let get_before_count = read_client
            .prepare_typed(
                "
            WITH ref AS (SELECT t FROM objects WHERE is_entry AND work_id = $1 LIMIT 1)
            SELECT COUNT(*)
            FROM objects, ref
            WHERE
                objects.t >= ref.t - $2 AND
                objects.t <= ref.t AND
                work_id != $1 AND
                is_entry",
                qtypes,
            )
            .await?;
        let get_after = read_client
            .prepare_typed(
                "
            WITH ref AS (SELECT t FROM objects WHERE is_entry AND work_id = $1 LIMIT 1)
            SELECT work_id, objects.t - ref.t AS dt
            FROM objects, ref
            WHERE
                objects.t > ref.t AND
                objects.t <= ref.t + $2 AND
                is_entry
            ORDER BY objects.t ASC",
                qtypes,
            )
            .await?;
        let get_after_count = read_client
            .prepare_typed(
                "
            WITH ref AS (SELECT t FROM objects WHERE is_entry AND work_id = $1 LIMIT 1)
            SELECT COUNT(*)
            FROM objects, ref
            WHERE
                objects.t > ref.t AND
                objects.t <= ref.t + $2 AND
                is_entry",
                qtypes,
            )
            .await?;
        debug!(
            "Connected to GraphDB {} at {}:{} in read-only mode",
            read_config.dbname, read_config.host, read_config.port
        );

        // Write side
        let write_config = &config.write_db;
        let mut pgcfg = tokio_postgres::Config::new();
        pgcfg
            .application_name("director_rw")
            .dbname(&write_config.dbname)
            .host(&write_config.host)
            .port(write_config.port)
            .user(&write_config.user)
            .password(&write_config.pass)
            .target_session_attrs(tokio_postgres::config::TargetSessionAttrs::ReadWrite);
        let (write_client, write_conn) =
            pgcfg.connect(tokio_postgres::NoTls).await.map_err(|e| {
                error!(
                    "Failed to connect to graph database {} at {}:{}: {}",
                    write_config.dbname, write_config.host, write_config.port, e
                );
                e
            })?;
        tokio::spawn(async move {
            if let Err(e) = write_conn.await {
                error!("Write connection error: {}", e);
            }
        });
        debug!(
            "Connected to GraphDB {} at {}:{} in read-write mode",
            read_config.dbname, read_config.host, read_config.port
        );
        let db_version: i32 = write_client
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
        let save_actions = write_client
            .prepare("INSERT INTO results (work_id, actions) VALUES ($1, $2)")
            .await?;
        let mut res = Self {
            read_client,
            write_client,
            scenarios: Vec::new(),
            get_before,
            get_before_count,
            get_after,
            get_after_count,
            save_actions,
        };
        res.load_scenarios().await?;
        Ok(res)
    }

    #[tracing::instrument(level=tracing::Level::ERROR, skip(self))]
    pub async fn load_scenarios(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.scenarios.clear();
        let txn = self.read_client.transaction().await?;
        // A portal is employed here in order to avoid deadlocks with nested queries
        // This typically happens when the network buffer is filled with the outer query
        // results and the inner query fails to fetch its results
        // In this case the second query is just a "prepare", but that still uses the same
        // communication channel
        let portal = txn.bind("SELECT id, def FROM scenarios", &[]).await?;
        let mut n_scenarios = 0usize;
        let mut n_actual_scenarios = 0usize;
        loop {
            let scn_it = txn.query_portal_raw(&portal, 1).await?;
            pin_mut!(scn_it);
            let next = scn_it.try_next().await?;
            if next.is_none() {
                break;
            }
            let row = next.unwrap();
            n_scenarios += 1;
            let id: i64 = row.try_get("id")?;
            let json_scenario: serde_json::Value = row.try_get("def")?;
            let scenario: shared::scene::Scenario = match serde_json::from_value(json_scenario) {
                Ok(v) => v,
                Err(e) => {
                    warn!("Skipping invalid scenario (id {}): {}", id, e);
                    continue;
                }
            };
            if !scenario.is_compatible() {
                warn!(
                    "Scenario {} (id {}) skipped due to unsatisfied version requirements ({})",
                    scenario.name,
                    id,
                    scenario.compatibility()
                );
                continue;
            }
            let rule =
                pgrules::parse_to_sql(&scenario.local_query, pgrules::QueryType::ScenarioLocal);
            if let Err(e) = rule {
                warn!(
                    "Scenario {} (id {}) skipped due to invalid rule ({}): {}",
                    scenario.name, id, scenario.local_query, e
                );
                continue;
            }
            let query = format!(
                "SELECT t, def FROM scenarios WHERE id = {} AND EXISTS (SELECT 1 {})",
                id,
                rule.unwrap().query
            );
            match txn.prepare(&query).await {
                Ok(stmt) => {
                    debug!("Scenario {} (id {}): {}", scenario.name, id, query);
                    self.scenarios.push((id, stmt));
                    n_actual_scenarios += 1;
                }
                Err(e) => {
                    warn!(
                        "Scenario id {} skipped due to rule compilation failure ({}): {}",
                        id, query, e
                    );
                }
            }
        }
        txn.commit().await?;
        self.scenarios.shrink_to_fit();
        info!(
            "Loaded {} scenarios out of {}",
            n_actual_scenarios, n_scenarios
        );
        metrics::gauge!(SCENARIOS_COUNT).set(n_actual_scenarios as f64);
        Ok(())
    }

    #[tracing::instrument(level=tracing::Level::ERROR, skip(self))]
    pub async fn apply_scenarios(
        &mut self,
        work_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();

        let mut actions: Vec<shared::scene::WorkAction> = Vec::new();
        for (id, stmt) in self.scenarios.iter() {
            debug!("Testing scenario {} for local matches...", id);
            let row = self.read_client.query_opt(stmt, &[&work_id]).await?;
            if row.is_none() {
                debug!("Scenario {}: no local match", id);
                continue;
            }
            let row = row.unwrap();
            debug!("Scenario {} has local match...", id);
            let ctime: std::time::SystemTime = row.try_get("t")?;
            let ctime = ctime
                .duration_since(std::time::SystemTime::UNIX_EPOCH)?
                .as_secs_f64();
            let json_scenario: serde_json::Value = row.try_get("def")?;
            let scenario: shared::scene::Scenario = serde_json::from_value(json_scenario).unwrap();
            if let Some(context) = scenario.context {
                debug!("Testing scenario {} for global matches...", id);
                let global_rule = pgrules::parse_to_sql(
                    &context.global_query,
                    pgrules::QueryType::ScenarioGlobal,
                );
                if let Err(e) = global_rule {
                    warn!(
                        "Scenario {} skipped due to global query compilation error ({}): {}",
                        scenario.name, context.global_query, e
                    );
                    continue;
                }
                let global_parsed = global_rule.unwrap();
                let Some(global_settings) = global_parsed.global_query_settings else {
                    warn!(
                        "Scenario {} skipped due to missing global query settings",
                        scenario.name,
                    );
                    continue;
                };
                let time_window = global_settings.time_window;
                let required_matches = global_settings.matches;
                let has_with_clause = global_parsed.with_clause.is_some();
                let global_query = format!(
                    "{} SELECT EXISTS (SELECT 1 {})",
                    global_parsed.with_clause.unwrap_or_default(),
                    global_parsed.query
                );
                let mut txn = self
                    .read_client
                    .build_transaction()
                    .read_only(true)
                    .isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
                    .start()
                    .await?;
                let global_stmt = txn.prepare(&global_query).await?;
                let params: &[&(dyn tokio_postgres::types::ToSql + Sync)] =
                    &[&work_id, &time_window];
                let before_it = txn.bind(&self.get_before, params).await?;
                let after_it = txn.bind(&self.get_after, params).await?;
                let avail_before: i64 = txn.query_one(&self.get_before_count, params).await?.get(0);
                let avail_after: i64 = txn.query_one(&self.get_after_count, params).await?.get(0);
                let avail_neighbors: u32 = avail_before
                    .saturating_add(avail_after)
                    .try_into()
                    .unwrap_or(u32::MAX);
                let max_neighbors = global_settings.max_neighbors.unwrap_or(avail_neighbors);
                let total_neigbors = max_neighbors.min(avail_neighbors);
                let mut before = portal_next(&mut txn, &before_it).await?;
                debug!("Before: {:?}", before);
                let mut after = portal_next(&mut txn, &after_it).await?;
                debug!("After: {:?}", after);
                let mut nmatches = 0u32;
                let target_matches = match &required_matches {
                    pgrules::Matches::MoreThan(req) => *req,
                    pgrules::Matches::MoreThanPercent(req) => {
                        (f64::from(*req) / 100.0 * f64::from(total_neigbors)) as u32
                    }
                    pgrules::Matches::LessThan(req) => (*req).saturating_sub(1),
                    pgrules::Matches::LessThanPercent(req) => {
                        ((f64::from(*req) / 100.0 * f64::from(total_neigbors)) as u32)
                            .saturating_sub(1)
                    }
                    pgrules::Matches::None => 0,
                };
                let mut attempts = 0u32;
                for _ in 0..max_neighbors {
                    attempts += 1;
                    if nmatches > target_matches {
                        break;
                    }
                    let neighbour_work_id = if let Some(before_row) = &before {
                        if let Some(after_row) = &after {
                            let before_t: Interval = before_row.try_get("dt")?;
                            let after_t: Interval = after_row.try_get("dt")?;
                            if before_t <= after_t {
                                let work_id: String = before_row.try_get("work_id")?;
                                before = portal_next(&mut txn, &before_it).await?;
                                work_id
                            } else {
                                let work_id: String = after_row.try_get("work_id")?;
                                after = portal_next(&mut txn, &after_it).await?;
                                work_id
                            }
                        } else {
                            let work_id: String = before_row.try_get("work_id")?;
                            before = portal_next(&mut txn, &before_it).await?;
                            work_id
                        }
                    } else if let Some(after_row) = &after {
                        let work_id: String = after_row.try_get("work_id")?;
                        after = portal_next(&mut txn, &after_it).await?;
                        work_id
                    } else {
                        // not reached
                        break;
                    };
                    let row = if has_with_clause {
                        txn.query_one(&global_stmt, &[&neighbour_work_id, &work_id])
                            .await?
                    } else {
                        txn.query_one(&global_stmt, &[&neighbour_work_id]).await?
                    };

                    let global_match: bool = row.try_get(0)?;
                    if global_match {
                        nmatches += 1;
                    }
                    debug!(
                        "Scenario {}: {}global match on {} ({} matches so far) - lookup {}/{}",
                        id,
                        if global_match { "" } else { "no " },
                        neighbour_work_id,
                        nmatches,
                        attempts,
                        max_neighbors
                    );
                }
                let matched = match required_matches {
                    pgrules::Matches::MoreThan(_) | pgrules::Matches::MoreThanPercent(_) => {
                        nmatches > target_matches
                    }
                    pgrules::Matches::LessThan(_) | pgrules::Matches::LessThanPercent(_) => {
                        nmatches <= target_matches
                    }
                    pgrules::Matches::None => nmatches == 0,
                };
                debug!(
                    "Scenario {}: condition ({}) {}met (matches: {}, target: {}, max_neighbors: {}, attempts: {})",
                    id,
                    required_matches,
                    if matched { "" } else { "not "},
                    nmatches,
                    target_matches,
                    max_neighbors,
                    attempts,
                );
                if !matched {
                    continue;
                }
            }
            debug!("Action {} triggered for scenario {}", scenario.action, id);
            actions.push(shared::scene::WorkAction {
                scenario: scenario.name,
                ctime,
                action: scenario.action,
            })
        }

        self.write_client
            .execute(
                &self.save_actions,
                &[&work_id, &serde_json::to_value(actions).unwrap()],
            )
            .await?;

        metrics::histogram!(PROCESSING_TIME).record(start.elapsed().as_secs_f64());
        metrics::counter!(WORKS_COUNT).increment(1);
        Ok(())
    }
}
