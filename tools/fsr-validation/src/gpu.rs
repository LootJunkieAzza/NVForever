use std::path::Path;
use wgpu::{util::DeviceExt, *};

pub(crate) fn texture(device: &Device, format: TextureFormat, size: [u32; 2]) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("synthetic"),
        size: Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC
            | TextureUsages::COPY_DST
            | TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

pub(crate) fn pipeline(
    device: &Device,
    source: &str,
    entry: &str,
    color: Option<TextureFormat>,
) -> RenderPipeline {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some(entry),
        source: ShaderSource::Wgsl(source.into()),
    });
    let targets: Vec<_> = color
        .into_iter()
        .map(|format| {
            Some(ColorTargetState {
                format,
                blend: None,
                write_mask: ColorWrites::ALL,
            })
        })
        .collect();
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(entry),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            targets: &targets,
        }),
        primitive: Default::default(),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
        depth_stencil: color.is_none().then_some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
    })
}

pub(crate) fn draw(
    encoder: &mut CommandEncoder,
    target: &Texture,
    pipeline: &RenderPipeline,
    group: Option<&BindGroup>,
) {
    let view = target.create_view(&Default::default());
    let colors = [Some(RenderPassColorAttachment {
        view: &view,
        resolve_target: None,
        depth_slice: None,
        ops: Operations {
            load: LoadOp::Clear(Color::BLACK),
            store: StoreOp::Store,
        },
    })];
    let depth = RenderPassDepthStencilAttachment {
        view: &view,
        depth_ops: Some(Operations {
            load: LoadOp::Clear(0.0),
            store: StoreOp::Store,
        }),
        stencil_ops: None,
    };
    let is_depth = target.format() == TextureFormat::Depth32Float;
    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("synthetic"),
        color_attachments: if is_depth { &[] } else { &colors },
        depth_stencil_attachment: is_depth.then_some(depth),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    if let Some(group) = group {
        pass.set_bind_group(0, group, &[]);
    }
    pass.draw(0..3, 0..1);
}

pub(crate) fn read(
    device: &Device,
    queue: &Queue,
    texture: &Texture,
    bytes_per_pixel: usize,
) -> Vec<u8> {
    let stride = (texture.width() as u64 * bytes_per_pixel as u64).div_ceil(256) * 256;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("readback"),
        size: stride * texture.height() as u64,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    let mut image = texture.as_image_copy();
    if texture.format() == TextureFormat::Depth32Float {
        image.aspect = TextureAspect::DepthOnly;
    }
    encoder.copy_texture_to_buffer(
        image,
        TexelCopyBufferInfo {
            buffer: &buffer,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride as u32),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(MapMode::Read, move |r| tx.send(r).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(20)),
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let mut result = Vec::new();
    for y in 0..texture.height() as usize {
        result.extend_from_slice(
            &mapped[y * stride as usize
                ..y * stride as usize + texture.width() as usize * bytes_per_pixel],
        );
    }
    result
}

/// The Vulkan device every check runs on. `WGPU_VALIDATION=1` turns on the Khronos
/// validation layer (Vulkan SDK); any wgpu validation error stops the run.
pub fn device() -> (Device, Queue) {
    let instance = Instance::new(InstanceDescriptor {
        backends: Backends::VULKAN,
        flags: InstanceFlags::from_build_config().with_env(),
        ..InstanceDescriptor::new_without_display_handle()
    });
    let adapter =
        pollster::block_on(instance.request_adapter(&RequestAdapterOptions::default())).unwrap();
    println!(
        "GPU {} {:?}",
        adapter.get_info().name,
        adapter.get_info().backend
    );
    let required = Features::PASSTHROUGH_SHADERS
        | Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
        | Features::FLOAT32_FILTERABLE
        | Features::TEXTURE_FORMAT_16BIT_NORM
        | Features::CLEAR_TEXTURE;
    assert!(
        adapter.features().contains(required),
        "Adapter lacks required FSR features: {:?}",
        required - adapter.features()
    );
    for format in [
        TextureFormat::R8Unorm,
        TextureFormat::R16Float,
        TextureFormat::Rg16Float,
        TextureFormat::Rgba16Float,
        TextureFormat::Rgba32Float,
    ] {
        let properties = adapter.get_texture_format_features(format);
        println!("FORMAT {format:?}: {properties:?}");
        assert!(properties
            .allowed_usages
            .contains(TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING));
        assert!(properties
            .flags
            .contains(TextureFormatFeatureFlags::STORAGE_READ_WRITE));
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
        required_features: required,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    device.on_uncaptured_error(std::sync::Arc::new(|error| panic!("wgpu validation error: {error}")));
    println!("PASS FSR device capability gate {:?}", device.features());
    (device, queue)
}

pub fn run(root: &Path) {
    let (device, queue) = device();
    crate::clusters::run(
        &device,
        &queue,
        &root.join("../../vendor/bevy_pbr/src/render/clustered_forward.wesl"),
    );
    crate::motion::run(
        &device,
        &queue,
        &root.join("../../vendor/bevy_pbr/src/render/pbr_prepass_functions.wesl"),
    );
    let depth_source = std::fs::read_to_string(root.join("fsr_depth.wgsl")).unwrap();
    let vertex = depth_source
        .split("@fragment")
        .next()
        .unwrap()
        .split("@vertex")
        .nth(1)
        .unwrap();
    let seed = format!("@vertex {vertex}\n@fragment fn seed(@builtin(position) p: vec4<f32>) -> @builtin(frag_depth) f32 {{ let z = array<f32,4>(0.0, 0.5, 63.0/64.0 + 0.25/64.0, 1.0); return z[u32(p.x) % 4u]; }}");
    let seed_pipeline = pipeline(&device, &seed, "seed", None);
    let prepare = pipeline(&device, &depth_source, "prepare_depth", None);
    let expand = pipeline(&device, &depth_source, "expand_depth", None);
    let depth = texture(&device, TextureFormat::Depth32Float, [4, 1]);
    let prepared = texture(&device, TextureFormat::Depth32Float, [4, 1]);
    let expanded = texture(&device, TextureFormat::Depth32Float, [8, 2]);
    let dimensions: Vec<u8> = [4u32, 1, 8, 2]
        .iter()
        .flat_map(|n| n.to_le_bytes())
        .collect();
    let uniform = device.create_buffer_init(&util::BufferInitDescriptor {
        label: None,
        contents: &dimensions,
        usage: BufferUsages::UNIFORM,
    });
    let view = depth.create_view(&Default::default());
    let prepare_group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &prepare.get_bind_group_layout(0),
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::TextureView(&view),
        }],
    });
    let expand_group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &expand.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&view),
            },
            BindGroupEntry {
                binding: 1,
                resource: uniform.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    draw(&mut encoder, &depth, &seed_pipeline, None);
    draw(&mut encoder, &prepared, &prepare, Some(&prepare_group));
    draw(&mut encoder, &expanded, &expand, Some(&expand_group));
    queue.submit([encoder.finish()]);
    let values: Vec<_> = read(&device, &queue, &prepared, 4)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    for (a, b) in values.iter().zip([0.0f32, 0.5, 0.25, 1.0]) {
        assert!((a - b).abs() < 1e-6, "prepare {values:?}");
    }
    let values: Vec<_> = read(&device, &queue, &expanded, 4)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    for (i, a) in values.iter().enumerate() {
        let b = [0.0f32, 0.5, 63.0 / 64.0 + 0.25 / 64.0, 1.0][(i % 8) / 2];
        assert!((a - b).abs() < 1e-6, "expand {values:?}");
    }
    println!("PASS GPU depth unsqueeze (world/first-person/sky) and 2x expansion (16 pixels)");

    let opaque = texture(&device, TextureFormat::Rgba8Unorm, [4, 1]);
    let scene = texture(&device, TextureFormat::Rgba8Unorm, [4, 1]);
    let opaque_data = [
        128, 128, 128, 255, 128, 128, 128, 255, 0, 0, 0, 255, 255, 255, 255, 255,
    ];
    let scene_data = [
        128, 128, 128, 255, 255, 128, 128, 255, 255, 255, 255, 255, 0, 0, 0, 255,
    ];
    for (texture, data) in [(&opaque, opaque_data), (&scene, scene_data)] {
        queue.write_texture(
            texture.as_image_copy(),
            &data,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16),
                rows_per_image: Some(1),
            },
            texture.size(),
        );
    }
    // Last frame's presence: 200 at pixel 0. Pixel 1's motion (a quarter of the 4-pixel
    // width) says its history comes from pixel 0; the others have none.
    let previous = texture(&device, TextureFormat::R8Unorm, [4, 1]);
    queue.write_texture(
        previous.as_image_copy(),
        &[200u8, 0, 0, 0],
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        previous.size(),
    );
    let motion = texture(&device, TextureFormat::Rg16Float, [4, 1]);
    let motion_data: Vec<u8> = [0.0f32, 0.0, 0.25, 0.0, 0.0, 0.0, 0.0, 0.0]
        .iter()
        .flat_map(|v| half::f16::from_f32(*v).to_le_bytes())
        .collect();
    queue.write_texture(
        motion.as_image_copy(),
        &motion_data,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16),
            rows_per_image: Some(1),
        },
        motion.size(),
    );
    // FSR's last output luminance: none (cleared), so the light term stays out of this check.
    let previous_luma = texture(&device, TextureFormat::R16Float, [8, 2]);
    let reactive_source = std::fs::read_to_string(root.join("fsr_reactive.wgsl")).unwrap();
    let pipelines = crate::ghosting::mask_pipelines(&device, &reactive_source);
    let view = |t: &Texture| t.create_view(&Default::default());
    let (opaque_view, scene_view, motion_view, previous_view, previous_luma_view) =
        (view(&opaque), view(&scene), view(&motion), view(&previous), view(&previous_luma));
    let linear = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..Default::default()
    });
    let no_jitter = device.create_buffer_init(&util::BufferInitDescriptor {
        label: None,
        contents: &[0u8; 16],
        usage: BufferUsages::UNIFORM,
    });
    // The first pass's outputs (presence, luminance) and the second's (reactive, composition).
    let presence_out = texture(&device, TextureFormat::R8Unorm, [4, 1]);
    let luma_out = texture(&device, TextureFormat::R16Float, [4, 1]);
    let reactive_out = texture(&device, TextureFormat::R8Unorm, [4, 1]);
    let composition_out = texture(&device, TextureFormat::R8Unorm, [4, 1]);
    let (presence_view, luma_view) = (view(&presence_out), view(&luma_out));
    let (reactive_view, composition_view) = (view(&reactive_out), view(&composition_out));
    let texture_entry = |binding, view| BindGroupEntry {
        binding,
        resource: BindingResource::TextureView(view),
    };
    let first = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &pipelines.presence.get_bind_group_layout(0),
        entries: &[texture_entry(0, &opaque_view), texture_entry(1, &scene_view)],
    });
    let second = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &pipelines.masks.get_bind_group_layout(0),
        entries: &[
            texture_entry(2, &motion_view),
            texture_entry(3, &previous_view),
            texture_entry(4, &previous_luma_view),
            BindGroupEntry {
                binding: 5,
                resource: BindingResource::Sampler(&linear),
            },
            BindGroupEntry {
                binding: 6,
                resource: no_jitter.as_entire_binding(),
            },
            texture_entry(7, &presence_view),
            texture_entry(8, &luma_view),
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    let attachment = |view| {
        Some(RenderPassColorAttachment {
            view,
            resolve_target: None,
            depth_slice: None,
            ops: Operations {
                load: LoadOp::Clear(Color::BLACK),
                store: StoreOp::Store,
            },
        })
    };
    for (targets, pipeline, group) in [
        ([attachment(&presence_view), attachment(&luma_view)], &pipelines.presence, &first),
        ([attachment(&reactive_view), attachment(&composition_view)], &pipelines.masks, &second),
    ] {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("fsr masks"),
            color_attachments: &targets,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit([encoder.finish()]);
    let [reactive, composition, presence] =
        [&reactive_out, &composition_out, &presence_out].map(|t| read(&device, &queue, t, 1));
    let near = |a: u8, b: i32| (a as i32 - b).abs() <= 1;
    assert_eq!(presence[0], 0);
    assert!(near(presence[1], 127) && near(presence[2], 230) && near(presence[3], 230), "presence {presence:?}");
    // Composition: pixel 0 keeps its own last presence; pixel 1 takes pixel 0's (its
    // history's source), above its own; pixels 2 and 3 had none. Reactive, what changed:
    // pixel 0's 200 went (127 is the most within a texel now), pixel 1's history still finds
    // 229 within a texel (nothing went) and its 127 had 200 near its source (nothing came),
    // pixels 2 and 3 appeared.
    assert!(near(composition[0], 200) && near(composition[1], 200) && near(composition[2], 229) && near(composition[3], 229), "composition {composition:?}");
    assert!(near(reactive[0], 73) && reactive[1] <= 1 && near(reactive[2], 229) && near(reactive[3], 229), "reactive {reactive:?}");
    println!("PASS GPU masks: presence unchanged/moderate/bright/dark {presence:?}; composition {composition:?} (last frame's presence where the history comes from) and reactive {reactive:?} (what came or went)");

    // The unrendered portion of the full-sized attachment must never leak into fallback.
    let source = texture(&device, TextureFormat::Rgba8Unorm, [8, 2]);
    let mut data = vec![0u8; 8 * 2 * 4];
    for (i, pixel) in data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        pixel.copy_from_slice(if i < 4 {
            &[255, 0, 0, 255]
        } else {
            &[0, 255, 255, 255]
        });
    }
    queue.write_texture(
        source.as_image_copy(),
        &data,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(32),
            rows_per_image: Some(2),
        },
        source.size(),
    );
    let source_view = source.create_view(&Default::default());
    let fallback_source = std::fs::read_to_string(root.join("fsr_fallback.wgsl")).unwrap();
    let fallback_pipeline = pipeline(
        &device,
        &fallback_source,
        "fragment",
        Some(TextureFormat::Rgba8Unorm),
    );
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..Default::default()
    });
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &fallback_pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&source_view),
            },
            BindGroupEntry {
                binding: 1,
                resource: uniform.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let fallback = texture(&device, TextureFormat::Rgba8Unorm, [8, 2]);
    let mut encoder = device.create_command_encoder(&Default::default());
    draw(&mut encoder, &fallback, &fallback_pipeline, Some(&group));
    queue.submit([encoder.finish()]);
    let values = read(&device, &queue, &fallback, 4);
    for pixel in values.as_chunks::<4>().0.iter() {
        assert_eq!(pixel, &[255, 0, 0, 255]);
    }
    println!(
        "PASS GPU spatial fallback clamps to rendered region (16 pixels, no stale-area bleed)"
    );
    crate::fsr::run(&device, &queue);
    crate::ghosting::run(&device, &queue, &reactive_source, &fallback_source);
    crate::fg::run(&device, &queue);
}
