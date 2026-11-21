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
//!   of their own. It still lights them: a light lights every view its layers meet.
//! - Detection does not count it (`detection.rs`).
//! - The Tweaks tab's Flashlight rows ([`FlashlightLook`], registered in `settings.rs`)
//!   set its colour, its type (Bevy's `SpotLight` or `PointLight`), whether it casts
//!   shadows, its reach, its brightness and a spot's beam; at their defaults it is the
//!   light above [design: not in the original, whose Pip-Boy light is no such light; the
//!   rows are those flashlight mods of other games commonly offer].

use bevy::prelude::*;
use nvf_play::flashlight::Flashlight;
use nvf_play::player::Player;
use crate::player_body::PlayerBody;

/// Marks the flashlight's light entity.
#[derive(Component)]
pub struct PlayerFlashlight;

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
    /// is the Pip-Boy's colour [owner statement 2026-11-21]. Off by default [design].
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
    /// The light as it is drawn.
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
/// Game units to a metre, as the Pip-Boy's figures and the Range row count them.
const UNITS_PER_METRE: f32 = 70.0;
/// The shadows' depth bias, game units: Bevy's defaults put back on the metre they were
/// chosen for. Bevy moves a surface this far towards the light, as a distance in the
/// world, before it tests the shadow map; its defaults (0.08 for a point light, 0.02 for
/// a spot) are metres, about a millimetre in game units, so a lit surface shadowed itself
/// in fine stripes, and the point light's cube faces showed as lighter and darker blocks
/// (owner report 2026-11-21) [design]. The normal bias is scaled to the map's texels by
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
/// heading and pitch.
fn placement(eye: [f32; 3], heading: f32, pitch: f32) -> Transform {
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
    let color = crate::lights::display_color(beam.colour);
    let intensity = crate::lights::lumens(beam.brightness, crate::sky::light_unit());
    let mut e = commands.spawn((Name::new("Flashlight"), PlayerFlashlight, MadeWith(beam), place));
    match beam.kind {
        LightKind::Spot => e.insert(SpotLight {
            color,
            intensity,
            range: beam.range,
            outer_angle: beam.outer_deg.to_radians(),
            inner_angle: beam.inner_deg.to_radians(),
            shadow_maps_enabled: beam.shadows,
            shadow_map_near_z: NEAR,
            shadow_depth_bias: SPOT_DEPTH_BIAS,
            ..default()
        }),
        LightKind::Point => e.insert(PointLight {
            color,
            intensity,
            range: beam.range,
            shadow_maps_enabled: beam.shadows,
            shadow_map_near_z: NEAR,
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
    fn the_types_are_named_as_the_list_shows_them() {
        assert_eq!(LightKind::ALL.map(LightKind::name), ["Spot", "Point"]);
        assert_eq!(LightKind::from_name(" point "), Some(LightKind::Point));
        assert_eq!(LightKind::from_name("Area"), None);
    }
}
