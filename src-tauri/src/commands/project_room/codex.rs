use super::*;
use crate::utils::cmd::kill_process_tree;
use rand::RngCore;
use std::fmt::Write as _;
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
    fs::write(&token_path, &bearer_token)
        .map_err(|error| format!("写入 Codex WebSocket capability token 失败: {error}"))?;

    let mut command = Command::new(executable);
    command
        .args([
            "app-server",
            "--listen",
            &endpoint,
            "--ws-auth",
            "capability-token",
            "--ws-token-file",
            &token_path.to_string_lossy(),
        ])
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_file(&token_path);
            return Err(format!("启动 Codex background app-server 失败: {error}"));
        }
    };
    let pid = child.id();

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
                "capabilities": {}
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
                    return Err(format!(
                        "Codex automation thread {existing} 正在执行另一项后台任务；请等待该任务完成。"
                    ));
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
    use super::{capability_token, terminal_update, user_message_turn_id, CodexRunUpdate};

    #[test]
    fn capability_token_is_256_bit_hex() {
        let token = capability_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_ne!(token, capability_token());
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
