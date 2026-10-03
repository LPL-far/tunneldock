use super::*;
use serde_json::Value;

const SERVICE_PREFIX: &str = "/exa.language_server_pb.LanguageServerService";
const IDLE_STATUS: &str = "CASCADE_RUN_STATUS_IDLE";
const STEP_BATCH: usize = 200;
const MAX_CONFIG_BATCHES: usize = 10;

#[derive(Debug, Clone)]
pub(super) struct CascadeBinding {
    pub cascade_id: String,
    pub step_count: usize,
    pub status: String,
}

#[derive(Debug)]
pub(super) enum CascadeRunUpdate {
    Running,
    Completed(String),
    Failed(String),
}

#[derive(Debug, Clone)]
struct RpcTarget {
    base_url: String,
    csrf_token: String,
}

fn rpc_target() -> Result<RpcTarget, String> {
    #[cfg(not(target_os = "windows"))]
    {
        Err("Antigravity Cascade RPC 当前仅支持 Windows".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        let (pid, csrf_token) = telemetry::antigravity_language_server_runtime()
            .ok_or_else(|| "未检测到 Antigravity Language Server".to_string())?;
        let csrf_token = csrf_token
            .ok_or_else(|| "Antigravity Language Server 启动参数缺少 CSRF token".to_string())?;
        let ports = telemetry::listening_ports_for_pid(pid);
        if ports.is_empty() {
            return Err("Antigravity Language Server 没有可用的本地 RPC 端口".to_string());
        }

        for port in ports {
            for scheme in ["https", "http"] {
                let base_url = format!("{scheme}://127.0.0.1:{port}");
                let url = format!("{}{}/GetAllCascadeTrajectories", base_url, SERVICE_PREFIX);
                if telemetry::local_json_post(
                    &url,
                    &csrf_token,
                    &serde_json::json!({ "excludeSubtrajectories": true }),
                    3,
                )
                .is_ok()
                {
                    return Ok(RpcTarget {
                        base_url,
                        csrf_token,
                    });
                }
            }
        }

        Err("无法发现 Antigravity Language Server RPC endpoint".to_string())
    }
}

fn rpc_post(target: &RpcTarget, method: &str, body: &Value) -> Result<Value, String> {
    let url = format!("{}{}/{}", target.base_url, SERVICE_PREFIX, method);
    telemetry::local_json_post(&url, &target.csrf_token, body, 6)
        .map_err(|error| format!("Antigravity RPC {method} 失败: {error}"))
}

fn normalize_workspace_uri(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/").to_ascii_lowercase();
    if let Some(rest) = normalized.strip_prefix("file:///") {
        normalized = rest.to_string();
    }
    normalized = normalized
        .replace("%3a", ":")
        .replace("%20", " ")
        .trim_end_matches('/')
        .to_string();
    normalized
}

fn cascade_summaries(target: &RpcTarget) -> Result<Vec<(String, Value)>, String> {
    let value = rpc_post(
        target,
        "GetAllCascadeTrajectories",
        &serde_json::json!({ "excludeSubtrajectories": true }),
    )?;
    let summaries = value
        .get("trajectorySummaries")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            "Antigravity GetAllCascadeTrajectories 缺少 trajectorySummaries".to_string()
        })?;
    Ok(summaries
        .iter()
        .map(|(id, summary)| (id.clone(), summary.clone()))
        .collect())
}

fn binding_from_summary(cascade_id: String, summary: &Value) -> CascadeBinding {
    CascadeBinding {
        cascade_id,
        step_count: summary
            .get("stepCount")
            .and_then(Value::as_u64)
            .unwrap_or_default() as usize,
        status: summary
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

fn summary_matches_workspace(summary: &Value, local_root: &str) -> bool {
    let expected = normalize_workspace_uri(local_root);
    summary
        .get("workspaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|workspace| workspace.get("workspaceFolderAbsoluteUri"))
        .filter_map(Value::as_str)
        .any(|uri| normalize_workspace_uri(uri) == expected)
}

pub(super) fn resolve_cascade(
    local_root: &str,
    preferred_cascade_id: Option<&str>,
) -> Result<CascadeBinding, String> {
    let target = rpc_target()?;
    let summaries = cascade_summaries(&target)?;

    if let Some(preferred) = preferred_cascade_id.filter(|value| !value.trim().is_empty()) {
        if let Some((id, summary)) = summaries
            .iter()
            .find(|(id, summary)| id == preferred && summary_matches_workspace(summary, local_root))
        {
            return Ok(binding_from_summary(id.clone(), summary));
        }
    }

    let matches = summaries
        .iter()
        .filter(|(_, summary)| summary_matches_workspace(summary, local_root))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Err(format!(
            "没有找到 workspace 与 {} 匹配的 Antigravity Cascade；请先在 Antigravity 中打开该项目的长期对话。",
            local_root
        )),
        [(id, summary)] => Ok(binding_from_summary((*id).clone(), summary)),
        many => {
            let candidates = many
                .iter()
                .take(6)
                .map(|(id, summary)| {
                    format!(
                        "{} ({})",
                        id,
                        summary
                            .get("summary")
                            .and_then(Value::as_str)
                            .unwrap_or("untitled")
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "{} 匹配到多个 Antigravity Cascade，请在 Project Config 指定 antigravity_cascade_id。候选: {}",
                local_root, candidates
            ))
        }
    }
}

fn get_steps(
    target: &RpcTarget,
    cascade_id: &str,
    step_offset: usize,
) -> Result<Vec<Value>, String> {
    let value = rpc_post(
        target,
        "GetCascadeTrajectorySteps",
        &serde_json::json!({
            "cascadeId": cascade_id,
            "stepOffset": step_offset,
            "disableRehydration": false
        }),
    )?;
    Ok(value
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn latest_cascade_config(target: &RpcTarget, binding: &CascadeBinding) -> Result<Value, String> {
    let mut end = binding.step_count;
    for _ in 0..MAX_CONFIG_BATCHES {
        let start = end.saturating_sub(STEP_BATCH);
        let steps = get_steps(target, &binding.cascade_id, start)?;
        if let Some(config) = steps.iter().rev().find_map(|step| {
            step.get("userInput")
                .and_then(|user_input| user_input.get("userConfig"))
                .filter(|config| !config.is_null())
                .cloned()
        }) {
            return Ok(config);
        }
        if start == 0 {
            break;
        }
        end = start;
    }
    Err(format!(
        "Cascade {} 中没有可复用的 userConfig；请先在 Antigravity 中手动发送一次正常消息。",
        binding.cascade_id
    ))
}

pub(super) fn send_task(binding: &CascadeBinding, prompt: &str) -> Result<(), String> {
    if binding.status != IDLE_STATUS {
        return Err(format!(
            "Antigravity Cascade {} 当前不是 idle（{}），拒绝插入并发消息。",
            binding.cascade_id, binding.status
        ));
    }

    let target = rpc_target()?;
    let cascade_config = latest_cascade_config(&target, binding)?;
    rpc_post(
        &target,
        "SendUserCascadeMessage",
        &serde_json::json!({
            "cascadeId": binding.cascade_id,
            "items": [{ "text": prompt }],
            "cascadeConfig": cascade_config,
            "blocking": false,
            "propagateError": true
        }),
    )?;
    Ok(())
}

fn compact_error(step: &Value) -> Option<String> {
    let error = step.get("errorMessage")?.get("error")?;
    for key in ["shortError", "userErrorMessage", "modelErrorMessage"] {
        if let Some(text) = error.get(key).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                return Some(text.chars().take(2_000).collect());
            }
        }
    }
    None
}

fn final_response(steps: &[Value]) -> Option<String> {
    steps.iter().rev().find_map(|step| {
        step.get("plannerResponse")
            .and_then(|response| response.get("response"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned)
    })
}

pub(super) fn poll_run(
    cascade_id: &str,
    start_step: usize,
    output_path: &Path,
) -> Result<CascadeRunUpdate, String> {
    let target = rpc_target()?;
    let summaries = cascade_summaries(&target)?;
    let Some((_, summary)) = summaries.iter().find(|(id, _)| id == cascade_id) else {
        return Ok(CascadeRunUpdate::Failed(format!(
            "Antigravity Cascade {} 已不存在",
            cascade_id
        )));
    };
    let binding = binding_from_summary(cascade_id.to_string(), summary);
    if binding.step_count > start_step {
        super::activity::record_progress(
            output_path,
            binding.step_count - start_step,
            if binding.status == IDLE_STATUS {
                "turn_finished"
            } else {
                "steps_received"
            },
            None,
        );
    }
    if binding.step_count <= start_step || binding.status != IDLE_STATUS {
        return Ok(CascadeRunUpdate::Running);
    }

    let steps = get_steps(&target, cascade_id, start_step)?;
    if let Some(error) = steps.iter().rev().find_map(compact_error) {
        return Ok(CascadeRunUpdate::Failed(error));
    }
    if let Some(response) = final_response(&steps) {
        return Ok(CascadeRunUpdate::Completed(response));
    }

    Ok(CascadeRunUpdate::Failed(
        "Antigravity 已回到 idle，但本轮没有产生可读的最终回复。".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::normalize_workspace_uri;

    #[test]
    fn normalizes_windows_workspace_uris() {
        assert_eq!(
            normalize_workspace_uri("file:///D:/gpt_proj/"),
            "d:/gpt_proj"
        );
        assert_eq!(
            normalize_workspace_uri("file:///d%3A/gpt_proj"),
            "d:/gpt_proj"
        );
        assert_eq!(normalize_workspace_uri(r"D:\gpt_proj"), "d:/gpt_proj");
    }
}
