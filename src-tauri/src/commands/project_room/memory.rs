use super::*;

pub(super) fn default_research_goal(project_id: &str) -> &'static str {
    match project_id {
        "point_tracking" => {
            "推进 2D Point Tracking 研究，所有实现与实验服务于顶会论文的核心 hypothesis、方法验证和可复现性。"
        }
        "iqa_agent" => {
            "推进 IQA Agent 研究，保持 agent 设计、benchmark、实验记录与论文结论之间的一致性。"
        }
        "3d_mllm" => {
            "推进 3D + MLLM 空间推理研究，围绕几何表征、关系推理、数据与实验形成可验证的顶会论文证据链。"
        }
        _ => "推进当前科研项目，代码与实验均服务于可验证、可复现的论文结论。",
    }
}

pub(super) fn default_memory_protocol() -> &'static str {
    r#"# Project Memory Protocol

## Source of truth
The files in this directory are the durable research memory for this project.
Pi/ChatGPT/Codex/Antigravity conversation history is working memory, not the source of truth.

## Canonical files
- PROJECT_STATE.md: current project snapshot; replace stale state instead of appending forever.
- SESSION_HANDOFF.md: most recent transition point for the next agent/session.
- DECISIONS.md: durable methodological/engineering decisions that change future behavior.
- EXPERIMENTS.md: reproducible evidence and meaningful negative results.

## Update policy
After meaningful work:
1. update PROJECT_STATE.md to the current truth;
2. replace SESSION_HANDOFF.md;
3. merge a decision only when it changes future behavior;
4. record an experiment only when it is reproducible or prevents repeated debugging;
5. remove contradictory or superseded summary statements.

Do not create V1/V2/V3, *_old, *_backup, *_new, or *_final copies. When the project's own instructions permit Git, use local Git for history; otherwise keep one current tree and record hashes/configs/evidence.
"#
}

pub(super) fn project_memory_dir(config: &ProjectRoomConfig) -> PathBuf {
    PathBuf::from(&config.local_root).join(PROJECT_MEMORY_DIR)
}

pub(super) fn ensure_project_memory(config: &ProjectRoomConfig) -> Result<(), String> {
    let dir = project_memory_dir(config);
    fs::create_dir_all(&dir).map_err(|error| format!("创建 {} 失败: {}", dir.display(), error))?;

    let state_path = dir.join(PROJECT_STATE_FILE);
    if !state_path.exists() {
        let content = format!(
            "# {} — Project State\n\nLast updated: {}\n\n## Goal\n{}\n\n## Current Method\n\n## Current Evidence\n\n## Known Problems\n\n## Next Actions\n",
            config.name,
            local_now_rfc3339(),
            default_research_goal(&config.id)
        );
        fs::write(&state_path, content)
            .map_err(|error| format!("写入 {} 失败: {}", state_path.display(), error))?;
    }

    for (file_name, content) in [
        (
            SESSION_HANDOFF_FILE,
            format!(
                "# Session Handoff\n\nUpdated: {}\n\nRead PROJECT_STATE.md first.\n",
                local_now_rfc3339()
            ),
        ),
        (
            DECISIONS_FILE,
            "# Durable Decisions\n\nNo durable decisions recorded yet.\n".to_string(),
        ),
        (
            MEMORY_EXPERIMENTS_FILE,
            "# Verified Experiments / Engineering Evidence\n\nNo experiments recorded yet.\n"
                .to_string(),
        ),
        (MEMORY_PROTOCOL_FILE, default_memory_protocol().to_string()),
    ] {
        let path = dir.join(file_name);
        if !path.exists() {
            fs::write(&path, content)
                .map_err(|error| format!("写入 {} 失败: {}", path.display(), error))?;
        }
    }

    Ok(())
}

pub(super) fn read_memory_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("读取 {} 失败: {}", path.display(), error))
}

pub(super) fn project_memory_updated_at(dir: &Path) -> String {
    let mut newest = None;
    for file_name in [
        PROJECT_STATE_FILE,
        SESSION_HANDOFF_FILE,
        DECISIONS_FILE,
        MEMORY_EXPERIMENTS_FILE,
        MEMORY_PROTOCOL_FILE,
    ] {
        if let Ok(modified) = fs::metadata(dir.join(file_name)).and_then(|meta| meta.modified()) {
            if newest.map(|current| modified > current).unwrap_or(true) {
                newest = Some(modified);
            }
        }
    }

    newest
        .map(|time| {
            let dt: chrono::DateTime<chrono::Local> = time.into();
            dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
        })
        .unwrap_or_else(local_now_rfc3339)
}

pub(super) fn load_project_memory(config: &ProjectRoomConfig) -> Result<ProjectMemory, String> {
    ensure_project_memory(config)?;
    let dir = project_memory_dir(config);
    Ok(ProjectMemory {
        project_state: read_memory_text(&dir.join(PROJECT_STATE_FILE))?,
        session_handoff: read_memory_text(&dir.join(SESSION_HANDOFF_FILE))?,
        decisions: read_memory_text(&dir.join(DECISIONS_FILE))?,
        experiments: read_memory_text(&dir.join(MEMORY_EXPERIMENTS_FILE))?,
        memory_protocol: read_memory_text(&dir.join(MEMORY_PROTOCOL_FILE))?,
        updated_at: project_memory_updated_at(&dir),
    })
}

pub(super) fn save_project_memory(
    config: &ProjectRoomConfig,
    memory: &ProjectMemory,
) -> Result<(), String> {
    ensure_project_memory(config)?;
    let dir = project_memory_dir(config);
    for (file_name, content) in [
        (PROJECT_STATE_FILE, &memory.project_state),
        (SESSION_HANDOFF_FILE, &memory.session_handoff),
        (DECISIONS_FILE, &memory.decisions),
        (MEMORY_EXPERIMENTS_FILE, &memory.experiments),
        (MEMORY_PROTOCOL_FILE, &memory.memory_protocol),
    ] {
        fs::write(dir.join(file_name), content)
            .map_err(|error| format!("写入 project memory 失败: {}", error))?;
    }
    Ok(())
}
