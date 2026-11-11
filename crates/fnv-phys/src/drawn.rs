//! Drawing between steps (docs/spec-notes/physics.md §5.2): the pose each moving body had
//! before the last fixed step, so that a frame falling between two steps can draw it part
//! of the way from there to its pose after the step, as smoothly as the frame rate allows.
//!
//! - **What is kept.** At the start of every [`PhysWorld::step`], the pose of each body that
//!   can move in it: the dynamic bodies awake (rapier's active set: clutter, dropped items,
//!   ragdoll bones, gore pieces), those added, struck or woken since the last step, and the
//!   body the grab holds ([`crate::grab`]). Nothing else: a body asleep, fixed or kinematic
//!   is drawn where it is. The table is refilled every step in the same storage, and only
//!   the drawing reads it: what the simulation sees (poses written to it, queries, the
//!   engine's write-back and saves) stays on the stepped poses, and the steps stay
//!   bit-identical.
//! - **Not across a jump.** A body added, placed by hand (a teleport: [`PhysWorld::body_placed`],
//!   `place_character`, [`PhysWorld::pose_ref_object`]), rebuilt (a ragdoll set at rest or
//!   roused, a reference's collision built again for a script's SetPos) or frozen after its
//!   state went NaN ([`crate::quarantine`]) since the last step has no pose kept from before
//!   it: it is drawn where it is until the next step, so its pose before that step is its
//!   pose now. A body that moved more than [`DRAWN_JUMP`] along an axis in one step is drawn
//!   where it is too (it was put somewhere, not moved).
//! - **Woken by a contact.** A body asleep at the start of a step and woken by something
//!   touching it during the step has no pose kept for that step and is drawn where it is
//!   until the next one. It starts from rest, so that first step moves it little
//!   [hypothesis: not measured].
//! - **Blending.** `alpha` is the share of a step the game's clock has gone since the last
//!   one (the engine's `PhysState::draw_alpha`), in [0, 1]: translations linearly, rotations
//!   by slerp the short way round. 0 draws the pose before the last step, 1 the pose after
//!   it, exactly.

use crate::world::{body_state_at, BodyState, Owner, PhysWorld};
use rapier3d::math::Pose;
use rapier3d::prelude::*;
use std::collections::HashMap;

/// A body that went further than this along an axis in one step was put somewhere (lifted,
/// carried off, built again), not moved: it is drawn where it is, game units [design: 100
/// units a step is 6000 a second at 60 steps; moved here from the engine's corpses].
pub const DRAWN_JUMP: f32 = 100.0;

/// The poses before the last step (see the module notes).
#[derive(Debug, Clone, Default)]
pub(crate) struct Before {
    /// The pose of each body kept before the last step, rapier units.
    poses: HashMap<RigidBodyHandle, Pose>,
    /// [`PhysWorld::steps`] when they were taken: they hold for the step after it only.
    step: u64,
    /// The most bodies kept before one step (diagnostics).
    most: usize,
}

impl Before {
    /// Not drawn between steps until the next step: the body was put where it is (see the
    /// module notes).
    pub(crate) fn forget(&mut self, h: RigidBodyHandle) {
        self.poses.remove(&h);
    }
}

impl PhysWorld {
    /// Keep the pose of every body that can move in the coming step (see the module notes).
    /// The storage of the last step's table is reused.
    pub(crate) fn keep_poses_before(&mut self) {
        let held = self.grip.as_ref().and_then(|g| self.grip_handle(g.on, g.body));
        let (bodies, islands) = (&self.w.bodies, &self.w.islands);
        let before = &mut self.before;
        before.poses.clear();
        before.step = self.steps;
        let mut keep = |h: RigidBodyHandle| {
            if let Some(b) = bodies.get(h).filter(|b| b.is_dynamic() && b.is_enabled()) {
                before.poses.insert(h, *b.position());
            }
        };
        islands.active_bodies().for_each(&mut keep);
        self.new_bodies.iter().chain(&self.struck).copied().chain(held).for_each(keep);
        before.most = before.most.max(before.poses.len());
    }

    /// The pose to draw body `h` at, `alpha` of the way from its pose before the last step
    /// to its pose now (see the module notes); its pose now when it has none kept.
    pub(crate) fn drawn_pose(&self, h: RigidBodyHandle, b: &RigidBody, alpha: f32) -> Pose {
        let now = *b.position();
        let Some(from) = self.before.poses.get(&h).filter(|_| self.before.step + 1 == self.steps) else { return now };
        // Not a number (a clock gone wrong): the step's own pose.
        let a = if alpha.is_nan() { 1.0 } else { alpha.clamp(0.0, 1.0) };
        if a >= 1.0 || (now.translation - from.translation).abs().max_element() > DRAWN_JUMP * self.k {
            return now;
        }
        if a <= 0.0 {
            return *from;
        }
        Pose::from_parts(from.translation.lerp(now.translation, a), from.rotation.slerp(now.rotation, a).normalize())
    }

    /// The state of body `h` as drawn (`alpha`): its drawn pose, its velocity and sleep now.
    pub(crate) fn drawn_state(&self, h: RigidBodyHandle, alpha: f32) -> Option<BodyState> {
        let b = self.w.bodies.get(h)?;
        Some(body_state_at(b, &self.drawn_pose(h, b, alpha), self.k))
    }

    /// Each body of a reference as drawn this frame, `alpha` of a step after the last one
    /// (see the module notes), in object order: as [`PhysWorld::ref_bodies`], with the
    /// translation and rotation blended. For drawing only.
    pub fn drawn_ref_bodies(&self, form_id: u32, alpha: f32) -> Vec<BodyState> {
        self.refs.get(&form_id).into_iter().flatten().filter_map(|&b| self.drawn_state(b, alpha)).collect()
    }

    /// The `index`-th body of a reference as drawn ([`PhysWorld::drawn_ref_bodies`]), without
    /// building the list.
    pub fn drawn_ref_body(&self, form_id: u32, index: usize, alpha: f32) -> Option<BodyState> {
        self.drawn_state(*self.refs.get(&form_id)?.get(index)?, alpha)
    }

    /// A gore piece as drawn: as [`PhysWorld::gore_state`], blended (see the module notes).
    pub fn drawn_gore_state(&self, id: u32, alpha: f32) -> Option<BodyState> {
        self.drawn_state(*self.gore.get(&id)?, alpha)
    }

    /// Whether body `h` is drawn between steps now: it has a pose kept from before the last
    /// step that differs from its pose now.
    fn drawn_between(&self, h: RigidBodyHandle) -> bool {
        self.before.step + 1 == self.steps && self.before.poses.get(&h).is_some_and(|p| self.w.bodies.get(h).is_some_and(|b| b.position() != p))
    }

    /// The references with a body drawn between steps now ([`Owner::Ref`]: placed and
    /// dropped clutter, a placed object's jointed bodies), each once, sorted.
    pub fn drawn_refs(&self) -> Vec<u32> {
        let mut out = Vec::new();
        self.drawn_refs_into(&mut out);
        out
    }

    /// [`PhysWorld::drawn_refs`] into `out` (cleared first), so a caller can keep the
    /// storage from frame to frame.
    pub fn drawn_refs_into(&self, out: &mut Vec<u32>) {
        out.clear();
        out.extend(self.before.poses.keys().filter(|&&h| self.drawn_between(h)).filter_map(|&h| match Owner::decode(self.w.bodies.get(h)?.user_data) {
            Some(Owner::Ref { form_id, .. }) => Some(form_id),
            _ => None,
        }));
        out.sort_unstable();
        out.dedup();
    }

    /// How many bodies are drawn between steps now, and the most kept before one step so
    /// far (diagnostics: the engine's exit report).
    pub fn drawn_counts(&self) -> (usize, usize) {
        (self.before.poses.keys().filter(|&&h| self.drawn_between(h)).count(), self.before.most)
    }
}

#[cfg(test)]
mod tests {
    use crate::world::{PhysParams, PhysWorld};
    use rapier3d::math::{Pose, Vector};
    use rapier3d::prelude::*;

    /// A body placed by hand between steps (`body_placed`, as `place_character` does) is
    /// drawn where it was put, not on the way there; from the next step on it is drawn
    /// between steps from there.
    #[test]
    fn a_body_placed_by_hand_is_drawn_where_it_was_put() {
        let mut w = PhysWorld::new(PhysParams::default());
        let h = w.insert_body(RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 0.0, 5.0)));
        w.w.insert_collider(ColliderBuilder::ball(0.1), Some(h));
        w.step();
        w.step();
        let drawn = |w: &PhysWorld, a: f32| w.drawn_pose(h, &w.w.bodies[h], a);
        assert_ne!(drawn(&w, 0.0), *w.w.bodies[h].position(), "falling: drawn behind");
        let to = Pose::from_translation(Vector::new(3.0, 0.0, 5.0));
        w.w.bodies[h].set_position(to, true);
        w.body_placed(h);
        assert_eq!(drawn(&w, 0.0), to);
        w.step();
        assert_eq!(drawn(&w, 0.0), to);
        assert_ne!(drawn(&w, 1.0), to);
    }
}
