//! Where everything lives on disk.

use std::path::{Path, PathBuf};

use upscalee_core::engine::EngineConfig;

pub const APP_DIR_NAME: &str = "Upscalee";
/// Must equal `identifier` in tauri.conf.json. App data lives in folders named
/// after it (the per-user installer already uses `%LOCALAPPDATA%\Upscalee` as
/// the *install* folder), and the uninstaller's "Delete the application data"
/// option removes exactly these folders.
pub const DATA_DIR_NAME: &str = "io.github.chamathdilshanc.upscalee";
pub const SHELL_DLL_INSTALLED: &str = "UpscaleeShell.dll";
/// Name produced by `cargo build -p upscalee-shell` (dev layout).
pub const SHELL_DLL_DEV: &str = "upscalee_shell.dll";
pub const SPARSE_PACKAGE: &str = "UpscaleeContextMenu.msix";
/// Written into the install folder by the NSIS installer.
pub const UNINSTALLER: &str = "uninstall.exe";

#[derive(Debug, Clone)]
pub struct AppPaths {
    /// `%LOCALAPPDATA%\<identifier>\logs`
    pub logs_dir: PathBuf,
    /// `%LOCALAPPDATA%\<identifier>\temp` – per-job scratch folders.
    pub temp_dir: PathBuf,
    /// `%APPDATA%\<identifier>\settings.json`
    pub settings_file: PathBuf,
    /// `Pictures\Upscalee` – used when the output folder isn't writable.
    pub fallback_output_dir: PathBuf,
    pub exe: PathBuf,
    pub install_dir: PathBuf,
    pub engine_dir: Option<PathBuf>,
    pub shell_dll: Option<PathBuf>,
    pub sparse_package: Option<PathBuf>,
}

impl AppPaths {
    pub fn detect() -> Self {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("Upscalee.exe"));
        let install_dir = exe.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        let config_dir = dirs::config_dir().unwrap_or_else(|| install_dir.clone()).join(DATA_DIR_NAME);
        let data_dir = dirs::data_local_dir().unwrap_or_else(|| install_dir.clone()).join(DATA_DIR_NAME);
        let fallback_output_dir =
            dirs::picture_dir().or_else(dirs::home_dir).unwrap_or_else(|| data_dir.clone()).join(APP_DIR_NAME);

        let shell_dll =
            [SHELL_DLL_INSTALLED, SHELL_DLL_DEV].into_iter().map(|name| install_dir.join(name)).find(|p| p.is_file());
        let sparse_package = Some(install_dir.join(SPARSE_PACKAGE)).filter(|p| p.is_file());

        Self {
            logs_dir: data_dir.join("logs"),
            temp_dir: data_dir.join("temp"),
            settings_file: config_dir.join("settings.json"),
            engine_dir: locate_engine(&install_dir),
            fallback_output_dir,
            exe,
            install_dir,
            shell_dll,
            sparse_package,
        }
    }

    pub fn engine(&self) -> Option<EngineConfig> {
        self.engine_dir.as_deref().and_then(EngineConfig::from_dir)
    }

    /// True only for a copy installed by the setup program. `tauri build` also
    /// copies resources (DLL, engine) next to `target\release\Upscalee.exe`, so
    /// the installer's `uninstall.exe` is the marker — a dev build must never
    /// register itself in Explorer.
    pub fn is_installed_layout(&self) -> bool {
        is_installed_dir(&self.install_dir)
    }
}

pub fn is_installed_dir(dir: &Path) -> bool {
    dir.join(UNINSTALLER).is_file() && dir.join(SHELL_DLL_INSTALLED).is_file()
}

fn locate_engine(install_dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::var_os("UPSCALEE_ENGINE_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    candidates.push(install_dir.join("engine"));
    if cfg!(debug_assertions) {
        // Dev layout: <repo>/engine/bin, populated by scripts/fetch-engine.ps1.
        candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/bin"));
    }
    candidates
        .into_iter()
        .find(|dir| EngineConfig::from_dir(dir).is_some())
        .map(|dir| dir.canonicalize().map(strip_verbatim).unwrap_or(dir))
}

/// `\\?\C:\x` → `C:\x` (child processes and the UI prefer plain paths).
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_verbatim_prefix_but_not_unc() {
        assert_eq!(strip_verbatim(PathBuf::from(r"\\?\C:\a")), PathBuf::from(r"C:\a"));
        assert_eq!(strip_verbatim(PathBuf::from(r"\\?\UNC\srv\share")), PathBuf::from(r"\\?\UNC\srv\share"));
    }

    #[test]
    fn detect_produces_consistent_layout() {
        let paths = AppPaths::detect();
        assert!(paths.settings_file.ends_with(Path::new(DATA_DIR_NAME).join("settings.json")));
        assert!(!paths.logs_dir.starts_with(&paths.install_dir), "data must not live in the install folder");
        assert!(paths.logs_dir.ends_with(Path::new(DATA_DIR_NAME).join("logs")));
        assert_eq!(paths.logs_dir.parent(), paths.temp_dir.parent());
        assert!(paths.fallback_output_dir.ends_with(APP_DIR_NAME));
    }

    #[test]
    fn only_installer_layouts_count_as_installed() {
        let dir = tempfile::tempdir().unwrap();
        // Shape of target\release after `tauri build`: DLL present, no uninstaller.
        std::fs::write(dir.path().join(SHELL_DLL_INSTALLED), b"").unwrap();
        assert!(!is_installed_dir(dir.path()));
        std::fs::write(dir.path().join(UNINSTALLER), b"").unwrap();
        assert!(is_installed_dir(dir.path()));
    }

    #[test]
    fn data_dir_matches_bundle_identifier() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], DATA_DIR_NAME);
    }
}
