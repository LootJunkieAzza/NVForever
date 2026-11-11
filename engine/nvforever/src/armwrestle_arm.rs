//! The wrestlers' right arms at the arm-wrestling table (queue item 199; the bout is
//! `nvf_play::armwrestle`): no clip the game or a mod has is arm wrestling, so the right
//! arm of whoever sits at the table is posed here, over the seated pose its body plays
//! [design: owner 2026-11-11, "arm wrestling animated"; the original game has no arm
//! wrestling].
//!
//! - **The clasp.** From the countdown on, each wrestler's elbow goes down onto the table
//!   on the line between the stools, a little short of the middle ([`ELBOW_IN`]), and the
//!   hands meet over the middle, the forearms upright ([`clasp`]).
//! - **The pull.** The clasped hands lean towards the side losing, in step with the
//!   meter: upright at even, up to [`MAX_LEAN`] just before a win ([`lean_angle`]). The
//!   meter is followed smoothly ([`FOLLOW`]) so single taps do not jerk it; near even
//!   the hands tremble a little ([`tremble`]).
//! - **The win.** The loser's hand goes down onto the table in [`SLAM`] and stays there
//!   through the celebration ([`PINNED`]); the pose is let go ([`EASE`]) when the bout's
//!   screen closes, the match view goes, or a wrestler gets up.
//! - **Everyone sees the same arms**: the pose is driven by the bout every game already
//!   has: this player's own (`ArmScreen`, in co-op filled from the deciding game), or the
//!   match a watcher sees (`ArmWatch`, the host's table view). Each side's body is
//!   whoever sits on that side's stool here: this player's own body, another player's
//!   body, or the NPC opponent.
//!
//! The bones are written after the animation has posed them and before transforms are
//! propagated, as the darts arm (`darts_arm.rs`), whose two-bone reach this uses.

use crate::darts_arm::{rot, solve, world_of};
use crate::furniture::OnFurniture;
use crate::limbs::ActorRig;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::armwrestle::{ArmScreen, ArmTable, Page};
use nvf_play::armwrestle_coop::{ArmPhase, ArmWatch};
use nvf_play::furniture::PlayerSeat;
use nvf_play::sim::SimState;
use nvf_play::space::to_bevy;

/// The arm's bones [observed: the human skeleton's node names].
const UPPER: &str = "Bip01 R UpperArm";
const FORE: &str = "Bip01 R Forearm";
const HAND: &str = "Bip01 R Hand";
/// The lean of the clasped forearms just before a win, radians (about 70 degrees)
/// [design: owner 2026-11-11].
pub const MAX_LEAN: f32 = 70.0 * std::f32::consts::PI / 180.0;
/// The loser's hand on the table, radians from upright [design: the forearm flat on it].
pub const PINNED: f32 = 88.0 * std::f32::consts::PI / 180.0;
/// Seconds for the loser's hand to go down [design: owner 2026-11-11, "about 0.25 s"].
pub const SLAM: f32 = 0.25;
/// How fast the shown lean follows the meter (per second) [design: smooth over taps].
pub const FOLLOW: f32 = 5.0;
/// Seconds for the pose to ease in or out [design].
const EASE: f32 = 0.3;
/// Seconds the loser's hand stays down before the arms let go [design: the celebration].
const HOLD: f32 = 3.0;
/// How near a chair a body sits to be taken for its wrestler, game units [design].
const ON_SEAT: f32 = 40.0;
/// The elbow's place on the table from the middle, in forearm lengths [design].
const ELBOW_IN: f32 = 0.45;
/// The tremble: within this much of even, at most this lean (radians) [design].
const TREMBLE_NEAR: f32 = 0.3;
const TREMBLE: f32 = 0.035;

/// The clasped hands' lean towards side 1's stool (radians) for the shown meter `m` (+1
/// side 0 has won, as `Bout::meter`): side 0 winning pushes them towards side 1.
pub fn lean_angle(m: f32) -> f32 {
    m.clamp(-1.0, 1.0) * MAX_LEAN
}

/// A small shake near even, `t` seconds on (radians).
pub fn tremble(m: f32, t: f32) -> f32 {
    let near = (1.0 - m.abs() / TREMBLE_NEAR).max(0.0);
    near * TREMBLE * (0.6 * (t * 23.0).sin() + 0.4 * (t * 37.0).sin())
}

/// The lean `s` seconds after side `winner` won, from the lean `from` it had then: the
/// loser's side goes down to [`PINNED`] in [`SLAM`] and stays.
pub fn slam(from: f32, winner: u8, s: f32) -> f32 {
    let to = if winner == 0 { PINNED } else { -PINNED };
    let k = (s / SLAM).clamp(0.0, 1.0);
    from + (to - from) * k * k * (3.0 - 2.0 * k)
}

/// Where a wrestler's elbow rests and its hand goes, for the table's middle `mid` (on its
/// top), the axis `u` from side 0's stool to side 1's (level, unit), the forearm's length
/// `b`, the side `k` and the lean (towards side 1, radians).
pub fn clasp(mid: Vec3, u: Vec3, b: f32, k: usize, lean: f32) -> (Vec3, Vec3) {
    let d = ELBOW_IN * b;
    let r = (b * b - d * d).max(0.0).sqrt();
    let side = if k == 0 { -1.0 } else { 1.0 };
    let elbow = mid + u * (side * d);
    let hand = mid + (Vec3::Y * lean.cos() + u * lean.sin()) * r;
    (elbow, hand)
}

/// The match whose arms are shown here: the table, the meter, the winner, each side's
/// stool (this game's references).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Live {
    table: u32,
    meter: f32,
    winner: Option<u8>,
    stools: [u32; 2],
}

#[derive(Default)]
pub struct Shown {
    table: u32,
    stools: [u32; 2],
    meter: f32,
    weight: f32,
    won: Option<(u8, f32, f32)>,
    clock: f32,
    noted: bool,
}

/// The match this game shows: this player's own bout (after the card), else the one it
/// watches.
fn live(screen: &ArmScreen, watch: &ArmWatch, table: &ArmTable) -> Option<Live> {
    if let Some(a) = screen.0.as_ref().filter(|a| a.page != Page::Offer && !a.leaving && a.left_at.is_none()) {
        let b = a.bout.as_ref()?;
        let mut stools = [0; 2];
        stools[a.side as usize & 1] = a.stool.0;
        stools[1 - (a.side as usize & 1)] = a.other.0;
        return Some(Live { table: table.table.map_or(a.stool.0, |t| t.0), meter: b.meter, winner: b.winner, stools });
    }
    let (_, v) = watch.0.as_ref()?;
    let (Some(s0), Some(s1)) = (table.stools[0], table.stools[1]) else { return None };
    let winner = match v.phase {
        ArmPhase::Won(s) => Some(s & 1),
        _ => None,
    };
    Some(Live { table: table.table.map_or(0, |t| t.0), meter: v.meter, winner, stools: [s0.0, s1.0] })
}

/// Each frame: the wrestlers' arms posed from the match shown.
#[allow(clippy::too_many_arguments)]
pub fn arms(
    time: Res<Time>,
    (screen, watch, table): (Option<Res<ArmScreen>>, Option<Res<ArmWatch>>, Option<Res<ArmTable>>),
    seat: Option<Res<PlayerSeat>>,
    sim: Option<Res<SimState>>,
    body: Option<Res<crate::player_body::PlayerBody>>,
    rigs: Query<(Entity, &ActorRig, Option<&OnFurniture>, &GlobalTransform)>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
    mut shown: Local<Shown>,
) {
    let dt = time.delta_secs();
    shown.clock += dt;
    let (Some(screen), Some(watch), Some(table), Some(sim)) = (screen, watch, table, sim) else { return };
    let mut now_live = live(&screen, &watch, &table);
    // Getting up, or a while after the win: the arms let go.
    if screen.0.as_ref().is_some_and(|a| a.leaving) || (now_live.is_some_and(|l| l.winner.is_some()) && shown.won.is_some_and(|(_, at, _)| shown.clock - at > HOLD)) {
        now_live = None;
    }
    match now_live {
        Some(l) => {
            if l.table != shown.table || l.stools != shown.stools {
                info!("arm wrestle arm: posing the arms at {:08X}", l.table);
                *shown = Shown { table: l.table, stools: l.stools, meter: l.meter, clock: shown.clock, ..default() };
            }
            shown.weight = (shown.weight + dt / EASE).min(1.0);
            shown.meter += (l.meter - shown.meter) * (dt * FOLLOW).min(1.0);
            match (l.winner, shown.won) {
                (Some(w), None) => {
                    let from = lean_angle(shown.meter);
                    shown.won = Some((w, shown.clock, from));
                    info!("arm wrestle arm: side {w} wins; side {}'s hand goes down", 1 - w);
                }
                (None, Some(_)) => shown.won = None,
                _ => {}
            }
        }
        None => {
            if shown.weight <= 0.0 {
                return;
            }
            shown.weight = (shown.weight - dt / EASE).max(0.0);
            if shown.weight <= 0.0 {
                info!("arm wrestle arm: the arms let go");
            }
        }
    }
    let lean = match shown.won {
        Some((w, at, from)) => slam(from, w, shown.clock - at),
        None => lean_angle(shown.meter) + tremble(shown.meter, shown.clock),
    };
    let w = {
        let x = shown.weight.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    // The table's middle and its axis, from this game's references.
    let at = |r: u32| sim.world.peek(FormId(r)).map(|s| to_bevy(s.pos));
    let (Some(mid), Some(p0), Some(p1)) = (at(shown.table), at(shown.stools[0]), at(shown.stools[1])) else {
        if !std::mem::replace(&mut shown.noted, true) {
            warn!("arm wrestle arm: the table {:08X} or its seats are not in the world", shown.table);
        }
        return;
    };
    let mut u = p1 - p0;
    u.y = 0.0;
    let Some(u) = u.try_normalize() else {
        if !std::mem::replace(&mut shown.noted, true) {
            warn!("arm wrestle arm: the seats stand at one spot");
        }
        return;
    };
    // Each side's body: whoever sits on that side's stool here.
    let mine = seat.as_deref().and_then(|s| s.on.filter(|_| s.hold.is_some()));
    let bodies: [Option<(Entity, ActorRig)>; 2] = std::array::from_fn(|k| {
        let stool = shown.stools[k];
        if mine == Some(stool) {
            return body.as_deref().and_then(|b| b.third_rig());
        }
        if let Some((e, r, _, _)) = rigs.iter().find(|(_, _, on, _)| on.is_some_and(|o| o.furniture() == stool && o.seated())) {
            return Some((e, r.clone()));
        }
        // Seated some other way (an NPC its AI sat down): the body nearest the chair.
        let local = body.as_deref().and_then(|b| b.third_rig()).map(|r| r.0);
        let seat_at = at(stool)?;
        rigs.iter()
            .filter(|(e, r, _, _)| Some(*e) != local && r.reference != FormId::PLAYER_REF.0)
            .map(|(e, r, _, g)| (e, r, Vec2::new(g.translation().x - seat_at.x, g.translation().z - seat_at.z).length()))
            .filter(|(_, _, d)| *d < ON_SEAT)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(e, r, _)| (e, r.clone()))
    });
    for (k, b) in bodies.iter().enumerate() {
        if b.is_none() && !std::mem::replace(&mut shown.noted, true) {
            warn!("arm wrestle arm: no body on side {k}'s seat {:08X}", shown.stools[k]);
        }
    }
    // The middle of the table's top: the table's x/y, its top's height
    // (`fnv_game::armwrestle_table::TABLE_TOP_Z`, game z is Bevy y).
    let mut arms = Vec::new();
    for (k, b) in bodies.iter().enumerate() {
        let Some((root, rig)) = b else { continue };
        // Leaning in over the table first: the arm hangs from the leaned chest.
        let toward = if k == 0 { u } else { -u };
        if let Err(why) = lean_in(*root, rig, Vec3::new(mid.x, 0.0, mid.z), toward, w, &parents, &mut transforms, &globals) {
            if !std::mem::replace(&mut shown.noted, true) {
                warn!("arm wrestle arm: side {k} does not lean in: {why}");
            }
        }
        match bones(*root, rig, &parents, &transforms, &globals) {
            Ok(a) => arms.push((k, a)),
            Err(why) => {
                if !std::mem::replace(&mut shown.noted, true) {
                    warn!("arm wrestle arm: side {k}'s arm is not posed: {why}");
                }
            }
        }
    }
    if arms.is_empty() {
        return;
    }
    // Over the table's top, half way between the two shoulders (where both arms reach).
    let (mx, mz) = if arms.len() == 2 { ((arms[0].1.s.x + arms[1].1.s.x) / 2.0, (arms[0].1.s.z + arms[1].1.s.z) / 2.0) } else { (mid.x, mid.z) };
    let mid = Vec3::new(mx, fnv_game::armwrestle_table::TABLE_TOP_Z, mz);
    for (k, a) in arms {
        let b = a.e0.distance(a.h0);
        let (elbow, hand) = clasp(mid, u, b, k, lean);
        let (e, h) = solve(a.s, a.e0, a.h0, hand, elbow - a.s);
        if (shown.clock * 2.0).fract() < dt * 2.0 {
            info!("arm wrestle arm: side {k} weight {w:.2} lean {:.2} shoulder {:.0?} elbow {:.0?} wrist {:.0?} -> elbow {:.0?} wrist {:.0?} target {:.0?}", lean, a.s, a.e0, a.h0, e, h, hand);
        }
        a.apply(e, h, w, &mut transforms);
    }
}

/// The bone the upper body bends at, and the one above it if the skeleton lacks it
/// [observed: the human skeleton's node names].
const SPINE: [&str; 2] = ["Bip01 Spine", "Bip01 Spine1"];
/// The upper body's lean in at most, radians [design: about 50 degrees].
const MAX_BEND: f32 = 0.87;
/// How near the clasp (level) the shoulder is to come, in arm lengths [design: the
/// elbow then goes down onto the table with the forearm up].
const SHOULDER_IN: f32 = 0.45;

/// Bend the upper body forward (towards `toward`, level) so the right shoulder comes in
/// towards the clasp at `clasp` (level: its y is ignored), by `w`.
#[allow(clippy::too_many_arguments)]
fn lean_in(root: Entity, rig: &ActorRig, clasp: Vec3, toward: Vec3, w: f32, parents: &Query<&ChildOf>, transforms: &mut Query<&mut Transform>, globals: &Query<&GlobalTransform>) -> Result<(), &'static str> {
    let node = |name: &str| rig.skel.names.iter().position(|n| n.as_deref() == Some(name)).and_then(|i| rig.nodes.get(i).copied());
    let spine = SPINE.iter().find_map(|n| node(n)).ok_or("no spine")?;
    let (Some(upper), Some(fore), Some(hand)) = (node(UPPER), node(FORE), node(HAND)) else { return Err("no right arm") };
    let above = parents.get(spine).map_err(|_| "the spine hangs from nothing")?.parent();
    let gp = world_of(above, root, parents, transforms, globals).ok_or("the spine is not under the body")?;
    let gs = world_of(spine, root, parents, transforms, globals).ok_or("the spine is not under the body")?;
    let sh = world_of(upper, root, parents, transforms, globals).ok_or("the arm is not under the body")?.translation();
    let el = world_of(fore, root, parents, transforms, globals).ok_or("the arm is not under the body")?.translation();
    let wr = world_of(hand, root, parents, transforms, globals).ok_or("the arm is not under the body")?.translation();
    let reach = sh.distance(el) + el.distance(wr);
    let pivot = gs.translation();
    let height = (sh.y - pivot.y).max(1.0);
    let flat = |v: Vec3| Vec2::new(v.x, v.z);
    let gap = flat(clasp).distance(flat(sh)) - SHOULDER_IN * reach;
    if gap <= 0.0 {
        return Ok(());
    }
    let bend = (gap / height).clamp(0.0, 1.0).asin().min(MAX_BEND) * w;
    // Forward is towards the table: turning about the level axis across it.
    let axis = Vec3::Y.cross(toward).normalize_or(Vec3::X);
    // About Y × toward a positive turn takes up towards `toward`.
    let q = Quat::from_axis_angle(axis, bend);
    let rp = rot(&gp);
    if let Ok(mut t) = transforms.get_mut(spine) {
        t.rotation = (rp.inverse() * q * rp * t.rotation).normalize();
    }
    Ok(())
}

/// An arm's bones as animated: the shoulder, elbow and wrist (world), the frame above
/// the upper arm and the bones' local transforms.
struct Arm {
    upper: Entity,
    fore: Entity,
    gp: GlobalTransform,
    lu: Transform,
    lf: Transform,
    s: Vec3,
    e0: Vec3,
    h0: Vec3,
}

fn bones(root: Entity, rig: &ActorRig, parents: &Query<&ChildOf>, transforms: &Query<&mut Transform>, globals: &Query<&GlobalTransform>) -> Result<Arm, &'static str> {
    let node = |name: &str| rig.skel.names.iter().position(|n| n.as_deref() == Some(name)).and_then(|i| rig.nodes.get(i).copied());
    let (Some(upper), Some(fore), Some(hand)) = (node(UPPER), node(FORE), node(HAND)) else { return Err("no right arm") };
    let above = parents.get(upper).map_err(|_| "the arm hangs from nothing")?.parent();
    let gp = world_of(above, root, parents, transforms, globals).ok_or("the arm is not under the body")?;
    let ro = |e: Entity| transforms.get(e).copied().map_err(|_| "a bone has no transform");
    let (lu, lf, lh) = (ro(upper)?, ro(fore)?, ro(hand)?);
    let gu = gp.mul_transform(lu);
    let gf = gu.mul_transform(lf);
    let gh = gf.mul_transform(lh);
    let (s, e0, h0) = (gu.translation(), gf.translation(), gh.translation());
    if s.distance(e0) < 1e-3 || e0.distance(h0) < 1e-3 {
        return Err("the arm has no length");
    }
    Ok(Arm { upper, fore, gp, lu, lf, s, e0, h0 })
}

impl Arm {
    /// Turn the upper arm and the forearm so the elbow is at `e` and the wrist at `h`,
    /// by `w`.
    fn apply(&self, e: Vec3, h: Vec3, w: f32, transforms: &mut Query<&mut Transform>) {
        let rp = rot(&self.gp);
        let ru = rp * self.lu.rotation;
        let du = Quat::from_rotation_arc((self.e0 - self.s).normalize(), (e - self.s).normalize_or(Vec3::NEG_Y));
        let ru2 = du * ru;
        let lu2 = (rp.inverse() * ru2).normalize();
        let cf = du * (self.h0 - self.e0);
        let df = Quat::from_rotation_arc(cf.normalize(), (h - e).normalize_or(Vec3::Y));
        let lf2 = (ru2.inverse() * df * ru2 * self.lf.rotation).normalize();
        if let Ok(mut t) = transforms.get_mut(self.upper) {
            t.rotation = self.lu.rotation.slerp(lu2, w);
        }
        if let Ok(mut t) = transforms.get_mut(self.fore) {
            t.rotation = self.lf.rotation.slerp(lf2, w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lean_follows_the_meter_towards_the_loser() {
        assert_eq!(lean_angle(0.0), 0.0);
        // Side 0 winning leans the hands towards side 1, up to about 70 degrees.
        assert!(lean_angle(0.99) > 1.1 && lean_angle(0.99) < MAX_LEAN);
        assert!(lean_angle(-0.5) < 0.0);
        // Trembling only near even, and only a little.
        assert_eq!(tremble(0.5, 1.3), 0.0);
        assert!((0..100).all(|i| tremble(0.0, i as f32 * 0.07).abs() <= TREMBLE));
    }

    #[test]
    fn the_loser_goes_down_in_a_quarter_second_and_stays() {
        assert_eq!(slam(0.5, 0, 0.0), 0.5);
        assert!((slam(0.5, 0, SLAM) - PINNED).abs() < 1e-6);
        assert!((slam(0.5, 0, 5.0) - PINNED).abs() < 1e-6);
        assert!((slam(-0.3, 1, SLAM) + PINNED).abs() < 1e-6);
    }

    #[test]
    fn the_hands_meet_over_the_middle_and_the_elbows_rest_either_side() {
        let (mid, u, b) = (Vec3::new(10.0, 50.0, 0.0), Vec3::X, 25.0);
        let (e0, h0) = clasp(mid, u, b, 0, 0.0);
        let (e1, h1) = clasp(mid, u, b, 1, 0.0);
        assert_eq!(h0, h1);
        assert!((h0.x - mid.x).abs() < 1e-4 && h0.y > mid.y);
        assert!(e0.x < mid.x && e1.x > mid.x && e0.y == mid.y);
        // Upright, each forearm keeps its length.
        assert!((e0.distance(h0) - b).abs() < 1e-3);
        // Leaning towards side 1 brings the hands over its half and lower.
        let (_, h) = clasp(mid, u, b, 0, lean_angle(0.9));
        assert!(h.x > mid.x && h.y < h0.y);
    }
}
