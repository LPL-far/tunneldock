# Codex Background Controller

TunnelDock deliberately separates the Codex conversation a researcher uses manually from the thread used for automated Project Room work.

## Problem

A Codex Desktop conversation is held by the Desktop app-server as an active writer. `codex queue` can enqueue text for that conversation, but execution may depend on the Desktop session being loaded/activated. A second app-server cannot `thread/resume` the same human thread because the writer lock correctly rejects it.

This makes direct automation against the human thread unsuitable for multiple Project Rooms: the researcher would have to switch Desktop conversations to make queued work advance.

## Design

Each Project Room stores two Codex identities:

- `codex_thread_id`: the existing **human/canonical thread** the researcher was already using.
- `codex_automation_thread_id`: a TunnelDock-managed **automation thread**.

On the first automated Codex task:

1. TunnelDock generates a per-run 256-bit capability token, writes it to a short-lived temp file, then starts a localhost-only `codex app-server --listen ws://127.0.0.1:<ephemeral-port> --ws-auth capability-token --ws-token-file <temp>` process.
2. It connects with the official app-server JSON-RPC protocol using `Authorization: Bearer <capability-token>`, then deletes the temp token file after the authenticated connection succeeds.
3. It forks the configured human thread with `thread/fork`. The fork inherits the human thread's context and workspace; TunnelDock does not move the code into the Project Room directory.
4. It names the fork `[TunnelDock] <Project Name>` and persists the returned thread ID as `codex_automation_thread_id`.
5. It starts the task with `turn/start` and then closes the WebSocket. The app-server continues the turn in the background.
6. The normal Project Room supervisor reads the automation rollout and detects `task_complete`, writes `HANDOFF.md`, then terminates the temporary app-server process.

On later tasks, TunnelDock starts a fresh temporary app-server and calls `thread/resume` on the persistent automation thread before `turn/start`. The thread therefore keeps automation continuity while the app-server process is disposable.

```text
Codex Desktop
  human thread A  ─────────────── researcher/manual use
        │ first automation only
        └── thread/fork
              ↓
       [TunnelDock] Project A ─── persistent automation thread
              ↑ resume                    ↑ resume
        temp app-server #1          temp app-server #N
              │ turn/start                │ turn/start
              └── Project Room run/handoff┘

Project B and Project C have their own automation threads and temporary app-servers,
so they can execute concurrently without changing the Desktop foreground chat.
```

## Workspace semantics

The human thread is the context/workspace seed. The automation fork inherits that workspace. `<project>/.project_memory` and `.tunneldock` are coordination/memory roots, not replacement code workspaces.

For consultation tasks TunnelDock applies a read-only sandbox (network enabled for evidence lookup/remote reads). For implementation tasks it applies workspace-write with network enabled and routes approval requests through Codex automatic review.

## Lifecycle and failure handling

- Temporary app-server PID is stored in the Project Room run and is owned by TunnelDock. WebSocket RPC reads/writes have bounded timeouts so a stuck app-server cannot block the web coordinator forever.
- `turn/start` may continue after the WebSocket client disconnects; TunnelDock does not need a long-lived WebSocket connection.
- Completion is detected from the automation thread rollout using its starting byte offset.
- On completion/failure TunnelDock terminates the temporary app-server, releasing the automation thread writer lock.
- If the app-server exits before producing a handoff, the run becomes blocked instead of waiting forever.
- If the stored automation thread was deleted externally, TunnelDock may fork a replacement from the human thread.
- If the automation thread already has an active writer, TunnelDock reports it as busy rather than stealing the writer.
- Changing the configured human thread clears the automation-thread binding so the next task forks from the new source.

## UI / multi-thread monitor

The Project Room list shows, for every project at the same time:

- Codex state: `unbound`, `ready`, `idle`, or `running`;
- shortened human thread ID;
- shortened automation thread ID;
- active run ID when running.

This monitor is independent from Codex Desktop's foreground conversation. The Desktop app can remain on any chat while multiple Project Rooms run in the background.

## Transport health semantics

Codex quota/runtime telemetry and task communication health are intentionally separate signals:

- `capacities[].available=true` means the Codex runtime/quota probe is available. It does **not** prove that a Project Room task can currently complete end to end.
- `transports[]` is the authoritative Project Room communication-health view. It records `unbound / untested / running / healthy / degraded`, the active run (if any), and the most recent success/failure evidence.
- Each background Codex app-server is **per-run and ephemeral**. Its localhost port is expected to disappear after the run completes and TunnelDock terminates the worker process. Probing a finished run's old `/readyz` or `/healthz` endpoint is therefore not a valid current-health check.
- `healthy` means a real end-to-end task reached a Codex final response and TunnelDock recovered the handoff. `degraded` means the most recent E2E transport evidence is a real dispatch/controller/poll failure.

Transport evidence is persisted in the Project Room operational store (`transport_health.json`) and mirrored into `project_room.json` / `web_context.json`; the research memory is not polluted with routine transport telemetry.

## Acceptance evidence

On 2026-10-02 the controller transport was exercised against all three existing Project Room automation threads simultaneously, without activating or switching any Codex Desktop conversation. Three independent localhost app-server processes resumed Point Tracking, IQA Agent, and 3D+MLLM automation threads at the same time; all three turns completed successfully in roughly 7–8 seconds and returned their distinct expected responses. No human/canonical thread received the diagnostic messages, and no temporary app-server or capability-token file remained afterward.

This verifies the property the old `codex queue` path could not guarantee: Project Room execution is driven by thread ID and app-server ownership, not by which Codex Desktop conversation is visible in the UI.

A later IQA Agent incident also established an important lifecycle rule: a finished run's old app-server port may have no listener even though Codex transport is healthy. Health must come from the latest end-to-end run evidence, not stale endpoint probing.

## What is canonical?

The human thread remains the user's manual conversation. The automation thread is a worker execution history. Durable project truth does not rely on either chat: it is committed to `.project_memory`, reviewed by ChatGPT Web, and finalized through Project Room review/memory/cleanup gates.
