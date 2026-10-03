use super::model::*;
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_JSON: usize = 2 * 1024 * 1024;
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn plan_hash(plan: &Plan) -> Result<String, String> {
    Ok(digest(
        &serde_json::to_vec(plan).map_err(|e| e.to_string())?,
    ))
}
pub fn bytes(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !f.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Expected regular file".into());
    }
    let mut data = Vec::new();
    f.take(limit as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > limit {
        return Err(format!("{} exceeds {limit} bytes", path.display()));
    }
    Ok(data)
}
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&bytes(path, MAX_JSON)?).map_err(|e| format!("{}: {e}", path.display()))
}
pub fn atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    atomic_bytes(path, &data)
}
pub fn atomic_bytes(path: &Path, data: &[u8]) -> Result<(), String> {
    if data.len() > MAX_JSON {
        return Err("Campaign store capacity exceeded".into());
    }
    let parent = path.parent().ok_or("Missing parent")?;
    safe_path(
        parent,
        path.file_name()
            .and_then(|s| s.to_str())
            .ok_or("Invalid filename")?,
    )?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut reserved = None;
    for _ in 0..32 {
        let tmp = path.with_extension(format!(
            "{}-{}.campaign-tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(f) => {
                reserved = Some((tmp, f));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    let (tmp, mut f) = reserved.ok_or("Cannot reserve unique campaign temporary file")?;
    let write_result = f.write_all(data).and_then(|_| f.sync_all());
    drop(f);
    let result = write_result.and_then(|_| fs::rename(&tmp, path));
    if let Err(error) = result {
        if let Err(cleanup) = fs::remove_file(&tmp) {
            return Err(format!(
                "Atomic campaign persistence failed: {error}; temp cleanup failed: {cleanup}"
            ));
        }
        return Err(format!("Atomic campaign persistence failed: {error}"));
    }
    Ok(())
}

pub fn archive(dir: &Path, data: &[u8]) -> Result<String, String> {
    let id = digest(data);
    let path = safe_path(dir, &format!("campaign-evidence/{id}.json"))?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            if bytes(&path, MAX_JSON)? != data {
                return Err("Archived evidence is corrupt".into());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => atomic_bytes(&path, data)?,
        Err(e) => return Err(e.to_string()),
    }
    Ok(id)
}
pub fn load(dir: &Path) -> Result<Store, String> {
    let path = safe_path(dir, "campaigns.json")?;
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(e) => Err(e.to_string()),
        Ok(m) if m.file_type().is_symlink() => Err("Campaign store symlink refused".into()),
        Ok(_) => {
            let store: Store = read(&path)?;
            if store.campaigns.len() > 32 {
                return Err("Too many campaigns".into());
            }
            super::validation::persisted(&store)?;
            Ok(store)
        }
    }
}
pub fn save(dir: &Path, data: &mut Store) -> Result<(), String> {
    let current = load(dir)?;
    if current.revision != data.revision {
        return Err("Stale campaign store revision".into());
    }
    for old in &current.campaigns {
        let next = data
            .campaigns
            .iter()
            .find(|c| c.plan.id == old.plan.id)
            .ok_or("Campaign history cannot be removed")?;
        if old.plan != next.plan
            || old.plan_sha256 != next.plan_sha256
            || (old.authorization_ref.is_some() && old.authorization_ref != next.authorization_ref)
            || (old.web_reviewed && old != next)
        {
            return Err("Persisted plan, authorization or final review is immutable".into());
        }
    }
    let mut next = data.clone();
    next.revision = data.revision.checked_add(1).ok_or("Revision overflow")?;
    super::validation::persisted(&next)?;
    atomic(&dir.join("campaigns.json"), &next)?;
    *data = next;
    Ok(())
}
pub fn stable_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub fn relative(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 240
        || value.contains(['\\', ':', '*', '?'])
        || value
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.ends_with(['.', ' ']))
        || value.chars().any(char::is_control)
    {
        return Err(format!("Unsafe relative path: {value}"));
    }
    Ok(())
}
/// Inspect each component, including Windows junction/reparse points; never resolve through links.
pub fn safe_path(root: &Path, value: &str) -> Result<PathBuf, String> {
    relative(value)?;
    let mut path = PathBuf::new();
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(root)
    };
    if absolute
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Parent traversal in campaign root".into());
    }
    let joined = absolute.join(value);
    for component in joined.components() {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(m) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if m.file_attributes() & 0x400 != 0 {
                        return Err("Reparse point refused".into());
                    }
                }
                if m.file_type().is_symlink() {
                    return Err("Symlink refused".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(path)
}
fn overlap(a: &str, b: &str) -> bool {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    a == b || a.starts_with(&(b.clone() + "/")) || b.starts_with(&(a + "/"))
}
pub fn validate(p: &Plan) -> Result<(), String> {
    if p.version != 1
        || p.id.is_empty()
        || p.id.len() > 64
        || !p
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || p.question.trim().is_empty()
        || p.hypothesis.trim().is_empty()
        || p.question.len() > 4000
        || p.hypothesis.len() > 4000
        || p.stages.is_empty()
        || p.stages.len() > 16
        || !(1..=64).contains(&p.max_submissions)
        || p.provenance.len() > 64
    {
        return Err("Invalid bounded plan".into());
    }
    chrono::DateTime::parse_from_rfc3339(&p.deadline).map_err(|_| "Invalid deadline")?;
    for role in ["baseline", "code", "config", "dataset", "evaluator"] {
        if !p.provenance.iter().any(|f| f.role == role) {
            return Err(format!("Missing frozen {role}"));
        }
    }
    let mut paths = std::collections::HashSet::new();
    for f in &p.provenance {
        relative(&f.path)?;
        if !paths.insert(f.path.to_lowercase())
            || f.sha256.len() != 64
            || !f
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Invalid/duplicate provenance".into());
        }
    }
    let mut ids = std::collections::HashSet::new();
    for (index, s) in p.stages.iter().enumerate() {
        relative(&s.intervention_scope)?;
        if s.id.is_empty()
            || s.id.len() > 64
            || !stable_id(&s.id)
            || !ids.insert(&s.id)
            || s.instruction.trim().is_empty()
            || s.instruction.len() > 8000
            || !(1..=16).contains(&s.max_attempts)
            || s.on_unknown != "awaiting_web_review"
            || !s.gate.minimum.is_finite()
            || !(1..=10000).contains(&s.gate.sample_count)
        {
            return Err("Invalid stage/gate".into());
        }
        for next in [s.on_pass, s.on_scientific_fail].into_iter().flatten() {
            if next <= index || next >= p.stages.len() {
                return Err("Transitions must point forward".into());
            }
        }
        if !p.provenance.iter().any(|f| f.path == s.gate.reference_path) {
            return Err("Reference data must be frozen".into());
        }
        if [".tunneldock", ".project_memory", ".git"]
            .iter()
            .any(|x| overlap(&s.intervention_scope, x))
            || p.provenance
                .iter()
                .any(|f| overlap(&s.intervention_scope, &f.path))
        {
            return Err("Allowed writes overlap protected provenance/control/memory".into());
        }
    }
    Ok(())
}
