# TunnelDock Memory Lifecycle V2

## Goal

Project memory must be able to live for years without turning every future agent run into a replay of the whole project history.

The key rule is:

> **History may grow without bound; default context may not.**

TunnelDock therefore treats current memory as a bounded materialized view over an unbounded historical evidence/event layer.

## Design influences

The design combines several mature ideas:

- **Event Sourcing:** keep an append-only record of important state transitions so history remains auditable.
- **CQRS / materialized views:** current query/read state is optimized separately from historical events.
- **Snapshots / durable execution:** resume from a compact current state instead of replaying the entire history on every operation.
- **Agent progressive disclosure:** start with a small current summary/index and open detailed memories only when the task requires them.
- **Core vs archival memory:** current/high-value facts stay directly accessible while cold history is searched on demand.
- **Memory consolidation / forgetting:** settled detail leaves the hot context without being destroyed.
- **Agentic memory evolution:** new evidence may supersede/recontextualize old memory; old claims should not remain equally active forever.

## Four memory layers

### L0 — Working set

Files:
- `PROJECT_STATE.md`
- `SESSION_HANDOFF.md`

Purpose: what the next decision/run needs now.

These files should be aggressively current. They are not a history log.

### L1 — Canonical semantic memory

Files:
- `MODEL_DESIGN.md`
- `DATA_CATALOG.md`
- `RESULTS.md`
- `DOCUMENTS.md`
- `DECISIONS.md`
- `EXPERIMENTS.md`
- `REFERENCES.md`

Purpose: accepted current facts, contracts, constraints, evidence pointers, and active unresolved questions.

These are bounded current materialized views. They have soft byte budgets monitored by `MEMORY_STATUS.json`.

### L2 — Historical memory

Paths:
- `archive/<domain>/...`
- `ledger/YYYY-MM.jsonl`

`archive/` stores cold/superseded research material that still has scientific value.

`ledger/` is append-only lifecycle metadata: memory commits, durable decisions, compaction/archive actions, and cleanup milestones.

Neither is injected into normal agent context. Agents search/open these only when current memory indicates that historical evidence matters.

### L3 — Evidence / artifacts

Datasets, checkpoints, raw experiment outputs, figures, PDFs, videos, raw logs, and source trees stay in their project-native locations.

Memory stores only stable references: path, version/hash, provenance, experiment/result ID, and interpretation status.

## Memory kinds

The layers also map naturally to common agent-memory categories:

- **Semantic memory:** accepted current project facts/contracts -> canonical root Markdown.
- **Episodic memory:** what happened in runs/experiments/reviews -> experiment records, ledger, archived handoffs/results.
- **Procedural memory:** how agents should work -> Constitution, Memory Protocol, AGENTS/skills/workflows.
- **Evidence memory:** immutable or reproducible artifacts -> external project-native paths.

Keeping these categories separate prevents a successful past workflow from being confused with a current scientific fact.

## Current-memory budgets

Initial soft budgets:

| File | Soft budget |
| --- | ---: |
| `PROJECT_STATE.md` | 16 KiB |
| `SESSION_HANDOFF.md` | 12 KiB |
| `DECISIONS.md` | 32 KiB |
| `MODEL_DESIGN.md` | 32 KiB |
| `DATA_CATALOG.md` | 32 KiB |
| `EXPERIMENTS.md` | 48 KiB |
| `RESULTS.md` | 32 KiB |
| `REFERENCES.md` | 32 KiB |
| `DOCUMENTS.md` | 24 KiB |

`MEMORY_STATUS.json` reports `healthy`, `near_budget` (>=80%), and `over_budget` (>100%).

Budgets are not deletion limits. They are **compaction triggers**.

## Compaction policy

When a canonical file is over budget:

1. Do not truncate blindly.
2. Identify settled, superseded, historical, or low-frequency detail.
3. Move that detail into the matching `archive/<domain>/` file with date/period and evidence pointers.
4. Rewrite the canonical file so it contains only current truth, active constraints, current accepted evidence, and unresolved questions.
5. Preserve links from the canonical view to the archived evidence when future interpretation may depend on it.
6. Submit `memory.commit` again, including any archive paths in `archive_files`.
7. `cleanup.commit` will reject finalization while any canonical file remains over budget.

## Read path — progressive disclosure

Agents should read in this order:

1. `MEMORY_INDEX.md`
2. `PROJECT_STATE.md`
3. `SESSION_HANDOFF.md`
4. only the relevant canonical domain file(s)
5. only if needed: search `ledger/` and `archive/`
6. only if decisive evidence is needed: open the referenced project-native artifact/source

This prevents long-running projects from paying the historical-context cost on every turn.

## Write path

A meaningful reviewed update flows through:

```text
new work/evidence
    -> worker handoffs
    -> Web review
    -> update bounded canonical views
    -> memory.commit
         -> append ledger event
         -> refresh memory health
    -> if over budget: compact current -> archive -> memory.commit again
    -> cleanup.commit
         -> only then finalized
```

Durable human decisions also append a ledger event in addition to updating `DECISIONS.md`.

## Conflict / staleness rule

Old statements must not remain equally authoritative merely because they exist.

For current canonical files:

- a new accepted fact replaces the stale current statement;
- a superseded claim moves to archive when scientifically useful;
- invalidated results remain discoverable as historical evidence but are excluded from default current conclusions;
- the current environment/code/data wins over stale memory when they conflict;
- agents should cite the evidence that caused a memory update.

## Why not automatic destructive summarization?

Research memory contains negative results, ablations, evaluation caveats, and provenance that a generic summarizer can incorrectly erase. TunnelDock therefore automates **pressure detection, event recording, lifecycle gating, and retrieval boundaries**, while ChatGPT Web remains responsible for semantic compaction after review.

## Next phase: indexed retrieval

The current V2 archive is filesystem + JSONL and can be searched with normal project tools. A later V2.1 can add a local metadata/search index (SQLite FTS and optional embeddings) with fields such as:

- memory/event ID
- domain
- active/superseded/retracted/pinned state
- valid-from / valid-to
- tags/topics
- evidence paths/hashes
- supersedes / related-to links
- last-accessed / access count
- importance/salience

That index should remain a retrieval layer, not a second source of truth. Canonical current Markdown + project evidence remain human-auditable.
