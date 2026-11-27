//! The flashlight's light (`nvf_play::flashlight`; owner request 2026-10-11): while it is
//! on, a spot light on the front of the player's body lights a cone ahead of the view.
//! [design: not a light of the game's; colour, reach, cone and brightness are chosen to
//! look like other games' flashlights, in the display units of `lights::spawn_light`.]
//!
//! - It sits on the body, in front of the face a little under the eye, facing where the
//!   player looks (owner, playtest list 5 items 66 and 67): in a view from outside it
//!   lights what is ahead of the body, not the body's back from the camera, and nearly on
//!   the line of the eye its shadows fall behind what casts them instead of beside it.
//! - In a view from outside it is placed from the third-person head's eye
//!   (`PlayerBody::head_eye`), so it moves with the head: from the first-person eye the
//!   head leaned into it walking and running and shaded the cone (owner 2026-10-18). In
//!   first person the head casts nothing on it (`sky::SHADOW_LAYER`) and the view's eye
//!   places it.
//! - It casts shadows: walls stop it instead of it lighting the rooms behind them. It
//!   falls off as the game's lights do (`lights.rs`).
//! - Shadow casters are the world's layer (0), as the light's own layers are: the player
//!   casts none (owner 2026-10-22: aiming in third person the arms shaded the cone). The
//!   first-person arms and weapon (`sky::ARMS_LAYER`), the seen third-person body and its
//!   weapon (`sky::BODY_LAYER`) and the unseen body (`sky::SHADOW_LAYER`) are on layers
//!   of their own. It still lights them: a light lights every view its layers meet. On
//!   first-person shapes (`FIRST_PERSON_DEPTH`, `lights/display_shader.rs`) the flashlight
//!   is scaled down (`FIRST_PERSON_FLASHLIGHT_DIFFUSE`, `FIRST_PERSON_FLASHLIGHT_SPECULAR`)
//!   [design, #3121] so aiming down sights into the beam's inner core does not wash the
//!   held weapon and hand out to white, whatever its Type and Shadows rows (the shader
//!   knows it by its mark, [`made`]), while world surfaces, third-person body/weapon
//!   (`BODY_LAYER`) and the world's lights (`LIGH`) keep full strength.
//! - Detection does not count it (`detection.rs`).
//! - The Tweaks tab's Flashlight rows ([`FlashlightLook`], registered in `settings.rs`)
//!   set its colour, its type (Bevy's `SpotLight` or `PointLight`), whether it casts
//!   shadows, its reach, its brightness and a spot's beam; at their defaults it is the
//!   light above [design: not in the original, whose Pip-Boy light is no such light; the
//!   rows are those flashlight mods of other games commonly offer].
//! - Another co-op player's light (`coop.rs` `peer_lights`, [`peer_light`]) is that player's
//!   own: their rows go with their state ([`FlashlightLook::wire`], the colour already
//!   their Pip-Boy's when it says Pip-Boy; the INI's values are not sent) and their puppet
//!   gets the same light, placed from its eye as this one is (#3279). The puppet is drawn on
//!   the world's layer 0, so its head, arms and weapon would cast into its own cone, as the
//!   player's own body did before it got `sky::BODY_LAYER`: a dark wedge swinging over the
//!   lit ground. Its shadow map starts at [`REMOTE_NEAR`] instead, beyond what the puppet
//!   holds in front of it; walls farther than that still stop it. A point light's shadow
//!   map starts there too: the puppet's legs, below that, still shade the ground around its
//!   feet, as a lantern held at the head would [design]. Detection counts it for that
//!   player, not this one (`detection.rs`). It is sent only while co-op is on.
//! - `BetterFlashlightNVSE.ini` (#3727, `flashlight_ini.rs`, while active): its battery,
//!   warm-up, flicker, weapon attach and offsets, and its colour, reach, cone and
//!   interior and exterior brightness. **The Tweaks rows win** where both set the same
//!   thing ([`with_ini`]): a Flashlight row the player has moved off its default keeps its
//!   value over the INI's, because the player set it in game; rows at their defaults
//!   take the INI's [design: #3280 review, maintainer decision 2026-11-24]. The INI has no
//!   type, so that is the rows' alone, and shadows follow the Shadows row.
//! - Its shadow depth bias is [`SPOT_DEPTH_BIAS`] / [`POINT_DEPTH_BIAS`] (Bevy's default
//!   times 70, game units) for every flashlight, at default rows too: before the rows the
//!   spot used Bevy's 0.02, a visible change for every player that removes the shadow
//!   stripes (#3279) [design].

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use nvf_play::combat::WeaponAnim;
use nvf_play::flashlight::Flashlight;
use nvf_play::pause::Paused;
use nvf_play::player::Player;
use nvf_play::save::SaveLog;
use nvf_play::script::NewGame;
use nvf_play::{Location, PlayState};
use crate::flashlight_ini::{BatteryConfig, BetterFlashlightConfig};
use nvf_coop::guard::{LIGHT_BRIGHTNESS_MAX, LIGHT_HALF_ANGLE_MAX, LIGHT_RANGE_MAX};
use nvf_coop::net::{light_kind, LightLook};
use crate::player_body::PlayerBody;
use crate::sky::SkyState;

/// Marks the flashlight's light entity.
#[derive(Component)]
pub struct PlayerFlashlight;

/// Marks another co-op player's flashlight (`coop.rs`), by their player id, and the look it
/// was made with: a change makes it again.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PeerFlashlight(pub u8, pub LightLook);

/// The light a flashlight entity was made with: a change makes it again.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct MadeWith(Beam);

/// Runtime state for `BetterFlashlightNVSE.ini` battery drain/recharge, warm-up ramp,
/// and sporadic flicker (unused when the INI is absent or disabled).
#[derive(Resource, Debug, Clone)]
pub struct FlashlightRuntime {
    /// Battery charge in `0.0..=100.0` (starts full at `100.0`).
    pub charge: f32,
    /// True after hitting `0.0` until `charge >= recharge_threshold`.
    pub depleted_lockout: bool,
    /// Seconds elapsed since the flashlight was last switched on.
    pub on_elapsed: f32,
    was_on: bool,
    seen_loads: usize,
    seen_new_game: bool,
}

impl Default for FlashlightRuntime {
    fn default() -> Self {
        Self {
            charge: 100.0,
            depleted_lockout: false,
            on_elapsed: 0.0,
            was_on: false,
            seen_loads: 0,
            seen_new_game: false,
        }
    }
}

impl FlashlightRuntime {
    /// Reset battery charge to 100% and clear depletion lockout (on load or new game).
    pub fn reset_on_load(&mut self) {
        self.charge = 100.0;
        self.depleted_lockout = false;
        self.on_elapsed = 0.0;
        self.was_on = false;
    }
}

/// Warm white, as an incandescent bulb, in the colour sliders' 0 to 255 (1.0, 0.95, 0.86
/// before the rows, to the 255th).
pub const COLOUR_RGB: [u8; 3] = [255, 242, 219];
/// Brightness at the light, display units (`lights::lumens`).
pub const BRIGHTNESS: f32 = 2.0;
/// Reach, game units (about 40 m).
pub const RANGE: f32 = 2800.0;
/// Half-angle of the cone's edge, and of its fully lit middle, degrees.
pub const OUTER_DEG: f32 = 30.0;
const INNER_DEG: f32 = 14.0;

/// Which of Bevy's lights the flashlight is [design].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LightKind {
    /// A cone ahead of the view (`SpotLight`), the built-in flashlight's.
    #[default]
    Spot,
    /// Light all round the player, as a lantern (`PointLight`).
    Point,
}

impl LightKind {
    /// The choices as the settings page lists them, the default first.
    pub const ALL: [LightKind; 2] = [LightKind::Spot, LightKind::Point];

    pub fn name(self) -> &'static str {
        match self {
            LightKind::Spot => "Spot",
            LightKind::Point => "Point",
        }
    }

    pub fn from_name(name: &str) -> Option<LightKind> {
        Self::ALL.into_iter().find(|k| k.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// The Tweaks tab's Flashlight rows (`settings::register_flashlight`): what the light is.
/// `Default`: the light the flashlight had before the rows [design: not in the original].
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct FlashlightLook {
    pub kind: LightKind,
    pub shadows: bool,
    /// Colour, 0 to 255 a channel, as display values (`lights::display_color`).
    pub rgb: [u8; 3],
    /// Whether the light takes the Pip-Boy's colour (`nvf_play::interface::pipboy_rgb`),
    /// following it as it changes, in place of [`Self::rgb`]: the original's Pip-Boy light
    /// is the Pip-Boy's colour [observed: buckallobill, in game, FNV 1.4.0.525, 2026-11-21
    /// (#3279): changed the Pip-Boy colour in the original's display settings, turned the
    /// Pip-Boy light on, and in third person saw the light's colour on the world change to
    /// match]. Off by default [design].
    pub pipboy_colour: bool,
    /// Reach, game units.
    pub range: f32,
    /// Brightness at the light, display units (`lights::lumens`).
    pub brightness: f32,
    /// A spot's half-angle at the edge of its cone, degrees.
    pub outer_deg: f32,
    /// How much of a spot's cone fades to its edge: 0 a hard edge, 1 a fade from its middle.
    pub softness: f32,
}

impl Default for FlashlightLook {
    fn default() -> Self {
        FlashlightLook {
            kind: LightKind::Spot,
            shadows: true,
            rgb: COLOUR_RGB,
            pipboy_colour: false,
            range: RANGE,
            brightness: BRIGHTNESS,
            outer_deg: OUTER_DEG,
            softness: 1.0 - INNER_DEG / OUTER_DEG,
        }
    }
}

/// The light the flashlight is made as.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Beam {
    kind: LightKind,
    shadows: bool,
    colour: [f32; 3],
    range: f32,
    brightness: f32,
    outer_deg: f32,
    inner_deg: f32,
}

impl FlashlightLook {
    /// The light as it goes to the other co-op players, `pipboy` the Pip-Boy's colour now:
    /// the colour they see is the one this game draws. The numbers are kept within what the
    /// host accepts (`nvf_coop::guard`), which are the rows' own tops.
    pub(crate) fn wire(&self, pipboy: [u8; 3]) -> LightLook {
        let num = |v: f32, max: f32| if v.is_finite() { v.clamp(0.0, max) } else { 0.0 };
        LightLook {
            kind: match self.kind {
                LightKind::Spot => light_kind::SPOT,
                LightKind::Point => light_kind::POINT,
            },
            shadows: self.shadows,
            rgb: if self.pipboy_colour { pipboy } else { self.rgb },
            range: num(self.range, LIGHT_RANGE_MAX),
            brightness: num(self.brightness, LIGHT_BRIGHTNESS_MAX),
            outer_deg: num(self.outer_deg, LIGHT_HALF_ANGLE_MAX),
            softness: num(self.softness, 1.0),
        }
    }

    /// Another player's light, as it came ([`Self::wire`]).
    fn from_wire(l: &LightLook) -> FlashlightLook {
        FlashlightLook {
            kind: if l.kind == light_kind::POINT { LightKind::Point } else { LightKind::Spot },
            shadows: l.shadows,
            rgb: l.rgb,
            pipboy_colour: false,
            range: l.range,
            brightness: l.brightness,
            outer_deg: l.outer_deg,
            softness: l.softness,
        }
    }

    /// The light for these rows, `pipboy` the Pip-Boy's colour now.
    fn beam(&self, pipboy: [u8; 3]) -> Beam {
        let outer_deg = self.outer_deg.clamp(1.0, 89.0);
        let rgb = if self.pipboy_colour { pipboy } else { self.rgb };
        Beam {
            kind: self.kind,
            shadows: self.shadows,
            colour: rgb.map(|c| f32::from(c) / 255.0),
            range: self.range.max(1.0),
            brightness: self.brightness.max(0.0),
            outer_deg,
            inner_deg: outer_deg * (1.0 - self.softness.clamp(0.0, 1.0)),
        }
    }
}
/// Where the light sits from the head's eye: ahead along the body's heading, and
/// down, game units [design].
const AHEAD: f32 = 14.0;
const DOWN: f32 = 6.0;
/// Shadow near plane, game units.
const NEAR: f32 = 8.0;
/// Shadow near plane of another player's light, game units: past the puppet's head
/// (the light sits [`AHEAD`] in front of the eye) and the arms and weapon held out
/// aiming, which reach about 60 units ahead of the eye [design: about 0.9 m].
pub(crate) const REMOTE_NEAR: f32 = 64.0;
/// Game units to a metre, as the Pip-Boy's figures and the Range row count them.
pub const UNITS_PER_METRE: f32 = 70.0;
/// The shadows' depth bias, game units: Bevy's defaults put back on the metre they were
/// chosen for. Bevy moves a surface this far towards the light, as a distance in the
/// world, before it tests the shadow map; its defaults (0.08 for a point light, 0.02 for
/// a spot) are metres, about a millimetre in game units, so a lit surface shadowed itself
/// in fine stripes, and the point light's cube faces showed as lighter and darker blocks
/// (#3279) [design]. The normal bias is scaled to the map's texels by
/// Bevy and needs no change.
const POINT_DEPTH_BIAS: f32 = PointLight::DEFAULT_SHADOW_DEPTH_BIAS * UNITS_PER_METRE;
const SPOT_DEPTH_BIAS: f32 = SpotLight::DEFAULT_SHADOW_DEPTH_BIAS * UNITS_PER_METRE;

pub struct FlashlightPlugin;

impl Plugin for FlashlightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlashlightLook>()
            .init_resource::<FlashlightRuntime>()
            .add_systems(Startup, crate::flashlight_ini::load_from_vfs)
            .add_systems(Update, sync.after(nvf_play::PlaySet::Physics));
    }
}

/// The light's place and rotation (Bevy space) for the player's eye (game space),
/// heading and pitch. Another co-op player's light goes on their puppet the same way
/// (`coop.rs`).
pub(crate) fn placement(eye: [f32; 3], heading: f32, pitch: f32) -> Transform {
    let at = [eye[0] + AHEAD * heading.sin(), eye[1] + AHEAD * heading.cos(), eye[2] - DOWN];
    // A spot light looks down its -z, as a camera does.
    Transform::from_translation(nvf_play::space::to_bevy(at)).with_rotation(nvf_play::space::look_rotation(heading, pitch))
}

/// Incandescent warm-up ramp multiplier in `0.0..=1.0` [design].
pub(crate) fn warmup_factor(elapsed: f32, warmup_time: f32) -> f32 {
    if warmup_time <= 0.0 {
        return 1.0;
    }
    let u = (elapsed / warmup_time).clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// Deterministic sporadic-dip flicker multiplier in `0.15..=1.0` [design].
///
/// `0.0` produces a steady `1.0`; larger values increase both dip probability and dip
/// depth while remaining mostly steady with brief dips.
pub(crate) fn flicker_factor(elapsed: f32, strength: f32) -> f32 {
    if strength <= 0.0 {
        return 1.0;
    }
    const SLOT_SECS: f32 = 0.08;
    let t = elapsed.max(0.0);
    let slot_f = (t / SLOT_SECS).floor();
    let slot = slot_f as u64;
    let frac = (t / SLOT_SECS - slot_f).clamp(0.0, 1.0);

    // SplitMix64 mixing on the time slot index.
    let mut z = slot.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    let hash = z ^ (z >> 31);

    let trigger_roll = ((hash & 0xFFFF) as f32) / 65536.0;
    let depth_roll = (((hash >> 16) & 0xFFFF) as f32) / 65536.0;
    let chance = (0.08 * strength).clamp(0.0, 0.55);
    if trigger_roll >= chance {
        return 1.0;
    }
    let pulse = (frac * std::f32::consts::PI).sin();
    let max_dip = (0.45 * strength * (0.5 + 0.5 * depth_roll)).clamp(0.0, 0.85);
    (1.0 - max_dip * pulse).clamp(0.15, 1.0)
}

/// Advance `[Battery]` charge and depletion-lockout state for one frame (`dt` seconds).
pub(crate) fn step_battery(
    runtime: &mut FlashlightRuntime,
    battery: BatteryConfig,
    flashlight: &mut Flashlight,
    dt: f32,
) {
    if !battery.enabled {
        return;
    }
    if runtime.depleted_lockout && flashlight.on {
        flashlight.on = false;
    }
    if flashlight.on {
        runtime.charge = (runtime.charge - battery.drain_rate * dt).max(0.0);
        if runtime.charge <= 0.0 {
            runtime.charge = 0.0;
            runtime.depleted_lockout = battery.recharge_threshold > 0.0;
            flashlight.on = false;
        }
    } else {
        runtime.charge = (runtime.charge + battery.recharge_rate * dt).min(100.0);
        if runtime.depleted_lockout && runtime.charge >= battery.recharge_threshold {
            runtime.depleted_lockout = false;
        }
    }
}

/// Spotlight placement when `BetterFlashlightNVSE.ini` is active [design].
///
/// Aligns with the drawn weapon's muzzle and barrel axis when `AttachToWeapon` is on and
/// a weapon is in hand, otherwise uses the camera/head-eye attach point, then applies the
/// local `(OffsetX, OffsetY, OffsetZ)` offset in game-space `(right, forward, up)` units
/// [hypothesis: FNV axis convention].
pub(crate) fn configured_placement(
    eye: [f32; 3],
    heading: f32,
    pitch: f32,
    weapon_aim: Option<([f32; 3], [f32; 3])>,
    weapon_muzzle: Option<[f32; 3]>,
    config: &BetterFlashlightConfig,
) -> Transform {
    let mut t = if config.attach_to_weapon {
        if let Some((origin, dir)) = weapon_aim {
            let fwd = nvf_play::space::to_bevy(dir).normalize_or_zero();
            if fwd.length_squared() > 0.5 {
                let up = if fwd.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
                Transform::from_translation(nvf_play::space::to_bevy(origin)).looking_to(fwd, up)
            } else {
                Transform::from_translation(nvf_play::space::to_bevy(origin))
                    .with_rotation(nvf_play::space::look_rotation(heading, pitch))
            }
        } else if let Some(origin) = weapon_muzzle {
            Transform::from_translation(nvf_play::space::to_bevy(origin))
                .with_rotation(nvf_play::space::look_rotation(heading, pitch))
        } else {
            placement(eye, heading, pitch)
        }
    } else {
        placement(eye, heading, pitch)
    };
    if config.offset != [0.0, 0.0, 0.0] {
        t.translation += t.rotation * nvf_play::space::to_bevy(config.offset);
    }
    t
}

/// Whether a slider's value is its default: a slider hands back a value on its step, to the
/// thousandth (the default softness 1 - 14/30 comes back as 16/30, 3e-8 away), so a row
/// moved and put back counts as never moved [design].
fn at_default(v: f32, default: f32) -> bool {
    (v - default).abs() <= default.abs().max(1.0) * 1e-4
}

/// The light for the rows `look` with an active `BetterFlashlightNVSE.ini` (module notes):
/// the Tweaks rows win where both set the same thing [design: #3280 review, maintainer
/// decision 2026-11-24: the player set them in game]. A row still at its default ([`FlashlightLook`]'s
/// `Default`) gives way to the INI's value: the colour (unless the Pip-Boy colour row is
/// on), the reach, the cone (Beam width and Softness together) and the brightness (the
/// INI's interior or exterior Dimmer × Intensity on the built-in brightness). The INI's
/// warm-up and flicker, which no row sets, scale whichever brightness that is.
fn with_ini(look: &FlashlightLook, pipboy: [u8; 3], config: &BetterFlashlightConfig, interior: bool, on_elapsed: f32) -> Beam {
    let default = FlashlightLook::default();
    let mut beam = look.beam(pipboy);
    if !look.pipboy_colour && look.rgb == default.rgb {
        beam.colour = config.color;
    }
    if at_default(look.range, default.range) {
        beam.range = config.distance;
    }
    if at_default(look.outer_deg, default.outer_deg) && at_default(look.softness, default.softness) {
        let (inner, outer) = config.cone_angles_rad();
        beam.inner_deg = inner.to_degrees();
        beam.outer_deg = outer.to_degrees();
    }
    if at_default(look.brightness, default.brightness) {
        let profile = config.profile(interior);
        beam.brightness = BRIGHTNESS * profile.dimmer * profile.intensity;
    }
    beam.brightness = (beam.brightness * warmup_factor(on_elapsed, config.warmup_time) * flicker_factor(on_elapsed, config.flicker)).max(0.0);
    beam
}

#[derive(SystemParam)]
struct FlashlightContext<'w> {
    player: Option<Res<'w, Player>>,
    body: Option<Res<'w, PlayerBody>>,
    weapon: Option<Res<'w, WeaponAnim>>,
    location: Option<Res<'w, Location>>,
    sky: Option<Res<'w, SkyState>>,
    config: Option<Res<'w, BetterFlashlightConfig>>,
    time: Option<Res<'w, Time<Real>>>,
    play_state: Option<Res<'w, State<PlayState>>>,
    paused: Option<Res<'w, Paused>>,
    save_log: Option<Res<'w, SaveLog>>,
    new_game: Option<Res<'w, NewGame>>,
}

type FlashlightLights<'w, 's> = Query<'w, 's, (Entity, &'static mut Transform, &'static mut MadeWith, Option<&'static mut SpotLight>, Option<&'static mut PointLight>), With<PlayerFlashlight>>;

fn sync(mut commands: Commands, mut flashlight: ResMut<Flashlight>, mut runtime: ResMut<FlashlightRuntime>, look: Option<Res<FlashlightLook>>, ctx: FlashlightContext, mut lights: FlashlightLights) {
    let look = look.map_or_else(FlashlightLook::default, |l| *l);
    let pipboy = nvf_play::interface::pipboy_rgb();
    let active = ctx.config.as_deref().filter(|c| c.is_active());
    if let Some(cfg) = active {
        // Battery charge back to full on a load or a new game.
        if let Some(log) = ctx.save_log.as_deref() {
            let loads = log.0.iter().filter(|line| line.starts_with("Loaded ")).count();
            if loads > runtime.seen_loads {
                runtime.reset_on_load();
            }
            runtime.seen_loads = loads;
        }
        let has_new_game = ctx.new_game.is_some();
        if has_new_game && !runtime.seen_new_game {
            runtime.reset_on_load();
        }
        runtime.seen_new_game = has_new_game;
        let playing = ctx.play_state.as_deref().is_none_or(|s| *s.get() == PlayState::Playing) && !ctx.paused.as_deref().is_some_and(|p| p.0);
        let dt = if playing { ctx.time.as_deref().map_or(0.0, |t| t.delta_secs().max(0.0)) } else { 0.0 };
        step_battery(&mut runtime, cfg.battery, &mut flashlight, dt);
        if flashlight.on {
            if !runtime.was_on {
                runtime.on_elapsed = 0.0;
                runtime.was_on = true;
            }
            runtime.on_elapsed += dt;
        } else {
            runtime.on_elapsed = 0.0;
            runtime.was_on = false;
        }
    }
    let Some(player) = ctx.player.as_deref().filter(|_| flashlight.on) else {
        for (e, ..) in &lights {
            commands.entity(e).despawn();
        }
        return;
    };
    let eye = ctx.body.as_deref().and_then(|b| b.head_eye()).unwrap_or_else(|| player.eye());
    let (beam, place) = match active {
        // Strict gate: without an active INI the light is the rows' alone.
        None => (look.beam(pipboy), placement(eye, player.heading, player.pitch)),
        Some(cfg) => {
            let interior = ctx.location.as_deref().map(|l| l.interior.is_some()).or_else(|| ctx.sky.as_deref().map(|s| s.interior.is_some())).unwrap_or(false);
            let aim = ctx.body.as_deref().and_then(PlayerBody::weapon_aim);
            let muzzle = ctx.weapon.as_deref().and_then(|w| w.muzzle);
            (with_ini(&look, pipboy, cfg, interior, runtime.on_elapsed), configured_placement(eye, player.heading, player.pitch, aim, muzzle, cfg))
        }
    };
    let color = crate::lights::display_color(beam.colour);
    let intensity = crate::lights::lumens(beam.brightness, crate::sky::light_unit());
    let mut current = false;
    for (e, mut t, mut made_with, spot, point) in &mut lights {
        if !current && made_with.0.kind == beam.kind && made_with.0.shadows == beam.shadows {
            *t = place;
            current = true;
            // The same type and shadows: its values change in place (the INI's flicker and
            // warm-up change the brightness every frame).
            if made_with.0 != beam {
                // Logged but for the INI's flicker and warm-up, which change only the
                // brightness, every frame.
                if active.is_none() || (Beam { brightness: beam.brightness, ..made_with.0 }) != beam {
                    log_beam(&beam);
                }
                made_with.0 = beam;
                match (made(&beam, color, intensity), spot, point) {
                    (Made::Spot(l), Some(mut s), _) => *s = l,
                    (Made::Point(l), _, Some(mut p)) => *p = l,
                    _ => {}
                }
            }
        } else {
            // Another type or shadows: made again below.
            commands.entity(e).despawn();
        }
    }
    if current {
        return;
    }
    log_beam(&beam);
    let mut e = commands.spawn((Name::new("Flashlight"), PlayerFlashlight, MadeWith(beam), place));
    match made(&beam, color, intensity) {
        Made::Spot(l) => e.insert(l),
        Made::Point(l) => e.insert(l),
    };
}

/// Another co-op player's flashlight, player `id`'s, as their rows make it ([`LightLook`]),
/// at `place` ([`placement`] from their puppet's eye). `coop.rs` makes it again when the
/// look changes ([`PeerFlashlight`]).
pub(crate) fn peer_light(commands: &mut Commands, id: u8, look: LightLook, place: Transform) -> Entity {
    let beam = FlashlightLook::from_wire(&look).beam([0; 3]);
    info!(
        "co-op: player {}'s flashlight light: {}, shadows {}, colour {:?}, range {:.0}, brightness {:.2}, cone {:.1}/{:.1} degrees",
        id + 1,
        beam.kind.name(),
        if beam.shadows { "on" } else { "off" },
        beam.colour,
        beam.range,
        beam.brightness,
        beam.inner_deg,
        beam.outer_deg
    );
    let mut e = commands.spawn((Name::new(format!("Flashlight of player {}", id + 1)), PeerFlashlight(id, look), place));
    let color = crate::lights::display_color(beam.colour);
    let intensity = crate::lights::lumens(beam.brightness, crate::sky::light_unit());
    match made(&beam, color, intensity) {
        Made::Spot(l) => e.insert(SpotLight { shadow_map_near_z: REMOTE_NEAR, ..l }),
        Made::Point(l) => e.insert(PointLight { shadow_map_near_z: REMOTE_NEAR, ..l }),
    };
    e.id()
}

/// The smoke scenario `flashlight-settings` reads this line.
fn log_beam(beam: &Beam) {
    info!(
        "flashlight light: {}, shadows {}, colour {:?}, range {:.0}, brightness {:.2}, cone {:.1}/{:.1} degrees",
        beam.kind.name(),
        if beam.shadows { "on" } else { "off" },
        beam.colour,
        beam.range,
        beam.brightness,
        beam.inner_deg,
        beam.outer_deg
    );
}

/// Bevy's light for a [`Beam`].
#[derive(Debug)]
enum Made {
    Spot(SpotLight),
    Point(PointLight),
}

impl Made {
    /// Whether the light carries the flashlight's mark for the first-person shapes.
    #[cfg(test)]
    fn marks_flashlight(&self) -> bool {
        match self {
            Made::Spot(l) => !l.affects_lightmapped_mesh_diffuse,
            Made::Point(l) => !l.affects_lightmapped_mesh_diffuse,
        }
    }
}

/// The light for `beam`. Each one, spot or point, with shadows or without, clears Bevy's
/// `affects_lightmapped_mesh_diffuse`: NVForever draws no lightmaps, so the flag changes
/// nothing else, and it is how the first-person shapes' shader tells the flashlight from
/// the world's lights (`lights/display_shader.rs`, `nvf_is_flashlight`; #3121, #3280)
/// [design].
fn made(beam: &Beam, color: Color, intensity: f32) -> Made {
    match beam.kind {
        LightKind::Spot => Made::Spot(SpotLight {
            color,
            intensity,
            range: beam.range,
            outer_angle: beam.outer_deg.to_radians(),
            inner_angle: beam.inner_deg.to_radians(),
            shadow_maps_enabled: beam.shadows,
            shadow_map_near_z: NEAR,
            shadow_depth_bias: SPOT_DEPTH_BIAS,
            affects_lightmapped_mesh_diffuse: false,
            ..default()
        }),
        LightKind::Point => Made::Point(PointLight {
            color,
            intensity,
            range: beam.range,
            shadow_maps_enabled: beam.shadows,
            shadow_map_near_z: NEAR,
            shadow_depth_bias: POINT_DEPTH_BIAS,
            affects_lightmapped_mesh_diffuse: false,
            ..default()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Pip-Boy's default colour (`nvf_play::interface::AMBER_RGB`).
    const AMBER: [u8; 3] = nvf_play::interface::AMBER_RGB;

    #[test]
    fn the_light_is_ahead_of_the_eye_and_looks_where_the_player_looks() {
        // Facing east (+x in game space), level.
        let t = placement([100.0, 0.0, 120.0], std::f32::consts::FRAC_PI_2, 0.0);
        let at = nvf_play::space::to_game(t.translation);
        assert!((at[0] - (100.0 + AHEAD)).abs() < 1e-3 && at[1].abs() < 1e-3, "{at:?}");
        assert!((at[2] - (120.0 - DOWN)).abs() < 1e-3);
        let dir = nvf_play::space::to_game(t.forward().as_vec3());
        assert!(dir[0] > 0.99, "{dir:?}");
    }

    #[test]
    fn the_defaults_are_the_light_before_the_rows() {
        let d = FlashlightLook::default().beam(AMBER);
        assert_eq!((d.kind, d.shadows, d.range, d.brightness, d.outer_deg), (LightKind::Spot, true, RANGE, BRIGHTNESS, OUTER_DEG));
        assert!((d.inner_deg - INNER_DEG).abs() < 1e-3, "{}", d.inner_deg);
        // Warm white (1.0, 0.95, 0.86) to the 255th.
        assert!(d.colour.iter().zip([1.0, 0.95, 0.86]).all(|(a, b)| (a - b).abs() < 1.0 / 255.0), "{:?}", d.colour);
    }

    #[test]
    fn the_light_takes_the_rows() {
        let look = FlashlightLook { kind: LightKind::Point, shadows: false, rgb: [255, 0, 51], pipboy_colour: false, range: 1000.0, brightness: 4.0, outer_deg: 40.0, softness: 0.25 };
        let b = look.beam(AMBER);
        assert_eq!((b.kind, b.shadows, b.range, b.brightness), (LightKind::Point, false, 1000.0, 4.0));
        assert_eq!(b.colour, [1.0, 0.0, 0.2]);
        assert!((b.inner_deg - 30.0).abs() < 1e-4 && b.outer_deg == 40.0);
    }

    #[test]
    fn the_pipboy_colour_follows_the_pipboy() {
        let look = FlashlightLook { pipboy_colour: true, ..FlashlightLook::default() };
        assert_eq!(look.beam([0, 255, 51]).colour, [0.0, 1.0, 0.2]);
        assert_eq!(look.beam([255, 0, 0]).colour, [1.0, 0.0, 0.0]);
        // Off, its own colour whatever the Pip-Boy's is.
        assert_eq!(FlashlightLook::default().beam([0, 255, 51]), FlashlightLook::default().beam(AMBER));
    }

    #[test]
    fn the_shadow_bias_is_bevys_default_in_game_units() {
        // 8 cm and 2 cm, as Bevy means them in metres.
        assert!((POINT_DEPTH_BIAS - 5.6).abs() < 1e-4 && (SPOT_DEPTH_BIAS - 1.4).abs() < 1e-4);
    }

    #[test]
    fn another_players_light_is_their_own_look() {
        // A green point light (here with the Pip-Boy row on and a green Pip-Boy, resolved
        // by the sender) and a yellow narrow spot (#3279).
        let host = FlashlightLook { kind: LightKind::Point, shadows: false, pipboy_colour: true, range: 1400.0, ..FlashlightLook::default() };
        let wire = host.wire([26, 255, 128]);
        assert_eq!((wire.kind, wire.shadows, wire.rgb), (light_kind::POINT, false, [26, 255, 128]));
        let seen = FlashlightLook::from_wire(&wire).beam(AMBER);
        assert_eq!(seen, host.beam([26, 255, 128]), "the others see what the sender draws");
        let guest = FlashlightLook { rgb: [255, 255, 0], outer_deg: 5.0, softness: 0.0, ..FlashlightLook::default() };
        let seen = FlashlightLook::from_wire(&guest.wire(AMBER)).beam([0, 255, 0]);
        assert_eq!(seen, guest.beam(AMBER), "the receiver's own Pip-Boy colour plays no part");
        // The default rows are the wire's default.
        assert_eq!(FlashlightLook::default().wire(AMBER), LightLook::default());
    }

    #[test]
    fn what_goes_out_is_within_what_the_host_takes() {
        // The rows' own tops pass unchanged; a value beyond them is cut.
        let top = FlashlightLook { range: LIGHT_RANGE_MAX, brightness: LIGHT_BRIGHTNESS_MAX, outer_deg: LIGHT_HALF_ANGLE_MAX, softness: 1.0, ..FlashlightLook::default() };
        let w = top.wire(AMBER);
        assert_eq!((w.range, w.brightness, w.outer_deg, w.softness), (LIGHT_RANGE_MAX, LIGHT_BRIGHTNESS_MAX, LIGHT_HALF_ANGLE_MAX, 1.0));
        let wild = FlashlightLook { range: 1.0e9, brightness: f32::NAN, outer_deg: -3.0, softness: 7.0, ..FlashlightLook::default() };
        let w = wild.wire(AMBER);
        assert_eq!((w.range, w.brightness, w.outer_deg, w.softness), (LIGHT_RANGE_MAX, 0.0, 0.0, 1.0));
    }

    #[test]
    fn a_remote_light_is_made_from_the_look() {
        let place = Transform::default();
        let mut app = App::new();
        let look = FlashlightLook { kind: LightKind::Point, shadows: false, rgb: [0, 255, 0], range: 1400.0, ..FlashlightLook::default() }.wire(AMBER);
        let e = peer_light(&mut app.world_mut().commands(), 1, look, place);
        app.world_mut().flush();
        let p = app.world().entity(e).get::<PointLight>().expect("a point light");
        assert_eq!((p.range, p.shadow_maps_enabled, p.shadow_map_near_z), (1400.0, false, REMOTE_NEAR));
        assert_eq!(app.world().entity(e).get::<PeerFlashlight>(), Some(&PeerFlashlight(1, look)));
        assert!(app.world().entity(e).get::<PlayerFlashlight>().is_none(), "not the player's own light");
        // A spot keeps its shadows on, from the same near plane.
        let spot = peer_light(&mut app.world_mut().commands(), 2, LightLook::default(), place);
        app.world_mut().flush();
        let s = app.world().entity(spot).get::<SpotLight>().expect("a spot light");
        assert!(s.shadow_maps_enabled && s.shadow_map_near_z == REMOTE_NEAR);
        assert!(app.world().entity(spot).get::<PointLight>().is_none());
        // It carries the flashlight's mark, as the player's own does.
        assert!(!s.affects_lightmapped_mesh_diffuse);
        // A changed look differs, which is how coop.rs knows to make it again.
        assert_ne!(PeerFlashlight(1, look), PeerFlashlight(1, LightLook { range: 1500.0, ..look }));
    }

    #[test]
    fn another_players_light_casts_nothing_from_their_puppet() {
        // Its shadows start past the puppet's face and what it holds out aiming: the
        // light sits AHEAD of the eye, a rifle's muzzle about 60 units.
        const {
            assert!(REMOTE_NEAR > 60.0 - AHEAD, "past the puppet's face and its weapon");
            assert!(REMOTE_NEAR < 100.0, "walls a step away still stop it");
            assert!(NEAR < REMOTE_NEAR, "the own light keeps its near plane");
        }
    }

    #[test]
    fn the_types_are_named_as_the_list_shows_them() {
        assert_eq!(LightKind::ALL.map(LightKind::name), ["Spot", "Point"]);
        assert_eq!(LightKind::from_name(" point "), Some(LightKind::Point));
        assert_eq!(LightKind::from_name("Area"), None);
    }

    /// The light `sync` makes for `look` (the Pip-Boy's colour amber).
    fn made_for(look: FlashlightLook) -> Made {
        let beam = look.beam(AMBER);
        made(&beam, crate::lights::display_color(beam.colour), 1.0)
    }

    /// CPU mirror of `nvf_point_display`/`nvf_spot_display` with `nvf_first_person_diffuse`
    /// (`lights/display_shader.rs`) and `GAME_ATTENUATION` (`lights.rs`): the display-value
    /// diffuse light of the flashlight `look` at `light` on a surface at `target` (Bevy world
    /// space) facing `normal`. The scale is read off the light `sync` makes, so it is the
    /// flashlight's mark that the first-person shapes go by.
    fn diffuse_display(look: FlashlightLook, light: &Transform, target: Vec3, normal: Vec3, first_person: bool) -> [f32; 3] {
        let beam = look.beam(AMBER);
        let to_light = light.translation - target;
        let d2 = to_light.length_squared();
        let attenuation = (1.0 - d2 / (beam.range * beam.range)).clamp(0.0, 1.0);
        let dir_to_light = to_light * d2.max(0.0001).sqrt().recip();
        let n_dot_l = normal.normalize().dot(dir_to_light).clamp(0.0, 1.0);
        let cone = match beam.kind {
            LightKind::Point => 1.0,
            LightKind::Spot => {
                let cd = (-light.forward().as_vec3()).dot(dir_to_light);
                let cos_outer = beam.outer_deg.to_radians().cos();
                let cos_inner = beam.inner_deg.to_radians().cos();
                let c = ((cd - cos_outer) / (cos_inner - cos_outer)).clamp(0.0, 1.0);
                c * c
            }
        };
        let scale = crate::lights::flashlight_display_scale(first_person, made_for(look).marks_flashlight());
        let factor = beam.brightness * attenuation * n_dot_l * cone * scale;
        beam.colour.map(|c| c * factor)
    }

    /// Final clipped linear diffuse colour (`lights/display_shader.rs` + `GAME_CLIP`).
    fn lit_clipped_linear(albedo: f32, ambient: f32, direct: [f32; 3]) -> [f32; 3] {
        direct.map(|d| nvf_assets::reflection_room::lit_texel(Vec3::splat(albedo), Vec3::splat(ambient + d)).x)
    }

    /// Every Type and Shadows choice of the Flashlight rows, the other rows at their defaults.
    fn every_kind_and_shadows() -> impl Iterator<Item = FlashlightLook> {
        LightKind::ALL
            .into_iter()
            .flat_map(|kind| [true, false].map(|shadows| FlashlightLook { kind, shadows, ..FlashlightLook::default() }))
    }

    #[test]
    fn every_flashlight_marks_itself_for_the_first_person_shapes() {
        for look in every_kind_and_shadows() {
            let made = made_for(look);
            assert!(made.marks_flashlight(), "{look:?}");
            match (look.kind, &made) {
                (LightKind::Spot, Made::Spot(l)) => assert_eq!(l.shadow_maps_enabled, look.shadows),
                (LightKind::Point, Made::Point(l)) => assert_eq!(l.shadow_maps_enabled, look.shadows),
                _ => panic!("{look:?} made as {made:?}"),
            }
            assert_eq!(crate::lights::flashlight_display_scale(true, made.marks_flashlight()), crate::lights::FIRST_PERSON_FLASHLIGHT_DIFFUSE);
            assert_eq!(crate::lights::flashlight_specular_scale(true, made.marks_flashlight()), crate::lights::FIRST_PERSON_FLASHLIGHT_SPECULAR);
        }
        // The world's lights keep Bevy's default, so they are not taken for the flashlight.
        assert!(SpotLight::default().affects_lightmapped_mesh_diffuse && PointLight::default().affects_lightmapped_mesh_diffuse);
    }

    #[test]
    fn aimed_in_first_person_weapons_and_hand_do_not_wash_out() {
        // Player at origin facing north (+y game, -z Bevy), level, indoors with typical dim ambient.
        let eye = [0.0, 0.0, 120.0];
        let light = placement(eye, 0.0, 0.0);
        let ambient = 0.25;
        // Aimed-in weapon & gloved hand sit directly ahead of the flashlight inside its 14 deg
        // inner core (25 units past the light), facing back toward the light.
        let aimed_pos = light.translation + light.forward().as_vec3() * 25.0;
        let facing_light = -light.forward().as_vec3();
        // Lit world wall in the same beam 300 and 1200 units ahead.
        let near_wall = light.translation + light.forward().as_vec3() * 300.0;
        let far_wall = light.translation + light.forward().as_vec3() * 1200.0;

        // Every Type and Shadows setting (#3280 review): a point light, or a light without
        // shadows, is the flashlight as much as the default shadow-casting spot.
        for look in every_kind_and_shadows() {
            let unscaled_direct = diffuse_display(look, &light, aimed_pos, facing_light, false);
            let fp_direct = diffuse_display(look, &light, aimed_pos, facing_light, true);
            let wall_near_direct = diffuse_display(look, &light, near_wall, facing_light, false);
            let wall_far_direct = diffuse_display(look, &light, far_wall, facing_light, false);

            // Reproduce the #3121 failing boundary: without the first-person scale, a bright
            // revolver (Ranger Sequoia silver, albedo 0.55) and gloved hand (0.35) blow out to white.
            assert_eq!(lit_clipped_linear(0.55, ambient, unscaled_direct)[0], 1.0, "{look:?}: unscaled revolver clipped");
            assert_eq!(lit_clipped_linear(0.35, ambient, unscaled_direct)[0], 1.0, "{look:?}: unscaled hand clipped");

            // With the fix, aimed-in revolver (0.55), energy weapon (0.45), gloved hand (0.35),
            // and rifle (0.25) all stay well below white clipping, receive gentle warm fill above
            // flashlight-off, and remain strictly dimmer than the lit world wall ahead.
            for (label, albedo) in [("revolver", 0.55), ("energy", 0.45), ("hand", 0.35), ("rifle", 0.25)] {
                let off = lit_clipped_linear(albedo, ambient, [0.0; 3]);
                let on = lit_clipped_linear(albedo, ambient, fp_direct);
                let wall_near = lit_clipped_linear(albedo, ambient, wall_near_direct);
                let wall_far = lit_clipped_linear(albedo, ambient, wall_far_direct);
                for c in 0..3 {
                    assert!(on[c] < 0.75, "{look:?} {label} channel {c} must not wash out: {on:?}");
                    assert!(on[c] > off[c], "{look:?} {label} channel {c} keeps subtle warm fill over off: {on:?} vs {off:?}");
                    assert!(
                        on[c] < wall_near[c] && on[c] < wall_far[c],
                        "{look:?} {label} channel {c} stays dimmer than lit world wall: {on:?} vs near {wall_near:?} / far {wall_far:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn non_aimed_third_person_world_and_world_spot_controls_hold() {
        let eye = [0.0, 0.0, 120.0];
        let light = placement(eye, 0.0, 0.0);
        let facing_light = -light.forward().as_vec3();
        let look = FlashlightLook::default();

        // Non-aimed hip-held weapon sits off to the right and below the beam axis: weaker than aimed-in.
        let aimed_pos = light.translation + light.forward().as_vec3() * 25.0;
        let hip_pos = light.translation + light.forward().as_vec3() * 25.0 + light.right().as_vec3() * 11.0 - light.up().as_vec3() * 6.0;
        let aimed = diffuse_display(look, &light, aimed_pos, facing_light, true);
        let hip = diffuse_display(look, &light, hip_pos, facing_light, true);
        assert!(hip[0] < aimed[0] && hip[0] >= 0.0, "hip {hip:?} weaker than aimed {aimed:?}");

        // Third-person body/weapon (BODY_LAYER) and world surfaces (!FIRST_PERSON_DEPTH)
        // keep full flashlight diffuse and specular strength.
        assert_eq!(crate::lights::flashlight_display_scale(false, true), 1.0);
        assert_eq!(crate::lights::flashlight_specular_scale(false, true), 1.0);
        let world_wall = diffuse_display(look, &light, light.translation + light.forward().as_vec3() * 200.0, facing_light, false);
        assert!(world_wall[0] > BRIGHTNESS * 0.98, "world illumination unchanged: {world_wall:?}");

        // The world's own lights (LIGH records, with shadows or not) still light first-person
        // weapons and hands at full strength.
        assert_eq!(crate::lights::flashlight_display_scale(true, false), 1.0);
        assert_eq!(crate::lights::flashlight_specular_scale(true, false), 1.0);
    }

    /// The light with no INI and the rows at their defaults, built from the constants and
    /// not through `made`, which it checks.
    fn vanilla_spotlight() -> SpotLight {
        SpotLight {
            color: crate::lights::display_color(COLOUR_RGB.map(|c| f32::from(c) / 255.0)),
            intensity: crate::lights::lumens(BRIGHTNESS, crate::sky::light_unit()),
            range: RANGE,
            outer_angle: OUTER_DEG.to_radians(),
            inner_angle: INNER_DEG.to_radians(),
            shadow_maps_enabled: true,
            shadow_map_near_z: NEAR,
            ..default()
        }
    }

    fn test_player(heading: f32, pitch: f32) -> Player {
        Player {
            heading,
            pitch,
            eye_height: 120.0,
            feet: [100.0, 200.0, 0.0],
            body_eye: None,
            body_view: None,
            placed: None,
        }
    }

    fn test_app(config: Option<BetterFlashlightConfig>) -> App {
        let mut app = App::new();
        app.insert_resource(Flashlight { on: true, toggles: 1 })
            .insert_resource(test_player(0.0, 0.0))
            .insert_resource(Time::<Real>::default())
            .insert_resource(State::new(PlayState::Playing))
            .insert_resource(Paused(false))
            .insert_resource(Location { world: 0x3C, interior: None })
            .insert_resource(SaveLog::default())
            .add_plugins(FlashlightPlugin);
        if let Some(cfg) = config {
            app.insert_resource(cfg);
        }
        app
    }

    fn step_app(app: &mut App, dt_secs: f32) {
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(std::time::Duration::from_secs_f32(dt_secs));
        app.update();
    }

    fn current_spot(app: &mut App) -> Option<(Transform, SpotLight)> {
        let mut q = app.world_mut().query_filtered::<(&Transform, &SpotLight), With<PlayerFlashlight>>();
        q.iter(app.world()).next().map(|(t, s)| (*t, *s))
    }

    #[test]
    fn absent_ini_keeps_vanilla_spotlight_bit_for_bit_identical() {
        let mut app = test_app(None);
        step_app(&mut app, 0.016);
        let (t, spot) = current_spot(&mut app).expect("vanilla flashlight spawned");
        let want_t = placement(test_player(0.0, 0.0).eye(), 0.0, 0.0);
        let want_s = vanilla_spotlight();
        assert_eq!(t, want_t);
        assert_eq!(spot.color, want_s.color);
        assert_eq!(spot.intensity, want_s.intensity);
        assert_eq!(spot.range, want_s.range);
        assert_eq!(spot.outer_angle, want_s.outer_angle);
        assert!((spot.inner_angle - want_s.inner_angle).abs() < 1e-5, "{} vs {}", spot.inner_angle, want_s.inner_angle);
        assert_eq!(spot.shadow_maps_enabled, want_s.shadow_maps_enabled);
        assert_eq!(spot.shadow_map_near_z, want_s.shadow_map_near_z);
        assert_eq!(spot.shadow_depth_bias, SPOT_DEPTH_BIAS, "a visible change from Bevy's 0.02 (module notes)");
    }

    #[test]
    fn disabled_main_or_flashlight_keeps_vanilla_identity_and_skips_battery() {
        for ini in [
            b"[Main]\nRenderEffects = 0\n[Flashlight]\nEnabled = 1\nDistance = 900\n[Battery]\nEnabled = 1\nDrainRate = 50\n".as_slice(),
            b"[Main]\nRenderEffects = 1\n[Flashlight]\nEnabled = 0\nDistance = 900\n[Battery]\nEnabled = 1\nDrainRate = 50\n".as_slice(),
        ] {
            let mut app = test_app(Some(BetterFlashlightConfig::parse(ini)));
            step_app(&mut app, 1.0);
            let (t, spot) = current_spot(&mut app).expect("vanilla flashlight spawned");
            let want_s = vanilla_spotlight();
            assert_eq!(t, placement(test_player(0.0, 0.0).eye(), 0.0, 0.0));
            assert_eq!(spot.intensity, want_s.intensity);
            assert_eq!(spot.range, want_s.range);
            assert_eq!(spot.outer_angle, want_s.outer_angle);
            assert_eq!(app.world().resource::<FlashlightRuntime>().charge, 100.0);
        }
    }

    #[test]
    fn interior_and_exterior_profiles_switch_dynamically() {
        let cfg = BetterFlashlightConfig::parse(
            b"[Main]\nRenderEffects = 1\n[Flashlight]\nEnabled = 1\nWarmupTime = 0\nFlicker = 0\n[Interior]\nDimmer = 1.0\nIntensity = 0.25\n[Exterior]\nDimmer = 3.0\nIntensity = 0.50\n",
        );
        let mut app = test_app(Some(cfg));

        // Exterior cell (Location.interior == None).
        step_app(&mut app, 0.016);
        let (_, ext_spot) = current_spot(&mut app).expect("exterior spotlight");
        let want_ext = crate::lights::lumens(BRIGHTNESS * 3.0 * 0.50, crate::sky::light_unit());
        assert!((ext_spot.intensity - want_ext).abs() < 1e-3, "{} vs {}", ext_spot.intensity, want_ext);

        // Transition into an interior cell.
        app.world_mut().resource_mut::<Location>().interior = Some(0x0010_0001);
        step_app(&mut app, 0.016);
        let (_, int_spot) = current_spot(&mut app).expect("interior spotlight");
        let want_int = crate::lights::lumens(BRIGHTNESS * 1.0 * 0.25, crate::sky::light_unit());
        assert!((int_spot.intensity - want_int).abs() < 1e-3, "{} vs {}", int_spot.intensity, want_int);
        assert!(ext_spot.intensity > int_spot.intensity);

        // Also verify SkyState::interior fallback when Location resource is absent.
        app.world_mut().remove_resource::<Location>();
        let mut sky = SkyState::default();
        sky.interior = Some(fnv_world::lighting::CellLighting::default());
        app.insert_resource(sky);
        step_app(&mut app, 0.016);
        let (_, sky_int_spot) = current_spot(&mut app).expect("sky interior spotlight");
        assert!((sky_int_spot.intensity - want_int).abs() < 1e-3);
    }

    #[test]
    fn spotlight_maps_angle_spot_size_distance_color_and_rotated_offsets() {
        let cfg = BetterFlashlightConfig::parse(
            b"[Main]\nRenderEffects = 1\n[Flashlight]\nEnabled = 1\nAttachToWeapon = 0\nAngle = 20\nSpotSize = 1.25\nDistance = 1500\nColorR = 1.2\nColorG = 1.4\nColorB = 1.8\nWarmupTime = 0\nFlicker = 0\nOffsetX = 10\nOffsetY = 20\nOffsetZ = -5\n",
        );
        let mut app = test_app(Some(cfg));
        // Face east (heading = pi/2, game +X): local +X (right) is game -Y, local +Y (forward) is game +X, local +Z (up) is game +Z.
        app.world_mut().resource_mut::<Player>().heading = std::f32::consts::FRAC_PI_2;
        step_app(&mut app, 0.016);

        let (t, spot) = current_spot(&mut app).expect("configured spotlight");
        assert_eq!(spot.range, 1500.0);
        assert!((spot.outer_angle.to_degrees() - 25.0).abs() < 1e-4);
        assert!((spot.inner_angle.to_degrees() - (25.0 * INNER_DEG / OUTER_DEG)).abs() < 1e-4);
        assert_eq!(spot.color, crate::lights::display_color([1.2, 1.4, 1.8]));
        // Preserves native shadow flags so #3121 first-person anti-washout scaling stays active.
        assert!(spot.shadow_maps_enabled);
        assert_eq!(spot.shadow_map_near_z, NEAR);

        let base = nvf_play::space::to_game(placement(test_player(std::f32::consts::FRAC_PI_2, 0.0).eye(), std::f32::consts::FRAC_PI_2, 0.0).translation);
        let got = nvf_play::space::to_game(t.translation);
        assert!((got[0] - (base[0] + 20.0)).abs() < 1e-3, "forward +20 along east (+X): {got:?} vs {base:?}");
        assert!((got[1] - (base[1] - 10.0)).abs() < 1e-3, "right +10 facing east (-Y): {got:?} vs {base:?}");
        assert!((got[2] - (base[2] - 5.0)).abs() < 1e-3, "up -5 (-Z): {got:?} vs {base:?}");
    }

    #[test]
    fn attach_to_weapon_uses_weapon_aim_and_falls_back_cleanly() {
        let cfg_weapon = BetterFlashlightConfig::parse(
            b"[Main]\nRenderEffects = 1\n[Flashlight]\nEnabled = 1\nAttachToWeapon = 1\nWarmupTime = 0\nFlicker = 0\n",
        );
        let mut app = test_app(Some(cfg_weapon));

        // 1. With weapon drawn (PlayerBody::weapon_aim), spotlight uses muzzle origin and barrel direction.
        let mut body = PlayerBody::default();
        body.set_weapon_aim_for_test(Some(([150.0, 250.0, 110.0], [1.0, 0.0, 0.0])));
        app.insert_resource(body);
        step_app(&mut app, 0.016);
        let (t_aim, _) = current_spot(&mut app).expect("weapon-attached spotlight");
        let pos_aim = nvf_play::space::to_game(t_aim.translation);
        let dir_aim = nvf_play::space::to_game(t_aim.forward().as_vec3());
        assert!((pos_aim[0] - 150.0).abs() < 1e-3 && (pos_aim[1] - 250.0).abs() < 1e-3 && (pos_aim[2] - 110.0).abs() < 1e-3, "{pos_aim:?}");
        assert!(dir_aim[0] > 0.99, "aligned with barrel +X: {dir_aim:?}");

        // 2. Without PlayerBody::weapon_aim, falls back to WeaponAnim::muzzle + player look rotation.
        app.world_mut().resource_mut::<PlayerBody>().set_weapon_aim_for_test(None);
        app.insert_resource(WeaponAnim { ready: true, muzzle: Some([140.0, 240.0, 105.0]), ..default() });
        step_app(&mut app, 0.016);
        let (t_muzzle, _) = current_spot(&mut app).expect("muzzle-fallback spotlight");
        let pos_muzzle = nvf_play::space::to_game(t_muzzle.translation);
        let dir_muzzle = nvf_play::space::to_game(t_muzzle.forward().as_vec3());
        assert!((pos_muzzle[0] - 140.0).abs() < 1e-3 && (pos_muzzle[1] - 240.0).abs() < 1e-3 && (pos_muzzle[2] - 105.0).abs() < 1e-3);
        assert!(dir_muzzle[1] > 0.99, "player facing north (+Y): {dir_muzzle:?}");

        // 3. When holstered/unarmed (no weapon aim or muzzle), falls back to camera/head-eye placement.
        app.world_mut().resource_mut::<WeaponAnim>().muzzle = None;
        step_app(&mut app, 0.016);
        let (t_unarmed, _) = current_spot(&mut app).expect("unarmed camera fallback");
        assert_eq!(t_unarmed, placement(test_player(0.0, 0.0).eye(), 0.0, 0.0));

        // 4. With AttachToWeapon = 0, ignores weapon aim and stays on camera/head-eye.
        app.world_mut().resource_mut::<PlayerBody>().set_weapon_aim_for_test(Some(([150.0, 250.0, 110.0], [1.0, 0.0, 0.0])));
        app.world_mut().resource_mut::<BetterFlashlightConfig>().attach_to_weapon = false;
        step_app(&mut app, 0.016);
        let (t_cam_only, _) = current_spot(&mut app).expect("camera-only spotlight");
        assert_eq!(t_cam_only, placement(test_player(0.0, 0.0).eye(), 0.0, 0.0));
    }

    #[test]
    fn warmup_ramp_and_flicker_modulate_intensity_cleanly() {
        // Warmup unit curve.
        assert_eq!(warmup_factor(0.0, 0.0), 1.0);
        assert_eq!(warmup_factor(0.0, 0.4), 0.0);
        assert!((warmup_factor(0.2, 0.4) - 0.5).abs() < 1e-5);
        assert_eq!(warmup_factor(0.4, 0.4), 1.0);
        assert_eq!(warmup_factor(2.0, 0.4), 1.0);

        // Flicker unit curve: 0 is steady, >0 is mostly 1.0 with occasional dips, higher = deeper/more often.
        for i in 0..200 {
            assert_eq!(flicker_factor(i as f32 * 0.037, 0.0), 1.0);
        }
        let sample = |strength: f32| {
            let mut dips = 0usize;
            let mut min_val = 1.0f32;
            for i in 0..400 {
                let v = flicker_factor(i as f32 * 0.08 + 0.04, strength);
                assert!((0.15..=1.0).contains(&v), "out of bounds: {v}");
                if v < 0.999 {
                    dips += 1;
                    min_val = min_val.min(v);
                }
            }
            (dips, min_val)
        };
        let (low_dips, low_min) = sample(0.25);
        let (high_dips, high_min) = sample(1.5);
        assert!(low_dips > 0 && low_dips < 60, "sporadic dips at 0.25: {low_dips}");
        assert!(high_dips > low_dips, "higher flicker dips more often: {high_dips} vs {low_dips}");
        assert!(high_min < low_min, "higher flicker dips deeper: {high_min} vs {low_min}");

        // Live ECS warmup progression and reset on off->on toggle.
        let cfg = BetterFlashlightConfig::parse(
            b"[Main]\nRenderEffects = 1\n[Flashlight]\nEnabled = 1\nWarmupTime = 1.0\nFlicker = 0\n[Exterior]\nDimmer = 1.0\nIntensity = 1.0\n",
        );
        let mut app = test_app(Some(cfg));
        step_app(&mut app, 0.25);
        let (_, early) = current_spot(&mut app).expect("warming up");
        step_app(&mut app, 0.25);
        let (_, mid) = current_spot(&mut app).expect("mid warmup");
        step_app(&mut app, 0.75);
        let (_, full) = current_spot(&mut app).expect("warmed up");
        assert!(early.intensity < mid.intensity && mid.intensity < full.intensity);

        // Turning off and back on restarts the warm-up ramp.
        app.world_mut().resource_mut::<Flashlight>().on = false;
        step_app(&mut app, 0.1);
        assert!(current_spot(&mut app).is_none());
        app.world_mut().resource_mut::<Flashlight>().on = true;
        step_app(&mut app, 0.1);
        let (_, restarted) = current_spot(&mut app).expect("restarted warmup");
        assert!(restarted.intensity < mid.intensity);
    }

    #[test]
    fn battery_drains_locks_out_recharges_pauses_and_resets_on_load_and_new_game() {
        let cfg = BetterFlashlightConfig::parse(
            b"[Main]\nRenderEffects = 1\n[Flashlight]\nEnabled = 1\nWarmupTime = 0\nFlicker = 0\n[Battery]\nEnabled = 1\nDrainRate = 25\nRechargeRate = 10\nRechargeThreshold = 15\n",
        );
        let mut app = test_app(Some(cfg));

        // 1. Drain while on and playing: 2.0s * 25/s = 50 remaining.
        step_app(&mut app, 2.0);
        assert!((app.world().resource::<FlashlightRuntime>().charge - 50.0).abs() < 1e-4);
        assert!(app.world().resource::<Flashlight>().on);

        // 2. Paused / menu state stops drain.
        app.world_mut().resource_mut::<Paused>().0 = true;
        step_app(&mut app, 3.0);
        assert!((app.world().resource::<FlashlightRuntime>().charge - 50.0).abs() < 1e-4);
        app.world_mut().resource_mut::<Paused>().0 = false;
        app.insert_resource(State::new(PlayState::Menu));
        step_app(&mut app, 3.0);
        assert!((app.world().resource::<FlashlightRuntime>().charge - 50.0).abs() < 1e-4);
        app.insert_resource(State::new(PlayState::Playing));

        // 3. Full depletion turns flashlight off and enters RechargeThreshold lockout.
        step_app(&mut app, 2.5);
        assert_eq!(app.world().resource::<FlashlightRuntime>().charge, 0.0);
        assert!(app.world().resource::<FlashlightRuntime>().depleted_lockout);
        assert!(!app.world().resource::<Flashlight>().on);
        assert!(current_spot(&mut app).is_none());

        // 4. Attempting to turn on before reaching RechargeThreshold (15) is blocked and keeps recharging.
        app.world_mut().resource_mut::<Flashlight>().on = true;
        step_app(&mut app, 1.0); // recharges to 10 (< 15)
        assert!(!app.world().resource::<Flashlight>().on, "blocked during lockout");
        assert!(app.world().resource::<FlashlightRuntime>().depleted_lockout);
        assert!((app.world().resource::<FlashlightRuntime>().charge - 10.0).abs() < 1e-4);
        assert!(current_spot(&mut app).is_none());

        // 5. Recharging past RechargeThreshold (15) clears lockout and allows turning back on.
        step_app(&mut app, 0.6); // 10 + 6 = 16 >= 15
        assert!(!app.world().resource::<FlashlightRuntime>().depleted_lockout);
        assert!((app.world().resource::<FlashlightRuntime>().charge - 16.0).abs() < 1e-4);
        app.world_mut().resource_mut::<Flashlight>().on = true;
        step_app(&mut app, 0.1); // drains 2.5 -> 13.5, stays on!
        assert!(app.world().resource::<Flashlight>().on);
        assert!(current_spot(&mut app).is_some());

        // 6. Save load resets charge to 100% and clears lockout.
        app.world_mut().resource_mut::<FlashlightRuntime>().charge = 0.0;
        app.world_mut().resource_mut::<FlashlightRuntime>().depleted_lockout = true;
        app.world_mut().resource_mut::<Flashlight>().on = false;
        app.world_mut().resource_mut::<SaveLog>().0.push("Loaded quicksave.nvfsave".into());
        step_app(&mut app, 0.0);
        assert_eq!(app.world().resource::<FlashlightRuntime>().charge, 100.0);
        assert!(!app.world().resource::<FlashlightRuntime>().depleted_lockout);

        // 7. New game also resets charge to 100% and clears lockout.
        app.world_mut().resource_mut::<FlashlightRuntime>().charge = 12.0;
        app.world_mut().resource_mut::<FlashlightRuntime>().depleted_lockout = true;
        app.insert_resource(NewGame);
        step_app(&mut app, 0.0);
        assert_eq!(app.world().resource::<FlashlightRuntime>().charge, 100.0);
        assert!(!app.world().resource::<FlashlightRuntime>().depleted_lockout);
    }

    /// #3280 review, maintainer decision 2026-11-24: with an active `BetterFlashlightNVSE.ini`, a
    /// Tweaks Flashlight row the player has set wins over the INI's value for the same
    /// thing; rows at their defaults take the INI's, and the INI's battery and warm-up
    /// still apply.
    #[test]
    fn tweaks_rows_win_over_the_ini_where_both_set_the_same_thing() {
        let ini = b"[Main]\nRenderEffects = 1\n[Flashlight]\nEnabled = 1\nAngle = 20\nSpotSize = 1.25\nDistance = 1500\nColorR = 1.2\nColorG = 1.4\nColorB = 1.8\nWarmupTime = 0\nFlicker = 0\n[Exterior]\nDimmer = 3.0\nIntensity = 0.5\n[Battery]\nEnabled = 1\nDrainRate = 25\n";
        // Rows at their defaults: the INI's colour, reach, cone and brightness.
        let mut app = test_app(Some(BetterFlashlightConfig::parse(ini)));
        step_app(&mut app, 0.016);
        let (_, spot) = current_spot(&mut app).expect("INI spotlight");
        assert_eq!(spot.range, 1500.0);
        assert_eq!(spot.color, crate::lights::display_color([1.2, 1.4, 1.8]));
        assert!((spot.outer_angle.to_degrees() - 25.0).abs() < 1e-4);
        assert!((spot.intensity - crate::lights::lumens(BRIGHTNESS * 1.5, crate::sky::light_unit())).abs() < 1e-3);
        // Rows the player set: theirs, the INI's for the rest, its battery still running.
        let look = FlashlightLook { rgb: [255, 0, 51], range: 1000.0, brightness: 4.0, outer_deg: 40.0, softness: 0.25, ..FlashlightLook::default() };
        let mut app = test_app(Some(BetterFlashlightConfig::parse(ini)));
        app.insert_resource(look);
        step_app(&mut app, 1.0);
        let (_, spot) = current_spot(&mut app).expect("rows over the INI");
        assert_eq!(spot.range, 1000.0);
        assert_eq!(spot.color, crate::lights::display_color([1.0, 0.0, 0.2]));
        assert!((spot.outer_angle.to_degrees() - 40.0).abs() < 1e-4 && (spot.inner_angle.to_degrees() - 30.0).abs() < 1e-4);
        assert!((spot.intensity - crate::lights::lumens(4.0, crate::sky::light_unit())).abs() < 1e-3);
        assert!((app.world().resource::<FlashlightRuntime>().charge - 75.0).abs() < 1e-3, "the INI's battery drains");
        // Type and shadows are the rows' alone: a point light without shadows, marked.
        let mut app = test_app(Some(BetterFlashlightConfig::parse(ini)));
        app.insert_resource(FlashlightLook { kind: LightKind::Point, shadows: false, ..FlashlightLook::default() });
        step_app(&mut app, 0.016);
        assert!(current_spot(&mut app).is_none());
        let mut q = app.world_mut().query_filtered::<&PointLight, With<PlayerFlashlight>>();
        let point = *q.iter(app.world()).next().expect("point flashlight");
        assert!(!point.shadow_maps_enabled && !point.affects_lightmapped_mesh_diffuse && point.range == 1500.0);
    }

    /// A slider moved and put back hands its default back on its step (the softness 14/30
    /// as 16/30, the range to the thousandth): that is the default, so the INI's value
    /// applies again; a row really moved still wins.
    #[test]
    fn a_row_put_back_to_its_default_gives_way_to_the_ini_again() {
        let ini = BetterFlashlightConfig::parse(b"[Main]\nRenderEffects = 1\n[Flashlight]\nEnabled = 1\nAngle = 20\nSpotSize = 1.0\nDistance = 1500\nWarmupTime = 0\nFlicker = 0\n");
        let put_back = FlashlightLook { softness: 16.0 / 30.0, range: 0.350 * 8000.0, outer_deg: 12.0 / 32.0 * 80.0, ..FlashlightLook::default() };
        let b = with_ini(&put_back, AMBER, &ini, false, 0.0);
        assert_eq!((b.range, b.outer_deg), (1500.0, 20.0), "put back: the INI's range and cone");
        let moved = FlashlightLook { softness: 20.0 / 30.0, ..FlashlightLook::default() };
        assert!((with_ini(&moved, AMBER, &ini, false, 0.0).outer_deg - 30.0).abs() < 1e-3, "moved: the rows' cone");
    }

    /// With the INI's flicker the light changes every frame; it is changed in place, not
    /// made again.
    #[test]
    fn the_ini_light_changes_in_place() {
        let ini = b"[Main]\nRenderEffects = 1\n[Flashlight]\nEnabled = 1\nWarmupTime = 1.0\nFlicker = 0\n";
        let mut app = test_app(Some(BetterFlashlightConfig::parse(ini)));
        step_app(&mut app, 0.25);
        let first = app.world_mut().query_filtered::<Entity, With<PlayerFlashlight>>().single(app.world()).unwrap();
        step_app(&mut app, 0.25);
        let second = app.world_mut().query_filtered::<Entity, With<PlayerFlashlight>>().single(app.world()).unwrap();
        assert_eq!(first, second);
    }
}
