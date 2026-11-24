//! The darts of a darts game in the world (the fork; nvf-play `minigames.rs`): the mod's
//! `PracticeDart.nif` held in the hand before the throw, flying to where it lands, and
//! stuck in the cabinet's board until the turn is over [observed: docs/spec-notes/darts.md
//! §10, the mod's darts placed square to the board on its face; design: the hand, the
//! pull-back while the throw is held and the arc].
//!
//! - **The stage** [design: owner 2026-11-04, 2026-11-05]: first person at the line on
//!   the player's own turn (`nvf_play::minigames::OWN_EYE`); otherwise the camera stands
//!   fixed behind the throw line (`EYE`) and the player's own body is seen in third
//!   person. The weapon is put away throughout. The player whose turn it is steps up to the line
//!   (`minigames::UP`), the other waits back from it (`WAIT`).
//! - **The throwers' arms** (`darts_arm.rs`): posed for each throw, the player's and the
//!   opponent's alike; each held dart is drawn in its thrower's hand and flies from where
//!   the hand was when it let go.
//! - **The opponent** (`nvf_play::darts_foes`): one of the board's people, held where the
//!   game walks it (`Residents::hold_pose`) and let go after the match.
//! - **The hand** ([`HAND`]): where a held dart is drawn when its thrower's hand is not
//!   known (no body drawn), the dart pointing at the aim. Holding the button draws it
//!   back ([`PULL`]); letting go throws it.
//! - **The flight**: [`fnv_game::darts::FLIGHT`] seconds from the hand of whoever is at
//!   the line to the landing point on an arc [`ARC`] high, the dart turned along its way.
//! - **Watchers**: a co-op player near a board another player is throwing at sees that
//!   board's darts too (its view's darts this turn, `nvf_play::coop_tables::DartsView`).

use crate::darts_arm::Hands;
use crate::world::WorldRoot;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use fnv_game::darts;
use fnv_sim::FormId;
use nvf_assets::model::{ModelCtx, PreparedModel};
use nvf_play::control::{PlayerControl, Pov};
use nvf_play::coop_tables::{game, DartsView, Tables};
use nvf_play::minigames::{first_person, BoardFrame, DartsScreen, Page, FACE_Y, OWN_EYE};
use nvf_play::sim::SimState;
use std::sync::Arc;

const DART: &str = "meshes\\minigames\\darts\\practicedart.nif";
/// Where the thrower at the line holds the dart, board units in the board's frame: right
/// of and in front of the body at `minigames::UP`, a little under the bull [design].
pub const HAND: [f32; 3] = [-6.0, -60.0, -4.0];
/// Where this player's darts leave the hand in first person (the player's own turn),
/// board units in the board's frame: in front of `minigames::OWN_EYE`, low and to the
/// right. The held dart itself is not drawn there [design: owner 2026-11-05; the body's
/// hand is under the bottom of that view, and the model is 9.3 long behind its tip].
pub const OWN_HAND: [f32; 3] = [OWN_EYE[0] + 7.0, OWN_EYE[1] + 24.0, OWN_EYE[2] - 6.0];
/// How far a held throw draws the dart back, board units, at full draw [design].
pub const PULL: f32 = 3.5;
/// A stuck dart's way into the board: its tip a little down, its flights up, as darts
/// thrown on an arc sit [design].
pub const STUCK: Vec3 = Vec3::new(0.0, 1.0, -0.3);
/// Seconds to full draw [design].
const DRAW: f32 = 0.35;
/// The flight's height over the straight line, board units [design].
pub const ARC: f32 = 2.5;

#[derive(Component)]
struct DartHolder;

#[derive(Resource, Default)]
struct DartModels {
    model: Option<Arc<PreparedModel>>,
    tried: bool,
    holders: Vec<Entity>,
    shown_most: usize,
}

pub struct DartsWorldPlugin;

impl Plugin for DartsWorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DartModels>()
            .init_resource::<Stage>()
            .init_resource::<Hands>()
            .init_resource::<crate::darts_arm::Watching>()
            .add_systems(Update, stage.before(nvf_play::player::BodySet).before(crate::idles::drive))
            .add_systems(Update, opponent.after(nvf_play::PlaySet::Ui))
            .add_systems(PostUpdate, (crate::darts_arm::arms, place).chain().before(bevy::transform::TransformSystems::Propagate))
            .add_systems(Last, report);
    }
}

/// The view while a darts game is open: what the player had before.
#[derive(Resource, Default)]
struct Stage {
    pov: Option<Pov>,
}

/// The weapon away while a game at a board is open; first person on the player's own
/// turn, third person otherwise (`minigames::first_person`).
fn stage(mut st: ResMut<Stage>, screen: Res<DartsScreen>, mut control: ResMut<PlayerControl>) {
    if screen.0.as_ref().filter(|d| d.frame.is_some()).is_none() {
        if let Some(pov) = st.pov.take() {
            control.pov = pov;
            info!("darts stage: the view is the player's again");
        }
        return;
    }
    if st.pov.is_none() {
        st.pov = Some(control.pov);
        info!("darts stage: the weapon away; first person on this player's turn, third otherwise");
    }
    let own = screen.0.as_ref().is_some_and(nvf_play::minigames::first_person);
    let pov = if own { Pov::First } else { Pov::Third };
    if control.pov != pov {
        info!("darts stage: {}", if own { "first person, this player's turn" } else { "third person behind the line" });
    }
    control.pov = pov;
    control.weapon_out = false;
}

/// The navmesh, the world's cells and the game, for the opponent's way.
type WayData<'w> = (Option<Res<'w, crate::nav::NavState>>, Option<Res<'w, crate::world::WorldData>>, Option<Res<'w, SimState>>);

/// The darts opponent's body: held where the game walks it and let go after (a new one,
/// or none, lets the last go). Its weapon is put out of sight meanwhile: a Boomer's
/// launcher on its back filled the view from behind the line (owner 2026-11-05).
/// While its body is on furniture the game's walk waits, from where the body is; once
/// up, the way to where the walk goes is planned on the navmesh (`darts_foes::follow`).
#[allow(clippy::too_many_arguments)]
fn opponent(
    mut opp: ResMut<nvf_play::darts_foes::DartsOpponent>,
    mut residents: Option<ResMut<crate::residents::Residents>>,
    mut held: Local<Option<(u32, Option<Entity>)>>,
    rigs: Query<(&crate::limbs::ActorRig, &Transform, Has<crate::furniture::OnFurniture>)>,
    mut shown: Query<&mut Visibility>,
    (nav, world, sim): WayData,
) {
    if let Some(o) = opp.0.as_mut().filter(|o| !o.gone) {
        let body = rigs.iter().find(|(r, _, _)| r.reference == o.actor.0);
        let seated = body.is_some_and(|(_, _, on)| on);
        if seated != o.seated {
            info!("darts opponent: 0x{:08X} {}", o.actor.0, if seated { "gets up first" } else { "is on its feet" });
        }
        o.seated = seated;
        if let Some((_, t, true)) = body {
            // It sets off, and comes back to, where it stands up.
            o.feet = t.translation.to_array();
            o.heading = crate::nav::tr_heading(t);
            (o.home, o.home_heading) = (o.feet, o.heading);
            o.route_for = None;
        } else if o.route_for != Some(o.target) {
            o.route = way(o, nav.as_deref(), world.as_deref(), sim.as_deref());
            o.route_for = Some(o.target);
        }
    }
    let o = opp.0.as_ref().filter(|o| !o.gone);
    if let Some((id, weapon)) = *held {
        if o.is_none_or(|o| o.actor.0 != id) {
            if let Some(r) = residents.as_deref_mut() {
                r.release_pose(id);
            }
            if let Some(mut v) = weapon.and_then(|w| shown.get_mut(w).ok()) {
                *v = Visibility::Inherited;
            }
            info!("darts opponent: 0x{id:08X} is its AI's again");
            *held = None;
        }
    }
    let Some(o) = o else { return };
    if let Some(r) = residents.as_deref_mut() {
        r.hold_pose(o.actor.0, crate::residents::RemotePose { feet: o.feet, heading: o.heading, moving: o.moving, furniture: None, space: 0 });
    }
    if held.is_none() {
        info!("darts opponent: 0x{:08X} ({}) is walked by the game", o.actor.0, o.name);
        *held = Some((o.actor.0, None));
    }
    // Its weapon's node, hidden once its body is drawn.
    if let Some((_, weapon @ None)) = held.as_mut() {
        let node = rigs.iter().find(|(r, _, _)| r.reference == o.actor.0).and_then(|(r, _, _)| crate::darts_arm::weapon_node(r));
        if let Some(mut v) = node.and_then(|n| shown.get_mut(n).ok()) {
            *v = Visibility::Hidden;
            *weapon = node;
        }
    }
}

/// The navmesh's way for the opponent from where it is to where its walk goes, as corners
/// (the last at the target); empty (straight) when there is no navmesh or no way, or the
/// way is much longer than the straight line (a wall between: better straight than
/// round the building) [design].
fn way(o: &nvf_play::darts_foes::Opponent, nav: Option<&crate::nav::NavState>, world: Option<&crate::world::WorldData>, sim: Option<&SimState>) -> Vec<[f32; 3]> {
    let (Some(nav), Some(world), Some(sim)) = (nav, world, sim) else { return Vec::new() };
    let Some(cell) = sim.world.peek(FormId(o.board)).and_then(|s| s.cell) else { return Vec::new() };
    let Some(space) = crate::nav::space_of_cell(world, cell.0) else { return Vec::new() };
    let params = fnv_nav::PathParams { doors: false, ..Default::default() };
    let from = fnv_nav::Place { space, pos: o.feet };
    let to = fnv_nav::Place { space, pos: o.target };
    let straight = (o.target[0] - o.feet[0]).hypot(o.target[1] - o.feet[1]);
    match nav.graph.find_path(from, to, &params) {
        Ok(p) if p.length < 3.0 * straight + 200.0 => {
            let mut pts: Vec<[f32; 3]> = p.legs.into_iter().flat_map(|l| l.points).collect();
            // It ends on the spot itself, not where the navmesh put it.
            pts.pop();
            pts.push(o.target);
            debug!("darts opponent: a way of {} corners, {:.0} units (straight {straight:.0})", pts.len(), p.length);
            pts
        }
        Ok(p) => {
            debug!("darts opponent: the way is {:.0} units against {straight:.0} straight: straight", p.length);
            Vec::new()
        }
        Err(e) => {
            debug!("darts opponent: no way on the navmesh ({e:?}): straight");
            Vec::new()
        }
    }
}

/// A dart square to the board at a local point, pointing along `dir` (local).
fn dart_at(f: &BoardFrame, at: [f32; 3], dir: Vec3) -> Transform {
    let w = f.to_world(at);
    let turn = if dir.length_squared() > 1e-8 { Quat::from_rotation_arc(Vec3::Y, dir.normalize()) } else { Quat::IDENTITY };
    Transform { translation: Vec3::from(w), rotation: f.rotation() * turn, scale: Vec3::splat(f.k) }
}

/// Where a dart in the air is, `t` (0..1) of its way from `from` to `to`, and its heading.
pub fn in_flight(from: [f32; 3], to: [f32; 3], t: f32) -> ([f32; 3], Vec3) {
    let t = t.clamp(0.0, 1.0);
    let (a, b) = (Vec3::from(from), Vec3::from(to));
    let up = Vec3::Z * ARC * (std::f32::consts::PI * t).sin();
    let vel = (b - a) + Vec3::Z * ARC * std::f32::consts::PI * (std::f32::consts::PI * t).cos();
    ((a + (b - a) * t + up).to_array(), vel)
}

/// The held dart: in the hand, drawn back by `draw` (0..1), pointing at the aim.
pub fn in_hand(hand: [f32; 3], aim: (f32, f32), draw: f32, bob: f32) -> ([f32; 3], Vec3) {
    let at = [hand[0], hand[1] - PULL * draw, hand[2] + 0.4 * draw + bob];
    let dir = Vec3::new(aim.0 - at[0], FACE_Y - at[1], aim.1 - at[2]);
    (at, dir)
}

#[allow(clippy::too_many_arguments)]
fn place(
    mut commands: Commands,
    mut st: ResMut<DartModels>,
    (hands, opp, watching, time): (Res<Hands>, Res<nvf_play::darts_foes::DartsOpponent>, Res<crate::darts_arm::Watching>, Res<Time>),
    mut models: ModelCtx,
    screen: Res<DartsScreen>,
    tables: Option<Res<Tables>>,
    sim: Option<Res<SimState>>,
    root: Query<Entity, With<WorldRoot>>,
    mut holders: Query<(&mut Transform, &mut Visibility), With<DartHolder>>,
) {
    let mut want: Vec<Transform> = Vec::new();
    if let Some(d) = screen.0.as_ref() {
        if let (Some(f), Page::Play) = (d.frame, d.page) {
            let now = d.now;
            let own = first_person(d);
            for (x, z) in &d.stuck {
                want.push(dart_at(&f, [*x, FACE_Y, *z], STUCK));
            }
            // Whoever throws stands at the line.
            for (since, (x, z), side) in &d.flights {
                let who = usize::from(*side != d.mine());
                // In first person this player's darts fly from the hand in view.
                let from = if own && who == 0 { OWN_HAND } else { hands.left(*since, who).unwrap_or(HAND) };
                let (at, dir) = in_flight(from, [*x, FACE_Y, *z], (now - since) / darts::FLIGHT);
                want.push(dart_at(&f, at, dir));
            }
            // The opponent's next dart, in its hand at the line: the game's opponent's, or
            // the other player's (its hand is known only while its arm is raised).
            // (Up to four players: whoever's turn it is when it is not this player's hand.)
            let theirs_up = !d.up_is_me();
            let at_line = (opp.0.as_ref().is_some_and(|o| o.board == d.board && o.at_line()) || d.joined.is_some() || matches!(d.foe, nvf_play::minigames::Foe::Player(_))) && theirs_up && d.m.winner.is_none();
            let to_throw = !d.flights.iter().any(|fl| fl.2 != d.mine()) && d.m.sides.get(d.m.up).is_some_and(|s| s.darts.len() < usize::from(darts::DARTS_PER_TURN));
            if let (true, true, Some(at)) = (at_line, to_throw, hands.now[1]) {
                want.push(dart_at(&f, at, Vec3::Y));
            }
            let mine_in_air = d.flights.iter().any(|fl| fl.2 == d.mine());
            let thrown = d.m.sides.get(d.mine()).map_or(0, |s| s.darts.len());
            // Not drawn in first person: the held dart stood in the view of the board, the
            // reason for that view (owner 2026-11-05); it shows once it leaves the hand.
            if !own && d.my_turn() && !mine_in_air && thrown < usize::from(darts::DARTS_PER_TURN) {
                let aim = d.press.map_or_else(|| d.sway_aim(now), |p| p.locked);
                let draw = d.press.map_or(0.0, |p| ((now - p.since) / DRAW).clamp(0.0, 1.0));
                let bob = 0.12 * (now * 2.1).sin();
                // In the hand when the body is drawn, pointing at the aim.
                let (at, dir) = match hands.now[0] {
                    Some(h) => (h, Vec3::new(aim.0 - h[0], FACE_Y - h[1], aim.1 - h[2])),
                    None => in_hand(HAND, aim, draw, bob),
                };
                want.push(dart_at(&f, at, dir));
            }
        }
    }
    // Boards other players near are throwing at (not this player's own).
    if let (Some(t), Some(sim)) = (tables.as_deref().filter(|t| t.on), sim.as_deref()) {
        let own = screen.0.as_ref().map(|d| d.board);
        for p in &t.near {
            let Some((board, g, blob)) = t.view(*p) else { continue };
            if g != game::DARTS || Some(board) == own {
                continue;
            }
            let Some(v) = DartsView::decode(blob) else { continue };
            let placed = sim.refs.get(FormId(board));
            let Some((pos, rot)) = sim.world.peek(FormId(board)).map(|r| (r.pos, r.rot)).or_else(|| placed.map(|r| (r.pos, r.rot))) else { continue };
            let f = BoardFrame::of(pos, rot, placed.and_then(|r| r.scale));
            for (x, z) in &v.stuck {
                want.push(dart_at(&f, [*x, FACE_Y, *z], STUCK));
            }
            // As the thrower's game draws them: each dart in the air flying from where the
            // hand let it go, and the next in the hand at the line.
            let now = time.elapsed_secs();
            let hand = watching.0.get(&board);
            for (age, x, z) in &v.flights {
                let flown = f32::from(*age) / 100.0;
                let from = hand.and_then(|h| h.left_near(now - flown)).unwrap_or(HAND);
                let (at, dir) = in_flight(from, [*x, FACE_Y, *z], flown / darts::FLIGHT);
                want.push(dart_at(&f, at, dir));
            }
            let left = v.sides.get(usize::from(v.up)).is_some_and(|s| s.2.len() < usize::from(darts::DARTS_PER_TURN));
            if let (Some(h), true) = (hand.and_then(|h| h.now), v.winner.is_none() && !v.turn_over && v.flights.is_empty() && left) {
                // The owner's dart points at its aim, the opponent's straight on.
                let dir = if v.up == 0 { Vec3::new(v.aim.0 - h[0], FACE_Y - h[1], v.aim.1 - h[2]) } else { Vec3::Y };
                want.push(dart_at(&f, h, dir));
            }
        }
    }
    if want.is_empty() && st.holders.is_empty() {
        return;
    }
    if !st.tried {
        st.tried = true;
        st.model = models.model(DART);
        match &st.model {
            Some(pm) => info!("darts world: {DART}: {} shapes, bounds {:?}", pm.shapes.len(), pm.bounds),
            None => warn!("darts world: {DART} not found; the darts are not drawn"),
        }
    }
    let (Some(pm), Ok(root)) = (st.model.clone(), root.single()) else { return };
    while st.holders.len() < want.len() {
        let t = want[st.holders.len()];
        let h = commands.spawn((DartHolder, t, Visibility::Inherited, ChildOf(root))).id();
        for s in &pm.shapes {
            let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(s.transform)), Visibility::Inherited, NotShadowCaster)).id();
            commands.entity(h).add_child(e);
        }
        st.holders.push(h);
    }
    st.shown_most = st.shown_most.max(want.len());
    for (i, h) in st.holders.iter().enumerate() {
        let Ok((mut t, mut vis)) = holders.get_mut(*h) else { continue };
        match want.get(i) {
            Some(w) => {
                *t = *w;
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, st: Res<DartModels>) {
    if exit.read().last().is_some() {
        info!("darts world summary: {} dart models made, at most {} shown at once", st.holders.len(), st.shown_most);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_frame_turns_with_its_reference() {
        let f = BoardFrame::of([100.0, 200.0, 50.0], [0.0, 0.0, std::f32::consts::FRAC_PI_2], None);
        // Turned 90 degrees clockwise: the board's +y (into it) is the world's +x.
        let w = f.to_world([0.0, 10.0, 0.0]);
        assert!((w[0] - (100.0 + 10.0 * f.k)).abs() < 1e-3 && (w[1] - 200.0).abs() < 1e-3, "{w:?}");
        let back = f.to_local(w);
        assert!((back[1] - 10.0).abs() < 1e-3 && back[0].abs() < 1e-3);
        // The camera's ray towards the bull meets the face at the bull.
        let eye = f.to_world(nvf_play::minigames::EYE);
        let bull = f.to_world([0.0, FACE_Y, 0.0]);
        let (x, z) = f.on_face(eye, [bull[0] - eye[0], bull[1] - eye[1], bull[2] - eye[2]]).unwrap();
        assert!(x.abs() < 1e-3 && z.abs() < 1e-3, "{x} {z}");
        // Looking away from the board meets nothing.
        assert!(f.on_face(eye, [-1.0, 0.0, 0.0]).is_none());
    }

    #[test]
    fn a_dart_flies_from_the_hand_to_where_it_lands() {
        let to = [2.0, FACE_Y, 3.0];
        let (a, _) = in_flight(HAND, to, 0.0);
        let (b, dir) = in_flight(HAND, to, 1.0);
        assert_eq!(a, HAND);
        assert!((Vec3::from(b) - Vec3::from(to)).length() < 1e-4);
        // At the end it is going into the board.
        assert!(dir.y > 0.0);
        let (mid, _) = in_flight(HAND, to, 0.5);
        assert!(mid[2] > (HAND[2] + to[2]) / 2.0 + ARC * 0.99);
    }

    #[test]
    fn a_held_throw_draws_the_dart_back() {
        let (rest, _) = in_hand(HAND, (0.0, 0.0), 0.0, 0.0);
        let (drawn, dir) = in_hand(HAND, (0.0, 0.0), 1.0, 0.0);
        assert!((rest[1] - drawn[1] - PULL).abs() < 1e-4);
        // Into the board, and across from the hand towards the aim at the centre.
        assert!(dir.y > 0.0 && dir.x * HAND[0] < 0.0);
    }

    #[test]
    fn the_camera_behind_the_line_sees_past_the_thrower() {
        use nvf_play::minigames::{EYE, LINE, UP, WAIT};
        // Behind the line; the thrower and the hand between it and the board.
        const { assert!(EYE[1] < LINE[1] && LINE[1] < HAND[1] && HAND[1] < FACE_Y) };
        // The thrower stands off the camera's line to the bull, the waiting player behind
        // the camera.
        let to_bull = Vec2::new(-EYE[0], FACE_Y - EYE[1]).normalize();
        let thrower = Vec2::new(UP[0] - EYE[0], UP[1] - EYE[1]);
        let off = (thrower - to_bull * thrower.dot(to_bull)).length();
        assert!(off > 20.0, "the thrower's body is {off:.1} off the view's middle");
        const { assert!(WAIT[1] < EYE[1]) };
    }
}
