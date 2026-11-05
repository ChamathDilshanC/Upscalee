//! Rolling log files in `%LOCALAPPDATA%\Upscalee\logs` (7 days kept).

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

pub fn init(logs_dir: &Path) -> Option<WorkerGuard> {
    let filter = EnvFilter::try_from_env("UPSCALEE_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,upscalee=debug,upscalee_core=debug"));

    let file = std::fs::create_dir_all(logs_dir).ok().and_then(|_| {
        RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("upscalee")
            .filename_suffix("log")
            .max_log_files(7)
            .build(logs_dir)
            .ok()
    });

    let (file_layer, guard) = match file {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            (Some(fmt::layer().with_ansi(false).with_writer(writer)), Some(guard))
        }
        None => (None, None),
    };
    let stderr_layer = cfg!(debug_assertions).then(|| fmt::layer().with_writer(std::io::stderr));

    let _ = tracing_subscriber::registry().with(filter).with(file_layer).with(stderr_layer).try_init();
    guard
}
