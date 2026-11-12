//! The arm-wrestling match's camera (queue item 199; the bout is `nvf_play::armwrestle`):
//! once the player sits down to a match, the game's camera looks at the clasped hands from
//! behind the player's side of the table, three-quarters round, the opponent across them
//! [design: the lead's spec 2026-11-11; the original game has no arm wrestling].
//!
//! - **Where it looks.** At the clasped hands: the player's own `Bip01 R Hand` as the arm
//!   pose (`armwrestle_arm`) puts it, a little on into the grip, followed slowly
//!   ([`AIM_FOLLOW`]) so the view goes with the lean and the pin without shaking with each
//!   tap; without the bone, the middle between the stools [`CLASP_UP`] over the table.
//! - **Where it stands.** Placed from the hands, not from the player's head: [`REACH`] from
//!   them, round from straight back along the table's axis towards one side ([`ANGLE`] to
//!   the player's right, [`ANGLE_LEFT`] to the left), and
//!   [`RISE`] above them, so it looks about 28 degrees down ([`spot`]). From behind the head
//!   (run16, armwrestle-a's elbows-planted pose) the wrestlers' heads, bent down to the
//!   hands' height within about 13 of them, covered the clasp from any shoulder offset; from
//!   this angle the line to the hands passes well clear of the player's own head and body.
//! - **Which side.** The one away from the wall: the side the camera is pulled in less on
//!   (the right, the gripping arm's, when both are clear), chosen as it takes the view and
//!   again [`SIDE_AGAIN`] later, once the hands are in place.
//! - **Walls.** Pulled in towards the hands in front of anything between the two
//!   ([`WALL_MARGIN`]), as the third-person camera is (`nvf_play::control::place_camera`).
//! - **In and out.** It eases in from where the camera was over [`EASE_IN`]. It stays
//!   through getting up, until the screen closes; the player's own view comes back then
//!   (first person, the eye), so the change never passes through the table.
//! - The camera counts as a third-person view while it has the view
//!   (`PlayerControl::scene_cam`), so the player's body is drawn and the first-person
//!   arms are not, as at the slot machine (`slots_view.rs`). The mouse does not move it.
//!
//! Log: `arm wrestle view: ...` when it takes the view (the side and the spot), once the
//! bout is under way (where the hands are, the camera's spot and pitch), and `arm wrestle
//! view: the player's camera again`.

use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::armwrestle::ArmScreen;
use nvf_play::control::PlayerControl;
use nvf_play::furniture::PlayerSeat;
use nvf_play::phys::PhysState;
use nvf_play::sim::SimState;
use nvf_play::space::{look_rotation, to_bevy};

/// From the hands to the camera, level, game units [design: run16 plan; the players'
/// bodies in view to the waist].
pub const REACH: f32 = 95.0;
/// Round from straight back along the table's axis to the player's right, radians (70
/// degrees) [design: run16, each wrestler's head is bent down to the hands' height, 12 to 13
/// behind them and 8 to 10 to its own left; from the right the player's own head is beyond
/// the hands, and the line passes about 15 from the opponent's].
pub const ANGLE: f32 = 70.0 * std::f32::consts::PI / 180.0;
/// The same to the player's left, radians (110 degrees, a little past side on) [design: the
/// player's own head leans to that side; run18, at 70 degrees it stood in front of the hands
/// in P2's view; at 110 the line passes about 15 from each head].
pub const ANGLE_LEFT: f32 = 110.0 * std::f32::consts::PI / 180.0;
/// Above the hands, game units: about 28 degrees down, so the line comes over a head bent
/// down level with the hands [design: the lead asked for 10-15 degrees; shallower, the
/// heads come in front of the hands; run17, at 40 (23 degrees) the winner's face came
/// beside the hands near the pin].
pub const RISE: f32 = 50.0;
/// Seconds after taking the view that the side is chosen again, from where the hands are by
/// then [design: run17, chosen at once the hand was still down by the seat, 75 from the
/// table, and P2's camera went into the north wall].
pub const SIDE_AGAIN: f32 = 1.5;
/// The clasped hands' middle over the table's top without the hand bone, game units
/// [observed: run16, the right hand at z 3527, 19.6 over the top].
pub const CLASP_UP: f32 = 20.0;
/// How close the camera may come to what is behind it, game units (the third-person
/// camera's margin, control.rs) [engineering choice].
pub const WALL_MARGIN: f32 = 8.0;
/// Seconds for the camera to come in from where it was [design].
pub const EASE_IN: f32 = 0.6;
/// How fast the aim follows the hands, per second [design].
pub const AIM_FOLLOW: f32 = 3.0;

/// The camera's spot and its heading and pitch (game space, radians; pitch up positive)
/// looking at `clasp` for a player sat facing `toward` (unit, level), round to their right
/// (`right`) or left.
pub fn spot(clasp: [f32; 3], toward: [f32; 2], right: bool) -> ([f32; 3], f32, f32) {
    // A player facing `toward` has their right at (y, −x).
    let side = if right { [toward[1], -toward[0]] } else { [-toward[1], toward[0]] };
    let (sn, cs) = if right { ANGLE } else { ANGLE_LEFT }.sin_cos();
    let out = [-toward[0] * cs + side[0] * sn, -toward[1] * cs + side[1] * sn];
    let at = [clasp[0] + out[0] * REACH, clasp[1] + out[1] * REACH, clasp[2] + RISE];
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
    /// Round to the player's right (else the left), chosen once as it takes the view.
    right: bool,
    /// The framing under way has been said in the log.
    told: bool,
    /// The side has been chosen again from the hands' place ([`SIDE_AGAIN`]).
    settled: bool,
    from: Option<Transform>,
    start: f32,
    /// Where it looks, following the hands.
    aim: Option<[f32; 3]>,
    /// What it put on the camera last frame.
    last: Option<Transform>,
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
    // The drawn body's right hand (last frame's), game space, if it is at this seat (not on
    // a body still standing somewhere else).
    let hand = body.as_deref().and_then(|b| b.third_rig()).and_then(|(_, rig)| {
        let i = rig.skel.names.iter().position(|n| n.as_deref().is_some_and(|n| n.eq_ignore_ascii_case("Bip01 R Hand")))?;
        let g = globals.get(*rig.nodes.get(i)?).ok()?.translation();
        // Bevy (x, y up, z) to game (x, -z, y), as `to_bevy` maps the other way.
        let p = nvf_play::space::to_game(g);
        let near = ((p[0] - s[0]).powi(2) + (p[1] - s[1]).powi(2)).sqrt() < 80.0 && p[2] > floor + 20.0 && p[2] < floor + 140.0;
        near.then_some(p)
    });
    // The bodies sit off the line between the stools' references (run14: 45 to its side),
    // so the table's middle is moved across by as much as the hand is.
    let across = [-toward[1], toward[0]];
    let off = hand.map_or(0.0, |h| (h[0] - s[0]) * across[0] + (h[1] - s[1]) * across[1]);
    let between = [(s[0] + o[0]) / 2.0 + across[0] * off, (s[1] + o[1]) / 2.0 + across[1] * off, top + CLASP_UP];
    // The hands: this player's right hand (the wrist), a little on into the grip.
    let target = hand.map_or(between, |h| [h[0] + toward[0] * 3.0, h[1] + toward[1] * 3.0, h[2]]);
    let k = (time.delta_secs() * AIM_FOLLOW).min(1.0);
    let clasp = match view.aim.filter(|_| view.on == Some(a.stool)) {
        Some(p) => [p[0] + (target[0] - p[0]) * k, p[1] + (target[1] - p[1]) * k, p[2] + (target[2] - p[2]) * k],
        None => target,
    };
    view.aim = Some(clasp);
    let filter = fnv_phys::Filter { exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()), ..Default::default() };
    // From `from` out to the spot, pulled in front of whatever is between (a wall): the spot
    // reached and how far short of the wanted one it is.
    let mut reach = |from: [f32; 3], want: [f32; 3]| -> ([f32; 3], f32) {
        let d = [want[0] - from[0], want[1] - from[1], want[2] - from[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let Some(phys) = phys.as_deref_mut().filter(|_| l > 1e-3) else { return (want, 0.0) };
        let u = [d[0] / l, d[1] / l, d[2] / l];
        match phys.ray(from, u, l + WALL_MARGIN, filter.clone()) {
            Some(h) => {
                let r = (h.distance - WALL_MARGIN).clamp(0.0, l);
                ([from[0] + u[0] * r, from[1] + u[1] * r, from[2] + u[2] * r], l - r)
            }
            None => (want, 0.0),
        }
    };
    let again = view.on == Some(a.stool) && !view.settled && now - view.start > SIDE_AGAIN;
    if view.on != Some(a.stool) || again {
        // The side away from the wall, tested from the hands: the one pulled in less (the
        // right, the gripping arm's, when both are clear) [design: the lead, ship10, P2's
        // camera in the north wall]. Chosen as it takes the view, and again once the hands
        // are in place; a change eases across.
        let r = reach(clasp, spot(clasp, toward, true).0).1;
        let l = reach(clasp, spot(clasp, toward, false).0).1;
        let right = r <= l + 1.0;
        info!("arm wrestle view: pulled in {r:.0} round to the right, {l:.0} to the left: the {}", if right { "right" } else { "left" });
        if again {
            view.settled = true;
            if right != view.right {
                view.from = view.last.or(Some(*t));
                view.start = now;
            }
        }
        view.right = right;
    }
    let (want, heading, pitch) = spot(clasp, toward, view.right);
    let (out, pulled) = reach(clasp, want);
    if view.on == Some(a.stool) && now - view.start > 4.0 && !std::mem::replace(&mut view.told, true) {
        info!(
            "arm wrestle view: at the {} {clasp:.0?}: camera {out:.0?} pulled in {pulled:.0}, heading {:.0} pitch {:.0} deg",
            if hand.is_some() { "right hand" } else { "table's middle" },
            heading.to_degrees(),
            pitch.to_degrees()
        );
    }
    let to = Transform { translation: to_bevy(out), rotation: look_rotation(heading, pitch), scale: Vec3::ONE };
    if view.on != Some(a.stool) {
        *view = View { on: Some(a.stool), right: view.right, told: false, settled: false, from: Some(*t), start: now, aim: view.aim, last: None };
        control.scene_cam = true;
        info!("arm wrestle view: at {:08X}, looking at {clasp:.0?} from {out:.0?} pulled in {pulled:.0}, heading {:.0} pitch {:.0} deg", a.stool.0, heading.to_degrees(), pitch.to_degrees());
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
    view.last = Some(*t);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The level distance from `p` to the segment from `a` to `b`.
    fn off_line(p: [f32; 2], a: [f32; 3], b: [f32; 3]) -> f32 {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        ((a[0] + dx * t - p[0]).powi(2) + (a[1] + dy * t - p[1]).powi(2)).sqrt()
    }

    #[test]
    fn the_camera_stands_behind_to_the_side_and_looks_down_onto_the_hands() {
        // Sat at x 312 facing east (+x); the hands 38 ahead of the seat.
        let clasp = [350.0, 337.0, 3527.0];
        let (at, heading, pitch) = spot(clasp, [1.0, 0.0], true);
        // A little behind (west of) the hands and well to the player's right (south, −y).
        assert!(at[0] < 350.0 - 20.0 && at[1] < 337.0 - 80.0, "{at:?}");
        assert!((at[2] - clasp[2] - RISE).abs() < 1e-3);
        // Looking north and a little east at them, 15 to 30 degrees down.
        assert!(heading.to_degrees() > 0.0 && heading.to_degrees() < 90.0, "{}", heading.to_degrees());
        assert!(pitch.to_degrees() < -15.0 && pitch.to_degrees() > -30.0, "{}", pitch.to_degrees());
        // The left: a little in front of the hands (east), well to the player's left (north).
        let (left, heading, _) = spot(clasp, [1.0, 0.0], false);
        assert!(left[0] > 350.0 + 20.0 && left[1] > 337.0 + 80.0, "{left:?}");
        assert!(heading.to_degrees() < -150.0 || heading.to_degrees() > 180.0, "{}", heading.to_degrees());
    }

    #[test]
    fn the_line_to_the_hands_passes_clear_of_the_players_head() {
        // Run16: the heads bent down to the hands' height, 12 to 13 back from them along
        // the table and up to 10 to the wrestler's own left (P1 facing east: north, +y);
        // the opponent's the same from his side (12 east of the hands, 10 south).
        let clasp = [350.0, 337.0, 3527.0];
        for right in [true, false] {
            let (at, ..) = spot(clasp, [1.0, 0.0], right);
            for (x, y) in [(-12.0, 10.0), (-13.0, 0.0), (-25.0, 10.0), (-38.0, 0.0), (12.0, -10.0), (13.0, 0.0)] {
                let d = off_line([350.0 + x, 337.0 + y], at, clasp);
                assert!(d > 12.0, "{right} {x} {y}: {d}");
            }
        }
        // Facing west, the right is north.
        let (at, ..) = spot([350.0, 337.0, 3527.0], [-1.0, 0.0], true);
        assert!(at[0] > 350.0 && at[1] > 337.0, "{at:?}");
    }
}
