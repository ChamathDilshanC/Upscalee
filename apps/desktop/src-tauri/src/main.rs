// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cli;
mod commands;
mod hud;
mod integration;
mod ipc_server;
mod jobs;
mod logging;
mod paths;
mod picker;
mod radial;
mod settings_window;
mod state;
mod tray;
mod win32;

fn main() {
    std::process::exit(app::main());
}
