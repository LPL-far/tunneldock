use super::{model::*, store::*};
use crate::models::AgentRun;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

struct Reference(BTreeMap<String, f64>);
impl<'de> Deserialize<'de> for Reference {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Unique;
        impl<'de> serde::de::Visitor<'de> for Unique {
            type Value = Reference;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("unique sample ID to numeric target map")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Reference, A::Error> {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, f64>()? {
                    if key.is_empty()
                        || key.len() > 256
                        || values.insert(key, value).is_some()
                        || values.len() > 10000
                    {
                        return Err(serde::de::Error::custom(
                            "Invalid or duplicate reference sample ID",
                        ));
                    }
                }
                Ok(Reference(values))
            }
        }
        deserializer.deserialize_map(Unique)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sample {
    id: String,
    value: f64,
    target: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    version: u32,
    campaign_id: String,
    plan_sha256: String,
    stage_id: String,
    task_id: String,
    run_id: String,
    samples: Vec<Sample>,
}
pub struct Verified {
    pub metric: f64,
    pub passed: bool,
    pub artifact: Vec<u8>,
    pub evidence_id: String,
}

pub fn provenance(root: &Path, plan: &Plan) -> Result<(), String> {
    // Small frozen manifests are supported; large dataset/model payloads belong behind manifests.
    let mut total = 0;
    for f in &plan.provenance {
        let data = bytes(&safe_path(root, &f.path)?, 8 * 1024 * 1024)?;
        total += data.len();
        if total > 32 * 1024 * 1024 {
            return Err("Frozen provenance exceeds 32 MiB".into());
        }
        if digest(&data) != f.sha256 {
            return Err(format!("Frozen source drift: {}", f.path));
        }
    }
    for stage in &plan.stages {
        safe_path(root, &stage.intervention_scope)?;
    }
    Ok(())
}
pub fn verify(
    root: &Path,
    c: &Campaign,
    a: &Attempt,
    runs: &[AgentRun],
) -> Result<Verified, String> {
    provenance(root, &c.plan)?;
    verify_current(root, c, a, runs)
}

pub(super) fn verify_current(
    root: &Path,
    c: &Campaign,
    a: &Attempt,
    runs: &[AgentRun],
) -> Result<Verified, String> {
    let matches = runs
        .iter()
        .filter(|r| r.task_id == a.task_id)
        .collect::<Vec<_>>();
    // Each attempt owns exactly one task and one reserved run. Any extra run is ambiguous.
    if matches.len() != 1 {
        return Err("Missing or ambiguous current attempt run".into());
    }
    let run = matches[0];
    if a.run_id.as_deref() != Some(&run.id)
        || run.status != "completed"
        || run.finished_at.is_none()
        || run.error_message.is_some()
        || a.terminal_confirmation.as_deref() != Some("success")
    {
        return Err("Current run is not transport-confirmed terminal success".into());
    }
    let handoff_rel = format!(".tunneldock/runs/{}/HANDOFF.md", run.id);
    let handoff = safe_path(root, &handoff_rel)?;
    if Path::new(&run.output_path) != handoff || bytes(&handoff, 256 * 1024)?.is_empty() {
        return Err("Missing/mismatched bounded HANDOFF".into());
    }
    let data = bytes(
        &safe_path(root, &format!(".tunneldock/runs/{}/EVIDENCE.json", run.id))?,
        MAX_JSON,
    )?;
    let artifact: Artifact =
        serde_json::from_slice(&data).map_err(|e| format!("Invalid evidence: {e}"))?;
    let stage = c.plan.stages.get(a.stage).ok_or("Invalid attempt stage")?;
    if artifact.version != 1
        || artifact.campaign_id != c.plan.id
        || artifact.plan_sha256 != c.plan_sha256
        || artifact.stage_id != stage.id
        || artifact.task_id != a.task_id
        || artifact.run_id != run.id
    {
        return Err("Evidence identity mismatch".into());
    }
    let reference_bytes = bytes(&safe_path(root, &stage.gate.reference_path)?, MAX_JSON)?;
    let reference_hash = &c
        .plan
        .provenance
        .iter()
        .find(|f| f.path == stage.gate.reference_path)
        .ok_or("Missing frozen reference")?
        .sha256;
    if digest(&reference_bytes) != *reference_hash {
        return Err("Immutable reference drift".into());
    }
    let targets = serde_json::from_slice::<Reference>(&reference_bytes)
        .map_err(|e| e.to_string())?
        .0;
    if targets.len() != stage.gate.sample_count
        || artifact.samples.len() != targets.len()
        || targets.values().any(|x| !x.is_finite())
    {
        return Err("Sample/reference count or numeric value invalid".into());
    }
    let mut seen = HashSet::new();
    let mut x = Vec::new();
    let mut y = Vec::new();
    for sample in artifact.samples {
        if !seen.insert(sample.id.clone())
            || !sample.value.is_finite()
            || !sample.target.is_finite()
            || targets.get(&sample.id).map(|v| v.to_bits()) != Some(sample.target.to_bits())
        {
            return Err(
                "Duplicate/unknown sample, nonfinite value, or immutable target mismatch".into(),
            );
        }
        x.push(sample.value);
        y.push(sample.target);
    }
    let metric = recompute(&stage.gate.metric, &x, &y)?;
    Ok(Verified {
        metric,
        passed: metric >= stage.gate.minimum,
        evidence_id: digest(&data),
        artifact: data,
    })
}
pub fn recompute(metric: &Metric, x: &[f64], y: &[f64]) -> Result<f64, String> {
    if x.is_empty() || x.len() != y.len() || x.iter().chain(y).any(|v| !v.is_finite()) {
        return Err("Invalid numeric samples".into());
    }
    let result = match metric {
        Metric::Mean => x.iter().map(|v| v / x.len() as f64).sum(),
        Metric::Pearson => pearson(x, y)?,
        Metric::Spearman => pearson(&ranks(x), &ranks(y))?,
    };
    if !result.is_finite() {
        return Err("Metric arithmetic overflow".into());
    }
    Ok(result)
}
fn pearson(x: &[f64], y: &[f64]) -> Result<f64, String> {
    if x.len() < 2 {
        return Err("Correlation requires at least two samples".into());
    }
    let mx = x.iter().map(|v| v / x.len() as f64).sum::<f64>();
    let my = y.iter().map(|v| v / y.len() as f64).sum::<f64>();
    let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        let a = a - mx;
        let b = b - my;
        xy += a * b;
        xx += a * a;
        yy += b * b;
    }
    if xx <= 0.0 || yy <= 0.0 || !xx.is_finite() || !yy.is_finite() {
        return Err("Undefined correlation/overflow".into());
    }
    let value = (xy / xx.sqrt()) / yy.sqrt();
    if !value.is_finite() {
        return Err("Correlation overflow".into());
    }
    Ok(value.clamp(-1.0, 1.0))
}
fn ranks(values: &[f64]) -> Vec<f64> {
    let mut order = (0..values.len()).collect::<Vec<_>>();
    order.sort_by(|a, b| values[*a].total_cmp(&values[*b]));
    let mut result = vec![0.0; values.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len() && values[order[start]] == values[order[end]] {
            end += 1;
        }
        for i in start..end {
            result[order[i]] = (start + end - 1) as f64 / 2.0 + 1.0;
        }
        start = end;
    }
    result
}
