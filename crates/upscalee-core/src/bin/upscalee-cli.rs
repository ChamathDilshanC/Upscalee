//! Developer CLI for the upscale pipeline (milestone M1).
//!
//! ```text
//! cargo run -p upscalee-core --features cli --release -- photo.png --preset 4K
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use tokio_util::sync::CancellationToken;
use upscalee_core::engine::{self, EngineConfig};
use upscalee_core::pipeline::{run_job, GpuAvailability, JobRequest, Outcome, PipelineEnv};
use upscalee_core::{ModelProfile, OutputFormat, Preset};

#[derive(Clone, Copy, ValueEnum)]
enum Profile {
    Photo,
    Anime,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Keep,
    Png,
    Jpg,
    Webp,
}

#[derive(Parser)]
#[command(name = "upscalee-cli", about = "Upscale images with the Upscalee pipeline")]
struct Args {
    /// Images to upscale.
    #[arg(required_unless_present = "list_gpus")]
    files: Vec<PathBuf>,
    /// Quality preset: 2K, 4K, 8K, 12K or 16K.
    #[arg(short, long, default_value = "4K")]
    preset: Preset,
    #[arg(long, value_enum, default_value_t = Profile::Photo)]
    profile: Profile,
    #[arg(long, value_enum, default_value_t = Format::Keep)]
    format: Format,
    /// Output folder (default: next to each source).
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// Folder containing realesrgan-ncnn-vulkan.exe and models/ (default: ./engine/bin).
    #[arg(long, default_value = "engine/bin")]
    engine: PathBuf,
    /// Engine tile size (0 = auto).
    #[arg(long, default_value_t = 0)]
    tile: u32,
    /// Vulkan GPU index.
    #[arg(long)]
    gpu: Option<u32>,
    /// Upscale even when the source is already at least as large as the preset.
    #[arg(long)]
    no_skip: bool,
    /// Print detected Vulkan GPUs and exit.
    #[arg(long)]
    list_gpus: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .init();
    let args = Args::parse();

    let engine_cfg = EngineConfig::from_dir(&args.engine).map(|e| e.with_overrides(args.gpu, args.tile));
    if engine_cfg.is_none() {
        eprintln!(
            "warning: no engine at {} (run scripts/fetch-engine.ps1); using plain Lanczos3 resize",
            args.engine.display()
        );
    }
    let temp_root = std::env::temp_dir().join("upscalee-cli");

    if args.list_gpus {
        let Some(cfg) = &engine_cfg else {
            return ExitCode::FAILURE;
        };
        let _ = std::fs::create_dir_all(&temp_root);
        return match engine::probe_gpus(cfg, &temp_root).await {
            Ok(gpus) => {
                for gpu in gpus {
                    println!("{}\t{}", gpu.index, gpu.name);
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::FAILURE
            }
        };
    }

    let env = PipelineEnv {
        engine: engine_cfg,
        gpu: std::sync::Arc::new(GpuAvailability::default()),
        temp_root,
        fallback_output_dir: std::env::temp_dir().join("upscalee-out"),
    };
    let cancel = CancellationToken::new();
    {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancel.cancel();
        });
    }

    let mut failures = 0;
    for file in &args.files {
        let req = JobRequest {
            source: file.clone(),
            preset: args.preset,
            profile: match args.profile {
                Profile::Photo => ModelProfile::Photo,
                Profile::Anime => ModelProfile::Anime,
            },
            format: match args.format {
                Format::Keep => OutputFormat::Keep,
                Format::Png => OutputFormat::Png,
                Format::Jpg => OutputFormat::Jpg,
                Format::Webp => OutputFormat::Webp,
            },
            output_folder: args.out.clone(),
            skip_if_large: !args.no_skip,
        };
        let name = file.display().to_string();
        let last = std::sync::Mutex::new(-1i32);
        let report = |p: upscalee_core::pipeline::Progress| {
            let pct = (p.fraction * 100.0) as i32;
            let mut last = last.lock().unwrap();
            if pct != *last {
                *last = pct;
                eprint!("\r{name}: {pct:3}% {:?}          ", p.stage);
            }
        };
        match run_job(&req, &env, &cancel, &report).await {
            Ok(Outcome::Saved(saved)) => {
                eprintln!();
                println!(
                    "{} -> {} ({}, {} ms{})",
                    name,
                    saved.path.display(),
                    saved.dimensions,
                    saved.elapsed_ms,
                    if saved.ai { ", AI" } else { ", Lanczos3 only" }
                );
            }
            Ok(Outcome::Skipped { reason }) => {
                eprintln!();
                println!("{name}: skipped ({reason})");
            }
            Err(err) => {
                eprintln!();
                eprintln!("{name}: error: {err}");
                failures += 1;
            }
        }
    }
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
