//! Collects pick requests and opens one radial menu per burst.
//!
//! The classic registry verb starts one `Upscalee.exe --pick "<file>"` per
//! selected file; each forwards over the pipe within a few milliseconds. We
//! wait for a short quiet period and merge them into one session.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use tauri::AppHandle;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::time::Instant;

/// Quiet period that ends a burst.
const QUIET: Duration = Duration::from_millis(160);
/// Upper bound on how long a burst may keep growing.
const MAX_WAIT: Duration = Duration::from_millis(700);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickRequest {
    pub paths: Vec<PathBuf>,
    pub cursor: Option<(i32, i32)>,
    /// Skip the debounce (the request is already a complete selection).
    pub immediate: bool,
}

pub type PickSender = UnboundedSender<PickRequest>;

pub fn merge(into: &mut PickRequest, next: PickRequest) {
    into.paths.extend(next.paths);
    if into.cursor.is_none() {
        into.cursor = next.cursor;
    }
    into.immediate |= next.immediate;
}

/// Remove duplicates (case-insensitive, like NTFS) while keeping order.
pub fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths.into_iter().filter(|p| seen.insert(p.to_string_lossy().to_lowercase())).collect()
}

pub fn start(app: AppHandle) -> PickSender {
    let (tx, mut rx) = unbounded_channel::<PickRequest>();
    tauri::async_runtime::spawn(async move {
        while let Some(mut batch) = rx.recv().await {
            if !batch.immediate {
                let deadline = Instant::now() + MAX_WAIT;
                while Instant::now() < deadline {
                    match tokio::time::timeout(QUIET, rx.recv()).await {
                        Ok(Some(next)) => {
                            merge(&mut batch, next);
                            if batch.immediate {
                                break;
                            }
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
            }
            let paths = dedupe_paths(batch.paths);
            if !paths.is_empty() {
                crate::radial::open(&app, paths, batch.cursor).await;
            }
        }
    });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupes_case_insensitively_keeping_order() {
        let out =
            dedupe_paths(vec![PathBuf::from(r"C:\A.png"), PathBuf::from(r"C:\b.png"), PathBuf::from(r"c:\a.PNG")]);
        assert_eq!(out, vec![PathBuf::from(r"C:\A.png"), PathBuf::from(r"C:\b.png")]);
    }

    #[test]
    fn merge_keeps_first_cursor_and_sticky_immediate() {
        let mut a = PickRequest { paths: vec![PathBuf::from("a.png")], cursor: None, immediate: false };
        merge(&mut a, PickRequest { paths: vec![PathBuf::from("b.png")], cursor: Some((5, 6)), immediate: true });
        assert_eq!(a.paths.len(), 2);
        assert_eq!(a.cursor, Some((5, 6)));
        assert!(a.immediate);
    }
}
