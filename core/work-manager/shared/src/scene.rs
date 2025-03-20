use serde::{Deserialize, Serialize};

/// The base version requirement
const SCN_MIN_VER: &str = ">=1.3.0";

#[derive(Deserialize, Serialize)]
pub struct Scenario {
    pub name: String,
    pub compatible_with: Option<semver::VersionReq>,
    pub creator: String,
    pub description: String,
    pub local_query: String,
    pub context: Option<Contextual>,
    pub action: String,
}

impl Scenario {
    pub fn is_compatible(&self) -> bool {
        self.compatible_with
            .as_ref()
            .map(|vreq| vreq.matches(&pgrules::CURRENT_VERSION))
            .unwrap_or(true)
    }

    pub fn compatibility(&self) -> String {
        self.compatible_with
            .as_ref()
            .map(|vreq| vreq.to_string())
            .unwrap_or_else(|| SCN_MIN_VER.to_string())
    }
}

#[derive(Deserialize, Serialize)]
pub struct Contextual {
    pub global_query: String,
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
