# ADR 0005 — Session-scoped pipe; classic menu via `ExplorerCommandHandler`

* Status: **Accepted** (implemented in `crates/upscalee-ipc`, `integration.rs`, `picker.rs`)
* Date: 2026-11-05

## Context

The brief used a fixed pipe `\\.\pipe\upscalee` and a classic verb under
`HKCR\SystemFileAssociations\image\shell` with `command = Upscalee.exe --pick "%1"`.
A fixed pipe name collides between users on the same machine (fast user
switching, RDP). Plain command verbs start one process per selected file, and
the `image` perceived type does not cover `.webp` everywhere.

## Decision

* Pipe name `\\.\pipe\upscalee-<session id>-<user>`, created with
  `FILE_FLAG_FIRST_PIPE_INSTANCE` (single-instance lock) and
  `PIPE_REJECT_REMOTE_CLIENTS`; clients connect with `SECURITY_IDENTIFICATION`
  and grant the server `AllowSetForegroundWindow` so the radial menu can take focus.
* Versioned JSON-lines protocol shared by the DLL, secondary processes and the app.
* Classic verbs are written per extension under
  `HKCU\Software\Classes\SystemFileAssociations\.<ext>\shell\Upscalee` with
  `ExplorerCommandHandler = {CLSID}` (one `Invoke` for a whole multi-selection)
  and a `command` subkey kept as a fallback.
* If the fallback path is used, the app merges picks arriving within a 160 ms
  quiet window (max 700 ms) into one radial session.

## Consequences

* Works for several signed-in users simultaneously.
* Multi-select shows one wheel with `3 × PNG` in both menus.
* The cursor position is captured by the sender (`GetCursorPos`), so the wheel opens where the user clicked.
