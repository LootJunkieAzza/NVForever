//! Engine-level picture improvements that need no new or changed assets
//! (docs/design/graphics-enhanced.md). `--graphics vanilla` draws the picture the parity
//! checks compare; `--graphics enhanced` adds, each on its own switch of the settings page:
//!
//! - **Ambient occlusion**: a screen-space pass on the depth prepass darkens creases,
//!   corners and the ground under things (`ssao.rs`; works with the multisampled view).
//! - **Finer, longer shadows**: the sun's shadow maps at 4096 instead of 2048 texels and
//!   reaching half as far again (`SkyState::shadow_reach`), the first cascade as near.
//! - **Bloom**: bright light (the sun and its glare, lamps, lit windows) glows into what
//!   lies around it, thresholded on the tonemapped picture (`post.rs`).
//! - **Sharpening**: contrast-adaptive sharpening of the picture (`post.rs`).
//! - **Anisotropic filtering 16x** (8x in vanilla) for the game's textures loaded from then on.
//! - **Temporal anti-aliasing** instead of multisampling (`taa.rs`; its own switch, not
//!   part of `enhanced`): the view is drawn without multisampling and smoothed over the
//!   frames; switching it off brings back `--msaa`.
//! - **Sun shafts**: light streaming past what stands in front of the sun, as strong as
//!   the weather's fog makes the air hazy (`shafts.rs`).
//! - **Screen-space water reflections** where the mirrored reflection camera does not
//!   reach: water at another height than the one it shows, and interiors (`water.wgsl`).
//!
//! The game's own image space (the grade and the modifiers' blur, imagespace.rs) is left
//! as it is: none of these replaces or repeats it. Every value here is an engineering
//! choice made by eye, not a fact about the original game.

use crate::fly::FlyCam;
use crate::sky::{SkyState, Sun};
use crate::ssao::AmbientOcclusion;
use bevy::light::DirectionalLightShadowMap;
use crate::post::PostEffects;
use crate::shafts::LightShafts;
use bevy::prelude::*;
use bevy::render::camera::{MipBias, TemporalJitter};

/// The look as a whole (`--graphics`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Preset {
    /// The picture the parity and picture checks compare: nothing of this module.
    Vanilla,
    /// Every improvement of this module on.
    Enhanced,
}

/// The default of `--graphics` for normal play (test runs default to vanilla; see
/// `main.rs` `graphics_preset`). Vanilla: on the owner's PC (GTX 1050) the enhanced look
/// cost about a third of the frame rate at Goodsprings (docs/design/graphics-enhanced.md
/// §5), so it is asked for with `--graphics enhanced` or switched on in the settings.
pub const PLAY_DEFAULT: Preset = Preset::Vanilla;

/// Which improvements are on (the settings page switches each).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Graphics {
    pub ambient_occlusion: bool,
    pub fine_shadows: bool,
    pub bloom: bool,
    pub sharpen: bool,
    pub anisotropic: bool,
    pub taa: bool,
    pub light_shafts: bool,
    pub water_reflections: bool,
}

/// The settings page's keys of the switches, in the order of [`Graphics::describe`].
pub const KEYS: [&str; 8] = ["ambient-occlusion", "fine-shadows", "bloom", "sharpen", "anisotropic", "taa", "light-shafts", "water-reflections"];

impl Graphics {
    pub fn preset(p: Preset) -> Self {
        let on = p == Preset::Enhanced;
        Graphics {
            ambient_occlusion: on,
            fine_shadows: on,
            bloom: on,
            sharpen: on,
            anisotropic: on,
            // Temporal anti-aliasing is the other choice to multisampling, not part of the
            // enhanced look: it softens textures while the view moves, and the owner plays
            // with 8x multisampling (docs/design/graphics-enhanced.md §3.6).
            taa: false,
            light_shafts: on,
            water_reflections: on,
        }
    }

    /// The switch with this settings key.
    pub fn switch(&mut self, key: &str) -> Option<&mut bool> {
        Some(match key {
            "ambient-occlusion" => &mut self.ambient_occlusion,
            "fine-shadows" => &mut self.fine_shadows,
            "bloom" => &mut self.bloom,
            "sharpen" => &mut self.sharpen,
            "anisotropic" => &mut self.anisotropic,
            "taa" => &mut self.taa,
            "light-shafts" => &mut self.light_shafts,
            "water-reflections" => &mut self.water_reflections,
            _ => return None,
        })
    }

    /// Exactly the improvements named (the settings page's keys) on (`--graphics-only`).
    pub fn only(keys: &[String]) -> Result<Self, String> {
        let mut g = Graphics::preset(Preset::Vanilla);
        for k in keys.iter().map(|k| k.trim()).filter(|k| !k.is_empty()) {
            match g.switch(k) {
                Some(on) => *on = true,
                None => return Err(format!("--graphics-only {k}: expected one of {}", KEYS.join(", "))),
            }
        }
        Ok(g)
    }

    /// The switches that are on, for the log.
    pub fn describe(&self) -> String {
        let names = [
            (self.ambient_occlusion, "ambient occlusion"),
            (self.fine_shadows, "finer, longer shadows"),
            (self.bloom, "bloom"),
            (self.sharpen, "sharpening"),
            (self.anisotropic, "16x anisotropic filtering"),
            (self.taa, "temporal anti-aliasing"),
            (self.light_shafts, "sun shafts"),
            (self.water_reflections, "screen-space water reflections"),
        ];
        let on: Vec<&str> = names.iter().filter(|(on, _)| *on).map(|(_, n)| *n).collect();
        if on.is_empty() {
            "vanilla (no engine-level improvements)".to_string()
        } else {
            on.join(", ")
        }
    }
}

/// Shadow map side with finer shadows, and without (Bevy's default).
const FINE_SHADOW_MAP: usize = 4096;
const SHADOW_MAP: usize = 2048;
/// With finer shadows they reach this many times `--shadow-distance`.
pub const FINE_SHADOW_REACH: f32 = 1.5;

/// The multisampling of the main view from `--msaa`, put back when temporal
/// anti-aliasing is switched off.
#[derive(Resource, Clone, Copy)]
pub struct MsaaSetting(pub Msaa);

/// Test aid for measuring (`--log-frame-times`, `--graphics-cycle`): log the frame time
/// every `every` seconds of real time and then, with a cycle, switch to its next set of
/// improvements (the first half second after a switch, when pipelines compile, is left
/// out of the figures). A set may also change the sun's shadows: `no-sun-shadows`,
/// `shadow-distance=<units>`, `cascades=<n>` (otherwise as at the start).
#[derive(Clone, Default)]
pub struct FrameTimes {
    pub every: f32,
    pub cycle: Vec<CycleStep>,
}

/// One set of `--graphics-cycle`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CycleStep {
    pub graphics: Graphics,
    /// `no-sun-shadows` was among the keys: the sun casts no shadows during this set.
    pub no_sun_shadows: bool,
    /// `shadow-distance=<units>`.
    pub shadow_distance: Option<f32>,
    /// `cascades=<n>`.
    pub cascades: Option<usize>,
}

impl FrameTimes {
    /// `--graphics-cycle`: sets separated by `;`, each `vanilla`, `enhanced` or keys as
    /// for `--graphics-only`, any of them with `no-sun-shadows` among its keys.
    pub fn parse_cycle(text: &str) -> Result<Vec<CycleStep>, String> {
        text.split(';').map(str::trim).filter(|t| !t.is_empty()).map(|t| {
            let keys: Vec<String> = t.split(',').map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).collect();
            let no_sun_shadows = keys.iter().any(|k| k == "no-sun-shadows");
            let value = |name: &str| {
                keys.iter()
                    .find_map(|k| k.strip_prefix(name))
                    .map(|v| v.parse::<f32>().map_err(|e| format!("--graphics-cycle {name}{v}: {e}")))
                    .transpose()
            };
            let shadow_distance = value("shadow-distance=")?;
            let cascades = value("cascades=")?.map(|n| n as usize);
            let keys: Vec<String> = keys.iter().filter(|k| *k != "no-sun-shadows" && !k.contains('=')).cloned().collect();
            let graphics = match keys.as_slice() {
                [] => Graphics::preset(Preset::Vanilla),
                [k] if k == "vanilla" => Graphics::preset(Preset::Vanilla),
                [k] if k == "enhanced" => Graphics::preset(Preset::Enhanced),
                _ => Graphics::only(&keys)?,
            };
            Ok(CycleStep { graphics, no_sun_shadows, shadow_distance, cascades })
        }).collect()
    }
}

/// The improvements to start with, `--msaa` and the frame-time test aid.
pub struct GraphicsPlugin(pub Graphics, pub Msaa, pub Option<FrameTimes>);

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        let g = self.0;
        nvf_assets::texture::set_anisotropy(if g.anisotropic { 16 } else { 8 });
        app.insert_resource(g)
            .insert_resource(MsaaSetting(self.1))
            .insert_resource(DirectionalLightShadowMap { size: if g.fine_shadows { FINE_SHADOW_MAP } else { SHADOW_MAP } })
            .add_plugins((crate::ssao::SsaoPlugin, crate::post::PostPlugin, crate::taa::TaaPlugin, crate::shafts::ShaftsPlugin))
            .add_systems(Startup, |g: Res<Graphics>| info!("graphics: {}", g.describe()))
            .add_systems(Update, apply);
        if let Some(ft) = self.2.clone().filter(|f| f.every > 0.0) {
            // GPU time per render pass (timestamp queries), logged with the frame time.
            app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
            // (time in this period, time counted, slowest, frames counted, cycle index)
            #[allow(clippy::type_complexity)]
            app.add_systems(Update, move |mut commands: Commands, time: Res<Time<Real>>, mut g: ResMut<Graphics>, sky: Res<SkyState>, store: Res<bevy::diagnostic::DiagnosticsStore>, lights: Query<(Option<&Name>, &DirectionalLight, &bevy::camera::visibility::CascadesVisibleEntities)>, views: Query<Option<&Name>, With<Camera>>, mut acc: Local<(f32, f32, f32, u32, usize)>, mut start_shadows: Local<Option<(bool, f32, usize)>>| {
                let first = start_shadows.is_none();
                let start = *start_shadows.get_or_insert((sky.shadows, sky.shadow_distance, sky.shadow_cascades));
                if first {
                    if let Some(step) = ft.cycle.first() {
                        apply_shadow_step(&mut commands, &sky, start, step);
                    }
                }
                let dt = time.delta_secs();
                acc.0 += dt;
                if acc.0 > 0.5 || ft.cycle.is_empty() {
                    acc.1 += dt;
                    acc.2 = acc.2.max(dt);
                    acc.3 += 1;
                }
                if acc.0 >= ft.every {
                    info!("frame time: {:.2} ms mean, {:.1} ms slowest over {} frames; graphics: {}; sun shadows {}; sun at heading {:.0}, elevation {:.0}", acc.1 * 1e3 / acc.3.max(1) as f32, acc.2 * 1e3, acc.3, g.describe(), if sky.shadows { format!("to {:.0} in {} cascades", sky.shadow_distance, sky.shadow_cascades) } else { "off".into() }, sky.sun_heading(), sky.sun_elevation());
                    // The GPU's time in the passes that cost the most (ms, about the last
                    // second's average).
                    let mut passes: Vec<(String, f64)> = store
                        .iter()
                        .filter(|d| d.path().as_str().starts_with("render/") && d.path().as_str().ends_with("/elapsed_gpu"))
                        .filter_map(|d| Some((d.path().as_str().trim_start_matches("render/").trim_end_matches("/elapsed_gpu").to_string(), d.average()?)))
                        .collect();
                    passes.sort_by(|a, b| b.1.total_cmp(&a.1));
                    let total: f64 = passes.iter().filter(|(n, _)| !n.contains('/')).map(|(_, t)| t).sum();
                    let list: Vec<String> = passes.iter().filter(|(n, _)| !n.contains('/')).take(12).map(|(n, t)| format!("{n} {t:.2}")).collect();
                    info!("gpu time: {total:.2} ms in all top-level passes; {}", list.join(", "));
                    // What the shadow passes draw (Bevy times no shadow pass): the casters
                    // in each cascade of each light with shadows, per view.
                    let mut casters = Vec::new();
                    for (name, light, cascades) in &lights {
                        if !light.shadow_maps_enabled {
                            continue;
                        }
                        let name = name.map_or("light", |n| n.as_str());
                        for (view, per_cascade) in &cascades.entities {
                            let view = views.get(*view).ok().flatten().map_or("view", |n| n.as_str());
                            let counts: Vec<String> = per_cascade.iter().map(|c| c.entities.len().to_string()).collect();
                            casters.push(format!("{name} for {view}: [{}]", counts.join(", ")));
                        }
                    }
                    info!("shadow casters per cascade: {}", if casters.is_empty() { "none (sun shadows off)".to_string() } else { casters.join("; ") });
                    let next = acc.4 + 1;
                    *acc = (0.0, 0.0, 0.0, 0, next);
                    if !ft.cycle.is_empty() {
                        let step = ft.cycle[next % ft.cycle.len()];
                        *g = step.graphics;
                        apply_shadow_step(&mut commands, &sky, start, &step);
                    }
                }
            });
        }
    }
}

/// The sun's shadows of a `--graphics-cycle` set: `start` (on, distance, cascades) as at
/// the start, changed by the set's keys.
fn apply_shadow_step(commands: &mut Commands, sky: &SkyState, start: (bool, f32, usize), step: &CycleStep) {
    let on = start.0 && !step.no_sun_shadows;
    let distance = step.shadow_distance.unwrap_or(start.1);
    let cascades = step.cascades.unwrap_or(start.2);
    if on != sky.shadows || distance != sky.shadow_distance || cascades != sky.shadow_cascades {
        commands.queue(move |w: &mut World| {
            let mut sky = w.resource_mut::<SkyState>();
            sky.shadow_distance = distance;
            sky.shadow_cascades = cascades;
            crate::sky::set_shadows(w, on);
        });
    }
}

/// Put the improvements that are on onto the main camera and the sun, take those that are
/// off away (when a switch changes, and once at the start).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn apply(
    mut commands: Commands,
    g: Res<Graphics>,
    msaa: Res<MsaaSetting>,
    mut sky: ResMut<SkyState>,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
    cams: Query<(Entity, &Msaa), With<FlyCam>>,
    suns: Query<(Entity, &DirectionalLight), With<Sun>>,
    mut done: Local<Option<(Graphics, usize)>>,
) {
    let cam_count = cams.iter().count();
    let suns_count = suns.iter().count();
    if cam_count == 0 || suns_count == 0 {
        return;
    }
    if done.as_ref().is_some_and(|(d, n)| *d == *g && *n == cam_count + suns_count) {
        return;
    }
    let first = done.is_none();
    *done = Some((*g, cam_count + suns_count));
    for (cam, cam_msaa) in &cams {
        let mut e = commands.entity(cam);
        if g.ambient_occlusion {
            e.insert(AmbientOcclusion::default());
        } else {
            e.remove::<AmbientOcclusion>();
        }
        if g.bloom || g.sharpen {
            e.insert(PostEffects::new(g.bloom, g.sharpen));
        } else {
            e.remove::<PostEffects>();
        }
        // Temporal anti-aliasing replaces the multisampling.
        let want_msaa = if g.taa { Msaa::Off } else { msaa.0 };
        if *cam_msaa != want_msaa {
            e.insert(want_msaa);
        }
        if g.taa {
            e.insert(crate::taa::components());
        } else {
            e.remove::<(crate::taa::TemporalAa, TemporalJitter, MipBias)>();
        }
        if g.light_shafts {
            e.insert(LightShafts::default());
        } else {
            e.remove::<LightShafts>();
        }
    }
    let size = if g.fine_shadows { FINE_SHADOW_MAP } else { SHADOW_MAP };
    if shadow_map.size != size {
        shadow_map.size = size;
    }
    let reach = if g.fine_shadows { FINE_SHADOW_REACH } else { 1.0 };
    let reach_changed = sky.shadow_reach != reach;
    if reach_changed {
        sky.shadow_reach = reach;
    }
    for (e, light) in &suns {
        if light.shadow_maps_enabled && (reach_changed || first) {
            commands.entity(e).insert(crate::sky::shadow_cascades(sky.shadow_distance, sky.shadow_reach, sky.shadow_cascades));
        }
    }
    if !first {
        info!("graphics: {}", g.describe());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_a_switch_and_enhanced_leaves_taa_off() {
        let all = Graphics::only(&KEYS.map(String::from)).expect("keys");
        // "finer, longer shadows" has a comma of its own.
        assert_eq!(all.describe().split(", ").count(), KEYS.len() + 1);
        let enhanced = Graphics::preset(Preset::Enhanced);
        assert!(!enhanced.taa && enhanced.light_shafts && enhanced.water_reflections);
        assert!(Graphics::only(&["nonsense".into()]).is_err());
        let cycle = FrameTimes::parse_cycle("vanilla; taa,light-shafts ;enhanced;vanilla,no-sun-shadows;shadow-distance=8000,cascades=3").expect("cycle");
        let graphics: Vec<Graphics> = cycle.iter().map(|c| c.graphics).collect();
        let vanilla = Graphics::preset(Preset::Vanilla);
        assert_eq!(graphics, vec![vanilla, Graphics::only(&["taa".into(), "light-shafts".into()]).unwrap(), enhanced, vanilla, vanilla]);
        assert_eq!(cycle.iter().map(|c| c.no_sun_shadows).collect::<Vec<_>>(), vec![false, false, false, true, false]);
        assert_eq!((cycle[4].shadow_distance, cycle[4].cascades), (Some(8000.0), Some(3)));
    }
}
