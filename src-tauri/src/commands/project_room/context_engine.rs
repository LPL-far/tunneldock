//! Bounded, local derived context; never rewrites canonical scientific memory.
use super::*;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const ENGINE: &str = include_str!("../../../../runtime/context-engine.cjs");

pub(super) fn status(config: &ProjectRoomConfig) -> Value {
    let path = Path::new(&config.local_root)
        .join(BRIDGE_DIR)
        .join("context/status.json");
    match fs::metadata(&path) {
        Ok(meta) if meta.len() <= 32_768 => {
            read_json(&path).unwrap_or_else(|error| json!({"phase":"unavailable","error":error}))
        }
        Ok(_) => json!({"phase":"unavailable","error":"Context status exceeds budget"}),
        Err(_) => json!({"phase":"not_indexed","canonical_modified":false}),
    }
}

fn configs(state: &AppState) -> Result<Vec<ProjectRoomConfig>, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(state)?
        .iter()
        .map(|id| read_json::<ProjectRoomConfig>(&project_dir(state, id)?.join(CONFIG_FILE)))
        .filter(|item| !matches!(item, Ok(config) if !config.enabled))
        .collect()
}

fn refresh(state: &AppState, config: &ProjectRoomConfig) -> Result<(), String> {
    let node = crate::utils::cmd::find_executable("node").ok_or("Node.js is unavailable")?;
    let runtime = state.app_data_dir.join("runtime");
    fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
    let script = runtime.join("context-engine.cjs");
    if fs::read_to_string(&script).ok().as_deref() != Some(ENGINE) {
        fs::write(&script, ENGINE).map_err(|e| e.to_string())?;
    }
    let error_log = runtime.join(format!("context-{}.log", config.id));
    let stderr = File::create(error_log).map_err(|e| e.to_string())?;
    let mut command = Command::new(node);
    command
        .arg(&script)
        .arg("refresh")
        .arg(&config.local_root)
        .arg(&config.id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr));
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if let Some(exit) = child.try_wait().map_err(|e| e.to_string())? {
            return if exit.success() {
                Ok(())
            } else {
                Err(format!(
                    "Context refresh exited {exit}; inspect context-{}.log",
                    config.id
                ))
            };
        }
        if start.elapsed() > Duration::from_secs(40) || state.cleanup_in_progress() {
            let killed = child.kill();
            let waited = child.wait();
            return Err(format!(
                "Context refresh cancelled/timed out; kill={killed:?}, wait={waited:?}"
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn start(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            if state.cleanup_in_progress() {
                break;
            }
            let worker_state = state.clone();
            let worker_app = app.clone();
            // One bounded worker batch, outside the task-store lock and UI thread.
            let result = tokio::task::spawn_blocking(move || match configs(&worker_state) {
                Ok(configs) => {
                    for config in configs {
                        if worker_state.cleanup_in_progress() {
                            break;
                        }
                        if let Err(error) = refresh(&worker_state, &config) {
                            eprintln!("Context engine {}: {error}", config.id);
                        }
                        let _ = worker_app
                            .emit("project-context-updated", json!({"project_id":config.id}));
                    }
                }
                Err(error) => eprintln!("Cannot load context projects: {error}"),
            })
            .await;
            if let Err(error) = result {
                eprintln!("Context worker failed: {error}");
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

#[tauri::command]
pub async fn query_project_context(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    action: String,
    query: Option<String>,
    target: Option<String>,
) -> Result<Value, String> {
    if !matches!(action.as_str(), "search" | "impact" | "status") {
        return Err("Unsupported context query action".into());
    }
    let config: ProjectRoomConfig =
        read_json(&project_dir(&state, &project_id)?.join(CONFIG_FILE))?;
    let script = state.app_data_dir.join("runtime/context-engine.cjs");
    let value = if action == "search" {
        query.unwrap_or_default()
    } else {
        target.unwrap_or_default()
    };
    if value.len() > 1200 || (action != "status" && value.trim().is_empty()) {
        return Err("Query/path is empty or too long".into());
    }
    let node = tokio::task::spawn_blocking(|| crate::utils::cmd::find_executable("node"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Node.js is unavailable")?;
    let mut command = Command::new(node);
    command
        .arg(script)
        .arg(action)
        .arg(config.local_root)
        .arg(value)
        .stdin(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child_command = tokio::process::Command::from(command);
    child_command.kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(25), child_command.output())
        .await
        .map_err(|_| "Context query timed out after 25 seconds".to_string())?
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(1000)
            .collect());
    }
    if output.stdout.len() > 32768 {
        return Err("Context query response exceeded budget".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}
