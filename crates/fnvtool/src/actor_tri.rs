//! `fnvtool actor tri` (fnv.actor.tri/1): the FaceGen `.tri` files of an install — their
//! morph labels, and whether each sits on the model beside it (same vertices in the same
//! order), which is what lets a morph be added to the drawn mesh
//! (docs/spec-notes/lipsync.md §4).

use fnv_actor::tri::Tri;
use fnv_nif::scene::{mesh_data, Decoded, SHAPE_TYPES};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

/// A triangle's corners, lowest first, so that the same three vertices compare equal.
fn corners(mut t: [u32; 3]) -> [u32; 3] {
    t.sort_unstable();
    t
}

/// A shape of a model: name, vertices, triangles.
type Shape = (Option<String>, Vec<[f32; 3]>, Vec<[u32; 3]>);

/// Every triangle shape of a model.
fn shapes(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, path: &str) -> Result<Vec<Shape>, String> {
    let data = match vfs.read(path) {
        None => return Err("no model beside it".to_string()),
        Some(Err(e)) => return Err(e.to_string()),
        Some(Ok(d)) => d,
    };
    let file = fnv_nif::file::NifFile::parse(&data, Some(nif)).map_err(|e| e.to_string())?;
    let dec = Decoded::new(&file, &data, nif);
    let mut out = Vec::new();
    for (i, b) in file.blocks.iter().enumerate() {
        if !SHAPE_TYPES.contains(&b.type_name.as_str()) {
            continue;
        }
        let Ok(s) = &dec.blocks[i] else { continue };
        let Some(d) = s.block_ref("data").and_then(|r| dec.get(r)) else { continue };
        let md = mesh_data(d);
        out.push((s.text("name").map(str::to_string), md.positions, md.triangles));
    }
    Ok(out)
}

/// One file's row. `detail` adds each morph's size.
fn row(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, path: &str, detail: bool) -> Value {
    let tri = match vfs.read(path).and_then(Result::ok).ok_or("unreadable".to_string()).and_then(|b| Tri::parse(&b).map_err(|e| e.to_string())) {
        Ok(t) => t,
        Err(e) => return json!({"path": path, "error": e}),
    };
    let model = format!("{}.nif", &path[..path.len() - 4]);
    let fit = match shapes(vfs, nif, &model) {
        Err(e) => json!({"model": model, "error": e}),
        Ok(shapes) => {
            // The shape the file sits on: the one with V vertices (the first, if several).
            let hit = shapes.iter().find(|(_, p, _)| p.len() == tri.vertices).map(|(n, p, t)| ((n, p), t)).map(|(a, t)| ((a.0, a.1), t));
            // Same vertex order: the model's triangles join the same vertex numbers as
            // triangles of the file (which may list more: 18 files do).
            let shared = hit.map(|(_, t)| {
                let file: HashSet<[u32; 3]> = tri.triangles.iter().map(|&x| corners(x)).collect();
                t.iter().filter(|&&x| file.contains(&corners(x))).count()
            });
            let model_triangles = hit.map(|(_, t)| t.len());
            let hit = hit.map(|(a, _)| a);
            let distances: Option<Vec<f32>> =
                hit.map(|(_, p)| p.iter().zip(&tri.positions).map(|(a, b)| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt()).collect());
            let worst = distances.as_ref().map(|d| d.iter().copied().fold(0.0f32, f32::max));
            let off = distances.as_ref().map(|d| d.iter().filter(|&&x| x > 0.01).count());
            // Same order: for how many of the model's vertices is the file's vertex of the
            // same index the nearest one (ties count).
            let d2 = |a: &[f32; 3], b: &[f32; 3]| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>();
            let same_order = hit.map(|(_, p)| {
                let base = &tri.positions[..tri.vertices];
                p.iter().enumerate().filter(|(i, a)| base.iter().all(|b| d2(a, b) >= d2(a, &base[*i]))).count()
            });
            json!({
                "model": model,
                "shapes": shapes.iter().map(|(n, p, _)| json!({"name": n, "vertices": p.len()})).collect::<Vec<_>>(),
                "model_triangles": model_triangles,
                "model_triangles_found_in_the_file": shared,
                "shape": hit.map(|(n, _)| n),
                "same_vertex_count": hit.is_some(),
                "largest_vertex_distance": worst,
                "vertices_farther_than_0.01": off,
                "vertices_nearest_to_their_own_index": same_order,
            })
        }
    };
    let morphs: Vec<Value> = tri
        .morphs
        .iter()
        .map(|m| {
            if !detail {
                return json!(m.label);
            }
            let moved = m.deltas.iter().filter(|d| **d != [0, 0, 0]).count();
            let far = m.deltas.iter().map(|d| d.iter().map(|&c| (f32::from(c) * m.scale).powi(2)).sum::<f32>().sqrt()).fold(0.0f32, f32::max);
            json!({"label": m.label, "scale": m.scale, "vertices_moved": moved, "largest_displacement": far})
        })
        .collect();
    let modifiers: Vec<Value> = tri
        .modifiers
        .iter()
        .map(|m| {
            if !detail {
                return json!(m.label);
            }
            let far = tri.displacements(&m.label, None).unwrap_or_default().iter().map(|(_, d)| d.iter().map(|c| c * c).sum::<f32>().sqrt()).fold(0.0f32, f32::max);
            json!({"label": m.label, "vertices_moved": m.indices.len(), "largest_displacement": far})
        })
        .collect();
    json!({
        "path": path,
        "vertices": tri.vertices,
        "target_vertices": tri.positions.len() - tri.vertices,
        "triangles": tri.triangles.len(),
        "quads": tri.quads,
        "residue_bytes": tri.residue,
        "difference_morphs": morphs,
        "statistical_morphs": modifiers,
        "fit": fit,
    })
}

pub fn report(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, only: Option<&str>) -> Value {
    let mut paths = match only {
        Some(p) => vec![fnv_vfs::normalize(p)],
        None => vfs.paths(|p| p.ends_with(".tri")),
    };
    paths.sort();
    let rows: Vec<Value> = paths.iter().map(|p| row(vfs, nif, p, only.is_some())).collect();
    let label = |v: &Value| v.as_str().or_else(|| v["label"].as_str()).unwrap_or_default().to_string();
    // How many files carry each `.lip` channel's morph.
    let mut carried: BTreeMap<String, u64> = BTreeMap::new();
    for r in &rows {
        let labels: Vec<String> = r["difference_morphs"].as_array().into_iter().flatten().chain(r["statistical_morphs"].as_array().into_iter().flatten()).map(label).collect();
        for ch in fnv_voice::lip::CHANNEL_NAMES {
            if fnv_actor::tri::label_aliases(ch).iter().any(|a| labels.iter().any(|l| l.eq_ignore_ascii_case(a))) {
                *carried.entry(ch.to_string()).or_insert(0) += 1;
            }
        }
    }
    let count = |f: &dyn Fn(&Value) -> bool| rows.iter().filter(|r| f(r)).count();
    json!({
        "format": "fnv.actor.tri/1",
        "summary": {
            "files": rows.len(),
            "parsed": count(&|r| r["error"].is_null()),
            "parsed_to_the_last_byte": count(&|r| r["residue_bytes"] == 0),
            "with_a_model_beside": count(&|r| r["fit"]["error"].is_null() && !r["fit"].is_null()),
            "same_vertex_count_as_a_shape": count(&|r| r["fit"]["same_vertex_count"] == true),
            "vertices_within_0.001": count(&|r| r["fit"]["largest_vertex_distance"].as_f64().is_some_and(|d| d <= 0.001)),
            "every_model_triangle_joins_the_same_vertices_as_one_of_the_file": count(&|r| r["fit"]["model_triangles_found_in_the_file"].as_u64().is_some_and(|n| Some(n) == r["fit"]["model_triangles"].as_u64())),
            "every_vertex_nearest_to_its_own_index": count(&|r| r["fit"]["vertices_nearest_to_their_own_index"].as_u64().is_some_and(|n| Some(n) == r["vertices"].as_u64())),
            "files_carrying_each_lip_channel": carried,
        },
        "files": rows,
    })
}
