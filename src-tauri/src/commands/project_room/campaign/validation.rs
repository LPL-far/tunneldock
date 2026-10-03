use super::{model::*, store};
use std::collections::{BTreeMap, HashSet};

pub fn persisted(data: &Store) -> Result<(), String> {
    if data.campaigns.len() > 32 {
        return Err("Too many campaigns".into());
    }
    let mut ids = HashSet::new();
    let mut tasks = HashSet::new();
    let mut runs = HashSet::new();
    for c in &data.campaigns {
        store::validate(&c.plan)?;
        if !ids.insert(&c.plan.id)
            || c.plan_sha256 != store::plan_hash(&c.plan)?
            || c.plan.stages.get(c.stage).is_none()
            || c.attempts.len() > c.plan.max_submissions as usize
            || !matches!(
                c.state.as_str(),
                "draft" | "active" | "awaiting_web_review" | "reviewed"
            )
            || c.authorization_ref
                .as_ref()
                .is_some_and(|r| r.trim().is_empty() || r.len() > 1000)
            || (c.authorization_ref.is_none() != (c.state == "draft"))
            || (c.state == "draft" && !c.attempts.is_empty())
            || c.web_reviewed != (c.state == "reviewed")
            || (c.accepted && !c.web_reviewed)
            || c.web_reviewed
                != c.review_ref
                    .as_ref()
                    .is_some_and(|r| !r.trim().is_empty() && r.len() <= 1000)
        {
            return Err("Corrupt campaign identity/state".into());
        }
        let mut counts = BTreeMap::new();
        let mut previous: Option<&Attempt> = None;
        for a in &c.attempts {
            let stage = c.plan.stages.get(a.stage).ok_or("Corrupt attempt stage")?;
            let count = counts.entry(a.stage).or_insert(0u32);
            *count += 1;
            if a.number != *count
                || a.number > stage.max_attempts
                || a.task_id != format!("CAMPAIGN-{}-{}-{}", c.plan.id, a.stage, a.number)
                || !tasks.insert(&a.task_id)
                || a.run_id
                    .as_ref()
                    .is_some_and(|id| !store::stable_id(id) || !runs.insert(id))
            {
                return Err("Corrupt campaign attempt identity/budget".into());
            }
            attempt_status(a)?;
            if let Some(p) = previous {
                if p.stage == a.stage {
                    if !c.plan.retry_confirmed_operational_failure
                        || p.execution != "operational_failure"
                        || p.terminal_confirmation.as_deref() != Some("failure")
                    {
                        return Err("Unapproved retry history".into());
                    }
                } else {
                    let prior_stage = c.plan.stages.get(p.stage).ok_or("Invalid prior stage")?;
                    let next = match p.gate {
                        Some(true) => prior_stage.on_pass,
                        Some(false) => prior_stage.on_scientific_fail,
                        None => None,
                    };
                    if next != Some(a.stage) {
                        return Err("Unapproved stage history".into());
                    }
                }
            } else if a.stage != 0 {
                return Err("Campaign must start at stage zero".into());
            }
            previous = Some(a);
        }
        if let Some(a) = previous {
            let stage = c.plan.stages.get(a.stage).ok_or("Invalid last stage")?;
            let next = match a.gate {
                Some(true) => stage.on_pass,
                Some(false) => stage.on_scientific_fail,
                None => None,
            };
            if c.stage != a.stage && next != Some(c.stage) {
                return Err("Corrupt current stage".into());
            }
        } else if c.stage != 0 {
            return Err("Empty campaign has nonzero stage".into());
        }
        if c.accepted {
            pass_policy(c)?;
        }
    }
    Ok(())
}

fn attempt_status(a: &Attempt) -> Result<(), String> {
    let terminal = a.terminal_confirmation.as_deref();
    if terminal.is_some_and(|s| !matches!(s, "success" | "failure"))
        || (terminal.is_some() && a.run_id.is_none())
    {
        return Err("Invalid terminal confirmation".into());
    }
    let valid = match a.evidence.as_str() {
        "pending" => match a.execution.as_str() {
            "queued" => a.run_id.is_none() && terminal.is_none(),
            "submission_uncertain" | "running" => a.run_id.is_some(),
            _ => false,
        },
        "evidence_verified" => {
            a.execution == "execution_success" && terminal == Some("success") && a.run_id.is_some()
        }
        "evidence_invalid" => {
            a.run_id.is_some()
                && matches!(
                    a.execution.as_str(),
                    "execution_success" | "terminal_unconfirmed"
                )
        }
        "not_available" => match a.execution.as_str() {
            "operational_failure" => terminal == Some("failure") && a.run_id.is_some(),
            "terminal_unconfirmed" => terminal.is_none() && a.run_id.is_some(),
            "not_submitted" => a.run_id.is_none() && terminal.is_none(),
            _ => false,
        },
        _ => false,
    };
    let verified = a.evidence == "evidence_verified";
    if !valid
        || a.gate.is_some() != verified
        || a.metric.is_some() != verified
        || a.evidence_id.is_some() != verified
        || a.metric.is_some_and(|m| !m.is_finite())
        || a.evidence_id.as_ref().is_some_and(|id| {
            id.len() != 64
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    {
        return Err("Corrupt campaign attempt status/evidence".into());
    }
    Ok(())
}

pub fn pass_policy(c: &Campaign) -> Result<(), String> {
    let mut effective = BTreeMap::new();
    for a in &c.attempts {
        if a.gate != Some(true)
            && !(a.gate.is_none()
                && a.execution == "operational_failure"
                && a.terminal_confirmation.as_deref() == Some("failure"))
        {
            return Err(
                "Cannot accept scientific negative, unknown or invalid evidence as PASS".into(),
            );
        }
        effective.insert(a.stage, a);
    }
    let last = c.attempts.last().ok_or("No verified attempts")?;
    if effective.values().any(|a| a.gate != Some(true))
        || c.plan
            .stages
            .get(last.stage)
            .ok_or("Invalid final stage")?
            .on_pass
            .is_some()
    {
        return Err("Every traversed stage must end in a verified pass".into());
    }
    Ok(())
}
