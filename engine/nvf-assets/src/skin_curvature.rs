//! Per-vertex skin curvature for pre-integrated skin scattering (`NVF_SKIN_SSS`,
//! nvforever `sss.wesl`): how tightly the surface bends at each vertex, in mm⁻¹, written
//! to the mesh's second texture-coordinate set (`UV_1`, x; y is 0), which no NIF shape
//! uses otherwise. `nif_material.wesl` reads it in place of its old screen-space estimate
//! (normal change over position change across a pixel), which was noisy, depended on the
//! view, and had to be clamped so hard (0.15 to 0.18 mm⁻¹) that every part of a face got
//! the same scattering width.
//!
//! The estimate [engineering choice]: vertices at the same position are welded (texture
//! seams split them), and each edge of each triangle gives the normal curvature along it,
//! `(n_b − n_a)·(p_b − p_a) / |p_b − p_a|²`, which is exactly 1/R on a sphere of radius R.
//! A vertex takes the mean of the magnitudes over its edges (a saddle, such as the side of
//! the nose, scatters as much as a bump), then one smoothing step half-way to its
//! neighbours' mean, because low-polygon heads give a jagged field otherwise.
//!
//! Units: mesh positions are game units, taken as 14.3 mm each, the figure `sss.wesl`
//! already uses for its shadow-map thickness [hypothesis: 70 units per metre, the scale combat-rules.md's gravity uses]. A shape
//! scaled by its node keeps its unscaled curvature (skin shapes are at scale 1 in the
//! heads, bodies and hands checked [hypothesis]).

use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use std::collections::HashMap;

/// Millimetres per game unit (`sss.wesl`, `nvf_dir_skin_thickness_mm`).
pub const MM_PER_UNIT: f32 = 14.3;

/// Positions closer than this (game units) are one vertex for the estimate.
const WELD: f32 = 1e-3;

/// Writes the curvature to `mesh`'s `UV_1`. Leaves a mesh that already has a `UV_1`, or
/// has no positions or normals, as it was; returns whether it wrote.
pub fn bake(mesh: &mut Mesh) -> bool {
    if mesh.attribute(Mesh::ATTRIBUTE_UV_1).is_some() {
        return false;
    }
    let Some(curvature) = curvature_mm(mesh) else { return false };
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, curvature.into_iter().map(|k| [k, 0.0]).collect::<Vec<[f32; 2]>>());
    true
}

/// The per-vertex curvature (mm⁻¹) of `mesh`, one value per vertex, or `None` without
/// positions or normals of the same length.
pub fn curvature_mm(mesh: &Mesh) -> Option<Vec<f32>> {
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else { return None };
    let count = positions.len();
    if count == 0 || normals.len() != count {
        return None;
    }

    // Weld by quantised position; a welded vertex's normal is its members' mean.
    let mut ids = HashMap::<[i64; 3], usize>::new();
    let mut weld = Vec::with_capacity(count);
    let mut pos = Vec::<Vec3>::new();
    let mut nor = Vec::<Vec3>::new();
    for (p, n) in positions.iter().zip(normals) {
        let key = p.map(|c| (c / WELD).round() as i64);
        let id = *ids.entry(key).or_insert_with(|| {
            pos.push(Vec3::from_array(*p));
            nor.push(Vec3::ZERO);
            pos.len() - 1
        });
        nor[id] += Vec3::from_array(*n);
        weld.push(id);
    }
    for n in &mut nor {
        *n = n.normalize_or_zero();
    }

    let tris: Vec<[usize; 3]> = match mesh.indices() {
        Some(Indices::U32(i)) => i.chunks_exact(3).map(|t| [t[0] as usize, t[1] as usize, t[2] as usize]).collect(),
        Some(Indices::U16(i)) => i.chunks_exact(3).map(|t| [t[0] as usize, t[1] as usize, t[2] as usize]).collect(),
        None => (0..count / 3).map(|t| [3 * t, 3 * t + 1, 3 * t + 2]).collect(),
    };

    let n = pos.len();
    let mut sum = vec![0.0f32; n];
    let mut edges = vec![0u32; n];
    let mut links: Vec<(usize, usize)> = Vec::with_capacity(tris.len() * 3);
    for t in tris {
        if t.iter().any(|&v| v >= count) {
            continue;
        }
        let w = t.map(|v| weld[v]);
        for (a, b) in [(w[0], w[1]), (w[1], w[2]), (w[2], w[0])] {
            if a == b || nor[a] == Vec3::ZERO || nor[b] == Vec3::ZERO {
                continue;
            }
            let d = pos[b] - pos[a];
            let len2 = d.length_squared();
            if len2 < WELD * WELD {
                continue;
            }
            let k = ((nor[b] - nor[a]).dot(d) / len2).abs();
            for v in [a, b] {
                sum[v] += k;
                edges[v] += 1;
            }
            links.push((a, b));
        }
    }
    let raw: Vec<f32> = sum.iter().zip(&edges).map(|(s, &e)| if e > 0 { s / e as f32 } else { 0.0 }).collect();

    // One smoothing step: half-way to the neighbours' mean.
    let mut near = vec![0.0f32; n];
    let mut near_count = vec![0u32; n];
    for &(a, b) in &links {
        near[a] += raw[b];
        near[b] += raw[a];
        near_count[a] += 1;
        near_count[b] += 1;
    }
    let smooth: Vec<f32> = (0..n)
        .map(|v| if near_count[v] > 0 { 0.5 * raw[v] + 0.5 * near[v] / near_count[v] as f32 } else { raw[v] })
        .collect();

    Some(weld.iter().map(|&w| smooth[w] / MM_PER_UNIT).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::PrimitiveTopology;

    /// A UV sphere of radius `r` with smooth normals and a seam (the first and last
    /// columns are separate vertices at the same positions, as a texture seam leaves them).
    fn sphere(r: f32, rings: usize, segments: usize) -> Mesh {
        let mut p = Vec::new();
        let mut nrm = Vec::new();
        for i in 0..=rings {
            let th = std::f32::consts::PI * i as f32 / rings as f32;
            for j in 0..=segments {
                let ph = std::f32::consts::TAU * j as f32 / segments as f32;
                let n = Vec3::new(th.sin() * ph.cos(), th.sin() * ph.sin(), th.cos());
                p.push((n * r).to_array());
                nrm.push(n.to_array());
            }
        }
        let mut idx = Vec::new();
        let row = segments as u32 + 1;
        for i in 0..rings as u32 {
            for j in 0..segments as u32 {
                let a = i * row + j;
                idx.extend([a, a + row, a + 1, a + 1, a + row, a + row + 1]);
            }
        }
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, p);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
        m.insert_indices(Indices::U32(idx));
        m
    }

    #[test]
    fn a_sphere_gets_one_over_its_radius_everywhere() {
        for r in [0.5f32, 2.0, 6.5] {
            let k = curvature_mm(&sphere(r, 16, 24)).unwrap();
            let want = 1.0 / (r * MM_PER_UNIT);
            for (i, v) in k.iter().enumerate() {
                assert!((v - want).abs() < 0.03 * want, "r={r} vertex {i}: {v} want {want}");
            }
        }
    }

    #[test]
    fn a_smaller_sphere_bends_more_and_a_plane_not_at_all() {
        let big = curvature_mm(&sphere(6.0, 12, 16)).unwrap()[40];
        let small = curvature_mm(&sphere(0.6, 12, 16)).unwrap()[40];
        assert!(small > 5.0 * big, "small {small} big {big}");

        let mut plane = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        plane.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]]);
        plane.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 4]);
        plane.insert_indices(Indices::U16(vec![0, 1, 2, 0, 2, 3]));
        assert!(curvature_mm(&plane).unwrap().iter().all(|&k| k == 0.0));
    }

    #[test]
    fn bake_writes_uv1_once_and_skips_meshes_without_normals() {
        let mut m = sphere(1.0, 6, 8);
        let n = m.count_vertices();
        assert!(bake(&mut m));
        let Some(VertexAttributeValues::Float32x2(uv)) = m.attribute(Mesh::ATTRIBUTE_UV_1) else { panic!("no UV_1") };
        assert_eq!(uv.len(), n);
        assert!(uv.iter().all(|c| c[0] > 0.0 && c[1] == 0.0));
        assert!(!bake(&mut m), "a mesh with a UV_1 keeps it");

        let mut bare = sphere(1.0, 6, 8);
        bare.remove_attribute(Mesh::ATTRIBUTE_NORMAL);
        assert!(!bake(&mut bare));
        assert!(bare.attribute(Mesh::ATTRIBUTE_UV_1).is_none());
    }
}
