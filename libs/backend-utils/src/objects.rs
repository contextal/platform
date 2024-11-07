//! Exchange object formats
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A key/value map - used by `object_metadata` and `relation_metadata`
pub type Metadata = serde_json::Map<String, serde_json::Value>;

/// The JSON struct passed to the backend
#[derive(Debug, Deserialize)]
pub struct BackendRequest {
    /// Static information related to this object
    pub object: Info,
    /// The symbols associated to this object (as set by the parent worker)
    pub symbols: Vec<String>,
    /// The relation metadata linking the parent object to this object
    pub relation_metadata: Metadata,
}

/// The JSON representing the object to perform work upon
#[derive(Debug, Deserialize)]
pub struct Info {
    /// The object origin
    pub org: String,
    /// The object ID
    pub object_id: String,
    /// The determined object type
    pub object_type: String,
    /// The determined object subtype
    pub object_subtype: Option<String>,
    /// The recursion level
    pub recursion_level: u32,
    /// The object size
    pub size: u64,
    /// Digests for of the object
    pub hashes: HashMap<String, String>,
    /// The creation time of the object
    pub ctime: f64,
}

/// The ok/error portion of the result
#[derive(Debug, Serialize)]
#[allow(non_camel_case_types)]
pub enum BackendResultKind {
    /// An "ok" result
    ok(BackendResultOk),
    /// An "error" result
    error(String),
}

/// The ok portion of the result
#[derive(Debug, Serialize)]
pub struct BackendResultOk {
    /// The symbols generated by the backend for the object in the request
    pub symbols: Vec<String>,
    /// The metadata generated by the backend for the object in the request
    pub object_metadata: Metadata,
    /// Any child objects extracted by the backend
    pub children: Vec<BackendResultChild>,
}

/// A child object extracted by the backend
#[derive(Debug, Serialize)]
pub struct BackendResultChild {
    /// The path to the extracted object data
    ///
    /// If the data extraction failed for whatever reason (*Failed Child*), this
    /// shall be set to `None`
    pub path: Option<String>,
    /// Skip type auto-detection on this child and force it (only affects non Failed children)
    pub force_type: Option<String>,
    /// The symbols associated to this child object
    pub symbols: Vec<String>,
    /// The relation metadata linking the main object to this child
    pub relation_metadata: Metadata,
}