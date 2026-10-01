use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

const RESULT_SUMMARY_CHAR_LIMIT: usize = 500;
const REDACTED: &str = "[REDACTED]";

static SECRET_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:sk-[A-Za-z0-9_-]{8,}|bearer\s+[A-Za-z0-9._~+/=-]{8,}|github_pat_[A-Za-z0-9_]{8,}|gh[pousr]_[A-Za-z0-9]{20,}|xox[baprs]-[A-Za-z0-9-]{8,})",
    )
    .expect("secret redaction regex is a fixed valid pattern")
});

static SENSITIVE_ASSIGNMENT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(["']?(?:api[_-]?key|password|passwd|secret|authorization|access[_-]?token|refresh[_-]?token|private[_-]?key|tunnel[_-]?key|credential|credentials)["']?\s*[:=]\s*)(?:"[^"]*"|'[^']*'|[^\s,;}]+)"#,
    )
    .expect("sensitive assignment regex is a fixed valid pattern")
});

#[derive(Debug, PartialEq)]
pub enum RpcAuditEvent {
    ToolStarted {
        call_id: String,
        tool_name: String,
        args_json: String,
        input_tokens: u64,
    },
    ToolFinished {
        call_id: String,
        result_summary: String,
        is_error: bool,
        output_tokens: u64,
    },
}

pub fn parse_rpc_audit_event(line: &str) -> Option<RpcAuditEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    let event_type = value.get("type")?.as_str()?;

    match event_type {
        "tool_execution_start" => {
            let call_id = value.get("toolCallId")?.as_str()?.to_string();
            let tool_name = value.get("toolName")?.as_str()?.to_string();
            let args = value
                .get("args")
                .cloned()
                .unwrap_or_else(|| Value::Object(Default::default()));
            let raw_args_json = args.to_string();
            let args_json = redact_audit_json(&raw_args_json);

            Some(RpcAuditEvent::ToolStarted {
                call_id,
                tool_name,
                input_tokens: count_tokens(&raw_args_json),
                args_json,
            })
        }
        "tool_execution_end" => {
            let call_id = value.get("toolCallId")?.as_str()?.to_string();
            let result = value.get("result").cloned().unwrap_or(Value::Null);
            let result_json = result.to_string();

            Some(RpcAuditEvent::ToolFinished {
                call_id,
                result_summary: summarize_result(&result, &result_json),
                is_error: value
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                output_tokens: count_tokens(&result_json),
            })
        }
        _ => None,
    }
}

fn count_tokens(text: &str) -> u64 {
    tiktoken_rs::o200k_base_singleton()
        .encode_ordinary(text)
        .len() as u64
}

fn normalize_sensitive_key(key: &str) -> String {
    key.chars()
        .filter(|ch| !matches!(ch, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        normalize_sensitive_key(key).as_str(),
        "apikey"
            | "password"
            | "passwd"
            | "secret"
            | "authorization"
            | "accesstoken"
            | "refreshtoken"
            | "privatekey"
            | "tunnelkey"
            | "credential"
            | "credentials"
    )
}

fn redact_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *child = Value::String(REDACTED.to_string());
                } else {
                    redact_value(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_value(item);
            }
        }
        Value::String(text) => {
            *text = redact_audit_text(text);
        }
        _ => {}
    }
}

pub(crate) fn redact_audit_text(text: &str) -> String {
    let tokens_redacted = SECRET_PATTERN.replace_all(text, REDACTED);
    SENSITIVE_ASSIGNMENT_PATTERN
        .replace_all(&tokens_redacted, |captures: &regex::Captures<'_>| {
            format!("{}{}", &captures[1], REDACTED)
        })
        .into_owned()
}

pub(crate) fn redact_audit_json(json: &str) -> String {
    match serde_json::from_str::<Value>(json) {
        Ok(mut value) => {
            redact_value(&mut value);
            value.to_string()
        }
        Err(_) => redact_audit_text(json),
    }
}

fn summarize_result(result: &Value, fallback_json: &str) -> String {
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .map(|content| {
            content
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| fallback_json.to_string());

    let redacted = redact_audit_text(&text);
    truncate_chars(&redacted, RESULT_SUMMARY_CHAR_LIMIT)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut truncated: String = text.chars().take(max_chars).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::{parse_rpc_audit_event, redact_audit_json, redact_audit_text, RpcAuditEvent};

    #[test]
    fn parses_tool_start_with_real_arguments() {
        let event = parse_rpc_audit_event(
            r#"{"type":"tool_execution_start","toolCallId":"call_123","toolName":"read","args":{"path":"README.md"}}"#,
        )
        .expect("tool start should be audited");

        assert_eq!(
            event,
            RpcAuditEvent::ToolStarted {
                call_id: "call_123".to_string(),
                tool_name: "read".to_string(),
                args_json: r#"{"path":"README.md"}"#.to_string(),
                input_tokens: 6,
            }
        );
    }

    #[test]
    fn parses_successful_tool_end_and_counts_the_full_result() {
        let full_output = "a".repeat(5_000);
        let line = serde_json::json!({
            "type": "tool_execution_end",
            "toolCallId": "call_123",
            "toolName": "read",
            "result": {
                "content": [{"type": "text", "text": full_output}],
                "details": {"truncation": null}
            },
            "isError": false
        })
        .to_string();

        let event = parse_rpc_audit_event(&line).expect("tool end should be audited");
        let RpcAuditEvent::ToolFinished {
            call_id,
            result_summary,
            is_error,
            output_tokens,
        } = event
        else {
            panic!("expected a completed tool event");
        };

        assert_eq!(call_id, "call_123");
        assert!(!is_error);
        assert!(result_summary.ends_with('…'));
        assert!(result_summary.chars().count() <= 501);
        assert!(
            output_tokens > 500,
            "must count the untruncated result payload"
        );
    }

    #[test]
    fn preserves_error_status_from_tool_end() {
        let event = parse_rpc_audit_event(
            r#"{"type":"tool_execution_end","toolCallId":"call_bad","toolName":"bash","result":{"content":[{"type":"text","text":"permission denied"}]},"isError":true}"#,
        )
        .expect("failed tool end should still be audited");

        assert!(matches!(
            event,
            RpcAuditEvent::ToolFinished {
                call_id,
                is_error: true,
                ..
            } if call_id == "call_bad"
        ));
    }

    #[test]
    fn redacts_sensitive_fields_and_embedded_tokens() {
        let sanitized = redact_audit_json(
            r#"{"api_key":"sk-live-example123456","nested":{"password":"hunter2"},"command":"curl -H 'Authorization: Bearer abcdefghijklmnop'"}"#,
        );

        assert!(!sanitized.contains("sk-live-example123456"));
        assert!(!sanitized.contains("hunter2"));
        assert!(!sanitized.contains("abcdefghijklmnop"));
        assert!(sanitized.matches("[REDACTED]").count() >= 3);

        let plain = redact_audit_text("token=sk-project-secret123456");
        assert_eq!(plain, "token=[REDACTED]");

        let password = redact_audit_text(r#"{"password":"plain-text-secret"}"#);
        assert!(!password.contains("plain-text-secret"));
        assert!(password.contains(r#""password":[REDACTED]"#));
    }

    #[test]
    fn tool_start_keeps_token_count_but_persists_redacted_args() {
        let event = parse_rpc_audit_event(
            r#"{"type":"tool_execution_start","toolCallId":"call_secret","toolName":"write","args":{"path":"secret.txt","api_key":"sk-project-secret123456"}}"#,
        )
        .expect("tool start should be audited");

        let RpcAuditEvent::ToolStarted {
            args_json,
            input_tokens,
            ..
        } = event
        else {
            panic!("expected a tool start event");
        };

        assert!(!args_json.contains("sk-project-secret123456"));
        assert!(args_json.contains("[REDACTED]"));
        assert!(input_tokens > 0);
    }

    #[test]
    fn ignores_non_tool_rpc_lines() {
        assert_eq!(
            parse_rpc_audit_event(r#"{"type":"message_update","usage":{"input":0}}"#),
            None
        );
        assert_eq!(parse_rpc_audit_event("not-json"), None);
    }
}
