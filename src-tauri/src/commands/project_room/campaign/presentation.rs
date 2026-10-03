use super::*;

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
fn item(
    c: &Campaign,
    config: &ProjectRoomConfig,
    dir: &Path,
    runs: &[AgentRun],
    detail: bool,
) -> Value {
    let last = c.attempts.last();
    let drain = c.attempts.iter().any(|a| {
        a.run_id.as_ref().is_some_and(|id| {
            a.terminal_confirmation.is_none()
                || runs
                    .iter()
                    .find(|r| &r.id == id)
                    .map(|r| matches!(r.status.as_str(), "running" | "interactive"))
                    .unwrap_or(true)
        })
    });
    let (owner, action) = if c.web_reviewed {
        ("none", "review_recorded")
    } else if c.authorization_ref.is_none() {
        ("user", "authorize")
    } else if c.state == "awaiting_web_review" {
        ("chatgpt", "review_evidence")
    } else if !config.enabled {
        ("user", "room_disabled")
    } else if c.paused {
        ("user", "resume")
    } else if last.is_some_and(|a| a.execution == "submission_uncertain") {
        ("chatgpt", "resolve_uncertain")
    } else if last.is_some_and(|a| a.execution == "running") {
        ("worker", "execute_stage")
    } else {
        ("dispatcher", "queue_stage")
    };
    let mut v = json!({"id":c.plan.id,"plan_sha256":c.plan_sha256,"question":if detail {c.plan.question.clone()}else{clip(&c.plan.question,120)},
        "stage":c.plan.stages.get(c.stage).map(|s|&s.id),"state":c.state,"paused":c.paused,"room_enabled":config.enabled,"active_drain":drain,
        "attempts":c.attempts.len(),"budget":c.plan.max_submissions,"deadline":c.plan.deadline,
        "execution":last.map(|a|&a.execution),"evidence":last.map(|a|&a.evidence),"gate":last.and_then(|a|a.gate),"metric":last.and_then(|a|a.metric),
        "web_reviewed":c.web_reviewed,"accepted":c.accepted,"reason":if detail {c.reason.clone()}else{clip(&c.reason,160)},
        "scientific_failures":c.attempts.iter().filter(|a|a.gate==Some(false)).count(),
        "evidence_count":c.attempts.iter().filter(|a|a.evidence_id.is_some()).count(),
        "authorization_pending":c.authorization_ref.is_none(),"next_owner":owner,"next_action":action});
    if detail {
        v["evidence_directory"] = json!(dir.join("campaign-evidence"));
        v["evidence_ids"] = json!(c
            .attempts
            .iter()
            .filter_map(|a| a.evidence_id.as_ref())
            .collect::<Vec<_>>());
    }
    v
}
pub fn details(config: &ProjectRoomConfig, dir: &Path, data: &Store, runs: &[AgentRun]) -> Value {
    json!({"items":data.campaigns.iter().map(|c|item(c,config,dir,runs,true)).collect::<Vec<_>>(),"revision":data.revision,"total":data.campaigns.len(),"room_enabled":config.enabled})
}
pub fn compact(config: &ProjectRoomConfig, dir: &Path, data: &Store, runs: &[AgentRun]) -> Value {
    let mut ordered = data.campaigns.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|c| match c.state.as_str() {
        "awaiting_web_review" => 0,
        "draft" => 1,
        "active" => 2,
        _ => 3,
    });
    let mut counts = serde_json::Map::new();
    for c in &data.campaigns {
        let key = if c.paused { "paused" } else { &c.state };
        let n = counts.get(key).and_then(Value::as_u64).unwrap_or(0) + 1;
        counts.insert(key.into(), json!(n));
    }
    json!({"items":ordered.into_iter().take(4).map(|c|item(c,config,dir,runs,false)).collect::<Vec<_>>(),
        "counts":counts,"total":data.campaigns.len(),"revision":data.revision,"room_enabled":config.enabled,
        "detail_path":dir.join("campaigns.json"),"detail_ipc":"campaign_details"})
}
pub fn publish(
    config: &ProjectRoomConfig,
    dir: &Path,
    data: &Store,
    runs: &[AgentRun],
) -> Result<(), String> {
    store::atomic(
        &Path::new(&config.local_root).join(BRIDGE_DIR).join(SUMMARY),
        &compact(config, dir, data, runs),
    )
}
