//! The flashlight's light (`nvf_play::flashlight`; owner request 2026-10-11): while it is
//! on, a spot light on the player's camera lights a cone ahead of the view. [design: not
//! a light of the game's; colour, reach, cone and brightness are chosen to look like other
//! games' flashlights, in the display units of `lights::spawn_light`.]
//!
//! - It sits a little right of and under the eye, so what it lights shows short shadows,
//!   and casts shadows: walls stop it instead of it lighting the rooms behind them. It
//!   falls off as the game's lights do (`lights.rs`).
//! - Shadow casters are the world's layer (0), as the light's own layers are: the
//!   first-person arms and weapon (`sky::ARMS_LAYER`) and the unseen body
//!   (`sky::SHADOW_LAYER`) cast none. In a view from outside, nothing nearer to the light
//!   than the player's eye and a margin casts ([`shadow_near`]): the body would otherwise
//!   throw its shadow over all it lights.
//! - Detection does not count it (`detection.rs`).

use bevy::prelude::*;
use nvf_play::control::PlayerControl;
use nvf_play::flashlight::Flashlight;
use nvf_play::player::Player;
use nvf_play::PlayerCamera;

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
/// Where the light sits in the camera's frame (right, up, back), game units.
const OFFSET: Vec3 = Vec3::new(9.0, -11.0, 0.0);
/// Shadow near plane in first person, game units.
const NEAR: f32 = 8.0;
/// What is this far beyond the eye in a view from outside still casts no shadow (the
/// body's depth from the eye, and the weapon held before it), game units.
const BODY_MARGIN: f32 = 60.0;

pub struct FlashlightPlugin;

impl Plugin for FlashlightPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync.after(nvf_play::PlaySet::Physics));
    }
}

/// The shadow near plane: in a view from outside, past the player's eye seen from the
/// light, so the body casts nothing.
fn shadow_near(outside: bool, light: Vec3, eye: Vec3) -> f32 {
    if outside {
        (light.distance(eye) + BODY_MARGIN).max(NEAR)
    } else {
        NEAR
    }
}

fn sync(
    mut commands: Commands,
    flashlight: Res<Flashlight>,
    control: Res<PlayerControl>,
    player: Option<Res<Player>>,
    cam: Query<(Entity, &GlobalTransform), With<PlayerCamera>>,
    mut lights: Query<(Entity, &mut SpotLight), With<PlayerFlashlight>>,
) {
    if !flashlight.on {
        for (e, _) in &lights {
            commands.entity(e).despawn();
        }
        return;
    }
    let Ok((cam_e, cam_t)) = cam.single() else { return };
    let light_at = cam_t.transform_point(OFFSET);
    let outside = control.third_person_view();
    let near = player.map_or(NEAR, |p| shadow_near(outside, light_at, nvf_play::space::to_bevy(p.eye())));
    if let Some((_, mut l)) = lights.iter_mut().next() {
        if (l.shadow_map_near_z - near).abs() > 1.0 {
            l.shadow_map_near_z = near;
        }
        return;
    }
    commands.spawn((
        Name::new("Flashlight"),
        PlayerFlashlight,
        SpotLight {
            color: Color::srgb(COLOUR[0], COLOUR[1], COLOUR[2]),
            intensity: crate::lights::lumens(BRIGHTNESS, crate::sky::light_unit()),
            range: RANGE,
            outer_angle: OUTER_DEG.to_radians(),
            inner_angle: INNER_DEG.to_radians(),
            shadow_maps_enabled: true,
            shadow_map_near_z: near,
            ..default()
        },
        // A camera looks down its -z, and so does a spot light.
        Transform::from_translation(OFFSET),
        ChildOf(cam_e),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_casts_no_shadow_in_a_view_from_outside() {
        let eye = Vec3::new(0.0, 120.0, 0.0);
        let cam = eye + Vec3::new(30.0, 10.0, 200.0);
        let near = shadow_near(true, cam, eye);
        assert!(near > cam.distance(eye) + 40.0, "{near}");
        assert_eq!(shadow_near(false, eye, eye), NEAR);
    }
}
