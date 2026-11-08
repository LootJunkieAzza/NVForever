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

It compiles twenty-one WESL/WGSL variants with Bevy's actual shader resolver and Naga,
covering motion/depth prepasses, first-person depth, foliage deformation, refraction,
water, the ambient occlusion (both its passes before FSR and after tonemapping), and the
FSR auxiliary passes. `NVF_FSR_FP16=1` runs every FSR dispatch on AMD's half-precision
accumulate pass (the engine's default where the GPU has 16-bit shader floats and integers;
the run fails if this one lacks them). Vulkan GPU readback checks depth conversion,
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

Ghosting (`src/ghosting.rs`): the engine's own mask shader (`fsr_reactive.wgsl`, with
`fsr_fallback.wgsl`'s output-luminance pass) feeding real FSR dispatches at Quality. A soft
disc drawn over a textured background as the transparent phase would (alpha-blended smoke or an
additive glow, with no motion vectors of its own) moves across a still or panned view, and a
moving light brightens opaque ground. The output is scored against the scene drawn at the
display pixels: in the disc's or light's trail, on it, and clear of its path. The temporal
masks must shorten the trail against this frame's presence alone (the masks before them),
leave the clear region and a still disc or light as they were, leave a fire-like flicker to
FSR, and beat a control whose lookup goes the wrong way. A still neon tube (thin, very
bright, additive) must flicker far less than with this frame's presence as the reactive
mask. Run only these with `-- --ghosting-only`. Diagnostics, printed and not checked:
`NVF_GHOST_DEBUG=1` FSR's own masks from its debug view, `NVF_GHOST_SWEEP=1` AMD's velocity
factor (`FsrView::configure`), `NVF_GHOST_FENCE=1` and `NVF_GHOST_NEON=1` the masks on fine
detail and on the neon tube against no masks and this frame's presence alone.

Frame generation (`src/fg.rs`; the wgpu-ffx port of FidelityFX SDK 1.1.4's optical flow and
frame interpolation): a textured plane slides across a 384x256 view at a known speed, with
reversed-infinite depth and Bevy-convention motion vectors, at Native AA and at a Quality
render size (render-resolution motion). Each generated frame is compared with the scene drawn
at the true midpoint, against three baselines (repeating the previous or the current frame,
and their blend), at 6,2 and 14,-5 pixels a frame. Controls: a still scene must come back
unchanged; reversed game motion and a reset on every frame must be clearly worse; at a scene
cut the generated frame must be the new frame (AMD's scene-change rule). A square sliding over
a still background, its depth and motion drawn at FSR's jittered positions, must keep the
midpoint with the jitter passed to the prepare pass and without. Run only these with
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
