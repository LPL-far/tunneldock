use std::fs;
use std::path::PathBuf;

/// All standard chappie.yaml locations used by otunnel / TunnelDock.
///
/// Keep this list centralized so sync, discovery and uninstall always operate on
/// the exact same set of files. Duplicate paths are removed because on many
/// Unix systems `dirs::config_dir()` resolves to `~/.config`.
pub fn chappie_yaml_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let mut push_unique = |path: PathBuf| {
        if !paths.contains(&path) {
            paths.push(path);
        }
    };

    // 1. ~/.config/tunnel-client/chappie.yaml (XDG / default for otunnel)
    if let Some(home) = dirs::home_dir() {
        push_unique(home.join(".config").join("tunnel-client").join("chappie.yaml"));
    }

    // 2. AppData/Roaming/tunnel-client/chappie.yaml (Windows standard config_dir)
    //    or the platform config directory on macOS/Linux.
    if let Some(config_dir) = dirs::config_dir() {
        push_unique(config_dir.join("tunnel-client").join("chappie.yaml"));
    }

    // 3. ~/.chappie/chappie.yaml (Chappie custom directory)
    if let Some(home) = dirs::home_dir() {
        push_unique(home.join(".chappie").join("chappie.yaml"));
    }

    paths
}

/// Find an existing non-empty chappie.yaml across known Windows & POSIX/XDG locations.
pub fn get_chappie_yaml_path() -> Option<PathBuf> {
    for path in chappie_yaml_paths() {
        if path.exists() {
            if let Ok(meta) = fs::metadata(&path) {
                if meta.len() > 0 {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Save chappie.yaml to all standard locations so otunnel and any CLI variant can always find it.
pub fn sync_chappie_yaml(content: &str) -> std::io::Result<PathBuf> {
    let target_paths = chappie_yaml_paths();
    let mut primary = None;

    for path in &target_paths {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let res = fs::write(path, content);
        if res.is_ok() && primary.is_none() {
            primary = Some(path.clone());
        }
    }

    primary.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "未能成功写入任何配置路径",
        )
    })
}

/// Remove every synchronized chappie.yaml copy.
///
/// This is intentionally file-scoped: parent directories may contain unrelated
/// Chappie/otunnel state and must never be recursively deleted by an environment
/// component uninstall action.
pub fn remove_chappie_yaml_files() -> std::io::Result<usize> {
    let mut removed = 0;
    for path in chappie_yaml_paths() {
        if path.exists() {
            fs::remove_file(&path)?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Ensure all standard config locations have the chappie.yaml if one already exists.
pub fn ensure_chappie_yaml_synced() {
    if let Some(existing) = get_chappie_yaml_path() {
        if let Ok(content) = fs::read_to_string(&existing) {
            let migrated = content.replace(
                "command: pi --chappie",
                "command: chappie",
            );

            let _ = sync_chappie_yaml(&migrated);
        }
    }
}
