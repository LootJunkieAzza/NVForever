//! Under-water fog: while the camera is below the water surface of its cell, the
//! camera's distance fog becomes the water type's under-water fog (WATR DNAM under fog
//! near / far [documented: xEdit names]). Its colour goes from the water type's shallow
//! colour at the surface to its deep colour [`DEEP_AT`] units down, and is not lit: the
//! picture's grade (the image space by time of day) does the rest ([`fog_bytes`];
//! world.md §6 "Under-water colour", #2916) [observed in the original, hypothesis for the rule]. Leaving the
//! water hands the fog back to the weather.

use crate::sky::SkyState;
use crate::water::DistantWater;
use crate::world::{bevy_to_game, Streamer, WorldData};
use nvf_play::phys::PhysState;
use nvf_play::sim::SimState;
use fnv_sim::FormId;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use fnv_world::cell_of;
use fnv_world::sky::{srgb_to_linear, Water};
use std::collections::HashMap;

/// Water types read so far, by FormID.
#[derive(Default)]
pub struct WaterTypes(HashMap<u32, Option<Water>>);

/// The water surface the camera is under, with its type: outdoors the camera cell's
/// water when the camera is below it; indoors the physics' water the camera is in (placed
/// water volumes), coloured by that volume's own PWAT water type ([`fog_water_type`]).
/// An interior's own XCLW is not water [observed: physics.md §6.6, `swim_h2o_drops`,
/// `sw_guardian`]: e.g. The Tops' 13th floor and presidential suite store XCLW 14,095.9,
/// 5,000+ units above every reference in them, and reading it as a surface drew the
/// under-water fog (WastelandNV's Potomac, −1,000..700 units) round the camera there
/// (world.md §8 "Interior XCLW is not water"). Without physics (the viewer, no `--play`)
/// an interior has no under-water fog.
fn water_above(
    world: &WorldData,
    streamer: &Streamer,
    dw: &mut DistantWater,
    phys: Option<&PhysState>,
    sim: Option<&SimState>,
    placed_types: &mut HashMap<u32, Option<u32>>,
    cam_game: Vec3,
) -> Option<(f32, u32)> {
    let (h, wid) = match streamer.interior {
        Some(id) => {
            let c = world.index.cells.get(&id)?;
            let ws = &world.index.worldspaces[&world.world];
            let p = cam_game.to_array();
            let pw = &phys?.world;
            let h = pw.water_surface_for(p)?;
            // The placed volume that gave the surface (the highest one holding the
            // camera, as `water_surface_for` picks), and its base PWAT's water type.
            let placed = pw
                .placed_water()
                .filter(|w| w.holds_below(p) && w.surface == h)
                .map(|w| w.form_id)
                .next()
                .and_then(|r| {
                    *placed_types.entry(r).or_insert_with(|| {
                        let base = sim?.refs.get(FormId(r))?.base;
                        fnv_world::sky::placeable_water_type(world.lo, world.schema, base.0)
                    })
                });
            (h, fog_water_type(placed, c.water_type, ws.water)?)
        }
        None => dw.surface(world, cell_of(cam_game.x, cam_game.y))?,
    };
    (cam_game.z < h).then_some((h, wid))
}

/// The water type an interior's under-water fog takes: the placed water's own (its PWAT's
/// DNAM "Water", the type the renderer draws that pool with, world.rs
/// `spawn_placed_water`), else the cell's (XCWT), else the worldspace's [hypothesis: the
/// fog matches the pool's look; e.g. Vault 34's pools hold toxic water, not the
/// worldspace's Potomac].
fn fog_water_type(placed: Option<u32>, cell: Option<u32>, worldspace: Option<u32>) -> Option<u32> {
    placed.or(cell).or(worldspace)
}

/// How far below the surface the camera sees the water's deep colour alone
/// [hypothesis, fitted: under Lake Mead the original's picture darkens evenly with the
/// player's depth and stops about 1,100 units down, the camera some 120 above the feet;
/// both waters measured store under fog near −1,000, which may be where it comes from].
pub const DEEP_AT: f32 = 1000.0;

/// The under-water colour as display bytes with the camera `depth` units below the
/// surface: the shallow colour at the surface, the deep colour from [`DEEP_AT`] down, in
/// proportion between [observed, base game and Tale of Two Wastelands, 2026-11-18,
/// world.md §6 "Under-water colour": Lake Mead at noon, feet 150 to 1,250 units down, (72, 81, 38) to
/// (36, 42, 15), the same whichever way the camera looks; the Pitt's river 180 down,
/// (46, 53, 33) for shallow (56, 60, 34)].
pub fn fog_bytes(shallow: [u8; 3], deep: [u8; 3], depth: f32) -> [u8; 3] {
    let t = if depth.is_finite() { (depth / DEEP_AT).clamp(0.0, 1.0) } else { 0.0 };
    std::array::from_fn(|i| (shallow[i] as f32 + (deep[i] as f32 - shallow[i] as f32) * t).round() as u8)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn underwater_fog(
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    mut dw: ResMut<DistantWater>,
    mut sky: ResMut<SkyState>,
    mut cam: Query<(&GlobalTransform, &mut DistanceFog), crate::fly::WorldCamera>,
    phys: Option<Res<PhysState>>,
    sim: Option<Res<SimState>>,
    mut types: Local<WaterTypes>,
    mut placed_types: Local<HashMap<u32, Option<u32>>>,
    mut was_under: Local<bool>,
) {
    let Ok((gt, mut fog)) = cam.single_mut() else { return };
    let under = water_above(&world, &streamer, &mut dw, phys.as_deref(), sim.as_deref(), &mut placed_types, bevy_to_game(gt.translation()));
    let water = under.and_then(|(_, wid)| {
        types.0.entry(wid).or_insert_with(|| fnv_world::sky::water(world.lo, world.schema, wid)).clone()
    });
    let Some(w) = water else {
        if std::mem::take(&mut *was_under) {
            // Back above the surface: the weather's fog again.
            info!("underwater: the camera is out of the water, the cell's fog again");
            sky.refresh();
        }
        return;
    };
    let depth = under.map_or(0.0, |(h, _)| h - bevy_to_game(gt.translation()).z);
    let colour = fog_bytes(w.shallow, w.deep, depth);
    if !std::mem::replace(&mut *was_under, true) {
        let h = under.map_or(0.0, |(h, _)| h);
        info!(
            "underwater: the camera is under water (surface {h:.1}), the water's under-water fog: colour ({}, {}, {}) {depth:.0} units down, between shallow ({}, {}, {}) and deep ({}, {}, {})",
            colour[0], colour[1], colour[2], w.shallow[0], w.shallow[1], w.shallow[2], w.deep[0], w.deep[1], w.deep[2]
        );
    }
    // Not lit: by day the original's colour is the same from 09:00 to 18:00, and its night
    // colour is the day's after the night's grade (world.md §6 "Under-water colour").
    let [r, g, b] = colour.map(srgb_to_linear);
    let [near, far, _amount] = w.under_fog;
    fog.color = Color::LinearRgba(LinearRgba::rgb(r, g, b));
    fog.falloff = FogFalloff::Linear { start: near, end: far.max(near + 1.0) };
}

#[cfg(test)]
mod tests {
    use super::{fog_bytes, fog_water_type, DEEP_AT};

    /// #2916: the colour goes from shallow at the surface to deep at `DEEP_AT`, and stays.
    #[test]
    fn the_colour_goes_from_shallow_to_deep_with_depth() {
        // Potomac, the Mojave's water [observed: FalloutNV.esm WATR 0x00030009].
        let (shallow, deep) = ([52, 75, 52], [27, 39, 24]);
        assert_eq!(fog_bytes(shallow, deep, 0.0), shallow);
        assert_eq!(fog_bytes(shallow, deep, -50.0), shallow);
        assert_eq!(fog_bytes(shallow, deep, DEEP_AT), deep);
        assert_eq!(fog_bytes(shallow, deep, 5000.0), deep);
        assert_eq!(fog_bytes(shallow, deep, DEEP_AT / 2.0), [40, 57, 38]);
        // Evenly: equal steps down darken by equal amounts.
        let g = |d: f32| fog_bytes(shallow, deep, d)[1] as i32;
        assert_eq!(g(250.0) - g(500.0), g(500.0) - g(750.0));
        assert_eq!(fog_bytes(shallow, deep, f32::NAN), shallow);
    }

    #[test]
    fn placed_water_fogs_in_its_own_type() {
        // A PWAT's own water type wins over the cell's XCWT and the worldspace's water.
        assert_eq!(fog_water_type(Some(0x10), Some(0x20), Some(0x30)), Some(0x10));
        // A volume whose type cannot be read: the cell's, else the worldspace's.
        assert_eq!(fog_water_type(None, Some(0x20), Some(0x30)), Some(0x20));
        assert_eq!(fog_water_type(None, None, Some(0x30)), Some(0x30));
        assert_eq!(fog_water_type(None, None, None), None);
    }
}
