//! Desktop lifecycle only. This guard never stops or restarts Project Room workers.
mod native;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Lease {
    version: u32,
    instance_id: String,
    pid: u32,
    started_at: u64,
    executable: PathBuf,
    cwd: PathBuf,
    args: Vec<String>,
    mode: String,
    intent: String,
    ready: bool,
}

pub(crate) struct AppSupervisor {
    root: PathBuf,
    lease: Mutex<Lease>,
    // OS releases the exclusive handle on a crash. A leftover filename is harmless.
    _instance_lock: File,
}

pub(crate) enum Startup {
    Primary(Arc<AppSupervisor>),
    AlreadyRunning,
}

fn write_atomic<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let bytes = serde_json::to_vec_pretty(value)?;
    let result = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        // A Windows reader can briefly deny replacement even for an atomic rename.
        for attempt in 0..20 {
            match fs::rename(&tmp, path) {
                Ok(()) => return Ok(()),
                Err(error) if attempt < 19 && matches!(error.raw_os_error(), Some(5 | 32 | 33)) => {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("bounded rename loop returns")
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

fn exclusive_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    options.open(path)
}

fn dev_command(name: &str, args: &[String]) -> bool {
    let name = name.to_ascii_lowercase();
    let dev = args.iter().any(|arg| arg == "dev");
    (name == "cargo.exe" || name == "cargo") && args.iter().any(|arg| arg == "run")
        || dev
            && args
                .iter()
                .any(|arg| arg.to_ascii_lowercase().contains("tauri"))
}

fn development_parent(system: &sysinfo::System) -> bool {
    let mut current = system
        .process(sysinfo::Pid::from_u32(std::process::id()))
        .and_then(|process| process.parent());
    for _ in 0..12 {
        let Some(pid) = current else {
            break;
        };
        let Some(process) = system.process(pid) else {
            break;
        };
        let args = process
            .cmd()
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        if dev_command(&process.name().to_string_lossy(), &args) {
            return true;
        }
        current = process.parent();
    }
    false
}

pub(crate) fn attach(app_data: &Path) -> io::Result<Startup> {
    let root = app_data.join("runtime").join("app-supervisor");
    fs::create_dir_all(&root)?;
    let instance_lock = match exclusive_file(&root.join("main.lock")) {
        Ok(lock) => lock,
        // Only a sharing violation is a duplicate, not arbitrary filesystem errors.
        Err(error) if matches!(error.raw_os_error(), Some(32) | Some(33)) => {
            if let Err(error) = fs::write(root.join("show.request"), b"show") {
                eprintln!("Desktop is already running, but cannot request its window: {error}");
            }
            return Ok(Startup::AlreadyRunning);
        }
        Err(error) => return Err(error),
    };
    let system = sysinfo::System::new_all();
    let pid = std::process::id();
    let started_at = system
        .process(sysinfo::Pid::from_u32(pid))
        .map(|process| process.start_time())
        .ok_or_else(|| io::Error::other("Cannot identify the desktop process birth time"))?;
    let mode = if development_parent(&system) {
        "development"
    } else {
        "standalone"
    };
    let guard = Arc::new(AppSupervisor {
        root,
        lease: Mutex::new(Lease {
            version: 1,
            instance_id: format!(
                "{}-{}",
                pid,
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
            ),
            pid,
            started_at,
            executable: std::env::current_exe()?,
            cwd: std::env::current_dir()?,
            args: std::env::args().skip(1).collect(),
            mode: mode.to_string(),
            intent: "running".to_string(),
            ready: false,
        }),
        _instance_lock: instance_lock,
    });
    write_atomic(&guard.root.join("state.json"), &*guard.lease.lock())?;
    if mode == "standalone" {
        guard.launch_guardian()?;
    } else {
        write_atomic(
            &guard.root.join("status.json"),
            &serde_json::json!({
                "status": "development_managed", "pid": pid,
                "reason": "cargo/tauri dev owns this process; desktop guardian is disabled"
            }),
        )?;
    }
    Ok(Startup::Primary(guard))
}

impl AppSupervisor {
    fn launch_guardian(self: &Arc<Self>) -> io::Result<()> {
        use sha2::{Digest, Sha256};
        let bytes = fs::read(std::env::current_exe()?)?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let guardian_dir = self.root.join("bin");
        fs::create_dir_all(&guardian_dir)?;
        let binary = guardian_dir.join(format!("TunnelDockGuard-{}.exe", &digest[..16]));
        if !binary.exists() {
            fs::write(&binary, &bytes)?;
        }
        let root = self.root.clone();
        let guard = self.clone();
        std::thread::spawn(move || {
            let mut attempts = std::collections::VecDeque::<std::time::Instant>::new();
            loop {
                let intent = guard.lease.lock().intent.clone();
                if matches!(intent.as_str(), "exit" | "restart") {
                    return;
                }
                if intent == "update" {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    continue;
                }
                match exclusive_file(&root.join("watcher.lock")) {
                    Ok(probe) => drop(probe),
                    Err(e) if matches!(e.raw_os_error(), Some(32) | Some(33)) => {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        continue;
                    }
                    Err(e) => {
                        eprintln!("Cannot inspect guardian ownership: {e}");
                        return;
                    }
                }
                while attempts
                    .front()
                    .is_some_and(|at| at.elapsed().as_secs() >= 300)
                {
                    attempts.pop_front();
                }
                if attempts.len() >= 3 {
                    if let Err(e) = write_atomic(
                        &root.join("status.json"),
                        &serde_json::json!({
                            "status":"watcher_start_failed", "reason":"Guardian failed to acquire its lease after three attempts"
                        }),
                    ) {
                        eprintln!("Cannot record guardian launch failure: {e}");
                    }
                    return;
                }
                attempts.push_back(std::time::Instant::now());
                let line = format!(
                    "\"{}\" --desktop-guardian \"{}\"",
                    binary.display(),
                    root.display()
                );
                let script = format!(
                    "$s=([wmiclass]'Win32_ProcessStartup').CreateInstance(); $s.ShowWindow=0; $r=([wmiclass]'Win32_Process').Create('{}',$null,$s); if($r.ReturnValue -ne 0){{throw ('guardian start failed: '+$r.ReturnValue)}}; [Console]::Out.Write($r.ProcessId)",
                    line.replace('\'', "''")
                );
                let result = crate::utils::cmd::execute_powershell(&script, None);
                if let Err(e) = write_atomic(
                    &root.join("launch.json"),
                    &serde_json::json!({
                        "success":result.success,"pid":result.stdout.trim(),"stderr":result.stderr,
                        "implementation":"native","at":chrono::Utc::now().to_rfc3339()
                    }),
                ) {
                    eprintln!("Cannot record guardian launch: {e}");
                }
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        });
        Ok(())
    }

    pub(crate) fn ready(&self) -> io::Result<()> {
        let mut lease = self.lease.lock();
        lease.ready = true;
        write_atomic(&self.root.join("state.json"), &*lease)
    }

    pub(crate) fn set_intent(&self, intent: &str) -> io::Result<()> {
        if !matches!(intent, "running" | "exit" | "update" | "restart") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid desktop exit intent",
            ));
        }
        let mut lease = self.lease.lock();
        lease.intent = intent.to_string();
        write_atomic(&self.root.join("state.json"), &*lease)
    }

    pub(crate) fn take_show_request(&self) -> bool {
        let path = self.root.join("show.request");
        match fs::remove_file(path) {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => {
                eprintln!("Cannot consume show request: {error}");
                false
            }
        }
    }
}

/// Handle the headless role before constructing a Tauri app or Project Room store.
pub(crate) fn run_watchdog_mode() -> bool {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.get(1).is_none_or(|arg| arg != "--desktop-guardian") {
        return false;
    }
    if let Some(root) = args.get(2) {
        if let Err(error) = native::run(Path::new(root)) {
            eprintln!("Desktop guardian: {error}");
        }
    } else {
        eprintln!("Desktop guardian requires its state directory");
    }
    true
}

#[tauri::command]
pub(crate) fn set_desktop_update_guard(
    guard: tauri::State<'_, Option<Arc<AppSupervisor>>>,
    suspended: bool,
) -> Result<(), String> {
    if let Some(guard) = guard.inner() {
        guard
            .set_intent(if suspended { "update" } else { "running" })
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn request_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    use tauri::Manager;
    if let Some(state) = app.try_state::<Option<Arc<AppSupervisor>>>() {
        if let Some(guard) = state.inner() {
            guard
                .set_intent("exit")
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn development_detection_does_not_disable_standalone_debug() {
        assert!(dev_command("cargo.exe", &["cargo".into(), "run".into()]));
        assert!(dev_command("node.exe", &["tauri.js".into(), "dev".into()]));
        assert!(!dev_command(
            "powershell.exe",
            &["Start-Process".into(), "tunneldock.exe".into()]
        ));
        assert!(!dev_command("cargo.exe", &["cargo".into(), "test".into()]));
    }
    #[test]
    fn atomic_state_replace_keeps_valid_json() {
        let root = std::env::temp_dir().join(format!(
            "td-supervisor-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("state.json");
        write_atomic(&path, &serde_json::json!({"intent":"running"})).unwrap();
        write_atomic(&path, &serde_json::json!({"intent":"exit"})).unwrap();
        let state: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(state["intent"], "exit");
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn exclusive_lock_is_released_without_deleting_lock_file() {
        let path = std::env::temp_dir().join(format!(
            "td-main-{}.lock",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let lock = exclusive_file(&path).unwrap();
        assert!(exclusive_file(&path).is_err());
        drop(lock);
        let replacement = exclusive_file(&path).unwrap();
        drop(replacement);
        fs::remove_file(path).unwrap();
    }
}
