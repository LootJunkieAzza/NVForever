//! FidelityFX optical flow (SDK 1.1.4) for FSR 3.1 frame generation.
//!
//! NVForever addition (2026-11-06): a port of AMD's MIT-licensed host code,
//! `sdk/src/components/opticalflow/ffx_opticalflow.cpp` (`opticalflowCreate`, `dispatch`), to
//! wgpu. It estimates block motion (8x8 pixels) between the previous and the current colour
//! at display resolution, coarse to fine over seven levels, and detects scene changes from
//! luminance histograms. [`crate::frameinterpolation`] consumes both results.

use crate::fg_pass::{self, FgPass, Kind, Res};
use wgpu_ffx_shaders_spv::opticalflow::{choose_shaders, Half, OpticalflowOptionHdrColorInput, Wave64};

/// Pyramid levels of the search (`OpticalFlowMaxPyramidLevels` and the dispatch's
/// `advancedAlgorithmIterations`).
pub const LEVELS: usize = 7;
/// Pixels per motion vector in each direction (`opticalFlowBlockSize`).
pub const BLOCK_SIZE: u32 = 8;
const HISTOGRAM_BINS: u32 = 256;
const HISTOGRAMS_PER_DIM: u32 = 3;
const HISTOGRAM_SHIFTS: u32 = 3;

#[derive(Debug, Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Constants {
    input_luma_resolution: [i32; 2],
    pyramid_level: u32,
    pyramid_level_count: u32,
    frame_index: i32,
    backbuffer_transfer_function: u32,
    min_max_luminance: [f32; 2],
}

#[derive(Debug, Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct SpdConstants {
    mips: u32,
    num_work_groups: u32,
    work_group_offset: [u32; 2],
    num_work_groups_input_pyramid: u32,
    _pad: [u32; 3],
}

// `(binding, GLSL variable)` per pass, from each pass's `.glsl` file.
const PREPARE_LUMA: &[(u32, &str)] = &[(0, "r_input_color"), (1, "rw_optical_flow_input"), (2, "cbOF")];
const LUMINANCE_PYRAMID: &[(u32, &str)] = &[
    (0, "rw_optical_flow_input"),
    (1, "rw_optical_flow_input_level_1"),
    (2, "rw_optical_flow_input_level_2"),
    (3, "rw_optical_flow_input_level_3"),
    (4, "rw_optical_flow_input_level_4"),
    (5, "rw_optical_flow_input_level_5"),
    (6, "rw_optical_flow_input_level_6"),
    (7, "cbOF"),
    (8, "cbOF_SPD"),
];
const SCD_HISTOGRAM: &[(u32, &str)] = &[(0, "r_optical_flow_input"), (1, "rw_optical_flow_scd_histogram"), (2, "cbOF")];
const SCD_DIVERGENCE: &[(u32, &str)] = &[
    (0, "rw_optical_flow_scd_histogram"),
    (1, "rw_optical_flow_scd_previous_histogram"),
    (2, "rw_optical_flow_scd_temp"),
    (3, "rw_optical_flow_scd_output"),
    (4, "cbOF"),
];
const SEARCH: &[(u32, &str)] = &[
    (0, "r_optical_flow_input"),
    (1, "r_optical_flow_previous_input"),
    (2, "rw_optical_flow"),
    (3, "rw_optical_flow_scd_output"),
    (4, "cbOF"),
];
const FILTER: &[(u32, &str)] = &[(0, "r_optical_flow_previous"), (1, "rw_optical_flow"), (2, "cbOF")];
const SCALE: &[(u32, &str)] = &[
    (0, "r_optical_flow_input"),
    (1, "r_optical_flow_previous_input"),
    (2, "r_optical_flow"),
    (3, "rw_optical_flow_next_level"),
    (4, "rw_optical_flow_scd_output"),
    (5, "cbOF"),
];

/// The types `ffx_opticalflow_callbacks_glsl.h` declares.
pub(crate) fn kind(name: &str) -> Kind {
    use wgpu::{TextureFormat as F, TextureSampleType as S};
    match name {
        "r_input_color" => Kind::Sampled(S::Float { filterable: true }),
        "r_optical_flow_input" | "r_optical_flow_previous_input" => Kind::Sampled(S::Uint),
        "r_optical_flow" | "r_optical_flow_previous" => Kind::Sampled(S::Sint),
        n if n.starts_with("rw_optical_flow_input") => Kind::Storage(F::R8Uint),
        "rw_optical_flow" | "rw_optical_flow_next_level" => Kind::Storage(F::Rg16Sint),
        "rw_optical_flow_scd_histogram" | "rw_optical_flow_scd_temp" | "rw_optical_flow_scd_output" => {
            Kind::Storage(F::R32Uint)
        }
        "rw_optical_flow_scd_previous_histogram" => Kind::Storage(F::R32Float),
        "cbOF" | "cbOF_SPD" => Kind::Uniform,
        other => panic!("optical flow: unknown resource {other}"),
    }
}

/// The storage formats the passes need with read-write access.
pub const STORAGE_FORMATS: &[wgpu::TextureFormat] = &[
    wgpu::TextureFormat::R8Uint,
    wgpu::TextureFormat::Rg16Sint,
    wgpu::TextureFormat::R32Uint,
    wgpu::TextureFormat::R32Float,
];

/// Size of the motion-vector texture for a display size (`GetOpticalFlowTextureSize`).
pub fn flow_size(display: [u32; 2]) -> [u32; 2] {
    [display[0].div_ceil(BLOCK_SIZE), display[1].div_ceil(BLOCK_SIZE)]
}

fn level_sizes(display: [u32; 2]) -> [[u32; 2]; LEVELS] {
    let mut sizes = [flow_size(display); LEVELS];
    for level in 1..LEVELS {
        sizes[level] = [sizes[level - 1][0].div_ceil(2), sizes[level - 1][1].div_ceil(2)];
    }
    sizes
}

pub struct OpticalFlowContext {
    device: wgpu::Device,
    prepare_luma: FgPass,
    luminance_pyramid: FgPass,
    scd_histogram: FgPass,
    scd_divergence: FgPass,
    search: FgPass,
    filter: FgPass,
    scale: FgPass,
}

/// Per-resolution optical-flow state. `flow_vector` and `scd_output` are the results
/// [`crate::frameinterpolation`] reads (AMD's shared resources).
pub struct OpticalFlowView {
    resolution: [u32; 2],
    input: [Vec<wgpu::Texture>; 2],
    flow: [Vec<wgpu::Texture>; 2],
    scd_histogram: wgpu::Texture,
    scd_previous_histogram: wgpu::Texture,
    scd_temp: wgpu::Texture,
    /// Rg16Sint motion per 8x8 block, in pixels.
    pub flow_vector: wgpu::Texture,
    /// R32Uint 3x1: scene-change detection.
    pub scd_output: wgpu::Texture,
    constants: wgpu::Buffer,
    spd_constants: wgpu::Buffer,
    frame_index: i32,
    resource_frame_index: u32,
    first_execution: bool,
    bind_groups: fg_pass::BindGroups,
}

pub struct OpticalFlowDispatchInfo<'a> {
    /// The colour to track, at the view's resolution.
    pub color: &'a wgpu::Texture,
    pub reset: bool,
    /// AMD's `FfxBackbufferTransferFunction`: 0 sRGB, 1 PQ, 2 scRGB (linear).
    pub backbuffer_transfer_function: u32,
    pub min_max_luminance: [f32; 2],
}

impl OpticalFlowContext {
    pub fn new(device: &wgpu::Device, hdr_color_input: bool) -> Self {
        let shaders = choose_shaders(
            Half::Off,
            if hdr_color_input {
                OpticalflowOptionHdrColorInput::On
            } else {
                OpticalflowOptionHdrColorInput::Off
            },
            Wave64::Off,
        );
        let pass = |label, spirv, table| FgPass::new(device, label, spirv, table, &kind);
        Self {
            device: device.clone(),
            prepare_luma: pass("FSR3 OF Prepare Luma", shaders.prepare_luma, PREPARE_LUMA),
            luminance_pyramid: pass("FSR3 OF Luminance Pyramid", shaders.compute_luminance_pyramid, LUMINANCE_PYRAMID),
            scd_histogram: pass("FSR3 OF SCD Histogram", shaders.generate_scd_histogram, SCD_HISTOGRAM),
            scd_divergence: pass("FSR3 OF SCD Divergence", shaders.compute_scd_divergence, SCD_DIVERGENCE),
            search: pass("FSR3 OF Search", shaders.compute_optical_flow_advanced_pass_v5, SEARCH),
            filter: pass("FSR3 OF Filter", shaders.filter_optical_flow_pass_v5, FILTER),
            scale: pass("FSR3 OF Scale", shaders.scale_optical_flow_advanced_pass_v5, SCALE),
        }
    }

    pub fn create_view(&self, resolution: [u32; 2]) -> OpticalFlowView {
        assert!(resolution[0] >= 128 && resolution[1] >= 128, "optical flow needs at least 128x128, got {resolution:?}");
        let device = &self.device;
        let no_extra = wgpu::TextureUsages::COPY_DST;
        let tex = |label: &str, size, format| fg_pass::texture(device, label, size, format, 1, no_extra);
        let sizes = level_sizes(resolution);
        let input = std::array::from_fn(|i| {
            (0..LEVELS)
                .map(|level| {
                    tex(
                        &format!("FSR3 OF Input {} Level {level}", i + 1),
                        [resolution[0] >> level, resolution[1] >> level],
                        wgpu::TextureFormat::R8Uint,
                    )
                })
                .collect()
        });
        let flow = std::array::from_fn(|i| {
            (0..LEVELS)
                .map(|level| tex(&format!("FSR3 OF Flow {} Level {level}", i + 1), sizes[level], wgpu::TextureFormat::Rg16Sint))
                .collect()
        });
        let histogram_width = HISTOGRAM_BINS * HISTOGRAMS_PER_DIM * HISTOGRAMS_PER_DIM;
        OpticalFlowView {
            resolution,
            input,
            flow,
            scd_histogram: tex("FSR3 OF SCD Histogram", [histogram_width, 1], wgpu::TextureFormat::R32Uint),
            scd_previous_histogram: tex("FSR3 OF SCD Previous Histogram", [histogram_width, 1], wgpu::TextureFormat::R32Float),
            scd_temp: tex("FSR3 OF SCD Temp", [3, 1], wgpu::TextureFormat::R32Uint),
            flow_vector: tex("FSR3 OF Result", sizes[0], wgpu::TextureFormat::Rg16Sint),
            scd_output: tex("FSR3 OF SCD Output", [3, 1], wgpu::TextureFormat::R32Uint),
            constants: fg_pass::uniform_slots(device, "FSR3 OF Constants", 1 + LEVELS as u64),
            spd_constants: fg_pass::uniform_slots(device, "FSR3 OF SPD Constants", 1),
            frame_index: 0,
            resource_frame_index: 0,
            first_execution: true,
            bind_groups: Default::default(),
        }
    }

    /// Record the optical-flow passes (`dispatch` in `ffx_opticalflow.cpp`).
    pub fn dispatch(&self, view: &mut OpticalFlowView, encoder: &mut wgpu::CommandEncoder, info: &OpticalFlowDispatchInfo) {
        let device = &self.device;
        let [width, height] = view.resolution;
        assert_eq!([info.color.width(), info.color.height()], view.resolution, "optical flow colour size");

        let reset = info.reset || view.first_execution;
        view.first_execution = false;
        view.frame_index = if reset { 0 } else { view.frame_index + 1 };

        if reset {
            let mut clear = |texture: &wgpu::Texture| encoder.clear_texture(texture, &Default::default());
            clear(&view.scd_temp);
            clear(&view.scd_output);
            clear(&view.scd_histogram);
            clear(&view.scd_previous_histogram);
            for texture in view.input.iter().flatten() {
                clear(texture);
            }
        }

        // Slot 0: the shared constants; slot 1 + level: the search at that level.
        let base = Constants {
            input_luma_resolution: [width as i32, height as i32],
            pyramid_level: 0,
            pyramid_level_count: 0,
            frame_index: view.frame_index,
            backbuffer_transfer_function: info.backbuffer_transfer_function,
            min_max_luminance: info.min_max_luminance,
        };
        let levels: Vec<Constants> = (0..LEVELS as u32)
            .map(|level| Constants { pyramid_level: level, pyramid_level_count: LEVELS as u32, ..base })
            .collect();
        let mut slots: Vec<&[u8]> = vec![bytemuck::bytes_of(&base)];
        slots.extend(levels.iter().map(bytemuck::bytes_of));
        fg_pass::write_slots(device, encoder, &view.constants, &slots);

        let (pyramid_groups, pyramid_work_groups, pyramid_mips) = fg_pass::spd_setup(view.resolution, Some(4));
        let spd = SpdConstants {
            mips: pyramid_mips,
            num_work_groups: pyramid_work_groups,
            work_group_offset: [0, 0],
            num_work_groups_input_pyramid: pyramid_work_groups,
            _pad: [0; 3],
        };
        fg_pass::write_slots(device, encoder, &view.spd_constants, &[bytemuck::bytes_of(&spd)]);

        let odd = view.resource_frame_index & 1 == 1;
        let (current, previous) = if odd { (1, 0) } else { (0, 1) };
        let slot = |index: u64| Res::Slot(&view.constants, index * fg_pass::SLOT, std::mem::size_of::<Constants>() as u64);
        let v = fg_pass::whole;
        let kept = &view.bind_groups;

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("FSR3 optical flow"),
            timestamp_writes: None,
        });

        self.prepare_luma.dispatch(
            device,
            &mut pass,
            kept,
            &mut |name| match name {
                "r_input_color" => v(info.color),
                "rw_optical_flow_input" => v(&view.input[current][0]),
                _ => slot(0),
            },
            [width.div_ceil(2).div_ceil(16), height.div_ceil(2).div_ceil(16), 1],
        );
        self.luminance_pyramid.dispatch(
            device,
            &mut pass,
            kept,
            &mut |name| match name {
                "cbOF" => slot(0),
                "cbOF_SPD" => Res::Slot(&view.spd_constants, 0, std::mem::size_of::<SpdConstants>() as u64),
                "rw_optical_flow_input" => v(&view.input[current][0]),
                level => {
                    let level: usize = level.trim_start_matches("rw_optical_flow_input_level_").parse().unwrap();
                    v(&view.input[current][level])
                }
            },
            [pyramid_groups[0], pyramid_groups[1], 1],
        );
        let strata_width = (width / 4) / HISTOGRAMS_PER_DIM;
        self.scd_histogram.dispatch(
            device,
            &mut pass,
            kept,
            &mut |name| match name {
                "r_optical_flow_input" => v(&view.input[current][0]),
                "rw_optical_flow_scd_histogram" => v(&view.scd_histogram),
                _ => slot(0),
            },
            [strata_width.div_ceil(32), 16, HISTOGRAMS_PER_DIM * HISTOGRAMS_PER_DIM],
        );
        self.scd_divergence.dispatch(
            device,
            &mut pass,
            kept,
            &mut |name| match name {
                "rw_optical_flow_scd_histogram" => v(&view.scd_histogram),
                "rw_optical_flow_scd_previous_histogram" => v(&view.scd_previous_histogram),
                "rw_optical_flow_scd_temp" => v(&view.scd_temp),
                "rw_optical_flow_scd_output" => v(&view.scd_output),
                _ => slot(0),
            },
            [HISTOGRAMS_PER_DIM * HISTOGRAMS_PER_DIM, HISTOGRAM_SHIFTS, 1],
        );

        let sizes = level_sizes(view.resolution);
        for level in (0..LEVELS).rev() {
            let odd_level = level & 1 == 1;
            let (a, b) = if odd != odd_level { (1, 0) } else { (0, 1) };
            let input_a = &view.input[current][level];
            let input_b = &view.input[previous][level];
            let flow_a = &view.flow[a][level];
            let flow_b = &view.flow[b][level];
            let luma = [(width >> level).max(1), (height >> level).max(1)];

            self.search.dispatch(
                device,
                &mut pass,
                kept,
                &mut |name| match name {
                    "r_optical_flow_input" => v(input_a),
                    "r_optical_flow_previous_input" => v(input_b),
                    "rw_optical_flow" => v(flow_a),
                    "rw_optical_flow_scd_output" => v(&view.scd_output),
                    _ => slot(1 + level as u64),
                },
                [(luma[0].div_ceil(4) * 16).div_ceil(64), luma[1].div_ceil(16), 1],
            );
            let filtered = if level == 0 { &view.flow_vector } else { flow_b };
            self.filter.dispatch(
                device,
                &mut pass,
                kept,
                &mut |name| match name {
                    "r_optical_flow_previous" => v(flow_a),
                    "rw_optical_flow" => v(filtered),
                    _ => slot(1 + level as u64),
                },
                [sizes[level][0].div_ceil(16), sizes[level][1].div_ceil(4), 1],
            );
            if level > 0 {
                let next = &view.flow[b][level - 1];
                self.scale.dispatch(
                    device,
                    &mut pass,
                    kept,
                    &mut |name| match name {
                        "r_optical_flow_input" => v(input_a),
                        "r_optical_flow_previous_input" => v(input_b),
                        "r_optical_flow" => v(flow_b),
                        "rw_optical_flow_next_level" => v(next),
                        "rw_optical_flow_scd_output" => v(&view.scd_output),
                        _ => slot(1 + level as u64),
                    },
                    [sizes[level - 1][0].div_ceil(4), sizes[level - 1][1].div_ceil(4), 1],
                );
            }
        }
        drop(pass);

        view.resource_frame_index = (view.resource_frame_index + 1) % 16;
    }
}

impl OpticalFlowView {
    pub fn resolution(&self) -> [u32; 2] {
        self.resolution
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fg_pass::tests::source_table;

    const CALLBACKS: &str = include_str!("../../shaders/include/opticalflow/ffx_opticalflow_callbacks_glsl.h");

    fn table(rust: &[(u32, &str)]) -> Vec<(u32, String)> {
        let mut out: Vec<_> = rust.iter().map(|(b, n)| (*b, n.to_string())).collect();
        out.sort();
        out
    }

    #[test]
    fn pass_tables_match_the_glsl_sources() {
        let src = |file: &str| -> &'static str {
            match file {
                "prepare" => include_str!("../../shaders/src/opticalflow/ffx_opticalflow_prepare_luma_pass.glsl"),
                "pyramid" => include_str!("../../shaders/src/opticalflow/ffx_opticalflow_compute_luminance_pyramid_pass.glsl"),
                "histogram" => include_str!("../../shaders/src/opticalflow/ffx_opticalflow_generate_scd_histogram_pass.glsl"),
                "divergence" => include_str!("../../shaders/src/opticalflow/ffx_opticalflow_compute_scd_divergence_pass.glsl"),
                "search" => include_str!("../../shaders/src/opticalflow/ffx_opticalflow_compute_optical_flow_advanced_pass_v5.glsl"),
                "filter" => include_str!("../../shaders/src/opticalflow/ffx_opticalflow_filter_optical_flow_pass_v5.glsl"),
                _ => include_str!("../../shaders/src/opticalflow/ffx_opticalflow_scale_optical_flow_advanced_pass_v5.glsl"),
            }
        };
        for (file, rust) in [
            ("prepare", PREPARE_LUMA),
            ("pyramid", LUMINANCE_PYRAMID),
            ("histogram", SCD_HISTOGRAM),
            ("divergence", SCD_DIVERGENCE),
            ("search", SEARCH),
            ("filter", FILTER),
            ("scale", SCALE),
        ] {
            assert_eq!(table(rust), source_table(src(file), CALLBACKS), "{file}");
            for (_, name) in rust {
                kind(name);
            }
        }
    }

    #[test]
    fn compiled_shaders_bind_only_what_the_tables_list() {
        for hdr in [OpticalflowOptionHdrColorInput::Off, OpticalflowOptionHdrColorInput::On] {
            let s = choose_shaders(Half::Off, hdr, Wave64::Off);
            for (spirv, rust) in [
                (s.prepare_luma, PREPARE_LUMA),
                (s.compute_luminance_pyramid, LUMINANCE_PYRAMID),
                (s.generate_scd_histogram, SCD_HISTOGRAM),
                (s.compute_scd_divergence, SCD_DIVERGENCE),
                (s.compute_optical_flow_advanced_pass_v5, SEARCH),
                (s.filter_optical_flow_pass_v5, FILTER),
                (s.scale_optical_flow_advanced_pass_v5, SCALE),
            ] {
                let words = fg_pass::spirv::words(spirv);
                for binding in fg_pass::spirv::bindings(&words) {
                    assert!(rust.iter().any(|(b, _)| *b == binding), "binding {binding} missing from {rust:?}");
                }
            }
        }
    }

    #[test]
    fn level_sizes_halve_rounding_up() {
        let sizes = level_sizes([2560, 1440]);
        assert_eq!(sizes[0], [320, 180]);
        assert_eq!(sizes[1], [160, 90]);
        assert_eq!(sizes[2], [80, 45]);
        assert_eq!(sizes[3], [40, 23]);
        assert_eq!(sizes[6], [5, 3]);
        assert_eq!(flow_size([1366, 768]), [171, 96]);
    }
}
