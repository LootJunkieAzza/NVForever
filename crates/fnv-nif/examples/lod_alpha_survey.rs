//! Diagnostic survey of how distant-object LOD geometry is cut out by texture alpha, for
//! deciding how the renderers draw the object LOD blocks (docs/spec-notes/world.md §7).
//! Counts visible triangle shapes by file kind (object LOD block, `*_lod.nif`, other),
//! the BSShaderFlags2 "LOD_Building" / "LOD_Landscape" bits [documented: nif.xml] and
//! their NiAlphaProperty (none, or blend / test / test function / threshold), with
//! example files and the textures the object LOD blocks name.
//!
//! cargo run --release -p fnv-nif --example lod_alpha_survey -- <schema dir> <archive.bsa>...

use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema};
use std::collections::{BTreeMap, HashMap};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut block_textures: BTreeMap<String, usize> = BTreeMap::new();
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif")).collect();
        for (path, rec) in entries {
            let kind = if path.contains("\\lod\\") && path.contains("\\blocks\\") {
                "block"
            } else if path.ends_with("_lod.nif") {
                "_lod.nif"
            } else {
                "other"
            };
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
                let (mut alpha, mut flags2, mut textures) = (None, None, Vec::new());
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        match file.blocks[r].type_name.as_str() {
                            "NiAlphaProperty" => alpha = Some((s.int("flags").unwrap_or(0), s.int("threshold").unwrap_or(0))),
                            _ if dec.is_a(r, "BSShaderProperty") => {
                                flags2 = s.int("shader_flags_2");
                                if let Some(ts) = s.block_ref("texture_set").and_then(|t| dec.get(t)) {
                                    textures = ts
                                        .list("textures")
                                        .unwrap_or_default()
                                        .iter()
                                        .map(|v| match v {
                                            fnv_nif::Value::Str { text: Some(t), .. } => t.to_ascii_lowercase(),
                                            _ => String::new(),
                                        })
                                        .collect();
                                }
                            }
                            _ => {}
                        }
                    }
                }
                let lod = match flags2 {
                    None => "no shader property",
                    Some(f) if f & 4 != 0 => "LOD_Building",
                    Some(f) if f & 2 != 0 => "LOD_Landscape",
                    Some(_) => "no LOD flag",
                };
                if kind == "other" && !lod.starts_with("LOD_") {
                    continue;
                }
                if kind == "block" {
                    *block_textures.entry(textures.first().cloned().unwrap_or_default()).or_default() += 1;
                }
                let alpha = match alpha {
                    None => "no NiAlphaProperty".to_string(),
                    Some((f, t)) => {
                        format!("blend {} test {} func {} threshold {t}", f & 1 != 0, f & 0x200 != 0, (f >> 10) & 7)
                    }
                };
                let key = format!("{kind:8} {lod:18} {alpha}");
                let ex = examples.entry(key.clone()).or_default();
                if ex.len() < 3 {
                    ex.push(format!("{path} :: {:?}", o.name));
                }
                *counts.entry(key).or_default() += 1;
            }
        }
    }
    for (k, c) in &counts {
        println!("{c:7}  {k}");
        for e in examples.get(k).into_iter().flatten() {
            println!("           {e}");
        }
    }
    println!("object LOD block shapes by base texture:");
    for (t, c) in &block_textures {
        println!("{c:7}  {t}");
    }
}
