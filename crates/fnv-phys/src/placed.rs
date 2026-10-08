//! What a placed reference contributes to the physics world: the collision objects of
//! its base object's model and of a CollisionMarker primitive. One rule shared by
//! `fnvtool phys` and the game (`engine/nvf-play`), so the two never diverge.
//!
//! Whether the reference is in the world at all (deleted, disabled, taken) is the
//! caller's decision: the tools read the plugins' flags, the game asks its simulation.
//! Models are read by the caller too ([`ModelCollision`]); fnv-phys never reads NIF
//! blocks (docs/design/layer4.md §7).

use crate::primitive::{primitive, primitive_object, BoundsMeaning, COLLISION_MARKER};
use fnv_collide::CollisionObject;
use fnv_esm::{LoadOrder, Schema};
use fnv_world::placed::base_object;
use fnv_world::Placement;
use std::sync::Arc;

/// Reads the collision objects of a model (a path relative to the data folder, as
/// `BaseObject::model_path` gives it), typically through `fnv_collide::nif`.
pub trait ModelCollision {
    /// The model's solid collision objects (empty when it has none), or why it could not
    /// be read.
    fn objects(&mut self, path: &str) -> Result<Arc<Vec<CollisionObject>>, String>;
}

/// The collision of one placed reference.
#[derive(Debug, Clone, Default)]
pub struct RefCollision {
    /// Signature of the base object, when it could be read.
    pub base_sig: Option<[u8; 4]>,
    /// Collision objects in the reference's frame: the CollisionMarker primitive, if any,
    /// then the model's objects. Empty when the reference has no collision.
    pub objects: Vec<CollisionObject>,
}

/// The collision objects of a placed reference: a CollisionMarker carries its own box or
/// sphere (physics.md §4.1); otherwise, and in addition, the base object's model, unless
/// it is a SpeedTree (`.spt` files hold no Havok blocks, physics.md §4). With no
/// `models`, model collision is left out.
///
/// `tally` is called once for each outcome, with the names of the `refs.counts` of
/// `fnv.phys.cell/1` (physics.md §7): `no_base`, `collision_primitive`, `bad_primitive`,
/// `no_model`, `speedtree`, `awaiting_collision`, `model_without_collision`,
/// `model_unreadable`.
pub fn ref_collision(
    lo: &LoadOrder,
    schema: &Schema,
    pl: &Placement,
    models: Option<&mut dyn ModelCollision>,
    tally: &mut dyn FnMut(&'static str),
) -> RefCollision {
    let Some(base) = pl.base.and_then(|b| base_object(lo, schema, b)) else {
        tally("no_base");
        return RefCollision::default();
    };
    let mut out = RefCollision { base_sig: Some(base.sig), objects: Vec::new() };
    if pl.base == Some(COLLISION_MARKER) {
        match primitive(lo, schema, pl.form_id).and_then(|p| primitive_object(&p, BoundsMeaning::HalfExtents)) {
            Some(o) => {
                out.objects.push(o);
                tally("collision_primitive");
            }
            None => tally("bad_primitive"),
        }
    }
    match base.model_path() {
        None if out.objects.is_empty() => tally("no_model"),
        None => {}
        Some(m) if m.ends_with(".spt") => tally("speedtree"),
        Some(m) => match models {
            None => tally("awaiting_collision"),
            Some(ms) => match ms.objects(&m) {
                Ok(o) if o.is_empty() => tally("model_without_collision"),
                Ok(o) => out.objects.extend(o.iter().cloned()),
                Err(_) => tally("model_unreadable"),
            },
        },
    }
    out
}
