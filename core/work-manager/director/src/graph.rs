//! Graph database communication
//!
//! This module interacts with the GraphDB

use futures::{pin_mut, stream::TryStreamExt};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

const SCENARIOS_COUNT: &str = "director_scenarios_total";
const WORKS_COUNT: &str = "director_works_total";
const PROCESSING_TIME: &str = "director_process_time_seconds";
const VERSION: u16 = 1;
// Limit the number of neighbour works to match by the global query to:
// max(scenario.min_matches * NEIGHBOUR_LIMIT_MUL, MIN_NEIGHBOURS)
const NEIGHBOUR_LIMIT_MUL: usize = 10;
const MIN_NEIGHBOURS: usize = 100;

/// The graph database connector
pub struct GraphDB {
    read_client: tokio_postgres::Client,
    write_client: tokio_postgres::Client,
    scenarios: Vec<(i64, tokio_postgres::Statement)>,
    get_before: tokio_postgres::Statement,
    get_after: tokio_postgres::Statement,
    save_actions: tokio_postgres::Statement,
}

#[derive(Debug, PartialEq)]
/// PostgreSQL interval (not supported by the tokio_postgres crate)
struct Interval {
    micros: i64,
    days: i32,
    months: i32,
}

impl<'a> tokio_postgres::types::FromSql<'a> for Interval {
    fn from_sql(
        _ty: &tokio_postgres::types::Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if raw.len() == 8 + 4 + 4 {
            Ok(Self {
                micros: i64::from_be_bytes(raw[0..8].try_into().unwrap()),
                days: i32::from_be_bytes(raw[8..12].try_into().unwrap()),
                months: i32::from_be_bytes(raw[12..16].try_into().unwrap()),
            })
        } else {
            Err("Failed to deserialize interval value".into())
        }
    }
    fn accepts(ty: &tokio_postgres::types::Type) -> bool {
        *ty == tokio_postgres::types::Type::INTERVAL
    }
}

impl PartialOrd for Interval {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.days != other.days || self.months != other.months {
            None
        } else {
            Some(self.micros.cmp(&other.micros))
        }
    }
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
        let get_before = read_client
            .prepare(
                "
            WITH ref AS (SELECT t FROM objects WHERE is_entry AND work_id = $1 LIMIT 1)
            SELECT work_id, ref.t - objects.t dt
            FROM objects, ref
            WHERE
                objects.t <= ref.t AND
                work_id != $1 AND
                is_entry
            ORDER BY objects.t DESC LIMIT $2",
            )
            .await?;
        let get_after = read_client
            .prepare(
                "
            WITH ref AS (SELECT t FROM objects WHERE is_entry AND work_id = $1 LIMIT 1)
            SELECT work_id, objects.t - ref.t AS dt
            FROM objects, ref
            WHERE
                objects.t > ref.t AND
                is_entry
            ORDER BY objects.t ASC LIMIT $2",
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
            .application_name("director_ro")
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
        let save_actions = write_client
            .prepare("INSERT INTO results (work_id, actions) VALUES ($1, $2)")
            .await?;
        let mut res = Self {
            read_client,
            write_client,
            scenarios: Vec::new(),
            get_before,
            get_after,
            save_actions,
        };
        res.load_scenarios().await?;
        Ok(res)
    }

    #[tracing::instrument(level=tracing::Level::ERROR, skip(self))]
    pub async fn load_scenarios(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.scenarios.clear();
        let scn_it = self
            .read_client
            .query_raw("SELECT id, def FROM scenarios", &[] as &[&str])
            .await?;
        pin_mut!(scn_it);
        let mut n_scenarios = 0usize;
        let mut n_actual_scenarios = 0usize;
        while let Some(row) = scn_it.try_next().await? {
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
            if VERSION < scenario.min_ver {
                warn!(
                    "Scenario {} (id {}) skipped due to unsatisfied minimum version requirements ({} < {})",
                    scenario.name, id, VERSION, scenario.min_ver
                );
                continue;
            }
            if let Some(max_ver) = scenario.max_ver {
                if VERSION > max_ver {
                    warn!(
                        "Scenario {} (id {}) skipped due to unsatisfied maximum version requirements ({} > {})",
                        scenario.name, id, VERSION, max_ver
                    );
                }
                continue;
            }
            let rule = pgrules::parse_to_sql_single_work(&scenario.local_query);
            if let Err(e) = rule {
                warn!(
                    "Scenario {} (id {}) skipped due to invalid rule ({}): {}",
                    scenario.name, id, scenario.local_query, e
                );
                continue;
            }
            let rule = format!(
                "SELECT t, def FROM scenarios WHERE id = {} AND EXISTS (SELECT 1 {})",
                id,
                rule.unwrap()
            );
            match self.read_client.prepare(&rule).await {
                Ok(stmt) => {
                    debug!("Scenario {}: {}", id, rule);
                    self.scenarios.push((id, stmt));
                    n_actual_scenarios += 1;
                }
                Err(e) => {
                    warn!(
                        "Scenario {} (id {}) skipped due to rule compilation failure ({}): {}",
                        scenario.name, id, rule, e
                    );
                }
            }
        }
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
        &self,
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
                let global_rule = pgrules::parse_to_sql_single_work(&context.global_query);
                if let Err(e) = global_rule {
                    warn!(
                        "Scenario {} skipped due to global query compilation error ({}): {}",
                        scenario.name, context.global_query, e
                    );
                    continue;
                }
                let global_query = format!("SELECT EXISTS (SELECT 1 {})", global_rule.unwrap());
                let global_stmt = self.read_client.prepare(&global_query).await?;
                let mut attempts: i64 =
                    i64::try_from((context.min_matches * NEIGHBOUR_LIMIT_MUL).max(MIN_NEIGHBOURS))
                        .unwrap_or(i64::MAX);
                let mut before_it = self
                    .read_client
                    .query(&self.get_before, &[&work_id, &attempts])
                    .await?
                    .into_iter();
                let mut after_it = self
                    .read_client
                    .query(&self.get_after, &[&work_id, &attempts])
                    .await?
                    .into_iter();
                let mut before = before_it.next();
                debug!("Before: {:?}", before);
                let mut after = after_it.next();
                debug!("After: {:?}", after);
                let mut nmatches = 0usize;
                while nmatches < context.min_matches && attempts > 0 {
                    attempts -= 1;
                    let neighbour_work_id = if let Some(before_row) = &before {
                        if let Some(after_row) = &after {
                            let before_t: Interval = before_row.try_get("dt")?;
                            let after_t: Interval = after_row.try_get("dt")?;
                            if before_t <= after_t {
                                let work_id: String = before_row.try_get("work_id")?;
                                before = before_it.next();
                                work_id
                            } else {
                                let work_id: String = after_row.try_get("work_id")?;
                                after = after_it.next();
                                work_id
                            }
                        } else {
                            let work_id: String = before_row.try_get("work_id")?;
                            before = before_it.next();
                            work_id
                        }
                    } else {
                        if let Some(after_row) = &after {
                            let work_id: String = after_row.try_get("work_id")?;
                            after = after_it.next();
                            work_id
                        } else {
                            break;
                        }
                    };
                    let row = self
                        .read_client
                        .query_one(&global_stmt, &[&neighbour_work_id])
                        .await?;
                    let global_match: bool = row.try_get(0)?;
                    if global_match {
                        nmatches += 1;
                    }
                    debug!(
                        "Scenario {}: {}global match on {} (currently {}/{}) - {} lookups remaining",
                        id, if global_match {""} else {"no "}, neighbour_work_id, nmatches, context.min_matches, attempts
                    );
                }
                if nmatches < context.min_matches {
                    debug!(
                        "Scenario {}: not enough matches ({}/{})",
                        id, nmatches, context.min_matches
                    );
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
