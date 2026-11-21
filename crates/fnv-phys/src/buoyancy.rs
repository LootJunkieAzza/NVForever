//! Buoyancy (docs/spec-notes/ragdoll.md §8): dynamic bodies in water ([`crate::water`]:
//! cell surfaces and placed volumes) are lifted and slowed. Ragdoll bodies and gore pieces
//! rise and float at the surface; a loose item floats or sinks by its Havok material.
//!
//! Before every step, each awake dynamic body in water gets, for the share `f` of its
//! height that lies under the surface (its colliders' bounding box, 0 to 1):
//!
//! - an upward change of velocity of `lift` × gravity × `f` per second, where `lift` is
//!   [`FLOAT_RATIO`] for a body that floats and [`SINK_LIFT`] for one that sinks. Gravity
//!   still pulls the whole body, so a floating body under water rises at
//!   (`FLOAT_RATIO` − 1) × g and floats where `f` = 1 / `FLOAT_RATIO`;
//! - water drag: its velocity and spin die out at [`WATER_DRAG`] and [`WATER_SPIN_DRAG`]
//!   per second times `f`, which stops it bobbing and caps how fast it rises or sinks.
//!
//! What floats:
//! - ragdoll bodies and gore pieces: the owner's choice (2026-10-25). The original sinks
//!   corpses [observed: an NCR trooper killed 150 units under Lake Mead lay on the bed
//!   within 1 s, three runs, captures fl_items_lake and fl_heavy_lake, 2026-10-25].
//! - a loose item whose first collision shape's Havok material is one of
//!   [`FLOATING_MATERIALS`]; every other item sinks (the same captures).
//!
//! All three numbers are [design] or fitted roughly to those captures.

use crate::layers;
use crate::world::{Owner, PhysWorld};
use rapier3d::prelude::*;

/// How much more than gravity water lifts a fully submerged floating body [design: 1.2
/// floats a body with about a sixth of its height above the surface; in the original an
/// item floated with its origin 0-2 units under the surface].
pub const FLOAT_RATIO: f32 = 1.2;

/// How much of gravity water takes off a sinking body [fitted: with [`WATER_DRAG`] it
/// sinks at about 170 units a second, as the original's sensor module and toaster did,
/// fl_heavy_lake; its hot plate sank slower, about 45].
pub const SINK_LIFT: f32 = 0.5;

/// How fast water slows a submerged body's motion and spin, per second [design: with
/// [`FLOAT_RATIO`] a floating body rises at most about 70 units a second; the original's
/// items rose at 20-60].
pub const WATER_DRAG: f32 = 2.0;
pub const WATER_SPIN_DRAG: f32 = 2.0;

/// The Havok materials (numbered as nif.xml's `Fallout3HavokMaterial`) of loose items that
/// float. [observed, fl_items_lake and fl_heavy_lake: wood (pencil, sledgehammer), cloth
/// (teddy bear, Pre-War book, Wonderglue), dirt (cigarette carton), bottle (whiskey
/// bottles, Nuka-Cola), soda can (tin can) floated; metal (sensor module, toaster) and
/// glass (hot plate) sank]. Organic, grass, baby rattle and rubber ball float too
/// [hypothesis: not captured; light materials by their names].
pub const FLOATING_MATERIALS: [u32; 9] = [CLOTH, DIRT, GRASS, ORGANIC, WOOD, BOTTLE, SODA_CAN, BABY_RATTLE, RUBBER_BALL];

const CLOTH: u32 = 1;
const DIRT: u32 = 2;
const GRASS: u32 = 4;
const ORGANIC: u32 = 6;
const WOOD: u32 = 9;
const BOTTLE: u32 = 24;
const SODA_CAN: u32 = 25;
const BABY_RATTLE: u32 = 30;
const RUBBER_BALL: u32 = 31;

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

    /// The Havok material of a body's first collider.
    fn body_material(&self, b: &RigidBody) -> Option<u32> {
        b.colliders().first().and_then(|c| self.materials.get(c)).and_then(|m| m.first()).copied()
    }

    /// How much of gravity water lifts this body: ragdolls and gore float, loose items by
    /// their material, others (characters, kinematic bodies) not at all.
    fn lift_of(&self, b: &RigidBody) -> Option<f32> {
        match Owner::decode(b.user_data)? {
            Owner::Actor { layer: layers::DEAD_BIP, .. } => Some(FLOAT_RATIO),
            Owner::Ref { .. } => Some(if self.body_material(b).is_some_and(|m| FLOATING_MATERIALS.contains(&m)) { FLOAT_RATIO } else { SINK_LIFT }),
            _ => None,
        }
    }

    /// Lift and slow the awake dynamic bodies in water, for one step.
    pub(crate) fn buoy(&mut self) {
        if self.water().next().is_none() && self.placed_water().next().is_none() {
            return;
        }
        let dt = self.params.dt;
        let g = self.params.gravity * self.k * dt;
        let mut changes = Vec::new();
        for h in self.w.islands.active_bodies() {
            let Some(b) = self.w.bodies.get(h).filter(|b| b.is_dynamic() && !b.is_sleeping()) else { continue };
            let Some(lift) = self.lift_of(b) else { continue };
            if let Some(f) = self.submerged(h) {
                changes.push((h, lift, f));
            }
        }
        for (h, lift, f) in changes {
            let b = &mut self.w.bodies[h];
            let mut v = b.linvel();
            v.z += lift * g * f;
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

    /// The placed references with a dynamic body in water, by FormID: whether its
    /// material floats and how far the top of its body is above the surface (game units;
    /// below it, negative). For reports.
    pub fn loose_in_water(&self) -> Vec<(u32, bool, f32)> {
        let mut out = Vec::new();
        for (&id, bodies) in &self.refs {
            for &h in bodies {
                let Some(b) = self.w.bodies.get(h).filter(|b| b.is_dynamic()) else { continue };
                let (Some(lift), Some(_), Some((_, hi))) = (self.lift_of(b), self.submerged(h), self.body_height(h)) else { continue };
                let c = b.center_of_mass() / self.k;
                let Some(surface) = self.water_surface_for([c.x, c.y, c.z]) else { continue };
                out.push((id, lift == FLOAT_RATIO, hi - surface));
            }
        }
        out
    }

    /// Whether a point lies under water (below the surface of the water it is in).
    pub fn under_water(&self, p: [f32; 3]) -> bool {
        self.water_surface_for(p).is_some_and(|s| p[2] < s)
    }
}
