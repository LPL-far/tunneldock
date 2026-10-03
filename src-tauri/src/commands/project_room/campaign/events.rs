use super::{model::*, store};
use serde_json::Value;

pub fn apply(store: &mut Store, value: &Value, human_control: bool) -> Result<(), String> {
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > 128 * 1024 {
        return Err("Campaign event too large".into());
    }
    let kind = value["kind"].as_str().ok_or("kind required")?;
    if kind == "campaign.draft" {
        let plan: Plan =
            serde_json::from_value(value["plan"].clone()).map_err(|e| e.to_string())?;
        store::validate(&plan)?;
        let hash = store::plan_hash(&plan)?;
        if let Some(c) = store.campaigns.iter().find(|c| c.plan.id == plan.id) {
            return if c.plan_sha256 == hash {
                Ok(())
            } else {
                Err(
                    "Campaign ID already binds a different immutable plan; use a new draft ID"
                        .into(),
                )
            };
        }
        if store.campaigns.len() >= 32 {
            return Err("Campaign store is full (32)".into());
        }
        store.campaigns.push(Campaign {
            plan,
            plan_sha256: hash,
            authorization_ref: None,
            paused: false,
            state: "draft".into(),
            stage: 0,
            attempts: Vec::new(),
            reason: String::new(),
            web_reviewed: false,
            accepted: false,
            review_ref: None,
        });
        return Ok(());
    }
    let c = store
        .campaigns
        .iter_mut()
        .find(|c| value["campaign_id"].as_str() == Some(&c.plan.id))
        .ok_or("Unknown campaign")?;
    if value["plan_sha256"].as_str() != Some(&c.plan_sha256) {
        return Err("Exact plan_sha256 required".into());
    }
    match kind {
        "campaign.authorize" => {
            // Inbox files, source reports and worker footers are not an authorization channel.
            if !human_control {
                return Err(
                    "Authorize through the human Campaign panel/IPC, never inbox or source reports"
                        .into(),
                );
            }
            let reference = value["authorization_ref"]
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.len() <= 1000)
                .ok_or("Nonempty user authorization_ref required")?;
            if let Some(old) = &c.authorization_ref {
                return if old == reference {
                    Ok(())
                } else {
                    Err("Authorization is immutable".into())
                };
            }
            if c.state != "draft" {
                return Err("Only drafts can be authorized".into());
            }
            c.authorization_ref = Some(reference.into());
            c.state = "active".into();
        }
        "campaign.pause" | "campaign.resume" => {
            if !human_control && value["author"] != "chatgpt" {
                return Err("Coordinator or human control required".into());
            }
            if c.web_reviewed {
                return Err("Reviewed campaign is immutable".into());
            }
            c.paused = kind == "campaign.pause";
        }
        "campaign.reviewed" => {
            if value["author"] != "chatgpt"
                || !matches!(c.state.as_str(), "awaiting_web_review" | "reviewed")
                || value["plan_version"].as_u64() != Some(c.plan.version as u64)
            {
                return Err(
                    "Separate Web review requires terminal campaign and exact plan version".into(),
                );
            }
            if c.attempts
                .iter()
                .any(|a| a.run_id.is_some() && a.evidence == "pending")
            {
                return Err("Reserved/running work has not drained; review cannot finalize".into());
            }
            let ids: Vec<String> = serde_json::from_value(value["evidence_ids"].clone())
                .map_err(|_| "evidence_ids required")?;
            let expected = c
                .attempts
                .iter()
                .filter_map(|a| a.evidence_id.clone())
                .collect::<Vec<_>>();
            if ids != expected {
                return Err("Web review evidence IDs differ from verified records".into());
            }
            let reference = value["review_ref"]
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.len() <= 1000)
                .ok_or("review_ref required")?;
            let accepted = value["accepted"]
                .as_bool()
                .ok_or("accepted boolean required")?;
            if c.web_reviewed {
                return if c.accepted == accepted && c.review_ref.as_deref() == Some(reference) {
                    Ok(())
                } else {
                    Err("Web review is immutable".into())
                };
            }
            if accepted {
                super::validation::pass_policy(c)?;
            }
            c.web_reviewed = true;
            c.accepted = accepted;
            c.review_ref = Some(reference.into());
            c.state = "reviewed".into();
            c.reason = format!("Independent Web review recorded; accepted={accepted}");
        }
        _ => return Err("Unknown campaign event".into()),
    }
    Ok(())
}
