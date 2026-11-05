//! End-to-end job: decode → plan → AI passes → exact resize → encode → atomic save.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use image::DynamicImage;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::engine::{self, EngineConfig, EngineError};
use crate::formats::{self, EncodedFormat, ModelProfile, OutputFormat, SourceFormat};
use crate::imaging::{self, ImagingError};
use crate::naming;
use crate::planner::{self, Decision, Dimensions, Plan, SkipReason};
use crate::preset::Preset;
use crate::system;

// Overall progress budget per stage (fractions of 1.0).
const PREPARE_END: f32 = 0.05;
const AI_END: f32 = 0.80;
const RESIZE_END: f32 = 0.88;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRequest {
    pub source: PathBuf,
    pub preset: Preset,
    pub profile: ModelProfile,
    pub format: OutputFormat,
    /// `None` → next to the source.
    pub output_folder: Option<PathBuf>,
    pub skip_if_large: bool,
}

/// Whether a usable Vulkan GPU exists, learned from the first engine run.
#[derive(Debug, Default)]
pub struct GpuAvailability(AtomicU8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GpuState {
    Unknown,
    Available,
    Missing,
}

impl GpuAvailability {
    pub fn state(&self) -> GpuState {
        match self.0.load(Ordering::Relaxed) {
            1 => GpuState::Available,
            2 => GpuState::Missing,
            _ => GpuState::Unknown,
        }
    }

    pub fn set(&self, state: GpuState) {
        let v = match state {
            GpuState::Unknown => 0,
            GpuState::Available => 1,
            GpuState::Missing => 2,
        };
        self.0.store(v, Ordering::Relaxed);
    }
}

/// Context for running jobs. Cheap to clone; `gpu` is shared process-wide.
#[derive(Debug, Clone)]
pub struct PipelineEnv {
    pub engine: Option<EngineConfig>,
    pub gpu: Arc<GpuAvailability>,
    /// Parent for per-job scratch directories (cleaned automatically).
    pub temp_root: PathBuf,
    /// Used when the chosen output folder is not writable (e.g. `Pictures\Upscalee`).
    pub fallback_output_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Stage {
    Queued,
    Preparing,
    Upscaling { pass: u32, passes: u32 },
    Resizing,
    Saving,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Progress {
    pub stage: Stage,
    /// Overall job progress in `0.0..=1.0`.
    pub fraction: f32,
}

impl Progress {
    fn new(stage: Stage, fraction: f32) -> Self {
        Self { stage, fraction: fraction.clamp(0.0, 1.0) }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Warning {
    /// No Vulkan GPU: used a plain Lanczos3 resize instead of AI.
    NoGpu,
    /// The engine or its model files are missing: used a plain resize.
    EngineMissing,
    /// Needed more than 16× – the last step is a plain resize.
    TinySource,
    /// The output folder was not writable; saved here instead.
    #[serde(rename_all = "camelCase")]
    OutputFolderFallback { folder: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub path: PathBuf,
    pub dimensions: Dimensions,
    pub ai: bool,
    pub warnings: Vec<Warning>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Outcome {
    Saved(Saved),
    Skipped { reason: SkipReason },
}

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("This file type isn't supported")]
    Unsupported,
    #[error("Could not read the image: {0}")]
    Read(String),
    #[error("{}", not_enough_memory_message(.preset, .suggestion))]
    NotEnoughMemory { preset: Preset, needed: u64, available: u64, suggestion: Option<Preset> },
    #[error("{0}")]
    Engine(EngineError),
    #[error("Could not save the result: {0}")]
    Save(String),
    #[error("Cancelled")]
    Cancelled,
    #[error("Internal error: {0}")]
    Internal(String),
}

fn not_enough_memory_message(preset: &Preset, suggestion: &Option<Preset>) -> String {
    match suggestion {
        Some(s) => format!("Not enough memory for {preset}, try {s}"),
        None => format!("Not enough memory for {preset}"),
    }
}

impl From<EngineError> for JobError {
    fn from(err: EngineError) -> Self {
        match err {
            EngineError::Cancelled => JobError::Cancelled,
            other => JobError::Engine(other),
        }
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, JobError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| JobError::Internal(format!("worker thread failed: {e}")))
}

fn check_cancel(cancel: &CancellationToken) -> Result<(), JobError> {
    if cancel.is_cancelled() {
        Err(JobError::Cancelled)
    } else {
        Ok(())
    }
}

/// Largest preset below `preset` that still upscales `source` and fits in `available` bytes.
fn memory_suggestion(
    source: Dimensions,
    preset: Preset,
    channels: u32,
    use_ai: bool,
    available: u64,
) -> Option<Preset> {
    let mut candidate = preset.smaller();
    while let Some(p) = candidate {
        if p.long_edge() <= source.long_edge() {
            return None;
        }
        let plan = planner::plan_for_long_edge(source, p.long_edge());
        let needed = if use_ai { plan.estimated_peak_bytes(channels) } else { plan.estimated_fallback_bytes(channels) };
        if needed <= available {
            return Some(p);
        }
        candidate = p.smaller();
    }
    None
}

/// Run one upscale job. `report` is called with monotonic-ish progress updates.
pub async fn run_job(
    req: &JobRequest,
    env: &PipelineEnv,
    cancel: &CancellationToken,
    report: &(dyn Fn(Progress) + Send + Sync),
) -> Result<Outcome, JobError> {
    let started = Instant::now();
    report(Progress::new(Stage::Preparing, 0.0));

    let source_format = SourceFormat::from_path(&req.source).ok_or(JobError::Unsupported)?;
    let info = {
        let path = req.source.clone();
        blocking(move || imaging::read_info(&path)).await?.map_err(|e| JobError::Read(e.to_string()))?
    };
    let plan = match planner::plan(info.dimensions, req.preset, req.skip_if_large) {
        Decision::Skip(reason) => return Ok(Outcome::Skipped { reason }),
        Decision::Upscale(plan) => plan,
    };

    let encoded = req.format.resolve(source_format);
    let max = encoded.max_dimension();
    if plan.target.width > max || plan.target.height > max {
        return Err(JobError::Save(format!("{encoded:?} cannot store {}", plan.target)));
    }

    let model = req.profile.model_name();
    let engine_ready = env.engine.as_ref().filter(|e| e.has_model(model));
    let use_ai = engine_ready.is_some() && env.gpu.state() != GpuState::Missing;

    check_cancel(cancel)?;
    let decoded = {
        let path = req.source.clone();
        blocking(move || imaging::decode(&path)).await?.map_err(|e| JobError::Read(e.to_string()))?
    };
    // Decoded dimensions are authoritative (should match the header).
    let plan = if decoded.dimensions() == plan.source {
        plan
    } else {
        planner::plan_for_long_edge(decoded.dimensions(), req.preset.long_edge())
    };
    let channels = decoded.channels();

    if let Some(available) = system::available_memory_bytes() {
        let needed = if use_ai { plan.estimated_peak_bytes(channels) } else { plan.estimated_fallback_bytes(channels) };
        if needed > available {
            return Err(JobError::NotEnoughMemory {
                preset: req.preset,
                needed,
                available,
                suggestion: memory_suggestion(plan.source, req.preset, channels, use_ai, available),
            });
        }
    }
    report(Progress::new(Stage::Preparing, PREPARE_END));

    let mut warnings = Vec::new();
    if plan.tiny_source {
        warnings.push(Warning::TinySource);
    }
    let icc_profile = decoded.icc_profile;
    let source_image = decoded.image;

    let mut ai = false;
    let mut final_image = None;
    if let (true, Some(engine_cfg)) = (use_ai, engine_ready) {
        std::fs::create_dir_all(&env.temp_root).map_err(|e| JobError::Internal(e.to_string()))?;
        let scratch = tempfile::Builder::new()
            .prefix("job-")
            .tempdir_in(&env.temp_root)
            .map_err(|e| JobError::Internal(format!("temp folder: {e}")))?;
        match run_ai(&plan, &source_image, engine_cfg, model, scratch.path(), cancel, report).await {
            Ok(image) => {
                env.gpu.set(GpuState::Available);
                ai = true;
                final_image = Some(image);
            }
            Err(JobError::Engine(EngineError::NoGpu)) => {
                tracing::warn!("no Vulkan GPU available; falling back to Lanczos3");
                env.gpu.set(GpuState::Missing);
                warnings.push(Warning::NoGpu);
            }
            Err(other) => return Err(other),
        }
    } else if engine_ready.is_none() {
        warnings.push(Warning::EngineMissing);
    } else {
        warnings.push(Warning::NoGpu);
    }

    let final_image = match final_image {
        Some(image) => image,
        None => {
            check_cancel(cancel)?;
            report(Progress::new(Stage::Resizing, AI_END));
            let target = plan.target;
            blocking(move || imaging::resize(&source_image, target))
                .await?
                .map_err(|e| JobError::Internal(e.to_string()))?
        }
    };

    check_cancel(cancel)?;
    report(Progress::new(Stage::Saving, RESIZE_END));
    let dimensions = Dimensions::new(final_image.width(), final_image.height());
    let (path, fallback_used) = {
        let out_dir = naming::output_dir(&req.source, req.output_folder.as_deref());
        let fallback_dir = env.fallback_output_dir.clone();
        let stem = naming::source_stem(&req.source);
        let ext = formats::output_extension(&req.source, encoded);
        let preset = req.preset;
        blocking(move || {
            save_with_fallback(
                &final_image,
                encoded,
                icc_profile.as_deref(),
                &out_dir,
                &fallback_dir,
                &stem,
                preset,
                &ext,
            )
        })
        .await?
        .map_err(|e| JobError::Save(e.to_string()))?
    };
    if let Some(folder) = fallback_used {
        warnings.push(Warning::OutputFolderFallback { folder });
    }
    report(Progress::new(Stage::Saving, 1.0));

    Ok(Outcome::Saved(Saved { path, dimensions, ai, warnings, elapsed_ms: started.elapsed().as_millis() as u64 }))
}

async fn run_ai(
    plan: &Plan,
    source: &DynamicImage,
    engine_cfg: &EngineConfig,
    model: &str,
    scratch: &Path,
    cancel: &CancellationToken,
    report: &(dyn Fn(Progress) + Send + Sync),
) -> Result<DynamicImage, JobError> {
    let passes = plan.passes.len() as u32;
    let weights = plan.pass_weights();
    let span = AI_END - PREPARE_END;

    let first_input = scratch.join("pass0-in.png");
    {
        let image = source.clone();
        let path = first_input.clone();
        blocking(move || imaging::write_intermediate_png(&image, &path))
            .await?
            .map_err(|e| JobError::Internal(format!("could not stage input: {e}")))?;
    }

    let mut done = 0.0f64;
    for (i, pass) in plan.passes.iter().enumerate() {
        check_cancel(cancel)?;
        let input = scratch.join(format!("pass{i}-in.png"));
        let output = scratch.join(format!("pass{i}-out.png"));
        if i > 0 {
            // Shrink the previous output to exactly what this pass needs.
            let previous = scratch.join(format!("pass{}-out.png", i - 1));
            let expected = plan.passes[i - 1].output;
            let wanted = pass.input;
            let staged = input.clone();
            blocking(move || -> Result<(), ImagingError> {
                let image = load_engine_output(&previous, expected)?;
                let image = imaging::resize(&image, wanted)?;
                imaging::write_intermediate_png(&image, &staged)?;
                let _ = std::fs::remove_file(&previous);
                Ok(())
            })
            .await?
            .map_err(|e| JobError::Internal(format!("between passes: {e}")))?;
        }

        let stage = Stage::Upscaling { pass: i as u32 + 1, passes };
        let weight = weights[i];
        let base = done;
        report(Progress::new(stage.clone(), PREPARE_END + span * base as f32));
        let mut on_progress = |p: f32| {
            let fraction = PREPARE_END + span * (base + weight * p as f64) as f32;
            report(Progress::new(stage.clone(), fraction));
        };
        let tile = engine::upscale(engine_cfg, model, &input, &output, cancel, &mut on_progress).await?;
        tracing::info!(pass = i + 1, passes, tile, "AI pass complete");
        let _ = std::fs::remove_file(&input);
        done += weight;
    }

    check_cancel(cancel)?;
    report(Progress::new(Stage::Resizing, AI_END));
    let last = scratch.join(format!("pass{}-out.png", plan.passes.len() - 1));
    let expected = plan.ai_output();
    let target = plan.target;
    blocking(move || -> Result<DynamicImage, ImagingError> {
        let image = load_engine_output(&last, expected)?;
        imaging::resize(&image, target)
    })
    .await?
    .map_err(|e| JobError::Internal(format!("final resize: {e}")))
}

fn load_engine_output(path: &Path, expected: Dimensions) -> Result<DynamicImage, ImagingError> {
    let decoded = imaging::decode(path)?;
    if decoded.dimensions() != expected {
        return Err(ImagingError::Decode(format!("engine produced {} instead of {expected}", decoded.dimensions())));
    }
    Ok(decoded.image)
}

fn is_location_error(err: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(err.kind(), PermissionDenied | NotFound | ReadOnlyFilesystem | NotADirectory)
        // ERROR_WRITE_PROTECT, ERROR_BAD_NETPATH, ERROR_BAD_NET_NAME
        || matches!(err.raw_os_error(), Some(19) | Some(53) | Some(67))
}

#[allow(clippy::too_many_arguments)]
fn save_with_fallback(
    image: &DynamicImage,
    format: EncodedFormat,
    icc_profile: Option<&[u8]>,
    out_dir: &Path,
    fallback_dir: &Path,
    stem: &str,
    preset: Preset,
    ext: &str,
) -> Result<(PathBuf, Option<PathBuf>), ImagingError> {
    let save_in = |dir: &Path| -> Result<PathBuf, ImagingError> {
        std::fs::create_dir_all(dir)?;
        imaging::save_atomic(image, format, icc_profile, dir, || {
            naming::unique_output_path(dir, stem, preset, ext, |p| p.exists())
        })
    };
    match save_in(out_dir) {
        Ok(path) => Ok((path, None)),
        Err(ImagingError::Io(err)) if is_location_error(&err) && out_dir != fallback_dir => {
            tracing::warn!(%err, dir = %out_dir.display(), "output folder not writable; using fallback");
            save_in(fallback_dir).map(|p| (p, Some(fallback_dir.to_path_buf())))
        }
        Err(other) => Err(other),
    }
}

/// Remove `job-*` scratch folders left behind by a crash (older than `max_age`).
pub fn clean_stale_scratch(temp_root: &Path, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(temp_root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("job-") {
            continue;
        }
        let old_enough = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age >= max_age);
        if old_enough {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use std::sync::Mutex;

    fn env(dir: &Path) -> PipelineEnv {
        PipelineEnv {
            engine: None,
            gpu: Arc::default(),
            temp_root: dir.join("temp"),
            fallback_output_dir: dir.join("fallback"),
        }
    }

    fn write_png(path: &Path, w: u32, h: u32) {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| Rgb([x as u8, y as u8, 99])));
        img.save(path).unwrap();
    }

    fn request(source: PathBuf, preset: Preset) -> JobRequest {
        JobRequest {
            source,
            preset,
            profile: ModelProfile::Photo,
            format: OutputFormat::Keep,
            output_folder: None,
            skip_if_large: true,
        }
    }

    #[tokio::test]
    async fn fallback_path_produces_exact_target_and_name() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("photo.png");
        write_png(&src, 192, 108);
        let updates = Mutex::new(Vec::new());
        let report = |p: Progress| updates.lock().unwrap().push(p.fraction);

        let outcome =
            run_job(&request(src, Preset::K2), &env(dir.path()), &CancellationToken::new(), &report).await.unwrap();
        let Outcome::Saved(saved) = outcome else { panic!("expected saved") };
        assert_eq!(saved.path, dir.path().join("photo_2K.png"));
        assert_eq!(saved.dimensions, Dimensions::new(2048, 1152));
        assert!(!saved.ai);
        assert_eq!(saved.warnings, vec![Warning::EngineMissing]);
        assert_eq!(imaging::read_info(&saved.path).unwrap().dimensions, Dimensions::new(2048, 1152));
        let updates = updates.into_inner().unwrap();
        assert_eq!(updates.last().copied(), Some(1.0));
        assert!(updates.windows(2).all(|w| w[0] <= w[1]), "progress is monotonic: {updates:?}");
    }

    #[tokio::test]
    async fn second_run_gets_a_numbered_name_and_jpeg_keeps_format() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("cat.jpeg");
        DynamicImage::ImageRgb8(RgbImage::new(100, 200)).save_with_format(&src, image::ImageFormat::Jpeg).unwrap();
        let e = env(dir.path());
        let req = request(src, Preset::K2);
        for expected in ["cat_2K.jpeg", "cat_2K (1).jpeg"] {
            let Outcome::Saved(saved) = run_job(&req, &e, &CancellationToken::new(), &|_| {}).await.unwrap() else {
                panic!()
            };
            assert_eq!(saved.path.file_name().unwrap(), expected);
            assert_eq!(saved.dimensions, Dimensions::new(1024, 2048));
        }
    }

    #[tokio::test]
    async fn large_sources_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("big.png");
        write_png(&src, 2100, 10);
        let outcome =
            run_job(&request(src, Preset::K2), &env(dir.path()), &CancellationToken::new(), &|_| {}).await.unwrap();
        assert_eq!(outcome, Outcome::Skipped { reason: SkipReason::AlreadyLargeEnough { preset: Preset::K2 } });
    }

    #[tokio::test]
    async fn unwritable_output_folder_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.png");
        write_png(&src, 64, 64);
        // A *file* where the output folder should be makes create_dir_all fail.
        let blocker = dir.path().join("not-a-dir");
        std::fs::write(&blocker, b"x").unwrap();
        let mut req = request(src, Preset::K2);
        req.output_folder = Some(blocker.join("sub"));
        let e = env(dir.path());
        let result = run_job(&req, &e, &CancellationToken::new(), &|_| {}).await;
        match result {
            Ok(Outcome::Saved(saved)) => {
                assert!(saved.path.starts_with(&e.fallback_output_dir));
                assert!(saved.warnings.iter().any(|w| matches!(w, Warning::OutputFolderFallback { .. })));
            }
            // Some file systems report a non-location error for this case.
            Err(JobError::Save(_)) => {}
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn cancelled_jobs_stop_early() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.png");
        write_png(&src, 64, 64);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = run_job(&request(src, Preset::K2), &env(dir.path()), &cancel, &|_| {}).await.unwrap_err();
        assert!(matches!(err, JobError::Cancelled));
        assert!(!dir.path().join("a_2K.png").exists());
    }

    #[tokio::test]
    async fn unsupported_and_corrupt_inputs_fail_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let gif = dir.path().join("a.gif");
        std::fs::write(&gif, b"GIF89a").unwrap();
        let e = env(dir.path());
        let err = run_job(&request(gif, Preset::K2), &e, &CancellationToken::new(), &|_| {}).await.unwrap_err();
        assert!(matches!(err, JobError::Unsupported));

        let bad = dir.path().join("bad.png");
        std::fs::write(&bad, b"definitely not a png").unwrap();
        let err = run_job(&request(bad, Preset::K2), &e, &CancellationToken::new(), &|_| {}).await.unwrap_err();
        assert!(matches!(err, JobError::Read(_)));
    }

    #[test]
    fn memory_suggestion_picks_largest_fitting_preset() {
        let src = Dimensions::new(1000, 750);
        let need_8k = planner::plan_for_long_edge(src, Preset::K8.long_edge()).estimated_peak_bytes(3);
        assert_eq!(memory_suggestion(src, Preset::K16, 3, true, need_8k), Some(Preset::K8));
        assert_eq!(memory_suggestion(src, Preset::K16, 3, true, 1), None);
        // Never suggests a preset that wouldn't upscale.
        assert_eq!(memory_suggestion(Dimensions::new(3000, 10), Preset::K4, 3, true, u64::MAX), None);
        assert_eq!(not_enough_memory_message(&Preset::K16, &Some(Preset::K8)), "Not enough memory for 16K, try 8K");
    }

    #[test]
    fn stale_scratch_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("job-old")).unwrap();
        std::fs::create_dir(dir.path().join("keep-me")).unwrap();
        clean_stale_scratch(dir.path(), Duration::ZERO);
        assert!(!dir.path().join("job-old").exists());
        assert!(dir.path().join("keep-me").exists());
    }
}
