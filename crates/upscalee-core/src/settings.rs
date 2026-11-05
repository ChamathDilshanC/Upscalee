//! Persisted user settings (`%APPDATA%\Upscalee\settings.json`).

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::formats::{ModelProfile, OutputFormat};
use crate::preset::Preset;

pub const MAX_CONCURRENT_JOBS: u8 = 4;
pub const MIN_TILE_SIZE: u32 = 32;
pub const MAX_TILE_SIZE: u32 = 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `None` → save next to the source image.
    pub output_folder: Option<PathBuf>,
    pub output_format: OutputFormat,
    pub default_profile: ModelProfile,
    /// Engine tile size; `0` = automatic (chosen by the engine from free VRAM).
    pub tile_size: u32,
    /// Vulkan device index; `None` = engine default.
    pub gpu: Option<u32>,
    pub concurrent_jobs: u8,
    pub run_at_startup: bool,
    pub skip_if_large_enough: bool,
    pub open_folder_when_done: bool,
    /// Last preset picked in the radial menu (pre-highlighted next time).
    pub last_preset: Option<Preset>,
    /// Keep the Explorer context-menu entry registered. Turned off when the user
    /// removes the integration from Settings, so self-repair doesn't re-add it.
    pub explorer_integration: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_folder: None,
            output_format: OutputFormat::Keep,
            default_profile: ModelProfile::Photo,
            tile_size: 0,
            gpu: None,
            concurrent_jobs: 1,
            run_at_startup: true,
            skip_if_large_enough: true,
            open_folder_when_done: false,
            last_preset: None,
            explorer_integration: true,
        }
    }
}

impl Settings {
    /// Clamp values a hand-edited file could break.
    pub fn sanitized(mut self) -> Self {
        self.concurrent_jobs = self.concurrent_jobs.clamp(1, MAX_CONCURRENT_JOBS);
        if self.tile_size != 0 {
            self.tile_size = self.tile_size.clamp(MIN_TILE_SIZE, MAX_TILE_SIZE);
        }
        if self.output_folder.as_ref().is_some_and(|p| p.as_os_str().is_empty()) {
            self.output_folder = None;
        }
        self
    }

    /// Load settings; a missing or corrupt file yields defaults (never fails).
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<Settings>(&bytes) {
                Ok(s) => s.sanitized(),
                Err(err) => {
                    tracing::warn!(%err, path = %path.display(), "settings file is corrupt; using defaults");
                    Settings::default()
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(err) => {
                tracing::warn!(%err, path = %path.display(), "could not read settings; using defaults");
                Settings::default()
            }
        }
    }

    /// Atomically write settings (temp file + rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)?;
        let mut tmp = tempfile::Builder::new().prefix(".settings-").suffix(".tmp").tempfile_in(dir)?;
        serde_json::to_writer_pretty(&mut tmp, self)?;
        tmp.write_all(b"\n")?;
        tmp.as_file().sync_all()?;
        tmp.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_spec() {
        let s = Settings::default();
        assert_eq!(s.output_folder, None);
        assert_eq!(s.output_format, OutputFormat::Keep);
        assert_eq!(s.default_profile, ModelProfile::Photo);
        assert_eq!(s.tile_size, 0);
        assert_eq!(s.concurrent_jobs, 1);
        assert!(s.run_at_startup);
        assert!(s.skip_if_large_enough);
        assert!(!s.open_folder_when_done);
        assert!(s.explorer_integration);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("settings.json");
        let s = Settings {
            output_folder: Some(PathBuf::from(r"D:\Upscaled")),
            output_format: OutputFormat::Webp,
            default_profile: ModelProfile::Anime,
            tile_size: 200,
            gpu: Some(1),
            concurrent_jobs: 2,
            last_preset: Some(Preset::K8),
            ..Settings::default()
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
    }

    #[test]
    fn corrupt_or_partial_files_fall_back_gracefully() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());

        std::fs::write(&path, br#"{"concurrentJobs": 99, "tileSize": 5, "outputFolder": ""}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.concurrent_jobs, MAX_CONCURRENT_JOBS);
        assert_eq!(s.tile_size, MIN_TILE_SIZE);
        assert_eq!(s.output_folder, None);
        assert!(s.run_at_startup, "missing fields use defaults");
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(&dir.path().join("nope.json")), Settings::default());
    }
}
