//! Under-water fog: while the camera is below the water surface of its cell, the
//! camera's distance fog becomes the water type's under-water fog (WATR DNAM under fog
//! near / far [documented: xEdit names]) in its deep colour, lit by the current sun and
//! ambient light like the water body [hypothesis; the light level is tuned to match the
//! water surface's own shading]. Leaving the water hands the fog back to the weather.

use crate::reflection::ReflectionCamera;
use crate::sky::{SkyState, Sun};
use crate::water::DistantWater;
use crate::world::{bevy_to_game, Streamer, WorldData};
use bevy::camera::Exposure;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use fnv_world::cell_of;
use fnv_world::sky::{srgb_to_linear, Water};
use std::collections::HashMap;

/// Water types read so far, by FormID.
#[derive(Default)]
pub struct WaterTypes(HashMap<u32, Option<Water>>);

fn linear(c: Color) -> Vec3 {
    let l = c.to_linear();
    Vec3::new(l.red, l.green, l.blue)
}

/// The water surface the camera is under, with its type: the camera cell's water (the
/// interior's, indoors) when the camera is below it.
fn water_above(world: &WorldData, streamer: &Streamer, dw: &mut DistantWater, cam_game: Vec3) -> Option<(f32, u32)> {
    let (h, wid) = match streamer.interior {
        Some(id) => {
            let c = world.index.cells.get(&id)?;
            let ws = &world.index.worldspaces[&world.world];
            (c.water_surface(ws.default_water_height)?, c.water_type.or(ws.water)?)
        }
        None => dw.surface(world, cell_of(cam_game.x, cam_game.y))?,
    };
    (cam_game.z < h).then_some((h, wid))
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn underwater_fog(
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    mut dw: ResMut<DistantWater>,
    mut sky: ResMut<SkyState>,
    ambient: Res<GlobalAmbientLight>,
    sun: Query<&DirectionalLight, With<Sun>>,
    mut cam: Query<(&GlobalTransform, &mut DistanceFog, Option<&Exposure>), (With<Camera3d>, Without<ReflectionCamera>)>,
    mut types: Local<WaterTypes>,
    mut was_under: Local<bool>,
) {
    let Ok((gt, mut fog, exposure)) = cam.single_mut() else { return };
    let under = water_above(&world, &streamer, &mut dw, bevy_to_game(gt.translation()));
    let water = under.and_then(|(_, wid)| {
        types.0.entry(wid).or_insert_with(|| fnv_world::sky::water(world.lo, world.schema, wid)).clone()
    });
    let Some(w) = water else {
        if std::mem::take(&mut *was_under) {
            // Back above the surface: the weather's fog again.
            sky.refresh();
        }
        return;
    };
    *was_under = true;
    // The light a diffuse surface facing the sun receives: the display-value sum of the
    // sun and the ambient light (lights.rs `DISPLAY_HELPERS`; light × exposure, Lambert's
    // 1/π for the sun), raised to the display gamma.
    let exposure = exposure.copied().unwrap_or_default().exposure();
    let light = sun
        .single()
        .map(|s| linear(s.color) * s.illuminance / std::f32::consts::PI)
        .unwrap_or(Vec3::ZERO)
        + linear(ambient.color) * ambient.brightness;
    let deep = Vec3::from_array(w.deep.map(srgb_to_linear)) * (light * exposure).max(Vec3::ZERO).powf(2.2);
    let [near, far, _amount] = w.under_fog;
    fog.color = Color::LinearRgba(LinearRgba::rgb(deep.x, deep.y, deep.z));
    fog.falloff = FogFalloff::Linear { start: near, end: far.max(near + 1.0) };
}
