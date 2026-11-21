//! Diagnostic survey of meshes, for deciding what the world renderer draws:
//! - visible triangle shapes (not hidden, no hidden ancestor, not under an editor marker)
//!   that carry no BSShaderProperty-derived property on themselves or an ancestor, with
//!   the other property types they do carry and where they occur;
//! - controller sequences embedded in meshes, by name;
//! - meshes with visible skinned shapes outside the actor folders (characters,
//!   creatures, armor), and which embedded sequence names they have.
//!
//! cargo run --release -p fnv-nif --example shape_survey -- <schema dir> <archive.bsa>...

use fnv_nif::anim::sequences;
use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema};
use std::collections::{BTreeMap, HashMap};

fn top(title: &str, m: &BTreeMap<String, usize>, n: usize) {
    println!("\n{title}:");
    let mut v: Vec<_> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, c) in v.iter().take(n) {
        println!("  {c:7}  {k}");
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let (mut files, mut shapes, mut shaderless, mut shaderless_files) = (0usize, 0usize, 0usize, 0usize);
    let mut prop_sets: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_folder: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: Vec<String> = Vec::new();
    let mut seq_names: BTreeMap<String, usize> = BTreeMap::new();
    let mut skinned_world: BTreeMap<String, usize> = BTreeMap::new();
    let mut skinned_world_files: Vec<String> = Vec::new();
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif")).collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            files += 1;
            let dec = Decoded::new(&file, &data, &schema);
            let objects = scene_objects(&file, &dec);
            let by_block: HashMap<usize, _> = objects.iter().map(|o| (o.block, o)).collect();
            let seqs: Vec<String> = sequences(&file, &dec).into_iter().map(|s| s.name.unwrap_or_default()).collect();
            for s in &seqs {
                *seq_names.entry(s.clone()).or_default() += 1;
            }
            let (mut file_shaderless, mut file_skinned) = (0, 0);
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
                shapes += 1;
                let mut types: Vec<String> = Vec::new();
                let mut shader = false;
                for &b in &chain {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        if r >= file.blocks.len() {
                            continue;
                        }
                        shader |= dec.is_a(r, "BSShaderProperty");
                        types.push(file.blocks[r].type_name.clone());
                    }
                }
                let skinned = dec.blocks[o.block].as_ref().ok().and_then(|s| s.block_ref("skin_instance")).is_some_and(|r| r >= 0);
                if shader && skinned {
                    file_skinned += 1;
                }
                if shader {
                    continue;
                }
                shaderless += 1;
                file_shaderless += 1;
                types.sort();
                types.dedup();
                let key = format!("{}{}", if types.is_empty() { "(none)".into() } else { types.join("+") }, if skinned { " skinned" } else { "" });
                *prop_sets.entry(key).or_default() += 1;
                let folder = path.split('\\').take(3).collect::<Vec<_>>().join("\\");
                *by_folder.entry(folder).or_default() += 1;
                let textured = types.iter().any(|t| t == "NiTexturingProperty");
                if examples.len() < 60 && (textured || (types.is_empty() && !path.contains("landscape\\lod"))) {
                    examples.push(format!("{path} :: {} {:?} {types:?}", o.type_name, o.name));
                }
            }
            if file_shaderless > 0 {
                shaderless_files += 1;
            }
            let actor = ["meshes\\characters\\", "meshes\\creatures\\", "meshes\\armor\\"].iter().any(|p| path.starts_with(p));
            if file_skinned > 0 && !actor {
                let key = if seqs.is_empty() { "(no sequence)".to_string() } else { seqs.join(",") };
                *skinned_world.entry(key).or_default() += 1;
                skinned_world_files.push(path.clone());
            }
        }
    }
    println!("files {files}, visible shapes {shapes}, without a shader property {shaderless} in {shaderless_files} files");
    top("property sets on shader-less shapes", &prop_sets, 40);
    top("shader-less shapes by folder", &by_folder, 40);
    println!("\nshader-less textured, or property-less outside landscape LOD:");
    for e in &examples {
        println!("  {e}");
    }
    top("embedded sequences by name (files)", &seq_names, 30);
    println!("\nmeshes with visible skinned shapes outside characters/creatures/armor: {}", skinned_world_files.len());
    top("their embedded sequence names", &skinned_world, 30);
    for f in skinned_world_files.iter().take(60) {
        println!("  {f}");
    }
}
