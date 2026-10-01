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
5. ChatGPT Web records `memory.commit` listing the canonical files actually updated. TunnelDock rejects finalization if the consultation review is incomplete or required memory files were not updated.
6. Only after the memory commit is accepted may the consultation be treated as finalized for the human decision flow.

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

## Hygiene
Keep one authoritative current implementation/document. Do not create V1/V2/V3, *_old, *_backup, *_new, *_final copies. Use Git when the project permits it; otherwise keep one current tree and record source hashes/config/evidence. Delete disposable scratch after conclusions are captured.
"#
}

pub(super) fn project_memory_dir(config: &ProjectRoomConfig) -> PathBuf {
    PathBuf::from(&config.local_root).join(PROJECT_MEMORY_DIR)
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

pub(super) fn save_project_memory(
    config: &ProjectRoomConfig,
    memory: &ProjectMemory,
) -> Result<(), String> {
    ensure_project_memory(config)?;
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
        fs::write(dir.join(file_name), content)
            .map_err(|error| format!("写入 project memory 失败: {}", error))?;
    }
    Ok(())
}
