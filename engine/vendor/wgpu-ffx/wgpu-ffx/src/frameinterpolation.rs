//! FidelityFX frame interpolation (SDK 1.1.4) for FSR 3.1 frame generation.
//!
//! NVForever addition (2026-11-06): a port of AMD's MIT-licensed host code,
//! `sdk/src/components/frameinterpolation/ffx_frameinterpolation.cpp`
//! (`frameinterpolationCreate`, `ffxFrameInterpolationPrepare`, `ffxFrameInterpolationDispatch`)
//! and the parameters `sdk/src/components/fsr3/ffx_fsr3.cpp` passes it, to wgpu.
//!
//! [`FrameInterpolationContext::prepare`] takes the frame's depth and motion (at render
//! resolution) and dilates them; [`FrameInterpolationContext::dispatch`] then builds the
//! frame halfway between the previous interpolation source and the current one, from the
//! game's motion and [`crate::opticalflow`]'s, into `output`. Left out: the debug view, the
//! async (second queue) mode and the distortion field input (a 1x1 zero field is bound).

use crate::{
    fg_pass::{self, FgPass, Kind, Res},
    opticalflow::{self, OpticalFlowView},
};
use wgpu_ffx_shaders_spv::frameinterpolation::{
    choose_shaders, FrameinterpolationOptionInvertedDepth, FrameinterpolationOptionLowResMotionVectors, Half, Wave64,
};

bitflags::bitflags! {
    /// `FfxFrameInterpolationInitializationFlagBits` (the ones this port implements).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FrameInterpolationFlags: u32 {
        const DEPTH_INVERTED = 1 << 0;
        const DEPTH_INFINITE = 1 << 1;
        const HDR_COLOR_INPUT = 1 << 3;
        const DISPLAY_RESOLUTION_MOTION_VECTORS = 1 << 4;
    }
}

/// `FrameInterpolationConstants` (`ffx_frameinterpolation_private.h`), std140 as `cbFI`.
#[derive(Debug, Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Constants {
    render_size: [i32; 2],
    display_size: [i32; 2],
    display_size_rcp: [f32; 2],
    camera_near: f32,
    camera_far: f32,
    upscaler_target_size: [i32; 2],
    mode: i32,
    reset: i32,
    device_to_view_depth: [f32; 4],
    delta_time: f32,
    hudless_attached_factor: i32,
    distortion_field_size: [i32; 2],
    optical_flow_scale: [f32; 2],
    optical_flow_block_size: i32,
    dispatch_flags: u32,
    max_render_size: [i32; 2],
    optical_flow_half_res_mode: i32,
    num_instances: i32,
    interpolation_rect_base: [i32; 2],
    interpolation_rect_size: [i32; 2],
    debug_bar_color: [f32; 3],
    back_buffer_transfer_function: u32,
    min_max_luminance: [f32; 2],
    tan_half_fov: f32,
    _pad1: f32,
    jitter: [f32; 2],
    motion_vector_scale: [f32; 2],
}

#[derive(Debug, Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct PyramidConstants {
    mips: u32,
    num_work_groups: u32,
    work_group_offset: [u32; 2],
}

const RECONSTRUCT_AND_DILATE: &[(u32, &str)] = &[
    (0, "r_input_motion_vectors"),
    (1, "r_input_depth"),
    (2, "rw_reconstructed_depth_previous_frame"),
    (3, "rw_dilated_motion_vectors"),
    (4, "rw_dilated_depth"),
    (5, "cbFI"),
];
const SETUP: &[(u32, &str)] = &[
    (0, "r_optical_flow_scd"),
    (1, "rw_game_motion_vector_field_x"),
    (2, "rw_game_motion_vector_field_y"),
    (3, "rw_optical_flow_motion_vector_field_x"),
    (4, "rw_optical_flow_motion_vector_field_y"),
    (5, "rw_disocclusion_mask"),
    (6, "rw_counters"),
    (7, "cbFI"),
];
const RECONSTRUCT_PREVIOUS_DEPTH: &[(u32, &str)] = &[
    (0, "r_dilated_motion_vectors"),
    (1, "r_dilated_depth"),
    (2, "r_current_interpolation_source"),
    (3, "r_input_distortion_field"),
    (4, "rw_reconstructed_depth_interpolated_frame"),
    (5, "cbFI"),
];
const GAME_MOTION_VECTOR_FIELD: &[(u32, &str)] = &[
    (0, "r_dilated_motion_vectors"),
    (1, "r_dilated_depth"),
    (2, "r_previous_interpolation_source"),
    (3, "r_current_interpolation_source"),
    (4, "r_input_distortion_field"),
    (5, "rw_game_motion_vector_field_x"),
    (6, "rw_game_motion_vector_field_y"),
    (7, "cbFI"),
];
const PYRAMID_MIPS: [&str; 13] = [
    "rw_inpainting_pyramid0",
    "rw_inpainting_pyramid1",
    "rw_inpainting_pyramid2",
    "rw_inpainting_pyramid3",
    "rw_inpainting_pyramid4",
    "rw_inpainting_pyramid5",
    "rw_inpainting_pyramid6",
    "rw_inpainting_pyramid7",
    "rw_inpainting_pyramid8",
    "rw_inpainting_pyramid9",
    "rw_inpainting_pyramid10",
    "rw_inpainting_pyramid11",
    "rw_inpainting_pyramid12",
];
const OPTICAL_FLOW_VECTOR_FIELD: &[(u32, &str)] = &[
    (0, "r_optical_flow"),
    (1, "r_optical_flow_confidence"),
    (2, "r_dilated_depth"),
    (3, "r_previous_interpolation_source"),
    (4, "r_current_interpolation_source"),
    (6, "rw_optical_flow_motion_vector_field_x"),
    (7, "rw_optical_flow_motion_vector_field_y"),
    (8, "cbFI"),
];
const DISOCCLUSION_MASK: &[(u32, &str)] = &[
    (0, "r_game_motion_vector_field_x"),
    (1, "r_game_motion_vector_field_y"),
    (2, "r_reconstructed_depth_previous_frame"),
    (3, "r_dilated_depth"),
    (4, "r_reconstructed_depth_interpolated_frame"),
    (5, "r_inpainting_pyramid"),
    (6, "r_input_distortion_field"),
    (7, "rw_disocclusion_mask"),
    (8, "cbFI"),
];
const INTERPOLATION: &[(u32, &str)] = &[
    (0, "r_game_motion_vector_field_x"),
    (1, "r_game_motion_vector_field_y"),
    (2, "r_optical_flow_motion_vector_field_x"),
    (3, "r_optical_flow_motion_vector_field_y"),
    (4, "r_previous_interpolation_source"),
    (5, "r_current_interpolation_source"),
    (6, "r_disocclusion_mask"),
    (7, "r_inpainting_pyramid"),
    (8, "r_counters"),
    (9, "rw_output"),
    (10, "cbFI"),
];
const INPAINTING: &[(u32, &str)] = &[
    (0, "r_optical_flow_scd"),
    (1, "r_inpainting_pyramid"),
    (2, "r_present_backbuffer"),
    (3, "r_current_interpolation_source"),
    (7, "rw_output"),
    (8, "cbFI"),
];

/// `(binding, name)` of a pyramid pass: its inputs, the counters, thirteen mips, then the
/// two constant blocks (`ffx_frameinterpolation_compute_*_pyramid_pass.glsl`).
fn pyramid_table(inputs: &[&'static str]) -> Vec<(u32, &'static str)> {
    let mut table: Vec<(u32, &'static str)> = inputs.iter().enumerate().map(|(i, n)| (i as u32, *n)).collect();
    table.push((inputs.len() as u32, "rw_counters"));
    for (i, mip) in PYRAMID_MIPS.iter().enumerate() {
        table.push((inputs.len() as u32 + 1 + i as u32, mip));
    }
    let next = table.len() as u32;
    table.push((next, "cbFI"));
    table.push((next + 1, "cbInpaintingPyramid"));
    table
}

/// The types `ffx_frameinterpolation_callbacks_glsl.h` declares; `rw_output` has the
/// output's format.
fn kind(name: &str, output_format: wgpu::TextureFormat) -> Kind {
    use wgpu::{TextureFormat as F, TextureSampleType as S};
    match name {
        "r_previous_interpolation_source" | "r_current_interpolation_source" | "r_dilated_motion_vectors"
        | "r_disocclusion_mask" | "r_output" | "r_inpainting_pyramid" | "r_present_backbuffer"
        | "r_input_motion_vectors" | "r_input_distortion_field" => Kind::Sampled(S::Float { filterable: true }),
        "r_dilated_depth" | "r_input_depth" => Kind::Sampled(S::Float { filterable: false }),
        "r_reconstructed_depth_previous_frame" | "r_reconstructed_depth_interpolated_frame"
        | "r_game_motion_vector_field_x" | "r_game_motion_vector_field_y"
        | "r_optical_flow_motion_vector_field_x" | "r_optical_flow_motion_vector_field_y"
        | "r_optical_flow_confidence" | "r_optical_flow_scd" => Kind::Sampled(S::Uint),
        "r_optical_flow" => Kind::Sampled(S::Sint),
        "rw_output" => Kind::Storage(output_format),
        "rw_dilated_motion_vectors" => Kind::Storage(F::Rg16Float),
        "rw_dilated_depth" => Kind::Storage(F::R32Float),
        "rw_reconstructed_depth_previous_frame" | "rw_reconstructed_depth_interpolated_frame"
        | "rw_game_motion_vector_field_x" | "rw_game_motion_vector_field_y"
        | "rw_optical_flow_motion_vector_field_x" | "rw_optical_flow_motion_vector_field_y" => Kind::Storage(F::R32Uint),
        "rw_disocclusion_mask" => Kind::Storage(F::Rg8Unorm),
        n if n.starts_with("rw_inpainting_pyramid") => Kind::Storage(F::Rgba16Float),
        "r_counters" => Kind::StorageBuffer { read_only: true },
        "rw_counters" => Kind::StorageBuffer { read_only: false },
        "cbFI" | "cbInpaintingPyramid" => Kind::Uniform,
        "s_LinearClamp" => Kind::LinearSampler,
        other => panic!("frame interpolation: unknown resource {other}"),
    }
}

/// The storage formats the passes need with read-write access (besides the output's).
pub const STORAGE_FORMATS: &[wgpu::TextureFormat] = &[
    wgpu::TextureFormat::Rg16Float,
    wgpu::TextureFormat::R32Float,
    wgpu::TextureFormat::R32Uint,
    wgpu::TextureFormat::Rg8Unorm,
    wgpu::TextureFormat::Rgba16Float,
];

pub struct FrameInterpolationContext {
    device: wgpu::Device,
    flags: FrameInterpolationFlags,
    output_format: wgpu::TextureFormat,
    reconstruct_and_dilate: FgPass,
    setup: FgPass,
    reconstruct_previous_depth: FgPass,
    game_motion_vector_field: FgPass,
    game_vector_field_pyramid: FgPass,
    optical_flow_vector_field: FgPass,
    disocclusion_mask: FgPass,
    interpolation: FgPass,
    inpainting_pyramid: FgPass,
    inpainting: FgPass,
}

/// Per-resolution state: the dilated inputs, the vector fields, the inpainting pyramid and
/// the previous interpolation source.
pub struct FrameInterpolationView {
    max_render_size: [u32; 2],
    display_size: [u32; 2],
    dilated_depth: wgpu::Texture,
    dilated_motion_vectors: wgpu::Texture,
    reconstructed_previous_depth: wgpu::Texture,
    reconstructed_interpolated_depth: wgpu::Texture,
    game_field: [wgpu::Texture; 2],
    optical_flow_field: [wgpu::Texture; 2],
    disocclusion_mask: wgpu::Texture,
    inpainting_pyramid: wgpu::Texture,
    counters: wgpu::Buffer,
    previous_source: wgpu::Texture,
    default_distortion_field: wgpu::Texture,
    null_uint: wgpu::Texture,
    sampler: wgpu::Sampler,
    prepare_constants: wgpu::Buffer,
    constants_buffer: wgpu::Buffer,
    pyramid_constants: wgpu::Buffer,
    constants: Constants,
    dispatch_count: u64,
    previous_frame_id: u64,
    debug_bar: usize,
    bind_groups: fg_pass::BindGroups,
}

/// `FfxFrameInterpolationPrepareDescription`.
pub struct FrameInterpolationPrepareInfo<'a> {
    /// Depth at render resolution (any float or depth format).
    pub depth: &'a wgpu::Texture,
    /// Motion vectors at render resolution (or display, with
    /// [`FrameInterpolationFlags::DISPLAY_RESOLUTION_MOTION_VECTORS`]).
    pub motion_vectors: &'a wgpu::Texture,
    pub render_size: [u32; 2],
    pub jitter_offset: [f32; 2],
    /// As the upscaler's: multiplied into the motion before dividing by its texture size.
    pub motion_vector_scale: [f32; 2],
    pub camera_near: f32,
    pub camera_far: f32,
    pub camera_fov_y: f32,
    pub view_space_to_meters_factor: f32,
}

/// `FfxFrameInterpolationDispatchDescription` with `ffx_fsr3.cpp`'s choices.
pub struct FrameInterpolationDispatchInfo<'a> {
    /// The frame as presented (with the HUD, if drawn).
    pub current_backbuffer: &'a wgpu::Texture,
    /// The same frame without the HUD; interpolation then runs on this, and the HUD's
    /// pixels are taken from `current_backbuffer` (AMD's "HUD-less" mode).
    pub current_backbuffer_hudless: Option<&'a wgpu::Texture>,
    /// Display-sized storage texture of the context's output format.
    pub output: &'a wgpu::Texture,
    pub optical_flow: &'a OpticalFlowView,
    pub render_size: [u32; 2],
    pub camera_near: f32,
    pub camera_far: f32,
    pub camera_fov_y: f32,
    pub view_space_to_meters_factor: f32,
    /// Milliseconds since the previous real frame.
    pub frame_time_delta: f32,
    pub reset: bool,
    /// 0 sRGB, 1 PQ, 2 scRGB (linear).
    pub backbuffer_transfer_function: u32,
    pub min_max_luminance: [f32; 2],
    /// Must grow by one per dispatch; a gap or a step back resets.
    pub frame_id: u64,
    // NVForever (2026-11-10): AMD keeps the interpolation source in a texture of its own, a
    // display-sized copy every frame. A caller that keeps last frame's source itself (two
    // textures used in turn) passes it here and saves the copy.
    /// The previous frame's interpolation source (its `current_backbuffer_hudless`, or
    /// `current_backbuffer` without one), kept by the caller. `None`: the view keeps a copy.
    pub previous_source: Option<&'a wgpu::Texture>,
}

const DEBUG_BAR_COLORS: [[f32; 3]; 7] = [
    [0.0, 1.0, 1.0],
    [1.0, 0.42, 0.0],
    [0.0, 0.16, 1.0],
    [0.74, 1.0, 0.0],
    [0.68, 0.0, 1.0],
    [0.0, 1.0, 0.1],
    [1.0, 1.0, 0.48],
];

impl FrameInterpolationContext {
    /// `output_format` is the backbuffer's (and the interpolation sources'), e.g.
    /// `Rgba16Float`.
    pub fn new(device: &wgpu::Device, flags: FrameInterpolationFlags, output_format: wgpu::TextureFormat) -> Self {
        let shaders = choose_shaders(
            if flags.contains(FrameInterpolationFlags::DEPTH_INVERTED) {
                FrameinterpolationOptionInvertedDepth::On
            } else {
                FrameinterpolationOptionInvertedDepth::Off
            },
            if flags.contains(FrameInterpolationFlags::DISPLAY_RESOLUTION_MOTION_VECTORS) {
                FrameinterpolationOptionLowResMotionVectors::Off
            } else {
                FrameinterpolationOptionLowResMotionVectors::On
            },
            Half::Off,
            Wave64::Off,
        );
        let kinds = move |name: &str| kind(name, output_format);
        let pass = |label, spirv, table: &[(u32, &'static str)]| FgPass::new(device, label, spirv, table, &kinds);
        Self {
            device: device.clone(),
            flags,
            output_format,
            reconstruct_and_dilate: pass("FSR3 FI Reconstruct and Dilate", shaders.reconstruct_and_dilate, RECONSTRUCT_AND_DILATE),
            setup: pass("FSR3 FI Setup", shaders.setup, SETUP),
            reconstruct_previous_depth: pass("FSR3 FI Reconstruct Previous Depth", shaders.reconstruct_previous_depth, RECONSTRUCT_PREVIOUS_DEPTH),
            game_motion_vector_field: pass("FSR3 FI Game Motion Vector Field", shaders.game_motion_vector_field, GAME_MOTION_VECTOR_FIELD),
            game_vector_field_pyramid: pass(
                "FSR3 FI Game Vector Field Inpainting Pyramid",
                shaders.compute_game_vector_field_inpainting_pyramid,
                &pyramid_table(&["r_game_motion_vector_field_x", "r_game_motion_vector_field_y"]),
            ),
            optical_flow_vector_field: pass("FSR3 FI Optical Flow Vector Field", shaders.optical_flow_vector_field, OPTICAL_FLOW_VECTOR_FIELD),
            disocclusion_mask: pass("FSR3 FI Disocclusion Mask", shaders.disocclusion_mask, DISOCCLUSION_MASK),
            interpolation: pass("FSR3 FI Interpolation", shaders.pass, INTERPOLATION),
            inpainting_pyramid: pass("FSR3 FI Inpainting Pyramid", shaders.compute_inpainting_pyramid, &pyramid_table(&["r_output"])),
            inpainting: pass("FSR3 FI Inpainting", shaders.inpainting, INPAINTING),
        }
    }

    pub fn output_format(&self) -> wgpu::TextureFormat {
        self.output_format
    }

    pub fn create_view(&self, queue: &wgpu::Queue, max_render_size: [u32; 2], display_size: [u32; 2]) -> FrameInterpolationView {
        use wgpu::{TextureFormat as F, TextureUsages as U};
        let device = &self.device;
        let tex = |label: &str, size, format| fg_pass::texture(device, label, size, format, 1, U::COPY_DST);
        let half = [display_size[0] / 2, display_size[1] / 2];
        let pyramid_mips = wgpu::Extent3d { width: half[0].max(1), height: half[1].max(1), depth_or_array_layers: 1 }
            .max_mips(wgpu::TextureDimension::D2);
        let one = |label: &str, format, bytes: &[u8]| {
            use wgpu::util::DeviceExt as _;
            device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: U::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::default(),
                bytes,
            )
        };
        let mut constants = Constants {
            max_render_size: [max_render_size[0] as i32, max_render_size[1] as i32],
            display_size: [display_size[0] as i32, display_size[1] as i32],
            display_size_rcp: [1.0 / display_size[0] as f32, 1.0 / display_size[1] as f32],
            interpolation_rect_size: [display_size[0] as i32, display_size[1] as i32],
            ..Default::default()
        };
        constants.distortion_field_size = [1, 1];
        FrameInterpolationView {
            max_render_size,
            display_size,
            dilated_depth: tex("FSR3 FI Dilated Depth", max_render_size, F::R32Float),
            dilated_motion_vectors: tex("FSR3 FI Dilated Motion Vectors", max_render_size, F::Rg16Float),
            reconstructed_previous_depth: tex("FSR3 FI Reconstructed Previous Depth", max_render_size, F::R32Uint),
            reconstructed_interpolated_depth: tex("FSR3 FI Reconstructed Interpolated Depth", max_render_size, F::R32Uint),
            game_field: [
                tex("FSR3 FI Game Motion Vector Field X", max_render_size, F::R32Uint),
                tex("FSR3 FI Game Motion Vector Field Y", max_render_size, F::R32Uint),
            ],
            optical_flow_field: [
                tex("FSR3 FI Optical Flow Motion Vector Field X", max_render_size, F::R32Uint),
                tex("FSR3 FI Optical Flow Motion Vector Field Y", max_render_size, F::R32Uint),
            ],
            disocclusion_mask: tex("FSR3 FI Disocclusion Mask", max_render_size, F::Rg8Unorm),
            inpainting_pyramid: fg_pass::texture(device, "FSR3 FI Inpainting Pyramid", half, F::Rgba16Float, pyramid_mips, U::COPY_DST),
            counters: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("FSR3 FI Counters"),
                size: 8,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            previous_source: device.create_texture(&wgpu::TextureDescriptor {
                label: Some("FSR3 FI Previous Interpolation Source"),
                size: wgpu::Extent3d { width: display_size[0], height: display_size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.output_format,
                usage: U::TEXTURE_BINDING | U::COPY_DST,
                view_formats: &[],
            }),
            default_distortion_field: one("FSR3 FI Default Distortion Field", F::Rg8Unorm, &[0, 0]),
            null_uint: one("FSR3 FI Null Uint", F::R32Uint, &[0; 4]),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("FSR3 FI Linear Clamp"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            }),
            prepare_constants: fg_pass::uniform_slots(device, "FSR3 FI Prepare Constants", 1),
            constants_buffer: fg_pass::uniform_slots(device, "FSR3 FI Constants", 1),
            pyramid_constants: fg_pass::uniform_slots(device, "FSR3 FI Pyramid Constants", 2),
            constants,
            dispatch_count: 0,
            previous_frame_id: 0,
            debug_bar: 0,
            bind_groups: Default::default(),
        }
    }

    fn depth_clear_value(&self) -> u32 {
        // The depth buffers hold float bits (atomic min/max on uint); AMD clears them to the
        // far plane: 0 with inverted depth, 1.0 otherwise.
        if self.flags.contains(FrameInterpolationFlags::DEPTH_INVERTED) { 0 } else { 1.0f32.to_bits() }
    }

    /// Fill an R32Uint texture: `clear_texture` for zero, a buffer copy otherwise.
    fn fill(&self, encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture, value: u32) {
        if value == 0 {
            encoder.clear_texture(texture, &Default::default());
            return;
        }
        use wgpu::util::DeviceExt as _;
        let row = (texture.width() * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let words = vec![value; (row / 4 * texture.height()) as usize];
        let staging = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("FSR3 FI depth clear"),
            contents: bytemuck::cast_slice(&words),
            usage: wgpu::BufferUsages::COPY_SRC,
        });
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None },
            },
            texture.as_image_copy(),
            texture.size(),
        );
    }

    fn device_to_view_depth(&self, near: f32, far: f32, fov_y: f32, meters: f32, render_size: [u32; 2]) -> [f32; 4] {
        // `setupDeviceDepthToViewSpaceDepthParams`, with the view-space-to-metres factor.
        let inverted = self.flags.contains(FrameInterpolationFlags::DEPTH_INVERTED);
        let infinite = self.flags.contains(FrameInterpolationFlags::DEPTH_INFINITE);
        let (mut min, mut max) = (near.min(far), near.max(far));
        if inverted {
            std::mem::swap(&mut min, &mut max);
        }
        let q = max / (min - max);
        let c = [[q, -1.0 - f32::EPSILON], [q, f32::EPSILON]][inverted as usize][infinite as usize];
        let e = [[q * min, -min - f32::EPSILON], [q * min, max]][inverted as usize][infinite as usize];
        let meters = if meters > 0.0 { meters } else { 1.0 };
        let aspect = render_size[0] as f32 / render_size[1] as f32;
        let cot = (0.5 * fov_y).cos() / (0.5 * fov_y).sin();
        [-c, e * meters, aspect / cot, 1.0 / cot]
    }

    /// `ffxFrameInterpolationPrepare`: reconstruct and dilate this frame's depth and motion.
    pub fn prepare(&self, view: &mut FrameInterpolationView, encoder: &mut wgpu::CommandEncoder, info: &FrameInterpolationPrepareInfo) {
        assert!(info.render_size[0] <= view.max_render_size[0] && info.render_size[1] <= view.max_render_size[1]);
        let c = &mut view.constants;
        c.render_size = [info.render_size[0] as i32, info.render_size[1] as i32];
        c.jitter = info.jitter_offset;
        let target = if self.flags.contains(FrameInterpolationFlags::DISPLAY_RESOLUTION_MOTION_VECTORS) {
            c.display_size
        } else {
            c.render_size
        };
        c.motion_vector_scale = [info.motion_vector_scale[0] / target[0] as f32, info.motion_vector_scale[1] / target[1] as f32];
        // AMD's prepare stages the transform its previous dispatch computed; this port computes
        // the current frame's (the same in steady state, right on the first frame).
        c.device_to_view_depth = self.device_to_view_depth(
            info.camera_near,
            info.camera_far,
            info.camera_fov_y,
            info.view_space_to_meters_factor,
            info.render_size,
        );
        let constants = view.constants;
        fg_pass::write_slots(&self.device, encoder, &view.prepare_constants, &[bytemuck::bytes_of(&constants)]);
        self.fill(encoder, &view.reconstructed_previous_depth, self.depth_clear_value());

        let v = fg_pass::whole;
        let size = std::mem::size_of::<Constants>() as u64;
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("FSR3 frame interpolation prepare"),
            timestamp_writes: None,
        });
        self.reconstruct_and_dilate.dispatch(
            &self.device,
            &mut pass,
            &view.bind_groups,
            &mut |name| match name {
                "r_input_motion_vectors" => v(info.motion_vectors),
                "r_input_depth" => v(info.depth),
                "rw_reconstructed_depth_previous_frame" => v(&view.reconstructed_previous_depth),
                "rw_dilated_motion_vectors" => v(&view.dilated_motion_vectors),
                "rw_dilated_depth" => v(&view.dilated_depth),
                _ => Res::Slot(&view.prepare_constants, 0, size),
            },
            [info.render_size[0].div_ceil(8), info.render_size[1].div_ceil(8), 1],
        );
    }

    /// `ffxFrameInterpolationDispatch`: the frame between the previous interpolation source
    /// and this one, into `info.output`. Run [`Self::prepare`] for this frame first.
    pub fn dispatch(&self, view: &mut FrameInterpolationView, encoder: &mut wgpu::CommandEncoder, info: &FrameInterpolationDispatchInfo) {
        let device = &self.device;
        let display = view.display_size;
        assert_eq!([info.output.width(), info.output.height()], display, "frame interpolation output size");
        assert_eq!(info.optical_flow.resolution(), display, "optical flow resolution");

        let reset = view.dispatch_count == 0 || info.reset;
        let disjoint = info.frame_id < view.previous_frame_id || info.frame_id - view.previous_frame_id > 1;
        view.previous_frame_id = info.frame_id;
        view.dispatch_count += 1;

        let c = &mut view.constants;
        c.render_size = [info.render_size[0] as i32, info.render_size[1] as i32];
        c.display_size = [display[0] as i32, display[1] as i32];
        c.display_size_rcp = [1.0 / display[0] as f32, 1.0 / display[1] as f32];
        c.upscaler_target_size = c.display_size;
        c.mode = 0;
        c.reset = (reset || disjoint) as i32;
        c.delta_time = info.frame_time_delta;
        c.hudless_attached_factor = info.current_backbuffer_hudless.is_some() as i32;
        c.optical_flow_scale = [1.0 / display[0] as f32, 1.0 / display[1] as f32];
        c.optical_flow_block_size = opticalflow::BLOCK_SIZE as i32;
        c.dispatch_flags = 0;
        c.camera_near = info.camera_near;
        c.camera_far = info.camera_far;
        c.interpolation_rect_base = [0, 0];
        c.interpolation_rect_size = c.display_size;
        c.debug_bar_color = DEBUG_BAR_COLORS[view.debug_bar];
        view.debug_bar = (view.debug_bar + 1) % DEBUG_BAR_COLORS.len();
        c.back_buffer_transfer_function = info.backbuffer_transfer_function;
        c.min_max_luminance = info.min_max_luminance;
        let aspect = info.render_size[0] as f32 / info.render_size[1] as f32;
        let horizontal = ((info.camera_fov_y / 2.0).tan() * aspect).atan() * 2.0;
        c.tan_half_fov = (horizontal * 0.5).tan();
        c.distortion_field_size = [1, 1];
        c.device_to_view_depth = self.device_to_view_depth(
            info.camera_near,
            info.camera_far,
            info.camera_fov_y,
            info.view_space_to_meters_factor,
            info.render_size,
        );
        let constants = view.constants;
        let execute_preparation = constants.reset == 0;

        let (game_groups, game_work_groups, game_mips) = fg_pass::spd_setup(info.render_size, None);
        let (pyramid_groups, pyramid_work_groups, pyramid_mips) = fg_pass::spd_setup(display, None);
        let game_pyramid = PyramidConstants { mips: game_mips, num_work_groups: game_work_groups, work_group_offset: [0, 0] };
        let pyramid = PyramidConstants { mips: pyramid_mips, num_work_groups: pyramid_work_groups, work_group_offset: [0, 0] };
        fg_pass::write_slots(device, encoder, &view.constants_buffer, &[bytemuck::bytes_of(&constants)]);
        fg_pass::write_slots(device, encoder, &view.pyramid_constants, &[bytemuck::bytes_of(&game_pyramid), bytemuck::bytes_of(&pyramid)]);
        if execute_preparation {
            self.fill(encoder, &view.reconstructed_interpolated_depth, self.depth_clear_value());
        }

        let (hudless, present) = match info.current_backbuffer_hudless {
            Some(hudless) => (hudless, info.current_backbuffer),
            None => (info.current_backbuffer, info.current_backbuffer),
        };
        let v = fg_pass::whole;
        let previous = info.previous_source.unwrap_or(&view.previous_source);
        let size = std::mem::size_of::<Constants>() as u64;
        let pyramid_size = std::mem::size_of::<PyramidConstants>() as u64;
        // Every pass's resources, by GLSL name; `pyramid_slot` picks which pyramid's constants.
        let resolve = |name: &str, pyramid_slot: u64| -> Res<'_> {
            match name {
                "r_previous_interpolation_source" => v(previous),
                "r_current_interpolation_source" => v(hudless),
                "r_present_backbuffer" => v(present),
                "r_dilated_motion_vectors" => v(&view.dilated_motion_vectors),
                "r_dilated_depth" => v(&view.dilated_depth),
                "r_reconstructed_depth_previous_frame" => v(&view.reconstructed_previous_depth),
                "r_reconstructed_depth_interpolated_frame" | "rw_reconstructed_depth_interpolated_frame" => {
                    v(&view.reconstructed_interpolated_depth)
                }
                "r_game_motion_vector_field_x" | "rw_game_motion_vector_field_x" => v(&view.game_field[0]),
                "r_game_motion_vector_field_y" | "rw_game_motion_vector_field_y" => v(&view.game_field[1]),
                "r_optical_flow_motion_vector_field_x" | "rw_optical_flow_motion_vector_field_x" => {
                    v(&view.optical_flow_field[0])
                }
                "r_optical_flow_motion_vector_field_y" | "rw_optical_flow_motion_vector_field_y" => {
                    v(&view.optical_flow_field[1])
                }
                "r_disocclusion_mask" | "rw_disocclusion_mask" => v(&view.disocclusion_mask),
                "r_inpainting_pyramid" => v(&view.inpainting_pyramid),
                "r_optical_flow" => v(&info.optical_flow.flow_vector),
                "r_optical_flow_scd" => v(&info.optical_flow.scd_output),
                "r_optical_flow_confidence" => v(&view.null_uint),
                "r_input_distortion_field" => v(&view.default_distortion_field),
                "r_output" | "rw_output" => v(info.output),
                "r_counters" | "rw_counters" => Res::Buffer(&view.counters),
                "cbFI" => Res::Slot(&view.constants_buffer, 0, size),
                "cbInpaintingPyramid" => Res::Slot(&view.pyramid_constants, pyramid_slot * fg_pass::SLOT, pyramid_size),
                "s_LinearClamp" => Res::Sampler(&view.sampler),
                mip if mip.starts_with("rw_inpainting_pyramid") => {
                    let level: u32 = mip.trim_start_matches("rw_inpainting_pyramid").parse().unwrap();
                    Res::Mip(&view.inpainting_pyramid, level)
                }
                other => panic!("frame interpolation: nothing bound for {other}"),
            }
        };

        let render_groups = [info.render_size[0].div_ceil(8), info.render_size[1].div_ceil(8), 1];
        let display_groups = [display[0].div_ceil(8), display[1].div_ceil(8), 1];
        let block = opticalflow::BLOCK_SIZE as f32;
        let flow_groups = [
            ((display[0] as f32 / block + 7.0) as u32) / 8,
            ((display[1] as f32 / block + 7.0) as u32) / 8,
            1,
        ];

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("FSR3 frame interpolation"),
            timestamp_writes: None,
        });
        let at = |slot: u64| move |name: &str| resolve(name, slot);
        self.setup.dispatch(device, &mut pass, &view.bind_groups, &mut at(0), render_groups);
        if execute_preparation {
            self.reconstruct_previous_depth.dispatch(device, &mut pass, &view.bind_groups, &mut at(0), render_groups);
            self.game_motion_vector_field.dispatch(device, &mut pass, &view.bind_groups, &mut at(0), render_groups);
            self.game_vector_field_pyramid.dispatch(device, &mut pass, &view.bind_groups, &mut at(0), [game_groups[0], game_groups[1], 1]);
            self.optical_flow_vector_field.dispatch(device, &mut pass, &view.bind_groups, &mut at(0), flow_groups);
            self.disocclusion_mask.dispatch(device, &mut pass, &view.bind_groups, &mut at(0), render_groups);
        }
        self.interpolation.dispatch(device, &mut pass, &view.bind_groups, &mut at(0), display_groups);
        self.inpainting_pyramid.dispatch(device, &mut pass, &view.bind_groups, &mut at(1), [pyramid_groups[0], pyramid_groups[1], 1]);
        self.inpainting.dispatch(device, &mut pass, &view.bind_groups, &mut at(0), display_groups);
        drop(pass);

        // Store the current interpolation source for the next frame, unless the caller keeps it.
        if info.previous_source.is_none() {
            encoder.copy_texture_to_texture(hudless.as_image_copy(), view.previous_source.as_image_copy(), view.previous_source.size());
        }
    }
}

impl FrameInterpolationView {
    pub fn display_size(&self) -> [u32; 2] {
        self.display_size
    }

    pub fn max_render_size(&self) -> [u32; 2] {
        self.max_render_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fg_pass::tests::source_table;

    const CALLBACKS: &str = include_str!("../../shaders/include/frameinterpolation/ffx_frameinterpolation_callbacks_glsl.h");

    fn sorted(rust: &[(u32, &str)]) -> Vec<(u32, String)> {
        let mut out: Vec<_> = rust.iter().map(|(b, n)| (*b, n.to_string())).collect();
        out.sort();
        out
    }

    fn tables() -> Vec<(&'static str, Vec<(u32, &'static str)>)> {
        vec![
            (include_str!("../../shaders/src/frameinterpolation/ffx_frameinterpolation_reconstruct_and_dilate_pass.glsl"), RECONSTRUCT_AND_DILATE.to_vec()),
            (include_str!("../../shaders/src/frameinterpolation/ffx_frameinterpolation_setup_pass.glsl"), SETUP.to_vec()),
            (include_str!("../../shaders/src/frameinterpolation/ffx_frameinterpolation_reconstruct_previous_depth_pass.glsl"), RECONSTRUCT_PREVIOUS_DEPTH.to_vec()),
            (include_str!("../../shaders/src/frameinterpolation/ffx_frameinterpolation_game_motion_vector_field_pass.glsl"), GAME_MOTION_VECTOR_FIELD.to_vec()),
            (
                include_str!("../../shaders/src/frameinterpolation/ffx_frameinterpolation_compute_game_vector_field_inpainting_pyramid_pass.glsl"),
                pyramid_table(&["r_game_motion_vector_field_x", "r_game_motion_vector_field_y"]),
            ),
            (include_str!("../../shaders/src/frameinterpolation/ffx_frameinterpolation_optical_flow_vector_field_pass.glsl"), OPTICAL_FLOW_VECTOR_FIELD.to_vec()),
            (include_str!("../../shaders/src/frameinterpolation/ffx_frameinterpolation_disocclusion_mask_pass.glsl"), DISOCCLUSION_MASK.to_vec()),
            (include_str!("../../shaders/src/frameinterpolation/ffx_frameinterpolation_pass.glsl"), INTERPOLATION.to_vec()),
            (include_str!("../../shaders/src/frameinterpolation/ffx_frameinterpolation_compute_inpainting_pyramid_pass.glsl"), pyramid_table(&["r_output"])),
            (include_str!("../../shaders/src/frameinterpolation/ffx_frameinterpolation_inpainting_pass.glsl"), INPAINTING.to_vec()),
        ]
    }

    #[test]
    fn pass_tables_match_the_glsl_sources() {
        for (glsl, rust) in tables() {
            assert_eq!(sorted(&rust), source_table(glsl, CALLBACKS), "{rust:?}");
            for (_, name) in &rust {
                kind(name, wgpu::TextureFormat::Rgba16Float);
            }
        }
    }

    #[test]
    fn compiled_shaders_bind_only_what_the_tables_list() {
        for inverted in [FrameinterpolationOptionInvertedDepth::Off, FrameinterpolationOptionInvertedDepth::On] {
            for low in [FrameinterpolationOptionLowResMotionVectors::Off, FrameinterpolationOptionLowResMotionVectors::On] {
                let s = choose_shaders(inverted, low, Half::Off, Wave64::Off);
                let spirv = [
                    s.reconstruct_and_dilate,
                    s.setup,
                    s.reconstruct_previous_depth,
                    s.game_motion_vector_field,
                    s.compute_game_vector_field_inpainting_pyramid,
                    s.optical_flow_vector_field,
                    s.disocclusion_mask,
                    s.pass,
                    s.compute_inpainting_pyramid,
                    s.inpainting,
                ];
                for (spirv, (_, rust)) in spirv.into_iter().zip(tables()) {
                    let mut words = fg_pass::spirv::words(spirv);
                    fg_pass::spirv::remap_binding(&mut words, 1000, fg_pass::SAMPLER_BINDING);
                    for binding in fg_pass::spirv::bindings(&words) {
                        assert!(
                            binding == fg_pass::SAMPLER_BINDING || rust.iter().any(|(b, _)| *b == binding),
                            "binding {binding} missing from {rust:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn constants_match_the_std140_block() {
        // cbFI's std140 offsets: the C struct is laid out to match them.
        assert_eq!(std::mem::size_of::<Constants>(), 176);
        assert_eq!(std::mem::offset_of!(Constants, device_to_view_depth), 48);
        assert_eq!(std::mem::offset_of!(Constants, debug_bar_color), 128);
        assert_eq!(std::mem::offset_of!(Constants, back_buffer_transfer_function), 140);
        assert_eq!(std::mem::offset_of!(Constants, jitter), 160);
        assert_eq!(std::mem::size_of::<PyramidConstants>(), 16);
    }
}
