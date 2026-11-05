//! Pure planning logic: target size, number of AI passes, memory estimate.
//!
//! Every bundled Real-ESRGAN model is a fixed ×4 network, so a plan is a chain
//! of ×4 passes followed by one exact Lanczos3 resize:
//!
//! | scale (target / source) | AI passes                                               |
//! |-------------------------|---------------------------------------------------------|
//! | ≤ 4                     | 1 × (×4), then downscale to target (supersampled)       |
//! | ≤ 16                    | ×4, shrink to ⌈target/4⌉, ×4 again → lands on target    |
//! | > 16                    | ×4, ×4, then Lanczos upscale the rest (tiny source)     |
//!
//! Shrinking the intermediate to exactly `⌈target / 4⌉` before the second pass
//! means the expensive second pass never produces more pixels than needed.

use serde::{Deserialize, Serialize};

use crate::formats::ModelProfile;
use crate::preset::Preset;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

impl Dimensions {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn long_edge(self) -> u32 {
        self.width.max(self.height)
    }

    pub fn short_edge(self) -> u32 {
        self.width.min(self.height)
    }

    pub fn pixels(self) -> u64 {
        self.width as u64 * self.height as u64
    }

    pub fn times(self, factor: u32) -> Self {
        Self::new(self.width.saturating_mul(factor), self.height.saturating_mul(factor))
    }

    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

impl std::fmt::Display for Dimensions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} × {}", self.width, self.height)
    }
}

/// Round to the nearest even integer (never below 2).
fn round_even(value: f64) -> u32 {
    let r = ((value / 2.0).round() * 2.0).max(2.0);
    if r >= u32::MAX as f64 {
        u32::MAX - 1
    } else {
        r as u32
    }
}

/// Output size for `long_edge`, preserving aspect ratio, short edge rounded to even.
pub fn target_dimensions(source: Dimensions, long_edge: u32) -> Dimensions {
    let long = source.long_edge().max(1) as f64;
    let short = round_even(source.short_edge() as f64 * long_edge as f64 / long).min(long_edge);
    if source.width >= source.height {
        Dimensions::new(long_edge, short)
    } else {
        Dimensions::new(short, long_edge)
    }
}

/// One invocation of the ×4 engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiPass {
    /// Size of the image handed to the engine.
    pub input: Dimensions,
    /// Size the engine produces (`input × 4`).
    pub output: Dimensions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub source: Dimensions,
    pub target: Dimensions,
    /// `target.long / source.long`.
    pub scale: f64,
    pub passes: Vec<AiPass>,
    /// Source is so small (>16× needed) that the last step is a plain upscale.
    pub tiny_source: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SkipReason {
    /// The source already has a long edge ≥ the preset.
    AlreadyLargeEnough { preset: Preset },
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkipReason::AlreadyLargeEnough { preset } => write!(f, "Already {preset} or larger"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Decision {
    Skip(SkipReason),
    Upscale(Plan),
}

const SCALE_EPSILON: f64 = 1e-9;

pub fn plan(source: Dimensions, preset: Preset, skip_if_large: bool) -> Decision {
    let long_edge = preset.long_edge();
    if skip_if_large && source.long_edge() >= long_edge {
        return Decision::Skip(SkipReason::AlreadyLargeEnough { preset });
    }
    Decision::Upscale(plan_for_long_edge(source, long_edge))
}

/// Plan an upscale of `source` to `long_edge` (no skip check).
pub fn plan_for_long_edge(source: Dimensions, long_edge: u32) -> Plan {
    let factor = ModelProfile::MODEL_SCALE;
    let target = target_dimensions(source, long_edge);
    let scale = long_edge as f64 / source.long_edge().max(1) as f64;
    let first = AiPass { input: source, output: source.times(factor) };

    let (passes, tiny_source) = if scale <= factor as f64 + SCALE_EPSILON {
        (vec![first], false)
    } else if scale <= (factor * factor) as f64 + SCALE_EPSILON {
        let mid = Dimensions::new(
            target.width.div_ceil(factor).min(first.output.width).max(1),
            target.height.div_ceil(factor).min(first.output.height).max(1),
        );
        let second = AiPass { input: mid, output: mid.times(factor) };
        (vec![first, second], false)
    } else {
        let second = AiPass { input: first.output, output: first.output.times(factor) };
        (vec![first, second], true)
    };

    Plan { source, target, scale, passes, tiny_source }
}

/// Bytes reserved for the engine's models, Vulkan staging buffers and our own
/// baseline, on top of the raw pixel buffers.
const BASELINE_BYTES: u64 = 384 * 1024 * 1024;

impl Plan {
    /// Size of the last AI output (before the exact resize).
    pub fn ai_output(&self) -> Dimensions {
        self.passes.last().map(|p| p.output).unwrap_or(self.source)
    }

    pub fn needs_final_resize(&self) -> bool {
        self.ai_output() != self.target
    }

    /// Conservative peak RAM needed to run this plan with `channels` (3 or 4)
    /// bytes per pixel. Stages run sequentially, so the peak is the largest stage.
    pub fn estimated_peak_bytes(&self, channels: u32) -> u64 {
        let c = channels as u64;
        let mut peak = 0u64;
        for (i, pass) in self.passes.iter().enumerate() {
            // Engine process: holds input + output.
            peak = peak.max((pass.input.pixels() + pass.output.pixels()) * c);
            // Our process: decode the output, then resize into the next input / target.
            let next = self.passes.get(i + 1).map(|p| p.input).unwrap_or(self.target);
            let encode_buffer = if i + 1 == self.passes.len() { self.target.pixels() * c } else { 0 };
            peak = peak.max((pass.output.pixels() + next.pixels()) * c + encode_buffer);
        }
        peak + BASELINE_BYTES
    }

    /// Peak RAM for the non-AI fallback (single Lanczos resize).
    pub fn estimated_fallback_bytes(&self, channels: u32) -> u64 {
        let c = channels as u64;
        (self.source.pixels() + 2 * self.target.pixels()) * c + BASELINE_BYTES
    }

    /// Relative cost of each pass (output pixels), used to weight progress.
    pub fn pass_weights(&self) -> Vec<f64> {
        let total: f64 = self.passes.iter().map(|p| p.output.pixels() as f64).sum();
        if total <= 0.0 {
            return vec![1.0 / self.passes.len().max(1) as f64; self.passes.len()];
        }
        self.passes.iter().map(|p| p.output.pixels() as f64 / total).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(w: u32, h: u32) -> Dimensions {
        Dimensions::new(w, h)
    }

    fn upscale(source: Dimensions, preset: Preset) -> Plan {
        match plan(source, preset, true) {
            Decision::Upscale(p) => p,
            Decision::Skip(r) => panic!("unexpected skip: {r}"),
        }
    }

    #[test]
    fn target_preserves_aspect_and_orientation() {
        assert_eq!(target_dimensions(d(1920, 1080), 3840), d(3840, 2160));
        assert_eq!(target_dimensions(d(1080, 1920), 3840), d(2160, 3840));
        assert_eq!(target_dimensions(d(500, 500), 15360), d(15360, 15360));
    }

    #[test]
    fn target_short_edge_is_even_and_at_least_two() {
        let t = target_dimensions(d(1001, 333), 2048);
        assert_eq!(t.width, 2048);
        assert_eq!(t.height % 2, 0);
        assert_eq!(t.height, 682); // 333 * 2048 / 1001 = 681.3 → 682
        assert_eq!(target_dimensions(d(10_000, 1), 2048), d(2048, 2));
    }

    #[test]
    fn skips_sources_that_are_already_large_enough() {
        assert_eq!(
            plan(d(4000, 3000), Preset::K4, true),
            Decision::Skip(SkipReason::AlreadyLargeEnough { preset: Preset::K4 })
        );
        assert_eq!(
            plan(d(3840, 100), Preset::K4, true),
            Decision::Skip(SkipReason::AlreadyLargeEnough { preset: Preset::K4 })
        );
        // With skipping disabled we still render at the preset size.
        assert!(matches!(plan(d(4000, 3000), Preset::K4, false), Decision::Upscale(_)));
    }

    #[test]
    fn up_to_4x_is_a_single_supersampled_pass() {
        let p = upscale(d(1920, 1080), Preset::K4); // 2×
        assert_eq!(p.passes, vec![AiPass { input: d(1920, 1080), output: d(7680, 4320) }]);
        assert_eq!(p.target, d(3840, 2160));
        assert!(p.needs_final_resize());
        assert!(!p.tiny_source);

        let exact = upscale(d(960, 540), Preset::K4); // exactly 4×
        assert_eq!(exact.passes.len(), 1);
        assert!(!exact.needs_final_resize());
    }

    #[test]
    fn up_to_16x_shrinks_between_passes_to_land_on_target() {
        let p = upscale(d(1000, 563), Preset::K16); // 15.36×
        assert_eq!(p.target, d(15360, 8648));
        assert_eq!(p.passes.len(), 2);
        assert_eq!(p.passes[0], AiPass { input: d(1000, 563), output: d(4000, 2252) });
        assert_eq!(p.passes[1].input, d(3840, 2162));
        assert_eq!(p.passes[1].output, d(15360, 8648));
        assert!(!p.needs_final_resize());
    }

    #[test]
    fn second_pass_never_exceeds_first_pass_output() {
        for w in [961u32, 1000, 1200, 1919] {
            for h in [1u32, 7, 333, 541, 960] {
                let source = d(w, h);
                if let Decision::Upscale(p) = plan(source, Preset::K16, true) {
                    if p.passes.len() == 2 && !p.tiny_source {
                        assert!(p.passes[1].input.width <= p.passes[0].output.width);
                        assert!(p.passes[1].input.height <= p.passes[0].output.height);
                        assert!(p.ai_output().width >= p.target.width);
                        assert!(p.ai_output().height >= p.target.height);
                    }
                }
            }
        }
    }

    #[test]
    fn tiny_sources_use_two_full_passes_then_resize() {
        let p = upscale(d(320, 240), Preset::K16); // 48×
        assert!(p.tiny_source);
        assert_eq!(p.passes[1].output, d(5120, 3840));
        assert_eq!(p.target, d(15360, 11520));
        assert!(p.needs_final_resize());
    }

    #[test]
    fn memory_estimate_grows_with_preset() {
        let src = d(1000, 750);
        let est = |p: Preset| upscale(src, p).estimated_peak_bytes(3);
        assert!(est(Preset::K2) < est(Preset::K8));
        assert!(est(Preset::K8) < est(Preset::K16));
        // 16K RGB target alone is ~400 MB; estimate must exceed that.
        assert!(est(Preset::K16) > 15360 * 11520 * 3);
    }

    #[test]
    fn pass_weights_sum_to_one() {
        let p = upscale(d(700, 500), Preset::K12);
        let sum: f64 = p.pass_weights().iter().sum();
        assert!((sum - 1.0).abs() < 1e-9);
        assert!(p.pass_weights()[1] > p.pass_weights()[0]);
    }
}
