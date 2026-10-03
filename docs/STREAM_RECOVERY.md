# Bounded Web turns and stream continuity

This repair has two distinct parts. AgentActivityPanel uses different sibling keys for campaign and context panels; mounted Chromium regression verifies that updates do not append duplicate campaign panels. This UI fault is not evidence that it caused ChatGPT's browser stream errors.

`tunneldock-stream.ts` supplies opt-in admission control and a metadata-only persistent checkpoint. It cannot restore a ChatGPT response stream, repair a server-side 404/410, end a browser response, or wake an inactive browser. It bounds new native work in a long Web turn; persistent Codex/Gemini tasks remain independent.

## Activation

Install the maintained Pi extensions, then verify the running session's native tool catalog and a real `tunneldock_recover` call. Installation alone is not activation. Legacy Pi hosts may be replaced once when no native operation is pending, retaining their Broker session ID. Do not kill GPU/Codex processes or blindly replay an uncertain operation. Record old/new host IDs and whether activation was deferred.

In a matched Project Room, `.tunneldock/stream-policy.json` enables the guard:

```json
{"enabled":true,"max_calls":20,"max_output_bytes":98304,"max_elapsed_ms":300000}
```

Values must be positive finite safe integers within implementation ceilings. The default 20 calls / 96 KiB / 5 minutes are advisory admission stops, not a browser-response ceiling. The elapsed limit starts at the first admitted native tool call. It does not measure total browser reasoning time. Policy is read at call boundaries; disabling it does not kill any worker. The existing delegation policy routes code edits and known execution/upload operations to a persistent agent task. Neither policy is an arbitrary-shell security sandbox. Neither can override upstream HTTP429 or end a browser stream.

## Identity and admission

The guard locates the exact current toolCallId in at most 64 ancestor session entries, then validates that assistant message's Chappie request ID. Both UUID/suffix and wfr_<32 hex>/suffix forms are supported. It never guesses from the newest unrelated message. Missing identity is reported as untracked and, when enabled, new work is blocked rather than charged to another client.

A per-request checkpoint is reserved before execution. Deduplication covers the same call ID within that request; arbitrary new call IDs or new requests can repeat an operation. Successful admission captures exact call/request identity in a runtime map, scoped by cwd and session, with at most 1024 unresolved entries. Pending entries are never evicted to admit newer work. Results use that association even after more than 64 newer session entries, and admitted results remain accounted when policy is subsequently disabled. Missing associations or reservations fail closed. An unresolved reservation outside the current runtime map blocks further admission for that request; recovery does not clear it.

On reaching the call/output/time allowance, new tools are blocked with the precise checkpoint path. Only read-only `tunneldock_recover` remains available. This is not proof that the model will end its reply; it prevents additional guarded native admissions. All required evidence must still be inspected before a final research judgment, possibly in a later turn. A checkpoint is not completed review.

`output_bytes` counts the UTF-8 encoding of JSON containing the content, details and error flag observed by this result hook. This includes text, base64 image data, content metadata and JSON escaping. It does not count decoded image bytes, raw network traffic, prompts, or subsequent hook changes. Images pass through intact. A single result can exceed the allowance before the next admission stops. Extension order matters: when flow runs first, stream observes its bounded text and metadata artifact references; when stream runs first, it observes the original result. Neither ordering makes this a raw response-size ceiling.

## Persistence and recovery

`.tunneldock/web_turns/<request-id>.json` contains project/original-session/request IDs, counters, timestamps, last observed call name/ID, error flag and bounded hashed pending/finished call sets. Error text is retained intact when it fits the 8 KiB record; otherwise `last_error_text_omitted` explicitly reports omission. Prompts, inputs and successful result bodies are not copied. Error text may contain sensitive evidence or filesystem paths and should receive the same access protection as tool output. Records are published using synced temporary files and atomic rename. These are cooperative, per-file guarantees, not a distributed transaction or power-loss guarantee.

Unresolved accounting and persistence failures visibly latch further guarded admissions in memory. The extension best-effort publishes a bounded `.tunneldock/stream-failure-<session-id>.json` latch, checked across extension reloads for that same session. Oversized reasons are omitted with an explicit flag. If publication fails, the error explicitly states that memory-only protection cannot be guaranteed after reload. A surviving pending checkpoint still records uncertainty, but cannot replace a missing durable failure record across requests or sessions. Latches are never automatically cleared; receipt/artifact reconciliation and deliberate operator remediation are required. Workers are never killed.

`tunneldock_recover` reads authoritative current `web_status.json` and only the exact current request checkpoint by default. A new request with no checkpoint stays unselected; project-wide `latest.json` is never used to select another client's work. An explicit `request_id` is required for an older checkpoint. `session_id` identifies the current runtime, while `checkpoint_session_id` preserves the checkpoint's original session.

Recovery reports pending/finished counts and the last observed call ID/name/error. Any pending reservation or failure latch means `EXECUTION_UNCERTAIN`: reservation happened before execution, so neither execution nor safe replay can be inferred. Reconcile receipts and artifacts before any retry. `RESULT_OBSERVED` means a result hook was observed, including possible errors or another hook's blocked result; it is not scientific acceptance or proof of successful execution. `NO_RESERVATION_OBSERVED` is only an observation of the selected checkpoint, not authorization to repeat historical work.

Recovery performs no writes, increments no counters, resubmits no tasks, and never converts a delivered handoff to accepted. Output is bounded as a whole to 8 KiB. Complete paths and error text are preserved when they fit; oversized fields or rows are omitted with explicit flags/counts, never shortened into invalid filenames. Historical run snapshots cannot replace current active-run IDs. Full original task/handoff evidence remains in existing stores. Delegate status shares wait's current-task receipt eligibility: only review/completed tasks with the latest completed run and a nonempty handoff file expose a handoff; queued, active, blocked and superseded tasks cannot expose an older completed handoff.

The loaded extension emits `stream-capability.json` with its PID, session, source fingerprint and activation time. Check this and the actual native tool call; a stale capability file alone is not proof of an active guard.

## Verification

- Mounted UI replay: original c382c6b duplicates campaign children; corrected key namespaces remain stable across 360 ticks, snapshot updates, null transitions and project switches.
- Unit regressions cover explicit cross-request selection, session provenance, pending/result states, delayed batched results, unresolved-accounting latches and reload, failed latch persistence, intact paths/errors or explicit omission, and both stream/flow hook orders with images and large details. Delegate regressions cover current task/run eligibility. Run `npm test -- --run pi-extensions/tunneldock-stream.test.ts pi-extensions/tunneldock-delegate.test.ts` in the native project environment; these new regressions have not run in the restricted repair sandbox because esbuild startup failed with `spawn EPERM`.
- A disposable native Pi fixture allows two reads, blocks a third before execution, and returns the saved counters through tunneldock_recover. The fixture is not a scientific task.
- Browser/server errors still require their own network evidence; a healthy Broker or successful build does not certify the ChatGPT stream.

### Native coordinator verification, 2026-10-03

After the restricted worker handed off, the coordinator ran the current tree in the native Windows environment: 101 frontend/extension tests, 148 Rust tests, TypeScript, frontend build and standalone build passed. A temporary real Pi/Chappie session loaded the verified extension, allowed two reads, rejected a third write before execution, and reported two finished calls with zero pending reservations and no failure latch. The rejected file was absent; the owned fixture process and directory were cleaned. This validates native admission and receipt recovery, not browser reconnection. Complete commands, hashes and observations are retained under `%APPDATA%/TunnelDock/runtime/stream-reliability/`.

Chrome console screenshots separately showed HTTP 429 errors on ChatGPT backend conversation requests. These were not correlated with a specific failed response/resume request; no root-cause claim is made from that screenshot alone. No authenticated request replay, rate-limit bypass, proxy change or browser refresh loop is implemented.
