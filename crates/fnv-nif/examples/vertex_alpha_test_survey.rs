//! Diagnostic survey of alpha-tested shapes that carry vertex alpha, for deciding whether
//! the alpha test can be meant to cut by vertex alpha (docs/spec-notes/nif.md §6 "Alpha",
//! "Shadow strips"). Over visible triangle shapes whose NiAlphaProperty sets the test bit,
//! whose shader property sets BSShaderFlags bit 3 "Vertex_Alpha" [documented: nif.xml] and
//! whose vertex alpha is below 1 somewhere, counts by (blend factors, threshold bucket,
//! shader property type, textured or not) how many shapes would keep all, some or none of
//! their vertices if the test compared the vertex alpha alone with the threshold, with the
//! vertex colours' range and example files.
//!
//! cargo run --release -p fnv-nif --example vertex_alpha_test_survey -- <schema dir> <archive.bsa>...

use fnv_nif::scene::{mesh_data, scene_objects, Decoded};
use fnv_nif::{NifFile, Schema, Value};
use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
struct Row {
    shapes: usize,
    /// Shapes by the share of their vertices whose alpha is above the threshold.
    all_pass: usize,
    some_pass: usize,
    none_pass: usize,
    /// Shapes whose vertex colours are black on every vertex.
    black: usize,
    examples: Vec<String>,
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif")).collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            let dec = Decoded::new(&file, &data, &schema);
            let objects = scene_objects(&file, &dec);
            let by_block: HashMap<usize, _> = objects.iter().map(|o| (o.block, o)).collect();
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
                // Nearest alpha and shader properties (the shape's own win over ancestors').
                let (mut alpha, mut shader) = (None, None);
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Ok(s) = dec.blocks[r].as_ref() else { continue };
                        let ty = file.blocks[r].type_name.as_str();
                        if ty == "NiAlphaProperty" {
                            alpha = Some((s.int("flags").unwrap_or(0), s.int("threshold").unwrap_or(0)));
                        } else if dec.is_a(r, "BSShaderProperty") {
                            let texture = match ty {
                                "BSShaderNoLightingProperty" => s.text("file_name").map(str::to_string),
                                _ => s
                                    .block_ref("texture_set")
                                    .and_then(|t| dec.get(t))
                                    .and_then(|t| t.list("textures"))
                                    .and_then(|t| match t.first() {
                                        Some(Value::Str { text, .. }) => text.clone(),
                                        _ => None,
                                    }),
                            };
                            let textured = texture.is_some_and(|t| !t.is_empty());
                            shader = Some((ty.to_string(), s.int("shader_flags").unwrap_or(0), textured));
                        }
                    }
                }
                let (Some((flags, threshold)), Some((ty, shader_flags, textured))) = (alpha, shader) else { continue };
                if flags & 0x200 == 0 || shader_flags & (1 << 3) == 0 {
                    continue;
                }
                let Some(d) = dec.blocks[o.block].as_ref().ok().and_then(|s| s.block_ref("data")).and_then(|r| dec.get(r)) else {
                    continue;
                };
                let colors = mesh_data(d).colors;
                if colors.is_empty() || colors.iter().flatten().any(|v| !v.is_finite()) || colors.iter().all(|c| c[3] >= 1.0) {
                    continue;
                }
                let blend = if flags & 1 == 0 {
                    "test only".to_string()
                } else {
                    format!("blend {}/{}", (flags >> 1) & 0xF, (flags >> 5) & 0xF)
                };
                let bucket = if threshold < 64 { "threshold 0-63" } else { "threshold 64+" };
                let key = format!("{blend:12} {bucket:15} {ty:28} {}", if textured { "textured" } else { "no texture" });
                let cut = threshold as f32 / 255.0;
                let pass = colors.iter().filter(|c| c[3] > cut).count();
                let row = rows.entry(key).or_default();
                row.shapes += 1;
                if pass == colors.len() {
                    row.all_pass += 1;
                } else if pass == 0 {
                    row.none_pass += 1;
                } else {
                    row.some_pass += 1;
                }
                if colors.iter().all(|c| c[..3].iter().all(|&v| v == 0.0)) {
                    row.black += 1;
                }
                if row.examples.len() < 8 {
                    let max = colors.iter().map(|c| c[3]).fold(0.0f32, f32::max);
                    row.examples.push(format!(
                        "{path} :: {:?} threshold {threshold}, vertex alpha max {max:.2}, {pass} of {} vertices above",
                        o.name,
                        colors.len()
                    ));
                }
            }
        }
    }
    println!("alpha-tested visible shapes with the vertex-alpha flag and some vertex alpha below 1:");
    println!("  shapes  (vertices above the threshold: all / some / none)  black  key");
    for (k, r) in &rows {
        println!("  {:6}  ({:5} / {:5} / {:5})  {:5}  {k}", r.shapes, r.all_pass, r.some_pass, r.none_pass, r.black);
    }
    for (k, r) in &rows {
        println!("\n{k}:");
        for e in &r.examples {
            println!("  {e}");
        }
    }
}
