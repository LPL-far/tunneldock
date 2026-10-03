use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FrozenFile {
    pub role: String,
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    Mean,
    Pearson,
    Spearman,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    pub metric: Metric,
    pub minimum: f64,
    /// Frozen JSON map from exact sample ID to immutable numeric target.
    pub reference_path: String,
    pub sample_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub id: String,
    pub instruction: String,
    pub intervention_scope: String,
    pub gate: Gate,
    pub max_attempts: u32,
    /// Only forward indices are accepted; None means stop for Web review.
    pub on_pass: Option<usize>,
    pub on_scientific_fail: Option<usize>,
    pub on_unknown: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub version: u32,
    pub id: String,
    pub question: String,
    pub hypothesis: String,
    pub provenance: Vec<FrozenFile>,
    pub stages: Vec<Stage>,
    pub max_submissions: u32,
    pub deadline: String,
    pub retry_confirmed_operational_failure: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Attempt {
    pub stage: usize,
    pub number: u32,
    pub task_id: String,
    pub run_id: Option<String>,
    pub terminal_confirmation: Option<String>,
    pub execution: String,
    pub evidence: String,
    pub gate: Option<bool>,
    pub metric: Option<f64>,
    pub evidence_id: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Campaign {
    pub plan: Plan,
    pub plan_sha256: String,
    pub authorization_ref: Option<String>,
    pub paused: bool,
    pub state: String,
    pub stage: usize,
    pub attempts: Vec<Attempt>,
    pub reason: String,
    pub web_reviewed: bool,
    pub accepted: bool,
    pub review_ref: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Store {
    pub revision: u64,
    pub campaigns: Vec<Campaign>,
}

pub fn is_task(task: &crate::models::ProjectTask) -> bool {
    task.kind == "campaign" || task.id.starts_with("CAMPAIGN-")
}
