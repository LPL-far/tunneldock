# Agent activity and review receipts

Project Rooms show **Codex and Antigravity/Gemini side by side**. The panel is independent of the selected Memory/Tasks/Discussion tab. It polls the bounded `agent_activity.json` through a read-only IPC command, including when there was no active run on initial page load.

## Meaning of states

`queued -> running -> received -> reviewing -> reviewed` are recorded lifecycle states, not a guessed percentage. `review.started` marks an actual Web reviewer beginning to inspect a specific task/run; opening a handoff preview or copying a prompt never marks it reviewed. Local completion-manifest acceptance is labeled **contract checked, not Web reviewed**. A durable consultation with pending memory/cleanup is `finalizing`, not finalized.

A retry reuses the task ID but gets a new run ID. While queued it must not display the previous run's handoff as a new result. `review.started` requires exact task/run IDs; `task.review` accepts an optional run ID for backward compatibility and rejects it when stale. Acceptance always checks the latest attempt, not an earlier successful run.

The panel displays the human and automation thread IDs separately and copies them without opening/resuming the Codex Desktop thread. This avoids changing thread ownership merely to show activity. Gemini displays its Cascade identity. `PROGRESS.json` contains only event count/type/timestamp, never model reasoning, command arguments or raw logs. No progress percentage or completion ETA is fabricated. Lack of recent events is labeled as such; it is not itself proof of failure.

## Review receipt example

```json
{"kind":"review.started","author":"chatgpt","task_id":"TASK-...","run_id":"RUN-..."}
```

Read the complete handoff in bounded pages and verify relevant source/tests before:

```json
{"kind":"task.review","author":"chatgpt","task_id":"TASK-...","run_id":"RUN-...","decision":"accept","summary":"Evidence actually checked..."}
```

For consultations use the existing `consult.reviewed` and required durable gates. The panel lists pending receipt IDs and produces a focused recovery prompt; it never self-accepts scientific results. Handoff preview is paginated at 4096 bytes, read-only and constrained to a run belonging to the selected project's run directory.

## Browser boundary

Automatic **receipt** works while the ChatGPT page is not generating. Actual Web review still needs an active ChatGPT execution. TunnelDock has no supported API in this integration to wake or finish another inactive ChatGPT browser reply. This limitation is displayed, not hidden by calling a worker's completion "Web-reviewed". The panel preserves results across browser disconnects and makes the outstanding review visible without repeatedly asking for status.

## Output backpressure

`pi-extensions/tunneldock-flow.ts` hooks Pi's `tool_result` contract. Only a session with an exact-cwd `.tunneldock/session_binding.json` is affected. Text beyond 8192 UTF-8 bytes is stored intact under `.tunneldock/output_cache/<sha256>.txt`; the returned excerpt carries its path and explicitly says it is partial. Images and the original error flag are retained. Oversized metadata is stored separately. Identical text is content-addressed, not copied per poll. Storage failure returns an explicit error and warns against rerunning a possibly already-executed write.

The extension is installed before launching durable project Pi. Existing active sessions require a safe extension reload or restart to load it. Installing a file is **not** evidence that an already-running Pi loaded the hook; deployment does not terminate ongoing research sessions. Full-output files are operational evidence, not canonical research memory, and are not silently deleted by this hook.

UI refresh is single-flight. A previous project's response cannot overwrite the selected project. Only lifecycle changes reload the larger room snapshot; frequent progress events do not. Background updates never replace unsaved configuration/memory form drafts.
