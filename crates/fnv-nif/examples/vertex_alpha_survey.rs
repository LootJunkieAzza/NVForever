//! Diagnostic survey of vertex colour alpha, for deciding when the renderers use it.
//! Over visible triangle shapes with vertex colours whose nearest shader property derives
//! from BSShaderProperty, counts by whether BSShaderFlags bit 3 "Vertex_Alpha" [documented:
//! nif.xml] is set: shapes whose vertex alpha is all 1, shapes with some alpha below 1,
//! and shapes with non-finite or out-of-range values (colour or alpha), with examples.
//!
//! cargo run --release -p fnv-nif --example vertex_alpha_survey -- <schema dir> <archive.bsa>...

use fnv_nif::scene::{mesh_data, scene_objects, Decoded};
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
                // Nearest shader property (the shape's own wins over ancestors').
                let mut flags = None;
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        if dec.is_a(r, "BSShaderProperty") {
                            flags = dec.blocks[r].as_ref().ok().and_then(|s| s.int("shader_flags"));
                        }
                    }
                }
                let Some(flags) = flags else { continue };
                let Some(d) = dec.blocks[o.block].as_ref().ok().and_then(|s| s.block_ref("data")).and_then(|r| dec.get(r)) else {
                    continue;
                };
                let colors = mesh_data(d).colors;
                if colors.is_empty() {
                    continue;
                }
                let flag = if flags & (1 << 3) != 0 { "vertex_alpha flag" } else { "no vertex_alpha flag" };
                let broken = colors.iter().flatten().any(|v| !v.is_finite() || *v < 0.0 || *v > 1.0);
                let below = colors.iter().any(|c| c[3] < 1.0);
                let kind = if broken {
                    "non-finite or out of range"
                } else if below {
                    "some alpha < 1"
                } else {
                    "alpha all 1"
                };
                let key = format!("{flag:22} {kind}");
                *counts.entry(key.clone()).or_default() += 1;
                let ex = examples.entry(key).or_default();
                if ex.len() < 5 {
                    ex.push(format!("{path} :: {:?}", o.name));
                }
            }
        }
    }
    println!("visible shapes with vertex colours and a shader property:");
    for (k, c) in &counts {
        println!("  {c:7}  {k}");
    }
    for (k, v) in &examples {
        println!("\n{k}:");
        for e in v {
            println!("  {e}");
        }
    }
}
