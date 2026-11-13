//! rapier's island bookkeeping kept whole between steps.
//!
//! rapier puts bodies to sleep by island: every body linked to another by a touching contact
//! or a joint shares its island (its *persistent island*), and an island sleeps or wakes
//! whole. The awake bodies all sit in one set, each sleeping island in a set of its own
//! [documented: rapier3d 0.36.1 `dynamics/island_manager`, `sleep.rs` `wake_up`
//! "waking any body of a sleeping island wakes the whole island"].
//!
//! A query between two steps brings the broad phase up to date first
//! ([`PhysWorld::update_queries`]) with rapier's collision-only update, which takes every
//! body's pending changes without telling the island manager [documented: rapier3d 0.36.1
//! `CollisionPipeline::step` passes no islands to `handle_user_changes_to_rigid_bodies` and
//! clears the changes after]. A change made since the last step was then lost to the islands:
//!
//! - a sleeping body woken (struck, pushed, cut from its ragdoll) was awake but left in its
//!   sleeping set, never simulated;
//! - a body turned fixed after a step (`crate::quarantine`) stayed a member of its island.
//!
//! Either way one island could end with bodies both in the awake set and in a sleeping one.
//! When it fell asleep rapier found a body of another set in it and panicked [observed: the
//! owner's crash of 2026-11-11 after a VATS kill, `extract_sleeping_sub_island`, island.rs:54;
//! tests/islands.rs `a_sleeping_corpse_struck_before_a_query_wakes_whole`]. After every
//! refresh [`PhysWorld::repair_islands`] puts that right, whatever made the change.

use crate::world::{Owner, PhysWorld};
use rapier3d::prelude::*;
use std::collections::{HashMap, HashSet};

impl PhysWorld {
    /// After a query's refresh (see the module notes): every island that has a body in
    /// the awake set, or a body awake but outside it, is woken whole, and a fixed body
    /// still listed in an island has its type told again, so the next step takes it out.
    /// Returns the bodies repaired.
    pub(crate) fn repair_islands(&mut self) -> usize {
        let (bodies, islands) = (&mut self.w.bodies, &mut self.w.islands);
        let awake: HashSet<RigidBodyHandle> = islands.active_bodies().collect();
        // Islands with a body in the awake set, and the members outside it.
        let mut lit: HashSet<u32> = HashSet::new();
        let mut outside: Vec<(RigidBodyHandle, u32)> = Vec::new();
        let mut fixed_members: Vec<RigidBodyHandle> = Vec::new();
        for (h, b) in bodies.iter() {
            if !b.is_enabled() {
                continue;
            }
            let island = islands.persistent_island_of(bodies, h);
            if b.is_fixed() {
                if island.is_some() {
                    fixed_members.push(h);
                }
                continue;
            }
            // Not registered yet (added since the last step): the next step does that.
            let Some(island) = island else { continue };
            if awake.contains(&h) {
                lit.insert(island);
            } else {
                outside.push((h, island));
            }
        }
        let mut repaired = 0;
        for (h, island) in outside {
            let awake_body = bodies.get(h).is_some_and(|b| !b.is_sleeping());
            if awake_body || lit.contains(&island) {
                islands.wake_up(bodies, h, false);
                repaired += 1;
            }
        }
        for h in fixed_members {
            if let Some(b) = bodies.get_mut(h) {
                b.set_body_type(RigidBodyType::KinematicPositionBased, false);
                b.set_body_type(RigidBodyType::Fixed, false);
                repaired += 1;
            }
        }
        repaired
    }

    /// rapier's island bookkeeping, checked from outside (tests and diagnostics): no fixed
    /// body is an island's member, no awake body is outside the awake set, and every island
    /// lies wholly in the awake set or wholly out of it (see the module notes). Returns the
    /// first fault found, as text; `None` when whole.
    #[doc(hidden)]
    pub fn island_split(&self) -> Option<String> {
        let (bodies, islands) = (&self.w.bodies, &self.w.islands);
        let awake: HashSet<RigidBodyHandle> = islands.active_bodies().collect();
        let owner = |h: RigidBodyHandle| bodies.get(h).and_then(|b| Owner::decode(b.user_data));
        let mut seen: HashMap<u32, (Option<RigidBodyHandle>, Option<RigidBodyHandle>)> = HashMap::new();
        for (h, b) in bodies.iter() {
            if !b.is_enabled() {
                continue;
            }
            let Some(id) = islands.persistent_island_of(bodies, h) else { continue };
            if b.is_fixed() {
                return Some(format!("island {id}: fixed body {h:?} {:?} is a member", owner(h)));
            }
            let e = seen.entry(id).or_default();
            if awake.contains(&h) {
                e.0.get_or_insert(h);
            } else if !b.is_sleeping() {
                return Some(format!("island {id}: {h:?} {:?} awake but outside the awake set, never simulated", owner(h)));
            } else {
                e.1.get_or_insert(h);
            }
            if let (Some(a), Some(s)) = *e {
                return Some(format!(
                    "island {id}: {a:?} {:?} awake, {s:?} {:?} in a sleeping set (asleep {})",
                    owner(a),
                    owner(s),
                    bodies[s].is_sleeping()
                ));
            }
        }
        None
    }
}
