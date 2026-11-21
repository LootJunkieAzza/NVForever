//! Diagnostic survey of NiAlphaProperty blend factors on visible triangle shapes, for
//! deciding how the renderers draw each blend equation. Counts blended shapes by (source
//! factor, destination factor, shader property type) with example files, and lists every
//! shape whose equation is not one of the two common ones (SRC_ALPHA with INV_SRC_ALPHA
//! or with ONE) with its material and shader data: emissive colour and multiplier,
//! material alpha, shader type and flags, textures (a BSShaderNoLightingProperty's file
//! name, or the NiTexturingProperty base texture when that is empty) and whether it has
//! vertex colours.
//!
//! cargo run --release -p fnv-nif --example blend_survey -- <schema dir> <archive.bsa>...

use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema, Value};
use std::collections::{BTreeMap, HashMap};

/// AlphaFunction [documented: nif.xml].
fn factor(f: i64) -> &'static str {
    [
        "ONE",
        "ZERO",
        "SRC_COLOR",
        "INV_SRC_COLOR",
        "DEST_COLOR",
        "INV_DEST_COLOR",
        "SRC_ALPHA",
        "INV_SRC_ALPHA",
        "DEST_ALPHA",
        "INV_DEST_ALPHA",
        "SRC_ALPHA_SATURATE",
    ]
    .get(f as usize)
    .copied()
    .unwrap_or("?")
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unusual: Vec<String> = Vec::new();
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
                // Ancestors first, the shape's own properties last (they win).
                let (mut alpha, mut shader, mut material, mut base_texture) = (None, None, None, None);
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        match file.blocks[r].type_name.as_str() {
                            "NiAlphaProperty" => alpha = Some((s.int("flags").unwrap_or(0), s.int("threshold").unwrap_or(0))),
                            "NiMaterialProperty" => {
                                material = Some((s.floats("emissive_color"), s.float("emissive_mult"), s.float("alpha")))
                            }
                            "NiTexturingProperty" => {
                                base_texture = s
                                    .strukt("base_texture")
                                    .and_then(|b| b.block_ref("source"))
                                    .and_then(|r| dec.get(r))
                                    .and_then(|t| t.text("file_name"))
                                    .map(str::to_string)
                            }
                            t if dec.is_a(r, "BSShaderProperty") => shader = Some((t.to_string(), r)),
                            _ => {}
                        }
                    }
                }
                let Some((flags, threshold)) = alpha else { continue };
                if flags & 1 == 0 {
                    continue;
                }
                let (src, dst) = ((flags >> 1) & 0xF, (flags >> 5) & 0xF);
                let ty = shader.as_ref().map_or("none", |(t, _)| t.as_str());
                let key = format!("src {:13} dst {:13} {ty}", factor(src), factor(dst));
                let ex = examples.entry(key.clone()).or_default();
                if ex.len() < 3 {
                    ex.push(format!("{path} :: {:?}", o.name));
                }
                *counts.entry(key).or_default() += 1;
                if src == 6 && (dst == 7 || dst == 0) {
                    continue;
                }
                let (shader_type, shader_flags, textures) = shader
                    .as_ref()
                    .and_then(|&(_, r)| dec.blocks[r].as_ref().ok())
                    .map(|s| {
                        let mut tex: Vec<String> = s
                            .block_ref("texture_set")
                            .and_then(|t| dec.get(t))
                            .and_then(|ts| ts.list("textures"))
                            .unwrap_or_default()
                            .iter()
                            .map(|v| match v {
                                Value::Str { text: Some(t), .. } => t.clone(),
                                _ => String::new(),
                            })
                            .collect();
                        if let Some(f) = s.text("file_name").filter(|f| !f.is_empty()) {
                            tex = vec![f.to_string()];
                        } else if let (true, Some(t)) = (tex.is_empty(), &base_texture) {
                            tex = vec![t.clone()];
                        }
                        (s.int("shader_type").unwrap_or(-1), s.int("shader_flags").unwrap_or(0), tex)
                    })
                    .unwrap_or((-1, 0, Vec::new()));
                let (emissive, mult, malpha) = material.unwrap_or((None, None, None));
                let vertex_colors = dec.blocks[o.block]
                    .as_ref()
                    .ok()
                    .and_then(|s| s.block_ref("data"))
                    .and_then(|d| dec.get(d))
                    .is_some_and(|d| d.get("vertex_colors").is_some());
                unusual.push(format!(
                    "{} / {} | {path} :: {:?} | flags {flags} thr {threshold} | {ty} type {shader_type} flags {shader_flags:#010x} | emissive {emissive:.2?} x{mult:?} alpha {malpha:?} | vertex colours {vertex_colors} | {textures:?}",
                    factor(src),
                    factor(dst),
                    o.name
                ));
            }
        }
    }
    let mut v: Vec<_> = counts.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, c) in v {
        println!("{c:7}  {k}");
        for e in examples.get(k).into_iter().flatten() {
            println!("           {e}");
        }
    }
    unusual.sort();
    println!("\nshapes with other factors than SRC_ALPHA / INV_SRC_ALPHA or SRC_ALPHA / ONE ({}):", unusual.len());
    for u in &unusual {
        println!("  {u}");
    }
}
