# ADR 0001 — Write the Explorer shell extension in Rust

* Status: **Accepted** (implemented in `apps/shell-extension`)
* Date: 2026-11-05

## Context

The brief proposed a C++ (or C#/.NET) COM DLL implementing `IExplorerCommand`.
The DLL runs inside `explorer.exe` (classic menu) or a COM surrogate (Windows 11
modern menu). A crash or hang there takes down the user's shell, so the code
must be tiny, defensive and never block. The rest of Upscalee is Rust.

## Decision

Implement `UpscaleeShell.dll` as a Rust `cdylib` with the `windows` crate's
`#[implement]` macro:

* `DllGetClassObject` / `DllCanUnloadNow` exports, `IClassFactory`, `IExplorerCommand`.
* Every COM entry point wrapped in `catch_unwind` → `E_UNEXPECTED` (release
  profile keeps `panic = "unwind"` for this reason).
* `Invoke` only collects paths + cursor and hands off to a worker thread that
  talks to the app over the pipe (shared `upscalee-ipc` crate).
* `GetState` checks extensions only (no disk I/O).
* Two CLSIDs: one for the classic menu (HKCU registration), one declared in the
  sparse package (modern menu), both served by the same factory.

## Consequences

* One toolchain (cargo) for app, core and DLL; the pipe protocol is shared code, not duplicated.
* No .NET runtime loaded into Explorer (C# shell extensions are discouraged by Microsoft for that reason).
* The DLL statically links the Rust std (~few hundred KB) — acceptable.
* COM behaviour is covered by `apps/shell-extension/tests/com.rs` without registry changes.
