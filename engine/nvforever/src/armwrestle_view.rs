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
//! - **Bodies.** Every drawn body near the table (spectators, NPCs, another player's
//!   puppet, the two wrestlers) is a capsule from its feet to its head ([`BODY_RADIUS`]). A
//!   spot whose line to the hands passes through a spectator, or that stands inside any
//!   body, is not used (the wrestlers are what it shows: bent over the hands or rising,
//!   they are not in its way; run b6, counted as in the way, getting up the view turned
//!   75 degrees or across to the wall's side): the camera keeps the spot it has while that stays
//!   clear, and otherwise takes the first clear one of: its own spot turned round the hands
//!   ([`TURNS`], up to 25 degrees for the bout and the win) on its side, then on the other
//!   side, then the same pulled in towards the hands ([`SCALES`]), once that has been the
//!   one to take for [`SETTLE`] s (at once with a body in the camera itself); it goes back
//!   the same way when the way clears [the lead, ship14 laptop run: the bout seen through the back of a
//!   spectator's head stood 18 from the camera; P2 getting up passed under the camera].
//!   Looked at again every [`RECHECK`] s; a change eases across.
//! - **In and out.** It eases in from where the camera was over [`EASE_IN`]. As getting up
//!   starts it rises ([`UP_REACH`] out, [`UP_RISE`] over the hands, the aim held on the
//!   table), up first and then across, to look down over the table between the two
//!   standing bodies, and keeps the view until the player stands and the screen closes; the
//!   end view (`match_end.rs`) or the player's own camera takes it then, so the change
//!   never passes through a body or the table [the lead, 2026-11-12: run22, handed back as
//!   getting up started, the first-person view dropped to the table's height with the
//!   leaderboard still up; run21, eased back to the match spot at head height, the standing
//!   bodies filled it; ship14, from 90 out the line to the table crossed the bodies stepping
//!   off their chairs towards the camera].
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

/// Getting up: out from the hands, level, game units: over the table's edge, the bodies
/// stepping off at either end of it [design: ship14, from 90 out they crossed the view].
pub const UP_REACH: f32 = 30.0;
/// Getting up: above the hands, game units (about 80 degrees down; the camera some 75 over
/// a standing head) [design: run21, at 60 the standing bodies came into the camera].
pub const UP_RISE: f32 = 165.0;

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

/// A body's capsule round the line from its feet to its head, game units [design: a
/// shoulder's half width].
pub const BODY_RADIUS: f32 = 18.0;
/// How far a spot's line to the hands keeps outside a body's capsule to be taken [design].
pub const BODY_CLEAR: f32 = 8.0;
/// How far the camera itself keeps outside a body's capsule [design].
pub const BODY_NEAR: f32 = 14.0;
/// The line to the hands is tested up to this short of them: the wrestlers' own hands and
/// forearms are there [design].
pub const HANDS_SPARED: f32 = 15.0;
/// The turns round the hands tried, degrees, in order [design].
/// The bout and the win try only the first three: turned further the view comes over a
/// wrestler's shoulder and the head hides the hands (run b7, 50 degrees: the Settler's hat
/// over the clasp; run16, from behind). Towards the player's own seat first: towards the
/// opponent's, the opponent's back filled the view (run b8).
pub const TURNS: [f32; 7] = [0.0, 25.0, -25.0, 50.0, -50.0, 75.0, -75.0];
/// The reaches tried, as parts of the spot's own, in order [design].
pub const SCALES: [f32; 3] = [1.0, 0.75, 0.55];
/// Seconds between looks at the bodies [design].
pub const RECHECK: f32 = 0.25;
/// Seconds a better spot must stay the one to take before the camera moves to it, unless a
/// body is in the camera itself [design: ship14 watch run b7, the Settler walking to his
/// chair crossed the line and swung the view 75 degrees].
pub const SETTLE: f32 = 0.75;

/// What the camera is framing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Framing {
    /// The bout ([`spot`]).
    #[default]
    Bout,
    /// The win ([`top_spot`]).
    Win,
    /// Getting up ([`up_spot`]).
    Up,
}

/// A spot of the framing: its side, turned round the hands (radians, positive towards the
/// player's own seat: over its shoulder, the opponent's face in view) and its reach scaled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick {
    pub right: bool,
    pub turn: f32,
    pub scale: f32,
}

/// The camera's spot, heading and pitch for `framing` and `pick`, looking at `clasp` for a
/// player sat facing `toward`; `away(right)` says whether the win's pin went away from that
/// side.
pub fn placed(framing: Framing, pick: Pick, clasp: [f32; 3], toward: [f32; 2], away: impl Fn(bool) -> bool) -> ([f32; 3], f32, f32) {
    let (at, ..) = match framing {
        Framing::Bout => spot(clasp, toward, pick.right),
        Framing::Win => top_spot(clasp, toward, pick.right, away(pick.right)),
        Framing::Up => up_spot(clasp, toward, pick.right),
    };
    let d = [at[0] - clasp[0], at[1] - clasp[1], at[2] - clasp[2]];
    // Towards the own seat is clockwise on the player's right, anticlockwise on its left.
    let turn = if pick.right { -pick.turn } else { pick.turn };
    let (s, c) = turn.sin_cos();
    let d = [(d[0] * c - d[1] * s) * pick.scale, (d[0] * s + d[1] * c) * pick.scale, d[2] * pick.scale];
    let at = [clasp[0] + d[0], clasp[1] + d[1], clasp[2] + d[2]];
    let (heading, pitch) = aim(at, clasp);
    (at, heading, pitch)
}

/// The shortest distance between the segments `a0`-`a1` and `b0`-`b1`.
pub fn segments_apart(a0: [f32; 3], a1: [f32; 3], b0: [f32; 3], b1: [f32; 3]) -> f32 {
    let (p, q) = (Vec3::from_array(a0), Vec3::from_array(b0));
    let (d1, d2) = (Vec3::from_array(a1) - p, Vec3::from_array(b1) - q);
    let r = p - q;
    let (a, e, f) = (d1.length_squared(), d2.length_squared(), d2.dot(r));
    let (s, t) = if a <= 1e-6 && e <= 1e-6 {
        (0.0, 0.0)
    } else if a <= 1e-6 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= 1e-6 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let den = a * e - b * b;
            let mut s = if den > 1e-6 { ((b * f - c * e) / den).clamp(0.0, 1.0) } else { 0.0 };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    ((p + d1 * s) - (q + d2 * t)).length()
}

/// A body near the match: its feet, its head, and whether it is one of the wrestlers.
pub type Body = ([f32; 3], [f32; 3], bool);

/// How far the camera at `cam` looking at `clasp` keeps outside the bodies: the line's room
/// ([`HANDS_SPARED`] short of the hands) from the bodies other than the wrestlers, and the
/// camera's own from every body, beyond [`BODY_RADIUS`]; large with no body near.
pub fn room(cam: [f32; 3], clasp: [f32; 3], bodies: &[Body]) -> (f32, f32) {
    let d = [clasp[0] - cam[0], clasp[1] - cam[1], clasp[2] - cam[2]];
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-3);
    let k = ((l - HANDS_SPARED) / l).max(0.0);
    let end = [cam[0] + d[0] * k, cam[1] + d[1] * k, cam[2] + d[2] * k];
    let mut out = (f32::MAX, f32::MAX);
    for &(feet, head, wrestler) in bodies {
        if !wrestler {
            out.0 = out.0.min(segments_apart(cam, end, feet, head) - BODY_RADIUS);
        }
        out.1 = out.1.min(segments_apart(cam, cam, feet, head) - BODY_RADIUS);
    }
    out
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
    /// Getting up: back and up, the aim held.
    rising: bool,
    /// The spot taken clear of the bodies, for which framing, and when last looked at.
    pick: Option<(Framing, Pick)>,
    checked: f32,
    /// A better spot and since when it has been the one to take ([`SETTLE`]).
    want: Option<(Pick, f32)>,
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
    rigs: Query<(&crate::limbs::ActorRig, &GlobalTransform, Option<&InheritedVisibility>)>,
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
        if view.right != right {
            view.pick = None;
        }
        view.right = right;
    }
    // The bout won: up over the table, from where the camera is.
    let winner = a.bout.as_ref().and_then(|b| b.winner);
    if winner.is_some() && view.on == Some(a.stool) && !view.top {
        view.top = true;
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
    let framing = if view.rising {
        Framing::Up
    } else if view.top {
        Framing::Win
    } else {
        Framing::Bout
    };
    // The bodies near the table: spectators, NPCs, another player's puppet; the opponent
    // (the body nearest the other stool) and this player's own as wrestlers.
    let near = |p: [f32; 3]| ((p[0] - clasp[0]).powi(2) + (p[1] - clasp[1]).powi(2)).sqrt() < 400.0;
    let head_of = |rig: &crate::limbs::ActorRig| {
        let i = rig.skel.names.iter().position(|n| n.as_deref().is_some_and(|n| n.eq_ignore_ascii_case("Bip01 Head")))?;
        Some(nvf_play::space::to_game(globals.get(*rig.nodes.get(i)?).ok()?.translation()))
    };
    let mut bodies: Vec<Body> = rigs
        .iter()
        .filter(|(rig, _, vis)| rig.reference != FormId::PLAYER_REF.0 && vis.is_none_or(|v| v.get()))
        .map(|(rig, g, _)| {
            let feet = nvf_play::space::to_game(g.translation());
            (feet, head_of(rig).unwrap_or([feet[0], feet[1], feet[2] + 120.0]), false)
        })
        .filter(|&(feet, ..)| near(feet))
        .collect();
    let foe = bodies
        .iter()
        .enumerate()
        .map(|(i, (f, ..))| (i, ((f[0] - o[0]).powi(2) + (f[1] - o[1]).powi(2)).sqrt()))
        .filter(|&(_, d)| d < 70.0)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((i, _)) = foe {
        bodies[i].2 = true;
    }
    if let (Some((root, _)), Some(head)) = (body.as_deref().and_then(|b| b.third_rig()), head) {
        if let Ok(g) = globals.get(root) {
            bodies.push((nvf_play::space::to_game(g.translation()), head, true));
        }
    }
    // The winner presses to his own left, the player's right when the player lost; away
    // when that is not the camera's side.
    let won_by = winner;
    let side = a.side;
    let away = |right: bool| (won_by != Some(side)) != right;
    let base = Pick { right: view.right, turn: 0.0, scale: 1.0 };
    if view.pick.is_none_or(|(f, _)| f != framing) || now - view.checked >= RECHECK {
        view.checked = now;
        let current = view.pick.filter(|(f, _)| *f == framing).map(|(_, p)| p);
        let base_pulled = reach(clasp, placed(framing, base, clasp, toward, away).0).1;
        let mut look = |p: Pick| {
            let (out, pulled) = reach(clasp, placed(framing, p, clasp, toward, away).0);
            let (line, cam) = room(out, clasp, &bodies);
            (line, cam, pulled <= base_pulled + 10.0)
        };
        // The first spot clear in the order, else the one with the most room.
        let turns: &[f32] = if framing == Framing::Up { &TURNS } else { &TURNS[..3] };
        let mut first = None;
        let mut best: Option<(Pick, f32)> = None;
        'search: for scale in SCALES {
            for &turn in turns {
                for right in [view.right, !view.right] {
                    let p = Pick { right, turn: turn.to_radians(), scale };
                    let (line, cam, walls) = look(p);
                    if walls && line >= BODY_CLEAR && cam >= BODY_NEAR {
                        first = Some(p);
                        break 'search;
                    }
                    let score = line.min(cam) - if walls { 0.0 } else { 100.0 };
                    if best.is_none_or(|(_, s)| score > s) {
                        best = Some((p, score));
                    }
                }
            }
        }
        let target = first.or(best.map(|(p, _)| p)).unwrap_or(base);
        // A body in the camera itself moves it at once; otherwise the change waits until
        // the same spot has been the one to take for [`SETTLE`] s, so a body walking past
        // the line does not swing the view, and it comes back once the way is clear.
        let had = current.map(&mut look);
        let blocked = had.is_some_and(|(line, cam, walls)| !walls || line < 0.0 || cam < 0.0);
        let inside = had.is_some_and(|(_, cam, _)| cam < 0.0);
        let chosen = match current {
            None => target,
            // Kept while clear, unless the plain spot is clear again.
            Some(c) if c == target || (!blocked && target != base) => {
                view.want = None;
                c
            }
            Some(_) if inside => target,
            Some(c) => match view.want {
                Some((w, since)) if w == target && now - since >= SETTLE => target,
                Some((w, _)) if w == target => c,
                _ => {
                    view.want = Some((target, now));
                    c
                }
            },
        };
        if current != Some(chosen) {
            if current.is_some() {
                view.from = view.last.or(Some(*t));
                view.start = now;
            }
            if chosen != base || current.is_some() {
                info!(
                    "arm wrestle view: clear of {} bodies: {framing:?} round {:.0} deg to the {}, at {:.2} of the reach (the spot it had: {})",
                    bodies.len(),
                    chosen.turn.to_degrees(),
                    if chosen.right { "right" } else { "left" },
                    chosen.scale,
                    had.map_or("none".to_string(), |(line, cam, walls)| format!("line room {line:.0}, camera room {cam:.0}, walls {}", if walls { "clear" } else { "in" }))
                );
            }
        }
        view.pick = Some((framing, chosen));
    }
    let pick = view.pick.map_or(base, |(_, p)| p);
    let (want, heading, pitch) = placed(framing, pick, clasp, toward, away);
    let (out, pulled) = reach(clasp, want);
    if view.top && now - view.start > EASE_IN && !std::mem::replace(&mut view.told, true) {
        info!("arm wrestle view: the win ({}), up over the table at {clasp:.0?} (the head at {head:.0?}): camera {out:.0?} pulled in {pulled:.0}, heading {:.0} pitch {:.0} deg", if away(pick.right) { "pinned away" } else { "pinned towards the camera" }, heading.to_degrees(), pitch.to_degrees());
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
        *view = View { on: Some(a.stool), right: view.right, told: false, settled: false, from: Some(*t), start: now, aim: view.aim, last: None, top: false, rising: false, pick: None, checked: 0.0, want: None };
        control.scene_cam = true;
        info!("arm wrestle view: at {:08X}, looking at {clasp:.0?} from {out:.0?} pulled in {pulled:.0}, heading {:.0} pitch {:.0} deg", a.stool.0, heading.to_degrees(), pitch.to_degrees());
    }
    if !control.scene_cam {
        control.scene_cam = true;
    }
    let f = ((now - view.start) / EASE_IN).clamp(0.0, 1.0);
    let f = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    // Getting up: up at twice the pace, so it is over the rising heads before it moves
    // across (Bevy's y is up).
    let raw = ((now - view.start) / EASE_IN).clamp(0.0, 1.0);
    let v = (raw * 2.0).min(1.0);
    let up = if view.rising { v * v * v * (v * (v * 6.0 - 15.0) + 10.0) } else { f };
    *t = match view.from {
        Some(from) if raw < 1.0 => {
            let mut p = from.translation.lerp(to.translation, f);
            p.y = from.translation.y + (to.translation.y - from.translation.y) * up;
            Transform { translation: p, rotation: from.rotation.slerp(to.rotation, f), scale: Vec3::ONE }
        }
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

    #[test]
    fn segments_apart_measures_the_gap() {
        assert!((segments_apart([0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [5.0, 3.0, -5.0], [5.0, 3.0, 5.0]) - 3.0).abs() < 1e-4);
        assert!((segments_apart([0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [14.0, 0.0, 0.0], [20.0, 0.0, 0.0]) - 4.0).abs() < 1e-4);
        assert!((segments_apart([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [3.0, 4.0, 0.0], [3.0, 4.0, 0.0]) - 5.0).abs() < 1e-4);
    }

    /// The first spot clear of `bodies` in the camera's order (no walls).
    fn first_clear(framing: Framing, clasp: [f32; 3], toward: [f32; 2], bodies: &[Body]) -> Option<Pick> {
        let turns: &[f32] = if framing == Framing::Up { &TURNS } else { &TURNS[..3] };
        for scale in SCALES {
            for &turn in turns {
                for right in [true, false] {
                    let p = Pick { right, turn: turn.to_radians(), scale };
                    let (at, ..) = placed(framing, p, clasp, toward, |_| false);
                    let (line, cam) = room(at, clasp, bodies);
                    if line >= BODY_CLEAR && cam >= BODY_NEAR {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    #[test]
    fn a_spectator_in_the_way_turns_the_camera() {
        // ship14 laptop run, watch-close: the hands at [337, 329, 3511], the player facing
        // east; a spectator standing at (350, 250), its head 120 over the floor (3482).
        let clasp = [337.0, 329.0, 3511.0];
        let watcher = ([350.0, 250.0, 3482.0], [350.0, 250.0, 3602.0], false);
        let (at, ..) = placed(Framing::Bout, Pick { right: true, turn: 0.0, scale: 1.0 }, clasp, [1.0, 0.0], |_| false);
        assert!(room(at, clasp, &[watcher]).0 < 0.0, "the plain spot looks through it");
        // With no wall the other side is clear first.
        assert_eq!(first_clear(Framing::Bout, clasp, [1.0, 0.0], &[watcher]), Some(Pick { right: false, turn: 0.0, scale: 1.0 }));
        // With the wall there (the workbench's north side), the right side turned 25 degrees
        // back towards the player's seat, away from the spectator: clear, the same height,
        // looking down at the hands (the game took it, run b8).
        let p = Pick { right: true, turn: 25f32.to_radians(), scale: 1.0 };
        let (at, heading, pitch) = placed(Framing::Bout, p, clasp, [1.0, 0.0], |_| false);
        let (line, cam) = room(at, clasp, &[watcher]);
        assert!(line >= BODY_CLEAR && cam >= BODY_NEAR, "{line} {cam}");
        assert!(at[0] < clasp[0] - 30.0, "towards the player's seat (west): {at:?}");
        assert!(room(placed(Framing::Bout, Pick { turn: -25f32.to_radians(), ..p }, clasp, [1.0, 0.0], |_| false).0, clasp, &[watcher]).1 < BODY_NEAR);
        assert!((at[2] - clasp[2] - RISE).abs() < 1e-3 && pitch.to_degrees() < -25.0, "{at:?} {heading} {pitch}");
        // Nobody about: the plain spot.
        assert_eq!(first_clear(Framing::Bout, clasp, [1.0, 0.0], &[]), Some(Pick { right: true, turn: 0.0, scale: 1.0 }));
    }

    #[test]
    fn getting_up_looks_down_between_the_standing_bodies() {
        // Both up from their chairs, 38 either side of the hands along the table, stepped
        // 40 out to the camera's side (ship14: the exits go to the chairs' sides).
        let clasp = [350.0, 337.0, 3517.0];
        let bodies = [
            ([312.0, 297.0, 3482.0], [312.0, 297.0, 3607.0], true),
            ([388.0, 297.0, 3482.0], [388.0, 297.0, 3607.0], true),
        ];
        let p = Pick { right: true, turn: 0.0, scale: 1.0 };
        let (at, ..) = placed(Framing::Up, p, clasp, [1.0, 0.0], |_| false);
        let (line, cam) = room(at, clasp, &bodies);
        assert!(line >= BODY_CLEAR && cam >= BODY_NEAR, "{line} {cam}");
        // The old spot, 90 out and 150 up, looked through them.
        let old = [350.0, 337.0 - 90.0, 3517.0 + 150.0];
        assert!(room(old, clasp, &[([350.0, 292.0, 3482.0], [350.0, 292.0, 3607.0], false)]).0 < 0.0);
        // Still bent over the hands as getting up starts (the winner's head 7 short of them
        // and 8 up, run21): the wrestlers do not turn the view, a spectator there would.
        let bent = [([350.0, 330.0, 3482.0], [350.0, 330.0, 3525.0], true)];
        assert_eq!(first_clear(Framing::Up, clasp, [1.0, 0.0], &bent), Some(p));
        let (line, cam) = room(at, clasp, &bent);
        assert!(line > 1e6 && cam >= BODY_NEAR, "{line} {cam}");
    }
}
