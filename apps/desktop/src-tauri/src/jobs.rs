//! Job queue + scheduler. Jobs run with bounded concurrency (Settings →
//! "Concurrent jobs", default 1 because a 16K job alone can use ~1–2 GB RAM and
//! most of the GPU). Progress is pushed to the HUD at most ~8×/s.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use tokio_util::sync::CancellationToken;
use upscalee_core::pipeline::{run_job, JobError, JobRequest, Outcome, Progress, Stage, Warning};
use upscalee_core::{Dimensions, ModelProfile, Preset};

use crate::state::AppState;

pub const EVENT: &str = "jobs://changed";
const TICK: Duration = Duration::from_millis(120);
const MAX_FINISHED_KEPT: usize = 40;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum JobStatus {
    Queued,
    Running {
        stage: Stage,
        progress: f32,
    },
    #[serde(rename_all = "camelCase")]
    Done {
        output: PathBuf,
        output_name: String,
        dimensions: Dimensions,
        ai: bool,
        warnings: Vec<Warning>,
        elapsed_ms: u64,
    },
    Skipped {
        reason: String,
    },
    Failed {
        message: String,
        suggestion: Option<Preset>,
    },
    Cancelled,
}

impl JobStatus {
    pub fn is_finished(&self) -> bool {
        !matches!(self, JobStatus::Queued | JobStatus::Running { .. })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub id: u64,
    pub file_name: String,
    pub folder: String,
    pub preset: Option<Preset>,
    pub profile: Option<ModelProfile>,
    pub status: JobStatus,
}

struct Entry {
    view: JobView,
    request: Option<JobRequest>,
    cancel: CancellationToken,
}

#[derive(Default)]
struct Inner {
    entries: Vec<Entry>,
    queue: VecDeque<u64>,
    running: usize,
    next_id: u64,
    last_saved: Option<PathBuf>,
}

impl Inner {
    fn entry(&mut self, id: u64) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.view.id == id)
    }

    fn prune(&mut self) {
        let finished = self.entries.iter().filter(|e| e.view.status.is_finished()).count();
        let mut excess = finished.saturating_sub(MAX_FINISHED_KEPT);
        self.entries.retain(|e| {
            if excess > 0 && e.view.status.is_finished() {
                excess -= 1;
                false
            } else {
                true
            }
        });
    }
}

pub struct JobManager {
    app: AppHandle,
    inner: Mutex<Inner>,
    dirty: AtomicBool,
}

fn split_path(path: &Path) -> (String, String) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    let folder = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
    (name, folder)
}

/// Tray tooltip for the current queue (≤ 127 chars, the Win32 limit).
pub fn tooltip(views: &[JobView]) -> String {
    let running: Vec<&JobView> = views.iter().filter(|v| matches!(v.status, JobStatus::Running { .. })).collect();
    let queued = views.iter().filter(|v| matches!(v.status, JobStatus::Queued)).count();
    let text = match running.as_slice() {
        [] if queued == 0 => "Upscalee — ready".to_owned(),
        [] => format!("Upscalee — {queued} queued"),
        [one] => {
            let pct = match one.status {
                JobStatus::Running { progress, .. } => (progress * 100.0).round() as u32,
                _ => 0,
            };
            let mut s = format!("Upscalee — {} {pct}%", one.file_name);
            if queued > 0 {
                s.push_str(&format!(" · {queued} queued"));
            }
            s
        }
        many => format!("Upscalee — {} running · {queued} queued", many.len()),
    };
    text.chars().take(127).collect()
}

impl JobManager {
    pub fn new(app: AppHandle) -> Arc<Self> {
        let manager = Arc::new(Self { app, inner: Mutex::new(Inner::default()), dirty: AtomicBool::new(false) });
        let ticker = manager.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(TICK);
            loop {
                interval.tick().await;
                if ticker.dirty.swap(false, Ordering::AcqRel) {
                    ticker.emit();
                }
            }
        });
        manager
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn snapshot(&self) -> Vec<JobView> {
        self.lock().entries.iter().map(|e| e.view.clone()).collect()
    }

    pub fn has_any(&self) -> bool {
        !self.lock().entries.is_empty()
    }

    fn emit(&self) {
        let views = self.snapshot();
        let _ = self.app.emit_to(crate::hud::LABEL, EVENT, &views);
        crate::tray::set_tooltip(&self.app, &tooltip(&views));
    }

    pub fn enqueue(self: &Arc<Self>, requests: Vec<JobRequest>) {
        if requests.is_empty() {
            return;
        }
        {
            let mut inner = self.lock();
            for request in requests {
                inner.next_id += 1;
                let id = inner.next_id;
                let (file_name, folder) = split_path(&request.source);
                inner.entries.push(Entry {
                    view: JobView {
                        id,
                        file_name,
                        folder,
                        preset: Some(request.preset),
                        profile: Some(request.profile),
                        status: JobStatus::Queued,
                    },
                    request: Some(request),
                    cancel: CancellationToken::new(),
                });
                inner.queue.push_back(id);
            }
            inner.prune();
        }
        self.pump();
    }

    /// Record a file that could not even be queued (unreadable, etc.).
    pub fn add_failed(&self, path: &Path, message: String) {
        {
            let mut inner = self.lock();
            inner.next_id += 1;
            let id = inner.next_id;
            let (file_name, folder) = split_path(path);
            inner.entries.push(Entry {
                view: JobView {
                    id,
                    file_name,
                    folder,
                    preset: None,
                    profile: None,
                    status: JobStatus::Failed { message, suggestion: None },
                },
                request: None,
                cancel: CancellationToken::new(),
            });
            inner.prune();
        }
        self.emit();
    }

    fn concurrency(&self) -> usize {
        self.app.state::<AppState>().settings().concurrent_jobs.max(1) as usize
    }

    fn pump(self: &Arc<Self>) {
        let limit = self.concurrency();
        let mut to_start = Vec::new();
        {
            let mut inner = self.lock();
            while inner.running < limit {
                let Some(id) = inner.queue.pop_front() else { break };
                let Some(entry) = inner.entry(id) else { continue };
                let Some(request) = entry.request.take() else { continue };
                entry.view.status = JobStatus::Running { stage: Stage::Preparing, progress: 0.0 };
                let cancel = entry.cancel.clone();
                inner.running += 1;
                to_start.push((id, request, cancel));
            }
        }
        for (id, request, cancel) in to_start {
            self.spawn_job(id, request, cancel);
        }
        self.emit();
    }

    fn spawn_job(self: &Arc<Self>, id: u64, request: JobRequest, cancel: CancellationToken) {
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let env = manager.app.state::<AppState>().pipeline_env();
            tracing::info!(id, source = %request.source.display(), preset = %request.preset, "job started");
            let reporter = {
                let manager = manager.clone();
                move |p: Progress| manager.progress(id, p)
            };
            let result = run_job(&request, &env, &cancel, &reporter).await;
            manager.finish(id, result);
            manager.pump();
        });
    }

    fn progress(&self, id: u64, progress: Progress) {
        let mut inner = self.lock();
        if let Some(entry) = inner.entry(id) {
            if matches!(entry.view.status, JobStatus::Running { .. }) {
                entry.view.status = JobStatus::Running { stage: progress.stage, progress: progress.fraction };
                self.dirty.store(true, Ordering::Release);
            }
        }
    }

    fn finish(&self, id: u64, result: Result<Outcome, JobError>) {
        let status = match result {
            Ok(Outcome::Saved(saved)) => {
                tracing::info!(id, output = %saved.path.display(), ms = saved.elapsed_ms, ai = saved.ai, "job done");
                JobStatus::Done {
                    output_name: saved.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    output: saved.path,
                    dimensions: saved.dimensions,
                    ai: saved.ai,
                    warnings: saved.warnings,
                    elapsed_ms: saved.elapsed_ms,
                }
            }
            Ok(Outcome::Skipped { reason }) => JobStatus::Skipped { reason: reason.to_string() },
            Err(JobError::Cancelled) => JobStatus::Cancelled,
            Err(err) => {
                tracing::warn!(id, %err, "job failed");
                let suggestion = match &err {
                    JobError::NotEnoughMemory { suggestion, .. } => *suggestion,
                    _ => None,
                };
                JobStatus::Failed { message: err.to_string(), suggestion }
            }
        };

        let reveal = {
            let mut inner = self.lock();
            if let JobStatus::Done { output, .. } = &status {
                inner.last_saved = Some(output.clone());
            }
            if let Some(entry) = inner.entry(id) {
                entry.view.status = status;
            }
            inner.running = inner.running.saturating_sub(1);
            let drained = inner.running == 0 && inner.queue.is_empty();
            if drained {
                inner.last_saved.take()
            } else {
                None
            }
        };
        self.emit();

        if let Some(path) = reveal {
            if self.app.state::<AppState>().settings().open_folder_when_done {
                let _ = self.app.opener().reveal_item_in_dir(&path);
            }
        }
    }

    pub fn cancel(&self, id: u64) {
        {
            let mut inner = self.lock();
            inner.queue.retain(|q| *q != id);
            if let Some(entry) = inner.entry(id) {
                entry.cancel.cancel();
                if matches!(entry.view.status, JobStatus::Queued) {
                    entry.view.status = JobStatus::Cancelled;
                    entry.request = None;
                }
            }
        }
        self.emit();
    }

    pub fn cancel_all(&self) {
        {
            let mut inner = self.lock();
            inner.queue.clear();
            for entry in &mut inner.entries {
                entry.cancel.cancel();
                if matches!(entry.view.status, JobStatus::Queued) {
                    entry.view.status = JobStatus::Cancelled;
                    entry.request = None;
                }
            }
        }
        self.emit();
    }

    pub fn clear_finished(&self) {
        self.lock().entries.retain(|e| !e.view.status.is_finished());
        self.emit();
    }

    pub fn output_of(&self, id: u64) -> Option<PathBuf> {
        let inner = self.lock();
        inner.entries.iter().find(|e| e.view.id == id).and_then(|e| match &e.view.status {
            JobStatus::Done { output, .. } => Some(output.clone()),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(name: &str, status: JobStatus) -> JobView {
        JobView {
            id: 1,
            file_name: name.into(),
            folder: String::new(),
            preset: Some(Preset::K4),
            profile: Some(ModelProfile::Photo),
            status,
        }
    }

    #[test]
    fn tooltip_summarises_the_queue() {
        assert_eq!(tooltip(&[]), "Upscalee — ready");
        let running = view("cat.png", JobStatus::Running { stage: Stage::Saving, progress: 0.456 });
        assert_eq!(tooltip(std::slice::from_ref(&running)), "Upscalee — cat.png 46%");
        assert_eq!(tooltip(&[running.clone(), view("b.png", JobStatus::Queued)]), "Upscalee — cat.png 46% · 1 queued");
        assert_eq!(tooltip(&[running.clone(), running]), "Upscalee — 2 running · 0 queued");
        let long = view(&"x".repeat(300), JobStatus::Running { stage: Stage::Saving, progress: 0.0 });
        assert!(tooltip(&[long]).chars().count() <= 127);
    }

    #[test]
    fn status_serialises_with_a_state_tag() {
        let json = serde_json::to_value(JobStatus::Failed {
            message: "Not enough memory for 16K, try 8K".into(),
            suggestion: Some(Preset::K8),
        })
        .unwrap();
        assert_eq!(json["state"], "failed");
        assert_eq!(json["suggestion"], "8K");
        assert!(JobStatus::Cancelled.is_finished());
        assert!(!JobStatus::Queued.is_finished());
    }
}
