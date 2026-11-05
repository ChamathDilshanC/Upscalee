//! Explorer integration: context-menu verbs, COM registration, Run-at-login and
//! the Windows 11 sparse package. Everything is per-user (HKCU) – no admin.
//!
//! Classic menu (Windows 10, Windows 11 "Show more options"):
//!
//! ```text
//! HKCU\Software\Classes\SystemFileAssociations\.png\shell\Upscalee
//!     MUIVerb                = "Upscale with Upscalee"
//!     Icon                   = "<exe>,0"
//!     MultiSelectModel       = "Player"
//!     ExplorerCommandHandler = "{58A04070-…}"      (only when UpscaleeShell.dll exists)
//!     command\(Default)      = "\"<exe>\" --pick \"%1\""   (fallback)
//! HKCU\Software\Classes\CLSID\{58A04070-…}\InprocServer32
//!     (Default)              = "<install dir>\UpscaleeShell.dll"
//!     ThreadingModel         = "Apartment"
//! ```
//!
//! With `ExplorerCommandHandler`, a multi-selection arrives as one `Invoke`
//! (one radial menu). Without it, Explorer starts one process per file and the
//! app's pick aggregator merges them.

use std::io;
use std::path::Path;

use serde::Serialize;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

use upscalee_ipc::SUPPORTED_EXTENSIONS;

pub const CLSID_CLASSIC: &str = "{58A04070-9783-4B15-AFFA-426AB1287F03}";
pub const PACKAGE_NAME: &str = "Upscalee.ContextMenu";
pub const VERB: &str = "Upscalee";
pub const MENU_TEXT: &str = "Upscale with Upscalee";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Upscalee";

/// Registry location to operate on (tests use a scratch key instead of HKCU\Software\Classes).
pub struct Registry {
    classes: String,
    run: String,
}

impl Registry {
    pub fn user() -> Self {
        Self { classes: r"Software\Classes".to_owned(), run: RUN_KEY.to_owned() }
    }

    #[cfg(test)]
    fn scratch(root: &str) -> Self {
        Self { classes: format!(r"{root}\Classes"), run: format!(r"{root}\Run") }
    }

    fn hkcu() -> RegKey {
        RegKey::predef(HKEY_CURRENT_USER)
    }

    fn verb_path(&self, ext: &str) -> String {
        format!(r"{}\SystemFileAssociations\.{ext}\shell\{VERB}", self.classes)
    }

    fn clsid_path(&self) -> String {
        format!(r"{}\CLSID\{CLSID_CLASSIC}", self.classes)
    }
}

fn ignore_missing(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

pub fn command_line(exe: &Path) -> String {
    format!("\"{}\" --pick \"%1\"", exe.display())
}

pub fn icon_value(exe: &Path) -> String {
    format!("{},0", exe.display())
}

pub fn install_classic(reg: &Registry, exe: &Path, dll: Option<&Path>) -> io::Result<()> {
    let hkcu = Registry::hkcu();
    if let Some(dll) = dll {
        let (clsid, _) = hkcu.create_subkey(reg.clsid_path())?;
        clsid.set_value("", &"Upscalee Explorer Command")?;
        let (inproc, _) = clsid.create_subkey("InprocServer32")?;
        inproc.set_value("", &dll.display().to_string())?;
        inproc.set_value("ThreadingModel", &"Apartment")?;
    } else {
        ignore_missing(hkcu.delete_subkey_all(reg.clsid_path()))?;
    }

    for ext in SUPPORTED_EXTENSIONS {
        let (verb, _) = hkcu.create_subkey(reg.verb_path(ext))?;
        verb.set_value("MUIVerb", &MENU_TEXT)?;
        verb.set_value("Icon", &icon_value(exe))?;
        verb.set_value("MultiSelectModel", &"Player")?;
        if dll.is_some() {
            verb.set_value("ExplorerCommandHandler", &CLSID_CLASSIC)?;
        } else {
            ignore_missing(verb.delete_value("ExplorerCommandHandler"))?;
        }
        let (command, _) = verb.create_subkey("command")?;
        command.set_value("", &command_line(exe))?;
    }
    Ok(())
}

pub fn remove_classic(reg: &Registry) -> io::Result<()> {
    let hkcu = Registry::hkcu();
    for ext in SUPPORTED_EXTENSIONS {
        ignore_missing(hkcu.delete_subkey_all(reg.verb_path(ext)))?;
    }
    ignore_missing(hkcu.delete_subkey_all(reg.clsid_path()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClassicStatus {
    /// Registered and pointing at this installation.
    Installed,
    /// Registered, but for another path / without the shell extension.
    Outdated,
    Missing,
}

pub fn classic_status(reg: &Registry, exe: &Path, dll: Option<&Path>) -> ClassicStatus {
    let hkcu = Registry::hkcu();
    let mut found = 0;
    let mut current = true;
    for ext in SUPPORTED_EXTENSIONS {
        let Ok(command) = hkcu.open_subkey(format!(r"{}\command", reg.verb_path(ext))) else {
            continue;
        };
        found += 1;
        let value: String = command.get_value("").unwrap_or_default();
        if !value.eq_ignore_ascii_case(&command_line(exe)) {
            current = false;
        }
    }
    if found == 0 {
        return ClassicStatus::Missing;
    }
    if let Some(dll) = dll {
        let registered: String = hkcu
            .open_subkey(format!(r"{}\InprocServer32", reg.clsid_path()))
            .and_then(|k| k.get_value(""))
            .unwrap_or_default();
        if !registered.eq_ignore_ascii_case(&dll.display().to_string()) {
            current = false;
        }
    }
    if found == SUPPORTED_EXTENSIONS.len() && current {
        ClassicStatus::Installed
    } else {
        ClassicStatus::Outdated
    }
}

pub fn set_run_at_startup(reg: &Registry, enabled: bool, exe: &Path) -> io::Result<()> {
    let (run, _) = Registry::hkcu().create_subkey(&reg.run)?;
    if enabled {
        run.set_value(RUN_VALUE, &format!("\"{}\" --background", exe.display()))
    } else {
        ignore_missing(run.delete_value(RUN_VALUE))
    }
}

pub fn run_at_startup_enabled(reg: &Registry) -> bool {
    Registry::hkcu().open_subkey(&reg.run).and_then(|k| k.get_value::<String, _>(RUN_VALUE)).is_ok()
}

/// Tell Explorer that verbs changed so menus refresh without a restart.
pub fn notify_shell() {
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    // SAFETY: documented broadcast with null items.
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
}

pub fn windows_build() -> u32 {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", KEY_READ)
        .and_then(|k| k.get_value::<String, _>("CurrentBuildNumber"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

pub fn is_windows_11() -> bool {
    windows_build() >= 22_000
}

// ---------------------------------------------------------------------------
// Windows 11 modern context menu (sparse MSIX package with external location)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ModernStatus {
    Installed,
    NotInstalled,
    /// No signed `UpscaleeContextMenu.msix` shipped with this build.
    PackageMissing,
    /// Windows 10: the modern menu doesn't exist; the classic menu is used.
    Unsupported,
    Error {
        message: String,
    },
}

fn file_uri(path: &Path) -> String {
    let mut out = String::from("file:///");
    for ch in path.display().to_string().chars() {
        match ch {
            '\\' => out.push('/'),
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '/' | ':' => out.push(ch),
            other => {
                let mut buf = [0u8; 4];
                for b in other.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
        }
    }
    out
}

fn installed_package_full_names() -> windows::core::Result<Vec<windows::core::HSTRING>> {
    use windows::Management::Deployment::PackageManager;
    let manager = PackageManager::new()?;
    let mut names = Vec::new();
    for package in manager.FindPackagesByUserSecurityId(&windows::core::HSTRING::new())? {
        let id = package.Id()?;
        if id.Name()? == PACKAGE_NAME {
            names.push(id.FullName()?);
        }
    }
    Ok(names)
}

pub fn modern_status(sparse_package: Option<&Path>) -> ModernStatus {
    if !is_windows_11() {
        return ModernStatus::Unsupported;
    }
    match installed_package_full_names() {
        Ok(names) if !names.is_empty() => ModernStatus::Installed,
        Ok(_) if sparse_package.is_none() => ModernStatus::PackageMissing,
        Ok(_) => ModernStatus::NotInstalled,
        Err(err) => ModernStatus::Error { message: err.message().to_string() },
    }
}

pub fn install_modern(msix: &Path, install_dir: &Path) -> Result<(), String> {
    use windows::Foundation::Uri;
    use windows::Management::Deployment::{AddPackageOptions, PackageManager};

    let run = || -> windows::core::Result<()> {
        let manager = PackageManager::new()?;
        let options = AddPackageOptions::new()?;
        options.SetExternalLocationUri(&Uri::CreateUri(&file_uri(install_dir).into())?)?;
        options.SetForceUpdateFromAnyVersion(true)?;
        let operation = manager.AddPackageByUriAsync(&Uri::CreateUri(&file_uri(msix).into())?, &options)?;
        let result = operation.join()?;
        let code = result.ExtendedErrorCode()?;
        if code.is_err() {
            return Err(windows::core::Error::new(code, result.ErrorText()?.to_string()));
        }
        Ok(())
    };
    run().map_err(|e| format!("{} (0x{:08X})", e.message(), e.code().0))
}

pub fn remove_modern() -> Result<(), String> {
    use windows::Management::Deployment::PackageManager;
    let run = || -> windows::core::Result<()> {
        let manager = PackageManager::new()?;
        for name in installed_package_full_names()? {
            manager.RemovePackageAsync(&name)?.join()?;
        }
        Ok(())
    };
    run().map_err(|e| format!("{} (0x{:08X})", e.message(), e.code().0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Scratch(String);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = Registry::hkcu().delete_subkey_all(&self.0);
        }
    }

    /// A scratch key unique to each test (tests run in parallel threads).
    fn scratch(test: &str) -> (Scratch, Registry) {
        let root = format!(r"Software\UpscaleeTest-{}-{test}", std::process::id());
        (Scratch(root.clone()), Registry::scratch(&root))
    }

    #[test]
    fn classic_verbs_install_report_and_remove() {
        let (_guard, reg) = scratch("classic");
        let exe = PathBuf::from(r"C:\Program Files\Upscalee\Upscalee.exe");
        let dll = PathBuf::from(r"C:\Program Files\Upscalee\UpscaleeShell.dll");

        assert_eq!(classic_status(&reg, &exe, Some(&dll)), ClassicStatus::Missing);
        install_classic(&reg, &exe, Some(&dll)).unwrap();
        assert_eq!(classic_status(&reg, &exe, Some(&dll)), ClassicStatus::Installed);

        let verb = Registry::hkcu().open_subkey(reg.verb_path("png")).unwrap();
        assert_eq!(verb.get_value::<String, _>("ExplorerCommandHandler").unwrap(), CLSID_CLASSIC);
        assert_eq!(verb.get_value::<String, _>("MUIVerb").unwrap(), MENU_TEXT);
        let command: String = verb.open_subkey("command").unwrap().get_value("").unwrap();
        assert_eq!(command, r#""C:\Program Files\Upscalee\Upscalee.exe" --pick "%1""#);

        let moved = PathBuf::from(r"D:\Apps\Upscalee\Upscalee.exe");
        assert_eq!(classic_status(&reg, &moved, Some(&dll)), ClassicStatus::Outdated);

        // Without the DLL the handler value disappears (plain command verb).
        install_classic(&reg, &exe, None).unwrap();
        let verb = Registry::hkcu().open_subkey(reg.verb_path("jpg")).unwrap();
        assert!(verb.get_value::<String, _>("ExplorerCommandHandler").is_err());

        remove_classic(&reg).unwrap();
        assert_eq!(classic_status(&reg, &exe, None), ClassicStatus::Missing);
        remove_classic(&reg).unwrap(); // idempotent
    }

    #[test]
    fn run_at_startup_toggles() {
        let (_guard, reg) = scratch("run");
        let exe = PathBuf::from(r"C:\U\Upscalee.exe");
        set_run_at_startup(&reg, true, &exe).unwrap();
        assert!(run_at_startup_enabled(&reg));
        set_run_at_startup(&reg, false, &exe).unwrap();
        assert!(!run_at_startup_enabled(&reg));
        set_run_at_startup(&reg, false, &exe).unwrap();
    }

    #[test]
    fn file_uris_are_escaped() {
        assert_eq!(
            file_uri(Path::new(r"C:\Program Files\Upscalee\x.msix")),
            "file:///C:/Program%20Files/Upscalee/x.msix"
        );
        assert_eq!(file_uri(Path::new(r"C:\Users\Zoë")), "file:///C:/Users/Zo%C3%AB");
    }

    #[test]
    fn build_number_is_read() {
        assert!(windows_build() >= 10_000);
    }
}
