//! Holding a body: the player's grab (docs/spec-notes/grab.md).
//!
//! One body at a time is held by a point on it ([`Grip`]): a loose object's dynamic body
//! (a placed or dropped reference whose collision is one dynamic body) or one body of a
//! corpse's ragdoll. Before every fixed step ([`PhysWorld::step`]) the held body's
//! velocity is set so that the grip point closes on the target the game gives
//! ([`PhysWorld::set_grip_target`]) at [`HoldParams::stiffness`] of the gap a second,
//! no faster than [`HoldParams::max_speed`], with the step's fall given back and the
//! change of velocity bounded by [`HoldParams::max_force`] over the body's mass; its spin
//! dies away by [`HoldParams::spin_kept`] a second. It stays a dynamic body throughout,
//! so walls and floors stop it and it pushes other loose objects [hypothesis: the
//! original holds the object with a spring (its settings `fZKeySpringElasticity`,
//! `fZKeySpringDamping`, `fZKeyMaxForce`); the velocity form here is an engineering
//! choice that needs no tuning per mass].
//!
//! The grip names its body by the reference and the body's index, not by a rapier
//! handle: a ragdoll held at rest is built anew when it is roused, and a reference's
//! collision can be rebuilt; a grip whose body is gone is dropped at the next step
//! ([`PhysWorld::holding`] then answers `None`).

use crate::convert::{arr3, vec3};
use crate::world::PhysWorld;
use rapier3d::prelude::*;

/// What a grip holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GripOn {
    /// A reference's dynamic body (loose clutter, a dropped item).
    Ref(u32),
    /// A body of the ragdoll of the actor with this reference (a corpse).
    Ragdoll(u32),
}

impl GripOn {
    pub fn form_id(self) -> u32 {
        match self {
            GripOn::Ref(id) | GripOn::Ragdoll(id) => id,
        }
    }
}

/// How a held body follows its target. Every value is [hypothesis] (grab.md §3).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct HoldParams {
    /// Share of the gap between the grip point and the target closed a second (1/s).
    pub stiffness: f32,
    /// Fastest the held body is moved, game units a second.
    pub max_speed: f32,
    /// Largest force the hold applies, mass (kg) × game units / s²: the change of
    /// velocity in a step is at most this ÷ mass × the step.
    pub max_force: f32,
    /// Share of the held body's spin kept after a second.
    pub spin_kept: f32,
}

impl Default for HoldParams {
    fn default() -> Self {
        HoldParams { stiffness: 12.0, max_speed: 700.0, max_force: 100_000.0, spin_kept: 0.05 }
    }
}

/// A held body.
#[derive(Debug, Clone, PartialEq)]
pub struct Grip {
    pub on: GripOn,
    /// The body's index among the reference's or the ragdoll's bodies.
    pub body: usize,
    /// The grip point in the body's frame, metres (rapier's unit).
    local: [f32; 3],
    /// The body's mass (kg, as the model stores it).
    pub mass: f32,
    /// Where the grip point is wanted, game units.
    pub target: [f32; 3],
    pub params: HoldParams,
}

impl PhysWorld {
    /// The body `on` would be held by at `at` (game units): the reference's dynamic body
    /// nearest to it, or the ragdoll body nearest to it; its index and handle.
    fn grip_body(&self, on: GripOn, at: [f32; 3]) -> Option<(usize, RigidBodyHandle)> {
        let p = vec3(at) * self.k;
        let handles: Vec<Option<RigidBodyHandle>> = match on {
            GripOn::Ref(id) => self.refs.get(&id)?.iter().map(|&b| Some(b)).collect(),
            GripOn::Ragdoll(id) => self.ragdolls.get(&id)?.bodies.clone(),
        };
        handles
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                let b = (*b)?;
                let body = self.w.bodies.get(b).filter(|x| x.is_dynamic() || matches!(on, GripOn::Ragdoll(_)))?;
                Some((i, b, (body.center_of_mass() - p).length_squared()))
            })
            .min_by(|x, y| x.2.total_cmp(&y.2))
            .map(|(i, b, _)| (i, b))
    }

    /// The body at index `body` of what `on` names, now.
    fn grip_handle(&self, on: GripOn, body: usize) -> Option<RigidBodyHandle> {
        match on {
            GripOn::Ref(id) => self.refs.get(&id)?.get(body).copied(),
            GripOn::Ragdoll(id) => *self.ragdolls.get(&id)?.bodies.get(body)?,
        }
    }

    /// The mass (kg) of the body a grab of `on` at `at` would hold, `None` when there is
    /// nothing to hold (no dynamic body, no ragdoll). A ragdoll held at rest answers with
    /// its fixed bodies' stored mass.
    pub fn grip_mass(&self, on: GripOn, at: [f32; 3]) -> Option<f32> {
        let (i, b) = self.grip_body(on, at)?;
        let m = self.w.bodies.get(b)?.mass();
        if m.is_finite() && m > 0.0 {
            return Some(m);
        }
        // A ragdoll at rest has fixed bodies: the mass stored for the body.
        match on {
            GripOn::Ragdoll(id) => self.ragdolls.get(&id)?.data.bodies.get(i).map(|d| d.object.mass),
            GripOn::Ref(_) => None,
        }
    }

    /// Hold `on` by the point `at` (game units, on or in its body), replacing any grip.
    /// A ragdoll held at rest moves again. Returns the held body's mass, `None` when there
    /// is nothing to hold.
    pub fn grab(&mut self, on: GripOn, at: [f32; 3], params: HoldParams) -> Option<f32> {
        self.grip = None;
        if let GripOn::Ragdoll(id) = on {
            self.rouse_ragdoll(id);
        }
        let (i, b) = self.grip_body(on, at)?;
        let body = self.w.bodies.get_mut(b)?;
        let m = body.mass();
        if !(m.is_finite() && m > 0.0) {
            return None;
        }
        let pose = *body.position();
        let local = pose.rotation.inverse() * (vec3(at) * self.k - pose.translation);
        body.wake_up(true);
        // A query before the next step must not lose the wake-up (`update_queries`).
        self.struck.push(b);
        self.grip = Some(Grip { on, body: i, local: arr3(local), mass: m, target: at, params });
        Some(m)
    }

    /// Where the held body's grip point is wanted now (game units).
    pub fn set_grip_target(&mut self, target: [f32; 3]) {
        if let Some(g) = self.grip.as_mut() {
            g.target = target;
        }
    }

    /// The grip, while a body is held and still there.
    pub fn holding(&self) -> Option<&Grip> {
        let g = self.grip.as_ref()?;
        self.grip_handle(g.on, g.body)?;
        Some(g)
    }

    /// The held body's grip point now (game units).
    pub fn grip_point(&self) -> Option<[f32; 3]> {
        let g = self.grip.as_ref()?;
        let body = self.w.bodies.get(self.grip_handle(g.on, g.body)?)?;
        let pose = body.position();
        Some(arr3((pose.rotation * vec3(g.local) + pose.translation) / self.k))
    }

    /// The held body's velocity now (game units a second).
    pub fn grip_velocity(&self) -> Option<[f32; 3]> {
        let g = self.grip.as_ref()?;
        let body = self.w.bodies.get(self.grip_handle(g.on, g.body)?)?;
        Some(arr3(body.linvel() / self.k))
    }

    /// Let go of the held body; with `throw`, its velocity becomes that (game units a
    /// second). Returns the grip that was let go.
    pub fn release_grip(&mut self, throw: Option<[f32; 3]>) -> Option<Grip> {
        let g = self.grip.take()?;
        if let Some(b) = self.grip_handle(g.on, g.body) {
            if let Some(body) = self.w.bodies.get_mut(b) {
                if let Some(v) = throw {
                    body.set_linvel(vec3(v) * self.k, true);
                }
                body.wake_up(true);
                self.struck.push(b);
            }
        }
        Some(g)
    }

    /// Set the held body's velocity for the next step (see the module notes). A grip
    /// whose body is gone is dropped.
    pub(crate) fn drive_grip(&mut self) {
        let Some(g) = self.grip.clone() else { return };
        let Some(b) = self.grip_handle(g.on, g.body) else {
            self.grip = None;
            return;
        };
        let (k, dt) = (self.k, self.params.dt);
        let up = -self.w.gravity * dt;
        let Some(body) = self.w.bodies.get_mut(b) else {
            self.grip = None;
            return;
        };
        if !body.is_dynamic() {
            return;
        }
        let pose = *body.position();
        let at = pose.rotation * vec3(g.local) + pose.translation;
        let gap = vec3(g.target) * k - at;
        let p = g.params;
        let want = (gap * p.stiffness).clamp_length_max(p.max_speed * k);
        let now = body.linvel();
        let max_dv = p.max_force / g.mass.max(1e-3) * dt * k;
        let v = now + (want - now).clamp_length_max(max_dv) + up;
        body.set_linvel(v, true);
        let spin = body.angvel() * p.spin_kept.clamp(0.0, 1.0).powf(dt);
        body.set_angvel(spin, true);
    }
}
