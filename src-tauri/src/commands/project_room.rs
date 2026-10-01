use crate::models::{
    AgentCapacity, AgentQuotaWindow, AgentRun, AgentRuntimeInfo, HygieneCandidate,
    ProjectAgentPolicy, ProjectDiscussionMessage, ProjectExperiment, ProjectMemory, ProjectRemote,
    ProjectRoomConfig, ProjectRoomSnapshot, ProjectRoomSummary, ProjectTask,
};
use crate::state::AppState;
use crate::utils::chappie_broker;
use crate::utils::cmd::{execute_cmd, find_executable, is_process_running};
use crate::utils::time::local_now_rfc3339;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::{AppHandle, State};

const REGISTRY_FILE: &str = "registry.json";
const CONFIG_FILE: &str = "project.json";
const PROJECT_MEMORY_DIR: &str = ".project_memory";
const PROJECT_STATE_FILE: &str = "PROJECT_STATE.md";
const SESSION_HANDOFF_FILE: &str = "SESSION_HANDOFF.md";
const DECISIONS_FILE: &str = "DECISIONS.md";
const MEMORY_EXPERIMENTS_FILE: &str = "EXPERIMENTS.md";
const MEMORY_PROTOCOL_FILE: &str = "MEMORY_PROTOCOL.md";
const LEGACY_CONTROL_MEMORY_FILE: &str = "memory.json";
const AGENTS_FILE: &str = "agents.json";
const CAPACITIES_FILE: &str = "capacities.json";
const TASKS_FILE: &str = "tasks.json";
const EXPERIMENTS_FILE: &str = "experiments.json";
const DISCUSSION_FILE: &str = "discussion.json";
const RUNS_FILE: &str = "runs.json";
const BRIDGE_DIR: &str = ".tunneldock";
const BRIDGE_FILE: &str = "project_room.json";
const WEB_CONTEXT_FILE: &str = "web_context.json";
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
mod coordination;
mod memory;
mod telemetry;

use agent_worker::{
    append_system_message, build_worker_command, ensure_dispatch_scope_is_safe, next_id,
    refresh_run_states_unlocked, run_dir, task_prompt,
};
use coordination::{reconcile_project_operational_state_once, sync_project_bridge};
use memory::{ensure_project_memory, load_project_memory, project_memory_dir, save_project_memory};
use telemetry::{agent_runtimes, current_agent_capacities, find_codex_executable};

#[cfg(test)]
use agent_worker::scopes_conflict;
#[cfg(test)]
use telemetry::{
    parse_antigravity_csrf_token, parse_antigravity_quota_summary,
    parse_codex_app_server_rate_limits, parse_codex_rate_limit,
};

fn projects_root(state: &AppState) -> PathBuf {
    state.app_data_dir.join("projects")
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
            root: "/data3/guodongyan/lpl/cowtracker_tap".to_string(),
            environment: "/data3/guodongyan/lpl/envs/cowtracker/bin/python".to_string(),
            notes: "Formal source/training workspace; local D:\\point_tracking stores references and durable project memory.".to_string(),
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
            if config.remote.root.trim().is_empty()
                || (project_id == "point_tracking"
                    && config.remote.root == "/data3/guodongyan/lpl/GeoSAMTracker")
            {
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
    Ok(ProjectRoomSnapshot {
        memory: load_project_memory(&config)?,
        config,
        agents: read_json(&dir.join(AGENTS_FILE))?,
        capacities: read_json(&dir.join(CAPACITIES_FILE))?,
        tasks: read_json(&dir.join(TASKS_FILE))?,
        experiments: read_json(&dir.join(EXPERIMENTS_FILE))?,
        discussion: read_json(&dir.join(DISCUSSION_FILE))?,
        runs: read_json(&dir.join(RUNS_FILE))?,
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
        let stdout = File::create(&log_path)
            .map_err(|error| format!("创建 {} 失败: {}", log_path.display(), error))?;
        let stderr = File::create(&error_path)
            .map_err(|error| format!("创建 {} 失败: {}", error_path.display(), error))?;
        let sandbox = if task.kind == "consultation" {
            "read-only"
        } else {
            "workspace-write"
        };
        let mut args = vec![
            "exec".to_string(),
            "-C".to_string(),
            snapshot.config.local_root.clone(),
            "--sandbox".to_string(),
            sandbox.to_string(),
            "--skip-git-repo-check".to_string(),
            "--json".to_string(),
            "-o".to_string(),
            output_path.to_string_lossy().to_string(),
        ];
        if let Some(model) = snapshot
            .capacities
            .iter()
            .find(|capacity| capacity.agent_id == "codex")
            .and_then(|capacity| capacity.model.as_ref())
            .filter(|model| !model.trim().is_empty())
        {
            args.push("--model".to_string());
            args.push(model.clone());
        }
        args.push(prompt.clone());

        let cwd = PathBuf::from(&snapshot.config.local_root);
        let mut command = build_worker_command(&executable, &args, &cwd, stdout, stderr);
        let child = command
            .spawn()
            .map_err(|error| format!("启动 Codex worker 失败: {}", error))?;
        let pid = child.id();
        state.running_agent_pids.lock().insert(run_id.clone(), pid);
        (Some(pid), None, None)
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
            let mut workspaces = state.workspaces.lock();
            if let Some(workspace) = workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
            {
                workspace.status = if matches!(session.status.as_str(), "executing" | "generating")
                {
                    "executing".to_string()
                } else {
                    "ready".to_string()
                };
                workspace.session_id = Some(session.id.clone());
                workspace.binding_count = session.binding_count;
                workspace.error_message = None;
                adopted_any = true;
            }
            continue;
        }

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
        summaries.push(ProjectRoomSummary {
            id: snapshot.config.id.clone(),
            name: snapshot.config.name.clone(),
            local_root: snapshot.config.local_root.clone(),
            repo_root: snapshot.config.repo_root.clone(),
            workspace_id: snapshot.config.workspace_id.clone(),
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
    if task.id.trim().is_empty() {
        task.id = next_id("TASK");
        task.created_at = now.clone();
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
1. Bind to the local Pi/Chappie session whose cwd is exactly '{local}'. Use sessions -> cwd -> sessionId -> init; never choose by an old remembered session ID.
2. Read '{local}\.tunneldock\CONSTITUTION.md'.
3. Read '{local}\.tunneldock\web_context.json' and '{local}\.tunneldock\INBOX_PROTOCOL.md'. Read full `project_room.json` only when the current question needs details omitted from the compact web context.
4. Read the default durable memory set:
   - '{local}\.project_memory\PROJECT_STATE.md'
   - '{local}\.project_memory\SESSION_HANDOFF.md'
   - '{local}\.project_memory\MEMORY_PROTOCOL.md'
   Read DECISIONS.md or EXPERIMENTS.md only when the current question actually needs historical decisions or experiment detail.
5. Treat chat history as working memory only. Durable project files are the source of truth.
6. When Codex/Gemini/ChatGPT need to exchange a durable question, disagreement, task, handoff, or experiment result, write one JSON event to '{local}\.tunneldock\inbox\' using INBOX_PROTOCOL.md. Never edit project_room.json directly.
7. When you need independent opinions from Codex and Gemini, write one `consult.request` event. TunnelDock will auto-dispatch both read-only consultations and collect their short handoffs into one discussion thread. If there is a material disagreement, you may send one focused follow-up using the same `thread_id`; avoid open-ended agent ping-pong.

Web response budget:
- Keep normal web replies compact: at most 6 short bullets or roughly 350 English words unless I explicitly ask for detail.
- Do not paste raw tool output, full worker handoffs, long logs, or large code excerpts. Put details in project files/artifacts and cite paths.
- After a multi-agent consultation, report only consensus, disagreement, decisive evidence, and the next action. Do not quote both agents verbatim.
- `web_context.json` is the default lightweight web snapshot. Read `project_room.json`, full memory, or run artifacts only when the current question actually needs them.

Research operating rules:
- The goal is top-conference research. Keep code and algorithms simple, explicit, and correct.
- Do not create V1/V2/V3, *_old, *_backup, *_new, or *_final copies. Follow the project's own Git policy; if Git is disallowed, keep one current tree and record hashes/configs/evidence instead.
- Remove obsolete files, temporary logs, superseded scripts, and stale documentation after replacement is verified.
- Record meaningful experiments, including negative results.
- Refine project memory after meaningful decisions/results instead of appending raw chat logs.
- ChatGPT is the coordinator and quota-allocation brain.
- Codex is the primary robust engineer.
- Antigravity Agent/Gemini is the visual/exploration agent; core-code changes require Codex review. It is integrated as an Agent Manager session, not as an IDE workspace.
- Important Codex algorithm changes require ChatGPT methodological review.
- You and the other agents may discuss and challenge each other; I remain the final decision maker.

When allocating work, first inspect current tasks, agent capacity telemetry, experiments, and discussion in web_context.json, then explain the allocation briefly before asking workers to execute."#,
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
1. 通过 Pi/Chappie 的 sessions -> cwd -> sessionId -> init，绑定 cwd 严格等于 '{local}' 的本地 Session；不要使用记忆中的旧 sessionId 猜测绑定。
2. 阅读 '{local}\.tunneldock\CONSTITUTION.md'。
3. 阅读 '{local}\.tunneldock\web_context.json' 和 '{local}\.tunneldock\INBOX_PROTOCOL.md'。只有当前问题确实需要轻量上下文中省略的细节时，才读取完整 `project_room.json`。
4. 默认只读取持久化记忆中的：
   - '{local}\.project_memory\PROJECT_STATE.md'
   - '{local}\.project_memory\SESSION_HANDOFF.md'
   - '{local}\.project_memory\MEMORY_PROTOCOL.md'
   只有涉及历史决策或实验细节时，再读取 DECISIONS.md / EXPERIMENTS.md。
5. 对话历史只作为工作记忆；项目持久化文件才是 source of truth。
6. ChatGPT / Codex / Gemini 需要跨 Agent 留下问题、分歧、任务、handoff 或实验结果时，按 INBOX_PROTOCOL.md 向 '{local}\.tunneldock\inbox\' 写入单个 JSON event；不得直接修改 project_room.json。
7. 需要 Codex 与 Gemini 独立给意见时，只写一个 `consult.request`；TunnelDock 自动创建两个只读咨询任务、分别调度，并把短 handoff 收敛到同一个 discussion thread。若存在实质分歧，可复用同一 `thread_id` 再追问一次；默认不要无限来回辩论。

网页回复上下文预算：
- 默认每次网页回复最多 6 个短要点，或约 500 个中文字符；除非我明确要求展开。
- 不在网页里粘贴原始工具输出、完整 worker handoff、长日志或大段代码；细节写入项目文件/产物，只返回路径和结论。
- 多智能体咨询完成后，只汇总：共识、分歧、决定性证据、下一步。不要逐字复述两个 Agent 的回答。
- `web_context.json` 是网页端默认轻量快照；只有当前问题确实需要时才读取 `project_room.json`、完整 memory 或 run artifact。

科研工作硬规则：
- 目标是顶会论文；代码和算法表达必须简洁、逻辑清晰、可验证、正确。
- 不创建 V1/V2/V3、*_old、*_backup、*_new、*_final 等平行旧版本；遵守项目自己的 Git 规则。允许 Git 时用本地 Git 保留历史；禁止 Git 时只保留当前代码树，并记录 source hash / config / evidence。
- 替代实现验证后清理过时代码、临时日志、废弃脚本和失效文档。
- 有意义的实验都要记录，包括 negative result。
- 重要决策/结果发生后要提炼更新项目记忆，而不是无限追加聊天记录。
- ChatGPT 负责科研规划、任务拆解、额度综合调配、实验解释和最终 review。
- Codex 是稳健实现的主工程 Agent。
- Antigravity Agent/Gemini 负责视觉、PCA/图表/视频/UI 和快速探索；这里接入的是 Agent Manager 会话，不依赖 IDE；涉及核心代码默认需要 Codex review。
- Codex 的重要算法改动需要 ChatGPT 做科研意图和方法一致性 review。
- 三个 Agent 可以互相讨论、质疑和反驳；我始终是最终研究决策者。

每次调度前，先从 web_context.json 查看当前 tasks、agent capacity、experiments 和 discussion，用一两句话说明分配原因，然后再让 worker 执行。"#,
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

fn stale_name_reason(name: &str, is_dir: bool) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let dir_candidates = ["old", "backup", "archive", "tmp", "debug"];
    if is_dir && dir_candidates.contains(&lower.as_str()) {
        return Some(format!("目录名 {} 属于过时/临时内容候选", name));
    }

    let file_markers = [
        "_v1", "_v2", "_v3", "-v1", "-v2", "-v3", "_old", "_backup", "_new", "_final", ".old",
        ".bak",
    ];
    if file_markers.iter().any(|marker| lower.contains(marker)) {
        return Some(format!("文件名包含版本/备份标记: {}", name));
    }

    None
}

fn scan_hygiene_dir(root: &Path, current: &Path, depth: usize, out: &mut Vec<HygieneCandidate>) {
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
        parse_antigravity_csrf_token, parse_antigravity_quota_summary,
        parse_codex_app_server_rate_limits, parse_codex_rate_limit, scopes_conflict,
        stale_name_reason, ProjectRemote, ProjectRoomConfig,
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
            antigravity_cascade_id: None,
            remote: ProjectRemote::default(),
            enabled: true,
            keep_session_alive: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
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
        assert!(memory_dir.join("SESSION_HANDOFF.md").exists());
        assert!(memory_dir.join("MEMORY_PROTOCOL.md").exists());

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
    fn parses_antigravity_csrf_from_local_app_config() {
        let html = r#"<script>window.__APP_CONFIG__ = {"productName":"antigravity","csrfToken":"token-123"};</script>"#;
        assert_eq!(
            parse_antigravity_csrf_token(html).as_deref(),
            Some("token-123")
        );
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
