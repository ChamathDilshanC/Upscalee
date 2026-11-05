# ADR 0006 — Without a Vulkan GPU, fall back to a Lanczos3 resize

* Status: **Accepted**, with a verification item (implemented in `engine.rs`, `pipeline.rs`)
* Date: 2026-11-05

## Context

The brief asked for a CPU fallback via `-g -1`. That flag is a CPU mode of
`waifu2x-ncnn-vulkan`; the help text and README of the bundled
`realesrgan-ncnn-vulkan` 20220424 build document GPU ids only, and the engine
exits with "invalid gpu device" when no Vulkan device exists.
(**To verify** on a machine without a Vulkan driver.)

## Decision

* When the engine reports no GPU (`invalid gpu device`, `vkCreateInstance failed`, …),
  mark the GPU as missing for the session and finish the job with a high-quality
  Lanczos3 resize to the exact target.
* The HUD shows "Standard resize — no Vulkan GPU found"; the radial centre shows
  "No GPU · standard resize" for later picks; Settings → GPU explains the state.
* The same fallback applies when the engine or a model file is missing.

## Consequences

* Users always get an output of the requested size, clearly labelled as non-AI.
* A true CPU inference path (e.g. ncnn CPU build or ONNX Runtime) remains possible
  future work if users without Vulkan matter.
