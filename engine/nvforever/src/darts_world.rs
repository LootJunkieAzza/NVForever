//! The darts of a darts game in the world (the fork; nvf-play `minigames.rs`): the mod's
//! `PracticeDart.nif` held in the hand before the throw, flying to where it lands, and
//! stuck in the cabinet's board until the turn is over [observed: docs/spec-notes/darts.md
//! §10, the mod's darts placed square to the board on its face; design: the hand, the
//! pull-back while the throw is held and the arc].
//!
//! - **The hand** ([`HAND`]): low and to the right of the throw-line camera, the dart
//!   pointing at the aim. Holding the button draws it back ([`PULL`]); letting go throws it.
//! - **The flight**: [`fnv_game::darts::FLIGHT`] seconds from the hand (the opponent's and
//!   the other player's from the left, [`OTHER_HAND`]) to the landing point on an arc
//!   [`ARC`] high, the dart turned along its way.
//! - **Watchers**: a co-op player near a board another player is throwing at sees that
//!   board's darts too (its view's darts this turn, `nvf_play::coop_tables::DartsView`).

use crate::world::WorldRoot;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use fnv_game::darts;
use fnv_sim::FormId;
use nvf_assets::model::{ModelCtx, PreparedModel};
use nvf_play::coop_tables::{game, DartsView, Tables};
use nvf_play::minigames::{BoardFrame, DartsScreen, Page, FACE_Y};
use nvf_play::sim::SimState;
use std::sync::Arc;

const DART: &str = "meshes\\minigames\\darts\\practicedart.nif";
/// Where the throwing hand holds the dart, board units in the board's frame [design].
pub const HAND: [f32; 3] = [2.4, -53.0, -2.6];
/// Where the other side's darts come from [design].
pub const OTHER_HAND: [f32; 3] = [-8.0, -50.0, -2.0];
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
        app.init_resource::<DartModels>().add_systems(PostUpdate, place.before(bevy::transform::TransformSystems::Propagate)).add_systems(Last, report);
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
pub fn in_hand(aim: (f32, f32), draw: f32, bob: f32) -> ([f32; 3], Vec3) {
    let at = [HAND[0], HAND[1] - PULL * draw, HAND[2] + 0.4 * draw + bob];
    let dir = Vec3::new(aim.0 - at[0], FACE_Y - at[1], aim.1 - at[2]);
    (at, dir)
}

#[allow(clippy::too_many_arguments)]
fn place(
    mut commands: Commands,
    mut st: ResMut<DartModels>,
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
            for (x, z) in &d.stuck {
                want.push(dart_at(&f, [*x, FACE_Y, *z], STUCK));
            }
            for (since, (x, z), side) in &d.flights {
                let from = if *side == d.mine() { HAND } else { OTHER_HAND };
                let (at, dir) = in_flight(from, [*x, FACE_Y, *z], (now - since) / darts::FLIGHT);
                want.push(dart_at(&f, at, dir));
            }
            let mine_in_air = d.flights.iter().any(|fl| fl.2 == d.mine());
            let thrown = d.m.sides.get(d.mine()).map_or(0, |s| s.darts.len());
            if d.my_turn() && !mine_in_air && thrown < usize::from(darts::DARTS_PER_TURN) {
                let aim = d.press.map_or_else(|| d.sway_aim(now), |p| p.locked);
                let draw = d.press.map_or(0.0, |p| ((now - p.since) / DRAW).clamp(0.0, 1.0));
                let (at, dir) = in_hand(aim, draw, 0.12 * (now * 2.1).sin());
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
        // The camera's ray straight ahead meets the face at the bull.
        let eye = f.to_world(nvf_play::minigames::EYE);
        let (x, z) = f.on_face(eye, [1.0, 0.0, 0.0]).unwrap();
        assert!(x.abs() < 1e-3 && z.abs() < 1e-3);
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
        let (rest, _) = in_hand((0.0, 0.0), 0.0, 0.0);
        let (drawn, dir) = in_hand((0.0, 0.0), 1.0, 0.0);
        assert!((rest[1] - drawn[1] - PULL).abs() < 1e-4);
        assert!(dir.y > 0.0 && dir.x < 0.0);
    }
}
