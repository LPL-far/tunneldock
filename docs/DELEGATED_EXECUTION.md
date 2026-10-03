# Delegated execution and activation

## Intent

The 2026-10-03 IQA latency audit showed direct Web editing/SSH loops while no Project Room worker was active. Installed extensions had not loaded in the old Pi process. This change makes delegation an explicit native tool and enables workflow routing for the authorized IQA room.

`tunneldock_delegate(action=create, request_id, title, goal, owner, write_scope)` submits one `task.create` event to the existing scheduler. It returns a task/run receipt, not an acceptance claim. Reuse the same request_id on uncertain delivery. The backend validates its reserved thread identity and refuses reuse with changed work. `action=status` reads the existing record. It never launches a second scheduler, accepts a result, clears a lock, or wakes an inactive browser.

`.tunneldock/execution-policy.json` with `delegate_execution=true` routes native write/edit calls outside coordination, diagnostics and canonical memory to workers. Known remote execution/upload command patterns are also routed. This is a limited workflow guard, NOT arbitrary-shell sandbox enforcement. Read-only investigation and Web-reviewed memory updates stay available. Do not bypass the workflow using encoded shell commands. Explicit maintenance may change the project policy.

## Activation is measured, not inferred

Installing `tunneldock-delegate.ts` is not proof a running Pi loaded it. The extension emits `delegation-capability.json` at session_start. Verify PI.tools plus one real invocation. For a legacy process, checkpoint work, check no pending tools and an idle quiet period, persist the current session ID, replace only that Pi host, and confirm the same ID and new tool catalog. Never terminate the Codex app-server, GPU process or another project to reload an extension.

## Memory responsibility

Automatic structural compaction is configured separately in `.tunneldock/memory-policy.json`. It archives full original bytes and makes continuation mandatory. It is not acceptance of scientific hypotheses. Web and the human still own interpretation. A failed scientific gate stays failed even after an operational task, a memory compaction, or a code test succeeds.

## Verification

Run `npm run test -- --run pi-extensions/tunneldock-delegate.test.ts`, the Rust regression suite, and one duplicate-ID native submission check after deployment. Record actual task ID, run ID, output path and reviewer outcome. Do not equate task creation or process liveness with completed work.
