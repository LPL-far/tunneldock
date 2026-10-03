use super::{evidence, model::*, store};
use crate::models::{AgentRun, ProjectTask};
use std::path::Path;

pub fn deadline_passed(c: &Campaign, now: i64) -> bool {
    chrono::DateTime::parse_from_rfc3339(&c.plan.deadline)
        .map(|d| now >= d.timestamp())
        .unwrap_or(true)
}
fn stop(c: &mut Campaign, reason: &str) {
    c.state = "awaiting_web_review".into();
    c.reason = reason.into();
}
/// One bounded protocol step, without dispatch. All file hashing happens outside the global lock.
pub fn reconcile(
    root: &Path,
    record_dir: &Path,
    c: &mut Campaign,
    runs: &[AgentRun],
    now: i64,
) -> Result<(), String> {
    if c.authorization_ref.is_none() || c.web_reviewed {
        return Ok(());
    }
    if let Some(a) = c.attempts.last().cloned() {
        if a.evidence == "pending" && a.run_id.is_some() {
            let run = runs
                .iter()
                .find(|r| Some(&r.id) == a.run_id.as_ref() && r.task_id == a.task_id);
            if let Some(run) = run {
                if matches!(run.status.as_str(), "running" | "interactive") {
                    c.attempts.last_mut().unwrap().execution = "running".into();
                } else if run.status == "completed" {
                    let verified = evidence::verify(root, c, &a, runs);
                    let current = c.attempts.last_mut().unwrap();
                    current.execution = if a.terminal_confirmation.as_deref() == Some("success") {
                        "execution_success".into()
                    } else {
                        "terminal_unconfirmed".into()
                    };
                    match verified {
                        Ok(v) => {
                            current.evidence = "evidence_verified".into();
                            current.gate = Some(v.passed);
                            current.metric = Some(v.metric);
                            let archived = store::archive(record_dir, &v.artifact)?;
                            if archived != v.evidence_id {
                                return Err("Evidence archive identity mismatch".into());
                            }
                            current.evidence_id = Some(archived);
                            let stage =
                                c.plan.stages.get(a.stage).ok_or("Invalid attempt stage")?;
                            let next = if v.passed {
                                stage.on_pass
                            } else {
                                stage.on_scientific_fail
                            };
                            if let Some(next) = next {
                                c.stage = next;
                            } else {
                                stop(
                                    c,
                                    if v.passed {
                                        "Approved stages finished"
                                    } else {
                                        "Valid scientific negative result"
                                    },
                                );
                            }
                        }
                        Err(e) => {
                            current.evidence = "evidence_invalid".into();
                            current.reason = e.clone();
                            stop(c, &e);
                        }
                    }
                } else if run.status == "failed" {
                    let confirmed = a.terminal_confirmation.as_deref() == Some("failure")
                        && run.finished_at.is_some();
                    let current = c.attempts.last_mut().unwrap();
                    current.execution = if confirmed {
                        "operational_failure"
                    } else {
                        "terminal_unconfirmed"
                    }
                    .into();
                    current.evidence = "not_available".into();
                    current.reason = run
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "Execution failed".into());
                    let used = c.attempts.iter().filter(|a| a.stage == c.stage).count() as u32;
                    if !confirmed
                        || !c.plan.retry_confirmed_operational_failure
                        || used
                            >= c.plan
                                .stages
                                .get(c.stage)
                                .ok_or("Invalid current stage")?
                                .max_attempts
                    {
                        stop(
                            c,
                            if confirmed {
                                "Operational failure; retry not allowed/exhausted"
                            } else {
                                "Uncertain terminal failure; never replay"
                            },
                        );
                    }
                }
            } else {
                c.attempts.last_mut().unwrap().execution = "submission_uncertain".into();
                stop(c, "Reserved submission has no durable run; never replay");
            }
        }
    }
    if deadline_passed(c, now) {
        stop(c, "Scheduling deadline reached; active work drains");
    }
    let pending = c
        .attempts
        .last()
        .map(|a| a.evidence == "pending")
        .unwrap_or(false);
    if c.paused || c.state != "active" || pending {
        return Ok(());
    }
    if c.attempts.len() >= c.plan.max_submissions as usize {
        stop(c, "Total attempt budget exhausted");
        return Ok(());
    }
    let number = c.attempts.iter().filter(|a| a.stage == c.stage).count() as u32 + 1;
    if number
        > c.plan
            .stages
            .get(c.stage)
            .ok_or("Invalid current stage")?
            .max_attempts
    {
        stop(c, "Stage attempt budget exhausted");
        return Ok(());
    }
    if let Err(e) = evidence::provenance(root, &c.plan) {
        stop(c, &format!("evidence_invalid: {e}"));
        return Ok(());
    }
    c.attempts.push(Attempt {
        stage: c.stage,
        number,
        task_id: format!("CAMPAIGN-{}-{}-{number}", c.plan.id, c.stage),
        run_id: None,
        terminal_confirmation: None,
        execution: "queued".into(),
        evidence: "pending".into(),
        gate: None,
        metric: None,
        evidence_id: None,
        reason: String::new(),
    });
    Ok(())
}

pub fn task(c: &Campaign, a: &Attempt, now: &str) -> Result<ProjectTask, String> {
    let s = c.plan.stages.get(a.stage).ok_or("Invalid task stage")?;
    let goal = format!("Approved campaign {} / stage {} / attempt {}. Plan SHA256: {}.\nQuestion: {}\nHypothesis: {}\nInstruction: {}\nOnly intervention scope: {}\nGate: {:?} >= {}; exact reference {} ({} samples).\nWrite EVIDENCE.json beside this run's HANDOFF.md: version=1, campaign_id, plan_sha256, stage_id, task_id, run_id, samples=[{{id,value,target}}]. Use this task ID and the run ID from the HANDOFF directory. All IDs/targets must match the frozen reference. Do not declare acceptance, change reference/evaluator, retry research, or create stages. Evidence is independently checked; Web review remains required.",
        c.plan.id,s.id,a.number,c.plan_sha256,c.plan.question,c.plan.hypothesis,s.instruction,
        s.intervention_scope,s.gate.metric,s.gate.minimum,s.gate.reference_path,s.gate.sample_count);
    Ok(ProjectTask {
        id: a.task_id.clone(),
        title: format!("{} / {}", c.plan.id, s.id),
        goal,
        owner: "codex".into(),
        reviewers: vec!["chatgpt".into()],
        status: "queued".into(),
        write_scope: vec![s.intervention_scope.clone()],
        summary: String::new(),
        kind: "campaign".into(),
        thread_id: a.task_id.clone(),
        consultation_id: String::new(),
        web_reviewed: false,
        memory_committed: false,
        cleanup_committed: false,
        auto_dispatch: true,
        finalization_policy: "campaign".into(),
        review_mode: "web".into(),
        reviewed_by: String::new(),
        created_at: now.into(),
        updated_at: now.into(),
    })
}

pub fn dispatch_allowed(c: &Campaign, a: &Attempt, now: i64) -> Result<(), String> {
    if c.authorization_ref
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
        || c.plan_sha256 != store::plan_hash(&c.plan)?
        || c.paused
        || c.state != "active"
        || deadline_passed(c, now)
        || a.run_id.is_some()
        || a.execution != "queued"
        || a.stage != c.stage
        || c.attempts.last().map(|x| &x.task_id) != Some(&a.task_id)
        || c.attempts.len() > c.plan.max_submissions as usize
    {
        return Err("Campaign dispatch closed (authorization, pause, deadline, budget or prior reservation)".into());
    }
    Ok(())
}
