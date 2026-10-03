mod app_supervisor;
mod audit;
pub mod commands;
pub mod i18n;
pub mod models;
pub mod state;
mod tray;
pub mod utils;

use state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Manager, WindowEvent};

pub fn run() {
    if app_supervisor::run_watchdog_mode() {
        return;
    }
    // Claim the desktop identity before constructing stores or cleanup-owning state.
    let desktop_guard = if cfg!(target_os = "windows") {
        match app_supervisor::attach(&AppState::resolve_app_data_dir()) {
            Ok(app_supervisor::Startup::Primary(guard)) => Some(guard),
            Ok(app_supervisor::Startup::AlreadyRunning) => return,
            Err(error) => {
                eprintln!("TunnelDock desktop lifecycle initialization failed: {error}");
                return;
            }
        }
    } else {
        None
    };
    let setup_guard = desktop_guard.clone();
    let exit_guard = desktop_guard.clone();
    let app_state = Arc::new(AppState::new());
    let state_exit = app_state.clone();
    let project_supervisor_state = app_state.clone();
    let exit_cleanup_started = Arc::new(AtomicBool::new(false));
    let exit_cleanup_flag = exit_cleanup_started.clone();
    let close_lifecycle = Arc::new(tray::CloseLifecycle::default());
    let setup_lifecycle = close_lifecycle.clone();
    let window_lifecycle = close_lifecycle.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .manage(desktop_guard)
        .manage(close_lifecycle)
        .setup(move |app| {
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            if let Some(window) = app.get_webview_window("main") {
                if let Some(icon) = app.default_window_icon() {
                    let _ = window.set_icon(icon.clone());
                }
            }
            tray::setup(app, setup_lifecycle)?;

            let supervisor_app = app.handle().clone();
            let supervisor_state = project_supervisor_state.clone();
            tauri::async_runtime::spawn(async move {
                commands::project_room::project_session_supervisor_loop(
                    supervisor_app,
                    supervisor_state,
                )
                .await;
            });

            if let Some(guard) = setup_guard {
                guard.ready()?;
                let handle = app.handle().clone();
                // A second launch asks the existing singleton to show its window.
                std::thread::spawn(move || {
                    while !project_supervisor_state.cleanup_in_progress() {
                        if guard.take_show_request() {
                            if let Some(window) = handle.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                });
            }
            Ok(())
        })
        .on_window_event(move |window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    tray::handle_close_requested(window, api, window_lifecycle.clone());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Environment
            commands::env::check_environment,
            commands::install::install_component_v2,
            commands::uninstall::uninstall_component,
            commands::env::save_tunnel_credentials,
            // Otunnel & Health & Doctor
            commands::otunnel::get_otunnel_status,
            commands::otunnel::start_otunnel,
            commands::otunnel::stop_otunnel,
            commands::otunnel::restart_otunnel,
            commands::otunnel::run_otunnel_doctor,
            commands::otunnel::probe_network_latency,
            // Workspaces
            commands::workspace::list_workspaces,
            commands::workspace::add_workspace,
            commands::workspace::remove_workspace,
            commands::workspace::start_workspace_session,
            commands::workspace::stop_workspace_session,
            commands::workspace::restart_workspace_session,
            commands::workspace::generate_chatgpt_prompt,
            // Project Rooms
            commands::project_room::list_project_rooms,
            commands::project_room::get_project_room,
            commands::project_room::update_project_config,
            commands::project_room::update_project_memory,
            commands::project_room::upsert_project_task,
            commands::project_room::append_project_message,
            commands::project_room::upsert_project_experiment,
            commands::project_room::update_agent_capacity,
            commands::project_room::list_agent_runtimes,
            commands::project_room::refresh_agent_capacities,
            commands::project_room::dispatch_project_task,
            commands::project_room::refresh_project_runs,
            commands::project_room::generate_project_room_prompt,
            commands::project_room::initialize_project_git,
            commands::project_room::scan_project_hygiene,
            // History
            commands::history::list_history,
            commands::history::clear_history,
            commands::history::export_history_json,
            // Settings
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::set_locale,
            commands::settings::refresh_process_environment,
            commands::settings::open_path_in_explorer,
            commands::settings::get_app_version,
            // Application lifecycle
            tray::resolve_close_request,
            app_supervisor::set_desktop_update_guard,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |app, event| match event {
            tauri::RunEvent::ExitRequested { api, code, .. } => {
                if let Some(guard) = &exit_guard {
                    let intent = if code == Some(tauri::RESTART_EXIT_CODE) {
                        "restart"
                    } else {
                        "exit"
                    };
                    if let Err(error) = guard.set_intent(intent) {
                        eprintln!("Cannot persist desktop exit intent: {error}");
                        api.prevent_exit();
                        return;
                    }
                }
                // Never perform process-tree teardown on Tauri's UI/event thread.
                // Doing so used to block window close long enough for Windows to
                // mark the application as "Not responding".
                if !exit_cleanup_flag.swap(true, Ordering::AcqRel) {
                    api.prevent_exit();
                    let app_handle = app.clone();
                    let state = state_exit.clone();
                    std::thread::spawn(move || {
                        state.cleanup_all_processes();
                        app_handle.exit(code.unwrap_or(0));
                    });
                }
            }
            tauri::RunEvent::Exit => {
                // Fallback for platform-specific exit paths. AppState cleanup is
                // idempotent, so this is a no-op if the background cleanup ran.
                state_exit.cleanup_all_processes();
            }
            _ => {}
        });
}
