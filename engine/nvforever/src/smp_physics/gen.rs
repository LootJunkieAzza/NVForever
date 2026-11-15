//! The SMP generator (docs/spec-notes/smp-physics.md §6): bone chains for shapes that
//! come with no physics bones (Character Kit Remake's hair, Qwib's poncho), made at
//! load time from the owner's own meshes [design: owner decision 2026-11-14, item 215:
//! "make them", generated from the local files, nothing derived is shipped].
//!
//! The shape's vertices (in the skeleton root's space at bind pose, game axes: z up) are
//! cut into sectors around a vertical axis. Below `top`, each sector that reaches far
//! enough down gets a chain: an anchor at `top` on the anchor bone, then simulated links
//! every `seg` units down to the sector's lowest vertex, each placed at the middle of
//! the sector's vertices at its height. Vertices are skinned to the two links about their
//! height in their sector (and the nearest link of the next sector, for a smooth seam),
//! fading in over `blend` units below `top`; above it they keep their own weights. The
//! links of neighbouring sectors are tied by springs so the sheet keeps together.

use bevy::math::{Affine3A, Vec2, Vec3};

/// The generator's choices for one kind of shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenParams {
    pub sectors: usize,
    pub seg: f32,
    pub blend: f32,
    pub min_len: f32,
    pub max_links: usize,
    /// Physics of the links.
    pub mass: f32,
    pub damping: f32,
    pub wind: f32,
    pub radius: f32,
    pub swing: f32,
    pub stiffness: f32,
    pub tie_slack: f32,
    pub tie_stiffness: f32,
}

/// Hair [tuned by eye on CKR's long styles: the strands swing at a run and settle back
/// into the style].
pub const HAIR: GenParams = GenParams { sectors: 10, seg: 4.0, blend: 4.0, min_len: 6.0, max_links: 6, mass: 0.05, damping: 0.75, wind: 1.0, radius: 0.8, swing: 0.55, stiffness: 3.0, tie_slack: 0.8, tie_stiffness: 4.0 };

/// A poncho's or a coat's hem [tuned by eye on Qwib's poncho].
pub const DRAPE: GenParams = GenParams { sectors: 12, seg: 5.0, blend: 5.0, min_len: 8.0, max_links: 5, mass: 0.1, damping: 0.65, wind: 1.0, radius: 1.5, swing: 0.8, stiffness: 6.0, tie_slack: 1.2, tie_stiffness: 8.0 };

/// A generated bone: its rest world transform (root space) and parent: `None` for the
/// anchor bone, else another generated bone's index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenBone {
    pub rest: Affine3A,
    pub parent: Option<usize>,
    pub dynamic: bool,
}

/// What the generator made: bones, the joint links (child, parent) and the springs
/// between sectors, and per vertex up to four (joint, weight) pairs, where joints are
/// numbered after the shape's own (`own` of them): `own + i` is generated bone `i`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Generated {
    pub bones: Vec<GenBone>,
    pub joints: Vec<(usize, usize)>,
    pub ties: Vec<(usize, usize)>,
    pub weights: Vec<[(u16, f32); 4]>,
    pub params: Option<GenParams>,
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A shape to rig.
#[derive(Debug, Clone, Copy)]
pub struct Shape<'a> {
    /// Its vertices in root space at bind.
    pub points: &'a [Vec3],
    /// Per vertex its own (joint, weight) pairs, joints numbered as the shape's.
    pub own: &'a [Vec<(u16, f32)>],
    /// How many joints the shape has.
    pub own_joints: usize,
    /// Per vertex how free it may be (0 held by its own weights, 1 free; multiplied with
    /// the fade below `top`).
    pub looseness: &'a [f32],
    /// The vertical axis (x, y).
    pub center: Vec2,
    /// The height (z) the chains hang from.
    pub top: f32,
    /// The anchor bone's rest transform.
    pub anchor: Affine3A,
}

/// Rig `shape`; `None` when no sector reaches `min_len` below its top.
pub fn generate(shape: Shape, p: GenParams) -> Option<Generated> {
    let Shape { points, own, own_joints, looseness, center, top, anchor } = shape;
    let s = p.sectors.max(3);
    let tau = std::f32::consts::TAU;
    let sector_of = |v: Vec3| {
        let a = (v.y - center.y).atan2(v.x - center.x).rem_euclid(tau);
        a / tau * s as f32
    };
    let loose: Vec<f32> = points.iter().zip(looseness).map(|(v, l)| smooth((top - v.z) / p.blend.max(0.01)) * l.clamp(0.0, 1.0)).collect();
    // Per sector, its loose vertices.
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); s];
    for (i, v) in points.iter().enumerate() {
        if loose[i] > 0.0 {
            members[(sector_of(*v).floor() as usize).min(s - 1)].push(i);
        }
    }
    let (_, rot, _) = anchor.to_scale_rotation_translation();
    let mut g = Generated { params: Some(p), ..Default::default() };
    // Per sector: its chain's bone indices (anchor first) and link length.
    let mut chains: Vec<Option<(Vec<usize>, f32)>> = vec![None; s];
    for (k, m) in members.iter().enumerate() {
        let Some(low) = m.iter().map(|&i| points[i].z).reduce(f32::min) else { continue };
        let len = top - low;
        if len < p.min_len || m.len() < 6 {
            continue;
        }
        let links = ((len / p.seg).ceil() as usize).clamp(2, p.max_links.max(2));
        let seg = len / links as f32;
        let mut ids = Vec::new();
        let mut last_xy: Option<Vec2> = None;
        for j in 0..=links {
            let z = top - j as f32 * seg;
            let near: Vec<Vec2> = m.iter().map(|&i| points[i]).filter(|v| (v.z - z).abs() <= seg * 0.75).map(|v| Vec2::new(v.x, v.y)).collect();
            let xy = if near.is_empty() {
                last_xy.unwrap_or_else(|| {
                    let all: Vec<Vec2> = m.iter().map(|&i| Vec2::new(points[i].x, points[i].y)).collect();
                    all.iter().copied().sum::<Vec2>() / all.len() as f32
                })
            } else {
                near.iter().copied().sum::<Vec2>() / near.len() as f32
            };
            last_xy = Some(xy);
            let id = g.bones.len();
            g.bones.push(GenBone { rest: Affine3A::from_rotation_translation(rot, Vec3::new(xy.x, xy.y, z)), parent: ids.last().copied(), dynamic: j > 0 });
            if j > 0 {
                g.joints.push((id, id - 1));
            }
            ids.push(id);
        }
        chains[k] = Some((ids, seg));
    }
    if chains.iter().all(Option::is_none) {
        return None;
    }
    // Ties between neighbouring sectors' links at the same level.
    for k in 0..s {
        let (Some((a, _)), Some((b, _))) = (&chains[k], &chains[(k + 1) % s]) else { continue };
        if s == 2 && k == 1 {
            break;
        }
        for j in 1..a.len().min(b.len()) {
            g.ties.push((a[j], b[j]));
        }
    }
    // Weights.
    let along = |k: usize, z: f32| chains[k].as_ref().map(|(ids, seg)| (ids, ((top - z) / seg).clamp(0.0, (ids.len() - 1) as f32)));
    for (i, v) in points.iter().enumerate() {
        let mut out: Vec<(u16, f32)> = Vec::with_capacity(8);
        let w = loose[i];
        let u = sector_of(*v) - 0.5;
        let s0 = (u.floor().rem_euclid(s as f32) as usize).min(s - 1);
        let s1 = (s0 + 1) % s;
        let f = u - u.floor();
        let (mut ka, mut kb, mut share) = (s0, s1, 1.0 - f);
        if share < 0.5 {
            (ka, kb, share) = (s1, s0, f);
        }
        if chains[ka].is_none() {
            (ka, kb, share) = (kb, ka, 1.0);
        }
        if chains[kb].is_none() {
            share = 1.0;
        }
        let chain_w = if chains[ka].is_some() { w } else { 0.0 };
        for &(j, ow) in &own[i] {
            out.push((j, ow * (1.0 - chain_w)));
        }
        if chain_w > 0.0 {
            if let Some((ids, t)) = along(ka, v.z) {
                let j0 = (t.floor() as usize).min(ids.len() - 1);
                let j1 = (j0 + 1).min(ids.len() - 1);
                let fr = t - j0 as f32;
                out.push(((own_joints + ids[j0]) as u16, chain_w * share * (1.0 - fr)));
                out.push(((own_joints + ids[j1]) as u16, chain_w * share * fr));
            }
            if share < 1.0 {
                if let Some((ids, t)) = along(kb, v.z) {
                    let j = (t.round() as usize).min(ids.len() - 1);
                    out.push(((own_joints + ids[j]) as u16, chain_w * (1.0 - share)));
                }
            }
        }
        g.weights.push(top4(out));
    }
    Some(g)
}

/// The four largest of `pairs` (same joints merged), normalized to sum to 1.
pub fn top4(mut pairs: Vec<(u16, f32)>) -> [(u16, f32); 4] {
    pairs.sort_by_key(|p| p.0);
    let mut merged: Vec<(u16, f32)> = Vec::new();
    for (j, w) in pairs {
        match merged.last_mut() {
            Some(last) if last.0 == j => last.1 += w,
            _ => merged.push((j, w)),
        }
    }
    merged.retain(|p| p.1 > 1e-5);
    merged.sort_by(|a, b| b.1.total_cmp(&a.1));
    merged.truncate(4);
    let sum: f32 = merged.iter().map(|p| p.1).sum();
    let mut out = [(0u16, 0.0f32); 4];
    for (o, (j, w)) in out.iter_mut().zip(merged) {
        *o = (j, if sum > 0.0 { w / sum } else { 0.0 });
    }
    if sum <= 0.0 {
        out[0] = (0, 1.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape<'a>(points: &'a [Vec3], own: &'a [Vec<(u16, f32)>], own_joints: usize, looseness: &'a [f32]) -> Shape<'a> {
        Shape { points, own, own_joints, looseness, center: Vec2::ZERO, top: 5.0, anchor: Affine3A::IDENTITY }
    }

    /// A cylinder of hair: 16 vertices around, every unit from z 10 down to z -10.
    fn cylinder() -> Vec<Vec3> {
        let mut v = Vec::new();
        for zi in 0..=20 {
            for a in 0..16 {
                let t = a as f32 / 16.0 * std::f32::consts::TAU;
                v.push(Vec3::new(t.cos() * 5.0, t.sin() * 5.0, 10.0 - zi as f32));
            }
        }
        v
    }

    #[test]
    fn hanging_hair_gets_chains_and_the_top_keeps_its_bone() {
        let pts = cylinder();
        let own = vec![vec![(0u16, 1.0)]; pts.len()];
        let g = generate(shape(&pts, &own, 1, &vec![1.0; pts.len()]), HAIR).expect("rigged");
        let anchors = g.bones.iter().filter(|b| !b.dynamic).count();
        assert_eq!(anchors, HAIR.sectors);
        assert!(g.bones.iter().all(|b| b.dynamic == b.parent.is_some()));
        assert_eq!(g.joints.len(), g.bones.len() - anchors);
        for (v, w) in pts.iter().zip(&g.weights) {
            let sum: f32 = w.iter().map(|p| p.1).sum();
            assert!((sum - 1.0).abs() < 1e-4);
            if v.z >= 5.0 {
                assert_eq!(w[0], (0, 1.0), "above the top the vertex stays on its bone");
            }
            if v.z <= 0.0 {
                assert!(w.iter().all(|p| p.0 != 0 || p.1 < 1e-4), "far below the top a vertex is all chain: {w:?}");
            }
        }
        // The lowest links reach the bottom.
        let low = g.bones.iter().map(|b| b.rest.translation.z).fold(f32::MAX, f32::min);
        assert!((low + 10.0).abs() < 1e-3);
    }

    #[test]
    fn short_hair_gets_nothing() {
        let pts: Vec<Vec3> = cylinder().into_iter().filter(|v| v.z > 3.0).collect();
        let own = vec![vec![(0u16, 1.0)]; pts.len()];
        assert!(generate(shape(&pts, &own, 1, &vec![1.0; pts.len()]), HAIR).is_none());
    }

    #[test]
    fn held_vertices_keep_their_own_weights() {
        let pts = cylinder();
        let own = vec![vec![(0u16, 0.5), (1u16, 0.5)]; pts.len()];
        let g = generate(shape(&pts, &own, 2, &vec![0.0; pts.len()]), DRAPE);
        assert!(g.is_none(), "nothing loose: no chains");
    }

    #[test]
    fn top4_merges_and_normalizes() {
        let t = top4(vec![(3, 0.2), (1, 0.1), (3, 0.2), (2, 0.05), (5, 0.3), (6, 0.01)]);
        assert_eq!(t[0].0, 3);
        let sum: f32 = t.iter().map(|p| p.1).sum();
        assert!((sum - 1.0).abs() < 1e-5);
        assert!(t.iter().all(|p| p.0 != 6));
    }
}
