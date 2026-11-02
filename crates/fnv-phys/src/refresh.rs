//! Why scene queries had to be brought up to date between steps
//! ([`crate::PhysWorld::update_queries`]): every change that leaves the broad phase stale
//! says what it was ([`DirtyReasons`]), the reasons accumulate until the next refresh, and
//! each refresh is counted by the combination of reasons that called for it
//! ([`RefreshStats`]).
//!
//! When the only changes since the last refresh are moved actor hit colliders (their
//! body or parent-relative poses, their shapes or owners; no collider or body added,
//! removed or rebuilt, nothing else), the refresh skips the collision detection pass:
//! it writes those colliders' world poses and keeps them aside ([`ActorPoses`]), and the
//! scene queries test them directly instead of through their stale broad-phase leaves
//! (`crate::world::Queries`). The colliders keep their change flags, so the next step or
//! detection pass updates the broad phase from them as it would have without a query.
//!
//! Eligible actor-only query refreshes skip collision detection and leave deferred
//! processing to the next step. This deliberately changes contact-refresh timing versus
//! the earlier behaviour (the detection pass that ran before most first steps of a frame,
//! rebuilding the contacts of awake pairs at a zero time step without clustering); mixed
//! changes still run detection. Exact-tie selection may change (below).
//!
//! Exact ties: the queries search the broad phase first, then test the kept-aside
//! colliders in their kept order (by actor FormID, then by the order of the actor's
//! capsules), and a kept-aside hit replaces the best so far only when strictly nearer. So
//! at exactly the same distance a broad-phase hit beats a kept-aside collider, and an
//! earlier kept-aside collider beats a later one. A full pass decides the same tie by the
//! order its tree traversal meets the colliders, so an exact-distance tie may select a
//! different collider than the full pass would, distance-0 ties included (a ray or sweep
//! starting inside overlapping shapes). `ray_all` and `overlap` return every hit, sorted.

use rapier3d::parry::bounding_volume::Aabb;
use rapier3d::prelude::ColliderHandle;
use std::collections::BTreeMap;
use std::fmt;

/// A set of reasons the broad phase is stale (a bit set; [`DirtyReasons::NONE`] when it is
/// up to date).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DirtyReasons(u16);

impl DirtyReasons {
    pub const NONE: DirtyReasons = DirtyReasons(0);
    /// An actor's body part capsules were set ([`crate::PhysWorld::set_actor_parts`]).
    pub const ACTOR_PARTS: DirtyReasons = DirtyReasons(1 << 0);
    /// An actor's single hit capsule was set ([`crate::PhysWorld::set_actor_volume`]).
    pub const ACTOR_VOLUME: DirtyReasons = DirtyReasons(1 << 1);
    /// Bodies were added or removed (references, actor volumes, characters).
    pub const BODIES: DirtyReasons = DirtyReasons(1 << 2);
    /// A keyframed body (a door) was moved or made solid or not.
    pub const DOORS: DirtyReasons = DirtyReasons(1 << 3);
    /// A character was added.
    pub const CHARACTERS: DirtyReasons = DirtyReasons(1 << 4);
    /// A body was placed by setting its position (a teleport).
    pub const PLACED: DirtyReasons = DirtyReasons(1 << 5);
    /// A ragdoll was built or cut.
    pub const RAGDOLLS: DirtyReasons = DirtyReasons(1 << 6);
    /// A gore piece was added.
    pub const GORE: DirtyReasons = DirtyReasons(1 << 7);
    /// Terrain was added or removed.
    pub const TERRAIN: DirtyReasons = DirtyReasons(1 << 8);
    /// Bodies added asleep need their first contacts before a step.
    pub const SLEEP: DirtyReasons = DirtyReasons(1 << 9);
    /// An actor's hit volume body or capsules were added or replaced (with
    /// [`DirtyReasons::ACTOR_PARTS`] or [`DirtyReasons::ACTOR_VOLUME`]).
    pub const ACTOR_REBUILT: DirtyReasons = DirtyReasons(1 << 10);
    /// What a refresh without a collision detection pass can bring up to date: actor hit
    /// colliders that moved, nothing added, removed or rebuilt.
    pub const ACTOR_POSES: DirtyReasons = DirtyReasons(DirtyReasons::ACTOR_PARTS.0 | DirtyReasons::ACTOR_VOLUME.0);

    const NAMES: [(DirtyReasons, &'static str); 11] = [
        (DirtyReasons::ACTOR_PARTS, "actor_parts"),
        (DirtyReasons::ACTOR_VOLUME, "actor_volume"),
        (DirtyReasons::BODIES, "bodies"),
        (DirtyReasons::DOORS, "doors"),
        (DirtyReasons::CHARACTERS, "characters"),
        (DirtyReasons::PLACED, "placed"),
        (DirtyReasons::RAGDOLLS, "ragdolls"),
        (DirtyReasons::GORE, "gore"),
        (DirtyReasons::TERRAIN, "terrain"),
        (DirtyReasons::SLEEP, "sleep"),
        (DirtyReasons::ACTOR_REBUILT, "actor_rebuilt"),
    ];

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every reason in `other` is in `self`.
    pub fn contains(self, other: DirtyReasons) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether every reason in `self` is in `other`.
    pub fn is_within(self, other: DirtyReasons) -> bool {
        self.0 & !other.0 == 0
    }

    pub fn insert(&mut self, other: DirtyReasons) {
        self.0 |= other.0;
    }

    pub fn bits(self) -> u16 {
        self.0
    }

    /// The names of the reasons in the set, in a fixed order.
    pub fn names(self) -> impl Iterator<Item = &'static str> {
        DirtyReasons::NAMES.into_iter().filter(move |(r, _)| self.contains(*r)).map(|(_, n)| n)
    }
}

impl std::ops::BitOr for DirtyReasons {
    type Output = DirtyReasons;
    fn bitor(self, other: DirtyReasons) -> DirtyReasons {
        DirtyReasons(self.0 | other.0)
    }
}

impl fmt::Display for DirtyReasons {
    /// `actor_parts+doors`, or `none`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("none");
        }
        for (i, n) in self.names().enumerate() {
            if i > 0 {
                f.write_str("+")?;
            }
            f.write_str(n)?;
        }
        Ok(())
    }
}

/// Counts of the refreshes [`crate::PhysWorld::update_queries`] ran since the world was
/// made. A map entry is added only the first time a combination of reasons is seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefreshStats {
    /// Full collision detection passes.
    pub full: u64,
    /// Refreshes that only moved actor hit colliders, without a detection pass.
    pub fast: u64,
    /// Full refreshes by the combination of reasons that called for them.
    pub by_reasons: BTreeMap<DirtyReasons, u64>,
}

impl RefreshStats {
    pub(crate) fn record_full(&mut self, reasons: DirtyReasons) {
        self.full += 1;
        *self.by_reasons.entry(reasons).or_default() += 1;
    }

    pub(crate) fn record_fast(&mut self) {
        self.fast += 1;
    }
}

/// An actor hit collider brought up to date by a refresh without a detection pass: its
/// world pose is current, its broad-phase leaf is not.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SideCollider {
    pub form_id: u32,
    /// Its place in the actor's capsule list (the order the caller gave the parts in).
    pub slot: u16,
    pub handle: ColliderHandle,
    /// Its world AABB at the current pose, loosened by [`SIDE_MARGIN`] (a prefilter for
    /// the queries' direct tests; the shape test decides).
    pub aabb: Aabb,
}

/// How much the AABB of a [`SideCollider`] is loosened, metres: a prefilter must never
/// turn away a hit the shape test finds (the broad phase's own leaves are loosened by
/// the contact skin, prediction and change-detection margins for the same reason).
pub(crate) const SIDE_MARGIN: f32 = 0.05;

/// The actor hit colliders that moved since the last detection pass or step.
#[derive(Debug, Clone, Default)]
pub(crate) struct ActorPoses {
    /// Actors whose hit colliders were moved (not added, removed or rebuilt) since the
    /// last refresh, in call order, possibly repeated.
    pub touched: Vec<u32>,
    /// The colliders refreshed without a detection pass since the last one or the last
    /// step, sorted by actor FormID, then by their place in the actor's capsule list:
    /// the order the queries test them in, which decides exact ties among them.
    side: Vec<SideCollider>,
    /// Their handles (`into_raw_parts`), sorted: the broad-phase leaves the queries must
    /// not use. By handle, not by owner: an actor's ragdoll and gore pieces report the
    /// same FormID and their leaves are up to date.
    hidden: Vec<(u32, u32)>,
}

impl ActorPoses {
    pub fn touch(&mut self, form_id: u32) {
        self.touched.push(form_id);
    }

    /// Forget everything: a detection pass or a step brought the broad phase up to date.
    pub fn clear(&mut self) {
        self.touched.clear();
        self.side.clear();
        self.hidden.clear();
    }

    /// Replace the colliders kept aside for `actors` by `fresh`.
    pub fn replace(&mut self, actors: &[u32], fresh: impl IntoIterator<Item = SideCollider>) {
        self.side.retain(|s| actors.binary_search(&s.form_id).is_err());
        self.side.extend(fresh);
        self.side.sort_by_key(|s| (s.form_id, s.slot));
        self.hidden.clear();
        self.hidden.extend(self.side.iter().map(|s| s.handle.into_raw_parts()));
        self.hidden.sort_unstable();
    }

    pub fn side(&self) -> &[SideCollider] {
        &self.side
    }

    /// Whether a collider is one kept aside, whose broad-phase leaf the queries must not
    /// use.
    pub fn hides(&self, handle: ColliderHandle) -> bool {
        !self.hidden.is_empty() && self.hidden.binary_search(&handle.into_raw_parts()).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_combine_and_print() {
        let mut r = DirtyReasons::NONE;
        assert!(r.is_empty());
        assert_eq!(r.to_string(), "none");
        r.insert(DirtyReasons::DOORS);
        r.insert(DirtyReasons::ACTOR_PARTS);
        assert!(r.contains(DirtyReasons::ACTOR_PARTS) && r.contains(DirtyReasons::DOORS));
        assert!(!r.contains(DirtyReasons::GORE));
        assert!(DirtyReasons::ACTOR_PARTS.is_within(DirtyReasons::ACTOR_POSES));
        assert!(!r.is_within(DirtyReasons::ACTOR_POSES));
        assert!(!(DirtyReasons::ACTOR_PARTS | DirtyReasons::ACTOR_REBUILT).is_within(DirtyReasons::ACTOR_POSES));
        assert_eq!(r.to_string(), "actor_parts+doors");
        assert_eq!(r, DirtyReasons::ACTOR_PARTS | DirtyReasons::DOORS);
    }

    #[test]
    fn refreshes_are_counted_by_combination() {
        let mut s = RefreshStats::default();
        s.record_full(DirtyReasons::ACTOR_PARTS);
        s.record_full(DirtyReasons::ACTOR_PARTS);
        s.record_full(DirtyReasons::ACTOR_PARTS | DirtyReasons::BODIES);
        assert_eq!(s.full, 3);
        assert_eq!(s.by_reasons[&DirtyReasons::ACTOR_PARTS], 2);
        assert_eq!(s.by_reasons[&(DirtyReasons::ACTOR_PARTS | DirtyReasons::BODIES)], 1);
    }
}
