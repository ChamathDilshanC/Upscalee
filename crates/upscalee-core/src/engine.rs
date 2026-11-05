//! Runs the bundled `realesrgan-ncnn-vulkan.exe` sidecar.
//!
//! The engine is a separate process on purpose: a GPU driver crash or VRAM
//! exhaustion kills only the sidecar, never the resident app, and cancelling a
//! job is just killing a process.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub const ENGINE_EXE: &str = "realesrgan-ncnn-vulkan.exe";
pub const MODELS_DIR: &str = "models";

/// Smallest tile we retry with before giving up on VRAM errors.
const MIN_TILE: u32 = 32;
/// Lines of engine stderr kept for diagnostics.
const STDERR_TAIL_LINES: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConfig {
    pub exe: PathBuf,
    pub models_dir: PathBuf,
    /// Vulkan device index; `None` lets the engine pick.
    pub gpu: Option<u32>,
    /// `0` = automatic (engine chooses from the VRAM budget).
    pub tile_size: u32,
    /// `load:proc:save` thread counts.
    pub threads: String,
}

impl EngineConfig {
    /// Locate an engine laid out as `<dir>/realesrgan-ncnn-vulkan.exe` + `<dir>/models/`.
    pub fn from_dir(dir: &Path) -> Option<Self> {
        let exe = dir.join(ENGINE_EXE);
        let models_dir = dir.join(MODELS_DIR);
        (exe.is_file() && models_dir.is_dir()).then(|| Self {
            exe,
            models_dir,
            gpu: None,
            tile_size: 0,
            threads: "1:2:2".to_owned(),
        })
    }

    pub fn with_overrides(mut self, gpu: Option<u32>, tile_size: u32) -> Self {
        self.gpu = gpu;
        self.tile_size = tile_size;
        self
    }

    pub fn has_model(&self, model: &str) -> bool {
        self.models_dir.join(format!("{model}.param")).is_file()
            && self.models_dir.join(format!("{model}.bin")).is_file()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GpuInfo {
    pub index: u32,
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("No compatible Vulkan GPU was found")]
    NoGpu,
    #[error("The GPU ran out of memory, even with the smallest tile size")]
    OutOfMemory,
    #[error("Cancelled")]
    Cancelled,
    #[error("The upscaling engine could not read the image")]
    UnreadableInput,
    #[error("The upscaling engine failed (exit code {code:?}): {detail}")]
    Failed { code: Option<i32>, detail: String },
    #[error("The upscaling engine produced no output")]
    NoOutput,
    #[error("The upscaling engine model '{0}' is missing")]
    MissingModel(String),
    #[error("Could not start the upscaling engine: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("The upscaling engine did not respond in time")]
    TimedOut,
}

/// Parse an engine progress line such as `" 12.50%"` into `0.125`.
pub fn parse_progress(line: &str) -> Option<f32> {
    let value: f32 = line.trim().strip_suffix('%')?.trim().parse().ok()?;
    value.is_finite().then(|| (value / 100.0).clamp(0.0, 1.0))
}

/// Parse an ncnn device line: `[0 NVIDIA GeForce RTX 3060]  queueC=2[8]  queueG=0[16]  queueT=1[2]`.
pub fn parse_gpu_line(line: &str) -> Option<GpuInfo> {
    let line = line.trim();
    if !line.contains("queueC=") {
        return None;
    }
    let rest = line.strip_prefix('[')?;
    let close = rest.find("]  queue").or_else(|| rest.find("] queue"))?;
    let inner = &rest[..close];
    let (index, name) = inner.split_once(' ')?;
    Some(GpuInfo { index: index.parse().ok()?, name: name.trim().to_owned() })
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

/// Turn an engine run's observable results into a typed outcome.
pub fn classify(exit_code: Option<i32>, stderr: &str, output_ok: bool) -> Result<(), EngineError> {
    let lower = stderr.to_ascii_lowercase();
    if contains_any(
        &lower,
        &[
            "invalid gpu device",
            "vkcreateinstance failed",
            "no vulkan device",
            "vkenumeratephysicaldevices failed",
            "vkcreatedevice failed",
        ],
    ) {
        return Err(EngineError::NoGpu);
    }
    if contains_any(&lower, &["vkallocatememory failed", "out_of_device_memory", "out_of_host_memory", "out of memory"])
    {
        return Err(EngineError::OutOfMemory);
    }
    if lower.contains("decode image") && lower.contains("failed") {
        return Err(EngineError::UnreadableInput);
    }
    match exit_code {
        Some(0) if output_ok => Ok(()),
        Some(0) => Err(EngineError::NoOutput),
        code => Err(EngineError::Failed {
            code,
            detail: last_meaningful_line(stderr).unwrap_or("no diagnostic output").to_owned(),
        }),
    }
}

fn last_meaningful_line(stderr: &str) -> Option<&str> {
    stderr.lines().rev().map(str::trim).find(|l| !l.is_empty() && parse_progress(l).is_none())
}

/// Tile sizes to try, largest first, halving on each VRAM failure.
pub fn tile_ladder(configured: u32) -> Vec<u32> {
    let mut ladder = Vec::new();
    let mut tile = if configured == 0 {
        ladder.push(0);
        256
    } else {
        configured.max(MIN_TILE)
    };
    while tile >= MIN_TILE {
        if !ladder.contains(&tile) {
            ladder.push(tile);
        }
        tile /= 2;
    }
    ladder
}

fn engine_command(cfg: &EngineConfig, model: &str, input: &Path, output: &Path, tile: u32) -> Command {
    let mut cmd = Command::new(&cfg.exe);
    cmd.arg("-i")
        .arg(input)
        .arg("-o")
        .arg(output)
        .arg("-n")
        .arg(model)
        .arg("-s")
        .arg("4")
        .arg("-t")
        .arg(tile.to_string())
        .arg("-m")
        .arg(&cfg.models_dir)
        .arg("-f")
        .arg("png")
        .arg("-j")
        .arg(&cfg.threads);
    if let Some(gpu) = cfg.gpu {
        cmd.arg("-g").arg(gpu.to_string());
    }
    if let Some(dir) = cfg.exe.parent() {
        cmd.current_dir(dir);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{BELOW_NORMAL_PRIORITY_CLASS, CREATE_NO_WINDOW};
        // Below-normal priority keeps the desktop responsive while the CPU side
        // of the engine (PNG encode/decode, tiling) is busy.
        cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
    cmd
}

struct RunResult {
    exit_code: Option<i32>,
    stderr: String,
}

async fn run_process(
    mut cmd: Command,
    cancel: &CancellationToken,
    on_progress: &mut (dyn FnMut(f32) + Send),
    timeout: Option<Duration>,
) -> Result<RunResult, EngineError> {
    let mut child = cmd.spawn().map_err(EngineError::Spawn)?;
    #[cfg(windows)]
    if let Some(handle) = child.raw_handle() {
        crate::system::bind_child_to_app_lifetime(handle);
    }

    let stderr = child.stderr.take().expect("stderr is piped");
    let mut segments = BufReader::new(stderr).split(b'\n');
    let mut tail: VecDeque<String> = VecDeque::with_capacity(STDERR_TAIL_LINES);
    let deadline = timeout.map(|t| tokio::time::Instant::now() + t);
    let sleep = async {
        match deadline {
            Some(d) => tokio::time::sleep_until(d).await,
            None => std::future::pending().await,
        }
    };
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(EngineError::Cancelled);
            }
            _ = &mut sleep => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(EngineError::TimedOut);
            }
            segment = segments.next_segment() => match segment {
                Ok(Some(bytes)) => {
                    let line = String::from_utf8_lossy(&bytes);
                    let line = line.trim();
                    if let Some(p) = parse_progress(line) {
                        on_progress(p);
                    } else if !line.is_empty() {
                        if tail.len() == STDERR_TAIL_LINES {
                            tail.pop_front();
                        }
                        tail.push_back(line.to_owned());
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }
    }

    let status = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err(EngineError::Cancelled);
        }
        status = child.wait() => status.map_err(EngineError::Spawn)?,
    };

    Ok(RunResult { exit_code: status.code(), stderr: tail.into_iter().collect::<Vec<_>>().join("\n") })
}

/// Run one ×4 pass with a fixed tile size.
pub async fn run_pass(
    cfg: &EngineConfig,
    model: &str,
    input: &Path,
    output: &Path,
    tile: u32,
    cancel: &CancellationToken,
    on_progress: &mut (dyn FnMut(f32) + Send),
) -> Result<(), EngineError> {
    if !cfg.has_model(model) {
        return Err(EngineError::MissingModel(model.to_owned()));
    }
    let _ = tokio::fs::remove_file(output).await;
    let result = run_process(engine_command(cfg, model, input, output, tile), cancel, on_progress, None).await?;
    let output_ok = tokio::fs::metadata(output).await.map(|m| m.len() > 0).unwrap_or(false);
    let outcome = classify(result.exit_code, &result.stderr, output_ok);
    if let Err(err) = &outcome {
        tracing::warn!(%err, tile, model, stderr = %result.stderr, "engine pass failed");
    } else {
        tracing::debug!(tile, model, "engine pass finished");
    }
    outcome
}

/// Run one ×4 pass, retrying with smaller tiles when the GPU runs out of memory.
/// Returns the tile size that succeeded.
pub async fn upscale(
    cfg: &EngineConfig,
    model: &str,
    input: &Path,
    output: &Path,
    cancel: &CancellationToken,
    on_progress: &mut (dyn FnMut(f32) + Send),
) -> Result<u32, EngineError> {
    let mut last = EngineError::OutOfMemory;
    for tile in tile_ladder(cfg.tile_size) {
        on_progress(0.0);
        match run_pass(cfg, model, input, output, tile, cancel, on_progress).await {
            Ok(()) => {
                on_progress(1.0);
                return Ok(tile);
            }
            Err(EngineError::OutOfMemory) => {
                tracing::info!(tile, "VRAM exhausted; retrying with a smaller tile");
                last = EngineError::OutOfMemory;
            }
            Err(other) => return Err(other),
        }
    }
    Err(last)
}

/// List Vulkan devices by running the engine on a tiny image.
pub async fn probe_gpus(cfg: &EngineConfig, scratch_dir: &Path) -> Result<Vec<GpuInfo>, EngineError> {
    let input = scratch_dir.join("probe-in.png");
    let output = scratch_dir.join("probe-out.png");
    let tiny = image::DynamicImage::ImageRgb8(image::RgbImage::new(8, 8));
    crate::imaging::write_intermediate_png(&tiny, &input)
        .map_err(|e| EngineError::Failed { code: None, detail: e.to_string() })?;
    let model = ["realesrgan-x4plus-anime", "realesrgan-x4plus"]
        .into_iter()
        .find(|m| cfg.has_model(m))
        .ok_or_else(|| EngineError::MissingModel("realesrgan-x4plus".into()))?;

    let probe_cfg = EngineConfig { gpu: None, ..cfg.clone() };
    let cancel = CancellationToken::new();
    let result = run_process(
        engine_command(&probe_cfg, model, &input, &output, 0),
        &cancel,
        &mut |_| {},
        Some(Duration::from_secs(45)),
    )
    .await;
    let _ = tokio::fs::remove_file(&input).await;
    let _ = tokio::fs::remove_file(&output).await;
    let result = result?;

    let gpus: Vec<GpuInfo> = result.stderr.lines().filter_map(parse_gpu_line).collect();
    if gpus.is_empty() {
        classify(result.exit_code, &result.stderr, true)?;
        return Err(EngineError::NoGpu);
    }
    Ok(gpus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lines() {
        assert_eq!(parse_progress("12.50%"), Some(0.125));
        assert_eq!(parse_progress(" 0.00%"), Some(0.0));
        assert_eq!(parse_progress("100%"), Some(1.0));
        assert_eq!(parse_progress("[0 GPU]  queueC=1"), None);
        assert_eq!(parse_progress("NaN%"), None);
    }

    #[test]
    fn gpu_lines() {
        assert_eq!(
            parse_gpu_line("[0 NVIDIA GeForce RTX 3060]  queueC=2[8]  queueG=0[16]  queueT=1[2]"),
            Some(GpuInfo { index: 0, name: "NVIDIA GeForce RTX 3060".into() })
        );
        assert_eq!(
            parse_gpu_line("[1 Intel(R) UHD Graphics 630]  queueC=0[1]  queueG=0[1]  queueT=0[1]").map(|g| g.index),
            Some(1)
        );
        assert_eq!(parse_gpu_line("[0 NVIDIA GeForce RTX 3060]  bugsbn1=0  bugbilz=0"), None);
        assert_eq!(parse_gpu_line("[0 NVIDIA]  fp16-p/s/a=1/1/1"), None);
    }

    #[test]
    fn classification() {
        assert!(classify(Some(0), "0.00%\n50.00%", true).is_ok());
        assert!(matches!(classify(Some(-1), "invalid gpu device", false), Err(EngineError::NoGpu)));
        assert!(matches!(classify(Some(0), "vkAllocateMemory failed\n", true), Err(EngineError::OutOfMemory)));
        assert!(matches!(classify(Some(0), "decode image C:\\x.png failed", false), Err(EngineError::UnreadableInput)));
        assert!(matches!(classify(Some(0), "", false), Err(EngineError::NoOutput)));
        match classify(Some(3), "something broke\n10.00%", false) {
            Err(EngineError::Failed { code, detail }) => {
                assert_eq!(code, Some(3));
                assert_eq!(detail, "something broke");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn tile_ladders() {
        assert_eq!(tile_ladder(0), vec![0, 256, 128, 64, 32]);
        assert_eq!(tile_ladder(400), vec![400, 200, 100, 50]);
        assert_eq!(tile_ladder(32), vec![32]);
        assert_eq!(tile_ladder(10), vec![32]);
    }

    #[test]
    fn locates_engine_layout() {
        let dir = tempfile::tempdir().unwrap();
        assert!(EngineConfig::from_dir(dir.path()).is_none());
        std::fs::create_dir(dir.path().join(MODELS_DIR)).unwrap();
        std::fs::write(dir.path().join(ENGINE_EXE), b"").unwrap();
        let cfg = EngineConfig::from_dir(dir.path()).unwrap();
        assert!(!cfg.has_model("realesrgan-x4plus"));
        std::fs::write(dir.path().join("models/realesrgan-x4plus.param"), b"").unwrap();
        std::fs::write(dir.path().join("models/realesrgan-x4plus.bin"), b"").unwrap();
        assert!(cfg.has_model("realesrgan-x4plus"));
    }
}
