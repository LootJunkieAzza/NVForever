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
//! - Another co-op player's light ([`remote_spot`], `coop.rs`) is the same light on their
//!   puppet. The puppet is drawn on the world's layer 0, so its head, arms and weapon
//!   would cast into its own cone, as the player's own body did before it got
//!   `sky::BODY_LAYER`: a dark wedge swinging over the lit ground (owner's co-op video
//!   2026-11-12, "synced flashlights have some weird shading"). Its shadow map starts at
//!   [`REMOTE_NEAR`] instead, beyond what the puppet holds in front of it; walls farther
//!   than that still stop it.

use bevy::prelude::*;
use nvf_play::flashlight::Flashlight;
use nvf_play::player::Player;
use crate::player_body::PlayerBody;

/// Marks the flashlight's light entity.
#[derive(Component)]
pub struct PlayerFlashlight;

/// Warm white, as an incandescent bulb.
const COLOUR: [f32; 3] = [1.0, 0.95, 0.86];
/// Brightness at the light, display units (`lights::lumens`).
const BRIGHTNESS: f32 = 2.0;
/// Reach, game units (about 40 m).
const RANGE: f32 = 2800.0;
/// Half-angle of the cone's edge, and of its fully lit middle, degrees.
const OUTER_DEG: f32 = 30.0;
const INNER_DEG: f32 = 14.0;
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

pub struct FlashlightPlugin;

impl Plugin for FlashlightPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync.after(nvf_play::PlaySet::Physics));
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
    player: Option<Res<Player>>,
    body: Option<Res<PlayerBody>>,
    mut lights: Query<(Entity, &mut Transform), With<PlayerFlashlight>>,
) {
    let Some(player) = player.filter(|_| flashlight.on) else {
        for (e, _) in &lights {
            commands.entity(e).despawn();
        }
        return;
    };
    let place = placement(body.and_then(|b| b.head_eye()).unwrap_or_else(|| player.eye()), player.heading, player.pitch);
    if let Some((_, mut t)) = lights.iter_mut().next() {
        *t = place;
        return;
    }
    commands.spawn((Name::new("Flashlight"), PlayerFlashlight, spot(), place));
}

/// The flashlight's spot light.
pub(crate) fn spot() -> SpotLight {
    SpotLight {
        color: crate::lights::display_color(COLOUR),
        intensity: crate::lights::lumens(BRIGHTNESS, crate::sky::light_unit()),
        range: RANGE,
        outer_angle: OUTER_DEG.to_radians(),
        inner_angle: INNER_DEG.to_radians(),
        shadow_maps_enabled: true,
        shadow_map_near_z: NEAR,
        ..default()
    }
}

/// Another co-op player's flashlight: [`spot`], with shadows that start past their puppet.
pub(crate) fn remote_spot() -> SpotLight {
    SpotLight { shadow_map_near_z: REMOTE_NEAR, ..spot() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_players_light_matches_the_own_but_casts_nothing_from_their_puppet() {
        let (own, other) = (spot(), remote_spot());
        // The same light: colour, brightness, reach, cone, shadows on.
        assert_eq!(own.color, other.color);
        assert_eq!((own.intensity, own.range, own.outer_angle, own.inner_angle), (other.intensity, other.range, other.outer_angle, other.inner_angle));
        assert!(other.shadow_maps_enabled);
        // Its shadows start past the puppet's face and what it holds out aiming: the
        // light sits AHEAD of the eye, a rifle's muzzle about 60 units.
        assert!(other.shadow_map_near_z > 60.0 - AHEAD, "{}", other.shadow_map_near_z);
        assert!(other.shadow_map_near_z < 100.0, "walls a step away still stop it");
        assert_eq!(own.shadow_map_near_z, NEAR, "the own light keeps its near plane");
    }

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
        COLOUR.map(|c| c * factor)
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
}
