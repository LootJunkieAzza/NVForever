//! Diagnostic survey for shapes whose shader property names no texture: over visible
//! triangle shapes with vertex colours whose nearest shader property derives from
//! BSShaderProperty and sets neither BSShaderFlags bit 3 "Vertex_Alpha" nor (on a
//! BSShaderNoLightingProperty) bit 19 [documented: nif.xml flag names], counts by property
//! type, whether the shape has a texture (a texture set entry or a NoLighting file name),
//! whether its NiAlphaProperty enables blending, and whether some vertex alpha is below 1,
//! with examples. Written for overnight queue 2026-11-07 item 76: the soft black shadow
//! quads under the casino tables (`clutter\casino\cardtable01.nif`, shape
//! `CardTable01:1`) name no texture, blend, and have vertex alpha 0.7 inside and 0 at the
//! edges, but set neither flag.
//!
//! cargo run --release -p fnv-nif --example untextured_blend_survey -- <schema dir> <archive.bsa>...

use fnv_nif::scene::{mesh_data, scene_objects, Decoded};
use fnv_nif::value::Value;
use fnv_nif::{NifFile, Schema};
use std::collections::{BTreeMap, HashMap};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
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
                // Nearest shader and alpha properties (the shape's own win over ancestors').
                let (mut shader, mut alpha) = (None, None);
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        if dec.is_a(r, "BSShaderProperty") {
                            shader = Some(r);
                        } else if file.blocks[r].type_name == "NiAlphaProperty" {
                            alpha = dec.blocks[r].as_ref().ok().and_then(|s| s.int("flags"));
                        }
                    }
                }
                let Some(shader) = shader else { continue };
                let Ok(s) = dec.blocks[shader].as_ref() else { continue };
                let ty = file.blocks[shader].type_name.as_str();
                let flags = s.int("shader_flags").unwrap_or(0);
                if flags & (1 << 3) != 0 || (ty == "BSShaderNoLightingProperty" && flags & (1 << 19) != 0) {
                    continue;
                }
                let set_tex = s
                    .block_ref("texture_set")
                    .and_then(|t| dec.get(t))
                    .and_then(|t| t.list("textures"))
                    .is_some_and(|t| matches!(t.first(), Some(Value::Str { text: Some(x), .. }) if !x.is_empty()));
                let file_tex = s.text("file_name").is_some_and(|x| !x.is_empty());
                let textured = set_tex || file_tex;
                let Some(d) = dec.blocks[o.block].as_ref().ok().and_then(|s| s.block_ref("data")).and_then(|r| dec.get(r)) else {
                    continue;
                };
                let colors = mesh_data(d).colors;
                if colors.is_empty() {
                    continue;
                }
                let below = colors.iter().any(|c| c[3].is_finite() && c[3] < 1.0);
                let blend = match alpha {
                    Some(f) if f & 1 != 0 => "blend",
                    Some(_) => "alpha prop, no blend",
                    None => "no alpha prop",
                };
                let key = format!(
                    "{ty:28} {:10} {blend:20} {}",
                    if textured { "textured" } else { "untextured" },
                    if below { "some alpha < 1" } else { "alpha all 1" }
                );
                *counts.entry(key.clone()).or_default() += 1;
                let ex = examples.entry(key).or_default();
                if ex.len() < 8 {
                    ex.push(format!("{path} :: {:?} alpha flags {:?} shader flags {flags:#x}", o.name, alpha));
                }
            }
        }
    }
    println!("visible shapes with vertex colours, a shader property and no vertex-alpha flag:");
    for (k, c) in &counts {
        println!("  {c:7}  {k}");
    }
    for (k, v) in &examples {
        if !k.contains("untextured") {
            continue;
        }
        println!("\n{k}:");
        for e in v {
            println!("  {e}");
        }
    }
}
