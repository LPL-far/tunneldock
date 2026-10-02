export type EnvStatus = "ready" | "missing" | "outdated" | "warning" | "config_needed";

export interface EnvCheckItem {
  id: string;
  name: string;
  category: "runtime" | "tools" | "mcp" | "credentials";
  installed: boolean;
  version: string | null;
  required_version: string | null;
  path: string | null;
  status: EnvStatus;
  message: string;
  can_auto_install: boolean;
}

export interface InstallProgressEvent {
  item_id: string;
  stage: string;
  log_line: string;
  is_error: boolean;
}

export interface DoctorCheckItem {
  name: string;
  status: "PASS" | "FAIL" | "SKIP";
  details: string;
  suggestion: string | null;
}

export interface DoctorReport {
  overall: "PASS" | "FAIL";
  items: DoctorCheckItem[];
  raw_output: string;
}

export interface OtunnelDaemonStatus {
  running: boolean;
  pid: number | null;
  healthz_ok: boolean;
  readyz_ok: boolean;
  latency_ms: number | null;
  listen_port: number | null;
  health_base_url: string | null;
  tunnel_id: string | null;
  uptime_seconds: number | null;
}

export type WorkspaceStatus = "stopped" | "starting" | "ready" | "executing" | "error";

export interface WorkspaceItem {
  id: string;
  name: string;
  path: string;
  status: WorkspaceStatus;
  session_id: string | null;
  pid: number | null;
  binding_count: number;
  git_branch: string | null;
  git_status: string | null;
  last_started_at: string | null;
  error_message: string | null;
}

export interface ProjectRemote {
  host: string;
  root: string;
  environment: string;
  notes: string;
}

export interface ProjectAgentPolicy {
  agent_id: string;
  display_name: string;
  role: string;
  strengths: string[];
  risk_notes: string[];
  review_rule: string;
  enabled: boolean;
}

export interface AgentQuotaWindow {
  id: string;
  label: string;
  window: string;
  remaining_percent: number;
  reset_at: string | null;
}

export interface AgentCapacity {
  agent_id: string;
  available: boolean;
  remaining_percent: number | null;
  reset_at: string | null;
  model: string | null;
  source: string;
  confidence: string;
  updated_at: string;
  quota_windows: AgentQuotaWindow[];
}

export interface MemoryFileHealth {
  file: string;
  bytes: number;
  budget_bytes: number;
  utilization: number;
  status: string;
}

export interface MemoryHealth {
  total_current_bytes: number;
  total_archive_bytes: number;
  ledger_events: number;
  requires_compaction: boolean;
  near_budget: boolean;
  files: MemoryFileHealth[];
  updated_at: string;
}

export interface AgentTransportHealth {
  agent_id: string;
  status: string;
  active_run_id: string | null;
  last_success_at: string | null;
  last_failure_at: string | null;
  last_error: string | null;
  source: string;
  updated_at: string;
}

export interface ProjectMemory {
  memory_index: string;
  project_state: string;
  session_handoff: string;
  decisions: string;
  model_design: string;
  data_catalog: string;
  experiments: string;
  results: string;
  references: string;
  documents: string;
  memory_protocol: string;
  updated_at: string;
}

export interface ProjectTask {
  id: string;
  title: string;
  goal: string;
  owner: string;
  reviewers: string[];
  status: string;
  write_scope: string[];
  summary: string;
  kind: string;
  thread_id: string;
  consultation_id: string;
  web_reviewed: boolean;
  memory_committed: boolean;
  cleanup_committed: boolean;
  auto_dispatch: boolean;
  finalization_policy: string;
  created_at: string;
  updated_at: string;
}

export interface ProjectDiscussionMessage {
  id: string;
  thread_id: string;
  author: string;
  recipients: string[];
  message: string;
  created_at: string;
}

export interface ProjectExperiment {
  id: string;
  hypothesis: string;
  code_revision: string;
  command: string;
  config: string;
  dataset: string;
  metrics: string;
  result: string;
  analysis: string;
  artifacts: string[];
  status: string;
  created_at: string;
  updated_at: string;
}

export interface ProjectRoomConfig {
  id: string;
  name: string;
  local_root: string;
  repo_root: string;
  workspace_id: string | null;
  codex_thread_id: string | null;
  codex_automation_thread_id: string | null;
  antigravity_cascade_id: string | null;
  remote: ProjectRemote;
  enabled: boolean;
  keep_session_alive: boolean;
  created_at: string;
  updated_at: string;
}

export interface ProjectRoomSummary {
  id: string;
  name: string;
  local_root: string;
  repo_root: string;
  workspace_id: string | null;
  codex_thread_id: string | null;
  codex_automation_thread_id: string | null;
  codex_status: string;
  codex_active_run_id: string | null;
  codex_last_success_at: string | null;
  codex_last_failure_at: string | null;
  codex_last_error: string | null;
  session_status: WorkspaceStatus;
  session_id: string | null;
  binding_count: number;
  remote_configured: boolean;
  git_initialized: boolean;
  active_tasks: number;
  experiments: number;
  discussion_messages: number;
  memory_updated_at: string;
}

export interface AgentRun {
  id: string;
  task_id: string;
  agent_id: string;
  status: string;
  pid: number | null;
  external_session_id: string | null;
  start_step: number | null;
  started_at: string;
  finished_at: string | null;
  prompt_path: string;
  output_path: string;
  log_path: string;
  error_path: string;
  error_message: string | null;
}

export interface AgentRuntimeInfo {
  agent_id: string;
  installed: boolean;
  executable: string | null;
  version: string | null;
  dispatch_mode: string;
  notes: string;
}

export interface ProjectRoomSnapshot {
  config: ProjectRoomConfig;
  memory: ProjectMemory;
  memory_health: MemoryHealth;
  agents: ProjectAgentPolicy[];
  capacities: AgentCapacity[];
  transports: AgentTransportHealth[];
  tasks: ProjectTask[];
  experiments: ProjectExperiment[];
  discussion: ProjectDiscussionMessage[];
  runs: AgentRun[];
}

export interface HygieneCandidate {
  path: string;
  kind: string;
  reason: string;
}

export interface McpCallRecord {
  id: string;
  timestamp: string;
  session_id: string | null;
  workspace_id: string | null;
  workspace_name: string | null;
  tool_name: string;
  args_json: string;
  result_summary: string;
  status: "success" | "executing" | "error";
  duration_ms: number;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
}

export type Locale = "zh-CN" | "en-US";

export interface TunnelSettings {
  tunnel_id: string;
  api_key: string;
  key_file_path: string;
  health_port: number;
  profile_name: string;
  locale: string;
}
