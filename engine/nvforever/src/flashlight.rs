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
    match made(&beam, color, intensity) {
        Made::Spot(l) => e.insert(l),
        Made::Point(l) => e.insert(l),
    };
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
        direct.map(|d| (albedo * (ambient + d).max(0.0).powf(2.2)).min(1.0))
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
}
