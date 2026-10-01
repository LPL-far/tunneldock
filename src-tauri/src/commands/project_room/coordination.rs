use super::*;

pub(super) fn project_constitution() -> &'static str {
    r#"# TunnelDock Research Project Constitution

This project is optimized for top-conference research, not product feature accumulation.

## Research rules
- Every code change, experiment, visualization, and document must serve a research hypothesis, method, ablation, analysis, or reproducibility goal.
- Keep one authoritative current implementation. Do not create V1/V2/V3, *_old, *_backup, *_new, *_final, or parallel stale implementations.
- Respect the project's own Git policy. When Git is permitted, use local Git for history/review/rollback; when it is not, keep one current tree and record source hashes/configs/evidence. Never preserve old code copies just to simulate version control.
- Keep core logic simple, explicit, and testable. Correctness is more important than speed of implementation.
- Remove obsolete files, temporary patches, stale logs, abandoned scripts, and outdated documentation after their replacement is verified.
- Record meaningful experiments, including negative results. Code completion alone does not finish a research task; evidence and interpretation must enter project memory.
- Project memory is a living current-state document. Update it when method decisions, evidence, known problems, or next actions change.

## Default agent roles
- ChatGPT: coordinator, research lead, experiment interpreter, final reviewer, and quota-allocation brain.
- Codex: primary engineer for robust implementation, refactoring, tests, and code correctness.
- Antigravity Agent / Gemini: Agent Manager session for visualization, UI, PCA/video/figure work, rapid exploration, and visual result inspection; no IDE dependency.

## Web coordinator context budget
- ChatGPT Web is the coordination surface, not the raw execution log. Keep normal replies compact: default to at most 6 short bullets or roughly 500 Chinese characters / 350 English words unless the user explicitly asks for a deep dive.
- Never paste full tool output, full worker handoffs, long code excerpts, or complete experiment logs into the web conversation. Put detail in project files/artifacts and cite paths.
- For multi-agent consultation, ChatGPT should report only: consensus, disagreement, decisive evidence, and next action. Preserve full worker evidence in Project Room state instead of repeating it in chat.
- Prefer one focused consultation round over open-ended agent-to-agent chatting. Start another round only when a concrete unresolved question remains.

## Review rules
- Gemini modifications to core model/training/data code require Codex review before acceptance.
- Important Codex algorithm changes require ChatGPT review for research intent and methodological consistency.
- Agents may discuss and challenge each other. Disagreement should be preserved in Project Room discussion until a decision is made.
- The human researcher remains the final decision maker.

## Project isolation
- Do not import task state, memory, or decisions from another Project Room unless the user explicitly requests a cross-project handoff.
"#
}

pub(super) fn inbox_protocol() -> &'static str {
    r#"# TunnelDock Project Room Inbox

Agents collaborate through this project-local inbox. TunnelDock consumes each JSON file once, applies it to the Project Room, updates `project_room.json`, then deletes the processed file.

Write one event per `.json` file under `.tunneldock/inbox/`.

## discussion.post
```json
{
  "kind": "discussion.post",
  "author": "codex",
  "thread_id": "general",
  "recipients": ["chatgpt", "gemini"],
  "message": "Concrete finding, disagreement, question, or review request."
}
```

## consult.request
Use this when ChatGPT wants Codex and/or Gemini to independently inspect the same focused question. TunnelDock creates read-only consultation tasks and auto-dispatches them. Their short handoffs return to one shared discussion thread.
```json
{
  "kind": "consult.request",
  "author": "chatgpt",
  "title": "Check whether the geometry gate is actually needed",
  "question": "Inspect current code and evidence independently. State facts, disagreement with the current interpretation if any, one recommendation, and the main uncertainty.",
  "agents": ["codex", "gemini"],
  "thread_id": "optional-existing-CONSULT-id-for-one-focused-follow-up"
}
```

## task.create
```json
{
  "kind": "task.create",
  "author": "chatgpt",
  "title": "Implement decoder visibility head",
  "goal": "Completion criteria and research reason.",
  "owner": "codex",
  "reviewers": ["chatgpt"],
  "write_scope": ["model/decoder/**"]
}
```

## task.handoff
```json
{
  "kind": "task.handoff",
  "author": "codex",
  "task_id": "TASK-...",
  "status": "review",
  "summary": "What changed, tests/evidence, remaining risks, and requested review."
}
```

## experiment.record
```json
{
  "kind": "experiment.record",
  "author": "chatgpt",
  "hypothesis": "What this run tests.",
  "dataset": "benchmark/split",
  "command": "exact reproducible command",
  "metrics": "measured results only",
  "result": "supported / rejected / inconclusive",
  "analysis": "interpretation separated from measurements",
  "artifacts": ["path/to/result.json"]
}
```

Rules:
- Do not edit `project_room.json` directly; it is generated state.
- Do not use this inbox as a raw chat dump. Post only decisions, disagreements, review requests, handoffs, and reproducible evidence that another agent needs.
- `consult.request` is read-only by default. Keep each worker's final answer under 1200 characters; ChatGPT should synthesize rather than quote both answers back verbatim.
- If the two agents materially disagree, ChatGPT may issue one focused follow-up `consult.request` with the same `thread_id`. Avoid recursive debate unless the user explicitly asks for it.
- Canonical research memory remains under `.project_memory/`; update it directly after meaningful decisions/results.
- Project isolation is strict. Never write an event into another project's inbox unless the user explicitly requests a cross-project handoff.
"#
}

fn clip_text(value: &str, limit: usize) -> String {
    let mut clipped = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        clipped.push_str("…");
    }
    clipped
}

pub(super) fn sync_project_bridge(snapshot: &ProjectRoomSnapshot) -> Result<(), String> {
    let bridge_dir = PathBuf::from(&snapshot.config.local_root).join(BRIDGE_DIR);
    fs::create_dir_all(&bridge_dir)
        .map_err(|error| format!("创建 {} 失败: {}", bridge_dir.display(), error))?;
    fs::create_dir_all(bridge_dir.join(INBOX_DIR))
        .map_err(|error| format!("创建 Project Room inbox 失败: {}", error))?;

    // The bridge is operational coordination state only. Canonical research memory
    // remains exclusively in .project_memory/*.md so agents never mistake a copied
    // JSON snapshot for an authoritative second memory source.
    let memory_dir = project_memory_dir(&snapshot.config);
    let tasks = snapshot
        .tasks
        .iter()
        .take(80)
        .map(|task| {
            serde_json::json!({
                "id": task.id,
                "title": task.title,
                "goal": clip_text(&task.goal, 1_200),
                "owner": task.owner,
                "reviewers": task.reviewers,
                "status": task.status,
                "write_scope": task.write_scope,
                "summary": clip_text(&task.summary, 2_400),
                "kind": task.kind,
                "thread_id": task.thread_id,
                "auto_dispatch": task.auto_dispatch,
                "created_at": task.created_at,
                "updated_at": task.updated_at,
            })
        })
        .collect::<Vec<_>>();
    let experiments = snapshot
        .experiments
        .iter()
        .take(40)
        .map(|experiment| {
            serde_json::json!({
                "id": experiment.id,
                "hypothesis": clip_text(&experiment.hypothesis, 800),
                "code_revision": experiment.code_revision,
                "command": clip_text(&experiment.command, 800),
                "dataset": experiment.dataset,
                "metrics": clip_text(&experiment.metrics, 1_200),
                "result": clip_text(&experiment.result, 1_200),
                "analysis": clip_text(&experiment.analysis, 1_600),
                "artifacts": experiment.artifacts,
                "status": experiment.status,
                "updated_at": experiment.updated_at,
            })
        })
        .collect::<Vec<_>>();
    let discussion = snapshot
        .discussion
        .iter()
        .take(80)
        .map(|message| {
            serde_json::json!({
                "id": message.id,
                "thread_id": message.thread_id,
                "author": message.author,
                "recipients": message.recipients,
                "message": clip_text(&message.message, 1_600),
                "created_at": message.created_at,
            })
        })
        .collect::<Vec<_>>();
    let runs = snapshot
        .runs
        .iter()
        .take(40)
        .map(|run| {
            serde_json::json!({
                "id": run.id,
                "task_id": run.task_id,
                "agent_id": run.agent_id,
                "status": run.status,
                "pid": run.pid,
                "external_session_id": run.external_session_id,
                "start_step": run.start_step,
                "started_at": run.started_at,
                "finished_at": run.finished_at,
                "output_path": run.output_path,
                "error_message": run.error_message.as_deref().map(|value| clip_text(value, 1_200)),
            })
        })
        .collect::<Vec<_>>();
    let bridge = serde_json::json!({
        "config": &snapshot.config,
        "memory": {
            "updated_at": &snapshot.memory.updated_at,
            "project_state": memory_dir.join(PROJECT_STATE_FILE),
            "session_handoff": memory_dir.join(SESSION_HANDOFF_FILE),
            "decisions": memory_dir.join(DECISIONS_FILE),
            "experiments": memory_dir.join(MEMORY_EXPERIMENTS_FILE),
            "protocol": memory_dir.join(MEMORY_PROTOCOL_FILE),
        },
        "bridge_limits": {
            "tasks": 80,
            "experiments": 40,
            "discussion": 80,
            "runs": 40,
            "text_is_truncated": true,
            "note": "This is a compact coordination snapshot. Read canonical memory or run artifacts only when more detail is needed."
        },
        "agents": &snapshot.agents,
        "capacities": &snapshot.capacities,
        "tasks": tasks,
        "experiments": experiments,
        "discussion": discussion,
        "runs": runs,
    });
    write_json(&bridge_dir.join(BRIDGE_FILE), &bridge)?;

    // The web coordinator gets an even smaller view so long-running rooms do not
    // bloat the ChatGPT conversation. Full operational detail remains in
    // project_room.json and per-run artifacts for on-demand reads.
    let web_tasks = tasks.iter().take(16).cloned().collect::<Vec<_>>();
    let web_experiments = experiments.iter().take(8).cloned().collect::<Vec<_>>();
    let web_discussion = discussion.iter().take(16).cloned().collect::<Vec<_>>();
    let web_runs = runs.iter().take(12).cloned().collect::<Vec<_>>();
    let web_context = serde_json::json!({
        "config": &snapshot.config,
        "memory": {
            "updated_at": &snapshot.memory.updated_at,
            "project_state": memory_dir.join(PROJECT_STATE_FILE),
            "session_handoff": memory_dir.join(SESSION_HANDOFF_FILE),
            "decisions": memory_dir.join(DECISIONS_FILE),
            "experiments": memory_dir.join(MEMORY_EXPERIMENTS_FILE),
            "protocol": memory_dir.join(MEMORY_PROTOCOL_FILE),
        },
        "agents": &snapshot.agents,
        "capacities": &snapshot.capacities,
        "tasks": web_tasks,
        "experiments": web_experiments,
        "discussion": web_discussion,
        "runs": web_runs,
        "detail_sources": {
            "full_snapshot": bridge_dir.join(BRIDGE_FILE),
            "inbox_protocol": bridge_dir.join(INBOX_PROTOCOL_FILE),
            "run_root": bridge_dir.join(RUNS_DIR),
        },
        "web_budget": {
            "normal_reply": "<= 6 short bullets or about 500 Chinese characters / 350 English words",
            "consultation_reply": "consensus + disagreement + decisive evidence + next action only",
            "note": "Read detail sources only on demand; never paste full logs or worker handoffs into the web conversation."
        }
    });
    write_json(&bridge_dir.join(WEB_CONTEXT_FILE), &web_context)?;

    fs::write(bridge_dir.join(CONSTITUTION_FILE), project_constitution())
        .map_err(|error| format!("写入 Project Constitution 失败: {}", error))?;
    fs::write(bridge_dir.join(INBOX_PROTOCOL_FILE), inbox_protocol())
        .map_err(|error| format!("写入 Project Room Inbox Protocol 失败: {}", error))?;
    Ok(())
}

pub(super) fn event_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub(super) fn event_string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub(super) fn process_project_event_unlocked(
    state: &AppState,
    project_id: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    let kind = event_string(value, "kind");
    let author = {
        let author = event_string(value, "author");
        if author.is_empty() {
            "agent".to_string()
        } else {
            author
        }
    };
    let dir = project_dir(state, project_id)?;
    let now = local_now_rfc3339();

    match kind.as_str() {
        "discussion.post" => {
            let message = event_string(value, "message");
            if message.is_empty() {
                return Err("discussion.post 缺少 message".to_string());
            }

            let recipients = {
                let items = event_string_array(value, "recipients");
                if items.is_empty() {
                    vec!["all".to_string()]
                } else {
                    items
                }
            };
            let thread_id = {
                let thread = event_string(value, "thread_id");
                if thread.is_empty() {
                    "general".to_string()
                } else {
                    thread
                }
            };

            let path = dir.join(DISCUSSION_FILE);
            let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&path)?;
            messages.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id,
                    author,
                    recipients,
                    message,
                    created_at: now,
                },
            );
            if messages.len() > 5_000 {
                messages.truncate(5_000);
            }
            write_json(&path, &messages)?;
        }
        "consult.request" => {
            let question = event_string(value, "question");
            if question.is_empty() {
                return Err("consult.request 缺少 question".to_string());
            }
            let title = {
                let title = event_string(value, "title");
                if title.is_empty() {
                    "Multi-agent consultation".to_string()
                } else {
                    title
                }
            };
            let mut agents = event_string_array(value, "agents");
            if agents.is_empty() {
                agents = vec!["codex".to_string(), "gemini".to_string()];
            }
            agents.retain(|agent| matches!(agent.as_str(), "codex" | "gemini"));
            agents.sort();
            agents.dedup();
            if agents.is_empty() {
                return Err("consult.request 至少需要 codex 或 gemini".to_string());
            }

            let thread_id = {
                let requested = event_string(value, "thread_id");
                if requested.is_empty() {
                    next_id("CONSULT")
                } else {
                    requested
                }
            };
            let tasks_path = dir.join(TASKS_FILE);
            let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
            for agent in &agents {
                tasks.insert(
                    0,
                    ProjectTask {
                        id: next_id("TASK"),
                        title: format!("{} · {}", title, agent),
                        goal: question.clone(),
                        owner: agent.clone(),
                        reviewers: vec!["chatgpt".to_string()],
                        status: "queued".to_string(),
                        write_scope: Vec::new(),
                        summary: String::new(),
                        kind: "consultation".to_string(),
                        thread_id: thread_id.clone(),
                        auto_dispatch: true,
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    },
                );
            }
            write_json(&tasks_path, &tasks)?;

            let discussion_path = dir.join(DISCUSSION_FILE);
            let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&discussion_path)?;
            messages.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id,
                    author,
                    recipients: agents,
                    message: question,
                    created_at: now,
                },
            );
            if messages.len() > 5_000 {
                messages.truncate(5_000);
            }
            write_json(&discussion_path, &messages)?;
        }
        "task.create" => {
            let title = event_string(value, "title");
            if title.is_empty() {
                return Err("task.create 缺少 title".to_string());
            }
            let owner = {
                let owner = event_string(value, "owner");
                if owner.is_empty() {
                    "codex".to_string()
                } else {
                    owner
                }
            };
            let reviewers = {
                let explicit = event_string_array(value, "reviewers");
                if !explicit.is_empty() {
                    explicit
                } else if owner == "gemini" {
                    vec!["codex".to_string(), "chatgpt".to_string()]
                } else {
                    vec!["chatgpt".to_string()]
                }
            };

            let path = dir.join(TASKS_FILE);
            let mut tasks = read_json::<Vec<ProjectTask>>(&path)?;
            let auto_dispatch = value
                .get("auto_dispatch")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            tasks.insert(
                0,
                ProjectTask {
                    id: next_id("TASK"),
                    title,
                    goal: event_string(value, "goal"),
                    owner,
                    reviewers,
                    status: if auto_dispatch {
                        "queued".to_string()
                    } else {
                        "backlog".to_string()
                    },
                    write_scope: event_string_array(value, "write_scope"),
                    summary: String::new(),
                    kind: {
                        let kind = event_string(value, "task_kind");
                        if kind.is_empty() {
                            "work".to_string()
                        } else {
                            kind
                        }
                    },
                    thread_id: event_string(value, "thread_id"),
                    auto_dispatch,
                    created_at: now.clone(),
                    updated_at: now,
                },
            );
            write_json(&path, &tasks)?;
        }
        "task.handoff" => {
            let task_id = event_string(value, "task_id");
            let summary = event_string(value, "summary");
            if task_id.is_empty() || summary.is_empty() {
                return Err("task.handoff 需要 task_id 和 summary".to_string());
            }

            let status = {
                let status = event_string(value, "status");
                if status.is_empty() {
                    "review".to_string()
                } else {
                    status
                }
            };
            let tasks_path = dir.join(TASKS_FILE);
            let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
            let task = tasks
                .iter_mut()
                .find(|task| task.id == task_id)
                .ok_or_else(|| format!("task.handoff 找不到 {}", task_id))?;
            let consultation = task.kind == "consultation";
            let summary_limit = if consultation { 2_400 } else { 12_000 };
            let compact_summary = clip_text(&summary, summary_limit);
            task.status = status;
            task.summary = compact_summary.clone();
            task.updated_at = now.clone();
            let recipients = if task.reviewers.is_empty() {
                vec!["chatgpt".to_string()]
            } else {
                task.reviewers.clone()
            };
            let thread_id = if task.thread_id.is_empty() {
                task.id.clone()
            } else {
                task.thread_id.clone()
            };
            write_json(&tasks_path, &tasks)?;

            let discussion_path = dir.join(DISCUSSION_FILE);
            let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&discussion_path)?;
            messages.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id,
                    author,
                    recipients,
                    message: compact_summary,
                    created_at: now,
                },
            );
            if messages.len() > 5_000 {
                messages.truncate(5_000);
            }
            write_json(&discussion_path, &messages)?;
        }
        "experiment.record" => {
            let hypothesis = event_string(value, "hypothesis");
            if hypothesis.is_empty() {
                return Err("experiment.record 缺少 hypothesis".to_string());
            }

            let path = dir.join(EXPERIMENTS_FILE);
            let mut experiments = read_json::<Vec<ProjectExperiment>>(&path)?;
            experiments.insert(
                0,
                ProjectExperiment {
                    id: next_id("EXP"),
                    hypothesis,
                    code_revision: event_string(value, "code_revision"),
                    command: event_string(value, "command"),
                    config: event_string(value, "config"),
                    dataset: event_string(value, "dataset"),
                    metrics: event_string(value, "metrics"),
                    result: event_string(value, "result"),
                    analysis: event_string(value, "analysis"),
                    artifacts: event_string_array(value, "artifacts"),
                    status: {
                        let status = event_string(value, "status");
                        if status.is_empty() {
                            "completed".to_string()
                        } else {
                            status
                        }
                    },
                    created_at: now.clone(),
                    updated_at: now,
                },
            );
            write_json(&path, &experiments)?;
        }
        "" => return Err("inbox event 缺少 kind".to_string()),
        _ => return Err(format!("不支持的 Project Room inbox kind: {}", kind)),
    }

    Ok(())
}

pub(super) fn process_project_inbox_unlocked(
    state: &AppState,
    project_id: &str,
) -> Result<usize, String> {
    let snapshot = load_snapshot_unlocked(state, project_id)?;
    let bridge_dir = PathBuf::from(&snapshot.config.local_root).join(BRIDGE_DIR);
    let inbox = bridge_dir.join(INBOX_DIR);
    fs::create_dir_all(&inbox)
        .map_err(|error| format!("创建 {} 失败: {}", inbox.display(), error))?;

    let mut files = fs::read_dir(&inbox)
        .map_err(|error| format!("读取 {} 失败: {}", inbox.display(), error))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    files.sort();

    let mut processed = 0usize;
    for path in files {
        // An agent may still be writing a file. Only ingest files that have been
        // stable for at least one second.
        if let Ok(modified) = fs::metadata(&path).and_then(|meta| meta.modified()) {
            if modified
                .elapsed()
                .map(|elapsed| elapsed < std::time::Duration::from_secs(1))
                .unwrap_or(false)
            {
                continue;
            }
        }

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                let _ = write_json(
                    &bridge_dir.join(INBOX_ERROR_FILE),
                    &serde_json::json!({
                        "file": path.to_string_lossy(),
                        "error": error.to_string(),
                        "updated_at": local_now_rfc3339(),
                    }),
                );
                let _ = fs::remove_file(&path);
                continue;
            }
        };
        let value = match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(value) => value,
            Err(error) => {
                let _ = write_json(
                    &bridge_dir.join(INBOX_ERROR_FILE),
                    &serde_json::json!({
                        "file": path.to_string_lossy(),
                        "error": format!("invalid JSON: {}", error),
                        "updated_at": local_now_rfc3339(),
                    }),
                );
                let _ = fs::remove_file(&path);
                continue;
            }
        };

        match process_project_event_unlocked(state, project_id, &value) {
            Ok(()) => {
                processed += 1;
                let _ = fs::remove_file(&path);
                let error_path = bridge_dir.join(INBOX_ERROR_FILE);
                if error_path.exists() {
                    let _ = fs::remove_file(error_path);
                }
            }
            Err(error) => {
                let _ = write_json(
                    &bridge_dir.join(INBOX_ERROR_FILE),
                    &serde_json::json!({
                        "file": path.to_string_lossy(),
                        "error": &error,
                        "updated_at": local_now_rfc3339(),
                    }),
                );
                let _ = append_system_message(
                    &project_dir(state, project_id)?,
                    "system",
                    format!("Rejected inbox event {}: {}", path.display(), error),
                );
                let _ = fs::remove_file(&path);
            }
        }
    }

    if processed > 0 {
        let snapshot = load_snapshot_unlocked(state, project_id)?;
        sync_project_bridge(&snapshot)?;
    }

    Ok(processed)
}

fn dispatch_queued_tasks_unlocked(state: &AppState, project_id: &str) -> Result<usize, String> {
    let snapshot = load_snapshot_unlocked(state, project_id)?;
    let queued = snapshot
        .tasks
        .iter()
        .filter(|task| {
            task.auto_dispatch
                && task.status == "queued"
                && matches!(task.owner.as_str(), "codex" | "gemini")
        })
        .map(|task| (task.id.clone(), task.owner.clone(), task.thread_id.clone()))
        .collect::<Vec<_>>();

    let mut dispatched = 0usize;
    for (task_id, owner, thread_id) in queued {
        match dispatch_project_task_unlocked(state, project_id, &task_id, &owner) {
            Ok(_) => dispatched += 1,
            Err(error) => {
                let dir = project_dir(state, project_id)?;
                let tasks_path = dir.join(TASKS_FILE);
                let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
                if let Some(task) = tasks.iter_mut().find(|task| task.id == task_id) {
                    task.status = "blocked".to_string();
                    task.summary = error.chars().take(2_000).collect();
                    task.updated_at = local_now_rfc3339();
                }
                write_json(&tasks_path, &tasks)?;

                let discussion_path = dir.join(DISCUSSION_FILE);
                let mut discussion = read_json::<Vec<ProjectDiscussionMessage>>(&discussion_path)?;
                discussion.insert(
                    0,
                    ProjectDiscussionMessage {
                        id: next_id("MSG"),
                        thread_id: if thread_id.is_empty() {
                            task_id.clone()
                        } else {
                            thread_id
                        },
                        author: "system".to_string(),
                        recipients: vec!["chatgpt".to_string()],
                        message: format!(
                            "{} dispatch failed: {}",
                            owner,
                            error.chars().take(1_500).collect::<String>()
                        ),
                        created_at: local_now_rfc3339(),
                    },
                );
                if discussion.len() > 5_000 {
                    discussion.truncate(5_000);
                }
                write_json(&discussion_path, &discussion)?;
            }
        }
    }
    Ok(dispatched)
}

pub(super) fn reconcile_project_operational_state_once(
    state: &Arc<AppState>,
) -> Result<(), String> {
    let _guard = state.project_store_lock.lock();
    let ids = ensure_store(state)?;

    for project_id in ids {
        let _ = refresh_run_states_unlocked(state, &project_id);
        let _ = process_project_inbox_unlocked(state, &project_id);
        let _ = dispatch_queued_tasks_unlocked(state, &project_id);
        if let Ok(snapshot) = load_snapshot_unlocked(state, &project_id) {
            let _ = sync_project_bridge(&snapshot);
        }
    }

    Ok(())
}
