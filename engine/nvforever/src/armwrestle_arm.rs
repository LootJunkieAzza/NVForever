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
//!   on the line between the chairs a little to its own right, and does not move again
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
//!   top ([`pin_angle`]), not through it, and stay through the celebration; the pose is
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
/// The forearms' lean towards each other at the start, radians (about 14 degrees)
/// [design: the hands meet over the middle of the two elbows, the forearms nearly
/// upright].
const TILT: f32 = 0.25;
/// How far each elbow sits to its own wrestler's right of the line between the chairs,
/// game units [design: the turn is about the line through both elbows, so the offsets skew
/// it off the line between the chairs and the clasp drifts that way as it goes down; 1.5
/// skewed it about 14 degrees with the elbows 12.4 apart (a 5 unit drift at the pin), 0.75
/// about 7].
const ELBOW_SIDE: f32 = 0.75;
/// How far above the table top the palms' middles stay at the pin, game units [design:
/// with 2.2 the loser's finger bones, wrapped round the other hand, went 1.7 under the
/// top (run 2026-11-11 18:01, owner: the arm "clipped through the table"); the hand's
/// lowest bone is about 4.3 under its palm's middle, so 6.5 leaves it about 2 over the
/// top, the skin on it].
const HAND_CLEAR: f32 = 6.5;
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
/// elbow joints' height), the axis `u` from side 0's chair to side 1's (level, unit) and
/// each side's reach from elbow to the middle of its palm. Side 0's right is `u x Y`.
/// Each palm's middle sits [`PALM_GAP`] its own side of the clasp, the forearms lean
/// [`TILT`] towards each other, and the clasp is as high as the shorter reach allows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plant {
    pub elbows: [Vec3; 2],
    pub clasp: Vec3,
    /// Each hand's palm middle at the start.
    pub palms: [Vec3; 2],
}

pub fn plant(mid: Vec3, u: Vec3, reach: [f32; 2]) -> Plant {
    let right = u.cross(Vec3::Y).normalize_or(Vec3::X);
    let o = ELBOW_SIDE;
    let high = |l: f32| (l * l - (l * TILT.sin()).powi(2) - o * o).max(0.0).sqrt();
    let hc = high(reach[0]).min(high(reach[1]));
    let out = |l: f32| PALM_GAP + (l * l - hc * hc - o * o).max(0.0).sqrt();
    let elbows = [mid - u * out(reach[0]) + right * o, mid + u * out(reach[1]) - right * o];
    let clasp = mid + Vec3::Y * hc;
    Plant { elbows, clasp, palms: [clasp - u * PALM_GAP, clasp + u * PALM_GAP] }
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

/// How far the clasp may turn before a hand reaches the table top `top`: the largest angle
/// (radians, at most [`PINNED`]) at which both palms' middles stay [`HAND_CLEAR`] over it.
pub fn pin_angle(p: &Plant, top: f32) -> f32 {
    let mut last = 0.0;
    for i in 0..=880 {
        let a = i as f32 * PINNED / 880.0;
        if p.palms.iter().any(|&m| turned(p, a, m).y.min(turned(p, -a, m).y) < top + HAND_CLEAR) {
            break;
        }
        last = a;
    }
    last
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
    // Each body's reach first (the bones' lengths do not change with the lean), then the
    // elbows planted for both.
    let reach: Vec<(usize, f32)> = bodies
        .iter()
        .enumerate()
        .filter_map(|(k, b)| {
            let (root, rig) = b.as_ref()?;
            let a = bones(*root, rig, &parents, &transforms, &globals).ok()?;
            Some((k, a.e0.distance(a.h0) + a.hand.as_ref().map_or(0.0, |g| g.palm)))
        })
        .collect();
    if reach.is_empty() {
        return;
    }
    let of = |k: usize| reach.iter().find(|(j, _)| *j == k).or(reach.first()).map_or(20.0, |r| r.1);
    let top = fnv_game::armwrestle_table::TABLE_TOP_Z;
    let centre = Vec3::new(mid.x, top + ELBOW_UP, mid.z);
    let pl = plant(centre, u, [of(0), of(1)]);
    let pin = pin_angle(&pl, top);
    let theta = lean.clamp(-pin, pin);
    let (q, _) = swing(&pl, theta);
    let clasp_now = turned(&pl, theta, pl.clasp);
    let log = (shown.clock * 2.0).fract() < dt * 2.0;
    if log {
        info!("arm wrestle arm: angle {theta:+.2} (pin {pin:.2}) elbows {:.1?} {:.1?} clasp {clasp_now:.1?} lowest palm {:.1} over the top", pl.elbows[0], pl.elbows[1], pl.palms.iter().map(|&m| turned(&pl, theta, m).y).fold(f32::MAX, f32::min) - top);
    }
    let mut drawn: [Option<Vec3>; 2] = [None, None];
    for (k, b) in bodies.iter().enumerate() {
        let Some((root, rig)) = b else { continue };
        let toward = if k == 0 { u } else { -u };
        // Leaning in over the table first, the shoulder over its planted elbow: the arm
        // hangs from the leaned chest.
        if let Err(why) = lean_in(*root, rig, pl.elbows[k], toward, w, &parents, &mut transforms, &globals) {
            if !std::mem::replace(&mut shown.noted, true) {
                warn!("arm wrestle arm: side {k} does not lean in: {why}");
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
        let pm = a.hand.as_ref().map_or(0.0, |g| g.palm);
        // The start pose: the forearm from the planted elbow straight to this palm's
        // middle, the wrist straight, the palm facing the other wrestler; then the whole
        // of it turned by the one angle about the elbows' line.
        let e_plant = pl.elbows[k];
        let along0 = (pl.palms[k] - e_plant).normalize_or(Vec3::Y);
        let palm0 = (toward - along0 * toward.dot(along0)).normalize_or(toward);
        let (along, palm) = (q * along0, q * palm0);
        let thumb = palm.cross(along).normalize();
        let wrist = turned(&pl, theta, pl.palms[k]) - along * pm;
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
                    info!("arm wrestle arm: check side {k}: elbow drawn {el:.1?} ({:.1} from the plant), palm middle {palm_mid:.1?}, lowest hand bone {:.1} over the top", el.distance(e_plant), low - top);
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
/// The neck: turned back up by this share of the lean [observed: the node name; design:
/// a 0.7 lean without it put the head down at the table, run 2026-11-11 15:3x].
const NECK: &str = "Bip01 Neck";
const HEAD_UP: f32 = 0.8;
/// How far (level) behind the elbow the shoulder may stay, as a share of what the upper
/// arm allows [design: the shoulder over the elbow, the upper arm sloping a little].
const SHOULDER_IN: f32 = 1.0;
/// The elbow joint's height over the table top, game units (the arm's thickness) [design].
const ELBOW_UP: f32 = 2.0;
/// Half the gap between the two hands' bones at the palms, game units [design: the palms
/// touching; 1.1 left them apart, run 16:48].
const PALM_GAP: f32 = 0.7;
/// The palm's middle from the wrist, as a share of the knuckles' distance [design].
const PALM_MID: f32 = 0.6;
/// The fingers' bones, index to little, knuckle to tip [observed: the human skeleton's
/// node names]; each joint's curl towards the palm about the knuckles' line, radians
/// [design: rounded as round a hand about 3 through; all three curled hard made two fists
/// knuckle to knuckle (run 16:04), the first joints straight and the rest hard made claws
/// (run 16:48)].
const GRIP: [[&str; 3]; 4] = [
    ["Bip01 R Finger1", "Bip01 R Finger11", "Bip01 R Finger12"],
    ["Bip01 R Finger2", "Bip01 R Finger21", "Bip01 R Finger22"],
    ["Bip01 R Finger3", "Bip01 R Finger31", "Bip01 R Finger32"],
    ["Bip01 R Finger4", "Bip01 R Finger41", "Bip01 R Finger42"],
];
const GRIP_CURL: [[f32; 3]; 4] = [[0.7, 0.95, 0.6], [0.75, 1.0, 0.6], [0.8, 1.05, 0.65], [0.85, 1.05, 0.65]];
/// The thumb's bones, base to tip [observed: node names], and where each segment points
/// in the grip, in the hand's frame (along, thumb side, palm) [design: up and forward over
/// the other hand, the tip curled down round the other thumb].
const THUMB: [&str; 3] = ["Bip01 R Thumb1", "Bip01 R Thumb11", "Bip01 R Thumb12"];
const THUMB_DIRS: [[f32; 3]; 3] = [[0.55, 0.45, 0.7], [0.75, -0.1, 0.65], [0.3, -0.45, 0.85]];

/// Bends the upper body in about the spine so the shoulder comes over its planted elbow
/// `spot` (an upper arm from it), by `w`.
#[allow(clippy::too_many_arguments)]
fn lean_in(root: Entity, rig: &ActorRig, spot: Vec3, toward: Vec3, w: f32, parents: &Query<&ChildOf>, transforms: &mut Query<&mut Transform>, globals: &Query<&GlobalTransform>) -> Result<(), &'static str> {
    // Bending lowers the shoulder as it brings it forward, so at full weight the bend is
    // found again from where the last one put it, a few times [design: one pass left the
    // shoulder far enough off that the elbow lifted 2.8 off the table at the pin, run
    // 2026-11-11 18:14]; fading in or out, one pass, so the weight still scales it.
    let mut used = 0.0;
    for _ in 0..if w >= 0.99 { 4 } else { 1 } {
        used += lean_step(root, rig, spot, toward, w, MAX_BEND - used, parents, transforms, globals)?;
        if used >= MAX_BEND - 1e-3 {
            break;
        }
    }
    Ok(())
}

/// One bend of [`lean_in`], at most `budget` radians; the bend made back.
#[allow(clippy::too_many_arguments)]
fn lean_step(root: Entity, rig: &ActorRig, spot: Vec3, toward: Vec3, w: f32, budget: f32, parents: &Query<&ChildOf>, transforms: &mut Query<&mut Transform>, globals: &Query<&GlobalTransform>) -> Result<f32, &'static str> {
    let node = |name: &str| rig.skel.names.iter().position(|n| n.as_deref() == Some(name)).and_then(|i| rig.nodes.get(i).copied());
    let spine = SPINE.iter().find_map(|n| node(n)).ok_or("no spine")?;
    let (Some(upper), Some(fore)) = (node(UPPER), node(FORE)) else { return Err("no right arm") };
    let above = parents.get(spine).map_err(|_| "the spine hangs from nothing")?.parent();
    let gp = world_of(above, root, parents, transforms, globals).ok_or("the spine is not under the body")?;
    let gs = world_of(spine, root, parents, transforms, globals).ok_or("the spine is not under the body")?;
    let sh = world_of(upper, root, parents, transforms, globals).ok_or("the arm is not under the body")?.translation();
    let el = world_of(fore, root, parents, transforms, globals).ok_or("the arm is not under the body")?.translation();
    let upper_len = sh.distance(el);
    let pivot = gs.translation();
    let height = (sh.y - pivot.y).max(1.0);
    let flat = |v: Vec3| Vec2::new(v.x, v.z);
    // Level from the elbow's spot, where the upper arm reaches down to it.
    let over = (upper_len * upper_len - (sh.y - spot.y).powi(2)).max(0.0).sqrt() * SHOULDER_IN;
    let gap = flat(spot).distance(flat(sh)) - over;
    if gap <= 0.0 || budget <= 0.0 {
        return Ok(0.0);
    }
    let bend = (gap / height).clamp(0.0, 1.0).asin().min(budget) * w;
    // Towards the planted elbow, not only straight at the table: a body seated off the
    // table's middle line rolls its shoulder in over the elbow [design: the player sat
    // with the right shoulder 16 to the right of the middle and a straight-ahead bend left
    // it 20.5 from the elbow for a 17.4 upper arm, run 2026-11-11 18:26]. Turning about the
    // level axis across that way.
    let to_spot = Vec3::new(spot.x - sh.x, 0.0, spot.z - sh.z).normalize_or(toward);
    let axis = Vec3::Y.cross(to_spot).normalize_or(Vec3::X);
    // About Y × toward a positive turn takes up towards `toward`.
    let q = Quat::from_axis_angle(axis, bend);
    let rp = rot(&gp);
    if let Ok(mut t) = transforms.get_mut(spine) {
        t.rotation = (rp.inverse() * q * rp * t.rotation).normalize();
    }
    // The head turned back up by most of it, looking at the other wrestler and not down at
    // the table.
    if let Some(neck) = node(NECK) {
        let above = parents.get(neck).map_err(|_| "the neck hangs from nothing")?.parent();
        let gn = world_of(above, root, parents, transforms, globals).ok_or("the neck is not under the body")?;
        let rn = rot(&gn);
        let back = Quat::from_axis_angle(axis, -bend * HEAD_UP);
        if let Ok(mut t) = transforms.get_mut(neck) {
            t.rotation = (rn.inverse() * back * rn * t.rotation).normalize();
        }
    }
    Ok(bend)
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
/// the knuckles' middle), thumb side (palm x along, as the grip frame) and palm, as
/// columns, in the hand bone's local space.
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
        // The palm's side, against the finger bones' own +Y [observed: with the palm taken
        // as the knuckle bones' +Y (or as thumb x along from Thumb1's place) the posed palms
        // faced away from each other, the fingers hooked back towards their own wrestler
        // (owner and armwrestle-a, desktop runs 2026-11-11 17:09 and 17:21)].
        let ys: Vec3 = fingers.iter().filter_map(|f| f[0].and_then(|e| transforms.get(e).ok())).map(|t| t.rotation * Vec3::Y).sum();
        let p = -(ys - a * ys.dot(a)).try_normalize()?;
        let t = p.cross(a);
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
                // About the thumb axis (palm x along) a negative turn takes along towards palm.
                Some(Quat::from_axis_angle(t, -*sum) * straight)
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
    fn the_elbows_stay_planted_and_the_clasp_only_turns_sideways() {
        let (mid, u) = (Vec3::new(10.0, 50.0, 0.0), Vec3::X);
        let p = plant(mid, u, [22.0, 24.0]);
        // Planted on the table, either side of the middle, each a little to its own right.
        assert!(p.elbows.iter().all(|e| (e.y - mid.y).abs() < 1e-4));
        assert!(p.elbows[0].x < mid.x && p.elbows[1].x > mid.x);
        let right0 = u.cross(Vec3::Y);
        assert!((p.elbows[0] - mid).dot(right0) > 0.0 && (p.elbows[1] - mid).dot(right0) < 0.0);
        // Each palm's middle a reach from its elbow, the clasp straight up over the middle.
        assert!((p.palms[0].distance(p.elbows[0]) - 22.0).abs() < 1e-3);
        assert!((p.palms[1].distance(p.elbows[1]) - 24.0).abs() < 1e-3);
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
    fn the_pin_stops_with_the_hands_on_the_table_not_through_it() {
        let top = 48.0;
        let p = plant(Vec3::new(0.0, top + ELBOW_UP, 0.0), Vec3::X, [22.0, 22.0]);
        let pin = pin_angle(&p, top);
        assert!(pin > 1.2 && pin <= PINNED, "{pin}");
        for a in [pin, -pin] {
            for m in p.palms {
                assert!(turned(&p, a, m).y >= top + HAND_CLEAR - 1e-3);
            }
        }
        // A little further would go through.
        let over = pin + 0.05;
        assert!(p.palms.iter().any(|&m| turned(&p, over, m).y < top + HAND_CLEAR) || over > PINNED);
    }
}
