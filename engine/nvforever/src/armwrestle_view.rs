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
//!   them, side on to the table ([`ANGLE`] round from straight back along its axis, to the
//!   player's right or left), and [`RISE`] above them, so it looks about 32 degrees down
//!   ([`spot`]): the two wrestlers face each other across the frame in profile, the clasp
//!   between their faces. From behind the head (run16) the heads, bent down to the hands'
//!   height, covered the clasp; from three-quarters behind (70 degrees, run19 to run20 on
//!   armwrestle-a's 6e3a810a7 pose) the clasp sat just beside the player's face and read as
//!   the player's hand at the mouth [the lead, 2026-11-12].
//! - **Which side.** The one away from the wall: the side the camera is pulled in less on
//!   (the right when both are clear), chosen as it takes the view and again [`SIDE_AGAIN`]
//!   later, once the hands are in place.
//! - **The win.** Once the bout has a winner the camera eases ([`EASE_IN`]) up on the same
//!   side, keeping its heading ([`top_spot`]). The winner presses to his own left (the
//!   right hands clasped). A pin towards the camera: [`TOP_REACH`] out and [`TOP_RISE`] up,
//!   about 55 degrees down, the pinned hands on the table's near half between the heads. A
//!   pin away from it: [`AWAY_REACH`] out and [`AWAY_RISE`] up, nearly straight down, since
//!   the winner's head follows the pin and sits just short of it and over it (run21: 7 short,
//!   8 up, 11 aside) [design: the lead, 2026-11-11 and 2026-11-12; armwrestle-a's 2f6432198
//!   frames, P1's win pinned north behind P1's head from the match view; run20, from over the
//!   far side the view turned upside down]. It holds there through the result.
//! - **Walls.** Pulled in towards the hands in front of anything between the two
//!   ([`WALL_MARGIN`]), as the third-person camera is (`nvf_play::control::place_camera`).
//! - **In and out.** It eases in from where the camera was over [`EASE_IN`]. As getting up
//!   starts it eases back and up ([`UP_REACH`] out, [`UP_RISE`] over the hands, the aim held
//!   on the table), well over the standing heads, and keeps the view until the player stands
//!   and the screen closes; the end view (`match_end.rs`) or the player's own camera takes it
//!   then, so the change never passes through a body or the table [the lead, 2026-11-12:
//!   run22, handed back as getting up started, the first-person view dropped to the table's
//!   height with the leaderboard still up; run21, eased back to the match spot at head
//!   height, the standing bodies filled it].
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
/// Round from straight back along the table's axis to the player's right or left, radians
/// (90 degrees, side on) [design: the lead, 2026-11-12, "from the side or over the
/// shoulder"; the heads lie along the axis either side of the clasp, so side on neither is
/// in front of it; at 70 degrees (run20) the player's own face was beside it].
pub const ANGLE: f32 = 90.0 * std::f32::consts::PI / 180.0;
/// Above the hands, game units: about 32 degrees down, the forearms and elbows on the table
/// under the clasp [design: run17, at 40 (23 degrees) the winner's face came beside the
/// hands near the pin].
pub const RISE: f32 = 60.0;
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
    let (sn, cs) = ANGLE.sin_cos();
    let out = [-toward[0] * cs + side[0] * sn, -toward[1] * cs + side[1] * sn];
    let at = [clasp[0] + out[0] * REACH, clasp[1] + out[1] * REACH, clasp[2] + RISE];
    let (heading, pitch) = aim(at, clasp);
    (at, heading, pitch)
}

/// The win's view, the pin towards the camera: out from the hands, level, game units
/// [design: run21].
pub const TOP_REACH: f32 = 62.0;
/// The same: above the hands, game units (about 55 degrees down) [design: run21].
pub const TOP_RISE: f32 = 88.0;
/// The win's view, the pin away from the camera: out from the hands, level, game units
/// [design: run21, the winner's head 7 short of the pinned hands and 8 over them].
pub const AWAY_REACH: f32 = 20.0;
/// The same: above the hands, game units (about 78 degrees down) [design].
pub const AWAY_RISE: f32 = 95.0;

/// Getting up: out from the hands, level, game units [design].
pub const UP_REACH: f32 = 90.0;
/// Getting up: above the hands, game units (about 59 degrees down; the camera some 80 over a
/// standing head) [design: run21, at 60 the standing bodies came into the camera].
pub const UP_RISE: f32 = 150.0;

/// Getting up: the camera's spot, heading and pitch, on the same side as [`spot`], looking
/// at `clasp` from [`UP_REACH`] out and [`UP_RISE`] up, for a player sat facing `toward`.
pub fn up_spot(clasp: [f32; 3], toward: [f32; 2], right: bool) -> ([f32; 3], f32, f32) {
    let side = if right { [toward[1], -toward[0]] } else { [-toward[1], toward[0]] };
    let at = [clasp[0] + side[0] * UP_REACH, clasp[1] + side[1] * UP_REACH, clasp[2] + UP_RISE];
    let (heading, pitch) = aim(at, clasp);
    (at, heading, pitch)
}

/// The win's view: the camera's spot, heading and pitch, on the same side as [`spot`]
/// (`right`), looking at the hands at `clasp` from [`TOP_REACH`] out and [`TOP_RISE`] up, or
/// from [`AWAY_REACH`] and [`AWAY_RISE`] when the pin went `away` from that side, for a player
/// sat facing `toward`.
pub fn top_spot(clasp: [f32; 3], toward: [f32; 2], right: bool, away: bool) -> ([f32; 3], f32, f32) {
    let side = if right { [toward[1], -toward[0]] } else { [-toward[1], toward[0]] };
    let (reach, rise) = if away { (AWAY_REACH, AWAY_RISE) } else { (TOP_REACH, TOP_RISE) };
    let at = [clasp[0] + side[0] * reach, clasp[1] + side[1] * reach, clasp[2] + rise];
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
    /// Up over the table for the win.
    top: bool,
    /// The pin went away from the camera's side.
    away: bool,
    /// Getting up: back and up, the aim held.
    rising: bool,
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
    // up, until the screen closes (the module notes).
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
    // The player's head (for the log: how near it is to the hands).
    let head = body.as_deref().and_then(|b| b.third_rig()).and_then(|(_, rig)| {
        let i = rig.skel.names.iter().position(|n| n.as_deref().is_some_and(|n| n.eq_ignore_ascii_case("Bip01 Head")))?;
        Some(nvf_play::space::to_game(globals.get(*rig.nodes.get(i)?).ok()?.translation()))
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
        // Getting up: the aim stays on the table, not on the rising hand.
        Some(p) if view.rising => p,
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
    // The bout won: up over the table, from where the camera is.
    let winner = a.bout.as_ref().and_then(|b| b.winner);
    if winner.is_some() && view.on == Some(a.stool) && !view.top {
        view.top = true;
        // The winner presses to his own left, the player's right when the player lost; away
        // when that is not the camera's side.
        view.away = (winner != Some(a.side)) != view.right;
        view.settled = true;
        view.from = view.last.or(Some(*t));
        view.start = now;
        view.told = false;
    }
    // Getting up: back and up, from where the camera is.
    if view.on == Some(a.stool) && a.leaving && !view.rising {
        view.rising = true;
        view.from = view.last.or(Some(*t));
        view.start = now;
        info!("arm wrestle view: getting up; back and up over the table");
    }
    let (want, heading, pitch) = if view.rising {
        up_spot(clasp, toward, view.right)
    } else if view.top { top_spot(clasp, toward, view.right, view.away) } else { spot(clasp, toward, view.right) };
    let (out, pulled) = reach(clasp, want);
    if view.top && now - view.start > EASE_IN && !std::mem::replace(&mut view.told, true) {
        info!("arm wrestle view: the win ({}), up over the table at {clasp:.0?} (the head at {head:.0?}): camera {out:.0?} pulled in {pulled:.0}, heading {:.0} pitch {:.0} deg", if view.away { "pinned away" } else { "pinned towards the camera" }, heading.to_degrees(), pitch.to_degrees());
    }
    if view.on == Some(a.stool) && now - view.start > 4.0 && !std::mem::replace(&mut view.told, true) {
        info!(
            "arm wrestle view: at the {} {clasp:.0?} (the head at {head:.0?}): camera {out:.0?} pulled in {pulled:.0}, heading {:.0} pitch {:.0} deg",
            if hand.is_some() { "right hand" } else { "table's middle" },
            heading.to_degrees(),
            pitch.to_degrees()
        );
    }
    let to = Transform { translation: to_bevy(out), rotation: look_rotation(heading, pitch), scale: Vec3::ONE };
    if view.on != Some(a.stool) {
        *view = View { on: Some(a.stool), right: view.right, told: false, settled: false, from: Some(*t), start: now, aim: view.aim, last: None, top: false, away: false, rising: false };
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
    fn the_camera_stands_side_on_and_looks_down_onto_the_hands() {
        // Sat at x 312 facing east (+x); the hands 38 ahead of the seat.
        let clasp = [350.0, 337.0, 3527.0];
        let (at, heading, pitch) = spot(clasp, [1.0, 0.0], true);
        // Level with the hands along the table, to the player's right (south, -y).
        assert!((at[0] - 350.0).abs() < 1e-3 && (at[1] - (337.0 - REACH)).abs() < 1e-3, "{at:?}");
        assert!((at[2] - clasp[2] - RISE).abs() < 1e-3);
        // Looking north, 25 to 40 degrees down.
        assert!(heading.to_degrees().abs() < 1.0, "{}", heading.to_degrees());
        assert!(pitch.to_degrees() < -25.0 && pitch.to_degrees() > -40.0, "{}", pitch.to_degrees());
        // The left: north, looking south.
        let (left, heading, _) = spot(clasp, [1.0, 0.0], false);
        assert!((left[1] - (337.0 + REACH)).abs() < 1e-3, "{left:?}");
        assert!((heading.to_degrees().abs() - 180.0).abs() < 1.0, "{}", heading.to_degrees());
    }

    #[test]
    fn the_line_to_the_hands_passes_clear_of_both_heads() {
        // The heads bent down to the hands' height, 8 to 13 back from them along the table
        // and up to 10 to either side (run16 to run20).
        let clasp = [350.0, 337.0, 3527.0];
        for right in [true, false] {
            let (at, ..) = spot(clasp, [1.0, 0.0], right);
            for (x, y) in [(-8.0, 10.0), (-8.0, -10.0), (-13.0, 0.0), (8.0, 10.0), (8.0, -10.0), (13.0, 0.0)] {
                let d = off_line([350.0 + x, 337.0 + y], at, clasp);
                assert!(d >= 8.0, "{right} {x} {y}: {d}");
            }
        }
        // The win's view: the same side, the same heading, steeper.
        for right in [true, false] {
            for away in [false, true] {
                let (at, heading, pitch) = top_spot(clasp, [1.0, 0.0], right, away);
                let (_, match_heading, _) = spot(clasp, [1.0, 0.0], right);
                assert!((heading - match_heading).abs() < 1e-3);
                let (lo, hi) = if away { (-82.0, -74.0) } else { (-60.0, -50.0) };
                assert!(pitch.to_degrees() > lo && pitch.to_degrees() < hi, "{away} {}", pitch.to_degrees());
                assert!((at[0] - 350.0).abs() < 1e-3);
            }
            // Pinned away: the winner's head 7 short of the hands towards the camera and 8 over
            // them (run21); the line to the hands passes well over it there.
            let (at, ..) = top_spot(clasp, [1.0, 0.0], right, true);
            let toward_cam = (at[1] - clasp[1]).signum();
            let k = 7.0 / (at[1] - clasp[1]).abs();
            let over = clasp[2] + (at[2] - clasp[2]) * k;
            assert!(over - (clasp[2] + 8.0) > 15.0, "{right} {toward_cam} {over}");
        }
        // Getting up: well over a standing head (about 120 over the floor, the hands 60 over
        // it), same heading.
        let (at, heading, pitch) = up_spot(clasp, [1.0, 0.0], true);
        assert!(at[2] - (clasp[2] - 60.0 + 120.0) > 60.0, "{at:?}");
        assert!(heading.to_degrees().abs() < 1.0 && pitch.to_degrees() < -50.0, "{} {}", heading.to_degrees(), pitch.to_degrees());
    }
}
