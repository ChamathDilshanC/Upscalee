//! Notification-area icon and menu.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use upscalee_ipc::SUPPORTED_EXTENSIONS;

use crate::picker::PickRequest;
use crate::state::AppState;
use crate::{hud, settings_window, win32};

pub const ID: &str = "upscalee-tray";

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    let pick = MenuItem::with_id(app, "pick", "Upscale images…", true, None::<&str>)?;
    let activity = MenuItem::with_id(app, "activity", "Show activity", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let logs = MenuItem::with_id(app, "logs", "Open logs folder", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Upscalee", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &pick,
            &activity,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &logs,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id(ID)
        .tooltip("Upscalee — ready")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "pick" => pick_files(app),
            "activity" => hud::show(app),
            "settings" => settings_window::open(app),
            "logs" => open_logs(app),
            "quit" => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                let app = tray.app_handle();
                if app.state::<AppState>().jobs.has_any() {
                    hud::show(app);
                } else {
                    settings_window::open(app);
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn set_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id(ID) {
        let _ = tray.set_tooltip(Some(text));
    }
}

/// "Upscale images…": file dialog, then the radial menu at the cursor.
pub fn pick_files(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let files = app
            .dialog()
            .file()
            .set_title("Choose images to upscale")
            .add_filter("Images", SUPPORTED_EXTENSIONS)
            .blocking_pick_files();
        let paths: Vec<PathBuf> = files.unwrap_or_default().into_iter().filter_map(|f| f.into_path().ok()).collect();
        if !paths.is_empty() {
            let _ = app.state::<AppState>().picker.send(PickRequest {
                paths,
                cursor: win32::cursor_position(),
                immediate: true,
            });
        }
    });
}

pub fn open_logs(app: &AppHandle) {
    let dir = app.state::<AppState>().paths.logs_dir.clone();
    let _ = std::fs::create_dir_all(&dir);
    let _ = app.opener().open_path(dir.to_string_lossy(), None::<&str>);
}

pub fn quit_app(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.quitting.store(true, Ordering::SeqCst);
    state.jobs.cancel_all();
    app.exit(0);
}
