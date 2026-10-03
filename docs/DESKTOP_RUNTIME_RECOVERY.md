# Desktop runtime and Codex ownership recovery

## Desktop lifecycle

Daily use runs a standalone, embedded-frontend executable, not `tauri dev` or a Vite development server. The deployed executable is `%LOCALAPPDATA%\TunnelDock\app\TunnelDock.exe`; project state remains under the existing `%APPDATA%\TunnelDock` store.

`app_supervisor.rs` claims an OS-exclusive `main.lock` BEFORE constructing AppState. A duplicate launch requests the existing window instead of creating another task dispatcher. The lock is automatically released on process death; its filename is not treated as liveness evidence.

A hash-named native copy of the same executable runs `--desktop-guardian <state-directory>`. It does not initialize Tauri, Project Rooms or workers. WMI starts it without a console and outside the UI process tree. Keeping a separate immutable copy avoids locking the installed UI binary during replacement. Direct WMI-to-PowerShell supervision was rejected with Access Denied on the current machine and is not the production watcher.

The guardian checks desktop PID, process birth time and executable identity. A recycled PID is an error, not a live desktop. It restarts only the configured executable and arguments, with 2/5/10-second backoff and at most three recovery attempts per five minutes. It does not kill a live but slow application. Windows shutdown, an explicit exit, updater suspension and Tauri-managed restart are not crashes. `tauri dev` retains ownership of its own development process lifecycle.

The UI checks guardian lock ownership as well: a watcher which exits during a previous-UI/replacement-UI handover is restarted with bounded attempts. A successful WMI process-create return code alone is not reported as healthy supervision.

### Operational evidence

Files under `%APPDATA%\TunnelDock\runtime\app-supervisor`:

- `state.json`: UI instance identity, current exit intent and setup readiness.
- `status.json`: guardian PID, observed UI identity and recovery status.
- `events.jsonl`: bounded recovery/event log, rotated at 256 KiB.
- `launch.json`: guardian creation result; not a substitute for `status.json` or the live process.

Both the themed Exit button and tray Exit persist intent before shutting down. Minimize/Cancel do not suppress crash recovery. Updater installation explicitly suspends recovery and restores it if installation returns/fails. The existing Project Room Pi/agent processes are never killed by this guardian.

Build the independent frontend and executable:

```powershell
npm run build
cargo build --release --features tauri/custom-protocol --manifest-path src-tauri/Cargo.toml
```

Tests:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
python src-tauri/tests/desktop_supervisor_windows.py
```

The Python tests use temporary, isolated process leases; they do not kill the real application or research workers. They cover intentional exit, update suspension, PID reuse refusal, duplicate watcher prevention, and bounded crash recovery.

## Codex writer ownership

An `active writer` error means the thread store is owned, not necessarily that an inference turn is running. The 3D incident's lock holder was the existing Codex Desktop app-server (PID 50888), while the latest stored automation turn had completed. No lock was deleted and Desktop was not terminated.

The controller now refuses a duplicate managed running/interactive run before dispatch. On a writer-ownership conflict only, it reads thread metadata plus ONE newest turn summary with `thread/turns/list(limit=1, itemsView=notLoaded)`. Reading all 163 historical turns produced a 32,141,892-byte reply exceeding the 16 MiB WebSocket limit; the tested newest-turn page was 535 bytes. Do not increase browser payloads or blindly raise message limits to fix this.

Recovery is allowed only from an identified completed turn, an unchanged source rollout and the same inherited cwd. The native fork pins `lastTurnId`, preserves the latest automation history (not the obsolete human seed), and creates one successor. Active, unresolved, failed-latest, unknown or changing histories fail closed. The old thread and its lock holder remain untouched.

`THREAD_RECOVERY.json` records old/new thread IDs, the completed-turn boundary, original error and whether the task turn was actually accepted. Project configuration changes only after successful dispatch. If turn submission fails or its outcome is uncertain, the controller does not blindly submit a duplicate task.

This is transport/session recovery, not a scientific branch or a new copy of the project code. The existing task ID, scope and validation gates remain authoritative. Gemini validation must follow actual implementation evidence; a validation failure against pre-fix code is not evidence that a completed fix is wrong.
