//! Headless native guardian. Never dispatches research tasks or kills processes.
use super::{exclusive_file, write_atomic, Lease};
use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};
use sysinfo::{Pid, ProcessesToUpdate, System};

fn read_lease(root: &Path) -> io::Result<Lease> {
    let lease: Lease = serde_json::from_slice(&fs::read(root.join("state.json"))?)?;
    if lease.version != 1
        || lease.pid == 0
        || lease.instance_id.is_empty()
        || !lease.executable.is_absolute()
        || !lease.cwd.is_absolute()
    {
        return Err(io::Error::other(
            "Invalid desktop lease; not guessing a target",
        ));
    }
    Ok(lease)
}

fn alive(system: &mut System, lease: &Lease) -> io::Result<bool> {
    let pid = Pid::from_u32(lease.pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let Some(process) = system.process(pid) else {
        return Ok(false);
    };
    let actual = process
        .exe()
        .ok_or_else(|| io::Error::other("Cannot inspect process identity"))?;
    let normalized = |p: &Path| {
        p.to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .replace('/', "\\")
            .to_ascii_lowercase()
    };
    if process.start_time() != lease.started_at
        || normalized(actual) != normalized(&lease.executable)
    {
        return Err(io::Error::other(
            "PID birth time or executable mismatch; refusing to adopt another process",
        ));
    }
    Ok(true)
}

fn event(root: &Path, lease: &Lease, status: &str, reason: &str, count: u32) -> io::Result<()> {
    let value = serde_json::json!({"status":status,"reason":reason,"watcher_pid":std::process::id(),
        "restart_count":count,"app_pid":lease.pid,"instance_id":lease.instance_id,"at":chrono::Utc::now().to_rfc3339(),"implementation":"native"});
    write_atomic(&root.join("status.json"), &value)?;
    let path = root.join("events.jsonl");
    if fs::metadata(&path).is_ok_and(|meta| meta.len() > 262144) {
        let previous = root.join("events.previous.jsonl");
        if previous.exists() {
            fs::remove_file(&previous)?;
        }
        fs::rename(&path, previous)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", serde_json::to_string(&value)?)
}

fn quitting() -> bool {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        extern "system" {
            fn GetSystemMetrics(index: i32) -> i32;
        }
        // SM_SHUTTINGDOWN: do not relaunch a GUI during Windows shutdown/logoff.
        unsafe { GetSystemMetrics(0x2000) != 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub(super) fn run(root: &Path) -> io::Result<()> {
    let _lock = match exclusive_file(&root.join("watcher.lock")) {
        Ok(lock) => lock,
        Err(e) if matches!(e.raw_os_error(), Some(32) | Some(33)) => return Ok(()),
        Err(e) => return Err(e),
    };
    let result = watch(root);
    if let Err(error) = &result {
        if let Ok(lease) = read_lease(root) {
            if let Err(record_error) = event(root, &lease, "watcher_error", &error.to_string(), 0) {
                eprintln!("Guardian error: {error}; recording error: {record_error}");
            }
        }
    }
    result
}

fn watch(root: &Path) -> io::Result<()> {
    let mut system = System::new();
    let mut attempts = VecDeque::<Instant>::new();
    let mut count = 0;
    let mut announced = String::new();
    let mut started = Instant::now();
    let mut stalled = false;
    loop {
        let lease = read_lease(root)?;
        if quitting() || matches!(lease.intent.as_str(), "exit" | "restart") {
            event(root, &lease, "stopped", &lease.intent, count)?;
            return Ok(());
        }
        if lease.mode != "standalone" {
            event(
                root,
                &lease,
                "development_managed",
                "Tauri dev owns lifecycle",
                count,
            )?;
            return Ok(());
        }
        if alive(&mut system, &lease)? {
            if announced != lease.instance_id {
                event(
                    root,
                    &lease,
                    "watching",
                    "desktop process identity verified",
                    count,
                )?;
                announced = lease.instance_id.clone();
                started = Instant::now();
                stalled = false;
            }
            if !lease.ready && !stalled && started.elapsed() >= Duration::from_secs(60) {
                event(
                    root,
                    &lease,
                    "startup_stalled",
                    "Process alive but not ready; not killing it",
                    count,
                )?;
                stalled = true;
            }
            thread::sleep(Duration::from_millis(500));
            continue;
        }
        if lease.intent != "running" {
            event(root, &lease, "stopped", &lease.intent, count)?;
            return Ok(());
        }
        while attempts
            .front()
            .is_some_and(|at| at.elapsed() >= Duration::from_secs(300))
        {
            attempts.pop_front();
        }
        if attempts.len() >= 3 {
            event(
                root,
                &lease,
                "circuit_open",
                "Three recovery attempts in five minutes; manual inspection required",
                count,
            )?;
            return Ok(());
        }
        let delay = [2, 5, 10][attempts.len()];
        event(
            root,
            &lease,
            "backoff",
            &format!("unexpected process exit; retry in {delay}s"),
            count,
        )?;
        thread::sleep(Duration::from_secs(delay));
        let latest = read_lease(root)?;
        if latest.instance_id != lease.instance_id || latest.intent != "running" {
            continue;
        }
        // Release the probe before launching. The GUI atomically claims this same
        // lock before constructing AppState, so manual-start races cannot dispatch twice.
        match exclusive_file(&root.join("main.lock")) {
            Ok(probe) => drop(probe),
            Err(e) if matches!(e.raw_os_error(), Some(32) | Some(33)) => {
                thread::sleep(Duration::from_millis(500));
                continue;
            }
            Err(e) => return Err(e),
        }
        if !lease.executable.is_file() {
            event(
                root,
                &lease,
                "executable_missing",
                "No alternate binary will be guessed",
                count,
            )?;
            return Ok(());
        }
        attempts.push_back(Instant::now());
        count += 1;
        let mut command = Command::new(&lease.executable);
        command.args(&lease.args).current_dir(&lease.cwd);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                event(root, &lease, "launch_failed", &e.to_string(), count)?;
                continue;
            }
        };
        event(
            root,
            &lease,
            "restarting",
            &format!("new process={}", child.id()),
            count,
        )?;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            thread::sleep(Duration::from_millis(250));
            let next = read_lease(root)?;
            if next.instance_id != lease.instance_id {
                break;
            }
            if child.try_wait()?.is_some() {
                break;
            }
            if Instant::now() >= deadline {
                event(
                    root,
                    &lease,
                    "startup_stalled",
                    "New process alive without a lease; refusing duplicate launch",
                    count,
                )?;
                return Ok(());
            }
        }
    }
}
