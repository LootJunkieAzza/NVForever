//! A weapon's scope while it is up (`nvf_play::aim_zoom::Scope`,
//! docs/spec-notes/weapon-scopes.md): its picture, its night vision and its sway.
//!
//! - **The picture** (§3): the weapon's MOD3 model (`interface\HUD\*.nif`) drawn by an
//!   orthographic camera of its own looking along the model's +Y, [`VIEW_HEIGHT`] model
//!   units high [hypothesis: the black mask's rectangle, 1480 × 920, is the screen at
//!   16:10], on a render layer of its own, into a picture shown full-screen under the HUD
//!   [design]. Black bars stand beyond the mask's sides on wider screens [design]. The
//!   materials are the model's own.
//! - **Night vision** (§4): a night-vision scope holds `NightScopeISFX` on the screen at its
//!   first keys in an interior or at night [documented: GECK wiki "Weapons"; hypothesis:
//!   the record, and night as sunset's end to sunrise's start].
//! - **Sway** (§5): `scopewobble.nif`'s 5 s loop of the `Camera1st` node turns the view,
//!   scaled so its widest point is half of PSp × 5.955° [documented: GECK wiki "Gun Spread
//!   Formula (New Vegas)"] × `fGunWobbleMultScope`, × `fNonAttackGunWobbleMult` while the
//!   player is not attacking [documented: the same page; hypothesis: the curve's meaning
//!   and the axes, X up and down, Z sideways]. The shots follow the view.

use crate::imagespace::ScreenModifiers;
use crate::sky::SkyState;
use crate::world::WorldData;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::AmbientLight;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use fnv_nif::anim::TransformTrack;
use nvf_assets::data::GameData;
use nvf_assets::model::ModelCtx;
use nvf_play::aim_zoom::Scope;
use nvf_play::combat::Combat;
use nvf_play::player::{Player, MAX_PITCH};
use std::sync::Arc;

/// The render layer of the scope's picture and its camera.
pub const SCOPE_LAYER: usize = 13;
/// Where the picture is put, far from the world and the other menus' models (Bevy space)
/// [design].
const PLACE: Vec3 = Vec3::new(0.0, -220_000.0, 0.0);
/// Model units across the screen's height [hypothesis, module docs].
pub const VIEW_HEIGHT: f32 = 920.0;
/// The mask's half width: black beyond it [observed: `keepScope01:0` spans ±740].
const MASK_HALF_WIDTH: f32 = 740.0;
/// The night-vision scope's image space modifier, by Editor ID [hypothesis, §4].
pub const NIGHT_SCOPE_IMAD: &str = "NightScopeISFX";
/// The scope's wobble [observed, §5].
pub const WOBBLE_MODEL: &str = "meshes\\characters\\weaponwobbles\\scopewobble.nif";
/// Spread units to degrees of the sway circle's diameter [documented, §5].
pub const SPREAD_TO_DEG: f32 = 5.955;
/// A shot within this many seconds counts as attacking (full sway) [hypothesis].
const ATTACK_SECONDS: f32 = 0.5;

pub struct ScopeViewPlugin;

impl Plugin for ScopeViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                show.after(nvf_play::PlaySet::Ui).before(nvf_play::PlaySet::Sync),
                night_vision.after(nvf_play::PlaySet::Sync),
                sway.in_set(nvf_play::PlaySet::Activation).run_if(resource_exists::<Combat>),
            ),
        );
    }
}

/// The game path of a MOD3 model (stored without `meshes\`).
pub fn model_path(mod3: &str) -> String {
    let p = mod3.replace('/', "\\");
    if p.to_ascii_lowercase().starts_with("meshes\\") {
        p
    } else {
        format!("meshes\\{p}")
    }
}

/// The time of day counts as night: from the end of sunset to the start of sunrise
/// (hours) [hypothesis, §4].
pub fn is_night(hour: f32, sunrise_begin: f32, sunset_end: f32) -> bool {
    let h = hour.rem_euclid(24.0);
    h >= sunset_end || h < sunrise_begin
}

/// The picture as drawn: its entities, the model and the window size it was built for.
#[derive(Default)]
struct Drawn {
    entities: Vec<Entity>,
    model: Option<String>,
    size: UVec2,
    warned: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    scope: Res<Scope>,
    mut models: ModelCtx,
    mut plain: ResMut<Assets<StandardMaterial>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut drawn: Local<Drawn>,
) {
    let size = window.single().map(|w| UVec2::new(w.physical_width().max(1), w.physical_height().max(1))).unwrap_or(UVec2::new(1280, 720));
    let want = scope.model.as_deref().filter(|_| scope.on).map(model_path);
    if drawn.model.is_some() && (drawn.model != want || drawn.size != size) {
        for e in drawn.entities.drain(..) {
            commands.entity(e).despawn();
        }
        drawn.model = None;
    }
    let Some(path) = want else { return };
    if drawn.model.is_some() {
        return;
    }
    let Some(model) = models.model(&path) else {
        if drawn.warned.as_deref() != Some(&path) {
            warn!("scope view: {path} not loaded; the scope is shown without its picture");
            drawn.warned = Some(path);
        }
        return;
    };
    let mut image = Image::new_fill(Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 }, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = models.images.add(image);
    let layer = RenderLayers::layer(SCOPE_LAYER);
    // Game space (Z up) into Bevy's (Y up), as the world root does.
    let place = Affine3A::from_translation(PLACE) * Affine3A::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let to = |p: Vec3| place.transform_point3(p);
    let mut entities = Vec::new();
    for s in &model.shapes {
        let world = place * model.root * s.transform;
        let name = s.name.as_deref().unwrap_or("");
        entities.push(commands.spawn((Name::new(format!("Scope {name}")), Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(world)), layer.clone())).id());
    }
    // Black beyond the mask's sides, for screens wider than 16:10.
    let bar = models.meshes.add(Rectangle::new(4.0 * VIEW_HEIGHT, 2.0 * VIEW_HEIGHT));
    let black = plain.add(StandardMaterial { base_color: Color::BLACK, unlit: true, ..default() });
    for side in [-1.0f32, 1.0] {
        let at = to(Vec3::new(side * (MASK_HALF_WIDTH + 2.0 * VIEW_HEIGHT - 1.0), 0.0, 0.0));
        entities.push(commands.spawn((Name::new("Scope side bar"), Mesh3d(bar.clone()), MeshMaterial3d(black.clone()), Transform::from_translation(at), layer.clone())).id());
    }
    let eye = Vec3::new(0.0, -1000.0, 0.0);
    entities.push(
        commands
            .spawn((
                Name::new("Scope camera"),
                Camera3d::default(),
                Camera { order: -5, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
                RenderTarget::Image(ImageRenderTarget::from(image.clone())),
                Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::FixedVertical { viewport_height: VIEW_HEIGHT }, near: 0.0, far: 3000.0, ..OrthographicProjection::default_3d() }),
                Tonemapping::None,
                Msaa::Sample4,
                AmbientLight { color: Color::WHITE, brightness: 500.0, ..default() },
                Transform::from_translation(to(eye)).looking_at(to(Vec3::ZERO), Vec3::Y),
                layer.clone(),
            ))
            .id(),
    );
    // Under the HUD (whose nodes sit at the default z index), over the world.
    entities.push(
        commands
            .spawn((
                Name::new("Scope picture"),
                Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
                ImageNode::new(image),
                GlobalZIndex(-1),
                Pickable::IGNORE,
            ))
            .id(),
    );
    info!("scope view: {path} drawn ({} shapes), {VIEW_HEIGHT} units high", model.shapes.len());
    drawn.entities = entities;
    drawn.model = Some(path);
    drawn.size = size;
}

/// Hold the night-vision scope's modifier on the screen while it gives night vision.
fn night_vision(scope: Res<Scope>, sky: Option<Res<SkyState>>, world: Option<Res<WorldData>>, mut mods: ResMut<ScreenModifiers>, mut state: Local<(Option<Option<u32>>, bool)>) {
    let night = sky.as_deref().is_some_and(|s| s.interior.is_some() || is_night(s.hour, s.climate.sunrise.0, s.climate.sunset.1));
    let on = scope.on && scope.night_vision && night;
    if on == state.1 {
        return;
    }
    state.1 = on;
    let Some(world) = world else { return };
    let form = *state.0.get_or_insert_with(|| fnv_world::image_modifier::find_image_modifier(world.lo, world.schema, NIGHT_SCOPE_IMAD));
    match form {
        Some(f) => {
            mods.hold(f, on);
            info!("scope: night vision {NIGHT_SCOPE_IMAD} {}", if on { "on" } else { "off" });
        }
        None if on => warn!("scope: no image space modifier {NIGHT_SCOPE_IMAD}; the night-vision scope shows no night vision"),
        None => {}
    }
}

/// The wobble's curves: the track, its span (seconds) and its widest point (radians).
struct Wobble {
    track: Arc<TransformTrack>,
    start: f32,
    stop: f32,
    peak: f32,
}

impl Wobble {
    fn load(data: &GameData) -> Option<Wobble> {
        let bytes = data.read(WOBBLE_MODEL)?;
        let file = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)).ok()?;
        let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
        let (_, track, cycle) = nvf_assets::model::free_transform_tracks(&file, &dec).into_iter().find(|(n, ..)| n.eq_ignore_ascii_case("Camera1st"))?;
        let mut w = Wobble { track, start: cycle.start, stop: cycle.stop, peak: 0.0 };
        if !(w.start.is_finite() && w.stop.is_finite()) {
            return None;
        }
        let widths: Vec<f32> = (0..=200).map(|k| w.angles(k as f32 / 200.0)).map(|[x, z]| x.hypot(z)).collect();
        // A broken file's NaN or infinite turn would reach the player's view: no sway.
        if !widths.iter().all(|v| v.is_finite()) {
            return None;
        }
        w.peak = widths.into_iter().fold(0.0, f32::max);
        (w.peak > 0.0).then_some(w)
    }

    /// The node's turn about X and Z (radians) at `share` (0–1) of the loop.
    fn angles(&self, share: f32) -> [f32; 2] {
        let t = self.start + (self.stop - self.start) * share.rem_euclid(1.0);
        if let Some(e) = &self.track.euler {
            let at = |k: usize| e[k].sample(t).map_or(0.0, |v| v[0]);
            return [at(0), at(2)];
        }
        let [w, x, y, z] = self.track.rotation_at(t).unwrap_or([1.0, 0.0, 0.0, 0.0]);
        let (ex, _, ez) = Quat::from_xyzw(x, y, z, w).to_euler(EulerRot::XYZ);
        [ex, ez]
    }

    /// The view's turn (yaw, pitch; degrees) `t` seconds into the loop, scaled so the
    /// widest point is `radius` degrees.
    fn turn(&self, t: f32, radius: f32) -> [f32; 2] {
        let span = (self.stop - self.start).max(0.01);
        let [x, z] = self.angles(t / span);
        [z / self.peak * radius, x / self.peak * radius]
    }
}

#[derive(Default)]
struct SwayState {
    wobble: Option<Option<Wobble>>,
    t: f32,
    last: [f32; 2],
    told: bool,
}

/// Sway the view while the scope is up (§5).
#[allow(clippy::too_many_arguments)]
fn sway(
    time: Res<Time>,
    scope: Res<Scope>,
    combat: Res<Combat>,
    sim: Option<Res<nvf_play::sim::SimState>>,
    data: Option<Res<GameData>>,
    mut player: ResMut<Player>,
    mut st: Local<SwayState>,
) {
    let want = if scope.on {
        if st.wobble.is_none() {
            // A panic reading a broken file counts as no wobble.
            let w = data.as_deref().and_then(|d| nvf_assets::guard::guarded(|| Ok(Wobble::load(d))).unwrap_or_else(|e| {
                warn!("scope sway: {WOBBLE_MODEL} not read: {e}");
                None
            }));
            if w.is_none() {
                warn!("scope sway: {WOBBLE_MODEL} has no Camera1st rotation; scopes do not sway");
            }
            st.wobble = Some(w);
        }
        let setting = |name: &str, or: f32| sim.as_deref().and_then(|s| s.forms.setting_f32(name)).unwrap_or(or);
        let spread = combat.player_spread.unwrap_or(0.0);
        let attacking = combat.since_last_shot() < ATTACK_SECONDS;
        let radius = spread * SPREAD_TO_DEG / 2.0 * setting("fGunWobbleMultScope", 1.0) * if attacking { 1.0 } else { setting("fNonAttackGunWobbleMult", 0.5) };
        if !st.told {
            st.told = true;
            info!("scope sway: PSp {spread:.3}, radius {radius:.2}°");
        }
        st.t += time.delta_secs();
        let t = st.t;
        st.wobble.as_ref().and_then(Option::as_ref).map_or([0.0, 0.0], |w| w.turn(t, radius))
    } else {
        st.t = 0.0;
        st.told = false;
        [0.0, 0.0]
    };
    if want == st.last {
        return;
    }
    player.heading += (want[0] - st.last[0]).to_radians();
    player.pitch = (player.pitch + (want[1] - st.last[1]).to_radians()).clamp(-MAX_PITCH, MAX_PITCH);
    st.last = want;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night_runs_from_sunset_end_to_sunrise_begin() {
        assert!(is_night(23.0, 6.0, 20.0));
        assert!(is_night(2.0, 6.0, 20.0));
        assert!(is_night(20.0, 6.0, 20.0));
        assert!(!is_night(6.0, 6.0, 20.0));
        assert!(!is_night(12.0, 6.0, 20.0));
        assert!(is_night(26.0, 6.0, 20.0));
    }

    #[test]
    fn mod3_paths_gain_the_meshes_folder_once() {
        assert_eq!(model_path("interface\\HUD\\mildotscope.NIF"), "meshes\\interface\\HUD\\mildotscope.NIF");
        assert_eq!(model_path("Meshes\\interface\\hud\\x.nif"), "Meshes\\interface\\hud\\x.nif");
    }
}
