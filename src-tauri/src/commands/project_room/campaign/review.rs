use super::{events, evidence, model::*, store};
use crate::models::AgentRun;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn path(dir: &Path, id: &str) -> Result<PathBuf, String> {
    if !store::stable_id(id) {
        return Err("Invalid review campaign ID".into());
    }
    store::safe_path(dir, &format!("campaign-review-{id}.json"))
}
pub fn pending(dir: &Path, id: &str) -> Result<Option<Value>, String> {
    let path = path(dir, id)?;
    match fs::metadata(&path) {
        Ok(_) => Ok(Some(store::read(&path)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
pub fn enqueue(dir: &Path, data: &Store, event: &Value) -> Result<(), String> {
    let mut candidate = data.clone();
    events::apply(&mut candidate, event, false)?;
    if candidate == *data {
        return Ok(());
    }
    let id = event["campaign_id"]
        .as_str()
        .ok_or("campaign_id required")?;
    if let Some(old) = pending(dir, id)? {
        return if old == *event {
            Ok(())
        } else {
            Err("A different Web review is pending verification".into())
        };
    }
    store::atomic(&path(dir, id)?, event)
}
pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    fs::remove_file(path(dir, id)?).map_err(|e| e.to_string())
}

/// All reads/hash/metric work is done by the off-lock reconciler. Commit is snapshot-CAS.
pub fn verify_acceptance(
    root: &Path,
    dir: &Path,
    c: &Campaign,
    runs: &[AgentRun],
) -> Result<(), String> {
    super::validation::pass_policy(c)?;
    evidence::provenance(root, &c.plan)?;
    for a in c
        .attempts
        .iter()
        .filter(|a| a.execution == "operational_failure")
    {
        let matching = runs
            .iter()
            .filter(|r| r.task_id == a.task_id)
            .collect::<Vec<_>>();
        if matching.len() != 1
            || Some(matching[0].id.as_str()) != a.run_id.as_deref()
            || matching[0].status != "failed"
            || matching[0].finished_at.is_none()
        {
            return Err("Historical operational failure is missing or no longer terminal".into());
        }
    }
    for a in c
        .attempts
        .iter()
        .filter(|a| a.evidence == "evidence_verified")
    {
        let id = a.evidence_id.as_ref().ok_or("Missing evidence digest")?;
        let sealed = store::bytes(
            &store::safe_path(dir, &format!("campaign-evidence/{id}.json"))?,
            store::MAX_JSON,
        )?;
        if store::digest(&sealed) != *id {
            return Err("Sealed evidence integrity failure".into());
        }
        let verified = evidence::verify_current(root, c, a, runs)?;
        if verified.artifact != sealed
            || verified.evidence_id != *id
            || Some(verified.passed) != a.gate
            || a.metric.map(f64::to_bits) != Some(verified.metric.to_bits())
        {
            return Err("Current evidence differs from sealed verified receipt".into());
        }
    }
    Ok(())
}
