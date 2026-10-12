//! Placed references drawn where the simulation holds them when that is not where the
//! plugins put them: clutter pushed or knocked down by physics (its pose written back by
//! `crate::phys::PhysState::settle_placed`), or moved by a script.
//!
//! Render streaming (`engine/nvforever`) draws placed references at their placement, as
//! children of their cell's [`RenderCell`] entity; this module moves those entities by
//! the change from the pose they were drawn at to the simulation's: each frame for the
//! references physics moved or a script's Rotate turned (`Game::turned`), and for every reference whose entities just appeared (a
//! cell streamed in) and whose simulation pose differs. Only entities whose parent is the
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
use std::collections::BTreeSet;

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

type Drawn<'w, 's> = Query<'w, 's, (Entity, &'static RefVisual, &'static mut Transform, Option<&'static Baked>, &'static ChildOf)>;

fn follow(
    mut commands: Commands,
    sim: Res<SimState>,
    phys: Res<PhysState>,
    script: Res<ScriptState>,
    added: Query<&RefVisual, Added<RefVisual>>,
    mut drawn: Drawn,
    cells: Query<&Transform, (With<RenderCell>, Without<RefVisual>)>,
) {
    let mut ids: BTreeSet<u32> = phys.moved_now.iter().copied().collect();
    ids.extend(script.rt.iter().flat_map(|rt| rt.game.turned.iter().map(|r| r.0)));
    ids.extend(added.iter().map(|r| r.0).filter(|&id| sim.moved_pose(id).is_some()));
    if ids.is_empty() {
        return;
    }
    for (e, r, mut t, baked, parent) in &mut drawn {
        if !ids.contains(&r.0) {
            continue;
        }
        let Ok(cell) = cells.get(parent.parent()) else { continue };
        let Some(p) = sim.refs.get(FormId(r.0)).filter(|p| &p.sig != b"ACHR" && &p.sig != b"ACRE") else { continue };
        let scale = p.scale.unwrap_or(1.0);
        let (pos, rot) = sim.moved_pose(r.0).unwrap_or((p.pos, p.rot));
        let want = ref_affine(pos, rot, scale);
        let have = baked.map_or_else(|| ref_affine(p.pos, p.rot, scale), |b| b.0);
        if want.abs_diff_eq(have, 1e-5) {
            continue;
        }
        *t = rebake(&t, cell.translation, have, want);
        commands.entity(e).insert(Baked(want));
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
}
