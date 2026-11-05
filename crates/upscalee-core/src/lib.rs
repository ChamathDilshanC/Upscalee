//! Upscalee core: everything that turns "this file, this preset" into a saved,
//! upscaled image. No UI and no Tauri dependency, so it is unit-testable and
//! reusable from the `upscalee-cli` developer binary.

pub mod engine;
pub mod formats;
pub mod imaging;
pub mod naming;
pub mod pipeline;
pub mod planner;
pub mod preset;
pub mod settings;
pub mod system;

pub use formats::{EncodedFormat, ModelProfile, OutputFormat, SourceFormat};
pub use planner::{Decision, Dimensions, Plan, SkipReason};
pub use preset::Preset;
pub use settings::Settings;
