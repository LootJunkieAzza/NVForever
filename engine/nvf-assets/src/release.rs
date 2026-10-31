//! Letting go of prepared models, actor parts and textures nothing uses any more.
//!
//! The caches ([`ModelCache`], [`ActorParts`], [`TextureCache`]) hold strong handles to
//! every mesh, material and image they ever made, so without this the GPU assets of every
//! cell ever visited stay alive after the cell unloads [observed: the stress tour
//! (docs/design/stress-tests.md), meshes 816 → 4,726 and images 618 → 2,962 at the same
//! place a lap later, cells and entities back to their start].
//!
//! Every [`SWEEP_SECS`] an entry is *unused* when nothing outside the cache holds it: its
//! `Arc` has no other owner and no handle in it has a strong holder besides the cache's own
//! copies (spawned entities hold clones in `Mesh3d` / `MeshMaterial3d`, rigs and other
//! caches hold the `Arc`). An unused entry stays for [`KEEP_UNUSED_SECS`] (a quick return
//! through a door, a reload or the cell buffer's next cell finds it still prepared), and at
//! most [`KEEP_UNUSED_MAX`] unused entries of each cache stay, the longest unused going
//! first; both are the [`KeepLimits`] resource, which the game scales from the machine's
//! RAM and GPU memory budget. Dropping the cache's handles lets Bevy free the assets; asking again prepares the
//! file again. Remembered failures (`None`) are kept: they hold nothing.

use crate::actor::ActorParts;
use crate::model::ModelCache;
use crate::texture::TextureCache;
use bevy::prelude::*;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

/// Seconds between sweeps.
pub const SWEEP_SECS: f32 = 2.0;
/// Seconds an unused entry stays cached [our choice: five minutes, so turning back
/// towards cells left behind finds their models and textures still prepared; memory is
/// plentiful (owner, 2026-10-22: about 4.7 GB used of 48 GB). It was 60 s and 256
/// entries; docs/design/streaming-hitches.md].
pub const KEEP_UNUSED_SECS: f64 = 300.0;
/// Unused entries each cache keeps at most [our choice: models read ahead for the cells
/// around the loaded ones wait unused until those cells load, a few hundred at a time].
pub const KEEP_UNUSED_MAX: usize = 2048;

/// How long unused entries stay and how many each cache keeps: [`KEEP_UNUSED_SECS`] and
/// [`KEEP_UNUSED_MAX`] unless the game sets them from the machine's memory (nvforever
/// `memory_budget.rs`, #1170: less RAM or GPU memory keeps fewer for less long, and near
/// the GPU budget the longest unused go first).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct KeepLimits {
    pub secs: f64,
    pub max: usize,
}

impl Default for KeepLimits {
    fn default() -> Self {
        KeepLimits { secs: KEEP_UNUSED_SECS, max: KEEP_UNUSED_MAX }
    }
}

/// Adds the sweep (the game; the viewer keeps everything it loaded).
pub struct AssetReleasePlugin;

impl Plugin for AssetReleasePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sweeps>().init_resource::<KeepLimits>().add_systems(Last, release_unused);
    }
}

/// When each unused entry was first seen unused (real seconds), by key.
#[derive(Debug, Default)]
pub struct Unused<K> {
    since: HashMap<K, f64>,
}

impl<K: Hash + Eq + Clone> Unused<K> {
    /// The keys to drop now, given every cached entry with whether it is in use. An entry
    /// in use forgets its time; an unused one goes after `keep_secs`, or sooner when more
    /// than `keep_max` are unused (the longest unused first).
    pub fn sweep(&mut self, now: f64, entries: impl IntoIterator<Item = (K, bool)>, keep_secs: f64, keep_max: usize) -> Vec<K> {
        self.sweep_timed(now, entries, keep_secs, keep_max, |_| None).into_iter().map(|(k, _)| k).collect()
    }

    /// [`Self::sweep`], with the time each dropped key was first seen unused. `since`
    /// backdates a key seen unused for the first time: it counts as unused from then
    /// (no later than `now`), as a texture does from when the model that held it went
    /// unused ([`ImageHints`]).
    pub fn sweep_timed(
        &mut self,
        now: f64,
        entries: impl IntoIterator<Item = (K, bool)>,
        keep_secs: f64,
        keep_max: usize,
        mut since: impl FnMut(&K) -> Option<f64>,
    ) -> Vec<(K, f64)> {
        let mut seen = HashMap::new();
        for (k, used) in entries {
            if used {
                self.since.remove(&k);
            } else {
                let t = match self.since.get(&k) {
                    Some(&t) => t,
                    None => {
                        let t = since(&k).map_or(now, |t| t.min(now));
                        self.since.insert(k.clone(), t);
                        t
                    }
                };
                seen.insert(k, t);
            }
        }
        // Keys no longer cached (dropped elsewhere) are forgotten.
        self.since.retain(|k, _| seen.contains_key(k));
        let mut idle: Vec<(K, f64)> = seen.into_iter().collect();
        idle.sort_by(|a, b| a.1.total_cmp(&b.1));
        let over = idle.len().saturating_sub(keep_max);
        let drop: Vec<(K, f64)> = idle.into_iter().enumerate().filter(|(i, (_, t))| *i < over || now - *t >= keep_secs).map(|(_, e)| e).collect();
        for (k, _) in &drop {
            self.since.remove(k);
        }
        drop
    }

    pub fn len(&self) -> usize {
        self.since.len()
    }

    pub fn is_empty(&self) -> bool {
        self.since.is_empty()
    }
}

/// When the images of dropped models and actor parts started to go unused: the time the
/// model (or part) was first seen unused, by image. A texture that the drop leaves unused
/// counts as unused from that time, not from the sweep that first sees it so: otherwise it
/// waited out [`KEEP_UNUSED_SECS`] twice, once inside its model and once on its own
/// (issue #904). A note no texture sweep takes within [`HINT_SECS`] (the texture is still
/// held elsewhere, or by no cache) is forgotten.
#[derive(Default)]
pub struct ImageHints(HashMap<AssetId<Image>, (f64, f64)>);

/// Seconds an [`ImageHints`] note waits for the texture sweep: the dropped materials
/// let go of their images when Bevy next frees assets, after the sweep that dropped them.
pub const HINT_SECS: f64 = 10.0 * SWEEP_SECS as f64;

impl ImageHints {
    /// The images of a dropped material went unused at `since`.
    pub fn note(&mut self, materials: &Assets<crate::NifMaterial>, h: &Handle<crate::NifMaterial>, since: f64, now: f64) {
        if let Some(m) = materials.get(h) {
            for i in material_images(m) {
                self.note_image(i.id(), since, now);
            }
        }
    }

    pub fn note_image(&mut self, id: AssetId<Image>, since: f64, now: f64) {
        // Two models let go of one image: it went unused when the later of them did.
        let e = self.0.entry(id).or_insert((since, now));
        *e = (e.0.max(since), now);
    }

    /// When the image went unused, if a dropped model's note says.
    pub fn since(&self, id: AssetId<Image>) -> Option<f64> {
        self.0.get(&id).map(|e| e.0)
    }

    /// Forget the notes older than [`HINT_SECS`].
    pub fn prune(&mut self, now: f64) {
        self.0.retain(|_, (_, at)| now - *at < HINT_SECS);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The strong handles inside one cached entry: per handle, the copies in the entry and the
/// strong holders all told. The entry is in use when any handle has a holder beyond its
/// own copies.
#[derive(Default)]
pub struct Holders(HashMap<usize, (usize, usize)>);

impl Holders {
    pub fn add<A: Asset>(&mut self, h: &Handle<A>) {
        if let Handle::Strong(a) = h {
            let e = self.0.entry(Arc::as_ptr(a) as usize).or_insert((0, Arc::strong_count(a)));
            e.0 += 1;
        }
    }

    pub fn held_elsewhere(&self) -> bool {
        self.0.values().any(|&(own, all)| all > own)
    }
}

/// Drop the material copies (or other assets) of a map that nothing holds besides the
/// map's `own` copies of each handle: the copies made from a material for one look
/// (emittance, see-through actors, swapped textures). Made again on the next ask, from
/// the cached original, without reading a file; dropping them lets the textures they
/// hold go with their models (issue #904). The number dropped.
pub fn retain_held<K, A: Asset>(map: &mut HashMap<K, Handle<A>>, own: usize) -> usize {
    let before = map.len();
    map.retain(|_, h| held_beyond(h, own));
    before - map.len()
}

/// Whether a handle has a strong holder besides `own` copies (a weak or UUID handle: no).
pub fn held_beyond<A: Asset>(h: &Handle<A>, own: usize) -> bool {
    matches!(h, Handle::Strong(a) if Arc::strong_count(a) > own)
}

/// The images a material samples (the fields the game's materials set; nif_material.rs,
/// nif_spawn.rs).
pub fn material_images(m: &crate::NifMaterial) -> impl Iterator<Item = &Handle<Image>> {
    let b = &m.base;
    [
        &b.base_color_texture,
        &b.emissive_texture,
        &b.normal_map_texture,
        &b.metallic_roughness_texture,
        &b.occlusion_texture,
        &b.specular_texture,
        &b.depth_map,
        &m.extension.holes_texture,
        &m.extension.facegen_offset,
    ]
    .into_iter()
    .flatten()
}

/// The GPU bytes of an image with its mip chain, from its descriptor (the pixels may be
/// gone from the main world once uploaded).
pub fn image_bytes(i: &Image) -> u64 {
    let d = &i.texture_descriptor;
    let (bw, bh) = d.format.block_dimensions();
    let block = u64::from(d.format.block_copy_size(None).unwrap_or(4));
    let layers = u64::from(d.size.depth_or_array_layers.max(1));
    (0..d.mip_level_count.max(1))
        .map(|level| {
            let w = (d.size.width >> level).max(1).div_ceil(bw);
            let h = (d.size.height >> level).max(1).div_ceil(bh);
            u64::from(w) * u64::from(h) * block * layers
        })
        .sum()
}

/// What one cache holds, for the stress tests' `stress caches:` line: its entries, those
/// nothing outside it holds, and the images its materials sample.
#[derive(Default)]
pub struct CacheShare {
    pub entries: usize,
    pub idle: usize,
    pub images: std::collections::HashSet<AssetId<Image>>,
}

impl CacheShare {
    /// One entry: a material the cache holds `own` handles of.
    pub fn material(&mut self, materials: &Assets<crate::NifMaterial>, h: &Handle<crate::NifMaterial>, own: usize) {
        self.entries += 1;
        self.idle += usize::from(!held_beyond(h, own));
        self.images_of(materials, h);
    }

    /// The images of a material counted under an entry already counted.
    pub fn images_of(&mut self, materials: &Assets<crate::NifMaterial>, h: &Handle<crate::NifMaterial>) {
        if let Some(m) = materials.get(h) {
            self.images.extend(material_images(m).map(Handle::id));
        }
    }

    /// `<entries> (<idle> idle, <n> images <MB> MB)`.
    pub fn text(&self, images: &Assets<Image>) -> String {
        let bytes: u64 = self.images.iter().filter_map(|&id| images.get(id)).map(image_bytes).sum();
        format!("{} ({} idle, {} images {:.0} MB)", self.entries, self.idle, self.images.len(), bytes as f64 / (1024.0 * 1024.0))
    }
}

/// The model cache's unused times: models, billboards and particle definitions.
#[derive(Default)]
pub struct ModelSweeps {
    pub models: Unused<String>,
    pub billboards: Unused<u32>,
    pub particles: Unused<String>,
}

#[derive(Resource, Default)]
struct Sweeps {
    since: f32,
    models: ModelSweeps,
    hints: ImageHints,
    parts: Unused<String>,
    part_materials: Unused<(String, usize, String)>,
    textures: Unused<(String, bool, bool)>,
}

fn release_unused(
    time: Res<Time<Real>>,
    mut s: ResMut<Sweeps>,
    mut models: Option<ResMut<ModelCache>>,
    mut parts: Option<ResMut<ActorParts>>,
    mut textures: Option<ResMut<TextureCache>>,
    materials: Option<Res<Assets<crate::NifMaterial>>>,
    limits: Option<Res<KeepLimits>>,
) {
    s.since += time.delta_secs();
    if s.since < SWEEP_SECS {
        return;
    }
    s.since = 0.0;
    let now = time.elapsed_secs_f64();
    let s = &mut *s;
    let none = Assets::default();
    let materials = materials.as_deref().unwrap_or(&none);
    let keep = limits.as_deref().copied().unwrap_or_default();
    let mut dropped = [0usize; 4];
    // Models before textures: a dropped model's materials let go of their images, which
    // the texture sweep sees from the next sweep on, as unused since the model was.
    s.hints.prune(now);
    if let Some(m) = models.as_deref_mut() {
        dropped[0] = m.release(&mut s.models, &mut s.hints, materials, now, keep);
    }
    if let Some(p) = parts.as_deref_mut() {
        (dropped[1], dropped[2]) = p.release(&mut s.parts, &mut s.part_materials, &mut s.hints, materials, now, keep);
    }
    if let Some(t) = textures.as_deref_mut() {
        dropped[3] = t.release(&mut s.textures, &s.hints, now, keep);
    }
    if dropped.iter().any(|&n| n > 0) {
        debug!("assets released: {} models, {} actor parts, {} part materials, {} textures", dropped[0], dropped[1], dropped[2], dropped[3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_entries_go_after_the_grace_time() {
        let mut u = Unused::default();
        assert!(u.sweep(0.0, [("a", false), ("b", true)], 60.0, 10).is_empty());
        assert!(u.sweep(30.0, [("a", false), ("b", false)], 60.0, 10).is_empty());
        // Used again: its time starts over.
        assert!(u.sweep(50.0, [("a", true), ("b", false)], 60.0, 10).is_empty());
        assert_eq!(u.sweep(90.0, [("a", false), ("b", false)], 60.0, 10), ["b"]);
        assert_eq!(u.sweep(150.0, [("a", false)], 60.0, 10), ["a"]);
        assert!(u.is_empty());
    }

    #[test]
    fn at_most_keep_max_unused_stay_the_longest_unused_going_first() {
        let mut u = Unused::default();
        u.sweep(0.0, [(1, false)], 60.0, 2);
        u.sweep(1.0, [(1, false), (2, false)], 60.0, 2);
        let mut d = u.sweep(2.0, [(1, false), (2, false), (3, false), (4, false)], 60.0, 2);
        d.sort();
        assert_eq!(d, [1, 2]);
        assert_eq!(u.len(), 2);
    }

    #[test]
    fn a_backdated_entry_counts_as_unused_from_its_hint() {
        let mut u = Unused::default();
        // Its model went unused at 0; the texture is first seen unused at 250.
        assert!(u.sweep_timed(250.0, [("t", false), ("u", false)], 300.0, 10, |k| (*k == "t").then_some(0.0)).is_empty());
        assert_eq!(u.sweep_timed(302.0, [("t", false), ("u", false)], 300.0, 10, |_| None), [("t", 0.0)]);
        // A hint later than now counts from now; one used again starts over.
        assert!(u.sweep_timed(310.0, [("u", true), ("v", false)], 300.0, 10, |_| Some(900.0)).is_empty());
        assert_eq!(u.sweep_timed(610.0, [("u", false), ("v", false)], 300.0, 10, |_| None), [("v", 310.0)]);
    }

    #[test]
    fn image_hints_keep_the_later_drop_and_are_forgotten() {
        let mut h = ImageHints::default();
        let id = AssetId::<Image>::default();
        h.note_image(id, 5.0, 10.0);
        h.note_image(id, 2.0, 12.0);
        assert_eq!(h.since(id), Some(5.0));
        h.prune(12.0 + HINT_SECS - 1.0);
        assert_eq!(h.len(), 1);
        h.prune(12.0 + HINT_SECS);
        assert!(h.is_empty());
    }

    #[test]
    fn entries_dropped_elsewhere_are_forgotten() {
        let mut u = Unused::default();
        u.sweep(0.0, [("a", false)], 60.0, 10);
        u.sweep(1.0, std::iter::empty::<(&str, bool)>(), 60.0, 10);
        assert!(u.is_empty());
    }

    #[test]
    fn copies_nothing_else_holds_are_dropped() {
        let mut meshes = Assets::<Mesh>::default();
        let (a, b) = (meshes.add(Mesh::from(Cuboid::default())), meshes.add(Mesh::from(Cuboid::default())));
        let drawn = a.clone();
        let mut map = HashMap::from([(1, a), (2, b)]);
        assert_eq!(retain_held(&mut map, 1), 1);
        assert!(map.contains_key(&1));
        drop(drawn);
        assert_eq!(retain_held(&mut map, 1), 1);
        assert!(map.is_empty());
    }

    #[test]
    fn a_handle_is_held_when_a_clone_lives_outside_the_cache() {
        let mut meshes = Assets::<Mesh>::default();
        let h = meshes.add(Mesh::from(Cuboid::default()));
        let mut own = Holders::default();
        own.add(&h);
        assert!(!own.held_elsewhere());
        let spawned = h.clone();
        let mut own = Holders::default();
        own.add(&h);
        assert!(own.held_elsewhere());
        assert!(held_beyond(&h, 1));
        drop(spawned);
        assert!(!held_beyond(&h, 1));
    }
}
