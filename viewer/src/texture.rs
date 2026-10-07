//! DDS → GPU textures. Block-compressed data is uploaded as is (BC1/BC2/BC3 with its
//! mip chain); the three uncompressed layouts are converted to RGBA8.

use crate::data::GameData;
use bevy::asset::RenderAssetUsages;
use bevy::image::{
    CompressedImageFormatSupport, CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler,
    ImageSamplerDescriptor,
};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use fnv_dds::{to_rgba8, Dds, Format};
use std::collections::HashMap;

/// Loaded textures by (VFS path, sRGB); `None` remembers a failure.
#[derive(Resource, Default)]
pub struct TextureCache {
    map: HashMap<(String, bool), Option<Handle<Image>>>,
    pub failures: Vec<String>,
}

pub fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::linear()
    })
}

/// Build a GPU image from DDS bytes; `None` for cube maps, unsupported formats, or BC
/// textures the device cannot sample.
pub fn dds_image(bytes: &[u8], srgb: bool, bc_supported: bool) -> Result<Image, String> {
    let dds = Dds::parse(bytes).map_err(|e| e.to_string())?;
    if dds.header.is_cubemap() {
        return Err("cube map (not used by the viewer)".into());
    }
    let (w, h) = (dds.header.width, dds.header.height);
    let levels = &dds.levels[0];
    let (format, data) = match dds.format {
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 => {
            if !bc_supported {
                return Err("GPU lacks BC texture support".into());
            }
            if w % 4 != 0 || h % 4 != 0 {
                return Err(format!("{w}x{h} block-compressed texture is not a multiple of 4"));
            }
            let f = match (dds.format, srgb) {
                (Format::Dxt1, true) => TextureFormat::Bc1RgbaUnormSrgb,
                (Format::Dxt1, false) => TextureFormat::Bc1RgbaUnorm,
                (Format::Dxt3, true) => TextureFormat::Bc2RgbaUnormSrgb,
                (Format::Dxt3, false) => TextureFormat::Bc2RgbaUnorm,
                (_, true) => TextureFormat::Bc3RgbaUnormSrgb,
                (_, false) => TextureFormat::Bc3RgbaUnorm,
            };
            (f, levels.concat())
        }
        other => {
            let mut out = Vec::new();
            for l in levels {
                out.extend(to_rgba8(other, l).ok_or("unsupported layout")?);
            }
            (if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm }, out)
        }
    };
    let mut img = Image::new(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = levels.len() as u32;
    img.sampler = sampler();
    Ok(img)
}

impl TextureCache {
    /// Load (once) the texture at a NIF texture path.
    pub fn get(
        &mut self,
        raw_path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        let path = fnv_vfs::texture_path(raw_path);
        if let Some(h) = self.map.get(&(path.clone(), srgb)) {
            return h.clone();
        }
        let bc = formats.is_none_or(|f| f.0.contains(CompressedImageFormats::BC));
        let result = match data.read(&path) {
            None => Err("not in any archive".to_string()),
            Some(bytes) => dds_image(&bytes, srgb, bc),
        };
        let handle = match result {
            Ok(img) => Some(images.add(img)),
            Err(e) => {
                self.failures.push(format!("{path}: {e}"));
                None
            }
        };
        self.map.insert((path, srgb), handle.clone());
        handle
    }
}
