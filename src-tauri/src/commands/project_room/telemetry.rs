use super::*;
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;
use std::time::Duration;

fn command_with_args(executable: &Path, args: &[&str]) -> Command {
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

    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
}

pub(super) fn command_version(executable: &Path, args: &[&str]) -> Option<String> {
    let mut command = command_with_args(executable, args);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

pub(super) fn local_json_post(
    url: &str,
    csrf_token: &str,
    body: &serde_json::Value,
    timeout_secs: u64,
) -> Result<serde_json::Value, String> {
    let body_text = serde_json::to_string(body)
        .map_err(|error| format!("序列化本地 RPC JSON 失败: {error}"))?;
    let timeout = timeout_secs.max(1).to_string();
    let csrf_header = format!("x-codeium-csrf-token: {csrf_token}");
    #[cfg(target_os = "windows")]
    let mut command = Command::new("curl.exe");
    #[cfg(not(target_os = "windows"))]
    let mut command = Command::new("curl");
    command.args([
        "-k",
        "-sS",
        "--max-time",
        &timeout,
        "-H",
        &csrf_header,
        "-H",
        "content-type: application/json",
        "--data-binary",
        "@-",
        "-w",
        "\n%{http_code}",
        url,
    ]);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = command
        .spawn()
        .map_err(|error| format!("启动 curl.exe 失败: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(body_text.as_bytes())
            .map_err(|error| format!("写入 curl.exe 请求体失败: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("等待 curl.exe 失败: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "本地 RPC curl 请求失败".to_string()
        } else {
            stderr
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let (response_text, status_text) = stdout
        .rsplit_once('\n')
        .ok_or_else(|| "本地 RPC curl 响应缺少 HTTP 状态码".to_string())?;
    let status = status_text
        .trim()
        .parse::<u16>()
        .map_err(|error| format!("解析本地 RPC HTTP 状态码失败: {error}"))?;
    if !(200..300).contains(&status) {
        return Err(format!(
            "本地 RPC 返回 HTTP {status}: {}",
            response_text.chars().take(1_000).collect::<String>()
        ));
    }
    if response_text.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(response_text)
        .map_err(|error| format!("解析本地 RPC JSON 响应失败: {error}"))
}

pub(super) fn find_codex_executable() -> Option<PathBuf> {
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

pub(super) fn find_antigravity_executable() -> Option<PathBuf> {
    // The research worker is the installed Antigravity Agent / Agent Manager
    // referenced by the user's Start Menu shortcut. Do not fall back to a
    // portable IDE/CLI copy from PATH or D:\Antigravity.
    #[cfg(target_os = "windows")]
    {
        let local = std::env::var_os("LOCALAPPDATA")?;
        let agent = PathBuf::from(local)
            .join("Programs")
            .join("antigravity")
            .join("Antigravity.exe");
        agent.exists().then_some(agent)
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
pub(super) fn antigravity_language_server_runtime() -> Option<(u32, Option<String>)> {
    let script = r#"$p=Get-CimInstance Win32_Process | Where-Object {$_.Name -eq 'language_server.exe' -and $_.ExecutablePath -like '*\Programs\antigravity\resources\bin\*'} | Select-Object -First 1; if($p){ Write-Output ($p.ProcessId.ToString() + '|' + $p.CommandLine) }"#;
    let output = execute_cmd("powershell", &["-NoProfile", "-Command", script], None);
    if !output.success {
        return None;
    }

    let line = output
        .stdout
        .lines()
        .find(|line| !line.trim().is_empty())?
        .trim();
    let (pid_text, command_line) = line.split_once('|')?;
    let pid = pid_text.trim().parse::<u32>().ok()?;
    let parts = command_line.split_whitespace().collect::<Vec<_>>();
    let csrf_token = parts
        .windows(2)
        .find(|pair| pair[0] == "--csrf_token")
        .map(|pair| pair[1].to_string())
        .or_else(|| {
            parts.iter().find_map(|arg| {
                arg.strip_prefix("--csrf_token=")
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            })
        });
    Some((pid, csrf_token))
}

#[cfg(target_os = "windows")]
pub(super) fn listening_ports_for_pid(pid: u32) -> Vec<u16> {
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

type AntigravityQuotaSummary = (f64, Option<String>, Option<String>, Vec<AgentQuotaWindow>);

pub(super) fn parse_antigravity_quota_summary(
    value: &serde_json::Value,
) -> Option<AntigravityQuotaSummary> {
    let groups = value.get("response")?.get("groups")?.as_array()?;
    let group = groups.iter().find(|group| {
        group
            .get("displayName")
            .and_then(serde_json::Value::as_str)
            .map(|name| name.to_ascii_lowercase().contains("gemini"))
            .unwrap_or(false)
    })?;
    let buckets = group.get("buckets")?.as_array()?;

    let quota_windows = buckets
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
            Some(AgentQuotaWindow {
                id: bucket
                    .get("bucketId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("quota")
                    .to_string(),
                label: bucket
                    .get("displayName")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Quota")
                    .to_string(),
                window: bucket
                    .get("window")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                remaining_percent: (remaining * 100.0).clamp(0.0, 100.0),
                reset_at: bucket
                    .get("resetTime")
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned),
            })
        })
        .collect::<Vec<_>>();

    let limiting = quota_windows.iter().min_by(|left, right| {
        left.remaining_percent
            .partial_cmp(&right.remaining_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;

    Some((
        limiting.remaining_percent,
        limiting.reset_at.clone(),
        group
            .get("displayName")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        quota_windows,
    ))
}

pub(super) fn antigravity_quota_capacity() -> Option<AgentCapacity> {
    #[cfg(not(target_os = "windows"))]
    {
        return None;
    }

    #[cfg(target_os = "windows")]
    {
        let (pid, csrf_token) = antigravity_language_server_runtime()?;
        let csrf_token = csrf_token?;
        let ports = listening_ports_for_pid(pid);
        if ports.is_empty() {
            return None;
        }

        let path = "/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary";
        for port in ports {
            for scheme in ["https", "http"] {
                let url = format!("{}://127.0.0.1:{}{}", scheme, port, path);
                let Ok(value) = local_json_post(
                    &url,
                    &csrf_token,
                    &serde_json::json!({ "forceRefresh": true }),
                    4,
                ) else {
                    continue;
                };
                let Some((remaining_percent, reset_at, model, quota_windows)) =
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
                    quota_windows,
                });
            }
        }

        None
    }
}

pub(super) fn collect_jsonl_files(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
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

pub(super) fn parse_codex_rate_limit(value: &serde_json::Value) -> Option<(f64, Option<i64>)> {
    let primary = value.get("payload")?.get("rate_limits")?.get("primary")?;
    let used_percent = primary.get("used_percent")?.as_f64()?;
    let resets_at = primary.get("resets_at").and_then(|value| value.as_i64());
    Some(((100.0 - used_percent).clamp(0.0, 100.0), resets_at))
}

pub(super) fn parse_codex_app_server_rate_limits(
    value: &serde_json::Value,
) -> Option<(f64, Option<i64>, Option<String>)> {
    let result = value.get("result")?;
    let rate_limits = result
        .get("rateLimitsByLimitId")
        .and_then(|limits| limits.get("codex"))
        .or_else(|| result.get("rateLimits"))?;
    let primary = rate_limits.get("primary")?;
    let used_percent = primary
        .get("usedPercent")
        .and_then(serde_json::Value::as_f64)?;
    let resets_at = primary.get("resetsAt").and_then(serde_json::Value::as_i64);
    let model = rate_limits
        .get("normalModelSlug")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            rate_limits
                .get("limitName")
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| {
            rate_limits
                .get("limitId")
                .and_then(serde_json::Value::as_str)
        })
        .map(ToOwned::to_owned);

    Some(((100.0 - used_percent).clamp(0.0, 100.0), resets_at, model))
}

pub(super) fn live_codex_capacity(executable: &Path) -> Option<AgentCapacity> {
    let mut command = command_with_args(executable, &["app-server", "--stdio"]);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = command.spawn().ok()?;
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };

    let initialize = serde_json::json!({
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "tunneldock",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "experimentalApi": true
            }
        }
    });
    let rate_limits = serde_json::json!({
        "id": 2,
        "method": "account/rateLimits/read",
        "params": {
            "excludeResetCreditDetails": true
        }
    });

    let request_written = writeln!(stdin, "{}", initialize)
        .and_then(|_| writeln!(stdin, "{}", rate_limits))
        .and_then(|_| stdin.flush())
        .is_ok();
    if !request_written {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if value.get("id").and_then(serde_json::Value::as_i64) == Some(2) {
                let _ = sender.send(value);
                break;
            }
        }
    });

    let response = receiver.recv_timeout(Duration::from_secs(5)).ok();
    let _ = child.kill();
    let _ = child.wait();

    let (remaining_percent, resets_at, model) =
        parse_codex_app_server_rate_limits(response.as_ref()?)?;

    Some(AgentCapacity {
        agent_id: "codex".to_string(),
        available: true,
        remaining_percent: Some(remaining_percent),
        reset_at: resets_at
            .and_then(|epoch| chrono::DateTime::from_timestamp(epoch, 0))
            .map(|utc| {
                utc.with_timezone(&chrono::Local)
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
            }),
        model,
        source: "codex_app_server_rate_limits".to_string(),
        confidence: "runtime_telemetry".to_string(),
        updated_at: local_now_rfc3339(),
        quota_windows: Vec::new(),
    })
}

pub(super) fn system_time_local_rfc3339(time: std::time::SystemTime) -> String {
    let local: chrono::DateTime<chrono::Local> = time.into();
    local.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

pub(super) fn latest_codex_capacity() -> AgentCapacity {
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
        quota_windows: Vec::new(),
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

pub(super) fn current_agent_capacities() -> Vec<AgentCapacity> {
    let now = local_now_rfc3339();
    let codex = find_codex_executable();
    let codex_capacity = codex
        .as_deref()
        .and_then(live_codex_capacity)
        .unwrap_or_else(latest_codex_capacity);
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
        quota_windows: Vec::new(),
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
            quota_windows: Vec::new(),
        },
        codex_capacity,
        gemini_capacity,
    ]
}

pub(super) fn agent_runtimes() -> Vec<AgentRuntimeInfo> {
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
            dispatch_mode: "shared_app_server_queue".to_string(),
            notes: "Codex 通过 shared app-server 的 queue 命令向 Project Room 已绑定的 Codex Desktop 长期对话投递任务，并从同一 thread 收集 handoff。"
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
            dispatch_mode: "cascade_rpc".to_string(),
            notes:
                "Antigravity Agent / Agent Manager 通过已绑定的 Cascade 对话和本地 Language Server RPC 接收任务；不使用便携 IDE/CLI。视觉/探索任务优先，核心代码需 Codex review。"
                    .to_string(),
        },
    ]
}
