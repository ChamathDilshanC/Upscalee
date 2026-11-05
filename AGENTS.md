# AGENTS.md — Guide for coding agents

Instructions for any automated coding agent (and humans) changing this
repository. Read this file, then [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md),
before editing.

## 1. What this repository is

Upscalee: a Windows-only, offline, right-click AI image upscaler.
Explorer → `UpscaleeShell.dll` (COM) → named pipe → `Upscalee.exe` (Tauri 2 tray
app, React UI) → `realesrgan-ncnn-vulkan.exe` (GPU) → upscaled file next to the
original.

## 2. Directory ownership

| Path | Owns | Language |
|------|------|----------|
| `crates/upscalee-core` | Pipeline logic: presets, planner, naming, imaging, engine runner, settings model | Rust |
| `crates/upscalee-ipc` | Pipe protocol + blocking client, shared by app and DLL | Rust |
| `apps/desktop/src-tauri` | Tray app: windows, IPC server, jobs, Explorer/registry integration | Rust |
| `apps/desktop/src` | Radial menu, HUD, Settings UI | React + TS + CSS |
| `apps/shell-extension` | `UpscaleeShell.dll` and the sparse package manifest | Rust (COM) |
| `engine/` | `engine.lock.json` only — binaries are downloaded into `engine/bin` (git-ignored) | — |
| `installer/` | NSIS hooks | NSIS |
| `scripts/` | Fetch engine, release build, sparse package, dev certificate | PowerShell |
| `docs/` | PRD, architecture, design system, security, testing, deployment, roadmap, ADRs | Markdown |

## 3. Source of truth (highest first)

1. Code and tests in this repository.
2. `docs/adr/*` (accepted decisions).
3. `docs/ARCHITECTURE.md`, `docs/PRD.md`, `docs/DESIGN_SYSTEM.md`.
4. `docs/Upscalee-Architecture.md` (original brief; superseded where ADRs say so).

If code and docs disagree, fix whichever is wrong in the same change and say so.

## 4. Commands (verified in this repo)

```powershell
pnpm install                                   # front-end deps (pnpm workspace)
pwsh scripts/fetch-engine.ps1                  # engine + models → engine/bin (needed for AI mode / release)
cargo test --workspace                         # all Rust tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                                # rustfmt.toml: max_width 120
pnpm --filter upscalee-desktop typecheck       # tsc --noEmit
pnpm --filter upscalee-desktop test            # Vitest
pnpm --filter upscalee-desktop build           # Vite production build → apps/desktop/dist
pwsh scripts/build-release.ps1                 # full installer build
cargo run -p upscalee-core --features cli --release -- <image> --preset 4K   # pipeline without UI
```

`pnpm dev` (= `tauri dev`) starts the app with hot reload; it has **not yet been
exercised in this repository's automated checks** — prefer the commands above
for verification. In dev the app looks for the engine in `engine/bin` and for
`upscalee_shell.dll` next to the exe (`cargo build -p upscalee-shell`).

## 5. Conventions

**Rust**
* Edition 2021, `rustfmt.toml` (120 cols), clippy clean with `-D warnings`.
* Errors: `thiserror` enums in libraries; user-facing messages are short and actionable.
* No `unwrap()` on I/O or user input outside tests; `unsafe` blocks carry a `// SAFETY:` comment.
* Long work never runs on the Tauri main thread or Explorer's thread; CPU-heavy image work goes through `spawn_blocking`.
* Windows APIs: `windows` 0.62 in app/DLL, `windows-sys` in the small crates.

**Shell extension (`apps/shell-extension`) — extra rules**
* Every exported function and COM method stays wrapped in `module::guard*` (`catch_unwind`).
* No blocking calls, no file-system access in `GetState`, no new heavy dependencies.
* Never change `CLSID_CLASSIC` / `CLSID_PACKAGED` (registry, manifest and tests depend on them).
* Release profile must keep `panic = "unwind"`.

**TypeScript / React**
* Strict TS; every backend payload typed in `src/shared/api.ts` mirroring the Rust serde shape.
* UI uses design tokens from `src/shared/tokens.css` only (see docs/DESIGN_SYSTEM.md); no UI libraries without an ADR.
* Overlay pages keep transparent backgrounds (`html.overlay`).

## 6. Adding things

* **New app command**: implement in Rust (`#[tauri::command]`), register in `app.rs::run`, add the typed wrapper in `api.ts`, document it in ARCHITECTURE.md §6.2. Do not accept raw file paths for opening/launching.
* **New setting**: add to `upscalee_core::Settings` with a serde default (old files must still load), `api.ts`, the Settings UI, PRD FR-014 and ARCHITECTURE.md §7.
* **New model/profile**: add the files to `engine/engine.lock.json` (with the release's SHA-256), `ModelProfile`, `tauri.release.conf.json` resources, notices.
* **New file format**: update `upscalee_ipc::SUPPORTED_EXTENSIONS`, `SourceFormat`, the sparse manifest `ItemType`s and tests.
* There is no database and no migrations; settings evolve through serde defaults.

## 7. Security rules

* No network calls, telemetry or auto-update without an accepted ADR.
* Never commit secrets or signing material (`*.pfx`, `*.p12`, `.env`); use parameters.
* Registry writes stay in HKCU and go through `integration.rs`.
* Keep the IPC limits (size, timeout, version check) and `SECURITY_IDENTIFICATION` on the client.
* See [docs/SECURITY.md](docs/SECURITY.md).

## 8. Prohibited without explicit maintainer approval

`git push --force`, history rewrites, deleting branches/tags, bypassing hooks
(`--no-verify`), changing the remote URL, committing `engine/bin` binaries,
registering the context menu on a developer machine as a side effect of tests,
trusting certificates machine-wide, editing `docs/Upscalee-Architecture.md`
(the original brief).

## 9. Definition of done

* Tests added/updated next to the code; all suites, clippy, fmt and `tsc` pass.
* Docs updated in the same change when behaviour, commands, settings or contracts change (PRD status labels, ARCHITECTURE, DESIGN_SYSTEM, ROADMAP).
* For Explorer/engine/installer changes, state which manual checks from docs/TESTING_STRATEGY.md were run — never claim a check you did not run.

## 10. Git protocol

* Branch from `main` (`feat/…`, `fix/…`, `docs/…`); Conventional Commit messages (`feat: …`, `fix: …`, `docs: …`).
* Stage explicit paths (no `git add -A` / `git add .`); review `git diff --staged` first.
* Push with a normal `git push origin <branch>`.

## 11. Authorship (mandatory)

* The only project maintainer/contributor identity for new commits, docs and metadata is **ChamathDilshanC** ([@ChamathDilshanC](https://github.com/ChamathDilshanC)).
* Commits use the maintainer's own Git identity (`user.name = ChamathDilshanC` and an email verified on that GitHub account), configured per repository — never invent or borrow an email.
* Do **not** add `Co-authored-by:` trailers, "generated by" footers, badges, or any AI model/tool/bot as author, contributor or maintainer — in commits, PRs, changelogs, file headers or docs.
* Never rewrite existing history to change authorship.
