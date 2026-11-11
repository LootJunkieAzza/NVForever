//! Keyframed bodies moved with a velocity (docs/spec-notes/physics.md §5.5): the bodies
//! an animation moves (a lift, a gate, a drawbridge, a trap's jaws, a door's leaf; issue
//! #1396, doors.md) go from pose to pose over the physics steps with the velocity that takes
//! them there, so rapier knows how fast they go: loose bodies resting on one ride along
//! with it (their contacts' friction), and one it moves into is pushed out of its way at
//! its speed instead of being found inside it after a jump.
//!
//! - **The bodies** are fixed, as before, except during a step that moves one: then it is a
//!   velocity-based kinematic body (`RigidBodyType::KinematicVelocityBased`) with the
//!   velocity that takes it its share of the way ([`PhysWorld::drive_movers`]), fixed again
//!   after the step ([`PhysWorld::movers_stepped`]) [design, 2026-11-11]. The step's contacts
//!   see the velocity; the characters, moved between steps, see a fixed body as before.
//!   rapier's character controller treats a kinematic body it touches as a moving platform
//!   (`detect_grounded_status_and_apply_friction`): it takes the body's velocity at the
//!   contact as the character's along the contact's normal and adds its sideways part, at
//!   rest too. That rule froze a character walking down a keyframed slope at rest (issue
//!   #1485, test `keyframed_slope_does_not_freeze_the_character`, which failed with the
//!   bodies kinematic between steps) and carried the player along with Easy Pete
//!   (physics.md §6.8). A position-based kinematic body (`set_next_kinematic_position`)
//!   would also have rapier derive the velocity itself, from the centre of mass that a
//!   degenerate collision mesh puts anywhere (#1485's phantom velocity). So the player and
//!   the people are not carried by movers: they never were; their own controller keeps
//!   them on a lift's floor as it rises, as before.
//! - **Swept or put** ([`PhysWorld::pose_ref_object`]): the caller says whether a pose
//!   continues the motion (the same sequence a little later: swept) or jumps (the first
//!   pose after the body is built, another sequence, time going back or far ahead: put
//!   there at once, as a teleport). A swept move further than [`SWEEP_MOST`] is put too.
//!   Building a reference (a cell loading, a script's SetPos, a load) always puts it.
//! - **Spread over the frame's steps**: a pose is asked for once a frame, at the frame's
//!   time; the engine says how many steps the frame takes ([`PhysWorld::spread_moves`]) and
//!   each of them takes the body the same share of the rest of the way, so its speed is the
//!   animation's whatever the physics rate, and in slow motion (shorter steps, smaller
//!   moves). A frame with no step keeps the target for the next. A caller that never
//!   spreads (the tools, the tests) moves the body the whole way in the next step.
//! - **Switch back** ([`PhysWorld::set_swept_movers`], the engine's `NVF_PHYS_MOVERS_PUT`):
//!   every pose is put, the behaviour before 2026-11-11, for comparing.

use crate::world::{Owner, PhysWorld};
use rapier3d::dynamics::RigidBodyPosition;
use rapier3d::math::{Pose, Vector};
use rapier3d::prelude::*;
use std::collections::{BTreeSet, HashMap};

/// A swept move longer than this (game units, one pose to the next) is put instead
/// [design: 200 units in a frame is 12,000 units a second at 60 frames; no lift or gate
/// goes that fast, and sweeping a body that far would fling what it meets].
pub const SWEEP_MOST: f32 = 200.0;

/// A body on its way to a pose.
#[derive(Debug, Clone, Copy)]
struct Sweep {
    to: Pose,
    /// Steps left to get there; 0: not spread yet (taken as 1).
    steps: u32,
}

/// Counts over the run ([`PhysWorld::mover_stats`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MoverStats {
    /// Poses swept and put.
    pub swept: u64,
    pub put: u64,
    /// Steps in which a body moved with a velocity (one per body and step).
    pub body_steps: u64,
    /// The fastest a body's centre of mass went in a step, game units a second.
    pub fastest: f32,
    /// Bodies a moving body touched (carried or pushed), each once per mover.
    pub touched: u64,
}

/// A body a mover touched while it moved: carried along or pushed
/// ([`PhysWorld::take_mover_contacts`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoverContact {
    /// The mover's reference.
    pub mover: u32,
    /// The reference whose body it touched: a loose object's, or an actor's (its ragdoll).
    pub other: u32,
    /// The body is an actor's.
    pub actor: bool,
}

/// The movers' state in [`PhysWorld`].
#[derive(Debug, Clone, Default)]
pub(crate) struct Movers {
    /// Swept movers off: every pose is put.
    pub(crate) put_only: bool,
    sweeps: HashMap<RigidBodyHandle, Sweep>,
    /// Bodies given a velocity for the coming step.
    moving: Vec<RigidBodyHandle>,
    stats: MoverStats,
    /// (mover, other) pairs already found.
    seen: BTreeSet<(u32, u32)>,
    found: Vec<MoverContact>,
}

impl Movers {
    pub(crate) fn forget(&mut self, h: RigidBodyHandle) {
        self.sweeps.remove(&h);
        self.moving.retain(|&m| m != h);
    }
}

/// `share` of the way from `a` to `b`: translations linearly, rotations by slerp.
fn between(a: &Pose, b: &Pose, share: f32) -> Pose {
    if share >= 1.0 {
        return *b;
    }
    Pose::from_parts(a.translation.lerp(b.translation, share), a.rotation.slerp(b.rotation, share).normalize())
}

impl PhysWorld {
    /// Whether keyframed bodies move to a pose with a velocity over the steps (the default;
    /// see the module notes) or are put at each pose, as before 2026-11-11.
    pub fn set_swept_movers(&mut self, on: bool) {
        self.movers.put_only = !on;
    }

    pub fn swept_movers(&self) -> bool {
        !self.movers.put_only
    }

    /// The coming frame takes `steps` steps: the poses asked for since the last step are
    /// reached over them, a share each (see the module notes). 0: kept for the next frame.
    pub fn spread_moves(&mut self, steps: u32) {
        if steps == 0 {
            return;
        }
        for s in self.movers.sweeps.values_mut().filter(|s| s.steps == 0) {
            s.steps = steps;
        }
    }

    /// The movers' counts over the run.
    pub fn mover_stats(&self) -> MoverStats {
        self.movers.stats
    }

    /// The bodies the movers touched for the first time since the last call (each pair of
    /// mover and body is told once).
    pub fn take_mover_contacts(&mut self) -> Vec<MoverContact> {
        std::mem::take(&mut self.movers.found)
    }

    /// Whether body `h` is a mover still on its way to a pose asked for.
    pub fn mover_pending(&self, h: RigidBodyHandle) -> bool {
        self.movers.sweeps.contains_key(&h)
    }

    /// Move keyframed body `b` to `p` (rapier units): swept when `sweep` and swept movers
    /// are on, otherwise put there at once. Returns whether it is put.
    pub(crate) fn move_keyframed(&mut self, b: RigidBodyHandle, p: Pose, sweep: bool) -> bool {
        let k = self.k;
        let Some(body) = self.w.bodies.get_mut(b) else { return false };
        let far = (p.translation - body.position().translation).length() / k > SWEEP_MOST;
        if sweep && !self.movers.put_only && !far {
            if *body.position() == p && !self.movers.sweeps.contains_key(&b) {
                return false;
            }
            self.movers.sweeps.insert(b, Sweep { to: p, steps: 0 });
            self.movers.stats.swept += 1;
            return false;
        }
        self.movers.sweeps.remove(&b);
        if *body.position() == p {
            return false;
        }
        body.set_position(p, true);
        self.movers.stats.put += 1;
        true
    }

    /// Before a step: give each mover on its way the velocity that takes it its share of
    /// the rest of the way in this step.
    pub(crate) fn drive_movers(&mut self) {
        if self.movers.sweeps.is_empty() {
            return;
        }
        let dt = self.w.integration_parameters.dt;
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }
        let k = self.k;
        let mut arrived = Vec::new();
        for (&h, s) in self.movers.sweeps.iter_mut() {
            let Some(body) = self.w.bodies.get_mut(h) else {
                arrived.push(h);
                continue;
            };
            let left = s.steps.max(1);
            let from = *body.position();
            let next = between(&from, &s.to, 1.0 / left as f32);
            let v = RigidBodyPosition { position: from, next_position: next }.interpolate_velocity(1.0 / dt, body.local_center_of_mass());
            if !(v.linvel.is_finite() && v.angvel.is_finite()) {
                // Put rather than flung (a pose gone wrong).
                body.set_position(s.to, true);
                arrived.push(h);
                continue;
            }
            // Kinematic for this step only (see the module notes); rapier integrates the
            // velocity about the same centre of mass.
            body.set_body_type(RigidBodyType::KinematicVelocityBased, true);
            body.set_linvel(v.linvel, true);
            body.set_angvel(v.angvel, true);
            self.movers.moving.push(h);
            self.movers.stats.body_steps += 1;
            self.movers.stats.fastest = self.movers.stats.fastest.max(v.linvel.length() / k);
            s.steps = left - 1;
            if s.steps == 0 {
                arrived.push(h);
            }
        }
        for h in arrived {
            self.movers.sweeps.remove(&h);
        }
    }

    /// After a step: the bodies that moved in it are fixed again (so the characters see them
    /// as before, see the module notes); what they touched is noted.
    pub(crate) fn movers_stepped(&mut self) {
        if self.movers.moving.is_empty() {
            return;
        }
        let moving = std::mem::take(&mut self.movers.moving);
        for &h in &moving {
            let Some(body) = self.w.bodies.get_mut(h) else { continue };
            body.set_linvel(Vector::ZERO, false);
            body.set_angvel(Vector::ZERO, false);
            body.set_body_type(RigidBodyType::Fixed, false);
            let Some(Owner::Ref { form_id: mover, .. }) = Owner::decode(body.user_data) else { continue };
            for &c in self.w.bodies[h].colliders() {
                for pair in self.w.narrow_phase.contact_pairs_with(c).filter(|p| p.has_any_active_contact()) {
                    let other = if pair.collider1 == c { pair.collider2 } else { pair.collider1 };
                    let Some(oc) = self.w.colliders.get(other) else { continue };
                    if !oc.parent().and_then(|p| self.w.bodies.get(p)).is_some_and(|b| b.is_dynamic()) {
                        continue;
                    }
                    let (other, actor) = match Owner::decode(oc.user_data) {
                        Some(Owner::Ref { form_id, .. }) => (form_id, false),
                        Some(Owner::Actor { form_id, .. }) => (form_id, true),
                        _ => continue,
                    };
                    if self.movers.seen.insert((mover, other)) {
                        self.movers.found.push(MoverContact { mover, other, actor });
                        self.movers.stats.touched += 1;
                    }
                }
            }
        }
        // The storage is kept for the next step.
        self.movers.moving = moving;
        self.movers.moving.clear();
    }
}
