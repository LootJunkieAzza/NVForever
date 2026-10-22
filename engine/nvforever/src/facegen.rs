//! FaceGen in the game (workstream 5 part C, docs/spec-notes/actors.md §4): an NPC's
//! FGGS/FGGA coefficients plus its race's morph every part that has an `.egm` (beside its
//! model: head, eyes, mouth, beards, hats, …; or one per shape: the hair models), and its
//! FGTS coefficients plus its race's add the `.egt` colour offset to the face and skin
//! textures. The combination rules are [hypothesis] (fnv_actor::facegen,
//! docs/spec-notes/actors.md §4.2, §4.3).
//!
//! The tinted face and skin textures are made on a worker thread: a full-size pass per
//! texture, two per NPC, was most of the hitch when NPCs came into view [observed: CPU
//! profile of the towns tour, 2026-10-22, docs/design/streaming-hitches.md]. Until one is
//! made, the NPC's material keeps the model's own texture; [`FaceGenCache::finish`] then
//! puts the tinted one in [engineering choice: the NPC is drawn throughout, its skin tone
//! settling a few frames later]. Made textures are reused by what they are made of while
//! anything draws them.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task, TaskPool};
use fnv_actor::facegen::{add_offset, Egm, Egt};
use fnv_actor::records::FaceGen;
use fnv_dds::{to_rgba8, Dds, Format};
use nvf_assets::actor::{ActorCtx, PreparedPart};
use nvf_assets::{GameData, NifMaterial};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Decoded top mip of a texture: width, height, RGBA8.
type Rgba = (usize, usize, Vec<u8>);

/// A file read once, by whichever thread asks first; `None` remembers that it is absent
/// or unreadable.
type Slot<T> = Arc<OnceLock<Option<Arc<T>>>>;

/// Parsed `.egt` files and decoded base textures by archive path, shared with the worker
/// threads that make tinted textures.
#[derive(Default)]
struct Files {
    egt: Mutex<HashMap<String, Slot<Egt>>>,
    base: Mutex<HashMap<String, Slot<Rgba>>>,
}

impl Files {
    fn egt(&self, data: &GameData, path: &str) -> Option<Arc<Egt>> {
        let slot = self.egt.lock().expect("egt files").entry(path.to_string()).or_default().clone();
        slot.get_or_init(|| data.vfs.exists(path).then(|| data.read(path)).flatten().and_then(|b| Egt::parse(&b).ok()).map(Arc::new)).clone()
    }

    fn base(&self, data: &GameData, path: &str) -> Option<Arc<Rgba>> {
        let slot = self.base.lock().expect("base textures").entry(path.to_string()).or_default().clone();
        slot.get_or_init(|| data.read(path).and_then(|b| decode_top(&b)).map(Arc::new)).clone()
    }

    /// The tinted texture `key` names, for the texture coefficients `fgts` (on a worker
    /// thread).
    fn make(&self, data: &GameData, key: &TintKey, fgts: &[f64]) -> Option<Image> {
        let egt = self.egt(data, &key.egt)?;
        let base = self.base(data, &key.texture)?;
        let (mut w, mut h, ref px) = *base;
        let mut rgba = px.clone();
        let offset = egt.offset(fgts, &[]);
        add_offset(&mut rgba, w, h, &offset, egt.rows, egt.columns);
        if key.skin {
            while w > SKIN_MAX && h > 1 {
                (w, h, rgba) = halve(w, h, &rgba);
            }
        }
        Some(image_with_mips(w, h, rgba))
    }
}

/// What a tinted texture is made of: face or body skin, the `.egt` and the base texture
/// (archive paths), and the texture coefficients' bits.
#[derive(Clone, PartialEq, Eq, Hash)]
struct TintKey {
    skin: bool,
    egt: String,
    texture: String,
    fgts: Vec<u64>,
}

/// A tinted texture being made, and the materials to draw with it once it is.
struct Bake {
    key: TintKey,
    image: Handle<Image>,
    task: Task<Option<Image>>,
    materials: Vec<Handle<NifMaterial>>,
}

/// A tinted face or skin texture ([`FaceGenCache::face_texture`]): made, or being made.
#[derive(Clone)]
pub enum Tint {
    Ready(Handle<Image>),
    Pending(Handle<Image>),
}

/// Parsed FaceGen files, decoded base textures and the tinted textures made from them;
/// `None` remembers that a file is absent or unreadable.
#[derive(Default)]
pub struct FaceGenCache {
    egm: HashMap<String, Option<Arc<Egm>>>,
    files: Arc<Files>,
    /// Tinted textures made, while something holds them.
    made: HashMap<TintKey, AssetId<Image>>,
    bakes: Vec<Bake>,
    pub morphed_shapes: usize,
    pub faces_textured: usize,
    /// Body skin textures tinted with the race's body `.egt`.
    pub skins_textured: usize,
    /// Tinted textures made on a worker thread and put into their materials.
    pub tints_put_in: usize,
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

/// The FaceGen file of one shape of a model: `meshes\a\hairbun.nif`, shape `NoHat` →
/// `meshes\a\hairbunnohat.<ext>` [observed: the hair models, actors.md §4.3].
pub fn shape_sibling(model: &str, shape: &str, ext: &str) -> String {
    let stem = model.rsplit_once('.').map_or(model, |(stem, _)| stem);
    format!("{stem}{}.{ext}", shape.to_ascii_lowercase())
}

impl FaceGenCache {
    pub(crate) fn egm(&mut self, data: &GameData, path: &str) -> Option<Arc<Egm>> {
        self.egm
            .entry(path.to_string())
            .or_insert_with(|| data.vfs.exists(path).then(|| data.read(path)).flatten().and_then(|b| Egm::parse(&b).ok()).map(Arc::new))
            .clone()
    }

    /// Per shape of `part` (model `model`), a mesh morphed by `fg`'s FGGS and FGGA
    /// (`Appearance::shape_facegen`) where an `.egm` covers the shape's vertices. With an
    /// `.egm` beside the model: a one-shape part, or a shape with exactly the EGM's
    /// vertex count. Without one: a shape with an `.egm` of its own ([`shape_sibling`])
    /// of exactly its vertex count.
    pub fn morph(&mut self, ctx: &mut ActorCtx, part: &PreparedPart, model: &str, fg: &FaceGen) -> Vec<Option<Handle<Mesh>>> {
        let mut out = vec![None; part.shapes.len()];
        let whole = self.egm(&ctx.models.data, &sibling(model, "egm"));
        for (i, s) in part.shapes.iter().enumerate() {
            let n = s.cpu.count_vertices();
            let (egm, fits) = match (&whole, s.name.as_deref()) {
                (Some(egm), _) => (egm.clone(), n <= egm.vertices && (part.shapes.len() == 1 || n == egm.vertices)),
                (None, Some(name)) => match self.egm(&ctx.models.data, &shape_sibling(model, name, "egm")) {
                    Some(egm) => (egm.clone(), n == egm.vertices),
                    None => continue,
                },
                (None, None) => continue,
            };
            if !fits {
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
    /// `.egt` offset beside `model` for the texture coefficients `fgts`
    /// (`Appearance::skin_fgts`). Put on a material with [`Self::tinted`].
    pub fn face_texture(&mut self, ctx: &mut ActorCtx, model: &str, texture: &str, fgts: &[f64]) -> Option<Tint> {
        self.tint(ctx, false, sibling(model, "egt"), fnv_vfs::texture_path(texture), fgts)
    }

    /// An NPC's body skin texture: `texture` plus the race's "Upper Body Texture" `.egt`
    /// (`egt`, archive path) for the texture coefficients `fgts`, combined as for the face
    /// [hypothesis, actors.md §4.1]. The result is halved until at most
    /// [`SKIN_MAX`] wide (the body texture is 1024² [observed], the EGT 32×32), so one
    /// per actor stays small.
    pub fn skin_texture(&mut self, ctx: &mut ActorCtx, egt: &str, texture: &str, fgts: &[f64]) -> Option<Tint> {
        self.tint(ctx, true, egt.to_string(), fnv_vfs::texture_path(texture), fgts)
    }

    /// The tinted texture made of `egt` and `texture` for `fgts`: the one made already,
    /// else one a worker thread makes. `None`: either file is missing.
    fn tint(&mut self, ctx: &mut ActorCtx, skin: bool, egt: String, texture: String, fgts: &[f64]) -> Option<Tint> {
        let data = &ctx.models.data;
        if !data.vfs.exists(&egt) || !data.vfs.exists(&texture) {
            return None;
        }
        let key = TintKey { skin, egt, texture, fgts: fgts.iter().map(|v| v.to_bits()).collect() };
        if let Some(image) = self.made.get(&key).and_then(|id| ctx.models.images.get_strong_handle(*id)) {
            return Some(Tint::Ready(image));
        }
        if let Some(b) = self.bakes.iter().find(|b| b.key == key) {
            return Some(Tint::Pending(b.image.clone()));
        }
        let image = ctx.models.images.reserve_handle();
        let (files, data, job, fgts) = (self.files.clone(), (*ctx.models.data).clone(), key.clone(), fgts.to_vec());
        let task = AsyncComputeTaskPool::get_or_init(TaskPool::default).spawn(async move { files.make(&data, &job, &fgts) });
        *(if skin { &mut self.skins_textured } else { &mut self.faces_textured }) += 1;
        self.bakes.push(Bake { key, image: image.clone(), task, materials: Vec::new() });
        Some(Tint::Pending(image))
    }

    /// A copy of `material` drawn with `tint`: with it now when it is made, else with the
    /// material's own texture until [`Self::finish`] puts it in.
    pub fn tinted(&mut self, materials: &mut Assets<NifMaterial>, material: &Handle<NifMaterial>, tint: &Tint) -> Option<Handle<NifMaterial>> {
        let mut m = materials.get(material)?.clone();
        let (Tint::Ready(image) | Tint::Pending(image)) = tint;
        let bake = self.bakes.iter_mut().find(|b| b.image.id() == image.id());
        if bake.is_none() {
            m.base.base_color_texture = Some(image.clone());
        }
        let copy = materials.add(m);
        if let Some(b) = bake {
            b.materials.push(copy.clone());
        }
        Some(copy)
    }

    /// Put the tinted textures made since the last call into their materials; the number
    /// put in. One that could not be made (an unreadable file) leaves its materials with
    /// their own texture.
    pub fn finish(&mut self, images: &mut Assets<Image>, materials: &mut Assets<NifMaterial>) -> usize {
        let mut done = 0;
        let mut i = 0;
        while i < self.bakes.len() {
            let Some(result) = check_ready(&mut self.bakes[i].task) else {
                i += 1;
                continue;
            };
            let b = self.bakes.swap_remove(i);
            let Some(image) = result else { continue };
            if images.insert(b.image.id(), image).is_err() {
                continue;
            }
            for m in &b.materials {
                if let Some(mut m) = materials.get_mut(m) {
                    m.base.base_color_texture = Some(b.image.clone());
                }
            }
            self.made.insert(b.key, b.image.id());
            self.tints_put_in += 1;
            done += 1;
        }
        if done > 0 {
            self.made.retain(|_, id| images.contains(*id));
        }
        done
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
        assert_eq!(shape_sibling(r"meshes\characters\hair\hairbun.nif", "NoHat", "egm"), r"meshes\characters\hair\hairbunnohat.egm");
        assert_eq!(shape_sibling("x", "Hat", "egm"), "xhat.egm");
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
