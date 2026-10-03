import { invoke } from "@tauri-apps/api/core";
import type { ProjectActivity, HandoffPage } from '../types/activity';

export async function getProjectActivity(projectId: string): Promise<ProjectActivity> {
  return invoke<ProjectActivity>('get_project_activity', { projectId });
}
export async function readProjectHandoff(projectId: string, runId: string, offset = 0): Promise<HandoffPage> {
  return invoke<HandoffPage>('read_project_handoff', { projectId, runId, offset });
}

import {
  EnvCheckItem,
  DoctorReport,
  OtunnelDaemonStatus,
  WorkspaceItem,
  McpCallRecord,
  TunnelSettings,
  ProjectRoomSummary,
  ProjectRoomSnapshot,
  ProjectRoomConfig,
  ProjectMemory,
  ProjectTask,
  ProjectDiscussionMessage,
  ProjectExperiment,
  AgentCapacity,
  AgentRun,
  AgentRuntimeInfo,
  HygieneCandidate,
} from "../types";

// Environment API
export async function checkEnvironment(): Promise<EnvCheckItem[]> {
  return await invoke<EnvCheckItem[]>("check_environment");
}

export async function installComponent(itemId: string): Promise<boolean> {
  const ok = await invoke<boolean>("install_component_v2", { itemId });
  if (!ok) {
    throw new Error(`组件 ${itemId} 安装后未通过可用性验证`);
  }
  return true;
}

export async function uninstallComponent(itemId: string): Promise<boolean> {
  const ok = await invoke<boolean>("uninstall_component", { itemId });
  if (!ok) {
    throw new Error(`组件 ${itemId} 卸载后未通过移除验证`);
  }
  return true;
}

export async function saveTunnelCredentials(
  tunnelId: string,
  apiKey: string,
  healthPort?: number
): Promise<TunnelSettings> {
  return await invoke<TunnelSettings>("save_tunnel_credentials", {
    tunnelId,
    apiKey,
    healthPort,
  });
}

// Otunnel & Health API
export async function getOtunnelStatus(): Promise<OtunnelDaemonStatus> {
  return await invoke<OtunnelDaemonStatus>("get_otunnel_status");
}

export async function startOtunnel(): Promise<number> {
  return await invoke<number>("start_otunnel");
}

export async function stopOtunnel(): Promise<boolean> {
  return await invoke<boolean>("stop_otunnel");
}

export async function restartOtunnel(): Promise<number> {
  return await invoke<number>("restart_otunnel");
}

export async function runOtunnelDoctor(): Promise<DoctorReport> {
  return await invoke<DoctorReport>("run_otunnel_doctor");
}

export async function probeNetworkLatency(): Promise<number> {
  return await invoke<number>("probe_network_latency");
}

// Workspace API
export async function listWorkspaces(): Promise<WorkspaceItem[]> {
  return await invoke<WorkspaceItem[]>("list_workspaces");
}

export async function addWorkspace(
  path: string,
  name?: string
): Promise<WorkspaceItem> {
  return await invoke<WorkspaceItem>("add_workspace", { path, name });
}

export async function removeWorkspace(workspaceId: string): Promise<boolean> {
  return await invoke<boolean>("remove_workspace", { workspaceId });
}

export async function startWorkspaceSession(
  workspaceId: string
): Promise<number> {
  return await invoke<number>("start_workspace_session", { workspaceId });
}

export async function stopWorkspaceSession(
  workspaceId: string
): Promise<boolean> {
  return await invoke<boolean>("stop_workspace_session", { workspaceId });
}

export async function restartWorkspaceSession(
  workspaceId: string
): Promise<number> {
  return await invoke<number>("restart_workspace_session", { workspaceId });
}

export async function generateChatGptPrompt(
  path: string,
  sessionId?: string | null,
  locale?: string | null
): Promise<string> {
  return await invoke<string>("generate_chatgpt_prompt", {
    path,
    sessionId: sessionId || null,
    locale: locale || null,
  });
}

// Project Room API
export async function listProjectRooms(): Promise<ProjectRoomSummary[]> {
  return await invoke<ProjectRoomSummary[]>("list_project_rooms");
}

export async function getProjectRoom(
  projectId: string
): Promise<ProjectRoomSnapshot> {
  return await invoke<ProjectRoomSnapshot>("get_project_room", { projectId });
}

export async function updateProjectConfig(
  config: ProjectRoomConfig
): Promise<ProjectRoomSnapshot> {
  return await invoke<ProjectRoomSnapshot>("update_project_config", { config });
}

export async function updateProjectMemory(
  projectId: string,
  memory: ProjectMemory
): Promise<ProjectMemory> {
  return await invoke<ProjectMemory>("update_project_memory", {
    projectId,
    memory,
  });
}

export async function upsertProjectTask(
  projectId: string,
  task: ProjectTask
): Promise<ProjectTask> {
  return await invoke<ProjectTask>("upsert_project_task", { projectId, task });
}

export async function appendProjectMessage(
  projectId: string,
  message: ProjectDiscussionMessage
): Promise<ProjectDiscussionMessage> {
  return await invoke<ProjectDiscussionMessage>("append_project_message", {
    projectId,
    message,
  });
}

export async function upsertProjectExperiment(
  projectId: string,
  experiment: ProjectExperiment
): Promise<ProjectExperiment> {
  return await invoke<ProjectExperiment>("upsert_project_experiment", {
    projectId,
    experiment,
  });
}

export async function updateAgentCapacity(
  projectId: string,
  capacity: AgentCapacity
): Promise<AgentCapacity> {
  return await invoke<AgentCapacity>("update_agent_capacity", {
    projectId,
    capacity,
  });
}

export async function listAgentRuntimes(): Promise<AgentRuntimeInfo[]> {
  return await invoke<AgentRuntimeInfo[]>("list_agent_runtimes");
}

export async function refreshAgentCapacities(): Promise<AgentCapacity[]> {
  return await invoke<AgentCapacity[]>("refresh_agent_capacities");
}

export async function dispatchProjectTask(
  projectId: string,
  taskId: string,
  agentId: string
): Promise<AgentRun> {
  return await invoke<AgentRun>("dispatch_project_task", {
    projectId,
    taskId,
    agentId,
  });
}

export async function refreshProjectRuns(
  projectId: string
): Promise<ProjectRoomSnapshot> {
  return await invoke<ProjectRoomSnapshot>("refresh_project_runs", { projectId });
}

export async function generateProjectRoomPrompt(
  projectId: string,
  locale?: string | null
): Promise<string> {
  return await invoke<string>("generate_project_room_prompt", {
    projectId,
    locale: locale || null,
  });
}

export async function initializeProjectGit(projectId: string): Promise<string> {
  return await invoke<string>("initialize_project_git", { projectId });
}

export async function scanProjectHygiene(
  projectId: string
): Promise<HygieneCandidate[]> {
  return await invoke<HygieneCandidate[]>("scan_project_hygiene", {
    projectId,
  });
}

// History API
export async function listHistory(): Promise<McpCallRecord[]> {
  return await invoke<McpCallRecord[]>("list_history");
}

export async function clearHistory(): Promise<boolean> {
  return await invoke<boolean>("clear_history");
}

export async function exportHistoryJson(): Promise<string> {
  return await invoke<string>("export_history_json");
}

// Settings API
export async function getSettings(): Promise<TunnelSettings> {
  return await invoke<TunnelSettings>("get_settings");
}

export async function updateSettings(
  newSettings: TunnelSettings
): Promise<TunnelSettings> {
  return await invoke<TunnelSettings>("update_settings", { newSettings });
}

export async function setLocale(locale: string): Promise<string> {
  return await invoke<string>("set_locale", { locale });
}

export async function refreshProcessEnvironment(): Promise<boolean> {
  return await invoke<boolean>("refresh_process_environment");
}

export async function openPathInExplorer(path: string): Promise<boolean> {
  return await invoke<boolean>("open_path_in_explorer", { path });
}

export async function getAppVersion(): Promise<string> {
  return await invoke<string>("get_app_version");
}

export async function resolveCloseRequest(
  action: "exit" | "hide" | "cancel"
): Promise<void> {
  await invoke("resolve_close_request", { action });
}
