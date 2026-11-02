//! Actor hit volumes (docs/design/combat.md §5.2, steps C3 and C8): a kinematic body per
//! placed actor with either one vertical capsule ([`PhysWorld::set_actor_volume`]) or one
//! capsule per body part ([`PhysWorld::set_actor_parts`], posed from the skeleton by the
//! caller each frame), so shots and melee rays hit NPCs and creatures, and say which part.
//!
//! - The colliders are on the Havok BIPED layer while the actor lives and DEAD_BIP once
//!   it is dead (`crate::layers`; the layer names are documented, using them for actors
//!   is our choice). Queries can skip either with `Filter::exclude_layers`.
//! - They take part in no contacts: dynamic clutter is not pushed and the character
//!   controller ignores them [design: combat volumes are for shots, not for walking].
//!
//! Sizes and bone positions come from the caller; this module keeps the shapes where it
//! is told.

use crate::refresh::{DirtyReasons, SideCollider, SIDE_MARGIN};
use crate::world::{Owner, PhysWorld};
use crate::{layers, BodyState};
use rapier3d::math::{Pose, Rotation};
use rapier3d::prelude::*;

/// A capsule standing on `feet`: `height` from the bottom of the lower cap to the top of
/// the upper cap, game units. With parts, the actor's overall extent.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct HitVolume {
    pub feet: [f32; 3],
    pub radius: f32,
    pub height: f32,
    pub dead: bool,
}

impl HitVolume {
    fn layer(&self) -> u8 {
        if self.dead {
            layers::DEAD_BIP
        } else {
            layers::BIPED
        }
    }

    /// Whether every size and position is finite (a broken skeleton or record can give
    /// NaN, which must not reach rapier).
    pub fn is_finite(&self) -> bool {
        self.feet.iter().all(|v| v.is_finite()) && self.radius.is_finite() && self.height.is_finite()
    }

    /// Centre of the capsule, game units.
    pub fn center(&self) -> [f32; 3] {
        let h = self.height.max(2.0 * self.radius);
        [self.feet[0], self.feet[1], self.feet[2] + h / 2.0]
    }
}

/// One body part's capsule: the segment `a`–`b` (game space) swept by `radius`. `part`
/// is the index of the part in its body part data (BPTD).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct PartCapsule {
    pub part: u8,
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub radius: f32,
}

impl PartCapsule {
    /// Whether its segment and radius are finite.
    pub fn is_finite(&self) -> bool {
        self.a.iter().chain(&self.b).all(|v| v.is_finite()) && self.radius.is_finite()
    }
}

/// The part index of a whole-actor capsule.
pub const WHOLE: u8 = u8::MAX;

/// An actor's body and its colliders.
#[derive(Debug, Clone)]
pub(crate) struct ActorBody {
    body: RigidBodyHandle,
    volume: HitVolume,
    /// (collider, part, half segment length, radius); one with [`WHOLE`] for the single
    /// capsule.
    colliders: Vec<(ColliderHandle, u8, f32, f32)>,
}

/// In no group and colliding with none: queries (whose filter has no groups) still see it.
const NO_CONTACTS: InteractionGroups = InteractionGroups::new(Group::NONE, Group::NONE, InteractionTestMode::And);

/// Shapes are rebuilt only when their size changes by more than this, game units.
const RESIZE: f32 = 0.5;

impl PhysWorld {
    /// Move an actor's body to `v.feet`, or add it; true when it was added.
    fn actor_body(&mut self, form_id: u32, v: HitVolume) -> bool {
        let k = self.k;
        let f = Vector::new(v.feet[0], v.feet[1], v.feet[2]) * k;
        if let Some(a) = self.actors.get(&form_id) {
            let b = a.body;
            if let Some(body) = self.w.bodies.get_mut(b) {
                body.set_translation(f, true);
            }
            return false;
        }
        let owner = Owner::Actor { form_id, layer: v.layer(), part: WHOLE };
        let body = self.insert_body(RigidBodyBuilder::kinematic_position_based().translation(f).user_data(owner.encode()));
        self.actors.insert(form_id, ActorBody { body, volume: v, colliders: Vec::new() });
        true
    }

    /// Replace an actor's colliders when their number or parts differ from `want`; true
    /// when they were replaced.
    fn ensure_colliders(&mut self, form_id: u32, want: &[(u8, f32, f32)]) -> bool {
        let Some(a) = self.actors.get(&form_id) else { return false };
        let same = a.colliders.len() == want.len() && a.colliders.iter().zip(want).all(|(c, w)| c.1 == w.0);
        if same {
            return false;
        }
        let body = a.body;
        let old: Vec<ColliderHandle> = a.colliders.iter().map(|c| c.0).collect();
        for c in old {
            self.w.remove_collider(c);
        }
        let k = self.k;
        let mut cols = Vec::new();
        for &(part, half, radius) in want {
            let co = ColliderBuilder::capsule_z(half * k, radius.max(1.0) * k).collision_groups(NO_CONTACTS).solver_groups(NO_CONTACTS);
            let h = self.w.insert_collider(co, Some(body));
            cols.push((h, part, half, radius));
        }
        if let Some(a) = self.actors.get_mut(&form_id) {
            a.colliders = cols;
        }
        true
    }

    /// Note an actor's hit colliders as changed: moved only (the refresh can update them
    /// without a detection pass, `crate::refresh`), or added or replaced (it cannot).
    fn actor_changed(&mut self, form_id: u32, reason: DirtyReasons, rebuilt: bool) {
        if rebuilt {
            self.mark_dirty(reason | DirtyReasons::ACTOR_REBUILT);
        } else {
            self.mark_dirty(reason);
            self.poses.touch(form_id);
        }
    }

    /// Bring the world poses of the actor hit colliders moved since the last refresh up
    /// to date for queries, without a detection pass (`crate::refresh`): each collider's
    /// pose becomes its body's pose times its parent-relative pose, the same product
    /// rapier's next pass computes for it before reading it [design:
    /// `handle_user_changes_to_colliders` and `RigidBodyColliders::update_positions`,
    /// rapier3d 0.36.0], and it is kept aside for the queries. Its change flags are left
    /// for that next pass (the PARENT flag `set_position_wrt_parent` set already makes
    /// that pass recompute the pose and add the POSITION flag `set_position` adds here):
    /// the broad phase, narrow phase and islands are left untouched, their processing
    /// deferred to the next step or pass. False, with nothing written, when a pose is not
    /// finite: the caller then runs the full pass.
    pub(crate) fn refresh_actor_poses(&mut self) -> bool {
        let mut touched = std::mem::take(&mut self.poses.touched);
        touched.sort_unstable();
        touched.dedup();
        let finite = touched.iter().all(|id| {
            let Some(a) = self.actors.get(id) else { return true };
            let Some(body) = self.w.bodies.get(a.body) else { return true };
            let parent = *body.position();
            a.colliders.iter().all(|c| self.w.colliders.get(c.0).and_then(|co| co.position_wrt_parent()).is_none_or(|local| (parent * *local).is_finite()))
        });
        if !finite {
            self.poses.touched = touched;
            return false;
        }
        let mut fresh = Vec::new();
        for &id in &touched {
            // An actor removed since it was posed is a `BODIES` change: not this path.
            let Some(a) = self.actors.get(&id) else { continue };
            let Some(body) = self.w.bodies.get(a.body) else { continue };
            let parent = *body.position();
            for (slot, c) in a.colliders.iter().enumerate() {
                let Some(co) = self.w.colliders.get_mut(c.0) else { continue };
                let Some(local) = co.position_wrt_parent().copied() else { continue };
                co.set_position(parent * local);
                let aabb = co.compute_aabb().loosened(SIDE_MARGIN);
                fresh.push(SideCollider { form_id: id, slot: slot as u16, handle: c.0, aabb });
            }
        }
        self.poses.replace(&touched, fresh);
        touched.clear();
        self.poses.touched = touched;
        true
    }

    /// Add the single hit capsule of actor reference `form_id`, or move and resize it
    /// (nothing changes when `v` is what it already is).
    pub fn set_actor_volume(&mut self, form_id: u32, v: HitVolume) {
        // A non-finite volume is ignored: the actor keeps the one it had (or none).
        if !v.is_finite() {
            return;
        }
        if self.actors.get(&form_id).is_some_and(|a| a.volume == v && a.colliders.len() == 1 && a.colliders[0].1 == WHOLE) {
            return;
        }
        let k = self.k;
        let radius = v.radius.max(1.0);
        let half = (v.height.max(2.0 * radius) / 2.0 - radius).max(0.0);
        let added = self.actor_body(form_id, v);
        let rebuilt = self.ensure_colliders(form_id, &[(WHOLE, half, radius)]);
        let owner = Owner::Actor { form_id, layer: v.layer(), part: WHOLE }.encode();
        let c = v.center();
        let local = Pose::from_translation((Vector::new(c[0], c[1], c[2]) - Vector::new(v.feet[0], v.feet[1], v.feet[2])) * k);
        let a = self.actors.get_mut(&form_id).expect("added");
        a.volume = v;
        let (h, _, old_half, old_r) = a.colliders[0];
        a.colliders[0].2 = half;
        a.colliders[0].3 = radius;
        if let Some(co) = self.w.colliders.get_mut(h) {
            co.user_data = owner;
            co.set_position_wrt_parent(local);
            if (old_half - half).abs() > RESIZE || (old_r - radius).abs() > RESIZE {
                co.set_shape(SharedShape::capsule_z(half * k, radius * k));
            }
        }
        self.actor_changed(form_id, DirtyReasons::ACTOR_VOLUME, added || rebuilt);
    }

    /// One capsule per body part (step C8): `v` is the actor's overall extent (its body
    /// stands at `v.feet`); each part's capsule is placed on its segment.
    /// Parts whose segment or radius is not finite are left out.
    pub fn set_actor_parts(&mut self, form_id: u32, v: HitVolume, parts: &[PartCapsule]) {
        let finite: Vec<PartCapsule>;
        let parts = if parts.iter().all(PartCapsule::is_finite) {
            parts
        } else {
            finite = parts.iter().filter(|p| p.is_finite()).copied().collect();
            &finite[..]
        };
        if parts.is_empty() || !v.is_finite() {
            return self.set_actor_volume(form_id, v);
        }
        let k = self.k;
        let feet = Vector::new(v.feet[0], v.feet[1], v.feet[2]);
        let want: Vec<(u8, f32, f32)> = parts
            .iter()
            .map(|p| {
                let len = (Vector::from(p.b) - Vector::from(p.a)).length();
                (p.part, (len / 2.0).max(0.0), p.radius)
            })
            .collect();
        let locals: Vec<Pose> = parts
            .iter()
            .map(|p| {
                let (pa, pb) = (Vector::from(p.a), Vector::from(p.b));
                let mid = (pa + pb) * 0.5;
                let dir = (pb - pa).try_normalize().unwrap_or(Vector::Z);
                Pose::from_parts((mid - feet) * k, Rotation::from_rotation_arc(Vector::Z, dir))
            })
            .collect();
        let layer = v.layer();
        // Read before anything is written: the writes below go ahead either way, so rapier
        // sees the calls it always did; only the refresh is skipped when they change
        // nothing. (The writes put the colliders in rapier's modified set, from which the
        // next pass builds its filtered broad-phase update workspace, whose size chooses
        // how leaves are updated: `BroadPhaseBvh::update`, rapier3d 0.36.0
        // broad_phase_bvh/update.rs ~79-144.)
        let unchanged = self.parts_unchanged(form_id, v, parts, &want, &locals);
        let added = self.actor_body(form_id, v);
        let rebuilt = self.ensure_colliders(form_id, &want);
        let a = self.actors.get_mut(&form_id).expect("added");
        a.volume = v;
        for (i, p) in parts.iter().enumerate() {
            let (h, _, old_half, old_r) = a.colliders[i];
            let local = locals[i];
            let half = want[i].1;
            a.colliders[i].2 = half;
            a.colliders[i].3 = p.radius;
            if let Some(co) = self.w.colliders.get_mut(h) {
                co.user_data = Owner::Actor { form_id, layer, part: p.part }.encode();
                co.set_position_wrt_parent(local);
                if (old_half - half).abs() > RESIZE || (old_r - p.radius).abs() > RESIZE {
                    co.set_shape(SharedShape::capsule_z(half * k, p.radius.max(1.0) * k));
                }
            }
        }
        if !unchanged {
            self.actor_changed(form_id, DirtyReasons::ACTOR_PARTS, added || rebuilt);
        }
    }

    /// Whether [`PhysWorld::set_actor_parts`] with these inputs would leave every stored
    /// value as it is (exact equality, no tolerance): the same overall volume, body
    /// translation, parts in the same order, stored sizes (so no shape is rebuilt and the
    /// `RESIZE` comparisons of later calls are unchanged), parent-relative poses and
    /// owners. Then the broad phase, the colliders' world poses and every query result
    /// are what they were after the last refresh or step, and no refresh is needed
    /// [design: rapier recomputes a collider's world pose only from its body's pose and
    /// its parent-relative pose, `handle_user_changes_to_colliders` /
    /// `RigidBodyColliders::update_positions`, rapier3d 0.36.0].
    fn parts_unchanged(&self, form_id: u32, v: HitVolume, parts: &[PartCapsule], want: &[(u8, f32, f32)], locals: &[Pose]) -> bool {
        let Some(a) = self.actors.get(&form_id) else { return false };
        if a.volume != v || a.colliders.len() != parts.len() {
            return false;
        }
        let Some(body) = self.w.bodies.get(a.body) else { return false };
        if body.translation() != Vector::new(v.feet[0], v.feet[1], v.feet[2]) * self.k {
            return false;
        }
        let layer = v.layer();
        a.colliders.iter().zip(parts).zip(want).zip(locals).all(|(((&(h, part, half, radius), p), w), local)| {
            part == p.part
                && half == w.1
                && radius == p.radius
                && self.w.colliders.get(h).is_some_and(|co| {
                    co.user_data == Owner::Actor { form_id, layer, part: p.part }.encode() && co.position_wrt_parent() == Some(local)
                })
        })
    }

    /// Remove an actor's hit volume (it left the loaded area or was disabled).
    pub fn remove_actor_volume(&mut self, form_id: u32) -> bool {
        let Some(a) = self.actors.remove(&form_id) else { return false };
        self.remove_body(a.body);
        true
    }

    /// The actors that have a hit volume, by FormID.
    pub fn actor_volumes(&self) -> impl Iterator<Item = u32> + '_ {
        self.actors.keys().copied()
    }

    /// An actor's hit volume (its overall extent) as last set.
    pub fn actor_volume(&self, form_id: u32) -> Option<HitVolume> {
        self.actors.get(&form_id).map(|a| a.volume)
    }

    /// How many part capsules an actor has (0 with the single capsule).
    pub fn actor_part_count(&self, form_id: u32) -> usize {
        self.actors.get(&form_id).map_or(0, |a| a.colliders.iter().filter(|c| c.1 != WHOLE).count())
    }

    /// A point to aim at on an actor: with parts, the centre of its widest part capsule
    /// (the torso of a person), the first of equally wide ones (a part's own chain comes
    /// before its side branches, which share its radius: a creature's legs), else the
    /// middle of its capsule. Game units.
    pub fn actor_aim_point(&self, form_id: u32) -> Option<[f32; 3]> {
        let a = self.actors.get(&form_id)?;
        let widest = a.colliders.iter().filter(|c| c.1 != WHOLE).reduce(|best, c| if c.3 > best.3 { c } else { best });
        match widest.and_then(|c| self.w.colliders.get(c.0)) {
            Some(co) => {
                let t = co.position().translation / self.k;
                Some([t.x, t.y, t.z])
            }
            None => Some(a.volume.center()),
        }
    }

    /// The centres of an actor's body part capsules, widest first (none without parts),
    /// game units.
    pub fn actor_part_centers(&self, form_id: u32) -> Vec<[f32; 3]> {
        let Some(a) = self.actors.get(&form_id) else { return Vec::new() };
        let mut caps: Vec<(f32, [f32; 3])> = a
            .colliders
            .iter()
            .filter(|c| c.1 != WHOLE)
            .filter_map(|c| self.w.colliders.get(c.0).map(|co| (c.3, co.position().translation / self.k)))
            .map(|(r, t)| (r, [t.x, t.y, t.z]))
            .collect();
        caps.sort_by(|x, y| y.0.total_cmp(&x.0));
        caps.into_iter().map(|c| c.1).collect()
    }

    /// The centre of an actor's first capsule for body part `part`, game units.
    pub fn actor_part_center(&self, form_id: u32, part: u8) -> Option<[f32; 3]> {
        let c = self.actors.get(&form_id)?.colliders.iter().find(|c| c.1 == part && part != WHOLE)?;
        let t = self.w.colliders.get(c.0)?.position().translation / self.k;
        Some([t.x, t.y, t.z])
    }

    /// The body state of an actor's hit volume.
    pub fn actor_volume_state(&self, form_id: u32) -> Option<BodyState> {
        let a = self.actors.get(&form_id)?;
        Some(crate::world::body_state(&self.w.bodies[a.body], self.k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Filter, PhysParams};

    fn capsules(x: f32) -> Vec<PartCapsule> {
        vec![
            PartCapsule { part: 0, a: [x, 0.0, 40.0], b: [x, 0.0, 100.0], radius: 15.0 },
            PartCapsule { part: 1, a: [x, 100.0, 40.0], b: [x, 100.0, 100.0], radius: 15.0 },
        ]
    }

    /// A collider rapier disabled (its quarantine does, for a non-finite collider) has no
    /// broad-phase leaf after a detection pass, so no query meets it; reposed and
    /// refreshed without a pass, it must not be met either.
    #[test]
    fn a_disabled_actor_collider_reposed_is_not_hit_after_a_refresh_without_a_pass() {
        let mut w = PhysWorld::new(PhysParams::default());
        let v = |x: f32| HitVolume { feet: [x, 0.0, 0.0], radius: 20.0, height: 128.0, dead: false };
        w.set_actor_parts(0x10, v(200.0), &capsules(200.0));
        w.step();
        let disabled = w.actors[&0x10].colliders[0].0;
        w.w.colliders[disabled].set_enabled(false);
        w.set_actor_parts(0x10, v(210.0), &capsules(210.0));
        let mut full = w.snapshot();
        w.update_queries();
        full.update_queries_full();
        assert_eq!(w.refresh_stats().fast, 1);
        let f = Filter::default();
        for (o, d) in [([0.0, 0.0, 70.0], [1.0, 0.0, 0.0]), ([210.0, 0.0, 300.0], [0.0, 0.0, -1.0]), ([0.0, 100.0, 70.0], [1.0, 0.0, 0.0])] {
            let (qa, qb) = (w.queries(), full.queries());
            let (a, b) = (qa.ray(o, d, 1_000.0, &f), qb.ray(o, d, 1_000.0, &f));
            assert_eq!(a, b, "{o:?}");
            assert!(!matches!(a, Some(h) if h.owner == Owner::Actor { form_id: 0x10, layer: layers::BIPED, part: 0 }), "{a:?}");
            assert_eq!(qa.ray_all(o, d, 1_000.0, &f), qb.ray_all(o, d, 1_000.0, &f));
        }
        // The enabled part is still met.
        let hit = w.queries().ray([0.0, 100.0, 70.0], [1.0, 0.0, 0.0], 1_000.0, &f).expect("part 1");
        assert_eq!(hit.owner, Owner::Actor { form_id: 0x10, layer: layers::BIPED, part: 1 });
    }
}
