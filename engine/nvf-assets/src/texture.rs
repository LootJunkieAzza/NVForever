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
use fnv_dds::{decode_blocks, to_rgba8, Dds, Format};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};

/// Loaded textures by (VFS path, sRGB); `None` remembers a failure.
#[derive(Resource, Default)]
pub struct TextureCache {
    map: HashMap<(String, bool), Option<Handle<Image>>>,
    /// Copies sampled with clamped edges ([`Self::get_clamped`]), by the same key.
    clamped: HashMap<(String, bool), Option<Handle<Image>>>,
    pub failures: Vec<String>,
}

/// Appended to a height map's path for its key in [`TextureCache`]: the depth image made
/// from it is not the texture itself ([`TextureCache::get_depth`]). No VFS path holds it.
const DEPTH_KEY: &str = "|depth";

/// Anisotropic filtering of the game's textures made from now on (8 unless the
/// renderer's graphics settings say otherwise: nvforever `graphics.rs`).
static ANISOTROPY: AtomicU16 = AtomicU16::new(8);

/// Set the anisotropic filtering (1, 2, 4, 8 or 16) of textures made from now on.
pub fn set_anisotropy(clamp: u16) {
    ANISOTROPY.store(clamp.clamp(1, 16), Ordering::Relaxed);
}

pub fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: ANISOTROPY.load(Ordering::Relaxed),
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

/// The depth of a height-map texel for Bevy's parallax mapping: Bevy's `depth_map` is
/// depth below the surface (white deepest), the game's `_p` height maps are height above
/// it (brighter = higher) [observed: docs/spec-notes/nif.md §6 "Parallax"]. The red
/// channel is the height [hypothesis: the maps are grey; `parallax_survey` reports any
/// whose channels differ].
pub(crate) fn height_to_depth(rgba: &[u8]) -> impl Iterator<Item = u8> + '_ {
    rgba.chunks_exact(4).map(|p| 255 - p[0])
}

/// Build a single-channel depth image (R8, with the file's mip chain) for Bevy's
/// parallax mapping from the DDS bytes of a height map ([`height_to_depth`]). Decoded on
/// the CPU whatever the format, so the inversion is exact; the vanilla game ships seven
/// such maps (docs/spec-notes/nif.md §6 "Parallax").
pub fn depth_image(bytes: &[u8]) -> Result<Image, String> {
    let dds = Dds::parse(bytes).map_err(|e| e.to_string())?;
    if dds.header.is_cubemap() {
        return Err("cube map used as a height map".into());
    }
    let (w, h) = (dds.header.width, dds.header.height);
    let levels = &dds.levels[0];
    let mut out = Vec::new();
    for (i, l) in levels.iter().enumerate() {
        let (lw, lh) = dds.header.level_dims(i as u32);
        let rgba = decode_blocks(dds.format, lw, lh, l).or_else(|| to_rgba8(dds.format, l)).ok_or("unsupported layout")?;
        out.extend(height_to_depth(&rgba));
    }
    let mut img = Image::new(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        out,
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = levels.len() as u32;
    img.sampler = sampler();
    Ok(img)
}

/// The PC's own form of a texture a model names in a console's folder: the same path
/// with `\pc\` for `\xbox\`, when that file exists. [observed: the Pip-Boy models
/// (`pipboy3000\pipboyarm.nif` and the rest) name `textures\pipboy3000\xbox\english\
/// PipBoyArm01.dds`, whose buttons are labelled with the console's LT and RT, and
/// `pipboy3000\pc\english\pipboyarm01.dds` lies beside it, as do `ps3` and `ps3o`
/// forms; the owner's screenshots of the PC game show no such labels. hypothesis: the
/// PC game reads the `pc` folder's file; docs/spec-notes/pipboy.md §1]
pub fn pc_texture(path: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    let i = lower.find("\\xbox\\")?;
    let pc = format!("{}\\pc\\{}", &path[..i], &path[i + "\\xbox\\".len()..]);
    exists(&pc).then_some(pc)
}

impl TextureCache {
    /// Drop the textures nothing else holds (no live material uses them), after their
    /// grace time (`crate::release`); the number dropped. Keys: (path, sRGB, clamped).
    pub fn release(&mut self, idle: &mut crate::release::Unused<(String, bool, bool)>, now: f64) -> usize {
        use crate::release::{held_beyond, KEEP_UNUSED_MAX, KEEP_UNUSED_SECS};
        let entries: Vec<_> = [(false, &self.map), (true, &self.clamped)]
            .into_iter()
            .flat_map(|(clamp, map)| map.iter().filter_map(move |((p, srgb), h)| h.as_ref().map(|h| ((p.clone(), *srgb, clamp), held_beyond(h, 1)))))
            .collect();
        let drop = idle.sweep(now, entries, KEEP_UNUSED_SECS, KEEP_UNUSED_MAX);
        for (p, srgb, clamp) in &drop {
            (if *clamp { &mut self.clamped } else { &mut self.map }).remove(&(p.clone(), *srgb));
        }
        drop.len()
    }

    /// Textures loaded and held now (failures not counted).
    pub fn len(&self) -> usize {
        self.map.values().chain(self.clamped.values()).filter(|h| h.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Load (once) the texture at a NIF texture path (the PC's own form of it, if any:
    /// [`pc_texture`]).
    pub fn get(
        &mut self,
        raw_path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        self.load(raw_path, srgb, false, data, images, formats)
    }

    /// Like [`Self::get`], a copy of the texture whose sampler clamps at its edges
    /// instead of repeating (the grass atlas, nvforever `grass.rs`).
    pub fn get_clamped(
        &mut self,
        raw_path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        self.load(raw_path, srgb, true, data, images, formats)
    }

    /// Load (once) the height map at a NIF texture path as the depth image Bevy's parallax
    /// mapping reads ([`depth_image`]). Kept in the same table as the colour textures,
    /// under the path with [`DEPTH_KEY`] appended, so [`Self::release`] drops it as any
    /// other.
    pub fn get_depth(&mut self, raw_path: &str, data: &GameData, images: &mut Assets<Image>) -> Option<Handle<Image>> {
        let path = fnv_vfs::texture_path(raw_path);
        let path = pc_texture(&path, |p| data.vfs.exists(p)).unwrap_or(path);
        let key = (format!("{path}{DEPTH_KEY}"), false);
        if let Some(h) = self.map.get(&key) {
            return h.clone();
        }
        let result = match data.read(&path) {
            None => Err("not in any archive".to_string()),
            Some(bytes) => depth_image(&bytes),
        };
        let handle = match result {
            Ok(img) => Some(images.add(img)),
            Err(e) => {
                self.failures.push(format!("{path} (height map): {e}"));
                None
            }
        };
        self.map.insert(key, handle.clone());
        handle
    }

    fn load(
        &mut self,
        raw_path: &str,
        srgb: bool,
        clamp: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        let path = fnv_vfs::texture_path(raw_path);
        let path = pc_texture(&path, |p| data.vfs.exists(p)).unwrap_or(path);
        if let Some(h) = (if clamp { &self.clamped } else { &self.map }).get(&(path.clone(), srgb)) {
            return h.clone();
        }
        let bc = formats.is_none_or(|f| f.0.contains(CompressedImageFormats::BC));
        let result = match data.read(&path) {
            None => Err("not in any archive".to_string()),
            Some(bytes) => dds_image(&bytes, srgb, bc),
        };
        let handle = match result {
            Ok(mut img) => {
                if clamp {
                    if let ImageSampler::Descriptor(d) = &mut img.sampler {
                        d.address_mode_u = ImageAddressMode::ClampToEdge;
                        d.address_mode_v = ImageAddressMode::ClampToEdge;
                    }
                }
                Some(images.add(img))
            }
            Err(e) => {
                self.failures.push(format!("{path}: {e}"));
                None
            }
        };
        (if clamp { &mut self.clamped } else { &mut self.map }).insert((path, srgb), handle.clone());
        handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_becomes_depth() {
        // White (highest) is no depth, black (lowest) the deepest; only red is read.
        let rgba = [255, 255, 255, 255, 0, 0, 0, 255, 200, 10, 10, 0];
        assert_eq!(height_to_depth(&rgba).collect::<Vec<_>>(), vec![0, 255, 55]);
    }

    #[test]
    fn depth_image_from_uncompressed_height_map() {
        // A 2 × 1 A8R8G8B8 DDS (BGRA in the file): heights 255 and 64, one mip level.
        let mut dds = vec![0u8; 128];
        dds[..4].copy_from_slice(b"DDS ");
        let put = |d: &mut Vec<u8>, at: usize, v: u32| d[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut dds, 4, 124); // header size
        put(&mut dds, 8, 0x1007); // caps, height, width, pixel format
        put(&mut dds, 12, 1); // height
        put(&mut dds, 16, 2); // width
        put(&mut dds, 76, 32); // pixel format size
        put(&mut dds, 80, 0x41); // RGB + alpha
        put(&mut dds, 88, 32); // bits
        put(&mut dds, 92, 0x00FF_0000);
        put(&mut dds, 96, 0x0000_FF00);
        put(&mut dds, 100, 0x0000_00FF);
        put(&mut dds, 104, 0xFF00_0000);
        put(&mut dds, 108, 0x1000); // texture
        dds.extend_from_slice(&[255, 255, 255, 255, 64, 64, 64, 255]);
        let img = depth_image(&dds).unwrap();
        assert_eq!(img.texture_descriptor.format, TextureFormat::R8Unorm);
        assert_eq!(img.data.as_deref(), Some(&[0u8, 191][..]));
    }
}
