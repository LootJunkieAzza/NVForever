//! The wrestlers' right arms at the arm-wrestling table (queue item 199; the bout is
//! `nvf_play::armwrestle`): no clip the game or a mod has is arm wrestling, so the right
//! arm of whoever sits at the table is posed here, over the seated pose its body plays
//! [design: owner 2026-11-11, "arm wrestling animated"; the original game has no arm
//! wrestling].
//!
//! How a real arm wrestle moves [documented: Canadian Armwrestling Federation rules,
//! cawf.ca/rulessummarized; armpower.net "savoir vivre at the armwrestling table"; the
//! Arm wrestling article on Wikipedia]: the elbow stays on the pad the whole round, the
//! grip is palm to palm with the thumb knuckle showing, and the match is won by turning
//! the clasped hands down to the pad.
//!
//! - **The plant.** At the countdown each wrestler's right elbow is set on the table top,
//!   near the line between the chairs a little to its own right (the pair set a few units
//!   off it, away from the wall, [`PLANT_OFF`]), and does not move again
//!   ([`plant`]). The clasp starts straight up over the middle of the two elbows, the
//!   forearms nearly upright and leaning a little towards each other.
//! - **One angle.** The only motion is the two forearms turning together about the line
//!   through the two elbows: one angle from the meter ([`lean_angle`], with a small
//!   [`tremble`] near even), the clasp swinging sideways towards the winner's left (the
//!   loser's right). Elbow to clasp stays one length; nothing pushes along the line
//!   between the wrestlers ([`swing`]).
//! - **One clasp.** Both right hands are posed from the same turned frame, palm to palm,
//!   so they cannot come apart.
//! - **The win.** The hands go down in [`SLAM`] and stop where the hands reach the table
//!   top ([`pin_sides`]), not through it, and stay through the celebration; the pose is
//!   let go ([`EASE`]) when the bout's screen closes, the match view goes, or a wrestler
//!   gets up.
//! - **Everyone sees the same arms**: the pose is driven by the bout every game already
//!   has: this player's own (`ArmScreen`, in co-op filled from the deciding game), or the
//!   match a watcher sees (`ArmWatch`, the host's table view). Each side's body is
//!   whoever sits on that side's stool here: this player's own body, another player's
//!   body, or the NPC opponent.
//!
//! The bones are written after the animation has posed them and before transforms are
//! propagated, as the darts arm (`darts_arm.rs`), whose two-bone reach this uses.

use crate::darts_arm::{rot, world_of};
use crate::furniture::OnFurniture;
use crate::limbs::ActorRig;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::armwrestle::{board_after, ArmScreen, ArmTable, Page, RESULT_HOLD};
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
/// Seconds for the pose to ease in [design].
const EASE: f32 = 0.3;
/// Seconds for the arms to let go after the result, back to the seated pose before the
/// wrestlers get up [design: armwrestle-c 2026-11-11, 0.3 s jumped between two frames a
/// quarter second apart]. The let-go starts this long before the player gets up.
const LET_GO: f32 = 0.8;
/// Seconds the loser's hand stays down before the arms let go, for a watcher whose
/// screen does not close [design: the YOU WIN highlight (2.45) and the leaderboard (2.6)];
/// the wrestlers let go when they get up.
const HOLD: f32 = 5.0;
/// How near a chair a body sits to be taken for its wrestler, game units [design].
const ON_SEAT: f32 = 40.0;
/// How far each elbow is planted from the table's middle, along the line between the
/// chairs, game units [design: the elbows about a forearm apart (owner 2026-11-11); 7
/// (the forearms leaning 14 degrees in) made the seated bodies, 35 from the middle, bend
/// over so far that their heads met the clasp, spectator run 2026-11-11 00:23; 9.5 still
/// bent them about 40 degrees, the faces level with the clasp, parity ship14 pvp run].
const ELBOW_OUT: f32 = 11.0;
/// How far each elbow sits to its own wrestler's right of the line between the chairs,
/// game units [design: the turn is about the line through both elbows, so the offsets skew
/// it off the line between the chairs and the clasp drifts that way as it goes down; 1.5
/// skewed it about 14 degrees with the elbows 12.4 apart (a 5 unit drift at the pin), 0.75
/// about 7].
const ELBOW_SIDE: f32 = 0.75;
/// How far above the table top the palms' middles stay at the pin, game units [design:
/// the hand's lowest bone about 2 over the top, the skin on it; with the handshake grip
/// the lowest bone is about 3.5 under its palm's middle at the pin (6.5 left it 5.8 over
/// the top, run 2026-11-11 19:1x); before it, 2.2 put the loser's finger bones 1.7 under
/// the top (owner: the arm "clipped through the table")].
const HAND_CLEAR: f32 = 3.0;
/// How far from the table's middle, across the line between the chairs, the knuckle bones
/// may go at the pin, game units [design: the workbench top runs 20.9 either side (19 x the
/// scale 1.1) and the ledge over the wall behind it starts at the top's height 23.3 out
/// (armwrestle-c, fnvtool phys ray). The fingers wrapped round the other hand go a little
/// further out than the knuckles: the outermost drawn finger bone is logged and must stay
/// under 23. 17 (armwrestle-c's ask) is out of reach with the elbows planted near the
/// middle: a lying forearm (about 18) already reaches past it, and 17 stopped the pin at 46
/// degrees with the hands 13 over the top; 18.5 at 56 degrees, 10 over (run 2026-11-11 20:1x)].
const EDGE_IN: f32 = 20.9;
/// The saloon table's top runs on a long way on the side away from the wall: the hands
/// may go this far that way [design: the workbench is 44.3 along (armwrestle_table), and
/// the shot from the south shows its top running past the bottom of the frame].
const OPEN_EDGE: f32 = 36.0;
/// The open side of the table (Bevy space, level): away from the wall behind it, the
/// cell's -y [observed: the wall at y 382.5, `fnv_game::armwrestle_table`].
const OPEN: Vec3 = Vec3::Z;
/// How far the plant (elbows and clasp) sits off the line between the chairs, towards
/// the open side, game units [design: planted on the line, a pin towards the wall lay at
/// the top's edge, 19.6 out, and from the south the hands showed against the wall at face
/// height, not on the table (parity, ship14 watch-close play-59s and 62s); 5 left the
/// clasp's top on the far edge's line from there, run tw-win2].
const PLANT_OFF: f32 = 9.0;
/// Going down, the clasped hands lie over along the table, each tilting this much more
/// towards the other wrestler at its wrist, from [`FLAT_FROM`] radians of lean to the pin
/// [design: kept upright the hands stuck out across the table past its edge, ship 11 pvp
/// run, the knuckles about 21.5 from the middle].
const FLAT: f32 = 0.5;
const FLAT_FROM: f32 = 0.6;
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

/// The two planted elbows and the clasp over them, from the table's middle `mid` (at the
/// elbow joints' height), the axis `u` from side 0's chair to side 1's (level, unit), each
/// side's forearm (elbow to wrist) and palm (wrist to the palm's middle). Side 0's right
/// is `u x Y`.
///
/// The grip is a handshake raised on the forearms [documented: CAWF rules summarized, the
/// grip palm to palm with the top thumb knuckle showing]: each right hand's palm faces its
/// own left, across the clasp, the back of the hand out to its own right, the hand
/// running up from the wrist and over towards the other wrestler ([`HAND_TILT`]) so the
/// thumb is on top. The palms' middles sit [`PALM_GAP`] either side of the clasp, skin on
/// skin; the elbows on the table, a forearm from their wrists, [`ELBOW_SIDE`] to their own
/// right.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plant {
    pub elbows: [Vec3; 2],
    pub clasp: Vec3,
    /// Each hand's palm middle at the start.
    pub palms: [Vec3; 2],
    /// Each wrist at the start.
    pub wrists: [Vec3; 2],
    /// Each hand's frame at the start: along (wrist to knuckles) and the palm's facing.
    pub hands: [(Vec3, Vec3); 2],
}

pub fn plant(mid: Vec3, u: Vec3, fore: [f32; 2], palm: [f32; 2]) -> Plant {
    let right = u.cross(Vec3::Y).normalize_or(Vec3::X);
    let toward = [u, -u];
    let own_right = [right, -right];
    let hands = [0, 1].map(|k| ((Vec3::Y * HAND_TILT.cos() + toward[k] * HAND_TILT.sin()).normalize(), -own_right[k]));
    // Level offsets from the middle: each palm's middle, each wrist below it, each elbow.
    let level = |v: Vec3| v - Vec3::Y * v.y;
    let palm_at = [0, 1].map(|k| own_right[k] * PALM_GAP);
    let wrist_at = [0, 1].map(|k| level(palm_at[k] - hands[k].0 * palm[k]));
    let elbow_at = [0, 1].map(|k| own_right[k] * ELBOW_SIDE - toward[k] * ELBOW_OUT);
    // As high as the shorter forearm reaches from its elbow.
    let hc = (0..2)
        .map(|k| hands[k].0.y * palm[k] + (fore[k] * fore[k] - (wrist_at[k] - elbow_at[k]).length_squared()).max(0.0).sqrt())
        .fold(f32::MAX, f32::min);
    let clasp = mid + Vec3::Y * hc;
    let palms = [0, 1].map(|k| clasp + palm_at[k]);
    let wrists = [0, 1].map(|k| palms[k] - hands[k].0 * palm[k]);
    // The longer forearm's elbow a little further out, so both reach exactly.
    let elbows = [0, 1].map(|k| {
        let w = wrists[k] - mid;
        let side = own_right[k] * ELBOW_SIDE;
        let along_u = u * w.dot(u);
        let across = w - along_u - Vec3::Y * w.y - side;
        let back = (fore[k] * fore[k] - w.y * w.y - across.length_squared()).max(0.0).sqrt();
        mid + along_u + side - toward[k] * back
    });
    Plant { elbows, clasp, palms, wrists, hands }
}

/// The turn by `theta` (towards side 1's stool is the lean's sign: side 0 winning takes the
/// clasp towards side 0's left) about the line through the two elbows, as a rotation and
/// the point it turns about.
pub fn swing(p: &Plant, theta: f32) -> (Quat, Vec3) {
    let axis = (p.elbows[1] - p.elbows[0]).normalize_or(Vec3::X);
    // About the elbows' line (about u) a positive turn takes up towards side 0's right, so
    // the lean turns the other way.
    (Quat::from_axis_angle(axis, -theta), p.elbows[0])
}

/// A point of the start pose turned by `theta`.
pub fn turned(p: &Plant, theta: f32, at: Vec3) -> Vec3 {
    let (q, o) = swing(p, theta);
    o + q * (at - o)
}

/// How much further the hands tilt towards each other at the lean `theta` ([`FLAT`]).
fn flat(theta: f32) -> f32 {
    let s = ((theta.abs() - FLAT_FROM) / (PINNED - FLAT_FROM)).clamp(0.0, 1.0);
    FLAT * s * s * (3.0 - 2.0 * s)
}

/// Side `k`'s hand at the lean `theta`: along (wrist to knuckles), the palm's facing and
/// the wrist. The wrist stays where the turn takes it, a forearm from the planted elbow;
/// the hand turns at it about the palm's facing, towards the other wrestler ([`flat`]).
pub fn hand_at(p: &Plant, theta: f32, k: usize) -> (Vec3, Vec3, Vec3) {
    let (q, _) = swing(p, theta);
    let (along0, palm0) = p.hands[k];
    let palm = (q * palm0).normalize();
    let along = (Quat::from_axis_angle(palm, flat(theta)) * (q * along0)).normalize();
    (along, palm, turned(p, theta, p.wrists[k]))
}

/// Side `k`'s palm middle and knuckles at the lean `theta`.
pub fn palm_and_knuckles(p: &Plant, theta: f32, k: usize) -> (Vec3, Vec3) {
    let (along, _, wrist) = hand_at(p, theta, k);
    let len = p.palms[k].distance(p.wrists[k]);
    (wrist + along * len, wrist + along * len / PALM_MID)
}

/// The elbow for an upper arm `ua` from the shoulder `s` and a forearm `b` to the wrist
/// `wrist`, as near the planted elbow `plant` as the two lengths allow: the wrist exactly
/// where the shared clasp puts it, so the hands stay joined. Out of reach, the arm
/// straight towards the wrist.
pub fn elbow_between(s: Vec3, wrist: Vec3, ua: f32, b: f32, plant: Vec3) -> Vec3 {
    let d = wrist - s;
    let len = d.length();
    let Some(n) = d.try_normalize() else { return s + (plant - s).normalize_or(Vec3::NEG_Y) * ua };
    if len >= ua + b || len <= (ua - b).abs() {
        return s + n * ua;
    }
    // The circle the elbow may be on: its middle along the shoulder-wrist line, its radius.
    let along = (ua * ua - b * b + len * len) / (2.0 * len);
    let c = s + n * along;
    let r = (ua * ua - along * along).max(0.0).sqrt();
    let off = plant - c;
    let side = (off - n * off.dot(n)).try_normalize().unwrap_or_else(|| n.any_orthonormal_vector());
    c + side * r
}

/// How far the clasp may turn each way (towards side 1's stool, then the other way) on a
/// table whose top is open further on one side of the line between the chairs: the
/// knuckles stay [`EDGE_IN`] from `line` (a point on that line) towards the wall and
/// [`OPEN_EDGE`] from it towards `open` (level, unit), and the palms over the top as
/// both palms' middles [`HAND_CLEAR`] over the top.
pub fn pin_sides(p: &Plant, top: f32, line: Vec3, open: Vec3) -> (f32, f32) {
    let low = |a: f32| (0..2).any(|k| palm_and_knuckles(p, a, k).0.y < top + HAND_CLEAR);
    let wide = |a: f32| {
        (0..2).any(|k| {
            let o = (palm_and_knuckles(p, a, k).1 - line).dot(open);
            o > OPEN_EDGE || o < -EDGE_IN
        })
    };
    let way = |s: f32| {
        let mut last = 0.0;
        for i in 0..=880 {
            let a = s * i as f32 * PINNED / 880.0;
            if low(a) {
                break;
            }
            if !wide(a) {
                last = a.abs();
            }
        }
        last
    };
    (way(1.0), way(-1.0))
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
    /// The arms have let go after this win: they stay so until the next bout (a wrestler
    /// who got up then sees the match as a watcher, whose hold is longer).
    released: bool,
    /// How far each side's upper body follows its losing arm, eased ([`losing`]).
    follow: [f32; 2],
}

/// The match this game shows: this player's own bout (after the card), else the one it
/// watches.
fn live(screen: &ArmScreen, watch: &ArmWatch, table: &ArmTable) -> Option<Live> {
    if let Some(a) = screen.0.as_ref().filter(|a| a.page != Page::Offer && !a.leaving && a.left_at.is_none()) {
        // Seated and waiting for the bout (the opponent still walking over, or the other
        // player not in yet): posed as even, so the lean and the braced free hand start
        // from the seat (armwrestle-c, 2026-11-11).
        let (meter, winner) = a.bout.as_ref().map_or((0.0, None), |b| (b.meter, b.winner));
        let mut stools = [0; 2];
        stools[a.side as usize & 1] = a.stool.0;
        stools[1 - (a.side as usize & 1)] = a.other.0;
        return Some(Live { table: table.table.map_or(a.stool.0, |t| t.0), meter, winner, stools });
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
    // How long the loser's hand stays down: a wrestler's arms let go [`LET_GO`] before it
    // gets up (after the YOU WIN highlight and the leaderboard), a watcher's after [`HOLD`].
    let own = screen.0.as_ref().filter(|a| a.page != Page::Offer && a.left_at.is_none()).map(|a| a.side & 1);
    let held = |w: u8| own.map_or(HOLD, |s| (board_after(w == s) + RESULT_HOLD - LET_GO).max(0.0));
    // A wrestler getting up after the win: this player off its stool, or a body getting
    // off one (a watcher's hold outlasted the wrestlers' sitting, both stood with the
    // hands still clasped on the table, run tw-loss3 2026-11-12).
    let stools = shown.stools;
    let mine_up = seat.as_deref().is_some_and(|s| s.hold.is_some() && !s.seated && s.on.is_some_and(|o| stools.contains(&o)));
    let body_up = rigs.iter().any(|(_, _, on, _)| on.is_some_and(|o| o.getting_off() && stools.contains(&o.furniture())));
    // Getting up, or a while after the win: the arms let go.
    let after_win = now_live.is_some_and(|l| l.winner.is_some()) && (shown.released || mine_up || body_up || shown.won.is_some_and(|(w, at, _)| shown.clock - at > held(w)));
    if screen.0.as_ref().is_some_and(|a| a.leaving) || after_win {
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
                (None, Some(_)) => {
                    shown.won = None;
                    shown.released = false;
                }
                _ => {}
            }
        }
        None => {
            if shown.weight <= 0.0 {
                return;
            }
            shown.weight = (shown.weight - dt / LET_GO).max(0.0);
            if shown.weight <= 0.0 {
                shown.released = shown.won.is_some();
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
    // Each body's forearm and palm first (the bones' lengths do not change with the
    // lean), then the elbows planted for both.
    let lengths: Vec<(usize, f32, f32)> = bodies
        .iter()
        .enumerate()
        .filter_map(|(k, b)| {
            let (root, rig) = b.as_ref()?;
            let a = bones(*root, rig, &parents, &transforms, &globals).ok()?;
            Some((k, a.e0.distance(a.h0), a.hand.as_ref().map_or(0.0, |g| g.palm)))
        })
        .collect();
    if lengths.is_empty() {
        return;
    }
    let of = |k: usize| lengths.iter().find(|(j, _, _)| *j == k).or(lengths.first()).map_or((18.0, 4.0), |r| (r.1, r.2));
    // The grip waits for both wrestlers: one seated alone leans in and braces its free
    // hand, its right hand not reaching for nobody (armwrestle-c, 2026-11-11).
    let both = bodies.iter().all(Option::is_some);
    let top = fnv_game::armwrestle_table::TABLE_TOP_Z;
    let line = Vec3::new(mid.x, top + ELBOW_UP, mid.z);
    let centre = line + OPEN * PLANT_OFF;
    let pl = plant(centre, u, [of(0).0, of(1).0], [of(0).1, of(1).1]);
    let (pin_up, pin_down) = pin_sides(&pl, top, line, OPEN);
    let pin = if lean >= 0.0 { pin_up } else { pin_down };
    let theta = lean.clamp(-pin_down, pin_up);
    for k in 0..2 {
        // Side 0 loses towards the down pin, side 1 towards the up one.
        let to = losing(theta, k, if k == 0 { pin_down } else { pin_up });
        shown.follow[k] += (to - shown.follow[k]) * (dt * BODY_FOLLOW).min(1.0);
    }
    let clasp_now = turned(&pl, theta, pl.clasp);
    let log = (shown.clock * 2.0).fract() < dt * 2.0;
    if log {
        info!("arm wrestle arm: angle {theta:+.2} (pin {pin:.2}) elbows {:.1?} {:.1?} clasp {clasp_now:.1?} lowest palm {:.1} over the top, knuckles {:+.1} {:+.1} from the chairs' line towards the open side", pl.elbows[0], pl.elbows[1], (0..2).map(|k| palm_and_knuckles(&pl, theta, k).0.y).fold(f32::MAX, f32::min) - top, (palm_and_knuckles(&pl, theta, 0).1 - line).dot(OPEN), (palm_and_knuckles(&pl, theta, 1).1 - line).dot(OPEN));
    }
    let mut drawn: [Option<Vec3>; 2] = [None, None];
    for (k, b) in bodies.iter().enumerate() {
        let Some((root, rig)) = b else { continue };
        let toward = if k == 0 { u } else { -u };
        // Leaning in over the table first, the shoulder over its planted elbow: the arm
        // hangs from the leaned chest.
        let follow = if both { shown.follow[k] } else { 0.0 };
        match lean_in(*root, rig, pl.elbows[k], toward, follow, w, &parents, &mut transforms, &globals) {
            Ok(Some(head)) if log => info!("arm wrestle arm: side {k}'s head {head:.1?}: {:.1} over the top, {:.1} over the clasp, {:.1} from it", head.y - top, head.y - clasp_now.y, head.distance(clasp_now)),
            Ok(_) => {}
            Err(why) => {
                if !std::mem::replace(&mut shown.noted, true) {
                    warn!("arm wrestle arm: side {k} does not lean in: {why}");
                }
            }
        }
        // The shoulders' line as posed: losing, it tips down to the playing arm's side
        // (owner 2026-11-12: "the shoulders and upper torso should tilt to the side of the
        // playing arm the more they lose").
        if log && both {
            if let Some(t) = shoulder_tilt(*root, rig, &parents, &transforms, &globals) {
                info!("arm wrestle arm: side {k} follows its losing arm by {follow:.2}: the shoulders' line {t:+.1} deg (negative: down to the right arm)");
            }
        }
        // The free hand braced flat on the table (`armwrestle_free_arm`).
        let top_mid = Vec3::new(mid.x, top + ELBOW_UP, mid.z);
        if let Err(why) = crate::armwrestle_free_arm::brace(*root, rig, top_mid, toward, w, &parents, &mut transforms, &globals) {
            if !std::mem::replace(&mut shown.noted, true) {
                warn!("arm wrestle arm: side {k}'s free hand is not braced: {why}");
            }
        }
        let a = match bones(*root, rig, &parents, &transforms, &globals) {
            Ok(a) => a,
            Err(why) => {
                if !std::mem::replace(&mut shown.noted, true) {
                    warn!("arm wrestle arm: side {k}'s arm is not posed: {why}");
                }
                continue;
            }
        };
        let (ua, b) = (a.s.distance(a.e0), a.e0.distance(a.h0));
        if !both {
            continue;
        }
        // The start pose of this hand ([`plant`]), turned by the one angle about the
        // elbows' line.
        let e_plant = pl.elbows[k];
        let (along, palm, wrist) = hand_at(&pl, theta, k);
        // The grip frame's second column, as [`Grip`] measures it (palm x along).
        let thumb = palm.cross(along).normalize();
        // The wrist exactly where the shared clasp puts it (the hands joined); the elbow as
        // near its planted spot as the arm's two lengths allow from where the lean put the
        // shoulder.
        let e = elbow_between(a.s, wrist, ua, b, e_plant);
        let h = e + (wrist - e).normalize_or(Vec3::Y) * b;
        if log {
            info!("arm wrestle arm: side {k} weight {w:.2} shoulder {:.1?} -> elbow {e:.1?} (planted {e_plant:.1?}, off {:.1}) wrist {h:.1?} (wanted {wrist:.1?}, off {:.1})", a.s, e.distance(e_plant), h.distance(wrist));
        }
        let fore = a.apply(e, h, w, &mut transforms);
        if let Some(g) = &a.hand {
            g.grip(fore, (along, thumb, palm), w, &mut transforms);
            // How it came out, from the bones as drawn last frame: the elbow's place, the
            // palm's middle and the hand's lowest bone over the top.
            let at = |e: Option<Entity>| e.and_then(|e| globals.get(e).ok()).map(|g| g.translation());
            if let (Some(el), Some(hb), Some(k2)) = (at(Some(a.fore)), at(Some(g.hand)), at(g.fingers[1][0])) {
                let palm_mid = hb + (k2 - hb) * PALM_MID;
                drawn[k] = Some(palm_mid);
                if log {
                    let low = g.fingers.iter().flatten().chain(g.thumb.iter()).chain(std::iter::once(&Some(g.hand))).filter_map(|e| at(*e)).map(|v| v.y).fold(f32::MAX, f32::min);
                    let out = g.fingers.iter().flatten().chain(g.thumb.iter()).filter_map(|e| at(*e)).map(|v| -(v - line).dot(OPEN)).fold(f32::MIN, f32::max);
                    info!("arm wrestle arm: check side {k}: elbow drawn {el:.1?} ({:.1} from the plant), palm middle {palm_mid:.1?}, lowest hand bone {:.1} over the top, the finger bone nearest the wall {out:.1} from the chairs' line that way", el.distance(e_plant), low - top);
                }
            }
        } else if !std::mem::replace(&mut shown.noted, true) {
            warn!("arm wrestle arm: side {k}'s hand has no fingers or thumb to grip with");
        }
    }
    if log {
        if let [Some(p0), Some(p1)] = drawn {
            info!("arm wrestle arm: check: the palms' middles {:.2} apart (the hands' bones, {:.1} is palm to palm)", p0.distance(p1), 2.0 * PALM_GAP);
        }
    }
}

/// The bone the upper body bends at, and the one above it if the skeleton lacks it
/// [observed: the human skeleton's node names].
const SPINE: [&str; 2] = ["Bip01 Spine", "Bip01 Spine1"];
/// The upper body's lean in at most, radians [design: about 43 degrees, with the head
/// turned back up ([`HEAD_UP`]): the shoulders must come over the elbows for the elbows
/// to rest on the table with the hands touching (owner, 2026-11-11 17:0x)].
const MAX_BEND: f32 = 0.75;
/// The head: turned back up by this share of the lean [observed: the node name; the
/// clavicles hang from the neck, so turning the neck back lifted the shoulders 4.4 off
/// where the lean put them (run 2026-11-11 19:0x); design: a 0.7 lean without it put the
/// head down at the table, run 2026-11-11 15:3x; 1.0, all of it, with the twist: the face
/// looks straight across, ship14 pvp run].
const NECK: &str = "Bip01 Head";
/// 0.5: half the bend kept, the face tipped down to the hands; at 1.0 the wrestlers looked
/// level across at each other (owner 2026-11-12: "players should be looking at their hands";
/// the head bone turns at its own joint, so its place, 10 over the clasp and 25 from it in
/// b-win-view15, does not change) [design].
const HEAD_UP: f32 = 0.5;
/// The upper body's twist about the vertical, right shoulder forward, radians (about 34
/// degrees) [design: arm wrestlers set the right shoulder in behind the arm and keep the
/// head back over their own side; bending in alone to bring the shoulder over the elbow
/// (about 40 degrees) put each face level with the clasp, the heads touching the hands
/// (parity, ship14 laptop pvp run, play-30s)].
const TWIST: f32 = 0.6;
/// The share of the twist the head turns back round, to look across the table [design].
const HEAD_ROUND: f32 = 0.85;
/// The elbow joint's height over the table top, game units (the arm's thickness) [design:
/// 2 sank the elbow and forearm into the top, laptop ship 12 watch-wide run 50-62 s].
const ELBOW_UP: f32 = 3.0;
/// Half the gap between the two palms' middles (the hand bones' line), game units: the
/// palm's skin stands this far off the bones, so two palms at twice it touch skin to skin
/// [observed: armwrestle-c's PALM_DEPTH, measured from lefthand.nif's skin; 0.7 put the
/// palms inside each other, owner 2026-11-11: "palm to palm is too close"].
const PALM_GAP: f32 = 3.0;
/// Each hand's lean from upright towards the other wrestler, radians (about 49 degrees)
/// [design: the handshake raised, thumbs on top; owner 2026-11-11, the hands read
/// "upside down"].
const HAND_TILT: f32 = 0.85;
/// The palm's middle from the wrist, as a share of the knuckles' distance [design].
const PALM_MID: f32 = 0.6;
/// The fingers' bones, index to little, knuckle to tip [observed: the human skeleton's
/// node names]; each joint's curl from its bind turn towards the palm about the finger's
/// own hinge, radians [design: closed round the edge and onto the back of the other hand,
/// about 140 degrees a finger; owner 2026-11-11: the fingers must close fully, not float
/// half open; 186 degrees closed them into a ring through the other hand, co-op run
/// 2026-11-12 00:33].
const GRIP: [[&str; 3]; 4] = [
    ["Bip01 R Finger1", "Bip01 R Finger11", "Bip01 R Finger12"],
    ["Bip01 R Finger2", "Bip01 R Finger21", "Bip01 R Finger22"],
    ["Bip01 R Finger3", "Bip01 R Finger31", "Bip01 R Finger32"],
    ["Bip01 R Finger4", "Bip01 R Finger41", "Bip01 R Finger42"],
];
const GRIP_CURL: [[f32; 3]; 4] = [[0.8, 1.0, 0.55], [0.85, 1.0, 0.6], [0.9, 1.05, 0.6], [0.95, 1.05, 0.65]];
/// The thumb's bones, base to tip [observed: node names], and where each segment points
/// in the grip, in the hand's frame (along, the thumb's side, palm) [design: out from the
/// thumb's side, then over the top of the other hand towards the palm, the tip hooked
/// round the other thumb].
const THUMB: [&str; 3] = ["Bip01 R Thumb1", "Bip01 R Thumb11", "Bip01 R Thumb12"];
const THUMB_DIRS: [[f32; 3]; 3] = [[0.5, 0.6, 0.6], [0.75, 0.05, 0.65], [0.35, -0.35, 0.85]];

/// The losing side's upper body follows its arm a little [design: owner 2026-11-12, "when
/// someone is losing the torso should follow a little bit, keep the elbow anchored"; with
/// only the arm bent, nothing on the body supported it]: at most this much rolled down to
/// its own right (where the clasp goes), radians (about 34 degrees; 0.2 hardly showed from
/// over the table, pvpwb run b-win-view12; at 0.3 the Settler's chest, seen from the front
/// mid-bout, still slanted away from his arm, owner 2026-11-12 drawing on b-win-view15
/// watch-close; at 0.5 the loser still read as going forward, not to the side, owner
/// 2026-11-12 on b-win-view16; 0.8 laid one seat's loser over the table, b-win-view18,
/// even with the bend held, so 0.6) ...
const FOLLOW_ROLL: f32 = 0.6;
/// ... and turned, the right shoulder brought a little forward after its hand, radians
/// (about 6 degrees) [design: turned back (0.25 the other way), it took the shoulder away
/// from its planted elbow and the bend in made that up by pitching the chest forward over
/// the table, the loser going forward, not to the side (owner 2026-11-12, b-win-view16);
/// with this turn, the bend for the full roll is about the plain lean's, the test below].
const FOLLOW_TWIST: f32 = 0.1;
/// How fast the upper body takes up the follow (per second) [design: it lags the hands a
/// little, so the 0.25 s slam does not snap the chest].
const BODY_FOLLOW: f32 = 6.0;
/// How far the leaned shoulder may miss an upper arm from its planted elbow before the
/// follow is taken back (world units) [design: the elbow is drawn as near its plant as
/// the arm's lengths allow, so this is about how far it may come off it].
const FOLLOW_MISS: f32 = 0.5;
/// How much further in than the plain lean the follow may bend the losing chest
/// (radians, about 6 degrees) [design: owner 2026-11-12, the loser goes to the side, not
/// forward].
const FOLLOW_PITCH: f32 = 0.1;

/// How far side `k` is losing at the lean `theta` (0 even or winning, 1 at the pin `pin`),
/// eased in: the clasp is going down towards that side's right.
pub fn losing(theta: f32, k: usize, pin: f32) -> f32 {
    let toward_loss = if k == 0 { -theta } else { theta };
    let x = (toward_loss / pin.max(0.1)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The turn of the upper body (world) for a wrestler facing `toward` (level) that follows
/// its losing arm by `follow` (0 to 1): rolled down towards its own right and twisted, the
/// right shoulder coming a little forward.
pub fn follow_turn(toward: Vec3, follow: f32) -> Quat {
    // About the way in, a positive turn takes up towards the wrestler's right; about up, a
    // positive one brings its right forward.
    let roll = Quat::from_axis_angle(toward, FOLLOW_ROLL * follow);
    let twist = Quat::from_axis_angle(Vec3::Y, FOLLOW_TWIST * follow);
    twist * roll
}

/// The shoulders' line as posed, degrees from level: positive with the right shoulder
/// higher than the left.
fn shoulder_tilt(root: Entity, rig: &ActorRig, parents: &Query<&ChildOf>, transforms: &Query<&mut Transform>, globals: &Query<&GlobalTransform>) -> Option<f32> {
    let node = |name: &str| rig.skel.names.iter().position(|n| n.as_deref() == Some(name)).and_then(|i| rig.nodes.get(i).copied());
    let r = world_of(node(UPPER)?, root, parents, transforms, globals)?.translation();
    let l = world_of(node("Bip01 L UpperArm")?, root, parents, transforms, globals)?.translation();
    let level = Vec2::new(r.x - l.x, r.z - l.z).length();
    Some((r.y - l.y).atan2(level).to_degrees())
}

/// Turns the upper body about the spine so the shoulder comes over its planted elbow
/// `spot` (an upper arm from it), by `w`: first the follow of a losing arm (`follow`,
/// [`follow_turn`]), then a twist about the vertical that brings the right shoulder forward
/// ([`TWIST`]), then the bend in for what those leave, so the elbow stays on its plant.
/// Returns where the head's bone ends up (world), for the logs.
#[allow(clippy::too_many_arguments)]
fn lean_in(root: Entity, rig: &ActorRig, spot: Vec3, toward: Vec3, follow: f32, w: f32, parents: &Query<&ChildOf>, transforms: &mut Query<&mut Transform>, globals: &Query<&GlobalTransform>) -> Result<Option<Vec3>, &'static str> {
    let node = |name: &str| rig.skel.names.iter().position(|n| n.as_deref() == Some(name)).and_then(|i| rig.nodes.get(i).copied());
    let spine = SPINE.iter().find_map(|n| node(n)).ok_or("no spine")?;
    let (Some(upper), Some(fore)) = (node(UPPER), node(FORE)) else { return Err("no right arm") };
    let above = parents.get(spine).map_err(|_| "the spine hangs from nothing")?.parent();
    let gp = world_of(above, root, parents, transforms, globals).ok_or("the spine is not under the body")?;
    let gs = world_of(spine, root, parents, transforms, globals).ok_or("the spine is not under the body")?;
    let sh = world_of(upper, root, parents, transforms, globals).ok_or("the arm is not under the body")?.translation();
    let el = world_of(fore, root, parents, transforms, globals).ok_or("the arm is not under the body")?.translation();
    let pivot = gs.translation();
    // Towards the planted elbow, not only straight at the table: a body seated off the
    // table's middle line rolls its shoulder in over the elbow [design: the player sat
    // with the right shoulder 16 to the right of the middle, run 2026-11-11 18:26].
    // The way in is taken from where the shoulder would be without the follow: from the
    // rolled shoulder it pointed back across under the roll, and the bend undid it, the
    // chest leaning away from its pinned arm (owner 2026-11-12, pvpwb: "roll that way ...
    // with the arm"). Bent along the unrolled way, the roll stays and the bend only keeps
    // the shoulder an upper arm from the elbow.
    let sh_0 = pivot + twist_towards(pivot, sh, spot, TWIST * w) * (sh - pivot);
    let to_spot = Vec3::new(spot.x - sh_0.x, 0.0, spot.z - sh_0.z).normalize_or(toward);
    // Losing, the bend goes straight in over the table: bent along a way in with a part
    // to the side, it rolled the shoulders back up and took the follow's tilt away (side 1
    // of pvpwb, b-win-view16: the shoulders' line went only from +4.5 to -4.3 deg, while side
    // 0 of pvpwb2 went from -5 to -32). The distance to the elbow is still solved, so the
    // elbow stays on its plant.
    let level_in = Vec3::new(toward.x, 0.0, toward.z).normalize_or(to_spot);
    let to_spot = to_spot.lerp(level_in, (follow * w).clamp(0.0, 1.0)).normalize_or(level_in);
    let axis = Vec3::Y.cross(to_spot).normalize_or(Vec3::X);
    let ua = sh.distance(el);
    // The follow, then the twist, then the bend for what those leave; and how far the
    // shoulder still misses an upper arm from the elbow.
    let solve = |f: f32| {
        let qf = follow_turn(toward, f * w);
        let sh_f = pivot + qf * (sh - pivot);
        let twist = twist_towards(pivot, sh_f, spot, TWIST * w);
        let sh_t = pivot + twist * (sh_f - pivot);
        let bend = lean_bend(pivot, sh_t, ua, spot, axis);
        let miss = ((pivot + Quat::from_axis_angle(axis, bend) * (sh_t - pivot)).distance(spot) - ua).abs();
        (qf, twist, bend, miss)
    };
    // The most of the follow that still lets the shoulder reach its elbow, and goes to the
    // side, not forward: rolled all the way, a seat whose shoulder sits low over the top
    // took it under the table's height and the elbow came 10 to 15 off its plant (side 1
    // of pvpwb, b-win-view17); on the other seat the bend in followed the roll down and the
    // loser lay over the table, his head on the top (side 0 of pvpwb2, b-win-view18).
    let plain = solve(0.0).2;
    let mut f = follow;
    let (mut qf, mut twist, mut bend, mut miss) = solve(f);
    while (miss > FOLLOW_MISS || bend > plain + FOLLOW_PITCH) && f > 0.0 {
        f = (f - 0.05).max(0.0);
        (qf, twist, bend, miss) = solve(f);
    }
    let bend = bend * w;
    // About Y x the way in, a positive turn takes up towards it.
    let q = Quat::from_axis_angle(axis, bend) * twist * qf;
    let rp = rot(&gp);
    if let Ok(mut t) = transforms.get_mut(spine) {
        t.rotation = (rp.inverse() * q * rp * t.rotation).normalize();
    }
    // The head turned back up by the bend and back round by most of the twist, looking
    // across at the other wrestler and not down at the table.
    let Some(neck) = node(NECK) else { return Ok(None) };
    let above = parents.get(neck).map_err(|_| "the neck hangs from nothing")?.parent();
    let gn = world_of(above, root, parents, transforms, globals).ok_or("the neck is not under the body")?;
    let rn = rot(&gn);
    // The follow's roll is not taken back: the head goes over with the shoulders (owner
    // 2026-11-12).
    let back = Quat::from_axis_angle(axis, -bend * HEAD_UP) * twist.inverse().slerp(Quat::IDENTITY, 1.0 - HEAD_ROUND);
    if let Ok(mut t) = transforms.get_mut(neck) {
        t.rotation = (rn.inverse() * back * rn * t.rotation).normalize();
    }
    Ok(world_of(neck, root, parents, transforms, globals).map(|g| g.translation()))
}

/// The turn about the vertical through `pivot`, by `angle` (radians), that way round which
/// brings the shoulder `sh` nearer the planted elbow `spot`.
pub fn twist_towards(pivot: Vec3, sh: Vec3, spot: Vec3, angle: f32) -> Quat {
    let level = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
    let miss = |a: f32| level(pivot + Quat::from_rotation_y(a) * (sh - pivot)).distance(level(spot));
    if miss(angle) <= miss(-angle) {
        Quat::from_rotation_y(angle)
    } else {
        Quat::from_rotation_y(-angle)
    }
}

/// The bend about `axis` through the spine's `pivot` (radians, within
/// [-[`MAX_BEND`], [`MAX_BEND`]], the smallest that does it) that puts the shoulder `sh` an
/// upper arm `ua` from the planted elbow `spot`, or as near as it comes: the shoulder turns
/// rigidly with the spine, so this is exact in one step [design: a step sized by the
/// shoulder's height over the pivot overshot by 5 once the body was bent, the shoulder
/// 12.6 from the elbow for a 17.4 upper arm, run 2026-11-11 18:55]. Negative unbends a
/// body bent too far.
pub fn lean_bend(pivot: Vec3, sh: Vec3, ua: f32, spot: Vec3, axis: Vec3) -> f32 {
    let miss = |b: f32| ((pivot + Quat::from_axis_angle(axis, b) * (sh - pivot)).distance(spot) - ua).abs();
    let steps = 150;
    (0..=2 * steps)
        .map(|i| (i as f32 - steps as f32) / steps as f32 * MAX_BEND)
        .min_by(|a, b| (miss(*a), a.abs()).partial_cmp(&(miss(*b), b.abs())).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0)
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

/// One hand's bones for the grip, and its own frame measured from the skeleton's bind
/// pose: along (wrist to the knuckles' middle), palm x along, and the palm's facing, as
/// columns, in the hand bone's local space.
struct Grip {
    hand: Entity,
    fingers: [[Option<Entity>; 3]; 4],
    /// Each finger bone's bind turn (local) and its hinge in its own local frame.
    hinges: [[Option<(Quat, Vec3)>; 3]; 4],
    thumb: [Option<Entity>; 3],
    thumb_bind: [Option<Quat>; 3],
    axes: Mat3,
    /// The thumb's side of the hand (hand space): along x palm, a right hand's.
    thumb_side: Vec3,
    palm: f32,
}

impl Grip {
    fn of(rig: &ActorRig, hand: Entity) -> Option<Grip> {
        let index = |name: &str| rig.skel.names.iter().position(|n| n.as_deref() == Some(name));
        let node = |i: usize| rig.nodes.get(i).copied();
        let bind = |i: usize| rig.skel.locals.get(i).copied();
        let hand_i = rig.nodes.iter().position(|&e| e == hand)?;
        let fi = GRIP.map(|f| f.map(index));
        let ti = THUMB.map(index);
        // The knuckles and the thumb's root, as the bind pose has them under the hand.
        let under_hand = |i: usize| if rig.skel.parents.get(i).copied().flatten() == Some(hand_i) { bind(i) } else { None };
        let knuckles: Vec<Vec3> = fi.iter().filter_map(|f| f[0].and_then(under_hand)).map(|t| t.translation).collect();
        if knuckles.len() < 2 {
            return None;
        }
        let k = knuckles.iter().sum::<Vec3>() / knuckles.len() as f32;
        let a = k.try_normalize()?;
        // The palm from where the thumb sits [observed: the thumb's root bone lies off the
        // knuckles' line to one side; for a right hand along x palm is that side, so palm =
        // thumb side x along]. The finger bones' +Y, used before, gave a frame whose thumb
        // column was the little finger's side (owner 2026-11-11: "upside down and
        // mutilated").
        let t_root = ti[0].and_then(under_hand)?.translation;
        let ts = (t_root - a * t_root.dot(a)).try_normalize()?;
        let p = ts.cross(a).normalize();
        // Each finger curls about one hinge, across the finger towards the palm, taken
        // from its first bone; in each bone's own frame, so the bone keeps its own twist.
        let hinges = fi.map(|f| {
            let mut out = [None; 3];
            let Some(r0) = f[0].and_then(bind).map(|t| t.rotation) else { return out };
            let Some(h) = (r0 * Vec3::X).cross(p).try_normalize() else { return out };
            let mut parent = Quat::IDENTITY;
            for (j, i) in f.iter().enumerate() {
                let Some(r) = i.and_then(bind).map(|t| t.rotation) else { break };
                let world = parent * r;
                out[j] = Some((r, world.inverse() * h));
                parent = world;
            }
            out
        });
        Some(Grip {
            hand,
            fingers: fi.map(|f| f.map(|i| i.and_then(node))),
            hinges,
            thumb: ti.map(|i| i.and_then(node)),
            thumb_bind: ti.map(|i| i.and_then(bind).map(|t| t.rotation)),
            axes: Mat3::from_cols(a, p.cross(a), p),
            thumb_side: ts,
            palm: k.length() * PALM_MID,
        })
    }

    /// Turn the hand to the world frame `(along, palm x along, palm)` under the forearm's
    /// world turn `fore`, and close the fingers and the thumb round the other hand, by `w`.
    fn grip(&self, fore: Quat, (along, thumb, palm): (Vec3, Vec3, Vec3), w: f32, transforms: &mut Query<&mut Transform>) {
        let Ok(lh) = transforms.get(self.hand).copied() else { return };
        let world = Mat3::from_cols(along, thumb, palm) * self.axes.transpose();
        let to = (fore.inverse() * Quat::from_mat3(&world)).normalize();
        if let Ok(mut t) = transforms.get_mut(self.hand) {
            t.rotation = lh.rotation.slerp(to, w);
        }
        // Every finger joint curled from its bind turn about its own hinge, round the back
        // of the other hand.
        for ((bones, hinges), curls) in self.fingers.iter().zip(&self.hinges).zip(GRIP_CURL) {
            for ((bone, hinge), curl) in bones.iter().zip(hinges).zip(curls) {
                let (Some(bone), Some((r, h))) = (bone, hinge) else { break };
                let target = (*r * Quat::from_axis_angle(*h, curl)).normalize();
                if let Ok(mut t) = transforms.get_mut(*bone) {
                    t.rotation = t.rotation.slerp(target, w);
                }
            }
        }
        // The thumb over the top of the other hand: each segment aimed in the hand's frame
        // from its bind turn (so it keeps its own twist).
        let (a, p, ts) = (self.axes.x_axis, self.axes.z_axis, self.thumb_side);
        let mut parent = Quat::IDENTITY;
        for ((bone, r), d) in self.thumb.iter().zip(&self.thumb_bind).zip(THUMB_DIRS) {
            let (Some(bone), Some(r)) = (bone, r) else { break };
            let dir = (a * d[0] + ts * d[1] + p * d[2]).normalize();
            let now = parent * *r;
            let aimed = Quat::from_rotation_arc((now * Vec3::X).normalize_or(Vec3::X), dir) * now;
            let local = (parent.inverse() * aimed).normalize();
            if let Ok(mut t) = transforms.get_mut(*bone) {
                t.rotation = t.rotation.slerp(local, w);
            }
            parent = aimed;
        }
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
    let hand = Grip::of(rig, hand);
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
    fn the_elbows_stay_planted_and_the_clasp_only_turns_sideways() {
        let (mid, u) = (Vec3::new(10.0, 50.0, 0.0), Vec3::X);
        let p = plant(mid, u, [18.0, 20.0], [4.0, 4.5]);
        // Planted on the table, either side of the middle, each a little to its own right.
        assert!(p.elbows.iter().all(|e| (e.y - mid.y).abs() < 1e-4));
        assert!(p.elbows[0].x < mid.x && p.elbows[1].x > mid.x);
        let right0 = u.cross(Vec3::Y);
        assert!((p.elbows[0] - mid).dot(right0) > 0.0 && (p.elbows[1] - mid).dot(right0) < 0.0);
        // A forearm from elbow to wrist and a palm from wrist to the palm's middle.
        for (k, (f, m)) in [(18.0, 4.0), (20.0, 4.5)].into_iter().enumerate() {
            assert!((p.wrists[k].distance(p.elbows[k]) - f).abs() < 1e-3);
            assert!((p.palms[k].distance(p.wrists[k]) - m).abs() < 1e-3);
            // Each palm faces the other hand, the thumb (along x palm) on top.
            let (along, palm) = p.hands[k];
            assert!(palm.dot(p.palms[1 - k] - p.palms[k]) > 0.0);
            assert!(along.cross(palm).y > 0.5, "{k}");
        }
        // The palms' middles side by side across the line between the chairs, skin on
        // skin, the clasp straight up over the middle.
        assert!((p.palms[0].distance(p.palms[1]) - 2.0 * PALM_GAP).abs() < 1e-3);
        assert!((p.palms[0] - p.palms[1]).dot(u).abs() < 1e-3);
        assert!(p.clasp.x == mid.x && p.clasp.z == mid.z && p.clasp.y > mid.y + 15.0);
        for theta in [0.3, -0.7, 1.2] {
            // The elbows do not move, elbow to palm stays one length, the palms stay
            // together, and the clasp goes sideways, not along the line between the chairs.
            for k in 0..2 {
                assert!(turned(&p, theta, p.elbows[k]).distance(p.elbows[k]) < 1e-4);
                assert!((turned(&p, theta, p.palms[k]).distance(p.elbows[k]) - p.palms[k].distance(p.elbows[k])).abs() < 1e-3);
            }
            let (a, b) = (turned(&p, theta, p.palms[0]), turned(&p, theta, p.palms[1]));
            assert!((a.distance(b) - 2.0 * PALM_GAP).abs() < 1e-3);
            // Along the line between the chairs only the little the elbows' own offsets
            // (each to its wrestler's right) skew the turning line: under a tenth of the
            // sideways travel.
            let c = turned(&p, theta, p.clasp) - p.clasp;
            assert!(c.x.abs() < 0.1 * c.z.abs(), "{c}");
        }
        // Side 0 winning (a positive lean) takes the clasp to side 0's left.
        assert!((turned(&p, 0.5, p.clasp) - p.clasp).dot(-right0) > 1.0);
    }

    #[test]
    fn the_elbow_is_solved_towards_its_plant_with_the_wrist_exact() {
        let (s, wrist, plant) = (Vec3::new(0.0, 30.0, 0.0), Vec3::new(12.0, 22.0, 0.0), Vec3::new(14.0, 5.0, 1.0));
        let e = elbow_between(s, wrist, 20.0, 18.0, plant);
        assert!((e.distance(s) - 20.0).abs() < 1e-3 && (e.distance(wrist) - 18.0).abs() < 1e-3);
        // Of the circle the elbow may be on, it takes the side towards the plant.
        assert!(e.y < 20.0, "{e}");
        // Out of reach: straight towards the wrist.
        let far = elbow_between(s, Vec3::new(50.0, 30.0, 0.0), 20.0, 18.0, plant);
        assert!(far.distance(Vec3::new(20.0, 30.0, 0.0)) < 1e-3);
    }

    #[test]
    fn the_twist_brings_the_right_shoulder_forward_over_the_elbow() {
        // A body facing +X, its right shoulder to its right (-Z), the elbow ahead on the
        // table a little to the right: either way round the twist takes the shoulder
        // nearer the elbow, never away.
        let pivot = Vec3::new(-30.0, 0.0, 0.0);
        let sh = Vec3::new(-28.0, 25.0, -9.0);
        let spot = Vec3::new(-11.0, 3.0, -0.75);
        let level = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
        let before = level(sh).distance(level(spot));
        let q = twist_towards(pivot, sh, spot, 0.45);
        let after = level(pivot + q * (sh - pivot)).distance(level(spot));
        assert!(after < before - 2.0, "{before} -> {after}");
        // Mirrored (the other side of the table), the same.
        let m = |v: Vec3| Vec3::new(-v.x, v.y, -v.z);
        let q = twist_towards(m(pivot), m(sh), m(spot), 0.45);
        let after_m = level(m(pivot) + q * (m(sh) - m(pivot))).distance(level(m(spot)));
        assert!((after_m - after).abs() < 1e-3);
    }

    #[test]
    fn the_lean_puts_the_shoulder_an_upper_arm_from_the_elbow() {
        let (pivot, axis) = (Vec3::new(0.0, 40.0, 0.0), Vec3::Z);
        // A positive turn about Z takes up towards -X: the elbow's spot that way.
        let (sh, spot) = (Vec3::new(0.0, 60.0, 0.0), Vec3::new(-20.0, 45.0, 0.0));
        for ua in [15.0, 18.0] {
            let b = lean_bend(pivot, sh, ua, spot, axis);
            let at = pivot + Quat::from_axis_angle(axis, b) * (sh - pivot);
            assert!((at.distance(spot) - ua).abs() < 0.2, "{ua} {b} {at}");
        }
        // Already too near: it unbends.
        assert!(lean_bend(pivot, Vec3::new(-14.0, 44.0, 0.0), 18.0, spot, axis) < 0.0);
    }

    #[test]
    fn the_losing_body_follows_its_arm_and_the_shoulder_still_reaches_the_elbow() {
        // Side 0 loses at a negative lean, side 1 at a positive one; even or winning, no
        // follow.
        assert_eq!(losing(0.0, 0, 1.2), 0.0);
        assert_eq!(losing(0.5, 0, 1.2), 0.0);
        assert!(losing(-0.6, 0, 1.2) > 0.3 && losing(-0.6, 0, 1.2) < 0.7);
        assert!((losing(1.2, 1, 1.2) - 1.0).abs() < 1e-6);
        // Facing -Z (right is +X): the right shoulder goes down, out to the right and a little
        // forward.
        let toward = Vec3::NEG_Z;
        let right = toward.cross(Vec3::Y);
        assert!(right.x > 0.99);
        let pivot = Vec3::new(0.0, 40.0, 0.0);
        let sh = Vec3::new(8.0, 58.0, -2.0);
        let moved = pivot + follow_turn(toward, 1.0) * (sh - pivot);
        assert!(moved.y < sh.y && (moved - sh).dot(right) > 0.0 && (moved - sh).dot(toward) > 0.0, "{moved}");
        // The bend in, solved from where the follow left the shoulder, still puts it an
        // upper arm from the planted elbow.
        // Bent along the way in from the unrolled shoulder (as lean_in does), the roll stays:
        // the shoulder ends out to the wrestler's right of where the plain bend puts it.
        let spot = Vec3::new(9.0, 45.0, -20.0);
        let axis = Vec3::Y.cross(Vec3::new(spot.x - sh.x, 0.0, spot.z - sh.z).normalize());
        let b = lean_bend(pivot, moved, 18.0, spot, axis);
        let at = pivot + Quat::from_axis_angle(axis, b) * (moved - pivot);
        assert!((at.distance(spot) - 18.0).abs() < 0.2, "{at}");
        let b0 = lean_bend(pivot, sh, 18.0, spot, axis);
        let plain = pivot + Quat::from_axis_angle(axis, b0) * (sh - pivot);
        assert!((at - plain).dot(right) > 2.0, "{at} {plain}");
        // To the side, not forward: losing, the chest pitches in no further than the plain
        // lean (owner 2026-11-12: it went forward, not to the side), and the shoulder is
        // lower than the plain lean leaves it.
        assert!(b <= b0 + 1e-3 && at.y < plain.y - 5.0, "{b} {b0} {at} {plain}");
    }

    #[test]
    fn planted_off_the_wall_both_pins_lie_on_the_top_inside_its_edges() {
        let top = 48.0;
        let line = Vec3::new(0.0, top + ELBOW_UP, 0.0);
        let p = plant(line + OPEN * PLANT_OFF, Vec3::X, [18.0, 18.0], [4.0, 4.0]);
        let (up, down) = pin_sides(&p, top, line, OPEN);
        // Off the wall, the edge no longer stops the pin towards it: both ways go down
        // until the palms reach the top.
        for (a, s) in [(up, 1.0), (down, -1.0)] {
            assert!(a > 1.3 && a <= PINNED, "{a}");
            let mut lowest = f32::MAX;
            for k in 0..2 {
                let (m, n) = palm_and_knuckles(&p, s * a, k);
                assert!(m.y >= top + HAND_CLEAR - 1e-3, "{a} {m}");
                lowest = lowest.min(m.y);
                let o = (n - line).dot(OPEN);
                assert!((-EDGE_IN..=OPEN_EDGE).contains(&o), "{a} {o}");
            }
            assert!(lowest < top + HAND_CLEAR + 1.5, "{a} {lowest}");
        }
        // The pin towards the wall leaves the top showing beyond the knuckles.
        let wall = (0..2).map(|k| -(palm_and_knuckles(&p, if OPEN.dot(p.hands[0].1) < 0.0 { up } else { -down }, k).1 - line).dot(OPEN)).fold(f32::MIN, f32::max);
        assert!(wall < EDGE_IN - 5.0, "{wall}");
    }

    #[test]
    fn the_pin_stops_with_the_hands_on_the_table_not_through_it() {
        let top = 48.0;
        let p = plant(Vec3::new(0.0, top + ELBOW_UP, 0.0), Vec3::X, [18.0, 18.0], [4.0, 4.0]);
        // Planted on the chairs' line, the wall's edge stops the pin towards it first.
        let (up, down) = pin_sides(&p, top, p.clasp - Vec3::Y * p.clasp.y + Vec3::Y * (top + ELBOW_UP), OPEN);
        let pin = up.min(down);
        assert!(pin > 1.1 && pin <= PINNED, "{pin}");
        for a in [pin, -pin] {
            for k in 0..2 {
                assert!(palm_and_knuckles(&p, a, k).0.y >= top + HAND_CLEAR - 1e-3);
            }
        }
        // Inside the table's edge, the hands down near the top, and still clasped.
        for a in [pin, -pin] {
            let (m0, n0) = palm_and_knuckles(&p, a, 0);
            let (m1, n1) = palm_and_knuckles(&p, a, 1);
            for n in [n0, n1] {
                assert!((n - p.clasp).dot(p.hands[0].1).abs() <= EDGE_IN + 1e-3, "{a} {n}");
            }
            eprintln!("pin {a:+.3}: palms {:.2} over, {:.2} apart; knuckles {:+.1} {:+.1} across", m0.y.min(m1.y) - top, m0.distance(m1), (n0 - p.clasp).dot(p.hands[0].1), (n1 - p.clasp).dot(p.hands[0].1));
            assert!(m0.y.min(m1.y) < top + HAND_CLEAR + 1.5, "{a} {m0} {m1}");
            assert!((m0.distance(m1) - 2.0 * PALM_GAP).abs() < 2.0, "{a} {}", m0.distance(m1));
            for k in 0..2 {
                assert!((hand_at(&p, a, k).2.distance(p.elbows[k]) - 18.0).abs() < 1e-3);
            }
        }
    }
}
