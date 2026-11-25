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

pub mod shader;
#[cfg(test)]
use shader::{BEVY_ATMOSPHERIC, GAME_FOG};
use shader::fog_patched;

/// The embedded asset path under which bevy_pbr registers `bevy_pbr::fog`.
const PBR_FOG: &str = "embedded://bevy_pbr/render/fog.wesl";

/// Set once the patched shader is in place; until then (or if Bevy's shader changed)
/// [`falloff`] gives Bevy's linear fog.
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Whether the game's fog curve is in place (the fog uniform then carries its numbers and
/// the desert haze's, haze.rs).
pub fn installed() -> bool {
    INSTALLED.load(Ordering::Relaxed)
}

/// The systems that write into the main camera's fog after the sky does (haze.rs, aerial.rs):
/// the cameras that copy that fog (the 3D scope's lens) run after them.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FogWriters;

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
    let bevy::shader::Source::Wesl(source) = &shader.source else {
        warn!("{PBR_FOG}: not WESL; the fog stays linear");
        return;
    };
    let physical = nvf_assets::authored::physical_pbr();
    let Some(patched_src) = fog_patched(source, physical) else {
        warn!("{PBR_FOG}: atmospheric fog not found (Bevy changed?); the fog stays linear");
        return;
    };
    let patched = with_source(shader, patched_src);
    let _ = shaders.insert(&state.handle, patched);
    INSTALLED.store(true, Ordering::Relaxed);
    if let Some(mut sky) = sky {
        sky.refresh();
    }
    if physical {
        info!("fog: linear Beer-Lambert fog installed (physical PBR)");
    } else {
        info!("fog: the game's fog curve installed");
    }
}

/// `shader` with its source replaced by `source` (a patched copy of it): the import
/// path, the imports and the shader defs stay, since the patches add no imports.
pub fn with_source(shader: &Shader, source: String) -> Shader {
    let mut out = shader.clone();
    out.source = bevy::shader::Source::Wesl(source.into());
    out
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

/// The vendored bevy_pbr `fog.wesl` with [`GAME_FOG`] in place, as [`patch_fog`] makes it
/// (for the shader tests here and in haze.rs).
#[cfg(test)]
pub(crate) fn patched_vendored_fog() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../vendor/bevy_pbr/src/render/fog.wesl");
    let source = crate::lights::unix_lines(&std::fs::read_to_string(path).expect("vendored fog.wesl"));
    assert_eq!(source.matches(BEVY_ATMOSPHERIC).count(), 1, "the atmospheric fog's body, once");
    source.replace(BEVY_ATMOSPHERIC, GAME_FOG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_fog_replaces_bevys_atmospheric_fog_once() {
        let patched = patched_vendored_fog();
        assert_eq!(patched.matches("let fog_share = ").count(), 1);
        assert!(!patched.contains("extinction_factor"));
        // The haze's branch comes first and the old display-value lines stay as they were.
        let haze = patched.find("if scattering.x > 0.0").expect("the haze branch");
        let old = patched.find("        let shown = mix(pow(max(input_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), pow(max(fog_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), fog_share);").expect("the old blend");
        assert!(haze < old);
    }

    #[test]
    fn share_follows_the_power() {
        // NVDLC01VillaWeather: 0..75,000, power 0.35: a wall 1,000 units away 22 % fogged.
        assert!((share(0.0, 75_000.0, 0.35, 1_000.0) - 0.22).abs() < 0.01);
        // Power 1 is Bevy's linear fog.
        assert!((share(1_000.0, 5_000.0, 1.0, 2_000.0) - 0.25).abs() < 1e-6);
        assert_eq!(share(350.0, 15_000.0, 0.21, 100.0), 0.0);
        assert_eq!(share(0.0, 100.0, 0.5, 1e6), 1.0);
    }

    #[test]
    fn physical_fog_replaces_atmospheric_fog_cleanly() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../vendor/bevy_pbr/src/render/fog.wesl");
        let raw = std::fs::read_to_string(path).expect("vendored fog.wesl");
        let physical = fog_patched(&raw, true).expect("physical fog patch applies");
        assert!(physical.contains("exp(-tau)"));
        assert!(physical.contains("tau_R"));
        assert!(physical.contains("phase_M"));
        assert!(!physical.contains("1.0 / 2.2"), "physical fog blends linearly without display gamma");
        assert!(!physical.contains("fog_params.be.z"), "no concave near-camera fog_power distortion");
        let game = fog_patched(&raw, false).expect("game fog patch applies");
        assert!(game.contains("1.0 / 2.2"));
    }
}
