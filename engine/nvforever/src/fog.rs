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
const PBR_FOG: &str = "embedded://bevy_pbr/render/fog.wesl";

/// Bevy's atmospheric fog factors [observed: bevy_pbr 0.20.1 `fog.wesl`].
const BEVY_ATMOSPHERIC: &str = "    let extinction_factor = 1.0 - 1.0 / exp(distance * fog_params.be);
    let inscattering_factor = 1.0 - 1.0 / exp(distance * fog_params.bi);
    return vec4<f32>(
        input_color.rgb * (1.0 - extinction_factor * fog_color.a)
            + fog_color.rgb * inscattering_factor * fog_color.a,
        input_color.a
    );";

/// The game's fog: near, far and power in `be` (the atmospheric fog's extinction). With
/// `bi.x` 1 the colour is blended in display values (gamma 2.2), else in linear light.
/// Outdoors the desert haze (haze.rs, an enhanced-look switch) arrives as `scattering`
/// (x its share, y its sun weight; Bevy's own use of the argument needs a directional light
/// alpha above 0, which is never set): the two are blended once, as transmittances, the
/// haze seen through the weather's fog. Without haze (`scattering.x` 0) the old two lines
/// run unchanged [design, #2675]. The aerial perspective (aerial.rs, an enhanced-look switch)
/// comes first: only when aerial.rs `nvf_aerial` resolved this fragment's in-scatter colour
/// into the uniform's copy (`nvf_params[0]`, display values, alpha -2, which the CPU never
/// writes (its lobe strength is within -1..1); `nvf_params[1].rgb` the channel weights,
/// `nvf_params[2]` the sun colour in display values and the sun lobe's weight) the weather's
/// share becomes one per channel, 1 - (1 - fog)^w, still one blend with the haze over it, and
/// the sun lobe moves the finished pixel (linear light) toward the same blend with the sun
/// colour as the in-scatter colour, by the lobe's weight times their luminance contrast, so
/// it only ever brightens the pixel shown; otherwise the lines below run as before
/// [design, #2993].
const GAME_FOG: &str = "    let fog_t = clamp((distance - fog_params.be.x) / max(fog_params.be.y - fog_params.be.x, 1.0), 0.0, 1.0);
    let fog_share = select(0.0, pow(fog_t, fog_params.be.z), fog_t > 0.0) * fog_color.a;
    if fog_params.bi.x > 0.5 {
        if fog_params.nvf_params[0].w < -1.5 {
            // NVForever aerial perspective (aerial.rs): per-channel shares, the in-scatter colour,
            // the sun lobe on the finished pixel.
            let kept = 1.0 - clamp(fog_share, 0.0, 1.0);
            var air_share = vec3<f32>(1.0);
            if kept > 0.0 {
                air_share = vec3<f32>(1.0) - pow(vec3<f32>(kept), max(fog_params.nvf_params[1].rgb, vec3<f32>(0.0)));
            }
            let air = fog_params.nvf_params[0].rgb;
            let sun = fog_params.nvf_params[2].rgb;
            var veil_share = air_share;
            var veil = air;
            var sun_veil = sun;
            if scattering.x > 0.0 && fog_params.directional_light_color.a <= 0.0 {
                let haze_share = clamp(scattering.x, 0.0, 1.0);
                let haze_colour = mix(fog_color.rgb, fog_params.directional_light_color.rgb, clamp(scattering.y, 0.0, 1.0));
                veil_share = vec3<f32>(1.0) - (vec3<f32>(1.0) - air_share) * (1.0 - haze_share);
                let haze_part = pow(max(haze_colour, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2)) * (haze_share * (vec3<f32>(1.0) - air_share));
                veil = (air * air_share + haze_part) / max(veil_share, vec3<f32>(1e-6));
                sun_veil = (sun * air_share + haze_part) / max(veil_share, vec3<f32>(1e-6));
            }
            let shown = pow(max(input_color.rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
            var aerial = pow(mix(shown, veil, veil_share), vec3<f32>(2.2));
            let lobe_weight = fog_params.nvf_params[2].w;
            if lobe_weight > 0.0 {
                let toward = pow(mix(shown, sun_veil, veil_share), vec3<f32>(2.2));
                let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
                let toward_luma = dot(toward, luma);
                let brightening = clamp((toward_luma - dot(aerial, luma)) / max(toward_luma, 1e-4), 0.0, 1.0);
                aerial = mix(aerial, toward, clamp(lobe_weight * brightening, 0.0, 1.0));
            }
            return vec4<f32>(aerial, input_color.a);
        }
        if scattering.x > 0.0 && fog_params.directional_light_color.a <= 0.0 {
            // NVForever desert haze (haze.rs): total share 1 - (1 - fog)(1 - haze), one blend.
            let haze_share = clamp(scattering.x, 0.0, 1.0);
            let haze_colour = mix(fog_color.rgb, fog_params.directional_light_color.rgb, clamp(scattering.y, 0.0, 1.0));
            let veil_share = 1.0 - (1.0 - fog_share) * (1.0 - haze_share);
            let veil = (pow(max(fog_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)) * fog_share + pow(max(haze_colour, vec3(0.0)), vec3(1.0 / 2.2)) * (haze_share * (1.0 - fog_share))) / max(veil_share, 1e-6);
            let hazy = mix(pow(max(input_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), veil, veil_share);
            return vec4<f32>(pow(hazy, vec3(2.2)), input_color.a);
        }
        let shown = mix(pow(max(input_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), pow(max(fog_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), fog_share);
        return vec4<f32>(pow(shown, vec3(2.2)), input_color.a);
    }
    return vec4<f32>(mix(input_color.rgb, fog_color.rgb, fog_share), input_color.a);";

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
    let source = crate::lights::unix_lines(source);
    if !source.contains(BEVY_ATMOSPHERIC) {
        warn!("{PBR_FOG}: atmospheric fog not found (Bevy changed?); the fog stays linear");
        return;
    }
    let patched = with_source(shader, source.replace(BEVY_ATMOSPHERIC, GAME_FOG));
    let _ = shaders.insert(&state.handle, patched);
    INSTALLED.store(true, Ordering::Relaxed);
    if let Some(mut sky) = sky {
        sky.refresh();
    }
    info!("fog: the game's fog curve installed");
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
/// mult"): the fog is the one per-view value the world shaders all read. So does, in
/// `bi.z`, whether the main view keeps Tony McMapface (1; `NVF_TONEMAP=tony`) instead of
/// showing display values clipped (0, `lights::display_clip`): the NIF shader's hot light
/// sources undo the tonemapper only when there is one (`hot_light`, nif.md §6 "Emitted
/// light on screen").
pub fn falloff(near: f32, far: f32, power: f32, display: bool, emissive_scale: f32) -> FogFalloff {
    let far = far.max(near + 1.0);
    let power = if power > 0.0 { power } else { 1.0 };
    if INSTALLED.load(Ordering::Relaxed) {
        let tony = if crate::lights::display_clip() { 0.0 } else { 1.0 };
        FogFalloff::Atmospheric { extinction: Vec3::new(near, far, power), inscattering: Vec3::new(if display { 1.0 } else { 0.0 }, emissive_scale.max(0.0), tony) }
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
}
