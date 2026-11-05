# Upscalee — Roadmap and Task Backlog

Milestones M1–M6 come from the original brief. Status reflects the code in this
repository: **Done** (implemented + automated tests), **Done – verify** (implemented,
needs the on-device checks in [TESTING_STRATEGY.md](TESTING_STRATEGY.md)),
**Todo**.

## Milestones

| Milestone | Scope | Requirements | Status |
|-----------|-------|--------------|--------|
| M1 – Engine CLI | Pipeline, presets, tiling, naming, `upscalee-cli` | FR-007…FR-010, FR-013, FR-019 | Done – verify engine on GPU |
| M2 – Radial UI | Transparent wheel, hotkeys, DPI-aware placement | FR-004…FR-006 | Done – verify on device |
| M3 – Shell integration | Classic verbs, COM handler, pipe IPC, single instance | FR-001, FR-003, FR-016 | Done – verify in Explorer |
| M4 – Win 11 modern menu | Sparse package manifest, build + signing scripts, PackageManager registration | FR-002 | Done – needs signing certificate |
| M5 – Installer | Per-user NSIS, hooks, auto-start, uninstall cleanup | FR-017 | Done – verify install/uninstall |
| M6 – Polish | Multi-select, HUD, settings, error handling, logs | FR-011…FR-015, FR-018 | Done – verify on device |
| v1.1 | Denoise, face enhance, before/after preview, folder right-click | FR-020…FR-023 | Todo |

## Next tasks (priority order)

| ID | Task | Acceptance criteria | Status |
|----|------|---------------------|--------|
| T-01 | Run the manual checklist M1–M15 on Windows 10 and 11 | All rows pass or have issues filed | Todo |
| T-02 | Benchmark 1080p → 4K / 8K / 16K on 2–3 GPUs | Numbers recorded in PRD §12 | Todo |
| T-03 | Obtain code-signing certificate; sign exe/DLL/setup/MSIX | SmartScreen-clean install; modern menu works for users | Todo (blocked on Q1) |
| T-04 | Verify duplicate entries under "Show more options" with the package installed (Q5) | Decision recorded; `LegacyDisable` added if needed | Todo |
| T-05 | Choose and add a project license (Q2) | `LICENSE` file; notices updated | Todo |
| T-06 | Narrator pass on the radial menu | Presets and options announced correctly | Todo |
| T-07 | Denoise toggle (FR-020) | Inner-ring toggle; engine option wired; tests | Todo |
| T-08 | Folder right-click (FR-023) | Non-recursive batch with one wheel | Todo |
| T-09 | Auto-update decision (Q3) | ADR accepted or rejected | Todo |
