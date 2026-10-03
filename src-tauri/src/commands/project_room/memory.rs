use super::*;

pub(super) fn default_research_goal(project_id: &str) -> &'static str {
    match project_id {
        "point_tracking" => {
            "推进 2D Point Tracking 研究，所有实现与实验服务于顶会论文的核心 hypothesis、方法验证和可复现性。"
        }
        "iqa_agent" => {
            "推进 IQA Agent 研究，保持 agent 设计、benchmark、实验记录与论文结论之间的一致性。"
        }
        "3d_mllm" => {
            "推进 3D + MLLM 空间推理研究，围绕几何表征、关系推理、数据与实验形成可验证的顶会论文证据链。"
        }
        _ => "推进当前科研项目，代码与实验均服务于可验证、可复现的论文结论。",
    }
}

fn default_memory_index(config: &ProjectRoomConfig) -> String {
    format!(
        r#"# {} — Research Memory Index

## Read order
1. `PROJECT_STATE.md` — current project truth and next actions.
2. `SESSION_HANDOFF.md` — latest continuation point.
3. Read only the domain files needed by the current task.
4. `DECISIONS.md` — durable decisions that constrain future work.
5. `MEMORY_PROTOCOL.md` — memory/review/finalization rules.

## Canonical domain files
- `DOCUMENTS.md` — authoritative project documents, ownership, status, and paths.
- `MODEL_DESIGN.md` — current method/model design, interfaces, losses, training/inference contracts, checkpoints, and unresolved design risks.
- `DATA_CATALOG.md` — datasets, versions, splits, provenance, preprocessing, cache locations, quality gates, and leakage constraints.
- `EXPERIMENTS.md` — reproducible experiment ledger and meaningful negative results.
- `RESULTS.md` — accepted evaluation results, protocols, tables/figures, and invalidated/superseded results.
- `REFERENCES.md` — papers, repositories, tools, citation keys, and exactly which project claims/design choices each source supports.

## Storage rule
This directory stores decisions, current truth, and indexes — not large assets. Datasets, checkpoints, videos, figures, raw logs, PDFs, and generated artifacts stay in stable project/native locations. Record their path, version/hash, provenance, and evidence status here instead of copying them into `.project_memory`.

## Memory lifecycle
- When automatic compaction is enabled, an over-budget file may begin with a `TunnelDock required-continuation` notice. Its hash-addressed archive and recursive manifest are REQUIRED canonical continuation, not obsolete facts. Expand every referenced original before decisive review or changing behavior. Context search includes these continuations; the first page alone is incomplete.
- Root canonical Markdown files are the bounded **current materialized view**. They contain only facts/constraints/evidence that are useful now.
- `archive/` keeps superseded or cold research history that still has scientific value. Archive history may grow; it is never injected by default.
- `ledger/YYYY-MM.jsonl` is an append-only audit/event stream for memory commits, decisions, compaction and cleanup milestones.
- `MEMORY_STATUS.json` is system-generated health telemetry with byte budgets and compaction pressure.
- Read progressively: current state -> relevant domain file -> archive/ledger only when the current task needs historical evidence.

## Automatic retrieval working set
TunnelDock independently refreshes `.tunneldock/context/current.json` and a versioned working-set Markdown (at most 12 KiB), without rewriting these canonical files. It preserves full source objects by SHA-256 and publishes coverage/missing-file information. Use `tunneldock_context` search/read/impact when available, or the native Project Room search UI, before expanding large files. The working set is incomplete and NOT canonical truth; reread the original constraint, result or evidence before final review. Empty lexical impact candidates never justify deleting code. Semantic memory consolidation remains a Web-reviewed change; cold snapshots are not automatically discarded.

## Review rule
Codex and Antigravity/Gemini may inspect and challenge any domain document. Their reviews are evidence, not final truth. ChatGPT Web is the final reviewer: it must wait for required worker reviews, inspect decisive source/code itself, update the relevant canonical memory documents, and only then mark the review finalized.

## Default review coverage
| Domain | Codex review focus | Antigravity/Gemini review focus | Final authority |
| --- | --- | --- | --- |
| Model | implementation contract, interfaces, losses, reproducibility | design alternatives, failure cases, qualitative/visual behavior | ChatGPT Web + human decision |
| Data | schema, leakage, provenance, split/cache correctness | visual quality, outliers, label credibility, coverage | ChatGPT Web + human decision |
| Experiments | command/config correctness, metric implementation, ablation validity | plots, qualitative evidence, failure-pattern inspection | ChatGPT Web |
| Results | numerical integrity, baseline comparability, protocol | figure/table/visual interpretation, case selection | ChatGPT Web |
| References | source/version/citation correctness, dependency facts | relevance/coverage/competing evidence | ChatGPT Web |
| Documents | technical consistency and stale claims | clarity/visual communication and missing context | ChatGPT Web |
"#,
        config.name
    )
}

fn default_model_design() -> String {
    r#"# Model Design

## Research hypothesis

## Current architecture

## Inputs / outputs

## Component contracts

## Training objectives / losses

## Training schedule

## Inference path

## Checkpoints / weights
| Name | Path / ID | Code/config revision | Data | Status | Notes |
| --- | --- | --- | --- | --- | --- |

## Required ablations

## Known failure modes / unresolved design risks

## Review notes
Record only durable findings that changed the accepted design. Full worker reviews stay in Project Room handoffs/discussion.
"#.to_string()
}

fn default_data_catalog() -> String {
    r#"# Data Catalog

## Data contract
State what counts as an input, target, pseudo-label, mask, confidence/provenance field, and what must never leak into student inputs.

## Dataset registry
| Dataset / source | Version / hash | Split | Canonical path | Provenance | Geometry/label source | Status |
| --- | --- | --- | --- | --- | --- | --- |

## Preprocessing / cache contract

## Reuse keys / identity rules
Define how scenes, videos, observations, queries, and derived caches are keyed so repeated samples do not silently duplicate work or leak targets.

## Quality gates

## Exclusions / known contamination risks

## Data review notes
"#.to_string()
}

fn default_experiments() -> String {
    r#"# Experiment Ledger

Keep one entry per meaningful experiment or engineering verification. Raw logs stay outside `.project_memory`; link only the evidence needed to reproduce or audit the conclusion.

## Entry template
### EXP-<id> — <short title>
- Status: planned / running / supported / rejected / inconclusive
- Hypothesis:
- Code revision / source hash:
- Model/config:
- Dataset/split/version:
- Exact command / entry point:
- Seed / hardware when material:
- Metrics:
- Result:
- Interpretation:
- Artifacts / result paths:
- Failure notes / negative result:
- Reviewer notes:

## Verified experiments

## Meaningful negative results
"#.to_string()
}

fn default_results() -> String {
    r#"# Results

## Evaluation contract
Define benchmark version, split, metric implementation, aggregation, baseline compatibility, and any exclusion rule before listing numbers.

## Accepted headline results
| Result ID | Method/checkpoint | Dataset/split | Metrics | Evidence path | Status |
| --- | --- | --- | --- | --- | --- |

## Baselines

## Ablations

## Qualitative / visual evidence

## Figures / tables ready for paper

## Invalidated / superseded results
Keep a concise reason and evidence pointer so bad numbers are not accidentally reused.

## Open evaluation gaps
"#.to_string()
}

fn default_references() -> String {
    r#"# References

Store citation identity and project relevance, not copied paper text.

## Literature / repository registry
| Key | Type | Title / repository | Source / URL / DOI | Version / commit | Project relevance | Claims/design supported | Notes path |
| --- | --- | --- | --- | --- | --- | --- | --- |

## Must-cite related work

## Method / implementation dependencies

## Benchmark / dataset references

## Open reading questions
"#.to_string()
}

fn default_documents() -> String {
    r#"# Document Map

This file says which document is authoritative for each topic. Do not keep multiple documents claiming to be the current design.

| Topic | Canonical document/path | Status | Owner/reviewer | Replaces / archive note |
| --- | --- | --- | --- | --- |
| Current project state | `.project_memory/PROJECT_STATE.md` | canonical | ChatGPT Web | living snapshot |
| Session continuation | `.project_memory/SESSION_HANDOFF.md` | canonical | ChatGPT Web | replace each meaningful handoff |
| Model design | `.project_memory/MODEL_DESIGN.md` | canonical | Web + Codex/Gemini review | living design |
| Data contract/catalog | `.project_memory/DATA_CATALOG.md` | canonical | Web + Codex/Gemini review | living data truth |
| Experiments | `.project_memory/EXPERIMENTS.md` | canonical index | Web | details stay in result artifacts |
| Results | `.project_memory/RESULTS.md` | canonical | Web + reviewer evidence | accepted numbers only |
| References | `.project_memory/REFERENCES.md` | canonical | Web + reviewer evidence | citation registry |

## Project-native documents
Add important README/AGENTS/paper/design/spec paths here with `draft / canonical / archived / generated` status.

## Archive policy
Superseded documents that still have scientific value go to the project's own archive location and must stop claiming to be current. Disposable drafts/duplicate exports are deleted rather than archived.
"#.to_string()
}

pub(super) fn default_memory_protocol() -> &'static str {
    r#"# Project Memory Protocol

## Source of truth
`.project_memory/` is the durable research memory. Chat/Pi/Codex/Antigravity conversation history is working context only. Large datasets, checkpoints, raw logs, media, and PDFs remain in their canonical project locations and are referenced by path/hash/provenance.

Memory is split by lifecycle rather than allowed to grow as one prompt:
- **Current / hot:** root canonical Markdown files, bounded by soft byte budgets and optimized for the next decisions.
- **Archive / cold:** `archive/`, containing superseded/history material that remains auditable but is not read by default.
- **Event ledger:** `ledger/YYYY-MM.jsonl`, append-only lifecycle/audit events.
- **Health:** `MEMORY_STATUS.json`, generated by TunnelDock. If a canonical file exceeds its budget, compaction is required before finalization.

## Canonical memory
- `MEMORY_INDEX.md`: read order and memory map.
- `PROJECT_STATE.md`: compact current truth; stale statements are replaced, not appended forever.
- `SESSION_HANDOFF.md`: latest continuation point; replace after meaningful work.
- `DECISIONS.md`: accepted decisions that constrain future work.
- `DOCUMENTS.md`: canonical document registry and archive status.
- `MODEL_DESIGN.md`: accepted model/method design and unresolved design risks.
- `DATA_CATALOG.md`: datasets, provenance, versions, splits, preprocessing, quality/leakage contract.
- `EXPERIMENTS.md`: reproducible experiment/evidence ledger, including meaningful negative results.
- `RESULTS.md`: accepted evaluation contract/results/figures plus invalidated results.
- `REFERENCES.md`: citation/source registry tied to concrete project claims/design choices.

## Review workflow
1. Codex and/or Antigravity/Gemini review the requested artifact/question independently through Project Room consultation.
2. TunnelDock waits for all requested workers and valid handoffs; partial results do not open the review gate.
3. ChatGPT Web reads every valid worker handoff in full, inspects decisive code/source/evidence itself, and records `consult.reviewed`.
4. ChatGPT Web updates `PROJECT_STATE.md`, `SESSION_HANDOFF.md`, and every affected domain memory file.
5. ChatGPT Web records `memory.commit` listing the canonical files actually updated. TunnelDock rejects it if the review is incomplete or required memory files were not updated.
6. ChatGPT Web performs a final hygiene/integration pass across memory, documents, code, logs, and scratch. Superseded content is deleted or archived; retained logs/artifacts must be unique evidence.
7. ChatGPT Web records `cleanup.commit`. TunnelDock scans the Project Room and rejects unresolved stale/log/tmp candidates unless they are explicitly archived or retained.
8. Only after `cleanup.commit` is accepted may the consultation state become `finalized` for the human decision flow.

## Domain update rules
- Model/method change -> update `MODEL_DESIGN.md` + state/handoff.
- Dataset/label/cache/split/provenance change -> update `DATA_CATALOG.md` + state/handoff.
- Experiment run or important negative result -> update `EXPERIMENTS.md`; update `RESULTS.md` only when evidence is accepted.
- Accepted metric/table/figure/evaluation change -> update `RESULTS.md` + state/handoff.
- New paper/repo/tool materially used -> update `REFERENCES.md`.
- Canonical doc ownership/status/path changes -> update `DOCUMENTS.md`.
- Human decision that changes future behavior -> update `DECISIONS.md` via `decision.record`.

## Evidence rules
Separate measured facts from interpretation. Every durable number should point to a reproducible result artifact. Every model/data claim should point to code/config/data/source evidence when practical. Do not copy raw worker handoffs or raw logs into memory.

## Compaction and forgetting
- Never solve growth by blindly truncating current memory.
- When `MEMORY_STATUS.json` marks a file `over_budget`, move superseded/history detail to the matching `archive/<domain>/` file, preserve evidence pointers, and rewrite the canonical file as current truth only.
- Recent unresolved work stays hot; settled historical detail becomes cold. Superseded facts are not returned by default retrieval.
- Session/run/raw handoff detail is working history, not canonical truth. Distill the durable lesson/evidence first, then allow cleanup/retention policy to remove the raw copy.
- Archive growth is acceptable because it is retrieved on demand; canonical growth is not.

## Hygiene
Cleanup is part of finishing work, not a future chore. Keep one authoritative current implementation/document; merge stale/current docs instead of leaving competing truth. Remove superseded code, abandoned scripts, temp/debug outputs, duplicate exports, stale caches, and disposable `.log/.tmp/.trace` files after replacement/evidence is verified. Retain a log only when it contains unique reproducibility/debug evidence and record why it is retained. Scientifically useful superseded material goes to the project's explicit archive and must stop claiming to be current. Do not create V1/V2/V3, *_old, *_backup, *_new, *_final copies. Use Git when permitted; otherwise keep one current tree and record source hashes/config/evidence.
"#
}

pub(super) fn project_memory_dir(config: &ProjectRoomConfig) -> PathBuf {
    PathBuf::from(&config.local_root).join(PROJECT_MEMORY_DIR)
}

fn memory_budget_bytes(file_name: &str) -> u64 {
    match file_name {
        PROJECT_STATE_FILE => 16 * 1024,
        SESSION_HANDOFF_FILE => 12 * 1024,
        DECISIONS_FILE => 32 * 1024,
        MODEL_DESIGN_FILE => 32 * 1024,
        DATA_CATALOG_FILE => 32 * 1024,
        MEMORY_EXPERIMENTS_FILE => 48 * 1024,
        RESULTS_FILE => 32 * 1024,
        REFERENCES_FILE => 32 * 1024,
        DOCUMENTS_FILE => 24 * 1024,
        _ => 16 * 1024,
    }
}

fn canonical_memory_files() -> [&'static str; 9] {
    [
        PROJECT_STATE_FILE,
        SESSION_HANDOFF_FILE,
        DECISIONS_FILE,
        MODEL_DESIGN_FILE,
        DATA_CATALOG_FILE,
        MEMORY_EXPERIMENTS_FILE,
        RESULTS_FILE,
        REFERENCES_FILE,
        DOCUMENTS_FILE,
    ]
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                dir_size(&path)
            } else {
                entry.metadata().map(|meta| meta.len()).unwrap_or_default()
            }
        })
        .sum()
}

fn ledger_event_count(path: &Path) -> usize {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("jsonl"))
        .map(|entry| {
            fs::read_to_string(entry.path())
                .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count())
                .unwrap_or_default()
        })
        .sum()
}

fn compute_memory_health_from_dir(dir: &Path, ledger_events: usize) -> MemoryHealth {
    let mut total_current_bytes = 0u64;
    let mut requires_compaction = false;
    let mut near_budget = false;
    let files = canonical_memory_files()
        .into_iter()
        .map(|file_name| {
            let bytes = fs::metadata(dir.join(file_name))
                .map(|meta| meta.len())
                .unwrap_or_default();
            let budget_bytes = memory_budget_bytes(file_name);
            let utilization = if budget_bytes == 0 {
                0.0
            } else {
                bytes as f64 / budget_bytes as f64
            };
            let status = if utilization > 1.0 {
                requires_compaction = true;
                "over_budget"
            } else if utilization >= 0.8 {
                near_budget = true;
                "near_budget"
            } else {
                "healthy"
            };
            total_current_bytes += bytes;
            MemoryFileHealth {
                file: file_name.to_string(),
                bytes,
                budget_bytes,
                utilization,
                status: status.to_string(),
            }
        })
        .collect::<Vec<_>>();

    MemoryHealth {
        total_current_bytes,
        total_archive_bytes: dir_size(&dir.join(MEMORY_ARCHIVE_DIR)),
        ledger_events,
        requires_compaction,
        near_budget,
        files,
        updated_at: local_now_rfc3339(),
    }
}

fn health_metrics_equal(left: &MemoryHealth, right: &MemoryHealth) -> bool {
    left.total_current_bytes == right.total_current_bytes
        && left.total_archive_bytes == right.total_archive_bytes
        && left.ledger_events == right.ledger_events
        && left.requires_compaction == right.requires_compaction
        && left.near_budget == right.near_budget
        && left.files == right.files
}

fn write_memory_status(dir: &Path, mut health: MemoryHealth) -> Result<MemoryHealth, String> {
    let status_path = dir.join(MEMORY_STATUS_FILE);
    if let Ok(previous_text) = fs::read_to_string(&status_path) {
        if let Ok(previous) = serde_json::from_str::<MemoryHealth>(&previous_text) {
            if health_metrics_equal(&previous, &health) {
                health.updated_at = previous.updated_at;
                return Ok(health);
            }
        }
    }
    write_if_changed(
        &status_path,
        &serde_json::to_string_pretty(&health)
            .map_err(|error| format!("序列化 memory status 失败: {error}"))?,
    )?;
    Ok(health)
}

pub(super) fn refresh_memory_status(config: &ProjectRoomConfig) -> Result<MemoryHealth, String> {
    let dir = project_memory_dir(config);
    let status_path = dir.join(MEMORY_STATUS_FILE);
    let ledger_events = fs::read_to_string(&status_path)
        .ok()
        .and_then(|text| serde_json::from_str::<MemoryHealth>(&text).ok())
        .map(|health| health.ledger_events)
        .unwrap_or_else(|| ledger_event_count(&dir.join(MEMORY_LEDGER_DIR)));
    write_memory_status(&dir, compute_memory_health_from_dir(&dir, ledger_events))
}

pub(super) fn append_memory_ledger(
    config: &ProjectRoomConfig,
    mut event: serde_json::Value,
) -> Result<(), String> {
    ensure_project_memory(config)?;
    let dir = project_memory_dir(config).join(MEMORY_LEDGER_DIR);
    fs::create_dir_all(&dir).map_err(|error| format!("创建 {} 失败: {}", dir.display(), error))?;
    if let Some(object) = event.as_object_mut() {
        object
            .entry("recorded_at".to_string())
            .or_insert_with(|| serde_json::Value::String(local_now_rfc3339()));
        object
            .entry("project_id".to_string())
            .or_insert_with(|| serde_json::Value::String(config.id.clone()));
    }
    let file_name = format!("{}.jsonl", chrono::Local::now().format("%Y-%m"));
    let path = dir.join(file_name);
    let line = serde_json::to_string(&event)
        .map_err(|error| format!("序列化 memory ledger event 失败: {error}"))?;
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("打开 {} 失败: {}", path.display(), error))?;
    writeln!(file, "{line}").map_err(|error| format!("写入 {} 失败: {}", path.display(), error))?;
    let status_path = project_memory_dir(config).join(MEMORY_STATUS_FILE);
    let previous_events = fs::read_to_string(&status_path)
        .ok()
        .and_then(|text| serde_json::from_str::<MemoryHealth>(&text).ok())
        .map(|health| health.ledger_events)
        .unwrap_or_else(|| ledger_event_count(&dir).saturating_sub(1));
    let memory_dir = project_memory_dir(config);
    let _ = write_memory_status(
        &memory_dir,
        compute_memory_health_from_dir(&memory_dir, previous_events.saturating_add(1)),
    )?;
    Ok(())
}

fn write_if_changed(path: &Path, content: &str) -> Result<(), String> {
    if fs::read_to_string(path).ok().as_deref() == Some(content) {
        return Ok(());
    }
    fs::write(path, content).map_err(|error| format!("写入 {} 失败: {}", path.display(), error))
}

pub(super) fn ensure_project_memory(config: &ProjectRoomConfig) -> Result<(), String> {
    let dir = project_memory_dir(config);
    fs::create_dir_all(&dir).map_err(|error| format!("创建 {} 失败: {}", dir.display(), error))?;
    for path in [
        dir.join(MEMORY_ARCHIVE_DIR),
        dir.join(MEMORY_ARCHIVE_DIR).join("decisions"),
        dir.join(MEMORY_ARCHIVE_DIR).join("model"),
        dir.join(MEMORY_ARCHIVE_DIR).join("data"),
        dir.join(MEMORY_ARCHIVE_DIR).join("experiments"),
        dir.join(MEMORY_ARCHIVE_DIR).join("results"),
        dir.join(MEMORY_ARCHIVE_DIR).join("references"),
        dir.join(MEMORY_ARCHIVE_DIR).join("documents"),
        dir.join(MEMORY_ARCHIVE_DIR).join("sessions"),
        dir.join(MEMORY_LEDGER_DIR),
    ] {
        fs::create_dir_all(&path)
            .map_err(|error| format!("创建 {} 失败: {}", path.display(), error))?;
    }

    let state_path = dir.join(PROJECT_STATE_FILE);
    if !state_path.exists() {
        let content = format!(
            "# {} — Project State\n\nLast updated: {}\n\n## Goal\n{}\n\n## Current Method\n\n## Current Data\n\n## Current Evidence / Results\n\n## Known Problems\n\n## Active Reviews / Decisions\n\n## Next Actions\n\n## Canonical detail pointers\n- Model: MODEL_DESIGN.md\n- Data: DATA_CATALOG.md\n- Experiments: EXPERIMENTS.md\n- Results: RESULTS.md\n- References: REFERENCES.md\n- Documents: DOCUMENTS.md\n",
            config.name,
            local_now_rfc3339(),
            default_research_goal(&config.id)
        );
        fs::write(&state_path, content)
            .map_err(|error| format!("写入 {} 失败: {}", state_path.display(), error))?;
    }

    // MEMORY_INDEX / MEMORY_PROTOCOL are system-managed contracts. Refresh them
    // in place so every Project Room sees the current organization/review rules.
    write_if_changed(&dir.join(MEMORY_INDEX_FILE), &default_memory_index(config))?;
    write_if_changed(&dir.join(MEMORY_PROTOCOL_FILE), default_memory_protocol())?;

    for (file_name, content) in [
        (
            SESSION_HANDOFF_FILE,
            format!(
                "# Session Handoff\n\nUpdated: {}\n\n## Read first\n- PROJECT_STATE.md\n- MEMORY_INDEX.md\n\n## What changed\n\n## Evidence / tests\n\n## Pending reviews / decisions\n\n## Exact next action\n",
                local_now_rfc3339()
            ),
        ),
        (
            DECISIONS_FILE,
            "# Durable Decisions\n\nNo durable decisions recorded yet.\n".to_string(),
        ),
        (MODEL_DESIGN_FILE, default_model_design()),
        (DATA_CATALOG_FILE, default_data_catalog()),
        (MEMORY_EXPERIMENTS_FILE, default_experiments()),
        (RESULTS_FILE, default_results()),
        (REFERENCES_FILE, default_references()),
        (DOCUMENTS_FILE, default_documents()),
    ] {
        let path = dir.join(file_name);
        if !path.exists() {
            fs::write(&path, content)
                .map_err(|error| format!("写入 {} 失败: {}", path.display(), error))?;
        }
    }

    let _ = refresh_memory_status(config)?;
    Ok(())
}

pub(super) fn read_memory_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("读取 {} 失败: {}", path.display(), error))
}

pub(super) fn project_memory_updated_at(dir: &Path) -> String {
    let mut newest = None;
    for file_name in [
        MEMORY_INDEX_FILE,
        PROJECT_STATE_FILE,
        SESSION_HANDOFF_FILE,
        DECISIONS_FILE,
        MODEL_DESIGN_FILE,
        DATA_CATALOG_FILE,
        MEMORY_EXPERIMENTS_FILE,
        RESULTS_FILE,
        REFERENCES_FILE,
        DOCUMENTS_FILE,
        MEMORY_PROTOCOL_FILE,
    ] {
        if let Ok(modified) = fs::metadata(dir.join(file_name)).and_then(|meta| meta.modified()) {
            if newest.map(|current| modified > current).unwrap_or(true) {
                newest = Some(modified);
            }
        }
    }

    newest
        .map(|time| {
            let dt: chrono::DateTime<chrono::Local> = time.into();
            dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
        })
        .unwrap_or_else(local_now_rfc3339)
}

pub(super) fn load_project_memory(config: &ProjectRoomConfig) -> Result<ProjectMemory, String> {
    ensure_project_memory(config)?;
    let dir = project_memory_dir(config);
    Ok(ProjectMemory {
        memory_index: read_memory_text(&dir.join(MEMORY_INDEX_FILE))?,
        project_state: read_memory_text(&dir.join(PROJECT_STATE_FILE))?,
        session_handoff: read_memory_text(&dir.join(SESSION_HANDOFF_FILE))?,
        decisions: read_memory_text(&dir.join(DECISIONS_FILE))?,
        model_design: read_memory_text(&dir.join(MODEL_DESIGN_FILE))?,
        data_catalog: read_memory_text(&dir.join(DATA_CATALOG_FILE))?,
        experiments: read_memory_text(&dir.join(MEMORY_EXPERIMENTS_FILE))?,
        results: read_memory_text(&dir.join(RESULTS_FILE))?,
        references: read_memory_text(&dir.join(REFERENCES_FILE))?,
        documents: read_memory_text(&dir.join(DOCUMENTS_FILE))?,
        memory_protocol: read_memory_text(&dir.join(MEMORY_PROTOCOL_FILE))?,
        updated_at: project_memory_updated_at(&dir),
    })
}

struct CanonicalWriteLease {
    path: PathBuf,
    token: String,
}
impl CanonicalWriteLease {
    fn acquire(root: &Path) -> Result<Self, String> {
        let path = root.join(".tunneldock/memory-compaction.lock");
        fs::create_dir_all(path.parent().ok_or("Missing lease parent")?)
            .map_err(|e| e.to_string())?;
        let token = format!(
            "web-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| {
                format!("Canonical memory is busy; retry after compaction/writer completes: {e}")
            })?;
        use std::io::Write;
        let value = serde_json::json!({"pid":std::process::id(),"token":token});
        file.write_all(value.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(Self { path, token })
    }
}
impl Drop for CanonicalWriteLease {
    fn drop(&mut self) {
        let owned = fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v["token"].as_str().map(str::to_owned))
            .as_deref()
            == Some(&self.token);
        if owned {
            if let Err(error) = fs::remove_file(&self.path) {
                eprintln!("Memory lease cleanup failed: {error}");
            }
        }
    }
}

pub(super) fn save_project_memory(
    config: &ProjectRoomConfig,
    memory: &ProjectMemory,
) -> Result<(), String> {
    ensure_project_memory(config)?;
    let _lease = CanonicalWriteLease::acquire(Path::new(&config.local_root))?;
    let dir = project_memory_dir(config);
    for (file_name, content) in [
        (PROJECT_STATE_FILE, &memory.project_state),
        (SESSION_HANDOFF_FILE, &memory.session_handoff),
        (DECISIONS_FILE, &memory.decisions),
        (MODEL_DESIGN_FILE, &memory.model_design),
        (DATA_CATALOG_FILE, &memory.data_catalog),
        (MEMORY_EXPERIMENTS_FILE, &memory.experiments),
        (RESULTS_FILE, &memory.results),
        (REFERENCES_FILE, &memory.references),
        (DOCUMENTS_FILE, &memory.documents),
    ] {
        let path = dir.join(file_name);
        let current = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if current.starts_with("<!-- TunnelDock required-continuation:")
            && !content.starts_with(current.lines().next().unwrap_or_default())
        {
            return Err(format!(
                "{} was compacted: reload before saving; required continuation must not be dropped",
                file_name
            ));
        }
        fs::write(path, content).map_err(|error| format!("写入 project memory 失败: {}", error))?;
    }
    let _ = refresh_memory_status(config)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{compute_memory_health_from_dir, write_memory_status, PROJECT_STATE_FILE};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn canonical_writer_respects_compactor_exclusive_lease() {
        let root = std::env::temp_dir().join(format!(
            "td-memory-lease-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        fs::create_dir_all(&root).expect("fixture");
        let first = super::CanonicalWriteLease::acquire(&root).expect("first writer");
        assert!(super::CanonicalWriteLease::acquire(&root).is_err());
        drop(first);
        let second = super::CanonicalWriteLease::acquire(&root).expect("released writer");
        drop(second);
        assert!(!root.join(".tunneldock/memory-compaction.lock").exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn memory_health_marks_over_budget_current_view() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tunneldock-memory-health-{nonce}"));
        fs::create_dir_all(&root).expect("temp dir");
        fs::write(root.join(PROJECT_STATE_FILE), vec![b'x'; 16 * 1024 + 1])
            .expect("write oversized memory");

        let health = compute_memory_health_from_dir(&root, 7);
        assert!(health.requires_compaction);
        assert_eq!(health.ledger_events, 7);
        let state = health
            .files
            .iter()
            .find(|file| file.file == PROJECT_STATE_FILE)
            .expect("project state health");
        assert_eq!(state.status, "over_budget");
        assert!(state.utilization > 1.0);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn memory_status_timestamp_is_stable_when_metrics_do_not_change() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tunneldock-memory-status-{nonce}"));
        fs::create_dir_all(&root).expect("temp dir");
        fs::write(root.join(PROJECT_STATE_FILE), "state").expect("write state");

        let first = write_memory_status(&root, compute_memory_health_from_dir(&root, 0))
            .expect("first status");
        let mut same_metrics = compute_memory_health_from_dir(&root, 0);
        same_metrics.updated_at = "2099-01-01T00:00:00+00:00".to_string();
        let second = write_memory_status(&root, same_metrics).expect("second status");
        assert_eq!(first.updated_at, second.updated_at);

        fs::remove_dir_all(root).expect("cleanup");
    }
}
