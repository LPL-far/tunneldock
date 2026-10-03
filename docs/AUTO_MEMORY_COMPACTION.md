# Automatic canonical-memory compaction

`runtime/memory-compaction.cjs` performs user-authorized **physical pagination**, using only Node standard libraries. It never decides which scientific conclusions are obsolete. An excerpt does **not** preserve all active constraints: unseen content still applies. Full recursive expansion is mandatory before decisive research review, acceptance, deletion, or finalization.

The module itself is synchronous. TunnelDock now invokes it from the existing context background worker after explicit per-project policy opt-in. It skips active Pi/agent work and recently modified files; the next batch follows after 30 seconds. Rust UI saves share the lease and reject stale editors and dropped required-continuation headers. Context indexing expands the archived originals. These caller integrations are verified separately and do not provide semantic acceptance.

## Invocation and authorization

```js
const {compact, expand, LEASE} = require('./runtime/memory-compaction.cjs');
const result = compact(projectRoot, {apply:false}); // explicit dry run
const applied = compact(projectRoot, {apply:true}); // explicit physical write
const automatic = compact(projectRoot, {policyEnabledOnly:true});
```

```text
node runtime/memory-compaction.cjs <root> [--apply] [--policy-enabled-only]
```

These are synchronous APIs. A matching `.tunneldock/session_binding.json` must contain a nonempty project ID (letters, digits, `_`, `-`) and an absolute `cwd` resolving to the supplied root. `.project_memory` must already exist. Optional `projectId` checks the caller's expected project ID. Missing identity never creates a project. Runtime session IDs are intentionally not identity keys.

When `apply` is omitted, only `.tunneldock/memory-policy.json` containing `{"auto_compact":true}` enables writes. Missing policy defaults to dry run. Explicit `apply:false` always disables writes; explicit `apply:true` authorizes them without policy. `policyEnabledOnly:true` additionally requires policy opt-in, including with `apply:true`; otherwise the result is `disabled`. Dry runs create and release only an exclusive lease, with no persistent compaction outputs.

`quietMs` defaults to 5000. Both modification time and change time must be older than that interval for an oversized source. A recent writer returns `busy`; the caller should retry later, not spin or rewrite status itself. `quietMs:0` is useful for isolated fixtures or callers already coordinating writers.

Resource ceilings: 64 MiB per input/object, 256 MiB of expansion objects, 4096 distinct objects. Options `maxFileBytes`, `maxTotalBytes`, and `maxObjects` may lower these ceilings. Resource exhaustion blocks the operation; it never silently shortens expansion. Extremely deep reference chains may also hit the JavaScript stack limit and block. File I/O and hashing are synchronous, so use a worker/process when integrating into an interactive runtime.

## Exact trigger thresholds

Only these root canonical files are eligible, and only when `bytes > budget`:

| File | Budget |
| --- | ---: |
| PROJECT_STATE.md | 16 KiB |
| SESSION_HANDOFF.md | 12 KiB |
| DECISIONS.md | 32 KiB |
| MODEL_DESIGN.md | 32 KiB |
| DATA_CATALOG.md | 32 KiB |
| RESULTS.md | 32 KiB |
| REFERENCES.md | 32 KiB |
| EXPERIMENTS.md | 48 KiB |
| DOCUMENTS.md | 24 KiB |

The budgets match `src-tauri/src/commands/project_room/memory.rs`. All other canonical files, code, results, datasets, and user archives remain untouched. No age, semantic score, percentage, or aggregate-size threshold triggers rewriting. Equality is not over budget.

## Storage and reconstruction

Before replacing an eligible source, the compactor stores its **entire exact Buffer**, including BOM if present, UTF-8 characters, line endings, units, negation, numbers, and final-newline state, at:

```text
.project_memory/archive/auto/<stem>/<sha256>.md
.project_memory/archive/auto/<stem>/<sha256>.md.json
```

The Markdown filename is the SHA-256 of its original bytes. A preexisting object must match exactly. A corrupt object or manifest is never overwritten or automatically repaired. Temporary staging files are fsynced, then published using an atomic no-clobber hard link, verified, and only then referenced by a canonical replacement. Filesystems without hard-link support fail closed. Temporary files created by the operation are removed during normal unwinding; archives are never deleted.

The replacement begins with an explicit required-continuation notice followed by a byte-for-byte prefix of the original, cut only after a complete LF, CRLF, or standalone CR line. The whole page, including its notice, fits the existing budget. No line is shortened; headings are ordinary complete lines. If even the first complete original line cannot fit beside the notice, or input is invalid UTF-8/binary, the operation returns `blocked` without replacing the source. This is pagination, not an assertion that a prefix is scientifically sufficient.

`changed_files[].notice_bytes` and `retained_bytes` locate the exact retained prefix in the replacement. To reconstruct the pre-compaction source, read `changed_files[].archive` relative to `.project_memory` and verify its SHA-256. That object alone equals the original source exactly; do not concatenate it with the duplicate prefix.

Each immutable JSON manifest contains:

- `version`, `sha256`, and `bytes` for the exact archived Markdown.
- `source_path`, the original path relative to `.project_memory`, for interpreting legacy relative links.
- `references`: `{path, archive}` mappings from original link targets to frozen content-addressed objects.
- `dependencies`: distinct direct archive paths, not recursively embedded manifests or duplicated ancestral text.
- `expansion_required:true`, `unseen_content_still_applies:true`, `semantic_acceptance:false`.

Legacy Markdown links and reference definitions to archive Markdown, plus explicit plain/backtick `archive/...md` paths, are followed transitively. Links inside archive pages can be relative to their original directory, including parent traversal that stays inside `archive/`. External URLs and non-Markdown evidence links are retained verbatim but are not fetched. Unknown bespoke continuation formats need a separate adapter before enabling this component. Unsafe, missing, or ambiguous local continuations block rather than pretend expansion succeeded. Identical archived bytes with conflicting relative-reference origins/changed dependency mappings also block; automatic rewriting cannot resolve that ambiguity.

Prior automatic generations are referenced directly. A visited-object set handles diamonds and cycles; each archive path is visited once per expansion. Storage grows with new source snapshots and direct edges, without recursively copying all prior generations. The current root still includes any later edits, so archive-only reading is also insufficient.

## Mandatory context-search integration

```js
const documents = expand(projectRoot, 'PROJECT_STATE.md');
for (const {path, sha256, data} of documents) {
  // Index the entire Buffer, keeping object identity and source provenance.
  // A search hit remains an excerpt; decisive review must read every document.
}
```

`expand` returns the current canonical page and each transitively required original as separate exact Buffers, with paths and hashes. It performs identity, path, stable-read, archive-hash, and manifest validation. It throws if full expansion cannot be verified. Callers must not convert that failure into an empty result or an accepted review. For a stable review snapshot, readers should participate in the same project lease or revalidate the canonical source hashes after consumption.

`runtime/context-engine.cjs` now recognizes required-continuation headers and indexes the complete `expand` output as versioned memory sources, while only root pages enter the bounded default working set. The maintained `tests/memory-context-integration.acceptance.cjs` proves archived negative evidence remains searchable and corrupt archives reject publication without replacing the last good index. Do not mark archived automatic content as superseded. Keep `semantic_acceptance:false`; neither successful compaction nor successful lexical search authorizes a scientific conclusion.

## Lease, writes, and recovery

The shared project lease is exported as `LEASE`, currently `.tunneldock/memory-compaction.lock`. Exclusive `wx` creation establishes ownership; its JSON contains `pid` and a random `token`. Every existing lease returns `busy`, whether live, malformed, remote, or apparently stale. There is no expiry-based stealing and no PID-based reclamation. Normal release verifies the token first. All canonical writers must use the **same exclusive lease** during integration.

The implementation rejects symlink/junction components (including root ancestors), escaping paths, nonregular files, and hardlinked inputs. It uses descriptor/path metadata checks around reads, a recent-writer quiet interval, full source SHA-256 rechecks immediately before rename, binding revalidation, immutable-object verification, and atomic canonical-file replacement. It preserves source permissions on replacement.

Portable Node standard libraries do not offer a filesystem compare-and-swap rename or protection against a hostile process swapping path components. **The lease is cooperative.** An uncoordinated writer can still race the final check/rename window. The Rust UI save path now uses the shared lease. Arbitrary external scripts and raw native file writes must still coordinate explicitly; they are not filesystem transactions. The background caller also skips active Pi/work runs and the module defers recently edited files. These safeguards do not guarantee exclusion of an uncoordinated external writer. POSIX directory entries are fsynced; portable directory fsync is unavailable on Windows, so sudden power-loss durability of directory metadata is not guaranteed there.

Commit is atomic per canonical file, not across all nine files. All required objects/manifests are published before any replacement. A later failure may leave earlier files compacted, but their exact originals remain available and `changed_files` reports successful replacements. No automatic rollback overwrites a possible later writer. Errors retain original files or their verified archived copies; they never delete research material.

If status publication fails after replacement, the returned result is `blocked` with `status_error`; the canonical page itself contains enough information to expand the original. A later refresh validates those continuations and republishes status. If a process crashes, an operator must first establish that its lease owner is stopped before removing the stale lease. Inspect any operation-owned `.tmp` files and hard links before cleanup; do not delete archives or guess that a live lock is stale. A retry verifies/reuses good staged archives; corrupt archives require explicit operator recovery from known-good bytes.

## Status and caller handling

Applied runs publish `.project_memory/AUTO_COMPACTION_STATUS.json` after processing. Status writes use atomic replacement and skip byte-identical content, with no ticking timestamp. The first no-change refresh can transition `compacted` to `unchanged`; subsequent identical refreshes do not rewrite status. Dry runs, disabled calls, and calls unable to obtain a lease do not overwrite another owner's status.

States: `compacted`, `unchanged`, `dry_run`, `disabled`, `busy`, `blocked`. The CLI emits JSON and exits 1 for `busy`/`blocked`, otherwise 0. API callers inspect the returned state. Reports include `changed_files`, per-file actual `before_bytes`/`after_bytes`, proposed `planned_after_bytes`, archive paths/hashes/byte counts/manifest paths, project ID, and explicit nonacceptance. In dry runs the archive list describes planned outputs, not files already created. A blocked run reports successful replacements so callers can distinguish partial application. Status describes the last run, not an append-only history; immutable manifests carry the durable expansion graph.

## Acceptance verification

```text
node --test tests/memory-compaction.acceptance.cjs
```

All tests create isolated temporary fixture projects; none target actual research projects. Coverage includes UTF-8 and line boundaries, exact archive reconstruction, every byte threshold, no-change idempotency, policies, live/unverifiable leases, missing identity, concurrent read/write changes, replacement and status failures, recovery, symlinks/junctions/hardlinks, path escape, corrupt/missing archives and manifests, legacy chains/cycles, generations, and resource limits.

If the environment denies the test runner's child-process spawn, use Node's supported no-isolation runner and record that limitation:

```text
node --test --test-isolation=none --experimental-test-coverage --test-coverage-include=runtime/memory-compaction.cjs tests/memory-compaction.acceptance.cjs
```

Verified in the Windows workspace with Node v26.7.0: 21/21 fixture tests pass with no skips using `--test-isolation=none`; coverage is 98.80% lines, 85.36% branches, and 100% functions. The exact requested command was attempted and failed before loading tests because the sandbox rejected its child-process spawn with `EPERM`. Direct CLI dry-run and `--apply` launches were separately verified against a disposable fixture (22,000 original bytes to a 16,378-byte canonical page, with exact archived original). The CLI argument contract is also tested through exported `main`. Actual crash/power-loss behavior and deployment writer coordination remain integration verification requirements.
