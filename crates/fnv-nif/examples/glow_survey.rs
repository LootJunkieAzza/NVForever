//! Diagnostic survey of emissive material settings, for deciding how the renderers light
//! glowing surfaces (neon signs, lit windows, bulbs). Over visible triangle shapes (not
//! hidden, no hidden ancestor, not under an editor marker) whose nearest shader property
//! is a BSShaderPPLightingProperty, counts:
//! - texture-set slot 2 by file-name suffix (`_g`, `_e`, `_n`, `_sk`, other, empty),
//!   split by whether the effective NiMaterialProperty emissive colour is black;
//! - the same for shapes with the face-gen or hair shader flags (slot 2 has other uses);
//! - emissive multiplier values; material colour and emittance controllers;
//! - for shapes drawn by a BSShaderNoLightingProperty, their emissive colour and multiplier.
//!
//! cargo run --release -p fnv-nif --example glow_survey -- <schema dir> <archive.bsa>...

use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema, Value};
use std::collections::{BTreeMap, HashMap};

fn top(title: &str, m: &BTreeMap<String, usize>, n: usize) {
    println!("\n{title}:");
    let mut v: Vec<_> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, c) in v.iter().take(n) {
        println!("  {c:7}  {k}");
    }
}

fn suffix(t: &str) -> &'static str {
    let t = t.to_ascii_lowercase();
    let stem = t.strip_suffix(".dds").unwrap_or(&t);
    if t.is_empty() {
        "(empty)"
    } else if stem.ends_with("_g") {
        "_g"
    } else if stem.ends_with("_e") {
        "_e"
    } else if stem.ends_with("_n") {
        "_n"
    } else if stem.ends_with("_sk") {
        "_sk"
    } else if stem.ends_with("_glow") {
        "_glow"
    } else {
        "other"
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let (mut files, mut shapes) = (0usize, 0usize);
    let mut slot2: BTreeMap<String, usize> = BTreeMap::new();
    let mut mults: BTreeMap<String, usize> = BTreeMap::new();
    let mut controllers: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unlit: BTreeMap<String, usize> = BTreeMap::new();
    let mut unlit_examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif")).collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            files += 1;
            let dec = Decoded::new(&file, &data, &schema);
            for b in &file.blocks {
                if b.type_name == "NiMaterialColorController" || b.type_name == "BSMaterialEmittanceMultController" {
                    *controllers.entry(b.type_name.clone()).or_default() += 1;
                }
            }
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
                let (mut shader, mut emissive, mut mult) = (None, None, None);
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        match file.blocks[r].type_name.as_str() {
                            "NiMaterialProperty" => {
                                emissive = s.floats("emissive_color");
                                mult = s.float("emissive_mult");
                            }
                            t if dec.is_a(r, "BSShaderProperty") => shader = Some((t.to_string(), r)),
                            _ => {}
                        }
                    }
                }
                let Some((ty, r)) = shader else { continue };
                if ty == "BSShaderNoLightingProperty" {
                    let e = emissive.as_ref().map(|e| format!("{:.2?}", e)).unwrap_or("(no material)".into());
                    let key = format!("emissive {e} x{}", mult.map_or("-".into(), |m| format!("{m}")));
                    let ex = unlit_examples.entry(key.clone()).or_default();
                    if ex.len() < 3 {
                        ex.push(format!("{path} :: {:?}", o.name));
                    }
                    *unlit.entry(key).or_default() += 1;
                    continue;
                }
                if ty != "BSShaderPPLightingProperty" {
                    continue;
                }
                shapes += 1;
                let s = dec.blocks[r].as_ref().unwrap();
                let flags = s.int("shader_flags").unwrap_or(0);
                let special = flags & (1 << 10) != 0 || flags & (1 << 18) != 0; // face gen, hair
                let tex2 = s
                    .block_ref("texture_set")
                    .and_then(|t| dec.get(t))
                    .and_then(|ts| ts.list("textures"))
                    .and_then(|l| l.get(2).cloned())
                    .and_then(|v| match v {
                        Value::Str { text: Some(t), .. } => Some(t),
                        _ => None,
                    })
                    .unwrap_or_default();
                let black = emissive.as_ref().is_none_or(|e| e.iter().all(|&c| c == 0.0));
                let key = format!(
                    "{} slot2 {:8} emissive {}",
                    if special { "facegen/hair" } else { "plain" },
                    suffix(&tex2),
                    if black { "black" } else { "non-black" }
                );
                let ex = examples.entry(key.clone()).or_default();
                if ex.len() < 4 {
                    ex.push(format!("{path} :: {:?} {tex2} emis {emissive:?} x{mult:?}", o.name));
                }
                *slot2.entry(key).or_default() += 1;
                if !black {
                    *mults.entry(format!("{:?}", mult)).or_default() += 1;
                }
            }
        }
    }
    println!("files {files}, visible shapes lit by BSShaderPPLightingProperty {shapes}");
    top("slot 2 suffix x emissive colour", &slot2, 40);
    top("emissive multiplier where emissive colour is non-black", &mults, 20);
    top("material controllers (blocks)", &controllers, 5);
    top("BSShaderNoLightingProperty shapes by emissive colour and multiplier", &unlit, 25);
    let mut v: Vec<_> = unlit.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, _) in v.iter().take(12) {
        println!("  {k}: {:?}", unlit_examples[*k]);
    }
    println!("\nexamples:");
    for (k, v) in &examples {
        println!("  {k}");
        for e in v {
            println!("    {e}");
        }
    }
}
