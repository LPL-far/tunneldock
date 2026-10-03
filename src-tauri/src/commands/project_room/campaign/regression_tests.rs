use super::tests::Fixture;
use super::*;

struct Runtime {
    f: Fixture,
    state: AppState,
    config: ProjectRoomConfig,
}
impl Runtime {
    fn new(single: bool) -> Self {
        let mut f = Fixture::new();
        if single {
            f.data.campaigns[0].plan.stages.truncate(1);
            f.data.campaigns[0].plan.stages[0].on_pass = None;
        } else {
            f.data.campaigns[0].plan.stages[1].intervention_scope =
                f.c().plan.stages[0].intervention_scope.clone();
        }
        f.data.campaigns[0].plan_sha256 = store::plan_hash(&f.c().plan).unwrap();
        let state = AppState::isolated_for_tests(f.root.join("runtime"));
        f.dir = project_dir(&state, "fixture").unwrap();
        fs::create_dir_all(&f.dir).unwrap();
        let config = ProjectRoomConfig {
            id: "fixture".into(),
            name: "Regression fixture".into(),
            local_root: f.root.to_string_lossy().into(),
            repo_root: f.root.to_string_lossy().into(),
            workspace_id: None,
            codex_thread_id: None,
            codex_automation_thread_id: None,
            antigravity_cascade_id: None,
            remote: crate::models::ProjectRemote::default(),
            enabled: true,
            keep_session_alive: false,
            created_at: "2026-10-03T00:00:00Z".into(),
            updated_at: "2026-10-03T00:00:00Z".into(),
        };
        store::atomic(&f.dir.join(CONFIG_FILE), &config).unwrap();
        store::atomic(&f.dir.join(TASKS_FILE), &Vec::<ProjectTask>::new()).unwrap();
        store::atomic(&f.dir.join(RUNS_FILE), &Vec::<AgentRun>::new()).unwrap();
        f.authorize();
        store::save(&f.dir, &mut f.data).unwrap();
        Self { f, state, config }
    }
    fn tick(&mut self) {
        reconcile_project(&self.state, "fixture").unwrap();
        self.f.data = store::load(&self.f.dir).unwrap();
    }
    fn tasks(&self) -> Vec<ProjectTask> {
        store::read(&self.f.dir.join(TASKS_FILE)).unwrap()
    }
    fn complete(&mut self) {
        let a = self.f.c().attempts.last().unwrap();
        let task = self
            .tasks()
            .into_iter()
            .find(|t| t.id == a.task_id)
            .unwrap();
        reserve(
            &self.state,
            "fixture",
            &task,
            &format!("RUN-FIXTURE-{}", self.f.c().attempts.len()),
        )
        .unwrap();
        self.f.data = store::load(&self.f.dir).unwrap();
        self.f.complete([1.0, 2.0, 3.0]);
        store::atomic(&self.f.dir.join(RUNS_FILE), &self.f.runs).unwrap();
        confirm_terminal(&self.f.dir, self.f.runs.last().unwrap(), true).unwrap();
    }
    fn review(&self) -> Value {
        json!({"kind":"campaign.reviewed","author":"chatgpt","campaign_id":"fixture","plan_sha256":self.f.c().plan_sha256,
        "plan_version":1,"evidence_ids":self.f.c().attempts.iter().filter_map(|a|a.evidence_id.clone()).collect::<Vec<_>>(),"review_ref":"independent-web-regression","accepted":true})
    }
    fn snapshot(&self) -> ProjectRoomSnapshot {
        serde_json::from_value(json!({"config":self.config,"tasks":self.tasks(),"runs":self.f.runs,
            "agents":[],"capacities":[],"transports":[],"experiments":[],"discussion":[],
            "memory":{"project_state":"","session_handoff":"","decisions":"","experiments":"","memory_protocol":"","updated_at":""},
            "memory_health":{"total_current_bytes":0,"total_archive_bytes":0,"ledger_events":0,"requires_compaction":false,"near_budget":false,"files":[],"updated_at":""}})).unwrap()
    }
}

#[test]
fn verified_execution_releases_real_writer_guard_without_scientific_acceptance() {
    let mut r = Runtime::new(false);
    r.tick();
    r.complete();
    let mut tasks = r.tasks();
    tasks[0].status = "review".into();
    store::atomic(&r.f.dir.join(TASKS_FILE), &tasks).unwrap();
    r.tick();
    let snapshot = r.snapshot();
    let next = &snapshot.tasks[1];
    let prior = &snapshot.tasks[0];
    assert_eq!(prior.status, "completed");
    assert_eq!(prior.reviewed_by, "campaign-evidence-verifier");
    assert!(!prior.web_reviewed);
    assert_eq!(prior.write_scope, next.write_scope);
    assert!(ensure_dispatch_scope_is_safe(&snapshot, next).is_ok());
    assert!(allowed(&r.state, "fixture", next).is_ok());
    assert!(!r.f.c().web_reviewed && !r.f.c().accepted);
}

#[test]
fn blocked_unreserved_task_is_restored_after_pause_without_replaying_reserved_work() {
    let mut r = Runtime::new(true);
    r.tick();
    let mut tasks = r.tasks();
    tasks[0].status = "blocked".into();
    tasks[0].summary = "writer scope conflict".into();
    store::atomic(&r.f.dir.join(TASKS_FILE), &tasks).unwrap();
    let mut event =
        json!({"kind":"campaign.pause","campaign_id":"fixture","plan_sha256":r.f.c().plan_sha256});
    process_event(&r.state, "fixture", &event, true).unwrap();
    r.tick();
    assert_eq!(r.tasks()[0].status, "blocked");
    assert!(!r.tasks()[0].auto_dispatch);
    event["kind"] = json!("campaign.resume");
    process_event(&r.state, "fixture", &event, true).unwrap();
    r.tick();
    assert_eq!(r.tasks()[0].status, "queued");
    reserve(&r.state, "fixture", &r.tasks()[0], "RUN-UNCERTAIN").unwrap();
    r.tick();
    r.tick();
    assert_eq!(r.f.c().attempts.len(), 1);
    assert!(allowed(&r.state, "fixture", &r.tasks()[0]).is_err());
    assert_eq!(r.f.c().state, "awaiting_web_review");
}

#[test]
fn disabled_room_blocks_reconciliation_and_manual_dispatch() {
    let mut r = Runtime::new(true);
    r.config.enabled = false;
    store::atomic(&r.f.dir.join(CONFIG_FILE), &r.config).unwrap();
    r.tick();
    assert!(r.f.c().attempts.is_empty());
    assert!(r.tasks().is_empty());
    r.config.enabled = true;
    store::atomic(&r.f.dir.join(CONFIG_FILE), &r.config).unwrap();
    r.tick();
    assert_eq!(r.tasks().len(), 1);
    r.config.enabled = false;
    store::atomic(&r.f.dir.join(CONFIG_FILE), &r.config).unwrap();
    assert!(allowed(&r.state, "fixture", &r.tasks()[0]).is_err());
    assert!(reserve(&r.state, "fixture", &r.tasks()[0], "RUN-NOT-ALLOWED").is_err());
    r.tick();
    assert!(!r.tasks()[0].auto_dispatch);
    assert_eq!(
        summary(&r.config)["items"][0]["next_action"],
        "room_disabled"
    );
}

#[test]
fn confirmed_operational_retry_can_pass_independent_web_review() {
    let mut r = Runtime::new(true);
    r.tick();
    let task = r.tasks()[0].clone();
    reserve(&r.state, "fixture", &task, "RUN-FIXTURE-1").unwrap();
    r.f.data = store::load(&r.f.dir).unwrap();
    r.f.complete([1.0, 2.0, 3.0]);
    r.f.runs[0].status = "failed".into();
    r.f.runs[0].error_message = Some("Confirmed operational failure".into());
    store::atomic(&r.f.dir.join(RUNS_FILE), &r.f.runs).unwrap();
    confirm_terminal(&r.f.dir, &r.f.runs[0], false).unwrap();
    r.tick();
    assert_eq!(r.f.c().attempts.len(), 2);
    r.complete();
    r.tick();
    assert_eq!(r.f.c().state, "awaiting_web_review");
    assert!(!r.f.c().web_reviewed);
    let event = r.review();
    process_event(&r.state, "fixture", &event, false).unwrap();
    assert!(!store::load(&r.f.dir).unwrap().campaigns[0].accepted);
    r.tick();
    assert!(r.f.c().accepted && r.f.c().web_reviewed);
    assert_eq!(r.f.c().attempts[0].execution, "operational_failure");
    process_event(&r.state, "fixture", &event, false).unwrap();
    r.tick();
    assert!(r.f.c().accepted);
}

#[test]
fn final_acceptance_rechecks_sealed_current_and_frozen_evidence() {
    for mutation in 0..3 {
        let mut r = Runtime::new(true);
        r.tick();
        r.complete();
        r.tick();
        let id = r.f.c().attempts[0].evidence_id.clone().unwrap();
        let sealed = r.f.dir.join("campaign-evidence").join(format!("{id}.json"));
        let original = fs::read(&sealed).unwrap();
        match mutation {
            0 => fs::write(&sealed, "corrupt").unwrap(),
            1 => {
                r.f.edit_artifact(|v| v["samples"][0]["value"] = json!(99.0))
            }
            _ => fs::write(r.f.root.join("evaluator.json"), "drift").unwrap(),
        };
        if mutation == 1 {
            assert_eq!(fs::read(&sealed).unwrap(), original);
        }
        process_event(&r.state, "fixture", &r.review(), false).unwrap();
        r.tick();
        assert!(!r.f.c().accepted && !r.f.c().web_reviewed);
        assert!(r.f.c().reason.contains("Web review rejected"));
        assert_eq!(r.f.c().attempts[0].gate, Some(true));
    }
}

#[test]
fn corrupt_attempt_states_are_rejected_before_dispatch() {
    let mut f = Fixture::new();
    f.authorize();
    f.tick();
    let valid = f.data.clone();
    for mutation in 0..7 {
        let mut data = valid.clone();
        let a = &mut data.campaigns[0].attempts[0];
        match mutation {
            0 => a.stage = usize::MAX,
            1 => a.number = 0,
            2 => a.task_id = "wrong".into(),
            3 => a.execution = "running".into(),
            4 => a.terminal_confirmation = Some("success".into()),
            5 => a.gate = Some(true),
            _ => a.evidence = "unknown-state".into(),
        };
        store::atomic(&f.dir.join("campaigns.json"), &data).unwrap();
        assert!(store::load(&f.dir).is_err());
    }
}

#[test]
fn failed_save_does_not_advance_revision_or_leave_temp_and_stale_saves_fail() {
    let mut f = Fixture::new();
    f.authorize();
    f.tick();
    let mut stale = f.data.clone();
    store::save(&f.dir, &mut f.data).unwrap();
    assert!(store::save(&f.dir, &mut stale).is_err());
    let revision = f.data.revision;
    f.data.campaigns[0].reason = "x".repeat(store::MAX_JSON);
    assert!(store::save(&f.dir, &mut f.data).is_err());
    assert_eq!(f.data.revision, revision);
    let target = f.root.join("occupied.json");
    fs::create_dir(&target).unwrap();
    assert!(store::atomic(&target, &json!({"x":1})).is_err());
    assert!(!fs::read_dir(&f.root).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with("campaign-tmp")));
}

#[test]
fn compact_poll_summary_has_fixed_size_no_evidence_arrays_and_correct_owners() {
    let r = Runtime::new(true);
    let mut data = r.f.data.clone();
    let mut c = data.campaigns[0].clone();
    c.plan.question = "问".repeat(1300);
    c.reason = "错".repeat(1300);
    c.paused = true;
    data.campaigns = (0..32)
        .map(|i| {
            let mut c = c.clone();
            c.plan.id = format!("campaign-{i}");
            c
        })
        .collect();
    let value = presentation::compact(&r.config, &r.f.dir, &data, &[]);
    assert_eq!(value["total"], 32);
    assert_eq!(value["items"].as_array().unwrap().len(), 4);
    assert!(serde_json::to_vec(&value).unwrap().len() < 12 * 1024);
    assert!(value["items"][0].get("evidence_ids").is_none());
    assert_eq!(value["items"][0]["next_owner"], "user");
    data.campaigns[0].state = "awaiting_web_review".into();
    let value = presentation::compact(&r.config, &r.f.dir, &data, &[]);
    assert_eq!(value["items"][0]["next_owner"], "chatgpt");
}

#[test]
fn documented_plan_event_and_hash_are_exactly_reproducible() {
    let doc = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/AUTO_RESEARCH_CAMPAIGNS.md"),
    )
    .unwrap()
    .replace("\r\n", "\n");
    let event: Value = serde_json::from_str(
        doc.split("```json\n")
            .nth(1)
            .unwrap()
            .split("```")
            .next()
            .unwrap(),
    )
    .unwrap();
    let plan: model::Plan = serde_json::from_value(event["plan"].clone()).unwrap();
    store::validate(&plan).unwrap();
    let hash = store::plan_hash(&plan).unwrap();
    assert_eq!(
        hash,
        "76fb42a137a13241426a8fec41d2cb8a23e79c19c43e2294101f9a08c490c6a2"
    );
    assert!(doc.contains(&format!("The resulting plan SHA256 is `{hash}`")));
}

#[test]
fn store_refuses_mutated_authorized_plan_even_with_recomputed_hash() {
    let mut f = Fixture::new();
    f.authorize();
    f.tick();
    let original = f.data.clone();
    f.data.campaigns[0].plan.question = "A different authorized question".into();
    f.data.campaigns[0].plan_sha256 = store::plan_hash(&f.c().plan).unwrap();
    assert!(store::save(&f.dir, &mut f.data).is_err());
    assert_eq!(store::load(&f.dir).unwrap(), original);
}

#[test]
fn manual_dispatch_rejects_expired_plan_without_reserving_run() {
    let mut r = Runtime::new(true);
    let mut expired = r.f.data.clone();
    expired.campaigns[0].plan.deadline = "2000-01-01T00:00:00Z".into();
    expired.campaigns[0].plan_sha256 = store::plan_hash(&expired.campaigns[0].plan).unwrap();
    // An isolated historical queue fixture, equivalent to a queued task surviving its deadline.
    engine::reconcile(&r.f.root, &r.f.dir, &mut expired.campaigns[0], &[], 0).unwrap();
    let task = engine::task(
        &expired.campaigns[0],
        &expired.campaigns[0].attempts[0],
        "1999-01-01T00:00:00Z",
    )
    .unwrap();
    store::atomic(&r.f.dir.join("campaigns.json"), &expired).unwrap();
    store::atomic(&r.f.dir.join(TASKS_FILE), &vec![task.clone()]).unwrap();
    assert!(allowed(&r.state, "fixture", &task).is_err());
    assert!(reserve(&r.state, "fixture", &task, "RUN-TOO-LATE").is_err());
    r.tick();
    assert_eq!(r.f.c().state, "awaiting_web_review");
    assert!(r.f.c().attempts[0].run_id.is_none());
}

#[test]
fn two_projects_each_reconcile_both_campaigns_without_round_robin_starvation() {
    let mut r = Runtime::new(true);
    let other = Fixture::new();
    let mut second = r.f.c().clone();
    second.plan.id = "second".into();
    second.plan_sha256 = store::plan_hash(&second.plan).unwrap();
    r.f.data.campaigns.push(second);
    store::save(&r.f.dir, &mut r.f.data).unwrap();
    let other_dir = project_dir(&r.state, "other").unwrap();
    fs::create_dir_all(&other_dir).unwrap();
    let mut config = r.config.clone();
    config.id = "other".into();
    config.local_root = other.root.to_string_lossy().into();
    config.repo_root = config.local_root.clone();
    store::atomic(&other_dir.join(CONFIG_FILE), &config).unwrap();
    store::atomic(&other_dir.join(TASKS_FILE), &Vec::<ProjectTask>::new()).unwrap();
    store::atomic(&other_dir.join(RUNS_FILE), &Vec::<AgentRun>::new()).unwrap();
    let mut data = r.f.data.clone();
    data.revision = 0;
    store::save(&other_dir, &mut data).unwrap();
    for _ in 0..2 {
        reconcile_once(&r.state, &["fixture".into(), "other".into()]).unwrap();
    }
    for dir in [&r.f.dir, &other_dir] {
        let data = store::load(dir).unwrap();
        assert!(data.campaigns.iter().all(|c| c.attempts.len() == 1));
        assert_eq!(
            store::read::<Vec<ProjectTask>>(&dir.join(TASKS_FILE))
                .unwrap()
                .len(),
            2
        );
    }
}
