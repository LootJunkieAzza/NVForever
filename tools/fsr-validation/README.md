# FSR shader and GPU validation

This standalone test program uses only synthetic inputs, without Fallout game data.
Its code is GPL-3.0-or-later, like NVForever. Dependencies retain their licenses;
the FSR port and AMD shader notices are under `engine/vendor/wgpu-ffx`.

From the repository root, with Rust and a working Vulkan GPU/driver:

```text
cargo fetch --locked --manifest-path engine/Cargo.toml
cargo run --locked --manifest-path tools/fsr-validation/Cargo.toml
```

The harness reads the shader sources fetched into `CARGO_HOME/registry/src` and
prefers this checkout's `engine/vendor/bevy_pbr` patch. For a custom source layout,
set `NVF_SHADER_REGISTRY` to the folder containing the Bevy crate source folders
(for example `bevy_render-0.20.1`). It does not write game files or open a window.
For the complete source archive, set `NVF_SHADER_REGISTRY` to its `cargo-vendor`
directory and add `--offline` to the run command.

It compiles nineteen WESL/WGSL variants with Bevy's actual shader resolver and Naga,
covering motion/depth prepasses, first-person depth, foliage deformation, refraction,
water, and the FSR auxiliary passes. Vulkan GPU readback checks depth conversion,
depth expansion, reactive masks, and the spatial fallback's viewport boundary.
It also executes the vendored clustered-light shader's actual indexing functions:
90 center/edge/corner cases verify the same light cluster at Native AA, Quality,
Balanced, Performance and Ultra Performance scales, including viewport offsets.
Full FSR dispatch checks use HDR, reversed infinite depth and automatic exposure,
zero velocities, native-sized input attachments with a smaller render rectangle,
multiple sequential frames, RCAS off/on, and nonzero flat/patterned colours.
Motion validation executes Bevy's actual velocity function with eight known camera
and object poses, then renders 512 jittered frames of an analytically translated
plane at Native AA and Quality, with RCAS off/on. It checks dilated vector sign/scale and image error
through motion start/stop, with reversed-motion, missing-motion and reversed-jitter
controls. This catches the port's former accumulation binding error: its low-resolution
shader read raw Bevy motion instead of the converted, dilated vector texture. Direction
is established by the direct Bevy/dilated-vector checks alongside the image comparison.

Frame generation (`src/fg.rs`; the wgpu-ffx port of FidelityFX SDK 1.1.4's optical flow and
frame interpolation): a textured plane slides across a 384x256 view at a known speed, with
reversed-infinite depth and Bevy-convention motion vectors, at Native AA and at a Quality
render size (render-resolution motion). Each generated frame is compared with the scene drawn
at the true midpoint, against three baselines (repeating the previous or the current frame,
and their blend), at 6,2 and 14,-5 pixels a frame. Controls: a still scene must come back
unchanged; reversed game motion and a reset on every frame must be clearly worse; at a scene
cut the generated frame must be the new frame (AMD's scene-change rule). Run only these with
`-- --frame-generation-only`; set `WGPU_VALIDATION=1` (Vulkan SDK installed) for the
Khronos validation layer. Any wgpu validation error stops the run.

A failure exits nonzero. Successful GPU checks are numerical evidence for synthetic
inputs; they do not replace visual gameplay checks for motion/disocclusion, water,
particles, camera cuts, resizing and settings changes.

For an opt-in live-game capture, `python tools/fsr-validation/analyze_capture.py
<capture-folder> --output <report.json>` compares prepass motion against camera-only
reprojection and adjacent-frame color matching. It uses only Python's standard
library. Its geometric comparison excludes sky, nearby first-person geometry,
strongly reactive pixels and depth edges; moving actors/deformation can still be
outliers. Capture imagery must stay outside distributable packages.
