# Upscalee — Security and Privacy

Upscalee is a local, offline desktop tool. The main security concerns are
(1) code that runs **inside Explorer**, (2) the **local IPC channel**, (3) the
**web views**, (4) the **third-party engine binary** and (5) **distribution
integrity** (signing). There is no server, no authentication and no user data
leaves the machine.

## 1. Assets and trust boundaries

| Asset | Boundary |
|-------|----------|
| Explorer stability | `UpscaleeShell.dll` loaded in `explorer.exe` / `dllhost.exe` |
| User's images | Read from user-chosen paths; outputs written next to them |
| User's session | Named pipe between processes of the same user |
| Install folder, HKCU keys | Written by the installer and the app only |

## 2. Threats and mitigations

| Threat | Mitigation | Status |
|--------|-----------|--------|
| Shell extension crash/hang takes down Explorer | Rust, every COM entry `catch_unwind`; `Invoke` returns immediately and does I/O on a worker thread; `GetState` does no disk I/O; module refcount prevents unloading while work runs | Implemented, COM tests |
| Another user / remote host talks to our pipe | Pipe name scoped to session id + user; `PIPE_REJECT_REMOTE_CLIENTS`; first-instance flag; default DACL (creator owner full access) | Implemented |
| Malicious server squats the pipe name to impersonate the client | Client connects with `SECURITY_SQOS_PRESENT \| SECURITY_IDENTIFICATION` (identify, never impersonate) | Implemented |
| Malformed / huge / slow IPC messages | Versioned JSON, 1 MiB cap, 5 s read timeout, one request per connection; paths are only treated as images to decode | Implemented, unit tests |
| Compromised web view content (XSS) escalates to the OS | Strict CSP (`default-src 'self'`); capabilities limited to `core:default`; no fs/shell/opener permissions for JS; "Open/Show in folder" accept **job ids**, not paths | Implemented |
| Outputs overwrite user files | Temp file + `persist_noclobber`; collision suffixes | Implemented, unit tests |
| Tampered engine download | URL + SHA-256 pinned in `engine/engine.lock.json`; build fails on mismatch | Implemented |
| Engine process outlives the app / hogs the PC | Kill-on-close job object, below-normal priority, kill on cancel | Implemented |
| Decompression bomb / huge input exhausts RAM | Memory estimate before decoding the full image; job fails with a suggestion | Implemented (estimate-based) |
| Unsigned binaries (SmartScreen, tampering) | Signing scripts exist; **release signing certificate not yet obtained** | Open |
| Privilege escalation via installer | Per-user install, HKCU only, no services, no elevation | Implemented |

## 3. Secrets

The application holds no secrets. Build-time signing material:

* Never commit `*.pfx`, `*.p12`, `*.cer` (git-ignored).
* Pass certificate paths and passwords as script parameters (`SecureString`),
  never hard-code them, never echo them into logs.
* `scripts/new-dev-certificate.ps1` creates a **development-only** self-signed
  certificate; trusting it (`-Trust`) affects only the developer's machine.

## 4. Privacy

* No network requests, analytics or crash reporting.
* Logs (local only) contain file paths and engine diagnostics; they rotate daily
  and keep 7 files. Users can open or delete them from Settings → About.
* Uninstall with "Delete the application data" removes settings and logs.

## 5. Secure development rules

* No new network access, telemetry or auto-update without an ADR and PRD change.
* Keep the shell extension dependency-light; no blocking or file I/O on Explorer's thread.
* New app commands must not accept raw paths for opening/launching; prefer ids.
* Bump `engine.lock.json` only with a new SHA-256 computed from the official release.
* Run `cargo clippy --workspace --all-targets -- -D warnings` and all tests before merging.

## 6. Reporting a vulnerability

Open a private security advisory on the GitHub repository (Security → Advisories)
or contact the maintainer [@ChamathDilshanC](https://github.com/ChamathDilshanC).
Please don't file public issues for undisclosed vulnerabilities.
