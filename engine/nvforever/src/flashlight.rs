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
}
