# ADR 0002 — Per-user NSIS installer from the Tauri bundler; app self-registers

* Status: **Accepted** (implemented: `tauri.conf.json` → `bundle.windows.nsis`, `installer/nsis-hooks.nsh`)
* Date: 2026-11-05

## Context

The brief proposed Inno Setup installing to `C:\Program Files\Upscalee`,
`regsvr32` for the DLL and HKCR registry keys. That requires administrator
rights (UAC prompt), a second build tool, and installer-script logic that is hard
to test.

## Decision

* Use the NSIS bundler built into `tauri build`, `installMode: "currentUser"`
  → installs to `%LOCALAPPDATA%\Upscalee` without elevation.
* All Explorer integration lives in the app (`integration.rs`):
  `Upscalee.exe --register` / `--unregister`, called from NSIS hooks, writing only
  **HKCU** keys (`Software\Classes\…`, `…\CurrentVersion\Run`) and registering the
  sparse package through the WinRT `PackageManager` API.
* The app re-checks and repairs its registration at start (installed layout only,
  unless the user removed the integration in Settings).
* Because `%LOCALAPPDATA%\Upscalee` is the install folder, settings/logs/scratch
  use the bundle-identifier folders (`%APPDATA%\io.github.chamathdilshanc.upscalee`,
  `%LOCALAPPDATA%\io.github.chamathdilshanc.upscalee`), which the uninstaller's
  "Delete the application data" checkbox removes.
* Explorer may keep `UpscaleeShell.dll` loaded; the pre-install hook renames it to
  `.old` (allowed for loaded DLLs) and the app deletes the leftover later.

## Consequences

* No UAC prompt; every user installs their own copy.
* Registry logic is unit-tested against a scratch HKCU key.
* A machine-wide install is not provided (open question if needed for IT deployment).
