//! Process-wide application state (managed by Tauri).

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use upscalee_core::engine::{EngineConfig, GpuInfo};
use upscalee_core::pipeline::{GpuAvailability, PipelineEnv};
use upscalee_core::Settings;

use crate::hud::HudState;
use crate::jobs::JobManager;
use crate::paths::AppPaths;
use crate::picker::PickSender;
use crate::radial::RadialState;

pub struct AppState {
    pub paths: AppPaths,
    settings: RwLock<Settings>,
    pub base_engine: Option<EngineConfig>,
    pub gpu: Arc<GpuAvailability>,
    pub gpus: Mutex<Option<Vec<GpuInfo>>>,
    pub radial: Mutex<RadialState>,
    pub hud: Mutex<HudState>,
    pub jobs: Arc<JobManager>,
    pub picker: PickSender,
    pub quitting: AtomicBool,
}

impl AppState {
    pub fn new(paths: AppPaths, settings: Settings, jobs: Arc<JobManager>, picker: PickSender) -> Self {
        let base_engine = paths.engine();
        match &base_engine {
            Some(engine) => tracing::info!(exe = %engine.exe.display(), "upscale engine found"),
            None => tracing::warn!("upscale engine not found; AI upscaling unavailable (Lanczos3 fallback)"),
        }
        Self {
            paths,
            settings: RwLock::new(settings),
            base_engine,
            gpu: Arc::default(),
            gpus: Mutex::new(None),
            radial: Mutex::new(RadialState::default()),
            hud: Mutex::new(HudState::default()),
            jobs,
            picker,
            quitting: AtomicBool::new(false),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Replace settings in memory and persist them.
    pub fn store_settings(&self, settings: Settings) -> std::io::Result<Settings> {
        let settings = settings.sanitized();
        settings.save(&self.paths.settings_file)?;
        *self.settings.write().unwrap_or_else(|e| e.into_inner()) = settings.clone();
        Ok(settings)
    }

    /// Update settings with a closure; persistence errors are logged, not fatal.
    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        let mut next = self.settings();
        f(&mut next);
        if let Err(err) = self.store_settings(next) {
            tracing::warn!(%err, "could not save settings");
        }
    }

    /// Pipeline environment reflecting the current settings (GPU / tile size).
    pub fn pipeline_env(&self) -> PipelineEnv {
        let settings = self.settings();
        PipelineEnv {
            engine: self.base_engine.clone().map(|e| e.with_overrides(settings.gpu, settings.tile_size)),
            gpu: self.gpu.clone(),
            temp_root: self.paths.temp_dir.clone(),
            fallback_output_dir: self.paths.fallback_output_dir.clone(),
        }
    }
}
