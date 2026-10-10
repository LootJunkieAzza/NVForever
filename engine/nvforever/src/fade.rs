//! Cell fades: exterior cells streamed in after the start fade in, and cells streamed out
//! fade out before they are dropped, crossfading with the distant LOD that stands in for
//! them (lod.rs draws each cell's part of its object LOD block with the complementary
//! pattern). The game fades objects in and out at its LOD transitions [observed: owner,
//! playing the game]; how it fades and for how long is not known, so this one dithers
//! (no sorting or blending needed, and the LOD's pixels fill exactly the ones the cell
//! leaves out) over `--cell-fade` seconds [hypothesis: duration chosen by eye].
//!
//! The dither is Bevy's visibility-range crossfade, driven by time instead of distance:
//! a range [`W`] wide that starts `k/16 × W` before the camera gives dither level `k`
//! (to ±0.1 of a level) at any distance the camera can see, so 17 fixed ranges per side
//! cover every step. Bevy keeps one GPU slot per distinct range for the whole run, which
//! is why the levels are quantised.
//!
//! Bevy picks a mesh's pipeline (with or without the dither) only when its mesh or
//! material changes, not when a range is added or removed: a mesh whose range went away
//! would keep the dithering pipeline with no range to read and vanish. So with fades on,
//! every mesh of a streamed cell gets a range in the frame it appears and keeps one for
//! its whole life ([`drawn`]`(16)`, fully drawn, at rest), and LOD block parts likewise.
//!
//! Terrain fades with its cell but the LOD terrain under it stays solid until the cell is
//! fully drawn (it is pushed behind the loaded terrain), so the ground never shows holes.

use crate::grass::GrassMaterial;
use crate::tree::{BarkMaterial, LeafMaterial};
use crate::world::Streamer;
use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use nvf_assets::NifMaterial;

/// Dither steps between hidden and fully drawn (Bevy's crossfade has 16).
pub const STEPS: u8 = 16;

/// Width of the fade ranges (game units): far more than any view distance, so where the
/// mesh stands barely moves its level.
const W: f32 = 1.0e8;

/// One cell's fade.
#[derive(Debug, Clone, Copy)]
pub struct CellFade {
    /// 0 hidden … 1 fully drawn.
    pub level: f32,
    /// Fading out (dropped at 0) rather than in.
    pub leaving: bool,
    /// The step last given to the cell's meshes.
    applied: Option<u8>,
}

impl CellFade {
    pub fn new(level: f32, leaving: bool) -> CellFade {
        CellFade { level, leaving, applied: None }
    }

    /// Give the cell's meshes their step again: meshes were added to it.
    pub fn again(&mut self) {
        self.applied = None;
    }

    /// The level as a dither step (0 hidden … [`STEPS`] fully drawn).
    pub fn step(&self) -> u8 {
        step(self.level)
    }
}

pub fn step(level: f32) -> u8 {
    (level.clamp(0.0, 1.0) * STEPS as f32).round() as u8
}

/// Draws `k`/16 of a mesh's pixels (Bevy's fading-in side).
pub fn drawn(k: u8) -> VisibilityRange {
    let x = -(k.min(STEPS) as f32) * (W / STEPS as f32);
    VisibilityRange { start_margin: x..x + W, end_margin: 2.0 * W..3.0 * W, use_aabb: false }
}

/// Draws exactly the pixels [`drawn`]`(k)` leaves out (Bevy's fading-out side); `k` =
/// [`STEPS`] culls the mesh.
pub fn complement(k: u8) -> VisibilityRange {
    let z = -(k.min(STEPS) as f32) * (W / STEPS as f32);
    VisibilityRange { start_margin: z..z, end_margin: z..z + W, use_aabb: false }
}

/// Meshes whose materials dither (nif_material.wgsl, Bevy's PBR fragment for trees and
/// grass); water keeps its own shader and does not fade.
type Fading = Or<(
    With<MeshMaterial3d<NifMaterial>>,
    With<MeshMaterial3d<LeafMaterial>>,
    With<MeshMaterial3d<BarkMaterial>>,
    With<MeshMaterial3d<GrassMaterial>>,
)>;

/// Give the meshes of every fading cell its current step (runs after `world::stream`,
/// so a cell loaded this frame has its range before it is first drawn; cells loaded
/// fully drawn come through here once too). A cell that has faded in fully drops its
/// fade; its meshes keep the fully drawn range (module docs).
pub fn fade_cells(mut commands: Commands, mut streamer: ResMut<Streamer>, children: Query<&Children>, meshes: Query<(), Fading>) {
    let Streamer { fades, loaded, .. } = &mut *streamer;
    fades.retain(|c, f| {
        let Some(&cell) = loaded.get(c) else { return false };
        let k = f.step();
        if f.applied != Some(k) {
            f.applied = Some(k);
            for e in children.iter_descendants(cell).filter(|&e| meshes.contains(e)) {
                commands.entity(e).insert(drawn(k));
            }
        }
        f.leaving || k < STEPS
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CPU mirror of Bevy's `get_visibility_range_dither_level` (mesh_functions.wgsl).
    fn level(r: &VisibilityRange, d: f32) -> i32 {
        let (x, y, z, w) = (r.start_margin.start, r.start_margin.end, r.end_margin.start, r.end_margin.end);
        let (offset, a, b) = if d >= z { (0, z, w) } else { (-16, x, y) };
        offset + (((d - a) / (b - a) * 16.0).round() as i32).clamp(0, 16)
    }

    /// Bevy's `visibility_range_dither`: whether a pixel with this threshold is drawn.
    fn kept(level: i32, threshold: i32) -> bool {
        if level == 0 {
            return true;
        }
        if level <= -16 || level >= 16 {
            return false;
        }
        !((level >= 0 && level + threshold >= 16) || (level < 0 && 1 + level + threshold <= 0))
    }

    #[test]
    fn steps_are_complementary_at_any_distance() {
        for d in [0.0, 5.0, 4096.0, 60_000.0, 400_000.0] {
            for k in 0..=STEPS {
                let (a, b) = (drawn(k), complement(k));
                assert_eq!(level(&a, d), k as i32 - 16, "drawn({k}) at {d}");
                assert_eq!(level(&b, d), k as i32, "complement({k}) at {d}");
                // CPU culling: only complement(16) is culled.
                assert!(a.is_visible_at_all(d));
                assert_eq!(b.is_visible_at_all(d), k < STEPS);
                assert!(!a.is_abrupt() && !b.is_abrupt());
                let shown = (0..16).filter(|&t| kept(level(&a, d), t)).count();
                assert_eq!(shown, k as usize);
                for t in 0..16 {
                    assert_ne!(kept(level(&a, d), t), kept(level(&b, d), t), "k {k} t {t}");
                }
            }
        }
    }

    #[test]
    fn step_rounds_and_clamps() {
        assert_eq!(step(-1.0), 0);
        assert_eq!(step(0.5), 8);
        assert_eq!(step(2.0), STEPS);
    }
}
