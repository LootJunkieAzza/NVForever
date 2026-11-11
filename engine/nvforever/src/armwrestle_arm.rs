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

/// The match resources the arms are posed from: the player's own screen, the watched match
/// and the table's stools.
type Matches<'w> = (Option<Res<'w, ArmScreen>>, Option<Res<'w, ArmWatch>>, Option<Res<'w, ArmTable>>);

/// Each frame: the wrestlers' arms posed from the match shown.
#[allow(clippy::too_many_arguments)]
pub fn arms(
    time: Res<Time>,
    (screen, watch, table): Matches,
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
        // The shoulder is to come over the elbow's spot on the table.
        let top = Vec3::new(mid.x, fnv_game::armwrestle_table::TABLE_TOP_Z + ELBOW_UP, mid.z);
        if let Err(why) = lean_in(*root, rig, (top, u, k), toward, w, &parents, &mut transforms, &globals) {
            if !std::mem::replace(&mut shown.noted, true) {
                warn!("arm wrestle arm: side {k} does not lean in: {why}");
            }
        }
        // The free hand braced flat on the table (`armwrestle_free_arm`).
        if let Err(why) = crate::armwrestle_free_arm::brace(*root, rig, top, toward, w, &parents, &mut transforms, &globals) {
            if !std::mem::replace(&mut shown.noted, true) {
                warn!("arm wrestle arm: side {k}'s free hand is not braced: {why}");
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
    let mid = Vec3::new(mx, fnv_game::armwrestle_table::TABLE_TOP_Z + ELBOW_UP, mz);
    for (k, a) in arms {
        let b = a.e0.distance(a.h0);
        // The palm's middle, from the wrist (0 for a hand without fingers).
        let pm = a.hand.as_ref().map_or(0.0, |g| g.palm);
        // The clasp: the palms' middle, a forearm and part of a hand over the elbows.
        let (elbow, centre) = clasp(mid, u, b + pm, k, lean);
        let toward = if k == 0 { u } else { -u };
        // The wrist goes where this hand, stood up in the grip, has its palm's middle just
        // this side of the clasp, palm to palm with the other; the arm reaches it with the
        // elbow bent down towards its spot on the table (on it when the arm reaches that
        // far: the palms together come first).
        let first = hand_frame((centre - elbow).normalize_or(Vec3::Y), toward, DEVIATE);
        let wrist = centre - first.0 * pm - first.2 * PALM_GAP;
        let (e, h) = solve(a.s, a.e0, a.h0, wrist, elbow - a.s);
        let log = (shown.clock * 2.0).fract() < dt * 2.0;
        if log {
            info!("arm wrestle arm: side {k} weight {w:.2} lean {:.2} shoulder {:.0?} elbow {:.0?} wrist {:.0?} -> elbow {:.0?} wrist {:.0?} clasp {:.0?}", lean, a.s, a.e0, a.h0, e, h, centre);
        }
        let fore = a.apply(e, h, w, &mut transforms);
        if let Some(g) = &a.hand {
            let (along, thumb, palm) = hand_frame((h - e).normalize_or(Vec3::Y), toward, DEVIATE);
            if log {
                info!("arm wrestle arm: side {k} hand along {along:.2?} thumb {thumb:.2?} palm {palm:.2?}; palm middle {:.1?}", h + along * pm);
            }
            g.grip(fore, (along, thumb, palm), w, &mut transforms);
        } else if !std::mem::replace(&mut shown.noted, true) {
            warn!("arm wrestle arm: side {k}'s hand has no fingers or thumb to grip with");
        }
    }
}

/// The hand's frame in the grip, in the world: along (wrist to knuckles), thumb side and
/// palm (its outward normal), for the forearm's direction `fore` (wrist from elbow) and
/// the direction `toward` the other wrestler (level). The hand stands upright, the wrist
/// bent back at most [`WRIST_UP`] to get there from a sloping forearm (straight with an
/// upright one), the palm faces the other wrestler, and the hand turns `deviate` radians
/// towards the little finger, which puts the thumb on top and crosses the two right
/// hands [design: the arm wrestling grip].
/// For a right hand, palm = thumb x along [design: the right-hand rule, checked on the
/// bones by `Grip::of`].
pub fn hand_frame(fore: Vec3, toward: Vec3, deviate: f32) -> (Vec3, Vec3, Vec3) {
    let f = fore.normalize_or(Vec3::Y);
    let up = f.angle_between(Vec3::Y).min(WRIST_UP);
    let a0 = match f.cross(Vec3::Y).try_normalize() {
        Some(axis) => Quat::from_axis_angle(axis, up) * f,
        None => f,
    };
    let p0 = (toward - a0 * toward.dot(a0)).normalize_or(toward);
    let t0 = a0.cross(p0);
    let along = (a0 * deviate.cos() - t0 * deviate.sin()).normalize();
    let thumb = along.cross(p0).normalize();
    (along, thumb, p0)
}

/// The bone the upper body bends at, and the one above it if the skeleton lacks it
/// [observed: the human skeleton's node names].
const SPINE: [&str; 2] = ["Bip01 Spine", "Bip01 Spine1"];
/// The upper body's lean in at most, radians [design: about 25 degrees; 40 put the head
/// down at the table and 50 hunched them head to head, desktop runs 2026-11-11].
const MAX_BEND: f32 = 0.45;
/// How far (level) behind the elbow the shoulder may stay, as a share of what the upper
/// arm allows [design: the shoulder over the elbow, the upper arm sloping a little].
const SHOULDER_IN: f32 = 1.0;
/// The elbow joint's height over the table top, game units (the arm's thickness) [design].
const ELBOW_UP: f32 = 2.0;
/// The hand's turn from the forearm towards the little finger in the grip, radians
/// [design: about 20 degrees, the thumbs on top; a wrist bends about 30 that way].
const DEVIATE: f32 = 0.35;
/// How far the wrist bends back to stand the hand up on a sloping forearm, radians
/// [design: about 35 degrees; a wrist bends back about 70].
const WRIST_UP: f32 = 0.6;
/// Half the gap between the two palms' middles, game units [design: a hand is about 2.5
/// thick].
const PALM_GAP: f32 = 1.1;
/// The palm's middle from the wrist, as a share of the knuckles' distance [design].
const PALM_MID: f32 = 0.6;
/// The fingers' bones, index to little, knuckle to tip [observed: the human skeleton's
/// node names]; each joint's curl towards the palm about the knuckles' line, radians
/// [design: the first joints nearly straight across the other hand's palm, the middle and
/// end ones wrapped round its back; all three curled hard made two fists knuckle to
/// knuckle, desktop run 2026-11-11 16:04].
const GRIP: [[&str; 3]; 4] = [
    ["Bip01 R Finger1", "Bip01 R Finger11", "Bip01 R Finger12"],
    ["Bip01 R Finger2", "Bip01 R Finger21", "Bip01 R Finger22"],
    ["Bip01 R Finger3", "Bip01 R Finger31", "Bip01 R Finger32"],
    ["Bip01 R Finger4", "Bip01 R Finger41", "Bip01 R Finger42"],
];
const GRIP_CURL: [[f32; 3]; 4] = [[0.3, 1.35, 1.0], [0.35, 1.4, 1.0], [0.45, 1.4, 1.0], [0.55, 1.4, 1.0]];
/// The thumb's bones, base to tip [observed: node names], and where each segment points
/// in the grip, in the hand's frame (along, thumb side, palm) [design: up and forward over
/// the other hand, the tip curled down round the other thumb].
const THUMB: [&str; 3] = ["Bip01 R Thumb1", "Bip01 R Thumb11", "Bip01 R Thumb12"];
const THUMB_DIRS: [[f32; 3]; 3] = [[0.55, 0.45, 0.7], [0.75, -0.1, 0.65], [0.3, -0.45, 0.85]];

/// Bends side `k`'s upper body in about the spine so the shoulder comes over its elbow's
/// spot by the table's middle `top` (axis `u` from side 0 to side 1), by `w`.
#[allow(clippy::too_many_arguments)]
fn lean_in(root: Entity, rig: &ActorRig, (top, u, k): (Vec3, Vec3, usize), toward: Vec3, w: f32, parents: &Query<&ChildOf>, transforms: &mut Query<&mut Transform>, globals: &Query<&GlobalTransform>) -> Result<(), &'static str> {
    let node = |name: &str| rig.skel.names.iter().position(|n| n.as_deref() == Some(name)).and_then(|i| rig.nodes.get(i).copied());
    let spine = SPINE.iter().find_map(|n| node(n)).ok_or("no spine")?;
    let (Some(upper), Some(fore), Some(hand)) = (node(UPPER), node(FORE), node(HAND)) else { return Err("no right arm") };
    let above = parents.get(spine).map_err(|_| "the spine hangs from nothing")?.parent();
    let gp = world_of(above, root, parents, transforms, globals).ok_or("the spine is not under the body")?;
    let gs = world_of(spine, root, parents, transforms, globals).ok_or("the spine is not under the body")?;
    let sh = world_of(upper, root, parents, transforms, globals).ok_or("the arm is not under the body")?.translation();
    let el = world_of(fore, root, parents, transforms, globals).ok_or("the arm is not under the body")?.translation();
    let wr = world_of(hand, root, parents, transforms, globals).ok_or("the arm is not under the body")?.translation();
    let upper_len = sh.distance(el);
    let (spot, _) = clasp(top, u, el.distance(wr), k, 0.0);
    let pivot = gs.translation();
    let height = (sh.y - pivot.y).max(1.0);
    let flat = |v: Vec3| Vec2::new(v.x, v.z);
    // Level from the elbow's spot, where the upper arm reaches down to it.
    let over = (upper_len * upper_len - (sh.y - spot.y).powi(2)).max(0.0).sqrt() * SHOULDER_IN;
    let gap = flat(spot).distance(flat(sh)) - over;
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
    hand: Option<Grip>,
}

/// One hand's bones for the grip, and its own frame measured from them: along (wrist to
/// the knuckles' middle), thumb side and palm (thumb x along, a right hand), as columns,
/// in the hand bone's local space.
struct Grip {
    hand: Entity,
    fingers: [[Option<Entity>; 3]; 4],
    thumb: [Option<Entity>; 3],
    axes: Mat3,
    palm: f32,
}

impl Grip {
    fn of(rig: &ActorRig, hand: Entity, parents: &Query<&ChildOf>, transforms: &Query<&mut Transform>) -> Option<Grip> {
        let node = |name: &str| rig.skel.names.iter().position(|n| n.as_deref() == Some(name)).and_then(|i| rig.nodes.get(i).copied());
        let fingers = GRIP.map(|f| f.map(node));
        let thumb = THUMB.map(node);
        // Where a bone hung from the hand sits, in the hand's space.
        let at = |e: Entity| -> Option<Vec3> {
            (parents.get(e).ok()?.parent() == hand).then_some(())?;
            Some(transforms.get(e).ok()?.translation)
        };
        let knuckles: Vec<Vec3> = fingers.iter().filter_map(|f| f[0].and_then(at)).collect();
        if knuckles.len() < 2 {
            return None;
        }
        let k = knuckles.iter().sum::<Vec3>() / knuckles.len() as f32;
        let a = k.try_normalize()?;
        let t = thumb[0].and_then(at)?;
        let t = (t - a * t.dot(a)).try_normalize()?;
        let p = t.cross(a).normalize();
        let t = a.cross(p);
        Some(Grip { hand, fingers, thumb, axes: Mat3::from_cols(a, t, p), palm: k.length() * PALM_MID })
    }

    /// Turn the hand to the world frame `(along, thumb, palm)` under the forearm's world
    /// turn `fore`, and close the fingers and the thumb round the other hand, by `w`.
    fn grip(&self, fore: Quat, (along, thumb, palm): (Vec3, Vec3, Vec3), w: f32, transforms: &mut Query<&mut Transform>) {
        let Ok(lh) = transforms.get(self.hand).copied() else { return };
        let world = Mat3::from_cols(along, thumb, palm) * self.axes.transpose();
        let to = (fore.inverse() * Quat::from_mat3(&world)).normalize();
        if let Ok(mut t) = transforms.get_mut(self.hand) {
            t.rotation = lh.rotation.slerp(to, w);
        }
        let (a, t, p) = (self.axes.x_axis, self.axes.y_axis, self.axes.z_axis);
        for (bones, curls) in self.fingers.iter().zip(GRIP_CURL) {
            // Straight out along the hand, keeping the finger's own spread, then curled
            // joint by joint towards the palm about the knuckles' line.
            let Some(first) = bones[0] else { continue };
            let Ok(l0) = transforms.get(first).copied() else { continue };
            let spread = l0.rotation * Vec3::X;
            let straight = (spread - p * spread.dot(p)).normalize_or(a);
            let dirs = curls.iter().scan(0.0, |sum, c| {
                *sum += c;
                Some(Quat::from_axis_angle(t, *sum) * straight)
            });
            pose_chain(bones, dirs, w, transforms);
        }
        let dirs = THUMB_DIRS.iter().map(|d| (a * d[0] + t * d[1] + p * d[2]).normalize());
        pose_chain(&self.thumb, dirs, w, transforms);
    }
}

/// Point each bone of a chain hung from the hand (each running along its own +X [observed:
/// the finger bones' children are offset only in X, as in `darts_arm`]) along its
/// direction in the hand's space, by `w`.
fn pose_chain(bones: &[Option<Entity>; 3], dirs: impl Iterator<Item = Vec3>, w: f32, transforms: &mut Query<&mut Transform>) {
    // The parent's turn in the hand's space, as posed so far.
    let mut parent = Quat::IDENTITY;
    for (bone, dir) in bones.iter().zip(dirs) {
        let Some(bone) = *bone else { return };
        let Ok(mut tr) = transforms.get_mut(bone) else { return };
        let now = parent * tr.rotation;
        let to = Quat::from_rotation_arc((now * Vec3::X).normalize_or(Vec3::X), dir) * now;
        let local = (parent.inverse() * to).normalize();
        tr.rotation = tr.rotation.slerp(local, w);
        parent *= tr.rotation;
    }
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
    let hand = Grip::of(rig, hand, parents, transforms);
    Ok(Arm { upper, fore, gp, lu, lf, s, e0, h0, hand })
}

impl Arm {
    /// Turn the upper arm and the forearm so the elbow is at `e` and the wrist at `h`,
    /// by `w`.
    fn apply(&self, e: Vec3, h: Vec3, w: f32, transforms: &mut Query<&mut Transform>) -> Quat {
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
        let (u2, f2) = (self.lu.rotation.slerp(lu2, w), self.lf.rotation.slerp(lf2, w));
        if let Ok(mut t) = transforms.get_mut(self.fore) {
            t.rotation = f2;
        }
        // The forearm's world turn as posed.
        rp * u2 * f2
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
    fn in_the_grip_the_palms_face_each_other_and_the_thumbs_are_up() {
        let (a0, t0, p0) = hand_frame(Vec3::Y, Vec3::X, DEVIATE);
        let (a1, t1, p1) = hand_frame(Vec3::Y, -Vec3::X, DEVIATE);
        // On a forearm sloping 45 degrees towards the other wrestler the hand still
        // stands nearly upright, the palm facing the other wrestler, not the table.
        let (a, _, p) = hand_frame(Vec3::new(1.0, 1.0, 0.0), Vec3::X, DEVIATE);
        assert!(a.y > 0.85 && p.x > 0.95, "{a} {p}");
        for (a, t, p) in [(a0, t0, p0), (a1, t1, p1)] {
            assert!(a.dot(t).abs() < 1e-5 && a.dot(p).abs() < 1e-5 && t.dot(p).abs() < 1e-5);
            // A right hand: palm = thumb x along.
            assert!(t.cross(a).distance(p) < 1e-5);
            // The thumb is on top, the hand still mostly upright.
            assert!(t.y > 0.2 && a.y > 0.9);
        }
        // Palm to palm, and the two hands crossed.
        assert!(p0.dot(p1) < -0.99 && p0.x > 0.99);
        assert!(a0.z * a1.z < 0.0);
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
