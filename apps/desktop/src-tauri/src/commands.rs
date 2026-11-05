//! Commands used by the Settings window, plus the register/unregister flows
//! shared with the installer (`--register` / `--unregister`).

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use upscalee_core::engine::{self, GpuInfo};
use upscalee_core::pipeline::GpuState;
use upscalee_core::Settings;

use crate::integration::{self, ClassicStatus, ModernStatus, Registry};
use crate::paths::AppPaths;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub classic: ClassicStatus,
    pub modern: ModernStatus,
    pub shell_extension: bool,
    pub run_at_startup: bool,
    pub windows11: bool,
    pub windows_build: u32,
}

pub fn integration_status_for(paths: &AppPaths) -> IntegrationStatus {
    let reg = Registry::user();
    IntegrationStatus {
        classic: integration::classic_status(&reg, &paths.exe, paths.shell_dll.as_deref()),
        modern: integration::modern_status(paths.sparse_package.as_deref()),
        shell_extension: paths.shell_dll.is_some(),
        run_at_startup: integration::run_at_startup_enabled(&reg),
        windows11: integration::is_windows_11(),
        windows_build: integration::windows_build(),
    }
}

/// Install every integration point. Never fails hard: problems are returned as
/// human-readable warnings (the installer must not abort over them).
pub fn register_everything(paths: &AppPaths, settings: &Settings) -> Vec<String> {
    let reg = Registry::user();
    let mut warnings = Vec::new();
    if let Err(err) = integration::install_classic(&reg, &paths.exe, paths.shell_dll.as_deref()) {
        warnings.push(format!("context menu: {err}"));
    }
    if let Err(err) = integration::set_run_at_startup(&reg, settings.run_at_startup, &paths.exe) {
        warnings.push(format!("run at startup: {err}"));
    }
    if integration::is_windows_11() {
        if let Some(msix) = &paths.sparse_package {
            if let Err(err) = integration::install_modern(msix, &paths.install_dir) {
                warnings.push(format!("Windows 11 menu: {err}"));
            }
        }
    }
    integration::notify_shell();
    warnings
}

pub fn unregister_everything() -> Vec<String> {
    let reg = Registry::user();
    let mut warnings = Vec::new();
    if let Err(err) = integration::remove_classic(&reg) {
        warnings.push(format!("context menu: {err}"));
    }
    if let Err(err) = integration::set_run_at_startup(&reg, false, std::path::Path::new("")) {
        warnings.push(format!("run at startup: {err}"));
    }
    if integration::is_windows_11() {
        if let Err(err) = integration::remove_modern() {
            warnings.push(format!("Windows 11 menu: {err}"));
        }
    }
    integration::notify_shell();
    warnings
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn settings_defaults() -> Settings {
    Settings::default()
}

#[tauri::command]
pub fn settings_save(state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    let previous = state.settings();
    let saved = state.store_settings(settings).map_err(|e| format!("Could not save settings: {e}"))?;
    if previous.run_at_startup != saved.run_at_startup {
        integration::set_run_at_startup(&Registry::user(), saved.run_at_startup, &state.paths.exe)
            .map_err(|e| format!("Saved, but could not update Run at startup: {e}"))?;
    }
    Ok(saved)
}

#[tauri::command]
pub async fn settings_pick_folder(app: AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Choose where upscaled images are saved")
            .blocking_pick_folder()
            .and_then(|f| f.into_path().ok())
            .map(|p| p.display().to_string())
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
pub async fn integration_status(app: AppHandle) -> IntegrationStatus {
    let paths = app.state::<AppState>().paths.clone();
    tauri::async_runtime::spawn_blocking(move || integration_status_for(&paths)).await.expect("status worker")
}

#[tauri::command]
pub async fn integration_install(app: AppHandle) -> Result<IntegrationStatus, String> {
    let state = app.state::<AppState>();
    state.update_settings(|s| s.explorer_integration = true);
    let paths = state.paths.clone();
    let settings = state.settings();
    tauri::async_runtime::spawn_blocking(move || {
        let warnings = register_everything(&paths, &settings);
        let status = integration_status_for(&paths);
        if warnings.is_empty() || status.classic == ClassicStatus::Installed {
            for w in warnings {
                tracing::warn!(warning = %w, "integration install");
            }
            Ok(status)
        } else {
            Err(warnings.join("; "))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn integration_remove(app: AppHandle) -> Result<IntegrationStatus, String> {
    let state = app.state::<AppState>();
    state.update_settings(|s| {
        s.explorer_integration = false;
        s.run_at_startup = false;
    });
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let warnings = unregister_everything();
        for w in &warnings {
            tracing::warn!(warning = %w, "integration remove");
        }
        integration_status_for(&paths)
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub installed: bool,
    pub path: Option<String>,
    pub photo_model: bool,
    pub anime_model: bool,
    pub gpu_state: GpuState,
    pub gpus: Option<Vec<GpuInfo>>,
}

#[tauri::command]
pub fn engine_status(state: State<'_, AppState>) -> EngineStatus {
    let engine = state.base_engine.as_ref();
    EngineStatus {
        installed: engine.is_some(),
        path: engine.map(|e| e.exe.display().to_string()),
        photo_model: engine.is_some_and(|e| e.has_model("realesrgan-x4plus")),
        anime_model: engine.is_some_and(|e| e.has_model("realesrgan-x4plus-anime")),
        gpu_state: state.gpu.state(),
        gpus: state.gpus.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    }
}

#[tauri::command]
pub async fn engine_probe(app: AppHandle) -> Result<Vec<GpuInfo>, String> {
    let state = app.state::<AppState>();
    let engine = state.base_engine.clone().ok_or("The upscaling engine is not installed.")?;
    let scratch = state.paths.temp_dir.clone();
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    match engine::probe_gpus(&engine, &scratch).await {
        Ok(gpus) => {
            state.gpu.set(GpuState::Available);
            *state.gpus.lock().unwrap_or_else(|e| e.into_inner()) = Some(gpus.clone());
            Ok(gpus)
        }
        Err(engine::EngineError::NoGpu) => {
            state.gpu.set(GpuState::Missing);
            *state.gpus.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
            Ok(Vec::new())
        }
        Err(err) => Err(err.to_string()),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub logs_dir: String,
    pub settings_file: String,
    pub install_dir: String,
    pub engine_version: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle, state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        logs_dir: state.paths.logs_dir.display().to_string(),
        settings_file: state.paths.settings_file.display().to_string(),
        install_dir: state.paths.install_dir.display().to_string(),
        engine_version: "Real-ESRGAN ncnn Vulkan 20220424 (v0.2.5.0)".into(),
    }
}

#[tauri::command]
pub fn open_logs(app: AppHandle) {
    crate::tray::open_logs(&app);
}
