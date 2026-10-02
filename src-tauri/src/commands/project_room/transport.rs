use super::*;
use crate::models::AgentTransportHealth;

fn transport_path(state: &AppState, project_id: &str) -> Result<PathBuf, String> {
    Ok(project_dir(state, project_id)?.join(TRANSPORT_HEALTH_FILE))
}

fn read_transport_file(
    state: &AppState,
    project_id: &str,
) -> Result<Vec<AgentTransportHealth>, String> {
    let path = transport_path(state, project_id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    read_json(&path)
}

fn write_transport_file(
    state: &AppState,
    project_id: &str,
    values: &[AgentTransportHealth],
) -> Result<(), String> {
    write_json(&transport_path(state, project_id)?, values)
}

fn bound(config: &ProjectRoomConfig, agent_id: &str) -> bool {
    match agent_id {
        "codex" => config
            .codex_thread_id
            .as_deref()
            .map(str::trim)
            .map(|value| !value.is_empty())
            .unwrap_or(false),
        "gemini" => config
            .antigravity_cascade_id
            .as_deref()
            .map(str::trim)
            .map(|value| !value.is_empty())
            .unwrap_or(false),
        _ => true,
    }
}

fn parse_time(value: &Option<String>) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    value
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
}

fn evidence_status(value: &AgentTransportHealth) -> &str {
    match (
        parse_time(&value.last_success_at),
        parse_time(&value.last_failure_at),
    ) {
        (Some(success), Some(failure)) if success >= failure => "healthy",
        (Some(_), Some(_)) => "degraded",
        (Some(_), None) => "healthy",
        (None, Some(_)) => "degraded",
        (None, None) => "untested",
    }
}

fn latest_terminal_run<'a>(runs: &'a [AgentRun], agent_id: &str) -> Option<&'a AgentRun> {
    runs.iter()
        .filter(|run| {
            run.agent_id == agent_id
                && matches!(run.status.as_str(), "completed" | "failed")
                && run.finished_at.is_some()
        })
        .max_by_key(|run| {
            run.finished_at
                .as_deref()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        })
}

fn ensure_entry<'a>(
    values: &'a mut Vec<AgentTransportHealth>,
    agent_id: &str,
) -> &'a mut AgentTransportHealth {
    if let Some(index) = values.iter().position(|value| value.agent_id == agent_id) {
        return &mut values[index];
    }
    values.push(AgentTransportHealth {
        agent_id: agent_id.to_string(),
        status: "untested".to_string(),
        active_run_id: None,
        last_success_at: None,
        last_failure_at: None,
        last_error: None,
        source: "none".to_string(),
        updated_at: local_now_rfc3339(),
    });
    values.last_mut().expect("transport health entry inserted")
}

pub(super) fn load_transport_health_unlocked(
    state: &AppState,
    project_id: &str,
    config: &ProjectRoomConfig,
    runs: &[AgentRun],
) -> Result<Vec<AgentTransportHealth>, String> {
    let mut values = read_transport_file(state, project_id)?;
    let original = values.clone();
    let mut migrated = false;

    for agent_id in ["codex", "gemini"] {
        let active_run = runs.iter().find(|run| {
            run.agent_id == agent_id && matches!(run.status.as_str(), "running" | "interactive")
        });
        let entry = ensure_entry(&mut values, agent_id);

        if entry.last_success_at.is_none() && entry.last_failure_at.is_none() {
            if let Some(run) = latest_terminal_run(runs, agent_id) {
                match run.status.as_str() {
                    "completed" => {
                        entry.last_success_at = run.finished_at.clone();
                        entry.last_error = None;
                    }
                    "failed" => {
                        entry.last_failure_at = run.finished_at.clone();
                        entry.last_error = run.error_message.clone();
                    }
                    _ => {}
                }
                entry.source = "migrated_project_run".to_string();
                entry.updated_at = run.finished_at.clone().unwrap_or_else(local_now_rfc3339);
                migrated = true;
            }
        }

        entry.active_run_id = active_run.map(|run| run.id.clone());
        entry.status = if !bound(config, agent_id) {
            "unbound".to_string()
        } else if active_run.is_some() {
            "running".to_string()
        } else {
            evidence_status(entry).to_string()
        };
    }

    if migrated || values != original || !transport_path(state, project_id)?.exists() {
        write_transport_file(state, project_id, &values)?;
    }
    Ok(values)
}

fn update_transport(
    state: &AppState,
    project_id: &str,
    agent_id: &str,
    status: &str,
    run_id: Option<&str>,
    success: bool,
    error: Option<&str>,
    source: &str,
) -> Result<(), String> {
    let mut values = read_transport_file(state, project_id)?;
    let entry = ensure_entry(&mut values, agent_id);
    let now = local_now_rfc3339();
    entry.status = status.to_string();
    entry.active_run_id = run_id.map(ToOwned::to_owned);
    entry.source = source.to_string();
    entry.updated_at = now.clone();
    if success {
        entry.last_success_at = Some(now);
        entry.last_error = None;
    } else if status == "degraded" {
        entry.last_failure_at = Some(now);
        entry.last_error = error.map(|value| value.chars().take(1_500).collect());
    }
    write_transport_file(state, project_id, &values)
}

pub(super) fn record_transport_running(
    state: &AppState,
    project_id: &str,
    agent_id: &str,
    run_id: &str,
) -> Result<(), String> {
    update_transport(
        state,
        project_id,
        agent_id,
        "running",
        Some(run_id),
        false,
        None,
        "project_run",
    )
}

pub(super) fn record_transport_success(
    state: &AppState,
    project_id: &str,
    agent_id: &str,
    run_id: Option<&str>,
    source: &str,
) -> Result<(), String> {
    update_transport(
        state, project_id, agent_id, "healthy", run_id, true, None, source,
    )
}

pub(super) fn record_transport_failure(
    state: &AppState,
    project_id: &str,
    agent_id: &str,
    run_id: Option<&str>,
    error: &str,
    source: &str,
) -> Result<(), String> {
    update_transport(
        state,
        project_id,
        agent_id,
        "degraded",
        run_id,
        false,
        Some(error),
        source,
    )
}

#[cfg(test)]
mod tests {
    use super::evidence_status;
    use crate::models::AgentTransportHealth;

    fn health(success: Option<&str>, failure: Option<&str>) -> AgentTransportHealth {
        AgentTransportHealth {
            agent_id: "codex".to_string(),
            status: String::new(),
            active_run_id: None,
            last_success_at: success.map(ToOwned::to_owned),
            last_failure_at: failure.map(ToOwned::to_owned),
            last_error: None,
            source: "test".to_string(),
            updated_at: "2026-10-02T00:00:00+08:00".to_string(),
        }
    }

    #[test]
    fn latest_e2e_evidence_controls_transport_health() {
        let value = health(
            Some("2026-10-02T13:45:00+08:00"),
            Some("2026-10-02T12:00:00+08:00"),
        );
        assert_eq!(evidence_status(&value), "healthy");

        let value = health(
            Some("2026-10-02T12:00:00+08:00"),
            Some("2026-10-02T13:45:00+08:00"),
        );
        assert_eq!(evidence_status(&value), "degraded");
    }
}
