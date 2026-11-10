//! Numerical checks of the exact source FSR backend used by the engine.
use half::f16;
use wgpu::*;
use wgpu_ffx::{FsrContext, FsrContextFlags, FsrContextInfo, FsrDispatchFlags, FsrDispatchInfo};

fn image(device: &Device, format: TextureFormat, side: u32, usage: TextureUsages) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("FSR synthetic input/output"),
        size: Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn color_bytes(side: u32, f: impl Fn(u32, u32) -> [f32; 4]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((side * side * 8) as usize);
    for y in 0..side {
        for x in 0..side {
            for c in f(x, y) {
                bytes.extend(f16::from_f32(c).to_le_bytes());
            }
        }
    }
    bytes
}

fn write(queue: &Queue, texture: &Texture, data: &[u8], bytes: u32) {
    queue.write_texture(
        texture.as_image_copy(),
        data,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texture.width() * bytes),
            rows_per_image: Some(texture.height()),
        },
        texture.size(),
    );
}

pub fn run(device: &Device, queue: &Queue) {
    let render = 64;
    let upscale = 96; // Quality, 1.5x per dimension.
    let output_side = 104; // A guard outside the requested output rectangle.
    let flags = FsrContextFlags::HIGH_DYNAMIC_RANGE
        | FsrContextFlags::DEPTH_INVERTED
        | FsrContextFlags::DEPTH_INFINITE
        | FsrContextFlags::AUTO_EXPOSURE;
    let context = FsrContext::new(FsrContextInfo {
        device: device.clone(),
        flags,
    });
    let sampled = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    let storage = TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING;
    let color = image(device, TextureFormat::Rgba16Float, upscale, sampled);
    let motion = image(device, TextureFormat::Rg16Float, upscale, sampled);
    write(
        queue,
        &motion,
        &vec![0; (upscale * upscale * 4) as usize],
        4,
    );
    let depth = image(
        device,
        TextureFormat::Depth32Float,
        render,
        TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
    );
    let depth_view = depth.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("synthetic reverse infinite depth"),
            color_attachments: &[],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(0.5),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    queue.submit([encoder.finish()]);
    let dilated_depth = image(device, TextureFormat::R32Float, render, storage);
    let dilated_motion = image(device, TextureFormat::Rg16Float, render, storage);
    let output = image(
        device,
        TextureFormat::Rgba16Float,
        output_side,
        storage | TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
    );
    let reconstructed = device.create_buffer(&BufferDescriptor {
        label: Some("synthetic reconstructed depth"),
        size: (render * render * 4) as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let phase_count = wgpu_ffx::get_jitter_phase_count(render as i32, upscale as i32);
    for patterned in [false, true] {
        let expected = |x: u32| {
            if patterned && x >= render / 2 {
                [1.0f32, 0.125, 0.25, 1.0]
            } else {
                [0.25, 0.5, 1.0, 1.0]
            }
        };
        let data = color_bytes(upscale, |x, y| {
            if x < render && y < render {
                expected(x)
            } else {
                [16.0, 0.0, 16.0, 1.0]
            }
        });
        write(queue, &color, &data, 8);
        for sharpen in [false, true] {
            let mut view = context.create_view(queue, [render, render], [upscale, upscale]);
            write(
                queue,
                &output,
                &color_bytes(output_side, |_, _| [7.0, 7.0, 7.0, 1.0]),
                8,
            );
            for frame in 0..8 {
                let mut encoder = device.create_command_encoder(&Default::default());
                context
                    .dispatch(
                        &mut view,
                        &mut encoder,
                        &FsrDispatchInfo {
                            color: color.clone(),
                            depth: depth.clone(),
                            motion_vectors: motion.clone(),
                            exposure: None,
                            reactive_mask: None,
                            transparency_and_composition: None,
                            dilated_depth: dilated_depth.clone(),
                            dilated_motion_vectors: dilated_motion.clone(),
                            reconstructed_previous_depth: reconstructed.clone(),
                            output: output.clone(),
                            jitter_offset: wgpu_ffx::get_jitter_offset(frame, phase_count),
                            motion_vector_scale: [-(render as f32); 2],
                            render_size: [render; 2],
                            upscale_size: [upscale; 2],
                            enable_sharpening: sharpen,
                            sharpness: 0.2,
                            frame_time_delta: 16.6667,
                            pre_exposure: 1.0,
                            reset_history: frame == 0,
                            camera_near: f32::MAX,
                            camera_far: 0.1,
                            camera_fov_y: std::f32::consts::FRAC_PI_3,
                            view_space_to_meters_factor: 1.0 / 70.0,
                            flags: FsrDispatchFlags::empty(),
                        },
                    )
                    .unwrap();
                queue.submit([encoder.finish()]);
                let raw = crate::gpu::read(device, queue, &output, 8);
                let pixels: Vec<[f32; 4]> = raw
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|p| {
                        std::array::from_fn(|c| {
                            f16::from_le_bytes([p[c * 2], p[c * 2 + 1]]).to_f32()
                        })
                    })
                    .collect();
                let mut maximum_error = 0.0f32;
                for y in 0..output_side {
                    for x in 0..output_side {
                        let pixel = pixels[(y * output_side + x) as usize];
                        if x >= upscale || y >= upscale {
                            assert_eq!(
                                pixel,
                                [7.0, 7.0, 7.0, 1.0],
                                "output guard overwritten at {x},{y}; RCAS {sharpen}"
                            );
                            continue;
                        }
                        assert!(
                            pixel.iter().all(|c| c.is_finite()),
                            "nonfinite FSR output {x},{y}: {pixel:?}"
                        );
                        assert!(
                            pixel[0] + pixel[1] + pixel[2] > 0.01,
                            "black FSR output at {x},{y}"
                        );
                        assert!(
                            pixel[..3].iter().all(|c| *c < 2.0),
                            "unrendered source sentinel leaked: {pixel:?}"
                        );
                        if !patterned
                            || (x.abs_diff(upscale / 2) > 8
                                && x > 3
                                && y > 3
                                && x < upscale - 4
                                && y < upscale - 4)
                        {
                            let target = expected(x * render / upscale);
                            for c in 0..3 {
                                maximum_error = maximum_error.max((pixel[c] - target[c]).abs());
                            }
                        }
                    }
                }
                assert!(maximum_error<0.04,"FSR colour error {maximum_error}, pattern={patterned} RCAS={sharpen} frame={frame}");
                if frame == 7 {
                    println!("PASS GPU full FSR 3.1.4 HDR/reverse-infinite/auto-exposure Quality, pattern={patterned}, RCAS={sharpen}, 8 frames, max colour error {maximum_error:.6}, output guard intact");
                }
            }
        }
    }
    moving_pattern(&context, device, queue);
}

// An analytically shaded plane translated in screen space. Every input sample
// is evaluated at its actual jittered raster location. The reference is evaluated
// independently at the unjittered display pixel, and motion follows the known
// current-minus-previous screen displacement used by Bevy's prepass.
fn moving_pattern(context: &FsrContext, device: &Device, queue: &Queue) {
    fn pattern(x: f32, y: f32) -> [f32; 4] {
        let tau = std::f32::consts::TAU;
        [
            0.45 + 0.25 * (tau * x / 12.0).sin() + 0.15 * (tau * (x + y) / 23.0).sin(),
            0.45 + 0.30 * (tau * y / 14.0).cos(),
            0.35 + 0.25 * (tau * (x - 2.0 * y) / 19.0).sin(),
            1.0,
        ]
    }
    let render = 128;
    let sampled = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    let storage = TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING;
    for upscale in [128, 192] {
        let color = image(device, TextureFormat::Rgba16Float, upscale, sampled);
        let motion = image(device, TextureFormat::Rg16Float, upscale, sampled);
        let depth = image(
            device,
            TextureFormat::Depth32Float,
            render,
            TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
        );
        let depth_view = depth.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("moving plane at 500 game units"),
                color_attachments: &[],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(0.01),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        queue.submit([encoder.finish()]);
        let dilated_depth = image(device, TextureFormat::R32Float, render, storage);
        let dilated_motion = image(
            device,
            TextureFormat::Rg16Float,
            render,
            storage | TextureUsages::COPY_SRC,
        );
        let output = image(
            device,
            TextureFormat::Rgba16Float,
            upscale,
            storage | TextureUsages::COPY_SRC,
        );
        let reconstructed = device.create_buffer(&BufferDescriptor {
            label: Some("moving plane reconstructed depth"),
            size: (render * render * 4) as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let phases = wgpu_ffx::get_jitter_phase_count(render as i32, upscale as i32);
        for sharpen in [false, true] {
            let mut results = Vec::new();
            for (label, motion_multiplier, jitter_multiplier) in [
                ("correct", 1.0, 1.0),
                ("reversed motion", -1.0, 1.0),
                ("missing motion", 0.0, 1.0),
                ("reversed jitter", 1.0, -1.0),
            ] {
                let mut view = context.create_view(queue, [render; 2], [upscale; 2]);
                let mut shift = [0.0f32; 2];
                let mut errors = [0.0f64; 3];
                let mut counts = [0usize; 3];
                for frame in 0..32 {
                    let velocity = if (8..24).contains(&frame) {
                        [0.75, -0.5]
                    } else {
                        [0.0, 0.0]
                    };
                    for axis in 0..2 {
                        shift[axis] += velocity[axis];
                    }
                    let jitter = wgpu_ffx::get_jitter_offset(frame, phases);
                    let data = color_bytes(upscale, |x, y| {
                        if x < render && y < render {
                            pattern(
                                x as f32 + 0.5 - jitter[0] - shift[0],
                                y as f32 + 0.5 - jitter[1] - shift[1],
                            )
                        } else {
                            [16.0, 0.0, 16.0, 1.0]
                        }
                    });
                    write(queue, &color, &data, 8);
                    let mv: Vec<u8> = velocity
                        .into_iter()
                        .flat_map(|v| {
                            f16::from_f32(v / render as f32 * motion_multiplier).to_le_bytes()
                        })
                        .collect();
                    write(queue, &motion, &mv.repeat((upscale * upscale) as usize), 4);
                    let mut encoder = device.create_command_encoder(&Default::default());
                    context
                        .dispatch(
                            &mut view,
                            &mut encoder,
                            &FsrDispatchInfo {
                                color: color.clone(),
                                depth: depth.clone(),
                                motion_vectors: motion.clone(),
                                exposure: None,
                                reactive_mask: None,
                                transparency_and_composition: None,
                                dilated_depth: dilated_depth.clone(),
                                dilated_motion_vectors: dilated_motion.clone(),
                                reconstructed_previous_depth: reconstructed.clone(),
                                output: output.clone(),
                                jitter_offset: jitter.map(|v| v * jitter_multiplier),
                                motion_vector_scale: [-(render as f32); 2],
                                render_size: [render; 2],
                                upscale_size: [upscale; 2],
                                enable_sharpening: sharpen,
                                sharpness: 0.2,
                                frame_time_delta: 16.6667,
                                pre_exposure: 1.0,
                                reset_history: frame == 0,
                                camera_near: f32::MAX,
                                camera_far: 5.0,
                                camera_fov_y: std::f32::consts::FRAC_PI_3,
                                view_space_to_meters_factor: 1.0 / 70.0,
                                flags: FsrDispatchFlags::empty(),
                            },
                        )
                        .unwrap();
                    queue.submit([encoder.finish()]);
                    if label == "correct" && frame == 16 {
                        let raw = crate::gpu::read(device, queue, &dilated_motion, 4);
                        let at = ((render / 2 * render + render / 2) * 4) as usize;
                        for axis in 0..2 {
                            let actual =
                                f16::from_le_bytes([raw[at + axis * 2], raw[at + axis * 2 + 1]])
                                    .to_f32();
                            assert!(
                                (actual + velocity[axis] / render as f32).abs() < 0.00001,
                                "FSR dilated motion axis {axis}: {actual}"
                            );
                        }
                    }
                    let raw = crate::gpu::read(device, queue, &output, 8);
                    let phase = if frame < 8 {
                        0
                    } else if frame < 24 {
                        1
                    } else {
                        2
                    };
                    if frame < 4 {
                        continue;
                    }
                    for y in 12..upscale - 12 {
                        for x in 12..upscale - 12 {
                            let expected = pattern(
                                (x as f32 + 0.5) * render as f32 / upscale as f32 - shift[0],
                                (y as f32 + 0.5) * render as f32 / upscale as f32 - shift[1],
                            );
                            let at = ((y * upscale + x) * 8) as usize;
                            for channel in 0..3 {
                                let actual = f16::from_le_bytes([
                                    raw[at + channel * 2],
                                    raw[at + channel * 2 + 1],
                                ])
                                .to_f32();
                                assert!(actual.is_finite(), "nonfinite translated output");
                                errors[phase] += (actual - expected[channel]).abs() as f64;
                                counts[phase] += 1;
                            }
                        }
                    }
                }
                let mean = std::array::from_fn::<_, 3, _>(|i| errors[i] / counts[i] as f64);
                println!("MOTION {render}->{upscale} RCAS={sharpen} {label}: static/moving/stopped MAE {mean:?}");
                results.push(mean);
            }
            // Regression for the port's accumulation input binding: the low-resolution
            // shader must sample the sign-converted, dilated vectors. Binding raw
            // Bevy vectors made the reversed-motion control better than valid motion.
            assert!(
                results[0][1] < results[1][1] * 0.8,
                "accumulation must consume converted/dilated motion, not raw Bevy vectors"
            );
            assert!(
                results[0][1] < results[2][1] * 0.8,
                "missing-motion control did not distinguish valid history"
            );
            assert!(
                results[0][0] < results[3][0] * 0.8,
                "reversed-jitter control did not distinguish valid jitter"
            );
            assert!(
                results[0].iter().all(|e| *e < 0.02),
                "translated-plane reconstruction error {:?}",
                results[0]
            );
            println!("PASS GPU translated plane {render}->{upscale} RCAS={sharpen}: bounded reconstruction error, reversed/missing-motion and reversed-jitter controls distinguished, movement start/stop");
        }
    }
}
