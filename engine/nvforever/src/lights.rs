//! Light sources of LIGH references (docs/spec-notes/world.md §8). The game's lights
//! brighten surfaces by colour × fade × attenuation × N·L in the display units of the sky
//! lighting (sky.rs `light_unit`), with an attenuation that reaches 0 at the light's
//! radius [hypothesis]. Bevy's physically based point lights fall off with the inverse
//! square of the distance instead: a hot spot at the bulb and little light a few feet
//! away. [`LightsPlugin`] replaces Bevy's distance attenuation (`getDistanceAttenuation`
//! in `bevy_pbr::lighting`) with the game's, and [`spawn_light`] sets intensities in
//! display units.

use bevy::prelude::*;
use bevy::shader::Shader;
use fnv_world::lighting::{light_data, light_flags};
use std::f32::consts::PI;

use crate::world::WorldData;

/// The embedded asset path under which bevy_pbr registers `bevy_pbr::lighting`.
const PBR_LIGHTING: &str = "embedded://bevy_pbr/render/pbr_lighting.wgsl";

/// Bevy's attenuation: windowed inverse square [observed: bevy_pbr 0.19.1].
const BEVY_ATTENUATION: &str = "return getRangeFalloff(distanceSquare, inverseRangeSquared) * 1.0 / max(distanceSquare, 0.0001);";

/// The game's attenuation: 1 − (d / radius)², clamped [hypothesis: shape to calibrate
/// against in-game captures; the LIGH falloff exponent, 1 for most lights, is not used].
const GAME_ATTENUATION: &str = "return saturate(1.0 - distanceSquare * inverseRangeSquared);";

pub struct LightsPlugin;

impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        let handle: Handle<Shader> = app.world().resource::<AssetServer>().load(PBR_LIGHTING);
        app.insert_resource(LightingShader { handle, patched: false }).add_systems(Update, patch_attenuation);
    }
}

#[derive(Resource)]
struct LightingShader {
    handle: Handle<Shader>,
    patched: bool,
}

/// Once bevy_pbr's lighting shader has loaded, swap its distance attenuation for the
/// game's. Pipelines that already use it are rebuilt.
fn patch_attenuation(mut state: ResMut<LightingShader>, mut shaders: ResMut<Assets<Shader>>) {
    if state.patched {
        return;
    }
    let Some(shader) = shaders.get(&state.handle) else { return };
    state.patched = true;
    let bevy::shader::Source::Wgsl(source) = &shader.source else {
        warn!("{PBR_LIGHTING}: not WGSL; lights keep Bevy's inverse-square falloff");
        return;
    };
    if !source.contains(BEVY_ATTENUATION) {
        warn!("{PBR_LIGHTING}: distance attenuation not found (Bevy changed?); lights keep Bevy's inverse-square falloff");
        return;
    }
    let patched = source.replace(BEVY_ATTENUATION, GAME_ATTENUATION);
    let path = shader.path.clone();
    let _ = shaders.insert(&state.handle, Shader::from_wgsl(patched, path));
    info!("lights: game attenuation installed");
}

/// Bevy lumens for a light that adds `brightness` × colour × attenuation × N·L on screen:
/// Bevy divides lumens by 4π (candela) and Lambert diffuse by π; the camera exposure is
/// undone by the light unit.
pub fn lumens(brightness: f32, unit: f32) -> f32 {
    4.0 * PI * PI * brightness * unit
}

/// A LIGH reference's light: a point light (spot light for spot types) with the light's
/// colour and radius as range [documented: LIGH DATA] and brightness fade × `scale`.
/// Negative and off-by-default lights are not drawn.
pub fn spawn_light(commands: &mut Commands, cell_e: Entity, world: &WorldData, ligh: u32, at: Vec3, scale: f32) -> bool {
    let Some(l) = light_data(world.lo, world.schema, ligh) else { return false };
    if l.flags & (light_flags::NEGATIVE | light_flags::OFF_BY_DEFAULT) != 0 || l.radius <= 0.0 {
        return false;
    }
    let lin = l.color.map(fnv_world::sky::srgb_to_linear);
    let color = Color::LinearRgba(LinearRgba::rgb(lin[0], lin[1], lin[2]));
    let intensity = lumens(l.fade.max(0.0) * scale, crate::sky::light_unit());
    let t = Transform::from_translation(at);
    if l.flags & light_flags::SPOT != 0 {
        let a = (l.fov.clamp(1.0, 179.0) / 2.0).to_radians();
        // Spot lights point down their reference's -z [hypothesis].
        commands.entity(cell_e).with_child((
            SpotLight { color, intensity, range: l.radius, outer_angle: a, inner_angle: a * 0.8, shadow_maps_enabled: false, ..default() },
            t.looking_to(-Vec3::Z, Vec3::Y),
        ));
    } else {
        commands.entity(cell_e).with_child((
            PointLight { color, intensity, range: l.radius, shadow_maps_enabled: false, ..default() },
            t,
        ));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_units() {
        // Lumens → candela (÷4π) → Lambert (÷π) → exposure (÷unit) gives back the brightness.
        let unit = 1000.0;
        let b = lumens(2.2, unit) / (4.0 * PI) / PI / unit;
        assert!((b - 2.2).abs() < 1e-5);
    }
}
