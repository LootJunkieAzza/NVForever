//! The character creator's picture (docs/spec-notes/character-creator.md §5): while its
//! camera is up (`nvf_play::control::PlayerControl::portrait`) the player is lit by two
//! lights of the engine's own that go with the camera, and actors standing between the
//! camera and the player are hidden. Both end when the creator closes: the lights are
//! removed and the actors shown again, so the scene's lighting and cast are as before.
//!
//! - **Light** [owner 2026-10-17: "we need some kind of light its hard to see details you
//!   are changing in the shadow"; design: the rig]: a warm key light above and to the
//!   camera's left, a dimmer cool fill to its right and lower, both point lights without
//!   shadows, their brightness in the units of the game's own lights (`lights::lumens`,
//!   scaled by `--light-scale` like them).
//! - **Nothing in the way** [owner 2026-10-17: Doc Mitchell in his chair stood between the
//!   camera and the player; design: hide, not move]: an actor whose root lies within
//!   [`CLEAR_RADIUS`] of the line from the camera to the point looked at, nearer the
//!   camera than the player, is hidden while it is in the way. The player is not moved:
//!   the opening's scene goes on round them (§6 of the note).
//!
//! Log lines start `creator view:`.

use crate::limbs::ActorRig;
use bevy::prelude::*;
use nvf_play::control::{BodyView, PlayerControl};
use nvf_play::player::Player;
use nvf_play::space;

/// How far from the line of sight an actor's root may stand before it is hidden, game
/// units [design: a seated or standing body's half width and a margin].
pub const CLEAR_RADIUS: f32 = 40.0;
/// The lights' brightness (a LIGH record's fade) and reach [design].
const KEY: f32 = 1.6;
const FILL: f32 = 0.55;
const RANGE: f32 = 320.0;

#[derive(Component)]
struct CreatorLight;

/// The actors hidden because they stood in the way, with their visibility before.
#[derive(Resource, Default)]
struct Hidden(Vec<(Entity, Visibility)>);

pub struct CreatorViewPlugin {
    /// `--light-scale`.
    pub light_scale: f32,
}

#[derive(Resource)]
struct LightScale(f32);

impl Plugin for CreatorViewPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LightScale(self.light_scale)).init_resource::<Hidden>().add_systems(PostUpdate, (lights, clear_view).before(TransformSystems::Propagate));
    }
}

/// The eye the creator's camera aims from: the drawn body's, as the camera's
/// (`nvf_play::control::place_camera`).
fn eye_of(p: &Player, body: Option<&BodyView>) -> [f32; 3] {
    body.and_then(BodyView::portrait_eye).unwrap_or_else(|| p.eye())
}

/// The camera's place and the point it looks at, Bevy space, while the creator is open.
fn view(control: &PlayerControl, eye: [f32; 3], facing: f32) -> Option<(Vec3, Vec3, f32)> {
    let p = control.portrait?;
    let (at, heading, _) = p.spot(eye, facing);
    let target = [eye[0], eye[1], eye[2] - p.drop];
    // The target as the camera sees it: shifted with the camera, so the lights stay on
    // the player and the line of sight runs to the picture's middle.
    let right = [(heading).cos(), -(heading).sin()];
    let seen = [target[0] - right[0] * p.shift, target[1] - right[1] * p.shift, target[2]];
    Some((space::to_bevy(at), space::to_bevy(seen), heading))
}

fn lights(
    mut commands: Commands,
    control: Option<Res<PlayerControl>>,
    player: Option<Res<Player>>,
    body: Option<Res<BodyView>>,
    scale: Res<LightScale>,
    mut rig: Query<(Entity, &mut Transform), With<CreatorLight>>,
) {
    let want = control.as_deref().zip(player.as_deref()).and_then(|(c, p)| view(c, eye_of(p, body.as_deref()), p.heading).map(|v| (v, p)));
    let Some(((camera, _, heading), p)) = want else {
        if !rig.is_empty() {
            for (e, _) in &rig {
                commands.entity(e).despawn();
            }
            info!("creator view: lights off");
        }
        return;
    };
    // Around the player's face: the key up and to the camera's left, the fill to its
    // right and lower, both on the camera's side of the player.
    let eye = space::to_bevy(eye_of(p, body.as_deref()));
    let toward = (camera - eye).with_y(0.0).normalize_or(Vec3::Z);
    let right = Vec3::new(heading.cos(), 0.0, heading.sin());
    // `right` in Bevy space: game x is Bevy x, game y is Bevy −z.
    let places = [eye + toward * 55.0 - right * 45.0 + Vec3::Y * 40.0, eye + toward * 60.0 + right * 55.0 - Vec3::Y * 10.0];
    if rig.is_empty() {
        let unit = crate::sky::light_unit();
        for (i, (f, c)) in [(KEY, Color::srgb(1.0, 0.94, 0.85)), (FILL, Color::srgb(0.82, 0.88, 1.0))].into_iter().enumerate() {
            let intensity = crate::lights::lumens(f * scale.0, unit);
            commands.spawn((Name::new("Creator light"), CreatorLight, PointLight { color: c, intensity, range: RANGE, shadow_maps_enabled: false, ..default() }, Transform::from_translation(places[i])));
        }
        info!("creator view: lights on (key {KEY}, fill {FILL}, reach {RANGE})");
        return;
    }
    for (i, (_, mut t)) in rig.iter_mut().enumerate() {
        t.translation = places[i.min(1)];
    }
}

fn clear_view(control: Option<Res<PlayerControl>>, player: Option<Res<Player>>, body: Option<Res<BodyView>>, mut hidden: ResMut<Hidden>, mut actors: Query<(Entity, &ActorRig, &GlobalTransform, &mut Visibility)>) {
    let line = control.as_deref().zip(player.as_deref()).and_then(|(c, p)| view(c, eye_of(p, body.as_deref()), p.heading));
    let in_way = |pos: Vec3| -> bool {
        let Some((camera, target, _)) = line else { return false };
        let d = target - camera;
        let len = d.length();
        if len < 1.0 {
            return false;
        }
        let k = (pos - camera).dot(d) / len;
        // Between the camera and the player (with a little room before the player).
        if k < 0.0 || k > len - 15.0 {
            return false;
        }
        let closest = camera + d / len * k;
        // Its root is at its feet: a standing or sitting body reaches 120 units up.
        let up = (closest.y - pos.y).clamp(0.0, 120.0);
        (pos + Vec3::Y * up - closest).length() < CLEAR_RADIUS
    };
    for (e, rig, t, mut v) in &mut actors {
        if rig.reference == fnv_sim::FormId::PLAYER_REF.0 {
            continue;
        }
        let was = hidden.0.iter().position(|(h, _)| *h == e);
        match (in_way(t.translation()), was) {
            (true, None) => {
                hidden.0.push((e, *v));
                *v = Visibility::Hidden;
                info!("creator view: {:08X} hidden (in the way)", rig.reference);
            }
            (false, Some(i)) => {
                let (_, before) = hidden.0.remove(i);
                *v = before;
                info!("creator view: {:08X} shown again", rig.reference);
            }
            _ => {}
        }
    }
    // Actors gone meanwhile.
    hidden.0.retain(|(e, _)| actors.contains(*e));
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvf_play::control::Portrait;

    #[test]
    fn the_view_runs_from_the_camera_to_the_player() {
        let mut c = PlayerControl::default();
        let eye = [0.0, 0.0, 120.0];
        assert!(view(&c, eye, 0.0).is_none());
        c.portrait = Some(Portrait { yaw: 0.0, distance: 50.0, drop: 0.0, pitch: 0.0, shift: 0.0 });
        let (camera, target, _) = view(&c, eye, 0.0).unwrap();
        // Facing +y: the camera is 50 in front (Bevy −z), level with the eye.
        assert!((camera - target).length() > 49.0 && (camera.z - target.z + 50.0).abs() < 1e-3, "{camera} {target}");
    }
}
