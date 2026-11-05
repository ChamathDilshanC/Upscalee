//! The radial quality picker: session building, window placement, commands.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State};
use upscalee_core::imaging::{self, ImageInfo};
use upscalee_core::pipeline::{GpuState, JobRequest};
use upscalee_core::planner::{target_dimensions, Dimensions};
use upscalee_core::{ModelProfile, OutputFormat, Preset, Settings};

use crate::state::AppState;
use crate::{hud, win32};

pub const LABEL: &str = "radial";
pub const LOGICAL_SIZE: f64 = 420.0;
pub const SESSION_EVENT: &str = "radial://session";
/// Focus can flicker right after showing; ignore blur events this soon.
const BLUR_GRACE: Duration = Duration::from_millis(350);
/// Show the window even if the web view never acknowledged the session.
const SHOW_FALLBACK: Duration = Duration::from_millis(900);
const MAX_LISTED_FILES: usize = 5;

#[derive(Debug, Clone)]
pub struct PickedFile {
    pub path: PathBuf,
    pub info: Result<ImageInfo, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    pub name: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    /// Every file is smaller than the preset.
    Ready,
    /// Some files are already that large and will be skipped.
    Partial,
    /// Every file is already that large.
    AlreadyLarger,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetOption {
    pub preset: Preset,
    pub long_edge: u32,
    pub availability: Availability,
    /// Files that would actually be upscaled.
    pub eligible: usize,
    /// Exact output size (single-file sessions only).
    pub target: Option<Dimensions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineMode {
    Ai,
    NoGpu,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadialSession {
    pub id: u64,
    pub count: usize,
    /// `1 × PNG`, `3 × JPG`, `4 images`.
    pub summary: String,
    /// `1920 × 1080` or `up to 4000 px`.
    pub detail: String,
    pub files: Vec<FileSummary>,
    pub unreadable: Vec<String>,
    pub presets: Vec<PresetOption>,
    pub profile: ModelProfile,
    pub format: OutputFormat,
    pub highlighted: Option<Preset>,
    pub engine: EngineMode,
}

struct ActiveSession {
    session: RadialSession,
    files: Vec<PickedFile>,
}

#[derive(Default)]
pub struct RadialState {
    active: Option<ActiveSession>,
    next_id: u64,
    shown_at: Option<Instant>,
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// Pure: everything the radial UI needs to render.
pub fn build_session(id: u64, files: &[PickedFile], settings: &Settings, engine: EngineMode) -> RadialSession {
    let readable: Vec<(&PickedFile, &ImageInfo)> =
        files.iter().filter_map(|f| f.info.as_ref().ok().map(|i| (f, i))).collect();
    let unreadable = files.iter().filter(|f| f.info.is_err()).map(|f| file_name(&f.path)).collect();
    let count = readable.len();

    let mut formats: Vec<&str> = readable.iter().map(|(_, i)| i.format.label()).collect();
    formats.dedup();
    let all_same = formats.windows(2).all(|w| w[0] == w[1]);
    let summary = match (count, all_same) {
        (0, _) => "No images".to_owned(),
        (n, true) => format!("{n} × {}", formats[0]),
        (n, false) => format!("{n} images"),
    };
    let detail = match readable.as_slice() {
        [] => String::new(),
        [(_, info)] => info.dimensions.to_string(),
        many => {
            let largest = many.iter().map(|(_, i)| i.dimensions.long_edge()).max().unwrap_or(0);
            format!("up to {largest} px")
        }
    };

    let presets: Vec<PresetOption> = Preset::ALL
        .iter()
        .map(|&preset| {
            let eligible = readable
                .iter()
                .filter(|(_, i)| !settings.skip_if_large_enough || i.dimensions.long_edge() < preset.long_edge())
                .count();
            let availability = if eligible == count && count > 0 {
                Availability::Ready
            } else if eligible > 0 {
                Availability::Partial
            } else {
                Availability::AlreadyLarger
            };
            let target = match readable.as_slice() {
                [(_, info)] => Some(target_dimensions(info.dimensions, preset.long_edge())),
                _ => None,
            };
            PresetOption { preset, long_edge: preset.long_edge(), availability, eligible, target }
        })
        .collect();

    let usable = |p: Preset| presets.iter().any(|o| o.preset == p && o.availability != Availability::AlreadyLarger);
    let highlighted = settings.last_preset.filter(|p| usable(*p)).or_else(|| {
        presets
            .iter()
            .find(|o| o.availability == Availability::Ready)
            .or_else(|| presets.iter().find(|o| o.availability == Availability::Partial))
            .map(|o| o.preset)
    });

    RadialSession {
        id,
        count,
        summary,
        detail,
        files: readable
            .iter()
            .take(MAX_LISTED_FILES)
            .map(|(f, i)| FileSummary {
                name: file_name(&f.path),
                format: i.format.label().to_owned(),
                width: i.dimensions.width,
                height: i.dimensions.height,
            })
            .collect(),
        unreadable,
        presets,
        profile: settings.default_profile,
        format: settings.output_format,
        highlighted,
        engine,
    }
}

/// Read image headers, in parallel for larger selections.
fn read_infos(paths: Vec<PathBuf>) -> Vec<PickedFile> {
    let read = |path: PathBuf| {
        let info = imaging::read_info(&path).map_err(|e| e.to_string());
        PickedFile { path, info }
    };
    if paths.len() <= 4 {
        return paths.into_iter().map(read).collect();
    }
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let chunk = paths.len().div_ceil(workers);
    std::thread::scope(|scope| {
        let handles: Vec<_> = paths
            .chunks(chunk)
            .map(|c| {
                let c = c.to_vec();
                scope.spawn(move || c.into_iter().map(read).collect::<Vec<_>>())
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

fn engine_mode(state: &AppState, profile: ModelProfile) -> EngineMode {
    match &state.base_engine {
        Some(engine) if engine.has_model(profile.model_name()) => {
            if state.gpu.state() == GpuState::Missing {
                EngineMode::NoGpu
            } else {
                EngineMode::Ai
            }
        }
        _ => EngineMode::Missing,
    }
}

fn place_window(app: &AppHandle, center: Option<(i32, i32)>) -> win32::WorkArea {
    let area = center.map(|(x, y)| win32::work_area_at(x, y)).unwrap_or_else(win32::primary_work_area);
    if let Some(window) = app.get_webview_window(LABEL) {
        let center = center.unwrap_or_else(|| area.center());
        let size = (LOGICAL_SIZE * area.scale).round() as i32;
        let (x, y) = win32::centered_square(center, size, &area);
        // Move first (may trigger a DPI change on another monitor), then size, then move again.
        let _ = window.set_position(PhysicalPosition::new(x, y));
        let _ = window.set_size(PhysicalSize::new(size as u32, size as u32));
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    area
}

/// Entry point from the pick aggregator.
pub async fn open(app: &AppHandle, paths: Vec<PathBuf>, cursor: Option<(i32, i32)>) {
    let files = tauri::async_runtime::spawn_blocking(move || read_infos(paths)).await.unwrap_or_default();
    let state = app.state::<AppState>();

    if files.iter().all(|f| f.info.is_err()) {
        for file in &files {
            if let Err(err) = &file.info {
                state.jobs.add_failed(&file.path, format!("Could not read the image: {err}"));
            }
        }
        hud::show(app);
        return;
    }

    let settings = state.settings();
    let engine = engine_mode(&state, settings.default_profile);
    let session = {
        let mut radial = state.radial.lock().unwrap_or_else(|e| e.into_inner());
        radial.next_id += 1;
        let session = build_session(radial.next_id, &files, &settings, engine);
        radial.active = Some(ActiveSession { session: session.clone(), files });
        radial.shown_at = None;
        session
    };

    let center = cursor.or_else(win32::cursor_position);
    let area = place_window(app, center);
    state.hud.lock().unwrap_or_else(|e| e.into_inner()).area = Some(area);
    tracing::debug!(id = session.id, count = session.count, "radial session ready");
    let _ = app.emit_to(LABEL, SESSION_EVENT, &session);

    let app = app.clone();
    let id = session.id;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_FALLBACK).await;
        reveal(&app, id);
    });
}

/// Show and focus the radial window for session `id` (once).
pub fn reveal(app: &AppHandle, id: u64) {
    let state = app.state::<AppState>();
    {
        let mut radial = state.radial.lock().unwrap_or_else(|e| e.into_inner());
        match &radial.active {
            Some(active) if active.session.id == id && radial.shown_at.is_none() => {}
            _ => return,
        }
        radial.shown_at = Some(Instant::now());
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        if let Ok(hwnd) = window.hwnd() {
            win32::force_foreground(win32::hwnd_from_raw(hwnd.0 as _));
        }
    }
}

fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

fn take_active(state: &AppState, id: Option<u64>) -> Option<ActiveSession> {
    let mut radial = state.radial.lock().unwrap_or_else(|e| e.into_inner());
    let matches = match (&radial.active, id) {
        (Some(active), Some(id)) => active.session.id == id,
        (Some(_), None) => true,
        (None, _) => false,
    };
    radial.shown_at = None;
    if matches {
        radial.active.take()
    } else {
        None
    }
}

pub fn on_focus_lost(app: &AppHandle) {
    let state = app.state::<AppState>();
    let recent = {
        let radial = state.radial.lock().unwrap_or_else(|e| e.into_inner());
        if radial.active.is_none() {
            return;
        }
        radial.shown_at.is_none_or(|t| t.elapsed() < BLUR_GRACE)
    };
    if !recent {
        take_active(&state, None);
        hide(app);
    }
}

#[tauri::command]
pub fn radial_current(state: State<'_, AppState>) -> Option<RadialSession> {
    let radial = state.radial.lock().unwrap_or_else(|e| e.into_inner());
    radial.active.as_ref().map(|a| a.session.clone())
}

#[tauri::command]
pub fn radial_ready(app: AppHandle, id: u64) {
    reveal(&app, id);
}

#[tauri::command]
pub fn radial_cancel(app: AppHandle, state: State<'_, AppState>) {
    take_active(&state, None);
    hide(&app);
}

#[tauri::command]
pub fn radial_select(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    preset: Preset,
    profile: ModelProfile,
    format: OutputFormat,
) -> Result<(), String> {
    let active = take_active(&state, Some(id));
    hide(&app);
    let Some(active) = active else {
        return Err("This menu has expired. Right-click the image again.".into());
    };
    let settings = state.settings();
    state.update_settings(|s| s.last_preset = Some(preset));

    let mut requests = Vec::new();
    for file in active.files {
        match file.info {
            Ok(_) => requests.push(JobRequest {
                source: file.path,
                preset,
                profile,
                format,
                output_folder: settings.output_folder.clone(),
                skip_if_large: settings.skip_if_large_enough,
            }),
            Err(err) => state.jobs.add_failed(&file.path, format!("Could not read the image: {err}")),
        }
    }
    tracing::info!(count = requests.len(), %preset, ?profile, ?format, "upscale requested");
    state.jobs.enqueue(requests);
    hud::show(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use upscalee_core::SourceFormat;

    fn file(name: &str, w: u32, h: u32, format: SourceFormat) -> PickedFile {
        PickedFile {
            path: PathBuf::from(format!(r"C:\pics\{name}")),
            info: Ok(ImageInfo { dimensions: Dimensions::new(w, h), format }),
        }
    }

    #[test]
    fn single_file_session() {
        let files = [file("photo.png", 1920, 1080, SourceFormat::Png)];
        let s = build_session(7, &files, &Settings::default(), EngineMode::Ai);
        assert_eq!(s.summary, "1 × PNG");
        assert_eq!(s.detail, "1920 × 1080");
        assert_eq!(s.presets[0].availability, Availability::Ready); // 2K > 1920
        assert_eq!(s.presets[1].target, Some(Dimensions::new(3840, 2160)));
        assert_eq!(s.highlighted, Some(Preset::K2));
    }

    #[test]
    fn mixed_selection_marks_partial_and_skipped_presets() {
        let files = [
            file("a.jpg", 1000, 800, SourceFormat::Jpeg),
            file("b.png", 3000, 2000, SourceFormat::Png),
            PickedFile { path: PathBuf::from(r"C:\pics\broken.png"), info: Err("bad".into()) },
        ];
        let settings = Settings { last_preset: Some(Preset::K2), ..Settings::default() };
        let s = build_session(1, &files, &settings, EngineMode::Ai);
        assert_eq!(s.count, 2);
        assert_eq!(s.summary, "2 images");
        assert_eq!(s.detail, "up to 3000 px");
        assert_eq!(s.unreadable, vec!["broken.png".to_owned()]);
        assert_eq!(s.presets[0].availability, Availability::Partial); // 2K
        assert_eq!(s.presets[0].eligible, 1);
        assert_eq!(s.presets[1].availability, Availability::Ready); // 4K
        assert!(s.presets[0].target.is_none());
        assert_eq!(s.highlighted, Some(Preset::K2), "last preset kept while usable");
    }

    #[test]
    fn already_larger_presets_are_never_highlighted() {
        let files = [file("big.png", 5000, 3000, SourceFormat::Png)];
        let settings = Settings { last_preset: Some(Preset::K4), ..Settings::default() };
        let s = build_session(1, &files, &settings, EngineMode::Ai);
        assert_eq!(s.presets[1].availability, Availability::AlreadyLarger);
        assert_eq!(s.highlighted, Some(Preset::K8));

        let no_skip = Settings { skip_if_large_enough: false, ..settings };
        let s = build_session(1, &files, &no_skip, EngineMode::Ai);
        assert!(s.presets.iter().all(|p| p.availability == Availability::Ready));
    }

    #[test]
    fn same_format_multi_selection_summary() {
        let files = [
            file("a.png", 100, 100, SourceFormat::Png),
            file("b.png", 200, 100, SourceFormat::Png),
            file("c.png", 300, 100, SourceFormat::Png),
        ];
        let s = build_session(1, &files, &Settings::default(), EngineMode::NoGpu);
        assert_eq!(s.summary, "3 × PNG");
        assert_eq!(s.engine, EngineMode::NoGpu);
    }
}
