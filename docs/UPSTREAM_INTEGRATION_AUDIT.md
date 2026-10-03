# Upstream integration audit

Inspected on 2026-10-03. This file is the source/version and adoption-boundary record for the local context integration. It is not a claim that five upstream products have been installed, benchmarked, or merged wholesale.

## Pinned sources

| Source | Inspected revision | License observed at that revision | Adopted idea / boundary |
| --- | --- | --- | --- |
| [JuliusBrussee/caveman](https://github.com/JuliusBrussee/caveman) | `17f574b51221b293c16c8e532bc3bf3fa9f4051c` | Apache-2.0; repository licensing notes say this applies from 3.0, not retroactively to older releases | Bounded useful context, preserve original evidence and failure/negation qualifiers. No proxy, cloud account, model replacement, or forced ungrammatical user replies. |
| [DietrichGebert/ponytail](https://github.com/DietrichGebert/ponytail) | `c982cd411abb53323c4baa1baa3c2f020b8d0b08` | MIT | Search the existing implementation before writing; prefer existing helpers, platform features and the smallest correct change. Never trade away validation, tests, accessibility or reproducibility for fewer lines. |
| [mksglu/context-mode](https://github.com/mksglu/context-mode) | `9f3ecc8b0aafd25d3592ffc31e4b02b1767f6089` | Elastic License 2.0 (ELv2), not MIT; GitHub metadata reports NOASSERTION | Retain originals outside the conversation, index/search before retrieving exact excerpts, preserve session continuity. This integration implements local concepts independently; it does not vendor the ELv2 implementation or launch its MCP runtime. |
| [colbymchenry/codegraph](https://github.com/colbymchenry/codegraph) | `dbb3d63cb1a92d04112dcc9ebb53bea4d1e317fb` | MIT | Versioned source/symbol lookup and import/test impact candidates. The native lightweight adapter is lexical and incomplete; it is not the upstream semantic CodeGraph kernel, a complete call graph or proof that code is unused. |
| [mvschwarz/openrig](https://github.com/mvschwarz/openrig) | `a8c1c375811d8126d42148e15b81ccb235406c9b` | Apache-2.0 | Stable project/agent identity, owned work, continuity and observable delivery/review boundaries. Keep the existing Windows durable hosts; do not install another daemon/tmux stack or modify global provider trust settings. The inspected upstream README states that native Windows is not supported. |

License observations are tied to the inspected revisions. Re-check notices and dependency terms before importing implementation code from any newer release. No upstream logos, fonts, binaries or hosted services are bundled by this concepts integration. Performance numbers in upstream READMEs are not TunnelDock measurements.

## Implementation mapping

- `runtime/context-engine.cjs`: original local derived-context engine; immutable source objects, versioned indexes, bounded working set, source/artifact search and lexical impact candidates.
- `src-tauri/src/commands/project_room/context_engine.rs`: bounded background refresh outside the task-store critical section; runtime state and error integration.
- `pi-extensions/tunneldock-context.ts`: scoped project identity validation and explicit search/read/impact tool access.
- `pi-extensions/tunneldock-flow.ts`: existing full-output preservation and bounded tool return path; no loss of full original evidence.
- `src/views/projectRooms/ContextEnginePanel.tsx`: actual scan/packing/searchable or error/stale state, source and working-set bytes, indexed/excluded file counts. No fabricated percentage-complete or token-savings claim.
- Existing agent-activity/review receipts remain authoritative for task progress. A successful refresh, healthy worker, delivered handoff, and accepted scientific result are different events.

The backend worker and Pi adapter must be verified separately: placing an extension on disk does not prove that an already-running Pi session loaded it. Do not restart active research sessions merely to claim deployment completion.

## Memory and cleanup contract

Automatic packing changes only the derived retrieval view. Canonical `.project_memory/*.md` files, source code, data, checkpoints, accepted/negative results and raw evidence are not rewritten or deleted by the packer. Original bytes are addressable by SHA-256; a generated excerpt remains incomplete and must point to its source.

A content change invalidates the relevant view. Before publishing a new generation, validate source identity and reject concurrent changes, invalid canonical encoding, corrupt objects or a competing live refresh. Failures must retain the last good generation and remain visible as failures, not a new success timestamp.

Semantic rewriting of canonical decisions/results still needs the existing Web review and memory-commit boundary. Automated working-set compression does not establish that the original files are obsolete, that an old negative result is irrelevant, or that a model conclusion has been accepted. Generated-object garbage collection must respect references and be separately verified; no blanket age-based deletion of research files.

Byte reduction means `(canonical source bytes - derived working-set bytes) / canonical source bytes`. It is not a measured tokenizer reduction, model quality guarantee or end-to-end latency improvement. Measure those separately before making such claims.

## Independent regression review

`tests/context-engine-review.acceptance.cjs` adds isolated fixture checks for failed-refresh lease cleanup, search-time content-integrity verification, excluded-source coverage changes, stale impact results, symlinked immutable-object reuse and invalid canonical encoding. Test outcomes belong in the verification logs for the exact built revision; the existence of tests is not a passing-test claim.

## Articles not yet accessible

The following exact sources were requested but returned verification pages rather than article bodies during both browser and direct read attempts:

- https://mp.weixin.qq.com/s/5YPKctLVxVKXVwa1KDiH4A
- https://mp.weixin.qq.com/s/mnU8PE-6DZ38rJqHeOiSfQ

Status: **awaiting accessible article text/export**. No research claims, experimental directions or algorithm changes have been inferred from their opaque URLs. Preserve the existing research hypotheses and evaluation gates until the article text can be reviewed and cited.
