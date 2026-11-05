# Third-party notices

Upscalee bundles or links the components below. This file ships in the install
folder. **Before every public release, re-check each upstream license file
(links below) and each model's license**. Upstream terms take precedence over
this summary.

## Bundled binaries and models (`engine/`)

| Component | Version | License | Upstream |
|-----------|---------|---------|----------|
| Real-ESRGAN models `realesrgan-x4plus`, `realesrgan-x4plus-anime` | v0.2.5.0 release | BSD-3-Clause | https://github.com/xinntao/Real-ESRGAN |
| `realesrgan-ncnn-vulkan.exe` | 20220424 | MIT | https://github.com/xinntao/Real-ESRGAN-ncnn-vulkan |
| ncnn (statically linked into the engine) | as built upstream | BSD-3-Clause | https://github.com/Tencent/ncnn |
| `vcomp140.dll` (Microsoft OpenMP runtime) | 14.x | Microsoft Visual C++ Redistributable license | https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files |

The engine archive is downloaded from the official Real-ESRGAN GitHub release
and verified against the SHA-256 in `engine/engine.lock.json`.

## Application dependencies (statically linked / bundled)

| Component | License |
|-----------|---------|
| Tauri, tao, wry | MIT OR Apache-2.0 |
| `windows`, `windows-sys` (windows-rs) | MIT OR Apache-2.0 |
| `image`, `png`, `zune-jpeg`, `image-webp`, `tiff` | MIT OR Apache-2.0 |
| `fast_image_resize` | MIT OR Apache-2.0 |
| `jpeg-encoder` | MIT OR Apache-2.0 |
| `webp` (Rust bindings) | MIT OR Apache-2.0 |
| libwebp (via `libwebp-sys`) | BSD-3-Clause (Google) |
| tokio, serde, tracing, thiserror, tempfile, uuid, winreg, dirs | MIT and/or Apache-2.0 |
| React, React DOM | MIT |
| `@tauri-apps/api` | MIT OR Apache-2.0 |

The complete dependency tree with exact versions is recorded in `Cargo.lock`
and `pnpm-lock.yaml`.
