//! Buoyancy (docs/spec-notes/ragdoll.md §8): ragdoll bodies and gore pieces in water
//! ([`crate::water`]: cell surfaces and placed volumes) are lifted and slowed, so a corpse
//! that dies or sinks under water rises and floats at the surface.
//!
//! Before every step, each awake ragdoll body and gore piece in water gets, for the share
//! `f` of its height that lies under the surface (its colliders' bounding box, 0 to 1):
//!
//! - an upward change of velocity of [`FLOAT_RATIO`] × gravity × `f` per second. Gravity
//!   still pulls the whole body, so a body under water rises at (`FLOAT_RATIO` − 1) × g
//!   and floats where `f` = 1 / `FLOAT_RATIO`;
//! - water drag: its velocity and spin die out at [`WATER_DRAG`] and [`WATER_SPIN_DRAG`]
//!   per second times `f`, which stops it bobbing and caps how fast it rises.
//!
//! All three numbers are [design]: how the original floats bodies has not been captured.
//! Every body is treated as equally dense; a sleeping body is left alone (it went to sleep
//! where the lift and gravity balance).

use crate::world::PhysWorld;
use rapier3d::prelude::*;

/// How much more than gravity water lifts a fully submerged body [design: 1.2 floats a
/// body with about a sixth of its height above the surface].
pub const FLOAT_RATIO: f32 = 1.2;

/// How fast water slows a submerged body's motion and spin, per second [design: with
/// [`FLOAT_RATIO`] a body rises at most about 70 units a second].
pub const WATER_DRAG: f32 = 2.0;
pub const WATER_SPIN_DRAG: f32 = 2.0;

impl PhysWorld {
    /// The lowest and highest point of a body's colliders, game units.
    fn body_height(&self, h: RigidBodyHandle) -> Option<(f32, f32)> {
        let b = self.w.bodies.get(h)?;
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for &c in b.colliders() {
            let Some(co) = self.w.colliders.get(c) else { continue };
            let a = co.compute_aabb();
            lo = lo.min(a.mins.z / self.k);
            hi = hi.max(a.maxs.z / self.k);
        }
        (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
    }

    /// The share of a body's height under water (0 to 1), and none out of water.
    fn submerged(&self, h: RigidBodyHandle) -> Option<f32> {
        let (lo, hi) = self.body_height(h)?;
        let c = self.w.bodies.get(h)?.center_of_mass() / self.k;
        let surface = self.water_surface_for([c.x, c.y, c.z])?;
        let f = ((surface - lo) / (hi - lo).max(1e-3)).clamp(0.0, 1.0);
        (f > 0.0).then_some(f)
    }

    /// Lift and slow the ragdoll bodies and gore pieces in water, for one step.
    pub(crate) fn buoy(&mut self) {
        if (self.ragdolls.is_empty() && self.gore.is_empty()) || (self.water().next().is_none() && self.placed_water().next().is_none()) {
            return;
        }
        // In FormID then id order, for determinism.
        let handles: Vec<RigidBodyHandle> =
            self.ragdolls.values().flat_map(|r| r.bodies.iter().flatten().copied()).chain(self.gore.values().copied()).collect();
        let dt = self.params.dt;
        let lift = FLOAT_RATIO * self.params.gravity * self.k * dt;
        for h in handles {
            if !self.w.bodies.get(h).is_some_and(|b| b.is_dynamic() && !b.is_sleeping()) {
                continue;
            }
            let Some(f) = self.submerged(h) else { continue };
            let b = &mut self.w.bodies[h];
            let mut v = b.linvel();
            v.z += lift * f;
            v *= (-WATER_DRAG * f * dt).exp();
            let w = b.angvel() * (-WATER_SPIN_DRAG * f * dt).exp();
            b.set_linvel(v, true);
            b.set_angvel(w, true);
        }
    }

    /// Whether any body of the actor's ragdoll is (partly) under water.
    pub fn ragdoll_in_water(&self, form_id: u32) -> bool {
        self.ragdolls.get(&form_id).is_some_and(|r| r.bodies.iter().flatten().any(|&h| self.submerged(h).is_some()))
    }

    /// Whether a point lies under water (below the surface of the water it is in).
    pub fn under_water(&self, p: [f32; 3]) -> bool {
        self.water_surface_for(p).is_some_and(|s| p[2] < s)
    }
}
