//! Bounded, read-only presentation of worker execution and actual Web review.
//! A completed worker is a received result, never proof of a Web review.
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};

const ACTIVITY_FILE: &str = "agent_activity.json";
const PROGRESS_FILE: &str = "PROGRESS.json";
const REVIEW_FILE: &str = "WEB_REVIEW.json";

fn write_snapshot(path: &Path, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    if fs::read_to_string(path).ok().as_deref() == Some(&text) {
        return Ok(());
    }
    let parent = path.parent().ok_or("Missing snapshot parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| format!("Atomic activity update failed: {e}"))
}

fn clip(s: &str, n: usize) -> String {
    crate::audit::redact_audit_text(s).chars().take(n).collect()
}
fn time(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|t| t.timestamp_millis())
        .unwrap_or(0)
}
fn sidecar(run: &AgentRun, name: &str) -> PathBuf {
    Path::new(&run.output_path).with_file_name(name)
}
fn read_small(path: &Path) -> Option<Value> {
    let f = File::open(path).ok()?;
    let mut s = String::new();
    f.take(16_384).read_to_string(&mut s).ok()?;
    serde_json::from_str(&s).ok()
}
fn has_handoff(run: &AgentRun) -> bool {
    run.status == "completed"
        && fs::metadata(&run.output_path)
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false)
}
fn latest_run<'a>(runs: &'a [AgentRun], task: &ProjectTask) -> Option<&'a AgentRun> {
    runs.iter()
        .filter(|r| r.task_id == task.id)
        .max_by_key(|r| time(&r.started_at))
}
fn current_run<'a>(runs: &'a [AgentRun], task: &ProjectTask) -> Option<&'a AgentRun> {
    let run = latest_run(runs, task)?;
    // A revise reuses the task ID. Its previous handoff is NOT the current result.
    if matches!(task.status.as_str(), "queued" | "backlog") {
        return None;
    }
    if task.status == "active" && !matches!(run.status.as_str(), "running" | "interactive") {
        return None;
    }
    Some(run)
}
fn phase(task: &ProjectTask, run: Option<&AgentRun>, review: Option<&Value>) -> &'static str {
    match task.status.as_str() {
        "queued" | "backlog" => "queued",
        "active" => "running",
        "failed" | "blocked" => "blocked",
        "superseded" => "superseded",
        "completed" if task.web_reviewed => {
            if task.kind == "consultation"
                && task.finalization_policy == "durable"
                && !(task.memory_committed && task.cleanup_committed)
            {
                "finalizing"
            } else {
                "reviewed"
            }
        }
        "completed" if task.reviewed_by == "local-finalizer" => "contract_checked",
        "review" if run.map(has_handoff).unwrap_or(false) => {
            if review.and_then(|v| v.get("status")).and_then(Value::as_str) == Some("reviewing") {
                "reviewing"
            } else {
                "received"
            }
        }
        "review" => "missing_evidence",
        "completed" => "closed_unverified",
        _ => "unknown",
    }
}
fn task_view(task: &ProjectTask, runs: &[AgentRun]) -> Value {
    let run = current_run(runs, task);
    let review = run
        .and_then(|r| read_small(&sidecar(r, REVIEW_FILE)))
        .filter(|v| v["run_id"].as_str() == run.map(|r| r.id.as_str()));
    let progress = run.and_then(|r| read_small(&sidecar(r, PROGRESS_FILE)));
    let outcome = phase(task, run, review.as_ref());
    json!({
        "task_id": task.id, "title": clip(&task.title, 140), "task_status": task.status,
        "agent_id": task.owner, "phase": outcome, "kind": task.kind,
        "attempts": runs.iter().filter(|r| r.task_id == task.id).count(),
        "run_id": run.map(|r| &r.id), "run_status": run.map(|r| &r.status),
        "thread_id": run.and_then(|r| r.external_session_id.as_ref()),
        "queued_at": task.created_at, "updated_at": task.updated_at,
        "started_at": run.map(|r| &r.started_at),
        "received_at": run.filter(|r| has_handoff(r)).and_then(|r| r.finished_at.as_ref()),
        "handoff_available": run.map(has_handoff).unwrap_or(false),
        "reviewed_by": task.reviewed_by, "web_reviewed": task.web_reviewed,
        "review_started_at": review.as_ref().and_then(|v| v.get("started_at")),
        "next_owner": if matches!(outcome, "received" | "reviewing" | "finalizing" | "blocked") { "chatgpt" } else if outcome == "running" { task.owner.as_str() } else { "none" },
        "progress": progress,
        "detail": if matches!(outcome, "blocked" | "received" | "reviewed" | "finalizing") {
            clip(&task.summary, 380)
        } else { String::new() },
        "error": run.and_then(|r| r.error_message.as_deref()).map(|s| clip(s, 300)),
    })
}

pub(super) fn sync(snapshot: &ProjectRoomSnapshot) -> Result<(), String> {
    let cards = ["codex", "gemini"].iter().map(|agent| {
        let mut tasks = snapshot.tasks.iter().filter(|t| t.owner == *agent && t.status != "superseded")
            .collect::<Vec<_>>();
        tasks.sort_by_key(|t| (match t.status.as_str() {
            "active" => 4, "review" => 3, "blocked" | "failed" => 2, "queued" => 1, _ => 0,
        }, time(&t.updated_at)));
        let task = tasks.last().copied();
        json!({"agent_id": agent, "current": task.map(|t| task_view(t, &snapshot.runs)),
            "queued": tasks.iter().filter(|t| t.status == "queued").count(),
            "transport": snapshot.transports.iter().find(|h| h.agent_id == *agent).map(|h| &h.status),
            "human_thread_id": if *agent == "codex" { snapshot.config.codex_thread_id.as_ref() } else { None },
            "automation_thread_id": if *agent == "codex" { snapshot.config.codex_automation_thread_id.as_ref() }
                else { snapshot.config.antigravity_cascade_id.as_ref() },
        })
    }).collect::<Vec<_>>();
    let mut pending = snapshot
        .tasks
        .iter()
        .filter(|t| t.status == "review" && !t.web_reviewed)
        .collect::<Vec<_>>();
    pending.sort_by_key(|t| time(&t.updated_at));
    let mut core = json!({"version": 1, "project_id": snapshot.config.id,
        "cards": cards, "pending_review_count": pending.len(),
        "pending_reviews": pending.iter().take(8).map(|t| task_view(t, &snapshot.runs)).collect::<Vec<_>>(),
        "boundary": "Worker completion is not Web review. An inactive ChatGPT browser is not automatically awakened."
    });
    let serialized = serde_json::to_vec(&core).map_err(|e| e.to_string())?;
    core["revision"] = json!(format!("{:x}", Sha256::digest(&serialized)));
    write_snapshot(
        &Path::new(&snapshot.config.local_root)
            .join(BRIDGE_DIR)
            .join(ACTIVITY_FILE),
        &core,
    )?;
    Ok(())
}

// Only event metadata is exposed, never tool arguments, raw logs or hidden reasoning.
pub(super) fn record_progress(output: &Path, units: usize, event: &str, at: Option<&str>) {
    let path = output.with_file_name(PROGRESS_FILE);
    let old = read_small(&path);
    if old.as_ref().is_some_and(|v| {
        v["units"].as_u64() == Some(units as u64) && v["event"].as_str() == Some(event)
    }) {
        return;
    }
    let value = json!({"units": units, "event": event, "last_event_at": at.map(ToOwned::to_owned)
        .unwrap_or_else(local_now_rfc3339)});
    if let Err(error) = write_snapshot(&path, &value) {
        eprintln!("progress write failed: {error}");
    }
}

pub(super) fn latest_review_target<'a>(
    tasks: &'a [ProjectTask],
    runs: &'a [AgentRun],
    value: &Value,
) -> Result<(&'a ProjectTask, &'a AgentRun), String> {
    let id = value["task_id"].as_str().ok_or("task_id required")?;
    let task = tasks.iter().find(|t| t.id == id).ok_or("task not found")?;
    let run = latest_run(runs, task).ok_or("No worker run to review")?;
    if let Some(expected) = value.get("run_id").and_then(Value::as_str) {
        if expected != run.id {
            return Err("Review refers to a superseded run; reread current result".into());
        }
    }
    Ok((task, run))
}

pub(super) fn start_review(
    state: &AppState,
    project_id: &str,
    value: &Value,
    author: &str,
) -> Result<(), String> {
    if author != "chatgpt" {
        return Err("Only the Web reviewer can report review.started".into());
    }
    if value["run_id"].as_str().is_none() {
        return Err("review.started requires run_id".into());
    }
    let dir = project_dir(state, project_id)?;
    let tasks: Vec<ProjectTask> = read_json(&dir.join(TASKS_FILE))?;
    let runs: Vec<AgentRun> = read_json(&dir.join(RUNS_FILE))?;
    let (task, run) = latest_review_target(&tasks, &runs, value)?;
    if task.status != "review" || !has_handoff(run) {
        return Err("Current completed handoff is not ready for review".into());
    }
    let path = sidecar(run, REVIEW_FILE);
    let started = read_small(&path)
        .and_then(|v| v["started_at"].as_str().map(ToOwned::to_owned))
        .unwrap_or_else(local_now_rfc3339);
    write_json(
        &path,
        &json!({"task_id":task.id,"run_id":run.id,"status":"reviewing",
        "reviewer":"chatgpt","started_at":started}),
    )
}

#[tauri::command]
pub fn get_project_activity(
    state: State<'_, Arc<AppState>>,
    project_id: String,
) -> Result<Value, String> {
    let dir = project_dir(&state, &project_id)?;
    let config: ProjectRoomConfig = read_json(&dir.join(CONFIG_FILE))?;
    let mut value: Value = read_json(
        &Path::new(&config.local_root)
            .join(BRIDGE_DIR)
            .join(ACTIVITY_FILE),
    )?;
    value["sampled_at"] = json!(local_now_rfc3339());
    value["context_engine"] = super::context_engine::status(&config);
    Ok(value)
}

#[tauri::command]
pub fn read_project_handoff(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    run_id: String,
    offset: Option<usize>,
) -> Result<Value, String> {
    let dir = project_dir(&state, &project_id)?;
    let runs: Vec<AgentRun> = read_json(&dir.join(RUNS_FILE))?;
    let run = runs
        .iter()
        .find(|r| r.id == run_id)
        .ok_or("Run does not belong to this project")?;
    if !has_handoff(run) {
        return Err("Completed handoff not available".into());
    }
    let config: ProjectRoomConfig = read_json(&dir.join(CONFIG_FILE))?;
    let root = Path::new(&config.local_root)
        .join(BRIDGE_DIR)
        .join(RUNS_DIR)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let path = Path::new(&run.output_path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(root) {
        return Err("Handoff lies outside the project run directory".into());
    }
    let mut f = File::open(&path).map_err(|e| e.to_string())?;
    let total = f.metadata().map_err(|e| e.to_string())?.len() as usize;
    let start = offset.unwrap_or(0).min(total);
    f.seek(SeekFrom::Start(start as u64))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    f.take(4096)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let end = match std::str::from_utf8(&bytes) {
        Ok(_) => bytes.len(),
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        Err(_) => bytes.len(),
    };
    let text = crate::audit::redact_audit_text(&String::from_utf8_lossy(&bytes[..end]));
    Ok(
        json!({"run_id":run.id,"path":run.output_path,"offset":start,"next_offset":start+end,
        "total_bytes":total,"complete":start+end>=total,"text":text}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task() -> ProjectTask {
        serde_json::from_value(json!({"id":"t","title":"test","goal":"g","owner":"codex","reviewers":[],
        "status":"queued","write_scope":[],"summary":"","created_at":"2026-10-03T01:00:00Z","updated_at":"2026-10-03T02:00:00Z"})).unwrap()
    }
    fn run() -> AgentRun {
        serde_json::from_value(json!({"id":"r","task_id":"t","agent_id":"codex","status":"completed","pid":null,
        "started_at":"2026-10-03T01:00:00Z","finished_at":"2026-10-03T01:10:00Z","prompt_path":"","output_path":"missing.md","log_path":"","error_path":"","error_message":null})).unwrap()
    }
    #[test]
    fn queued_retry_does_not_inherit_old_handoff() {
        let t = task();
        let rs = vec![run()];
        assert!(current_run(&rs, &t).is_none());
        assert_eq!(phase(&t, None, None), "queued");
    }
    #[test]
    fn local_contract_is_not_web_review() {
        let mut t = task();
        t.status = "completed".into();
        t.reviewed_by = "local-finalizer".into();
        assert_eq!(phase(&t, None, None), "contract_checked");
        t.web_reviewed = true;
        assert_eq!(phase(&t, None, None), "reviewed");
    }
    #[test]
    fn receipt_is_not_review_until_reviewer_explicitly_starts() {
        let root = std::env::temp_dir().join(format!(
            "td-receipt-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        fs::create_dir_all(&root).unwrap();
        let mut r = run();
        r.output_path = root.join("HANDOFF.md").to_string_lossy().into_owned();
        fs::write(&r.output_path, "Worker result").unwrap();
        let mut t = task();
        t.status = "review".into();
        assert_eq!(phase(&t, Some(&r), None), "received");
        assert_eq!(
            phase(&t, Some(&r), Some(&json!({"status":"reviewing"}))),
            "reviewing"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stale_review_attempt_is_rejected() {
        assert!(
            latest_review_target(&[task()], &[run()], &json!({"task_id":"t","run_id":"old"}))
                .is_err()
        );
    }
    #[test]
    fn completed_run_without_evidence_is_not_received() {
        let mut t = task();
        t.status = "review".into();
        assert_eq!(phase(&t, Some(&run()), None), "missing_evidence");
    }
}
