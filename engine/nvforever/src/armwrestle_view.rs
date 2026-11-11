//! The arm-wrestling match's camera (queue item 199; the bout is `nvf_play::armwrestle`):
//! once the player sits down to a match, the game's camera goes close over the player's
//! shoulder, looking across the table at the clasped hands, the opponent behind them
//! [design: the lead's spec 2026-11-11, after the run12 shots showed a wide room view with
//! the clasp hidden behind the player's head; the original game has no arm wrestling].
//!
//! - **Where it stands.** [`BACK`] behind the seated player's head along the table's axis,
//!   [`SIDE`] to one side and [`UP`] above it: the shoulder away from the wall, the side
//!   the camera is pulled in less on (the left when both are clear); the head is the drawn body's `Bip01 Head`
//!   (leaning in over the table with the arm, `armwrestle_arm`), else a seated head over the
//!   stool ([`SEATED_HEAD`]).
//! - **Where it looks.** At the clasp: the middle between the two stools, [`CLASP_UP`] over
//!   the table's top. From that spot it is about 10 to 15 degrees down.
//! - **Walls.** The camera is pulled in towards the head in front of anything between the
//!   two ([`WALL_MARGIN`]), as the third-person camera is (`nvf_play::control::place_camera`).
//! - **In and out.** It eases in from where the camera was over [`EASE_IN`]. It stays
//!   through getting up, until the screen closes; the player's own view comes back then
//!   (first person, the eye), so the change never passes through the table.
//! - The camera counts as a third-person view while it has the view
//!   (`PlayerControl::scene_cam`), so the player's body is drawn and the first-person
//!   arms are not, as at the slot machine (`slots_view.rs`). The mouse does not move it.
//!
//! Log: `arm wrestle view: over the shoulder at <stool> ...` once when it takes the view,
//! with the camera's spot and its pitch, and `arm wrestle view: the player's camera again`.

use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::armwrestle::ArmScreen;
use nvf_play::control::PlayerControl;
use nvf_play::furniture::PlayerSeat;
use nvf_play::phys::PhysState;
use nvf_play::sim::SimState;
use nvf_play::space::{look_rotation, to_bevy};

/// Behind the head along the table's axis, game units [design: the lead, "about 60-80"].
pub const BACK: f32 = 55.0;
/// To the head's side, game units: enough that the line from the camera to the clasp passes
/// beside the player's own head leaned in over the table (about 15 to its side there)
/// [design: run13, at 40 the leaned head still covered part of the clasp; the lead's run12
/// shot had it behind the head].
pub const SIDE: f32 = 50.0;
/// Above the head, game units [design: "head height or a little above"; run14, at 22 the
/// opponent, leaned in with his face near the hands, was in the middle of the view: from
/// about his head's height his face stands well above the clasp].
pub const UP: f32 = 6.0;
/// The clasped hands' middle over the table's top, game units [design: an upright forearm
/// and half a hand over the elbow's spot, as `armwrestle_arm::clasp` puts them].
pub const CLASP_UP: f32 = 28.0;
/// A seated head over the floor when the body's head bone is not found, game units
/// [observed: run14, the seated player's `Bip01 Head` at z 3567 over the saloon's floor at
/// 3456, leaned in over the table].
pub const SEATED_HEAD: f32 = 111.0;
/// How close the camera may come to what is behind it, game units (the third-person
/// camera's margin, control.rs) [engineering choice].
pub const WALL_MARGIN: f32 = 8.0;
/// Seconds for the camera to come in from where it was [design].
pub const EASE_IN: f32 = 0.6;

/// The camera's spot and its heading and pitch (game space, radians; pitch up positive)
/// for a seated head at `head`, sat facing `toward` (unit, level), looking at `clasp`, over
/// the left shoulder (`left`) or the right.
pub fn spot(head: [f32; 3], toward: [f32; 2], clasp: [f32; 3], left: bool) -> ([f32; 3], f32, f32) {
    // The left of a player facing `toward`: (−y, x).
    let k = if left { SIDE } else { -SIDE };
    let side = [-toward[1], toward[0]];
    let at = [head[0] - toward[0] * BACK + side[0] * k, head[1] - toward[1] * BACK + side[1] * k, head[2] + UP];
    let (heading, pitch) = aim(at, clasp);
    (at, heading, pitch)
}

/// The heading (0 north, π/2 east) and pitch (up positive) from `from` to `to`.
pub fn aim(from: [f32; 3], to: [f32; 3]) -> (f32, f32) {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let level = (d[0] * d[0] + d[1] * d[1]).sqrt();
    (d[0].atan2(d[1]), d[2].atan2(level))
}

#[derive(Default)]
pub struct View {
    on: Option<FormId>,
    /// Over the left shoulder (else the right), chosen once as it takes the view.
    left: bool,
    /// The body's head has been said in the log.
    told: bool,
    from: Option<Transform>,
    start: f32,
}

/// The arm-wrestling match's camera, after the player's camera has been placed this frame.
#[allow(clippy::too_many_arguments)]
pub fn camera(
    time: Res<Time>,
    screen: Option<Res<ArmScreen>>,
    seat: Option<Res<PlayerSeat>>,
    sim: Option<Res<SimState>>,
    body: Option<Res<crate::player_body::PlayerBody>>,
    globals: Query<&GlobalTransform>,
    mut phys: Option<ResMut<PhysState>>,
    mut control: ResMut<PlayerControl>,
    mut cams: Query<&mut Transform, With<nvf_play::PlayerCamera>>,
    mut view: Local<View>,
) {
    let now = time.elapsed_secs();
    // Seated at this match's stool; once it has the view it keeps it while the player gets
    // up, until the screen closes and the player's own view (first person) comes back, so
    // the change never passes through the table (the lead's ship10 run: the third-person
    // camera dropped inside the table's edge after the bout).
    let arm = screen.as_deref().and_then(|s| s.0.as_ref());
    let sat = seat.as_deref().and_then(|s| s.on.filter(|_| s.hold.is_some()));
    let mine = arm.filter(|a| sat == Some(a.stool.0) || view.on == Some(a.stool));
    let (Some(a), Some(sim), Ok(mut t)) = (mine, sim.as_deref(), cams.single_mut()) else {
        if view.on.take().is_some() {
            control.scene_cam = false;
            *view = View::default();
            info!("arm wrestle view: the player's camera again");
        }
        return;
    };
    let at = |r: FormId| sim.world.peek(r).map(|x| x.pos).or_else(|| sim.world.placed.get(r).map(|p| p.pos));
    let (Some(s), Some(o)) = (at(a.stool), at(a.other)) else { return };
    let (dx, dy) = (o[0] - s[0], o[1] - s[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let toward = [dx / len, dy / len];
    let floor = fnv_game::armwrestle_table::FLOOR_Z;
    let top = fnv_game::armwrestle_table::TABLE_TOP_Z;
    // The drawn body's head (last frame's, leaned in), else one over the stool.
    let bone = body.as_deref().and_then(|b| b.third_rig()).and_then(|(_, rig)| {
        let i = rig.skel.names.iter().position(|n| n.as_deref().is_some_and(|n| n.eq_ignore_ascii_case("Bip01 Head")))?;
        let g = globals.get(*rig.nodes.get(i)?).ok()?.translation();
        // Bevy (x, y up, z) to game (x, -z, y), as `to_bevy` maps the other way.
        let p = nvf_play::space::to_game(g);
        // Only a head that is at this seat (not one still standing up somewhere else).
        let near = ((p[0] - s[0]).powi(2) + (p[1] - s[1]).powi(2)).sqrt() < 60.0 && p[2] > floor + 40.0 && p[2] < floor + 140.0;
        near.then_some(p)
    });
    let head = bone.unwrap_or([s[0], s[1], floor + SEATED_HEAD]);
    // The clasp is over the middle between the two wrestlers' shoulders
    // (`armwrestle_arm`): the bodies sit off the line between the stools' references (run14:
    // 45 to its side), so the middle is moved across by as much as this head is.
    let across = [-toward[1], toward[0]];
    let off = (head[0] - s[0]) * across[0] + (head[1] - s[1]) * across[1];
    let clasp = [(s[0] + o[0]) / 2.0 + across[0] * off, (s[1] + o[1]) / 2.0 + across[1] * off, top + CLASP_UP];
    let filter = fnv_phys::Filter { exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()), ..Default::default() };
    // From the head out to the spot, pulled in front of whatever is between (a wall, the
    // table): the spot reached and how far short of the wanted one it is.
    let mut reach = |want: [f32; 3]| -> ([f32; 3], f32) {
        let d = [want[0] - head[0], want[1] - head[1], want[2] - head[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let Some(phys) = phys.as_deref_mut().filter(|_| l > 1e-3) else { return (want, 0.0) };
        let u = [d[0] / l, d[1] / l, d[2] / l];
        match phys.ray(head, u, l + WALL_MARGIN, filter.clone()) {
            Some(h) => {
                let r = (h.distance - WALL_MARGIN).clamp(0.0, l);
                ([head[0] + u[0] * r, head[1] + u[1] * r, head[2] + u[2] * r], l - r)
            }
            None => (want, 0.0),
        }
    };
    if view.on != Some(a.stool) {
        // The shoulder away from the wall: the side the camera is pulled in less on (the
        // left when both are clear) [design: the lead, ship10, P2's camera in the north wall].
        let short = |left: bool, reach: &mut dyn FnMut([f32; 3]) -> ([f32; 3], f32)| reach(spot(head, toward, clasp, left).0).1;
        let (l, r) = (short(true, &mut reach), short(false, &mut reach));
        view.left = l <= r + 1.0;
        info!("arm wrestle view: pulled in {l:.0} over the left shoulder, {r:.0} over the right: the {}", if view.left { "left" } else { "right" });
    }
    let (want, heading, pitch) = spot(head, toward, clasp, view.left);
    let (out, pulled) = reach(want);
    if bone.is_some() && view.on == Some(a.stool) && !std::mem::replace(&mut view.told, true) {
        info!("arm wrestle view: from the body's head {head:.0?}: camera {out:.0?} pulled in {pulled:.0}, heading {:.0} pitch {:.0} deg", heading.to_degrees(), pitch.to_degrees());
    }
    let to = Transform { translation: to_bevy(out), rotation: look_rotation(heading, pitch), scale: Vec3::ONE };
    if view.on != Some(a.stool) {
        *view = View { on: Some(a.stool), left: view.left, told: false, from: Some(*t), start: now };
        control.scene_cam = true;
        info!(
            "arm wrestle view: over the shoulder at {:08X}: head {:.0?} ({}), camera {:.0?} pulled in {:.0}, heading {:.0} pitch {:.0} deg",
            a.stool.0,
            head,
            if bone.is_some() { "the body's" } else { "over the stool" },
            out,
            pulled,
            heading.to_degrees(),
            pitch.to_degrees()
        );
    }
    if !control.scene_cam {
        control.scene_cam = true;
    }
    let f = ((now - view.start) / EASE_IN).clamp(0.0, 1.0);
    let f = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    *t = match view.from {
        Some(from) if f < 1.0 => Transform { translation: from.translation.lerp(to.translation, f), rotation: from.rotation.slerp(to.rotation, f), scale: Vec3::ONE },
        _ => to,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_is_behind_left_and_looks_down_onto_the_clasp() {
        // Sat at x 312 facing east (+x) across a table whose clasp is 38 ahead.
        let head = [312.0, 337.0, 3567.0];
        let clasp = [350.0, 337.0, 3456.0 + 51.7 + CLASP_UP];
        let (at, heading, pitch) = spot(head, [1.0, 0.0], clasp, true);
        // Behind (west of) the head, to its left (north, +y), a little above.
        assert!((at[0] - (312.0 - BACK)).abs() < 1e-3 && (at[1] - (337.0 + SIDE)).abs() < 1e-3 && (at[2] - (3567.0 + UP)).abs() < 1e-3, "{at:?}");
        // Looking east, a little south to the clasp, and 10 to 25 degrees down.
        assert!(heading.to_degrees() > 90.0 && heading.to_degrees() < 135.0, "{}", heading.to_degrees());
        assert!(pitch.to_degrees() < -10.0 && pitch.to_degrees() > -25.0, "{}", pitch.to_degrees());
    }

    #[test]
    fn the_other_side_mirrors_it() {
        let head = [388.0, 337.0, 3567.0];
        let clasp = [350.0, 337.0, 3536.0];
        let (at, heading, _) = spot(head, [-1.0, 0.0], clasp, true);
        // Facing west, the left is south.
        assert!(at[0] > 388.0 && at[1] < 337.0, "{at:?}");
        assert!((heading.to_degrees().rem_euclid(360.0) - 270.0).abs() < 45.0);
        // Over the right shoulder, facing west: north of the head.
        let (at, ..) = spot(head, [-1.0, 0.0], clasp, false);
        assert!(at[0] > 388.0 && at[1] > 337.0, "{at:?}");
    }

    #[test]
    fn the_line_to_the_clasp_passes_beside_the_head() {
        let head = [312.0, 337.0, 3567.0];
        let clasp = [350.0, 337.0, 3536.0];
        for left in [true, false] {
            let (at, ..) = spot(head, [1.0, 0.0], clasp, left);
            // Where the line from the camera to the clasp passes the head's x.
            let k = (head[0] - at[0]) / (clasp[0] - at[0]);
            let y = at[1] + (clasp[1] - at[1]) * k;
            assert!((y - head[1]).abs() > 12.0, "{left}: {}", y - head[1]);
        }
    }
}
