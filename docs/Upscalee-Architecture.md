# Upscalee — Architecture

> Right-click any image in Windows Explorer → **Upscale with Upscalee** → pick a quality on a radial menu (2K / 4K / 8K / 12K / 16K) → a new, upscaled image is saved next to the original. No window, no extra clicks.

---

## 1. Goals

| # | Goal |
|---|------|
| 1 | Installs like a normal Windows app (single `Upscalee-Setup.exe`). |
| 2 | Adds **"Upscale with Upscalee"** to the Windows 11 right-click menu (top-level, like "Convert with Filee"), and to the classic menu ("Show more options") on Windows 10/11. |
| 3 | Clicking it opens a **radial (donut) quality picker** at the cursor, same idea as Filee's format wheel. |
| 4 | Selecting a quality starts upscaling immediately and saves a new file automatically. |
| 5 | Supports quality up to **16K** (longest edge 15360 px). |
| 6 | Works on multiple selected images at once (center label shows `3 × PNG`). |
| 7 | Fully offline, GPU accelerated (Vulkan), with CPU fallback. |

Non-goals (v1): video upscaling, cloud processing, batch folders recursion, macOS/Linux.

---

## 2. Quality Presets

"K" is defined by the **longest edge** of the output, so portrait and landscape both work.

| Preset | Longest edge | Reference (16:9) |
|--------|-------------|------------------|
| 2K | 2048 px | 2048 × 1152 |
| 4K | 3840 px | 3840 × 2160 |
| 8K | 7680 px | 7680 × 4320 |
| 12K | 11520 px | 11520 × 6480 |
| 16K | 15360 px | 15360 × 8640 |

Rule: if the source is already ≥ the chosen preset, the item is skipped with a toast ("Already 4K or larger").

---

## 3. High-Level Architecture

```
┌────────────────────────────── Windows Explorer ──────────────────────────────┐
│  Right-click image                                                           │
│        │                                                                     │
│        ▼                                                                     │
│  ┌──────────────────────────┐                                                │
│  │ Shell Extension (COM DLL)│  IExplorerCommand  (Win11 modern menu)         │
│  │  UpscaleeShell.dll       │  + registry verb   (classic menu fallback)     │
│  └────────────┬─────────────┘                                                │
└───────────────┼──────────────────────────────────────────────────────────────┘
                │ launches / signals with file paths  (named pipe: \\.\pipe\upscalee)
                ▼
┌──────────────────────────────── Upscalee App (Tauri 2) ──────────────────────┐
│                                                                              │
│  ┌────────────────────┐   IPC    ┌──────────────────────────────────────┐    │
│  │ Radial Menu UI     │◄────────►│ Core (Rust)                          │    │
│  │ (WebView, React)   │          │  • Pipe server / single-instance     │    │
│  │ frameless, transp. │          │  • Job queue + scheduler             │    │
│  │ always-on-top      │          │  • Target-size calculator            │    │
│  └────────────────────┘          │  • Output naming / writer            │    │
│                                  │  • Settings + logging                │    │
│  ┌────────────────────┐          └──────────────┬───────────────────────┘    │
│  │ Tray icon + Toasts │◄────────────────────────┤                            │
│  │ progress / done    │                         ▼                            │
│  └────────────────────┘          ┌──────────────────────────────────────┐    │
│                                  │ Upscale Engine (sidecar process)     │    │
│                                  │  realesrgan-ncnn-vulkan.exe          │    │
│                                  │  + models (x4plus, anime, x2, x3)    │    │
│                                  │  tiled inference on GPU (Vulkan)     │    │
│                                  └──────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Components

| Component | Tech | Responsibility |
|-----------|------|----------------|
| **Shell Extension** | C++ (or C#/.NET) COM DLL implementing `IExplorerCommand` | Adds the menu entry, collects selected file paths, launches the app, passes the cursor position. Must be tiny and never crash Explorer. |
| **Packaging identity (sparse package)** | MSIX sparse package + signed manifest | Required for Windows 11 to show the entry in the *modern* context menu instead of only under "Show more options". |
| **Classic menu fallback** | Registry verb under `HKCR\SystemFileAssociations\image\shell\Upscalee` | Windows 10 and "Show more options" on Win11. |
| **App shell** | Tauri 2 (Rust core + WebView2) | Single-instance app, tray, windows, IPC. Small footprint (~10 MB) compared with Electron. |
| **Radial Menu UI** | React + SVG (or Canvas) | Donut with wedges, hover/keyboard nav, animations. |
| **Core / Job Queue** | Rust (`tokio`) | Validates files, computes targets, runs the engine, writes outputs, reports progress. |
| **Upscale Engine** | Real-ESRGAN ncnn Vulkan (bundled `.exe` sidecar) | GPU upscaling with tiling; works on NVIDIA, AMD and Intel GPUs without CUDA. |
| **Image I/O** | Rust `image` crate + `libvips` (optional) | Decode, final Lanczos resize to exact target, encode PNG/JPG/WebP. |
| **Installer** | Inno Setup (or WiX) | Copies files, registers shell extension, installs sparse package, adds uninstall entry. |

---

## 4. User Flow

```
1. User right-clicks  "Confident Portrait.png"
2. Menu shows         "Upscale with Upscalee"
3. Click              → shell extension sends {paths[], cursorXY} to the app via named pipe
4. App                → opens the Radial Menu centered on the cursor
5. User               → hovers / clicks a wedge  (or presses 2, 4, 8, 1, 6 / arrow keys / Enter)
6. Radial closes      → job(s) enqueued, toast: "Upscaling to 8K…"
7. Engine runs        → progress toast / tray tooltip updates
8. Output saved       → "Confident Portrait_8K.png" next to the original
9. Toast              → "Done — Open / Show in folder"
```

Esc closes the wheel without doing anything. Click outside the wheel also cancels.

---

## 5. Radial Menu Spec

Mirrors the Filee wheel in the reference screenshot.

```
                 ┌─────┐
            ┌────┤ 16K ├────┐
        ┌───┤ 12K └─────┘ AI+├───┐
        │ 8K│    ┌───────┐    │2K │
        ├───┤    │ 1 × PNG│   ├───┤
        │ 4K│    │ Esc to │   │Ret│
        └───┤    │ close  │   ├───┘
            └────┤ Tab ↻ ├────┘
                 └───────┘
```

* **Center disc:** file count + type (`1 × PNG`), hint text `Esc to close`, `Tab` cycles the **model profile**.
* **Outer wedges (quality):** `2K`, `4K`, `8K`, `12K`, `16K`.
* **Inner wedges (options):**
  * `Photo` / `Anime` — model profile (Tab to switch).
  * `Keep format` / `PNG` / `JPG` / `WEBP` — output format (default: keep original).
  * `Denoise` toggle (optional, v1.1).
* **Interaction:** hover highlight, click to select, numeric hotkeys, arrow keys + Enter, Esc to cancel.
* **Window:** frameless, transparent, always-on-top, ~420×420 px, positioned at cursor and clamped to the screen / DPI-aware; fades in under 120 ms.

---

## 6. Upscale Pipeline

```
source ─► decode ─► choose scale ─► AI upscale (tiled) ─► exact resize ─► encode ─► save
```

1. **Decode** the image (PNG/JPG/WEBP/BMP/TIFF), convert to RGBA 8-bit, respect EXIF orientation.
2. **Compute target size:** `scale = presetLongEdge / sourceLongEdge`, then preserve aspect ratio and round to even pixels.
3. **Pick AI passes:**
   * `scale ≤ 2` → 1 pass with x2 model
   * `scale ≤ 4` → 1 pass with x4 model
   * `scale ≤ 16` → 2 passes of x4 (4 × 4 = 16×)
   * `scale > 16` → 2 passes + final Lanczos (warn: very small source)
4. **Tiled inference:** engine runs with `-t <tile>` chosen from available VRAM (auto: 0 → 400/200/100 on out-of-memory). Tiles are stitched by the engine with overlap padding to avoid seams.
5. **Exact resize:** after AI passes, downscale/upscale with Lanczos3 to the exact target size (AI output is usually larger than needed).
6. **Encode:** PNG (compression level 6) by default for lossless; JPG q=95 / WEBP q=95 if the original was lossy.
7. **Save atomically:** write to `*.tmp`, then rename, so Explorer never sees partial files.

### Memory & performance notes

* 16K RGBA frame ≈ 15360 × 8640 × 4 ≈ **530 MB** in RAM; 2 passes means the intermediate may also reach several hundred MB, so jobs run **one at a time** by default (configurable).
* Check free RAM/VRAM before start; if insufficient, drop to smaller tiles or show "Not enough memory for 16K, try 8K".
* GPU preferred; if Vulkan device not found, fall back to CPU (`-g -1`) with a "slow mode" toast.
* Cancel support: killing the sidecar process and deleting temp files.

---

## 7. Output Naming

| Source | Output |
|--------|--------|
| `photo.png` + 4K | `photo_4K.png` |
| Output already exists | `photo_4K (1).png`, `photo_4K (2).png`… |
| `photo.jpg` + 8K | `photo_8K.jpg` |

Default location: same folder as the source. Optional setting: a fixed output folder.

---

## 8. Windows Integration Details

### 8.1 Windows 11 modern menu
* Implement `IExplorerCommand` in `UpscaleeShell.dll`.
* Declare it in the sparse package `AppxManifest.xml` under `windows.fileExplorerContextMenus` for the image types (`.png .jpg .jpeg .webp .bmp .tiff`).
* Register with `Add-AppxPackage -Register` using `-ExternalLocation <install dir>`.
* Package must be **signed** (a code-signing certificate; self-signed works only for dev).

### 8.2 Classic menu / Windows 10
```
HKCR\SystemFileAssociations\image\shell\Upscalee
    (Default)  = "Upscale with Upscalee"
    Icon       = "C:\Program Files\Upscalee\Upscalee.exe,0"
    MultiSelectModel = "Player"
HKCR\...\Upscalee\command
    (Default)  = "C:\Program Files\Upscalee\Upscalee.exe" --pick "%1"
```
(For multi-select, the shell extension path is used because plain verbs spawn one process per file.)

### 8.3 Launch & IPC
* `Upscalee.exe` is **single-instance**; the shell extension connects to the named pipe `\\.\pipe\upscalee`; if the app is not running it launches `Upscalee.exe --background`.
* Message: `{ "paths": [...], "cursor": {"x":..., "y":...}, "dpi": 1.25 }`.
* App starts on login (optional, tray, hidden) so the first click is instant.

---

## 9. Folder Structure

```
upscalee/
├─ apps/
│  ├─ desktop/                  # Tauri app
│  │  ├─ src-tauri/
│  │  │  ├─ src/
│  │  │  │  ├─ main.rs
│  │  │  │  ├─ ipc/pipe_server.rs
│  │  │  │  ├─ jobs/{queue.rs, planner.rs, runner.rs}
│  │  │  │  ├─ engine/{realesrgan.rs, resize.rs, memory.rs}
│  │  │  │  ├─ io/{decode.rs, encode.rs, naming.rs}
│  │  │  │  └─ notify/{toast.rs, tray.rs}
│  │  │  └─ tauri.conf.json
│  │  └─ src/                   # React UI
│  │     ├─ radial/{RadialMenu.tsx, Wedge.tsx, geometry.ts}
│  │     ├─ settings/
│  │     └─ main.tsx
│  └─ shell-extension/          # C++ COM DLL
│     ├─ UpscaleeShell.cpp
│     ├─ UpscaleeShell.def
│     └─ AppxManifest.xml
├─ engine/
│  ├─ realesrgan-ncnn-vulkan.exe
│  └─ models/                   # *.param / *.bin
├─ installer/
│  ├─ upscalee.iss              # Inno Setup script
│  └─ register-shell.ps1
└─ docs/Upscalee-Architecture.md
```

---

## 10. Installer Behavior

**Install**
1. Copy app, engine and models to `C:\Program Files\Upscalee\` (~150–250 MB with models).
2. Register `UpscaleeShell.dll` (`regsvr32`) and install the sparse package for the modern menu.
3. Write classic-menu registry keys.
4. Add Start menu entry, optional "Run at startup", uninstall entry.
5. Restart Explorer (or prompt sign-out) so the menu appears.

**Uninstall**
Remove package, unregister DLL, delete registry keys and install dir; keep user settings unless "Remove settings" is ticked.

---

## 11. Settings (tray → Settings)

| Setting | Default |
|---------|---------|
| Output folder | Same as source |
| Output format | Keep original |
| Default model | Photo (`realesrgan-x4plus`) |
| Tile size | Auto |
| GPU selection | Auto |
| Concurrent jobs | 1 |
| Run at startup | On |
| Skip if already large enough | On |
| Open folder when done | Off |

---

## 12. Error Handling

| Situation | Behavior |
|-----------|----------|
| No Vulkan GPU | Fall back to CPU, show slow-mode warning |
| Out of VRAM | Retry with smaller tile; then fail with message |
| Not enough RAM for target | Suggest next lower preset |
| Unsupported / corrupt image | Skip file, toast with filename |
| Output not writable | Fall back to `Pictures\Upscalee` |
| Engine crash | Log, clean temp files, toast "Upscale failed — view log" |

Logs: `%LOCALAPPDATA%\Upscalee\logs\`.

---

## 13. Security & Licensing

* The app has no network access requirement; no telemetry in v1.
* Shell extension runs inside Explorer: keep it minimal, no heavy work, all errors swallowed safely.
* Code-sign the installer, DLL and app to avoid SmartScreen warnings.
* Licenses: Real-ESRGAN (BSD-3), ncnn (BSD-3), Tauri (MIT/Apache-2.0). Include notices in the installer. Check each model's license before redistributing.

---

## 14. Quality Notes (set expectations)

* AI upscaling **invents plausible detail**; it does not recover real detail. A 500 px image upscaled to 16K will look clean but synthetic.
* 16K PNGs can exceed **100–300 MB**; offer JPG/WEBP output for these.
* Photo vs Anime model choice matters a lot for results; the radial menu exposes it via Tab.

---

## 15. Roadmap

| Milestone | Scope |
|-----------|-------|
| **M1 – Engine CLI** | Rust CLI wrapping Real-ESRGAN, presets, tiling, naming. |
| **M2 – Radial UI** | Tauri window, radial menu, hotkeys, DPI-aware placement. |
| **M3 – Shell integration** | Classic registry verb + named-pipe IPC. |
| **M4 – Win11 modern menu** | `IExplorerCommand` DLL + sparse package + signing. |
| **M5 – Installer** | Inno Setup, uninstall cleanup, auto-start. |
| **M6 – Polish** | Multi-select, progress toasts, settings, error handling, logs. |
| **v1.1** | Denoise/face-enhance (GFPGAN), before/after preview, folder right-click. |

---

## 16. Tech Choices — Why

| Decision | Reason |
|----------|--------|
| Tauri over Electron | ~10 MB runtime, fast startup (important for a right-click flow). |
| Real-ESRGAN ncnn Vulkan | Runs on any GPU vendor, no CUDA/Python install, single portable exe. |
| Sparse MSIX package | Only supported way for third-party apps to appear in the Windows 11 modern menu. |
| Named-pipe single instance | Instant wheel open when app is already resident in tray. |
| Tiled inference + final Lanczos | Makes 16K feasible on 6–8 GB GPUs and gives exact target dimensions. |
