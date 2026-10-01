use crate::models::{
    AgentCapacity, AgentRun, AgentRuntimeInfo, HygieneCandidate, ProjectAgentPolicy,
    ProjectDiscussionMessage, ProjectExperiment, ProjectMemory, ProjectRemote, ProjectRoomConfig,
    ProjectRoomSnapshot, ProjectRoomSummary, ProjectTask,
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
const CONSTITUTION_FILE: &str = "CONSTITUTION.md";
const INBOX_DIR: &str = "inbox";
const INBOX_PROTOCOL_FILE: &str = "INBOX_PROTOCOL.md";
const INBOX_ERROR_FILE: &str = "inbox_error.json";
const RUNS_DIR: &str = "runs";

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

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
            display_name: "Antigravity / Gemini".to_string(),
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
        })
        .collect()
}

fn command_version(executable: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new(executable);
    command.args(args);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn find_codex_executable() -> Option<PathBuf> {
    if let Some(path) = find_executable("codex") {
        return Some(PathBuf::from(path));
    }

    #[cfg(target_os = "windows")]
    {
        let local = std::env::var_os("LOCALAPPDATA")?;
        let bin_root = PathBuf::from(local)
            .join("OpenAI")
            .join("Codex")
            .join("bin");
        let mut candidates = fs::read_dir(bin_root)
            .ok()?
            .flatten()
            .map(|entry| entry.path().join("codex.exe"))
            .filter(|path| path.exists())
            .collect::<Vec<_>>();

        candidates.sort_by_key(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok());
        candidates.pop()
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

fn find_antigravity_executable() -> Option<PathBuf> {
    if let Some(path) = find_executable("antigravity") {
        return Some(PathBuf::from(path));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let path = PathBuf::from(local)
                .join("Programs")
                .join("antigravity")
                .join("Antigravity.exe");
            if path.exists() {
                return Some(path);
            }
        }

        let portable = PathBuf::from(r"D:\Antigravity\Antigravity\Antigravity.exe");
        if portable.exists() {
            return Some(portable);
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn antigravity_language_server_pid() -> Option<u32> {
    use sysinfo::{ProcessesToUpdate, System};

    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    system.processes().iter().find_map(|(pid, process)| {
        let name = process.name().to_string_lossy().to_ascii_lowercase();
        (name == "language_server.exe" || name == "language_server").then(|| pid.as_u32())
    })
}

fn parse_antigravity_csrf_token(html: &str) -> Option<String> {
    let marker = r#""csrfToken":""#;
    let start = html.find(marker)? + marker.len();
    let rest = &html[start..];
    let end = rest.find('"')?;
    let token = rest[..end].trim();
    (!token.is_empty()).then(|| token.to_string())
}

#[cfg(target_os = "windows")]
fn listening_ports_for_pid(pid: u32) -> Vec<u16> {
    let out = execute_cmd("netstat", &["-ano", "-p", "tcp"], None);
    if !out.success {
        return Vec::new();
    }

    let pid_text = pid.to_string();
    let mut ports = out
        .stdout
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 5
                || !fields[0].eq_ignore_ascii_case("TCP")
                || !fields[3].eq_ignore_ascii_case("LISTENING")
                || fields[4] != pid_text
            {
                return None;
            }

            let local = fields[1];
            local.rsplit_once(':')?.1.parse::<u16>().ok()
        })
        .collect::<Vec<_>>();
    ports.sort_unstable();
    ports.dedup();
    ports
}

fn parse_antigravity_quota_summary(
    value: &serde_json::Value,
) -> Option<(f64, Option<String>, Option<String>)> {
    let groups = value.get("response")?.get("groups")?.as_array()?;
    let group = groups.iter().find(|group| {
        group
            .get("displayName")
            .and_then(serde_json::Value::as_str)
            .map(|name| name.to_ascii_lowercase().contains("gemini"))
            .unwrap_or(false)
    })?;
    let buckets = group.get("buckets")?.as_array()?;

    let (remaining, bucket) = buckets
        .iter()
        .filter(|bucket| {
            !bucket
                .get("disabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|bucket| {
            let remaining = bucket
                .get("remainingFraction")
                .and_then(serde_json::Value::as_f64)?;
            Some((remaining, bucket))
        })
        .min_by(|(left, _), (right, _)| {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        })?;

    Some((
        (remaining * 100.0).clamp(0.0, 100.0),
        bucket
            .get("resetTime")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        group
            .get("displayName")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
    ))
}

fn antigravity_quota_capacity() -> Option<AgentCapacity> {
    #[cfg(not(target_os = "windows"))]
    {
        return None;
    }

    #[cfg(target_os = "windows")]
    {
        let pid = antigravity_language_server_pid()?;
        let ports = listening_ports_for_pid(pid);
        if ports.is_empty() {
            return None;
        }

        let client = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(true)
            .no_proxy()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .ok()?;

        let mut csrf_token = None;
        for port in &ports {
            for scheme in ["http", "https"] {
                let url = format!("{}://127.0.0.1:{}/", scheme, port);
                let response = match client.get(url).send() {
                    Ok(response) if response.status().is_success() => response,
                    _ => continue,
                };
                let Ok(html) = response.text() else {
                    continue;
                };
                if let Some(token) = parse_antigravity_csrf_token(&html) {
                    csrf_token = Some(token);
                    break;
                }
            }
            if csrf_token.is_some() {
                break;
            }
        }
        let csrf_token = csrf_token?;

        let path = "/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary";
        for port in ports {
            for scheme in ["http", "https"] {
                let url = format!("{}://127.0.0.1:{}{}", scheme, port, path);
                let response = match client
                    .post(url)
                    .header("x-codeium-csrf-token", &csrf_token)
                    .json(&serde_json::json!({ "forceRefresh": true }))
                    .send()
                {
                    Ok(response) if response.status().is_success() => response,
                    _ => continue,
                };

                let Ok(value) = response.json::<serde_json::Value>() else {
                    continue;
                };
                let Some((remaining_percent, reset_at, model)) =
                    parse_antigravity_quota_summary(&value)
                else {
                    continue;
                };

                return Some(AgentCapacity {
                    agent_id: "gemini".to_string(),
                    available: true,
                    remaining_percent: Some(remaining_percent),
                    reset_at,
                    model,
                    source: "antigravity_quota_summary".to_string(),
                    confidence: "runtime_telemetry".to_string(),
                    updated_at: local_now_rfc3339(),
                });
            }
        }

        None
    }
}

fn collect_jsonl_files(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 8 || out.len() >= 5000 {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, depth + 1, out);
        } else if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}

fn parse_codex_rate_limit(value: &serde_json::Value) -> Option<(f64, Option<i64>)> {
    let primary = value.get("payload")?.get("rate_limits")?.get("primary")?;
    let used_percent = primary.get("used_percent")?.as_f64()?;
    let resets_at = primary.get("resets_at").and_then(|value| value.as_i64());
    Some(((100.0 - used_percent).clamp(0.0, 100.0), resets_at))
}

fn system_time_local_rfc3339(time: std::time::SystemTime) -> String {
    let local: chrono::DateTime<chrono::Local> = time.into();
    local.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

fn latest_codex_capacity() -> AgentCapacity {
    let now = local_now_rfc3339();
    let executable = find_codex_executable();

    let mut capacity = AgentCapacity {
        agent_id: "codex".to_string(),
        available: executable.is_some(),
        remaining_percent: None,
        reset_at: None,
        model: None,
        source: if executable.is_some() {
            "runtime_detected".to_string()
        } else {
            "unavailable".to_string()
        },
        confidence: if executable.is_some() {
            "availability_only".to_string()
        } else {
            "unavailable".to_string()
        },
        updated_at: now,
    };

    let Some(home) = dirs::home_dir() else {
        return capacity;
    };
    let sessions_root = home.join(".codex").join("sessions");
    let mut files = Vec::new();
    collect_jsonl_files(&sessions_root, 0, &mut files);
    files.sort_by_key(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok());

    for path in files.into_iter().rev().take(60) {
        let source_modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok();
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };

        for line in content.lines().rev() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some((remaining_percent, resets_at)) = parse_codex_rate_limit(&value) else {
                continue;
            };

            capacity.available = true;
            capacity.updated_at = source_modified
                .map(system_time_local_rfc3339)
                .unwrap_or_else(local_now_rfc3339);
            capacity.reset_at = resets_at
                .and_then(|epoch| chrono::DateTime::from_timestamp(epoch, 0))
                .map(|utc| {
                    utc.with_timezone(&chrono::Local)
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
                });
            capacity.source = "codex_rollout_rate_limits".to_string();

            let reset_expired = resets_at
                .map(|epoch| epoch <= chrono::Utc::now().timestamp())
                .unwrap_or(false);
            let age = source_modified
                .and_then(|modified| modified.elapsed().ok())
                .unwrap_or_else(|| std::time::Duration::from_secs(u64::MAX));

            if reset_expired {
                capacity.remaining_percent = None;
                capacity.confidence = "expired_runtime_telemetry".to_string();
            } else {
                capacity.remaining_percent = Some(remaining_percent);
                capacity.confidence = if age <= std::time::Duration::from_secs(30 * 60) {
                    "runtime_telemetry".to_string()
                } else {
                    "stale_runtime_telemetry".to_string()
                };
            }
            return capacity;
        }
    }

    capacity
}

fn current_agent_capacities() -> Vec<AgentCapacity> {
    let now = local_now_rfc3339();
    let antigravity = find_antigravity_executable();
    let gemini_capacity = antigravity_quota_capacity().unwrap_or_else(|| AgentCapacity {
        agent_id: "gemini".to_string(),
        available: antigravity.is_some(),
        remaining_percent: None,
        reset_at: None,
        model: None,
        source: if antigravity.is_some() {
            "antigravity_runtime".to_string()
        } else {
            "unavailable".to_string()
        },
        confidence: if antigravity.is_some() {
            "quota_unavailable".to_string()
        } else {
            "unavailable".to_string()
        },
        updated_at: now.clone(),
    });

    vec![
        AgentCapacity {
            agent_id: "chatgpt".to_string(),
            available: true,
            remaining_percent: None,
            reset_at: None,
            model: None,
            source: "web_coordinator".to_string(),
            confidence: "quota_unavailable".to_string(),
            updated_at: now,
        },
        latest_codex_capacity(),
        gemini_capacity,
    ]
}

fn agent_runtimes() -> Vec<AgentRuntimeInfo> {
    let codex = find_codex_executable();
    let antigravity = find_antigravity_executable();

    vec![
        AgentRuntimeInfo {
            agent_id: "chatgpt".to_string(),
            installed: true,
            executable: None,
            version: None,
            dispatch_mode: "web_coordinator".to_string(),
            notes: "ChatGPT 网页通过 OpenAI Tunnel + Chappie + Pi 进入当前 Project Room。"
                .to_string(),
        },
        AgentRuntimeInfo {
            agent_id: "codex".to_string(),
            installed: codex.is_some(),
            executable: codex
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            version: codex
                .as_ref()
                .and_then(|path| command_version(path, &["--version"])),
            dispatch_mode: "headless_exec".to_string(),
            notes: "Codex 使用非交互 exec worker；任务完成后写入 Project Room handoff。"
                .to_string(),
        },
        AgentRuntimeInfo {
            agent_id: "gemini".to_string(),
            installed: antigravity.is_some(),
            executable: antigravity
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            version: antigravity
                .as_ref()
                .and_then(|path| command_version(path, &["--version"])),
            dispatch_mode: "interactive_chat".to_string(),
            notes:
                "Antigravity 打开项目专属 agent chat；视觉/探索任务优先，核心代码需 Codex review。"
                    .to_string(),
        },
    ]
}

fn default_research_goal(project_id: &str) -> &'static str {
    match project_id {
        "point_tracking" => {
            "推进 2D Point Tracking 研究，所有实现与实验服务于顶会论文的核心 hypothesis、方法验证和可复现性。"
        }
        "iqa_agent" => {
            "推进 IQA Agent 研究，保持 agent 设计、benchmark、实验记录与论文结论之间的一致性。"
        }
        "3d_mllm" => {
            "推进 3D + MLLM 空间推理研究，围绕几何表征、关系推理、数据与实验形成可验证的顶会论文证据链。"
        }
        _ => "推进当前科研项目，代码与实验均服务于可验证、可复现的论文结论。",
    }
}

fn default_memory_protocol() -> &'static str {
    r#"# Project Memory Protocol

## Source of truth
The files in this directory are the durable research memory for this project.
Pi/ChatGPT/Codex/Antigravity conversation history is working memory, not the source of truth.

## Canonical files
- PROJECT_STATE.md: current project snapshot; replace stale state instead of appending forever.
- SESSION_HANDOFF.md: most recent transition point for the next agent/session.
- DECISIONS.md: durable methodological/engineering decisions that change future behavior.
- EXPERIMENTS.md: reproducible evidence and meaningful negative results.

## Update policy
After meaningful work:
1. update PROJECT_STATE.md to the current truth;
2. replace SESSION_HANDOFF.md;
3. merge a decision only when it changes future behavior;
4. record an experiment only when it is reproducible or prevents repeated debugging;
5. remove contradictory or superseded summary statements.

Do not create V1/V2/V3, *_old, *_backup, *_new, or *_final copies. When the project's own instructions permit Git, use local Git for history; otherwise keep one current tree and record hashes/configs/evidence.
"#
}

fn project_memory_dir(config: &ProjectRoomConfig) -> PathBuf {
    PathBuf::from(&config.local_root).join(PROJECT_MEMORY_DIR)
}

fn ensure_project_memory(config: &ProjectRoomConfig) -> Result<(), String> {
    let dir = project_memory_dir(config);
    fs::create_dir_all(&dir).map_err(|error| format!("创建 {} 失败: {}", dir.display(), error))?;

    let state_path = dir.join(PROJECT_STATE_FILE);
    if !state_path.exists() {
        let content = format!(
            "# {} — Project State\n\nLast updated: {}\n\n## Goal\n{}\n\n## Current Method\n\n## Current Evidence\n\n## Known Problems\n\n## Next Actions\n",
            config.name,
            local_now_rfc3339(),
            default_research_goal(&config.id)
        );
        fs::write(&state_path, content)
            .map_err(|error| format!("写入 {} 失败: {}", state_path.display(), error))?;
    }

    for (file_name, content) in [
        (
            SESSION_HANDOFF_FILE,
            format!(
                "# Session Handoff\n\nUpdated: {}\n\nRead PROJECT_STATE.md first.\n",
                local_now_rfc3339()
            ),
        ),
        (
            DECISIONS_FILE,
            "# Durable Decisions\n\nNo durable decisions recorded yet.\n".to_string(),
        ),
        (
            MEMORY_EXPERIMENTS_FILE,
            "# Verified Experiments / Engineering Evidence\n\nNo experiments recorded yet.\n"
                .to_string(),
        ),
        (MEMORY_PROTOCOL_FILE, default_memory_protocol().to_string()),
    ] {
        let path = dir.join(file_name);
        if !path.exists() {
            fs::write(&path, content)
                .map_err(|error| format!("写入 {} 失败: {}", path.display(), error))?;
        }
    }

    Ok(())
}

fn read_memory_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("读取 {} 失败: {}", path.display(), error))
}

fn project_memory_updated_at(dir: &Path) -> String {
    let mut newest = None;
    for file_name in [
        PROJECT_STATE_FILE,
        SESSION_HANDOFF_FILE,
        DECISIONS_FILE,
        MEMORY_EXPERIMENTS_FILE,
        MEMORY_PROTOCOL_FILE,
    ] {
        if let Ok(modified) = fs::metadata(dir.join(file_name)).and_then(|meta| meta.modified()) {
            if newest.map(|current| modified > current).unwrap_or(true) {
                newest = Some(modified);
            }
        }
    }

    newest
        .map(|time| {
            let dt: chrono::DateTime<chrono::Local> = time.into();
            dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
        })
        .unwrap_or_else(local_now_rfc3339)
}

fn load_project_memory(config: &ProjectRoomConfig) -> Result<ProjectMemory, String> {
    ensure_project_memory(config)?;
    let dir = project_memory_dir(config);
    Ok(ProjectMemory {
        project_state: read_memory_text(&dir.join(PROJECT_STATE_FILE))?,
        session_handoff: read_memory_text(&dir.join(SESSION_HANDOFF_FILE))?,
        decisions: read_memory_text(&dir.join(DECISIONS_FILE))?,
        experiments: read_memory_text(&dir.join(MEMORY_EXPERIMENTS_FILE))?,
        memory_protocol: read_memory_text(&dir.join(MEMORY_PROTOCOL_FILE))?,
        updated_at: project_memory_updated_at(&dir),
    })
}

fn save_project_memory(config: &ProjectRoomConfig, memory: &ProjectMemory) -> Result<(), String> {
    ensure_project_memory(config)?;
    let dir = project_memory_dir(config);
    for (file_name, content) in [
        (PROJECT_STATE_FILE, &memory.project_state),
        (SESSION_HANDOFF_FILE, &memory.session_handoff),
        (DECISIONS_FILE, &memory.decisions),
        (MEMORY_EXPERIMENTS_FILE, &memory.experiments),
        (MEMORY_PROTOCOL_FILE, &memory.memory_protocol),
    ] {
        fs::write(dir.join(file_name), content)
            .map_err(|error| format!("写入 project memory 失败: {}", error))?;
    }
    Ok(())
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

fn project_constitution() -> &'static str {
    r#"# TunnelDock Research Project Constitution

This project is optimized for top-conference research, not product feature accumulation.

## Research rules
- Every code change, experiment, visualization, and document must serve a research hypothesis, method, ablation, analysis, or reproducibility goal.
- Keep one authoritative current implementation. Do not create V1/V2/V3, *_old, *_backup, *_new, *_final, or parallel stale implementations.
- Respect the project's own Git policy. When Git is permitted, use local Git for history/review/rollback; when it is not, keep one current tree and record source hashes/configs/evidence. Never preserve old code copies just to simulate version control.
- Keep core logic simple, explicit, and testable. Correctness is more important than speed of implementation.
- Remove obsolete files, temporary patches, stale logs, abandoned scripts, and outdated documentation after their replacement is verified.
- Record meaningful experiments, including negative results. Code completion alone does not finish a research task; evidence and interpretation must enter project memory.
- Project memory is a living current-state document. Update it when method decisions, evidence, known problems, or next actions change.

## Default agent roles
- ChatGPT: coordinator, research lead, experiment interpreter, final reviewer, and quota-allocation brain.
- Codex: primary engineer for robust implementation, refactoring, tests, and code correctness.
- Antigravity / Gemini: visualization, UI, PCA/video/figure work, rapid exploration, and visual result inspection.

## Review rules
- Gemini modifications to core model/training/data code require Codex review before acceptance.
- Important Codex algorithm changes require ChatGPT review for research intent and methodological consistency.
- Agents may discuss and challenge each other. Disagreement should be preserved in Project Room discussion until a decision is made.
- The human researcher remains the final decision maker.

## Project isolation
- Do not import task state, memory, or decisions from another Project Room unless the user explicitly requests a cross-project handoff.
"#
}

fn inbox_protocol() -> &'static str {
    r#"# TunnelDock Project Room Inbox

Agents collaborate through this project-local inbox. TunnelDock consumes each JSON file once, applies it to the Project Room, updates `project_room.json`, then deletes the processed file.

Write one event per `.json` file under `.tunneldock/inbox/`.

## discussion.post
```json
{
  "kind": "discussion.post",
  "author": "codex",
  "thread_id": "general",
  "recipients": ["chatgpt", "gemini"],
  "message": "Concrete finding, disagreement, question, or review request."
}
```

## task.create
```json
{
  "kind": "task.create",
  "author": "chatgpt",
  "title": "Implement decoder visibility head",
  "goal": "Completion criteria and research reason.",
  "owner": "codex",
  "reviewers": ["chatgpt"],
  "write_scope": ["model/decoder/**"]
}
```

## task.handoff
```json
{
  "kind": "task.handoff",
  "author": "codex",
  "task_id": "TASK-...",
  "status": "review",
  "summary": "What changed, tests/evidence, remaining risks, and requested review."
}
```

## experiment.record
```json
{
  "kind": "experiment.record",
  "author": "chatgpt",
  "hypothesis": "What this run tests.",
  "dataset": "benchmark/split",
  "command": "exact reproducible command",
  "metrics": "measured results only",
  "result": "supported / rejected / inconclusive",
  "analysis": "interpretation separated from measurements",
  "artifacts": ["path/to/result.json"]
}
```

Rules:
- Do not edit `project_room.json` directly; it is generated state.
- Do not use this inbox as a raw chat dump. Post only decisions, disagreements, review requests, handoffs, and reproducible evidence that another agent needs.
- Canonical research memory remains under `.project_memory/`; update it directly after meaningful decisions/results.
- Project isolation is strict. Never write an event into another project's inbox unless the user explicitly requests a cross-project handoff.
"#
}

fn sync_project_bridge(snapshot: &ProjectRoomSnapshot) -> Result<(), String> {
    let bridge_dir = PathBuf::from(&snapshot.config.local_root).join(BRIDGE_DIR);
    fs::create_dir_all(&bridge_dir)
        .map_err(|error| format!("创建 {} 失败: {}", bridge_dir.display(), error))?;
    fs::create_dir_all(bridge_dir.join(INBOX_DIR))
        .map_err(|error| format!("创建 Project Room inbox 失败: {}", error))?;

    // The bridge is operational coordination state only. Canonical research memory
    // remains exclusively in .project_memory/*.md so agents never mistake a copied
    // JSON snapshot for an authoritative second memory source.
    let memory_dir = project_memory_dir(&snapshot.config);
    let bridge = serde_json::json!({
        "config": &snapshot.config,
        "memory": {
            "updated_at": &snapshot.memory.updated_at,
            "project_state": memory_dir.join(PROJECT_STATE_FILE),
            "session_handoff": memory_dir.join(SESSION_HANDOFF_FILE),
            "decisions": memory_dir.join(DECISIONS_FILE),
            "experiments": memory_dir.join(MEMORY_EXPERIMENTS_FILE),
            "protocol": memory_dir.join(MEMORY_PROTOCOL_FILE),
        },
        "agents": &snapshot.agents,
        "capacities": &snapshot.capacities,
        "tasks": &snapshot.tasks,
        "experiments": &snapshot.experiments,
        "discussion": &snapshot.discussion,
        "runs": &snapshot.runs,
    });
    write_json(&bridge_dir.join(BRIDGE_FILE), &bridge)?;
    fs::write(bridge_dir.join(CONSTITUTION_FILE), project_constitution())
        .map_err(|error| format!("写入 Project Constitution 失败: {}", error))?;
    fs::write(bridge_dir.join(INBOX_PROTOCOL_FILE), inbox_protocol())
        .map_err(|error| format!("写入 Project Room Inbox Protocol 失败: {}", error))?;
    Ok(())
}

fn event_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn event_string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn process_project_event_unlocked(
    state: &AppState,
    project_id: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    let kind = event_string(value, "kind");
    let author = {
        let author = event_string(value, "author");
        if author.is_empty() {
            "agent".to_string()
        } else {
            author
        }
    };
    let dir = project_dir(state, project_id)?;
    let now = local_now_rfc3339();

    match kind.as_str() {
        "discussion.post" => {
            let message = event_string(value, "message");
            if message.is_empty() {
                return Err("discussion.post 缺少 message".to_string());
            }

            let recipients = {
                let items = event_string_array(value, "recipients");
                if items.is_empty() {
                    vec!["all".to_string()]
                } else {
                    items
                }
            };
            let thread_id = {
                let thread = event_string(value, "thread_id");
                if thread.is_empty() {
                    "general".to_string()
                } else {
                    thread
                }
            };

            let path = dir.join(DISCUSSION_FILE);
            let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&path)?;
            messages.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id,
                    author,
                    recipients,
                    message,
                    created_at: now,
                },
            );
            if messages.len() > 5_000 {
                messages.truncate(5_000);
            }
            write_json(&path, &messages)?;
        }
        "task.create" => {
            let title = event_string(value, "title");
            if title.is_empty() {
                return Err("task.create 缺少 title".to_string());
            }
            let owner = {
                let owner = event_string(value, "owner");
                if owner.is_empty() {
                    "codex".to_string()
                } else {
                    owner
                }
            };
            let reviewers = {
                let explicit = event_string_array(value, "reviewers");
                if !explicit.is_empty() {
                    explicit
                } else if owner == "gemini" {
                    vec!["codex".to_string(), "chatgpt".to_string()]
                } else {
                    vec!["chatgpt".to_string()]
                }
            };

            let path = dir.join(TASKS_FILE);
            let mut tasks = read_json::<Vec<ProjectTask>>(&path)?;
            tasks.insert(
                0,
                ProjectTask {
                    id: next_id("TASK"),
                    title,
                    goal: event_string(value, "goal"),
                    owner,
                    reviewers,
                    status: "backlog".to_string(),
                    write_scope: event_string_array(value, "write_scope"),
                    summary: String::new(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            );
            write_json(&path, &tasks)?;
        }
        "task.handoff" => {
            let task_id = event_string(value, "task_id");
            let summary = event_string(value, "summary");
            if task_id.is_empty() || summary.is_empty() {
                return Err("task.handoff 需要 task_id 和 summary".to_string());
            }

            let status = {
                let status = event_string(value, "status");
                if status.is_empty() {
                    "review".to_string()
                } else {
                    status
                }
            };
            let tasks_path = dir.join(TASKS_FILE);
            let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
            let task = tasks
                .iter_mut()
                .find(|task| task.id == task_id)
                .ok_or_else(|| format!("task.handoff 找不到 {}", task_id))?;
            task.status = status;
            task.summary = summary.clone();
            task.updated_at = now.clone();
            let recipients = if task.reviewers.is_empty() {
                vec!["chatgpt".to_string()]
            } else {
                task.reviewers.clone()
            };
            write_json(&tasks_path, &tasks)?;

            let discussion_path = dir.join(DISCUSSION_FILE);
            let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&discussion_path)?;
            messages.insert(
                0,
                ProjectDiscussionMessage {
                    id: next_id("MSG"),
                    thread_id: task_id.clone(),
                    author,
                    recipients,
                    message: format!("Handoff for {}:\n\n{}", task_id, summary),
                    created_at: now,
                },
            );
            if messages.len() > 5_000 {
                messages.truncate(5_000);
            }
            write_json(&discussion_path, &messages)?;
        }
        "experiment.record" => {
            let hypothesis = event_string(value, "hypothesis");
            if hypothesis.is_empty() {
                return Err("experiment.record 缺少 hypothesis".to_string());
            }

            let path = dir.join(EXPERIMENTS_FILE);
            let mut experiments = read_json::<Vec<ProjectExperiment>>(&path)?;
            experiments.insert(
                0,
                ProjectExperiment {
                    id: next_id("EXP"),
                    hypothesis,
                    code_revision: event_string(value, "code_revision"),
                    command: event_string(value, "command"),
                    config: event_string(value, "config"),
                    dataset: event_string(value, "dataset"),
                    metrics: event_string(value, "metrics"),
                    result: event_string(value, "result"),
                    analysis: event_string(value, "analysis"),
                    artifacts: event_string_array(value, "artifacts"),
                    status: {
                        let status = event_string(value, "status");
                        if status.is_empty() {
                            "completed".to_string()
                        } else {
                            status
                        }
                    },
                    created_at: now.clone(),
                    updated_at: now,
                },
            );
            write_json(&path, &experiments)?;
        }
        "" => return Err("inbox event 缺少 kind".to_string()),
        _ => return Err(format!("不支持的 Project Room inbox kind: {}", kind)),
    }

    Ok(())
}

fn process_project_inbox_unlocked(state: &AppState, project_id: &str) -> Result<usize, String> {
    let snapshot = load_snapshot_unlocked(state, project_id)?;
    let bridge_dir = PathBuf::from(&snapshot.config.local_root).join(BRIDGE_DIR);
    let inbox = bridge_dir.join(INBOX_DIR);
    fs::create_dir_all(&inbox)
        .map_err(|error| format!("创建 {} 失败: {}", inbox.display(), error))?;

    let mut files = fs::read_dir(&inbox)
        .map_err(|error| format!("读取 {} 失败: {}", inbox.display(), error))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    files.sort();

    let mut processed = 0usize;
    for path in files {
        // An agent may still be writing a file. Only ingest files that have been
        // stable for at least one second.
        if let Ok(modified) = fs::metadata(&path).and_then(|meta| meta.modified()) {
            if modified
                .elapsed()
                .map(|elapsed| elapsed < std::time::Duration::from_secs(1))
                .unwrap_or(false)
            {
                continue;
            }
        }

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                let _ = write_json(
                    &bridge_dir.join(INBOX_ERROR_FILE),
                    &serde_json::json!({
                        "file": path.to_string_lossy(),
                        "error": error.to_string(),
                        "updated_at": local_now_rfc3339(),
                    }),
                );
                let _ = fs::remove_file(&path);
                continue;
            }
        };
        let value = match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(value) => value,
            Err(error) => {
                let _ = write_json(
                    &bridge_dir.join(INBOX_ERROR_FILE),
                    &serde_json::json!({
                        "file": path.to_string_lossy(),
                        "error": format!("invalid JSON: {}", error),
                        "updated_at": local_now_rfc3339(),
                    }),
                );
                let _ = fs::remove_file(&path);
                continue;
            }
        };

        match process_project_event_unlocked(state, project_id, &value) {
            Ok(()) => {
                processed += 1;
                let _ = fs::remove_file(&path);
                let error_path = bridge_dir.join(INBOX_ERROR_FILE);
                if error_path.exists() {
                    let _ = fs::remove_file(error_path);
                }
            }
            Err(error) => {
                let _ = write_json(
                    &bridge_dir.join(INBOX_ERROR_FILE),
                    &serde_json::json!({
                        "file": path.to_string_lossy(),
                        "error": &error,
                        "updated_at": local_now_rfc3339(),
                    }),
                );
                let _ = append_system_message(
                    &project_dir(state, project_id)?,
                    "system",
                    format!("Rejected inbox event {}: {}", path.display(), error),
                );
                let _ = fs::remove_file(&path);
            }
        }
    }

    if processed > 0 {
        let snapshot = load_snapshot_unlocked(state, project_id)?;
        sync_project_bridge(&snapshot)?;
    }

    Ok(processed)
}

fn reconcile_project_operational_state_once(state: &Arc<AppState>) -> Result<(), String> {
    let _guard = state.project_store_lock.lock();
    let ids = ensure_store(state)?;

    for project_id in ids {
        let _ = refresh_run_states_unlocked(state, &project_id);
        let _ = process_project_inbox_unlocked(state, &project_id);
        if let Ok(snapshot) = load_snapshot_unlocked(state, &project_id) {
            let _ = sync_project_bridge(&snapshot);
        }
    }

    Ok(())
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

fn next_id(prefix: &str) -> String {
    format!(
        "{}-{}",
        prefix,
        chrono::Local::now()
            .timestamp_nanos_opt()
            .unwrap_or_default()
    )
}

fn normalize_scope(scope: &str) -> String {
    scope
        .trim()
        .replace('\\', "/")
        .trim_end_matches("/**")
        .trim_end_matches("/*")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn scopes_conflict(left: &[String], right: &[String]) -> bool {
    if left.is_empty() || right.is_empty() {
        return true;
    }

    left.iter().any(|a| {
        let a = normalize_scope(a);
        right.iter().any(|b| {
            let b = normalize_scope(b);
            a.is_empty()
                || b.is_empty()
                || a == b
                || a.starts_with(&(b.clone() + "/"))
                || b.starts_with(&(a.clone() + "/"))
        })
    })
}

fn run_dir(config: &ProjectRoomConfig, run_id: &str) -> PathBuf {
    PathBuf::from(&config.local_root)
        .join(BRIDGE_DIR)
        .join(RUNS_DIR)
        .join(run_id)
}

fn task_prompt(
    snapshot: &ProjectRoomSnapshot,
    task: &ProjectTask,
    agent_id: &str,
    output_path: &Path,
) -> String {
    let role = snapshot
        .agents
        .iter()
        .find(|agent| agent.agent_id == agent_id)
        .map(|agent| agent.role.as_str())
        .unwrap_or("Research worker");
    let reviewers = if task.reviewers.is_empty() {
        "ChatGPT".to_string()
    } else {
        task.reviewers.join(", ")
    };
    let write_scope = if task.write_scope.is_empty() {
        "No narrow write scope was declared. Minimize edits and do not touch unrelated files."
            .to_string()
    } else {
        format!(
            "Only modify these declared scopes: {}",
            task.write_scope.join(", ")
        )
    };
    let handoff_delivery = if agent_id == "gemini" {
        format!(
            "Before finishing, write the same handoff text to this exact file: {}",
            output_path.display()
        )
    } else {
        format!(
            "TunnelDock will persist your final response to: {}",
            output_path.display()
        )
    };

    format!(
        r#"You are {agent} working inside TunnelDock Project Room "{project}".

Role: {role}
Task ID: {task_id}
Task: {title}
Goal / completion criteria:
{goal}

Project boundaries:
- Local root: {local}
- Remote workspace: {remote_host}:{remote_root}
- Project Room snapshot: {local}\.tunneldock\project_room.json
- Constitution: {local}\.tunneldock\CONSTITUTION.md
- Canonical memory: {local}\.project_memory\PROJECT_STATE.md and SESSION_HANDOFF.md
- {write_scope}

Research rules:
- This is top-conference research. Prefer the simplest correct implementation that tests the hypothesis.
- Do not create V1/V2/V3, *_old, *_backup, *_new, or *_final copies.
- Do not leave temporary scripts/logs after their conclusion is captured.
- Preserve reproducibility evidence and meaningful negative results.
- Never import assumptions from another Project Room.
- Reviewer(s): {reviewers}

Before editing, read the Project Room snapshot, Constitution, PROJECT_STATE.md, SESSION_HANDOFF.md, and relevant project instructions (AGENTS.md/README).
If the task is underspecified or conflicts with current evidence, state the conflict instead of inventing a design.
After implementation, run the smallest sufficient correctness checks.
Your final response must be a concise handoff with:
1. Summary
2. Files changed
3. Tests / experiment evidence
4. Remaining risks
5. Questions or requested review
6. Memory/experiment updates that should be made

{handoff_delivery}
"#,
        agent = agent_id,
        project = snapshot.config.name,
        role = role,
        task_id = task.id,
        title = task.title,
        goal = task.goal,
        local = snapshot.config.local_root,
        remote_host = snapshot.config.remote.host,
        remote_root = snapshot.config.remote.root,
        write_scope = write_scope,
        reviewers = reviewers,
        handoff_delivery = handoff_delivery,
    )
}

fn build_worker_command(
    executable: &Path,
    args: &[String],
    cwd: &Path,
    stdout: File,
    stderr: File,
) -> Command {
    #[cfg(target_os = "windows")]
    let mut command = {
        let extension = executable
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if matches!(extension.as_str(), "cmd" | "bat") {
            let mut command = Command::new("cmd.exe");
            command.args(["/d", "/s", "/c"]);
            command.arg(executable);
            command.args(args);
            command
        } else {
            let mut command = Command::new(executable);
            command.args(args);
            command
        }
    };

    #[cfg(not(target_os = "windows"))]
    let mut command = {
        let mut command = Command::new(executable);
        command.args(args);
        command
    };

    command.current_dir(cwd).stdout(stdout).stderr(stderr);

    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
}

fn append_system_message(
    dir: &Path,
    author: &str,
    message: String,
) -> Result<ProjectDiscussionMessage, String> {
    let path = dir.join(DISCUSSION_FILE);
    let mut messages = read_json::<Vec<ProjectDiscussionMessage>>(&path)?;
    let item = ProjectDiscussionMessage {
        id: next_id("MSG"),
        thread_id: "agent-runs".to_string(),
        author: author.to_string(),
        recipients: vec!["all".to_string()],
        message,
        created_at: local_now_rfc3339(),
    };
    messages.insert(0, item.clone());
    if messages.len() > 5_000 {
        messages.truncate(5_000);
    }
    write_json(&path, &messages)?;
    Ok(item)
}

fn refresh_run_states_unlocked(state: &AppState, project_id: &str) -> Result<bool, String> {
    let dir = project_dir(state, project_id)?;
    let runs_path = dir.join(RUNS_FILE);
    let tasks_path = dir.join(TASKS_FILE);
    let mut runs = read_json::<Vec<AgentRun>>(&runs_path)?;
    let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
    let mut changed = false;

    for run in &mut runs {
        if !matches!(run.status.as_str(), "running" | "interactive") {
            continue;
        }

        let output_path = PathBuf::from(&run.output_path);
        let output = fs::read_to_string(&output_path)
            .ok()
            .filter(|text| !text.trim().is_empty());

        let process_alive = run.pid.map(is_process_running).unwrap_or(false);
        let completed = output.is_some();
        let terminal_without_output = run.status == "running" && !process_alive && output.is_none();

        if completed || terminal_without_output {
            run.finished_at = Some(local_now_rfc3339());
            run.pid = None;
            state.running_agent_pids.lock().remove(&run.id);

            if let Some(text) = output {
                run.status = "completed".to_string();
                let handoff = text.chars().take(12_000).collect::<String>();
                if let Some(task) = tasks.iter_mut().find(|task| task.id == run.task_id) {
                    task.status = "review".to_string();
                    task.summary = handoff.clone();
                    task.updated_at = local_now_rfc3339();
                }
                let _ = append_system_message(
                    &dir,
                    &run.agent_id,
                    format!("Agent handoff for {}:\n\n{}", run.task_id, handoff),
                );
            } else {
                run.status = "failed".to_string();
                let error_text = fs::read_to_string(&run.error_path)
                    .unwrap_or_else(|_| "Worker exited without a handoff.".to_string());
                let error = error_text.chars().take(4_000).collect::<String>();
                run.error_message = Some(error.clone());
                if let Some(task) = tasks.iter_mut().find(|task| task.id == run.task_id) {
                    task.status = "blocked".to_string();
                    task.summary = error.clone();
                    task.updated_at = local_now_rfc3339();
                }
                let _ =
                    append_system_message(&dir, "system", format!("{} failed: {}", run.id, error));
            }
            changed = true;
        }
    }

    if changed {
        write_json(&runs_path, &runs)?;
        write_json(&tasks_path, &tasks)?;
    }

    Ok(changed)
}

fn ensure_dispatch_scope_is_safe(
    snapshot: &ProjectRoomSnapshot,
    task: &ProjectTask,
) -> Result<(), String> {
    for other in &snapshot.tasks {
        if other.id == task.id || !matches!(other.status.as_str(), "active" | "review") {
            continue;
        }

        if scopes_conflict(&task.write_scope, &other.write_scope) {
            return Err(format!(
                "Task {} 与正在进行/待 review 的 {} 写入范围可能冲突；请先完成 review 或明确拆分 write_scope。",
                task.id, other.id
            ));
        }
    }

    Ok(())
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

#[tauri::command]
pub fn dispatch_project_task(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    task_id: String,
    agent_id: String,
) -> Result<AgentRun, String> {
    let _guard = state.project_store_lock.lock();
    ensure_store(&state)?;
    refresh_run_states_unlocked(&state, &project_id)?;

    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    let task = snapshot
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .ok_or_else(|| format!("找不到 Task: {}", task_id))?;

    if !matches!(agent_id.as_str(), "codex" | "gemini") {
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
    let prompt = task_prompt(&snapshot, &task, &agent_id, &output_path);
    fs::write(&prompt_path, &prompt)
        .map_err(|error| format!("写入 {} 失败: {}", prompt_path.display(), error))?;

    let executable = match agent_id.as_str() {
        "codex" => find_codex_executable().ok_or_else(|| "未找到 Codex CLI".to_string())?,
        "gemini" => {
            find_antigravity_executable().ok_or_else(|| "未找到 Antigravity CLI".to_string())?
        }
        _ => unreachable!(),
    };

    let stdout = File::create(&log_path)
        .map_err(|error| format!("创建 {} 失败: {}", log_path.display(), error))?;
    let stderr = File::create(&error_path)
        .map_err(|error| format!("创建 {} 失败: {}", error_path.display(), error))?;

    let args = if agent_id == "codex" {
        let mut args = vec![
            "exec".to_string(),
            "-C".to_string(),
            snapshot.config.local_root.clone(),
            "--sandbox".to_string(),
            "workspace-write".to_string(),
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
        args
    } else {
        vec![
            "chat".to_string(),
            "--mode".to_string(),
            "agent".to_string(),
            "--new-window".to_string(),
            "--add-file".to_string(),
            PathBuf::from(&snapshot.config.local_root)
                .join(BRIDGE_DIR)
                .join(BRIDGE_FILE)
                .to_string_lossy()
                .to_string(),
            "--add-file".to_string(),
            project_memory_dir(&snapshot.config)
                .join(PROJECT_STATE_FILE)
                .to_string_lossy()
                .to_string(),
            prompt.clone(),
        ]
    };

    let cwd = PathBuf::from(&snapshot.config.local_root);
    let mut command = build_worker_command(&executable, &args, &cwd, stdout, stderr);
    let child = command
        .spawn()
        .map_err(|error| format!("启动 {} worker 失败: {}", agent_id, error))?;
    let pid = child.id();

    let mut run = AgentRun {
        id: run_id.clone(),
        task_id: task.id.clone(),
        agent_id: agent_id.clone(),
        status: if agent_id == "gemini" {
            "interactive".to_string()
        } else {
            "running".to_string()
        },
        pid: Some(pid),
        started_at: local_now_rfc3339(),
        finished_at: None,
        prompt_path: prompt_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        log_path: log_path.to_string_lossy().to_string(),
        error_path: error_path.to_string_lossy().to_string(),
        error_message: None,
    };

    state.running_agent_pids.lock().insert(run_id.clone(), pid);

    let dir = project_dir(&state, &project_id)?;
    let runs_path = dir.join(RUNS_FILE);
    let mut runs = read_json::<Vec<AgentRun>>(&runs_path)?;
    runs.insert(0, run.clone());
    write_json(&runs_path, &runs)?;

    let tasks_path = dir.join(TASKS_FILE);
    let mut tasks = read_json::<Vec<ProjectTask>>(&tasks_path)?;
    if let Some(current) = tasks.iter_mut().find(|item| item.id == task.id) {
        current.owner = agent_id.clone();
        current.status = "active".to_string();
        current.updated_at = local_now_rfc3339();
    }
    write_json(&tasks_path, &tasks)?;

    let _ = append_system_message(
        &dir,
        "system",
        format!(
            "Dispatched {} to {} as {}. Handoff: {}",
            task.id, agent_id, run.id, run.output_path
        ),
    );

    let snapshot = load_snapshot_unlocked(&state, &project_id)?;
    sync_project_bridge(&snapshot)?;

    // Antigravity's launcher may exit after handing the chat to the GUI. Keep the
    // run interactive until the instructed HANDOFF.md appears.
    if agent_id == "gemini" {
        run.pid = None;
        state.running_agent_pids.lock().remove(&run_id);
        if let Some(saved) = runs.iter_mut().find(|item| item.id == run_id) {
            saved.pid = None;
        }
        write_json(&runs_path, &runs)?;
    }

    Ok(run)
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
3. Read '{local}\.tunneldock\project_room.json' and '{local}\.tunneldock\INBOX_PROTOCOL.md'.
4. Read the canonical durable project memory:
   - '{local}\.project_memory\PROJECT_STATE.md'
   - '{local}\.project_memory\SESSION_HANDOFF.md'
   - '{local}\.project_memory\DECISIONS.md'
   - '{local}\.project_memory\EXPERIMENTS.md'
   - '{local}\.project_memory\MEMORY_PROTOCOL.md'
5. Treat chat history as working memory only. The files above are the durable source of truth.
6. When Codex/Gemini/ChatGPT need to exchange a durable question, disagreement, task, handoff, or experiment result, write one JSON event to '{local}\.tunneldock\inbox\' using INBOX_PROTOCOL.md. Never edit project_room.json directly.

Research operating rules:
- The goal is top-conference research. Keep code and algorithms simple, explicit, and correct.
- Do not create V1/V2/V3, *_old, *_backup, *_new, or *_final copies. Follow the project's own Git policy; if Git is disallowed, keep one current tree and record hashes/configs/evidence instead.
- Remove obsolete files, temporary logs, superseded scripts, and stale documentation after replacement is verified.
- Record meaningful experiments, including negative results.
- Refine project memory after meaningful decisions/results instead of appending raw chat logs.
- ChatGPT is the coordinator and quota-allocation brain.
- Codex is the primary robust engineer.
- Antigravity/Gemini is the visual/exploration agent; core-code changes require Codex review.
- Important Codex algorithm changes require ChatGPT methodological review.
- You and the other agents may discuss and challenge each other; I remain the final decision maker.

When allocating work, first inspect current tasks, agent capacity telemetry, experiments, and discussion in project_room.json, then explain the allocation before asking workers to execute."#,
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
3. 阅读 '{local}\.tunneldock\project_room.json' 和 '{local}\.tunneldock\INBOX_PROTOCOL.md'。
4. 阅读项目唯一持久化科研记忆：
   - '{local}\.project_memory\PROJECT_STATE.md'
   - '{local}\.project_memory\SESSION_HANDOFF.md'
   - '{local}\.project_memory\DECISIONS.md'
   - '{local}\.project_memory\EXPERIMENTS.md'
   - '{local}\.project_memory\MEMORY_PROTOCOL.md'
5. 对话历史只作为工作记忆；上述文件才是持久化 source of truth。
6. ChatGPT / Codex / Gemini 需要跨 Agent 留下问题、分歧、任务、handoff 或实验结果时，按 INBOX_PROTOCOL.md 向 '{local}\.tunneldock\inbox\' 写入单个 JSON event；不得直接修改 project_room.json。

科研工作硬规则：
- 目标是顶会论文；代码和算法表达必须简洁、逻辑清晰、可验证、正确。
- 不创建 V1/V2/V3、*_old、*_backup、*_new、*_final 等平行旧版本；遵守项目自己的 Git 规则。允许 Git 时用本地 Git 保留历史；禁止 Git 时只保留当前代码树，并记录 source hash / config / evidence。
- 替代实现验证后清理过时代码、临时日志、废弃脚本和失效文档。
- 有意义的实验都要记录，包括 negative result。
- 重要决策/结果发生后要提炼更新项目记忆，而不是无限追加聊天记录。
- ChatGPT 负责科研规划、任务拆解、额度综合调配、实验解释和最终 review。
- Codex 是稳健实现的主工程 Agent。
- Antigravity/Gemini 负责视觉、PCA/图表/视频/UI 和快速探索；涉及核心代码默认需要 Codex review。
- Codex 的重要算法改动需要 ChatGPT 做科研意图和方法一致性 review。
- 三个 Agent 可以互相讨论、质疑和反驳；我始终是最终研究决策者。

每次调度前，先从 project_room.json 查看当前 tasks、agent capacity、experiments 和 discussion，再说明为什么这样分配额度与任务，然后再让 worker 执行。"#,
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
        parse_antigravity_csrf_token, parse_antigravity_quota_summary, parse_codex_rate_limit,
        scopes_conflict, stale_name_reason, ProjectRemote, ProjectRoomConfig,
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
                                "displayName": "Weekly Limit Remaining",
                                "remainingFraction": 0.19,
                                "resetTime": "2026-10-02T01:15:23Z"
                            },
                            {
                                "displayName": "Five Hour Limit Remaining",
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

        let (remaining, reset, group) =
            parse_antigravity_quota_summary(&value).expect("Gemini quota should parse");
        assert!((remaining - 19.0).abs() < f64::EPSILON);
        assert_eq!(reset.as_deref(), Some("2026-10-02T01:15:23Z"));
        assert_eq!(group.as_deref(), Some("Gemini Models"));
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
