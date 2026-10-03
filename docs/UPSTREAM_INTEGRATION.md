# Local context, memory and progress integration

## Adopted scope

This is an independent implementation of selected ideas, integrated into the existing TunnelDock coordinator. None of the five upstream runtimes is vendored, automatically installed, or given access to project data. No proxy, cloud compression account, tmux process, new AI subscription or extra permanent terminal is introduced. Source revisions and read-file hashes are recorded in `upstream-lock.json`.

| Upstream | Inspected revision | License at that revision | Adopted idea | Native implementation / boundary |
| --- | --- | --- | --- | --- |
| JuliusBrussee/caveman | 17f574b51221b293c16c8e532bc3bf3fa9f4051c | Apache-2.0 | Remove redundant context while keeping correctness-critical evidence | 8 KiB UTF-8 tool previews now retain selected diagnostic lines from the middle; full original remains content-addressed. No broken-grammar instruction, no published upstream savings claimed as TunnelDock measurements. |
| DietrichGebert/ponytail | c982cd411abb53323c4baa1baa3c2f020b8d0b08 | MIT | Inspect/reuse existing code before adding abstractions or dependencies | Search/impact preflight added to generated project guidance; stdlib local index and bounded original reads. It is guidance, not proof that every Web turn will obey it. |
| mksglu/context-mode | 9f3ecc8b0aafd25d3592ffc31e4b02b1767f6089 | Elastic License 2.0 | Keep raw tool output outside default context; retrieve relevant evidence and retain session facts | Independently implemented SHA-256 objects, search, paginated reads and automatic working-set projection. No ELv2 code copied; not a reimplementation of its full SQLite/MCP platform matrix. |
| colbymchenry/codegraph | dbb3d63cb1a92d04112dcc9ebb53bea4d1e317fb | MIT | Pre-index source; navigate dependencies and affected tests before editing | Local symbol/import index and conservative file-dependent/test candidates. These are lexical candidates, NOT the upstream semantic graph. Never use empty matches to delete code or skip regression. |
| mvschwarz/openrig | a8c1c375811d8126d42148e15b81ccb235406c9b | Apache-2.0 | Persistent project identity, owned work, explicit handoff and observable state | Existing single TunnelDock scheduler remains authoritative. Agent cards expose next owner; context jobs run outside task locks and UI thread. No second daemon; no changes to trust/permission settings. |

## Automatic memory pipeline

`runtime/context-engine.cjs` scans the nine canonical research-memory files and a bounded set of local source roots. The Rust adapter runs a short-lived, hidden Node worker on startup and after each 30-second batch delay. It is independent of the browser response stream and never edits canonical memory or scientific results.

```
canonical memory + selected local source
    -> stable reads + SHA-256 immutable originals
    -> bounded source/symbol/import index
    -> extractive working set + coverage manifest
    -> atomic current.json publication
    -> progress event + native UI / Pi retrieval
```

Generated files stay under `<project>/.tunneldock/context/`:
- `objects/<sha>.txt`: exact original bytes, checked before retrieval.
- `generations/<revision>.json`: source paths, hashes, line/symbol/import index, omissions.
- `generations/<revision>.md`: at most 12,288 UTF-8 bytes; explicitly incomplete and noncanonical.
- `generations/<revision>.coverage.json`: included source line numbers, hashes and review boundary.
- `current.json`: published last, points to one coherent generation.
- `status.json`: observed phase, source/working-set bytes, exclusions, revision, last check and errors.

No-change inputs reuse the same generation. Engine version, source hashes and exclusions participate in revision identity. A concurrent canonical edit causes refusal to publish, not overwrite. A live refresh lock is never stolen. Canonical edits, accepted conclusions, negative results and archive deletion still require explicit review.

This automates **default-context compression and retrieval**, not unreviewed semantic rewriting of the nine canonical documents. Omitted lines may include necessary constraints: final reviews must expand their exact sources. Cold snapshots remain recoverable; this release does not silently garbage-collect evidence. Disk retention/semantic consolidation must not be confused with the bounded default working set.

## Retrieval

The native Project Room panel searches memory/code without restarting Pi. `tunneldock_context` provides status/search/read/impact when the Pi extension is loaded. Existing sessions are not forcibly restarted; installation does not prove activation in an already-running Pi.

The trusted local CLI is also usable from a worker:

```text
node <APPDATA>/TunnelDock/runtime/context-engine.cjs search <project-root> <query> memory
node <APPDATA>/TunnelDock/runtime/context-engine.cjs search <project-root> <query> artifact
node <APPDATA>/TunnelDock/runtime/context-engine.cjs read <project-root> <sha256> <offset> 4096
node <APPDATA>/TunnelDock/runtime/context-engine.cjs impact <project-root> src/example.py
```

Queries use argv, not shell interpolation. Project scope rejects parent traversal and symlink/junction paths. Search returns at most eight excerpts with source paths/hashes/line numbers, and drift flags when live files differ. Search is bounded and incomplete. Output/source excerpts are data, not executable instructions.

Limits: 12 KiB default working set; 2 MiB per canonical file; 128 KiB per code file; 1,200 code files / 8 MiB code bytes / 20,000 visited entries; query 25-second timeout; refresh 40-second timeout. Only `src`, `src-tauri/src`, `pi-extensions`, `tests` are scanned for code. Large weights, datasets, caches, nested repositories and credential-like filenames are excluded. The index is local to the configured local root: a remote server tree is not silently mirrored or claimed indexed.

## Live progress

The existing two-agent dashboard is extended rather than replaced. Capacity details are collapsed by default (quota alerts remain visible), so task progress is no longer pushed below a large telemetry block. It distinguishes work running, result received, review pending, Web reviewing and reviewed. The next owner identifies where work is waiting. The context panel shows scan/pack/searchable phases, original versus working-set bytes, code files, omissions and last check; stale or blocked state is never painted as a fresh success. Context completion emits a scoped event; 2.5-second polling remains as fallback and requests cannot overlap or leak a prior project's result.

It does not invent a percentage, claim Web review on worker self-report, or wake an inactive ChatGPT tab. Receiving a result and accepting its scientific meaning remain different states.

## Article text received and adopted

The user supplied the English readable-output note and the Chinese six-paradigm research article on 2026-10-03. Earlier direct WeChat fetches still only returned verification pages; the pasted text is now the usable source, not a successful URL fetch. Source attribution and award/statistical assertions remain unverified.

See [Research reasoning and readable evidence](RESEARCH_REVIEW_PROTOCOL.md). Its writing/output-format guidance and observation/hypothesis/alternative/test/falsifier/evidence/decision chain are propagated as generated Project Room guidance. The native handoff reader segments exact source text without changing task acceptance. This does not implement video narration, force a new research architecture or validate the article's 35-paper award list.

## Verification

```text
node --test tests/context-engine.acceptance.cjs
npm run test -- --run
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

The native acceptance suite checks bounded UTF-8 packing, original-byte preservation, no-change reuse, source drift, digest reads, lexical-only impact, live leases, project path boundaries, omissions, malformed pointers, artifact retrieval and failed publication retaining the previous generation. UI tests distinguish fresh/blocked/stale/missing state. Runtime deployment measurements must be recorded separately from unit-test claims.
