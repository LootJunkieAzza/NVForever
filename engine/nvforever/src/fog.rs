//! The game's distance fog (docs/spec-notes/world.md §6 "Fog power"). Weathers and
//! interior lighting store a fog near, far and power; Bevy's distance fog has no curve
//! with a power. [`FogPlugin`] gives Bevy's fog shader (`bevy_pbr::fog`, which every
//! world shader reaches through `main_pass_post_lighting_processing`: NIF materials,
//! terrain, trees, grass, water) a curve of the game's kind, the share of fog colour
//! being ((distance − near) / (far − near))^power, clamped to 0..1 [hypothesis, fitted
//! to Dead Money's frames]. It takes the place of Bevy's atmospheric fog, which nothing
//! here uses: [`falloff`] packs near, far and power where that fog keeps its extinction.

use bevy::pbr::FogFalloff;
use bevy::prelude::*;
use bevy::shader::Shader;
use std::sync::atomic::{AtomicBool, Ordering};

/// The embedded asset path under which bevy_pbr registers `bevy_pbr::fog`.
const PBR_FOG: &str = "embedded://bevy_pbr/render/fog.wgsl";

/// Bevy's atmospheric fog factors [observed: bevy_pbr 0.19.1 `fog.wgsl`].
const BEVY_ATMOSPHERIC: &str = "    let extinction_factor = 1.0 - 1.0 / exp(distance * fog_params.be);
    let inscattering_factor = 1.0 - 1.0 / exp(distance * fog_params.bi);
    return vec4<f32>(
        input_color.rgb * (1.0 - extinction_factor * fog_color.a)
            + fog_color.rgb * inscattering_factor * fog_color.a,
        input_color.a
    );";

/// The game's fog: near, far and power in `be` (the atmospheric fog's extinction). With
/// `bi.x` 1 the colour is blended in display values (gamma 2.2), else in linear light.
const GAME_FOG: &str = "    let fog_t = clamp((distance - fog_params.be.x) / max(fog_params.be.y - fog_params.be.x, 1.0), 0.0, 1.0);
    let fog_share = select(0.0, pow(fog_t, fog_params.be.z), fog_t > 0.0) * fog_color.a;
    if fog_params.bi.x > 0.5 {
        let shown = mix(pow(max(input_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), pow(max(fog_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), fog_share);
        return vec4<f32>(pow(shown, vec3(2.2)), input_color.a);
    }
    return vec4<f32>(mix(input_color.rgb, fog_color.rgb, fog_share), input_color.a);";

/// Set once the patched shader is in place; until then (or if Bevy's shader changed)
/// [`falloff`] gives Bevy's linear fog.
static INSTALLED: AtomicBool = AtomicBool::new(false);

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        let handle: Handle<Shader> = app.world().resource::<AssetServer>().load(PBR_FOG);
        app.insert_resource(FogShader { handle, patched: false }).add_systems(PreUpdate, patch_fog);
    }
}

#[derive(Resource)]
struct FogShader {
    handle: Handle<Shader>,
    patched: bool,
}

/// Once bevy_pbr's fog shader has loaded, swap its atmospheric fog for the game's curve.
/// Pipelines that already use it are rebuilt; the sky system is told to set the fog again.
fn patch_fog(mut state: ResMut<FogShader>, mut shaders: ResMut<Assets<Shader>>, sky: Option<ResMut<crate::sky::SkyState>>) {
    if state.patched {
        return;
    }
    let Some(shader) = shaders.get(&state.handle) else { return };
    state.patched = true;
    let bevy::shader::Source::Wgsl(source) = &shader.source else {
        warn!("{PBR_FOG}: not WGSL; the fog stays linear");
        return;
    };
    if !source.contains(BEVY_ATMOSPHERIC) {
        warn!("{PBR_FOG}: atmospheric fog not found (Bevy changed?); the fog stays linear");
        return;
    }
    let patched = source.replace(BEVY_ATMOSPHERIC, GAME_FOG);
    let path = shader.path.clone();
    let _ = shaders.insert(&state.handle, Shader::from_wgsl(patched, path));
    INSTALLED.store(true, Ordering::Relaxed);
    if let Some(mut sky) = sky {
        sky.refresh();
    }
    info!("fog: the game's fog curve installed");
}

/// The fog falloff for a near, far and power (a power not above 0 is 1, linear); with
/// `display` the fog colour is blended in display values (outdoors), else in linear
/// light (indoors) [hypothesis, fitted: world.md §6 "Fog power"]. `emissive_scale` rides
/// along for the NIF material shader (`view_emissive_scale`, world.md §6 "Emissive
/// mult"): the fog is the one per-view value the world shaders all read.
pub fn falloff(near: f32, far: f32, power: f32, display: bool, emissive_scale: f32) -> FogFalloff {
    let far = far.max(near + 1.0);
    let power = if power > 0.0 { power } else { 1.0 };
    if INSTALLED.load(Ordering::Relaxed) {
        FogFalloff::Atmospheric { extinction: Vec3::new(near, far, power), inscattering: Vec3::new(if display { 1.0 } else { 0.0 }, emissive_scale.max(0.0), 0.0) }
    } else {
        FogFalloff::Linear { start: near, end: far }
    }
}

/// The share of fog colour at `distance`, as the patched shader computes it.
pub fn share(near: f32, far: f32, power: f32, distance: f32) -> f32 {
    let t = ((distance - near) / (far - near).max(1.0)).clamp(0.0, 1.0);
    let power = if power > 0.0 { power } else { 1.0 };
    if t > 0.0 { t.powf(power) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_follows_the_power() {
        // NVDLC01VillaWeather: 0..75,000, power 0.35: a wall 1,000 units away 22 % fogged.
        assert!((share(0.0, 75_000.0, 0.35, 1_000.0) - 0.22).abs() < 0.01);
        // Power 1 is Bevy's linear fog.
        assert!((share(1_000.0, 5_000.0, 1.0, 2_000.0) - 0.25).abs() < 1e-6);
        assert_eq!(share(350.0, 15_000.0, 0.21, 100.0), 0.0);
        assert_eq!(share(0.0, 100.0, 0.5, 1e6), 1.0);
    }
}
