use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Scenario {
    pub name: String,
    pub min_ver: u16,
    pub max_ver: Option<u16>,
    pub creator: String,
    pub description: String,
    pub local_query: String,
    pub context: Option<Contextual>,
    pub action: String,
}

#[derive(Deserialize, Serialize)]
pub struct Contextual {
    pub global_query: String,
    pub min_matches: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DirectorRequest {
    pub work_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WorkAction {
    pub scenario: String,
    pub ctime: f64,
    pub action: String,
}

#[derive(Debug, Serialize)]
pub struct WorkActions {
    pub work_id: String,
    #[serde(serialize_with = "crate::time_to_f64")]
    pub t: std::time::SystemTime,
    pub actions: Vec<WorkAction>,
}
