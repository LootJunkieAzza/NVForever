//! The player's camera for JohnnyGuitar's camera functions (`fnv_game::nvse_jg_nif`):
//! after the camera is placed (`control::place_camera`), the scripts' overrides of the
//! third-person camera (SetCameraTranslate, SetCameraRotate) are put on it, and the
//! camera's place, angles and field of view are told to the game for
//! GetCameraTranslation, WorldToScreen and GetPlayerCamFOV. The fly camera, when on, is
//! left alone (`free_cam` puts the view there after this).

use crate::control::PlayerControl;
use crate::player::Player;
use crate::script::ScriptState;
use crate::{space, PlayerCamera, PlaySet};
use bevy::prelude::*;
use fnv_game::functions::camera::CameraView;

pub struct ScriptCameraPlugin;

impl Plugin for ScriptCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, script_camera.in_set(PlaySet::Physics).after(crate::control::place_camera));
    }
}

/// The heading (radians, clockwise from north) and pitch (up positive, as
/// `space::look_rotation`'s) from `from` to `to`, game space.
fn aim(from: [f32; 3], to: [f32; 3]) -> (f32, f32) {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let flat = (d[0] * d[0] + d[1] * d[1]).sqrt();
    (d[0].atan2(d[1]), d[2].atan2(flat.max(1e-6)))
}

fn script_camera(mut script: Option<ResMut<ScriptState>>, control: Res<PlayerControl>, player: Option<Res<Player>>, mut cams: Query<(&mut Transform, &Projection), With<PlayerCamera>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let Ok((mut t, projection)) = cams.single_mut() else { return };
    let third = control.third_person_view();
    let (fly, over, rot, track) = (rt.game.ui.camera.fly_on, rt.game.ui.camera.translate_override, rt.game.ui.camera.rotate_override, rt.game.ui.camera.rotate_track);
    if third && !fly {
        if let (Some(o), Some(p)) = (over, player.as_deref()) {
            // The player's frame: x right, y ahead, z up, turned by the heading.
            let (s, c) = p.heading.sin_cos();
            let at = [p.feet[0] + o[0] * c + o[1] * s, p.feet[1] - o[0] * s + o[1] * c, p.feet[2] + o[2]];
            t.translation = space::to_bevy(at);
        }
        if rot.iter().any(Option::is_some) || track.is_some() {
            let (yaw, mut pitch, mut roll) = t.rotation.to_euler(EulerRot::YXZ);
            let mut heading = -yaw;
            if let Some(target) = track.and_then(|r| rt.game.ref_pos_rot(r)).map(|(p, _)| p) {
                (heading, pitch) = aim(space::to_game(t.translation), target);
            }
            // X as the game's angles have it, down positive.
            if let Some(x) = rot[0] {
                pitch = -x.to_radians();
            }
            if let Some(y) = rot[1] {
                roll = y.to_radians();
            }
            if let Some(z) = rot[2] {
                heading = z.to_radians();
            }
            t.rotation = Quat::from_euler(EulerRot::YXZ, -heading, pitch, roll);
        }
    }
    let (yaw, pitch, roll) = t.rotation.to_euler(EulerRot::YXZ);
    let (fov_y, aspect) = match projection {
        Projection::Perspective(p) => (p.fov, p.aspect_ratio),
        _ => (std::f32::consts::FRAC_PI_2, 16.0 / 9.0),
    };
    rt.game.ui.camera.view = Some(CameraView { pos: space::to_game(t.translation), heading: -yaw, pitch, roll, fov_y, aspect, first_person: !third });
}
