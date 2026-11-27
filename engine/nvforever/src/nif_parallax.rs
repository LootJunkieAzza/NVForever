//! The live height and switch of NIF materials' parallax (a texture set's `_p`, and the
//! game's own height slot, #2979), the F11 window's Parallax tab: `nvf-assets` puts a
//! material's parallax on when the material is built (`authored::apply_height`) and keeps
//! what it gave it (`NifSurface::parallax_height`); this scales that depth or takes the
//! depth map off, with no rebuild and no restart. The landscape has a march of its own
//! (terrain.rs "Parallax") which the same tab tunes; the two are separate because the
//! landscape's is one march over its blended layers, not Bevy's per-material one.
//!
//! Off (the tab's Objects box clear, or the height at 0) the materials keep their place in
//! the renderer and only lose the depth map, so turning it back on puts the same map back.

use bevy::prelude::*;
use nvf_assets::authored::Parallax;
use nvf_assets::NifMaterial;

use crate::terrain::ParallaxTuning;

pub struct NifParallaxPlugin;

impl Plugin for NifParallaxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync);
    }
}

/// A height map's depth as the tab asks: its own times the multiplier, 0 with objects off.
fn wanted(p: &Parallax, t: &ParallaxTuning) -> f32 {
    if t.objects {
        p.depth_scale * t.height
    } else {
        0.0
    }
}

/// Apply the tab's height and switch to every NIF material that carries a height map, when
/// either changed or a material was made (the count is watched, as terrain.rs's own sync
/// does). Only the materials whose base differs are touched: a slider move updates a few
/// hundred materials' uniforms, not every material in the game every frame.
fn sync(tuning: Res<ParallaxTuning>, mut materials: ResMut<Assets<NifMaterial>>, mut last: Local<Option<(usize, ParallaxTuning)>>) {
    let seen = (materials.len(), tuning.clone());
    if last.as_ref() == Some(&seen) {
        return;
    }
    *last = Some(seen);
    let stale: Vec<_> = materials
        .iter()
        .filter(|(_, m)| {
            let Some(p) = &m.extension.parallax_height else { return false };
            let depth = wanted(p, &tuning);
            (m.base.parallax_depth_scale - depth).abs() > 1e-6 || m.base.depth_map.is_some() != (depth > 0.0)
        })
        .map(|(id, _)| id)
        .collect();
    for id in stale {
        let Some(mut m) = materials.get_mut(id) else { continue };
        let Some(p) = m.extension.parallax_height.clone() else { continue };
        let depth = wanted(&p, &tuning);
        m.base.parallax_depth_scale = depth;
        m.base.depth_map = if depth > 0.0 { p.depth_map.clone() } else { None };
    }
}