# ADR 0003 — Use only ×4 models; shrink between passes; Lanczos3 to the exact size

* Status: **Accepted** (implemented in `crates/upscalee-core/src/planner.rs`)
* Date: 2026-11-05

## Context

The brief planned an ×2 model for `scale ≤ 2`. The official
`realesrgan-ncnn-vulkan-20220424-windows.zip` ships `realesrgan-x4plus`
(photo), `realesrgan-x4plus-anime` and `realesr-animevideov3-x2/x3/x4` (video).
There is no ×2 *photo* model, and `realesrgan-x4plus` must be run with `-s 4`.

## Decision

* Bundle two ×4 models: Photo → `realesrgan-x4plus`, Anime → `realesrgan-x4plus-anime` (~42 MB).
* `scale ≤ 4`: one ×4 pass, then Lanczos3 **down** to the target (supersampling gives
  sharper results than a ×2 model would).
* `4 < scale ≤ 16`: ×4, then shrink to `⌈target / 4⌉`, then ×4 — the expensive second
  pass produces (almost) exactly the target, never 16× the source.
* `scale > 16`: two full ×4 passes, then Lanczos3 up, with a "very small source" warning.

## Consequences

* Less GPU time and RAM for 8K–16K than a naive 4×4 chain.
* For small upscales (e.g. 1.5×) the engine still does ×4 work; acceptable because
  such jobs are fast.
* Adding a model profile = one entry in `ModelProfile` + two files in `engine.lock.json`.
