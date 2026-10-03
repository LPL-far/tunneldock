use super::*;
use crate::utils::cmd::{execute_powershell, find_executable, kill_process_tree};
use rand::RngCore;
use std::fmt::Write as _;
#[cfg(not(target_os = "windows"))]
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom};
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::{header::AUTHORIZATION, HeaderValue};
use tungstenite::{connect, Message};

#[derive(Debug, Clone)]
pub(super) struct CodexBinding {
    pub rollout_path: PathBuf,
    pub start_offset: usize,
}

#[derive(Debug, Clone)]
pub(super) struct CodexDispatch {
    pub thread_id: String,
    pub start_offset: usize,
    pub pid: u32,
}

#[derive(Debug)]
pub(super) enum CodexRunUpdate {
    Running,
    Completed(String),
    Failed(String),
}

fn find_rollout_recursive(current: &Path, thread_id: &str, depth: usize) -> Option<PathBuf> {
    if depth > 6 {
        return None;
    }
    let entries = fs::read_dir(current).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_rollout_recursive(&path, thread_id, depth + 1) {
                return Some(found);
            }
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if name.ends_with(".jsonl") && name.contains(thread_id) {
            return Some(path);
        }
    }
    None
}

pub(super) fn resolve_thread(thread_id: &str) -> Result<CodexBinding, String> {
    let thread_id = thread_id.trim();
    if thread_id.is_empty() {
        return Err("Codex Thread ID 为空".to_string());
    }
    let sessions_root = dirs::home_dir()
        .ok_or_else(|| "无法定位用户目录".to_string())?
        .join(".codex")
        .join("sessions");
    let rollout_path = find_rollout_recursive(&sessions_root, thread_id, 0)
        .ok_or_else(|| format!("找不到已绑定的 Codex Desktop thread: {thread_id}"))?;
    let start_offset = fs::metadata(&rollout_path)
        .map_err(|error| format!("读取 {} 元数据失败: {error}", rollout_path.display()))?
        .len() as usize;
    Ok(CodexBinding {
        rollout_path,
        start_offset,
    })
}

fn free_loopback_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("为 Codex controller 分配端口失败: {error}"))?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|error| format!("读取 Codex controller 端口失败: {error}"))
}

fn rpc_request(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    next_id: &mut u64,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let id = *next_id;
    *next_id += 1;
    let request = serde_json::json!({
        "id": id,
        "method": method,
        "params": params,
    });
    socket
        .send(Message::Text(request.to_string().into()))
        .map_err(|error| format!("Codex RPC {method} send 失败: {error}"))?;

    loop {
        let message = socket
            .read()
            .map_err(|error| format!("Codex RPC {method} read 失败: {error}"))?;
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8(bytes.to_vec())
                .map_err(|error| format!("Codex RPC {method} binary UTF-8 失败: {error}"))?,
            Message::Ping(payload) => {
                let _ = socket.send(Message::Pong(payload));
                continue;
            }
            Message::Pong(_) => continue,
            Message::Close(frame) => {
                return Err(format!("Codex RPC {method} 连接被关闭: {frame:?}"));
            }
            _ => continue,
        };
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("Codex RPC {method} JSON 解析失败: {error}"))?;
        if value.get("id").and_then(serde_json::Value::as_u64) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| error.as_str().unwrap_or("unknown Codex RPC error"));
            return Err(format!("Codex RPC {method} 失败: {message}"));
        }
        return value
            .get("result")
            .cloned()
            .ok_or_else(|| format!("Codex RPC {method} 响应缺少 result"));
    }
}

fn capability_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

fn set_controller_io_timeout(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    timeout: Duration,
) -> Result<(), String> {
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|error| format!("设置 Codex controller read timeout 失败: {error}"))?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|error| format!("设置 Codex controller write timeout 失败: {error}"))?;
    }
    Ok(())
}

fn connect_controller(
    endpoint: &str,
    bearer_token: &str,
    timeout: Duration,
) -> Result<tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>, String>
{
    let deadline = Instant::now() + timeout;
    loop {
        let mut request = endpoint
            .into_client_request()
            .map_err(|error| format!("构造 Codex WebSocket 请求失败: {error}"))?;
        let authorization = HeaderValue::from_str(&format!("Bearer {bearer_token}"))
            .map_err(|error| format!("构造 Codex WebSocket Authorization 失败: {error}"))?;
        request.headers_mut().insert(AUTHORIZATION, authorization);
        match connect(request) {
            Ok((mut socket, _)) => {
                set_controller_io_timeout(&mut socket, Duration::from_secs(15))?;
                return Ok(socket);
            }
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                thread::sleep(Duration::from_millis(80));
            }
            Err(error) => {
                return Err(format!("连接 Codex background app-server 失败: {error}"));
            }
        }
    }
}

fn thread_from_result(result: &serde_json::Value) -> Result<&serde_json::Value, String> {
    result
        .get("thread")
        .ok_or_else(|| "Codex RPC 响应缺少 thread".to_string())
}

fn thread_id_and_path(result: &serde_json::Value) -> Result<(String, PathBuf), String> {
    let thread = thread_from_result(result)?;
    let thread_id = thread
        .get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Codex RPC thread 缺少 id".to_string())?
        .to_string();
    let path = thread
        .get("path")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "Codex RPC thread 缺少 rollout path".to_string())?;
    Ok((thread_id, path))
}

#[cfg(target_os = "windows")]
fn powershell_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(target_os = "windows")]
fn spawn_windows_durable_app_server(
    executable: &Path,
    endpoint: &str,
    token_path: &Path,
    log_path: &Path,
    error_path: &Path,
) -> Result<(u32, PathBuf), String> {
    let run_dir = log_path
        .parent()
        .ok_or_else(|| format!("Codex run log 缺少父目录: {}", log_path.display()))?;

    let node = find_executable("node");
    let (command_line, launcher_path) = if let Some(node) = node {
        let launcher_path = run_dir.join("codex_worker.js");
        let js_string =
            |value: &str| serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string());
        let args = vec![
            "app-server".to_string(),
            "--listen".to_string(),
            endpoint.to_string(),
            "--ws-auth".to_string(),
            "capability-token".to_string(),
            "--ws-token-file".to_string(),
            token_path.to_string_lossy().to_string(),
        ];
        let launcher = format!(
            "const fs=require(\"fs\");\nconst {{spawn}}=require(\"child_process\");\nconst errPath={};\nconst out=fs.openSync({},\"a\");\nconst err=fs.openSync(errPath,\"a\");\nfunction hostError(error){{try{{fs.appendFileSync(errPath,`[codex-host] ${{error?.stack||error}}\\n`);}}catch{{}}}}\nprocess.on(\"uncaughtException\",error=>{{hostError(error);process.exit(1);}});\nconst child=spawn({}, {}, {{stdio:[\"ignore\",out,err],windowsHide:true}});\nchild.on(\"error\",error=>{{hostError(error);process.exit(1);}});\nchild.on(\"exit\",code=>process.exit(code??0));\n",
            js_string(&error_path.to_string_lossy()),
            js_string(&log_path.to_string_lossy()),
            js_string(&executable.to_string_lossy()),
            serde_json::to_string(&args).unwrap_or_else(|_| "[]".to_string()),
        );
        fs::write(&launcher_path, launcher)
            .map_err(|error| format!("写入 {} 失败: {error}", launcher_path.display()))?;
        (
            format!("\"{}\" \"{}\"", node, launcher_path.display()),
            launcher_path,
        )
    } else {
        let launcher_path = run_dir.join("codex_worker.cmd");
        let launcher = format!(
            "@echo off\r\n\"{}\" app-server --listen \"{}\" --ws-auth capability-token --ws-token-file \"{}\" 1>>\"{}\" 2>>\"{}\"\r\nexit /b %ERRORLEVEL%\r\n",
            executable.display(),
            endpoint,
            token_path.display(),
            log_path.display(),
            error_path.display(),
        );
        fs::write(&launcher_path, launcher)
            .map_err(|error| format!("写入 {} 失败: {error}", launcher_path.display()))?;
        (
            format!("cmd.exe /d /s /c \"\"{}\"\"", launcher_path.display()),
            launcher_path,
        )
    };
    let script = format!(
        "$startup=([wmiclass]'Win32_ProcessStartup').CreateInstance(); $startup.ShowWindow=0; $r=([wmiclass]'Win32_Process').Create('{}',$null,$startup); if($r.ReturnValue -ne 0){{Write-Error ('Win32_Process.Create failed: '+$r.ReturnValue); exit 1}}; [Console]::Out.Write($r.ProcessId)",
        powershell_single_quote(&command_line)
    );
    let output = execute_powershell(&script, None);
    if !output.success {
        let _ = fs::remove_file(&launcher_path);
        let detail = if output.stderr.trim().is_empty() {
            output.stdout.trim().to_string()
        } else {
            output.stderr.trim().to_string()
        };
        return Err(format!("WMI 启动 Codex durable app-server 失败: {detail}"));
    }
    let pid = output
        .stdout
        .trim()
        .parse::<u32>()
        .map_err(|error| format!("解析 Codex durable wrapper PID 失败: {error}"))?;
    Ok((pid, launcher_path))
}

#[cfg(not(target_os = "windows"))]
fn spawn_direct_app_server(
    executable: &Path,
    endpoint: &str,
    token_path: &Path,
    log_path: &Path,
    error_path: &Path,
) -> Result<u32, String> {
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|error| format!("打开 {} 失败: {error}", log_path.display()))?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(error_path)
        .map_err(|error| format!("打开 {} 失败: {error}", error_path.display()))?;
    let child = Command::new(executable)
        .args([
            "app-server",
            "--listen",
            endpoint,
            "--ws-auth",
            "capability-token",
            "--ws-token-file",
            &token_path.to_string_lossy(),
        ])
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("启动 Codex background app-server 失败: {error}"))?;
    Ok(child.id())
}

pub(super) fn cleanup_worker_artifacts(prompt_path: &str) {
    if let Some(parent) = Path::new(prompt_path).parent() {
        let _ = fs::remove_file(parent.join("codex_worker.cmd"));
        let _ = fs::remove_file(parent.join("codex_worker.js"));
    }
}

// Writer ownership is not the same thing as a running task. Desktop may keep
// an idle thread loaded. Never delete its lock or terminate that other client.
fn continuation_anchor(result: &serde_json::Value, expected_id: &str) -> Result<String, String> {
    let thread = thread_from_result(result)?;
    if thread.get("id").and_then(serde_json::Value::as_str) != Some(expected_id) {
        return Err("ownership recovery: thread identity mismatch".to_string());
    }
    if !matches!(
        thread
            .pointer("/status/type")
            .and_then(serde_json::Value::as_str),
        Some("idle" | "notLoaded")
    ) {
        return Err(
            "ownership recovery: owner is active or state is unknown; no automatic fork"
                .to_string(),
        );
    }
    let turns = thread
        .get("turns")
        .and_then(serde_json::Value::as_array)
        .filter(|turns| !turns.is_empty())
        .ok_or_else(|| "ownership recovery: no complete history available".to_string())?;
    if turns.iter().any(|turn| {
        !matches!(
            turn.get("status").and_then(serde_json::Value::as_str),
            Some("completed" | "failed" | "interrupted")
        )
    }) {
        return Err("ownership recovery: unresolved turn exists; no automatic fork".to_string());
    }
    let last = turns.last().expect("non-empty checked");
    if last.get("status").and_then(serde_json::Value::as_str) != Some("completed") {
        return Err("ownership recovery: most recent turn did not complete".to_string());
    }
    last.get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "ownership recovery: missing anchor ID".to_string())
}

pub(super) fn start_background_turn(
    executable: &Path,
    source_thread_id: &str,
    automation_thread_id: Option<&str>,
    automation_name: &str,
    prompt: &str,
    consultation: bool,
    log_path: &Path,
    error_path: &Path,
) -> Result<CodexDispatch, String> {
    let source_thread_id = source_thread_id.trim();
    if source_thread_id.is_empty() {
        return Err("Codex source Thread ID 为空".to_string());
    }

    let port = free_loopback_port()?;
    let endpoint = format!("ws://127.0.0.1:{port}");
    let bearer_token = capability_token();
    let token_path = std::env::temp_dir().join(format!(
        "tunneldock-codex-ws-{}-{}.token",
        std::process::id(),
        &bearer_token[..16]
    ));
    fs::write(&token_path, &bearer_token)
        .map_err(|error| format!("写入 Codex WebSocket capability token 失败: {error}"))?;

    #[cfg(target_os = "windows")]
    let (pid, launcher_path) = match spawn_windows_durable_app_server(
        executable,
        &endpoint,
        &token_path,
        log_path,
        error_path,
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&token_path);
            return Err(error);
        }
    };

    #[cfg(not(target_os = "windows"))]
    let pid =
        match spawn_direct_app_server(executable, &endpoint, &token_path, log_path, error_path) {
            Ok(pid) => pid,
            Err(error) => {
                let _ = fs::remove_file(&token_path);
                return Err(error);
            }
        };

    let result = (|| -> Result<CodexDispatch, String> {
        let mut socket = connect_controller(&endpoint, &bearer_token, Duration::from_secs(5))?;
        let _ = fs::remove_file(&token_path);
        let mut request_id = 1u64;
        rpc_request(
            &mut socket,
            &mut request_id,
            "initialize",
            serde_json::json!({
                "clientInfo": {
                    "name": "TunnelDock",
                    "title": "TunnelDock Codex Controller",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {"experimentalApi": true}
            }),
        )?;
        socket
            .send(Message::Text(
                serde_json::json!({"method":"initialized","params":{}})
                    .to_string()
                    .into(),
            ))
            .map_err(|error| format!("发送 Codex initialized 失败: {error}"))?;

        let mut created_thread = false;
        let mut recovery: Option<serde_json::Value> = None;
        let thread_result = if let Some(existing) = automation_thread_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            match rpc_request(
                &mut socket,
                &mut request_id,
                "thread/resume",
                serde_json::json!({
                    "threadId": existing,
                    "excludeTurns": true,
                    "approvalsReviewer": "auto_review",
                }),
            ) {
                Ok(result) => result,
                Err(error) if error.contains("active writer") => {
                    let before = resolve_thread(existing)?;
                    let mut stored = rpc_request(&mut socket, &mut request_id, "thread/read",
                        serde_json::json!({"threadId": existing, "includeTurns": false}))
                        .map_err(|read_error| format!("Writer ownership conflict (not proof of active work): {error}; read-only inspection failed: {read_error}"))?;
                    // Full history for long-lived threads exceeds the WebSocket
                    // message limit. Only the latest turn boundary is required.
                    let page = rpc_request(
                        &mut socket,
                        &mut request_id,
                        "thread/turns/list",
                        serde_json::json!({"threadId": existing, "limit": 1,
                            "sortDirection": "desc", "itemsView": "notLoaded"}),
                    )?;
                    stored["thread"]["turns"] = page
                        .get("data")
                        .cloned()
                        .ok_or("ownership recovery: missing turn-summary page")?;
                    let anchor = continuation_anchor(&stored, existing).map_err(|reason| {
                        format!("Writer ownership conflict: {error}; {reason}")
                    })?;
                    let source = thread_from_result(&stored)?;
                    let cwd = source
                        .get("cwd")
                        .and_then(serde_json::Value::as_str)
                        .filter(|cwd| !cwd.is_empty())
                        .ok_or("ownership recovery: missing source cwd")?;
                    if resolve_thread(existing)?.start_offset != before.start_offset {
                        return Err(format!(
                            "Writer ownership conflict: {error}; source changed during inspection"
                        ));
                    }
                    // Copy the latest automation history, NOT the old human seed.
                    // Pin the native fork to a completed turn; do not copy a partial turn.
                    let forked = rpc_request(
                        &mut socket,
                        &mut request_id,
                        "thread/fork",
                        serde_json::json!({"threadId": existing, "lastTurnId": anchor,
                            "excludeTurns": true, "approvalsReviewer": "auto_review"}),
                    )?;
                    let (successor, _) = thread_id_and_path(&forked)?;
                    let successor_cwd = thread_from_result(&forked)?
                        .get("cwd")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    if successor == existing
                        || successor_cwd != cwd
                        || resolve_thread(existing)?.start_offset != before.start_offset
                    {
                        return Err(format!("Ownership recovery source/cwd changed; fork {successor} left unactivated for inspection"));
                    }
                    created_thread = true;
                    recovery = Some(serde_json::json!({
                        "kind": "idle_writer_continuation", "previous_thread_id": existing,
                        "current_thread_id": successor, "last_completed_turn_id": anchor,
                        "cwd": cwd, "original_error": error, "lock_deleted": false,
                        "old_owner_terminated": false, "turn_started": false,
                        "recorded_at": local_now_rfc3339()
                    }));
                    if let Some(parent) = log_path.parent() {
                        write_json(
                            &parent.join("THREAD_RECOVERY.json"),
                            recovery.as_ref().unwrap(),
                        )?;
                    }
                    forked
                }
                Err(error)
                    if error.to_ascii_lowercase().contains("not found")
                        || error.to_ascii_lowercase().contains("does not exist")
                        || error.to_ascii_lowercase().contains("no rollout") =>
                {
                    created_thread = true;
                    rpc_request(
                        &mut socket,
                        &mut request_id,
                        "thread/fork",
                        serde_json::json!({
                            "threadId": source_thread_id,
                            "excludeTurns": true,
                            "approvalsReviewer": "auto_review",
                        }),
                    )?
                }
                Err(error) => {
                    return Err(format!(
                        "恢复 Codex automation thread {existing} 失败: {error}"
                    ));
                }
            }
        } else {
            created_thread = true;
            rpc_request(
                &mut socket,
                &mut request_id,
                "thread/fork",
                serde_json::json!({
                    "threadId": source_thread_id,
                    "excludeTurns": true,
                    "approvalsReviewer": "auto_review",
                }),
            )?
        };
        let (thread_id, rollout_path) = thread_id_and_path(&thread_result)?;
        if created_thread {
            let _ = rpc_request(
                &mut socket,
                &mut request_id,
                "thread/name/set",
                serde_json::json!({
                    "threadId": thread_id,
                    "name": automation_name,
                }),
            );
        }
        let start_offset = fs::metadata(&rollout_path)
            .map_err(|error| format!("读取 {} 元数据失败: {error}", rollout_path.display()))?
            .len() as usize;

        let mut turn = serde_json::json!({
            "threadId": thread_id,
            "input": [{"type":"text","text": prompt}],
            "approvalsReviewer": "auto_review",
        });
        if consultation {
            turn["approvalPolicy"] = serde_json::json!("never");
            turn["sandboxPolicy"] = serde_json::json!({
                "type": "readOnly",
                "networkAccess": true,
            });
        } else {
            turn["approvalPolicy"] = serde_json::json!("on-request");
            turn["sandboxPolicy"] = serde_json::json!({
                "type": "workspaceWrite",
                "networkAccess": true,
            });
        }
        if let Err(error) = rpc_request(&mut socket, &mut request_id, "turn/start", turn) {
            // Do not synchronously delete a freshly-forked thread here. Codex may
            // still hold the writer while this app-server owns the fork, and a
            // delete request can block. The app-server is terminated below on
            // error, which releases the writer; any orphaned fork can then be
            // inspected/cleaned without blocking dispatch.
            let detail = if created_thread {
                format!("{error}; forked automation thread {thread_id} was not activated")
            } else {
                error
            };
            return Err(detail);
        }
        if let Some(mut record) = recovery {
            record["turn_started"] = serde_json::json!(true);
            if let Some(parent) = log_path.parent() {
                // Once the turn is accepted, do not kill it because a secondary
                // audit write failed. Its initial recovery record already exists.
                if let Err(error) = write_json(&parent.join("THREAD_RECOVERY.json"), &record) {
                    eprintln!("Cannot update Codex recovery receipt: {error}");
                }
            }
        }
        let _ = socket.close(None);

        Ok(CodexDispatch {
            thread_id,
            start_offset,
            pid,
        })
    })();

    let _ = fs::remove_file(&token_path);
    if result.is_err() {
        let _ = kill_process_tree(pid);
        #[cfg(target_os = "windows")]
        let _ = fs::remove_file(&launcher_path);
    }
    result
}

fn user_message_turn_id(value: &serde_json::Value, prompt: &str) -> Option<String> {
    if value.get("type").and_then(serde_json::Value::as_str) == Some("response_item") {
        let payload = value.get("payload")?;
        if payload.get("type").and_then(serde_json::Value::as_str) == Some("message")
            && payload.get("role").and_then(serde_json::Value::as_str) == Some("user")
        {
            let matches = payload
                .get("content")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .any(|item| item.get("text").and_then(serde_json::Value::as_str) == Some(prompt));
            if matches {
                return payload
                    .get("internal_chat_message_metadata_passthrough")
                    .and_then(|meta| meta.get("turn_id"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned);
            }
        }
    }

    let payload = value.get("payload")?;
    if value.get("type").and_then(serde_json::Value::as_str) == Some("event_msg")
        && payload.get("type").and_then(serde_json::Value::as_str) == Some("item_completed")
        && payload
            .get("item")
            .and_then(|item| item.get("type"))
            .and_then(serde_json::Value::as_str)
            == Some("UserMessage")
    {
        let matches = payload
            .get("item")
            .and_then(|item| item.get("content"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .any(|item| item.get("text").and_then(serde_json::Value::as_str) == Some(prompt));
        if matches {
            return payload
                .get("turn_id")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
        }
    }
    None
}

fn terminal_update(value: &serde_json::Value, turn_id: &str) -> Option<CodexRunUpdate> {
    if value.get("type").and_then(serde_json::Value::as_str) != Some("event_msg") {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("turn_id").and_then(serde_json::Value::as_str) != Some(turn_id) {
        return None;
    }
    match payload.get("type").and_then(serde_json::Value::as_str)? {
        "task_complete" => {
            let message = payload
                .get("last_agent_message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            if message.is_empty() {
                Some(CodexRunUpdate::Failed(
                    "Codex turn completed without a final message".to_string(),
                ))
            } else {
                Some(CodexRunUpdate::Completed(message))
            }
        }
        "task_failed" | "turn_aborted" => Some(CodexRunUpdate::Failed(
            payload
                .get("message")
                .or_else(|| payload.get("error"))
                .map(ToString::to_string)
                .unwrap_or_else(|| "Codex turn failed".to_string()),
        )),
        _ => None,
    }
}

pub(super) fn poll_run(
    thread_id: &str,
    start_offset: usize,
    prompt: &str,
) -> Result<CodexRunUpdate, String> {
    let mut binding = resolve_thread(thread_id)?;
    binding.start_offset = start_offset;
    let mut file = File::open(&binding.rollout_path)
        .map_err(|error| format!("读取 {} 失败: {error}", binding.rollout_path.display()))?;
    file.seek(SeekFrom::Start(start_offset as u64))
        .map_err(|error| format!("定位 Codex rollout 失败: {error}"))?;
    let mut appended = String::new();
    file.read_to_string(&mut appended)
        .map_err(|error| format!("读取 Codex rollout 增量失败: {error}"))?;

    let mut target_turn = None;
    for line in appended.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if target_turn.is_none() {
            target_turn = user_message_turn_id(&value, prompt);
        }
        if let Some(turn_id) = target_turn.as_deref() {
            if let Some(update) = terminal_update(&value, turn_id) {
                return Ok(update);
            }
        }
    }
    Ok(CodexRunUpdate::Running)
}

#[cfg(test)]
mod tests {
    use super::{
        capability_token, continuation_anchor, terminal_update, user_message_turn_id,
        CodexRunUpdate,
    };

    #[test]
    fn ownership_recovery_requires_verified_completed_boundary() {
        let mut value = serde_json::json!({"thread":{"id":"thread-a", "status":{"type":"notLoaded"},
            "turns":[{"id":"turn-a","status":"completed"}]}});
        assert_eq!(continuation_anchor(&value, "thread-a").unwrap(), "turn-a");
        assert!(continuation_anchor(&value, "wrong-thread").is_err());
        value["thread"]["turns"][0]["status"] = serde_json::json!("inProgress");
        assert!(continuation_anchor(&value, "thread-a").is_err());
        value["thread"]["turns"][0]["status"] = serde_json::json!("failed");
        assert!(continuation_anchor(&value, "thread-a").is_err());
        value["thread"]["turns"][0]["status"] = serde_json::json!("completed");
        value["thread"]["status"]["type"] = serde_json::json!("active");
        assert!(continuation_anchor(&value, "thread-a").is_err());
        value["thread"]["status"]["type"] = serde_json::json!("notLoaded");
        value["thread"]["turns"] = serde_json::json!([]);
        assert!(continuation_anchor(&value, "thread-a").is_err());
    }

    #[test]
    fn capability_token_is_256_bit_hex() {
        let token = capability_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_ne!(token, capability_token());
    }

    #[test]
    fn idle_writer_recovery_anchors_only_completed_history() {
        let result = serde_json::json!({"thread":{"id":"owned","status":{"type":"notLoaded"},
            "turns":[{"id":"earlier","status":"completed"},{"id":"latest","status":"completed"}]}});
        assert_eq!(
            super::continuation_anchor(&result, "owned").unwrap(),
            "latest"
        );
    }

    #[test]
    fn writer_recovery_refuses_active_or_unsettled_turns() {
        for (runtime, status) in [
            ("active", "completed"),
            ("notLoaded", "inProgress"),
            ("notLoaded", "interrupted"),
            ("systemError", "completed"),
        ] {
            let result = serde_json::json!({"thread":{"id":"owned","status":{"type":runtime},
                "turns":[{"id":"last","status":status}]}});
            assert!(super::continuation_anchor(&result, "owned").is_err());
        }
    }

    #[test]
    fn writer_recovery_refuses_missing_or_wrong_identity() {
        for result in [
            serde_json::json!({}),
            serde_json::json!({"thread":{"id":"other","turns":[]}}),
            serde_json::json!({"thread":{"id":"owned","status":{"type":"notLoaded"},"turns":[]}}),
        ] {
            assert!(super::continuation_anchor(&result, "owned").is_err());
        }
    }

    #[test]
    fn finds_queued_user_turn_and_completion() {
        let prompt = "check this";
        let user = serde_json::json!({
            "type":"event_msg",
            "payload":{
                "type":"item_completed",
                "turn_id":"turn-1",
                "item":{"type":"UserMessage","content":[{"type":"text","text":prompt}]}
            }
        });
        assert_eq!(
            user_message_turn_id(&user, prompt).as_deref(),
            Some("turn-1")
        );

        let done = serde_json::json!({
            "type":"event_msg",
            "payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":"done"}
        });
        match terminal_update(&done, "turn-1") {
            Some(CodexRunUpdate::Completed(text)) => assert_eq!(text, "done"),
            other => panic!("unexpected update: {other:?}"),
        }
    }
}
