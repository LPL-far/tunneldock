use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::models::InstallProgressEvent;
use crate::state::AppState;
use crate::utils::cmd::{
    execute_cmd, find_executable, kill_process_tree, refresh_process_path, run_streaming,
};
use crate::utils::paths::{chappie_yaml_paths, remove_chappie_yaml_files};
use super::operation::component_operation_lock;

#[derive(Clone)]
struct UninstallLogger {
    app: AppHandle,
    item_id: String,
}

impl UninstallLogger {
    fn emit(&self, stage: &str, line: impl Into<String>, is_error: bool) {
        let _ = self.app.emit(
            "install-log",
            InstallProgressEvent {
                item_id: self.item_id.clone(),
                stage: stage.to_string(),
                log_line: line.into(),
                is_error,
            },
        );
    }

    fn info(&self, line: impl Into<String>) {
        self.emit("uninstalling", line, false);
    }

    fn success(&self, line: impl Into<String>) {
        self.emit("success", line, false);
    }

    fn error(&self, line: impl Into<String>) {
        self.emit("failed", line, true);
    }
}

fn run_logged(logger: &UninstallLogger, command: &str, args: &[&str]) -> bool {
    logger.info(format!("> {} {}", command, args.join(" ")));
    let ok = run_streaming(command, args, None, {
        let logger = logger.clone();
        move |line, _from_stderr| logger.info(line)
    });
    if !ok {
        logger.info(format!(
            "命令返回非零退出状态: {} {}",
            command,
            args.join(" ")
        ));
    }
    ok
}

fn command_text(command: &str, args: &[&str]) -> Option<String> {
    let out = execute_cmd(command, args, None);
    if !out.success {
        return None;
    }
    let text = if !out.stdout.trim().is_empty() {
        out.stdout.trim()
    } else {
        out.stderr.trim()
    };
    (!text.is_empty()).then(|| text.to_string())
}

fn path_is_under(path: &Path, root: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        let normalize = |value: &Path| {
            value
                .to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_ascii_lowercase()
        };
        let path = normalize(path);
        let root = normalize(root);
        path == root || path.starts_with(&format!("{}\\", root))
    }

    #[cfg(not(target_os = "windows"))]
    {
        path.starts_with(root)
    }
}

fn cargo_bin_dir() -> Option<PathBuf> {
    env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .map(|home| home.join("bin"))
        .or_else(|| dirs::home_dir().map(|home| home.join(".cargo").join("bin")))
}

fn remove_known_cargo_binary(
    logger: &UninstallLogger,
    executable: &str,
) -> Result<bool, String> {
    let Some(resolved) = find_executable(executable) else {
        return Ok(true);
    };
    let resolved_path = PathBuf::from(&resolved);
    let cargo_bin = cargo_bin_dir().ok_or_else(|| {
        format!(
            "{} 仍存在于 {}，但无法确定 Cargo bin 目录，拒绝删除未知路径",
            executable, resolved
        )
    })?;

    if !path_is_under(&resolved_path, &cargo_bin) {
        return Err(format!(
            "{} 仍存在于 {}，该路径不属于 {}；为避免误删其他软件，已停止自动删除",
            executable,
            resolved,
            cargo_bin.display()
        ));
    }

    logger.info(format!(
        "Cargo 卸载记录未清除二进制，安全删除 Cargo bin 中的文件: {}",
        resolved_path.display()
    ));
    fs::remove_file(&resolved_path)
        .map(|_| true)
        .map_err(|err| format!("删除 {} 失败: {}", resolved_path.display(), err))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn brew_manages(formula: &str) -> bool {
    find_executable("brew").is_some()
        && execute_cmd("brew", &["list", "--versions", formula], None).success
}

#[cfg(target_os = "windows")]
fn active_nvm_node_version() -> Option<String> {
    find_executable("nvm")?;
    let node = PathBuf::from(find_executable("node")?);
    let nvm_symlink = PathBuf::from(env::var_os("NVM_SYMLINK")?);
    if !path_is_under(&node, &nvm_symlink) {
        return None;
    }
    command_text("node", &["-v"])
        .map(|version| version.trim().trim_start_matches('v').to_string())
}

#[allow(clippy::needless_return)]
fn uninstall_node_runtime(logger: &UninstallLogger) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(version) = active_nvm_node_version() {
            logger.info(format!(
                "检测到当前 Node.js 由 nvm-windows 管理，将卸载活动版本 {}。",
                version
            ));
            return Ok(run_logged(logger, "nvm", &["uninstall", &version]));
        }

        logger.info("通过 winget 卸载 Node.js；npm 会随同一运行时包一起移除。".to_string());
        return Ok(run_logged(
            logger,
            "winget",
            &[
                "uninstall",
                "--id",
                "OpenJS.NodeJS",
                "-e",
                "--silent",
                "--disable-interactivity",
            ],
        ));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        if brew_manages("node") {
            return Ok(run_logged(logger, "brew", &["uninstall", "node"]));
        }
        Err("当前 Node.js 不是 TunnelDock 可安全识别的 Homebrew 安装；为避免删除系统包，请使用原安装来源卸载。".to_string())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err("当前平台不支持自动卸载 Node.js".to_string())
    }
}

#[allow(clippy::needless_return)]
fn uninstall_git(logger: &UninstallLogger) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        return Ok(run_logged(
            logger,
            "winget",
            &[
                "uninstall",
                "--id",
                "Git.Git",
                "-e",
                "--silent",
                "--disable-interactivity",
            ],
        ));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        if brew_manages("git") {
            return Ok(run_logged(logger, "brew", &["uninstall", "git"]));
        }
        Err("当前 Git 不是 Homebrew 管理的用户级安装。系统 Git/发行版 Git 不应由桌面应用静默删除，请使用原包管理器卸载。".to_string())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err("当前平台不支持自动卸载 Git".to_string())
    }
}

#[allow(clippy::needless_return)]
fn uninstall_rust(logger: &UninstallLogger) -> Result<bool, String> {
    if find_executable("rustup").is_some() {
        logger.info("检测到 rustup，将使用官方 rustup self uninstall 移除 Rust/Cargo 工具链。".to_string());
        return Ok(run_logged(logger, "rustup", &["self", "uninstall", "-y"]));
    }

    #[cfg(target_os = "windows")]
    {
        return Ok(run_logged(
            logger,
            "winget",
            &[
                "uninstall",
                "--id",
                "Rustlang.Rustup",
                "-e",
                "--silent",
                "--disable-interactivity",
            ],
        ));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        if brew_manages("rustup") {
            return Ok(run_logged(logger, "brew", &["uninstall", "rustup"]));
        }
        if brew_manages("rust") {
            return Ok(run_logged(logger, "brew", &["uninstall", "rust"]));
        }
        Err("当前 Rust/Cargo 不是 rustup 或 Homebrew 管理的安装；请使用原发行版包管理器卸载。".to_string())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err("当前平台不支持自动卸载 Rust/Cargo".to_string())
    }
}

fn uninstall_cargo_package(
    logger: &UninstallLogger,
    package: &str,
    executable: &str,
) -> Result<bool, String> {
    let mut command_ok = false;
    if find_executable("cargo").is_some() {
        command_ok = run_logged(logger, "cargo", &["uninstall", package]);
        refresh_process_path();
        if find_executable(executable).is_none() {
            return Ok(true);
        }
    } else {
        logger.info(format!(
            "未检测到 Cargo，将仅在确认 {} 位于 Cargo bin 目录时执行安全文件清理。",
            executable
        ));
    }

    let removed = remove_known_cargo_binary(logger, executable)?;
    Ok(command_ok || removed)
}

fn chappie_package_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join(".pi")
            .join("agent")
            .join("npm")
            .join("node_modules")
            .join("@zetaloop")
            .join("chappie")
    })
}

fn uninstall_chappie(logger: &UninstallLogger) -> Result<bool, String> {
    if find_executable("pi").is_some() {
        let ok = run_logged(logger, "pi", &["uninstall", "npm:@zetaloop/chappie"]);
        if ok {
            return Ok(true);
        }
        logger.info("Pi 卸载扩展命令失败，将检查受控的 Chappie 本地包目录。".to_string());
    }

    let Some(package_dir) = chappie_package_dir() else {
        return Ok(false);
    };
    if !package_dir.exists() {
        return Ok(true);
    }

    logger.info(format!(
        "删除 Pi 管理目录中的 Chappie 包: {}",
        package_dir.display()
    ));
    fs::remove_dir_all(&package_dir)
        .map(|_| true)
        .map_err(|err| format!("删除 {} 失败: {}", package_dir.display(), err))
}

fn uninstall_pi(logger: &UninstallLogger) -> Result<bool, String> {
    if find_executable("npm").is_none() {
        return Err("卸载 Pi 需要 npm；当前 npm 不可用，请先恢复 Node.js/npm 后再卸载 Pi".to_string());
    }
    Ok(run_logged(
        logger,
        "npm",
        &[
            "uninstall",
            "-g",
            "--ignore-scripts",
            "@earendil-works/pi-coding-agent",
        ],
    ))
}

fn uninstall_tunnel_key(state: &Arc<AppState>, logger: &UninstallLogger) -> Result<bool, String> {
    let key_file = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".chappie")
        .join("tunnelkey.txt");

    if key_file.exists() {
        logger.info(format!("删除 Tunnel API Key 文件: {}", key_file.display()));
        fs::remove_file(&key_file)
            .map_err(|err| format!("删除 {} 失败: {}", key_file.display(), err))?;
    }

    state.settings.lock().api_key.clear();
    state.save_settings();
    Ok(true)
}

fn uninstall_tunnel_config(
    state: &Arc<AppState>,
    logger: &UninstallLogger,
) -> Result<bool, String> {
    for path in chappie_yaml_paths().into_iter().filter(|path| path.exists()) {
        logger.info(format!("删除 Tunnel Profile: {}", path.display()));
    }
    remove_chappie_yaml_files().map_err(|err| format!("删除 Tunnel Profile 失败: {}", err))?;
    state.settings.lock().tunnel_id.clear();
    state.save_settings();
    Ok(true)
}

fn stop_otunnel_process(state: &Arc<AppState>, logger: &UninstallLogger) {
    if let Some(pid) = state.otunnel_owned_pid.lock().take() {
        logger.info(format!("卸载前停止 TunnelDock 管理的 otunnel 进程 PID {}。", pid));
        let _ = kill_process_tree(pid);
        let mut known_pid = state.otunnel_pid.lock();
        if *known_pid == Some(pid) {
            *known_pid = None;
        }
        drop(known_pid);
        state.clear_otunnel_runtime();
    }
}

fn stop_workspace_processes(state: &Arc<AppState>, logger: &UninstallLogger) {
    state.running_workspace_stdins.lock().clear();
    let pids: Vec<u32> = state
        .running_workspace_pids
        .lock()
        .drain()
        .map(|(_, pid)| pid)
        .collect();

    if !pids.is_empty() {
        logger.info(format!(
            "卸载前停止 {} 个由 TunnelDock 管理的 Pi 工作区进程。",
            pids.len()
        ));
    }
    for pid in pids {
        let _ = kill_process_tree(pid);
    }

    let mut workspaces = state.workspaces.lock();
    for workspace in workspaces.iter_mut() {
        workspace.status = "stopped".to_string();
        workspace.pid = None;
        workspace.session_id = None;
        workspace.binding_count = 0;
        workspace.error_message = None;
    }
    drop(workspaces);
    state.save_workspaces();
}

fn prepare_for_uninstall(item_id: &str, state: &Arc<AppState>, logger: &UninstallLogger) {
    if matches!(item_id, "node" | "npm" | "pi" | "chappie") {
        stop_workspace_processes(state, logger);
    }
    if matches!(item_id, "cargo" | "otunnel" | "tunnel_key" | "tunnel_config") {
        stop_otunnel_process(state, logger);
    }
}

fn ensure_executable_absent(command: &str, display_name: &str) -> Result<String, String> {
    match find_executable(command) {
        None => Ok(format!("{} 已卸载", display_name)),
        Some(path) => Err(format!("仍检测到 {} 可执行文件: {}", display_name, path)),
    }
}

fn verify_chappie_absent() -> Result<String, String> {
    let package_dir_exists = chappie_package_dir().is_some_and(|path| path.exists());
    let listed = if find_executable("pi").is_some() {
        let out = execute_cmd("pi", &["list"], None);
        out.success && out.stdout.contains("@zetaloop/chappie")
    } else {
        false
    };

    if package_dir_exists || listed {
        Err("仍检测到 @zetaloop/chappie 扩展或其 Pi 管理目录".to_string())
    } else {
        Ok("Chappie 扩展已卸载".to_string())
    }
}

fn verify_removed(item_id: &str, state: &Arc<AppState>) -> Result<String, String> {
    refresh_process_path();
    match item_id {
        "node" => ensure_executable_absent("node", "Node.js"),
        "npm" => ensure_executable_absent("npm", "npm"),
        "git" => ensure_executable_absent("git", "Git"),
        "cargo" => ensure_executable_absent("cargo", "Cargo"),
        "cargo_binstall" => ensure_executable_absent("cargo-binstall", "cargo-binstall"),
        "otunnel" => ensure_executable_absent("otunnel", "otunnel"),
        "pi" => ensure_executable_absent("pi", "Pi"),
        "chappie" => verify_chappie_absent(),
        "tunnel_key" => {
            let key_file = dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".chappie")
                .join("tunnelkey.txt");
            if key_file.exists() || !state.settings.lock().api_key.trim().is_empty() {
                Err("Tunnel API Key 文件或应用设置中仍存在密钥".to_string())
            } else {
                Ok("Tunnel API Key 已清除".to_string())
            }
        }
        "tunnel_config" => {
            let config_exists = chappie_yaml_paths().into_iter().any(|path| path.exists());
            if config_exists || !state.settings.lock().tunnel_id.trim().is_empty() {
                Err("仍检测到 chappie.yaml 或应用设置中的 Tunnel ID".to_string())
            } else {
                Ok("Tunnel Profile 已清除".to_string())
            }
        }
        _ => Err(format!("未知的卸载组件: {}", item_id)),
    }
}

fn uninstall_component_blocking(
    app: AppHandle,
    state: Arc<AppState>,
    item_id: String,
) -> Result<bool, String> {
    let logger = UninstallLogger {
        app,
        item_id: item_id.clone(),
    };

    logger.emit("starting", format!("开始卸载组件: {}", item_id), false);

    if let Ok(message) = verify_removed(&item_id, &state) {
        logger.success(format!("无需重复卸载：{}", message));
        return Ok(true);
    }

    prepare_for_uninstall(&item_id, &state, &logger);

    let command_ok = match item_id.as_str() {
        "node" => uninstall_node_runtime(&logger)?,
        "npm" => {
            logger.info("npm 随 Node.js 运行时分发；卸载 npm 将卸载 Node.js/npm 整套运行时。".to_string());
            uninstall_node_runtime(&logger)?
        }
        "git" => uninstall_git(&logger)?,
        "cargo" => {
            logger.info("卸载 Rust/Cargo 会同时影响该 Cargo Home 中的 cargo-binstall、otunnel 等 Cargo 二进制。".to_string());
            uninstall_rust(&logger)?
        }
        "cargo_binstall" => uninstall_cargo_package(&logger, "cargo-binstall", "cargo-binstall")?,
        "otunnel" => uninstall_cargo_package(&logger, "otunnel", "otunnel")?,
        "pi" => uninstall_pi(&logger)?,
        "chappie" => uninstall_chappie(&logger)?,
        "tunnel_key" => uninstall_tunnel_key(&state, &logger)?,
        "tunnel_config" => uninstall_tunnel_config(&state, &logger)?,
        _ => return Err(format!("未知的卸载组件: {}", item_id)),
    };

    refresh_process_path();
    match verify_removed(&item_id, &state) {
        Ok(message) => {
            if !command_ok {
                logger.info("卸载命令曾返回非零状态，但最终后置条件已满足，以实际系统状态为准。".to_string());
            }
            logger.success(format!("卸载完成：{}", message));
            Ok(true)
        }
        Err(verify_error) => {
            let message = if command_ok {
                format!("卸载命令已结束，但后置验证失败：{}", verify_error)
            } else {
                format!("卸载命令失败，且后置验证未通过：{}", verify_error)
            };
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn uninstall_component(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    item_id: String,
) -> Result<bool, String> {
    // Same lock as install_component_v2: package managers, PATH and toolchains are
    // shared mutable state and must never be changed by overlapping IPC requests.
    let _guard = component_operation_lock().lock().await;

    let state = state.inner().clone();
    let failure_app = app.clone();
    let failure_item_id = item_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        uninstall_component_blocking(app, state, item_id)
    })
    .await
    .map_err(|join_error| format!("卸载任务异常终止: {}", join_error))?;

    if let Err(ref error) = result {
        UninstallLogger {
            app: failure_app,
            item_id: failure_item_id,
        }
        .error(error.clone());
    }

    result
}
