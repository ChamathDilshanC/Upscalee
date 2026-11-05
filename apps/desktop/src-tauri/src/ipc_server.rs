//! Named-pipe server: the single-instance lock *and* the inbound channel for the
//! shell extension and secondary `Upscalee.exe` processes.

use std::path::PathBuf;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use upscalee_ipc::{decode_message, encode_line, Origin, Request, Response, MAX_MESSAGE_BYTES};

use crate::picker::PickRequest;
use crate::state::AppState;

const READ_TIMEOUT: Duration = Duration::from_secs(5);

fn server_options(first: bool) -> ServerOptions {
    let mut options = ServerOptions::new();
    options.first_pipe_instance(first).reject_remote_clients(true);
    options
}

/// Try to become *the* Upscalee instance. Fails if another instance owns the pipe.
pub fn claim(name: &str) -> std::io::Result<NamedPipeServer> {
    tauri::async_runtime::block_on(async { server_options(true).create(name) })
}

pub fn spawn(app: AppHandle, first: NamedPipeServer, name: String) {
    tauri::async_runtime::spawn(async move {
        let mut server = first;
        loop {
            if let Err(err) = server.connect().await {
                tracing::warn!(%err, "pipe connect failed");
            } else {
                let app = app.clone();
                let connected = server;
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = handle_client(&app, connected).await {
                        tracing::debug!(%err, "pipe client error");
                    }
                });
            }
            server = loop {
                match server_options(false).create(&name) {
                    Ok(next) => break next,
                    Err(err) => {
                        tracing::error!(%err, "could not create pipe instance; retrying");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            };
        }
    });
}

async fn handle_client(app: &AppHandle, mut pipe: NamedPipeServer) -> std::io::Result<()> {
    let mut line = Vec::with_capacity(256);
    {
        let mut reader = BufReader::new((&mut pipe).take(MAX_MESSAGE_BYTES as u64 + 1));
        match tokio::time::timeout(READ_TIMEOUT, reader.read_until(b'\n', &mut line)).await {
            Ok(result) => {
                result?;
            }
            Err(_) => return Ok(()),
        }
    }
    let response = match decode_message(&line) {
        Ok(message) => {
            dispatch(app, message.request);
            Response::ok()
        }
        Err(err) => {
            tracing::warn!(%err, "rejected pipe message");
            Response::err(err.to_string())
        }
    };
    if let Ok(bytes) = encode_line(&response) {
        pipe.write_all(&bytes).await?;
        pipe.flush().await?;
    }
    Ok(())
}

pub fn dispatch(app: &AppHandle, request: Request) {
    match request {
        Request::Pick { paths, cursor, origin } => {
            tracing::info!(count = paths.len(), ?origin, "pick request");
            let state = app.state::<AppState>();
            let _ = state.picker.send(PickRequest {
                paths: paths.into_iter().map(PathBuf::from).collect(),
                cursor: cursor.map(|c| (c.x, c.y)),
                // The shell extension already batches a multi-selection.
                immediate: origin == Origin::ShellExtension,
            });
        }
        Request::ShowSettings => crate::settings_window::open(app),
        Request::Ping => {}
    }
}
