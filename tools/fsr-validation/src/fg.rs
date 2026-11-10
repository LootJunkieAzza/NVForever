//! FSR 3.1 frame generation (the wgpu-ffx port of FidelityFX SDK 1.1.4's optical flow and
//! frame interpolation): generated frames against the true midpoint of a synthetic scene.
//!
//! A textured plane slides across the view at a known speed. Each real frame gets its colour,
//! a constant reversed-infinite depth and Bevy-convention motion vectors (current minus
//! previous position in UV, scale minus the render size, as `fsr.rs` passes them). The frame
//! generated between real frames t-1 and t is compared with the scene drawn at t-0.5, inside
//! a border the motion cannot fill. Baselines: repeating the previous or the current frame,
//! and their 50/50 blend. Controls: reversed game motion, a reset on every frame, a scene
//! cut, and a still scene.
use half::f16;
use wgpu::*;
use wgpu_ffx::frameinterpolation::{
    FrameInterpolationContext, FrameInterpolationDispatchInfo, FrameInterpolationFlags, FrameInterpolationPrepareInfo,
};
use wgpu_ffx::opticalflow::{OpticalFlowContext, OpticalFlowDispatchInfo};

const DISPLAY: [u32; 2] = [384, 256];
const FRAMES: u32 = 14;
/// Frames scored (after the first real frames have built history).
const SCORED: std::ops::Range<u32> = 4..14;

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

/// Smooth value noise: continuous, with detail at 24 and 8 pixels.
fn noise(x: f32, y: f32, cell: f32, seed: u32) -> f32 {
    let (fx, fy) = (x / cell, y / cell);
    let (ix, iy) = (fx.floor() as i32, fy.floor() as i32);
    let (tx, ty) = (fx - fx.floor(), fy - fy.floor());
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(tx), s(ty));
    let a = hash(ix, iy, seed) + (hash(ix + 1, iy, seed) - hash(ix, iy, seed)) * sx;
    let b = hash(ix, iy + 1, seed) + (hash(ix + 1, iy + 1, seed) - hash(ix, iy + 1, seed)) * sx;
    a + (b - a) * sy
}

/// The scene: two octaves of noise per channel. Seed 77 is the cut's other scene, darker
/// and bluer, so its luminance histograms differ as a real cut's would (AMD's scene-change
/// detection compares them; two textures of the same statistics look like one scene).
fn scene(x: f32, y: f32, seed: u32) -> [f32; 3] {
    let n1 = 0.65 * noise(x, y, 24.0, seed) + 0.35 * noise(x, y, 8.0, seed + 1);
    let n2 = 0.6 * noise(x, y, 24.0, seed + 2) + 0.4 * noise(x, y, 8.0, seed + 3);
    if seed == 77 {
        return [0.15 * n1, 0.1 * n2, 0.25 + 0.5 * n1];
    }
    [n1, n2, 0.5 * (n1 + n2)]
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    render: [u32; 2],
    /// Display pixels per real frame.
    velocity: [f32; 2],
    /// Multiplies the game motion vectors (1 correct, -1 reversed).
    motion_sign: f32,
    reset_every_frame: bool,
    /// From this frame on, a different scene.
    cut_at: Option<u32>,
}

/// Errors over the scored frames: (generated, previous, current, blend), mean absolute RGB.
#[derive(Debug, Default, Clone, Copy)]
struct Score {
    generated: f64,
    previous: f64,
    current: f64,
    blend: f64,
}

fn texture(device: &Device, label: &str, size: [u32; 2], format: TextureFormat, usage: TextureUsages) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn write_f16(queue: &Queue, texture: &Texture, channels: usize, f: impl Fn(u32, u32) -> Vec<f32>) {
    let (w, h) = (texture.width(), texture.height());
    let mut bytes = Vec::with_capacity((w * h) as usize * channels * 2);
    for y in 0..h {
        for x in 0..w {
            for c in f(x, y) {
                bytes.extend(f16::from_f32(c).to_le_bytes());
            }
        }
    }
    queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * channels as u32 * 2), rows_per_image: None },
        texture.size(),
    );
}

fn read_rgba16f(device: &Device, queue: &Queue, texture: &Texture) -> Vec<[f32; 4]> {
    let (w, h) = (texture.width(), texture.height());
    let row = (w * 8).next_multiple_of(COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("fg readback"),
        size: (row * h) as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        TexelCopyBufferInfo { buffer: &buffer, layout: TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None } },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(MapMode::Read, |r| r.unwrap());
    device.poll(PollType::wait_indefinitely()).unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let at = (y * row + x * 8) as usize;
            let c = |i: usize| f16::from_le_bytes([data[at + i * 2], data[at + i * 2 + 1]]).to_f32();
            out.push([c(0), c(1), c(2), c(3)]);
        }
    }
    out
}

fn simulate(device: &Device, queue: &Queue, case: Case) -> (Score, Vec<(f64, f64)>) {
    let flags = FrameInterpolationFlags::DEPTH_INVERTED | FrameInterpolationFlags::DEPTH_INFINITE | FrameInterpolationFlags::HDR_COLOR_INPUT;
    let fi = FrameInterpolationContext::new(device, flags, TextureFormat::Rgba16Float);
    let of = OpticalFlowContext::new(device, true);
    let mut fi_view = fi.create_view(queue, case.render, DISPLAY);
    let mut of_view = of.create_view(DISPLAY);

    let usage_color = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::COPY_SRC;
    let colors = [
        texture(device, "fg color a", DISPLAY, TextureFormat::Rgba16Float, usage_color),
        texture(device, "fg color b", DISPLAY, TextureFormat::Rgba16Float, usage_color),
    ];
    let depth = texture(device, "fg depth", case.render, TextureFormat::R32Float, TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST);
    let motion = texture(device, "fg motion", case.render, TextureFormat::Rg16Float, TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST);
    let output = texture(
        device,
        "fg output",
        DISPLAY,
        TextureFormat::Rgba16Float,
        TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC,
    );

    // A plane 500 units out with the engine's near plane of 5: reversed infinite depth 0.01.
    let near = 5.0f32;
    let depth_bytes: Vec<u8> = (0..case.render[0] * case.render[1]).flat_map(|_| (near / 500.0f32).to_le_bytes()).collect();
    queue.write_texture(
        depth.as_image_copy(),
        &depth_bytes,
        TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(case.render[0] * 4), rows_per_image: None },
        depth.size(),
    );
    let uv_motion = [case.velocity[0] / DISPLAY[0] as f32 * case.motion_sign, case.velocity[1] / DISPLAY[1] as f32 * case.motion_sign];
    write_f16(queue, &motion, 2, |_, _| uv_motion.to_vec());

    let seed_at = |t: u32| if case.cut_at.is_some_and(|cut| t >= cut) { 77 } else { 11 };
    let draw = |t: f32, seed: u32| move |x: u32, y: u32| {
        let c = scene(x as f32 + 0.5 - case.velocity[0] * t, y as f32 + 0.5 - case.velocity[1] * t, seed);
        vec![c[0], c[1], c[2], 1.0]
    };
    let margin = (case.velocity[0].abs().max(case.velocity[1].abs()) * 2.0) as u32 + 24;
    let inside = |x: u32, y: u32| x >= margin && y >= margin && x + margin < DISPLAY[0] && y + margin < DISPLAY[1];

    let mut score = Score::default();
    let mut per_frame = Vec::new();
    let mut scored = 0.0;
    for t in 0..FRAMES {
        let current = &colors[(t % 2) as usize];
        write_f16(queue, current, 4, draw(t as f32, seed_at(t)));
        let mut encoder = device.create_command_encoder(&Default::default());
        let reset = case.reset_every_frame || t == 0;
        fi.prepare(&mut fi_view, &mut encoder, &FrameInterpolationPrepareInfo {
            depth: &depth,
            motion_vectors: &motion,
            render_size: case.render,
            jitter_offset: [0.0, 0.0],
            motion_vector_scale: [-(case.render[0] as f32), -(case.render[1] as f32)],
            camera_near: f32::MAX,
            camera_far: near,
            camera_fov_y: 1.0,
            view_space_to_meters_factor: 1.0 / 70.0,
        });
        of.dispatch(&mut of_view, &mut encoder, &OpticalFlowDispatchInfo {
            color: current,
            reset,
            backbuffer_transfer_function: 2,
            min_max_luminance: [0.0, 80.0],
        });
        fi.dispatch(&mut fi_view, &mut encoder, &FrameInterpolationDispatchInfo {
            current_backbuffer: current,
            current_backbuffer_hudless: None,
            output: &output,
            optical_flow: &of_view,
            render_size: case.render,
            camera_near: f32::MAX,
            camera_far: near,
            camera_fov_y: 1.0,
            view_space_to_meters_factor: 1.0 / 70.0,
            frame_time_delta: 16.7,
            reset,
            backbuffer_transfer_function: 2,
            min_max_luminance: [0.0, 80.0],
            frame_id: t as u64,
        });
        queue.submit([encoder.finish()]);
        if !SCORED.contains(&t) {
            continue;
        }
        let generated = read_rgba16f(device, queue, &output);
        let (previous_seed, current_seed) = (seed_at(t - 1), seed_at(t));
        let truth_seed = current_seed; // at a cut the midpoint is undefined; score against the new scene
        let mut s = [0.0f64; 5];
        let mut n = 0.0;
        for y in 0..DISPLAY[1] {
            for x in 0..DISPLAY[0] {
                if !inside(x, y) {
                    continue;
                }
                let truth = draw(t as f32 - 0.5, truth_seed)(x, y);
                let prev = draw(t as f32 - 1.0, previous_seed)(x, y);
                let cur = draw(t as f32, current_seed)(x, y);
                let g = generated[(y * DISPLAY[0] + x) as usize];
                for c in 0..3 {
                    s[0] += (g[c] - truth[c]).abs() as f64;
                    s[1] += (prev[c] - truth[c]).abs() as f64;
                    s[2] += (cur[c] - truth[c]).abs() as f64;
                    s[3] += (0.5 * (prev[c] + cur[c]) - truth[c]).abs() as f64;
                    s[4] += (g[c] - cur[c]).abs() as f64;
                }
                n += 3.0;
            }
        }
        per_frame.push((s[0] / n, s[4] / n));
        score.generated += s[0] / n;
        score.previous += s[1] / n;
        score.current += s[2] / n;
        score.blend += s[3] / n;
        scored += 1.0;
    }
    score.generated /= scored;
    score.previous /= scored;
    score.current /= scored;
    score.blend /= scored;
    (score, per_frame)
}

pub fn run(device: &Device, queue: &Queue) {
    let mut formats = wgpu_ffx::opticalflow::STORAGE_FORMATS.to_vec();
    formats.extend_from_slice(wgpu_ffx::frameinterpolation::STORAGE_FORMATS);
    // The adapter is queried through the device's features: the passes need read-write storage.
    println!("FG storage formats {formats:?}");

    let native = [384, 256];
    let quality = [256, 170];
    let moving = Case { name: "", render: native, velocity: [6.0, 2.0], motion_sign: 1.0, reset_every_frame: false, cut_at: None };
    let mut failures = Vec::new();
    let mut check = |ok: bool, line: String| {
        println!("{} {line}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            failures.push(line);
        }
    };

    for (label, render) in [("Native AA", native), ("Quality (render-resolution motion)", quality)] {
        let case = Case { name: label, render, ..moving };
        let (s, frames) = simulate(device, queue, case);
        let best_baseline = s.previous.min(s.current).min(s.blend);
        check(
            s.generated < 0.5 * best_baseline,
            format!(
                "FG midpoint, {}: generated {:.4}, previous {:.4}, current {:.4}, blend {:.4} (mean |error|, frames {:?}: {:?})",
                case.name, s.generated, s.previous, s.current, s.blend, SCORED,
                frames.iter().map(|e| format!("{:.4}", e.0)).collect::<Vec<_>>()
            ),
        );
        let fast = Case { name: label, velocity: [14.0, -5.0], ..case };
        let (f, _) = simulate(device, queue, fast);
        check(
            f.generated < 0.5 * f.previous.min(f.current).min(f.blend),
            format!("FG midpoint, {} at 14,-5 px/frame: generated {:.4}, previous {:.4}, current {:.4}, blend {:.4}", fast.name, f.generated, f.previous, f.current, f.blend),
        );
    }

    let (correct, _) = simulate(device, queue, Case { name: "correct", ..moving });
    let (still, _) = simulate(device, queue, Case { name: "still", velocity: [0.0, 0.0], ..moving });
    check(still.generated < 0.01, format!("FG still scene reproduces the frame: {:.5}", still.generated));
    let (reversed, _) = simulate(device, queue, Case { name: "reversed motion", motion_sign: -1.0, ..moving });
    check(
        reversed.generated > 1.5 * correct.generated,
        format!("FG control, reversed game motion: {:.4} against {:.4} correct", reversed.generated, correct.generated),
    );
    let (reset, _) = simulate(device, queue, Case { name: "reset every frame", reset_every_frame: true, ..moving });
    check(
        reset.generated > 2.0 * correct.generated,
        format!("FG control, reset on every frame (no interpolation): {:.4} against {:.4} correct", reset.generated, correct.generated),
    );
    // At a cut there is no midpoint to build. AMD's passes then show the current frame
    // (`computeFrameinterpolation`: no interpolation while `FrameIndexSinceLastReset` is 0,
    // which the scene-change detection sets), not a warp or blend of the two scenes.
    let (_, frames) = simulate(device, queue, Case { name: "scene cut", cut_at: Some(8), ..moving });
    let (_, at_cut_vs_current) = frames[(8 - SCORED.start) as usize];
    check(
        at_cut_vs_current < 0.005,
        format!("FG control, scene cut at frame 8 shows the new frame: |generated - current| {at_cut_vs_current:.4} (per frame {:?})",
            frames.iter().map(|e| format!("{:.4}", e.1)).collect::<Vec<_>>()),
    );

    assert!(failures.is_empty(), "{} frame generation check(s) failed", failures.len());
}
