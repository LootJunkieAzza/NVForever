//! The throwing arm at a darts board (`darts_world.rs`): no clip the game or a mod has is
//! a dart throw (a grenade's lob, a knife's swipe), so the right arm of whoever is at the
//! line is posed here, over whatever its body plays [design: owner 2026-11-05, the
//! clips "look weird"].
//!
//! - **The pose.** Two bones, the upper arm and the forearm, turned so the hand reaches a
//!   point set from the shoulder in the board's directions, the elbow bent down and out
//!   ([`solve`]). The rest of the body, the clavicle and the hand's own turn stay as
//!   animated.
//! - **The throw.** At the line the dart is held up in front of the face ([`AIM`]);
//!   drawing (the player holding the button, the opponent before it lets go) brings the
//!   hand back by the ear ([`DRAWN`]); letting go snaps the arm out towards the board
//!   ([`OUT`]), follows through ([`FOLLOW`]) and comes back to the aim ([`throw_path`]).
//! - **Easing.** The pose eases in as the thrower comes to the line and out as it leaves
//!   ([`EASE`]).
//! - **Watchers** (co-op): at a board near that another player plays at, whoever stands
//!   at the line (that player's body, or the opponent it plays) is posed the same way,
//!   from the draw and the throws its table's view sends (`coop_tables::DartsView`).
//! - **The darts.** Where each thrower's hand is (board-local) is kept in [`Hands`]: the
//!   held dart is drawn there, and each dart flies from where the hand was as it left.
//!
//! The bones are written after the animation has posed them (`Update`) and before
//! transforms are propagated, like head tracking (`head_look.rs`); they are found by the
//! human skeleton's names [observed].

use crate::limbs::ActorRig;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::coop_tables::{game, DartsView, Tables};
use nvf_play::darts_foes::DartsOpponent;
use nvf_play::minigames::{arm_draw, BoardFrame, DartsScreen, Page, UP};
use nvf_play::sim::SimState;
use nvf_play::space::{to_bevy, to_game};
use std::collections::HashMap;

/// The arm's bones [observed: the human skeleton's node names].
const UPPER: &str = "Bip01 R UpperArm";
const FORE: &str = "Bip01 R Forearm";
const HAND: &str = "Bip01 R Hand";
/// The dart is pinched between the thumb's tip and the index finger's middle joint
/// [observed: the human skeleton's bones under the right hand, about 11 units past the
/// wrist]; without them it sits at the wrist.
const PINCH: [&str; 2] = ["Bip01 R Thumb12", "Bip01 R Finger12"];
/// How far the dart's tip is ahead of the pinch, towards the board: the fingers hold the
/// barrel, not the point (the model's tip is its origin, its flights 9.3 behind
/// [observed: practicedart.nif's bounds]) [design].
const TIP_AHEAD: f32 = 3.5;

/// Where the held dart's tip is, board-local, for the pinch at `grip` (world).
fn tip(f: &BoardFrame, grip: Vec3) -> [f32; 3] {
    let mut p = f.to_local(to_game(grip));
    p[1] += TIP_AHEAD;
    p
}

/// Where the hand goes, from the shoulder, in arm lengths: (right, towards the board, up)
/// [design].
pub const AIM: [f32; 3] = [0.0, 0.48, 0.22];
pub const DRAWN: [f32; 3] = [0.04, 0.16, 0.36];
pub const OUT: [f32; 3] = [-0.04, 0.95, 0.12];
pub const FOLLOW: [f32; 3] = [0.0, 0.80, -0.20];
/// The way the elbow points, the same way [design].
const POLE: [f32; 3] = [0.5, 0.3, -1.0];
/// Seconds from letting go: arm out, followed through, back at the aim [design].
const SNAP: f32 = 0.08;
const THROUGH: f32 = 0.25;
const BACK: f32 = 0.70;
/// Seconds for the pose to ease in or out [design].
const EASE: f32 = 0.25;

/// What a thrower's arm is doing: at the line or not, drawn back (0..1), seconds since it
/// last let go.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Throwing {
    pub at_line: bool,
    pub draw: f32,
    pub since_release: Option<f32>,
}

/// The hands of this player (0) and the opponent (1), board-local; where each dart left
/// a hand (by its flight's start time: thrower, place).
#[derive(Resource, Default)]
pub struct Hands {
    pub now: [Option<[f32; 3]>; 2],
    pub left_from: Vec<(f32, usize, [f32; 3])>,
    released: [Option<f32>; 2],
    weight: [f32; 2],
    noted: [bool; 2],
}

/// The hand of whoever throws at a board near that another player plays at (co-op
/// watchers), board-local, by board: where it is now (while posed at the line) and where
/// each dart left it (by this game's clock, `Time::elapsed_secs`), so watchers draw the
/// held dart and the flights as the thrower's game does.
#[derive(Resource, Default)]
pub struct Watching(pub HashMap<u32, WatchedHand>);

#[derive(Default)]
pub struct WatchedHand {
    pub now: Option<[f32; 3]>,
    pub left: Vec<(f32, [f32; 3])>,
}

impl WatchedHand {
    /// Where the dart let go nearest `at` (this game's clock) left the hand.
    pub fn left_near(&self, at: f32) -> Option<[f32; 3]> {
        self.left.iter().filter(|(t, _)| (t - at).abs() < 0.5).min_by(|a, b| (a.0 - at).abs().total_cmp(&(b.0 - at).abs())).map(|l| l.1)
    }
}

impl Hands {
    /// Where the dart thrown at `since` by `who` (0 this player, 1 the opponent) left the
    /// hand.
    pub fn left(&self, since: f32, who: usize) -> Option<[f32; 3]> {
        self.left_from.iter().find(|(s, w, _)| *s == since && *w == who).map(|l| l.2)
    }
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = smooth(t);
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// The hand's place (from the shoulder, arm lengths, as [`AIM`]) for `t`.
pub fn throw_path(t: &Throwing) -> [f32; 3] {
    let held = mix(AIM, DRAWN, t.draw);
    match t.since_release {
        Some(s) if s < SNAP => mix(DRAWN, OUT, s / SNAP),
        Some(s) if s < THROUGH => mix(OUT, FOLLOW, (s - SNAP) / (THROUGH - SNAP)),
        Some(s) if s < BACK => mix(FOLLOW, held, (s - THROUGH) / (BACK - THROUGH)),
        _ => held,
    }
}

/// Two-bone reach: the elbow and the hand for a shoulder `s`, elbow `e0` and hand `h0`
/// (their lengths kept), the hand going to `target` (as near as the arm reaches) and the
/// elbow bent towards `pole`.
pub fn solve(s: Vec3, e0: Vec3, h0: Vec3, target: Vec3, pole: Vec3) -> (Vec3, Vec3) {
    let (a, b) = (e0.distance(s), h0.distance(e0));
    let d = target - s;
    let dir = d.normalize_or(Vec3::Y);
    let dist = d.length().clamp((a - b).abs() + 1e-3, a + b - 1e-3);
    let cos = ((a * a + dist * dist - b * b) / (2.0 * a * dist)).clamp(-1.0, 1.0);
    let side = (pole - dir * pole.dot(dir)).normalize_or(dir.any_orthonormal_vector());
    let e = s + (dir * cos + side * (1.0 - cos * cos).sqrt()) * a;
    (e, s + dir * dist)
}

/// The world transform of node `e` from the rig's root down, or `None` when `e` is not
/// under `root`.
fn world_of(e: Entity, root: Entity, parents: &Query<&ChildOf>, transforms: &Query<&mut Transform>, globals: &Query<&GlobalTransform>) -> Option<GlobalTransform> {
    let mut chain = Vec::new();
    let mut at = e;
    while at != root {
        chain.push(at);
        at = parents.get(at).ok()?.parent();
        if chain.len() > 64 {
            return None;
        }
    }
    let mut g = *globals.get(root).ok()?;
    for c in chain.iter().rev() {
        g = g.mul_transform(*transforms.get(*c).ok()?);
    }
    Some(g)
}

fn rot(g: &GlobalTransform) -> Quat {
    g.to_scale_rotation_translation().1
}

/// Pose one arm: the hand towards `path` (from the shoulder, arm lengths, in the board's
/// directions), weighted `w`; the hand's place (world) back, or why not.
#[allow(clippy::too_many_arguments)]
fn pose(
    root: Entity,
    rig: &ActorRig,
    f: &BoardFrame,
    path: [f32; 3],
    w: f32,
    parents: &Query<&ChildOf>,
    transforms: &mut Query<&mut Transform>,
    globals: &Query<&GlobalTransform>,
) -> Result<Vec3, &'static str> {
    let node = |name: &str| rig.skel.names.iter().position(|n| n.as_deref() == Some(name)).and_then(|i| rig.nodes.get(i).copied());
    let (Some(upper), Some(fore), Some(hand)) = (node(UPPER), node(FORE), node(HAND)) else { return Err("no right arm") };
    let above = parents.get(upper).map_err(|_| "the arm hangs from nothing")?.parent();
    let gp = world_of(above, root, parents, transforms, globals).ok_or("the arm is not under the body")?;
    let ro = |q: &Query<&mut Transform>, e: Entity| q.get(e).copied().map_err(|_| "a bone has no transform");
    let (lu, lf, lh) = (ro(transforms, upper)?, ro(transforms, fore)?, ro(transforms, hand)?);
    let gu = gp.mul_transform(lu);
    let gf = gu.mul_transform(lf);
    let gh = gf.mul_transform(lh);
    let (s, e0, h0) = (gu.translation(), gf.translation(), gh.translation());
    let length = s.distance(e0) + e0.distance(h0);
    if length < 1e-3 {
        return Err("the arm has no length");
    }
    let dirs = |v: [f32; 3]| {
        let o = to_bevy(f.to_world([0.0; 3]));
        let x = (to_bevy(f.to_world([1.0, 0.0, 0.0])) - o).normalize_or(Vec3::X);
        let y = (to_bevy(f.to_world([0.0, 1.0, 0.0])) - o).normalize_or(Vec3::Z);
        x * v[0] + y * v[1] + Vec3::Y * v[2]
    };
    let target = s + dirs(path) * length;
    let (e, h) = solve(s, e0, h0, target, dirs(POLE));
    let rp = rot(&gp);
    let ru = rp * lu.rotation;
    let du = Quat::from_rotation_arc((e0 - s).normalize(), (e - s).normalize());
    let ru2 = du * ru;
    let lu2 = (rp.inverse() * ru2).normalize();
    let cf = du * (h0 - e0);
    let df = Quat::from_rotation_arc(cf.normalize(), (h - e).normalize());
    let lf2 = (ru2.inverse() * df * ru2 * lf.rotation).normalize();
    let (u, fo) = (lu.rotation.slerp(lu2, w), lf.rotation.slerp(lf2, w));
    if let Ok(mut t) = transforms.get_mut(upper) {
        t.rotation = u;
    }
    if let Ok(mut t) = transforms.get_mut(fore) {
        t.rotation = fo;
    }
    // Where the dart is held: the pinch, carried by the posed hand.
    let posed = gp.mul_transform(Transform { rotation: u, ..lu }).mul_transform(Transform { rotation: fo, ..lf }).mul_transform(lh);
    let pinch: Vec<Vec3> = PINCH.iter().filter_map(|n| node(n)).filter_map(|e| under(e, hand, parents, transforms)).collect();
    let grip = if pinch.is_empty() { Vec3::ZERO } else { pinch.iter().sum::<Vec3>() / pinch.len() as f32 };
    Ok(posed.transform_point(grip))
}

/// Where a bone below `hand` is in the hand's own frame, from the bones' local transforms.
fn under(bone: Entity, hand: Entity, parents: &Query<&ChildOf>, transforms: &Query<&mut Transform>) -> Option<Vec3> {
    let mut at = bone;
    let mut p = Vec3::ZERO;
    while at != hand {
        p = transforms.get(at).ok()?.transform_point(p);
        at = parents.get(at).ok()?.parent();
    }
    Some(p)
}

/// A board another player is throwing at, as this game watches it: the thrower's body,
/// its throws this turn as last seen and when the last one went, the pose's weight.
#[derive(Default)]
pub struct Watched {
    body: Option<Entity>,
    /// The game's opponent's weapon node, hidden while it plays (as the thrower's game
    /// hides it, `darts_world.rs`).
    weapon: Option<Entity>,
    released: u8,
    at: Option<f32>,
    weight: f32,
}

/// How near the line spot a body stands to be the one throwing, game units [design].
const AT_LINE: f32 = 45.0;

/// The arms of this player and the opponent at the board, and of whoever throws at the
/// boards near that the others play at (from their tables' views).
#[allow(clippy::too_many_arguments)]
pub fn arms(
    time: Res<Time>,
    screen: Res<DartsScreen>,
    opp: Res<DartsOpponent>,
    body: Option<Res<crate::player_body::PlayerBody>>,
    (tables, sim): (Option<Res<Tables>>, Option<Res<SimState>>),
    rigs: Query<(Entity, &ActorRig, &GlobalTransform)>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
    mut hands: ResMut<Hands>,
    (mut watching, mut watched): (ResMut<Watching>, Local<HashMap<u32, Watched>>),
    mut shown: Query<&mut Visibility>,
) {
    let dt = time.delta_secs();
    let own = screen.0.as_ref().map(|d| d.board);
    match screen.0.as_ref().filter(|d| d.page == Page::Play && d.frame.is_some()) {
        Some(d) => own_board(d, dt, &opp, body.as_deref(), &rigs, &parents, &globals, &mut transforms, &mut hands),
        None => *hands = Hands::default(),
    }

    // The boards near that others play at.
    let now = time.elapsed_secs();
    let mut seen: Vec<u32> = Vec::new();
    if let (Some(t), Some(sim)) = (tables.as_deref().filter(|t| t.on), sim.as_deref()) {
        let local = body.as_deref().and_then(|b| b.third_rig()).map(|r| r.0);
        for p in &t.near {
            let Some((board, g, blob)) = t.view(*p) else { continue };
            if g != game::DARTS || Some(board) == own || seen.contains(&board) {
                continue;
            }
            let Some(v) = DartsView::decode(blob) else { continue };
            let placed = sim.refs.get(FormId(board));
            let Some((pos, rot)) = sim.world.peek(FormId(board)).map(|r| (r.pos, r.rot)).or_else(|| placed.map(|r| (r.pos, r.rot))) else { continue };
            let f = BoardFrame::of(pos, rot, placed.and_then(|r| r.scale));
            seen.push(board);
            let w = watched.entry(board).or_default();
            // Whoever stands at the line throws.
            let line = to_bevy(f.to_world(UP));
            let thrower = rigs
                .iter()
                .filter(|(e, _, _)| Some(*e) != local)
                .map(|(e, _, g)| (e, Vec2::new(g.translation().x - line.x, g.translation().z - line.z).length()))
                .filter(|(_, dist)| *dist < AT_LINE)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(e, _)| e);
            if thrower != w.body {
                debug!("darts arm: board 0x{board:08X}'s thrower is now {thrower:?}");
                (w.body, w.weight, w.at) = (thrower, 0.0, None);
            }
            // The game's opponent has its weapon put away from when it is brought over.
            let foe_weapon = Some(v.foe).filter(|f| *f != 0).and_then(|f| rigs.iter().find(|(_, r, _)| r.reference == f)).and_then(|(_, r, _)| weapon_node(r));
            if foe_weapon != w.weapon {
                show(w.weapon.take(), &mut shown);
                if let Some(mut vis) = foe_weapon.and_then(|n| shown.get_mut(n).ok()) {
                    *vis = Visibility::Hidden;
                    w.weapon = foe_weapon;
                }
            }
            let hand = watching.0.entry(board).or_default();
            if v.released > w.released {
                w.at = Some(now);
                if let Some(at) = hand.now {
                    hand.left.push((now, at));
                }
            }
            hand.left.retain(|(t, _)| now - t < 3.0);
            w.released = v.released;
            let since_release = w.at.map(|a| now - a).filter(|s| *s < BACK);
            let at_line = v.winner.is_none() && (!v.turn_over || since_release.is_some());
            let t = Throwing { at_line, draw: f32::from(v.draw) / 255.0, since_release };
            let Some((root, rig, _)) = thrower.and_then(|e| rigs.get(e).ok()) else {
                hand.now = None;
                continue;
            };
            w.weight = if t.at_line { (w.weight + dt / EASE).min(1.0) } else { (w.weight - dt / EASE).max(0.0) };
            hand.now = match pose(root, rig, &f, throw_path(&t), smooth(w.weight), &parents, &mut transforms, &globals) {
                Ok(h) if w.weight > 0.0 => Some(tip(&f, h)),
                Ok(_) => None,
                Err(why) => {
                    debug!("darts arm: the thrower at 0x{board:08X} is not posed: {why}");
                    None
                }
            };
        }
    }
    watched.retain(|b, w| {
        let keep = seen.contains(b);
        if !keep {
            show(w.weapon.take(), &mut shown);
        }
        keep
    });
    watching.0.retain(|b, _| seen.contains(b));
}

/// A body's weapon node [`crate::weapons::WEAPON_NODE`], if its skeleton has one.
pub fn weapon_node(rig: &ActorRig) -> Option<Entity> {
    let i = rig.skel.names.iter().position(|n| n.as_deref() == Some(crate::weapons::WEAPON_NODE))?;
    rig.nodes.get(i).copied()
}

/// A weapon node put out of sight shown again.
fn show(node: Option<Entity>, shown: &mut Query<&mut Visibility>) {
    if let Some(mut v) = node.and_then(|n| shown.get_mut(n).ok()) {
        *v = Visibility::Inherited;
    }
}

/// This player's own board: its arm and the opponent's.
#[allow(clippy::too_many_arguments)]
fn own_board(
    d: &nvf_play::minigames::Darts,
    dt: f32,
    opp: &DartsOpponent,
    body: Option<&crate::player_body::PlayerBody>,
    rigs: &Query<(Entity, &ActorRig, &GlobalTransform)>,
    parents: &Query<&ChildOf>,
    globals: &Query<&GlobalTransform>,
    transforms: &mut Query<&mut Transform>,
    hands: &mut Hands,
) {
    let Some(f) = d.frame else { return };
    let now = d.now;
    let opponent = opp.0.as_ref().filter(|o| o.board == d.board && !o.gone);
    // Who threw each dart in the air: this player, or the opponent (the other side).
    let who = |side: usize| if side == d.mine() { Some(0) } else if opponent.is_some() { Some(1) } else { None };
    for (since, _, side) in &d.flights {
        if let Some(k) = who(*side) {
            if hands.released[k].is_none_or(|r| r < *since) {
                hands.released[k] = Some(*since);
                if let Some(at) = hands.now[k] {
                    hands.left_from.push((*since, k, at));
                }
            }
        }
    }
    hands.left_from.retain(|(s, _, _)| d.flights.iter().any(|fl| fl.0 == *s));
    let since = |k: usize, h: &Hands| h.released[k].map(|r| now - r).filter(|s| *s < BACK);
    let draw = arm_draw(d, now);

    let mut throwers: [Option<(Entity, ActorRig, Throwing)>; 2] = [None, None];
    if let Some((root, rig)) = body.and_then(|b| b.third_rig()) {
        let at_line = d.stands_up == Some(true) && (d.my_turn() || since(0, hands).is_some());
        throwers[0] = Some((root, rig, Throwing { at_line, draw: if d.m.up == d.mine() { draw } else { 0.0 }, since_release: since(0, hands) }));
    }
    if let Some(o) = opponent {
        if let Some((root, rig, _)) = rigs.iter().find(|(_, r, _)| r.reference == o.actor.0) {
            let at_line = o.at_line() && (d.m.up == 1 || o.release.is_some() || since(1, hands).is_some());
            throwers[1] = Some((root, rig.clone(), Throwing { at_line, draw: if d.m.up == 1 { draw } else { 0.0 }, since_release: since(1, hands) }));
        }
    }
    for (k, t) in throwers.into_iter().enumerate() {
        let Some((root, rig, t)) = t else {
            hands.now[k] = None;
            hands.weight[k] = 0.0;
            continue;
        };
        let step = dt / EASE;
        hands.weight[k] = if t.at_line { (hands.weight[k] + step).min(1.0) } else { (hands.weight[k] - step).max(0.0) };
        let w = smooth(hands.weight[k]);
        match pose(root, &rig, &f, throw_path(&t), w, parents, transforms, globals) {
            Ok(h) => hands.now[k] = Some(tip(&f, h)),
            Err(why) => {
                hands.now[k] = None;
                if !std::mem::replace(&mut hands.noted[k], true) {
                    warn!("darts arm: {}'s arm is not posed: {why}", if k == 0 { "this player" } else { "the opponent" });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arm_reaches_the_target_and_keeps_its_lengths() {
        let (s, e0, h0) = (Vec3::ZERO, Vec3::new(0.0, -10.0, 0.0), Vec3::new(0.0, -19.0, 0.0));
        let target = Vec3::new(8.0, 6.0, 5.0);
        let (e, h) = solve(s, e0, h0, target, Vec3::new(0.0, -1.0, 0.0));
        assert!(h.distance(target) < 1e-3, "{h}");
        assert!((e.distance(s) - 10.0).abs() < 1e-3 && (h.distance(e) - 9.0).abs() < 1e-3);
        // The elbow bends towards the pole.
        assert!(e.y < 0.0, "{e}");
        // Out of reach: the arm points at it, straight.
        let (e, h) = solve(s, e0, h0, Vec3::new(100.0, 0.0, 0.0), Vec3::NEG_Y);
        assert!((h.x - 19.0).abs() < 1e-2 && e.x > 9.9, "{e} {h}");
    }

    #[test]
    fn a_throw_draws_back_snaps_out_and_comes_back() {
        let at = |draw: f32, s: Option<f32>| throw_path(&Throwing { at_line: true, draw, since_release: s });
        assert_eq!(at(0.0, None), AIM);
        assert_eq!(at(1.0, None), DRAWN);
        // The hand is furthest towards the board just after it lets go, then follows through
        // lower and is back at the aim.
        assert!(at(0.0, Some(SNAP))[1] > at(0.0, Some(THROUGH))[1] - 0.2);
        assert_eq!(at(0.0, Some(SNAP)), OUT);
        assert!(at(0.0, Some(THROUGH))[2] < AIM[2]);
        assert_eq!(at(0.0, Some(BACK)), AIM);
        // The drawn hand is behind and above the aiming one.
        const { assert!(DRAWN[1] < AIM[1] && DRAWN[2] > AIM[2]) };
    }
}
