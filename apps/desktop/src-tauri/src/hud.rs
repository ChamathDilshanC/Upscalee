//! Activity HUD: a small always-on-top panel in the bottom-right corner that
//! shows progress and "Open / Show in folder" actions. It never steals focus.

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, State};
use tauri_plugin_opener::OpenerExt;

use crate::jobs::JobView;
use crate::state::AppState;
use crate::win32::{self, WorkArea};

pub const LABEL: &str = "hud";
pub const WIDTH: f64 = 380.0;
const DEFAULT_HEIGHT: f64 = 120.0;
const MARGIN: f64 = 12.0;
const MIN_HEIGHT: f64 = 48.0;
const MAX_HEIGHT: f64 = 560.0;

#[derive(Default)]
pub struct HudState {
    /// Content height reported by the web view (logical px).
    pub height: Option<f64>,
    /// Monitor of the last radial menu; the HUD follows the user there.
    pub area: Option<WorkArea>,
}

fn place(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    let state = app.state::<AppState>();
    let (area, height) = {
        let hud = state.hud.lock().unwrap_or_else(|e| e.into_inner());
        (hud.area.unwrap_or_else(win32::primary_work_area), hud.height.unwrap_or(DEFAULT_HEIGHT))
    };
    let w = (WIDTH * area.scale).round() as i32;
    let h = (height * area.scale).round() as i32;
    let margin = (MARGIN * area.scale).round() as i32;
    let (x, y) = win32::bottom_right(&area, w, h, margin);
    let _ = window.set_position(PhysicalPosition::new(x, y));
    let _ = window.set_size(PhysicalSize::new(w as u32, h as u32));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

pub fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    place(app);
    match window.hwnd() {
        Ok(hwnd) => win32::show_without_activating(win32::hwnd_from_raw(hwnd.0 as _)),
        Err(_) => {
            let _ = window.show();
        }
    }
}

#[tauri::command]
pub fn hud_set_height(app: AppHandle, state: State<'_, AppState>, height: f64) {
    if !height.is_finite() {
        return;
    }
    state.hud.lock().unwrap_or_else(|e| e.into_inner()).height = Some(height.clamp(MIN_HEIGHT, MAX_HEIGHT));
    if app.get_webview_window(LABEL).and_then(|w| w.is_visible().ok()).unwrap_or(false) {
        place(&app);
    }
}

#[tauri::command]
pub fn hud_hide(app: AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn jobs_snapshot(state: State<'_, AppState>) -> Vec<JobView> {
    state.jobs.snapshot()
}

#[tauri::command]
pub fn job_cancel(state: State<'_, AppState>, id: u64) {
    state.jobs.cancel(id);
}

#[tauri::command]
pub fn jobs_cancel_all(state: State<'_, AppState>) {
    state.jobs.cancel_all();
}

#[tauri::command]
pub fn jobs_clear_finished(state: State<'_, AppState>) {
    state.jobs.clear_finished();
}

/// Open a finished job's output with the default viewer. Takes a job id (not a
/// path) so the web view can only open files Upscalee itself produced.
#[tauri::command]
pub fn job_open(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    let path = state.jobs.output_of(id).ok_or("That result is no longer available.")?;
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn job_reveal(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    let path = state.jobs.output_of(id).ok_or("That result is no longer available.")?;
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}
