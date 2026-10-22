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
//! first. Dropping the cache's handles lets Bevy free the assets; asking again prepares the
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

/// Adds the sweep (the game; the viewer keeps everything it loaded).
pub struct AssetReleasePlugin;

impl Plugin for AssetReleasePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sweeps>().add_systems(Last, release_unused);
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
        let mut seen = HashMap::new();
        for (k, used) in entries {
            if used {
                self.since.remove(&k);
            } else {
                let t = *self.since.entry(k.clone()).or_insert(now);
                seen.insert(k, t);
            }
        }
        // Keys no longer cached (dropped elsewhere) are forgotten.
        self.since.retain(|k, _| seen.contains_key(k));
        let mut idle: Vec<(K, f64)> = seen.into_iter().collect();
        idle.sort_by(|a, b| a.1.total_cmp(&b.1));
        let over = idle.len().saturating_sub(keep_max);
        let drop: Vec<K> = idle.iter().enumerate().filter(|(i, (_, t))| *i < over || now - *t >= keep_secs).map(|(_, (k, _))| k.clone()).collect();
        for k in &drop {
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

/// Whether a handle has a strong holder besides `own` copies (a weak or UUID handle: no).
pub fn held_beyond<A: Asset>(h: &Handle<A>, own: usize) -> bool {
    matches!(h, Handle::Strong(a) if Arc::strong_count(a) > own)
}

#[derive(Resource, Default)]
struct Sweeps {
    since: f32,
    models: Unused<String>,
    billboards: Unused<u32>,
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
) {
    s.since += time.delta_secs();
    if s.since < SWEEP_SECS {
        return;
    }
    s.since = 0.0;
    let now = time.elapsed_secs_f64();
    let s = &mut *s;
    let mut dropped = [0usize; 4];
    // Models before textures: a dropped model's materials let go of their images, which
    // the texture sweep sees from the next sweep on.
    if let Some(m) = models.as_deref_mut() {
        dropped[0] = m.release(&mut s.models, &mut s.billboards, now);
    }
    if let Some(p) = parts.as_deref_mut() {
        (dropped[1], dropped[2]) = p.release(&mut s.parts, &mut s.part_materials, now);
    }
    if let Some(t) = textures.as_deref_mut() {
        dropped[3] = t.release(&mut s.textures, now);
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
    fn entries_dropped_elsewhere_are_forgotten() {
        let mut u = Unused::default();
        u.sweep(0.0, [("a", false)], 60.0, 10);
        u.sweep(1.0, std::iter::empty::<(&str, bool)>(), 60.0, 10);
        assert!(u.is_empty());
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
