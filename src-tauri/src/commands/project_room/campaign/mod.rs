//! Bounded protocol reconciliation on the existing supervisor; never starts workers itself.
mod engine;
mod events;
mod evidence;
#[cfg(test)]
mod independent_tests;
pub(super) mod model;
mod presentation;
#[cfg(test)]
mod regression_tests;
mod review;
mod store;
#[cfg(test)]
mod tests;
mod validation;

use super::*;
use model::*;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

const SUMMARY: &str = "campaign_status.json";
static CURSOR: AtomicUsize = AtomicUsize::new(0);
#[tauri::command]
pub fn campaign_plan(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    campaign_id: String,
) -> Result<Value, String> {
    let _guard = state.project_store_lock.lock();
    let data = store::load(&project_dir(&state, &project_id)?)?;
    let c = data
        .campaigns
        .iter()
        .find(|c| c.plan.id == campaign_id)
        .ok_or("Unknown campaign")?;
    Ok(json!({"plan":c.plan,"plan_sha256":c.plan_sha256,"record":c}))
}
pub(super) fn summary(config: &ProjectRoomConfig) -> Value {
    let path = Path::new(&config.local_root).join(BRIDGE_DIR).join(SUMMARY);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({"items":[]}),
        Err(e) => json!({"items":[],"error":e.to_string()}),
        Ok(_) => match store::read::<Value>(&path) {
            Ok(value)
                if value["items"]
                    .as_array()
                    .is_some_and(|items| items.len() <= 4)
                    && serde_json::to_vec(&value)
                        .map(|v| v.len() <= 12 * 1024)
                        .unwrap_or(false) =>
            {
                value
            }
            Ok(_) => {
                json!({"items":[],"error":"Campaign summary exceeds polling bounds; awaiting reconciliation"})
            }
            Err(e) => json!({"items":[],"error":e.chars().take(400).collect::<String>()}),
        },
    }
}
use presentation::publish;
#[tauri::command]
pub fn campaign_details(
    state: State<'_, Arc<AppState>>,
    project_id: String,
) -> Result<Value, String> {
    let _guard = state.project_store_lock.lock();
    let dir = project_dir(&state, &project_id)?;
    let config = store::read(&dir.join(CONFIG_FILE))?;
    let data = store::load(&dir)?;
    let runs = store::read::<Vec<AgentRun>>(&dir.join(RUNS_FILE))?;
    Ok(presentation::details(&config, &dir, &data, &runs))
}
pub(super) fn process_event(
    state: &AppState,
    project_id: &str,
    value: &Value,
    human: bool,
) -> Result<(), String> {
    let dir = project_dir(state, project_id)?;
    let mut data = store::load(&dir)?;
    let before = data.clone();
    if value["kind"] == "campaign.reviewed" {
        return review::enqueue(&dir, &data, value);
    }
    events::apply(&mut data, value, human)?;
    if data != before {
        store::save(&dir, &mut data)?;
    }
    let config = store::read(&dir.join(CONFIG_FILE))?;
    let runs = store::read::<Vec<AgentRun>>(&dir.join(RUNS_FILE))?;
    publish(&config, &dir, &data, &runs)
}

#[tauri::command]
pub fn campaign_control(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    event: Value,
) -> Result<Value, String> {
    if !matches!(
        event["kind"].as_str(),
        Some("campaign.authorize" | "campaign.pause" | "campaign.resume" | "campaign.draft")
    ) {
        return Err("Human controls support draft, authorize, pause and resume".into());
    }
    let _guard = state.project_store_lock.lock();
    process_event(&state, &project_id, &event, true)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    sync_project_bridge(&snapshot)?;
    Ok(summary(&snapshot.config))
}

pub(super) fn allowed(state: &AppState, project: &str, task: &ProjectTask) -> Result<(), String> {
    if !model::is_task(task) {
        return Ok(());
    }
    let dir = project_dir(state, project)?;
    let config: ProjectRoomConfig = store::read(&dir.join(CONFIG_FILE))?;
    if !config.enabled {
        return Err("Project Room is disabled".into());
    }
    let data = store::load(&dir)?;
    let (c, a) = data
        .campaigns
        .iter()
        .find_map(|c| {
            c.attempts
                .iter()
                .find(|a| a.task_id == task.id)
                .map(|a| (c, a))
        })
        .ok_or("Unregistered campaign task")?;
    engine::dispatch_allowed(c, a, chrono::Utc::now().timestamp())?;
    let runs = store::read::<Vec<AgentRun>>(&dir.join(RUNS_FILE))?;
    if runs.iter().any(|r| r.task_id == task.id) {
        return Err("Campaign attempt already has a run; never replay".into());
    }
    let expected = engine::task(c, a, &task.created_at)?;
    if task.kind != expected.kind
        || task.owner != expected.owner
        || task.goal != expected.goal
        || task.write_scope != expected.write_scope
        || task.thread_id != expected.thread_id
        || task.review_mode != "web"
        || task.status != "queued"
        || task.web_reviewed
    {
        return Err("Campaign task differs from approved plan".into());
    }
    Ok(())
}
/// Must run immediately before any external submission; errors after this point remain uncertain.
pub(super) fn reserve(
    state: &AppState,
    project: &str,
    task: &ProjectTask,
    run_id: &str,
) -> Result<(), String> {
    if !model::is_task(task) {
        return Ok(());
    }
    allowed(state, project, task)?;
    let dir = project_dir(state, project)?;
    let mut data = store::load(&dir)?;
    let a = data
        .campaigns
        .iter_mut()
        .flat_map(|c| &mut c.attempts)
        .find(|a| a.task_id == task.id)
        .ok_or("Missing reservation")?;
    a.run_id = Some(run_id.into());
    a.execution = "submission_uncertain".into();
    store::save(&dir, &mut data)
}
pub(super) fn confirm_terminal(dir: &Path, run: &AgentRun, success: bool) -> Result<(), String> {
    if !run.task_id.starts_with("CAMPAIGN-") {
        return Ok(());
    }
    let mut data = store::load(dir)?;
    let a = data
        .campaigns
        .iter_mut()
        .flat_map(|c| &mut c.attempts)
        .find(|a| a.task_id == run.task_id && a.run_id.as_deref() == Some(&run.id))
        .ok_or("Terminal run reservation mismatch")?;
    a.terminal_confirmation = Some(if success { "success" } else { "failure" }.into());
    store::save(dir, &mut data)
}

fn reconcile_project(state: &AppState, id: &str) -> Result<(), String> {
    let dir = project_dir(state, id)?;
    let (config, original, runs, original_tasks) = {
        let _guard = state.project_store_lock.lock();
        (
            store::read::<ProjectRoomConfig>(&dir.join(CONFIG_FILE))?,
            store::load(&dir)?,
            store::read::<Vec<AgentRun>>(&dir.join(RUNS_FILE))?,
            store::read::<Vec<ProjectTask>>(&dir.join(TASKS_FILE))?,
        )
    };
    if original.campaigns.is_empty() {
        let cached = summary(&config);
        if cached["total"].as_u64().unwrap_or(0) > 0
            || cached["error"]
                .as_str()
                .is_some_and(|e| e.contains("campaign control store is missing or empty"))
        {
            return Err("Previously recorded campaign control store is missing or empty".into());
        }
        return publish(&config, &dir, &original, &runs);
    }
    let mut data = original.clone();
    let eligible = data
        .campaigns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.authorization_ref.is_some() && !c.web_reviewed)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let mut reviewed_request = None;
    if !eligible.is_empty() {
        let index = eligible[CURSOR.load(Ordering::Relaxed) % eligible.len()];
        let was_paused = data.campaigns[index].paused;
        // Disabling a room stops new scheduling but still records already-running evidence.
        if !config.enabled {
            data.campaigns[index].paused = true;
        }
        engine::reconcile(
            Path::new(&config.local_root),
            &dir,
            &mut data.campaigns[index],
            &runs,
            chrono::Utc::now().timestamp(),
        )?;
        data.campaigns[index].paused = was_paused;
        let c = &data.campaigns[index];
        if let Some(event) = review::pending(&dir, &c.plan.id)? {
            let checked = if event["accepted"] == true {
                review::verify_acceptance(Path::new(&config.local_root), &dir, c, &runs)
            } else {
                Ok(())
            };
            let id = c.plan.id.clone();
            match checked.and_then(|_| events::apply(&mut data, &event, false)) {
                Ok(()) => {}
                Err(error) => {
                    data.campaigns[index].reason = format!("Web review rejected: {error}")
                }
            }
            reviewed_request = Some((id, event));
        }
    }
    let _guard = state.project_store_lock.lock();
    if store::load(&dir)? != original
        || store::read::<Vec<AgentRun>>(&dir.join(RUNS_FILE))? != runs
        || store::read::<Vec<ProjectTask>>(&dir.join(TASKS_FILE))? != original_tasks
        || store::read::<ProjectRoomConfig>(&dir.join(CONFIG_FILE))? != config
    {
        return Ok(());
    }
    if let Some((id, event)) = &reviewed_request {
        if review::pending(&dir, id)?.as_ref() != Some(event) {
            return Ok(());
        }
    }
    // Persist the attempt/task identity first. A crash before tasks.json is repaired below.
    if data != original {
        store::save(&dir, &mut data)?;
    }
    if let Some((id, _)) = &reviewed_request {
        review::remove(&dir, id)?;
    }
    let mut tasks = original_tasks;
    let before = tasks.clone();
    for c in &data.campaigns {
        for a in &c.attempts {
            if a.run_id.is_none() && !tasks.iter().any(|t| t.id == a.task_id) {
                tasks.push(engine::task(c, a, &local_now_rfc3339())?);
            }
            if let Some(t) = tasks.iter_mut().find(|t| t.id == a.task_id) {
                t.auto_dispatch = config.enabled
                    && engine::dispatch_allowed(c, a, chrono::Utc::now().timestamp()).is_ok();
                if a.evidence == "evidence_verified"
                    && a.terminal_confirmation.as_deref() == Some("success")
                {
                    t.status = "completed".into();
                    t.reviewed_by = "campaign-evidence-verifier".into();
                    t.summary=format!("Execution complete; evidence_verified {}; gate={:?}; Web acceptance pending",a.evidence_id.as_deref().unwrap_or("missing"),a.gate);
                    t.web_reviewed = false;
                }
                // No external reservation means restoring the same queued task cannot replay a run.
                if t.auto_dispatch
                    && a.run_id.is_none()
                    && !runs.iter().any(|r| r.task_id == a.task_id)
                    && matches!(t.status.as_str(), "blocked" | "queued")
                {
                    t.status = "queued".into();
                }
                if c.web_reviewed {
                    t.web_reviewed = true;
                    t.reviewed_by = "chatgpt".into();
                    t.status = if c.accepted && a.gate == Some(true) {
                        "completed"
                    } else {
                        "blocked"
                    }
                    .into();
                    t.summary=format!("Web review recorded; campaign accepted={}; execution={}; evidence={}; gate={:?}; evidence_id={}",
                        c.accepted,a.execution,a.evidence,a.gate,a.evidence_id.as_deref().unwrap_or("none"));
                }
            }
        }
    }
    if tasks != before {
        store::atomic(&dir.join(TASKS_FILE), &tasks)?;
    }
    publish(&config, &dir, &data, &runs)
}
pub(super) fn reconcile_once(state: &AppState, ids: &[String]) -> Result<(), String> {
    // Advance once per supervisor tick, not once per project (which can starve slots).
    CURSOR.fetch_add(1, Ordering::Relaxed);
    let mut errors = Vec::new();
    for id in ids {
        if let Err(error) = reconcile_project(state, &id) {
            let dir = project_dir(state, &id)?;
            let config: ProjectRoomConfig = store::read(&dir.join(CONFIG_FILE))?;
            store::atomic(
                &Path::new(&config.local_root).join(BRIDGE_DIR).join(SUMMARY),
                &json!({"items":[],"error":error}),
            )?;
            errors.push(format!("{id}: {error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
