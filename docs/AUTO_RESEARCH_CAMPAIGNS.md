# Bounded auto-research campaigns (version 1)

Campaigns are immutable, human-authorized execution protocols. They use the existing ProjectTask / AgentRun queue and writer conflict checks. They do not provide a new scheduler, scientific acceptance engine, GPU resource manager, or canonical memory system.

## Working draft protocol

Write one UTF-8 JSON event file (at most 128 KiB) into the project's existing `.tunneldock/inbox/`, or paste the **plan object** into Project Room ? Research campaigns ? Create draft. `campaign.draft` never creates a task/run. The same ID plus identical typed plan is idempotent; a different plan under the same ID is rejected even while draft. Amendments require a new ID and a new authorization.

This runnable **synthetic local fixture** uses five files (`baseline.json`, `code.json`, `config.json`, `dataset.json`, `evaluator.json`) whose exact bytes are `frozen` (no newline). `reference.json` contains exactly `{"a":1.0,"b":2.0,"c":3.0}` (no newline). Create these only in an isolated fixture workspace, never overwrite scientific project files. The SHA256 values below match those exact bytes. This documentation does not request execution of the fixture.

```json
{
  "kind": "campaign.draft",
  "author": "chatgpt",
  "plan": {
    "version": 1,
    "id": "fixture",
    "question": "Can the fixture mean gate be reproduced?",
    "hypothesis": "The declared measurements have mean >= 2.",
    "provenance": [
      {
        "role": "baseline",
        "path": "baseline.json",
        "sha256": "ffb304816a1090313e833215c08dae3d209cfad1ffd1f674f0909a2ae99e1394"
      },
      {
        "role": "code",
        "path": "code.json",
        "sha256": "ffb304816a1090313e833215c08dae3d209cfad1ffd1f674f0909a2ae99e1394"
      },
      {
        "role": "config",
        "path": "config.json",
        "sha256": "ffb304816a1090313e833215c08dae3d209cfad1ffd1f674f0909a2ae99e1394"
      },
      {
        "role": "dataset",
        "path": "dataset.json",
        "sha256": "ffb304816a1090313e833215c08dae3d209cfad1ffd1f674f0909a2ae99e1394"
      },
      {
        "role": "evaluator",
        "path": "evaluator.json",
        "sha256": "ffb304816a1090313e833215c08dae3d209cfad1ffd1f674f0909a2ae99e1394"
      },
      {
        "role": "reference",
        "path": "reference.json",
        "sha256": "8f986277e74d65ead84bc1bd812e96f1a987260e5b8fcb404d34070ef7401f79"
      }
    ],
    "stages": [
      {
        "id": "baseline-check",
        "instruction": "Read the frozen reference and record the approved local fixture measurements. Do not run remote jobs.",
        "intervention_scope": "results/first",
        "gate": {
          "metric": "mean",
          "minimum": 2.0,
          "reference_path": "reference.json",
          "sample_count": 3
        },
        "max_attempts": 2,
        "on_pass": 1,
        "on_scientific_fail": null,
        "on_unknown": "awaiting_web_review"
      },
      {
        "id": "confirm",
        "instruction": "Read the frozen reference and record the approved local fixture measurements. Do not run remote jobs.",
        "intervention_scope": "results/second",
        "gate": {
          "metric": "mean",
          "minimum": 2.0,
          "reference_path": "reference.json",
          "sample_count": 3
        },
        "max_attempts": 2,
        "on_pass": null,
        "on_scientific_fail": null,
        "on_unknown": "awaiting_web_review"
      }
    ],
    "max_submissions": 3,
    "deadline": "2030-01-01T00:00:00Z",
    "retry_confirmed_operational_failure": true
  }
}
```

The resulting plan SHA256 is `76fb42a137a13241426a8fec41d2cb8a23e79c19c43e2294101f9a08c490c6a2`. The digest covers the compact UTF-8 **typed Rust Plan serialization**, including field order and explicit null transitions, rather than the original event's whitespace or key order. Always inspect the server-provided digest and frozen plan with the panel before authorization.

## Authorization and controls

Authorization is deliberately unavailable through inbox files. Source reports, worker handoffs, completion footers and `author=user` in an inbox file cannot authorize. The human panel first fetches the full plan (`campaign_plan`, arguments `projectId`, `campaignId`), then calls Tauri `campaign_control` with `projectId` and this `event`:

```json
{
  "kind": "campaign.authorize",
  "campaign_id": "fixture",
  "plan_sha256": "76fb42a137a13241426a8fec41d2cb8a23e79c19c43e2294101f9a08c490c6a2",
  "authorization_ref": "user-approved-fixture-2026-10-03"
}
```

The authorization reference must be nonempty (at most 1000 bytes). It records the user's approval of the exact displayed plan. Repeating the same authorization is a no-op; replacing it fails. `campaign_control` also supports `campaign.draft`, `campaign.pause`, and `campaign.resume`. The panel can inspect all frozen plans and shows a reference input only after fetching a draft. It does not offer a scientific acceptance button.

Pause/resume also support coordinator inbox events:

```json
{
  "kind": "campaign.pause",
  "author": "chatgpt",
  "campaign_id": "fixture",
  "plan_sha256": "76fb42a137a13241426a8fec41d2cb8a23e79c19c43e2294101f9a08c490c6a2"
}
```

Use `campaign.resume` with the same identity to resume scheduling. Resume removes the pause; it does not reopen a terminal campaign, extend a deadline, reset a budget or replay an uncertain submission. IPC is the existing trusted local desktop control boundary. Inbox `author=chatgpt` for pause/review follows the existing coordinator convention, **not cryptographic authentication**; filesystem/desktop access control remains required. Hostile same-user processes or workers with unrestricted desktop control are outside this boundary.

## Plan constraints and continuation

- `version=1`; ID is 1?64 ASCII letters/digits/hyphens/underscores. Nonempty question/hypothesis are limited to 4000 bytes each.
- Up to 64 frozen regular files, including roles `baseline`, `code`, `config`, `dataset`, `evaluator`; each stores a relative path and lowercase SHA256. The gate reference file must also be frozen. Read limits: 8 MiB per provenance file and 32 MiB total. Large code/dataset/model payloads require explicit small manifests; only declared bytes are hash-verified, not files merely mentioned by a manifest.
- 1?16 stages. Each declares one literal local intervention path, an instruction (at most 8000 bytes), `max_attempts` (1?16), a quantitative gate, and three continuation decisions. No glob scopes. Absolute paths, traversal, backslashes, symlinks and Windows reparse points are rejected. Case-insensitive ancestor/descendant overlap between any stage scope and frozen files, `.git`, `.tunneldock`, or `.project_memory` is rejected.
- `on_pass` and `on_scientific_fail` are either null (stop for Web) or a strictly greater in-bounds stage index. Thus transitions are acyclic and only declared stages are reachable. A failure edge names a preapproved diagnostic; it never retries that failed scientific stage. `on_unknown` must be `awaiting_web_review`.
- `max_submissions` is 1?64 **attempt reservations**, conservatively including queued attempts and uncertain submissions. `deadline` is RFC3339 and tested against current UTC time. `retry_confirmed_operational_failure` permits another attempt only after an actual transport-confirmed terminal failure and within both finite budgets.
- Mean, Pearson and Spearman use `minimum` (inclusive lower threshold), `reference_path`, and `sample_count` (1?10000). No absolute-correlation transform, p-value test, multiple-testing correction, upper-bound gate or automatic metric selection is implemented. Pearson/Spearman require at least two samples and nonzero variance; Spearman uses average ranks for ties. Undefined/overflow/nonfinite arithmetic is invalid evidence, never scientific failure.

## Evidence format and independent verification

A campaign worker gets only its stage-specific goal, gate, reference path, intervention scope and identity. Paths are relative to the Project Room local root. Only the existing Codex queue is used in this increment. Remote/SSH work is not supported by this campaign protocol. The worker writes `EVIDENCE.json` beside its current run's `HANDOFF.md` in `.tunneldock/runs/<actual-run-id>/`:

```json
{
  "version": 1,
  "campaign_id": "fixture",
  "plan_sha256": "76fb42a137a13241426a8fec41d2cb8a23e79c19c43e2294101f9a08c490c6a2",
  "stage_id": "baseline-check",
  "task_id": "CAMPAIGN-fixture-0-1",
  "run_id": "RUN-FIXTURE-1",
  "samples": [
    {"id": "a", "value": 1.0, "target": 1.0},
    {"id": "b", "value": 2.0, "target": 2.0},
    {"id": "c", "value": 3.0, "target": 3.0}
  ]
}
```

`RUN-FIXTURE-1` is an isolated-test identity. A real worker must use its actual generated run ID from the HANDOFF directory. Each attempt owns exactly one stable task/thread ID (`CAMPAIGN-<campaign-id>-<zero-based-stage>-<attempt-number>`) and one separately reserved run ID. Evidence must match all five campaign/plan/stage/task/run identities. Any duplicate run for the attempt is ambiguous and rejected; older runs cannot certify new attempts.

Reference data is a JSON object of unique exact sample IDs to finite numeric targets. Samples must cover exactly this set, have exactly the declared count, contain no duplicate IDs and contain only finite numeric values. Supplied targets must match the frozen reference's parsed f64 bits exactly, including signed zero. Both mean and correlations check targets. Metric strings, worker PASS, footer claims and prewritten HANDOFF files do not certify anything.

The verifier requires an actual current `AgentRun` with successful terminal transport confirmation, `status=completed`, `finished_at`, no execution error, and a nonempty bounded HANDOFF at the expected path. Campaign polling continues until transport termination even if a worker writes HANDOFF early. HANDOFF is limited to 256 KiB and evidence/reference JSON to 2 MiB. All frozen hashes are rechecked, and the exact reference bytes used for arithmetic are hashed again before parsing.

Verified raw evidence bytes are archived under the control store's `campaign-evidence/<evidence-sha256>.json`; on-demand details expose `evidence_directory` and `evidence_ids`. Each attempt retains execution, evidence, numeric metric, gate, reason and evidence digest. The digest binds the raw artifact and its identities. Missing/corrupt/mismatched/stale evidence transitions to `awaiting_web_review` with `evidence_invalid`; it never becomes a negative scientific result or silently retries. An invalid artifact is not automatically reverified after edits: recovery needs Web inspection and a new explicitly authorized plan.

These checks establish protocol consistency, not originality, causal attribution, evaluator integrity beyond the declared hash, truthful sample collection, or trustworthy worker-produced predictions. Write scopes are existing orchestration/writer-conflict controls, not an OS filesystem sandbox. Hash checking cannot prevent a same-user adversary from racing filesystem operations.

## Persistence, restart, pause and deadline

The authoritative `campaigns.json` lives in the existing app-data `projects/<project-id>/` control directory, separate from the scientific workspace. Updates use bounded (2 MiB), uniquely named `create_new` temporary files, file sync and atomic rename. Normal failures clean their temporary files; a failed save leaves the caller revision unchanged. Saves compare the current revision and refuse stale revisions or changed immutable plans/authorizations/reviews; malformed authoritative data produces a visible error instead of an empty campaign list. Up to 32 campaigns are retained. A partial temporary file is ignored on restart; an invalid committed file blocks scheduling and requires operator repair from known-good evidence. Evidence archives are bounded per artifact and per campaign's finite attempt count. Filesystem atomic rename/sync guarantees apply; coordination is cooperative within one app process (`project_store_lock` plus off-lock snapshot-CAS). No database-style whole-project transaction or cross-process multi-writer guarantee is claimed.

The existing supervisor performs one bounded campaign reconciliation per project per tick, selecting at most one campaign for heavy verification, **outside the global project-store lock**. It reacquires the lock, compares the original campaign store, task/run snapshots and room configuration, and commits only if unchanged. Existing legacy worker transport polling/dispatch retain their preexisting locking; no campaign network poll or second scheduler is introduced.

An attempt/task identity is persisted before the queue task is written. Missing queue tasks are repaired from the immutable reservation after restart. Immediately before external submission, the existing dispatcher rechecks authorization/hash, exact task contents, owner, pause, deadline and reservation, then atomically reserves the run ID. No second dispatch is allowed for that reservation. A crash after reservation but before a durable AgentRun means **submission uncertain**, even if the request may never have reached a worker: no automatic replay. Missing task/run/control files and disk errors are surfaced.

Pause and disabled Project Rooms prevent queued campaign dispatch, including direct/manual dispatch. Disabled rooms still observe already-running results, but allocate no new attempt/task. A blocked queue task with no external reservation or matching run can be restored to the same queue identity after guards open; a reserved/uncertain/running submission is never requeued. Writer-scope conflicts wait without repeated external dispatch attempts. Deadline/budget stop new scheduling. Already-running work is allowed to drain and its evidence can still be recorded, with active/uncertain drain shown separately. Nothing here kills processes or claims hard GPU enforcement. An uncertain reservation remains awaiting operator/Web review; this increment has no force-retry or pretend-drained control. Confirmed operational failure retries use a new attempt/task ID. Valid scientific failures cannot be retried until they pass.

## Separate Web review and canonical memory

A transport-terminal, independently evidence-verified task releases its execution write ownership as `status=completed`, `reviewed_by=campaign-evidence-verifier`, with its evidence digest in the summary and `web_reviewed=false`. This is execution/protocol completion, not scientific acceptance. It permits the next preapproved stage to reuse the scope. The final scheduling state is `awaiting_web_review`. Generic `task.review`, task upsert and LocalFinalizer cannot revise, accept or retry campaign attempts. Actual execution success, verified evidence, quantitative gate pass/fail and Web reviewed/accepted are separate fields. Generic pending-review actions exclude campaign attempts and instead surface campaign-specific actions.

After reading the full handoffs, archived evidence, declared frozen provenance and limitations, the Web coordinator submits a separate inbox event with this schema:

```json
{
  "kind": "campaign.reviewed",
  "author": "chatgpt",
  "campaign_id": "fixture",
  "plan_sha256": "76fb42a137a13241426a8fec41d2cb8a23e79c19c43e2294101f9a08c490c6a2",
  "plan_version": 1,
  "evidence_ids": [],
  "review_ref": "web-review-record-2026-10-03",
  "accepted": false
}
```

`evidence_ids` must equal the **entire ordered array** from on-demand `campaign_details` (or the campaign record returned by `campaign_plan`). The empty array above works only for a campaign stopped before any evidence was verified. For a completed campaign copy its actual full array; fabricated/omitted/stale IDs are rejected. Pending reserved/running attempts prevent final review. `accepted=true` additionally requires the final attempt to pass, no negative/invalid scientific attempts, and the final successful stage's `on_pass=null`. Earlier confirmed operational failures may precede a successful preauthorized retry; they remain recorded. An unconfirmed failure or a scientific FAIL cannot be relabeled as PASS. A valid review event is persisted as a review request, not immediately accepted. On the next bounded supervisor reconciliation, `accepted=true` rechecks current frozen hashes, each sealed evidence digest, current run/artifact identities and independently recomputed metrics outside the global lock. Mutable worker evidence must still match its sealed snapshot. Missing/changed sealed or current evidence, drifted sources and inconsistent historical terminal failures reject acceptance while preserving earlier results; the reason appears in status. Commit compares the full snapshots before setting Web review fields. Identical review replay is idempotent; changing a recorded review is rejected. `accepted=false` records that review happened without endorsing the campaign as passing.

`.tunneldock/campaign_status.json`, `web_status.campaigns`, `web_context.campaigns` and Project Room activity polling carry counts, a detail path and at most four prioritized entries (question 120 characters; reason 160 characters). Evidence-ID arrays and full plans are omitted. The worst-case 32-campaign fixture asserts a summary smaller than 12 KiB. The Chinese/English panel fetches all status entries only through explicit `campaign_details` IPC (`projectId`), and full plan/attempt records through `campaign_plan` (`projectId`, `campaignId`). It remounts by project ID and ignores stale async responses. Paused, disabled, uncertain, empty, pending-authorization and error states are distinct from active execution. `state_token` includes campaign status. Campaign errors and pending authorization are visible in the panel; no synthetic percentage is shown.

Campaigns never rewrite canonical scientific decisions or `.project_memory`. Existing automatic memory remains enabled. Scientific interpretation and any change to accepted project truth still use the existing Web review and memory commit protocol. The campaign record is an auditable execution/evidence record for that review, not canonical memory.

## Maintained verification

`src-tauri/src/commands/project_room/campaign/tests.rs` uses real isolated temporary files, an inert AppState, actual production reconciliation, persistence and reservation hooks. It never starts a worker, uses SSH or consumes a GPU. Tests cover authorization/hash immutability, DAG/scopes, mean/correlations/ties, invalid numeric/sample/reference identities, provenance drift, symlinks/junctions, valid pass/fail/unknown, finite retries, restart repair, duplicate reservations, pause/deadline/budgets, old run rejection and separate human/Web review. Temporary fixtures clean themselves up. The legacy finalizer has a campaign exclusion regression test; `CampaignPanel.test.tsx` checks the visible distinctions.

Run from the repository root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File C:/Users/23586/AppData/Local/Temp/run-rust-tests.ps1
npm run test -- --run
npm run build
```

No real research plan was authorized or deployed by this development run. All authorization/run/evidence examples in tests are isolated fixtures. No scientific jobs, deployment, commit, session restart or dependency installation is part of verification. There is no bootstrap confidence-interval engine, global GPU allocator or forced GPU stop. Deadlines bound scheduling only.

Development evidence and full failure logs are retained under `runtime/auto-research-development/`; see `VERIFICATION.md` there for exact commands, counts and environment limitations.
