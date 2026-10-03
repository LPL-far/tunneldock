use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::audit::{redact_audit_json, redact_audit_text};
use crate::models::{McpCallRecord, TunnelSettings, WorkspaceItem};
use crate::utils::cmd::{is_process_running, kill_process_tree};
use crate::utils::paths::{ensure_chappie_yaml_synced, get_chappie_yaml_path};
use crate::utils::time::normalize_timestamp_to_local;

const APP_DATA_DIR_NAME: &str = "TunnelDock";
const LEGACY_APP_DATA_DIR_NAMES: [&str; 2] = ["local-mcp-console", "chappie-desktop"];
const PERSISTED_FILES: [&str; 3] = ["settings.json", "workspaces.json", "history.json"];
pub(crate) const HISTORY_RECORD_LIMIT: usize = 2_000;

pub struct AppState {
    pub workspaces: Arc<Mutex<Vec<WorkspaceItem>>>,
    pub running_workspace_pids: Arc<Mutex<HashMap<String, u32>>>,
    pub running_workspace_stdins: Arc<Mutex<HashMap<String, std::process::ChildStdin>>>,
    pub running_agent_pids: Arc<Mutex<HashMap<String, u32>>>,
    pub otunnel_pid: Arc<Mutex<Option<u32>>>,
    pub otunnel_owned_pid: Arc<Mutex<Option<u32>>>,
    pub otunnel_health_url: Arc<Mutex<Option<String>>>,
    pub otunnel_health_url_file: Arc<Mutex<Option<PathBuf>>>,
    pub history: Arc<Mutex<Vec<McpCallRecord>>>,
    pub settings: Arc<Mutex<TunnelSettings>>,
    pub project_store_lock: Arc<Mutex<()>>,
    pub app_data_dir: PathBuf,
    cleanup_started: AtomicBool,
}

impl AppState {
    /// An inert test state: no user configuration, sessions or process cleanup is touched.
    #[cfg(test)]
    pub(crate) fn isolated_for_tests(app_data_dir: PathBuf) -> Self {
        Self {
            workspaces: Arc::new(Mutex::new(Vec::new())),
            running_workspace_pids: Arc::new(Mutex::new(HashMap::new())),
            running_workspace_stdins: Arc::new(Mutex::new(HashMap::new())),
            running_agent_pids: Arc::new(Mutex::new(HashMap::new())),
            otunnel_pid: Arc::new(Mutex::new(None)),
            otunnel_owned_pid: Arc::new(Mutex::new(None)),
            otunnel_health_url: Arc::new(Mutex::new(None)),
            otunnel_health_url_file: Arc::new(Mutex::new(None)),
            history: Arc::new(Mutex::new(Vec::new())),
            settings: Arc::new(Mutex::new(TunnelSettings {
                tunnel_id: String::new(), api_key: String::new(), key_file_path: String::new(),
                health_port: 0, profile_name: String::new(), locale: "en".into(),
            })),
            project_store_lock: Arc::new(Mutex::new(())),
            app_data_dir,
            cleanup_started: AtomicBool::new(true),
        }
    }

    pub fn new() -> Self {
        let app_data_dir = Self::resolve_app_data_dir();

        let settings = Self::load_settings(&app_data_dir);
        let mut workspaces = Self::load_workspaces(&app_data_dir);
        let history = Self::load_history(&app_data_dir);
        let durable_workspace_ids = Self::durable_project_workspace_ids_from_dir(&app_data_dir);
        let mut running_workspace_pids = HashMap::new();
        for workspace in &mut workspaces {
            if !durable_workspace_ids.contains(&workspace.id) {
                continue;
            }
            let pid = workspace
                .pid
                .filter(|pid| is_process_running(*pid))
                .or_else(|| Self::durable_workspace_pid_from_dir(&app_data_dir, &workspace.id));
            if let Some(pid) = pid {
                workspace.pid = Some(pid);
                running_workspace_pids.insert(workspace.id.clone(), pid);
            }
        }

        Self {
            workspaces: Arc::new(Mutex::new(workspaces)),
            running_workspace_pids: Arc::new(Mutex::new(running_workspace_pids)),
            running_workspace_stdins: Arc::new(Mutex::new(HashMap::new())),
            running_agent_pids: Arc::new(Mutex::new(HashMap::new())),
            otunnel_pid: Arc::new(Mutex::new(None)),
            otunnel_owned_pid: Arc::new(Mutex::new(None)),
            otunnel_health_url: Arc::new(Mutex::new(None)),
            otunnel_health_url_file: Arc::new(Mutex::new(None)),
            history: Arc::new(Mutex::new(history)),
            settings: Arc::new(Mutex::new(settings)),
            project_store_lock: Arc::new(Mutex::new(())),
            app_data_dir,
            cleanup_started: AtomicBool::new(false),
        }
    }

    pub(crate) fn resolve_app_data_dir() -> PathBuf {
        let base_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        Self::resolve_app_data_dir_in(&base_dir)
    }

    fn resolve_app_data_dir_in(base_dir: &Path) -> PathBuf {
        let app_data_dir = base_dir.join(APP_DATA_DIR_NAME);

        if !app_data_dir.exists() {
            for legacy_name in LEGACY_APP_DATA_DIR_NAMES {
                let legacy_app_data_dir = base_dir.join(legacy_name);
                if legacy_app_data_dir.exists() {
                    if fs::rename(&legacy_app_data_dir, &app_data_dir).is_err() {
                        let _ = fs::create_dir_all(&app_data_dir);
                        Self::copy_legacy_data(&legacy_app_data_dir, &app_data_dir);
                    }
                    break;
                }
            }
        }

        let _ = fs::create_dir_all(&app_data_dir);
        app_data_dir
    }

    fn copy_legacy_data(source_dir: &Path, target_dir: &Path) {
        for file_name in PERSISTED_FILES {
            let source = source_dir.join(file_name);
            let target = target_dir.join(file_name);
            if source.exists() && !target.exists() {
                let _ = fs::copy(source, target);
            }
        }
    }

    fn durable_project_workspace_ids_from_dir(app_data_dir: &Path) -> HashSet<String> {
        let projects_root = app_data_dir.join("projects");
        let registry_path = projects_root.join("registry.json");
        let project_ids = fs::read_to_string(&registry_path)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
            .unwrap_or_default();
        let mut workspace_ids = HashSet::new();
        for project_id in project_ids {
            let config_path = projects_root.join(project_id).join("project.json");
            let Some(value) = fs::read_to_string(&config_path)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            else {
                continue;
            };
            let enabled = value
                .get("enabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            let keep_alive = value
                .get("keep_session_alive")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let workspace_id = value
                .get("workspace_id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if enabled && keep_alive {
                if let Some(workspace_id) = workspace_id {
                    workspace_ids.insert(workspace_id.to_string());
                }
            }
        }
        workspace_ids
    }

    fn durable_workspace_pid_from_dir(app_data_dir: &Path, workspace_id: &str) -> Option<u32> {
        let path = app_data_dir
            .join("runtime")
            .join("project-pi")
            .join(workspace_id)
            .join("wrapper.pid");
        fs::read_to_string(path)
            .ok()
            .and_then(|text| text.trim().parse::<u32>().ok())
            .filter(|pid| is_process_running(*pid))
    }

    pub fn durable_project_workspace_ids(&self) -> HashSet<String> {
        Self::durable_project_workspace_ids_from_dir(&self.app_data_dir)
    }

    pub fn durable_workspace_pid(&self, workspace_id: &str) -> Option<u32> {
        Self::durable_workspace_pid_from_dir(&self.app_data_dir, workspace_id)
    }

    pub fn workspace_is_durable_project(&self, workspace_id: &str) -> bool {
        self.durable_project_workspace_ids().contains(workspace_id)
    }

    pub fn cleanup_in_progress(&self) -> bool {
        self.cleanup_started.load(Ordering::Acquire)
    }

    pub fn cleanup_all_processes(&self) {
        // Shutdown can be observed through ExitRequested, Exit and Drop. Only the
        // first caller performs cleanup so we never wait on the same children more
        // than once during application teardown.
        if self.cleanup_started.swap(true, Ordering::AcqRel) {
            return;
        }

        let durable_workspace_ids = self.durable_project_workspace_ids();

        // 1. Close RPC stdin only for UI-scoped workspaces. Durable Project Room
        // Pi workers own their stdin in an external WMI wrapper and intentionally
        // survive TunnelDock/Tauri restarts.
        self.running_workspace_stdins
            .lock()
            .retain(|workspace_id, _| durable_workspace_ids.contains(workspace_id));

        // 2. Terminate only the otunnel process actually spawned by this
        // application. A daemon merely discovered by PID may be an independent
        // control tunnel and must survive TunnelDock restarts.
        if let Some(pid) = self.otunnel_owned_pid.lock().take() {
            let _ = kill_process_tree(pid);
            let mut known_pid = self.otunnel_pid.lock();
            if *known_pid == Some(pid) {
                *known_pid = None;
            }
            drop(known_pid);
            self.clear_otunnel_runtime();
        } else {
            self.otunnel_pid.lock().take();
            self.forget_otunnel_runtime();
        }

        let workspace_pids = self.running_workspace_pids.lock().clone();
        for (workspace_id, pid) in workspace_pids {
            if !durable_workspace_ids.contains(&workspace_id) {
                let _ = kill_process_tree(pid);
            }
        }

        // Project Room agent workers are durable task processes, not UI-scoped
        // children. In particular, a Codex background app-server may be in the
        // middle of a long implementation/review when TunnelDock is restarted
        // (including Tauri dev hot reload). Killing it here would convert an
        // otherwise healthy run into `background_process_exit`. Keep the worker
        // alive; its PID is already persisted in runs.json, so the next TunnelDock
        // process can resume polling the rollout and will terminate the app-server
        // when the run completes or genuinely fails.
        //
        // Do not clear `running_agent_pids` here either: the current AppState is
        // being torn down, and retaining the map until drop makes the ownership
        // semantics explicit without touching the durable child process.

        // 3. Persist a clean stopped state for the next launch.
        let mut list = self.workspaces.lock();
        for w in list.iter_mut() {
            if durable_workspace_ids.contains(&w.id)
                && w.pid.map(is_process_running).unwrap_or(false)
            {
                continue;
            }
            w.status = "stopped".to_string();
            w.pid = None;
            w.session_id = None;
            w.binding_count = 0;
            w.error_message = None;
        }
        drop(list);
        self.save_workspaces();
    }

    fn tunnel_key_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".chappie")
            .join("tunnelkey.txt")
    }

    fn read_tunnel_key(path: &Path) -> String {
        fs::read_to_string(path)
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    fn persist_settings_file(file: &Path, settings: &TunnelSettings) {
        let mut persisted = settings.clone();
        // The API key has a single durable source: ~/.chappie/tunnelkey.txt.
        // Keep it in memory for the settings form, but never duplicate it in
        // TunnelDock's settings.json.
        persisted.api_key.clear();
        if let Ok(json) = serde_json::to_string_pretty(&persisted) {
            let _ = fs::write(file, json);
        }
    }

    fn load_settings(data_dir: &Path) -> TunnelSettings {
        let file = data_dir.join("settings.json");
        let key_file = Self::tunnel_key_path();
        let mut key = Self::read_tunnel_key(&key_file);

        if let Ok(content) = fs::read_to_string(&file) {
            if let Ok(mut settings) = serde_json::from_str::<TunnelSettings>(&content) {
                // Migrate the pre-hardening duplicate key out of settings.json.
                if key.is_empty() && !settings.api_key.trim().is_empty() {
                    if let Some(parent) = key_file.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if fs::write(&key_file, settings.api_key.trim()).is_ok() {
                        key = settings.api_key.trim().to_string();
                    }
                } else if !key.is_empty() {
                    settings.api_key = key.clone();
                }

                // 8080 was TunnelDock's legacy hard-coded default. Migrate it to
                // automatic allocation so upgrades do not retain the collision-prone
                // behavior. Users can still choose any explicit non-zero port later.
                if settings.health_port == 8080 {
                    settings.health_port = 0;
                }

                if !key.is_empty() {
                    settings.api_key = key;
                }
                settings.key_file_path = key_file.to_string_lossy().to_string();
                if key_file.exists() || settings.api_key.is_empty() {
                    Self::persist_settings_file(&file, &settings);
                }
                return settings;
            }
        }

        // Try reading existing from ~/.chappie/tunnelkey.txt or chappie.yaml if present.
        // These names belong to the Chappie/otunnel integration and are intentionally
        // retained independently from the TunnelDock product brand.
        ensure_chappie_yaml_synced();

        // Try reading tunnel_id from chappie.yaml across all standard locations.
        let mut tunnel_id = String::new();
        if let Some(yaml_file) = get_chappie_yaml_path() {
            if let Ok(yaml_content) = fs::read_to_string(yaml_file) {
                for line in yaml_content.lines() {
                    if line.trim().starts_with("tunnel_id:") {
                        tunnel_id = line
                            .trim()
                            .trim_start_matches("tunnel_id:")
                            .trim()
                            .to_string();
                        break;
                    }
                }
            }
        }

        TunnelSettings {
            tunnel_id,
            api_key: key,
            key_file_path: key_file.to_string_lossy().to_string(),
            health_port: 0,
            profile_name: "chappie".to_string(),
            locale: "zh-CN".to_string(),
        }
    }

    pub fn forget_otunnel_runtime(&self) {
        self.otunnel_health_url.lock().take();
        self.otunnel_health_url_file.lock().take();
    }

    pub fn clear_otunnel_runtime(&self) {
        self.otunnel_health_url.lock().take();
        if let Some(path) = self.otunnel_health_url_file.lock().take() {
            let _ = fs::remove_file(path);
        }
    }

    pub fn save_settings(&self) {
        let settings = self.settings.lock().clone();
        let file = self.app_data_dir.join("settings.json");
        Self::persist_settings_file(&file, &settings);
    }

    fn load_workspaces(data_dir: &Path) -> Vec<WorkspaceItem> {
        let file = data_dir.join("workspaces.json");
        if let Ok(content) = fs::read_to_string(&file) {
            if let Ok(list) = serde_json::from_str::<Vec<WorkspaceItem>>(&content) {
                // When app starts, reset running state to stopped.
                return list
                    .into_iter()
                    .map(|mut w| {
                        w.status = "stopped".to_string();
                        w.pid = None;
                        w.session_id = None;
                        w.binding_count = 0;
                        w.error_message = None;
                        w
                    })
                    .collect();
            }
        }
        Vec::new()
    }

    pub fn save_workspaces(&self) {
        let list = self.workspaces.lock().clone();
        let file = self.app_data_dir.join("workspaces.json");
        if let Ok(json) = serde_json::to_string_pretty(&list) {
            let _ = fs::write(file, json);
        }
    }

    fn load_history(data_dir: &Path) -> Vec<McpCallRecord> {
        let file = data_dir.join("history.json");
        if let Ok(content) = fs::read_to_string(&file) {
            if let Ok(list) = serde_json::from_str::<Vec<McpCallRecord>>(&content) {
                let history = Self::sanitize_history(list);
                if let Ok(json) = serde_json::to_string_pretty(&history) {
                    let _ = fs::write(&file, json);
                }
                return history;
            }
        }
        Vec::new()
    }

    fn sanitize_history(history: Vec<McpCallRecord>) -> Vec<McpCallRecord> {
        history
            .into_iter()
            .filter(|record| record.id != "init_sample_1" && record.id != "sessions_sample_2")
            .take(HISTORY_RECORD_LIMIT)
            .map(|mut record| {
                record.timestamp = normalize_timestamp_to_local(&record.timestamp);
                record.args_json = redact_audit_json(&record.args_json);
                record.result_summary = redact_audit_text(&record.result_summary);
                record
            })
            .collect()
    }

    pub fn save_history(&self) {
        let list = {
            let mut history = self.history.lock();
            if history.len() > HISTORY_RECORD_LIMIT {
                history.truncate(HISTORY_RECORD_LIMIT);
            }
            history.clone()
        };
        let file = self.app_data_dir.join("history.json");
        if let Ok(json) = serde_json::to_string_pretty(&list) {
            let _ = fs::write(file, json);
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.cleanup_all_processes();
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, HISTORY_RECORD_LIMIT};
    use crate::models::{McpCallRecord, TunnelSettings};
    use crate::utils::time::normalize_timestamp_to_local;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_data_root(case: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "tunneldock-state-test-{}-{}-{}",
            std::process::id(),
            nonce,
            case
        ))
    }

    #[test]
    fn migrates_data_from_all_pre_tunneldock_directories() {
        for legacy_name in ["local-mcp-console", "chappie-desktop"] {
            let base_dir = temporary_data_root(legacy_name);
            let legacy_dir = base_dir.join(legacy_name);
            fs::create_dir_all(&legacy_dir).expect("legacy directory should be created");
            fs::write(legacy_dir.join("settings.json"), legacy_name)
                .expect("legacy settings should be written");

            let resolved = AppState::resolve_app_data_dir_in(&base_dir);

            assert_eq!(resolved, base_dir.join("TunnelDock"));
            assert_eq!(
                fs::read_to_string(resolved.join("settings.json"))
                    .expect("migrated settings should exist"),
                legacy_name
            );
            fs::remove_dir_all(base_dir).expect("test data should be removed");
        }
    }

    #[test]
    fn detects_only_enabled_keep_alive_project_workspaces() {
        let data_dir = temporary_data_root("durable-project-workspaces");
        let projects = data_dir.join("projects");
        fs::create_dir_all(projects.join("keep")).expect("keep project dir");
        fs::create_dir_all(projects.join("disabled")).expect("disabled project dir");
        fs::create_dir_all(projects.join("no_keep")).expect("no-keep project dir");
        fs::write(
            projects.join("registry.json"),
            r#"["keep","disabled","no_keep"]"#,
        )
        .expect("registry");
        fs::write(
            projects.join("keep").join("project.json"),
            r#"{"enabled":true,"keep_session_alive":true,"workspace_id":"ws_keep"}"#,
        )
        .expect("keep config");
        fs::write(
            projects.join("disabled").join("project.json"),
            r#"{"enabled":false,"keep_session_alive":true,"workspace_id":"ws_disabled"}"#,
        )
        .expect("disabled config");
        fs::write(
            projects.join("no_keep").join("project.json"),
            r#"{"enabled":true,"keep_session_alive":false,"workspace_id":"ws_no_keep"}"#,
        )
        .expect("no-keep config");

        let ids = AppState::durable_project_workspace_ids_from_dir(&data_dir);
        assert_eq!(ids.len(), 1);
        assert!(ids.contains("ws_keep"));
        fs::remove_dir_all(data_dir).expect("cleanup");
    }

    #[test]
    fn restores_live_durable_workspace_pid_from_pidfile() {
        let data_dir = temporary_data_root("durable-project-pid");
        let workspace_id = "ws_pid";
        let runtime_dir = data_dir
            .join("runtime")
            .join("project-pi")
            .join(workspace_id);
        fs::create_dir_all(&runtime_dir).expect("runtime dir");
        fs::write(
            runtime_dir.join("wrapper.pid"),
            std::process::id().to_string(),
        )
        .expect("pid file");

        assert_eq!(
            AppState::durable_workspace_pid_from_dir(&data_dir, workspace_id),
            Some(std::process::id())
        );
        fs::remove_dir_all(data_dir).expect("cleanup");
    }

    #[test]
    fn removes_only_the_two_legacy_demo_history_records() {
        let records: Vec<McpCallRecord> = serde_json::from_str(
            r#"[
                {"id":"init_sample_1","timestamp":"x","session_id":null,"workspace_name":"系统初始化","tool_name":"init","args_json":"{}","result_summary":"demo","status":"success","duration_ms":45},
                {"id":"sessions_sample_2","timestamp":"x","session_id":null,"workspace_name":"MCP Broker","tool_name":"sessions","args_json":"{}","result_summary":"demo","status":"success","duration_ms":18},
                {"id":"real-call","timestamp":"x","session_id":"s1","workspace_name":"hola","tool_name":"read","args_json":"{}","result_summary":"ok","status":"success","duration_ms":3}
            ]"#,
        )
        .expect("fixture should deserialize");

        let sanitized = AppState::sanitize_history(records);

        assert_eq!(sanitized.len(), 1);
        assert_eq!(sanitized[0].id, "real-call");
    }

    #[test]
    fn persisted_settings_never_duplicate_the_api_key() {
        let data_dir = temporary_data_root("settings-redaction");
        fs::create_dir_all(&data_dir).expect("test data directory should be created");
        let file = data_dir.join("settings.json");
        let settings = TunnelSettings {
            tunnel_id: "tunnel_test".to_string(),
            api_key: "sk-project-secret123456".to_string(),
            key_file_path: "C:\\Users\\test\\.chappie\\tunnelkey.txt".to_string(),
            health_port: 0,
            profile_name: "chappie".to_string(),
            locale: "zh-CN".to_string(),
        };

        AppState::persist_settings_file(&file, &settings);

        let content = fs::read_to_string(&file).expect("persisted settings should be readable");
        let persisted: TunnelSettings =
            serde_json::from_str(&content).expect("persisted settings should remain valid JSON");
        assert!(!content.contains("sk-project-secret123456"));
        assert!(persisted.api_key.is_empty());
        fs::remove_dir_all(data_dir).expect("test data should be removed");
    }

    #[test]
    fn sanitize_history_redacts_secrets_and_limits_retention() {
        let records = (0..(HISTORY_RECORD_LIMIT + 5))
            .map(|index| McpCallRecord {
                id: format!("call-{index}"),
                timestamp: "2026-09-18T03:04:04+00:00".to_string(),
                session_id: None,
                workspace_id: None,
                workspace_name: Some("workspace".to_string()),
                tool_name: "write".to_string(),
                args_json: r#"{"api_key":"sk-project-secret123456"}"#.to_string(),
                result_summary: "Bearer abcdefghijklmnop".to_string(),
                status: "success".to_string(),
                duration_ms: 1,
                input_tokens: 1,
                output_tokens: 1,
                total_tokens: 2,
            })
            .collect();

        let sanitized = AppState::sanitize_history(records);

        assert_eq!(sanitized.len(), HISTORY_RECORD_LIMIT);
        assert!(!sanitized[0].args_json.contains("sk-project-secret123456"));
        assert!(!sanitized[0].result_summary.contains("abcdefghijklmnop"));
        assert!(sanitized[0].args_json.contains("[REDACTED]"));
        assert!(sanitized[0].result_summary.contains("[REDACTED]"));
    }

    #[test]
    fn migrates_legacy_history_timestamps_and_persists_the_local_offset() {
        let data_dir = temporary_data_root("history-timezone");
        fs::create_dir_all(&data_dir).expect("test data directory should be created");
        fs::write(
            data_dir.join("history.json"),
            r#"[{
                "id":"legacy-call",
                "timestamp":"2026-09-18 03:04:04",
                "session_id":null,
                "workspace_name":"hola",
                "tool_name":"read",
                "args_json":"{}",
                "result_summary":"ok",
                "status":"success",
                "duration_ms":10
            }]"#,
        )
        .expect("legacy history should be written");

        let history = AppState::load_history(&data_dir);
        let expected = normalize_timestamp_to_local("2026-09-18 03:04:04");
        let persisted: Vec<McpCallRecord> = serde_json::from_str(
            &fs::read_to_string(data_dir.join("history.json"))
                .expect("migrated history should be readable"),
        )
        .expect("migrated history should remain valid JSON");

        assert_eq!(history[0].timestamp, expected);
        assert_eq!(persisted[0].timestamp, expected);
        fs::remove_dir_all(data_dir).expect("test data should be removed");
    }
}
