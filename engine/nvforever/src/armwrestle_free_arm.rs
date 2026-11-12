//! The wrestlers' free (left) arms at the arm-wrestling table (queue item 199): while the
//! right hands are clasped (`armwrestle_arm`), the left hand lies flat on the table on the
//! wrestler's own side, a little to the left of the right elbow, fingers towards the
//! opponent, the elbow out to the left [design: the free hand braced on the table, as
//! arm wrestlers brace; the chair's sitting pose leaves it on the lap]. Posed over the
//! seated pose by the same weight as the right arm, after the upper body has leaned in.

use crate::darts_arm::{rot, solve, world_of};
use crate::limbs::ActorRig;
use bevy::prelude::*;

/// An arm's bones: upper arm, forearm, hand, the fingers (index to little, knuckle to tip;
/// the first bones measure the hand's length and frame) and the thumb (base to tip)
/// [observed: the human skeleton's node names].
struct ArmNames {
    upper: &'static str,
    fore: &'static str,
    hand: &'static str,
    fingers: [[&'static str; 3]; 4],
    thumb: [&'static str; 3],
}
const LEFT: ArmNames = ArmNames {
    upper: "Bip01 L UpperArm",
    fore: "Bip01 L Forearm",
    hand: "Bip01 L Hand",
    fingers: [
        ["Bip01 L Finger1", "Bip01 L Finger11", "Bip01 L Finger12"],
        ["Bip01 L Finger2", "Bip01 L Finger21", "Bip01 L Finger22"],
        ["Bip01 L Finger3", "Bip01 L Finger31", "Bip01 L Finger32"],
        ["Bip01 L Finger4", "Bip01 L Finger41", "Bip01 L Finger42"],
    ],
    thumb: ["Bip01 L Thumb1", "Bip01 L Thumb11", "Bip01 L Thumb12"],
};
const RIGHT: ArmNames = ArmNames {
    upper: "Bip01 R UpperArm",
    fore: "Bip01 R Forearm",
    hand: "Bip01 R Hand",
    fingers: [
        ["Bip01 R Finger1", "Bip01 R Finger11", "Bip01 R Finger12"],
        ["Bip01 R Finger2", "Bip01 R Finger21", "Bip01 R Finger22"],
        ["Bip01 R Finger3", "Bip01 R Finger31", "Bip01 R Finger32"],
        ["Bip01 R Finger4", "Bip01 R Finger41", "Bip01 R Finger42"],
    ],
    thumb: ["Bip01 R Thumb1", "Bip01 R Thumb11", "Bip01 R Thumb12"],
};
/// Each finger joint's bend towards the palm, radians [design: a hand at rest on a table,
/// the fingers nearly straight, the tips just touching].
const REST_CURL: [f32; 3] = [0.12, 0.18, 0.12];
/// Where each thumb segment points flat on the table, in the hand's frame (along, thumb
/// side, palm) [design: out to the side and forward, lying down].
const THUMB_DIRS: [[f32; 3]; 3] = [[0.5, 0.75, 0.45], [0.75, 0.6, 0.25], [0.9, 0.4, 0.15]];
/// Where the palm's middle lies on the table, from the table's middle: back towards the
/// wrestler and out to the left, game units [design: beside the right elbow, which is
/// about 8 back from the middle, `armwrestle_arm::ELBOW_IN`].
const BACK: f32 = 9.0;
const OUT: f32 = 14.0;
/// The palm's skin under the hand bone's line (wrist to knuckles), game units: the hand
/// bone lies this far over the table top for the palm to lie on it [observed: the male
/// left hand's skin (meshes\characters\_male\lefthand.nif, the vertices weighted to
/// Bip01 L Hand, in its bind space): the palm side reaches 2.8 to 3.2 from the line
/// between the wrist and the knuckles; 3.0 lets the palm's middle just touch].
const PALM_DEPTH: f32 = 3.0;
/// The fingers' turn inwards from straight towards the opponent, radians [design: about
/// 25 degrees, the hand angled in as it lies].
const TURN_IN: f32 = 0.45;
/// The palm's middle from the wrist, as a share of the knuckles' distance [design].
const PALM_MID: f32 = 0.6;

/// Lays wrestler `root`'s left hand flat on the table: `top` over the middle of the table's
/// top (Bevy; its height is not used, the palm goes onto `TABLE_TOP_Z`), `toward` the level
/// direction to the opponent, by `w`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn brace(root: Entity, rig: &ActorRig, top: Vec3, toward: Vec3, w: f32, parents: &Query<&ChildOf>, transforms: &mut Query<&mut Transform>, globals: &Query<&GlobalTransform>) -> Result<(), &'static str> {
    if w <= 0.0 {
        return Ok(());
    }
    let left = Vec3::Y.cross(toward).normalize_or(Vec3::X);
    let along_w = (toward * TURN_IN.cos() - left * TURN_IN.sin()).normalize();
    let palm_mid = Vec3::new(top.x, fnv_game::armwrestle_table::TABLE_TOP_Z + PALM_DEPTH, top.z) - toward * BACK + left * OUT;
    // The elbow out to the left and a little back and down, as a braced arm bends.
    let pole = left - toward * 0.4 - Vec3::Y * 0.3;
    let r = lay_hand(root, rig, &LEFT, palm_mid, along_w, pole, |_| w, parents, transforms, globals)?;
    // Now and then: how near the wrist comes to its spot (an arm too short leaves the hand
    // over the table).
    if LOGGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed).is_multiple_of(240) {
        info!("arm wrestle free hand: {root:?} weight {w:.2} shoulder {:.0?} wrist to {:.1?} reached {:.1?} (short {:.1}), palm side {}", r.shoulder, r.wrist, r.reached, r.reached.distance(r.wrist), if r.measured { "measured" } else { "not found" });
    }
    Ok(())
}

/// How [`lay_hand`] came out.
struct Laid {
    shoulder: Vec3,
    wrist: Vec3,
    reached: Vec3,
    measured: bool,
    /// The weight it was laid by.
    w: f32,
    /// The shoulder's distance to the wrist's spot, and the arm's length.
    span: (f32, f32),
}

/// Lays the hand of arm `names` flat, palm down, its palm's middle at `palm_mid` (Bevy) and
/// its fingers along `along_w` (level), the elbow bent towards `pole`; by the weight
/// `w(reach)` gives for how far the arm reaches past the wrist's spot (the arm's length
/// less the shoulder's distance to it; below zero it cannot).
#[allow(clippy::too_many_arguments)]
fn lay_hand(
    root: Entity,
    rig: &ActorRig,
    names: &ArmNames,
    palm_mid: Vec3,
    along_w: Vec3,
    pole: Vec3,
    w: impl Fn(f32) -> f32,
    parents: &Query<&ChildOf>,
    transforms: &mut Query<&mut Transform>,
    globals: &Query<&GlobalTransform>,
) -> Result<Laid, &'static str> {
    let node = |name: &str| rig.skel.names.iter().position(|n| n.as_deref() == Some(name)).and_then(|i| rig.nodes.get(i).copied());
    let (Some(upper), Some(fore), Some(hand)) = (node(names.upper), node(names.fore), node(names.hand)) else { return Err("no arm") };
    let above = parents.get(upper).map_err(|_| "the arm hangs from nothing")?.parent();
    let gp = world_of(above, root, parents, transforms, globals).ok_or("the arm is not under the body")?;
    let local = |e: Entity| transforms.get(e).copied().map_err(|_| "a bone has no transform");
    let (lu, lf, lh) = (local(upper)?, local(fore)?, local(hand)?);
    let gu = gp.mul_transform(lu);
    let gf = gu.mul_transform(lf);
    let gh = gf.mul_transform(lh);
    let (s, e0, h0) = (gu.translation(), gf.translation(), gh.translation());
    if s.distance(e0) < 1e-3 || e0.distance(h0) < 1e-3 {
        return Err("the arm has no length");
    }
    // The hand's own frame, in its local space: along (wrist to the knuckles' middle), and
    // the palm's normal: of the two normals to along and the thumb, the side the fingers
    // curl to [observed: the bind pose's middle finger tip lies 1.1 to the palm side of the
    // knuckles' line, lefthand.nif]; with straight fingers, the side the seated pose turns
    // down (the hand rests palm down on the lap [observed: the chair's sitting idle]).
    let at = |e: Entity| -> Option<Vec3> {
        (parents.get(e).ok()?.parent() == hand).then_some(())?;
        Some(transforms.get(e).ok()?.translation)
    };
    let knuckles: Vec<Vec3> = names.fingers.iter().filter_map(|f| node(f[0]).and_then(at)).collect();
    let frame = if knuckles.len() >= 2 {
        let k = knuckles.iter().sum::<Vec3>() / knuckles.len() as f32;
        node(names.thumb[0]).and_then(at).and_then(|t| {
            let a = k.try_normalize()?;
            let t = (t - a * t.dot(a)).try_normalize()?;
            let n = t.cross(a).normalize();
            let tip = curl_tip(&names.fingers[1].map(node), transforms);
            let p = match tip.map(|v| v.dot(n)) {
                Some(c) if c.abs() > 0.2 => n * c.signum(),
                _ if (rot(&gh) * n).y < 0.0 => n,
                _ => -n,
            };
            Some((a, p, k.length() * PALM_MID, t))
        })
    } else {
        None
    };
    let reach = frame.map_or(0.0, |f| f.2);
    let wrist = palm_mid - along_w * reach;
    let length = s.distance(e0) + e0.distance(h0);
    let span = s.distance(wrist);
    let w = w(length - span).clamp(0.0, 1.0);
    let laid = |reached: Vec3| Laid { shoulder: s, wrist, reached, measured: frame.is_some(), w, span: (span, length) };
    if w <= 0.0 {
        return Ok(laid(h0));
    }
    let (e, h) = solve(s, e0, h0, wrist, pole);
    // The upper arm and the forearm turned to reach there (as `armwrestle_arm::Arm::apply`).
    let rp = rot(&gp);
    let ru = rp * lu.rotation;
    let du = Quat::from_rotation_arc((e0 - s).normalize(), (e - s).normalize_or(Vec3::NEG_Y));
    let ru2 = du * ru;
    let lu2 = (rp.inverse() * ru2).normalize();
    let cf = du * (h0 - e0);
    let df = Quat::from_rotation_arc(cf.normalize(), (h - e).normalize_or(Vec3::Y));
    let lf2 = (ru2.inverse() * df * ru2 * lf.rotation).normalize();
    let (u2, f2) = (lu.rotation.slerp(lu2, w), lf.rotation.slerp(lf2, w));
    if let Ok(mut t) = transforms.get_mut(upper) {
        t.rotation = u2;
    }
    if let Ok(mut t) = transforms.get_mut(fore) {
        t.rotation = f2;
    }
    // The hand flat, palm down, fingers along `along_w`.
    if let Some((a, p, _, thumb_side)) = frame {
        let t = p.cross(a).normalize();
        let palm_w = Vec3::NEG_Y;
        let thumb_w = palm_w.cross(along_w).normalize();
        let world = Mat3::from_cols(along_w, thumb_w, palm_w) * Mat3::from_cols(a, t, p).transpose();
        let fore_w = rp * u2 * f2;
        let to = (fore_w.inverse() * Quat::from_mat3(&world)).normalize();
        if let Ok(mut tr) = transforms.get_mut(hand) {
            tr.rotation = lh.rotation.slerp(to, w);
        }
        // The fingers laid out nearly straight along the hand, keeping their own spread,
        // and the thumb out to its own side: turning about along x palm bends towards the
        // palm.
        let bend = a.cross(p).normalize();
        for f in names.fingers {
            let bones = f.map(node);
            let Some(first) = bones[0] else { continue };
            let Ok(l0) = transforms.get(first).copied() else { continue };
            let spread = l0.rotation * Vec3::X;
            let straight = (spread - p * spread.dot(p)).normalize_or(a);
            let dirs = REST_CURL.iter().scan(0.0, |sum, c| {
                *sum += c;
                Some(Quat::from_axis_angle(bend, *sum) * straight)
            });
            lay(&bones, dirs, w, transforms);
        }
        let dirs = THUMB_DIRS.iter().map(|d| (a * d[0] + thumb_side * d[1] + p * d[2]).normalize());
        lay(&names.thumb.map(node), dirs, w, transforms);
    }
    Ok(laid(h))
}

/// Where a resting hand's palm lies, from the table's middle: back towards its wrestler,
/// and out to its own side, game units [design: forearms laid on the top, the hands in
/// front of the chest and turned in towards each other, about 8 inside the near edge (the
/// edge 20.9 from the middle, `armwrestle_table`); the elbows off the edge either side].
const REST_BACK: f32 = 13.0;
const REST_OUT: f32 = 5.0;
/// A resting hand's fingers turned in from pointing at the opponent, radians [design:
/// about 50 degrees, the forearms angled in].
const REST_TURN: f32 = 0.9;
/// How long the resting pose takes to come on once a body sits (after its sitting-down
/// clip), and to go if it is not getting up, seconds [design].
const REST_EASE: f32 = 0.5;
/// How much the arm may lack of reaching its spot before the resting hand lets go, and
/// over how much more it goes, game units [design: getting up, the hands stay on the top,
/// pushing off, until the shoulder has risen away from them, then lift with the body].
const REST_SLACK: f32 = 1.0;
const REST_FADE: f32 = 5.0;

/// Where a body on one of the table's seats is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    /// Sitting down (its getting-on clip playing).
    Entering,
    Seated,
    /// Getting up (its getting-off clip playing).
    Leaving,
}

/// What [`rest`] keeps from frame to frame, per seat.
#[derive(Default)]
pub struct Rest {
    w: [f32; 2],
    /// The player was seated on that seat (so "on, not seated" is getting up).
    player_seated: [bool; 2],
    phase: [Option<Phase>; 2],
}

/// The table, its seats and the player's place (the resources [`rest`] reads).
type Seats<'w> = (Option<Res<'w, nvf_play::armwrestle::ArmTable>>, Option<Res<'w, nvf_play::sim::SimState>>, Option<Res<'w, nvf_play::furniture::PlayerSeat>>, Option<Res<'w, crate::player_body::PlayerBody>>);

/// The bodies that may sit on a seat: the rig, its furniture, its place, and whether it is
/// another player's puppet.
type SeatBodies = (Entity, &'static ActorRig, Option<&'static crate::furniture::OnFurniture>, &'static GlobalTransform, Has<crate::coop::Puppet>);

/// Each frame, before the match poses the arms (`armwrestle_arm::arms`, which blends from
/// this): a body on one of the arm-wrestling table's seats rests both forearms on the top.
/// The chairs stand 35 from the table's middle for the wrestlers' arms to meet, where the
/// game stands them 57 to 60 from such a table [observed, `armwrestle_table::SEAT_OFFSET`];
/// that close the chair's own sitting and getting-up clips, made for a table that far, put
/// the forearms through the top (hands on the lap under it) [observed: the night pass's run
/// of 2026-11-11 23:54, the player's Pip-Boy through the top waiting on the card and getting
/// up]. Getting up, the hands stay on the top while the arms reach it and let go as the
/// body rises away.
#[allow(clippy::too_many_arguments)]
pub fn rest(
    time: Res<Time>,
    (table, sim, seat, body): Seats,
    rigs: Query<SeatBodies>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
    mut st: Local<Rest>,
) {
    let dt = time.delta_secs();
    let (Some(table), Some(sim)) = (table, sim) else { return };
    let at = |r: Option<fnv_sim::FormId>| r.and_then(|r| sim.world.peek(r)).map(|s| nvf_play::space::to_bevy(s.pos));
    let (Some(mid), Some(p0), Some(p1)) = (at(table.table), at(table.stools[0]), at(table.stools[1])) else {
        *st = Rest::default();
        return;
    };
    let player = seat.as_deref().filter(|s| s.hold.is_some()).and_then(|s| s.on.map(|on| (on, s.seated)));
    let top = fnv_game::armwrestle_table::TABLE_TOP_Z;
    for k in 0..2 {
        let Some(stool) = table.stools[k] else { continue };
        let (seat_at, other) = if k == 0 { (p0, p1) } else { (p1, p0) };
        let mut toward = other - seat_at;
        toward.y = 0.0;
        let Some(toward) = toward.try_normalize() else { continue };
        // Who is on it, and where in sitting.
        let found: Option<((Entity, ActorRig), Phase)> = if let Some((_, seated)) = player.filter(|(on, _)| *on == stool.0) {
            let was = std::mem::replace(&mut st.player_seated[k], seated);
            let phase = if seated {
                Phase::Seated
            } else if was || st.phase[k] == Some(Phase::Leaving) {
                Phase::Leaving
            } else {
                Phase::Entering
            };
            body.as_deref().and_then(|b| b.third_rig()).map(|r| (r, phase))
        } else {
            st.player_seated[k] = false;
            rigs.iter()
                .find_map(|(e, r, on, _, _)| {
                    let on = on.filter(|o| o.furniture() == stool.0 && o.seated())?;
                    let phase = if on.getting_off() {
                        Phase::Leaving
                    } else if on.on() {
                        Phase::Seated
                    } else {
                        Phase::Entering
                    };
                    Some(((e, r.clone()), phase))
                })
                // Another player's puppet on it (co-op): seated while near enough, the
                // reach letting go as it gets up.
                .or_else(|| {
                    rigs.iter()
                        .filter(|(_, _, _, _, puppet)| *puppet)
                        .map(|(e, r, _, g, _)| (e, r, Vec2::new(g.translation().x - seat_at.x, g.translation().z - seat_at.z).length()))
                        .filter(|(_, _, d)| *d < 40.0)
                        .min_by(|a, b| a.2.total_cmp(&b.2))
                        .map(|(e, r, _)| ((e, r.clone()), Phase::Seated))
                })
        };
        let Some(((root, rig), phase)) = found else {
            st.w[k] = 0.0;
            st.phase[k] = None;
            continue;
        };
        if st.phase[k] != Some(phase) {
            info!("arm wrestle rest: seat {k} {phase:?}, weight {:.2}", st.w[k]);
            st.phase[k] = Some(phase);
        }
        // Coming on once seated; held while getting up (the reach lets go); going while
        // sitting down.
        st.w[k] = match phase {
            Phase::Seated => (st.w[k] + dt / REST_EASE).min(1.0),
            Phase::Leaving => st.w[k],
            Phase::Entering => (st.w[k] - dt / REST_EASE).max(0.0),
        };
        let x = st.w[k];
        let w = x * x * (3.0 - 2.0 * x);
        if w <= 0.0 {
            continue;
        }
        let left = Vec3::Y.cross(toward).normalize_or(Vec3::X);
        for (names, side) in [(&LEFT, 1.0), (&RIGHT, -1.0)] {
            let out = left * side;
            let palm_mid = Vec3::new(mid.x, top + PALM_DEPTH, mid.z) - toward * REST_BACK + out * REST_OUT;
            let along_w = (toward * REST_TURN.cos() - out * REST_TURN.sin()).normalize();
            let pole = out - toward * 0.2 - Vec3::Y * 0.6;
            let by_reach = |r: f32| {
                let f = ((r + REST_SLACK) / REST_FADE).clamp(0.0, 1.0);
                w * f * f * (3.0 - 2.0 * f)
            };
            match lay_hand(root, &rig, names, palm_mid, along_w, pole, by_reach, &parents, &mut transforms, &globals) {
                Ok(r) => {
                    if REST_LOGGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed).is_multiple_of(120) {
                        info!("arm wrestle rest: seat {k} {} {phase:?} weight {:.2} span {:.1} of {:.1}, wrist off {:.1}", names.hand, r.w, r.span.0, r.span.1, r.reached.distance(r.wrist));
                    }
                }
                Err(why) => {
                    if REST_LOGGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed).is_multiple_of(120) {
                        warn!("arm wrestle rest: seat {k} {}: {why}", names.hand);
                    }
                }
            }
        }
    }
}

/// Calls of [`rest`]'s hands, for its occasional log line.
static REST_LOGGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Calls to [`brace`], for its occasional log line.
static LOGGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A finger's tip in the hand's space: the chain's three bones and the last one's length
/// taken as the middle one's (each bone's child is offset along its +X).
fn curl_tip(bones: &[Option<Entity>; 3], transforms: &Query<&mut Transform>) -> Option<Vec3> {
    let l: Vec<Transform> = bones.iter().map(|b| b.and_then(|b| transforms.get(b).ok().copied())).collect::<Option<_>>()?;
    let last = l[2].translation.length();
    Some(l[0].transform_point(l[1].transform_point(l[2].transform_point(Vec3::X * last))))
}

/// Point each bone of a chain hung from the hand (each running along its own +X [observed:
/// the finger bones' children are offset only in X, as in `darts_arm`]) along its
/// direction in the hand's space, by `w`.
fn lay(bones: &[Option<Entity>; 3], dirs: impl Iterator<Item = Vec3>, w: f32, transforms: &mut Query<&mut Transform>) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_free_hand_lies_on_the_wrestlers_own_left() {
        // Facing -Z (Bevy forward), the left is -X.
        let toward = Vec3::NEG_Z;
        let left = Vec3::Y.cross(toward);
        assert!((left - Vec3::NEG_X).length() < 1e-6);
        // The fingers point mostly at the opponent, turned in (to the right).
        let along = (toward * TURN_IN.cos() - left * TURN_IN.sin()).normalize();
        assert!(along.dot(toward) > 0.85 && along.x > 0.0);
    }
}
