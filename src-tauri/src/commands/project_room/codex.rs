use super::*;
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, Clone)]
pub(super) struct CodexBinding {
    pub thread_id: String,
    pub rollout_path: PathBuf,
    pub start_offset: usize,
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
        thread_id: thread_id.to_string(),
        rollout_path,
        start_offset,
    })
}

pub(super) fn queue_task(
    executable: &Path,
    binding: &CodexBinding,
    prompt: &str,
) -> Result<String, String> {
    let mut command = Command::new(executable);
    command.args(["queue", "--thread", &binding.thread_id, "--message", prompt]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .output()
        .map_err(|error| format!("调用 Codex shared app-server queue 失败: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(if stderr.is_empty() {
            format!("Codex queue 失败: {stdout}")
        } else {
            format!("Codex queue 失败: {stderr}")
        });
    }
    Ok(stdout)
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
    use super::{terminal_update, user_message_turn_id, CodexRunUpdate};

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
