use crate::models::{
    AgentCapacity, AgentQuotaWindow, AgentRun, AgentRuntimeInfo, HygieneCandidate,
    MemoryFileHealth, MemoryHealth, ProjectAgentPolicy, ProjectDiscussionMessage,
    ProjectExperiment, ProjectMemory, ProjectRemote, ProjectRoomConfig, ProjectRoomSnapshot,
    ProjectRoomSummary, ProjectTask,
};
use crate::state::AppState;
use crate::utils::chappie_broker;
use crate::utils::cmd::{execute_cmd, find_executable, is_process_running, kill_process_tree};
use crate::utils::time::local_now_rfc3339;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

const REGISTRY_FILE: &str = "registry.json";
const CONFIG_FILE: &str = "project.json";
const PROJECT_MEMORY_DIR: &str = ".project_memory";
const MEMORY_INDEX_FILE: &str = "MEMORY_INDEX.md";
const PROJECT_STATE_FILE: &str = "PROJECT_STATE.md";
const SESSION_HANDOFF_FILE: &str = "SESSION_HANDOFF.md";
const DECISIONS_FILE: &str = "DECISIONS.md";
const MODEL_DESIGN_FILE: &str = "MODEL_DESIGN.md";
const DATA_CATALOG_FILE: &str = "DATA_CATALOG.md";
const MEMORY_EXPERIMENTS_FILE: &str = "EXPERIMENTS.md";
const RESULTS_FILE: &str = "RESULTS.md";
const REFERENCES_FILE: &str = "REFERENCES.md";
const DOCUMENTS_FILE: &str = "DOCUMENTS.md";
const MEMORY_PROTOCOL_FILE: &str = "MEMORY_PROTOCOL.md";
const MEMORY_STATUS_FILE: &str = "MEMORY_STATUS.json";
const MEMORY_ARCHIVE_DIR: &str = "archive";
const MEMORY_LEDGER_DIR: &str = "ledger";
const LEGACY_CONTROL_MEMORY_FILE: &str = "memory.json";
const AGENTS_FILE: &str = "agents.json";
const CAPACITIES_FILE: &str = "capacities.json";
const TASKS_FILE: &str = "tasks.json";
const EXPERIMENTS_FILE: &str = "experiments.json";
const DISCUSSION_FILE: &str = "discussion.json";
const RUNS_FILE: &str = "runs.json";
const TRANSPORT_HEALTH_FILE: &str = "transport_health.json";
const BRIDGE_DIR: &str = ".tunneldock";
const BRIDGE_FILE: &str = "project_room.json";
const WEB_CONTEXT_FILE: &str = "web_context.json";
const WEB_STATUS_FILE: &str = "web_status.json";
const SESSION_BINDING_FILE: &str = "session_binding.json";
const CONSTITUTION_FILE: &str = "CONSTITUTION.md";
const INBOX_DIR: &str = "inbox";
const INBOX_PROTOCOL_FILE: &str = "INBOX_PROTOCOL.md";
const INBOX_ERROR_FILE: &str = "inbox_error.json";
const RUNS_DIR: &str = "runs";

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

mod agent_worker;
mod antigravity;
mod codex;
mod coordination;
mod memory;
mod telemetry;
mod transport;

use agent_worker::{
    append_system_message, ensure_dispatch_scope_is_safe, next_id, refresh_run_states_unlocked,
    run_dir, task_prompt,
};
use coordination::{reconcile_project_operational_state_once, sync_project_bridge};
use memory::{
    append_memory_ledger, ensure_project_memory, load_project_memory, project_memory_dir,
    refresh_memory_status, save_project_memory,
};
use telemetry::{agent_runtimes, current_agent_capacities, find_codex_executable};
use transport::{
    load_transport_health_unlocked, record_transport_failure, record_transport_running,
    record_transport_success,
};

#[cfg(test)]
use agent_worker::scopes_conflict;
#[cfg(test)]
use telemetry::{
    parse_antigravity_quota_summary, parse_codex_app_server_rate_limits, parse_codex_rate_limit,
};

fn projects_root(state: &AppState) -> PathBuf {
    state.app_data_dir.join("projects")
}

fn project_session_binding(
    config: &ProjectRoomConfig,
    current_session_id: Option<&str>,
) -> serde_json::Value {
    let workspace_id = config.workspace_id.as_deref().unwrap_or("unbound");
    serde_json::json!({
        "version": 1,
        "logical_id": format!(
            "project:{}|workspace:{}|cwd:{}",
            config.id,
            workspace_id,
            config.local_root.replace('\\', "/").to_ascii_lowercase()
        ),
        "project_id": config.id,
        "workspace_id": config.workspace_id,
        "cwd": config.local_root,
        "current_session_id": current_session_id,
        "session_id_is_ephemeral": true,
        "resolver": "PI.sessions -> exact normalized cwd match -> if returned binding differs, PI.init(current sessionId)",
        "resolve_before_each_web_turn": true,
        "retry_on_session_error": true
    })
}

fn project_dir(state: &AppState, project_id: &str) -> Result<PathBuf, String> {
    if project_id.is_empty()
        || !project_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(format!("非法 project_id: {}", project_id));
    }

    Ok(projects_root(state).join(project_id))
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("读取 {} 失败: {}", path.display(), error))?;
    serde_json::from_str(&content)
        .map_err(|error| format!("解析 {} 失败: {}", path.display(), error))
}

fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建 {} 失败: {}", parent.display(), error))?;
    }

    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("序列化 {} 失败: {}", path.display(), error))?;
    fs::write(path, json).map_err(|error| format!("写入 {} 失败: {}", path.display(), error))
}

fn write_json_if_changed<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<bool, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建 {} 失败: {}", parent.display(), error))?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("序列化 {} 失败: {}", path.display(), error))?;
    if fs::read_to_string(path).ok().as_deref() == Some(json.as_str()) {
        return Ok(false);
    }
    fs::write(path, json).map_err(|error| format!("写入 {} 失败: {}", path.display(), error))?;
    Ok(true)
}

fn default_agents() -> Vec<ProjectAgentPolicy> {
    vec![
        ProjectAgentPolicy {
            agent_id: "chatgpt".to_string(),
            display_name: "ChatGPT".to_string(),
            role: "Coordinator / Research Lead / Final Reviewer".to_string(),
            strengths: vec![
                "科研规划与方法推理".to_string(),
                "任务拆解与跨 Agent 协调".to_string(),
                "实验解释与最终 review".to_string(),
            ],
            risk_notes: vec!["避免与 worker 无约束抢写同一文件".to_string()],
            review_rule: "负责方法取舍与最终合并判断".to_string(),
            enabled: true,
        },
        ProjectAgentPolicy {
            agent_id: "codex".to_string(),
            display_name: "Codex".to_string(),
            role: "Primary Engineer".to_string(),
            strengths: vec![
                "稳健实现核心代码".to_string(),
                "重构、测试与工程正确性".to_string(),
                "代码 review 与 bug 修复".to_string(),
            ],
            risk_notes: vec![
                "执行速度较慢".to_string(),
                "高质量模型额度应优先留给高风险实现".to_string(),
            ],
            review_rule: "重要算法实现由 ChatGPT 做科研意图 review".to_string(),
            enabled: true,
        },
        ProjectAgentPolicy {
            agent_id: "gemini".to_string(),
            display_name: "Antigravity Agent / Gemini".to_string(),
            role: "Visual + Exploration Agent".to_string(),
            strengths: vec![
                "PCA、图表、视频与论文 case 可视化".to_string(),
                "UI / 视觉呈现".to_string(),
                "快速探索与结果检查".to_string(),
            ],
            risk_notes: vec![
                "完成判断可能过于乐观".to_string(),
                "核心代码设计需要额外验证".to_string(),
            ],
            review_rule: "修改核心代码时默认必须由 Codex review".to_string(),
            enabled: true,
        },
    ]
}

fn default_capacities() -> Vec<AgentCapacity> {
    ["chatgpt", "codex", "gemini"]
        .into_iter()
        .map(|agent_id| AgentCapacity {
            agent_id: agent_id.to_string(),
            available: false,
            remaining_percent: None,
            reset_at: None,
            model: None,
            source: "unavailable".to_string(),
            confidence: "unavailable".to_string(),
            updated_at: local_now_rfc3339(),
            quota_windows: Vec::new(),
        })
        .collect()
}

fn default_remote(project_id: &str) -> ProjectRemote {
    match project_id {
        "point_tracking" => ProjectRemote {
            host: "guodongyan@10.12.54.65".to_string(),
            root: "/data3/guodongyan/lpl/GeoSAMTracker".to_string(),
            environment: "/data3/guodongyan/lpl/envs/cowtracker/bin/python".to_string(),
            notes: "Active GeoSAMTracker source/training workspace; local D:\\point_tracking stores references and durable project memory. Legacy cowtracker_tap is reference-only.".to_string(),
        },
        "iqa_agent" => ProjectRemote {
            host: "lpl@10.12.54.150".to_string(),
            root: "/data1/lpl/code/agent-iqa".to_string(),
            environment: "/data0/lpl/miniconda3/envs/verl_h200/bin/python".to_string(),
            notes: "Formal IQA Agent server workspace; runtime and dataset paths are documented by the local project README.".to_string(),
        },
        "3d_mllm" => ProjectRemote {
            host: "lpl@10.12.54.150".to_string(),
            root: "/data1/lpl/code/GeoSR/GeoSR_static".to_string(),
            environment: String::new(),
            notes: "GeoSR static student/source workspace on the H200 server; /data1/lpl is the broader remote workspace root.".to_string(),
        },
        _ => ProjectRemote::default(),
    }
}

fn seed_configs(state: &AppState) -> Vec<ProjectRoomConfig> {
    let workspaces = state.workspaces.lock();
    let now = local_now_rfc3339();

    let seeds = [
        ("point_tracking", "Point Tracking", r"D:\point_tracking"),
        ("iqa_agent", "IQA Agent", r"D:\iqa_agent"),
        ("3d_mllm", "3D + MLLM", r"D:\gpt_proj"),
    ];

    seeds
        .into_iter()
        .map(|(id, name, local_root)| {
            let workspace_id = workspaces
                .iter()
                .find(|workspace| workspace.path.eq_ignore_ascii_case(local_root))
                .map(|workspace| workspace.id.clone());

            ProjectRoomConfig {
                id: id.to_string(),
                name: name.to_string(),
                local_root: local_root.to_string(),
                repo_root: local_root.to_string(),
                workspace_id,
                codex_thread_id: None,
                codex_automation_thread_id: None,
                antigravity_cascade_id: None,
                remote: default_remote(id),
                enabled: true,
                keep_session_alive: true,
                created_at: now.clone(),
                updated_at: now.clone(),
            }
        })
        .collect()
}

fn migrate_iqa_workspace(state: &AppState) {
    let desired = r"D:\iqa_agent";
    let legacy = r"D:\agent_iqa";

    let mut changed = false;
    {
        let mut workspaces = state.workspaces.lock();
        let desired_exists = workspaces
            .iter()
            .any(|workspace| workspace.path.eq_ignore_ascii_case(desired));

        if !desired_exists {
            if let Some(workspace) = workspaces.iter_mut().find(|workspace| {
                workspace.path.eq_ignore_ascii_case(legacy) || workspace.name == "agent_iqa"
            }) {
                workspace.name = "iqa_agent".to_string();
                workspace.path = desired.to_string();
                workspace.status = "stopped".to_string();
                workspace.session_id = None;
                workspace.pid = None;
                workspace.binding_count = 0;
                workspace.error_message = None;
                changed = true;
            }
        }
    }

    if changed {
        state.save_workspaces();
    }
}

fn ensure_store(state: &AppState) -> Result<Vec<String>, String> {
    migrate_iqa_workspace(state);

    let root = projects_root(state);
    fs::create_dir_all(&root).map_err(|error| format!("创建 Project Room 目录失败: {}", error))?;
    let registry_path = root.join(REGISTRY_FILE);

    let project_ids = if registry_path.exists() {
        read_json::<Vec<String>>(&registry_path)?
    } else {
        let configs = seed_configs(state);
        let ids = configs
            .iter()
            .map(|config| config.id.clone())
            .collect::<Vec<_>>();

        for config in &configs {
            initialize_project_files(state, config)?;
        }
        write_json(&registry_path, &ids)?;
        ids
    };

    // Fill files added by newer TunnelDock builds without overwriting existing
    // project-specific research state.
    for project_id in &project_ids {
        let dir = project_dir(state, project_id)?;
        fs::create_dir_all(&dir)
            .map_err(|error| format!("创建 {} 失败: {}", dir.display(), error))?;

        if !dir.join(AGENTS_FILE).exists() {
            write_json(&dir.join(AGENTS_FILE), &default_agents())?;
        }
        let capacities_path = dir.join(CAPACITIES_FILE);
        if !capacities_path.exists() {
            write_json(&capacities_path, &default_capacities())?;
        } else {
            let mut capacities = read_json::<Vec<AgentCapacity>>(&capacities_path)?;
            let mut changed = false;
            for capacity in &mut capacities {
                if capacity.source == "unavailable"
                    && capacity.remaining_percent.is_none()
                    && capacity.available
                {
                    capacity.available = false;
                    changed = true;
                }
            }
            if changed {
                write_json(&capacities_path, &capacities)?;
            }
        }
        let legacy_memory = dir.join(LEGACY_CONTROL_MEMORY_FILE);
        if legacy_memory.exists() {
            let _ = fs::remove_file(&legacy_memory);
        }
        if !dir.join(TASKS_FILE).exists() {
            write_json(&dir.join(TASKS_FILE), &Vec::<ProjectTask>::new())?;
        }
        if !dir.join(EXPERIMENTS_FILE).exists() {
            write_json(
                &dir.join(EXPERIMENTS_FILE),
                &Vec::<ProjectExperiment>::new(),
            )?;
        }
        if !dir.join(DISCUSSION_FILE).exists() {
            write_json(
                &dir.join(DISCUSSION_FILE),
                &Vec::<ProjectDiscussionMessage>::new(),
            )?;
        }
        if !dir.join(RUNS_FILE).exists() {
            write_json(&dir.join(RUNS_FILE), &Vec::<AgentRun>::new())?;
        }

        // Attach migrated workspace ids to seeded projects when possible.
        let config_path = dir.join(CONFIG_FILE);
        if config_path.exists() {
            let mut config = read_json::<ProjectRoomConfig>(&config_path)?;
            let mut config_changed = false;
            if config.workspace_id.is_none() {
                config.workspace_id = state
                    .workspaces
                    .lock()
                    .iter()
                    .find(|workspace| workspace.path.eq_ignore_ascii_case(&config.local_root))
                    .map(|workspace| workspace.id.clone());
                if config.workspace_id.is_some() {
                    config_changed = true;
                }
            }
            if config.remote.root.trim().is_empty() {
                let seeded_remote = default_remote(project_id);
                if !seeded_remote.root.is_empty() {
                    config.remote = seeded_remote;
                    config_changed = true;
                }
            }
            if config_changed {
                config.updated_at = local_now_rfc3339();
                write_json(&config_path, &config)?;
            }
            ensure_project_memory(&config)?;
        }
    }

    Ok(project_ids)
}

fn initialize_project_files(state: &AppState, config: &ProjectRoomConfig) -> Result<(), String> {
    let dir = project_dir(state, &config.id)?;
    fs::create_dir_all(&dir).map_err(|error| format!("创建 {} 失败: {}", dir.display(), error))?;

    write_json(&dir.join(CONFIG_FILE), config)?;
    ensure_project_memory(config)?;
    write_json(&dir.join(AGENTS_FILE), &default_agents())?;
    write_json(&dir.join(CAPACITIES_FILE), &default_capacities())?;
    write_json(&dir.join(TASKS_FILE), &Vec::<ProjectTask>::new())?;
    write_json(
        &dir.join(EXPERIMENTS_FILE),
        &Vec::<ProjectExperiment>::new(),
    )?;
    write_json(
        &dir.join(DISCUSSION_FILE),
        &Vec::<ProjectDiscussionMessage>::new(),
    )?;
    write_json(&dir.join(RUNS_FILE), &Vec::<AgentRun>::new())?;
    Ok(())
}

fn load_snapshot_unlocked(
    state: &AppState,
    project_id: &str,
) -> Result<ProjectRoomSnapshot, String> {
    let dir = project_dir(state, project_id)?;

    let config = read_json::<ProjectRoomConfig>(&dir.join(CONFIG_FILE))?;
    let runs = read_json::<Vec<AgentRun>>(&dir.join(RUNS_FILE))?;
    let transports = load_transport_health_unlocked(state, project_id, &config, &runs)?;
    Ok(ProjectRoomSnapshot {
        memory: load_project_memory(&config)?,
        memory_health: refresh_memory_status(&config)?,
        config,
        agents: read_json(&dir.join(AGENTS_FILE))?,
        capacities: read_json(&dir.join(CAPACITIES_FILE))?,
        transports,
        tasks: read_json(&dir.join(TASKS_FILE))?,
        experiments: read_json(&dir.join(EXPERIMENTS_FILE))?,
        discussion: read_json(&dir.join(DISCUSSION_FILE))?,
        runs,
    })
}

fn git_initialized(repo_root: &str) -> bool {
    if repo_root.trim().is_empty() {
        return false;
    }
    Path::new(repo_root).join(".git").exists()
}

fn nested_git_roots(root: &Path, current: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 3 || out.len() >= 20 {
        return;
    }

    let Ok(entries) = fs::read_dir(current) else {
        return;
    };

    for entry in entries.flatten() {
        if out.len() >= 20 {
            break;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(
            name.as_str(),
            ".git" | ".tunneldock" | ".project_memory" | "node_modules" | "target" | ".venv"
        ) {
            continue;
        }

        if path.join(".git").exists() {
            if path != root {
                out.push(path);
            }
            continue;
        }

        nested_git_roots(root, &path, depth + 1, out);
    }
}

fn discover_nested_git_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    nested_git_roots(root, root, 0, &mut roots);
    roots
}

#[tauri::command]
pub fn list_agent_runtimes() -> Vec<AgentRuntimeInfo> {
    agent_runtimes()
}

fn persist_agent_capacities_unlocked(
    state: &AppState,
    capacities: &[AgentCapacity],
) -> Result<(), String> {
    let project_ids = ensure_store(state)?;
    for project_id in project_ids {
        let dir = project_dir(state, &project_id)?;
        write_json(&dir.join(CAPACITIES_FILE), capacities)?;
        let snapshot = load_snapshot_unlocked(state, &project_id)?;
        sync_project_bridge(&snapshot)?;
    }
    Ok(())
}

#[tauri::command]
pub fn refresh_agent_capacities(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<AgentCapacity>, String> {
    let capacities = current_agent_capacities();
    let _guard = state.project_store_lock.lock();
    persist_agent_capacities_unlocked(&state, &capacities)?;
    Ok(capacities)
}

pub(super) fn dispatch_project_task_unlocked(
    state: &AppState,
    project_id: &str,
    task_id: &str,
    agent_id: &str,
) -> Result<AgentRun, String> {
    refresh_run_states_unlocked(state, project_id)?;

    let mut snapshot = load_snapshot_unlocked(state, project_id)?;
    let task = snapshot
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .ok_or_else(|| format!("找不到 Task: {}", task_id))?;

    if !matches!(agent_id, "codex" | "gemini") {
        return Err("当前只能直接 dispatch 给 Codex 或 Antigravity/Gemini；ChatGPT 由网页 Project Room 协调。".to_string());
    }

    ensure_dispatch_scope_is_safe(&snapshot, &task)?;

    let run_id = next_id(&format!("RUN-{}", agent_id.to_ascii_uppercase()));
    let run_root = run_dir(&snapshot.config, &run_id);
    fs::create_dir_all(&run_root)
        .map_err(|error| format!("创建 {} 失败: {}", run_root.display(), error))?;

    let prompt_path = run_root.join("prompt.md");
    let output_path = run_root.join("HANDOFF.md");
    let log_path = run_root.join("stdout.log");
    let error_path = run_root.join("stderr.log");
    let prompt = task_prompt(&snapshot, &task, agent_id, &output_path);
    fs::write(&prompt_path, &prompt)
        .map_err(|error| format!("写入 {} 失败: {}", prompt_path.display(), error))?;
    File::create(&log_path)
        .map_err(|error| format!("创建 {} 失败: {}", log_path.display(), error))?;
    File::create(&error_path)
        .map_err(|error| format!("创建 {} 失败: {}", error_path.display(), error))?;

    let (pid, external_session_id, start_step) = if agent_id == "codex" {
        let executable = find_codex_executable().ok_or_else(|| "未找到 Codex CLI".to_string())?;
        let source_thread_id = snapshot
            .config
            .codex_thread_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "Project Room {} 尚未绑定 Codex Thread ID；请在 Project Config 绑定你一直使用的 Codex Desktop 对话。",
                    snapshot.config.name
                )
            })?;
        let automation_name = format!("[TunnelDock] {}", snapshot.config.name);
        let dispatch = match codex::start_background_turn(
            &executable,
            source_thread_id,
            snapshot.config.codex_automation_thread_id.as_deref(),
            &automation_name,
            &prompt,
            task.kind == "consultation",
            &log_path,
            &error_path,
        ) {
            Ok(dispatch) => dispatch,
            Err(error) => {
                let _ =
                    record_transport_failure(state, project_id, "codex", None, &error, "dispatch");
                return Err(error);
            }
        };
        if snapshot.config.codex_automation_thread_id.as_deref() != Some(&dispatch.thread_id) {
            snapshot.config.codex_automation_thread_id = Some(dispatch.thread_id.clone());
            snapshot.config.updated_at = local_now_rfc3339();
            let dir = project_dir(state, project_id)?;
            write_json(&dir.join(CONFIG_FILE), &snapshot.config)?;
        }
        state
            .running_agent_pids
            .lock()
            .insert(run_id.clone(), dispatch.pid);
        let _ = record_transport_running(state, project_id, "codex", &run_id);
        (
            Some(dispatch.pid),
            Some(dispatch.thread_id),
            Some(dispatch.start_offset),
        )
    } else {
        let binding = antigravity::resolve_cascade(
            &snapshot.config.local_root,
            snapshot.config.antigravity_cascade_id.as_deref(),
        )?;
        if snapshot.config.antigravity_cascade_id.as_deref() != Some(&binding.cascade_id) {
            snapshot.config.antigravity_cascade_id = Some(binding.cascade_id.clone());
            snapshot.config.updated_at = local_now_rfc3339();
            let dir = project_dir(state, project_id)?;
            write_json(&dir.join(CONFIG_FILE), &snapshot.config)?;
        }
        antigravity::send_task(&binding, &prompt)?;
        (None, Some(binding.cascade_id), Some(binding.step_count))
    };

    let run = AgentRun {
        id: run_id.clone(),
        task_id: task.id.clone(),
        agent_id: agent_id.to_string(),
        status: "running".to_string(),
        pid,
        external_session_id,
        start_step,
        started_at: local_now_rfc3339(),
        finished_at: None,
        prompt_path: prompt_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        log_path: log_path.to_string_lossy().to_string(),
        error_path: error_path.to_string_lossy().to_string(),
        error_message: None,
    };

    let dir = project_dir(state, project_id)?;
    let runs_path = dir.join(RUNS_FILE);
    let mut runs = read_json::<Vec<AgentRun>>(&runs_path)?;
    runs.insert(0, run.clone());
    write_json(&runs_path, &runs)?;

    let tasks_path = dir.join(TASKS_FILE);
    let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
    if let Some(current) = tasks.iter_mut().find(|item| item.id == task.id) {
        current.owner = agent_id.to_string();
        current.status = "active".to_string();
        current.updated_at = local_now_rfc3339();
    }
    write_json(&tasks_path, &tasks)?;

    let _ = append_system_message(
        &dir,
        "system",
        format!("Dispatched {} to {} as {}.", task.id, agent_id, run.id),
    );

    let snapshot = load_snapshot_unlocked(state, project_id)?;
    sync_project_bridge(&snapshot)?;
    Ok(run)
}

#[tauri::command]
pub fn dispatch_project_task(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    task_id: String,
    agent_id: String,
) -> Result<AgentRun, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    dispatch_project_task_unlocked(&state, &project_id, &task_id, &agent_id)
}

#[tauri::command]
pub fn refresh_project_runs(
    state: State<'_, Arc<AppState>>,
    project_id: String,
) -> Result<ProjectRoomSnapshot, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    refresh_run_states_unlocked(&state, &project_id)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    sync_project_bridge(&snapshot)?;
    Ok(snapshot)
}

async fn reconcile_project_sessions_once(
    app: AppHandle,
    state: Arc<AppState>,
) -> Result<(), String> {
    if state.cleanup_in_progress() {
        return Ok(());
    }

    let broker_sessions = match chappie_broker::list_sessions().await {
        Ok(sessions) => sessions,
        Err(_) => {
            // Chappie/otunnel may still be booting. Do not churn Pi processes
            // until the Broker itself is reachable.
            return Ok(());
        }
    };

    let configs = {
        let _guard = state.project_store_lock.lock();
        let project_ids = ensure_store(&state)?;
        let mut configs = Vec::new();
        for project_id in project_ids {
            let dir = project_dir(&state, &project_id)?;
            let config = read_json::<ProjectRoomConfig>(&dir.join(CONFIG_FILE))?;
            if config.enabled && config.keep_session_alive {
                configs.push(config);
            }
        }
        configs
    };

    let mut adopted_any = false;
    for config in configs {
        if state.cleanup_in_progress() {
            break;
        }

        let Some(workspace_id) = config.workspace_id.clone() else {
            continue;
        };

        let expected_cwd = config.local_root.replace('\\', "/").to_ascii_lowercase();
        if let Some(session) = broker_sessions
            .iter()
            .find(|session| session.cwd.replace('\\', "/").to_ascii_lowercase() == expected_cwd)
        {
            let binding_path = PathBuf::from(&config.local_root)
                .join(BRIDGE_DIR)
                .join(SESSION_BINDING_FILE);
            let _ = write_json_if_changed(
                &binding_path,
                &project_session_binding(&config, Some(&session.id)),
            );
            let mut should_migrate_legacy_host = false;
            {
                let mut workspaces = state.workspaces.lock();
                if let Some(workspace) = workspaces
                    .iter_mut()
                    .find(|workspace| workspace.id == workspace_id)
                {
                    let next_status =
                        if matches!(session.status.as_str(), "executing" | "generating") {
                            "executing"
                        } else {
                            "ready"
                        };
                    let durable_pid = workspace
                        .pid
                        .filter(|pid| is_process_running(*pid))
                        .or_else(|| state.durable_workspace_pid(&workspace_id));
                    if let Some(pid) = durable_pid {
                        state
                            .running_workspace_pids
                            .lock()
                            .insert(workspace_id.clone(), pid);
                    }
                    let changed = workspace.status != next_status
                        || workspace.session_id.as_deref() != Some(session.id.as_str())
                        || workspace.binding_count != session.binding_count
                        || workspace.error_message.is_some()
                        || durable_pid.is_some_and(|pid| workspace.pid != Some(pid));

                    workspace.status = next_status.to_string();
                    workspace.session_id = Some(session.id.clone());
                    crate::commands::workspace::persist_durable_session_hint(
                        &state,
                        &workspace_id,
                        &session.id,
                    );
                    workspace.binding_count = session.binding_count;
                    workspace.error_message = None;
                    if let Some(pid) = durable_pid {
                        workspace.pid = Some(pid);
                    }
                    should_migrate_legacy_host = next_status == "ready"
                        && crate::commands::workspace::durable_project_pi_needs_node_migration(
                            &state,
                            &workspace_id,
                        );
                    if changed {
                        adopted_any = true;
                        let _ = app.emit(
                            "workspace-updated",
                            serde_json::json!({ "workspace_id": &workspace_id }),
                        );
                    }
                }
            }

            if should_migrate_legacy_host {
                let _ = crate::commands::workspace::restart_workspace_session_inner(
                    app.clone(),
                    state.clone(),
                    workspace_id.clone(),
                )
                .await;
            }
            continue;
        }

        let binding_path = PathBuf::from(&config.local_root)
            .join(BRIDGE_DIR)
            .join(SESSION_BINDING_FILE);
        let _ = write_json_if_changed(&binding_path, &project_session_binding(&config, None));

        let live_pid = state
            .running_workspace_pids
            .lock()
            .get(&workspace_id)
            .copied()
            .map(is_process_running)
            .unwrap_or(false);
        if live_pid {
            continue;
        }

        let _ = crate::commands::workspace::start_workspace_session_inner(
            app.clone(),
            state.clone(),
            workspace_id,
        )
        .await;
    }

    if adopted_any {
        state.save_workspaces();
    }

    Ok(())
}

pub async fn project_session_supervisor_loop(app: AppHandle, state: Arc<AppState>) {
    // Give the desktop app time to recover a pre-existing otunnel health endpoint
    // and let Chappie finish booting before the first reconciliation.
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    let mut cycle = 0u32;

    loop {
        if state.cleanup_in_progress() {
            break;
        }

        // Runs and inbox events are project-local coordination state. Process
        // them even when the TunnelDock window is hidden in the system tray.
        let _ = reconcile_project_operational_state_once(&state);
        let _ = reconcile_project_sessions_once(app.clone(), state.clone()).await;

        // Quota probing can touch local agent runtimes and must not block Tauri's
        // async/UI work. Refresh it roughly once per minute in a blocking worker.
        if cycle.is_multiple_of(15) {
            let quota_state = state.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let capacities = current_agent_capacities();
                let _guard = quota_state.project_store_lock.lock();
                let _ = persist_agent_capacities_unlocked(&quota_state, &capacities);
            })
            .await;
        }
        cycle = cycle.wrapping_add(1);

        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    }
}

#[tauri::command]
pub fn list_project_rooms(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ProjectRoomSummary>, String> {
    let _guard = state.project_store_lock.lock();
    let ids = ensure_store(&state)?;
    let mut summaries = Vec::with_capacity(ids.len());

    for project_id in ids {
        refresh_run_states_unlocked(&state, &project_id)?;
        let snapshot = load_snapshot_unlocked(&state, &project_id)?;
        sync_project_bridge(&snapshot)?;
        let workspace_runtime = snapshot
            .config
            .workspace_id
            .as_deref()
            .and_then(|workspace_id| {
                state
                    .workspaces
                    .lock()
                    .iter()
                    .find(|workspace| workspace.id == workspace_id)
                    .cloned()
            });
        let codex_transport = snapshot
            .transports
            .iter()
            .find(|health| health.agent_id == "codex");
        summaries.push(ProjectRoomSummary {
            id: snapshot.config.id.clone(),
            name: snapshot.config.name.clone(),
            local_root: snapshot.config.local_root.clone(),
            repo_root: snapshot.config.repo_root.clone(),
            workspace_id: snapshot.config.workspace_id.clone(),
            codex_thread_id: snapshot.config.codex_thread_id.clone(),
            codex_automation_thread_id: snapshot.config.codex_automation_thread_id.clone(),
            codex_status: codex_transport
                .map(|health| health.status.clone())
                .unwrap_or_else(|| "untested".to_string()),
            codex_active_run_id: codex_transport.and_then(|health| health.active_run_id.clone()),
            codex_last_success_at: codex_transport
                .and_then(|health| health.last_success_at.clone()),
            codex_last_failure_at: codex_transport
                .and_then(|health| health.last_failure_at.clone()),
            codex_last_error: codex_transport.and_then(|health| health.last_error.clone()),
            session_status: workspace_runtime
                .as_ref()
                .map(|workspace| workspace.status.clone())
                .unwrap_or_else(|| "stopped".to_string()),
            session_id: workspace_runtime
                .as_ref()
                .and_then(|workspace| workspace.session_id.clone()),
            binding_count: workspace_runtime
                .as_ref()
                .map(|workspace| workspace.binding_count)
                .unwrap_or(0),
            remote_configured: !snapshot.config.remote.root.trim().is_empty(),
            git_initialized: git_initialized(&snapshot.config.repo_root),
            active_tasks: snapshot
                .tasks
                .iter()
                .filter(|task| matches!(task.status.as_str(), "active" | "review" | "blocked"))
                .count(),
            experiments: snapshot.experiments.len(),
            discussion_messages: snapshot.discussion.len(),
            memory_updated_at: snapshot.memory.updated_at,
        });
    }

    Ok(summaries)
}

#[tauri::command]
pub fn get_project_room(
    state: State<'_, Arc<AppState>>,
    project_id: String,
) -> Result<ProjectRoomSnapshot, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    refresh_run_states_unlocked(&state, &project_id)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    sync_project_bridge(&snapshot)?;
    Ok(snapshot)
}

#[tauri::command]
pub fn update_project_config(
    state: State<'_, Arc<AppState>>,
    mut config: ProjectRoomConfig,
) -> Result<ProjectRoomSnapshot, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    let dir = project_dir(&state, &config.id)?;
    let existing = read_json::<ProjectRoomConfig>(&dir.join(CONFIG_FILE))?;

    if config.name.trim().is_empty() || config.local_root.trim().is_empty() {
        return Err("项目名称和本地工作区不能为空".to_string());
    }

    if !existing
        .local_root
        .eq_ignore_ascii_case(config.local_root.trim())
    {
        return Err(
            "local_root 是 Project Room 的持久化身份边界，当前不允许直接修改；请创建显式迁移流程，避免丢失或分叉 .project_memory。"
                .to_string(),
        );
    }

    if !Path::new(&config.local_root).exists() {
        return Err(format!("local_root 不存在: {}", config.local_root));
    }

    if config.repo_root.trim().is_empty() {
        config.repo_root = config.local_root.clone();
    }

    let source_thread_changed = existing.codex_thread_id.as_deref().map(str::trim)
        != config.codex_thread_id.as_deref().map(str::trim);
    config.codex_automation_thread_id = if source_thread_changed {
        None
    } else {
        existing.codex_automation_thread_id.clone()
    };

    config.updated_at = local_now_rfc3339();
    write_json(&dir.join(CONFIG_FILE), &config)?;
    let snapshot = load_snapshot_unlocked(&state, &config.id)?;
    sync_project_bridge(&snapshot)?;
    Ok(snapshot)
}

#[tauri::command]
pub fn update_project_memory(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    mut memory: ProjectMemory,
) -> Result<ProjectMemory, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    memory.updated_at = local_now_rfc3339();
    save_project_memory(&snapshot.config, &memory)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    sync_project_bridge(&snapshot)?;
    Ok(memory)
}

#[tauri::command]
pub fn upsert_project_task(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    mut task: ProjectTask,
) -> Result<ProjectTask, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    let dir = project_dir(&state, &project_id)?;
    let path = dir.join(TASKS_FILE);
    let mut tasks = read_json::<Vec<ProjectTask>>(&path)?;

    let now = local_now_rfc3339();
    let is_new = task.id.trim().is_empty();
    if is_new {
        task.id = next_id("TASK");
        task.created_at = now.clone();
        if task.kind.trim().is_empty() {
            task.kind = "work".to_string();
        }
        if task.finalization_policy.trim().is_empty() {
            task.finalization_policy = "work".to_string();
        }
        if matches!(task.owner.as_str(), "codex" | "gemini") {
            if task.auto_dispatch {
                task.status = "queued".to_string();
            }
        } else {
            task.auto_dispatch = false;
            task.status = "backlog".to_string();
        }
    }
    task.updated_at = now;

    if task.title.trim().is_empty() {
        return Err("Task 标题不能为空".to_string());
    }

    if let Some(existing) = tasks.iter_mut().find(|existing| existing.id == task.id) {
        *existing = task.clone();
    } else {
        tasks.insert(0, task.clone());
    }
    write_json(&path, &tasks)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    sync_project_bridge(&snapshot)?;
    Ok(task)
}

#[tauri::command]
pub fn append_project_message(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    mut message: ProjectDiscussionMessage,
) -> Result<ProjectDiscussionMessage, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    let dir = project_dir(&state, &project_id)?;
    let path = dir.join(DISCUSSION_FILE);
    let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&path)?;

    if message.message.trim().is_empty() {
        return Err("Discussion 消息不能为空".to_string());
    }
    if message.id.trim().is_empty() {
        message.id = next_id("MSG");
    }
    if message.thread_id.trim().is_empty() {
        message.thread_id = "general".to_string();
    }
    if message.created_at.trim().is_empty() {
        message.created_at = local_now_rfc3339();
    }

    messages.insert(0, message.clone());
    if messages.len() > 5_000 {
        messages.truncate(5_000);
    }
    write_json(&path, &messages)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    sync_project_bridge(&snapshot)?;
    Ok(message)
}

#[tauri::command]
pub fn upsert_project_experiment(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    mut experiment: ProjectExperiment,
) -> Result<ProjectExperiment, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    let dir = project_dir(&state, &project_id)?;
    let path = dir.join(EXPERIMENTS_FILE);
    let mut experiments = read_json::<Vec<ProjectExperiment>>(&path)?;

    let now = local_now_rfc3339();
    if experiment.id.trim().is_empty() {
        experiment.id = next_id("EXP");
        experiment.created_at = now.clone();
    }
    experiment.updated_at = now;

    if experiment.hypothesis.trim().is_empty() {
        return Err("Experiment hypothesis 不能为空".to_string());
    }

    if let Some(existing) = experiments
        .iter_mut()
        .find(|existing| existing.id == experiment.id)
    {
        *existing = experiment.clone();
    } else {
        experiments.insert(0, experiment.clone());
    }
    write_json(&path, &experiments)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    sync_project_bridge(&snapshot)?;
    Ok(experiment)
}

#[tauri::command]
pub fn update_agent_capacity(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    mut capacity: AgentCapacity,
) -> Result<AgentCapacity, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    let dir = project_dir(&state, &project_id)?;
    let path = dir.join(CAPACITIES_FILE);
    let mut capacities = read_json::<Vec<AgentCapacity>>(&path)?;

    capacity.updated_at = local_now_rfc3339();
    if let Some(existing) = capacities
        .iter_mut()
        .find(|existing| existing.agent_id == capacity.agent_id)
    {
        *existing = capacity.clone();
    } else {
        capacities.push(capacity.clone());
    }

    write_json(&path, &capacities)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    sync_project_bridge(&snapshot)?;
    Ok(capacity)
}

#[tauri::command]
pub fn generate_project_room_prompt(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    locale: Option<String>,
) -> Result<String, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    sync_project_bridge(&snapshot)?;

    let is_en = locale
        .as_deref()
        .unwrap_or("zh-CN")
        .to_ascii_lowercase()
        .starts_with("en");
    let remote = if snapshot.config.remote.root.trim().is_empty() {
        if is_en {
            "Not configured".to_string()
        } else {
            "未配置".to_string()
        }
    } else if snapshot.config.remote.host.trim().is_empty() {
        snapshot.config.remote.root.clone()
    } else {
        format!(
            "{}:{}",
            snapshot.config.remote.host, snapshot.config.remote.root
        )
    };

    if is_en {
        Ok(format!(
            r#"You are the dedicated ChatGPT research coordinator for this Project Room.

Project: {name}
Project ID: {id}
Local workspace: {local}
Remote workspace: {remote}
TunnelDock workspace ID: {workspace}

Hard isolation rule:
- Work only on this project unless I explicitly request a cross-project handoff.
- Do not import memory, tasks, conclusions, or code state from another Project Room.

At the beginning of this web conversation:
1. Session binding is logical, not fixed to a sessionId. Before the first Pi call of EVERY user turn, call PI.sessions once and find the live session whose normalized cwd exactly equals '{local}'. Compare that ID with the `binding` returned by PI.sessions; call PI.init only when they differ. Treat any remembered sessionId only as a cache. If a Pi call fails with session missing/disconnected/unavailable, resolve by exact cwd again and retry that failed Pi call once. Never fall back to another cwd/project.
2. Read '{local}\.tunneldock\CONSTITUTION.md'.
3. Read '{local}\.tunneldock\web_status.json' first, then '{local}\.tunneldock\INBOX_PROTOCOL.md'. If `web_status.actions` is non-empty, process those actions in priority order before creating more work. Read `web_context.json` once when you need current task/discussion detail, and reread it only when `web_status.json.state_token` changes or the review gate opens. Read full `project_room.json` only for details omitted from the compact web context.
4. Read the default durable memory set:
   - '{local}\.project_memory\MEMORY_INDEX.md'
   - '{local}\.project_memory\PROJECT_STATE.md'
   - '{local}\.project_memory\SESSION_HANDOFF.md'
   - '{local}\.project_memory\MEMORY_PROTOCOL.md'
   Then read only the relevant domain memory for the task: MODEL_DESIGN.md, DATA_CATALOG.md, EXPERIMENTS.md, RESULTS.md, REFERENCES.md, DOCUMENTS.md, or DECISIONS.md.
5. Treat chat history as working memory only. Durable project files are the source of truth.
6. When Codex/Gemini/ChatGPT need to exchange a durable question, disagreement, task, handoff, or experiment result, write one JSON event to '{local}\.tunneldock\inbox\' using INBOX_PROTOCOL.md. Never edit project_room.json directly.
7. When you need independent opinions from Codex and Gemini, write one `consult.request` event. Use `finalization=review_only` for advisory analysis (default); use `finalization=durable` only when the consultation changes canonical project truth/results and must be written into memory. TunnelDock auto-dispatches the requested workers and assigns a `consultation_id`. Never keep one browser response stream open indefinitely while workers run: poll only `web_status.json`, at most 5 cycles / about 20 seconds. If `state_token` is unchanged and the barrier is still closed, end this web response with a compact checkpoint naming pending/invalid agents and no substantive conclusion. On the next user turn, reread `web_status.json` and continue from the persisted barrier. Once `ready_for_review=true`, read `web_context.json` and every successful `agents[].handoff_path` in full, account for failed/blocked agents as missing evidence, perform your own review, and write `consult.reviewed`. If this is `review_only`, it should now finalize and you stop there. If it is `durable`, update PROJECT_STATE.md, SESSION_HANDOFF.md, and affected domain memory, write `memory.commit`, then clean/integrate obsolete memory/docs/code/logs/scratch and write `cleanup.commit` until state=`finalized`. A focused follow-up may reuse the same `thread_id`, but it gets a new consultation barrier.
8. For any code/method decision, do not act as a passive summarizer. Personally inspect the relevant diff/source through Pi before discussing the decision with me. Review only the decisive code points: intent alignment, data/control flow, correctness/boundaries, and test/evidence coverage.
9. Before I decide, present a compact decision brief: (a) 2-4 code-review findings, (b) agent consensus/disagreement, (c) at most 2-3 realistic options with tradeoffs, and (d) your recommended direction plus the exact point that needs my decision. I remain the final decision maker.
10. After I explicitly decide, write one `decision.record` inbox event so the durable conclusion enters DECISIONS.md. Never record a recommendation as if it were my decision.

Web response budget:
- Keep normal web replies compact: at most 6 short bullets or roughly 350 English words unless I explicitly ask for detail.
- Do not paste raw tool output, full worker handoffs, long logs, or large code excerpts. Keep individual Pi tool results preferably <= 8 KiB: grep/find first, then read narrow ranges. Do not batch multiple potentially-large tool outputs into one web turn; save large diagnostics to files and inspect only decisive slices.
- During a multi-agent consultation, do not give a substantive conclusion until the matching consultation state is `finalized`. For `review_only`, finalization is worker completion + Web review; for `durable`, canonical memory commit + cleanup/integration are additionally required. Then report only code-review findings, consensus/disagreement, decisive evidence, decision options, and the next action. Do not quote both agents verbatim.
- `web_status.json` is the polling/checkpoint surface. `web_context.json` is the compact detail snapshot and should be read only after `state_token` changes or when current task/discussion detail is needed. Read `project_room.json`, full memory, or run artifacts only on demand.

Research operating rules:
- The goal is top-conference research. Keep code and algorithms simple, explicit, and correct.
- Do not create V1/V2/V3, *_old, *_backup, *_new, or *_final copies. Follow the project's own Git policy; if Git is disallowed, keep one current tree and record hashes/configs/evidence instead.
- After replacement/evidence is verified, consolidate the authoritative document and remove superseded code/scripts, disposable logs, caches, temp/debug artifacts, and scratch. Retain old material only as explicit archive or unique reproducibility/debug evidence.
- Record meaningful experiments, including negative results.
- Refine project memory after meaningful decisions/results instead of appending raw chat logs.
- ChatGPT is the coordinator and quota-allocation brain.
- Codex is the primary robust engineer.
- Antigravity Agent/Gemini is the visual/exploration agent; core-code changes require Codex review. It is integrated as an Agent Manager session, not as an IDE workspace.
- Important Codex algorithm changes require ChatGPT methodological review.
- You and the other agents may discuss and challenge each other; I remain the final decision maker.

Before allocating new work, clear the highest-priority `web_status.actions`: review completed worker tasks with `task.review`, retry invalid consultations, and resolve stale blocked/backlog items. Only then inspect current tasks, capacity, experiments, and discussion and allocate new work. A worker handoff is not completion: accept a work task only after verifying a real completed run, non-empty handoff, decisive source/diff, and focused verification evidence."#,
            name = snapshot.config.name,
            id = snapshot.config.id,
            local = snapshot.config.local_root,
            remote = remote,
            workspace = snapshot.config.workspace_id.as_deref().unwrap_or("unbound")
        ))
    } else {
        Ok(format!(
            r#"你是这个 Project Room 专属的 ChatGPT 网页科研协调者。

项目：{name}
Project ID：{id}
本地工作区：{local}
服务器工作区：{remote}
TunnelDock workspace ID：{workspace}

硬隔离规则：
- 除非我明确要求跨项目 handoff，否则只处理这个项目。
- 不得自动引用其他 Project Room 的记忆、任务、结论或代码状态。

这个网页对话开始时必须：
1. Session 绑定使用逻辑身份而不是固定 sessionId。每个 user turn 第一次 Pi 调用前，只调用一次 PI.sessions，找到 normalized cwd 严格等于 '{local}' 的 live session，并与 PI.sessions 返回的 `binding` 比较；仅当二者不同才 PI.init 当前 sessionId。记忆中的 sessionId 只能作为缓存。若 Pi 调用出现 session missing / disconnected / unavailable，必须重新按 exact cwd resolve，再重试该次 Pi 调用一次；绝不能 fallback 到其他项目/cwd。
2. 阅读 '{local}\.tunneldock\CONSTITUTION.md'。
3. 先读取 '{local}\.tunneldock\web_status.json'，再读取 '{local}\.tunneldock\INBOX_PROTOCOL.md'。若 `web_status.actions` 非空，必须先按 priority 处理这些 action，再创建新任务。只有需要当前任务/讨论细节时才读取一次 `web_context.json`；仅当 `web_status.json.state_token` 变化或 review gate 打开时才重新读取。只有轻量上下文确实不足时才读取完整 `project_room.json`。
4. 默认先读取持久化记忆中的：
   - '{local}\.project_memory\MEMORY_INDEX.md'
   - '{local}\.project_memory\PROJECT_STATE.md'
   - '{local}\.project_memory\SESSION_HANDOFF.md'
   - '{local}\.project_memory\MEMORY_PROTOCOL.md'
   然后只按当前任务读取对应领域记忆：MODEL_DESIGN.md、DATA_CATALOG.md、EXPERIMENTS.md、RESULTS.md、REFERENCES.md、DOCUMENTS.md 或 DECISIONS.md。
5. 对话历史只作为工作记忆；项目持久化文件才是 source of truth。
6. ChatGPT / Codex / Gemini 需要跨 Agent 留下问题、分歧、任务、handoff 或实验结果时，按 INBOX_PROTOCOL.md 向 '{local}\.tunneldock\inbox\' 写入单个 JSON event；不得直接修改 project_room.json。
7. 需要 Codex 与 Gemini 独立给意见时，只写一个 `consult.request`。纯意见/分析默认使用 `finalization=review_only`；只有会改变 canonical 项目事实、accepted result、设计真相且必须落入 memory 的咨询才用 `finalization=durable`。TunnelDock 自动调度指定 Agent，并生成独立 `consultation_id`。严禁为了等待 worker 而让同一个网页回复流无限保持开启：只轮询 `web_status.json`，最多 5 次/约 20 秒。若 `state_token` 未变化且 barrier 仍未打开，本轮只返回一个极短 checkpoint，说明仍在等待/重试哪些 Agent，不给实质性结论；下一次用户消息再读取 `web_status.json`，从持久化 barrier 继续。`ready_for_review=true` 后，再读取 `web_context.json` 和每个成功 Agent 的完整 `agents[].handoff_path`，把 failed/blocked 视为缺失证据，完成自己的 review 并写入 `consult.reviewed`。若为 `review_only`，此时应直接 finalized，不再做 memory/cleanup；若为 `durable`，再更新 PROJECT_STATE.md、SESSION_HANDOFF.md 和受影响领域记忆并写 `memory.commit`，完成 cleanup 并写 `cleanup.commit`，直到 state=`finalized`。若存在实质分歧，可复用同一 `thread_id` 追问一次，但新一轮拥有新的 consultation barrier。
8. 只要涉及代码/方法取舍，网页 GPT 不能只是转述 Agent 结论；必须通过 Pi 自己抽查相关 diff / 源码，再和我讨论。只抓决定性的代码点：研究意图是否一致、数据/控制流是否正确、边界/错误处理、测试/证据是否足够。
9. 在让我拍板前，给一个极简决策包：(a) 2-4 个代码 review 要点，(b) Agent 共识/分歧，(c) 最多 2-3 个现实选项及代价，(d) 你的技术倾向和需要我决定的唯一关键点。我始终是最终决策者。
10. 我明确做出决定后，写一个 `decision.record` inbox event，把最终结论持久化到 DECISIONS.md；不能把尚未确认的建议当成我的决定记录。

网页回复上下文预算：
- 默认每次网页回复最多 6 个短要点，或约 500 个中文字符；除非我明确要求展开。
- 不在网页里粘贴原始工具输出、完整 worker handoff、长日志或大段代码。单次 Pi 工具结果尽量 <= 8 KiB：先 grep/find，再按行号/offset 小段读取；不要在一个 web turn 里批量调用多个可能返回几十 KB 的 read/bash，大诊断先落盘，只读取决定性片段。
- 多智能体咨询期间，在 matching consultation 进入 state=`finalized` 前不得给实质性结论。`review_only` 只要求 Agent 完成 + 网页 review；`durable` 额外要求 canonical memory commit + cleanup/integration。完成后只汇总代码 review 要点、共识/分歧、决定性证据、决策选项、下一步。不要逐字复述两个 Agent 的回答。
- `web_status.json` 是网页端轮询/checkpoint 面；`web_context.json` 是轻量详情快照，只在 `state_token` 变化或确实需要当前任务/讨论细节时读取。`project_room.json`、完整 memory、run artifact 继续按需读取。

科研工作硬规则：
- 目标是顶会论文；代码和算法表达必须简洁、逻辑清晰、可验证、正确。
- 不创建 V1/V2/V3、*_old、*_backup、*_new、*_final 等平行旧版本；遵守项目自己的 Git 规则。允许 Git 时用本地 Git 保留历史；禁止 Git 时只保留当前代码树，并记录 source hash / config / evidence。
- 替代实现/证据验证后必须整合权威文档，并清理过时代码、废弃脚本、临时日志、cache、debug/tmp/scratch；旧内容只有作为明确归档或唯一复现/调试证据时才保留。
- 有意义的实验都要记录，包括 negative result。
- 重要决策/结果发生后要提炼更新项目记忆，而不是无限追加聊天记录。
- ChatGPT 负责科研规划、任务拆解、额度综合调配、实验解释和最终 review。
- Codex 是稳健实现的主工程 Agent。
- Antigravity Agent/Gemini 负责视觉、PCA/图表/视频/UI 和快速探索；这里接入的是 Agent Manager 会话，不依赖 IDE；涉及核心代码默认需要 Codex review。
- Codex 的重要算法改动需要 ChatGPT 做科研意图和方法一致性 review。
- 三个 Agent 可以互相讨论、质疑和反驳；我始终是最终研究决策者。

创建新任务前，必须先清空最高优先级 `web_status.actions`：review 已完成 worker task（写 `task.review`）、重试 invalid consultation、处理 stale blocked/backlog。然后再看 tasks/capacity/experiments/discussion 并分配新工作。worker handoff 不等于完成；只有确认真实 completed run、非空 handoff、决定性源码/diff 与 focused verification 后，才能 `task.review=accept`。"#,
            name = snapshot.config.name,
            id = snapshot.config.id,
            local = snapshot.config.local_root,
            remote = remote,
            workspace = snapshot.config.workspace_id.as_deref().unwrap_or("unbound")
        ))
    }
}

#[tauri::command]
pub fn initialize_project_git(
    state: State<'_, Arc<AppState>>,
    project_id: String,
) -> Result<String, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    let repo_root = PathBuf::from(&snapshot.config.repo_root);

    if !repo_root.exists() {
        return Err(format!("repo_root 不存在: {}", repo_root.display()));
    }

    if repo_root.join(".git").exists() {
        return Ok("already_initialized".to_string());
    }

    let nested = discover_nested_git_roots(&repo_root);
    if !nested.is_empty() {
        let preview = nested
            .iter()
            .take(6)
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "repo_root 下已存在独立 Git 仓库，拒绝直接创建 umbrella Git。请先在 Project Config 中把 repo_root 指向真正需要管理的代码仓库。发现: {}",
            preview
        ));
    }

    let out = execute_cmd("git", &["init"], Some(&repo_root));
    if !out.success {
        return Err(if out.stderr.trim().is_empty() {
            "git init 失败".to_string()
        } else {
            out.stderr.trim().to_string()
        });
    }

    Ok("initialized".to_string())
}

pub(super) fn stale_name_reason(name: &str, is_dir: bool) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let dir_candidates = ["old", "backup", "archive", "tmp", "debug"];
    if is_dir && dir_candidates.contains(&lower.as_str()) {
        return Some(format!("目录名 {} 属于过时/临时内容候选", name));
    }

    let file_markers = [
        "_v1", "_v2", "_v3", "-v1", "-v2", "-v3", "_old", "_backup", "_new", "_final", ".old",
        ".bak", ".tmp", ".temp", ".log", ".trace",
    ];
    if file_markers.iter().any(|marker| lower.contains(marker)) {
        return Some(format!("文件名包含版本/备份标记: {}", name));
    }

    None
}

pub(super) fn scan_hygiene_dir(
    root: &Path,
    current: &Path,
    depth: usize,
    out: &mut Vec<HygieneCandidate>,
) {
    if depth > 5 || out.len() >= 300 {
        return;
    }

    let Ok(entries) = fs::read_dir(current) else {
        return;
    };

    for entry in entries.flatten() {
        if out.len() >= 300 {
            break;
        }

        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(
            name.as_str(),
            ".git" | ".tunneldock" | "node_modules" | "target" | ".venv" | "__pycache__"
        ) {
            continue;
        }

        let is_dir = path.is_dir();
        if let Some(reason) = stale_name_reason(&name, is_dir) {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            out.push(HygieneCandidate {
                path: relative,
                kind: if is_dir { "directory" } else { "file" }.to_string(),
                reason,
            });
        }

        if is_dir {
            scan_hygiene_dir(root, &path, depth + 1, out);
        }
    }
}

#[tauri::command]
pub fn scan_project_hygiene(
    state: State<'_, Arc<AppState>>,
    project_id: String,
) -> Result<Vec<HygieneCandidate>, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    let root = PathBuf::from(snapshot.config.local_root);
    if !root.exists() {
        return Err(format!("项目路径不存在: {}", root.display()));
    }

    let mut candidates = Vec::new();
    scan_hygiene_dir(&root, &root, 0, &mut candidates);
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::{
        discover_nested_git_roots, ensure_project_memory, load_project_memory,
        parse_antigravity_quota_summary, parse_codex_app_server_rate_limits,
        parse_codex_rate_limit, project_session_binding, scopes_conflict, stale_name_reason,
        ProjectRemote, ProjectRoomConfig,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(case: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "tunneldock-project-room-{}-{}-{}",
            std::process::id(),
            nonce,
            case
        ))
    }

    fn test_config(root: &std::path::Path) -> ProjectRoomConfig {
        ProjectRoomConfig {
            id: "test_project".to_string(),
            name: "Test Project".to_string(),
            local_root: root.to_string_lossy().to_string(),
            repo_root: root.to_string_lossy().to_string(),
            workspace_id: None,
            codex_thread_id: None,
            codex_automation_thread_id: None,
            antigravity_cascade_id: None,
            remote: ProjectRemote::default(),
            enabled: true,
            keep_session_alive: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn project_session_binding_identity_does_not_depend_on_runtime_session_id() {
        let root = temp_root("session-binding");
        let mut config = test_config(&root);
        config.workspace_id = Some("ws-test".to_string());

        let first = project_session_binding(&config, Some("session-a"));
        let second = project_session_binding(&config, Some("session-b"));
        assert_eq!(first["logical_id"], second["logical_id"]);
        assert_eq!(first["current_session_id"], "session-a");
        assert_eq!(second["current_session_id"], "session-b");
        assert_eq!(first["session_id_is_ephemeral"], true);
    }

    #[test]
    fn preserves_existing_canonical_project_memory() {
        let root = temp_root("memory-preserve");
        let memory_dir = root.join(".project_memory");
        fs::create_dir_all(&memory_dir).expect("memory dir");
        let state_path = memory_dir.join("PROJECT_STATE.md");
        fs::write(&state_path, "# Existing State\nimportant evidence").expect("seed state");

        let config = test_config(&root);
        ensure_project_memory(&config).expect("ensure memory");
        let memory = load_project_memory(&config).expect("load memory");

        assert_eq!(
            fs::read_to_string(&state_path).expect("read state"),
            "# Existing State\nimportant evidence"
        );
        assert!(memory.project_state.contains("important evidence"));
        for file_name in [
            "MEMORY_INDEX.md",
            "SESSION_HANDOFF.md",
            "DECISIONS.md",
            "MODEL_DESIGN.md",
            "DATA_CATALOG.md",
            "EXPERIMENTS.md",
            "RESULTS.md",
            "REFERENCES.md",
            "DOCUMENTS.md",
            "MEMORY_PROTOCOL.md",
        ] {
            assert!(memory_dir.join(file_name).exists(), "missing {file_name}");
        }
        assert!(memory
            .memory_protocol
            .contains("ChatGPT Web updates `PROJECT_STATE.md`"));

        fs::remove_dir_all(root).expect("remove temp project");
    }

    #[test]
    fn detects_nested_git_repositories_before_umbrella_init() {
        let root = temp_root("nested-git");
        fs::create_dir_all(root.join("third_party").join("repo").join(".git")).expect("nested git");
        let nested = discover_nested_git_roots(&root);
        assert_eq!(nested.len(), 1);
        assert!(nested[0].ends_with(std::path::Path::new("third_party").join("repo")));
        fs::remove_dir_all(root).expect("remove temp project");
    }

    #[test]
    fn parses_codex_rate_limit_telemetry() {
        let value = serde_json::json!({
            "payload": {
                "rate_limits": {
                    "primary": {
                        "used_percent": 58.0,
                        "window_minutes": 10080,
                        "resets_at": 1791047782
                    }
                }
            }
        });

        let (remaining, reset) =
            parse_codex_rate_limit(&value).expect("valid rate limit should parse");
        assert_eq!(remaining, 42.0);
        assert_eq!(reset, Some(1791047782));
        assert!(parse_codex_rate_limit(&serde_json::json!({"payload": {}})).is_none());
    }

    #[test]
    fn parses_codex_live_rate_limit_response() {
        let value = serde_json::json!({
            "id": 2,
            "result": {
                "rateLimits": {
                    "limitId": "codex",
                    "normalModelSlug": null,
                    "primary": {
                        "usedPercent": 62,
                        "windowDurationMins": 10080,
                        "resetsAt": 1791047782
                    }
                },
                "rateLimitsByLimitId": {
                    "codex": {
                        "limitId": "codex",
                        "primary": {
                            "usedPercent": 62,
                            "windowDurationMins": 10080,
                            "resetsAt": 1791047782
                        }
                    }
                }
            }
        });

        let (remaining, reset, model) =
            parse_codex_app_server_rate_limits(&value).expect("live quota should parse");
        assert_eq!(remaining, 38.0);
        assert_eq!(reset, Some(1791047782));
        assert_eq!(model.as_deref(), Some("codex"));
    }

    #[test]
    fn parses_antigravity_gemini_quota_and_uses_limiting_bucket() {
        let value = serde_json::json!({
            "response": {
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "buckets": [
                            {
                                "bucketId": "gemini-weekly",
                                "displayName": "Weekly Limit Remaining",
                                "window": "weekly",
                                "remainingFraction": 0.19,
                                "resetTime": "2026-10-02T01:15:23Z"
                            },
                            {
                                "bucketId": "gemini-5h",
                                "displayName": "Five Hour Limit Remaining",
                                "window": "5h",
                                "remainingFraction": 1.0,
                                "resetTime": "2026-10-01T12:26:15Z"
                            }
                        ]
                    },
                    {
                        "displayName": "Claude and GPT models",
                        "buckets": [{"remainingFraction": 0.01}]
                    }
                ]
            }
        });

        let (remaining, reset, group, windows) =
            parse_antigravity_quota_summary(&value).expect("Gemini quota should parse");
        assert!((remaining - 19.0).abs() < f64::EPSILON);
        assert_eq!(reset.as_deref(), Some("2026-10-02T01:15:23Z"));
        assert_eq!(group.as_deref(), Some("Gemini Models"));
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].window, "weekly");
        assert_eq!(windows[1].window, "5h");
        assert!((windows[0].remaining_percent - 19.0).abs() < f64::EPSILON);
        assert!((windows[1].remaining_percent - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn task_write_scopes_detect_real_overlap() {
        assert!(scopes_conflict(
            &["model/**".to_string()],
            &["model/decoder/**".to_string()]
        ));
        assert!(scopes_conflict(&[], &["visualization/**".to_string()]));
        assert!(!scopes_conflict(
            &["model/**".to_string()],
            &["visualization/**".to_string()]
        ));
    }

    #[test]
    fn hygiene_marks_versioned_and_backup_names() {
        assert!(stale_name_reason("model_v2.py", false).is_some());
        assert!(stale_name_reason("method_final.md", false).is_some());
        assert!(stale_name_reason("backup", true).is_some());
        assert!(stale_name_reason("model.py", false).is_none());
        assert!(stale_name_reason("src", true).is_none());
    }
}
