//! The Settings window (created on demand, destroyed on close).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::win32;

pub const LABEL: &str = "settings";

pub fn open(app: &AppHandle) {
    let app = app.clone();
    // Window creation must not run on the event-loop thread inside a handler
    // (WebView2 deadlock), so always hop to the async runtime.
    tauri::async_runtime::spawn(async move {
        if let Some(window) = app.get_webview_window(LABEL) {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            if let Ok(hwnd) = window.hwnd() {
                win32::force_foreground(win32::hwnd_from_raw(hwnd.0 as _));
            }
            return;
        }
        let built = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("settings.html".into()))
            .title("Upscalee Settings")
            .inner_size(780.0, 720.0)
            .min_inner_size(640.0, 540.0)
            .center()
            .visible(true)
            .build();
        match built {
            Ok(window) => {
                let _ = window.set_focus();
                if let Ok(hwnd) = window.hwnd() {
                    win32::force_foreground(win32::hwnd_from_raw(hwnd.0 as _));
                }
            }
            Err(err) => tracing::error!(%err, "could not open settings"),
        }
    });
}
