//! FaceGen in the game (workstream 5 part C, docs/spec-notes/actors.md §4): an NPC's
//! FGGS/FGGA coefficients morph every part that has an `.egm` beside its model (head,
//! eyes, mouth, beards, hats, …), and its FGTS coefficients add the `.egt` colour offset
//! to the face texture. The combination rules are [hypothesis] (fnv_actor::facegen).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use fnv_actor::facegen::{add_offset, Egm, Egt};
use fnv_actor::records::FaceGen;
use fnv_dds::{to_rgba8, Dds, Format};
use nvf_assets::actor::{ActorCtx, PreparedPart};
use nvf_assets::GameData;
use std::collections::HashMap;
use std::sync::Arc;

/// Decoded top mip of a texture: width, height, RGBA8.
type Rgba = (usize, usize, Vec<u8>);

/// Parsed FaceGen files and decoded base textures by archive path; `None` remembers
/// that a file is absent or unreadable.
#[derive(Default)]
pub struct FaceGenCache {
    egm: HashMap<String, Option<Arc<Egm>>>,
    egt: HashMap<String, Option<Arc<Egt>>>,
    base: HashMap<String, Option<Arc<Rgba>>>,
    pub morphed_shapes: usize,
    pub faces_textured: usize,
    /// Body skin textures tinted with the race's body `.egt`.
    pub skins_textured: usize,
    /// EGMs whose vertex count does not cover the shape they sit beside.
    pub mismatched: usize,
}

/// `meshes\a\b.nif` → `meshes\a\b.<ext>`.
pub fn sibling(model: &str, ext: &str) -> String {
    match model.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.{ext}"),
        None => format!("{model}.{ext}"),
    }
}

impl FaceGenCache {
    fn egm(&mut self, data: &GameData, path: &str) -> Option<Arc<Egm>> {
        self.egm
            .entry(path.to_string())
            .or_insert_with(|| data.vfs.exists(path).then(|| data.read(path)).flatten().and_then(|b| Egm::parse(&b).ok()).map(Arc::new))
            .clone()
    }

    fn egt(&mut self, data: &GameData, path: &str) -> Option<Arc<Egt>> {
        self.egt
            .entry(path.to_string())
            .or_insert_with(|| data.vfs.exists(path).then(|| data.read(path)).flatten().and_then(|b| Egt::parse(&b).ok()).map(Arc::new))
            .clone()
    }

    fn base(&mut self, data: &GameData, path: &str) -> Option<Arc<Rgba>> {
        self.base.entry(path.to_string()).or_insert_with(|| data.read(path).and_then(|b| decode_top(&b)).map(Arc::new)).clone()
    }

    /// Per shape of `part` (model `model`), a FaceGen-morphed mesh where an `.egm` sits
    /// beside the model and covers the shape's vertices: a one-shape part, or a shape
    /// with exactly the EGM's vertex count.
    pub fn morph(&mut self, ctx: &mut ActorCtx, part: &PreparedPart, model: &str, fg: &FaceGen) -> Vec<Option<Handle<Mesh>>> {
        let mut out = vec![None; part.shapes.len()];
        let Some(egm) = self.egm(&ctx.models.data, &sibling(model, "egm")) else { return out };
        for (i, s) in part.shapes.iter().enumerate() {
            let n = s.cpu.count_vertices();
            if n > egm.vertices || (part.shapes.len() > 1 && n != egm.vertices) {
                self.mismatched += 1;
                continue;
            }
            let mut mesh = (*s.cpu).clone();
            let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) else { continue };
            egm.apply(p, &fg.fggs, &fg.fgga);
            out[i] = Some(ctx.models.meshes.add(mesh));
            self.morphed_shapes += 1;
        }
        out
    }

    /// The face texture of an NPC: `texture` (or the shape's own diffuse) plus the
    /// `.egt` offset beside `model` for its FGTS coefficients.
    pub fn face_texture(&mut self, ctx: &mut ActorCtx, model: &str, texture: &str, fg: &FaceGen) -> Option<Handle<Image>> {
        let egt = self.egt(&ctx.models.data, &sibling(model, "egt"))?;
        let base = self.base(&ctx.models.data, &fnv_vfs::texture_path(texture))?;
        let (w, h, ref px) = *base;
        let mut rgba = px.clone();
        let offset = egt.offset(&fg.fgts, &[]);
        add_offset(&mut rgba, w, h, &offset, egt.rows, egt.columns);
        self.faces_textured += 1;
        Some(ctx.models.images.add(image_with_mips(w, h, rgba)))
    }

    /// An NPC's body skin texture: `texture` plus the race's "Upper Body Texture" `.egt`
    /// (`egt`, archive path) for its FGTS coefficients, combined as for the face
    /// [hypothesis, actors.md §4.1]. The result is halved until at most
    /// [`SKIN_MAX`] wide (the body texture is 1024² [observed], the EGT 32×32), so one
    /// per actor stays small.
    pub fn skin_texture(&mut self, ctx: &mut ActorCtx, egt: &str, texture: &str, fg: &FaceGen) -> Option<Handle<Image>> {
        let egt = self.egt(&ctx.models.data, egt)?;
        let base = self.base(&ctx.models.data, &fnv_vfs::texture_path(texture))?;
        let (mut w, mut h, ref px) = *base;
        let mut rgba = px.clone();
        let offset = egt.offset(&fg.fgts, &[]);
        add_offset(&mut rgba, w, h, &offset, egt.rows, egt.columns);
        while w > SKIN_MAX && h > 1 {
            (w, h, rgba) = halve(w, h, &rgba);
        }
        self.skins_textured += 1;
        Some(ctx.models.images.add(image_with_mips(w, h, rgba)))
    }
}

/// Widest body skin texture made per actor ([`FaceGenCache::skin_texture`]).
pub const SKIN_MAX: usize = 512;

/// Box-filter an RGBA8 image to half size.
fn halve(w: usize, h: usize, px: &[u8]) -> (usize, usize, Vec<u8>) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut next = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        for x in 0..nw {
            for k in 0..4 {
                let at = |xx: usize, yy: usize| u32::from(px[(yy.min(h - 1) * w + xx.min(w - 1)) * 4 + k]);
                let sum = at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1);
                next[(y * nw + x) * 4 + k] = ((sum + 2) / 4) as u8;
            }
        }
    }
    (nw, nh, next)
}

/// Decode the top mip of a DDS texture to RGBA8.
fn decode_top(bytes: &[u8]) -> Option<Rgba> {
    let dds = Dds::parse(bytes).ok()?;
    let level = dds.levels.first()?.first()?;
    let (w, h) = (dds.header.width as usize, dds.header.height as usize);
    let bc = match dds.format {
        Format::Dxt1 => Some(texpresso::Format::Bc1),
        Format::Dxt3 => Some(texpresso::Format::Bc2),
        Format::Dxt5 => Some(texpresso::Format::Bc3),
        _ => None,
    };
    let rgba = match bc {
        Some(f) => {
            let mut out = vec![0u8; w * h * 4];
            f.decompress(level, w, h, &mut out);
            out
        }
        None => to_rgba8(dds.format, level)?,
    };
    Some((w, h, rgba))
}

/// An sRGB RGBA8 image with a box-filtered mip chain.
fn image_with_mips(w: usize, h: usize, top: Vec<u8>) -> Image {
    let mut data = top.clone();
    let (mut cw, mut ch, mut cur) = (w, h, top);
    let mut levels = 1;
    while cw > 1 || ch > 1 {
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for k in 0..4 {
                    let at = |xx: usize, yy: usize| u32::from(cur[(yy.min(ch - 1) * cw + xx.min(cw - 1)) * 4 + k]);
                    let sum = at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1);
                    next[(y * nw + x) * 4 + k] = ((sum + 2) / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        (cw, ch, cur) = (nw, nh, next);
        levels += 1;
    }
    let mut img = Image::new(
        Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = levels;
    img.sampler = nvf_assets::texture::sampler();
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siblings() {
        assert_eq!(sibling(r"meshes\characters\head\headhuman.nif", "egm"), r"meshes\characters\head\headhuman.egm");
        assert_eq!(sibling("x", "egt"), "x.egt");
    }

    #[test]
    fn halving_averages_blocks() {
        let px: Vec<u8> = [0u8, 0, 0, 255, 100, 100, 100, 255].repeat(4);
        let (w, h, out) = halve(4, 2, &px);
        assert_eq!((w, h), (2, 1));
        assert_eq!(&out[..4], &[50, 50, 50, 255]);
    }

    #[test]
    fn mip_chain() {
        let img = image_with_mips(4, 2, vec![100; 4 * 2 * 4]);
        assert_eq!(img.texture_descriptor.mip_level_count, 3);
        assert_eq!(img.data.as_ref().unwrap().len(), (8 + 2 + 1) * 4);
    }
}
