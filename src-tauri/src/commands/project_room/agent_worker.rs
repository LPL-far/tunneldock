use super::*;

pub(super) fn next_id(prefix: &str) -> String {
    format!(
        "{}-{}",
        prefix,
        chrono::Local::now()
            .timestamp_nanos_opt()
            .unwrap_or_default()
    )
}

pub(super) fn normalize_scope(scope: &str) -> String {
    scope
        .trim()
        .replace('\\', "/")
        .trim_end_matches("/**")
        .trim_end_matches("/*")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

pub(super) fn scopes_conflict(left: &[String], right: &[String]) -> bool {
    if left.is_empty() || right.is_empty() {
        return true;
    }

    left.iter().any(|a| {
        let a = normalize_scope(a);
        right.iter().any(|b| {
            let b = normalize_scope(b);
            a.is_empty()
                || b.is_empty()
                || a == b
                || a.starts_with(&(b.clone() + "/"))
                || b.starts_with(&(a.clone() + "/"))
        })
    })
}

const COMPLETION_PREFIX: &str = "TUNNELDOCK_COMPLETION:";

#[derive(Debug, Clone, serde::Deserialize, Serialize, PartialEq)]
struct CompletionManifest {
    outcome: String,
    verification: String,
    #[serde(default)]
    files_changed: bool,
    #[serde(default)]
    evidence_paths: Vec<String>,
    risks: String,
    review: String,
}

#[derive(Debug, Clone, PartialEq)]
enum LocalFinalizerDecision {
    Accept,
    Block(String),
    Defer(String),
}

fn parse_completion_manifest(text: &str) -> Result<CompletionManifest, String> {
    let line = text
        .lines()
        .rev()
        .find_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix(COMPLETION_PREFIX)
                .map(str::trim)
                .filter(|payload| !payload.is_empty())
        })
        .ok_or_else(|| "missing TUNNELDOCK_COMPLETION footer".to_string())?;
    serde_json::from_str::<CompletionManifest>(line)
        .map_err(|error| format!("invalid TUNNELDOCK_COMPLETION JSON: {error}"))
}

fn task_requires_web_judgment(task: &ProjectTask, manifest: &CompletionManifest) -> Option<String> {
    let research_text = format!("{} {}", task.title, task.goal).to_ascii_lowercase();
    for keyword in [
        "architecture",
        "algorithm",
        "method",
        "loss",
        "objective",
        "ablation",
        "novelty",
        "paper",
        "hypothesis",
        "scientific conclusion",
        "research direction",
    ] {
        if research_text.contains(keyword) {
            return Some(format!(
                "research-sensitive keyword '{}' requires Web review",
                keyword
            ));
        }
    }

    if task.owner == "gemini" && manifest.files_changed {
        let visual_only = !task.write_scope.is_empty()
            && task.write_scope.iter().all(|scope| {
                let scope = scope.to_ascii_lowercase();
                [
                    "visual", "figure", "plot", "pca", "video", "ui", "asset", "docs", "report",
                ]
                .iter()
                .any(|safe| scope.contains(safe))
            });
        if !visual_only {
            return Some("Gemini non-visual code change requires Codex/Web review".to_string());
        }
    }
    None
}

fn local_finalizer_decision(
    task: &ProjectTask,
    manifest: &CompletionManifest,
) -> LocalFinalizerDecision {
    let outcome = manifest.outcome.trim().to_ascii_uppercase();
    let verification = manifest.verification.trim().to_ascii_uppercase();
    let risks = manifest.risks.trim().to_ascii_uppercase();
    let review = manifest.review.trim().to_ascii_uppercase();

    if outcome == "BLOCKED" || verification == "FAIL" {
        return LocalFinalizerDecision::Block(format!(
            "worker outcome={}, verification={}",
            outcome, verification
        ));
    }
    if outcome != "DONE" {
        return LocalFinalizerDecision::Defer(format!(
            "unsupported outcome {}; Web review required",
            outcome
        ));
    }
    if task.review_mode != "auto" {
        return LocalFinalizerDecision::Defer("task review_mode=web".to_string());
    }
    if let Some(reason) = task_requires_web_judgment(task, manifest) {
        return LocalFinalizerDecision::Defer(reason);
    }
    if verification != "PASS" {
        return LocalFinalizerDecision::Defer(format!("verification={} is not PASS", verification));
    }
    if risks != "NONE" {
        return LocalFinalizerDecision::Defer(format!("risks={} requires Web judgment", risks));
    }
    if review != "MECHANICAL" {
        return LocalFinalizerDecision::Defer(format!("review={} requires Web judgment", review));
    }
    if !manifest.files_changed && manifest.evidence_paths.is_empty() {
        return LocalFinalizerDecision::Defer(
            "no changed files or evidence paths to audit".to_string(),
        );
    }
    LocalFinalizerDecision::Accept
}

fn write_finalizer_record(
    run: &AgentRun,
    task: &ProjectTask,
    manifest: Option<&CompletionManifest>,
    decision: &str,
    reason: &str,
) {
    let Some(parent) = Path::new(&run.output_path).parent() else {
        return;
    };
    let record = serde_json::json!({
        "run_id": run.id,
        "task_id": task.id,
        "review_mode": task.review_mode,
        "decision": decision,
        "reason": reason,
        "manifest": manifest,
        "updated_at": local_now_rfc3339(),
    });
    let _ = write_json(&parent.join("FINALIZER.json"), &record);
}

fn apply_local_finalizer(
    task: &mut ProjectTask,
    run: &AgentRun,
    handoff: &str,
    discussion: &mut Vec<ProjectDiscussionMessage>,
) -> bool {
    if task.kind == "consultation" || task.status != "review" {
        return false;
    }
    let manifest = match parse_completion_manifest(handoff) {
        Ok(manifest) => manifest,
        Err(reason) => {
            if task.review_mode == "auto" {
                write_finalizer_record(run, task, None, "defer_web", &reason);
            }
            return false;
        }
    };
    let decision = local_finalizer_decision(task, &manifest);
    let thread_id = if task.thread_id.is_empty() {
        task.id.clone()
    } else {
        task.thread_id.clone()
    };
    match decision {
        LocalFinalizerDecision::Accept => {
            task.status = "completed".to_string();
            task.auto_dispatch = false;
            task.reviewed_by = "local-finalizer".to_string();
            task.updated_at = local_now_rfc3339();
            let reason = "strict completion contract passed: DONE + PASS + MECHANICAL + NONE";
            write_finalizer_record(run, task, Some(&manifest), "accept", reason);
            discussion.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id,
                    author: "local-finalizer".to_string(),
                    recipients: vec!["chatgpt".to_string()],
                    message: format!("Auto-finalized {}: {}", task.id, reason),
                    created_at: local_now_rfc3339(),
                },
            );
            true
        }
        LocalFinalizerDecision::Block(reason) => {
            task.status = "blocked".to_string();
            task.auto_dispatch = false;
            task.reviewed_by = "local-finalizer".to_string();
            task.updated_at = local_now_rfc3339();
            write_finalizer_record(run, task, Some(&manifest), "block", &reason);
            discussion.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id,
                    author: "local-finalizer".to_string(),
                    recipients: vec!["chatgpt".to_string()],
                    message: format!("Blocked {} from completion manifest: {}", task.id, reason),
                    created_at: local_now_rfc3339(),
                },
            );
            true
        }
        LocalFinalizerDecision::Defer(reason) => {
            write_finalizer_record(run, task, Some(&manifest), "defer_web", &reason);
            false
        }
    }
}

fn normalize_finalized_consultation_tasks(tasks: &mut [ProjectTask]) -> bool {
    let mut changed = false;
    for task in tasks {
        if task.kind != "consultation"
            || !matches!(task.status.as_str(), "review" | "blocked" | "failed")
            || !task.web_reviewed
        {
            continue;
        }
        let lifecycle_done = task.finalization_policy == "review_only"
            || (task.memory_committed && task.cleanup_committed);
        if lifecycle_done {
            task.status = "completed".to_string();
            task.auto_dispatch = false;
            if task.reviewed_by.is_empty() {
                task.reviewed_by = "chatgpt".to_string();
            }
            task.updated_at = local_now_rfc3339();
            changed = true;
        }
    }
    changed
}

pub(super) fn run_dir(config: &ProjectRoomConfig, run_id: &str) -> PathBuf {
    PathBuf::from(&config.local_root)
        .join(BRIDGE_DIR)
        .join(RUNS_DIR)
        .join(run_id)
}

pub(super) fn task_prompt(
    snapshot: &ProjectRoomSnapshot,
    task: &ProjectTask,
    agent_id: &str,
    output_path: &Path,
) -> String {
    let role = snapshot
        .agents
        .iter()
        .find(|agent| agent.agent_id == agent_id)
        .map(|agent| agent.role.as_str())
        .unwrap_or("Research worker");
    let reviewers = if task.reviewers.is_empty() {
        "ChatGPT".to_string()
    } else {
        task.reviewers.join(", ")
    };
    let is_consultation = task.kind == "consultation";
    let write_scope = if is_consultation {
        "This is a read-only consultation. Do not modify project files, run destructive commands, or create artifacts unless the question explicitly requires a tiny diagnostic output."
            .to_string()
    } else if task.write_scope.is_empty() {
        "No narrow write scope was declared. Minimize edits and do not touch unrelated files."
            .to_string()
    } else {
        format!(
            "Only modify these declared scopes: {}",
            task.write_scope.join(", ")
        )
    };
    let handoff_delivery = if agent_id == "gemini" {
        format!(
            "TunnelDock captures your final Cascade response and persists it to: {}",
            output_path.display()
        )
    } else {
        format!(
            "TunnelDock will persist your final response to: {}",
            output_path.display()
        )
    };
    let retry_context =
        if !is_consultation && task.status == "queued" && !task.summary.trim().is_empty() {
            format!(
                "\nPrior review feedback / retry context:\n{}\n",
                task.summary.chars().take(2_000).collect::<String>()
            )
        } else {
            String::new()
        };
    let response_rule = if is_consultation {
        "This is a consultation round. Keep the final response under 1200 characters and write the final handoff in plain English UTF-8. Give only: Facts, Interpretation/Disagreement, Recommendation, and the single most important uncertainty. Do not repeat logs or long code excerpts. The web coordinator will localize the final synthesis for the user."
    } else {
        "This is an execution task. Do not hand off merely because code was edited. Finish the requested scope, run the smallest sufficient verification, and clean the touched scope first. The final handoff must be compact and evidence-oriented. If completion is blocked, say BLOCKED and name the exact dependency; never present partial work as done."
    };
    let workspace_rule = if agent_id == "codex" {
        "This run executes on a TunnelDock-managed Codex automation thread forked from the project's human/canonical Codex conversation. Continue using the workspace inherited from that human thread for code edits. The Project Room local root below is the coordination/memory root; do not migrate or duplicate the existing code workspace into it."
    } else {
        "Use the Project Room local root as the local project workspace."
    };

    format!(
        r#"You are {agent} working inside TunnelDock Project Room "{project}".

Role: {role}
Task ID: {task_id}
Task: {title}
Review mode: {review_mode}
Goal / completion criteria:
{goal}
{retry_context}
Project boundaries:
- Project Room local root: {local}
- {workspace_rule}
- Remote workspace: {remote_host}:{remote_root}
- Compact task state: {local}\.tunneldock\web_status.json
- Versioned context index (when ready): {local}\.tunneldock\context\current.json
- Full Project Room snapshot (on demand): {local}\.tunneldock\project_room.json
- Constitution: {local}\.tunneldock\CONSTITUTION.md
- Research reasoning / readable evidence: {local}\.tunneldock\RESEARCH_PROTOCOL.md
- Canonical memory: {local}\.project_memory\PROJECT_STATE.md and SESSION_HANDOFF.md
- {write_scope}

Research rules:
- This is top-conference research. Prefer the simplest correct implementation that tests the hypothesis.
- Do not create V1/V2/V3, *_old, *_backup, *_new, or *_final copies.
- Do not leave temporary scripts/logs after their conclusion is captured.
- Preserve reproducibility evidence and meaningful negative results.
- Never import assumptions from another Project Room.
- Reviewer(s): {reviewers}

Before editing/reviewing, start with web_status.json, Constitution, MEMORY_INDEX.md, and the project instructions (AGENTS.md/README). When .tunneldock/context/status.json is ready, read .tunneldock/context/current.json and its hot working-set path for orientation; use the scoped tunneldock_context search/read/impact tool when available. This incomplete generated view is NOT canonical truth. Read the current original constraints in PROJECT_STATE.md/SESSION_HANDOFF.md and the relevant domain/source sections before changing behavior; fully inspect decisive evidence and all required handoffs before accepting conclusions. If the index is missing, stale or blocked, use the original files directly. Read full project_room.json only when compact state omits needed task details. Do not batch-read unrelated historical memory or infer an absent constraint from a compressed excerpt.
If a canonical memory file starts with TunnelDock required-continuation, read its exact archived original and required recursive manifest. Automatic pagination never removes constraints or retires negative results. Use the local context index to locate archived evidence, then expand decisive sources. A shorter root file is not the whole memory.
If the task is underspecified or conflicts with current evidence, state the conflict instead of inventing a design.
After implementation, run the smallest sufficient correctness checks.
Explain the decisive change with short complete sentences and stable terminology. Preserve exact conditions, numbers, paths, errors and uncertainty; do not claim certified ASD-STE100 compliance. For a research decision only, consult RESEARCH_PROTOCOL.md and include a compact Research reasoning section within the existing response budget: observation, hypothesis, alternatives, decisive test/falsifier, evidence/limits, next decision. Use UNKNOWN/NOT_TESTED for missing evidence. Do not run unrelated experiments to fill this outline, manufacture causal/novelty claims from score gains, or change the accepted method because of an article. Diagrams/HTML/videos are on demand, never a substitute for reviewed evidence.
{response_rule}
Your final response must be a concise handoff with:
1. Outcome: DONE or BLOCKED
2. Summary: what is now actually complete
3. Files changed: exact paths, or "none"
4. Verification: exact tests/commands run and pass/fail result; if none, explain why
5. Evidence/artifacts: exact result/log/figure paths needed for review
6. Cleanup: obsolete/scratch/log files removed, consolidated, or intentionally retained
7. Remaining risks/blockers
8. Requested review: the smallest decisive thing the reviewer must check
9. Memory/experiment updates that should be made

For non-consultation execution tasks, append ONE final machine-readable line exactly in this form:
TUNNELDOCK_COMPLETION: {{"outcome":"DONE","verification":"PASS","files_changed":true,"evidence_paths":["path/to/evidence"],"risks":"NONE","review":"MECHANICAL"}}
Contract rules:
- outcome=BLOCKED unless the requested scope is actually complete.
- verification=PASS only when the stated current-tree checks really passed; use FAIL or NOT_RUN otherwise.
- files_changed=true only for real persisted edits; evidence_paths must list real reviewable artifacts/results/logs and may be empty only when files_changed=true.
- risks=NONE only when no unresolved correctness/blocker remains. If experiment interpretation, novelty, method choice, scientific acceptance, or another human judgment remains, use PRESENT.
- review=MECHANICAL only when the remaining review is deterministic correctness/evidence checking. Use WEB for method/experiment interpretation, acceptance of a scientific conclusion, or any consequential judgment.
- Never claim MECHANICAL merely to get auto-finalized.

{handoff_delivery}
"#,
        agent = agent_id,
        project = snapshot.config.name,
        role = role,
        task_id = task.id,
        review_mode = task.review_mode,
        title = task.title,
        goal = task.goal,
        retry_context = retry_context,
        local = snapshot.config.local_root,
        remote_host = snapshot.config.remote.host,
        remote_root = snapshot.config.remote.root,
        write_scope = write_scope,
        reviewers = reviewers,
        handoff_delivery = handoff_delivery,
        response_rule = response_rule,
        workspace_rule = workspace_rule,
    )
}

pub(super) fn append_system_message(
    dir: &Path,
    author: &str,
    message: String,
) -> Result<ProjectDiscussionMessage, String> {
    let path = dir.join(DISCUSSION_FILE);
    let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&path)?;
    let item = ProjectDiscussionMessage {
        id: next_id("MSG"),
        thread_id: "agent-runs".to_string(),
        author: author.to_string(),
        recipients: vec!["all".to_string()],
        message,
        created_at: local_now_rfc3339(),
    };
    messages.insert(0, item.clone());
    if messages.len() > 5_000 {
        messages.truncate(5_000);
    }
    write_json(&path, &messages)?;
    Ok(item)
}

fn should_poll_codex_run(run: &AgentRun, output_exists: bool) -> bool {
    run.agent_id == "codex" && !output_exists
}

pub(super) fn refresh_run_states_unlocked(
    state: &AppState,
    project_id: &str,
) -> Result<bool, String> {
    let dir = project_dir(state, project_id)?;
    let runs_path = dir.join(RUNS_FILE);
    let tasks_path = dir.join(TASKS_FILE);
    let discussion_path = dir.join(DISCUSSION_FILE);
    let mut runs = read_json::<Vec<AgentRun>>(&runs_path)?;
    let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
    let mut discussion = read_json::<Vec<ProjectDiscussionMessage>>(&discussion_path)?;
    let mut changed = false;

    for run in &mut runs {
        if !matches!(run.status.as_str(), "running" | "interactive") {
            continue;
        }

        let output_path = PathBuf::from(&run.output_path);
        if should_poll_codex_run(run, output_path.exists()) {
            let Some(thread_id) = run.external_session_id.as_deref() else {
                run.status = "failed".to_string();
                run.finished_at = Some(local_now_rfc3339());
                run.error_message = Some("Codex run 缺少 external_session_id".to_string());
                changed = true;
                continue;
            };
            let start_offset = run.start_step.unwrap_or_default();
            let prompt = match fs::read_to_string(&run.prompt_path) {
                Ok(prompt) => prompt,
                Err(error) => {
                    run.status = "failed".to_string();
                    run.finished_at = Some(local_now_rfc3339());
                    run.error_message = Some(format!("读取 Codex run prompt 失败: {error}"));
                    changed = true;
                    continue;
                }
            };
            match codex::poll_run(thread_id, start_offset, &prompt, &output_path) {
                Ok(codex::CodexRunUpdate::Running) => {
                    if run.pid.map(is_process_running) == Some(false) {
                        let error = "Codex background app-server exited before the turn produced a handoff.".to_string();
                        run.status = "failed".to_string();
                        run.finished_at = Some(local_now_rfc3339());
                        run.error_message = Some(error.clone());
                        state.running_agent_pids.lock().remove(&run.id);
                        codex::cleanup_worker_artifacts(&run.prompt_path);
                        let _ = record_transport_failure(
                            state,
                            project_id,
                            "codex",
                            Some(&run.id),
                            &error,
                            "background_process_exit",
                        );
                        if let Some(task) = tasks.iter_mut().find(|task| task.id == run.task_id) {
                            task.status = "blocked".to_string();
                            task.summary = error;
                            task.updated_at = local_now_rfc3339();
                        }
                        changed = true;
                    }
                    continue;
                }
                Ok(codex::CodexRunUpdate::Completed(text)) => {
                    run.error_message = None;
                    let _ = record_transport_success(
                        state,
                        project_id,
                        "codex",
                        Some(&run.id),
                        "project_run",
                    );
                    fs::write(&output_path, text).map_err(|error| {
                        format!("写入 {} 失败: {}", output_path.display(), error)
                    })?;
                }
                Ok(codex::CodexRunUpdate::Failed(error)) => {
                    if let Some(pid) = run.pid {
                        let _ = kill_process_tree(pid);
                    }
                    state.running_agent_pids.lock().remove(&run.id);
                    codex::cleanup_worker_artifacts(&run.prompt_path);
                    let _ = record_transport_failure(
                        state,
                        project_id,
                        "codex",
                        Some(&run.id),
                        &error,
                        "poll_terminal_failure",
                    );
                    run.pid = None;
                    run.status = "failed".to_string();
                    run.finished_at = Some(local_now_rfc3339());
                    run.error_message = Some(error.clone());
                    if let Some(task) = tasks.iter_mut().find(|task| task.id == run.task_id) {
                        task.status = "blocked".to_string();
                        task.summary = error.chars().take(2_000).collect();
                        task.updated_at = local_now_rfc3339();
                    }
                    discussion.insert(
                        0,
                        ProjectDiscussionMessage {
                            id: next_id("MSG"),
                            thread_id: tasks
                                .iter()
                                .find(|task| task.id == run.task_id)
                                .map(|task| {
                                    if task.thread_id.is_empty() {
                                        task.id.clone()
                                    } else {
                                        task.thread_id.clone()
                                    }
                                })
                                .unwrap_or_else(|| "agent-runs".to_string()),
                            author: "system".to_string(),
                            recipients: vec!["chatgpt".to_string()],
                            message: format!(
                                "codex failed: {}",
                                error.chars().take(1_500).collect::<String>()
                            ),
                            created_at: local_now_rfc3339(),
                        },
                    );
                    changed = true;
                    continue;
                }
                Err(error) => {
                    run.error_message = Some(format!(
                        "Codex thread poll temporarily unavailable: {}",
                        error.chars().take(1_000).collect::<String>()
                    ));
                    changed = true;
                    continue;
                }
            }
        }

        if run.agent_id == "gemini" && !output_path.exists() {
            let Some(cascade_id) = run.external_session_id.as_deref() else {
                run.status = "failed".to_string();
                run.finished_at = Some(local_now_rfc3339());
                run.error_message = Some("Gemini run 缺少 external_session_id".to_string());
                changed = true;
                continue;
            };
            let start_step = run.start_step.unwrap_or_default();
            match antigravity::poll_run(cascade_id, start_step, &output_path) {
                Ok(antigravity::CascadeRunUpdate::Running) => continue,
                Ok(antigravity::CascadeRunUpdate::Completed(text)) => {
                    run.error_message = None;
                    fs::write(&output_path, text).map_err(|error| {
                        format!("写入 {} 失败: {}", output_path.display(), error)
                    })?;
                }
                Ok(antigravity::CascadeRunUpdate::Failed(error)) => {
                    run.status = "failed".to_string();
                    run.finished_at = Some(local_now_rfc3339());
                    run.error_message = Some(error.clone());
                    if let Some(task) = tasks.iter_mut().find(|task| task.id == run.task_id) {
                        task.status = "blocked".to_string();
                        task.summary = error.chars().take(2_000).collect();
                        task.updated_at = local_now_rfc3339();
                    }
                    discussion.insert(
                        0,
                        ProjectDiscussionMessage {
                            id: next_id("MSG"),
                            thread_id: tasks
                                .iter()
                                .find(|task| task.id == run.task_id)
                                .map(|task| {
                                    if task.thread_id.is_empty() {
                                        task.id.clone()
                                    } else {
                                        task.thread_id.clone()
                                    }
                                })
                                .unwrap_or_else(|| "agent-runs".to_string()),
                            author: "system".to_string(),
                            recipients: vec!["chatgpt".to_string()],
                            message: format!(
                                "{} failed: {}",
                                run.agent_id,
                                error.chars().take(1_500).collect::<String>()
                            ),
                            created_at: local_now_rfc3339(),
                        },
                    );
                    changed = true;
                    continue;
                }
                Err(error) => {
                    // The local Agent Manager may briefly restart or rotate RPC
                    // ports. Keep the run alive and retry on the next supervisor
                    // cycle instead of turning a transport hiccup into task failure.
                    run.error_message = Some(format!(
                        "Antigravity poll temporarily unavailable: {}",
                        error.chars().take(1_000).collect::<String>()
                    ));
                    changed = true;
                    continue;
                }
            }
        }

        let output = fs::read_to_string(&output_path)
            .ok()
            .filter(|text| !text.trim().is_empty());
        let process_alive = run
            .pid
            .map(is_process_running)
            .unwrap_or(run.agent_id == "gemini");
        let completed = output.is_some();
        let terminal_without_output = run.status == "running" && !process_alive && output.is_none();

        if completed || terminal_without_output {
            run.finished_at = Some(local_now_rfc3339());
            if run.agent_id == "codex" {
                if let Some(pid) = run.pid {
                    let _ = kill_process_tree(pid);
                }
                codex::cleanup_worker_artifacts(&run.prompt_path);
            }
            run.pid = None;
            state.running_agent_pids.lock().remove(&run.id);

            if let Some(text) = output {
                run.status = "completed".to_string();
                let consultation = tasks
                    .iter()
                    .find(|task| task.id == run.task_id)
                    .map(|task| task.kind == "consultation")
                    .unwrap_or(false);
                let limit = if consultation { 1_600 } else { 12_000 };
                let handoff = text.chars().take(limit).collect::<String>();
                if let Some(task) = tasks.iter_mut().find(|task| task.id == run.task_id) {
                    task.status = "review".to_string();
                    task.reviewed_by.clear();
                    task.summary = handoff.clone();
                    task.updated_at = local_now_rfc3339();
                    discussion.insert(
                        0,
                        ProjectDiscussionMessage {
                            id: next_id("MSG"),
                            thread_id: if task.thread_id.is_empty() {
                                task.id.clone()
                            } else {
                                task.thread_id.clone()
                            },
                            author: run.agent_id.clone(),
                            recipients: if task.reviewers.is_empty() {
                                vec!["chatgpt".to_string()]
                            } else {
                                task.reviewers.clone()
                            },
                            message: handoff.clone(),
                            created_at: local_now_rfc3339(),
                        },
                    );
                    let _ = apply_local_finalizer(task, run, &text, &mut discussion);
                }
            } else {
                run.status = "failed".to_string();
                let error_text = fs::read_to_string(&run.error_path)
                    .unwrap_or_else(|_| "Worker exited without a handoff.".to_string());
                let error = if run.agent_id == "codex" {
                    format!(
                        "Codex background app-server exited without a handoff. stderr: {}",
                        error_text.chars().take(3_000).collect::<String>()
                    )
                } else {
                    error_text.chars().take(4_000).collect::<String>()
                };
                run.error_message = Some(error.clone());
                if run.agent_id == "codex" {
                    let _ = record_transport_failure(
                        state,
                        project_id,
                        "codex",
                        Some(&run.id),
                        &error,
                        "terminal_without_handoff",
                    );
                }
                if let Some(task) = tasks.iter_mut().find(|task| task.id == run.task_id) {
                    task.status = "blocked".to_string();
                    task.summary = error.clone();
                    task.updated_at = local_now_rfc3339();
                }
            }
            changed = true;
        }
    }

    if normalize_finalized_consultation_tasks(&mut tasks) {
        changed = true;
    }

    if changed {
        if discussion.len() > 5_000 {
            discussion.truncate(5_000);
        }
        write_json(&runs_path, &runs)?;
        write_json(&tasks_path, &tasks)?;
        write_json(&discussion_path, &discussion)?;
    }

    Ok(changed)
}

pub(super) fn ensure_dispatch_scope_is_safe(
    snapshot: &ProjectRoomSnapshot,
    task: &ProjectTask,
) -> Result<(), String> {
    if task.kind == "consultation" {
        return Ok(());
    }

    for other in &snapshot.tasks {
        if other.id == task.id || !matches!(other.status.as_str(), "active" | "review") {
            continue;
        }

        if scopes_conflict(&task.write_scope, &other.write_scope) {
            return Err(format!(
                "Task {} 与正在进行/待 review 的 {} 写入范围可能冲突；请先完成 review 或明确拆分 write_scope。",
                task.id, other.id
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        apply_local_finalizer, local_finalizer_decision, normalize_finalized_consultation_tasks,
        parse_completion_manifest, should_poll_codex_run, CompletionManifest,
        LocalFinalizerDecision,
    };
    use crate::models::{AgentRun, ProjectDiscussionMessage, ProjectTask};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn run(agent_id: &str, pid: Option<u32>) -> AgentRun {
        AgentRun {
            id: "RUN-1".to_string(),
            task_id: "TASK-1".to_string(),
            agent_id: agent_id.to_string(),
            status: "running".to_string(),
            pid,
            external_session_id: Some("thread-1".to_string()),
            start_step: Some(10),
            started_at: "2026-10-02T12:00:00+08:00".to_string(),
            finished_at: None,
            prompt_path: "prompt.md".to_string(),
            output_path: "HANDOFF.md".to_string(),
            log_path: "stdout.log".to_string(),
            error_path: "stderr.log".to_string(),
            error_message: None,
        }
    }

    fn task(review_mode: &str) -> ProjectTask {
        ProjectTask {
            id: "TASK-1".to_string(),
            title: "Mechanical implementation".to_string(),
            goal: "Implement and verify".to_string(),
            owner: "codex".to_string(),
            reviewers: vec!["chatgpt".to_string()],
            status: "review".to_string(),
            write_scope: vec!["src/**".to_string()],
            summary: String::new(),
            kind: "work".to_string(),
            thread_id: String::new(),
            consultation_id: String::new(),
            web_reviewed: false,
            memory_committed: false,
            cleanup_committed: false,
            auto_dispatch: true,
            finalization_policy: "work".to_string(),
            review_mode: review_mode.to_string(),
            reviewed_by: String::new(),
            created_at: "2026-10-03T10:00:00+08:00".to_string(),
            updated_at: "2026-10-03T10:00:00+08:00".to_string(),
        }
    }

    #[test]
    fn background_codex_run_with_pid_is_still_polled() {
        let background = run("codex", Some(4242));
        assert!(should_poll_codex_run(&background, false));
        assert!(!should_poll_codex_run(&background, true));

        let legacy_queue = run("codex", None);
        assert!(should_poll_codex_run(&legacy_queue, false));

        let gemini = run("gemini", Some(4242));
        assert!(!should_poll_codex_run(&gemini, false));
    }

    #[test]
    fn strict_completion_footer_parses_and_auto_accepts_mechanical_work() {
        let text = r#"handoff
TUNNELDOCK_COMPLETION: {"outcome":"DONE","verification":"PASS","files_changed":true,"evidence_paths":["tests/report.json"],"risks":"NONE","review":"MECHANICAL"}"#;
        let manifest = parse_completion_manifest(text).expect("valid completion footer");
        assert_eq!(manifest.outcome, "DONE");
        assert_eq!(
            local_finalizer_decision(&task("auto"), &manifest),
            LocalFinalizerDecision::Accept
        );
    }

    #[test]
    fn finalizer_defers_scientific_or_web_review() {
        let manifest = CompletionManifest {
            outcome: "DONE".to_string(),
            verification: "PASS".to_string(),
            files_changed: true,
            evidence_paths: vec!["result.json".to_string()],
            risks: "PRESENT".to_string(),
            review: "WEB".to_string(),
        };
        assert!(matches!(
            local_finalizer_decision(&task("auto"), &manifest),
            LocalFinalizerDecision::Defer(_)
        ));
        let safe = CompletionManifest {
            risks: "NONE".to_string(),
            review: "MECHANICAL".to_string(),
            ..manifest
        };
        assert!(matches!(
            local_finalizer_decision(&task("web"), &safe),
            LocalFinalizerDecision::Defer(_)
        ));
    }

    #[test]
    fn local_finalizer_writes_audit_record_and_completes_task() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tunneldock-finalizer-{nonce}"));
        fs::create_dir_all(&root).expect("temp dir");
        let output = root.join("HANDOFF.md");
        let text = r#"1. Outcome: DONE
4. Verification: cargo test passed
TUNNELDOCK_COMPLETION: {"outcome":"DONE","verification":"PASS","files_changed":true,"evidence_paths":["tests/report.json"],"risks":"NONE","review":"MECHANICAL"}"#;
        fs::write(&output, text).expect("handoff");

        let mut work = task("auto");
        let mut completed_run = run("codex", None);
        completed_run.status = "completed".to_string();
        completed_run.output_path = output.to_string_lossy().to_string();
        let mut discussion = Vec::<ProjectDiscussionMessage>::new();
        assert!(apply_local_finalizer(
            &mut work,
            &completed_run,
            text,
            &mut discussion
        ));
        assert_eq!(work.status, "completed");
        assert_eq!(work.reviewed_by, "local-finalizer");
        assert_eq!(discussion[0].author, "local-finalizer");
        let audit: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root.join("FINALIZER.json")).expect("finalizer record"),
        )
        .expect("valid finalizer json");
        assert_eq!(audit["decision"], "accept");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn research_sensitive_task_is_never_auto_accepted() {
        let manifest = CompletionManifest {
            outcome: "DONE".to_string(),
            verification: "PASS".to_string(),
            files_changed: true,
            evidence_paths: vec!["evidence.json".to_string()],
            risks: "NONE".to_string(),
            review: "MECHANICAL".to_string(),
        };
        let mut research = task("auto");
        research.title = "Implement new architecture operator".to_string();
        assert!(matches!(
            local_finalizer_decision(&research, &manifest),
            LocalFinalizerDecision::Defer(_)
        ));
    }

    #[test]
    fn finalizer_blocks_reported_blocker_or_failed_verification() {
        let blocked = CompletionManifest {
            outcome: "BLOCKED".to_string(),
            verification: "NOT_RUN".to_string(),
            files_changed: false,
            evidence_paths: Vec::new(),
            risks: "PRESENT".to_string(),
            review: "WEB".to_string(),
        };
        assert!(matches!(
            local_finalizer_decision(&task("auto"), &blocked),
            LocalFinalizerDecision::Block(_)
        ));
        let failed = CompletionManifest {
            outcome: "DONE".to_string(),
            verification: "FAIL".to_string(),
            files_changed: true,
            evidence_paths: Vec::new(),
            risks: "NONE".to_string(),
            review: "MECHANICAL".to_string(),
        };
        assert!(matches!(
            local_finalizer_decision(&task("auto"), &failed),
            LocalFinalizerDecision::Block(_)
        ));
    }

    #[test]
    fn stale_finalized_consultation_no_longer_holds_review_lock() {
        let mut consultation = task("web");
        consultation.kind = "consultation".to_string();
        consultation.status = "review".to_string();
        consultation.web_reviewed = true;
        consultation.finalization_policy = "durable".to_string();
        consultation.memory_committed = true;
        consultation.cleanup_committed = true;
        let mut tasks = vec![consultation];
        assert!(normalize_finalized_consultation_tasks(&mut tasks));
        assert_eq!(tasks[0].status, "completed");
        assert_eq!(tasks[0].reviewed_by, "chatgpt");
    }
}
