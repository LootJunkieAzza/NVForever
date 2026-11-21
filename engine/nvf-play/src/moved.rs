//! Placed references drawn where the simulation holds them when that is not where the
//! plugins put them: clutter pushed or knocked down by physics (its pose written back by
//! `crate::phys::PhysState::settle_placed`), or moved by a script.
//!
//! Render streaming (`engine/nvforever`) draws placed references at their placement, as
//! children of their cell's [`RenderCell`] entity; this module moves those entities by
//! the change from the pose they were drawn at to the simulation's: each frame for the
//! references physics moved (every frame while their body moves: drawn between the last
//! two physics steps, `crate::phys::PhysState::drawn_ref_pose`, so they move as smoothly as
//! the frame rate allows while the simulation keeps the stepped pose) or a script's Rotate
//! turned or SetScale resized
//! (`Game::turned`) or the console moved (`crate::RefRepose`, which still names what the
//! tick cleared from `turned`), and for every reference whose entities just appeared (a cell
//! streamed in) and whose simulation pose or script scale differs. The scale is a
//! script's (SetScale, kept on the reference's state) in place of the placement's. Only entities whose parent is the
//! cell are moved (a model with its own node hierarchy moves with its root). Actors are
//! left alone (their placement is not their pose once AI runs; Layer 6).

use crate::phys::PhysState;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::{PlaySet, RefVisual, RenderCell};
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_sim::FormId;
use fnv_world::transform::ref_rotation;
use std::collections::{BTreeSet, HashMap};

/// The reference pose (game space, scale included) baked into an entity's transform,
/// when it is not the placement's.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Baked(pub Affine3A);

/// A reference's placement in game space: `p' = pos + scale R(rot) p`.
pub fn ref_affine(pos: [f32; 3], rot: [f32; 3], scale: f32) -> Affine3A {
    let m = Mat3::from_cols_array(&ref_rotation(rot)).transpose(); // row-major
    Affine3A::from_mat3_translation(m * scale, Vec3::from_array(pos))
}

/// The new transform (relative to a cell whose origin is `origin`) of an entity drawn
/// with reference pose `have` baked in, for reference pose `want`.
pub fn rebake(t: &Transform, origin: Vec3, have: Affine3A, want: Affine3A) -> Transform {
    let to_cell = Affine3A::from_translation(-origin);
    let m = to_cell * want * have.inverse() * to_cell.inverse();
    Transform::from_matrix(Mat4::from(m) * t.to_matrix())
}

pub struct MovedPlugin;

impl Plugin for MovedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, follow.in_set(PlaySet::Sync));
    }
}

/// A reference pose with a row-major rotation matrix: `p' = pos + scale R p`.
pub fn ref_affine_matrix(pos: [f32; 3], rot: &[f32; 9], scale: f32) -> Affine3A {
    Affine3A::from_mat3_translation(Mat3::from_cols_array(rot).transpose() * scale, Vec3::from_array(pos))
}

/// Where reference `id` (placed at `p`) is drawn this frame: its body drawn between the
/// last two physics steps when it has one (`PhysState::drawn_ref_pose`), else the
/// simulation's pose.
pub fn drawn_affine(sim: &SimState, phys: &PhysState, id: u32, p: &fnv_sim::refs::PlacedRef) -> Affine3A {
    let scale = sim.ref_scale(id);
    match phys.drawn_ref_pose(id) {
        Some((pos, rot)) => ref_affine_matrix(pos, &rot, scale),
        None => {
            let (pos, rot) = sim.moved_pose(id).unwrap_or((p.pos, p.rot));
            ref_affine(pos, rot, scale)
        }
    }
}

type Drawn<'w, 's> = Query<'w, 's, (&'static RefVisual, &'static mut Transform, Option<&'static mut Baked>, &'static ChildOf)>;

/// The entities [`follow`] moves for each reference it has moved, so that the references
/// drawn between physics steps every frame are not looked for among every drawn entity
/// each time. A reference's list is found again when an entity of it appears or one of
/// them is gone.
#[derive(Default)]
struct Found(HashMap<u32, Vec<Entity>>);

#[allow(clippy::too_many_arguments)]
fn follow(
    mut commands: Commands,
    sim: Res<SimState>,
    phys: Res<PhysState>,
    script: Res<ScriptState>,
    mut repose: MessageReader<crate::RefRepose>,
    added: Query<&RefVisual, Added<RefVisual>>,
    mut drawn: Drawn,
    all: Query<(Entity, &RefVisual), With<ChildOf>>,
    cells: Query<&Transform, (With<RenderCell>, Without<RefVisual>)>,
    mut found: Local<Found>,
) {
    let mut ids: BTreeSet<u32> = phys.moved_now.iter().copied().collect();
    ids.extend(script.rt.iter().flat_map(|rt| rt.game.turned.iter().map(|r| r.0)));
    ids.extend(repose.read().map(|m| m.form_id));
    ids.extend(added.iter().map(|r| r.0).filter(|&id| sim.moved_pose(id).is_some() || sim.script_scale(id).is_some()));
    // Clutter drawn between physics steps, every frame while it moves and once after.
    ids.extend(phys.between.redraw.iter().copied());
    for r in &added {
        found.0.remove(&r.0);
    }
    if ids.is_empty() {
        return;
    }
    let missing: BTreeSet<u32> = ids.iter().copied().filter(|id| !found.0.contains_key(id)).collect();
    if !missing.is_empty() {
        for &id in &missing {
            found.0.insert(id, Vec::new());
        }
        for (e, r) in &all {
            if missing.contains(&r.0) {
                found.0.entry(r.0).or_default().push(e);
            }
        }
    }
    for id in ids {
        let Some(p) = sim.refs.get(FormId(id)).filter(|p| &p.sig != b"ACHR" && &p.sig != b"ACRE") else { continue };
        let want = drawn_affine(&sim, &phys, id, p);
        let placed = ref_affine(p.pos, p.rot, p.scale.unwrap_or(1.0));
        let mut gone = false;
        for &e in found.0.get(&id).into_iter().flatten() {
            // Despawned, or now another reference's: looked for again next time.
            let Ok((r, mut t, baked, parent)) = drawn.get_mut(e) else {
                gone = true;
                continue;
            };
            if r.0 != id {
                gone = true;
                continue;
            }
            let Ok(cell) = cells.get(parent.parent()) else { continue };
            let have = baked.as_ref().map_or(placed, |b| b.0);
            if want.abs_diff_eq(have, 1e-5) {
                continue;
            }
            *t = rebake(&t, cell.translation, have, want);
            match baked {
                Some(mut b) => b.0 = want,
                None => {
                    commands.entity(e).insert(Baked(want));
                }
            }
        }
        if gone {
            found.0.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebaking_moves_an_entity_by_the_pose_change() {
        // A mesh 5 units above a reference's origin, drawn in a cell at (4096, 0, 0).
        let origin = Vec3::new(4096.0, 0.0, 0.0);
        let placed = ref_affine([4100.0, 10.0, 20.0], [0.0, 0.0, 0.0], 1.0);
        let shape = Affine3A::from_translation(Vec3::Z * 5.0);
        let drawn = Transform::from_matrix(Mat4::from(Affine3A::from_translation(-origin) * placed * shape));
        // Pushed 30 units east and turned 90 degrees about z.
        let moved = ref_affine([4130.0, 10.0, 20.0], [0.0, 0.0, std::f32::consts::FRAC_PI_2], 1.0);
        let t = rebake(&drawn, origin, placed, moved);
        let want = Affine3A::from_translation(-origin) * moved * shape;
        assert!(Affine3A::from_mat4(t.to_matrix()).abs_diff_eq(want, 1e-3), "{t:?}");
        // Back again: the first transform.
        let back = rebake(&t, origin, moved, placed);
        assert!(back.translation.abs_diff_eq(drawn.translation, 1e-3), "{back:?}");
    }

    /// Issue #1618: what the console moved reaches the drawing through `RefRepose` alone
    /// (the tick has cleared `Game::turned` by then, and physics moved nothing), once.
    #[test]
    fn a_console_move_moves_the_drawn_model_once() {
        use crate::sim::placed_ref;
        use fnv_sim::refs::PlacedRef;
        const BOX: u32 = 0x301;
        const ACTOR: u32 = 0x302;
        let at = [4100.0, 10.0, 20.0];
        let origin = Vec3::new(4096.0, 0.0, 0.0);
        let sim = SimState::over_placed([PlacedRef { pos: at, ..placed_ref(BOX) }, PlacedRef { sig: *b"ACHR", pos: at, ..placed_ref(ACTOR) }]);
        let mut app = App::new();
        app.add_message::<crate::RefRepose>().insert_resource(sim).insert_resource(PhysState::default()).insert_resource(ScriptState::over_no_plugins()).add_systems(Update, follow);
        let cell = app.world_mut().spawn((RenderCell::Interior(0x200), Transform::from_translation(origin))).id();
        // A mesh 5 units above the reference's origin.
        let shape = Affine3A::from_translation(Vec3::Z * 5.0);
        let drawn = Transform::from_matrix(Mat4::from(Affine3A::from_translation(-origin) * ref_affine(at, [0.0; 3], 1.0) * shape));
        let mesh = app.world_mut().spawn((RefVisual(BOX), drawn, ChildOf(cell))).id();
        let body = app.world_mut().spawn((RefVisual(ACTOR), drawn, ChildOf(cell))).id();
        // Past `Added<RefVisual>`, which would find a moved pose by itself.
        app.update();
        let z = |app: &App, e: Entity| app.world().get::<Transform>(e).unwrap().translation.z;
        assert!((z(&app, mesh) - 25.0).abs() < 1e-3);
        // As `SetPos z 170` from the console: the simulation holds the new height.
        for r in [BOX, ACTOR] {
            app.world_mut().resource_mut::<SimState>().world.get(FormId(r)).unwrap().pos[2] = 170.0;
        }
        app.update();
        assert!((z(&app, mesh) - 25.0).abs() < 1e-3, "nothing named it: no sweep of the scene");
        app.world_mut().write_message(crate::RefRepose { form_id: BOX });
        app.world_mut().write_message(crate::RefRepose { form_id: ACTOR });
        app.update();
        assert!((z(&app, mesh) - 175.0).abs() < 1e-3);
        assert!((z(&app, body) - 25.0).abs() < 1e-3, "actors are left alone");
        // No second application: on a later frame, or when it is named again.
        app.update();
        assert!((z(&app, mesh) - 175.0).abs() < 1e-3);
        app.world_mut().write_message(crate::RefRepose { form_id: BOX });
        app.update();
        assert!((z(&app, mesh) - 175.0).abs() < 1e-3);
        // Its cell streamed out and in again: the new entity, drawn at the placement, is
        // found (not the gone one kept from before) and moved.
        app.world_mut().despawn(mesh);
        let again = app.world_mut().spawn((RefVisual(BOX), drawn, ChildOf(cell))).id();
        app.update();
        assert!((z(&app, again) - 175.0).abs() < 1e-3);
        app.world_mut().write_message(crate::RefRepose { form_id: BOX });
        app.update();
        assert!((z(&app, again) - 175.0).abs() < 1e-3);
    }
}
