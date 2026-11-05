//! Wire protocol shared by every process that talks to the resident Upscalee app.
//!
//! * The **shell extension** (`UpscaleeShell.dll`, loaded by Explorer) sends a
//!   [`Request::Pick`] when the user clicks "Upscale with Upscalee".
//! * A second `Upscalee.exe` process (classic registry verb, drag-and-drop onto
//!   the exe, Start-menu launch) forwards its command line the same way and
//!   exits immediately.
//! * The resident app runs the server side (see `apps/desktop/src-tauri/src/ipc_server.rs`).
//!
//! Framing: one UTF-8 JSON object per line (`\n` terminated), one request and one
//! response per connection. Messages larger than [`MAX_MESSAGE_BYTES`] are rejected.

use serde::{Deserialize, Serialize};

#[cfg(windows)]
pub mod client;

/// Bump when the message format changes incompatibly.
pub const PROTOCOL_VERSION: u32 = 1;

/// Hard cap for a single request/response line (1 MiB ≈ several thousand paths).
pub const MAX_MESSAGE_BYTES: usize = 1 << 20;

/// A point in physical (device) pixels on the virtual desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Who produced a pick request. Used for logging and for debouncing behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// `IExplorerCommand` in the shell extension (modern or classic menu).
    ShellExtension,
    /// Plain registry verb / drag-and-drop / any command line.
    CommandLine,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Open the radial quality picker for these files at `cursor`.
    Pick { paths: Vec<String>, cursor: Option<Point>, origin: Origin },
    /// Bring up the settings window.
    ShowSettings,
    /// Liveness check; the server answers `ok`.
    Ping,
}

/// Versioned envelope actually written to the pipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub v: u32,
    #[serde(flatten)]
    pub request: Request,
}

impl Message {
    pub fn new(request: Request) -> Self {
        Self { v: PROTOCOL_VERSION, request }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok() -> Self {
        Self { ok: true, error: None }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self { ok: false, error: Some(message.into()) }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("message exceeds {MAX_MESSAGE_BYTES} bytes")]
    TooLarge,
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u32),
    #[error("malformed message: {0}")]
    Malformed(#[from] serde_json::Error),
}

/// Serialise a value as a single `\n`-terminated JSON line.
pub fn encode_line<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    Ok(bytes)
}

/// Parse one request line (with or without the trailing newline).
pub fn decode_message(line: &[u8]) -> Result<Message, ProtocolError> {
    if line.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    let message: Message = serde_json::from_slice(trim_line(line))?;
    if message.v != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion(message.v));
    }
    Ok(message)
}

/// Parse one response line.
pub fn decode_response(line: &[u8]) -> Result<Response, ProtocolError> {
    if line.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    Ok(serde_json::from_slice(trim_line(line))?)
}

fn trim_line(line: &[u8]) -> &[u8] {
    let mut end = line.len();
    while end > 0 && matches!(line[end - 1], b'\n' | b'\r') {
        end -= 1;
    }
    &line[..end]
}

/// Build the pipe name for a given session/user pair.
///
/// The name is scoped to the interactive session *and* the user so that fast
/// user switching / RDP sessions never talk to someone else's instance.
pub fn pipe_name_for(session_id: u32, user: &str) -> String {
    let user: String =
        user.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
    let user = if user.is_empty() { "user".to_owned() } else { user };
    format!(r"\\.\pipe\upscalee-{session_id}-{user}")
}

/// Pipe name for the current process' session and user.
#[cfg(windows)]
pub fn pipe_name() -> String {
    pipe_name_for(client::current_session_id(), &client::current_user_name())
}

/// Image extensions that Upscalee accepts (lower-case, without dot).
///
/// Kept here because both the shell extension (menu visibility) and the core
/// pipeline (decoding) must agree on the list.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff"];

/// True when `path` ends with one of [`SUPPORTED_EXTENSIONS`] (case-insensitive).
pub fn has_supported_extension(path: &str) -> bool {
    let Some((_, ext)) = path.rsplit_once('.') else {
        return false;
    };
    if ext.contains(['\\', '/']) {
        return false;
    }
    SUPPORTED_EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_round_trips_through_a_line() {
        let msg = Message::new(Request::Pick {
            paths: vec![r"C:\Users\me\Pictures\cat ü.png".into()],
            cursor: Some(Point { x: -1200, y: 340 }),
            origin: Origin::ShellExtension,
        });
        let line = encode_line(&msg).unwrap();
        assert_eq!(*line.last().unwrap(), b'\n');
        assert_eq!(decode_message(&line).unwrap(), msg);
    }

    #[test]
    fn wire_format_is_flat_and_tagged() {
        let line = encode_line(&Message::new(Request::Ping)).unwrap();
        assert_eq!(std::str::from_utf8(&line).unwrap(), "{\"v\":1,\"type\":\"ping\"}\n");
    }

    #[test]
    fn rejects_other_versions() {
        let err = decode_message(br#"{"v":99,"type":"ping"}"#).unwrap_err();
        assert!(matches!(err, ProtocolError::UnsupportedVersion(99)));
    }

    #[test]
    fn rejects_garbage_and_oversized_lines() {
        assert!(matches!(decode_message(b"not json"), Err(ProtocolError::Malformed(_))));
        let huge = vec![b' '; MAX_MESSAGE_BYTES + 1];
        assert!(matches!(decode_message(&huge), Err(ProtocolError::TooLarge)));
    }

    #[test]
    fn responses_round_trip() {
        let line = encode_line(&Response::err("nope")).unwrap();
        assert_eq!(decode_response(&line).unwrap(), Response::err("nope"));
        assert_eq!(decode_response(b"{\"ok\":true}\r\n").unwrap(), Response::ok());
    }

    #[test]
    fn pipe_names_are_sanitised_and_scoped() {
        assert_eq!(pipe_name_for(2, "Chamath D"), r"\\.\pipe\upscalee-2-chamath_d");
        assert_eq!(pipe_name_for(0, ""), r"\\.\pipe\upscalee-0-user");
        assert_eq!(pipe_name_for(1, r"a\b"), r"\\.\pipe\upscalee-1-a_b");
    }

    #[test]
    fn extension_filter() {
        assert!(has_supported_extension(r"C:\a\photo.PNG"));
        assert!(has_supported_extension("x.jpeg"));
        assert!(has_supported_extension("x.TiF"));
        assert!(!has_supported_extension("x.gif"));
        assert!(!has_supported_extension(r"C:\folder.png\noext"));
        assert!(!has_supported_extension("noext"));
    }
}
