//! Blocking named-pipe client. Safe to use from Explorer's process (no async
//! runtime, no panics on error paths, bounded waits).

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, OPEN_EXISTING, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
};
use windows_sys::Win32::System::Pipes::{GetNamedPipeServerProcessId, WaitNamedPipeW};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;
use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

use crate::{decode_response, encode_line, pipe_name, Message, ProtocolError, Response, MAX_MESSAGE_BYTES};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Upscalee is not running")]
    NotRunning,
    #[error("timed out waiting for Upscalee")]
    Timeout,
    #[error("pipe I/O failed (Win32 error {0})")]
    Io(u32),
    #[error("could not launch Upscalee: {0}")]
    Launch(#[from] std::io::Error),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("Upscalee rejected the request: {0}")]
    Rejected(String),
}

pub fn current_session_id() -> u32 {
    let mut session = 0u32;
    // SAFETY: plain out-parameter call.
    let ok = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };
    if ok == 0 {
        0
    } else {
        session
    }
}

pub fn current_user_name() -> String {
    let mut buf = [0u16; 257];
    let mut len = buf.len() as u32;
    // SAFETY: buffer and length describe valid writable memory.
    let ok = unsafe { GetUserNameW(buf.as_mut_ptr(), &mut len) };
    if ok == 0 || len == 0 {
        return std::env::var("USERNAME").unwrap_or_default();
    }
    // `len` includes the terminating NUL.
    String::from_utf16_lossy(&buf[..(len as usize).saturating_sub(1)])
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

struct PipeHandle(HANDLE);

impl Drop for PipeHandle {
    fn drop(&mut self) {
        // SAFETY: handle was returned by CreateFileW and is closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

fn connect(name: &[u16], timeout: Duration) -> Result<PipeHandle, ClientError> {
    let deadline = Instant::now() + timeout;
    loop {
        // SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION: a squatting server can
        // identify us but never impersonate us.
        // SAFETY: `name` is NUL-terminated UTF-16.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            return Ok(PipeHandle(handle));
        }
        // SAFETY: trivially safe.
        let err = unsafe { GetLastError() };
        let now = Instant::now();
        if now >= deadline {
            return Err(if err == ERROR_FILE_NOT_FOUND { ClientError::NotRunning } else { ClientError::Timeout });
        }
        let remaining = (deadline - now).as_millis().clamp(1, 500) as u32;
        match err {
            ERROR_PIPE_BUSY => {
                // SAFETY: NUL-terminated name.
                unsafe { WaitNamedPipeW(name.as_ptr(), remaining) };
            }
            ERROR_FILE_NOT_FOUND => {
                // Server not listening (yet). The caller decides whether to keep waiting.
                if timeout <= Duration::from_millis(50) {
                    return Err(ClientError::NotRunning);
                }
                std::thread::sleep(Duration::from_millis(remaining.min(40) as u64));
            }
            other => return Err(ClientError::Io(other)),
        }
    }
}

fn write_all(pipe: &PipeHandle, mut bytes: &[u8]) -> Result<(), ClientError> {
    while !bytes.is_empty() {
        let mut written = 0u32;
        let chunk = bytes.len().min(64 * 1024) as u32;
        // SAFETY: buffer valid for `chunk` bytes; synchronous handle.
        let ok = unsafe { WriteFile(pipe.0, bytes.as_ptr(), chunk, &mut written, std::ptr::null_mut()) };
        if ok == 0 {
            // SAFETY: trivially safe.
            return Err(ClientError::Io(unsafe { GetLastError() }));
        }
        bytes = &bytes[written as usize..];
    }
    Ok(())
}

fn read_line(pipe: &PipeHandle) -> Result<Vec<u8>, ClientError> {
    let mut out = Vec::with_capacity(64);
    let mut buf = [0u8; 4096];
    loop {
        let mut read = 0u32;
        // SAFETY: buffer valid for its length; synchronous handle.
        let ok = unsafe { ReadFile(pipe.0, buf.as_mut_ptr(), buf.len() as u32, &mut read, std::ptr::null_mut()) };
        if ok == 0 || read == 0 {
            if out.is_empty() {
                // SAFETY: trivially safe.
                return Err(ClientError::Io(unsafe { GetLastError() }));
            }
            return Ok(out);
        }
        out.extend_from_slice(&buf[..read as usize]);
        if out.contains(&b'\n') || out.len() > MAX_MESSAGE_BYTES {
            return Ok(out);
        }
    }
}

/// Send one message to the running app and wait for its response.
///
/// Before writing, the server process is granted the right to take the
/// foreground (we are usually Explorer or a process Explorer just launched, so
/// we hold that right) – this is what lets the radial menu appear focused.
pub fn send(message: &Message, connect_timeout: Duration) -> Result<Response, ClientError> {
    let line = encode_line(message)?;
    let name = wide(&pipe_name());
    let pipe = connect(&name, connect_timeout)?;

    let mut server_pid = 0u32;
    // SAFETY: valid pipe handle and out-parameter.
    if unsafe { GetNamedPipeServerProcessId(pipe.0, &mut server_pid) } != 0 && server_pid != 0 {
        // SAFETY: best effort; failure just means the server cannot steal focus.
        unsafe { AllowSetForegroundWindow(server_pid) };
    }

    write_all(&pipe, &line)?;
    let response = decode_response(&read_line(&pipe)?)?;
    if response.ok {
        Ok(response)
    } else {
        Err(ClientError::Rejected(response.error.unwrap_or_default()))
    }
}

/// Send a message, launching `exe --background` first if no instance is listening.
pub fn send_or_launch(message: &Message, exe: &Path, total_timeout: Duration) -> Result<Response, ClientError> {
    match send(message, Duration::from_millis(250)) {
        Err(ClientError::NotRunning) | Err(ClientError::Timeout) => {}
        other => return other,
    }

    let mut command = Command::new(exe);
    command.arg("--background").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command.spawn()?;

    let deadline = Instant::now() + total_timeout;
    loop {
        match send(message, Duration::from_millis(400)) {
            Err(ClientError::NotRunning) | Err(ClientError::Timeout) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(60));
            }
            other => return other,
        }
    }
}
