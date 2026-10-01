# Project Room Persistent Research Memory

TunnelDock Project Room uses a two-layer design:

1. **Canonical research memory** under `<project>/.project_memory/` stores current truth, decisions, evidence indexes, and pointers.
2. **Project-native assets** stay where they belong: datasets, checkpoints, code, videos, figures, PDFs, raw logs, and result artifacts are not copied into memory. Canonical memory records their path, version/hash, provenance, and evidence status.

The goal is to let ChatGPT Web, Codex, and Antigravity/Gemini review the same project state without turning chat history into the database.

## Canonical memory layout

```text
.project_memory/
├── MEMORY_INDEX.md       # generated memory map + review coverage
├── PROJECT_STATE.md      # compact current truth / active problems / next actions
├── SESSION_HANDOFF.md    # latest continuation point
├── DECISIONS.md          # accepted decisions that constrain future work
├── MODEL_DESIGN.md       # method/model architecture and training/inference contract
├── DATA_CATALOG.md       # datasets, versions, splits, provenance, preprocessing, leakage contract
├── EXPERIMENTS.md        # reproducible experiment ledger + meaningful negative results
├── RESULTS.md            # accepted evaluation contract, numbers, figures/tables, invalidated results
├── REFERENCES.md         # papers/repos/tools tied to concrete project claims/design choices
├── DOCUMENTS.md          # authoritative project-document map and archive status
└── MEMORY_PROTOCOL.md    # generated storage/review/finalization rules
```

`MEMORY_INDEX.md` and `MEMORY_PROTOCOL.md` are TunnelDock-managed contracts and are refreshed automatically. Existing project-authored canonical files are never overwritten when the layout is introduced; missing domain files are created from templates.

## Responsibility of each file

### PROJECT_STATE.md
Small, current, decision-oriented. It answers: What are we building, what is currently true, what evidence is accepted, what is blocked, what happens next? It should point to domain files instead of duplicating them.

### SESSION_HANDOFF.md
Replace after a meaningful unit of work. It contains only what the next session needs: what changed, evidence/tests, pending review/decision, and the exact next action.

### MODEL_DESIGN.md
Current research hypothesis, architecture, component contracts, inputs/outputs, losses, training schedule, inference path, checkpoints/weights, required ablations, failure modes, and unresolved design risks.

### DATA_CATALOG.md
Data contract plus dataset/source registry. Record version/hash, split, canonical location, provenance, label/geometry source, preprocessing/cache rules, quality gates, identity/reuse keys, exclusions, and leakage constraints.

### EXPERIMENTS.md
One entry per meaningful experiment or engineering verification. Record hypothesis, code/source revision, config, dataset version, exact entry point/command, seed/hardware when material, metrics, result, interpretation, artifacts, and negative evidence. Raw logs remain outside memory.

### RESULTS.md
Only accepted results. Freeze evaluation protocol before numbers: benchmark version, split, metric implementation, aggregation, baseline compatibility, exclusions. Separate headline results, baselines, ablations, qualitative evidence, paper-ready figures/tables, and invalidated/superseded numbers.

### REFERENCES.md
Citation/source registry rather than copied paper text. Store citation key, paper/repository/tool identity, source/URL/DOI, version/commit, relevance, and exactly which project claims/design choices it supports.

### DOCUMENTS.md
Maps a topic to its one authoritative document. Project-native design docs, READMEs, paper drafts, specs, and archives are listed with `canonical / draft / generated / archived` status so two files cannot both claim to be the current design.

### DECISIONS.md
Only accepted durable decisions that change future behavior. A proposal or worker recommendation is not a decision until the human researcher explicitly decides.

## Organizing data, experiments, results, models, and references

TunnelDock does not impose a new physical asset tree on an existing research repository. Existing canonical locations remain valid. New work should follow these invariants:

- **Data:** one canonical source/cache location per version; memory stores provenance, hashes, splits, and contracts. Temporary local inspection copies are deleted after verification.
- **Experiments:** every meaningful run has a stable experiment identity and reproducible entry point/config. Large logs are reduced to concise evidence summaries plus artifact paths.
- **Results:** raw outputs remain with experiment artifacts; only accepted metrics/figures become entries in `RESULTS.md`.
- **Models/checkpoints:** weights stay in project/server storage. `MODEL_DESIGN.md` records checkpoint ID/path, code/config revision, training data version, and status.
- **References:** PDFs and code repositories stay in normal project/library locations. `REFERENCES.md` stores the citation identity and why it matters.
- **Documents:** there is one canonical current document per topic; superseded but scientifically useful docs move to the project's archive location, while disposable duplicates are deleted.

## Multi-agent review contract

All domains can be reviewed by all three agents, but their roles differ:

| Domain | Codex | Antigravity/Gemini | ChatGPT Web |
| --- | --- | --- | --- |
| Model | implementation/interface/loss correctness, reproducibility | alternative design, failure cases, qualitative behavior | inspect decisive source, reconcile evidence, final synthesis |
| Data | schema, leakage, provenance, split/cache correctness | visual quality, label credibility, outliers/coverage | decide accepted data contract |
| Experiments | command/config/metric/ablation validity | plots, qualitative evidence, failure-pattern review | accept/reject interpretation |
| Results | numerical integrity, baseline comparability | figure/table/case interpretation | decide which results become canonical |
| References | source/version/dependency correctness | relevance and competing evidence | decide how source supports project claims |
| Documents | stale/contradictory technical claims | clarity and missing visual/contextual evidence | resolve canonical document state |

Worker reviews are evidence only. ChatGPT Web remains the final reviewer before the human decision discussion.

## Consultation barrier and finalization

```text
consult.request
    ↓
wait for all requested workers
    ↓
validate complete + transport-safe handoffs
    ├─ invalid → consult.retry only affected agent(s)
    ↓
ready_for_review=true
    ↓
ChatGPT Web reads every successful handoff in full
+ inspects decisive code/source/evidence itself
    ↓
consult.reviewed
    ↓
ChatGPT Web updates canonical memory:
PROJECT_STATE.md
SESSION_HANDOFF.md
+ every affected domain file
    ↓
memory.commit
    ↓
TunnelDock validates files were really updated after web review
    ↓
ChatGPT Web performs cleanup/integration:
- merge authoritative docs / remove stale claims
- remove superseded code/scripts
- delete disposable logs/cache/tmp/debug/scratch
- archive only scientifically useful superseded material
- retain logs only as unique reproducibility/debug evidence
    ↓
cleanup.commit
    ↓
TunnelDock scans and validates removed/archived/retained paths
    ↓
state=finalized
    ↓
final human-facing consultation analysis / decision discussion
```

`memory.commit` requires `PROJECT_STATE.md`, `SESSION_HANDOFF.md`, and at least one affected domain file. It is accepted only from ChatGPT Web after `consult.reviewed` and only when listed files exist, are non-empty, and have been modified after the web review.

`cleanup.commit` is the final lifecycle gate. ChatGPT Web must check memory, documents, code, logs, and scratch across the Project Room. Typical stale/log/tmp candidates must be removed, archived, or explicitly retained. Removed paths must no longer exist; archived/retained paths must exist. A retained log must be unique evidence rather than routine output. The consultation does not finalize before this gate passes.

If the human later chooses a direction, ChatGPT Web records `decision.record`, updates the affected state/domain documents again, and repeats memory/cleanup commit when the decision changes the project truth.

## Web context budget

`web_context.json` contains paths and compact status, not full memory or full handoffs. The web coordinator polls this lightweight snapshot while workers run. Once the barrier opens, it reads only the handoffs and domain files needed for the decision. This keeps the browser conversation short while preserving durable project depth on disk.

## Non-goals

- Chat history is not durable memory.
- `.project_memory` is not a dataset/checkpoint/log storage directory.
- Agent handoffs are not copied verbatim into canonical memory.
- A worker recommendation is not automatically accepted truth.
- A consultation is not complete merely because all workers answered; it is complete only after web review, canonical memory commit, and cleanup/integration commit.
