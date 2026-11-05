# ADR 0004 — Custom activity HUD instead of Windows toast notifications

* Status: **Accepted** (implemented in `apps/desktop/src/hud`, `src-tauri/src/hud.rs`)
* Date: 2026-11-05

## Context

The brief used toasts for "Upscaling to 8K…", progress and "Done — Open / Show
in folder". Native toasts with live progress need WinRT notification data
binding and a registered AppUserModelID (Start-menu shortcut); they don't work
reliably in dev builds, can be suppressed by Focus Assist, and can't host a
Cancel button bound to our job queue.

## Decision

A small frameless, transparent, always-on-top web view in the bottom-right of the
monitor where the user last opened the radial menu:

* shown with `SW_SHOWNOACTIVATE` so it never steals focus;
* sized to its content (the page reports its height);
* rows show stage, %, warnings, Cancel / Open / Show in folder (actions take a
  job id, not a path);
* auto-hides 8 s after the last job (15 s if something failed), paused on hover.

The tray tooltip mirrors progress for users who dismiss the HUD.

## Consequences

* Consistent look with the radial menu (same design tokens, light/dark).
* One more web view kept in memory (~ tens of MB) — acceptable for a tray app.
* Native toast integration can be added later for Focus-Assist-aware summaries.
