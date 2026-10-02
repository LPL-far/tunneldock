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
- Project memory is lifecycle-managed, not append-only prompt text. Keep canonical/current memory bounded; move settled or superseded history into `.project_memory/archive/`, preserve lifecycle events in the append-only ledger, and read historical detail only on demand.
- Agent capacity/quota telemetry and task-transport health are separate signals. `capacities[].available` only means runtime/quota availability. Use `transports[]` for communication health. Codex app-server endpoints are per-run ephemeral; once a run finishes, its old localhost port is expected to disappear and must not be probed as a current health check.

## Default agent roles
- ChatGPT: coordinator, research lead, experiment interpreter, final reviewer, and quota-allocation brain.
- Codex: primary engineer for robust implementation, refactoring, tests, and code correctness.
- Antigravity Agent / Gemini: Agent Manager session for visualization, UI, PCA/video/figure work, rapid exploration, and visual result inspection; no IDE dependency.

## Web coordinator context budget
- ChatGPT Web is the coordination surface, not the raw execution log. Keep normal replies compact: default to at most 6 short bullets or roughly 500 Chinese characters / 350 English words unless the user explicitly asks for a deep dive.
- Never paste full tool output, full worker handoffs, long code excerpts, or complete experiment logs into the web conversation. Put detail in project files/artifacts and cite paths. Keep individual Pi tool results preferably <= 8 KiB: grep/find first, then read narrow line/offset ranges. Do not batch multiple potentially-50KiB read/bash calls into one web turn; write large diagnostics to a file and inspect only decisive slices.
- The consultation barrier is durable Project Room state, not a reason to keep one browser response stream open. Poll `.tunneldock/web_status.json` at most 5 times / about 20 seconds in one web turn. If its `state_token` is unchanged and the gate remains closed, end with a compact checkpoint and resume on the next user turn. Do not reread `web_context.json` or logs while the token is unchanged.
- For multi-agent consultation, never synthesize while `ready_for_review=false`. Once ready, read `web_context.json` and every successful worker handoff in full, then report only consensus, disagreement, decisive evidence, and next action. Preserve full worker evidence in Project Room state instead of repeating it in chat.
- After reading all settled worker evidence and completing its own review, ChatGPT must emit `consult.reviewed`, update canonical project memory and emit `memory.commit`, then perform cleanup/integration and emit `cleanup.commit`. A consultation is final only when the barrier state is `finalized`. If a worker failed/blocked, report it as missing evidence rather than inventing consensus.
- Prefer one focused consultation round over open-ended agent-to-agent chatting. Start another round only when a concrete unresolved question remains.

## Review and decision rules
- Gemini modifications to core model/training/data code require Codex review before acceptance.
- Important Codex algorithm changes require ChatGPT review for research intent and methodological consistency.
- ChatGPT Web must personally inspect the decisive diff/source before a human-facing code or method decision. Worker conclusions are inputs, not the final review. A consultation is not finalized until all requested workers are terminal, all successful handoffs were read in full, `consult.reviewed` was accepted, canonical memory was updated and committed, and cleanup/integration was verified by `cleanup.commit`.
- Keep the human-facing review compact: 2-4 decisive code findings, agent consensus/disagreement, at most 2-3 options with tradeoffs, and the exact question requiring the researcher's decision.
- Agents may discuss and challenge each other. Disagreement should be preserved in Project Room discussion until a decision is made.
- The human researcher remains the final decision maker. Only after the human explicitly decides should ChatGPT persist a `decision.record` to DECISIONS.md.

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

## consult.retry
Use this only when the matching consultation barrier reports `state=invalid_handoff` (or a terminal worker handoff is missing/unreadable). Retry only the affected agents; the same `consultation_id` remains the review barrier.
```json
{
  "kind": "consult.retry",
  "author": "chatgpt",
  "consultation_id": "CONSULTATION-...",
  "agents": ["codex"]
}
```

## consult.reviewed
Write this only after `web_context.json` shows the consultation `ready_for_review=true`, ChatGPT has read every successful `agents[].handoff_path` in full, accounted for any failed/blocked agent, and completed its own code/method review.
```json
{
  "kind": "consult.reviewed",
  "author": "chatgpt",
  "consultation_id": "CONSULTATION-..."
}
```
TunnelDock rejects this event while any requested worker is still non-terminal or a successful worker handoff is missing/not transport-safe. After writing it, wait until `web_context.json` shows `web_review_complete=true` before answering the human.

## memory.commit
Use this after `consult.reviewed` and after ChatGPT Web has actually updated canonical memory. `changed_files` must include `PROJECT_STATE.md`, `SESSION_HANDOFF.md`, and at least one affected domain file (`MODEL_DESIGN.md`, `DATA_CATALOG.md`, `EXPERIMENTS.md`, `RESULTS.md`, `REFERENCES.md`, `DOCUMENTS.md`, or `DECISIONS.md`). TunnelDock validates that the files exist, are non-empty, and were modified after the web review.
```json
{
  "kind": "memory.commit",
  "author": "chatgpt",
  "consultation_id": "CONSULTATION-...",
  "changed_files": ["PROJECT_STATE.md", "SESSION_HANDOFF.md", "MODEL_DESIGN.md"],
  "archive_files": ["archive/model/2026-Q4.md"]
}
```
After `memory.commit`, the consultation moves to `awaiting_cleanup` rather than finalizing.

## cleanup.commit
Use this only after memory has been committed. If `web_context.json -> memory.health.requires_compaction=true`, first move superseded/history detail from the listed over-budget canonical files into the matching `.project_memory/archive/<domain>/` location, rewrite canonical memory as current truth, and submit `memory.commit` again. Then ChatGPT Web must review and integrate five categories: `memory`, `documents`, `code`, `logs`, and `scratch`. `checked_paths` must include `.` so the entire Project Room is scanned. Paths listed as removed must truly be gone; archived/retained paths must exist; remaining hygiene candidates must be explicitly accounted for as archived or retained evidence.
```json
{
  "kind": "cleanup.commit",
  "author": "chatgpt",
  "consultation_id": "CONSULTATION-...",
  "reviewed_categories": ["memory", "documents", "code", "logs", "scratch"],
  "checked_paths": ["."],
  "removed_paths": ["tmp/debug.log", "research/obsolete_note.md"],
  "archived_paths": ["research/archive/2026-10-02/old_design.md"],
  "retained_paths": ["research/archive", "results/unique_failure.log"],
  "summary": "Merged the current design into the canonical document, removed disposable logs/scratch and superseded code; retained only unique evidence and the explicit research archive."
}
```
Only after TunnelDock accepts `cleanup.commit` and the consultation state becomes `finalized` should ChatGPT give the human the final consultation conclusion.

## decision.record
Use this only after the human researcher has made the final decision. Record the durable conclusion, not the whole discussion.
```json
{
  "kind": "decision.record",
  "author": "chatgpt",
  "title": "Keep geometry gate before decoder",
  "decision": "Use the geometry gate in the main method and keep the ungated path only as an ablation.",
  "rationale": "Matches the intended hypothesis and current evidence; the simpler ungated path remains useful only for ablation.",
  "evidence": ["model/decoder.py", "EXP-..."],
  "thread_id": "CONSULT-..."
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
- `consult.request` is read-only by default. Keep each worker's final answer under 1200 characters. TunnelDock assigns one `consultation_id` to the whole round. Poll `.tunneldock/web_status.json` for the barrier; `web_context.json` is detail-on-demand.
- ChatGPT must not synthesize or recommend from partial results. Poll `web_status.json` for no more than 5 cycles / about 20 seconds in one browser turn. If unchanged, emit only a compact checkpoint and let the next user turn resume from `state_token`. If `state=invalid_handoff`, send one `consult.retry` for the listed `invalid_handoffs`. Once `ready_for_review=true`, read `web_context.json` and every successful full handoff, perform your own review, write `consult.reviewed`, update canonical memory and write `memory.commit`, then clean/integrate obsolete memory/docs/code/logs/scratch and write `cleanup.commit`. Wait until state=`finalized` before the final human-facing analysis.
- If the two agents materially disagree, ChatGPT may issue one focused follow-up `consult.request` with the same `thread_id`. The follow-up gets a new `consultation_id`, so it has its own barrier. Avoid recursive debate unless the user explicitly asks for it.
- Before asking the human to decide a code/method question, ChatGPT must personally inspect the relevant diff or source files and surface only the decisive code-review points; worker handoffs are evidence, not a substitute for review.
- `decision.record` is written only after the human researcher explicitly decides. Keep it concise and durable; never record an unresolved recommendation as a decision.
- Canonical research memory remains under `.project_memory/`; update it directly after meaningful decisions/results. Root canonical files are bounded current materialized views. Historical/superseded research memory belongs in `archive/`; lifecycle/audit events belong in `ledger/`. If memory health reports over-budget canonical files, compact before finalization. Finalization also requires cleanup/integration: superseded code/docs are removed or archived, disposable logs/scratch are deleted, and retained logs must be unique evidence.
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

fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

fn compact_json_fields(value: &serde_json::Value, limits: &[(&str, usize)]) -> serde_json::Value {
    let mut compact = value.clone();
    let Some(object) = compact.as_object_mut() else {
        return compact;
    };
    for (key, limit) in limits {
        if let Some(text) = object.get(*key).and_then(serde_json::Value::as_str) {
            object.insert(
                (*key).to_string(),
                serde_json::Value::String(clip_text(text, *limit)),
            );
        }
    }
    compact
}

fn select_relevant_values<F>(
    values: &[serde_json::Value],
    priority: F,
    total_limit: usize,
) -> Vec<serde_json::Value>
where
    F: Fn(&serde_json::Value) -> bool,
{
    let mut selected = Vec::with_capacity(total_limit);
    let mut ids = std::collections::HashSet::new();

    for value in values.iter().filter(|value| priority(value)) {
        if selected.len() >= total_limit {
            break;
        }
        let id = json_str(value, "id");
        if id.is_empty() || ids.insert(id.to_string()) {
            selected.push(value.clone());
        }
    }
    for value in values {
        if selected.len() >= total_limit {
            break;
        }
        let id = json_str(value, "id");
        if id.is_empty() || ids.insert(id.to_string()) {
            selected.push(value.clone());
        }
    }
    selected
}

fn stable_state_token(value: &serde_json::Value) -> String {
    use std::hash::{Hash, Hasher};
    let serialized = serde_json::to_string(value).unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serialized.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn consultation_group_id(task: &ProjectTask) -> String {
    if !task.consultation_id.trim().is_empty() {
        return task.consultation_id.clone();
    }
    format!("legacy:{}:{}", task.thread_id, task.created_at)
}

fn consultation_barriers(tasks: &[ProjectTask], runs: &[AgentRun]) -> Vec<serde_json::Value> {
    let mut seen = std::collections::HashSet::new();
    let mut barriers = Vec::new();

    for task in tasks.iter().filter(|task| task.kind == "consultation") {
        let consultation_id = consultation_group_id(task);
        if !seen.insert(consultation_id.clone()) {
            continue;
        }

        let members = tasks
            .iter()
            .filter(|candidate| {
                candidate.kind == "consultation"
                    && consultation_group_id(candidate) == consultation_id
            })
            .collect::<Vec<_>>();
        let all_settled = members.iter().all(|member| {
            matches!(
                member.status.as_str(),
                "review" | "completed" | "blocked" | "failed" | "cancelled"
            )
        });
        let has_failures = members
            .iter()
            .any(|member| matches!(member.status.as_str(), "blocked" | "failed" | "cancelled"));

        let agents = members
            .iter()
            .map(|member| {
                let run = runs.iter().find(|run| run.task_id == member.id);
                let handoff_path = run.map(|run| run.output_path.clone());
                let handoff_text = handoff_path
                    .as_deref()
                    .and_then(|path| fs::read_to_string(path).ok());
                let handoff_ready = handoff_text
                    .as_deref()
                    .map(str::trim)
                    .map(|text| !text.is_empty())
                    .unwrap_or(false);
                let replacement_chars = handoff_text
                    .as_deref()
                    .map(|text| text.chars().filter(|ch| *ch == '\u{FFFD}').count())
                    .unwrap_or_default();
                let (character_count, non_ascii_chars) = handoff_text
                    .as_deref()
                    .map(|text| {
                        (
                            text.chars().count(),
                            text.chars().filter(|ch| !ch.is_ascii()).count(),
                        )
                    })
                    .unwrap_or_default();
                let non_ascii_limit = std::cmp::max(8, character_count / 10);
                let handoff_transport_safe =
                    handoff_ready && replacement_chars < 3 && non_ascii_chars <= non_ascii_limit;
                let success = matches!(member.status.as_str(), "review" | "completed");
                serde_json::json!({
                    "agent_id": member.owner,
                    "task_id": member.id,
                    "status": member.status,
                    "success": success,
                    "handoff_ready": handoff_ready,
                    "handoff_readable": handoff_transport_safe,
                    "handoff_transport_safe": handoff_transport_safe,
                    "replacement_chars": replacement_chars,
                    "non_ascii_chars": non_ascii_chars,
                    "handoff_path": handoff_path,
                    "error": run.and_then(|run| run.error_message.as_deref()).map(|value| clip_text(value, 800)),
                    "summary_preview": clip_text(&member.summary, 400),
                })
            })
            .collect::<Vec<_>>();

        let invalid_handoffs = agents
            .iter()
            .filter(|agent| {
                agent
                    .get("success")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
                    && agent
                        .get("handoff_ready")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                    && !agent
                        .get("handoff_readable")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
            })
            .filter_map(|agent| agent.get("agent_id").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>();
        let successful_handoffs_ready = agents.iter().all(|agent| {
            !agent
                .get("success")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                || agent
                    .get("handoff_readable")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
        });
        let ready_for_review = all_settled && successful_handoffs_ready;
        let web_review_complete =
            ready_for_review && members.iter().all(|member| member.web_reviewed);
        let memory_commit_complete =
            web_review_complete && members.iter().all(|member| member.memory_committed);
        let cleanup_commit_complete =
            memory_commit_complete && members.iter().all(|member| member.cleanup_committed);
        let waiting_for = agents
            .iter()
            .filter(|agent| {
                !matches!(
                    agent
                        .get("status")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default(),
                    "review" | "completed" | "blocked" | "failed" | "cancelled"
                ) || (agent
                    .get("success")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
                    && !agent
                        .get("handoff_readable")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false))
            })
            .filter_map(|agent| agent.get("agent_id").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>();

        barriers.push(serde_json::json!({
            "consultation_id": consultation_id,
            "thread_id": task.thread_id,
            "title": task.title.split(" · ").next().unwrap_or(&task.title),
            "created_at": task.created_at,
            "ready_for_review": ready_for_review,
            "web_review_complete": web_review_complete,
            "memory_commit_complete": memory_commit_complete,
            "cleanup_commit_complete": cleanup_commit_complete,
            "all_settled": all_settled,
            "has_failures": has_failures,
            "state": if !invalid_handoffs.is_empty() {
                "invalid_handoff"
            } else if !ready_for_review {
                "waiting_workers"
            } else if !web_review_complete {
                if has_failures { "awaiting_web_review_with_failures" } else { "awaiting_web_review" }
            } else if !memory_commit_complete {
                "awaiting_memory_commit"
            } else if !cleanup_commit_complete {
                "awaiting_cleanup"
            } else {
                "finalized"
            },
            "waiting_for": waiting_for,
            "invalid_handoffs": invalid_handoffs,
            "agents": agents,
            "review_rule": "Do not synthesize partial worker results. After ready_for_review=true, read every successful handoff in full and write consult.reviewed. Then update canonical memory and write memory.commit. Finally clean/consolidate obsolete docs/code/logs/scratch and write cleanup.commit. Only state=finalized permits the final human-facing consultation analysis.",
        }));
    }

    barriers
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
                "consultation_id": task.consultation_id,
                "web_reviewed": task.web_reviewed,
                "memory_committed": task.memory_committed,
                "cleanup_committed": task.cleanup_committed,
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
    let consultations = consultation_barriers(&snapshot.tasks, &snapshot.runs);
    let bridge = serde_json::json!({
        "config": &snapshot.config,
        "memory": {
            "updated_at": &snapshot.memory.updated_at,
            "index": memory_dir.join(MEMORY_INDEX_FILE),
            "project_state": memory_dir.join(PROJECT_STATE_FILE),
            "session_handoff": memory_dir.join(SESSION_HANDOFF_FILE),
            "decisions": memory_dir.join(DECISIONS_FILE),
            "model_design": memory_dir.join(MODEL_DESIGN_FILE),
            "data_catalog": memory_dir.join(DATA_CATALOG_FILE),
            "experiments": memory_dir.join(MEMORY_EXPERIMENTS_FILE),
            "results": memory_dir.join(RESULTS_FILE),
            "references": memory_dir.join(REFERENCES_FILE),
            "documents": memory_dir.join(DOCUMENTS_FILE),
            "protocol": memory_dir.join(MEMORY_PROTOCOL_FILE),
            "status": memory_dir.join(MEMORY_STATUS_FILE),
            "archive": memory_dir.join(MEMORY_ARCHIVE_DIR),
            "ledger": memory_dir.join(MEMORY_LEDGER_DIR),
            "health": &snapshot.memory_health,
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
        "transports": &snapshot.transports,
        "tasks": tasks,
        "experiments": experiments,
        "discussion": discussion,
        "runs": runs,
        "consultations": &consultations,
    });
    let _ = write_json_if_changed(&bridge_dir.join(BRIDGE_FILE), &bridge)?;

    // The web coordinator gets an even smaller view so long-running rooms do not
    // bloat the ChatGPT conversation. Full operational detail remains in
    // project_room.json and per-run artifacts for on-demand reads.
    let web_tasks = select_relevant_values(
        &tasks,
        |task| {
            matches!(
                json_str(task, "status"),
                "queued" | "active" | "running" | "interactive" | "review" | "blocked"
            )
        },
        12,
    )
    .iter()
    .map(|task| compact_json_fields(task, &[("goal", 420), ("summary", 700)]))
    .collect::<Vec<_>>();
    let web_experiments = experiments
        .iter()
        .take(4)
        .map(|experiment| {
            compact_json_fields(
                experiment,
                &[
                    ("hypothesis", 360),
                    ("command", 320),
                    ("metrics", 500),
                    ("result", 500),
                    ("analysis", 700),
                ],
            )
        })
        .collect::<Vec<_>>();
    let web_discussion = discussion
        .iter()
        .take(8)
        .map(|message| compact_json_fields(message, &[("message", 600)]))
        .collect::<Vec<_>>();
    let web_runs = select_relevant_values(
        &runs,
        |run| {
            matches!(
                json_str(run, "status"),
                "running" | "interactive" | "failed"
            )
        },
        8,
    )
    .iter()
    .map(|run| compact_json_fields(run, &[("error_message", 500)]))
    .collect::<Vec<_>>();
    let web_consultations = select_relevant_values(
        &consultations,
        |consultation| json_str(consultation, "state") != "finalized",
        6,
    )
    .iter()
    .map(|consultation| compact_json_fields(consultation, &[("review_rule", 360)]))
    .collect::<Vec<_>>();
    let pending_worker_consultations = web_consultations
        .iter()
        .filter(|item| {
            item.get("state").and_then(serde_json::Value::as_str) == Some("waiting_workers")
        })
        .count();
    let invalid_handoff_consultations = web_consultations
        .iter()
        .filter(|item| {
            item.get("state").and_then(serde_json::Value::as_str) == Some("invalid_handoff")
        })
        .count();
    let awaiting_web_reviews = web_consultations
        .iter()
        .filter(|item| {
            item.get("ready_for_review")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                && !item
                    .get("web_review_complete")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
        })
        .count();
    let awaiting_memory_commits = web_consultations
        .iter()
        .filter(|item| {
            item.get("state").and_then(serde_json::Value::as_str) == Some("awaiting_memory_commit")
        })
        .count();
    let awaiting_cleanups = web_consultations
        .iter()
        .filter(|item| {
            item.get("state").and_then(serde_json::Value::as_str) == Some("awaiting_cleanup")
        })
        .count();
    let review_gate = serde_json::json!({
        "pending_worker_consultations": pending_worker_consultations,
        "invalid_handoff_consultations": invalid_handoff_consultations,
        "awaiting_web_reviews": awaiting_web_reviews,
        "awaiting_memory_commits": awaiting_memory_commits,
        "awaiting_cleanups": awaiting_cleanups,
        "rule": "A consultation is final only when state=finalized. Wait for workers, review all handoffs, commit canonical memory, compact over-budget current memory into archive when memory.health.requires_compaction=true, then clean/consolidate obsolete documents, code, logs and scratch and write cleanup.commit. Failed/blocked agents are missing evidence, never consensus."
    });

    let status_tasks = web_tasks
        .iter()
        .filter(|task| {
            matches!(
                json_str(task, "status"),
                "queued" | "active" | "running" | "interactive" | "review" | "blocked"
            )
        })
        .take(8)
        .map(|task| {
            serde_json::json!({
                "id": task.get("id"),
                "owner": task.get("owner"),
                "status": task.get("status"),
                "updated_at": task.get("updated_at"),
            })
        })
        .collect::<Vec<_>>();
    let status_consultations = web_consultations
        .iter()
        .filter(|consultation| json_str(consultation, "state") != "finalized")
        .take(6)
        .map(|consultation| {
            serde_json::json!({
                "consultation_id": consultation.get("consultation_id"),
                "state": consultation.get("state"),
                "ready_for_review": consultation.get("ready_for_review"),
                "web_review_complete": consultation.get("web_review_complete"),
                "memory_commit_complete": consultation.get("memory_commit_complete"),
                "cleanup_commit_complete": consultation.get("cleanup_commit_complete"),
                "waiting_for": consultation.get("waiting_for"),
                "invalid_handoffs": consultation.get("invalid_handoffs"),
            })
        })
        .collect::<Vec<_>>();
    let status_transports = snapshot
        .transports
        .iter()
        .map(|transport| {
            serde_json::json!({
                "agent_id": transport.agent_id,
                "status": transport.status,
                "active_run_id": transport.active_run_id,
                "last_success_at": transport.last_success_at,
                "last_failure_at": transport.last_failure_at,
            })
        })
        .collect::<Vec<_>>();
    let status_core = serde_json::json!({
        "project_id": &snapshot.config.id,
        "memory_updated_at": &snapshot.memory.updated_at,
        "tasks": &status_tasks,
        "consultations": &status_consultations,
        "transports": &status_transports,
        "review_gate": &review_gate,
    });
    let state_token = stable_state_token(&status_core);
    let web_status = serde_json::json!({
        "project_id": &snapshot.config.id,
        "state_token": state_token,
        "has_pending_work": !status_tasks.is_empty() || !status_consultations.is_empty(),
        "tasks": status_tasks,
        "consultations": status_consultations,
        "transports": status_transports,
        "review_gate": &review_gate,
        "detail_source": bridge_dir.join(WEB_CONTEXT_FILE),
        "flow_control": {
            "poll_interval_seconds": 4,
            "max_inline_wait_seconds": 20,
            "max_poll_cycles": 5,
            "unchanged_state": "If state_token is unchanged, do not reread web_context.json or full logs.",
            "timeout_action": "End the current web stream with a compact checkpoint and no substantive conclusion. On the next user turn, reread web_status.json and continue from state_token. Logical consultation barriers remain persisted on disk."
        }
    });

    let web_context = serde_json::json!({
        "config": &snapshot.config,
        "memory": {
            "updated_at": &snapshot.memory.updated_at,
            "index": memory_dir.join(MEMORY_INDEX_FILE),
            "project_state": memory_dir.join(PROJECT_STATE_FILE),
            "session_handoff": memory_dir.join(SESSION_HANDOFF_FILE),
            "decisions": memory_dir.join(DECISIONS_FILE),
            "model_design": memory_dir.join(MODEL_DESIGN_FILE),
            "data_catalog": memory_dir.join(DATA_CATALOG_FILE),
            "experiments": memory_dir.join(MEMORY_EXPERIMENTS_FILE),
            "results": memory_dir.join(RESULTS_FILE),
            "references": memory_dir.join(REFERENCES_FILE),
            "documents": memory_dir.join(DOCUMENTS_FILE),
            "protocol": memory_dir.join(MEMORY_PROTOCOL_FILE),
            "status": memory_dir.join(MEMORY_STATUS_FILE),
            "archive": memory_dir.join(MEMORY_ARCHIVE_DIR),
            "ledger": memory_dir.join(MEMORY_LEDGER_DIR),
            "health": &snapshot.memory_health,
        },
        "agents": &snapshot.agents,
        "capacities": &snapshot.capacities,
        "transports": &snapshot.transports,
        "agent_health_semantics": {
            "capacities": "quota/runtime telemetry only; available=true does NOT prove task communication health",
            "transports": "end-to-end task transport evidence; use this for healthy/degraded/running/untested decisions",
            "ephemeral_endpoints": "Codex per-run app-server ports are temporary and normally disappear after a run; never probe a finished run's old port to infer current health"
        },
        "tasks": web_tasks,
        "experiments": web_experiments,
        "discussion": web_discussion,
        "runs": web_runs,
        "consultations": web_consultations,
        "review_gate": &review_gate,
        "detail_sources": {
            "status": bridge_dir.join(WEB_STATUS_FILE),
            "full_snapshot": bridge_dir.join(BRIDGE_FILE),
            "inbox_protocol": bridge_dir.join(INBOX_PROTOCOL_FILE),
            "run_root": bridge_dir.join(RUNS_DIR),
        },
        "web_budget": {
            "normal_reply": "<= 6 short bullets or about 500 Chinese characters / 350 English words",
            "consultation_reply": "after review gate opens: code-review findings + consensus/disagreement + decisive evidence + options + next action only",
            "max_inline_wait_seconds": 20,
            "tool_output_rule": "Never paste raw logs or large tool output into the web stream. Save/read artifacts by path and quote only decisive lines.",
            "note": "Logical waiting lives in persisted Project Room state, not in one long browser response stream. Poll web_status.json; read web_context/full handoffs only after state_token changes or the review gate opens."
        }
    });
    let _ = write_json_if_changed(&bridge_dir.join(WEB_STATUS_FILE), &web_status)?;
    let _ = write_json_if_changed(&bridge_dir.join(WEB_CONTEXT_FILE), &web_context)?;

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

fn normalize_cleanup_key(value: &str) -> String {
    let mut key = value.trim().replace('\\', "/");
    while key.starts_with("./") {
        key = key[2..].to_string();
    }
    key.trim_end_matches('/').to_ascii_lowercase()
}

fn resolve_cleanup_path(root: &Path, value: &str) -> Result<PathBuf, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("cleanup path 不能为空".to_string());
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(format!("cleanup path 必须相对 project root: {}", value));
    }
    for component in path.components() {
        if matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        ) {
            return Err(format!("cleanup path 越界: {}", value));
        }
    }
    Ok(root.join(path))
}

fn cleanup_path_is_covered(path: &str, declared: &[String]) -> bool {
    let key = normalize_cleanup_key(path);
    declared.iter().any(|value| {
        let parent = normalize_cleanup_key(value);
        key == parent || (!parent.is_empty() && key.starts_with(&format!("{parent}/")))
    })
}

fn validate_cleanup_commit(
    root: &Path,
    checked_paths: &[String],
    removed_paths: &[String],
    archived_paths: &[String],
    retained_paths: &[String],
    reviewed_categories: &[String],
) -> Result<Vec<HygieneCandidate>, String> {
    let required_categories = ["memory", "documents", "code", "logs", "scratch"];
    for category in required_categories {
        if !reviewed_categories
            .iter()
            .any(|value| value.eq_ignore_ascii_case(category))
        {
            return Err(format!(
                "cleanup.commit 缺少 reviewed category: {}",
                category
            ));
        }
    }
    if !checked_paths
        .iter()
        .any(|value| normalize_cleanup_key(value) == ".")
    {
        return Err(
            "cleanup.commit 的 checked_paths 必须包含 '.'，确保检查整个 Project Room".to_string(),
        );
    }

    for value in checked_paths {
        let path = resolve_cleanup_path(root, value)?;
        if !path.exists() {
            return Err(format!("cleanup.commit checked path 不存在: {}", value));
        }
    }
    for value in removed_paths {
        let path = resolve_cleanup_path(root, value)?;
        if path.exists() {
            return Err(format!(
                "cleanup.commit 标记 removed 但路径仍存在: {}",
                value
            ));
        }
    }
    for value in archived_paths.iter().chain(retained_paths.iter()) {
        let path = resolve_cleanup_path(root, value)?;
        if !path.exists() {
            return Err(format!(
                "cleanup.commit 声明保留/归档但路径不存在: {}",
                value
            ));
        }
    }

    let mut candidates = Vec::new();
    scan_hygiene_dir(root, root, 0, &mut candidates);
    let unresolved = candidates
        .iter()
        .filter(|candidate| {
            !cleanup_path_is_covered(&candidate.path, archived_paths)
                && !cleanup_path_is_covered(&candidate.path, retained_paths)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        let preview = unresolved
            .iter()
            .take(12)
            .map(|candidate| candidate.path.clone())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "cleanup.commit 仍有未处理 hygiene 候选: {}",
            preview
        ));
    }
    Ok(candidates)
}

fn validate_memory_archive_files(
    memory_dir: &Path,
    archive_files: &[String],
) -> Result<(), String> {
    for value in archive_files {
        let path = Path::new(value);
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(format!("memory.commit archive_files 路径越界: {}", value));
        }
        let normalized = value.replace('\\', "/");
        if normalized != MEMORY_ARCHIVE_DIR
            && !normalized.starts_with(&format!("{}/", MEMORY_ARCHIVE_DIR))
        {
            return Err(format!(
                "memory.commit archive_files 必须位于 {}/ 下: {}",
                MEMORY_ARCHIVE_DIR, value
            ));
        }
        let full_path = memory_dir.join(path);
        if !full_path.exists() {
            return Err(format!(
                "memory.commit 声明 archive 但路径不存在: {}",
                full_path.display()
            ));
        }
    }
    Ok(())
}

fn validate_memory_commit_files(
    memory_dir: &Path,
    changed_files: &[String],
    reviewed_at_millis: i64,
) -> Result<(), String> {
    let allowed = [
        PROJECT_STATE_FILE,
        SESSION_HANDOFF_FILE,
        DECISIONS_FILE,
        MODEL_DESIGN_FILE,
        DATA_CATALOG_FILE,
        MEMORY_EXPERIMENTS_FILE,
        RESULTS_FILE,
        REFERENCES_FILE,
        DOCUMENTS_FILE,
    ];
    if let Some(invalid) = changed_files
        .iter()
        .find(|file| !allowed.contains(&file.as_str()))
    {
        return Err(format!(
            "memory.commit 不允许的 canonical file: {}",
            invalid
        ));
    }
    for required in [PROJECT_STATE_FILE, SESSION_HANDOFF_FILE] {
        if !changed_files.iter().any(|file| file == required) {
            return Err(format!("memory.commit 必须包含 {}", required));
        }
    }
    let domain_files = [
        DECISIONS_FILE,
        MODEL_DESIGN_FILE,
        DATA_CATALOG_FILE,
        MEMORY_EXPERIMENTS_FILE,
        RESULTS_FILE,
        REFERENCES_FILE,
        DOCUMENTS_FILE,
    ];
    if !changed_files
        .iter()
        .any(|file| domain_files.contains(&file.as_str()))
    {
        return Err(
            "memory.commit 除 PROJECT_STATE.md / SESSION_HANDOFF.md 外，至少要更新一个领域记忆文件"
                .to_string(),
        );
    }

    for file_name in changed_files {
        let path = memory_dir.join(file_name);
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("memory.commit 读取 {} 失败: {}", path.display(), error))?;
        if content.trim().is_empty() {
            return Err(format!("memory.commit 拒绝空文件: {}", file_name));
        }
        let modified = fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .map_err(|error| {
                format!(
                    "memory.commit 读取 {} 修改时间失败: {}",
                    path.display(),
                    error
                )
            })?;
        let modified_dt: chrono::DateTime<chrono::Utc> = modified.into();
        if modified_dt.timestamp_millis() + 2_000 < reviewed_at_millis {
            return Err(format!(
                "memory.commit 拒绝旧记忆：{} 没有在本轮 web review 后更新",
                file_name
            ));
        }
    }
    Ok(())
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
        "decision.record" => {
            let title = event_string(value, "title");
            let decision = event_string(value, "decision");
            if title.is_empty() || decision.is_empty() {
                return Err("decision.record 需要 title 和 decision".to_string());
            }
            let rationale = event_string(value, "rationale");
            let evidence = event_string_array(value, "evidence");
            let thread_id = event_string(value, "thread_id");
            let config = load_snapshot_unlocked(state, project_id)?.config;
            let memory_dir = project_memory_dir(&config);
            let decisions_path = memory_dir.join(DECISIONS_FILE);
            let mut current = fs::read_to_string(&decisions_path)
                .unwrap_or_else(|_| "# Durable Decisions\n".to_string());
            if current.contains("No durable decisions recorded yet.") {
                current = current.replace("No durable decisions recorded yet.\n", "");
            }
            if !current.ends_with('\n') {
                current.push('\n');
            }
            current.push_str(&format!(
                "\n## {} — {}\n\n**Decision:** {}\n",
                now, title, decision
            ));
            if !rationale.is_empty() {
                current.push_str(&format!("\n**Rationale:** {}\n", rationale));
            }
            if !evidence.is_empty() {
                current.push_str(&format!("\n**Evidence:** {}\n", evidence.join(", ")));
            }
            if !thread_id.is_empty() {
                current.push_str(&format!("\n**Discussion thread:** {}\n", thread_id));
            }
            fs::write(&decisions_path, current)
                .map_err(|error| format!("写入 {} 失败: {}", decisions_path.display(), error))?;
            append_memory_ledger(
                &config,
                serde_json::json!({
                    "kind": "decision.record",
                    "title": title.clone(),
                    "decision": decision.clone(),
                    "rationale": rationale.clone(),
                    "evidence": evidence.clone(),
                    "thread_id": thread_id.clone(),
                    "author": author.clone(),
                }),
            )?;

            let discussion_path = dir.join(DISCUSSION_FILE);
            let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&discussion_path)?;
            messages.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id: if thread_id.is_empty() {
                        "decision".to_string()
                    } else {
                        thread_id
                    },
                    author,
                    recipients: vec!["all".to_string()],
                    message: format!("Decision recorded: {} — {}", title, decision),
                    created_at: now,
                },
            );
            if messages.len() > 5_000 {
                messages.truncate(5_000);
            }
            write_json(&discussion_path, &messages)?;
        }
        "consult.retry" => {
            let consultation_id = event_string(value, "consultation_id");
            if consultation_id.is_empty() {
                return Err("consult.retry 缺少 consultation_id".to_string());
            }
            let mut agents = event_string_array(value, "agents");
            agents.retain(|agent| matches!(agent.as_str(), "codex" | "gemini"));
            agents.sort();
            agents.dedup();
            if agents.is_empty() {
                return Err("consult.retry 至少需要一个 agents: codex/gemini".to_string());
            }

            let tasks_path = dir.join(TASKS_FILE);
            let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
            let mut retried = Vec::new();
            for task in &mut tasks {
                if task.kind != "consultation"
                    || consultation_group_id(task) != consultation_id
                    || !agents.iter().any(|agent| agent == &task.owner)
                {
                    continue;
                }
                if !matches!(
                    task.status.as_str(),
                    "review" | "completed" | "blocked" | "failed" | "cancelled"
                ) {
                    return Err(format!(
                        "consult.retry 拒绝并发重试：{} 当前仍处于 {}",
                        task.owner, task.status
                    ));
                }
                task.status = "queued".to_string();
                task.summary.clear();
                task.web_reviewed = false;
                task.memory_committed = false;
                task.cleanup_committed = false;
                task.auto_dispatch = true;
                task.updated_at = now.clone();
                retried.push(task.owner.clone());
            }
            if retried.is_empty() {
                return Err(format!(
                    "consult.retry 在 {} 中没有找到指定 Agent",
                    consultation_id
                ));
            }
            write_json(&tasks_path, &tasks)?;

            let discussion_path = dir.join(DISCUSSION_FILE);
            let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&discussion_path)?;
            let thread_id = tasks
                .iter()
                .find(|task| {
                    task.kind == "consultation" && consultation_group_id(task) == consultation_id
                })
                .map(|task| task.thread_id.clone())
                .unwrap_or_else(|| "consultation-retry".to_string());
            messages.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id,
                    author,
                    recipients: retried.clone(),
                    message: format!(
                        "Retry requested for {} in {}.",
                        retried.join(", "),
                        consultation_id
                    ),
                    created_at: now,
                },
            );
            if messages.len() > 5_000 {
                messages.truncate(5_000);
            }
            write_json(&discussion_path, &messages)?;
        }
        "consult.reviewed" => {
            let consultation_id = event_string(value, "consultation_id");
            if consultation_id.is_empty() {
                return Err("consult.reviewed 缺少 consultation_id".to_string());
            }

            let tasks_path = dir.join(TASKS_FILE);
            let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
            let member_indexes = tasks
                .iter()
                .enumerate()
                .filter(|(_, task)| {
                    task.kind == "consultation" && consultation_group_id(task) == consultation_id
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if member_indexes.is_empty() {
                return Err(format!(
                    "consult.reviewed 找不到 consultation_id: {}",
                    consultation_id
                ));
            }

            let runs = read_json::<Vec<AgentRun>>(&dir.join(RUNS_FILE))?;
            for index in &member_indexes {
                let task = &tasks[*index];
                if !matches!(
                    task.status.as_str(),
                    "review" | "completed" | "blocked" | "failed" | "cancelled"
                ) {
                    return Err(format!(
                        "consult.reviewed 拒绝提前确认：{} 仍处于 {}",
                        task.owner, task.status
                    ));
                }
                if matches!(task.status.as_str(), "review" | "completed") {
                    let handoff_text = runs
                        .iter()
                        .find(|run| run.task_id == task.id)
                        .and_then(|run| fs::read_to_string(&run.output_path).ok());
                    let handoff_ready = handoff_text
                        .as_deref()
                        .map(str::trim)
                        .map(|text| !text.is_empty())
                        .unwrap_or(false);
                    let transport_safe = handoff_text
                        .as_deref()
                        .map(|text| {
                            let character_count = text.chars().count();
                            let non_ascii_chars = text.chars().filter(|ch| !ch.is_ascii()).count();
                            let non_ascii_limit = std::cmp::max(8, character_count / 10);
                            text.chars().filter(|ch| *ch == '\u{FFFD}').count() < 3
                                && non_ascii_chars <= non_ascii_limit
                        })
                        .unwrap_or(false);
                    if !handoff_ready || !transport_safe {
                        return Err(format!(
                            "consult.reviewed 缺少 {} 的完整 transport-safe handoff",
                            task.owner
                        ));
                    }
                }
            }

            for index in member_indexes {
                tasks[index].web_reviewed = true;
                tasks[index].memory_committed = false;
                tasks[index].cleanup_committed = false;
                tasks[index].updated_at = now.clone();
            }
            write_json(&tasks_path, &tasks)?;

            let discussion_path = dir.join(DISCUSSION_FILE);
            let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&discussion_path)?;
            let thread_id = tasks
                .iter()
                .find(|task| {
                    task.kind == "consultation" && consultation_group_id(task) == consultation_id
                })
                .map(|task| task.thread_id.clone())
                .unwrap_or_else(|| "consultation-review".to_string());
            messages.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id,
                    author,
                    recipients: vec!["all".to_string()],
                    message: format!("Web review completed for {}.", consultation_id),
                    created_at: now,
                },
            );
            if messages.len() > 5_000 {
                messages.truncate(5_000);
            }
            write_json(&discussion_path, &messages)?;
        }
        "memory.commit" => {
            if author != "chatgpt" {
                return Err("memory.commit 只能由 ChatGPT Web 最终协调者提交".to_string());
            }
            let consultation_id = event_string(value, "consultation_id");
            if consultation_id.is_empty() {
                return Err("memory.commit 缺少 consultation_id".to_string());
            }
            let mut changed_files = event_string_array(value, "changed_files");
            changed_files.sort();
            changed_files.dedup();
            let archive_files = event_string_array(value, "archive_files");
            if changed_files.is_empty() {
                return Err("memory.commit 缺少 changed_files".to_string());
            }

            let tasks_path = dir.join(TASKS_FILE);
            let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
            let member_indexes = tasks
                .iter()
                .enumerate()
                .filter(|(_, task)| {
                    task.kind == "consultation" && consultation_group_id(task) == consultation_id
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if member_indexes.is_empty() {
                return Err(format!(
                    "memory.commit 找不到 consultation_id: {}",
                    consultation_id
                ));
            }
            if member_indexes
                .iter()
                .any(|index| !tasks[*index].web_reviewed)
            {
                return Err("memory.commit 拒绝提前提交：consult.reviewed 尚未完成".to_string());
            }

            let reviewed_at = member_indexes
                .iter()
                .filter_map(|index| {
                    chrono::DateTime::parse_from_rfc3339(&tasks[*index].updated_at).ok()
                })
                .map(|time| time.timestamp_millis())
                .max()
                .ok_or_else(|| "memory.commit 无法解析 web review 时间".to_string())?;
            let config = load_snapshot_unlocked(state, project_id)?.config;
            let memory_dir = project_memory_dir(&config);
            validate_memory_commit_files(&memory_dir, &changed_files, reviewed_at)?;
            validate_memory_archive_files(&memory_dir, &archive_files)?;
            append_memory_ledger(
                &config,
                serde_json::json!({
                    "kind": "memory.commit",
                    "consultation_id": consultation_id.clone(),
                    "changed_files": changed_files.clone(),
                    "archive_files": archive_files,
                    "author": author.clone(),
                }),
            )?;

            for index in member_indexes {
                tasks[index].memory_committed = true;
                tasks[index].cleanup_committed = false;
                tasks[index].updated_at = now.clone();
            }
            write_json(&tasks_path, &tasks)?;

            let discussion_path = dir.join(DISCUSSION_FILE);
            let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&discussion_path)?;
            let thread_id = tasks
                .iter()
                .find(|task| {
                    task.kind == "consultation" && consultation_group_id(task) == consultation_id
                })
                .map(|task| task.thread_id.clone())
                .unwrap_or_else(|| "memory-commit".to_string());
            messages.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id,
                    author,
                    recipients: vec!["all".to_string()],
                    message: format!(
                        "Canonical memory committed for {}: {}",
                        consultation_id,
                        changed_files.join(", ")
                    ),
                    created_at: now,
                },
            );
            if messages.len() > 5_000 {
                messages.truncate(5_000);
            }
            write_json(&discussion_path, &messages)?;
        }
        "cleanup.commit" => {
            if author != "chatgpt" {
                return Err("cleanup.commit 只能由 ChatGPT Web 最终协调者提交".to_string());
            }
            let consultation_id = event_string(value, "consultation_id");
            if consultation_id.is_empty() {
                return Err("cleanup.commit 缺少 consultation_id".to_string());
            }
            let summary = event_string(value, "summary");
            if summary.is_empty() {
                return Err("cleanup.commit 缺少 summary".to_string());
            }
            let checked_paths = event_string_array(value, "checked_paths");
            let removed_paths = event_string_array(value, "removed_paths");
            let archived_paths = event_string_array(value, "archived_paths");
            let retained_paths = event_string_array(value, "retained_paths");
            let reviewed_categories = event_string_array(value, "reviewed_categories");

            let tasks_path = dir.join(TASKS_FILE);
            let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
            let member_indexes = tasks
                .iter()
                .enumerate()
                .filter(|(_, task)| {
                    task.kind == "consultation" && consultation_group_id(task) == consultation_id
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if member_indexes.is_empty() {
                return Err(format!(
                    "cleanup.commit 找不到 consultation_id: {}",
                    consultation_id
                ));
            }
            if member_indexes
                .iter()
                .any(|index| !tasks[*index].memory_committed)
            {
                return Err("cleanup.commit 拒绝提前提交：memory.commit 尚未完成".to_string());
            }

            let config = load_snapshot_unlocked(state, project_id)?.config;
            let memory_health = refresh_memory_status(&config)?;
            if memory_health.requires_compaction {
                let over_budget = memory_health
                    .files
                    .iter()
                    .filter(|file| file.status == "over_budget")
                    .map(|file| format!("{} ({}/{})", file.file, file.bytes, file.budget_bytes))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "cleanup.commit 拒绝 finalization：canonical memory 超预算，必须先 compact 到 archive 并重新 memory.commit。{}",
                    over_budget
                ));
            }
            let root = PathBuf::from(&config.local_root);
            let candidates = validate_cleanup_commit(
                &root,
                &checked_paths,
                &removed_paths,
                &archived_paths,
                &retained_paths,
                &reviewed_categories,
            )?;

            for index in member_indexes {
                tasks[index].cleanup_committed = true;
                tasks[index].updated_at = now.clone();
            }
            write_json(&tasks_path, &tasks)?;
            append_memory_ledger(
                &config,
                serde_json::json!({
                    "kind": "cleanup.commit",
                    "consultation_id": consultation_id.clone(),
                    "reviewed_categories": reviewed_categories.clone(),
                    "checked_paths": checked_paths.clone(),
                    "removed_paths": removed_paths.clone(),
                    "archived_paths": archived_paths.clone(),
                    "retained_paths": retained_paths.clone(),
                    "summary": summary.clone(),
                    "author": author.clone(),
                }),
            )?;

            let discussion_path = dir.join(DISCUSSION_FILE);
            let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&discussion_path)?;
            let thread_id = tasks
                .iter()
                .find(|task| {
                    task.kind == "consultation" && consultation_group_id(task) == consultation_id
                })
                .map(|task| task.thread_id.clone())
                .unwrap_or_else(|| "cleanup-commit".to_string());
            messages.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id,
                    author,
                    recipients: vec!["all".to_string()],
                    message: format!(
                        "Cleanup committed for {}. removed={}, archived={}, retained={}, scanned_candidates={}. {}",
                        consultation_id,
                        removed_paths.len(),
                        archived_paths.len(),
                        retained_paths.len(),
                        candidates.len(),
                        summary
                    ),
                    created_at: now,
                },
            );
            if messages.len() > 5_000 {
                messages.truncate(5_000);
            }
            write_json(&discussion_path, &messages)?;
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
            let consultation_id = next_id("CONSULTATION");
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
                        consultation_id: consultation_id.clone(),
                        web_reviewed: false,
                        memory_committed: false,
                        cleanup_committed: false,
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
                    consultation_id: event_string(value, "consultation_id"),
                    web_reviewed: false,
                    memory_committed: false,
                    cleanup_committed: false,
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

    let mut busy_agents = snapshot
        .runs
        .iter()
        .filter(|run| run.status == "running")
        .map(|run| run.agent_id.clone())
        .collect::<std::collections::HashSet<_>>();
    let mut dispatched = 0usize;
    for (task_id, owner, thread_id) in queued {
        if busy_agents.contains(&owner) {
            continue;
        }
        match dispatch_project_task_unlocked(state, project_id, &task_id, &owner) {
            Ok(_) => {
                busy_agents.insert(owner.clone());
                dispatched += 1;
            }
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

#[cfg(test)]
mod tests {
    use super::{consultation_barriers, validate_cleanup_commit, validate_memory_commit_files};
    use crate::models::{AgentRun, ProjectTask};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn task(id: &str, owner: &str, status: &str, web_reviewed: bool) -> ProjectTask {
        ProjectTask {
            id: id.to_string(),
            title: format!("Review · {owner}"),
            goal: "focused question".to_string(),
            owner: owner.to_string(),
            reviewers: vec!["chatgpt".to_string()],
            status: status.to_string(),
            write_scope: Vec::new(),
            summary: String::new(),
            kind: "consultation".to_string(),
            thread_id: "thread-1".to_string(),
            consultation_id: "CONSULTATION-1".to_string(),
            web_reviewed,
            memory_committed: false,
            cleanup_committed: false,
            auto_dispatch: true,
            created_at: "2026-10-01T22:00:00+08:00".to_string(),
            updated_at: "2026-10-01T22:00:00+08:00".to_string(),
        }
    }

    fn run(task_id: &str, agent_id: &str, output_path: String) -> AgentRun {
        AgentRun {
            id: format!("RUN-{agent_id}"),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            status: "completed".to_string(),
            pid: None,
            external_session_id: None,
            start_step: None,
            started_at: "2026-10-01T22:00:00+08:00".to_string(),
            finished_at: Some("2026-10-01T22:01:00+08:00".to_string()),
            prompt_path: String::new(),
            output_path,
            log_path: String::new(),
            error_path: String::new(),
            error_message: None,
        }
    }

    #[test]
    fn consultation_barrier_waits_for_all_handoffs_then_web_review() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tunneldock-consultation-{nonce}"));
        fs::create_dir_all(&root).expect("temp dir");
        let codex_path = root.join("codex.md");
        let gemini_path = root.join("gemini.md");
        fs::write(&codex_path, "codex answer").expect("codex handoff");

        let mut tasks = vec![
            task("TASK-CODEX", "codex", "review", false),
            task("TASK-GEMINI", "gemini", "active", false),
        ];
        let runs = vec![
            run(
                "TASK-CODEX",
                "codex",
                codex_path.to_string_lossy().to_string(),
            ),
            run(
                "TASK-GEMINI",
                "gemini",
                gemini_path.to_string_lossy().to_string(),
            ),
        ];

        let barriers = consultation_barriers(&tasks, &runs);
        assert_eq!(barriers[0]["state"], "waiting_workers");
        assert_eq!(barriers[0]["ready_for_review"], false);

        fs::write(&gemini_path, "gemini \u{FFFD}\u{FFFD}\u{FFFD} answer")
            .expect("corrupt gemini handoff");
        tasks[1].status = "review".to_string();
        let barriers = consultation_barriers(&tasks, &runs);
        assert_eq!(barriers[0]["state"], "invalid_handoff");
        assert_eq!(barriers[0]["ready_for_review"], false);
        assert_eq!(barriers[0]["invalid_handoffs"][0], "gemini");

        fs::write(&gemini_path, "gemini answer").expect("gemini handoff");
        let barriers = consultation_barriers(&tasks, &runs);
        assert_eq!(barriers[0]["state"], "awaiting_web_review");
        assert_eq!(barriers[0]["ready_for_review"], true);
        assert_eq!(barriers[0]["web_review_complete"], false);

        tasks[0].web_reviewed = true;
        tasks[1].web_reviewed = true;
        let barriers = consultation_barriers(&tasks, &runs);
        assert_eq!(barriers[0]["state"], "awaiting_memory_commit");
        assert_eq!(barriers[0]["web_review_complete"], true);
        assert_eq!(barriers[0]["memory_commit_complete"], false);

        tasks[0].memory_committed = true;
        tasks[1].memory_committed = true;
        let barriers = consultation_barriers(&tasks, &runs);
        assert_eq!(barriers[0]["state"], "awaiting_cleanup");
        assert_eq!(barriers[0]["memory_commit_complete"], true);
        assert_eq!(barriers[0]["cleanup_commit_complete"], false);

        tasks[0].cleanup_committed = true;
        tasks[1].cleanup_committed = true;
        let barriers = consultation_barriers(&tasks, &runs);
        assert_eq!(barriers[0]["state"], "finalized");
        assert_eq!(barriers[0]["cleanup_commit_complete"], true);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn memory_commit_requires_core_and_domain_files_updated_after_review() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tunneldock-memory-commit-{nonce}"));
        fs::create_dir_all(&root).expect("temp dir");
        let reviewed_at = chrono::Utc::now().timestamp_millis() - 1_000;

        for (name, content) in [
            ("PROJECT_STATE.md", "state"),
            ("SESSION_HANDOFF.md", "handoff"),
            ("MODEL_DESIGN.md", "model"),
        ] {
            fs::write(root.join(name), content).expect("write memory file");
        }

        let missing_domain = vec![
            "PROJECT_STATE.md".to_string(),
            "SESSION_HANDOFF.md".to_string(),
        ];
        assert!(validate_memory_commit_files(&root, &missing_domain, reviewed_at).is_err());

        let valid = vec![
            "PROJECT_STATE.md".to_string(),
            "SESSION_HANDOFF.md".to_string(),
            "MODEL_DESIGN.md".to_string(),
        ];
        validate_memory_commit_files(&root, &valid, reviewed_at)
            .expect("valid memory commit should pass");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn cleanup_commit_requires_all_categories_and_resolves_hygiene_candidates() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tunneldock-cleanup-{nonce}"));
        fs::create_dir_all(&root).expect("temp dir");
        let log = root.join("debug.log");
        fs::write(&log, "temporary log").expect("temp log");
        let checked = vec![".".to_string()];
        let categories = ["memory", "documents", "code", "logs", "scratch"]
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();

        assert!(validate_cleanup_commit(&root, &checked, &[], &[], &[], &categories,).is_err());

        let retained = vec!["debug.log".to_string()];
        validate_cleanup_commit(&root, &checked, &[], &[], &retained, &categories)
            .expect("explicit retained evidence should pass");

        fs::remove_file(&log).expect("remove log");
        let removed = vec!["debug.log".to_string()];
        validate_cleanup_commit(&root, &checked, &removed, &[], &[], &categories)
            .expect("removed stale log should pass");

        fs::remove_dir_all(root).expect("cleanup");
    }
}
