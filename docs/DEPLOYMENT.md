# Upscalee — Build, Packaging and Release

Upscalee is distributed as a single per-user NSIS installer. There are no
servers or environments to deploy; "deployment" means producing, signing and
publishing the installer.

## 1. Prerequisites (build machine)

| Tool | Version used | Notes |
|------|--------------|-------|
| Windows | 10 2004+ / 11, x64 | |
| Rust | stable ≥ 1.85, `x86_64-pc-windows-msvc` | built with 1.97.1 |
| Visual Studio 2022 Build Tools | C++ workload + Windows 10/11 SDK | `link.exe`, `cl.exe` (libwebp), `makeappx`, `signtool` |
| Node.js / pnpm | Node 24.11, pnpm 10.28 | |
| PowerShell | 7+ (`pwsh`) | scripts |
| Internet (build time only) | — | engine download, NSIS toolchain download by the Tauri CLI |

## 2. One-command release build

```powershell
pnpm install
pwsh scripts/build-release.ps1            # add -SkipTests to skip the test suites
```

Steps performed:

1. `scripts/fetch-engine.ps1` — downloads `realesrgan-ncnn-vulkan-20220424-windows.zip`,
   verifies SHA-256 `abc02804…802d`, copies the exe, `vcomp140.dll` and the two ×4
   models to `engine/bin/` (git-ignored).
2. `cargo test --workspace` and the Vitest suite.
3. `cargo build -p upscalee-shell --release` → `target/release/upscalee_shell.dll`.
4. *(optional)* sparse package (see §4).
5. `tauri build --config src-tauri/tauri.release.conf.json` → bundles the DLL as
   `UpscaleeShell.dll`, the engine, models and `THIRD_PARTY_NOTICES.md`.

Output: `target/release/bundle/nsis/Upscalee_<version>_x64-setup.exe`.

## 3. What the installer does

| Phase | Action |
|-------|--------|
| Pre-install | Closes a running Upscalee (template check); renames a loaded `UpscaleeShell.dll` to `.old` |
| Install | Copies files to `%LOCALAPPDATA%\Upscalee`; Start-menu shortcut; uninstall entry (HKCU) |
| Post-install | `Upscalee.exe --register`: classic verbs + COM class (HKCU), Run-at-login, Windows 11 sparse package if `UpscaleeContextMenu.msix` is present |
| Finish page | Optional "Run Upscalee" → opens Settings |
| Uninstall | `Upscalee.exe --unregister` (skipped during updates), removes files; app data removed only if "Delete the application data" is ticked |

## 4. Windows 11 modern menu (sparse package) and signing

The modern top-level entry needs package identity, which requires a **signed**
sparse MSIX whose `Publisher` equals the certificate subject.

```powershell
# Development only: create (and optionally trust, as admin) a self-signed cert
$pw = Read-Host -AsSecureString
pwsh scripts/new-dev-certificate.ps1 -Password $pw           # add -Trust in an elevated prompt

# Build installer including the signed package
pwsh scripts/build-release.ps1 -CertificatePath target/dev-signing.pfx `
     -Publisher "CN=Upscalee Dev" -CertificatePassword $pw
```

For public releases use a real OV/EV code-signing certificate and also sign
`Upscalee.exe`, `UpscaleeShell.dll` and the setup `.exe` (Tauri:
`bundle.windows.certificateThumbprint` / `signCommand`) — **TBD: certificate not
yet obtained** ([PRD Q1](PRD.md#11-open-questions)).

## 5. Versioning and release checklist

1. Bump `version` in `Cargo.toml` (`[workspace.package]`), `apps/desktop/src-tauri/tauri.conf.json`,
   `apps/desktop/package.json` and root `package.json` (keep them equal).
2. Run the full build (no `-SkipTests`) and the manual checklist in
   [TESTING_STRATEGY.md](TESTING_STRATEGY.md#4-manual-release-checklist).
3. Re-check third-party licenses ([THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md)).
4. Tag `vX.Y.Z`, create a GitHub release, attach the setup `.exe` and its SHA-256.

## 6. Rollback

Installers are self-contained: users reinstall the previous setup `.exe` (it
replaces the files and re-registers). Settings are forward/backward tolerant
(unknown fields ignored, missing fields defaulted).

## 7. CI

`.github/workflows/ci.yml` runs on `windows-latest`: rustfmt check, clippy
(`-D warnings`), Rust tests, TypeScript check, Vitest and a production web build.
It does not build the installer or run GPU inference.
