//! Ghosting behind blended shapes under FSR 3.1: the engine's own mask shader
//! (`engine/nvforever/src/fsr_reactive.wgsl`) and the wgpu-ffx upscaler, on a synthetic scene.
//!
//! A textured background (static, or panned with Bevy-convention motion vectors) carries a
//! soft disc drawn over it as the transparent phase would: alpha-blended smoke or an
//! additive glow. The disc moves across the view and has no motion vectors of its own, as
//! no blended shape does. Each frame the opaque-only image and the finished image (both
//! sampled at the frame's jittered positions) go through the mask shader, and its reactive
//! and transparency/composition outputs into FSR. The output is compared with the scene
//! drawn at the display pixel centres:
//! - in the disc's trail (where it was during the last frames, not where it is now), where a
//!   ghost shows;
//! - on the disc;
//! - far from its path (control).
//!
//! Variants: no masks; this frame's presence only (last frame's presence and FSR's last
//! output withheld: the masks before 2026-11-08); the temporal masks (the lookup of last
//! frame's presence and the light term); and those with the lookup's motion reversed (a
//! control that the lookup follows the history, in the panned scene). A still disc must come
//! out nearly the same.
//!
//! A moving light on opaque ground (no blended shape) checks the masks' light-change term
//! against FSR's own shading-change detection alone (last frame's luminance withheld), and a
//! fire-like flicker of the whole ground checks that the term leaves it to FSR. A still neon
//! tube (thin, very bright, additive, fixed to the surface behind it) must flicker far less
//! than with this frame's presence as the reactive mask (the Strip's signs, 2026-11-08).
//! Fine detail (`Kind::Fence`) is for the opt-in diagnostics below.
use half::f16;
use wgpu::{util::DeviceExt, *};
use wgpu_ffx::{FsrConfigureKey, FsrContext, FsrContextFlags, FsrContextInfo, FsrDispatchFlags, FsrDispatchInfo};

const RENDER: u32 = 128;
/// Quality: 1.5x per axis.
const DISPLAY: u32 = 192;
const FRAMES: u32 = 44;
/// Frames scored, after the history has built up.
const SCORED_FROM: u32 = 14;
/// Frames back that count as the trail.
const TRAIL_FRAMES: u32 = 8;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// Alpha-blended light grey (smoke, dust, glass).
    Smoke,
    /// An additive HDR glow (a lamp's halo, a projectile, a muzzle flash).
    Glow,
    /// No blended shape: a moving light brightening the opaque ground.
    Light,
    /// No blended shape: the whole ground flickers by a few percent a frame, as a fire's
    /// light does (lights.rs `LightMotion::Flicker`).
    Flicker,
    /// No blended shape: thin dark slats and a bright ridge over the background, as fences
    /// and roof edges against a hill are: fine, high-contrast detail that aliases.
    Fence,
    /// A neon tube: a thin, very bright additive line fixed to the surface behind it (it
    /// moves with the background), as the Strip's signs are.
    Neon,
}

/// Distance (render pixels) from the neon tube's centre line, in background coordinates.
fn neon_distance(u: f32, v: f32) -> f32 {
    (v - 64.0 - 0.12 * u).abs() / (1.0f32 + 0.12 * 0.12).sqrt()
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Masks {
    None,
    /// This frame's presence only: last frame's presence and FSR's last output luminance
    /// cleared every frame, which is what the masks were before the temporal lookup and the
    /// light term.
    CurrentOnly,
    Temporal,
    /// The lookup goes the wrong way (motion negated for the mask pass only).
    TemporalReversed,
}

#[derive(Clone, Copy)]
struct Case {
    kind: Kind,
    /// Render pixels per frame.
    shape_velocity: [f32; 2],
    /// The background's motion, render pixels per frame (a camera pan).
    pan: [f32; 2],
}

#[derive(Default, Debug, Clone, Copy)]
struct Score {
    trail: f64,
    shape: f64,
    clear: f64,
    /// Mean frame-to-frame change of the output away from the disc (shimmer).
    change: f64,
    /// The same on the disc (the neon tube).
    shape_change: f64,
}

fn background(x: f32, y: f32) -> [f32; 3] {
    let tau = std::f32::consts::TAU;
    // Foliage-like detail: several octaves, some of it near the render texel size.
    let a = (tau * x / 9.0).sin() * (tau * y / 11.0).cos();
    let b = (tau * (x + 0.7 * y) / 4.3).sin();
    let c = (tau * (x - 1.3 * y) / 17.0).cos();
    [
        0.30 + 0.12 * a + 0.06 * b + 0.05 * c,
        0.34 + 0.10 * b + 0.08 * c,
        0.22 + 0.08 * a - 0.05 * c,
    ]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

const RADIUS: f32 = 9.0;

thread_local! {
    /// Diagnostics: draw FSR's debug view on the last frame and print its masks.
    static DEBUG_VIEW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The upscaler tuning the engine uses: none, AMD's defaults (fsr.rs configures nothing).
const TUNING: &[(FsrConfigureKey, f32)] = &[];

fn center(case: &Case, t: f32) -> [f32; 2] {
    [24.0 + case.shape_velocity[0] * t, 64.0 + case.shape_velocity[1] * t]
}

/// The opaque-only image and the finished one at render position (x, y), frame t.
fn sample(case: &Case, x: f32, y: f32, t: f32) -> ([f32; 3], [f32; 3]) {
    let bg = background(x - case.pan[0] * t, y - case.pan[1] * t);
    let c = center(case, t);
    let d = ((x - c[0]).powi(2) + (y - c[1]).powi(2)).sqrt();
    let w = smoothstep(RADIUS, RADIUS * 0.55, d);
    match case.kind {
        Kind::Smoke => {
            let a = 0.65 * w;
            (bg, std::array::from_fn(|i| bg[i] * (1.0 - a) + 0.85 * a))
        }
        Kind::Glow => (bg, std::array::from_fn(|i| bg[i] + [3.0, 2.0, 0.8][i] * w)),
        Kind::Light => {
            let lit: [f32; 3] = std::array::from_fn(|i| bg[i] * (1.0 + 4.0 * smoothstep(RADIUS * 1.6, 0.0, d)));
            (lit, lit)
        }
        Kind::Flicker => {
            let f = 0.92 + 0.08 * (0.9 * t).sin();
            let lit: [f32; 3] = std::array::from_fn(|i| bg[i] * f);
            (lit, lit)
        }
        Kind::Neon => {
            let (u, v) = (x - case.pan[0] * t, y - case.pan[1] * t);
            let w = smoothstep(0.7, 0.35, neon_distance(u, v));
            (bg, std::array::from_fn(|i| bg[i] + [14.0, 6.0, 1.5][i] * w))
        }
        Kind::Fence => {
            // Slats 1.2 render pixels wide every 7, slightly slanted, and a ridge line every
            // 23 rows at a shallow angle; they move with the background.
            let (u, v) = (x - case.pan[0] * t, y - case.pan[1] * t);
            let slat = (u + 0.15 * v).rem_euclid(7.0) < 1.2;
            let ridge = (v - 0.08 * u).rem_euclid(23.0) < 0.9;
            let c = if slat { [0.03, 0.025, 0.02] } else if ridge { [1.6, 1.5, 1.3] } else { bg };
            (c, c)
        }
    }
}

fn tonemap(c: f32) -> f32 {
    c / (1.0 + c)
}

fn texture(device: &Device, label: &str, size: u32, format: TextureFormat, usage: TextureUsages) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: Extent3d { width: size, height: size, depth_or_array_layers: 1 },
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

/// The mask shader's two passes (fsr_reactive.wgsl): presence and luminance, then the masks.
pub(crate) struct MaskPipelines {
    pub(crate) presence: RenderPipeline,
    pub(crate) masks: RenderPipeline,
}

pub(crate) fn mask_pipelines(device: &Device, source: &str) -> MaskPipelines {
    let shader = device.create_shader_module(ShaderModuleDescriptor { label: Some("fsr_reactive"), source: ShaderSource::Wgsl(source.into()) });
    let r8 = Some(ColorTargetState { format: TextureFormat::R8Unorm, blend: None, write_mask: ColorWrites::ALL });
    let r16 = Some(ColorTargetState { format: TextureFormat::R16Float, blend: None, write_mask: ColorWrites::ALL });
    let make = |entry: &str, targets: &[Option<ColorTargetState>]| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(entry),
            layout: None,
            vertex: VertexState { module: &shader, entry_point: Some("vertex"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(FragmentState { module: &shader, entry_point: Some(entry), compilation_options: Default::default(), targets }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    MaskPipelines { presence: make("presence_pass", &[r8.clone(), r16]), masks: make("fragment", &[r8.clone(), r8]) }
}

fn simulate(context: &FsrContext, device: &Device, queue: &Queue, (pipeline, luma_pipeline): (&MaskPipelines, &RenderPipeline), case: Case, masks: Masks, tuning: &[(FsrConfigureKey, f32)]) -> Score {
    let sampled = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    let storage = TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING;
    let attachment = TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    // Display-size inputs with the render rectangle filled, as the engine's are.
    let opaque = texture(device, "ghost opaque", DISPLAY, TextureFormat::Rgba16Float, sampled);
    let scene = texture(device, "ghost scene", DISPLAY, TextureFormat::Rgba16Float, sampled);
    let motion = texture(device, "ghost motion", DISPLAY, TextureFormat::Rg16Float, sampled);
    let mask_motion = texture(device, "ghost mask motion", DISPLAY, TextureFormat::Rg16Float, sampled);
    let reactive = texture(device, "ghost reactive", RENDER, TextureFormat::R8Unorm, attachment);
    let composition = texture(device, "ghost composition", RENDER, TextureFormat::R8Unorm, attachment);
    let presence = [
        texture(device, "ghost presence a", RENDER, TextureFormat::R8Unorm, attachment),
        texture(device, "ghost presence b", RENDER, TextureFormat::R8Unorm, attachment),
    ];
    let history_luma = texture(device, "ghost history luma", RENDER, TextureFormat::R16Float, attachment);
    let depth = texture(device, "ghost depth", RENDER, TextureFormat::Depth32Float, TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT);
    let output = texture(device, "ghost output", DISPLAY, TextureFormat::Rgba16Float, storage | TextureUsages::COPY_SRC);
    let dilated_depth = texture(device, "ghost dilated depth", RENDER, TextureFormat::R32Float, storage);
    let dilated_motion = texture(device, "ghost dilated motion", RENDER, TextureFormat::Rg16Float, storage);
    let reconstructed = device.create_buffer(&BufferDescriptor {
        label: Some("ghost reconstructed depth"),
        size: (RENDER * RENDER * 4) as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    {
        // A plane 500 units out at the engine's near plane of 5.
        let mut encoder = device.create_command_encoder(&Default::default());
        let view = depth.create_view(&Default::default());
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("ghost depth"),
            color_attachments: &[],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &view,
                depth_ops: Some(Operations { load: LoadOp::Clear(0.01), store: StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        drop(_pass);
        queue.submit([encoder.finish()]);
    }
    let mv = [case.pan[0] / RENDER as f32, case.pan[1] / RENDER as f32];
    write_f16(queue, &motion, 2, |_, _| mv.to_vec());
    let sign = if masks == Masks::TemporalReversed { -1.0 } else { 1.0 };
    write_f16(queue, &mask_motion, 2, |_, _| vec![mv[0] * sign, mv[1] * sign]);

    let jitter_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("ghost jitter"),
        size: 16,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let sampler = device.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..Default::default() });
    let views = |t: &Texture| t.create_view(&Default::default());
    let (opaque_view, scene_view, mask_motion_view) = (views(&opaque), views(&scene), views(&mask_motion));
    let presence_views = [views(&presence[0]), views(&presence[1])];
    let history_luma_view = views(&history_luma);
    let output_view = views(&output);
    let resolution_bytes: Vec<u8> = [RENDER, RENDER, DISPLAY, DISPLAY].iter().flat_map(|v| v.to_le_bytes()).collect();
    let resolution = device.create_buffer_init(&util::BufferInitDescriptor { label: Some("ghost resolution"), contents: &resolution_bytes, usage: BufferUsages::UNIFORM });
    let luma_group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &luma_pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry { binding: 0, resource: BindingResource::TextureView(&output_view) },
            BindGroupEntry { binding: 1, resource: resolution.as_entire_binding() },
            BindGroupEntry { binding: 2, resource: BindingResource::Sampler(&sampler) },
        ],
    });
    let (reactive_view, composition_view) = (views(&reactive), views(&composition));
    let luma_now = texture(device, "ghost luma now", RENDER, TextureFormat::R16Float, attachment);
    let luma_now_view = views(&luma_now);
    let presence_group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &pipeline.presence.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry { binding: 0, resource: BindingResource::TextureView(&opaque_view) },
            BindGroupEntry { binding: 1, resource: BindingResource::TextureView(&scene_view) },
        ],
    });
    let groups: Vec<BindGroup> = (0..2)
        .map(|i| {
            device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &pipeline.masks.get_bind_group_layout(0),
                entries: &[
                    BindGroupEntry { binding: 2, resource: BindingResource::TextureView(&mask_motion_view) },
                    // Frame parity i writes presence[i] and reads the other.
                    BindGroupEntry { binding: 3, resource: BindingResource::TextureView(&presence_views[1 - i]) },
                    BindGroupEntry { binding: 4, resource: BindingResource::TextureView(&history_luma_view) },
                    BindGroupEntry { binding: 5, resource: BindingResource::Sampler(&sampler) },
                    BindGroupEntry { binding: 6, resource: jitter_buffer.as_entire_binding() },
                    BindGroupEntry { binding: 7, resource: BindingResource::TextureView(&presence_views[i]) },
                    BindGroupEntry { binding: 8, resource: BindingResource::TextureView(&luma_now_view) },
                ],
            })
        })
        .collect();

    let mut view = context.create_view(queue, [RENDER; 2], [DISPLAY; 2]);
    for (key, value) in tuning {
        view.configure(*key, *value);
    }
    let phases = wgpu_ffx::get_jitter_phase_count(RENDER as i32, DISPLAY as i32);
    let scale = RENDER as f32 / DISPLAY as f32;
    let mut score = Score::default();
    let mut counts = [0usize; 5];
    let mut last: Option<Vec<f32>> = None;
    for t in 0..FRAMES {
        let jitter = wgpu_ffx::get_jitter_offset(t as i32, phases);
        let jitters: Vec<u8> = [jitter[0], jitter[1], 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        queue.write_buffer(&jitter_buffer, 0, &jitters);
        let tf = t as f32;
        let inputs: Vec<([f32; 3], [f32; 3])> = (0..RENDER * RENDER)
            .map(|i| sample(&case, (i % RENDER) as f32 + 0.5 - jitter[0], (i / RENDER) as f32 + 0.5 - jitter[1], tf))
            .collect();
        let at = |x: u32, y: u32| (x < RENDER && y < RENDER).then(|| inputs[(y * RENDER + x) as usize]);
        write_f16(queue, &opaque, 4, |x, y| at(x, y).map_or(vec![16.0, 0.0, 16.0, 1.0], |(o, _)| vec![o[0], o[1], o[2], 1.0]));
        write_f16(queue, &scene, 4, |x, y| at(x, y).map_or(vec![16.0, 0.0, 16.0, 1.0], |(_, s)| vec![s[0], s[1], s[2], 1.0]));
        let parity = (t % 2) as usize;
        let mut encoder = device.create_command_encoder(&Default::default());
        if masks == Masks::CurrentOnly || t == 0 {
            encoder.clear_texture(&presence[1 - parity], &ImageSubresourceRange::default());
            encoder.clear_texture(&history_luma, &ImageSubresourceRange::default());
        }
        if masks != Masks::None {
            let target = |view| Some(RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Clear(Color::BLACK), store: StoreOp::Store } });
            for (label, targets, pipe, group) in [
                ("ghost presence", [target(&presence_views[parity]), target(&luma_now_view)], &pipeline.presence, &presence_group),
                ("ghost masks", [target(&reactive_view), target(&composition_view)], &pipeline.masks, &groups[parity]),
            ] {
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some(label),
                    color_attachments: &targets,
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(pipe);
                pass.set_bind_group(0, group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        let with_masks = masks != Masks::None;
        context
            .dispatch(&mut view, &mut encoder, &FsrDispatchInfo {
                color: scene.clone(),
                depth: depth.clone(),
                motion_vectors: motion.clone(),
                exposure: None,
                reactive_mask: with_masks.then(|| reactive.clone()),
                transparency_and_composition: with_masks.then(|| composition.clone()),
                dilated_depth: dilated_depth.clone(),
                dilated_motion_vectors: dilated_motion.clone(),
                reconstructed_previous_depth: reconstructed.clone(),
                output: output.clone(),
                jitter_offset: jitter,
                motion_vector_scale: [-(RENDER as f32); 2],
                render_size: [RENDER; 2],
                upscale_size: [DISPLAY; 2],
                enable_sharpening: false,
                sharpness: 0.0,
                frame_time_delta: 16.6667,
                pre_exposure: 1.0,
                reset_history: t == 0,
                camera_near: f32::MAX,
                camera_far: 5.0,
                camera_fov_y: std::f32::consts::FRAC_PI_3,
                view_space_to_meters_factor: 1.0 / 70.0,
                flags: if DEBUG_VIEW.with(|d| d.get()) && t == FRAMES - 1 { FsrDispatchFlags::DRAW_DEBUG_VIEW } else { FsrDispatchFlags::empty() },
            })
            .unwrap();
        if masks != Masks::None && masks != Masks::CurrentOnly {
            // What FSR's history shows, for the next frame's light term (fsr.rs does the same).
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("ghost history luma"),
                color_attachments: &[Some(RenderPassColorAttachment { view: &history_luma_view, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Clear(Color::BLACK), store: StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(luma_pipeline);
            pass.set_bind_group(0, &luma_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        let raw = crate::gpu::read(device, queue, &output, 8);
        if DEBUG_VIEW.with(|d| d.get()) && t == FRAMES - 1 {
            // FSR's debug view: a 3x3 grid; bottom row disocclusion, shading change, reactive (green).
            let cell = DISPLAY / 3;
            for (name, column) in [("disocclusion", 0), ("shading change", 1), ("reactive", 2)] {
                let mut values = Vec::new();
                for y in 2 * cell..3 * cell {
                    for x in column * cell..(column + 1) * cell {
                        let at = ((y * DISPLAY + x) * 8 + 2) as usize;
                        values.push(f16::from_le_bytes([raw[at], raw[at + 1]]).to_f32());
                    }
                }
                let max = values.iter().cloned().fold(0.0f32, f32::max);
                let mean = values.iter().sum::<f32>() / values.len() as f32;
                let over = values.iter().filter(|v| **v > 0.05).count() as f32 / values.len() as f32;
                println!("DEBUG {:?} {:?} {name}: max {max:.3}, mean {mean:.4}, share over 0.05 {over:.3}", case.kind, masks);
            }
            return Score::default();
        }
        let luma: Vec<f32> = raw
            .as_chunks::<8>()
            .0
            .iter()
            .map(|p| {
                let c = |i: usize| f16::from_le_bytes([p[i * 2], p[i * 2 + 1]]).to_f32();
                tonemap(0.2126 * c(0) + 0.7152 * c(1) + 0.0722 * c(2))
            })
            .collect();
        if t >= SCORED_FROM {
            let now = center(&case, tf);
            for y in 8..DISPLAY - 8 {
                for x in 8..DISPLAY - 8 {
                    let (rx, ry) = ((x as f32 + 0.5) * scale, (y as f32 + 0.5) * scale);
                    let truth = sample(&case, rx, ry, tf).1;
                    let truth = tonemap(0.2126 * truth[0] + 0.7152 * truth[1] + 0.0722 * truth[2]);
                    let error = (luma[(y * DISPLAY + x) as usize] - truth).abs() as f64;
                    let distance = |c: [f32; 2]| ((rx - c[0]).powi(2) + (ry - c[1]).powi(2)).sqrt();
                    let from_now = distance(now);
                    // Nearest approach of the disc's centre over the last frames, carried along
                    // as the history is (by the background's motion each frame since).
                    let swept = (1..=TRAIL_FRAMES)
                        .map(|k| {
                            let c = center(&case, tf - k as f32);
                            distance([c[0] + case.pan[0] * k as f32, c[1] + case.pan[1] * k as f32])
                        })
                        .fold(f32::MAX, f32::min);
                    let neon = case.kind == Kind::Neon;
                    let near_neon = neon_distance(rx - case.pan[0] * tf, ry - case.pan[1] * tf) < 2.5;
                    let region = if neon {
                        Some(if near_neon { 1 } else { 2 })
                    } else if from_now < RADIUS * 0.5 {
                        Some(1)
                    } else if from_now > RADIUS + 2.0 && swept < RADIUS * 0.8 {
                        Some(0)
                    } else if from_now > 3.0 * RADIUS && swept > 3.0 * RADIUS {
                        Some(2)
                    } else {
                        None
                    };
                    match region {
                        Some(0) => score.trail += error,
                        Some(1) => {
                            score.shape += error;
                            if let Some(last) = &last {
                                score.shape_change += (luma[(y * DISPLAY + x) as usize] - last[(y * DISPLAY + x) as usize]).abs() as f64;
                                counts[4] += 1;
                            }
                        }
                        Some(2) => {
                            score.clear += error;
                            if let Some(last) = &last {
                                score.change += (luma[(y * DISPLAY + x) as usize] - last[(y * DISPLAY + x) as usize]).abs() as f64;
                                counts[3] += 1;
                            }
                        }
                        _ => {}
                    }
                    if let Some(r) = region {
                        counts[r] += 1;
                    }
                }
            }
        }
        last = Some(luma);
    }
    score.trail /= counts[0].max(1) as f64;
    score.shape /= counts[1].max(1) as f64;
    score.clear /= counts[2].max(1) as f64;
    score.change /= counts[3].max(1) as f64;
    score.shape_change /= counts[4].max(1) as f64;
    score
}

fn sweep(context: &FsrContext, device: &Device, queue: &Queue, pipelines: (&MaskPipelines, &RenderPipeline)) {
    let cases = [
        ("light still", Case { kind: Kind::Light, shape_velocity: [2.5, 0.0], pan: [0.0, 0.0] }),
        ("light pan", Case { kind: Kind::Light, shape_velocity: [2.5, 0.4], pan: [-0.75, 0.5] }),
        ("smoke still", Case { kind: Kind::Smoke, shape_velocity: [2.5, 0.0], pan: [0.0, 0.0] }),
        ("smoke pan", Case { kind: Kind::Smoke, shape_velocity: [2.5, 0.4], pan: [-0.75, 0.5] }),
        ("glow still", Case { kind: Kind::Glow, shape_velocity: [2.5, 0.0], pan: [0.0, 0.0] }),
        ("flicker", Case { kind: Kind::Flicker, shape_velocity: [0.0, 0.0], pan: [0.0, 0.0] }),
        ("glow parked", Case { kind: Kind::Glow, shape_velocity: [0.0, 0.0], pan: [0.0, 0.0] }),
    ];
    let cases = [
        ("fence still", Case { kind: Kind::Fence, shape_velocity: [0.0, 0.0], pan: [0.0, 0.0] }),
        ("fence pan", Case { kind: Kind::Fence, shape_velocity: [0.0, 0.0], pan: [-0.75, 0.5] }),
        ("fence slow pan", Case { kind: Kind::Fence, shape_velocity: [0.0, 0.0], pan: [0.3, 0.1] }),
        ("fence fast pan", Case { kind: Kind::Fence, shape_velocity: [0.0, 0.0], pan: [2.0, 0.3] }),
    ]
    .into_iter()
    .chain(cases)
    .collect::<Vec<_>>();
    for velocity in [1.0f32, 0.5, 0.25, 0.1, 0.0] {
        {
            let tuning = [(FsrConfigureKey::VelocityFactor, velocity)];
            let mut line = format!("SWEEP velocity factor {velocity:.2}:");
            for (name, case) in cases.iter().copied() {
                let s = simulate(context, device, queue, pipelines, case, Masks::Temporal, &tuning);
                line += &format!(" | {name} trail {:.4} shape {:.4} clear {:.4} change {:.5}", s.trail, s.shape, s.clear, s.change);
            }
            println!("{line}");
        }
    }
}

pub fn run(device: &Device, queue: &Queue, mask_shader: &str, fallback_shader: &str) {
    let context = FsrContext::new(FsrContextInfo {
        device: device.clone(),
        flags: FsrContextFlags::HIGH_DYNAMIC_RANGE | FsrContextFlags::DEPTH_INVERTED | FsrContextFlags::DEPTH_INFINITE | FsrContextFlags::AUTO_EXPOSURE,
    });
    let pipeline = mask_pipelines(device, mask_shader);
    let luma_pipeline = crate::gpu::pipeline(device, fallback_shader, "luma", Some(TextureFormat::R16Float));
    if std::env::var("NVF_GHOST_DEBUG").is_ok() {
        DEBUG_VIEW.with(|d| d.set(true));
        for case in [
            Case { kind: Kind::Light, shape_velocity: [2.5, 0.0], pan: [0.0, 0.0] },
            Case { kind: Kind::Flicker, shape_velocity: [0.0, 0.0], pan: [0.0, 0.0] },
        ] {
            for masks in [Masks::CurrentOnly, Masks::Temporal] {
                simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, masks, TUNING);
            }
        }
        DEBUG_VIEW.with(|d| d.set(false));
        for v in [0.0f32, 0.5, 1.0, 2.5] {
            let case = Case { kind: Kind::Light, shape_velocity: [v, 0.0], pan: [0.0, 0.0] };
            for masks in [Masks::CurrentOnly, Masks::Temporal] {
                let s = simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, masks, TUNING);
                println!("DEBUG light at {v} px/frame {masks:?}: trail {:.4} spot {:.4} clear {:.4}", s.trail, s.shape, s.clear);
            }
        }
        return;
    }
    if std::env::var("NVF_GHOST_NEON").is_ok() {
        for (name, pan) in [("still", [0.0, 0.0]), ("pan", [-0.75, 0.5])] {
            let case = Case { kind: Kind::Neon, shape_velocity: [0.0, 0.0], pan };
            for masks in [Masks::None, Masks::CurrentOnly, Masks::Temporal] {
                let s = simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, masks, TUNING);
                println!("NEON {name} {masks:?}: on the tube {:.4} change {:.5}, clear {:.4} change {:.5}", s.shape, s.shape_change, s.clear, s.change);
            }
        }
        for (name, case) in [
            ("smoke still", Case { kind: Kind::Smoke, shape_velocity: [2.5, 0.0], pan: [0.0, 0.0] }),
            ("smoke pan", Case { kind: Kind::Smoke, shape_velocity: [2.5, 0.4], pan: [-0.75, 0.5] }),
            ("glow still", Case { kind: Kind::Glow, shape_velocity: [2.5, 0.0], pan: [0.0, 0.0] }),
            ("glow pan", Case { kind: Kind::Glow, shape_velocity: [2.5, 0.4], pan: [-0.75, 0.5] }),
            ("light still", Case { kind: Kind::Light, shape_velocity: [2.5, 0.0], pan: [0.0, 0.0] }),
            ("fence still", Case { kind: Kind::Fence, shape_velocity: [0.0, 0.0], pan: [0.0, 0.0] }),
        ] {
            let s = simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, Masks::Temporal, TUNING);
            println!("NEON {name} Temporal: trail {:.4} shape {:.4} clear {:.4} change {:.5}", s.trail, s.shape, s.clear, s.change);
        }
        return;
    }
    if std::env::var("NVF_GHOST_FENCE").is_ok() {
        for (name, pan) in [("still", [0.0, 0.0]), ("slow pan", [0.3, 0.1]), ("pan", [-0.75, 0.5]), ("fast pan", [2.0, 0.3])] {
            let case = Case { kind: Kind::Fence, shape_velocity: [0.0, 0.0], pan };
            for masks in [Masks::None, Masks::CurrentOnly, Masks::Temporal] {
                let s = simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, masks, TUNING);
                println!("FENCE {name} {masks:?}: clear {:.4} change {:.5}", s.clear, s.change);
            }
        }
        for pan in [[0.0, 0.0], [-0.75, 0.5]] {
            let case = Case { kind: Kind::Light, shape_velocity: [2.5, 0.0], pan };
            let s = simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, Masks::Temporal, TUNING);
            println!("FENCE light {pan:?}: trail {:.4} spot {:.4} clear {:.4}", s.trail, s.shape, s.clear);
        }
        return;
    }
    if std::env::var("NVF_GHOST_SWEEP").is_ok() {
        sweep(&context, device, queue, (&pipeline, &luma_pipeline));
        return;
    }
    let mut failures = Vec::new();
    let mut check = |ok: bool, line: String| {
        println!("{} {line}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            failures.push(line);
        }
    };
    let show = |s: &Score| format!("trail {:.4}, on the shape {:.4}, clear {:.4}, clear change {:.5}", s.trail, s.shape, s.clear, s.change);
    // How much smaller the trail must be: under a pan, FSR's own reconstruction error of the
    // moving background is part of every region's error.
    let needed = |case: &Case| if case.pan == [0.0, 0.0] { 0.8 } else { 1.0 };
    for kind in [Kind::Smoke, Kind::Glow] {
        for (label, case) in [
            ("still camera", Case { kind, shape_velocity: [2.5, 0.0], pan: [0.0, 0.0] }),
            ("panning camera", Case { kind, shape_velocity: [2.5, 0.4], pan: [-0.75, 0.5] }),
        ] {
            let none = simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, Masks::None, TUNING);
            let current = simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, Masks::CurrentOnly, TUNING);
            let temporal = simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, Masks::Temporal, TUNING);
            println!("GHOST {kind:?} {label}: no masks {}", show(&none));
            println!("GHOST {kind:?} {label}: this frame's presence only {}", show(&current));
            println!("GHOST {kind:?} {label}: temporal masks {}", show(&temporal));
            check(
                temporal.trail < needed(&case) * current.trail && current.trail <= 1.02 * none.trail,
                format!("{kind:?}, {label}: trail error {:.4} with the temporal masks, {:.4} with this frame's presence only, {:.4} without masks", temporal.trail, current.trail, none.trail),
            );
            // On the moving shape the history is kept now where its presence did not change
            // (only clamped): a little more error there, the price of keeping it on shapes
            // that stay (the neon case below).
            check(
                temporal.shape < current.shape + 0.003 && temporal.clear < 1.05 * current.clear + 1e-4 && temporal.change < 1.05 * current.change + 1e-6,
                format!("{kind:?}, {label}: on the shape {:.4} (against {:.4}), clear of its path {:.4} (against {:.4})", temporal.shape, current.shape, temporal.clear, current.clear),
            );
            if case.pan != [0.0, 0.0] {
                let reversed = simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, Masks::TemporalReversed, TUNING);
                println!("GHOST {kind:?} {label}: lookup reversed {}", show(&reversed));
                check(
                    temporal.trail < reversed.trail,
                    format!("{kind:?}, {label}: control, the lookup reversed leaves a larger trail ({:.4} against {:.4})", reversed.trail, temporal.trail),
                );
            }
        }
        let still = Case { kind, shape_velocity: [0.0, 0.0], pan: [0.0, 0.0] };
        let current = simulate(&context, device, queue, (&pipeline, &luma_pipeline), still, Masks::CurrentOnly, TUNING);
        let temporal = simulate(&context, device, queue, (&pipeline, &luma_pipeline), still, Masks::Temporal, TUNING);
        // A still shape is not reactive any more (nothing comes or goes): it keeps its
        // history, which changes its error by little, and nothing around it changes.
        check(
            (temporal.shape - current.shape).abs() < 0.003 && (temporal.change - current.change).abs() <= 0.02 * current.change + 1e-6,
            format!("{kind:?}, still shape: next to no change (on the shape {:.4} / {:.4}, clear change {:.5} / {:.5})", temporal.shape, current.shape, temporal.change, current.change),
        );
    }
    for (label, case) in [
        ("still camera", Case { kind: Kind::Light, shape_velocity: [2.5, 0.0], pan: [0.0, 0.0] }),
        ("panning camera", Case { kind: Kind::Light, shape_velocity: [2.5, 0.4], pan: [-0.75, 0.5] }),
    ] {
        let current = simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, Masks::CurrentOnly, TUNING);
        let temporal = simulate(&context, device, queue, (&pipeline, &luma_pipeline), case, Masks::Temporal, TUNING);
        println!("GHOST Light {label}: FSR's own shading change only {}", show(&current));
        println!("GHOST Light {label}: with the light-change term {}", show(&temporal));
        // With a still camera the lit spot lags too; under a pan FSR keeps less history
        // anyway (it lowers accumulation with velocity), and the spot must not get worse.
        let spot = if case.pan == [0.0, 0.0] { 0.8 } else { 1.0 };
        check(
            temporal.trail < needed(&case) * current.trail && temporal.shape < spot * current.shape,
            format!("moving light, {label}: trail {:.4} and lit spot {:.4} with the light term, {:.4} and {:.4} without", temporal.trail, temporal.shape, current.trail, current.shape),
        );
        check(
            temporal.clear < 1.05 * current.clear + 1e-4 && temporal.change < 1.05 * current.change + 1e-6,
            format!("moving light, {label}: clear of its path {:.4} (against {:.4}), change {:.5} (against {:.5})", temporal.clear, current.clear, temporal.change, current.change),
        );
    }
    let flicker = Case { kind: Kind::Flicker, shape_velocity: [0.0, 0.0], pan: [0.0, 0.0] };
    let current = simulate(&context, device, queue, (&pipeline, &luma_pipeline), flicker, Masks::CurrentOnly, TUNING);
    let temporal = simulate(&context, device, queue, (&pipeline, &luma_pipeline), flicker, Masks::Temporal, TUNING);
    println!("GHOST Flicker: without the light term {}; with it {}", show(&current), show(&temporal));
    check(
        (temporal.clear - current.clear).abs() <= 0.02 * current.clear + 1e-5 && (temporal.change - current.change).abs() <= 0.02 * current.change + 1e-6,
        format!("fire-like flicker: the light term leaves it to FSR (clear {:.4} / {:.4}, change {:.5} / {:.5})", temporal.clear, current.clear, temporal.change, current.change),
    );
    // A neon tube on the surface behind it, still: marked reactive (this frame's presence)
    // FSR drew it from one jittered frame at a time and it flickered; now it keeps its
    // history.
    let neon = Case { kind: Kind::Neon, shape_velocity: [0.0, 0.0], pan: [0.0, 0.0] };
    let none = simulate(&context, device, queue, (&pipeline, &luma_pipeline), neon, Masks::None, TUNING);
    let current = simulate(&context, device, queue, (&pipeline, &luma_pipeline), neon, Masks::CurrentOnly, TUNING);
    let temporal = simulate(&context, device, queue, (&pipeline, &luma_pipeline), neon, Masks::Temporal, TUNING);
    println!("GHOST Neon still: no masks {}, tube change {:.5}", show(&none), none.shape_change);
    println!("GHOST Neon still: this frame's presence only {}, tube change {:.5}", show(&current), current.shape_change);
    println!("GHOST Neon still: temporal masks {}, tube change {:.5}", show(&temporal), temporal.shape_change);
    check(
        temporal.shape_change < 0.2 * current.shape_change && temporal.shape_change < 2.5 * none.shape_change + 1e-4 && temporal.shape < current.shape,
        format!(
            "still neon tube: frame-to-frame change on it {:.5} with the masks, {:.5} with this frame's presence as the reactive mask, {:.5} without masks; error on it {:.4} (against {:.4})",
            temporal.shape_change, current.shape_change, none.shape_change, temporal.shape, current.shape
        ),
    );
    assert!(failures.is_empty(), "{} ghosting check(s) failed", failures.len());
}
