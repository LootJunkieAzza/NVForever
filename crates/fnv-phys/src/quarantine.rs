//! Bodies and colliders whose state went NaN or infinite (docs/spec-notes/physics.md §5.3).
//!
//! rapier 0.35 and later check every step for non-finite state: a body whose pose or
//! velocity went NaN or infinite is rolled back to its last finite pose (when it has one),
//! its velocities and forces zeroed and the body disabled; a collider whose own geometry
//! went non-finite is disabled; both are listed for that step in
//! `PhysicsWorld::quarantine()` [documented: rapier3d 0.36.1
//! `src/pipeline/physics_pipeline/quarantine.rs`]. After each step [`PhysWorld::step`]
//! reads that list:
//!
//! - **Reported once.** Each body or collider is logged the first time it is quarantined,
//!   with what it belongs to ([`Owner`]), and counted in `Stats::quarantined_bodies` and
//!   `Stats::quarantined_colliders`; [`PhysWorld::quarantined`] lists the reports.
//! - **Not left disabled out of sight** [design]. A disabled body has no contacts and no
//!   query finds it, while the engine still draws its object where it was: a bottle no
//!   shot, grab or step could touch. So:
//!   - a dynamic body of a reference (clutter, a dropped item) or a gore piece is turned
//!     into a fixed body where rapier rolled it back and switched on again: it stays drawn
//!     where it was, solid and found by rays, but never simulated again. Simulating it
//!     again could run into the same NaN every step; taking it out of the world would make
//!     the item vanish (a quest item too) and leave the engine's tables (references,
//!     write-back, the grab) naming a body that is gone;
//!   - a ragdoll body takes its whole ragdoll to rest ([`PhysWorld::set_ragdoll_at_rest`],
//!     fixed bodies where they lie), letting the grab go first if it holds the ragdoll;
//!   - a kinematic body (a character, an actor's hit volume) is switched on again: its
//!     owner puts it somewhere new before every step;
//!   - a body with no finite pose to go back to stays disabled (nothing sensible to draw or
//!     collide), and the report says so;
//!   - a collider with non-finite geometry stays disabled: its shape itself is bad.
//!
//! A frozen body has no pose kept for drawing between steps (`crate::drawn`): it is drawn
//! where it was rolled back to.

use crate::world::{Owner, PhysWorld};
use rapier3d::prelude::*;
use std::collections::HashSet;

/// What was done with a quarantined body or collider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Remedy {
    /// A dynamic body made fixed where it was rolled back to.
    Frozen,
    /// Its ragdoll set at rest.
    RagdollAtRest,
    /// A kinematic or fixed body switched on again.
    Enabled,
    /// Left disabled (no finite pose to go back to, or a collider's own geometry).
    Disabled,
}

/// One body or collider quarantined, reported once.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct QuarantineReport {
    /// The step it happened in ([`PhysWorld::steps`] before that step).
    pub step: u64,
    /// A collider rather than a body.
    pub collider: bool,
    /// What it belongs to: the body's or collider's `user_data`.
    pub owner: Option<Owner>,
    /// Where it was left, game units (the body's pose, the collider's), when finite.
    pub at: Option<[f32; 3]>,
    pub remedy: Remedy,
}

/// The bodies and colliders quarantined so far.
#[derive(Debug, Clone, Default)]
pub(crate) struct Quarantined {
    bodies: HashSet<RigidBodyHandle>,
    colliders: HashSet<ColliderHandle>,
    reports: Vec<QuarantineReport>,
}

impl Quarantined {
    /// Distinct bodies and colliders quarantined so far.
    pub(crate) fn counts(&self) -> (usize, usize) {
        (self.bodies.len(), self.colliders.len())
    }
}

/// An owner in words for the log.
fn describe(owner: Option<Owner>) -> String {
    match owner {
        Some(Owner::Ref { form_id, object, layer }) => format!("reference {form_id:08X} object {object} (layer {layer})"),
        Some(Owner::Actor { form_id, part: crate::gore::GORE_PART, .. }) => format!("a gore piece of actor {form_id:08X}"),
        Some(Owner::Actor { form_id, part, .. }) => format!("actor {form_id:08X} part {part}"),
        Some(Owner::Character { id }) => format!("character {id:08X}"),
        Some(Owner::Terrain { x, y }) => format!("terrain ({x}, {y})"),
        None => "nothing known".into(),
    }
}

impl PhysWorld {
    /// The bodies and colliders quarantined so far, each reported once, in the order found.
    pub fn quarantined(&self) -> &[QuarantineReport] {
        &self.quarantined.reports
    }

    /// Log, count and deal with what the last rapier step quarantined (see the module notes).
    pub(crate) fn handle_quarantine(&mut self) {
        let q = self.w.quarantine();
        // Copied out (only when there is something): the remedies change the world.
        let bodies: Vec<RigidBodyHandle> = q.bodies().to_vec();
        let colliders: Vec<ColliderHandle> = q.colliders().to_vec();
        let mut ragdolls = Vec::new();
        for h in bodies {
            let Some(b) = self.w.bodies.get(h) else { continue };
            let owner = Owner::decode(b.user_data);
            let finite = b.position().is_finite();
            let at = finite.then(|| crate::convert::arr3(b.position().translation / self.k));
            let ragdoll = self.ragdoll_of.get(&h).map(|r| r.0);
            let remedy = match (finite, ragdoll) {
                (false, _) => Remedy::Disabled,
                (true, Some(id)) => {
                    ragdolls.push(id);
                    Remedy::RagdollAtRest
                }
                (true, None) if b.is_dynamic() => Remedy::Frozen,
                (true, None) => Remedy::Enabled,
            };
            self.before.forget(h);
            if remedy == Remedy::Frozen {
                // The grab lets go of a body that no longer moves.
                if self.grip.as_ref().is_some_and(|g| self.grip_handle(g.on, g.body) == Some(h)) {
                    self.grip = None;
                }
                let b = &mut self.w.bodies[h];
                b.set_body_type(RigidBodyType::Fixed, false);
                b.set_enabled(true);
            } else if remedy == Remedy::Enabled {
                self.w.bodies[h].set_enabled(true);
            }
            if remedy != Remedy::Disabled {
                self.mark_dirty_bodies();
            }
            if self.quarantined.bodies.insert(h) {
                let step = self.steps;
                let what = match remedy {
                    Remedy::Frozen => "rolled back and frozen there (a fixed body from now on)",
                    Remedy::RagdollAtRest => "its ragdoll is set at rest where it lies",
                    Remedy::Enabled => "rolled back and switched on again (its owner places it)",
                    Remedy::Disabled => "no finite pose to go back to: left disabled",
                };
                tracing::warn!("physics: quarantined the body of {} at step {step}: its state went NaN or infinite; {what}", describe(owner));
                self.quarantined.reports.push(QuarantineReport { step, collider: false, owner, at, remedy });
            }
        }
        ragdolls.sort_unstable();
        ragdolls.dedup();
        for id in ragdolls {
            if self.ragdoll_held(id) {
                self.grip = None;
            }
            self.set_ragdoll_at_rest(id);
        }
        for c in colliders {
            let Some(co) = self.w.colliders.get(c) else { continue };
            if !self.quarantined.colliders.insert(c) {
                continue;
            }
            let owner = Owner::decode(co.user_data);
            let p = co.position();
            let at = p.is_finite().then(|| crate::convert::arr3(p.translation / self.k));
            let step = self.steps;
            tracing::warn!("physics: quarantined a collider of {} at step {step}: its geometry went NaN or infinite; left disabled", describe(owner));
            self.quarantined.reports.push(QuarantineReport { step, collider: true, owner, at, remedy: Remedy::Disabled });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::world::{Owner, PhysParams, PhysWorld};
    use rapier3d::prelude::*;

    /// A collider whose shape is not a number is disabled by rapier, reported once with its
    /// owner and counted; the world steps on.
    #[test]
    fn a_collider_gone_nan_is_reported_once() {
        let mut w = PhysWorld::new(PhysParams::default());
        let owner = Owner::Ref { form_id: 0x77, object: 0, layer: 1 };
        let c = w.w.insert_collider(ColliderBuilder::ball(f32::NAN).user_data(owner.encode()), None);
        for _ in 0..5 {
            w.step();
        }
        assert!(!w.w.colliders[c].is_enabled());
        let r = w.quarantined();
        assert_eq!(r.len(), 1, "{r:?}");
        assert!(r[0].collider && r[0].owner == Some(owner) && r[0].remedy == super::Remedy::Disabled, "{r:?}");
        assert_eq!((w.stats().quarantined_bodies, w.stats().quarantined_colliders), (0, 1));
    }
}
