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
    let write_scope = if task.write_scope.is_empty() {
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
            "Before finishing, write the same handoff text to this exact file: {}",
            output_path.display()
        )
    } else {
        format!(
            "TunnelDock will persist your final response to: {}",
            output_path.display()
        )
    };

    format!(
        r#"You are {agent} working inside TunnelDock Project Room "{project}".

Role: {role}
Task ID: {task_id}
Task: {title}
Goal / completion criteria:
{goal}

Project boundaries:
- Local root: {local}
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

Before editing, read the Project Room snapshot, Constitution, PROJECT_STATE.md, SESSION_HANDOFF.md, and relevant project instructions (AGENTS.md/README).
If the task is underspecified or conflicts with current evidence, state the conflict instead of inventing a design.
After implementation, run the smallest sufficient correctness checks.
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
    )
}

pub(super) fn build_worker_command(
    executable: &Path,
    args: &[String],
    cwd: &Path,
    stdout: File,
    stderr: File,
) -> Command {
    #[cfg(target_os = "windows")]
    let mut command = {
        let extension = executable
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if matches!(extension.as_str(), "cmd" | "bat") {
            let mut command = Command::new("cmd.exe");
            command.args(["/d", "/s", "/c"]);
            command.arg(executable);
            command.args(args);
            command
        } else {
            let mut command = Command::new(executable);
            command.args(args);
            command
        }
    };

    #[cfg(not(target_os = "windows"))]
    let mut command = {
        let mut command = Command::new(executable);
        command.args(args);
        command
    };

    command.current_dir(cwd).stdout(stdout).stderr(stderr);

    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
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
    let mut runs = read_json::<Vec<AgentRun>>(&runs_path)?;
    let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
    let mut changed = false;

    for run in &mut runs {
        if !matches!(run.status.as_str(), "running" | "interactive") {
            continue;
        }

        let output_path = PathBuf::from(&run.output_path);
        let output = fs::read_to_string(&output_path)
            .ok()
            .filter(|text| !text.trim().is_empty());

        let process_alive = run.pid.map(is_process_running).unwrap_or(false);
        let completed = output.is_some();
        let terminal_without_output = run.status == "running" && !process_alive && output.is_none();

        if completed || terminal_without_output {
            run.finished_at = Some(local_now_rfc3339());
            run.pid = None;
            state.running_agent_pids.lock().remove(&run.id);

            if let Some(text) = output {
                run.status = "completed".to_string();
                let handoff = text.chars().take(12_000).collect::<String>();
                if let Some(task) = tasks.iter_mut().find(|task| task.id == run.task_id) {
                    task.status = "review".to_string();
                    task.summary = handoff.clone();
                    task.updated_at = local_now_rfc3339();
                }
                let _ = append_system_message(
                    &dir,
                    &run.agent_id,
                    format!("Agent handoff for {}:\n\n{}", run.task_id, handoff),
                );
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
                let _ =
                    append_system_message(&dir, "system", format!("{} failed: {}", run.id, error));
            }
            changed = true;
        }
    }

    if changed {
        write_json(&runs_path, &runs)?;
        write_json(&tasks_path, &tasks)?;
    }

    Ok(changed)
}

pub(super) fn ensure_dispatch_scope_is_safe(
    snapshot: &ProjectRoomSnapshot,
    task: &ProjectTask,
) -> Result<(), String> {
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
