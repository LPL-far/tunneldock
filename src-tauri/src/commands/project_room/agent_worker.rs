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
    let response_rule = if is_consultation {
        "This is a consultation round. Keep the final response under 1200 characters and write the final handoff in plain English UTF-8. Give only: Facts, Interpretation/Disagreement, Recommendation, and the single most important uncertainty. Do not repeat logs or long code excerpts. The web coordinator will localize the final synthesis for the user."
    } else {
        "Keep the final handoff concise. Before handoff, clean the scope you touched: remove superseded code/scripts, disposable scratch and redundant logs; merge stale/current documentation into one authoritative document; retain a log only when it contains unique reproducibility/debug evidence and point to that evidence instead of pasting it. Do not paste raw logs or large code excerpts; point to files/artifacts and mention what was removed/consolidated/retained."
    };
    let workspace_rule = if agent_id == "codex" {
        "Continue using the workspace already bound to this Codex Desktop thread for code edits. The Project Room local root below is the coordination/memory root; do not migrate or duplicate the existing code workspace into it."
    } else {
        "Use the Project Room local root as the local project workspace."
    };

    format!(
        r#"You are {agent} working inside TunnelDock Project Room "{project}".

Role: {role}
Task ID: {task_id}
Task: {title}
Goal / completion criteria:
{goal}

Project boundaries:
- Project Room local root: {local}
- {workspace_rule}
- Remote workspace: {remote_host}:{remote_root}
- Project Room snapshot: {local}\.tunneldock\project_room.json
- Constitution: {local}\.tunneldock\CONSTITUTION.md
- Canonical memory: {local}\.project_memory\PROJECT_STATE.md and SESSION_HANDOFF.md
- {write_scope}

Research rules:
- This is top-conference research. Prefer the simplest correct implementation that tests the hypothesis.
- Do not create V1/V2/V3, *_old, *_backup, *_new, or *_final copies.
- Do not leave temporary scripts/logs after their conclusion is captured.
- Preserve reproducibility evidence and meaningful negative results.
- Never import assumptions from another Project Room.
- Reviewer(s): {reviewers}

Before editing/reviewing, read the Project Room snapshot, Constitution, MEMORY_INDEX.md, PROJECT_STATE.md, SESSION_HANDOFF.md, and the relevant domain memory named by MEMORY_INDEX.md (MODEL_DESIGN / DATA_CATALOG / EXPERIMENTS / RESULTS / REFERENCES / DOCUMENTS / DECISIONS), plus project instructions (AGENTS.md/README) needed by the task.
If the task is underspecified or conflicts with current evidence, state the conflict instead of inventing a design.
After implementation, run the smallest sufficient correctness checks.
{response_rule}
Your final response must be a concise handoff with:
1. Summary
2. Files changed
3. Tests / experiment evidence
4. Remaining risks
5. Questions or requested review
6. Memory/experiment updates that should be made

{handoff_delivery}
"#,
        agent = agent_id,
        project = snapshot.config.name,
        role = role,
        task_id = task.id,
        title = task.title,
        goal = task.goal,
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
        if run.agent_id == "codex" && run.pid.is_none() && !output_path.exists() {
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
            match codex::poll_run(thread_id, start_offset, &prompt) {
                Ok(codex::CodexRunUpdate::Running) => continue,
                Ok(codex::CodexRunUpdate::Completed(text)) => {
                    run.error_message = None;
                    fs::write(&output_path, text).map_err(|error| {
                        format!("写入 {} 失败: {}", output_path.display(), error)
                    })?;
                }
                Ok(codex::CodexRunUpdate::Failed(error)) => {
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
            match antigravity::poll_run(cascade_id, start_step) {
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
                }
            } else {
                run.status = "failed".to_string();
                let error_text = fs::read_to_string(&run.error_path)
                    .unwrap_or_else(|_| "Worker exited without a handoff.".to_string());
                let error = error_text.chars().take(4_000).collect::<String>();
                run.error_message = Some(error.clone());
                if let Some(task) = tasks.iter_mut().find(|task| task.id == run.task_id) {
                    task.status = "blocked".to_string();
                    task.summary = error.clone();
                    task.updated_at = local_now_rfc3339();
                }
            }
            changed = true;
        }
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
