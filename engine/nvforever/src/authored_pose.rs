//! The pose a placed corpse was left in by its designer (docs/spec-notes/dead-on-spawn.md
//! §5): a placed actor reference may carry `XRGD` "Ragdoll Bones" [documented: xEdit
//! wbRagdoll; schema `common.json`], one entry per ragdoll body of its skeleton, each a
//! position and three angles. What the entries mean [observed on FalloutNV.esm's 278
//! human corpses with `XRGD`, each with 18 entries for the 18 bodies of
//! `characters\_male\skeleton.nif`; hypothesis where noted]:
//!
//! - Entry k belongs to the skeleton's k-th body in file order (the order
//!   `fnv_collide::ragdoll` reads them in). The byte the schema calls the bone id does not
//!   name it [observed: the ids run 2, 8, 9, 10, 14, 15, 16, 2, 3, 4, 0, 1, 5, 6, 7, 11, 12,
//!   13 on all 278, the first entry 2 again].
//! - Entries 1… are their bone's transform under its parent node in the skeleton: their
//!   positions match the bind pose's local translations (thigh to calf 31.94 against 31.97,
//!   spine to spine1 7.48 against 7.48, neck to neck1 0.63 against 0.65). The angles are
//!   in the convention of a reference's rotation (`fnv_world::transform::ref_rotation`):
//!   with it the calves turn about their hinge axis alone (0.5° off it, median), the
//!   bodies lie flat (bone heights spread 24 units, median; the other orders and signs
//!   give 39 to 47), and the bones turn least from the bind pose.
//! - Entry 0 is the root body (the pelvis, `Bip01 NonAccum`): its position is about 0,
//!   and its rotation is relative to the reference's: so read, the heads of 185 of the 278
//!   lie behind where the reference faces (bodies fallen backwards); read as a world
//!   rotation, they point every way [hypothesis: the reading].
//! - `XRGB` "Ragdoll Biped Rotation" is always (0, 0, −π/2): the bind rotation of `Bip01`
//!   in the same convention. Not used.
//!
//! Nodes between bodies (pelvis, neck, clavicles) keep their bind pose. The result is the
//! bodies' poses in game space, which `ragdoll.rs` puts the ragdoll in, at rest.

use bevy::math::{Mat3, Vec3};
use fnv_collide::Iso;
use nvf_assets::actor::PreparedSkeleton;

/// One `XRGD` entry: position (game units) and angles (radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bone {
    pub position: [f32; 3],
    pub angles: [f32; 3],
}

/// The entries of an `XRGD` subrecord: 28 bytes each, a byte (not used), three bytes of
/// padding, then six little-endian floats [documented: xEdit wbRagdoll].
pub fn parse(data: &[u8]) -> Vec<Bone> {
    let f = |c: &[u8; 28], i: usize| f32::from_le_bytes([c[4 + 4 * i], c[5 + 4 * i], c[6 + 4 * i], c[7 + 4 * i]]);
    let (entries, _) = data.as_chunks::<28>();
    entries.iter().map(|c| Bone { position: [f(c, 0), f(c, 1), f(c, 2)], angles: [f(c, 3), f(c, 4), f(c, 5)] }).collect()
}

/// A reference-style rotation as a matrix (`v' = M v`).
fn rotation(angles: [f32; 3]) -> Mat3 {
    Mat3::from_cols_array(&fnv_world::transform::ref_rotation(angles)).transpose()
}

fn iso(t: Vec3, r: Mat3) -> Iso {
    let m = r.transpose().to_cols_array();
    Iso { translation: t.to_array(), rotation: m }
}

/// The world poses (game space) of the bones of a ragdoll's bodies (`body_nodes`: each
/// body's bone, in the ragdoll's order) for a reference at `at` turned by `angles` and
/// scaled by `scale`, from its `XRGD` entries (module notes). `None` for a body whose bone
/// is not under the root body.
pub fn pose(body_nodes: &[Option<&str>], skel: &PreparedSkeleton, bones: &[Bone], at: [f32; 3], angles: [f32; 3], scale: f32) -> Result<Vec<Option<Iso>>, String> {
    if bones.len() != body_nodes.len() || bones.is_empty() {
        return Err(format!("{} entries for {} bodies", bones.len(), body_nodes.len()));
    }
    let n = skel.parents.len();
    let mut body_of = vec![None; n];
    let mut nodes = Vec::with_capacity(body_nodes.len());
    for (k, name) in body_nodes.iter().enumerate() {
        let name = name.ok_or("a body without a node")?;
        let &i = skel.by_name.get(name).ok_or_else(|| format!("no bone {name}"))?;
        body_of[i] = Some(k);
        nodes.push(i);
    }
    let reference = rotation(angles);
    let mut world: Vec<Option<(Vec3, Mat3)>> = vec![None; n];
    let root = nodes[0];
    let b0 = &bones[0];
    world[root] = Some((Vec3::from(at) + reference * (Vec3::from(b0.position) * scale), reference * rotation(b0.angles)));
    for i in 0..n {
        if world[i].is_some() {
            continue;
        }
        let Some((pt, pr)) = skel.parents[i].and_then(|p| world.get(p).copied().flatten()) else { continue };
        let (t, r) = match body_of[i] {
            Some(k) => (Vec3::from(bones[k].position), rotation(bones[k].angles)),
            None => {
                let l = &skel.locals[i];
                (l.translation, Mat3::from_quat(l.rotation))
            }
        };
        world[i] = Some((pt + pr * (t * scale), pr * r));
    }
    Ok(nodes.iter().map(|&i| world[i].map(|(t, r)| iso(t, r))).collect())
}

/// Quaternion of an [`Iso`] rotation (tests).
#[cfg(test)]
fn quat(i: &Iso) -> bevy::math::Quat {
    bevy::math::Quat::from_mat3(&Mat3::from_cols_array(&i.rotation).transpose())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Quat;
    use bevy::prelude::Transform;
    use std::collections::HashMap;

    /// Root -> pelvis (body 0) -> hip (no body) -> thigh (body 1).
    fn skeleton() -> PreparedSkeleton {
        let names = ["Root", "Pelvis", "Hip", "Thigh"].map(|s| Some(s.to_string())).to_vec();
        let locals = vec![
            Transform::from_xyz(0.0, 0.0, 60.0),
            Transform::IDENTITY,
            Transform::from_xyz(0.0, 0.0, 7.0).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
            Transform::from_xyz(30.0, 0.0, 0.0),
        ];
        let by_name: HashMap<String, usize> = names.iter().enumerate().map(|(i, n)| (n.clone().unwrap(), i)).collect();
        PreparedSkeleton { names, parents: vec![None, Some(0), Some(1), Some(2)], locals, bind_world: vec![Default::default(); 4], by_name }
    }

    const RAG: [Option<&str>; 2] = [Some("Pelvis"), Some("Thigh")];

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn entries_are_read_in_28_byte_steps() {
        let mut d = vec![2u8, 4, 29, 2];
        for v in [1.0f32, 2.0, 3.0, 0.1, 0.2, 0.3] {
            d.extend(v.to_le_bytes());
        }
        let b = parse(&[d.clone(), d].concat());
        assert_eq!(b.len(), 2);
        assert_eq!(b[1], Bone { position: [1.0, 2.0, 3.0], angles: [0.1, 0.2, 0.3] });
    }

    #[test]
    fn the_root_follows_the_reference_and_the_others_their_parents() {
        let skel = skeleton();
        let rag = RAG;
        // Pelvis turned 90° about z relative to the reference (clockwise angles), thigh 31
        // units out along the hip's x, unturned.
        let bones = [Bone { position: [0.0; 3], angles: [0.0, 0.0, -std::f32::consts::FRAC_PI_2] }, Bone { position: [31.0, 0.0, 0.0], angles: [0.0; 3] }];
        let at = [1000.0, 2000.0, 50.0];
        let p = pose(&rag, &skel, &bones, at, [0.0; 3], 1.0).unwrap();
        let (pelvis, thigh) = (p[0].unwrap(), p[1].unwrap());
        assert!(close(Vec3::from(pelvis.translation), Vec3::from(at)));
        // The pelvis's x now points along world y.
        assert!(close(quat(&pelvis) * Vec3::X, Vec3::Y), "{:?}", quat(&pelvis) * Vec3::X);
        // Hip: 7 up the pelvis's z (world z), turned another 90°: its x along world −x.
        let hip_at = Vec3::from(at) + Vec3::Z * 7.0;
        assert!(close(Vec3::from(thigh.translation), hip_at - Vec3::X * 31.0), "{:?}", thigh.translation);
        // Scaled 2: offsets double.
        let p2 = pose(&rag, &skel, &bones, at, [0.0; 3], 2.0).unwrap();
        assert!(close(Vec3::from(p2[1].unwrap().translation), Vec3::from(at) + Vec3::Z * 14.0 - Vec3::X * 62.0));
        // The reference turned 90° (clockwise angle, as references store it) turns it all.
        let p3 = pose(&rag, &skel, &bones, at, [0.0, 0.0, -std::f32::consts::FRAC_PI_2], 1.0).unwrap();
        assert!(close(quat(&p3[0].unwrap()) * Vec3::X, -Vec3::X));
        assert!(pose(&rag, &skel, &bones[..1], at, [0.0; 3], 1.0).is_err(), "one entry per body");
    }
}
