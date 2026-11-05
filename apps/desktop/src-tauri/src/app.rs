//! Process entry: single-instance negotiation and Tauri app construction.

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tokio::net::windows::named_pipe::NamedPipeServer;
use upscalee_core::pipeline::clean_stale_scratch;
use upscalee_core::Settings;
use upscalee_ipc::client::{self, ClientError};
use upscalee_ipc::{Message, Origin, Point, Request};

use crate::cli::LaunchCommand;
use crate::integration::{self, ClassicStatus, Registry};
use crate::jobs::JobManager;
use crate::paths::AppPaths;
use crate::picker::PickRequest;
use crate::state::AppState;
use crate::{cli, commands, hud, ipc_server, logging, picker, radial, settings_window, tray, win32};

pub fn main() -> i32 {
    let command = cli::parse(std::env::args_os().skip(1));
    let paths = AppPaths::detect();
    let _log_guard = logging::init(&paths.logs_dir);
    tracing::info!(version = env!("CARGO_PKG_VERSION"), ?command, exe = %paths.exe.display(), "Upscalee starting");

    match command {
        LaunchCommand::Register => return register(&paths),
        LaunchCommand::Unregister => return unregister(),
        _ => {}
    }

    let pipe_name = upscalee_ipc::pipe_name();
    let server = match ipc_server::claim(&pipe_name) {
        Ok(server) => Some(server),
        Err(err) => {
            tracing::info!(%err, "another instance is running; forwarding");
            match forward(&command) {
                Ok(()) => return 0,
                Err(err) => {
                    tracing::warn!(%err, "forwarding failed; starting without single-instance pipe");
                    None
                }
            }
        }
    };

    run(command, paths, server, pipe_name);
    0
}

fn register(paths: &AppPaths) -> i32 {
    let mut settings = Settings::load(&paths.settings_file);
    settings.explorer_integration = true;
    if let Err(err) = settings.save(&paths.settings_file) {
        tracing::warn!(%err, "could not save settings during register");
    }
    for warning in commands::register_everything(paths, &settings) {
        tracing::warn!(%warning, "register");
    }
    0
}

fn unregister() -> i32 {
    for warning in commands::unregister_everything() {
        tracing::warn!(%warning, "unregister");
    }
    0
}

/// Send this process' command to the running instance.
fn forward(command: &LaunchCommand) -> Result<(), ClientError> {
    let request = match command {
        LaunchCommand::Pick(paths) => Request::Pick {
            paths: paths.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
            cursor: win32::cursor_position().map(|(x, y)| Point { x, y }),
            origin: Origin::CommandLine,
        },
        LaunchCommand::Default | LaunchCommand::Settings => Request::ShowSettings,
        _ => Request::Ping,
    };
    client::send(&Message::new(request), Duration::from_secs(5)).map(|_| ())
}

fn create_overlay_windows(app: &tauri::App) -> tauri::Result<()> {
    // Created once, hidden, so the radial menu appears instantly on demand.
    WebviewWindowBuilder::new(app, radial::LABEL, WebviewUrl::App("radial.html".into()))
        .title("Upscalee")
        .inner_size(radial::LOGICAL_SIZE, radial::LOGICAL_SIZE)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()?;
    WebviewWindowBuilder::new(app, hud::LABEL, WebviewUrl::App("hud.html".into()))
        .title("Upscalee activity")
        .inner_size(hud::WIDTH, 120.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()?;
    Ok(())
}

/// Keep the Explorer entry pointing at this executable after updates or moves.
fn self_repair(paths: &AppPaths, settings: &Settings) {
    if !paths.is_installed_layout() {
        return;
    }
    // Left behind by the installer when Explorer had the old DLL loaded.
    let _ = std::fs::remove_file(paths.install_dir.join("UpscaleeShell.dll.old"));
    let reg = Registry::user();
    if settings.explorer_integration
        && integration::classic_status(&reg, &paths.exe, paths.shell_dll.as_deref()) != ClassicStatus::Installed
    {
        tracing::info!("repairing Explorer integration");
        if let Err(err) = integration::install_classic(&reg, &paths.exe, paths.shell_dll.as_deref()) {
            tracing::warn!(%err, "could not repair Explorer integration");
        } else {
            integration::notify_shell();
        }
    }
    if settings.run_at_startup {
        let _ = integration::set_run_at_startup(&reg, true, &paths.exe);
    }
}

fn handle_initial(app: &tauri::AppHandle, command: LaunchCommand) {
    let state = app.state::<AppState>();
    match command {
        LaunchCommand::Pick(paths) => {
            let _ = state.picker.send(PickRequest { paths, cursor: win32::cursor_position(), immediate: false });
        }
        LaunchCommand::Default | LaunchCommand::Settings => settings_window::open(app),
        LaunchCommand::Background | LaunchCommand::Register | LaunchCommand::Unregister => {}
    }
}

fn run(initial: LaunchCommand, paths: AppPaths, server: Option<NamedPipeServer>, pipe_name: String) {
    let settings = Settings::load(&paths.settings_file);
    clean_stale_scratch(&paths.temp_dir, Duration::from_secs(60 * 60));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            radial::radial_current,
            radial::radial_ready,
            radial::radial_select,
            radial::radial_cancel,
            hud::hud_set_height,
            hud::hud_hide,
            hud::jobs_snapshot,
            hud::job_cancel,
            hud::jobs_cancel_all,
            hud::jobs_clear_finished,
            hud::job_open,
            hud::job_reveal,
            commands::settings_get,
            commands::settings_defaults,
            commands::settings_save,
            commands::settings_pick_folder,
            commands::integration_status,
            commands::integration_install,
            commands::integration_remove,
            commands::engine_status,
            commands::engine_probe,
            commands::app_info,
            commands::open_logs,
        ])
        .on_window_event(|window, event| match (window.label(), event) {
            (radial::LABEL, WindowEvent::Focused(false)) => radial::on_focus_lost(window.app_handle()),
            (radial::LABEL | hud::LABEL, WindowEvent::CloseRequested { api, .. }) => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let jobs = JobManager::new(handle.clone());
            let picker = picker::start(handle.clone());
            self_repair(&paths, &settings);
            app.manage(AppState::new(paths, settings, jobs, picker));
            create_overlay_windows(app)?;
            tray::create(app)?;
            if let Some(server) = server {
                ipc_server::spawn(handle.clone(), server, pipe_name);
            }
            handle_initial(&handle, initial);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build the Upscalee app");

    app.run(|app, event| {
        if let RunEvent::ExitRequested { api, code, .. } = event {
            let quitting = app.state::<AppState>().quitting.load(Ordering::SeqCst);
            // Closing Settings must not quit the tray app.
            if code.is_none() && !quitting {
                api.prevent_exit();
            }
        }
    });
}
