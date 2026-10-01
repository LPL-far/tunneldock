use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvCheckItem {
    pub id: String,
    pub name: String,
    pub category: String, // "runtime", "tools", "mcp", "credentials"
    pub installed: bool,
    pub version: Option<String>,
    pub required_version: Option<String>,
    pub path: Option<String>,
    pub status: String, // "ready", "missing", "outdated", "warning", "config_needed"
    pub message: String,
    pub can_auto_install: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProgressEvent {
    pub item_id: String,
    pub stage: String, // "starting", "downloading", "installing", "success", "failed"
    pub log_line: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheckItem {
    pub name: String,
    pub status: String, // "PASS", "FAIL", "SKIP"
    pub details: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub overall: String, // "PASS", "FAIL"
    pub items: Vec<DoctorCheckItem>,
    pub raw_output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtunnelDaemonStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub healthz_ok: bool,
    pub readyz_ok: bool,
    pub latency_ms: Option<u64>,
    pub listen_port: Option<u16>,
    pub health_base_url: Option<String>,
    pub tunnel_id: Option<String>,
    pub uptime_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceItem {
    pub id: String,
    pub name: String,
    pub path: String,
    pub status: String, // "stopped", "starting", "ready", "executing", "error"
    pub session_id: Option<String>,
    pub pid: Option<u32>,
    #[serde(default)]
    pub binding_count: u32,
    pub git_branch: Option<String>,
    pub git_status: Option<String>,
    pub last_started_at: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCallRecord {
    pub id: String,
    pub timestamp: String,
    pub session_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub workspace_name: Option<String>,
    pub tool_name: String, // "read", "bash", "edit", "write", "transfer", "sessions", "init", "chat"
    pub args_json: String,
    pub result_summary: String,
    pub status: String, // "success", "executing", "error"
    pub duration_ms: u64,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

fn default_locale() -> String {
    "zh-CN".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelSettings {
    pub tunnel_id: String,
    pub api_key: String,
    pub key_file_path: String,
    pub health_port: u16,
    pub profile_name: String,
    #[serde(default = "default_locale")]
    pub locale: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProjectRemote {
    pub host: String,
    pub root: String,
    pub environment: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectAgentPolicy {
    pub agent_id: String,
    pub display_name: String,
    pub role: String,
    pub strengths: Vec<String>,
    pub risk_notes: Vec<String>,
    pub review_rule: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentQuotaWindow {
    pub id: String,
    pub label: String,
    pub window: String,
    pub remaining_percent: f64,
    pub reset_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCapacity {
    pub agent_id: String,
    pub available: bool,
    pub remaining_percent: Option<f64>,
    pub reset_at: Option<String>,
    pub model: Option<String>,
    pub source: String,
    pub confidence: String,
    pub updated_at: String,
    #[serde(default)]
    pub quota_windows: Vec<AgentQuotaWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectMemory {
    pub project_state: String,
    pub session_handoff: String,
    pub decisions: String,
    pub experiments: String,
    pub memory_protocol: String,
    pub updated_at: String,
}

fn default_task_kind() -> String {
    "work".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectTask {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub owner: String,
    pub reviewers: Vec<String>,
    pub status: String,
    pub write_scope: Vec<String>,
    pub summary: String,
    #[serde(default = "default_task_kind")]
    pub kind: String,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub consultation_id: String,
    #[serde(default)]
    pub web_reviewed: bool,
    #[serde(default)]
    pub auto_dispatch: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectDiscussionMessage {
    pub id: String,
    pub thread_id: String,
    pub author: String,
    pub recipients: Vec<String>,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectExperiment {
    pub id: String,
    pub hypothesis: String,
    pub code_revision: String,
    pub command: String,
    pub config: String,
    pub dataset: String,
    pub metrics: String,
    pub result: String,
    pub analysis: String,
    pub artifacts: Vec<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

fn default_keep_session_alive() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectRoomConfig {
    pub id: String,
    pub name: String,
    pub local_root: String,
    pub repo_root: String,
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub codex_thread_id: Option<String>,
    #[serde(default)]
    pub antigravity_cascade_id: Option<String>,
    pub remote: ProjectRemote,
    pub enabled: bool,
    #[serde(default = "default_keep_session_alive")]
    pub keep_session_alive: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectRoomSummary {
    pub id: String,
    pub name: String,
    pub local_root: String,
    pub repo_root: String,
    pub workspace_id: Option<String>,
    pub session_status: String,
    pub session_id: Option<String>,
    pub binding_count: u32,
    pub remote_configured: bool,
    pub git_initialized: bool,
    pub active_tasks: usize,
    pub experiments: usize,
    pub discussion_messages: usize,
    pub memory_updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRun {
    pub id: String,
    pub task_id: String,
    pub agent_id: String,
    pub status: String,
    pub pid: Option<u32>,
    #[serde(default)]
    pub external_session_id: Option<String>,
    #[serde(default)]
    pub start_step: Option<usize>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub prompt_path: String,
    pub output_path: String,
    pub log_path: String,
    pub error_path: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRuntimeInfo {
    pub agent_id: String,
    pub installed: bool,
    pub executable: Option<String>,
    pub version: Option<String>,
    pub dispatch_mode: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectRoomSnapshot {
    pub config: ProjectRoomConfig,
    pub memory: ProjectMemory,
    pub agents: Vec<ProjectAgentPolicy>,
    pub capacities: Vec<AgentCapacity>,
    pub tasks: Vec<ProjectTask>,
    pub experiments: Vec<ProjectExperiment>,
    pub discussion: Vec<ProjectDiscussionMessage>,
    pub runs: Vec<AgentRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HygieneCandidate {
    pub path: String,
    pub kind: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::{
        AgentCapacity, AgentRun, McpCallRecord, ProjectRoomConfig, ProjectTask, TunnelSettings,
        WorkspaceItem,
    };

    #[test]
    fn old_history_records_default_workspace_identity_and_tokens() {
        let legacy = r#"{
            "id":"legacy-1",
            "timestamp":"2026-09-16 03:14:11",
            "session_id":null,
            "workspace_name":"旧工作区",
            "tool_name":"read",
            "args_json":"{}",
            "result_summary":"ok",
            "status":"success",
            "duration_ms":12
        }"#;

        let record: McpCallRecord =
            serde_json::from_str(legacy).expect("legacy history should still load");

        assert_eq!(record.workspace_id, None);
        assert_eq!(record.input_tokens, 0);
        assert_eq!(record.output_tokens, 0);
        assert_eq!(record.total_tokens, 0);
    }

    #[test]
    fn legacy_workspace_without_binding_count_defaults_to_zero() {
        let legacy = r#"{
            "id":"ws-legacy",
            "name":"legacy",
            "path":"D:\\legacy",
            "status":"stopped",
            "session_id":null,
            "pid":null,
            "git_branch":null,
            "git_status":null,
            "last_started_at":null,
            "error_message":null
        }"#;

        let workspace: WorkspaceItem =
            serde_json::from_str(legacy).expect("legacy workspace should still load");

        assert_eq!(workspace.binding_count, 0);
    }

    #[test]
    fn legacy_agent_capacity_without_quota_windows_defaults_empty() {
        let legacy = r#"{
            "agent_id":"gemini",
            "available":true,
            "remaining_percent":19.0,
            "reset_at":"2026-10-02T01:15:23Z",
            "model":"Gemini Models",
            "source":"antigravity_quota_summary",
            "confidence":"runtime_telemetry",
            "updated_at":"2026-10-01T18:00:00+08:00"
        }"#;

        let capacity: AgentCapacity =
            serde_json::from_str(legacy).expect("legacy capacity should still load");
        assert!(capacity.quota_windows.is_empty());
    }

    #[test]
    fn legacy_project_room_records_get_new_collaboration_defaults() {
        let task: ProjectTask = serde_json::from_str(
            r#"{
                "id":"TASK-1","title":"legacy","goal":"g","owner":"codex",
                "reviewers":["chatgpt"],"status":"backlog","write_scope":[],
                "summary":"","created_at":"x","updated_at":"x"
            }"#,
        )
        .expect("legacy task should load");
        assert_eq!(task.kind, "work");
        assert!(task.thread_id.is_empty());
        assert!(task.consultation_id.is_empty());
        assert!(!task.web_reviewed);
        assert!(!task.auto_dispatch);

        let config: ProjectRoomConfig = serde_json::from_str(
            r#"{
                "id":"p","name":"P","local_root":"D:\\\\p","repo_root":"D:\\\\p",
                "workspace_id":null,"remote":{"host":"","root":"","environment":"","notes":""},
                "enabled":true,"created_at":"x","updated_at":"x"
            }"#,
        )
        .expect("legacy config should load");
        assert_eq!(config.codex_thread_id, None);
        assert_eq!(config.antigravity_cascade_id, None);
        assert!(config.keep_session_alive);

        let run: AgentRun = serde_json::from_str(
            r#"{
                "id":"RUN-1","task_id":"TASK-1","agent_id":"codex","status":"completed",
                "pid":null,"started_at":"x","finished_at":"x","prompt_path":"p",
                "output_path":"o","log_path":"l","error_path":"e","error_message":null
            }"#,
        )
        .expect("legacy run should load");
        assert_eq!(run.external_session_id, None);
        assert_eq!(run.start_step, None);
    }

    #[test]
    fn legacy_settings_without_locale_defaults_to_zh_cn() {
        let legacy = r#"{
            "tunnel_id": "tunnel_123",
            "api_key": "sk-xxx",
            "key_file_path": "/path/key",
            "health_port": 0,
            "profile_name": "chappie"
        }"#;

        let settings: TunnelSettings =
            serde_json::from_str(legacy).expect("legacy settings without locale should still load");
        assert_eq!(settings.locale, "zh-CN");
    }
}
