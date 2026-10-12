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
//!
//! The game's own image space (the grade and the modifiers' blur, imagespace.rs) is left
//! as it is: none of these replaces or repeats it. Every value here is an engineering
//! choice made by eye, not a fact about the original game.

use crate::fly::FlyCam;
use crate::sky::{SkyState, Sun};
use crate::ssao::AmbientOcclusion;
use bevy::light::DirectionalLightShadowMap;
use crate::post::PostEffects;
use bevy::prelude::*;

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
}

impl Graphics {
    pub fn preset(p: Preset) -> Self {
        let on = p == Preset::Enhanced;
        Graphics { ambient_occlusion: on, fine_shadows: on, bloom: on, sharpen: on, anisotropic: on }
    }

    /// Exactly the improvements named (the settings page's keys) on (`--graphics-only`).
    pub fn only(keys: &[String]) -> Result<Self, String> {
        let mut g = Graphics::preset(Preset::Vanilla);
        for k in keys.iter().map(|k| k.trim()).filter(|k| !k.is_empty()) {
            match k {
                "ambient-occlusion" => g.ambient_occlusion = true,
                "fine-shadows" => g.fine_shadows = true,
                "bloom" => g.bloom = true,
                "sharpen" => g.sharpen = true,
                "anisotropic" => g.anisotropic = true,
                other => return Err(format!("--graphics-only {other}: expected ambient-occlusion, fine-shadows, bloom, sharpen or anisotropic")),
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

pub struct GraphicsPlugin(pub Graphics);

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        let g = self.0;
        nvf_assets::texture::set_anisotropy(if g.anisotropic { 16 } else { 8 });
        app.insert_resource(g)
            .insert_resource(DirectionalLightShadowMap { size: if g.fine_shadows { FINE_SHADOW_MAP } else { SHADOW_MAP } })
            .add_plugins((crate::ssao::SsaoPlugin, crate::post::PostPlugin))
            .add_systems(Startup, |g: Res<Graphics>| info!("graphics: {}", g.describe()))
            .add_systems(Update, apply);
    }
}

/// Put the improvements that are on onto the main camera and the sun, take those that are
/// off away (when a switch changes, and once at the start).
#[allow(clippy::type_complexity)]
fn apply(
    mut commands: Commands,
    g: Res<Graphics>,
    mut sky: ResMut<SkyState>,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
    cams: Query<Entity, With<FlyCam>>,
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
    for cam in &cams {
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
            commands.entity(e).insert(crate::sky::shadow_cascades(sky.shadow_distance, sky.shadow_reach));
        }
    }
    if !first {
        info!("graphics: {}", g.describe());
    }
}
