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
//!   first-person shapes (`FIRST_PERSON_DEPTH`, `lights/display_shader.rs`) shadow-casting
//!   spot lights are scaled down (`FIRST_PERSON_FLASHLIGHT_DIFFUSE`,
//!   `FIRST_PERSON_FLASHLIGHT_SPECULAR`) [design, #3121] so aiming down sights into the
//!   beam's inner core does not wash the held weapon and hand out to white, while world
//!   surfaces, third-person body/weapon (`BODY_LAYER`) and non-shadow-casting world spot
//!   lamps (`LIGH` `SPOT`) keep full strength.
//! - Detection does not count it (`detection.rs`).
//! - The Tweaks tab's Flashlight rows ([`FlashlightLook`], registered in `settings.rs`)
//!   set its colour, its type (Bevy's `SpotLight` or `PointLight`), whether it casts
//!   shadows, its reach, its brightness and a spot's beam; at their defaults it is the
//!   light above [design: not in the original, whose Pip-Boy light is no such light; the
//!   rows are those flashlight mods of other games commonly offer].
//! - Another co-op player's light (`coop.rs` `peer_lights`, [`peer_light`]) is that
//!   player's own: their rows go with their state ([`FlashlightLook::wire`], the colour
//!   already their Pip-Boy's when it says Pip-Boy) and their puppet gets the same light,
//!   placed from its eye as this one is (owner 2026-11-21: "if host has a green point
//!   light and guest has a yellow spot light, they see eachother's lights properly"). The
//!   puppet is drawn on the world's layer 0, so its head, arms and weapon would cast into
//!   its own cone, as the player's own body did before it got `sky::BODY_LAYER`: a dark
//!   wedge swinging over the lit ground (owner's co-op video 2026-11-12, "synced
//!   flashlights have some weird shading"). Its shadow map starts at [`REMOTE_NEAR`]
//!   instead, beyond what the puppet holds in front of it; walls farther than that still
//!   stop it. A point light's shadow map starts there too: the puppet's legs, below that,
//!   still shade the ground around its feet, as a lantern held at the head would [design].
//!   Detection counts it for that player, not this one (`detection.rs`).

use bevy::prelude::*;
use nvf_play::flashlight::Flashlight;
use nvf_play::player::Player;
use crate::player_body::PlayerBody;
use nvf_coop::net::{light_kind, LightLook};

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
    /// is the Pip-Boy's colour [observed: original Fallout: New Vegas, owner, 2026-11-21
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
    /// the colour they see is the one this game draws.
    pub(crate) fn wire(&self, pipboy: [u8; 3]) -> LightLook {
        LightLook {
            kind: match self.kind {
                LightKind::Spot => light_kind::SPOT,
                LightKind::Point => light_kind::POINT,
            },
            shadows: self.shadows,
            rgb: if self.pipboy_colour { pipboy } else { self.rgb },
            range: self.range,
            brightness: self.brightness,
            outer_deg: self.outer_deg,
            softness: self.softness,
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
const UNITS_PER_METRE: f32 = 70.0;
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
        app.init_resource::<FlashlightLook>().add_systems(Update, sync.after(nvf_play::PlaySet::Physics));
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

fn sync(
    mut commands: Commands,
    flashlight: Res<Flashlight>,
    look: Option<Res<FlashlightLook>>,
    player: Option<Res<Player>>,
    body: Option<Res<PlayerBody>>,
    mut lights: Query<(Entity, &mut Transform, &MadeWith), With<PlayerFlashlight>>,
) {
    let Some(player) = player.filter(|_| flashlight.on) else {
        for (e, _, _) in &lights {
            commands.entity(e).despawn();
        }
        return;
    };
    let beam = look.map_or_else(FlashlightLook::default, |l| *l).beam(nvf_play::interface::pipboy_rgb());
    let place = placement(body.and_then(|b| b.head_eye()).unwrap_or_else(|| player.eye()), player.heading, player.pitch);
    let mut current = false;
    for (e, mut t, made) in &mut lights {
        if made.0 == beam && !current {
            *t = place;
            current = true;
        } else {
            // Another type or other values: made again below.
            commands.entity(e).despawn();
        }
    }
    if current {
        return;
    }
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
    let mut e = commands.spawn((Name::new("Flashlight"), PlayerFlashlight, MadeWith(beam), place));
    insert_light(&mut e, beam, NEAR);
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
    insert_light(&mut e, beam, REMOTE_NEAR);
    e.id()
}

/// The light itself, its shadow map from `near`.
fn insert_light(e: &mut EntityCommands, beam: Beam, near: f32) {
    let color = crate::lights::display_color(beam.colour);
    let intensity = crate::lights::lumens(beam.brightness, crate::sky::light_unit());
    match beam.kind {
        LightKind::Spot => e.insert(SpotLight {
            color,
            intensity,
            range: beam.range,
            outer_angle: beam.outer_deg.to_radians(),
            inner_angle: beam.inner_deg.to_radians(),
            shadow_maps_enabled: beam.shadows,
            shadow_map_near_z: near,
            shadow_depth_bias: SPOT_DEPTH_BIAS,
            ..default()
        }),
        LightKind::Point => e.insert(PointLight {
            color,
            intensity,
            range: beam.range,
            shadow_maps_enabled: beam.shadows,
            shadow_map_near_z: near,
            shadow_depth_bias: POINT_DEPTH_BIAS,
            ..default()
        }),
    };
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

    /// CPU mirror of `nvf_spot_display` (`lights/display_shader.rs`) + `GAME_ATTENUATION`
    /// (`lights.rs`) for `PlayerFlashlight` at `light`, illuminating a surface at `target`
    /// (Bevy world space) facing `normal`.
    fn spot_diffuse_display(
        light: &Transform,
        target: Vec3,
        normal: Vec3,
        first_person: bool,
        casts_shadows: bool,
    ) -> [f32; 3] {
        let to_light = light.translation - target;
        let d2 = to_light.length_squared();
        let inv_range_sq = 1.0 / (RANGE * RANGE);
        let attenuation = (1.0 - d2 * inv_range_sq).clamp(0.0, 1.0);
        let dir_to_light = to_light * d2.max(0.0001).sqrt().recip();
        let n_dot_l = normal.normalize().dot(dir_to_light).clamp(0.0, 1.0);
        let spot_dir = light.forward().as_vec3();
        let cd = (-spot_dir).dot(dir_to_light);
        let cos_outer = OUTER_DEG.to_radians().cos();
        let cos_inner = INNER_DEG.to_radians().cos();
        let cone = ((cd - cos_outer) / (cos_inner - cos_outer)).clamp(0.0, 1.0);
        let scale = crate::lights::spot_display_scale(first_person, casts_shadows);
        let factor = BRIGHTNESS * attenuation * n_dot_l * cone * cone * scale;
        COLOUR_RGB.map(|c| f32::from(c) / 255.0 * factor)
    }

    /// Final clipped linear diffuse colour (`lights/display_shader.rs` + `GAME_CLIP`).
    fn lit_clipped_linear(albedo: f32, ambient: f32, direct: [f32; 3]) -> [f32; 3] {
        direct.map(|d| (albedo * (ambient + d).max(0.0).powf(2.2)).min(1.0))
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

        let unscaled_direct = spot_diffuse_display(&light, aimed_pos, facing_light, false, true);
        let fp_direct = spot_diffuse_display(&light, aimed_pos, facing_light, true, true);
        let wall_near_direct = spot_diffuse_display(&light, near_wall, facing_light, false, true);
        let wall_far_direct = spot_diffuse_display(&light, far_wall, facing_light, false, true);

        // Reproduce the #3121 failing boundary: without the first-person scale, a bright
        // revolver (Ranger Sequoia silver, albedo 0.55) and gloved hand (0.35) blow out to white.
        assert_eq!(lit_clipped_linear(0.55, ambient, unscaled_direct)[0], 1.0, "unscaled revolver clipped");
        assert_eq!(lit_clipped_linear(0.35, ambient, unscaled_direct)[0], 1.0, "unscaled hand clipped");

        // With the fix, aimed-in revolver (0.55), energy weapon (0.45), gloved hand (0.35),
        // and rifle (0.25) all stay well below white clipping, receive gentle warm fill above
        // flashlight-off, and remain strictly dimmer than the lit world wall ahead.
        for (label, albedo) in [("revolver", 0.55), ("energy", 0.45), ("hand", 0.35), ("rifle", 0.25)] {
            let off = lit_clipped_linear(albedo, ambient, [0.0; 3]);
            let on = lit_clipped_linear(albedo, ambient, fp_direct);
            let wall_near = lit_clipped_linear(albedo, ambient, wall_near_direct);
            let wall_far = lit_clipped_linear(albedo, ambient, wall_far_direct);
            for c in 0..3 {
                assert!(on[c] < 0.75, "{label} channel {c} must not wash out: {on:?}");
                assert!(on[c] > off[c], "{label} channel {c} keeps subtle warm fill over off: {on:?} vs {off:?}");
                assert!(on[c] < wall_near[c] && on[c] < wall_far[c], "{label} channel {c} stays dimmer than lit world wall: {on:?} vs near {wall_near:?} / far {wall_far:?}");
            }
        }

        // Specular on first-person shapes is attenuated as well to stop white highlights.
        let fp_spec = crate::lights::spot_specular_scale(true, true);
        assert_eq!(fp_spec, crate::lights::FIRST_PERSON_FLASHLIGHT_SPECULAR);
        assert!(fp_spec < 0.25);
    }

    #[test]
    fn non_aimed_third_person_world_and_world_spot_controls_hold() {
        let eye = [0.0, 0.0, 120.0];
        let light = placement(eye, 0.0, 0.0);
        let facing_light = -light.forward().as_vec3();

        // Non-aimed hip-held weapon sits off to the right and below the beam axis: weaker than aimed-in.
        let aimed_pos = light.translation + light.forward().as_vec3() * 25.0;
        let hip_pos = light.translation + light.forward().as_vec3() * 25.0 + light.right().as_vec3() * 11.0 - light.up().as_vec3() * 6.0;
        let aimed = spot_diffuse_display(&light, aimed_pos, facing_light, true, true);
        let hip = spot_diffuse_display(&light, hip_pos, facing_light, true, true);
        assert!(hip[0] < aimed[0] && hip[0] >= 0.0, "hip {hip:?} weaker than aimed {aimed:?}");

        // Third-person body/weapon (BODY_LAYER) and world surfaces (!FIRST_PERSON_DEPTH)
        // keep full flashlight diffuse and specular strength.
        assert_eq!(crate::lights::spot_display_scale(false, true), 1.0);
        assert_eq!(crate::lights::spot_specular_scale(false, true), 1.0);
        let world_wall = spot_diffuse_display(&light, light.translation + light.forward().as_vec3() * 200.0, facing_light, false, true);
        assert!(world_wall[0] > BRIGHTNESS * 0.98, "world illumination unchanged: {world_wall:?}");

        // Non-shadow-casting world spot lamps (LIGH SPOT records) still light first-person
        // weapons and hands at full strength.
        assert_eq!(crate::lights::spot_display_scale(true, false), 1.0);
        assert_eq!(crate::lights::spot_specular_scale(true, false), 1.0);
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
        // The owner's example: a green point light (here with the Pip-Boy row on and a
        // green Pip-Boy, resolved by the sender) and a yellow narrow spot.
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
}
