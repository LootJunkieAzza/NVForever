//! The jointed rigid bodies of a mesh (docs/spec-notes/ragdoll.md): a skeleton's Havok
//! bodies, one per bone that has one, and the constraints that join them. This is the
//! data a ragdoll is built from; `fnv_phys::ragdoll` turns it into bodies and joints.
//!
//! Everything is in game units ([`crate::nif::HAVOK_SCALE`] and the node's scale applied)
//! and in the **body's frame**, which is its node's frame (composed with the body
//! transform for bhkRigidBodyT, as for shapes, collision.md §4).
//!
//! The angle conventions ([`RagdollJoint::rest`]) are the readings under which the
//! skeletons' bind poses lie inside their own limits (ragdoll.md §3).

use crate::nif::{extract, HAVOK_SCALE};
use crate::shape::{CollisionObject, Iso};
use fnv_nif::file::NifFile;
use fnv_nif::scene::Decoded;
use fnv_nif::Struct;
use serde::Serialize;
use std::collections::HashMap;

/// One body of a ragdoll.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RagdollBody {
    /// The collision object: its node (the bone), shape, mass, friction, layer.
    pub object: CollisionObject,
    pub linear_damping: f32,
    pub angular_damping: f32,
    /// The body's stored translation (game units) and rotation: its pose in the mesh
    /// root's frame when the file was made [observed: equal to the node's bind pose in
    /// `characters\_male\skeleton.nif`, ragdoll.md §2]. Not used to place anything.
    pub stored: Iso,
}

impl RagdollBody {
    /// The body's frame in the mesh root's frame, bind pose.
    pub fn rest(&self) -> Iso {
        self.object.node_transform.compose(&self.object.body_transform)
    }
}

/// How a joint limits the attached body `a` against body `b`. Axes are unit vectors in
/// the frame of the body their name ends with.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JointKind {
    /// A ball joint with a cone, two plane cones and a twist range (bhkRagdollConstraint).
    Ragdoll {
        twist_a: [f32; 3],
        plane_a: [f32; 3],
        twist_b: [f32; 3],
        plane_b: [f32; 3],
        cone_max: f32,
        plane_min: f32,
        plane_max: f32,
        twist_min: f32,
        twist_max: f32,
    },
    /// A hinge about `axis`, with an angle range when limited (bhkLimitedHingeConstraint,
    /// bhkHingeConstraint). `perp` is the direction in the hinge's plane the angle is
    /// measured from.
    Hinge { axis_a: [f32; 3], perp_a: [f32; 3], axis_b: [f32; 3], perp_b: [f32; 3], limits: Option<[f32; 2]> },
    /// A free ball joint (bhkBallAndSocketConstraint).
    Ball,
    /// The pivots stay `length` apart (bhkStiffSpringConstraint).
    Spring { length: f32 },
    /// A constraint type that is read but not converted (prismatic); held as a ball joint.
    Other { constraint_type: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RagdollJoint {
    /// Block index of the constraint.
    pub block: usize,
    /// Indices into [`Ragdoll::bodies`]: the attached body and the one it is attached to.
    pub a: usize,
    pub b: usize,
    /// The joint's point in each body's frame, game units.
    pub pivot_a: [f32; 3],
    pub pivot_b: [f32; 3],
    pub kind: JointKind,
    /// The constraint's stored `max_friction` (unit unknown), 0 when it has none.
    pub max_friction: f32,
    /// The `strength` of the malleable constraint wrapping it, if one does.
    pub strength: Option<f32>,
}

/// A joint's angles for a pair of body rotations, in the stored limits' terms.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct JointAngles {
    /// Ragdoll: angle between the two twist axes (limit `cone_max`).
    pub cone: f32,
    /// Ragdoll: how far a's twist axis leans towards b's plane axis (limits `plane_*`).
    pub plane: f32,
    /// Ragdoll: rotation about the twist axis (limits `twist_*`). Hinge: the hinge angle.
    pub twist: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Ragdoll {
    pub bodies: Vec<RagdollBody>,
    pub joints: Vec<RagdollJoint>,
    /// Constraints that could not be followed.
    pub problems: Vec<String>,
}

fn v3(s: &Struct, name: &str) -> Option<[f32; 3]> {
    let v = s.floats(name)?;
    (v.len() >= 3).then(|| [v[0], v[1], v[2]])
}

pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn unit(v: [f32; 3]) -> Option<[f32; 3]> {
    let n = dot(v, v).sqrt();
    (n.is_finite() && n > 1e-6).then(|| v.map(|x| x / n))
}

/// The limits of one constraint info struct, by Havok constraint type number
/// [documented: nif.xml hkConstraintType]: 0 ball and socket, 1 hinge, 2 limited hinge,
/// 6 prismatic, 7 ragdoll, 8 stiff spring. Returns the kind, the pivots (stored units)
/// and `max_friction`.
fn read_info(ty: i64, c: &Struct) -> Option<(JointKind, [f32; 3], [f32; 3], f32)> {
    let pivots = (v3(c, "pivot_a")?, v3(c, "pivot_b")?);
    let friction = c.float("max_friction").unwrap_or(0.0);
    let ax = |n: &str| v3(c, n).and_then(unit);
    let kind = match ty {
        0 => JointKind::Ball,
        1 | 2 => JointKind::Hinge {
            axis_a: ax("axis_a")?,
            perp_a: ax("perp_axis_in_a1")?,
            axis_b: ax("axis_b")?,
            perp_b: ax("perp_axis_in_b1")?,
            limits: (ty == 2).then_some(()).and_then(|()| Some([c.float("min_angle")?, c.float("max_angle")?])),
        },
        7 => JointKind::Ragdoll {
            twist_a: ax("twist_a")?,
            plane_a: ax("plane_a")?,
            twist_b: ax("twist_b")?,
            plane_b: ax("plane_b")?,
            cone_max: c.float("cone_max_angle")?,
            plane_min: c.float("plane_min_angle")?,
            plane_max: c.float("plane_max_angle")?,
            twist_min: c.float("twist_min_angle")?,
            twist_max: c.float("twist_max_angle")?,
        },
        8 => JointKind::Spring { length: c.float("length")? },
        other => JointKind::Other { constraint_type: other },
    };
    Some((kind, pivots.0, pivots.1, friction))
}

/// The field of a wrapped constraint that holds the info for its type.
fn wrapped(w: &Struct) -> Option<(i64, &Struct, Option<f32>)> {
    let ty = w.int("constraint_type")?;
    let name = match ty {
        0 => "ball_and_socket",
        1 => "hinge",
        2 => "limited_hinge",
        6 => "prismatic",
        7 => "ragdoll",
        8 => "stiff_spring",
        13 => {
            // A malleable constraint inside a breakable one.
            let (ty, inner, _) = wrapped(w.strukt("malleable")?)?;
            return Some((ty, inner, w.strukt("malleable")?.float("strength")));
        }
        _ => return None,
    };
    Some((ty, w.strukt(name)?, w.float("strength")))
}

/// The jointed bodies of a decoded mesh: every solid collision object, and every
/// constraint listed by a body whose two entities are among them.
pub fn ragdoll(file: &NifFile, dec: &Decoded) -> Ragdoll {
    let ex = extract(file, dec);
    let mut out = Ragdoll { problems: ex.problems.clone(), ..Default::default() };
    let index: HashMap<usize, usize> = ex.objects.iter().enumerate().map(|(i, o)| (o.body_block, i)).collect();
    let mut scales = Vec::with_capacity(ex.objects.len());
    for o in &ex.objects {
        let info = dec.get(o.body_block as i32).and_then(|b| b.strukt("rigid_body_info"));
        let f = HAVOK_SCALE * o.node_to_root.scale;
        let mut stored = o.body_iso();
        stored.translation = stored.translation.map(|v| v * HAVOK_SCALE);
        out.bodies.push(RagdollBody {
            object: o.to_object(),
            linear_damping: info.and_then(|s| s.float("linear_damping")).unwrap_or(0.0),
            angular_damping: info.and_then(|s| s.float("angular_damping")).unwrap_or(0.0),
            stored,
        });
        scales.push(f);
    }
    for o in &ex.objects {
        let Some(body) = dec.get(o.body_block as i32) else { continue };
        for &r in body.refs("constraints").unwrap_or_default() {
            let Some(con) = dec.get(r) else {
                out.problems.push(format!("constraint {r} missing or undecodable"));
                continue;
            };
            let ty_name = file.blocks[r as usize].type_name.as_str();
            if !ty_name.ends_with("Constraint") {
                out.problems.push(format!("{ty_name} {r} in a constraint list is not a constraint"));
                continue;
            }
            let ends = con.strukt("constraint_info").and_then(|i| Some((i.block_ref("entity_a")?, i.block_ref("entity_b")?)));
            let ends = ends.and_then(|(a, b)| Some((*index.get(&(a as usize))?, *index.get(&(b as usize))?)));
            let Some((a, b)) = ends.filter(|(a, b)| a != b) else {
                out.problems.push(format!("{ty_name} {r}: its entities are not two bodies of this mesh"));
                continue;
            };
            let read = match ty_name {
                "bhkBallAndSocketConstraint" => con.strukt("constraint").map(|c| (0, c, None)),
                "bhkHingeConstraint" => con.strukt("constraint").map(|c| (1, c, None)),
                "bhkLimitedHingeConstraint" => con.strukt("constraint").map(|c| (2, c, None)),
                "bhkPrismaticConstraint" => con.strukt("constraint").map(|c| (6, c, None)),
                "bhkRagdollConstraint" => con.strukt("constraint").map(|c| (7, c, None)),
                "bhkStiffSpringConstraint" => con.strukt("constraint").map(|c| (8, c, None)),
                "bhkMalleableConstraint" => con.strukt("constraint").and_then(wrapped),
                "bhkBreakableConstraint" => con.strukt("constraint_data").and_then(wrapped),
                _ => None,
            };
            let Some((kind, pa, pb, max_friction, strength)) = read.and_then(|(ty, c, s)| read_info(ty, c).map(|(k, pa, pb, f)| (k, pa, pb, f, s))) else {
                out.problems.push(format!("{ty_name} {r}: not readable"));
                continue;
            };
            let kind = match kind {
                JointKind::Spring { length } => JointKind::Spring { length: length * scales[a] },
                k => k,
            };
            out.joints.push(RagdollJoint {
                block: r as usize,
                a,
                b,
                pivot_a: pa.map(|v| v * scales[a]),
                pivot_b: pb.map(|v| v * scales[b]),
                kind,
                max_friction,
                strength,
            });
        }
    }
    out
}

impl RagdollJoint {
    /// The joint's angles when body a's frame is `a` and body b's is `b` (any common
    /// frame). Conventions (ragdoll.md §3): `plane` is the angle by which a's twist axis
    /// leaves the plane square to b's plane axis, positive towards that axis; `twist` is
    /// the rotation of a against b about the twist axis once the swing is taken out; a
    /// hinge's angle is that of a's `perp` from b's `perp`, positive about b's axis.
    pub fn angles(&self, a: &Iso, b: &Iso) -> JointAngles {
        match self.kind {
            JointKind::Ragdoll { twist_a, plane_a, twist_b, plane_b, .. } => {
                let (ta, tb, pb) = (a.rotate(twist_a), b.rotate(twist_b), b.rotate(plane_b));
                // a's frame (twist, plane, twist × plane) in b's: its twist component.
                let (pa, mb) = (a.rotate(plane_a), cross(tb, pb));
                let ma = cross(ta, pa);
                // Rotation matrix of a's frame in b's frame, m[i][j] = b_i · a_j.
                let fb = [tb, pb, mb];
                let fa = [ta, pa, ma];
                let m = |i: usize, j: usize| dot(fb[i], fa[j]);
                // Quaternion x and w of that rotation; the twist about x is 2·atan2(x, w).
                let w = (1.0 + m(0, 0) + m(1, 1) + m(2, 2)).max(0.0).sqrt() / 2.0;
                let x = if w > 1e-4 { (m(2, 1) - m(1, 2)) / (4.0 * w) } else { (1.0 + m(0, 0) - m(1, 1) - m(2, 2)).max(0.0).sqrt() / 2.0 };
                let mut twist = 2.0 * x.atan2(w);
                if twist > std::f32::consts::PI {
                    twist -= std::f32::consts::TAU;
                }
                JointAngles { cone: dot(ta, tb).clamp(-1.0, 1.0).acos(), plane: dot(ta, pb).clamp(-1.0, 1.0).asin(), twist }
            }
            JointKind::Hinge { perp_a, axis_b, perp_b, .. } => {
                let (a1, xb, b1) = (a.rotate(perp_a), b.rotate(axis_b), b.rotate(perp_b));
                JointAngles { twist: dot(a1, cross(xb, b1)).atan2(dot(a1, b1)), ..Default::default() }
            }
            _ => JointAngles::default(),
        }
    }

    /// Whether `angles` lie inside the joint's limits, each widened by `slack` radians.
    pub fn within(&self, angles: &JointAngles, slack: f32) -> bool {
        let inside = |v: f32, lo: f32, hi: f32| v >= lo - slack && v <= hi + slack;
        match self.kind {
            JointKind::Ragdoll { cone_max, plane_min, plane_max, twist_min, twist_max, .. } => {
                inside(angles.cone, 0.0, cone_max) && inside(angles.plane, plane_min, plane_max) && inside(angles.twist, twist_min, twist_max)
            }
            JointKind::Hinge { limits: Some([lo, hi]), .. } => inside(angles.twist, lo, hi),
            _ => true,
        }
    }

    /// Distance between the joint's point as each body places it, for body frames `a`
    /// and `b` (translations in game units).
    pub fn pivot_gap(&self, a: &Iso, b: &Iso) -> f32 {
        let (p, q) = (a.apply(self.pivot_a), b.apply(self.pivot_b));
        (0..3).map(|i| (p[i] - q[i]).powi(2)).sum::<f32>().sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rot(axis: usize, a: f32) -> Iso {
        let (s, c) = a.sin_cos();
        let rotation = match axis {
            0 => [1.0, 0.0, 0.0, 0.0, c, -s, 0.0, s, c],
            1 => [c, 0.0, s, 0.0, 1.0, 0.0, -s, 0.0, c],
            _ => [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0],
        };
        Iso { translation: [0.0; 3], rotation }
    }

    fn joint(kind: JointKind) -> RagdollJoint {
        RagdollJoint { block: 0, a: 0, b: 1, pivot_a: [0.0; 3], pivot_b: [0.0, 7.0, 0.0], kind, max_friction: 0.0, strength: None }
    }

    const X: [f32; 3] = [1.0, 0.0, 0.0];
    const Y: [f32; 3] = [0.0, 1.0, 0.0];

    #[test]
    fn ragdoll_angles_follow_single_axis_turns() {
        let j = joint(JointKind::Ragdoll {
            twist_a: X,
            plane_a: Y,
            twist_b: X,
            plane_b: Y,
            cone_max: 0.8,
            plane_min: -0.9,
            plane_max: 0.5,
            twist_min: -0.2,
            twist_max: 0.2,
        });
        let at = |a: &Iso| j.angles(a, &Iso::IDENTITY);
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
        // About the twist axis: twist only.
        let t = at(&rot(0, 0.15));
        assert!(close(t.twist, 0.15) && close(t.cone, 0.0) && close(t.plane, 0.0), "{t:?}");
        // About z the twist axis leans towards the plane axis: plane and cone.
        let t = at(&rot(2, 0.4));
        assert!(close(t.plane, 0.4) && close(t.cone, 0.4) && close(t.twist, 0.0), "{t:?}");
        let t = at(&rot(2, -0.7));
        assert!(close(t.plane, -0.7) && close(t.cone, 0.7), "{t:?}");
        // About the plane axis: the cone only.
        let t = at(&rot(1, 0.6));
        assert!(close(t.cone, 0.6) && close(t.plane, 0.0) && close(t.twist, 0.0), "{t:?}");
        assert!(j.within(&at(&rot(2, -0.7)), 0.0) && !j.within(&at(&rot(2, 0.7)), 0.0));
        assert!(!j.within(&at(&rot(0, 0.3)), 0.0) && j.within(&at(&rot(0, 0.3)), 0.11));
        // Moving both bodies together changes nothing.
        let both = rot(1, 1.1);
        let t = j.angles(&both.compose(&rot(2, 0.4)), &both);
        assert!(close(t.plane, 0.4) && close(t.twist, 0.0), "{t:?}");
    }

    #[test]
    fn hinge_angle_is_positive_about_the_axis() {
        let j = joint(JointKind::Hinge { axis_a: X, perp_a: Y, axis_b: X, perp_b: Y, limits: Some([-0.9, 1.5]) });
        let t = j.angles(&rot(0, 1.2), &Iso::IDENTITY);
        assert!((t.twist - 1.2).abs() < 1e-5 && j.within(&t, 0.0));
        let t = j.angles(&rot(0, -1.2), &Iso::IDENTITY);
        assert!((t.twist + 1.2).abs() < 1e-5 && !j.within(&t, 0.0));
    }

    #[test]
    fn pivots_meet_when_the_bodies_are_placed_to_match() {
        let j = joint(JointKind::Ball);
        let b = Iso { translation: [10.0, 0.0, 5.0], rotation: rot(2, 0.5).rotation };
        let a = Iso { translation: b.apply([0.0, 7.0, 0.0]), rotation: Iso::IDENTITY.rotation };
        assert!(j.pivot_gap(&a, &b) < 1e-5);
        assert!((j.pivot_gap(&Iso::IDENTITY, &Iso::IDENTITY) - 7.0).abs() < 1e-6);
    }

    #[test]
    fn iso_inverse_undoes() {
        let i = Iso { translation: [3.0, -2.0, 7.0], rotation: rot(2, 0.7).compose(&rot(0, -0.4)).rotation };
        let p = i.inverse().apply(i.apply([1.0, 2.0, 3.0]));
        assert!((p[0] - 1.0).abs() < 1e-5 && (p[1] - 2.0).abs() < 1e-5 && (p[2] - 3.0).abs() < 1e-5);
    }
}
