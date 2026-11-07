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
    let reactive_source = std::fs::read_to_string(root.join("fsr_reactive.wgsl")).unwrap();
    let reactive_pipeline = pipeline(
        &device,
        &reactive_source,
        "fragment",
        Some(TextureFormat::R8Unorm),
    );
    let opaque_view = opaque.create_view(&Default::default());
    let scene_view = scene.create_view(&Default::default());
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &reactive_pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&opaque_view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(&scene_view),
            },
        ],
    });
    let reactive = texture(&device, TextureFormat::R8Unorm, [4, 1]);
    let mut encoder = device.create_command_encoder(&Default::default());
    draw(&mut encoder, &reactive, &reactive_pipeline, Some(&group));
    queue.submit([encoder.finish()]);
    let values = read(&device, &queue, &reactive, 1);
    assert_eq!(values[0], 0);
    assert!((values[1] as i32 - 127).abs() <= 1);
    assert!((values[2] as i32 - 230).abs() <= 1);
    assert!((values[3] as i32 - 230).abs() <= 1);
    println!("PASS GPU reactive mask unchanged/moderate/bright/dark pixels {values:?}");

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
    crate::fg::run(&device, &queue);
}
