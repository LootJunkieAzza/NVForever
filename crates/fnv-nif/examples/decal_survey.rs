//! Diagnostic survey of the decal bits of BSShaderProperty on visible triangle shapes, for
//! deciding how the renderers draw shapes that lie on another surface. BSShaderFlags bit
//! 26 is `decal_single_pass` and bit 27 `dynamic_decal_single_pass`; BSShaderFlags2 bit
//! 21 is `alpha_decal` [documented: nif.xml].
//!
//! Prints
//! - the shapes by decal bits, shader property type and blend class (from
//!   NiAlphaProperty), with example files;
//! - how far each shape lies from the other shapes of its file: for up to 256 of its
//!   triangles, the distance from the triangle's centre to the nearest triangle of any
//!   other visible shape without a decal bit, in the file's own space and units (centres,
//!   not vertices: a pane welded to its frame shares vertices with it without lying on
//!   it). Per shape the median and the 90th percentile of those distances are binned, for
//!   flagged shapes and, as a control, for blended shapes without a decal bit;
//! - the side the flagged shapes lie on: the median of the signed distance along the
//!   normal of the nearest triangle (counter-clockwise front), over centres within one
//!   unit of it.
//!
//! With `--list` as the first argument every flagged shape is printed as well.
//!
//! cargo run --release -p fnv-nif --example decal_survey -- [--list] <schema dir> <archive.bsa>...

use fnv_nif::scene::{mesh_data, scene_objects, Decoded, SceneObject, Transform};
use fnv_nif::{NifFile, Schema};
use std::collections::{BTreeMap, HashMap};

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn madd(a: V3, b: V3, s: f64) -> V3 {
    [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s]
}

/// p' = R·p·scale + t, the rotation stored row by row (docs/spec-notes/nif.md §6).
fn apply(t: &Transform, p: V3) -> V3 {
    let r = t.rotation.map(f64::from);
    let s = f64::from(t.scale);
    std::array::from_fn(|i| (r[3 * i] * p[0] + r[3 * i + 1] * p[1] + r[3 * i + 2] * p[2]) * s + f64::from(t.translation[i]))
}

/// The point of triangle `t` nearest to `p` (Ericson, Real-Time Collision Detection §5.1.5).
fn closest(p: V3, t: &[V3; 3]) -> V3 {
    let [a, b, c] = *t;
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return madd(a, ab, d1 / (d1 - d3));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return madd(a, ac, d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return madd(b, sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = va + vb + vc;
    if denom.abs() < 1e-30 {
        return a;
    }
    madd(madd(a, ab, vb / denom), ac, vc / denom)
}

struct Shape {
    name: String,
    /// Triangles in the file's space.
    triangles: Vec<[V3; 3]>,
    shader: Option<(String, i64, i64, i64)>,
    blend: &'static str,
    textured: bool,
    vertex_colors: bool,
}

impl Shape {
    fn decal_bits(&self) -> i64 {
        self.shader.as_ref().map_or(0, |s| (s.2 >> 26) & 3)
    }
}

/// Blend class of NiAlphaProperty flags (bit 0 blend, bits 1-4 source factor, bits 5-8
/// destination factor, bit 9 test [documented: nif.xml AlphaFlags]).
fn blend_class(alpha: Option<i64>) -> &'static str {
    let Some(f) = alpha else { return "opaque" };
    let (blend, test, src, dst) = (f & 1 != 0, f & 0x200 != 0, (f >> 1) & 0xF, (f >> 5) & 0xF);
    match (blend, test, src, dst) {
        (false, false, ..) => "opaque",
        (false, true, ..) => "test",
        (true, _, 6, 7) => "blend",
        (true, _, 6, 0) => "add",
        (true, _, 1, 2) => "multiply",
        (true, ..) => "other blend",
    }
}

fn bin(d: f64) -> &'static str {
    match d {
        d if d <= 0.001 => "a  <= 0.001",
        d if d <= 0.01 => "b  <= 0.01",
        d if d <= 0.1 => "c  <= 0.1",
        d if d <= 1.0 => "d  <= 1",
        d if d <= 10.0 => "e  <= 10",
        _ => "f  > 10",
    }
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn main() {
    let mut a: Vec<String> = std::env::args().skip(1).collect();
    let list = a.first().is_some_and(|x| x == "--list");
    if list {
        a.remove(0);
    }
    let schema = Schema::load_dir(std::path::Path::new(&a[0])).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut flags2: BTreeMap<String, usize> = BTreeMap::new();
    let mut dirs: BTreeMap<String, usize> = BTreeMap::new();
    // (group, "median" | "p90", bin) → shapes
    let mut distance: BTreeMap<(String, &'static str, &'static str), usize> = BTreeMap::new();
    let mut side: BTreeMap<(String, &'static str), usize> = BTreeMap::new();
    let mut files_with_flagged: BTreeMap<usize, usize> = BTreeMap::new();
    let mut lines: Vec<String> = Vec::new();
    for arc in &a[1..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif")).collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            let dec = Decoded::new(&file, &data, &schema);
            let objects = scene_objects(&file, &dec);
            let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
            let mut shapes: Vec<Shape> = Vec::new();
            for o in objects.iter().filter(|o| !o.is_node) {
                let mut chain = vec![o.block];
                let mut cur = o.parent;
                while let Some(p) = cur {
                    if chain.len() > 512 {
                        break;
                    }
                    chain.push(p);
                    cur = by_block.get(&p).and_then(|x| x.parent);
                }
                let hidden = chain.iter().any(|&b| {
                    let s = dec.blocks[b].as_ref().ok();
                    let name = s.and_then(|s| s.text("name")).unwrap_or("");
                    s.and_then(|s| s.int("flags")).unwrap_or(0) & 1 != 0
                        || (name.len() >= 12 && name[..12].eq_ignore_ascii_case("editormarker"))
                });
                if hidden {
                    continue;
                }
                // Ancestors first, the shape's own properties last (they win).
                let (mut alpha, mut shader, mut textured) = (None, None, false);
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        match file.blocks[r].type_name.as_str() {
                            "NiAlphaProperty" => alpha = s.int("flags"),
                            "NiTexturingProperty" => textured = true,
                            t if dec.is_a(r, "BSShaderProperty") => {
                                textured = s.block_ref("texture_set").and_then(|t| dec.get(t)).is_some()
                                    || s.text("file_name").is_some_and(|f| !f.is_empty());
                                shader = Some((
                                    t.to_string(),
                                    s.int("shader_type").unwrap_or(-1),
                                    s.int("shader_flags").unwrap_or(0),
                                    s.int("shader_flags_2").unwrap_or(0),
                                ))
                            }
                            _ => {}
                        }
                    }
                }
                let Some(md) = dec.blocks[o.block].as_ref().ok().and_then(|s| s.block_ref("data")).and_then(|d| dec.get(d)).map(mesh_data)
                else {
                    continue;
                };
                let positions: Vec<V3> = md
                    .positions
                    .iter()
                    .map(|p| chain.iter().fold(p.map(f64::from), |p, b| apply(&by_block[b].transform, p)))
                    .collect();
                let triangles = md
                    .triangles
                    .iter()
                    .filter(|t| t.iter().all(|&i| (i as usize) < positions.len()))
                    .map(|t| t.map(|i| positions[i as usize]))
                    .collect();
                shapes.push(Shape {
                    name: o.name.clone().unwrap_or_default(),
                    triangles,
                    shader,
                    blend: blend_class(alpha),
                    textured,
                    vertex_colors: !md.colors.is_empty(),
                });
            }

            let flagged = shapes.iter().filter(|s| s.decal_bits() != 0).count();
            if flagged > 0 {
                *files_with_flagged.entry(flagged.min(5)).or_default() += 1;
            }
            for (i, s) in shapes.iter().enumerate() {
                let Some((ty, shader_type, f1, f2)) = &s.shader else { continue };
                let bits = s.decal_bits();
                let bits_name = ["none", "26", "27", "26+27"][bits as usize];
                let key = format!("decal bits {bits_name:5} {ty:28} {:11}", s.blend);
                *counts.entry(key.clone()).or_default() += 1;
                let ex = examples.entry(key).or_default();
                if ex.len() < 2 {
                    ex.push(format!("{path} :: {:?}", s.name));
                }
                if bits != 0 || f2 & (1 << 21) != 0 {
                    let k = format!(
                        "decal bits {bits_name:5} | flags2 alpha_decal {} zbuffer_write {} | flags zbuffer_test {} | {} {}",
                        (f2 >> 21) & 1,
                        f2 & 1,
                        (f1 >> 31) & 1,
                        if s.textured { "textured" } else { "untextured" },
                        if s.vertex_colors { "vertex colours" } else { "no vertex colours" },
                    );
                    *flags2.entry(k).or_default() += 1;
                }
                if bits != 0 {
                    let dir: Vec<&str> = path.split('\\').collect();
                    *dirs.entry(dir[..dir.len().saturating_sub(1).min(3)].join("\\")).or_default() += 1;
                }
                // Distances: flagged shapes, and blended shapes without the bits as a control.
                if bits == 0 && matches!(s.blend, "opaque" | "test") {
                    continue;
                }
                let group = if bits != 0 { format!("flagged, {}", s.blend) } else { format!("not flagged, {}", s.blend) };
                let under: Vec<&[V3; 3]> = shapes
                    .iter()
                    .enumerate()
                    .filter(|(j, u)| *j != i && u.decal_bits() == 0)
                    .flat_map(|(_, u)| u.triangles.iter())
                    .collect();
                if under.is_empty() || s.triangles.is_empty() {
                    *distance.entry((group.clone(), "median", "g  no other shape")).or_default() += 1;
                    *distance.entry((group, "p90", "g  no other shape")).or_default() += 1;
                    continue;
                }
                let boxes: Vec<(V3, V3)> = under
                    .iter()
                    .map(|t| (std::array::from_fn(|k| t[0][k].min(t[1][k]).min(t[2][k])), std::array::from_fn(|k| t[0][k].max(t[1][k]).max(t[2][k]))))
                    .collect();
                let step = s.triangles.len().div_ceil(256);
                let (mut dists, mut signed) = (Vec::new(), Vec::new());
                for t in s.triangles.iter().step_by(step) {
                    let p: &V3 = &std::array::from_fn(|k| (t[0][k] + t[1][k] + t[2][k]) / 3.0);
                    let (mut best, mut best_signed) = (f64::INFINITY, 0.0);
                    for (t, (lo, hi)) in under.iter().zip(&boxes) {
                        let to_box: f64 = (0..3).map(|k| (lo[k] - p[k]).max(p[k] - hi[k]).max(0.0).powi(2)).sum();
                        if to_box >= best {
                            continue;
                        }
                        let d = sub(*p, closest(*p, t));
                        let d2 = dot(d, d);
                        if d2 < best {
                            let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
                            let len = dot(n, n).sqrt();
                            best = d2;
                            best_signed = if len > 0.0 { dot(d, n) / len } else { 0.0 };
                        }
                    }
                    dists.push(best.sqrt());
                    if best.sqrt() <= 1.0 {
                        signed.push(best_signed);
                    }
                }
                dists.sort_by(f64::total_cmp);
                signed.sort_by(f64::total_cmp);
                let (median, p90) = (percentile(&dists, 0.5), percentile(&dists, 0.9));
                *distance.entry((group.clone(), "median", bin(median))).or_default() += 1;
                *distance.entry((group.clone(), "p90", bin(p90))).or_default() += 1;
                let offset = (!signed.is_empty()).then(|| percentile(&signed, 0.5));
                if bits != 0 {
                    let class = match offset {
                        None => "no centre within 1 unit",
                        Some(o) if o > 0.6 => "in front, 0.6 to 1",
                        Some(o) if o > 0.3 => "in front, 0.3 to 0.6",
                        Some(o) if o > 0.15 => "in front, 0.15 to 0.3",
                        Some(o) if o > 0.05 => "in front, 0.05 to 0.15",
                        Some(o) if o > 0.001 => "in front, 0.001 to 0.05",
                        Some(o) if o < -0.001 => "behind (< -0.001)",
                        Some(_) => "in the plane (within 0.001)",
                    };
                    *side.entry((s.blend.to_string(), class)).or_default() += 1;
                    if list {
                        lines.push(format!(
                            "{path} :: {:?} | {ty} type {shader_type} flags {f1:#010x} flags2 {f2:#010x} | {} | {} triangles | distance median {median:.4} p90 {p90:.4} max {:.4} | signed median {}",
                            s.name,
                            s.blend,
                            s.triangles.len(),
                            dists[dists.len() - 1],
                            offset.map_or("-".to_string(), |o| format!("{o:+.4}")),
                        ));
                    }
                }
            }
        }
    }
    println!("visible shapes with a shader property, by decal bits, shader type and blend class:");
    for (k, c) in &counts {
        println!("{c:7}  {k}");
        for e in examples.get(k).into_iter().flatten().filter(|_| !k.contains("none")) {
            println!("           {e}");
        }
    }
    println!("\nshapes with a decal bit or alpha_decal, by the other depth flags:");
    for (k, c) in &flags2 {
        println!("{c:7}  {k}");
    }
    println!("\nflagged shapes by directory:");
    let mut v: Vec<_> = dirs.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, c) in v.iter().take(25) {
        println!("{c:7}  {k}");
    }
    println!("\nfiles by number of flagged shapes (5 = 5 or more): {files_with_flagged:?}");
    println!("\ndistance to the nearest other shape without a decal bit, shapes per bin:");
    for ((group, stat, b), c) in &distance {
        println!("{c:7}  {group:28} {stat:6} {b}");
    }
    println!("\nside of the nearest triangle (median signed distance of the centres within 1 unit):");
    for ((blend, class), c) in &side {
        println!("{c:7}  flagged, {blend:11} {class}");
    }
    lines.sort();
    for l in &lines {
        println!("  {l}");
    }
}
