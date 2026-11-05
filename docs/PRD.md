# Upscalee — Product Requirements Document

| | |
|---|---|
| Product | Upscalee — right-click AI image upscaler for Windows |
| Owner | [@ChamathDilshanC](https://github.com/ChamathDilshanC) |
| Version covered | v0.1.0 (MVP) + planned v1.1 |
| Inputs | Original brief [Upscalee-Architecture.md](Upscalee-Architecture.md), current code |
| Related | [ARCHITECTURE.md](ARCHITECTURE.md) · [DESIGN_SYSTEM.md](DESIGN_SYSTEM.md) · [ROADMAP.md](ROADMAP.md) |

Status labels used below: **Verified** (automated tests prove it) ·
**Implemented** (built; on-device manual check pending) · **Planned** ·
**Assumption** · **Open question**.

---

## 1. Executive summary

Upscalee lets a Windows user right-click any image in File Explorer, choose
**Upscale with Upscalee**, pick a quality (2K / 4K / 8K / 12K / 16K) on a radial
menu that opens at the cursor, and get a new AI-upscaled file saved next to the
original — with no app window to open and no extra clicks. It runs fully offline
on the user's GPU (Vulkan, any vendor) using Real-ESRGAN.

## 2. Problem statement

Upscaling an image today means opening a separate app or website, importing the
file, choosing settings, exporting and finding the result. Web services upload
private images to third parties and often limit resolution. Desktop tools are
heavy, require CUDA/Python, or hide the result in their own folders. Users want
the action where the file already is: in Explorer.

## 3. Product vision

"Upscaling is a right-click away": the fastest possible path from *this image is
too small* to *a sharp, larger copy sits next to it*, private by default and
predictable in its output (exact sizes, clear names, never overwriting).

## 4. Target users and personas

No user research has been done yet; personas are **Assumptions** to validate.

| Persona | Need | Typical use |
|---------|------|-------------|
| Hobby photographer | Print or crop old/low-res photos | Photo model, 4K–8K, JPG/PNG |
| Digital artist / anime fan | Wallpapers from illustrations | Anime model, 4K–16K, PNG/WEBP |
| Content creator / designer | Quick asset upscales for slides, thumbnails, web | Multi-select, 2K–4K, keep format |
| Privacy-conscious user | Avoid uploading images online | Any; values offline operation |

There is a single role: the **local interactive Windows user** who installs and
uses the app. No accounts, no administrators, no remote users.

## 5. Goals and non-goals

### Goals

| # | Goal | Status |
|---|------|--------|
| G1 | Installs like a normal Windows app (single setup `.exe`) | Implemented (per-user NSIS, see [ADR 0002](adr/0002-per-user-nsis-installer.md)) |
| G2 | "Upscale with Upscalee" in the Windows 11 modern menu and the classic menu | Classic: Implemented · Modern: Implemented, requires a signed package |
| G3 | Radial quality picker at the cursor | Implemented |
| G4 | Selecting a quality starts immediately and saves automatically | Implemented |
| G5 | Up to 16K (longest edge 15360 px) | Verified |
| G6 | Multi-select (`3 × PNG`) | Implemented |
| G7 | Offline, GPU accelerated (Vulkan), graceful fallback without GPU | Implemented (fallback is Lanczos3, see [ADR 0006](adr/0006-no-gpu-fallback.md)) |

### Non-goals (v1)

Video upscaling · cloud processing · recursive folder batches · macOS/Linux ·
image editing beyond resizing · accounts, sync or telemetry.

## 6. Scope

| Release | Scope |
|---------|-------|
| **MVP v0.1 (this repo)** | FR-001 … FR-027 below |
| **v1.1 (Planned)** | Denoise toggle, face enhancement (GFPGAN), before/after preview, folder right-click |
| **Later (Open question)** | Auto-update, machine-wide install, additional models |

## 7. Functional requirements

Priority: **P0** must ship · **P1** should ship · **P2** nice to have.

### Explorer integration

**FR-001 Classic context-menu entry — P0 — Implemented**
*As a user, I want "Upscale with Upscalee" when I right-click a supported image so I can start without opening an app.*
- AC1: Entry appears for `.png .jpg .jpeg .webp .bmp .tif .tiff` (case-insensitive) and not for other files.
- AC2: Works on Windows 10 and in Windows 11 "Show more options".
- AC3: Registration is per-user (HKCU), needs no admin, and is removed by uninstall / Settings → Remove.
- Verification: registry layout and status logic unit-tested against a scratch key; menu appearance pending manual check.

**FR-002 Windows 11 modern menu entry — P1 — Implemented (needs signed package)**
- AC1: With a signed `UpscaleeContextMenu.msix` in the install folder, the installer registers the sparse package and the entry appears top-level on Windows 11.
- AC2: Without a package, Settings shows "Needs signed package" and the classic entry still works.

**FR-003 Multi-selection — P0 — Implemented**
- AC1: Selecting N supported images and invoking the entry opens **one** radial menu showing `N × FMT` (or `N images` for mixed formats).
- AC2: Unsupported files in the selection are ignored; unreadable images are reported in the activity panel.
- Verification: session summary/availability unit-tested; pick-burst merging unit-tested.

### Radial quality picker

**FR-004 Radial menu at the cursor — P0 — Implemented**
- AC1: Opens centred on the click position, fully inside the monitor's work area, at the monitor's DPI.
- AC2: Outer ring: 2K, 4K, 8K, 12K, 16K; inner ring: Photo/Anime and Keep/PNG/JPG/WEBP; centre: `1 × PNG`, source size, `Esc to close · Tab ↻`.
- AC3: Shows the exact output size per preset for single images.
- Verification: placement maths unit-tested; UI rendered and checked in a browser preview with a mocked backend.

**FR-005 Interaction — P0 — Verified (logic) / Implemented (window)**
- AC1: Hover highlights; click selects; hotkeys `2`, `4`, `8`, `1` (12K), `6` (16K) select immediately.
- AC2: Arrow keys move the highlight (skipping disabled wedges); Enter confirms; Tab / Shift+Tab cycle the model; F cycles the format.
- AC3: Esc, clicking outside the wheel, or switching focus away cancels without side effects.

**FR-006 Already-large detection — P0 — Verified**
- AC1: Presets whose long edge ≤ the source's are shown "Already larger" and can't be picked (single image).
- AC2: For multi-select, presets that only some files need show "k of N"; skipped files report "Already 4K or larger".
- AC3: Setting "Skip if already large enough" off re-renders at the preset size instead.

### Upscaling

**FR-007 Presets by longest edge — P0 — Verified**
2048 / 3840 / 7680 / 11520 / 15360 px; aspect ratio kept; short edge rounded to an even number.

**FR-008 AI pipeline — P0 — Implemented**
- AC1: Uses Real-ESRGAN ×4 models (Photo `realesrgan-x4plus`, Anime `realesrgan-x4plus-anime`) on the GPU via Vulkan.
- AC2: Chooses 1 or 2 passes per [ADR 0003](adr/0003-x4-models-with-shrink-between-passes.md); final size equals the target exactly.
- AC3: EXIF orientation respected; transparency preserved for PNG/WEBP; ICC profile kept for PNG/JPEG.
- Verification: planner, decode/resize/encode and the non-AI path are unit-tested; engine execution needs on-device verification.

**FR-009 Output formats — P0 — Verified**
Keep (BMP/TIFF → PNG), PNG (lossless), JPG q95 4:4:4 (transparency flattened on white), WEBP q95.

**FR-010 Naming and location — P0 — Verified**
- AC1: `photo.png` + 4K → `photo_4K.png`; collisions → `photo_4K (1).png`, `(2)`, …
- AC2: Saved next to the source, or in the configured folder.
- AC3: Never overwrites an existing file; partial files are never visible under the final name.

**FR-011 Job queue — P0 — Implemented**
- AC1: Jobs run one at a time by default; Settings allows 1–4 concurrent jobs.
- AC2: Each job can be cancelled; "Cancel all" cancels queued and running jobs; cancelling leaves no output or temp files.

**FR-012 Progress and completion feedback — P0 — Implemented**
- AC1: An activity panel appears bottom-right without stealing focus, showing stage, % and a progress bar.
- AC2: Finished rows show the output name, size and time with **Open** and **Show in folder**.
- AC3: The tray tooltip shows current progress; the panel auto-hides after completion (longer when something failed).

**FR-013 Error handling — P0 — Verified (logic)**
| Situation | Required behaviour |
|-----------|--------------------|
| No Vulkan GPU / engine missing | Produce the file with Lanczos3 and say so |
| Out of VRAM | Retry with smaller tiles automatically, then fail with a message |
| Not enough RAM | Fail before starting: "Not enough memory for 16K, try 8K" |
| Unsupported/corrupt image | Report file name + reason; continue with other files |
| Output folder not writable | Save to `Pictures\Upscalee` and say so |
| Engine crash | Fail the job with the reason; clean temp files; log details |

### App shell

**FR-014 Settings — P0 — Implemented**
Output folder · output format · default model · tile size · GPU · concurrent jobs ·
run at startup · skip if already large · open folder when done; changes save
automatically; Explorer integration status with Install/Repair/Remove; GPU detection;
version, log folder and license notice.

**FR-015 Tray presence — P0 — Implemented**
Tray icon with Upscale images… (file picker → radial), Show activity, Settings,
Open logs folder, Quit. Left-click opens the activity panel (or Settings when idle).

**FR-016 Instant open / single instance — P0 — Implemented**
- AC1: Only one instance per user session; later launches forward their request and exit.
- AC2: With "Run at startup" on, the app is resident and the radial menu opens without starting a process.

**FR-017 Installer / uninstaller — P0 — Implemented**
- AC1: One setup `.exe`, no admin; Start-menu entry; uninstall entry.
- AC2: Install registers the context menu, COM class, Run-at-login (and the Win 11 package when present).
- AC3: Uninstall removes all of the above; app data is kept unless "Delete the application data" is ticked.

**FR-018 Logging — P1 — Implemented**
Daily log files (7 kept), openable from Settings and the tray.

**FR-019 Developer CLI — P2 — Implemented**
`upscalee-cli` runs the same pipeline from a terminal (milestone M1).

### Planned (v1.1)

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-020 | Denoise toggle on the inner ring | P1 |
| FR-021 | Face enhancement (GFPGAN) option | P2 |
| FR-022 | Before/after preview of the result | P2 |
| FR-023 | Folder right-click (non-recursive batch) | P2 |

## 8. Permissions and boundaries

* Single local user; the app runs with the user's normal (medium) integrity.
* No admin rights required or requested; all registry writes are HKCU.
* The UI layer cannot read/write arbitrary files or launch programs; it can only
  open outputs that Upscalee created (by job id). See [SECURITY.md](SECURITY.md).

## 9. Key workflows and edge cases

| Workflow / edge case | Expected result |
|----------------------|-----------------|
| App not running, user right-clicks | Shell extension starts the app; wheel opens when ready (~1 s first time) |
| Classic verb without the shell extension, 5 files | Five processes forward; one wheel for 5 files |
| Portrait source | Height becomes the preset's long edge |
| Source 5000 px, user wants 4K | 2K/4K shown "Already larger"; 8K+ available |
| Two jobs produce the same name at once | Second gets `(1)` suffix; nothing overwritten |
| Output on read-only media / network share offline | Saved to `Pictures\Upscalee`; panel says where |
| User closes the activity panel while jobs run | Jobs continue; tray tooltip shows progress |
| User quits from tray during a job | Engine process is killed; no partial outputs |
| Several Windows users signed in | Each has an isolated instance (session-scoped pipe) |

## 10. Non-functional requirements

Targets marked *(proposed)* are not yet measured.

| Area | Requirement |
|------|-------------|
| Performance | Wheel visible ≤ 150 ms after click when resident *(proposed)*; app idle CPU ≈ 0 %; engine at below-normal CPU priority |
| Memory | Resident app ≤ 150 MB idle *(proposed)*; per-job peak estimated and guarded before work starts |
| Reliability | A shell-extension fault must never crash Explorer (panic guards); jobs never leave partial outputs; engine killed if the app exits |
| Privacy | No network access or telemetry; logs stay local (they contain file paths) |
| Security | Local-only, user-scoped IPC; least-privilege web views; pinned engine hash |
| Accessibility | Full keyboard operation of the wheel; WCAG AA contrast for text; reduced-motion respected; screen-reader labels |
| Compatibility | Windows 10 2004+ and Windows 11, x64; WebView2 runtime |
| Footprint | Installer ≈ 55–60 MB (engine + 2 models ≈ 48 MB) *(estimate)* |

## 11. Dependencies, assumptions, risks, open questions

**Dependencies:** Real-ESRGAN ncnn Vulkan binary and models (BSD-3 / MIT); WebView2; a Vulkan-capable GPU driver for AI mode.

**Assumptions:** Users have a GPU with a Vulkan driver (all modern NVIDIA/AMD/Intel drivers ship one) · the personas above · per-user install is acceptable.

**Risks:** see [ARCHITECTURE.md §16](ARCHITECTURE.md#16-risks-and-open-questions) — most importantly signing for the Win 11 menu and SmartScreen.

### 11. Open questions

| # | Question | Needed for |
|---|----------|-----------|
| Q1 | Which code-signing certificate (OV/EV) and publisher name will be used? | Public release, Win 11 modern menu |
| Q2 | Project license (none chosen yet) | Public repository / distribution |
| Q3 | Auto-update channel (Tauri updater + hosting)? | Post-MVP |
| Q4 | Is a machine-wide (admin) installer needed? | IT deployments |
| Q5 | Does Win 11 show both packaged and classic verbs under "Show more options"? | De-duplication rule |

## 12. Success indicators (proposed)

No telemetry is collected, so success is judged through testing and user feedback:

* Right-click → wheel visible in ≤ 150 ms on a resident app (manual timing).
* 100 % of jobs either produce an exact-size output or a clear error (manual test matrix).
* 1080p photo → 4K completes in under ~10 s on a mid-range GPU *(to be benchmarked)*.
* Zero Explorer crashes attributable to `UpscaleeShell.dll`.

## 13. Roadmap

See [ROADMAP.md](ROADMAP.md) for milestones M1–M6 (all delivered in v0.1 code),
the verification work still open, and v1.1.
