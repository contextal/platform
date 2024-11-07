//! Shared library with common structs and routines

pub mod amqp;
pub mod clamd;
pub mod config;
pub mod object;
pub mod scene;
pub mod utils;

use serde::Serializer;

/// The maximum time a work can take (from the entry to the result)
///
/// While processing the object tree, workers check if the remaining TTL of
/// the received object. If the level exceeds the maximum they return an
/// error, effectively stopping further processing.
///
/// The actual TTL is specified per work request. This value acts both as a
/// default (if not specified) and as an absolute maximum (larger TTLs are
/// silently capped)
pub const MAX_WORK_TTL: std::time::Duration = std::time::Duration::from_secs(1 * 60 * 60);

/// The maximum recursion level a work can reach
///
/// While processing the object tree, workers check the recursion depth of the
/// received object. If the level exceeds the maximum they return an error,
/// effectively stopping further processing.
///
/// The actual maximum is specified per work request. This value acts both as a
/// default (if not specified) and as an absolute maximum (larger recursion levels
/// are silently capped)
pub const MAX_WORK_DEPTH: u32 = 24;

/// The name of the global result queue which holds full results
pub const RESULTS_QUEUE_NAME: &str = "CTX-JobRes";
/// The name of the global director queue
pub const DIRECTOR_QUEUE_NAME: &str = "CTX-Director";
/// The name of the global scenario reload exchange
pub const SC_RELOAD_EXCHANGE_NAME: &str = "ctx.screload";
/// The `content-type` to use in all the messages
pub const MSG_CONTENT_TYPE: &str = "application/json";
/// The `message-type` to use in job requests
pub const REQUEST_TYPE: &str = "job.request";
/// The `message-type` to use in job results
pub const RESULT_TYPE: &str = "job.result";
/// The `message-type` to use in scenario application requests
pub const SC_PROCESS_TYPE: &str = "scenarios.process";
/// The `message-type` to use in scenario reload requests
pub const SC_RELOAD_TYPE: &str = "scenarios.reload";
/// The length of the `correlation_id` to use in messages
pub const MSG_CORRID_LEN: usize = 24;
/// The relation metadata key holding the work global data (bubbled)
pub const META_KEY_GLOBAL: &str = "_global";
/// The relation metadata key holding the work origin
pub const META_KEY_ORIGIN: &str = "_origin";
/// The relation metatadata origin key controlling work reprocession
pub const META_KEY_REPROCESSABLE: &str = "_can_reprocess";

pub fn time_to_f64<S: Serializer>(
    time: &std::time::SystemTime,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(
        time.duration_since(std::time::UNIX_EPOCH)
            .map_err(serde::ser::Error::custom)?
            .as_secs_f64(),
    )
}
