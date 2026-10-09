//! Actor hit volumes (docs/design/combat.md §5.2, step C3): one vertical capsule per
//! placed actor, so shots and melee rays can hit NPCs and creatures.
//!
//! - The body is kinematic and moved by the caller to the actor's feet each frame.
//! - The collider is on the Havok BIPED layer while the actor lives and DEAD_BIP once it
//!   is dead (`crate::layers`; the layer names are documented, using them for actors is
//!   our choice). Queries can skip either with `Filter::exclude_layers`.
//! - It takes part in no contacts: dynamic clutter is not pushed by it and the character
//!   controller ignores it [design: combat volumes are for shots, not for walking].
//!
//! Sizes come from the caller (the actor's bounds or race height); this module only
//! keeps the capsule where it is told.

use crate::world::{Owner, PhysWorld};
use crate::{layers, BodyState};
use rapier3d::prelude::*;

/// A capsule standing on `feet`: `height` from the bottom of the lower cap to the top of
/// the upper cap, game units.
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

    /// Centre of the capsule, game units.
    pub fn center(&self) -> [f32; 3] {
        let h = self.height.max(2.0 * self.radius);
        [self.feet[0], self.feet[1], self.feet[2] + h / 2.0]
    }
}

/// In no group and colliding with none: queries (whose filter has no groups) still see it.
const NO_CONTACTS: InteractionGroups = InteractionGroups::new(Group::NONE, Group::NONE, InteractionTestMode::And);

impl PhysWorld {
    /// Add the hit volume of actor reference `form_id`, or move and resize it (nothing
    /// changes when `v` is what it already is; the capsule is rebuilt only when its size
    /// changes).
    pub fn set_actor_volume(&mut self, form_id: u32, v: HitVolume) {
        let old = self.actors.get(&form_id).copied();
        if old.is_some_and(|(_, o)| o == v) {
            return;
        }
        let k = self.k;
        let radius = v.radius.max(1.0);
        let half_segment = (v.height.max(2.0 * radius) / 2.0 - radius).max(0.0);
        let c = v.center();
        let center = Vector::new(c[0], c[1], c[2]) * k;
        let owner = Owner::Actor { form_id, layer: v.layer() };
        if let Some((b, o)) = old {
            if let Some(body) = self.w.bodies.get_mut(b) {
                body.set_translation(center, true);
                let resized = o.radius != v.radius || o.height != v.height;
                let handles: Vec<ColliderHandle> = body.colliders().to_vec();
                for h in handles {
                    if let Some(co) = self.w.colliders.get_mut(h) {
                        co.user_data = owner.encode();
                        if resized {
                            co.set_shape(SharedShape::capsule_z(half_segment * k, radius * k));
                        }
                    }
                }
                self.actors.insert(form_id, (b, v));
                self.mark_dirty();
                return;
            }
        }
        let body = self.insert_body(RigidBodyBuilder::kinematic_position_based().translation(center).user_data(owner.encode()));
        self.w.insert_collider(
            ColliderBuilder::capsule_z(half_segment * k, radius * k).user_data(owner.encode()).collision_groups(NO_CONTACTS).solver_groups(NO_CONTACTS),
            Some(body),
        );
        self.actors.insert(form_id, (body, v));
        self.mark_dirty();
    }

    /// Remove an actor's hit volume (it left the loaded area or was disabled).
    pub fn remove_actor_volume(&mut self, form_id: u32) -> bool {
        let Some((b, _)) = self.actors.remove(&form_id) else { return false };
        self.remove_body(b);
        true
    }

    /// The actors that have a hit volume, by FormID.
    pub fn actor_volumes(&self) -> impl Iterator<Item = u32> + '_ {
        self.actors.keys().copied()
    }

    /// An actor's hit volume as last set.
    pub fn actor_volume(&self, form_id: u32) -> Option<HitVolume> {
        self.actors.get(&form_id).map(|(_, v)| *v)
    }

    /// The body state of an actor's hit volume.
    pub fn actor_volume_state(&self, form_id: u32) -> Option<BodyState> {
        let (b, _) = self.actors.get(&form_id)?;
        Some(crate::world::body_state(&self.w.bodies[*b], self.k))
    }
}
