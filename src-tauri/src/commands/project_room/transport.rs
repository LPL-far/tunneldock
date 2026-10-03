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
        (Some(success), Some(failure)) if success > failure => "healthy",
        (Some(_), Some(_)) => "degraded",
        (Some(_), None) => "healthy",
        (None, Some(_)) => "degraded",
        (None, None) => "untested",
    }
}

fn latest_terminal_run<'a>(
    runs: &'a [AgentRun],
    agent_id: &str,
    status: &str,
    after: &Option<String>,
) -> Option<&'a AgentRun> {
    let mut candidates: Vec<_> = runs
        .iter()
        .filter_map(|run| {
            let time = parse_time(&run.finished_at)?;
            (run.agent_id == agent_id && run.status == status && Some(time) > parse_time(after))
                .then_some((time, run))
        })
        .collect();
    // Stable tie-breaking makes projection independent of persisted array order.
    candidates.sort_by(|(a_time, a), (b_time, b)| {
        (b_time, &b.id, &b.finished_at, &b.error_message).cmp(&(
            a_time,
            &a.id,
            &a.finished_at,
            &a.error_message,
        ))
    });
    candidates
        .into_iter()
        .map(|(_, run)| run)
        .find(|run| status != "completed" || has_transport_handoff(run))
}

fn has_transport_handoff(run: &AgentRun) -> bool {
    use std::io::Read;
    // A receipt means a regular file with non-whitespace UTF-8 text in its first
    // 4 KiB, not scientific acceptance. Reject binary/corrupt previews; do not
    // parse or reread large bodies each tick. Corruption beyond this preview is
    // outside this delivery check. Already projected timestamps stay monotonic.
    let Ok(file) = fs::File::open(&run.output_path) else {
        return false;
    };
    let Ok(metadata) = file.metadata() else {
        return false;
    };
    if !metadata.is_file() || metadata.len() == 0 {
        return false;
    }
    let mut bytes = Vec::with_capacity(4096);
    if file.take(4096).read_to_end(&mut bytes).is_err() {
        return false;
    }
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(error) if error.error_len().is_none() && metadata.len() > bytes.len() as u64 => {
            // A valid multibyte character may straddle the preview boundary.
            match std::str::from_utf8(&bytes[..error.valid_up_to()]) {
                Ok(text) => text,
                Err(_) => return false,
            }
        }
        Err(_) => return false,
    };
    !text
        .chars()
        .any(|ch| ch.is_control() && !ch.is_whitespace())
        && text
            .chars()
            .any(|ch| !ch.is_whitespace() && ch != '\u{feff}')
}

// Run receipts are the durable source for delivery health, not scientific acceptance.
// Merge both outcomes monotonically: a delayed older success must never hide a
// newer transport failure, and an already-populated migrated value is not final.
fn reconcile_terminal_evidence(entry: &mut AgentTransportHealth, runs: &[AgentRun]) {
    let success = latest_terminal_run(runs, &entry.agent_id, "completed", &entry.last_success_at);
    let failure = latest_terminal_run(runs, &entry.agent_id, "failed", &entry.last_failure_at);
    let mut changed = false;
    if let Some(run) = success {
        if parse_time(&run.finished_at) > parse_time(&entry.last_success_at) {
            entry.last_success_at = run.finished_at.clone();
            changed = true;
        }
    }
    if let Some(run) = failure {
        if parse_time(&run.finished_at) > parse_time(&entry.last_failure_at) {
            entry.last_failure_at = run.finished_at.clone();
            // Preserve an even newer failure recorded directly by the provider.
            entry.last_error = run.error_message.clone();
            changed = true;
        }
    }
    if changed {
        if evidence_status(entry) == "healthy" {
            entry.last_error = None;
        }
        // `source` describes the newest outcome, not a historical timestamp
        // backfill. A newer direct probe retains its provenance and error.
        let newest_outcome_projected = if evidence_status(entry) == "healthy" {
            success.is_some()
        } else {
            failure.is_some()
        };
        if newest_outcome_projected {
            entry.source = "reconciled_project_run".to_string();
        }
        let latest = [&entry.last_success_at, &entry.last_failure_at]
            .into_iter()
            .filter(|value| parse_time(value).is_some())
            .max_by_key(|value| parse_time(value));
        if let Some(Some(value)) = latest {
            let previous = chrono::DateTime::parse_from_rfc3339(&entry.updated_at).ok();
            if chrono::DateTime::parse_from_rfc3339(value).ok() > previous {
                entry.updated_at = value.clone();
            }
        }
    }
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

    for agent_id in ["codex", "gemini"] {
        let active_run = runs.iter().find(|run| {
            run.agent_id == agent_id && matches!(run.status.as_str(), "running" | "interactive")
        });
        let entry = ensure_entry(&mut values, agent_id);

        reconcile_terminal_evidence(entry, runs);

        entry.active_run_id = active_run.map(|run| run.id.clone());
        entry.status = if !bound(config, agent_id) {
            "unbound".to_string()
        } else if active_run.is_some() {
            "running".to_string()
        } else {
            evidence_status(entry).to_string()
        };
    }

    if values != original || !transport_path(state, project_id)?.exists() {
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
    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "transport-test-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        fn run(&self, agent: &str, status: &str, time: Option<&str>) -> AgentRun {
            let mut run = raw_run(agent, status, time);
            run.output_path = self.0.join("HANDOFF.md").to_string_lossy().into_owned();
            fs::write(
                &run.output_path,
                "# Handoff\nScientific gate failed; experiment completed.",
            )
            .unwrap();
            run
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

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

    fn raw_run(agent: &str, status: &str, finished: Option<&str>) -> AgentRun {
        AgentRun {
            id: format!("RUN-{agent}-{status}"),
            task_id: "TASK-fixture".into(),
            agent_id: agent.into(),
            status: status.into(),
            pid: None,
            external_session_id: None,
            start_step: None,
            started_at: "2026-10-03T08:00:00Z".into(),
            finished_at: finished.map(str::to_owned),
            prompt_path: "prompt.md".into(),
            output_path: "HANDOFF.md".into(),
            log_path: "stdout.log".into(),
            error_path: "stderr.log".into(),
            error_message: if status == "failed" {
                Some("terminal failure".into())
            } else {
                None
            },
        }
    }

    #[test]
    fn stale_gemini_migration_is_reconciled_from_real_completion_time() {
        let f = Fixture::new();
        let mut value = health(Some("2026-10-02T02:32:54+08:00"), None);
        value.agent_id = "gemini".into();
        value.source = "migrated_project_run".into();
        let runs = [f.run("gemini", "completed", Some("2026-10-03T16:52:08+08:00"))];
        reconcile_terminal_evidence(&mut value, &runs);
        assert_eq!(
            value.last_success_at.as_deref(),
            Some("2026-10-03T16:52:08+08:00")
        );
        assert_eq!(value.updated_at, "2026-10-03T16:52:08+08:00");
        assert_eq!(value.source, "reconciled_project_run");
        let once = value.clone();
        reconcile_terminal_evidence(&mut value, &runs);
        assert_eq!(
            once, value,
            "identical receipts must not tick state or rewrite timestamps"
        );
    }

    #[test]
    fn late_old_success_never_clears_a_newer_transport_failure() {
        let f = Fixture::new();
        let mut value = health(Some("2026-10-02T08:00:00Z"), Some("2026-10-03T10:00:00Z"));
        value.last_error = Some("new provider failure".into());
        reconcile_terminal_evidence(
            &mut value,
            &[f.run("codex", "completed", Some("2026-10-03T09:00:00Z"))],
        );
        assert_eq!(evidence_status(&value), "degraded");
        assert_eq!(value.last_error.as_deref(), Some("new provider failure"));
        assert_eq!(
            value.last_failure_at.as_deref(),
            Some("2026-10-03T10:00:00Z")
        );
    }

    #[test]
    fn unordered_run_times_merge_success_and_failure_with_offsets() {
        let f = Fixture::new();
        let mut value = health(None, None);
        reconcile_terminal_evidence(
            &mut value,
            &[
                f.run("codex", "failed", Some("2026-10-03T09:00:00Z")),
                f.run("codex", "completed", Some("2026-10-03T18:00:00+08:00")),
                f.run("codex", "completed", Some("2026-10-03T08:59:00Z")),
            ],
        );
        assert_eq!(
            value.last_failure_at.as_deref(),
            Some("2026-10-03T09:00:00Z")
        );
        assert_eq!(
            value.last_success_at.as_deref(),
            Some("2026-10-03T18:00:00+08:00")
        );
        assert_eq!(evidence_status(&value), "healthy");
        assert!(value.last_error.is_none());
    }

    #[test]
    fn nonterminal_invalid_time_and_other_agent_cannot_advance_health() {
        let f = Fixture::new();
        let mut value = health(Some("2026-10-03T08:00:00Z"), None);
        let before = value.clone();
        reconcile_terminal_evidence(
            &mut value,
            &[
                f.run("gemini", "completed", Some("2026-10-03T20:00:00Z")),
                f.run("codex", "running", Some("2026-10-03T20:00:00Z")),
                f.run("codex", "completed", None),
                f.run("codex", "completed", Some("not-a-time")),
                f.run("codex", "completed", Some("2026-10-02T20:00:00Z")),
            ],
        );
        assert_eq!(value, before);
    }

    #[test]
    fn newer_terminal_failure_degrades_and_retains_last_success() {
        let f = Fixture::new();
        let mut value = health(Some("2026-10-03T08:00:00Z"), None);
        reconcile_terminal_evidence(
            &mut value,
            &[f.run("codex", "failed", Some("2026-10-03T09:00:00Z"))],
        );
        assert_eq!(evidence_status(&value), "degraded");
        assert_eq!(
            value.last_success_at.as_deref(),
            Some("2026-10-03T08:00:00Z")
        );
        assert_eq!(value.last_error.as_deref(), Some("terminal failure"));
    }
    #[test]
    fn missing_empty_corrupt_handoffs_cannot_advance_success() {
        let f = Fixture::new();
        let run = f.run("codex", "completed", Some("2026-10-03T09:00:00Z"));
        let mut value = health(None, None);
        let before = value.clone();
        for body in [
            &b""[..],
            &b" \n\t"[..],
            &b"\xff\x00"[..],
            &b"result\x00"[..],
        ] {
            fs::write(&run.output_path, body).unwrap();
            reconcile_terminal_evidence(&mut value, &[run.clone()]);
            assert_eq!(value, before);
        }
        fs::remove_file(&run.output_path).unwrap();
        reconcile_terminal_evidence(&mut value, &[run]);
        assert_eq!(value, before);
    }

    #[test]
    fn newer_probe_source_and_failure_survive_historical_success() {
        let f = Fixture::new();
        let mut value = health(None, Some("2026-10-03T10:00:00Z"));
        value.source = "direct_probe".into();
        value.last_error = Some("probe disconnected".into());
        reconcile_terminal_evidence(
            &mut value,
            &[f.run("codex", "completed", Some("2026-10-03T17:00:00+08:00"))],
        );
        assert_eq!(
            value.last_success_at.as_deref(),
            Some("2026-10-03T17:00:00+08:00")
        );
        assert_eq!(value.source, "direct_probe");
        assert_eq!(value.last_error.as_deref(), Some("probe disconnected"));
        assert_eq!(evidence_status(&value), "degraded");
    }

    #[test]
    fn equal_time_failure_is_not_cleared_and_input_order_does_not_change_source() {
        let f = Fixture::new();
        let runs = [
            f.run("codex", "completed", Some("2026-10-03T17:00:00+08:00")),
            f.run("codex", "failed", Some("2026-10-03T09:00:00Z")),
        ];
        let mut a = health(None, None);
        let mut b = a.clone();
        reconcile_terminal_evidence(&mut a, &runs);
        reconcile_terminal_evidence(&mut b, &[runs[1].clone(), runs[0].clone()]);
        assert_eq!(a, b);
        assert_eq!(a.last_error.as_deref(), Some("terminal failure"));
        assert_eq!(evidence_status(&a), "degraded");
    }

    #[test]
    fn newer_probe_success_is_preserved_and_old_failure_does_not_degrade() {
        let f = Fixture::new();
        let mut value = health(Some("2026-10-03T10:00:00Z"), None);
        value.source = "probe".into();
        let success = f.run("codex", "completed", Some("2026-10-03T17:00:00+08:00"));
        let before = value.clone();
        reconcile_terminal_evidence(&mut value, &[success]);
        assert_eq!(value, before);
        reconcile_terminal_evidence(
            &mut value,
            &[f.run("codex", "failed", Some("2026-10-03T09:00:00Z"))],
        );
        assert_eq!(value.last_success_at, before.last_success_at);
        assert_eq!(value.source, "probe");
        assert_eq!(evidence_status(&value), "healthy");
        assert_eq!(value.last_error, None);
    }

    #[test]
    fn bounded_preview_handles_utf8_boundary_and_whitespace_conservatively() {
        let f = Fixture::new();
        let run = f.run("codex", "completed", Some("2026-10-03T09:00:00Z"));
        let mut body = vec![b'x'; 4095];
        body.extend_from_slice("中".as_bytes());
        fs::write(&run.output_path, &body).unwrap();
        assert!(has_transport_handoff(&run));
        body = vec![b' '; 4096];
        body.extend_from_slice(b"text beyond preview");
        fs::write(&run.output_path, body).unwrap();
        assert!(!has_transport_handoff(&run));
        fs::write(&run.output_path, b"\xef\xbb\xbf \n").unwrap();
        assert!(!has_transport_handoff(&run));
    }

    #[test]
    fn invalid_newest_receipt_falls_back_to_latest_valid_completion() {
        let f = Fixture::new();
        let good = f.run("codex", "completed", Some("2026-10-03T09:00:00Z"));
        let mut missing = good.clone();
        missing.finished_at = Some("2026-10-03T10:00:00Z".into());
        missing.output_path = f.0.join("missing.md").to_string_lossy().into_owned();
        let mut invalid_failure = good.clone();
        invalid_failure.status = "failed".into();
        invalid_failure.finished_at = Some("invalid".into());
        let mut value = health(None, None);
        reconcile_terminal_evidence(&mut value, &[missing, invalid_failure, good.clone()]);
        assert_eq!(value.last_success_at, good.finished_at);
        assert_eq!(value.last_failure_at, None);
        assert_eq!(evidence_status(&value), "healthy");
    }

    #[test]
    fn reads_reconcile_persisted_gemini_and_do_not_rewrite_unchanged_health() {
        let f = Fixture::new();
        let state = AppState::isolated_for_tests(f.0.clone());
        let mut config = ProjectRoomConfig {
            id: "fixture".into(),
            name: "fixture".into(),
            local_root: String::new(),
            repo_root: String::new(),
            workspace_id: None,
            codex_thread_id: None,
            codex_automation_thread_id: None,
            antigravity_cascade_id: Some("fixture".into()),
            remote: crate::models::ProjectRemote::default(),
            enabled: true,
            keep_session_alive: false,
            created_at: String::new(),
            updated_at: String::new(),
        };
        fs::create_dir_all(project_dir(&state, &config.id).unwrap()).unwrap();
        let mut old = health(Some("2026-10-02T02:32:54+08:00"), None);
        old.agent_id = "gemini".into();
        write_transport_file(&state, &config.id, &[old]).unwrap();
        let run = f.run("gemini", "completed", Some("2026-10-03T16:52:08+08:00"));
        let first =
            load_transport_health_unlocked(&state, &config.id, &config, &[run.clone()]).unwrap();
        let gemini = first.iter().find(|v| v.agent_id == "gemini").unwrap();
        assert_eq!(gemini.last_success_at, run.finished_at);
        assert_eq!(gemini.status, "healthy");
        let path = transport_path(&state, &config.id).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let bytes = fs::read(&path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(25));
        assert_eq!(
            load_transport_health_unlocked(&state, &config.id, &config, &[run]).unwrap(),
            first
        );
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        assert_eq!(fs::read(&path).unwrap(), bytes);

        let mut early = f.run("gemini", "running", Some("2026-10-04T00:00:00Z"));
        let active =
            load_transport_health_unlocked(&state, &config.id, &config, &[early.clone()]).unwrap();
        let gemini = active.iter().find(|v| v.agent_id == "gemini").unwrap();
        assert_eq!(
            gemini.last_success_at.as_deref(),
            Some("2026-10-03T16:52:08+08:00")
        );
        assert_eq!(gemini.status, "running");
        assert_eq!(gemini.active_run_id.as_deref(), Some(early.id.as_str()));
        early.status = "queued".into();
        config.antigravity_cascade_id = None;
        let unbound =
            load_transport_health_unlocked(&state, &config.id, &config, &[early]).unwrap();
        let gemini = unbound.iter().find(|v| v.agent_id == "gemini").unwrap();
        assert_eq!(gemini.status, "unbound");
        assert_eq!(gemini.active_run_id, None);
        assert_eq!(
            gemini.last_success_at.as_deref(),
            Some("2026-10-03T16:52:08+08:00")
        );
    }
}
