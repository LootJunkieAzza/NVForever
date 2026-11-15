//! Landscape anti-tiling's shared pieces (nvforever `terrain.rs` "Anti-tiling";
//! docs/design/graphics-enhanced.md §3.13) [design, #2551]: the two shader modules, the
//! pipeline definitions both terrain draws use, and the CPU mirror of the plain-WGSL part
//! (in the tests).
//!
//! - `terrain_sampling.wesl` (plain WGSL, no imports or conditions): the triangle lattice,
//!   the corner hash, the Hex3 tap generator, the gradient
//!   reads of a texture at the taps and the normal decoding and rotation. nvforever's
//!   `terrain.rs` tests parse and validate this very file with naga (this crate has no naga).
//! - `terrain_pbr.wesl`: the surface set-up that takes the place of Bevy's
//!   `pbr_input_from_standard_material` for an anti-tiled landscape draw, and the reads of
//!   the StandardMaterial's own textures in both binding modes.
//!
//! Both are imported as `nvf_assets::terrain_sampling` and `nvf_assets::terrain_pbr`, only
//! by pipelines with `TERRAIN_ANTI_TILING` ([`anti_tiling_defs`]), so every other draw is
//! compiled as before.

use bevy::asset::uuid_handle;
use bevy::prelude::*;
use bevy::render::render_resource::RenderPipelineDescriptor;
use bevy::shader::{Shader, ShaderDefVal};

/// terrain_sampling.wesl.
pub const SAMPLING_SHADER: Handle<Shader> = uuid_handle!("0f6d3b2a-7e41-4c58-9a2d-25510a1e5c01");
/// terrain_pbr.wesl.
pub const PBR_SHADER: Handle<Shader> = uuid_handle!("6b9e1c47-3a20-4f8d-b5c3-25510a1e5c02");

/// The plain-WGSL module's source, for the naga test in nvforever's `terrain.rs`.
pub const SAMPLING_SOURCE: &str = include_str!("terrain_sampling.wesl");
/// The surface set-up module's source, for the contract tests.
pub const PBR_SOURCE: &str = include_str!("terrain_pbr.wesl");

/// Whether `repeat` (world units per texture repeat) can place the anti-tiled reads: a
/// finite number above 0. Anything else keeps the ordinary reads.
pub fn repeat_valid(repeat: f32) -> bool {
    repeat.is_finite() && repeat > 0.0
}

/// The main-pass fragment definitions of an anti-tiled landscape pipeline:
/// `TERRAIN_ANTI_TILING` and
/// `TERRAIN_SEPARATE` for the terrain's own NifMaterial draws (`separate`), which the
/// pipeline log reads to tell the two paths apart. The prepass and the shadows draw the same
/// depth either way and are left alone.
pub fn anti_tiling_defs(descriptor: &mut RenderPipelineDescriptor, separate: bool) {
    if descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE")) {
        return;
    }
    let Some(fragment) = descriptor.fragment.as_mut() else { return };
    fragment.shader_defs.push("TERRAIN_ANTI_TILING".into());
    if separate {
        fragment.shader_defs.push("TERRAIN_SEPARATE".into());
    }
}

#[cfg(test)]
mod tests {
    //! CPU mirror of terrain_sampling.wesl, with wrapping whole-number arithmetic and f32
    //! like the GPU, and an independent f64 reference of the lattice.
    use super::*;

    const H: f32 = 0.866_025_4;
    const LATTICE: f32 = 4.0;
    const SALTS: [u32; 3] = [0x2551_0001, 0x2551_0002, 0x2551_0003];
    const FAINT: f32 = 1.0 / 256.0;

    fn mix32(x: u32) -> u32 {
        let mut h = x;
        h ^= h >> 16;
        h = h.wrapping_mul(0x7feb_352d);
        h ^= h >> 15;
        h = h.wrapping_mul(0x846c_a68b);
        h ^ (h >> 16)
    }

    fn corner_hash(id: [i32; 2], salt: u32) -> u32 {
        mix32((id[0] as u32) ^ mix32((id[1] as u32) ^ 0x9e37_79b9 ^ mix32(salt)))
    }

    fn unit_float(h: u32) -> f32 {
        (h >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    #[derive(Debug, Clone, Copy)]
    struct Lattice {
        id: [[i32; 2]; 3],
        bary: [f32; 3],
        rel: [[f32; 2]; 3],
    }

    fn lattice(uv: [f32; 2]) -> Lattice {
        let p = [uv[0] / LATTICE, uv[1] / LATTICE];
        let q = [p[0] - p[1] / (2.0 * H), p[1] / H];
        let fl = [q[0].floor(), q[1].floor()];
        let cell = [fl[0] as i32, fl[1] as i32];
        let f = [q[0] - fl[0], q[1] - fl[1]];
        let (d, mut b) = if f[0] + f[1] > 1.0 { ([[1, 1], [0, 1], [1, 0]], [f[0] + f[1] - 1.0, 1.0 - f[0], 1.0 - f[1]]) } else { ([[0, 0], [1, 0], [0, 1]], [1.0 - f[0] - f[1], f[0], f[1]]) };
        for x in b.iter_mut() {
            *x = x.max(0.0);
        }
        let sum = b[0] + b[1] + b[2];
        for x in b.iter_mut() {
            *x /= sum;
        }
        let mut out = Lattice { id: [[0; 2]; 3], bary: b, rel: [[0.0; 2]; 3] };
        for (j, dj) in d.iter().enumerate() {
            out.id[j] = [cell[0] + dj[0], cell[1] + dj[1]];
            let e = [f[0] - dj[0] as f32, f[1] - dj[1] as f32];
            out.rel[j] = [LATTICE * (e[0] + 0.5 * e[1]), LATTICE * (H * e[1])];
        }
        out
    }

    type Mat2 = [[f32; 2]; 2]; // columns

    fn mul(m: Mat2, v: [f32; 2]) -> [f32; 2] {
        [m[0][0] * v[0] + m[1][0] * v[1], m[0][1] * v[0] + m[1][1] * v[1]]
    }

    fn transpose(m: Mat2) -> Mat2 {
        [[m[0][0], m[1][0]], [m[0][1], m[1][1]]]
    }

    fn rotation(r: u32) -> Mat2 {
        let c = [1.0, 0.5, -0.5, -1.0, -0.5, 0.5];
        let s = [0.0, H, H, 0.0, -H, -H];
        let i = r.min(5) as usize;
        [[c[i], s[i]], [-s[i], c[i]]]
    }

    const IDENTITY: Mat2 = [[1.0, 0.0], [0.0, 1.0]];

    #[derive(Debug, Clone, Copy)]
    struct Tap {
        uv: [f32; 2],
        rotation: Mat2,
        weight: f32,
        id: [i32; 2],
    }

    fn offset(id: [i32; 2]) -> [f32; 2] {
        [unit_float(corner_hash(id, SALTS[0])), unit_float(corner_hash(id, SALTS[1]))]
    }

    fn drop_faint(taps: &mut [Tap]) {
        let mut total = 0.0;
        for t in taps.iter_mut() {
            t.weight = (t.weight - FAINT).max(0.0);
            total += t.weight;
        }
        for t in taps.iter_mut() {
            t.weight /= total;
        }
    }

    fn hex3(uv: [f32; 2]) -> Vec<Tap> {
        let l = lattice(uv);
        let b4: Vec<f32> = l.bary.iter().map(|b| b * b * b * b).collect();
        let sum: f32 = b4.iter().sum();
        let mut taps: Vec<Tap> = (0..3)
            .map(|j| {
                let r = rotation(corner_hash(l.id[j], SALTS[2]) % 6);
                let o = offset(l.id[j]);
                let t = mul(r, l.rel[j]);
                Tap { uv: [t[0] + o[0], t[1] + o[1]], rotation: r, weight: b4[j] / sum, id: l.id[j] }
            })
            .collect();
        drop_faint(&mut taps);
        taps
    }

    fn normal_tap(encoded: [f32; 3], two_component: bool, flip_y: bool, rotation: Mat2) -> [f32; 3] {
        let mut n = [encoded[0] * 2.0 - 1.0, encoded[1] * 2.0 - 1.0, encoded[2] * 2.0 - 1.0];
        if two_component {
            n[2] = (1.0 - n[0] * n[0] - n[1] * n[1]).max(0.0).sqrt();
        }
        if flip_y {
            n[1] = -n[1];
        }
        let xy = mul(transpose(rotation), [n[0], n[1]]);
        [xy[0], xy[1], n[2]]
    }

    /// A smooth periodic "texture" (period 1 in u and v, as a repeating texture), standing in
    /// for a read: what the blend of the taps gives at `uv`.
    fn texel(uv: [f32; 2]) -> f32 {
        let tau = std::f32::consts::TAU;
        (tau * uv[0]).sin() * 0.5 + (tau * (uv[0] + 2.0 * uv[1])).cos() * 0.3 + (tau * 3.0 * uv[1]).sin() * 0.2
    }

    fn blended(taps: &[Tap]) -> f32 {
        taps.iter().filter(|t| t.weight > 0.0).map(|t| t.weight * texel(t.uv)).sum()
    }

    // Pseudo-random numbers for sweeps (synthetic, no game data).
    fn sweep(seed: u32, n: usize) -> Vec<[f32; 2]> {
        let mut s = seed;
        let mut next = || {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (s >> 8) as f32 / (1u32 << 24) as f32
        };
        (0..n).map(|_| [(next() - 0.5) * 1600.0, (next() - 0.5) * 1600.0]).collect()
    }

    #[test]
    fn the_hash_has_fixed_values_and_takes_negative_names_bit_for_bit() {
        // Golden values worked out independently (Python, 32-bit wrapping arithmetic).
        let golden: [([i32; 2], [u32; 3]); 5] = [
            ([0, 0], [0x44bb_6c5b, 0xe79c_2667, 0x6d5a_6038]),
            ([1, 0], [0xb0dd_c9b8, 0x5c96_ab18, 0x2eb0_f237]),
            ([-1, -1], [0xaf3d_3afd, 0xd5d6_0900, 0x78fa_c33e]),
            ([123, -456], [0xfb43_78b7, 0xd6e0_e0b1, 0x59b2_040c]),
            ([i32::MAX, i32::MIN], [0x7919_2a86, 0xb4ef_62cc, 0x2b75_9a95]),
        ];
        for (id, want) in golden {
            for (salt, w) in SALTS.iter().zip(want) {
                assert_eq!(corner_hash(id, *salt), w, "{id:?} {salt:#x}");
            }
        }
        assert_eq!((mix32(0), mix32(1)), (0, 0x6889_90c0));
        // No two of a corner's three hashes, nor a neighbour's, agree over a wide patch.
        let mut seen = std::collections::HashSet::new();
        for x in -40..=40 {
            for y in -40..=40 {
                for s in SALTS {
                    assert!(seen.insert(corner_hash([x, y], s)), "({x}, {y}) {s:#x}");
                }
            }
        }
        assert!(unit_float(u32::MAX) < 1.0 && unit_float(0) == 0.0);
    }

    #[test]
    fn the_shader_declares_what_the_mirror_uses() {
        let source = SAMPLING_SOURCE;
        for line in [
            "const TERRAIN_HEX_H: f32 = 0.8660254037844386;",
            "const TERRAIN_LATTICE_REPEATS: f32 = 4.0;",
            "const TERRAIN_SALT_OFFSET_X: u32 = 0x25510001u;",
            "const TERRAIN_SALT_OFFSET_Y: u32 = 0x25510002u;",
            "const TERRAIN_SALT_ROTATION: u32 = 0x25510003u;",
            "const TERRAIN_FAINT_TAP: f32 = 0.00390625;",
            "h *= 0x7feb352du;",
            "h *= 0x846ca68bu;",
            "return terrain_mix32(bitcast<u32>(id.x) ^ terrain_mix32(bitcast<u32>(id.y) ^ 0x9e3779b9u ^ terrain_mix32(salt)));",
            "var c = array<f32, 6>(1.0, 0.5, -0.5, -1.0, -0.5, 0.5);",
            "var s = array<f32, 6>(0.0, TERRAIN_HEX_H, TERRAIN_HEX_H, 0.0, -TERRAIN_HEX_H, -TERRAIN_HEX_H);",
            "return vec2<f32>(world.x, -world.z) / repeat;",
        ] {
            assert!(source.contains(line), "terrain_sampling.wesl must have `{line}` (the tests' mirror)");
        }
        assert_eq!(H, 0.866_025_403_784_438_6_f64 as f32);
        // Plain WGSL: nothing for the WESL linker to resolve, so naga can read it as it is.
        assert!(!source.contains("import ") && !source.contains("@if"));
    }

    #[test]
    fn the_lattice_weights_are_barycentric_everywhere() {
        let mut points = sweep(0x2551, 4000);
        // Lattice edges, vertices, the axes and both signs, and far out (about +-200,000
        // world units at the game's repeat: 781 repeats).
        for k in -8..=8 {
            let x = k as f32 * LATTICE * 0.5;
            points.extend([[x, 0.0], [0.0, x], [x, x * H], [-x, -x * H], [x + 1e-4, 1e-4], [x - 1e-4, -1e-4]]);
        }
        for far in [781.25f32, -781.25] {
            points.extend([[far, far], [far, -far], [far + 0.37, far - 0.11]]);
        }
        for uv in points {
            let l = lattice(uv);
            let sum: f32 = l.bary.iter().sum();
            assert!((sum - 1.0).abs() < 1e-5 && l.bary.iter().all(|b| b.is_finite() && *b >= 0.0), "{uv:?} {l:?}");
            // The corners are a lattice triangle (two neighbours apart on each axis).
            for j in 0..3 {
                for k in 0..3 {
                    assert!((l.id[j][0] - l.id[k][0]).abs() <= 1 && (l.id[j][1] - l.id[k][1]).abs() <= 1);
                }
            }
            // The relative coordinates are the point less each corner's place.
            for j in 0..3 {
                let c = [LATTICE * (l.id[j][0] as f32 + 0.5 * l.id[j][1] as f32), LATTICE * H * l.id[j][1] as f32];
                let tol = 1e-3 * (1.0 + uv[0].abs().max(uv[1].abs()) / 16.0);
                assert!((uv[0] - c[0] - l.rel[j][0]).abs() < tol && (uv[1] - c[1] - l.rel[j][1]).abs() < tol, "{uv:?} {j}: {l:?}");
            }
            let taps = hex3(uv);
            let w: f32 = taps.iter().map(|t| t.weight).sum();
            assert!((w - 1.0).abs() < 1e-5 && taps.iter().all(|t| t.weight.is_finite() && t.weight >= 0.0), "{uv:?} {taps:?}");
            assert!(taps.iter().any(|t| t.weight > 0.0));
        }
    }

    #[test]
    fn f32_agrees_with_an_f64_reference_far_out() {
        // The reference: the same lattice in f64, from the world position (as the shader
        // starts), for repeats 256 and 300, close to the origin and near +-200,000 units.
        let h64 = 3.0f64.sqrt() / 2.0;
        for repeat in [256.0f32, 300.0] {
            for base in [[1234.5f32, -987.25], [199_987.0, -200_011.5], [-200_003.25, 199_990.75]] {
                for (i, d) in sweep(7, 200).iter().enumerate() {
                    let world = [base[0] + d[0] * 0.1, base[1] + d[1] * 0.1];
                    let uv = [world[0] / repeat, world[1] / repeat];
                    let l = lattice(uv);
                    let uv64 = [world[0] as f64 / repeat as f64, world[1] as f64 / repeat as f64];
                    let q = [uv64[0] / 4.0 - uv64[1] / 4.0 / (2.0 * h64), uv64[1] / 4.0 / h64];
                    let f = [q[0] - q[0].floor(), q[1] - q[1].floor()];
                    // Points within rounding of an edge may land on either side; skip them.
                    if f[0] < 1e-4 || f[1] < 1e-4 || (f[0] + f[1] - 1.0).abs() < 1e-4 || f[0] > 1.0 - 1e-4 || f[1] > 1.0 - 1e-4 {
                        continue;
                    }
                    let cell = [q[0].floor() as i32, q[1].floor() as i32];
                    let upper = f[0] + f[1] > 1.0;
                    let first = if upper { [cell[0] + 1, cell[1] + 1] } else { cell };
                    assert_eq!(l.id[0], first, "repeat {repeat} point {i} {world:?}");
                    let b0 = if upper { f[0] + f[1] - 1.0 } else { 1.0 - f[0] - f[1] };
                    // f32 positions near 200,000 are 1/64 unit apart: about 0.03 texel of a
                    // 512-texel texture over 256 units; the weights stay within 1e-3.
                    assert!((l.bary[0] as f64 - b0).abs() < 1e-3, "{repeat} {world:?}: {} vs {b0}", l.bary[0]);
                    // A tap's coordinate stays small (relative to its corner) and within a
                    // small fraction of a texel of the f64 one (offsets added the same).
                    let e = [f[0] - (l.id[0][0] - cell[0]) as f64, f[1] - (l.id[0][1] - cell[1]) as f64];
                    let rel64 = [4.0 * (e[0] + 0.5 * e[1]), 4.0 * h64 * e[1]];
                    let err = ((l.rel[0][0] as f64 - rel64[0]).abs()).max((l.rel[0][1] as f64 - rel64[1]).abs());
                    assert!(err < 2e-3 && l.rel[0][0].abs() <= 8.0 && l.rel[0][1].abs() <= 8.0, "{repeat} {world:?}: {err}");
                }
            }
        }
    }

    #[test]
    fn hex3_is_continuous_and_shared_corners_read_the_same_place() {
        let step = [2e-4f32, 1.4e-4];
        let mut largest = 0.0f32;
        for uv in sweep(0xbeef, 3000) {
            let a = hex3(uv);
            let b = hex3([uv[0] + step[0], uv[1] + step[1]]);
            largest = largest.max((blended(&a) - blended(&b)).abs());
            // A corner both points keep reads the same texel (to rounding), moved by the
            // step turned with the corner's rotation.
            for ta in &a {
                if let Some(tb) = b.iter().find(|t| t.id == ta.id) {
                    let moved = mul(ta.rotation, step);
                    assert!((tb.uv[0] - ta.uv[0] - moved[0]).abs() < 1e-3 && (tb.uv[1] - ta.uv[1] - moved[1]).abs() < 1e-3, "{uv:?} {ta:?} {tb:?}");
                }
            }
        }
        // The stand-in texture's slope is at most about 11 per unit of uv, so a step of
        // 2.4e-4 moves a read by under 0.003, and the weights move about as little: a seam
        // would be a jump of tenths.
        assert!(largest < 0.01, "a jump of {largest} across a step of {step:?}");
        // Across the edge between a cell's two triangles too (the corner across from the
        // edge has weight 0 on it).
        for k in -5..5 {
            let on_edge = [LATTICE * (k as f32 + 0.25 + 0.5 * 0.75), LATTICE * H * 0.75];
            let below = hex3([on_edge[0], on_edge[1] - 1e-4]);
            let above = hex3([on_edge[0], on_edge[1] + 1e-4]);
            assert_ne!(lattice([on_edge[0], on_edge[1] - 1e-4]).id, lattice([on_edge[0], on_edge[1] + 1e-4]).id, "edge {k}: the two points lie in different triangles");
            assert!((blended(&below) - blended(&above)).abs() < 2e-3, "edge {k}");
        }
    }

    #[test]
    fn faint_taps_fade_to_nothing_and_one_always_stays() {
        let mut taps = vec![Tap { uv: [0.0; 2], rotation: IDENTITY, weight: 0.998, id: [0, 0] }, Tap { uv: [0.0; 2], rotation: IDENTITY, weight: 0.002, id: [1, 0] }];
        drop_faint(&mut taps);
        assert_eq!((taps[0].weight, taps[1].weight), (1.0, 0.0));
        // Just above the cut: a small weight that shrinks smoothly towards it.
        let mut a = vec![Tap { weight: 0.5, ..taps[0] }, Tap { weight: 0.3, ..taps[0] }, Tap { weight: 0.2, ..taps[0] }];
        drop_faint(&mut a);
        assert!((a.iter().map(|t| t.weight).sum::<f32>() - 1.0).abs() < 1e-6 && a[2].weight > 0.19);
        // Equal thirds survive.
        let mut b = vec![Tap { weight: 1.0 / 3.0, ..taps[0] }; 3];
        drop_faint(&mut b);
        assert!(b.iter().all(|t| (t.weight - 1.0 / 3.0).abs() < 1e-6));
        // At a lattice vertex Hex3 reads one place only.
        let v = hex3([0.0, 0.0]);
        assert_eq!(v.iter().filter(|t| t.weight > 0.0).count(), 1);
    }

    #[test]
    fn rotations_turn_normals_back_with_the_texture() {
        for r in 0..6 {
            let m = rotation(r);
            // A rotation: unit columns at right angles, determinant 1, by r x 60 degrees.
            let det = m[0][0] * m[1][1] - m[1][0] * m[0][1];
            assert!((det - 1.0).abs() < 1e-6, "{r}");
            let angle = (r as f32) * std::f32::consts::FRAC_PI_3;
            assert!((m[0][0] - angle.cos()).abs() < 1e-6 && (m[0][1] - angle.sin()).abs() < 1e-6, "{r}");
            // A flat normal stays flat.
            let flat = normal_tap([0.5, 0.5, 1.0], false, true, m);
            assert!(flat[0].abs() < 1e-6 && flat[1].abs() < 1e-6 && (flat[2] - 1.0).abs() < 1e-6);
            // A slope in the texture, read through rotation m, tilts the surface's normal
            // the way the surface's own slope runs: the texture's height h(t) = a . t read
            // at t = m s has the surface gradient m^T a, and the normal leans against it.
            let a = [0.3f32, -0.2];
            let encoded_tilt = [-a[0], -a[1]]; // the normal leans against the gradient
            let n = normal_tap([encoded_tilt[0] * 0.5 + 0.5, encoded_tilt[1] * 0.5 + 0.5, 1.0], false, false, m);
            let surface_gradient = mul(transpose(m), a);
            assert!((n[0] + surface_gradient[0]).abs() < 1e-5 && (n[1] + surface_gradient[1]).abs() < 1e-5, "{r}: {n:?} {surface_gradient:?}");
        }
        // The green flip comes before the turn: a +y texel with the flip leans to -y in
        // the texture frame, and the turn back (60 degrees here) takes that, not +y, round.
        let m = rotation(1);
        let flipped = normal_tap([0.5, 1.0, 0.5], false, true, m);
        let expect = mul(transpose(m), [0.0, -1.0]);
        assert!((flipped[0] - expect[0]).abs() < 1e-6 && (flipped[1] - expect[1]).abs() < 1e-6);
        // Two-component maps rebuild z from x and y (and hold it at 0 past the unit disc).
        let two = normal_tap([0.8, 0.5, 0.0], true, false, IDENTITY);
        assert!((two[2] - (1.0f32 - 0.6 * 0.6).sqrt()).abs() < 1e-6);
        assert_eq!(normal_tap([1.0, 1.0, 0.0], true, false, IDENTITY)[2], 0.0);
        // A three-component map keeps its own z.
        assert!((normal_tap([0.5, 0.5, 0.75], false, false, IDENTITY)[2] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_pipeline_definitions_cover_both_paths_and_leave_the_prepass_alone() {
        use bevy::render::render_resource::FragmentState;
        let main = || RenderPipelineDescriptor { fragment: Some(FragmentState { shader_defs: vec!["VERTEX_UVS_A".into()], ..default() }), ..default() };
        let defs = |d: &RenderPipelineDescriptor| d.fragment.as_ref().unwrap().shader_defs.clone();
        let mut d = main();
        anti_tiling_defs(&mut d, false);
        assert_eq!(defs(&d), vec![ShaderDefVal::from("VERTEX_UVS_A"), "TERRAIN_ANTI_TILING".into()]);
        let mut d = main();
        anti_tiling_defs(&mut d, true);
        assert_eq!(defs(&d), vec![ShaderDefVal::from("VERTEX_UVS_A"), "TERRAIN_ANTI_TILING".into(), "TERRAIN_SEPARATE".into()]);
        let mut prepass = main();
        prepass.vertex.shader_defs.push("PREPASS_PIPELINE".into());
        anti_tiling_defs(&mut prepass, true);
        assert_eq!(defs(&prepass), vec![ShaderDefVal::from("VERTEX_UVS_A")], "the prepass draws the same depth either way");
        assert!(repeat_valid(256.0) && repeat_valid(300.0) && !repeat_valid(0.0) && !repeat_valid(-256.0) && !repeat_valid(f32::NAN) && !repeat_valid(f32::INFINITY));
    }

    #[test]
    fn the_surface_set_up_reads_only_the_landscape_materials_textures() {
        // terrain_pbr.wesl's contract (its header): diffuse texture, normal map, colours and
        // scalar values; no other StandardMaterial texture and no lightmap. nvforever's
        // terrain.rs checks the landscape materials hold nothing else. A StandardMaterial
        // binding is reached as `pbr_bindings::<name>` (bound one by one) or as a field of
        // the bindless index table (`.<name>`), so those exact references are looked for:
        // a bare substring would also match the view's SSAO texture,
        // `screen_space_ambient_occlusion_texture`, which the set-up does read.
        for absent in [
            "emissive_texture",
            "metallic_roughness_texture",
            "occlusion_texture",
            "depth_map_texture",
            "clearcoat_texture",
            "clearcoat_normal_texture",
            "clearcoat_roughness_texture",
            "anisotropy_texture",
            "specular_transmission_texture",
            "thickness_texture",
            "diffuse_transmission_texture",
            "specular_texture",
            "specular_tint_texture",
        ] {
            for reference in [format!("pbr_bindings::{absent}"), format!(".{absent}")] {
                assert!(!PBR_SOURCE.contains(&reference), "terrain_pbr.wesl reads `{reference}`: widen the contract (and its test) first");
            }
        }
        assert!(!PBR_SOURCE.contains("lightmap("), "terrain_pbr.wesl reads a lightmap: widen the contract first");
        // The references the set-up does make, in both binding modes.
        for present in ["pbr_bindings::base_color_texture", "pbr_bindings::normal_map_texture", ".base_color_texture", ".normal_map_texture", "screen_space_ambient_occlusion_texture"] {
            assert!(PBR_SOURCE.contains(present), "{present}");
        }
        for read in ["base_color_texture", "normal_map_texture", "base_color_sampler", "normal_map_sampler", "ssao_multibounce", "pbr_input_from_vertex_output"] {
            assert!(PBR_SOURCE.contains(read), "{read}");
        }
        // The bindless tables are indexed where they are read (non-uniform marking).
        assert!(!PBR_SOURCE.contains("terrain_sample_diffuse(bindless") && !PBR_SOURCE.contains("terrain_sample_normal(bindless"));
    }
}
