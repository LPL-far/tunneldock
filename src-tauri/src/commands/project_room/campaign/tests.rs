use super::*;
use model::{FrozenFile, Gate, Metric, Plan, Stage};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct Fixture {
    pub(super) root: PathBuf,
    pub(super) dir: PathBuf,
    pub(super) data: Store,
    pub(super) runs: Vec<AgentRun>,
}
impl Fixture {
    pub(super) fn new() -> Self {
        static ID: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "tunneldock-campaign-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let dir = root.join("control");
        fs::create_dir(&dir).unwrap();
        let mut provenance = Vec::new();
        for role in [
            "baseline",
            "code",
            "config",
            "dataset",
            "evaluator",
            "reference",
        ] {
            let path = format!("{role}.json");
            let bytes = if role == "reference" {
                br#"{"a":1.0,"b":2.0,"c":3.0}"#.to_vec()
            } else {
                b"frozen".to_vec()
            };
            fs::write(root.join(&path), &bytes).unwrap();
            provenance.push(FrozenFile {
                role: role.into(),
                path,
                sha256: store::digest(&bytes),
            });
        }
        let stage = Stage {
            id: "baseline-check".into(),
            instruction: "Measure fixture".into(),
            intervention_scope: "results/first".into(),
            gate: Gate {
                metric: Metric::Mean,
                minimum: 2.0,
                reference_path: "reference.json".into(),
                sample_count: 3,
            },
            max_attempts: 2,
            on_pass: Some(1),
            on_scientific_fail: None,
            on_unknown: "awaiting_web_review".into(),
        };
        let mut second = stage.clone();
        second.id = "confirm".into();
        second.on_pass = None;
        second.intervention_scope = "results/second".into();
        let plan = Plan {
            version: 1,
            id: "fixture".into(),
            question: "Can the gate be reproduced?".into(),
            hypothesis: "Mean >= 2".into(),
            provenance,
            stages: vec![stage, second],
            max_submissions: 3,
            deadline: "2030-01-01T00:00:00Z".into(),
            retry_confirmed_operational_failure: true,
        };
        let mut data = Store::default();
        events::apply(
            &mut data,
            &json!({"kind":"campaign.draft","plan":plan}),
            false,
        )
        .unwrap();
        Self {
            root,
            dir,
            data,
            runs: Vec::new(),
        }
    }
    pub(super) fn c(&self) -> &Campaign {
        &self.data.campaigns[0]
    }
    pub(super) fn authorize(&mut self) {
        let v = json!({"kind":"campaign.authorize","campaign_id":"fixture","plan_sha256":self.c().plan_sha256,"authorization_ref":"human-approval-17"});
        events::apply(&mut self.data, &v, true).unwrap();
    }
    pub(super) fn tick(&mut self) {
        engine::reconcile(
            &self.root,
            &self.dir,
            &mut self.data.campaigns[0],
            &self.runs,
            1_800_000_000,
        )
        .unwrap();
        store::save(&self.dir, &mut self.data).unwrap();
    }
    pub(super) fn complete(&mut self, values: [f64; 3]) {
        let index = self.c().attempts.len();
        let a = self.data.campaigns[0].attempts.last_mut().unwrap();
        let run_id = format!("RUN-FIXTURE-{index}");
        a.run_id = Some(run_id.clone());
        a.terminal_confirmation = Some("success".into());
        let run_root = self.root.join(".tunneldock/runs").join(&run_id);
        fs::create_dir_all(&run_root).unwrap();
        let output = run_root.join("HANDOFF.md");
        fs::write(&output, "Transport-confirmed fixture handoff").unwrap();
        self.runs.push(AgentRun {
            id: run_id.clone(),
            task_id: a.task_id.clone(),
            agent_id: "codex".into(),
            status: "completed".into(),
            pid: None,
            external_session_id: None,
            start_step: None,
            started_at: "2027-01-01T00:00:00Z".into(),
            finished_at: Some("2027-01-01T00:01:00Z".into()),
            prompt_path: String::new(),
            output_path: output.to_string_lossy().into(),
            log_path: String::new(),
            error_path: String::new(),
            error_message: None,
        });
        let c = self.c();
        let a = c.attempts.last().unwrap();
        let artifact = json!({"version":1,"campaign_id":c.plan.id,"plan_sha256":c.plan_sha256,"stage_id":c.plan.stages[a.stage].id,
            "task_id":a.task_id,"run_id":run_id,"samples":[{"id":"a","value":values[0],"target":1.0},{"id":"b","value":values[1],"target":2.0},{"id":"c","value":values[2],"target":3.0}]});
        store::atomic(&run_root.join("EVIDENCE.json"), &artifact).unwrap();
    }
    pub(super) fn artifact(&self) -> PathBuf {
        Path::new(&self.runs.last().unwrap().output_path).with_file_name("EVIDENCE.json")
    }
    pub(super) fn edit_artifact(&self, edit: impl FnOnce(&mut Value)) {
        let path = self.artifact();
        let mut v: Value = store::read(&path).unwrap();
        edit(&mut v);
        store::atomic(&path, &v).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("Fixture cleanup");
    }
}

#[test]
fn draft_never_creates_tasks() {
    let mut f = Fixture::new();
    f.tick();
    assert!(f.c().attempts.is_empty());
    assert_eq!(f.c().state, "draft");
}
#[test]
fn authorization_is_exact_separate_and_immutable() {
    let mut f = Fixture::new();
    let hash = f.c().plan_sha256.clone();
    for (digest, reference, human) in [
        ("wrong", "ref", true),
        (hash.as_str(), "", true),
        (hash.as_str(), "worker-report", false),
    ] {
        assert!(events::apply(&mut f.data,&json!({"kind":"campaign.authorize","campaign_id":"fixture","plan_sha256":digest,"authorization_ref":reference}),human).is_err());
    }
    f.authorize();
    f.authorize();
    assert!(f.c().attempts.is_empty());
    let mut changed = f.c().plan.clone();
    changed.question = "changed".into();
    assert!(events::apply(
        &mut f.data,
        &json!({"kind":"campaign.draft","plan":changed}),
        false
    )
    .is_err());
    let same = f.c().plan.clone();
    events::apply(
        &mut f.data,
        &json!({"kind":"campaign.draft","plan":same}),
        false,
    )
    .unwrap();
}
#[test]
fn real_files_pass_next_restart_and_review_separation() {
    let mut f = Fixture::new();
    f.authorize();
    f.tick();
    f.complete([1.0, 2.0, 3.0]);
    f.tick();
    assert_eq!(f.c().stage, 1);
    assert_eq!(f.c().attempts.len(), 2);
    assert_eq!(f.c().attempts[0].gate, Some(true));
    f.data = store::load(&f.dir).unwrap();
    for _ in 0..4 {
        f.tick();
    }
    assert_eq!(f.c().attempts.len(), 2);
    f.complete([2.0, 3.0, 4.0]);
    f.tick();
    assert_eq!(f.c().state, "awaiting_web_review");
    assert!(!f.c().web_reviewed);
    let review = json!({"kind":"campaign.reviewed","author":"chatgpt","campaign_id":"fixture","plan_sha256":f.c().plan_sha256,
        "plan_version":1,"evidence_ids":f.c().attempts.iter().filter_map(|a|a.evidence_id.clone()).collect::<Vec<_>>(),"review_ref":"web-review-1","accepted":true});
    let mut wrong = review.clone();
    wrong["evidence_ids"] = json!([]);
    assert!(events::apply(&mut f.data, &wrong, false).is_err());
    events::apply(&mut f.data, &review, false).unwrap();
    assert!(f.c().accepted && f.c().web_reviewed);
}
#[test]
fn valid_scientific_fail_stops_without_retry() {
    let mut f = Fixture::new();
    f.authorize();
    f.tick();
    f.complete([0.0, 0.0, 0.0]);
    f.tick();
    f.tick();
    assert_eq!(f.c().attempts.len(), 1);
    assert_eq!(f.c().attempts[0].gate, Some(false));
    assert_eq!(f.c().state, "awaiting_web_review");
    let mut review = json!({"kind":"campaign.reviewed","author":"chatgpt","campaign_id":"fixture","plan_sha256":f.c().plan_sha256,
        "plan_version":1,"evidence_ids":[f.c().attempts[0].evidence_id],"review_ref":"web-negative","accepted":true});
    assert!(events::apply(&mut f.data, &review, false).is_err());
    review["accepted"] = json!(false);
    events::apply(&mut f.data, &review, false).unwrap();
    assert_eq!(f.c().attempts[0].gate, Some(false));
    assert!(!f.c().accepted);
}
#[test]
fn negative_can_only_follow_predeclared_diagnostic() {
    let mut f = Fixture::new();
    f.data.campaigns[0].plan.stages[0].on_scientific_fail = Some(1);
    f.data.campaigns[0].plan_sha256 = store::plan_hash(&f.c().plan).unwrap();
    f.authorize();
    f.tick();
    f.complete([0.0; 3]);
    f.tick();
    assert_eq!(f.c().stage, 1);
    assert_eq!(f.c().attempts.len(), 2);
    assert_eq!(f.c().attempts[0].gate, Some(false));
}
#[test]
fn independent_raw_metrics_include_ties() {
    assert_eq!(
        evidence::recompute(&Metric::Mean, &[1.0, 2.0, 3.0], &[0.0; 3]).unwrap(),
        2.0
    );
    assert!(
        (evidence::recompute(&Metric::Pearson, &[1.0, 2.0, 3.0], &[3.0, 2.0, 1.0]).unwrap() + 1.0)
            .abs()
            < 1e-12
    );
    assert!(
        (evidence::recompute(&Metric::Spearman, &[10.0, 10.0, 30.0], &[1.0, 1.0, 2.0]).unwrap()
            - 1.0)
            .abs()
            < 1e-12
    );
    for x in [
        vec![f64::NAN, 1.0],
        vec![f64::INFINITY, 1.0],
        vec![1.0, 1.0],
    ] {
        assert!(evidence::recompute(&Metric::Pearson, &x, &[1.0, 2.0]).is_err());
    }
}
#[test]
fn invalid_artifacts_never_become_scientific_fail() {
    for mutation in 0..9 {
        let mut f = Fixture::new();
        f.authorize();
        f.tick();
        f.complete([2.0; 3]);
        f.edit_artifact(|v| match mutation {
            0 => v["run_id"] = json!("old-run"),
            1 => v["task_id"] = json!("old-task"),
            2 => v["stage_id"] = json!("wrong"),
            3 => v["campaign_id"] = json!("wrong"),
            4 => v["plan_sha256"] = json!("wrong"),
            5 => v["samples"][0]["target"] = json!(9.0),
            6 => v["samples"][0]["id"] = json!("b"),
            7 => v["samples"] = json!([]),
            _ => v["samples"][0]["value"] = json!("NaN"),
        });
        f.tick();
        assert_eq!(f.c().state, "awaiting_web_review");
        assert_eq!(f.c().attempts[0].evidence, "evidence_invalid");
        assert_eq!(f.c().attempts[0].gate, None);
    }
}
#[test]
fn missing_corrupt_oversized_evidence_and_source_drift() {
    for mutation in 0..4 {
        let mut f = Fixture::new();
        f.authorize();
        f.tick();
        f.complete([2.0; 3]);
        match mutation {
            0 => fs::remove_file(f.artifact()).unwrap(),
            1 => fs::write(f.artifact(), "{").unwrap(),
            2 => fs::write(f.artifact(), vec![b'x'; store::MAX_JSON + 1]).unwrap(),
            _ => fs::write(f.root.join("evaluator.json"), "drift").unwrap(),
        };
        f.tick();
        assert_eq!(f.c().attempts[0].evidence, "evidence_invalid");
        assert_eq!(f.c().attempts[0].gate, None);
    }
}
#[test]
fn footer_and_old_run_cannot_certify_new_attempt() {
    let mut f = Fixture::new();
    f.authorize();
    f.tick();
    f.complete([2.0; 3]);
    f.data.campaigns[0].attempts[0].terminal_confirmation = None;
    f.tick();
    assert_eq!(f.c().attempts[0].gate, None);
    let mut f = Fixture::new();
    f.authorize();
    f.tick();
    f.complete([2.0; 3]);
    f.tick();
    let old = f.runs[0].id.clone();
    f.data.campaigns[0].attempts[1].run_id = Some(old);
    engine::reconcile(
        &f.root,
        &f.dir,
        &mut f.data.campaigns[0],
        &f.runs,
        1_800_000_000,
    )
    .unwrap();
    assert!(
        store::save(&f.dir, &mut f.data).is_err(),
        "Duplicate historical run reservation must not persist"
    );
    assert_eq!(f.c().attempts[1].gate, None);
    assert_eq!(f.c().attempts.len(), 2);
}
#[test]
fn pause_deadline_and_total_budget_close_dispatch() {
    let mut f = Fixture::new();
    f.authorize();
    f.tick();
    assert!(engine::dispatch_allowed(f.c(), &f.c().attempts[0], 1_800_000_000).is_ok());
    f.data.campaigns[0].paused = true;
    f.tick();
    assert!(engine::dispatch_allowed(f.c(), &f.c().attempts[0], 1_800_000_000).is_err());
    f.data.campaigns[0].paused = false;
    assert!(engine::dispatch_allowed(f.c(), &f.c().attempts[0], 2_000_000_000).is_err());
    engine::reconcile(
        &f.root,
        &f.dir,
        &mut f.data.campaigns[0],
        &f.runs,
        2_000_000_000,
    )
    .unwrap();
    assert_eq!(f.c().state, "awaiting_web_review");
    let mut f = Fixture::new();
    f.data.campaigns[0].plan.max_submissions = 1;
    f.data.campaigns[0].plan_sha256 = store::plan_hash(&f.c().plan).unwrap();
    f.authorize();
    f.tick();
    f.complete([2.0; 3]);
    f.tick();
    assert_eq!(f.c().attempts.len(), 1);
    assert!(f.c().reason.contains("budget"));
}
#[test]
fn reserved_uncertain_or_running_submission_never_replays() {
    let mut f = Fixture::new();
    f.authorize();
    f.tick();
    f.data.campaigns[0].attempts[0].run_id = Some("RUN-uncertain".into());
    f.data.campaigns[0].attempts[0].execution = "submission_uncertain".into();
    store::save(&f.dir, &mut f.data).unwrap();
    f.data = store::load(&f.dir).unwrap();
    f.tick();
    f.tick();
    assert_eq!(f.c().attempts.len(), 1);
    assert!(engine::dispatch_allowed(f.c(), &f.c().attempts[0], 1_800_000_000).is_err());
    let mut f = Fixture::new();
    f.authorize();
    f.tick();
    f.complete([2.0; 3]);
    f.runs[0].status = "running".into();
    f.runs[0].finished_at = None;
    f.data.campaigns[0].paused = true;
    for _ in 0..3 {
        f.tick();
    }
    assert_eq!(f.c().attempts.len(), 1);
    assert_eq!(f.c().attempts[0].gate, None);
}
#[test]
fn retries_require_confirmed_failure_and_are_finite() {
    for confirmed in [false, true] {
        let mut f = Fixture::new();
        f.authorize();
        f.tick();
        f.complete([2.0; 3]);
        f.runs[0].status = "failed".into();
        f.data.campaigns[0].attempts[0].terminal_confirmation = confirmed.then(|| "failure".into());
        f.tick();
        assert_eq!(f.c().attempts.len(), if confirmed { 2 } else { 1 });
        if confirmed {
            f.complete([2.0; 3]);
            f.runs[1].status = "failed".into();
            f.data.campaigns[0].attempts[1].terminal_confirmation = Some("failure".into());
            f.tick();
            f.tick();
            assert_eq!(f.c().attempts.len(), 2);
            assert_eq!(f.c().state, "awaiting_web_review");
        }
    }
}
#[test]
fn structural_paths_cycles_and_overlaps_rejected() {
    let f = Fixture::new();
    for path in [
        "../outside",
        "/absolute",
        "C:/escape",
        "a\\b",
        ".tunneldock",
        "evaluator.json",
        "code.json/child",
        "results/../code.json",
    ] {
        let mut p = f.c().plan.clone();
        p.stages[0].intervention_scope = path.into();
        assert!(store::validate(&p).is_err(), "{path}");
    }
    let mut p = f.c().plan.clone();
    p.stages[1].on_pass = Some(0);
    assert!(store::validate(&p).is_err());
}
#[test]
fn linked_paths_are_rejected() {
    let f = Fixture::new();
    let target = f.root.join("target");
    fs::create_dir(&target).unwrap();
    let link = f.root.join("linked");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let result = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(result.status.success(), "junction fixture failed");
    }
    assert!(store::safe_path(&f.root, "linked/file.json").is_err());
}
#[test]
fn disk_corruption_is_visible_and_atomic_restart_ignores_temp() {
    let mut f = Fixture::new();
    f.authorize();
    f.tick();
    fs::write(f.dir.join("campaigns.campaign-tmp"), "partial").unwrap();
    assert_eq!(store::load(&f.dir).unwrap(), f.data);
    fs::write(f.dir.join("campaigns.json"), "{").unwrap();
    assert!(store::load(&f.dir).is_err());
}

#[test]
fn production_reconciliation_recovers_task_link_and_reservation_without_dispatch() {
    let mut f = Fixture::new();
    let state = AppState::isolated_for_tests(f.root.join("runtime"));
    let dir = project_dir(&state, "fixture").unwrap();
    fs::create_dir_all(&dir).unwrap();
    let config = ProjectRoomConfig {
        id: "fixture".into(),
        name: "Inert fixture".into(),
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
    store::atomic(&dir.join(CONFIG_FILE), &config).unwrap();
    store::atomic(&dir.join(TASKS_FILE), &Vec::<ProjectTask>::new()).unwrap();
    store::atomic(&dir.join(RUNS_FILE), &Vec::<AgentRun>::new()).unwrap();
    process_event(
        &state,
        "fixture",
        &json!({"kind":"campaign.draft","plan":f.c().plan}),
        false,
    )
    .unwrap();
    reconcile_project(&state, "fixture").unwrap();
    assert!(store::read::<Vec<ProjectTask>>(&dir.join(TASKS_FILE))
        .unwrap()
        .is_empty());
    let authorization = json!({"kind":"campaign.authorize","campaign_id":"fixture","plan_sha256":f.c().plan_sha256,"authorization_ref":"human-fixture"});
    assert!(process_event(&state, "fixture", &authorization, false).is_err());
    process_event(&state, "fixture", &authorization, true).unwrap();
    reconcile_project(&state, "fixture").unwrap();
    let tasks: Vec<ProjectTask> = store::read(&dir.join(TASKS_FILE)).unwrap();
    assert_eq!(tasks.len(), 1);
    // Simulate a restart after durable campaign reservation but before tasks.json commit.
    store::atomic(&dir.join(TASKS_FILE), &Vec::<ProjectTask>::new()).unwrap();
    let restarted = AppState::isolated_for_tests(state.app_data_dir.clone());
    for _ in 0..3 {
        reconcile_project(&restarted, "fixture").unwrap();
    }
    let repaired: Vec<ProjectTask> = store::read(&dir.join(TASKS_FILE)).unwrap();
    assert_eq!(repaired.len(), 1);
    assert_eq!(repaired[0].id, tasks[0].id);
    reserve(&restarted, "fixture", &repaired[0], "RUN-FIXTURE-1").unwrap();
    assert!(reserve(&restarted, "fixture", &repaired[0], "RUN-duplicate").is_err());
    // Persist the actual fixture result and the independent terminal transport confirmation.
    f.data = store::load(&dir).unwrap();
    f.complete([1.0, 2.0, 3.0]);
    store::atomic(&dir.join(RUNS_FILE), &f.runs).unwrap();
    confirm_terminal(&dir, &f.runs[0], true).unwrap();
    reconcile_project(&restarted, "fixture").unwrap();
    let after = store::load(&dir).unwrap();
    assert_eq!(after.campaigns[0].attempts.len(), 2);
    assert_eq!(after.campaigns[0].attempts[0].gate, Some(true));
    let second: Vec<ProjectTask> = store::read(&dir.join(TASKS_FILE)).unwrap();
    assert_eq!(second.len(), 2);
    let c = &after.campaigns[0];
    process_event(
        &restarted,
        "fixture",
        &json!({"kind":"campaign.pause","campaign_id":"fixture","plan_sha256":c.plan_sha256}),
        true,
    )
    .unwrap();
    assert!(allowed(&restarted, "fixture", &second[1]).is_err());
    reconcile_project(&restarted, "fixture").unwrap();
    let paused: Vec<ProjectTask> = store::read(&dir.join(TASKS_FILE)).unwrap();
    assert!(!paused[1].auto_dispatch);
    let summary = summary(&config);
    assert_eq!(summary["items"][0]["paused"], true);
    assert_eq!(summary["items"][0]["web_reviewed"], false);
}
