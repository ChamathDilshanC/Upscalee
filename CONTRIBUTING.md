# Contributing to Upscalee

Thanks for helping! Upscalee is maintained by
[@ChamathDilshanC](https://github.com/ChamathDilshanC), who reviews and merges
all changes. Coding agents must also follow [AGENTS.md](AGENTS.md).

## Prerequisites

* Windows 10 2004+ or Windows 11, x64
* Rust stable (≥ 1.85) with the `x86_64-pc-windows-msvc` toolchain
* Visual Studio 2022 Build Tools: "Desktop development with C++" + Windows 10/11 SDK
* Node.js 20+ and pnpm 10 (`corepack enable`)
* PowerShell 7 (`pwsh`)

## Setup

```powershell
git clone https://github.com/ChamathDilshanC/Upscalee.git
cd Upscalee
pnpm install
pwsh scripts/fetch-engine.ps1          # downloads the pinned Real-ESRGAN engine into engine/bin
cargo test --workspace
pnpm --filter upscalee-desktop test
```

Run the pipeline without the UI:

```powershell
cargo run -p upscalee-core --features cli --release -- C:\path\photo.png --preset 4K
```

Run the app in development (`tauri dev`, hot reload):

```powershell
cargo build -p upscalee-shell          # optional: shell extension for local Explorer testing
pnpm dev
```

> Registering the context menu from a dev build (`target\debug\upscalee.exe --register`)
> points Explorer at files inside `target/`. Explorer then locks `upscalee_shell.dll`
> and rebuilds fail until you run `--unregister` and restart Explorer. Prefer a VM
> or a test user for Explorer work.

## Workflow

1. Open or pick an issue; for larger changes, discuss the approach first (and add an ADR in `docs/adr/` if it changes architecture).
2. Branch from `main`: `feat/<topic>`, `fix/<topic>`, `docs/<topic>`.
3. Keep changes focused; update docs in the same pull request.
4. Commit with [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`).
5. Open a pull request against `main`.

## Code style

* Rust: `cargo fmt --all` (see `rustfmt.toml`), `cargo clippy --workspace --all-targets -- -D warnings`.
* TypeScript: strict mode, `pnpm --filter upscalee-desktop typecheck`.
* UI: design tokens only — see [docs/DESIGN_SYSTEM.md](docs/DESIGN_SYSTEM.md).
* Shell extension: follow the extra rules in [AGENTS.md §5](AGENTS.md#5-conventions).

## Testing requirements

* New logic needs unit tests next to it (Rust `#[cfg(test)]`, Vitest `*.test.ts`).
* Explorer, GPU and installer changes: run the relevant rows of the manual checklist in
  [docs/TESTING_STRATEGY.md](docs/TESTING_STRATEGY.md#4-manual-release-checklist) and list them in the PR.

## Pull request checklist

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` are clean
- [ ] `pnpm --filter upscalee-desktop typecheck` and `test` pass
- [ ] Docs updated (PRD status, ARCHITECTURE contracts, DESIGN_SYSTEM, ROADMAP) where behaviour changed
- [ ] No secrets, signing files or `engine/bin` binaries committed
- [ ] Manual checks performed are listed (or "none needed" with a reason)

## Reporting issues

Include: Windows version/build, GPU model and driver, Upscalee version (Settings → About),
steps to reproduce, the source image size/format, and the relevant lines from the log
(Settings → Open logs folder). Remove personal paths or images you don't want to share.
Security problems: see [docs/SECURITY.md](docs/SECURITY.md#6-reporting-a-vulnerability) — don't open a public issue.

## Sensitive data

Never commit certificates (`*.pfx`), passwords, `.env` files, personal images or logs.
Signing parameters are passed to scripts at build time only.
