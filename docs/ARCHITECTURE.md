# Upscalee — System Architecture

> Status of this document: describes the **implemented** architecture of v0.1.0
> (commit on `main`). Where the implementation deliberately differs from the
> original design brief ([Upscalee-Architecture.md](Upscalee-Architecture.md)),
> the difference is listed in [§15](#15-deviations-from-the-original-brief) and
> recorded as an ADR in [adr/](adr/).
>
> Legend: **[Implemented]** code exists and is covered by automated tests ·
> **[Implemented – manual check pending]** code exists, needs on-device
> verification · **[Planned]** not built yet.

---

## 1. System context

Upscalee is a Windows-only, fully offline desktop utility. Its boundary is the
local machine of one interactive user: Windows Explorer, the user's image files,
the GPU (Vulkan) and per-user registry/AppData. There is **no server, no
network traffic, no account and no telemetry**.

```mermaid
flowchart LR
    user([User]) -->|right-click image| explorer[Windows Explorer]
    explorer -->|loads| shell[UpscaleeShell.dll<br/>IExplorerCommand]
    shell -->|named pipe JSON| app[Upscalee.exe<br/>Tauri 2 tray app]
    explorer -. classic verb fallback .->|Upscalee.exe --pick file| app
    app -->|WebView2| ui[Radial menu · HUD · Settings<br/>React + SVG]
    app -->|spawn per AI pass| engine[realesrgan-ncnn-vulkan.exe]
    engine -->|Vulkan| gpu[(GPU)]
    app -->|read source / atomic write| files[(Image files)]
    app -->|HKCU| reg[(Registry: verbs, COM, Run)]
    app -->|PackageManager| pkg[(Sparse MSIX package<br/>Windows 11 menu)]
```

## 2. Technology inventory

| Area | Technology | Version (lockfiles) | Status |
|------|------------|---------------------|--------|
| Desktop shell | Tauri 2 (Rust + WebView2) | tauri 2.x (`Cargo.lock`), CLI 2.12.4 | [Implemented] |
| UI | React 19 + TypeScript (native `tsc` 7) + plain CSS tokens, SVG | `pnpm-lock.yaml` | [Implemented] |
| UI build / tests | Vite 8 (Rolldown), Vitest 5 | `pnpm-lock.yaml` | [Implemented] |
| Core pipeline | Rust 2021, tokio, `image` 0.25, `fast_image_resize` 6, `jpeg-encoder`, `webp` (libwebp) | `Cargo.lock` | [Implemented] |
| Explorer extension | Rust `cdylib` COM server using `windows` 0.62 | `Cargo.lock` | [Implemented – manual check pending] |
| AI engine | Real-ESRGAN ncnn Vulkan 20220424 (v0.2.5.0), SHA-256 pinned | `engine/engine.lock.json` | [Implemented – manual check pending] |
| Installer | Tauri NSIS bundler, per-user, custom hooks | `installer/nsis-hooks.nsh` | [Implemented] |
| Win 11 modern menu | Sparse MSIX package (external location) + signing scripts | `apps/shell-extension/package/` | [Implemented – needs signing certificate] |
| Logging | `tracing` + daily rolling files (7 kept) | — | [Implemented] |
| CI | GitHub Actions (Windows runner) | `.github/workflows/ci.yml` | [Implemented – not yet run on GitHub] |

## 3. Repository map

```
Upscalee/
├─ Cargo.toml                    # Rust workspace (4 crates)
├─ package.json, pnpm-workspace.yaml
├─ crates/
│  ├─ upscalee-ipc/              # pipe protocol + blocking Win32 client (used by app AND DLL)
│  └─ upscalee-core/             # presets, planner, naming, settings, imaging, engine runner, pipeline
│     └─ src/bin/upscalee-cli.rs # developer CLI (milestone M1)
├─ apps/
│  ├─ desktop/                   # Tauri app
│  │  ├─ radial.html hud.html settings.html
│  │  ├─ src/{radial,hud,settings,shared}/   # React UI
│  │  └─ src-tauri/              # Rust app: tray, windows, IPC server, jobs, integration
│  └─ shell-extension/           # UpscaleeShell.dll (Rust COM) + sparse package manifest
├─ engine/engine.lock.json       # pinned engine download (binaries in engine/bin, git-ignored)
├─ installer/nsis-hooks.nsh      # register/unregister hooks for the NSIS installer
├─ scripts/                      # fetch-engine, build-release, sparse package, dev certificate
└─ docs/
```

## 4. Layers and responsibilities

| Layer | Crate / folder | Responsibilities | Must not |
|-------|----------------|------------------|----------|
| Explorer integration | `apps/shell-extension` | Report menu title/icon/visibility, collect selected paths + cursor, forward over the pipe, launch the app if needed | Block Explorer's thread, panic across FFI, touch the file system in `GetState`, depend on heavy crates |
| IPC contract | `crates/upscalee-ipc` | Message types, framing, pipe naming, blocking client with foreground hand-off | Depend on tokio/Tauri |
| Application | `apps/desktop/src-tauri` | Single instance, pipe server, pick aggregation, radial session, job queue, HUD/tray/settings windows, registry + package integration | Contain image-processing logic |
| Core | `crates/upscalee-core` | Plan, decode, AI passes via sidecar, resize, encode, atomic save, settings persistence | Know about windows, Tauri or the registry |
| UI | `apps/desktop/src` | Render sessions/jobs/settings, keyboard + pointer interaction, report HUD height | Access the file system or shell directly (only typed commands) |

### 4.1 App modules (`apps/desktop/src-tauri/src`)

| Module | Responsibility |
|--------|----------------|
| `app.rs` | Entry: parse CLI, `--register/--unregister`, claim the pipe or forward and exit, build Tauri app, self-repair integration |
| `cli.rs` | Command-line contract (`--background`, `--pick`, `--settings`, `--register`, `--unregister`, bare paths) |
| `ipc_server.rs` | Named-pipe server (first-instance lock + inbound requests) |
| `picker.rs` | Debounces bursts of picks (classic verb starts one process per file) into one radial session |
| `radial.rs` | Reads image headers, builds `RadialSession`, places the window at the cursor (DPI aware), handles select/cancel/blur |
| `jobs.rs` | Queue, bounded concurrency, cancellation, throttled progress events, tray tooltip, "open folder when done" |
| `hud.rs` | Bottom-right activity panel (shown without activating), job actions by **job id** |
| `settings_window.rs`, `commands.rs` | Settings window and its commands (settings, integration, engine/GPU probe, app info) |
| `integration.rs` | HKCU verbs, COM registration, Run key, Windows 11 sparse package via `PackageManager` |
| `tray.rs` | Tray icon/menu (Upscale images…, Show activity, Settings, Open logs, Quit) |
| `win32.rs` | Cursor, monitor work area + DPI, foreground hand-off, show-without-activate |
| `paths.rs`, `state.rs`, `logging.rs` | Disk layout, shared state, rolling logs |

### 4.2 Core modules (`crates/upscalee-core/src`)

| Module | Responsibility |
|--------|----------------|
| `preset.rs` | 2K/4K/8K/12K/16K = longest edge 2048/3840/7680/11520/15360, labels, hotkeys |
| `planner.rs` | Target size (aspect kept, short edge even), AI pass chain, memory estimate, progress weights |
| `formats.rs` | Source formats, output format resolution, model profiles |
| `naming.rs` | `photo_4K.png`, `photo_4K (1).png`, output folder resolution |
| `imaging.rs` | Header-only info, decode with EXIF orientation + ICC, RGB/RGBA normalisation, Lanczos3 resize, PNG/JPEG/WebP encode, atomic no-clobber save |
| `engine.rs` | Sidecar process runner: progress parsing, error classification, VRAM tile ladder, cancellation, GPU probe |
| `pipeline.rs` | `run_job`: skip check → memory guard → AI passes or fallback → exact resize → save (with folder fallback) |
| `settings.rs` | `settings.json` model, defaults, sanitising, atomic save |
| `system.rs` | Free physical memory, kill-on-close job object for child processes |

## 5. End-to-end flow (right-click → saved file)

```mermaid
sequenceDiagram
    autonumber
    actor U as User
    participant E as Explorer
    participant D as UpscaleeShell.dll
    participant A as Upscalee.exe (tray)
    participant W as Radial web view
    participant J as Job manager
    participant R as realesrgan-ncnn-vulkan
    U->>E: Right-click image(s) → "Upscale with Upscalee"
    E->>D: IExplorerCommand::Invoke(items)
    D->>D: paths (supported ext only) + GetCursorPos
    D-->>E: S_OK immediately (work continues on a worker thread)
    D->>A: connect \\.\pipe\upscalee-<session>-<user> (launch --background if absent)
    D->>A: AllowSetForegroundWindow(server pid); {"type":"pick",...}
    A-->>D: {"ok":true}
    A->>A: read image headers, build RadialSession, place window at cursor
    A->>W: event radial://session
    W->>A: radial_ready(id) → show + focus
    U->>W: click 8K (or press 8)
    W->>A: radial_select(id, "8K", profile, format)
    A->>J: enqueue JobRequest per file, show HUD
    J->>J: decode, plan, memory guard
    loop each ×4 pass (1–2)
        J->>R: -i in.png -o out.png -n model -s 4 -t tile
        R-->>J: stderr "NN.NN%" progress
    end
    J->>J: Lanczos3 → exact target, encode, temp file → no-clobber rename
    J-->>W: jobs://changed (HUD: Done — Open / Show in folder)
```

### 5.1 Single instance and launch paths

| Launch | What happens |
|--------|--------------|
| First process (any args) | Creates the pipe with `FILE_FLAG_FIRST_PIPE_INSTANCE`; becomes the resident tray app |
| Later process | Pipe creation fails → forwards its command (`pick` / `show_settings` / `ping`) and exits in milliseconds |
| Shell extension | Connects; if nothing listens, launches `Upscalee.exe --background` and retries for up to 20 s |
| Classic verb without the DLL | Explorer starts one process per file; each forwards; `picker.rs` merges picks that arrive within a 160 ms quiet window (max 700 ms) |
| `--register` / `--unregister` | Run by the installer; perform integration work and exit without claiming the pipe |

### 5.2 Upscale pipeline

| Scale (target ÷ source) | Passes | Notes |
|-------------------------|--------|-------|
| ≤ 4 | 1 × (×4) | Output is supersampled, then Lanczos3 **down** to target |
| ≤ 16 | ×4 → shrink to ⌈target/4⌉ → ×4 | Second pass lands exactly on (or ≥) target; no wasted pixels |
| > 16 | ×4 → ×4 → Lanczos3 up | `TinySource` warning shown |

* All bundled models are fixed ×4 (`realesrgan-x4plus` for Photo, `realesrgan-x4plus-anime` for Anime). See [ADR 0003](adr/0003-x4-models-with-shrink-between-passes.md).
* Sources are normalised to 8-bit RGB (or RGBA only when transparency is real), EXIF orientation applied, ICC profile carried to PNG/JPEG outputs.
* Engine runs at below-normal priority, inside a kill-on-close job object, with `-t 0` (auto) and an out-of-VRAM ladder 0 → 256 → 128 → 64 → 32.
* Progress budget: prepare 5 % · AI passes 75 % (weighted by output pixels) · resize 8 % · save 12 %.
* Memory guard: peak RAM is estimated per stage; if it exceeds available physical memory the job fails with "Not enough memory for 16K, try 8K" (largest preset that fits).

## 6. Interfaces

### 6.1 Named-pipe protocol (`crates/upscalee-ipc`)

* Name: `\\.\pipe\upscalee-<session id>-<sanitised user name>` — scoped per interactive session and user, local clients only.
* Framing: one UTF-8 JSON object per line, one request + one response per connection, ≤ 1 MiB, 5 s read timeout.

```json
{"v":1,"type":"pick","paths":["C:\\Pictures\\cat.png"],"cursor":{"x":1200,"y":640},"origin":"shell_extension"}
{"v":1,"type":"show_settings"}
{"v":1,"type":"ping"}
```

Response: `{"ok":true}` or `{"ok":false,"error":"…"}`. Unknown versions are rejected.

### 6.2 Web view ↔ Rust contract

The web views have only `core:default` permissions; everything else is an app command.

| Command | Window | Purpose |
|---------|--------|---------|
| `radial_current`, `radial_ready(id)`, `radial_select(id, preset, profile, format)`, `radial_cancel` | radial | Session lifecycle |
| `jobs_snapshot`, `job_cancel(id)`, `jobs_cancel_all`, `jobs_clear_finished`, `job_open(id)`, `job_reveal(id)`, `hud_set_height(h)`, `hud_hide` | hud | Activity panel |
| `settings_get`, `settings_defaults`, `settings_save`, `settings_pick_folder`, `integration_status/install/remove`, `engine_status`, `engine_probe`, `app_info`, `open_logs` | settings | Settings window |

| Event | Payload |
|-------|---------|
| `radial://session` | `RadialSession` (summary, presets with availability/target, defaults, engine mode) |
| `jobs://changed` | `JobView[]` (throttled to ≤ ~8 per second) |

TypeScript mirrors of every payload live in `apps/desktop/src/shared/api.ts`.

### 6.3 Developer CLI

`cargo run -p upscalee-core --features cli --release -- <files> --preset 8K [--profile anime] [--format webp] [--out DIR] [--engine DIR] [--tile N] [--gpu N] [--no-skip] [--list-gpus]`

## 7. Data and storage

There is no database. Persistent state:

| Data | Location | Format / notes |
|------|----------|----------------|
| Settings | `%APPDATA%\io.github.chamathdilshanc.upscalee\settings.json` | camelCase JSON, sanitised on load, atomic write |
| Logs | `%LOCALAPPDATA%\io.github.chamathdilshanc.upscalee\logs\upscalee.YYYY-MM-DD.log` | 7 daily files kept |
| Job scratch | `%LOCALAPPDATA%\io.github.chamathdilshanc.upscalee\temp\job-*` | Deleted after each job; stale folders (> 1 h) removed at start |
| Outputs | Next to the source, or the configured folder, or `Pictures\Upscalee` fallback | Never overwrites; temp file + no-clobber rename |
| Registry | `HKCU\Software\Classes\SystemFileAssociations\.<ext>\shell\Upscalee`, `HKCU\Software\Classes\CLSID\{58A04070-…}`, `HKCU\…\CurrentVersion\Run\Upscalee` | Written/removed by `integration.rs` |
| Install | `%LOCALAPPDATA%\Upscalee\` (per-user NSIS) | exe, `UpscaleeShell.dll`, `engine\`, notices |

Settings schema (defaults): `outputFolder: null`, `outputFormat: "keep"`, `defaultProfile: "photo"`, `tileSize: 0`, `gpu: null`, `concurrentJobs: 1`, `runAtStartup: true`, `skipIfLargeEnough: true`, `openFolderWhenDone: false`, `lastPreset: null`, `explorerIntegration: true`.

## 8. Errors, retries and idempotency

| Situation | Behaviour | Where |
|-----------|-----------|-------|
| No Vulkan GPU | Engine reports invalid device → GPU marked missing for the session → plain Lanczos3 resize, HUD warning | `engine::classify`, `pipeline` |
| Engine not installed / model missing | Plain Lanczos3 resize, HUD warning | `pipeline` |
| Out of VRAM | Retry the pass with smaller tiles (0→256→128→64→32), then fail | `engine::upscale` |
| Not enough RAM | Fail before work starts, suggest largest preset that fits | `pipeline::memory_suggestion` |
| Unsupported / corrupt image | Shown as failed row (name + reason); other files continue | `radial`, `pipeline` |
| Output folder not writable | Save to `Pictures\Upscalee`, HUD notes the folder | `pipeline::save_with_fallback` |
| Name collision (even racing) | `photo_4K (n).ext`; no-clobber rename retries the next name | `imaging::save_atomic` |
| Engine crash / unexpected size | Job fails with the engine's last stderr line; scratch folder removed | `engine`, `pipeline` |
| Cancel | Kills the sidecar, drops scratch folder, row shows Cancelled | `jobs`, `engine` |
| App crash mid-job | Sidecar dies with the job object; stale scratch cleaned next start | `system`, `pipeline::clean_stale_scratch` |
| Integration registration | Idempotent install/remove; self-repair on start for installed layouts | `integration`, `app::self_repair` |

## 9. Configuration and secrets

| Variable | Purpose |
|----------|---------|
| `UPSCALEE_ENGINE_DIR` | Override the engine folder (dev/testing) |
| `UPSCALEE_LOG` | `tracing` filter, e.g. `debug` (default `info,upscalee=debug,upscalee_core=debug`) |

The app holds **no secrets**. Code-signing certificates are used only by
`scripts/build-sparse-package.ps1` at build time, passed as parameters, and are
git-ignored (`*.pfx`). See [SECURITY.md](SECURITY.md).

## 10. Security boundary (summary)

Explorer-hosted code is minimal and panic-guarded; the pipe is session/user
scoped, local-only, size/time bounded, and the client connects with
`SECURITY_IDENTIFICATION`; web views get no file-system or shell permissions and
can only open outputs **by job id**. Full threat model: [SECURITY.md](SECURITY.md).

## 11. Observability

Structured `tracing` logs (job start/finish, engine failures with stderr tail,
integration repairs, pipe errors). The Settings window links to the log folder.
The shell extension writes diagnostics with `OutputDebugString` only (visible in
DebugView), never to disk.

## 12. Test architecture

Unit tests sit next to the code (Rust `#[cfg(test)]`, Vitest `*.test.ts`); the
shell extension has a COM integration test that drives `DllGetClassObject` →
`IClassFactory` → `IExplorerCommand` without registration. Details and manual
on-device checklist: [TESTING_STRATEGY.md](TESTING_STRATEGY.md).

## 13. Deployment topology

```
%LOCALAPPDATA%\Upscalee\                 (per-user install, no admin)
├─ Upscalee.exe                          (Tauri app, ~10 MB)
├─ UpscaleeShell.dll                     (COM context-menu handler)
├─ UpscaleeContextMenu.msix              (only when built with a signing certificate)
├─ THIRD_PARTY_NOTICES.md
└─ engine\
   ├─ realesrgan-ncnn-vulkan.exe, vcomp140.dll
   └─ models\realesrgan-x4plus{.param,.bin}, realesrgan-x4plus-anime{.param,.bin}
```

Build and release steps: [DEPLOYMENT.md](DEPLOYMENT.md).

## 14. Target architecture (not yet built)

* **[Planned]** Denoise and face enhancement (GFPGAN) as additional engine passes (v1.1).
* **[Planned]** Before/after preview window; folder right-click (non-recursive batch).
* **[Planned]** Signed release pipeline (installer, exe, DLL, MSIX) with a real code-signing certificate.
* **[Planned]** Update channel (Tauri updater) — open question, see [PRD](PRD.md#11-open-questions).

## 15. Deviations from the original brief

| Brief | Implemented | Why | ADR |
|-------|-------------|-----|-----|
| C++ or C# COM DLL | Rust COM DLL (`windows` crate) | One toolchain, memory safety inside Explorer, panic guards, shared IPC crate | [0001](adr/0001-rust-shell-extension.md) |
| Inno Setup, `Program Files`, `regsvr32` | Tauri NSIS, per-user `%LOCALAPPDATA%\Upscalee`, app self-registers (HKCU) | No admin/UAC, built into `tauri build`, registration logic testable in Rust | [0002](adr/0002-per-user-nsis-installer.md) |
| x2/x3/x4 models; `scale ≤ 2` uses x2 | x4 models only; shrink between passes | x2 photo model not in the ncnn release; supersampling quality; fewer models (~42 MB) | [0003](adr/0003-x4-models-with-shrink-between-passes.md) |
| Native toasts for progress/done | Custom always-on-top HUD window | Live progress bars, Cancel/Open/Show in folder, works without an AppUserModelID | [0004](adr/0004-custom-activity-hud.md) |
| `\\.\pipe\upscalee`; classic verb `HKCR\…\image\shell` | Session+user scoped pipe; HKCU per-extension verbs with `ExplorerCommandHandler` + debounced fallback | Multi-user safety; one Invoke for multi-select in the classic menu | [0005](adr/0005-ipc-and-classic-menu.md) |
| CPU fallback `-g -1` | Lanczos3 fallback with a clear warning | The bundled engine targets Vulkan devices; see ADR for the verification note | [0006](adr/0006-no-gpu-fallback.md) |
| Logs in `%LOCALAPPDATA%\Upscalee\logs` | `%LOCALAPPDATA%\<bundle id>\logs` | `%LOCALAPPDATA%\Upscalee` is the per-user install folder; bundle-id folders are removed by the uninstaller's "delete app data" option | [0002](adr/0002-per-user-nsis-installer.md) |

## 16. Risks and open questions

| Risk / question | Impact | Mitigation / next step |
|-----------------|--------|------------------------|
| Engine behaviour not exercised end-to-end in CI (no GPU on runners) | Medium | Manual checklist in TESTING_STRATEGY.md; `upscalee-cli` for on-device checks |
| Windows 11 may list the packaged verb **and** the classic verb under "Show more options" | Low (duplicate entry) | Verify on Win 11 with a signed package; if duplicated, write `LegacyDisable` on the classic verb when the package is installed |
| Self-signed certificates trigger SmartScreen / can't install the MSIX for users | High for distribution | Obtain a code-signing certificate before public release |
| Very large outputs (16K PNG 100–300 MB) on slow disks | Medium | JPG/WEBP options; HUD shows "Saving" stage |
| Model licenses for redistribution | Legal | Re-check upstream licenses before each release (THIRD_PARTY_NOTICES.md) |
