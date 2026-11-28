//! Apple MetalFX in place of FSR 3.1's own reconstruction (macOS): the Upscaler setting's
//! MetalFX (`UpscalerChoice::MetalFx`, Temporal), or `NVF_METALFX=temporal|spatial|off`, a test
//! aid that overrides the setting while it is set.
//!
//! It rides on FSR's preparation (`FsrUpscaling` with `metalfx` set) and the common upscaler
//! preparation (`upscaler/prep.rs`):
//! the render size of the chosen FSR mode, the jitter, mip bias, history cuts, the corrected
//! reverse-Z depth and Bevy's motion vectors. Only the dispatch differs: MetalFX's scaler is
//! encoded into a command buffer of its own (a wgpu encoder's raw `MTLCommandBuffer`, as the
//! FidelityFX API path does with Vulkan), spliced in between the frame's commands
//! (`RenderContext::add_command_buffer`). wgpu-hal's Metal textures are hazard-tracked, so Metal
//! orders the scaler's reads and writes against wgpu's own passes [observed: clean under Metal API
//! validation, `MTL_DEBUG_LAYER=1`].
//!
//! - **Temporal** (`MTLFXTemporalScaler`): colour (HDR, Rgba16Float), depth (Depth32Float,
//!   reversed), motion (Rg16Float, current → previous, scaled to render pixels like FSR's),
//!   jitter in render pixels, reset on the preparation's history cuts [documented: Apple's
//!   MTLFXTemporalScaler reference; the jitter's sign observed: FSR's value as it is matches
//!   native at a still pose, the opposite doubles edges].
//! - **Spatial** (`MTLFXSpatialScaler`): colour only, HDR processing; the jitter is turned off
//!   for it (`upscaler/prep.rs`), as it has no history to undo it with [design].
//!
//! The colour and motion are drawn into the top-left render rectangle of display-sized
//! targets. The colour is copied into a render-sized texture first; the motion (Bevy's prepass
//! texture has no `COPY_SRC`) is given as it is, its content size set by
//! `inputContentWidth/Height` [observed: accepted by Metal API validation].

#![cfg(target_os = "macos")]

use std::sync::Mutex;

use bevy::math::UVec2;
use bevy::prelude::*;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLDevice, MTLPixelFormat, MTLTexture};
use objc2_metal_fx::{
    MTLFXSpatialScaler, MTLFXSpatialScalerBase, MTLFXSpatialScalerColorProcessingMode, MTLFXSpatialScalerDescriptor,
    MTLFXTemporalScaler, MTLFXTemporalScalerBase, MTLFXTemporalScalerDescriptor,
};

/// Which MetalFX scaler `NVF_METALFX` asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Temporal,
    Spatial,
}

/// What `NVF_METALFX` asks for, read once: while it is set it decides over the Upscaler
/// setting for FSR's and MetalFX's views alike (`temporal`, `spatial`, `off`; a test aid).
static FORCED: std::sync::LazyLock<Option<Option<Mode>>> = std::sync::LazyLock::new(|| {
    let value = std::env::var("NVF_METALFX").ok()?;
    let forced = match value.trim().to_ascii_lowercase().as_str() {
        "temporal" | "1" | "on" => Some(Mode::Temporal),
        "spatial" => Some(Mode::Spatial),
        _ => None,
    };
    info!("MetalFX: NVF_METALFX={} decides over the Upscaler setting ({forced:?})", value.trim());
    Some(forced)
});

/// The scaler a view drawn in FSR's pass gets: `NVF_METALFX` when set, else Temporal when the
/// Upscaler setting is MetalFX (`chosen`, the camera's `FsrUpscaling::metalfx`), else none.
pub(crate) fn mode_for(chosen: bool) -> Option<Mode> {
    match *FORCED {
        Some(forced) => forced,
        None => chosen.then_some(Mode::Temporal),
    }
}

/// The sign MetalFX's jitter is given relative to FSR's (`NVF_METALFX_JITTER=-1` flips it).
static JITTER_SIGN: std::sync::LazyLock<f32> =
    std::sync::LazyLock::new(|| if std::env::var("NVF_METALFX_JITTER").is_ok_and(|v| v.trim() == "-1") { -1.0 } else { 1.0 });

/// Whether the spatial scaler is asked for (only by `NVF_METALFX=spatial`): the preparation
/// then leaves the jitter off.
pub(crate) fn spatial() -> bool {
    matches!(*FORCED, Some(Some(Mode::Spatial)))
}

enum Raw {
    Temporal(Retained<ProtocolObject<dyn MTLFXTemporalScaler>>),
    Spatial(Retained<ProtocolObject<dyn MTLFXSpatialScaler>>),
}

// SAFETY: MetalFX scalers are used from one render system at a time, behind the Mutex below.
unsafe impl Send for Raw {}

/// One view's MetalFX scaler, with the render-sized copies of its colour and motion.
pub(crate) struct Scaler {
    raw: Mutex<Raw>,
    pub(crate) color: wgpu::Texture,
    outputs: Mutex<Vec<wgpu::Texture>>,
}

fn raw_device(device: &wgpu::Device) -> Option<Retained<ProtocolObject<dyn MTLDevice>>> {
    // SAFETY: the raw device is only used to create the scaler; it outlives it (the scaler
    // retains it).
    unsafe { device.as_hal::<wgpu::hal::api::Metal>().map(|hal| hal.raw_device().clone()) }
}

fn raw_texture(texture: &wgpu::Texture) -> Option<Retained<ProtocolObject<dyn MTLTexture>>> {
    // SAFETY: retained for the length of one encode; wgpu keeps the texture alive meanwhile.
    unsafe { texture.as_hal::<wgpu::hal::api::Metal>().map(|hal| Retained::from(hal.raw_handle())) }
}

impl Scaler {
    pub(crate) fn new(device: &wgpu::Device, mode: Mode, render: UVec2, output: UVec2) -> Result<Self, String> {
        let mtl = raw_device(device).ok_or("the render device is not Metal")?;
        let raw = unsafe {
            match mode {
                Mode::Temporal => {
                    if !MTLFXTemporalScalerDescriptor::supportsDevice(&mtl) {
                        return Err("MTLFXTemporalScaler does not support this device".into());
                    }
                    let desc = MTLFXTemporalScalerDescriptor::new();
                    desc.setColorTextureFormat(MTLPixelFormat::RGBA16Float);
                    desc.setDepthTextureFormat(MTLPixelFormat::Depth32Float);
                    desc.setMotionTextureFormat(MTLPixelFormat::RG16Float);
                    desc.setOutputTextureFormat(MTLPixelFormat::RGBA16Float);
                    desc.setInputWidth(render.x as usize);
                    desc.setInputHeight(render.y as usize);
                    desc.setOutputWidth(output.x as usize);
                    desc.setOutputHeight(output.y as usize);
                    // FSR's preparation gives no exposure texture either; MetalFX computes one [design].
                    desc.setAutoExposureEnabled(true);
                    Raw::Temporal(desc.newTemporalScalerWithDevice(&mtl).ok_or("newTemporalScalerWithDevice returned nil")?)
                }
                Mode::Spatial => {
                    if !MTLFXSpatialScalerDescriptor::supportsDevice(&mtl) {
                        return Err("MTLFXSpatialScaler does not support this device".into());
                    }
                    let desc = MTLFXSpatialScalerDescriptor::new();
                    desc.setColorTextureFormat(MTLPixelFormat::RGBA16Float);
                    desc.setOutputTextureFormat(MTLPixelFormat::RGBA16Float);
                    desc.setInputWidth(render.x as usize);
                    desc.setInputHeight(render.y as usize);
                    desc.setOutputWidth(output.x as usize);
                    desc.setOutputHeight(output.y as usize);
                    desc.setColorProcessingMode(MTLFXSpatialScalerColorProcessingMode::HDR);
                    Raw::Spatial(desc.newSpatialScalerWithDevice(&mtl).ok_or("newSpatialScalerWithDevice returned nil")?)
                }
            }
        };
        let usages = unsafe {
            match &raw {
                Raw::Temporal(s) => (s.colorTextureUsage(), s.outputTextureUsage()),
                Raw::Spatial(s) => (s.colorTextureUsage(), s.outputTextureUsage()),
            }
        };
        info!("MetalFX {mode:?} scaler: {}x{} -> {}x{} (colour usage {:?}, output usage {:?})", render.x, render.y, output.x, output.y, usages.0, usages.1);
        let texture = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: render.x, height: render.y, depth_or_array_layers: 1 },
                mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        };
        Ok(Self {
            raw: Mutex::new(raw),
            color: texture("metalfx_color", wgpu::TextureFormat::Rgba16Float),
            outputs: Mutex::new(Vec::new()),
        })
    }

    /// Whether `output` is given to MetalFX for the first time: wgpu clears a texture it saw
    /// no write to before its first read, which would wipe MetalFX's (see `fsr.rs`
    /// `Upscaler::first_output`). The caller clears it itself first.
    pub(crate) fn first_output(&self, output: &wgpu::Texture) -> bool {
        let mut outputs = self.outputs.lock().unwrap();
        if outputs.contains(output) {
            return false;
        }
        if outputs.len() == 2 {
            outputs.remove(0);
        }
        outputs.push(output.clone());
        true
    }

    /// The scaler's command buffer: reads [`Self::color`], `motion` and `depth`, writes
    /// `output`. `jitter` and `motion_scale` are FSR's (render pixels).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode(
        &self, device: &wgpu::Device, depth: &wgpu::Texture, motion: &wgpu::Texture, output: &wgpu::Texture,
        jitter: [f32; 2], motion_scale: [f32; 2], render: UVec2, reset: bool,
    ) -> Result<wgpu::CommandBuffer, String> {
        let color = raw_texture(&self.color).ok_or("colour is not a Metal texture")?;
        let out = raw_texture(output).ok_or("output is not a Metal texture")?;
        let raw = self.raw.lock().unwrap();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("metalfx") });
        let encoded = match &*raw {
            Raw::Temporal(scaler) => {
                let depth = raw_texture(depth).ok_or("depth is not a Metal texture")?;
                let motion = raw_texture(motion).ok_or("motion is not a Metal texture")?;
                // SAFETY: the encoder holds no wgpu commands; MetalFX encodes onto its open
                // command buffer and neither commits nor ends it.
                unsafe {
                    scaler.setColorTexture(Some(&color));
                    scaler.setDepthTexture(Some(&depth));
                    scaler.setMotionTexture(Some(&motion));
                    scaler.setOutputTexture(Some(&out));
                    scaler.setInputContentWidth(render.x as usize);
                    scaler.setInputContentHeight(render.y as usize);
                    scaler.setJitterOffsetX(jitter[0] * *JITTER_SIGN);
                    scaler.setJitterOffsetY(jitter[1] * *JITTER_SIGN);
                    scaler.setMotionVectorScaleX(motion_scale[0]);
                    scaler.setMotionVectorScaleY(motion_scale[1]);
                    scaler.setDepthReversed(true);
                    scaler.setReset(reset);
                    encoder.as_hal_mut::<wgpu::hal::api::Metal, _, _>(|hal| {
                        let buffer = hal?.raw_command_buffer()?;
                        scaler.encodeToCommandBuffer(buffer);
                        Some(())
                    })
                }
            }
            Raw::Spatial(scaler) => unsafe {
                scaler.setColorTexture(Some(&color));
                scaler.setOutputTexture(Some(&out));
                scaler.setInputContentWidth(render.x as usize);
                scaler.setInputContentHeight(render.y as usize);
                encoder.as_hal_mut::<wgpu::hal::api::Metal, _, _>(|hal| {
                    let buffer = hal?.raw_command_buffer()?;
                    scaler.encodeToCommandBuffer(buffer);
                    Some(())
                })
            },
        };
        encoded.ok_or_else(|| "wgpu gave no Metal command buffer".to_string())?;
        Ok(encoder.finish())
    }
}
