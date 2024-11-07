//! HTTP API endpoints
//!
//! Contextal platform API endpoints

mod tempobj;

use crate::graphdb;
use actix_multipart::form;
use actix_web::{body, delete, error, get, http, route, web, HttpRequest, HttpResponse};
use futures::{StreamExt, TryStreamExt};
use metrics_exporter_prometheus::PrometheusHandle;
use serde::{Deserialize, Serialize};
use shared::{clamd, object, scene};
use std::ops::Deref;
use tempobj::TempObject;
use tokio::sync::{mpsc, oneshot};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

/// Miscellaneous limits
#[derive(Clone)]
pub struct Limits {
    /// Maximum work-related results per query
    pub max_work_results: usize,
    /// Max search results (default 1000)
    pub max_search_results: u32,
    /// Max action results (default 100)
    pub max_action_results: u32,
}

/// Used in internal communication with the publisher
#[derive(Debug)]
pub struct JobRequest {
    pub object: object::Info,
    pub ttl: std::time::Duration,
    pub max_recursion: u32,
    pub reply_tx: oneshot::Sender<String>,
    pub relation_metadata: shared::object::Metadata,
}

/// Used in internal communication with the publisher
#[derive(Debug)]
pub enum BrokerAction {
    Job(JobRequest),
    ApplyScenarios(Vec<String>),
    Reload,
}

/// The prometheus endpoint
#[get("/metrics")]
async fn metrics(prom: web::Data<PrometheusHandle>) -> String {
    prom.render()
}

/// The form params for [`submit_v1`]
#[derive(form::MultipartForm)]
#[multipart(deny_unknown_fields, duplicate_field = "deny")]
struct SubmitFormV1 {
    /// Org
    org: Option<form::text::Text<String>>,
    /// Relation metadata
    relation_metadata: Option<form::json::Json<shared::object::Metadata>>,
    /// The number of seconds allowed to fully complete this work request
    ttl: Option<form::text::Text<u64>>,
    /// The maximum recursion level a work can reach
    maxrec: Option<form::text::Text<u32>>,
    /// The object data
    object_data: TempObject,
    // FIXME: more params?
    // FIXME: do we want to force object_metadata via params?
    // FIXME: do we want to force the ftype?
}

/// The response object returned by [`submit_v1`]
#[derive(Serialize)]
struct SubmitResultV1 {
    /// The id assigned to the work object
    object_id: String,
    /// The id assigned to the work request - used for retrieving graph results
    work_id: String,
    /// The effective ttl of the work
    ttl: u64,
}

#[derive(Serialize)]
struct Origin<'a> {
    peer: Option<&'a str>,
    real_peer: Option<&'a str>,
    ttl: u64,
    max_recursion: u32,
}

/// The file submission endpoint
#[route("/api/v1/submit", method = "POST", method = "PUT")]
async fn submit_v1(
    req: HttpRequest,
    form::MultipartForm(submit_form): form::MultipartForm<SubmitFormV1>,
    typedet: web::Data<clamd::Typedet>,
    tx: web::Data<mpsc::WeakSender<BrokerAction>>,
    objects_path: web::Data<String>,
    is_reprocess_enabled: web::Data<bool>,
) -> Result<(web::Json<SubmitResultV1>, http::StatusCode), error::Error> {
    // Get the temp object from the form and transform it into an object
    let org = submit_form
        .org
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("ctx");
    let mut object = submit_form
        .object_data
        .into_object(org)
        .await
        .map_err(|e| {
            error!("Failed to convert uploaded file to object: {e}");
            error::ErrorInternalServerError("Internal error: object error")
        })?;
    if object.is_empty() {
        return Err(error::ErrorBadRequest("Empty object"));
    }
    let object_id = object.object_id.clone();
    debug!("Object \"{}\" complely received", object_id);

    // Query clamd for file type
    typedet
        .set_ftype(&mut object, objects_path.get_ref())
        .await
        .map_err(|e| {
            error!("Typedet failed: {e}");
            error::ErrorInternalServerError("Internal error: object detection failed")
        })?;
    debug!(
        "Object \"{}\" has type \"{}\"",
        object_id, object.object_type
    );

    // Setup the content of the work request
    let (reply_tx, reply_rx) = oneshot::channel();
    let ttl = submit_form
        .ttl
        .map(|txt| std::time::Duration::from_secs(txt.into_inner()))
        .unwrap_or(shared::MAX_WORK_TTL)
        .min(shared::MAX_WORK_TTL);
    let max_recursion = submit_form
        .maxrec
        .map(|txt| txt.into_inner())
        .unwrap_or(shared::MAX_WORK_DEPTH)
        .clamp(1, shared::MAX_WORK_DEPTH);
    debug!(
        "Posted job request for object \"{}\", with ttl {}, max recursion {}",
        object_id,
        ttl.as_secs(),
        max_recursion
    );
    let mut relation_metadata = submit_form
        .relation_metadata
        .map(|m| m.into_inner())
        .unwrap_or_else(shared::object::Metadata::new);
    relation_metadata.insert(
        shared::META_KEY_ORIGIN.to_string(),
        serde_json::to_value(Origin {
            peer: req.connection_info().peer_addr(),
            real_peer: req.connection_info().realip_remote_addr(),
            ttl: ttl.as_secs(),
            max_recursion,
        })
        .unwrap(),
    );
    if !is_reprocess_enabled.get_ref() {
        relation_metadata.insert(shared::META_KEY_REPROCESSABLE.to_string(), false.into());
    }
    shared::object::sanitize_meta_keys(&mut relation_metadata);
    let jobreq = JobRequest {
        object,
        ttl,
        max_recursion,
        reply_tx,
        relation_metadata,
    };
    // Send the request to the publisher (tx is Weak and needs upgrading)
    tx.upgrade()
        .ok_or_else(|| {
            error!("Cannot upgrade Sender: publisher lost");
            error::ErrorInternalServerError("Internal error: publisher lost")
        })?
        .send(BrokerAction::Job(jobreq))
        .await
        .map_err(|e| {
            error!("Failed to communicate with publisher: {e}");
            error::ErrorInternalServerError("Internal error: publisher communication failed")
        })?;

    // Await the assigned work_id
    let work_id = reply_rx.await.map_err(|e| {
        error!("Failed to get request id from publisher: {e}");
        error::ErrorInternalServerError("Internal error: work allocation falied")
    })?;
    Ok((
        web::Json(SubmitResultV1 {
            object_id,
            work_id,
            ttl: ttl.as_secs(),
        }),
        http::StatusCode::CREATED,
    ))
}

/// The get_work_graph endpoint
#[get("/api/v1/get_work_graph/{work_id}")]
async fn get_work_graph_v1(
    work_id: web::Path<String>,
    graphdb: web::Data<graphdb::GraphDB>,
) -> Result<web::Json<shared::amqp::JobResult>, error::Error> {
    debug!("Processing get_work_graph for work_id {work_id}");
    graphdb
        .get_work_graph(&work_id)
        .await
        .map_err(|e| {
            error!("Failed to lookup work graph: {e}");
            error::ErrorInternalServerError("Internal error: work graph lookup failed")
        })?
        .map(web::Json)
        .ok_or_else(|| error::ErrorNotFound("No such work"))
}

#[derive(Deserialize, Debug)]
struct GetGraphsReq {
    work_ids: Vec<String>,
}

#[derive(Serialize)]
struct GetGraphsResp(std::collections::HashMap<String, Option<shared::amqp::JobResult>>);

#[route("/api/v1/get_works_graphs", method = "POST", method = "PUT")]
async fn get_works_graphs_v1(
    req_body: web::Json<GetGraphsReq>,
    graphdb: web::Data<graphdb::GraphDB>,
    limits: web::Data<Limits>,
) -> Result<web::Json<GetGraphsResp>, error::Error> {
    if req_body.work_ids.len() > limits.max_work_results {
        return Err(error::ErrorBadRequest(format!(
            "Too many work graphs requested: (max {} allowed)",
            limits.max_work_results
        )));
    }
    Ok(web::Json(GetGraphsResp(
        futures::stream::iter(req_body.work_ids.iter().map(|work_id| async {
            graphdb
                .get_work_graph(work_id)
                .await
                .map(|qres| (work_id.to_string(), qres))
        }))
        .buffer_unordered(10)
        .try_collect()
        .await
        .map_err(|e| {
            error!("Failed to lookup work graph: {e}");
            error::ErrorInternalServerError("Internal error: work graph lookup failed")
        })?,
    )))
}

/// Download object
#[get("/api/v1/get_object/{object_id}")]
async fn get_object_v1(
    req: HttpRequest,
    object_id: web::Path<String>,
    objects_path: web::Data<String>,
) -> Result<HttpResponse, error::Error> {
    debug!("Processing get_object for object_id {object_id}");
    if req.headers().get("if-none-match").map(|h| h.as_bytes()) == Some(object_id.as_bytes()) {
        return Ok(HttpResponse::NotModified().finish());
    }
    // Note: the following stops traversals without hitting the db and
    // without a strict definition of the object_id format
    let canon_root = std::path::PathBuf::from(objects_path.get_ref())
        .canonicalize()
        .map_err(|e| {
            error!(
                "Failed to canonicalize objects_path {}: {}",
                objects_path.get_ref(),
                e
            );
            error::ErrorInternalServerError("Internal error: file system error")
        })?;
    let canon_object = [objects_path.get_ref(), object_id.as_ref()]
        .into_iter()
        .collect::<std::path::PathBuf>()
        .canonicalize()
        .map_err(|e| {
            debug!("Failed to canonicalize object_id {object_id}: {e}");
            error::ErrorNotFound("Object not found")
        })?;
    if canon_object.as_path().parent() != Some(canon_root.as_path()) {
        warn!("Object_id {object_id} sits outside of objects_path");
        return Err(error::ErrorNotFound("Object not found"));
    }
    let f = tokio::fs::File::open(canon_object.as_path())
        .await
        .map_err(|e| {
            debug!("Failed to open object_id {object_id}: {e}");
            error::ErrorNotFound("Object not found")
        })?;
    let size = f
        .metadata()
        .await
        .map_err(|e| {
            debug!("Failed to get size of object_id {object_id}: {e}");
            error::ErrorInternalServerError("Internal error: file error")
        })?
        .len();
    Ok(HttpResponse::Ok()
        .content_type("application/octet-stream")
        .insert_header(http::header::ETag(http::header::EntityTag::new_strong(
            object_id.clone(),
        )))
        .insert_header(http::header::ContentDisposition::attachment(
            object_id.as_str(),
        ))
        .body(body::SizedStream::new(
            size,
            tokio_util::io::ReaderStream::new(f),
        )))
}

/// The URL params for [`search_v1`] and [`count_v1`]
#[derive(Deserialize)]
struct SearchParamsV1 {
    /// The search string
    q: String,
    /// Requests the query to return objects rather than works
    getobjects: Option<bool>,
    /// The maximum number of items to return
    maxitems: Option<u32>,
}

/// Search error
#[derive(Serialize)]
struct SearchError {
    kind: &'static str,
    message: String,
}

/// Search for works matching query
#[route("/api/v1/search", method = "GET", method = "POST", method = "PUT")]
async fn search_v1(
    params: actix_web::Either<web::Json<SearchParamsV1>, web::Query<SearchParamsV1>>,
    graphdb: web::Data<graphdb::GraphDB>,
    limits: web::Data<Limits>,
) -> HttpResponse {
    let params = match &params {
        actix_web::Either::Left(v) => v.deref(),
        actix_web::Either::Right(v) => v.deref(),
    };
    let getobjects = params.getobjects.unwrap_or(false);
    let maxitems = params.maxitems.unwrap_or(limits.max_search_results);
    let maxitems = if maxitems != 0 {
        maxitems
    } else {
        limits.max_search_results
    };
    debug!(
        "Processing search query(getobjects: {}, max: {}): {}",
        getobjects, maxitems, params.q
    );
    match graphdb
        .search(params.q.as_str(), getobjects, maxitems)
        .await
    {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(e) => match e {
            graphdb::SearchError::Rule(e) => HttpResponse::BadRequest().json(SearchError {
                kind: "Rule compilation error",
                message: e,
            }),
            graphdb::SearchError::Query(e) => HttpResponse::BadRequest().json(SearchError {
                kind: "Query error",
                message: e,
            }),
            graphdb::SearchError::Internal => {
                error::ErrorInternalServerError("Internal error: search error").into()
            }
            graphdb::SearchError::Timeout => HttpResponse::BadRequest().json(SearchError {
                kind: "Timeout",
                message: "The query exceeded the maximum allowed run time".to_string(),
            }),
        },
    }
}

/// Search for works matching query and return count
#[route("/api/v1/count", method = "GET", method = "POST", method = "PUT")]
async fn count_v1(
    params: actix_web::Either<web::Json<SearchParamsV1>, web::Query<SearchParamsV1>>,
    graphdb: web::Data<graphdb::GraphDB>,
) -> HttpResponse {
    let params = match &params {
        actix_web::Either::Left(v) => v.deref(),
        actix_web::Either::Right(v) => v.deref(),
    };
    let getobjects = params.getobjects.unwrap_or(false);
    debug!(
        "Processing count query(getobjects: {}): {}",
        getobjects, params.q
    );
    match graphdb.count(params.q.as_str(), getobjects).await {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(e) => match e {
            graphdb::SearchError::Rule(e) => HttpResponse::BadRequest().json(SearchError {
                kind: "Rule compilation error",
                message: e,
            }),
            graphdb::SearchError::Query(e) => HttpResponse::BadRequest().json(SearchError {
                kind: "Query error",
                message: e,
            }),
            graphdb::SearchError::Internal => {
                error::ErrorInternalServerError("Internal error: search error").into()
            }
            graphdb::SearchError::Timeout => HttpResponse::BadRequest().json(SearchError {
                kind: "Timeout",
                message: "The query exceeded the maximum allowed run time".to_string(),
            }),
        },
    }
}

/// The URL params for [`search_v1`] and [`count_v1`]
#[derive(Deserialize)]
struct AddScenarioParamsV1 {
    /// The id of the scenario to replace
    replace_id: Option<i64>,
}

/// Add scenario
#[route("/api/v1/scenarios", method = "POST", method = "PUT")]
async fn add_scenario_v1(
    params: web::Query<AddScenarioParamsV1>,
    scenario: web::Json<scene::Scenario>,
    graphdb: web::Data<graphdb::GraphDB>,
) -> HttpResponse {
    match graphdb
        .add_scenario(scenario.deref(), params.replace_id)
        .await
    {
        Ok(s) => HttpResponse::Created().json(s),
        Err(e) => match e {
            graphdb::ScenaryError::Invalid(e) => HttpResponse::BadRequest().body(e),
            graphdb::ScenaryError::Duplicate => HttpResponse::Conflict().body("Scenario exists"),
            graphdb::ScenaryError::NotFound => {
                HttpResponse::NotFound().body("The scenario identified by replace_id was not found")
            }
            _ => HttpResponse::InternalServerError().body("Internal error: database error"),
        },
    }
}

/// Delete scenario
#[delete("/api/v1/scenarios/{id}")]
async fn del_scenario_v1(id: web::Path<i64>, graphdb: web::Data<graphdb::GraphDB>) -> HttpResponse {
    match graphdb.del_scenario(id.into_inner()).await {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => HttpResponse::NotFound().finish(),
        Err(_) => HttpResponse::InternalServerError().body("Internal error: database error"),
    }
}

/// Get scenario details
#[get("/api/v1/scenarios/{id}")]
async fn get_scenario_v1(
    id: web::Path<i64>,
    graphdb: web::Data<graphdb::GraphDB>,
) -> Result<web::Json<scene::Scenario>, error::Error> {
    let s = graphdb
        .get_scenario(id.into_inner())
        .await
        .map_err(|_| error::ErrorInternalServerError("Internal error: database error"))?;
    if let Some(s) = s {
        Ok(web::Json(s))
    } else {
        Err(error::ErrorNotFound("No such scenario"))
    }
}

/// List scenarios
#[get("/api/v1/scenarios")]
async fn list_scenarios_v1(
    graphdb: web::Data<graphdb::GraphDB>,
) -> Result<web::Json<Vec<graphdb::ScenarioDetails>>, error::Error> {
    Ok(web::Json(graphdb.list_scenarios().await.map_err(|_| {
        error::ErrorInternalServerError("Internal error: database error")
    })?))
}

#[derive(Deserialize)]
struct ActionLimitsV1 {
    maxitems: Option<u32>,
}

/// Get work actions
#[get("/api/v1/actions/{work_id}")]
async fn get_work_actions_v1(
    work_id: web::Path<String>,
    params: web::Query<ActionLimitsV1>,
    graphdb: web::Data<graphdb::GraphDB>,
    limits: web::Data<Limits>,
) -> Result<web::Json<Vec<shared::scene::WorkActions>>, error::Error> {
    let maxitems = params.maxitems.unwrap_or(1).min(limits.max_action_results);
    let maxitems = if maxitems != 0 {
        maxitems
    } else {
        limits.max_search_results
    };
    Ok(web::Json(
        graphdb
            .get_work_actions(&work_id, maxitems)
            .await
            .map_err(|_| error::ErrorInternalServerError("Internal error: database error"))?,
    ))
}

/// Request that all directors reload their rules
#[route("/api/v1/scenarios/reload", method = "POST", method = "PUT")]
async fn reload_actions_v1(
    tx: web::Data<mpsc::WeakSender<BrokerAction>>,
) -> Result<HttpResponse, error::Error> {
    // Send the request to the publisher (tx is Weak and needs upgrading)
    tx.upgrade()
        .ok_or_else(|| {
            error!("Cannot upgrade Sender: publisher lost");
            error::ErrorInternalServerError("Internal error: publisher lost")
        })?
        .send(BrokerAction::Reload)
        .await
        .map_err(|e| {
            error!("Failed to communicate with publisher: {e}");
            error::ErrorInternalServerError("Internal error: publisher communication failed")
        })?;
    Ok(HttpResponse::NoContent().finish())
}

/// (Re-)Apply scenarios
#[route("/api/v1/scenarios/apply", method = "POST", method = "PUT")]
async fn apply_scenarios_v1(
    req_body: web::Json<GetGraphsReq>,
    tx: web::Data<mpsc::WeakSender<BrokerAction>>,
    limits: web::Data<Limits>,
) -> Result<HttpResponse, error::Error> {
    let req_body = req_body.into_inner();
    if req_body.work_ids.len() > limits.max_work_results {
        return Err(error::ErrorBadRequest(format!(
            "Too many work results requested: (max {} allowed)",
            limits.max_work_results
        )));
    }
    // Send the request to the publisher (tx is Weak and needs upgrading)
    tx.upgrade()
        .ok_or_else(|| {
            error!("Cannot upgrade Sender: publisher lost");
            error::ErrorInternalServerError("Internal error: publisher lost")
        })?
        .send(BrokerAction::ApplyScenarios(req_body.work_ids))
        .await
        .map_err(|e| {
            error!("Failed to communicate with publisher: {e}");
            error::ErrorInternalServerError("Internal error: publisher communication failed")
        })?;
    Ok(HttpResponse::NoContent().finish())
}

/// Sets up the URL routing
pub fn app_setup(cfg: &mut web::ServiceConfig) {
    cfg.service(metrics)
        .service(submit_v1)
        .service(get_work_graph_v1)
        .service(get_works_graphs_v1)
        .service(get_object_v1)
        .service(search_v1)
        .service(count_v1)
        .service(add_scenario_v1)
        .service(del_scenario_v1)
        .service(get_scenario_v1)
        .service(list_scenarios_v1)
        .service(get_work_actions_v1)
        .service(reload_actions_v1)
        .service(apply_scenarios_v1);
}
