# Upscalee — Testing Strategy

## 1. Pyramid

| Level | What | Tooling | Count (v0.1) |
|-------|------|---------|--------------|
| Unit — core | presets, planner (sizes, passes, memory), naming, formats, settings, engine output parsing/classification, tile ladder, imaging (EXIF, alpha, encoders, atomic save) | `cargo test -p upscalee-core` | 48 |
| Integration — core pipeline | `run_job` end-to-end on real files without the engine (fallback path): exact size, names, collisions, skip, unwritable folder fallback, cancel, corrupt/unsupported input | `cargo test -p upscalee-core` | included above |
| Unit — IPC | framing, versioning, size limits, pipe naming, extension filter | `cargo test -p upscalee-ipc` | 7 |
| Unit — app | CLI parsing, pick merging, radial session building, tooltip, placement maths, paths (installed-layout detection, bundle-id data folders), **registry install/status/remove against a scratch HKCU key**, Run key | `cargo test -p upscalee` | 22 |
| Integration — COM | `DllGetClassObject` → `IClassFactory` → `IExplorerCommand` (title, canonical name, visibility per file type, unload refcount) on real `IShellItemArray`s | `cargo test -p upscalee-shell` | 4 |
| Unit — UI | radial geometry, keyboard mapping, enabled-wedge stepping | `pnpm --filter upscalee-desktop test` (Vitest) | 14 |
| Static | `cargo clippy -D warnings`, `cargo fmt --check`, `tsc --noEmit` | CI | — |
| Manual on-device | Explorer menus, GPU engine runs, installer, DPI/multi-monitor | checklist below | — |

## 2. Running

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
pnpm install
pnpm --filter upscalee-desktop typecheck
pnpm --filter upscalee-desktop test
```

The registry tests create and delete `HKCU\Software\UpscaleeTest-<pid>`; they
never touch the real `Software\Classes` keys.

## 3. Not covered automatically (and why)

* **GPU inference**: CI runners have no Vulkan GPU, and the engine binary is
  third-party. Use the developer CLI on a real machine:
  `cargo run -p upscalee-core --features cli --release -- sample.png --preset 8K`.
* **Explorer behaviour** (menu placement, focus hand-off) needs an interactive desktop.
* **Installer** runs need a disposable Windows user or VM.

## 4. Manual release checklist

| # | Scenario | Expected |
|---|----------|----------|
| M1 | Install the setup `.exe` as a standard user | No UAC prompt; Start-menu entry; Settings opens on "Run Upscalee" |
| M2 | Right-click a PNG (Win 10 / Win 11 "Show more options") | "Upscale with Upscalee" with icon |
| M3 | Click it | Wheel at the cursor, focused; Esc closes; click-outside closes |
| M4 | Press `4` on a 1080p photo | HUD shows AI pass progress; `photo_4K.png` = 3840 × 2160 |
| M5 | Repeat M4 | `photo_4K (1).png` |
| M6 | Select 3 images → invoke once | One wheel, `3 × PNG`; three jobs |
| M7 | 500 px image → 16K | Two passes; 15360 px long edge; tiny-source warning |
| M8 | Source larger than 4K | 2K/4K disabled "Already larger" |
| M9 | Tab / F on the wheel | Model / format cycle; output format honoured |
| M10 | Cancel a running 16K job | Engine process gone; no output; no temp folder left |
| M11 | Output folder on read-only location | Saved to `Pictures\Upscalee`; HUD note |
| M12 | Disable GPU driver / VM without Vulkan | Lanczos3 output + "no Vulkan GPU" warning |
| M13 | Second monitor with different scaling | Wheel correctly sized and inside that monitor |
| M14 | Settings → Remove / Install | Menu entry disappears / returns (after Explorer refresh) |
| M15 | Uninstall (with and without "delete app data") | Menu, Run key and files removed; data kept/removed accordingly |
| M16 | Signed MSIX on Windows 11 | Top-level entry in the modern menu; check for duplicates under "Show more options" |

## 5. Quality gates

A change is done when: all automated suites pass, clippy/fmt/tsc are clean, new
logic has unit tests next to it, UI changes pass the
[design checks](DESIGN_SYSTEM.md#12-implementation-checks-before-merging-ui-changes),
and any affected manual scenario above has been re-run (state which in the PR).
