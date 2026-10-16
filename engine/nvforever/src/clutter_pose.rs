//! The pose a designer left a placed jointed object in: clutter skeletons
//! (`meshes\clutter\skeleton\…`, MSTT) whose reference carries `XRGD` "Ragdoll Bones"
//! (docs/spec-notes/dead-on-spawn.md §5.1). Without it they stood in the file's own pose,
//! upright with the arms out, where the original shows them lying on the floor (playtest
//! list 5 item 71, the Goodsprings cave).
//!
//! What the entries mean here [observed: `fnvtool esm decode` and `nif decode` of the
//! Goodsprings cave's three skeletons, `skeletonfemale50scasual.nif` and
//! `skeletonmale50scasual.nif`, 17 bodies and 17 entries each]:
//!
//! - Entry k belongs to the file's k-th collision object (the order `fnv_collide` reads
//!   them in, the blocks' order).
//! - Each entry is its body node's transform under the node's parent, in the convention
//!   of a reference's rotation (`authored_pose`): for a node under another body node the
//!   positions repeat the file's own local translations (forearm under upper arm 23.81 in
//!   both, head under torso 45.09 against 45.10, right upper arm under torso (40.72, −11.59,
//!   −2.33) against (40.76, −11.68, −2.21)); for a node under the file's root, the root
//!   standing for the reference. Read so, the bodies of the three lie within 15 to 35 units
//!   of height of each other, scattered as a fallen skeleton's, the first entry at the
//!   reference's position [hypothesis for the root-level ones: the alternative, under the
//!   first body, put the thighs 30 units below the arm].
//! - Nodes without a body keep their place under their parent.
//!
//! Only the drawing is posed: the collision stays in the file's pose (the jointed bodies
//! are held where placed, `nvf_play::phys::prepare::hold_jointed`) [not done: posing the
//! collision needs the node hierarchy on the physics side].

use crate::authored_pose::{self, Bone};
use bevy::math::{Affine3A, Mat3, Vec3};
use fnv_esm::LoadOrder;
use fnv_nif::scene::Decoded;
use fnv_nif::NifFile;
use nvf_assets::data::GameData;
use nvf_assets::model::ModelNode;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

/// The nodes a file's collision objects hang on, in body order.
type BodyNodes = Arc<Vec<Option<String>>>;

/// [`BodyNodes`] by archive path (`None`: no collision).
static BODIES: LazyLock<Mutex<HashMap<String, Option<BodyNodes>>>> = LazyLock::new(Default::default);

/// The base record types whose references are posed [observed: the clutter skeletons are
/// MSTT; the 1,570 REFR with `XRGD` in FalloutNV.esm were not sorted by type].
pub fn posable(sig: &[u8; 4]) -> bool {
    matches!(sig, b"MSTT" | b"STAT" | b"MISC")
}

/// The `XRGD` entries of reference `r`, if it has any.
pub fn stored(lo: &LoadOrder, r: u32) -> Option<Vec<Bone>> {
    let d = lo.index.get(&r)?.last()?;
    let subs = lo.records[d.plugin as usize][d.ordinal as usize].subrecords().ok()?;
    let x = subs.iter().find(|s| &s.sig == b"XRGD")?;
    let bones = authored_pose::parse(&x.data);
    (!bones.is_empty()).then_some(bones)
}

/// The nodes the collision objects of the file at `path` hang on, in body order.
pub fn body_nodes(data: &GameData, path: &str) -> Option<BodyNodes> {
    let key = fnv_vfs::normalize(path);
    let mut cache = BODIES.lock().ok()?;
    cache
        .entry(key.clone())
        .or_insert_with(|| {
            let bytes = data.read(&key)?;
            let file = NifFile::parse(&bytes, Some(&data.schema)).ok()?;
            let dec = Decoded::new(&file, &bytes, &data.schema);
            let ex = fnv_collide::nif::extract(&file, &dec);
            (!ex.objects.is_empty()).then(|| Arc::new(ex.objects.iter().map(|o| o.node_name.clone()).collect()))
        })
        .clone()
}

/// Every node's transform from the model's root (game space, the root's own left out)
/// with the bodies' nodes put where `bones` says (module notes); `None` when the entries
/// do not match the bodies one for one or a body's node is missing.
pub fn pose(nodes: &[ModelNode], bodies: &[Option<String>], bones: &[Bone]) -> Option<Vec<Affine3A>> {
    if bodies.len() != bones.len() || bones.is_empty() {
        return None;
    }
    let mut local: Vec<Affine3A> = nodes.iter().map(|n| n.local).collect();
    for (body, bone) in bodies.iter().zip(bones) {
        let name = body.as_deref()?;
        let i = nodes.iter().position(|n| n.name.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(name)))?;
        let scale = Vec3::new(local[i].matrix3.x_axis.length(), local[i].matrix3.y_axis.length(), local[i].matrix3.z_axis.length());
        let r: Mat3 = authored_pose::rotation(bone.angles);
        local[i] = Affine3A::from_mat3_translation(r * Mat3::from_diagonal(scale), Vec3::from(bone.position));
    }
    // Parents come before their children (`model_nodes`).
    let mut world: Vec<Affine3A> = Vec::with_capacity(nodes.len());
    for (i, n) in nodes.iter().enumerate() {
        let parent = n.parent.map_or(Affine3A::IDENTITY, |p| world[p]);
        world.push(parent * local[i]);
    }
    Some(world)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, parent: Option<usize>, t: Vec3) -> ModelNode {
        ModelNode { name: Some(name.into()), parent, local: Affine3A::from_translation(t), billboard: None, addon: None }
    }

    #[test]
    fn bodies_take_their_entries_and_children_follow() {
        // Root → Torso (body 0) → Ribs (no body) and Head (body 1).
        let nodes = vec![node("Torso", None, Vec3::new(0.0, 0.0, 69.0)), node("Ribs", Some(0), Vec3::new(10.0, 0.0, 0.0)), node("Head", Some(0), Vec3::new(45.0, 0.0, 0.0))];
        let bodies = vec![Some("Torso".to_string()), Some("head".to_string())];
        // The torso lies at the reference turned 90° about z (a clockwise angle, as
        // references store it), the head 45 out along it, unturned.
        let bones = [Bone { position: [0.0; 3], angles: [0.0, 0.0, -std::f32::consts::FRAC_PI_2] }, Bone { position: [45.0, 0.0, 0.0], angles: [0.0; 3] }];
        let w = pose(&nodes, &bodies, &bones).unwrap();
        let at = |i: usize| w[i].translation;
        assert!((Vec3::from(at(0)) - Vec3::ZERO).length() < 1e-4);
        assert!((Vec3::from(at(2)) - Vec3::new(0.0, 45.0, 0.0)).length() < 1e-4, "{:?}", at(2));
        // The ribs keep their place under the torso.
        assert!((Vec3::from(at(1)) - Vec3::new(0.0, 10.0, 0.0)).length() < 1e-4, "{:?}", at(1));
        assert!(pose(&nodes, &bodies, &bones[..1]).is_none(), "one entry per body");
        assert!(pose(&nodes, &[Some("Torso".into()), Some("Tail".into())], &bones).is_none(), "a body's node must exist");
    }
}
