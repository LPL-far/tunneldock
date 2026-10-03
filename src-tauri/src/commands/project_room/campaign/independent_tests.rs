//! Independent adversarial checks: fixtures only, no real Project Room or agent calls.
use super::{evidence, model::*, store};
use crate::models::AgentRun;
use serde_json::json;
use std::{fs, path::PathBuf, sync::atomic::{AtomicU64, Ordering}, time::{SystemTime, UNIX_EPOCH}};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture { root: PathBuf, campaign: Campaign, attempt: Attempt, run: AgentRun }
impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("td-independent-campaign-{}-{}-{}", std::process::id(), nonce, NEXT.fetch_add(1, Ordering::SeqCst)));
        fs::create_dir_all(root.join("frozen")).unwrap();
        let mut provenance = Vec::new();
        for role in ["baseline", "code", "config", "evaluator"] {
            let path = format!("frozen/{role}.txt");
            fs::write(root.join(&path), role.as_bytes()).unwrap();
            provenance.push(FrozenFile { role: role.into(), path, sha256: store::digest(role.as_bytes()) });
        }
        let targets = br#"{"a":1.0,"b":2.0,"c":3.0}"#;
        fs::write(root.join("frozen/targets.json"), targets).unwrap();
        provenance.push(FrozenFile { role: "dataset".into(), path: "frozen/targets.json".into(), sha256: store::digest(targets) });
        let plan = Plan {
            version: 1, id: "fixture-campaign".into(), question: "Is the numeric gate reproducible?".into(),
            hypothesis: "A frozen threshold can be checked from raw values.".into(), provenance,
            stages: vec![Stage { id:"baseline".into(), instruction:"Fixture evidence, no inference".into(), intervention_scope:"candidate".into(),
                gate:Gate {metric:Metric::Mean, minimum:0.5, reference_path:"frozen/targets.json".into(), sample_count:3},
                max_attempts:1,on_pass:None,on_scientific_fail:None,on_unknown:"awaiting_web_review".into() }],
            max_submissions:1, deadline:"2099-01-01T00:00:00Z".into(),retry_confirmed_operational_failure:false,
        };
        store::validate(&plan).expect("valid independent fixture");
        let campaign = Campaign { plan_sha256:store::plan_hash(&plan).unwrap(),plan,authorization_ref:Some("isolated-test-only".into()),
            paused:false,state:"active".into(),stage:0,attempts:vec![],reason:String::new(),web_reviewed:false,accepted:false,review_ref:None };
        let attempt = Attempt { stage:0,number:1,task_id:"CAMPAIGN-fixture-campaign-0-1".into(),run_id:Some("RUN-CODEX-FIXTURE".into()),
            terminal_confirmation:Some("success".into()),execution:"completed".into(),evidence:"pending".into(),gate:None,metric:None,evidence_id:None,reason:String::new() };
        let dir = root.join(".tunneldock/runs/RUN-CODEX-FIXTURE");
        fs::create_dir_all(&dir).unwrap();fs::write(dir.join("HANDOFF.md"), "Operation reported complete; scientific gate is separately verified.").unwrap();
        let run = AgentRun { id:"RUN-CODEX-FIXTURE".into(),task_id:attempt.task_id.clone(),agent_id:"codex".into(),status:"completed".into(),pid:None,
            external_session_id:Some("fixture-thread".into()),start_step:Some(0),started_at:"2026-10-03T00:00:00Z".into(),finished_at:Some("2026-10-03T00:00:01Z".into()),
            prompt_path:dir.join("prompt.md").to_string_lossy().into(),output_path:dir.join("HANDOFF.md").to_string_lossy().into(),
            log_path:dir.join("stdout.log").to_string_lossy().into(),error_path:dir.join("stderr.log").to_string_lossy().into(),error_message:None };
        Self {root,campaign,attempt,run}
    }
    fn packet(&self, values:[f64;3]) -> serde_json::Value {
        json!({"version":1,"campaign_id":self.campaign.plan.id,"plan_sha256":self.campaign.plan_sha256,
            "stage_id":"baseline","task_id":self.attempt.task_id,"run_id":self.run.id,
            "samples":[{"id":"a","value":values[0],"target":1.0},{"id":"b","value":values[1],"target":2.0},{"id":"c","value":values[2],"target":3.0}]})
    }
    fn write(&self, data:&serde_json::Value) {
        fs::write(self.root.join(".tunneldock/runs/RUN-CODEX-FIXTURE/EVIDENCE.json"),serde_json::to_vec(data).unwrap()).unwrap();
    }
    fn verified(&self) -> Result<evidence::Verified,String> {
        evidence::verify(&self.root,&self.campaign,&self.attempt,std::slice::from_ref(&self.run))
    }
}
impl Drop for Fixture {fn drop(&mut self) {if let Err(e)=fs::remove_dir_all(&self.root) {eprintln!("independent fixture cleanup {}: {e}",self.root.display());}}}

#[test]
fn independent_metrics_handle_ties_negative_correlation_and_nonfinite_inputs() {
    let pearson=evidence::recompute(&Metric::Pearson,&[1.0,2.0,3.0,4.0],&[4.0,3.0,2.0,1.0]).unwrap();
    assert!((pearson+1.0).abs()<1e-12);
    let spearman=evidence::recompute(&Metric::Spearman,&[1.0,1.0,3.0,4.0],&[4.0,4.0,2.0,1.0]).unwrap();
    assert!((spearman+1.0).abs()<1e-12);
    for metric in [Metric::Pearson,Metric::Spearman] {assert!(evidence::recompute(&metric,&[1.0,1.0],&[2.0,3.0]).is_err());}
    assert!(evidence::recompute(&Metric::Mean,&[f64::NAN],&[1.0]).is_err());
    assert!(evidence::recompute(&Metric::Mean,&[f64::INFINITY],&[1.0]).is_err());
    assert!(evidence::recompute(&Metric::Mean,&[],&[]).is_err());
}
#[test]
fn independent_valid_negative_result_is_not_corrupt_evidence() {
    let f=Fixture::new();f.write(&f.packet([0.1,0.2,0.3]));let verified=f.verified().expect("negative result remains valid evidence");
    assert!(!verified.passed);assert!((verified.metric-0.2).abs()<1e-12);assert_eq!(verified.evidence_id.len(),64);
    assert!(!f.campaign.web_reviewed);assert!(!f.campaign.accepted);
}
#[test]
fn independent_unknown_and_duplicate_candidate_ids_reject() {
    let f=Fixture::new();let mut p=f.packet([1.0,2.0,3.0]);p["samples"][1]["id"]=json!("a");p["samples"][1]["target"]=json!(1.0);f.write(&p);assert!(f.verified().is_err());
    p=f.packet([1.0,2.0,3.0]);p["samples"][2]["id"]=json!("unseen");f.write(&p);assert!(f.verified().is_err());
}
#[test]
fn independent_missing_samples_and_modified_targets_reject() {
    let f=Fixture::new();let mut p=f.packet([1.0,2.0,3.0]);p["samples"].as_array_mut().unwrap().pop();f.write(&p);assert!(f.verified().is_err());
    p=f.packet([1.0,2.0,3.0]);p["samples"][0]["target"]=json!(9.0);f.write(&p);assert!(f.verified().is_err());
}
#[test]
fn independent_old_receipt_cannot_certify_current_run() {
    let f=Fixture::new();let mut p=f.packet([1.0,2.0,3.0]);p["run_id"]=json!("RUN-PREVIOUS");f.write(&p);assert!(f.verified().is_err());
    p=f.packet([1.0,2.0,3.0]);p["plan_sha256"]=json!("a".repeat(64));f.write(&p);assert!(f.verified().is_err());
}
#[test]
fn independent_second_run_makes_attempt_ambiguous() {
    let f=Fixture::new();f.write(&f.packet([1.0,2.0,3.0]));let mut another=f.run.clone();another.id="RUN-OTHER".into();
    assert!(evidence::verify(&f.root,&f.campaign,&f.attempt,&[f.run.clone(),another]).is_err());
}
#[test]
fn independent_frozen_source_drift_and_missing_handoff_reject() {
    let f=Fixture::new();f.write(&f.packet([1.0,2.0,3.0]));fs::write(f.root.join("frozen/evaluator.txt"),"changed evaluator").unwrap();assert!(f.verified().is_err());
    fs::write(f.root.join("frozen/evaluator.txt"),"evaluator").unwrap();fs::write(&f.run.output_path,"").unwrap();assert!(f.verified().is_err());
}
#[test]
fn independent_success_requires_terminal_confirmation_not_just_worker_text() {
    let mut f=Fixture::new();f.write(&f.packet([1.0,2.0,3.0]));f.attempt.terminal_confirmation=None;assert!(f.verified().is_err());
    f.attempt.terminal_confirmation=Some("success".into());f.run.status="running".into();assert!(f.verified().is_err());
}
#[test]
fn independent_reference_duplicate_keys_reject_even_with_matching_file_hash() {
    let mut f=Fixture::new();let bytes=br#"{"a":9.0,"a":1.0,"b":2.0,"c":3.0}"#;
    fs::write(f.root.join("frozen/targets.json"),bytes).unwrap();
    f.campaign.plan.provenance.iter_mut().find(|p|p.role=="dataset").unwrap().sha256=store::digest(bytes);
    f.campaign.plan_sha256=store::plan_hash(&f.campaign.plan).unwrap();f.write(&f.packet([1.0,2.0,3.0]));
    assert!(f.verified().is_err(),"duplicate frozen target keys must not be silently overwritten");
}
#[test]
fn independent_stage_ids_cannot_escape_or_smuggle_task_identity() {
    let f=Fixture::new();
    for id in ["../other","a/b","a\\b","a:b","with space","a\0b"] {
        let mut p=f.campaign.plan.clone();p.stages[0].id=id.into();assert!(store::validate(&p).is_err(),"unsafe stage id {id:?}");
    }
}
#[test]
fn independent_protected_inputs_and_backward_transitions_are_not_writable() {
    let f=Fixture::new();let mut p=f.campaign.plan.clone();p.stages[0].intervention_scope="frozen".into();assert!(store::validate(&p).is_err());
    p=f.campaign.plan.clone();p.stages[0].on_pass=Some(0);assert!(store::validate(&p).is_err());
    for scope in [".project_memory",".tunneldock","../out","frozen/targets.json"] {
        p=f.campaign.plan.clone();p.stages[0].intervention_scope=scope.into();assert!(store::validate(&p).is_err());
    }
}

#[test]
fn independent_draft_and_pause_block_dispatch_not_just_ui() {
    let f=Fixture::new();let now=1790985600;
    let mut c=f.campaign.clone();c.authorization_ref=None;c.state="draft".into();
    super::engine::reconcile(&f.root,&f.root.join("control"),&mut c,&[],now).unwrap();assert!(c.attempts.is_empty());
    c=f.campaign.clone();c.paused=true;
    super::engine::reconcile(&f.root,&f.root.join("control"),&mut c,&[],now).unwrap();assert!(c.attempts.is_empty());
    c.paused=false;super::engine::reconcile(&f.root,&f.root.join("control"),&mut c,&[],now).unwrap();assert_eq!(c.attempts.len(),1);
    super::engine::dispatch_allowed(&c,&c.attempts[0],now).unwrap();
    c.paused=true;assert!(super::engine::dispatch_allowed(&c,&c.attempts[0],now).is_err());
}
#[test]
fn independent_pass_advances_without_web_and_repeated_ticks_do_not_duplicate() {
    let mut f=Fixture::new();let mut second=f.campaign.plan.stages[0].clone();second.id="replication".into();
    f.campaign.plan.stages[0].on_pass=Some(1);f.campaign.plan.stages.push(second);f.campaign.plan.max_submissions=2;
    f.campaign.plan_sha256=store::plan_hash(&f.campaign.plan).unwrap();f.campaign.state="active".into();
    f.campaign.attempts=vec![f.attempt.clone()];f.write(&f.packet([1.0,2.0,3.0]));
    super::engine::reconcile(&f.root,&f.root.join("control"),&mut f.campaign,std::slice::from_ref(&f.run),1790985600).unwrap();
    assert_eq!(f.campaign.stage,1);assert_eq!(f.campaign.attempts.len(),2);assert_eq!(f.campaign.attempts[0].gate,Some(true));
    assert_eq!(f.campaign.attempts[1].execution,"queued");assert!(!f.campaign.web_reviewed);
    let task=f.campaign.attempts[1].task_id.clone();
    for _ in 0..4 {super::engine::reconcile(&f.root,&f.root.join("control"),&mut f.campaign,std::slice::from_ref(&f.run),1790985601).unwrap();}
    assert_eq!(f.campaign.attempts.len(),2);assert_eq!(f.campaign.attempts[1].task_id,task);
}
#[test]
fn independent_valid_scientific_failure_is_never_operational_retry() {
    let mut f=Fixture::new();f.campaign.plan.retry_confirmed_operational_failure=true;
    f.campaign.plan.max_submissions=3;f.campaign.plan.stages[0].max_attempts=3;
    f.campaign.plan_sha256=store::plan_hash(&f.campaign.plan).unwrap();f.campaign.state="active".into();
    f.campaign.attempts=vec![f.attempt.clone()];f.write(&f.packet([0.1,0.2,0.3]));
    super::engine::reconcile(&f.root,&f.root.join("control"),&mut f.campaign,std::slice::from_ref(&f.run),1790985600).unwrap();
    assert_eq!(f.campaign.state,"awaiting_web_review");assert_eq!(f.campaign.attempts.len(),1);
    assert_eq!(f.campaign.attempts[0].evidence,"evidence_verified");assert_eq!(f.campaign.attempts[0].gate,Some(false));
}
#[test]
fn independent_deadline_preserves_running_attempt_but_stops_followup() {
    let mut f=Fixture::new();f.campaign.plan.deadline="2026-10-03T00:00:00Z".into();
    f.campaign.plan_sha256=store::plan_hash(&f.campaign.plan).unwrap();f.campaign.state="active".into();
    f.campaign.attempts=vec![f.attempt.clone()];f.run.status="running".into();f.run.finished_at=None;
    super::engine::reconcile(&f.root,&f.root.join("control"),&mut f.campaign,std::slice::from_ref(&f.run),1790985601).unwrap();
    assert_eq!(f.campaign.state,"awaiting_web_review");assert_eq!(f.campaign.attempts.len(),1);
    assert_eq!(f.campaign.attempts[0].execution,"running");assert!(!f.campaign.web_reviewed);
}
#[test]
fn independent_inbox_cannot_authorize_and_hash_mismatch_is_rejected() {
    let f=Fixture::new();let mut state=Store::default();let draft=json!({"kind":"campaign.draft","plan":f.campaign.plan});
    super::events::apply(&mut state,&draft,false).unwrap();super::events::apply(&mut state,&draft,false).unwrap();assert_eq!(state.campaigns.len(),1);
    let mut authorize=json!({"kind":"campaign.authorize","campaign_id":f.campaign.plan.id,"plan_sha256":f.campaign.plan_sha256,"authorization_ref":"isolated-test-only"});
    assert!(super::events::apply(&mut state,&authorize,false).is_err());
    authorize["plan_sha256"]=json!("f".repeat(64));assert!(super::events::apply(&mut state,&authorize,true).is_err());
    authorize["plan_sha256"]=json!(f.campaign.plan_sha256);super::events::apply(&mut state,&authorize,true).unwrap();assert_eq!(state.campaigns[0].state,"active");
    let mut altered=draft;altered["plan"]["stages"][0]["gate"]["minimum"]=json!(0.1);assert!(super::events::apply(&mut state,&altered,false).is_err());
}


#[test]
fn independent_corrupt_persisted_attempt_stage_is_rejected_before_indexing() {
    let f=Fixture::new();let dir=f.root.join("control");fs::create_dir_all(&dir).unwrap();
    let mut c=f.campaign.clone();let mut a=f.attempt.clone();a.stage=usize::MAX;c.attempts.push(a);
    let state=Store{revision:1,campaigns:vec![c]};
    fs::write(dir.join("campaigns.json"),serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(store::load(&dir).is_err(),"corrupt attempt must not survive until a panic in verifier");
}

#[test]
fn independent_root_link_cannot_escape_frozen_provenance_boundary() {
    let f=Fixture::new();let link=f.root.join("linked-root");
    #[cfg(unix)] std::os::unix::fs::symlink(f.root.join("frozen"),&link).unwrap();
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        let result=std::process::Command::new("cmd.exe").args(["/d","/c","mklink","/J"])
            .arg(&link).arg(f.root.join("frozen")).creation_flags(0x08000000).output().unwrap();
        assert!(result.status.success(),"junction fixture setup failed");
    }
    let result=store::safe_path(&link,"targets.json");
    #[cfg(unix)] fs::remove_file(&link).unwrap();
    #[cfg(windows)] fs::remove_dir(&link).unwrap();
    assert!(result.is_err(),"root junction must be rejected, not only its children");
}


#[test]
fn independent_production_adapter_releases_verified_stage_scope_without_accepting_science() {
    let mut f=Fixture::new();
    let mut second=f.campaign.plan.stages[0].clone();second.id="repeat".into();
    f.campaign.plan.stages[0].on_pass=Some(1);f.campaign.plan.stages.push(second);f.campaign.plan.max_submissions=2;
    f.campaign.plan_sha256=store::plan_hash(&f.campaign.plan).unwrap();
    let state=super::AppState::isolated_for_tests(f.root.join("runtime"));
    let dir=super::project_dir(&state,"independent").unwrap();fs::create_dir_all(&dir).unwrap();
    let config=super::ProjectRoomConfig {id:"independent".into(),name:"Isolated review fixture".into(),local_root:f.root.to_string_lossy().into(),
        repo_root:f.root.to_string_lossy().into(),workspace_id:None,codex_thread_id:None,codex_automation_thread_id:None,antigravity_cascade_id:None,
        remote:crate::models::ProjectRemote::default(),enabled:true,keep_session_alive:false,
        created_at:"2026-10-03T00:00:00Z".into(),updated_at:"2026-10-03T00:00:00Z".into()};
    store::atomic(&dir.join(super::CONFIG_FILE),&config).unwrap();
    store::atomic(&dir.join(super::TASKS_FILE),&Vec::<super::ProjectTask>::new()).unwrap();
    store::atomic(&dir.join(super::RUNS_FILE),&Vec::<AgentRun>::new()).unwrap();
    super::process_event(&state,"independent",&json!({"kind":"campaign.draft","plan":f.campaign.plan}),false).unwrap();
    super::process_event(&state,"independent",&json!({"kind":"campaign.authorize","campaign_id":f.campaign.plan.id,
        "plan_sha256":f.campaign.plan_sha256,"authorization_ref":"isolated-review-fixture"}),true).unwrap();
    super::reconcile_project(&state,"independent").unwrap();
    let before:Vec<super::ProjectTask>=store::read(&dir.join(super::TASKS_FILE)).unwrap();assert_eq!(before.len(),1);
    super::reserve(&state,"independent",&before[0],&f.run.id).unwrap();
    f.campaign=store::load(&dir).unwrap().campaigns.remove(0);f.attempt=f.campaign.attempts[0].clone();
    f.run.task_id=f.attempt.task_id.clone();f.write(&f.packet([1.0,2.0,3.0]));
    store::atomic(&dir.join(super::RUNS_FILE),&vec![f.run.clone()]).unwrap();
    super::confirm_terminal(&dir,&f.run,true).unwrap();super::reconcile_project(&state,"independent").unwrap();
    let data=store::load(&dir).unwrap();assert_eq!(data.campaigns[0].attempts.len(),2);
    let after:Vec<super::ProjectTask>=store::read(&dir.join(super::TASKS_FILE)).unwrap();
    let previous=after.iter().find(|t|t.id==f.attempt.task_id).unwrap();
    assert_eq!(previous.status,"completed","verified experiment must release its write scope for the next approved stage");
    assert!(!previous.web_reviewed);assert_ne!(previous.reviewed_by,"chatgpt");
    let next=after.iter().find(|t|t.id==data.campaigns[0].attempts[1].task_id).unwrap();
    assert_eq!(previous.write_scope,next.write_scope);assert_eq!(next.status,"queued");
    super::allowed(&state,"independent",next).unwrap();
    assert!(!data.campaigns[0].web_reviewed);assert!(!data.campaigns[0].accepted);
}
