//! Diagnostic survey of skin shapes, for deciding which shapes of an actor's models take
//! the race's skin texture (docs/spec-notes/actors.md §3.1). Counts visible triangle
//! shapes with a BSShaderPPLightingProperty by (shader type, FaceGen flag, class of the
//! diffuse texture), with example files; then the skin shapes (type 14) by diffuse
//! texture, and the files holding skin shapes by the classes of those shapes' textures
//! (does any file mix hand and upper body skin?). With `--detail <text>`, also lists
//! every such shape of the files whose path contains the text.
//!
//! cargo run --release -p fnv-nif --example skin_survey -- <schema dir> [--detail <text>] <archive.bsa>...

use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema};
use std::collections::{BTreeMap, HashMap};

/// The class of a diffuse texture path: the skin textures by body part, the underwear,
/// or the rest.
fn class(texture: &str) -> &'static str {
    let t = texture.to_ascii_lowercase().replace('/', "\\");
    let file = t.rsplit('\\').next().unwrap_or("");
    if !t.contains("characters\\") {
        return if t.contains("armor\\underwear\\") { "underwear" } else { "other" };
    }
    match file {
        f if f.starts_with("upperbody") => "characters upperbody",
        f if f.starts_with("hand") => "characters hand",
        f if f.starts_with("head") => "characters head",
        _ => "characters other",
    }
}

fn main() {
    let mut a: Vec<String> = std::env::args().skip(1).collect();
    let detail = a.iter().position(|x| x == "--detail").map(|i| {
        a.remove(i);
        a.remove(i).to_ascii_lowercase()
    });
    let schema = Schema::load_dir(std::path::Path::new(&a[0])).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Skin shapes by texture, and per file the classes of its skin shapes' textures.
    let mut skin_textures: BTreeMap<String, usize> = BTreeMap::new();
    let mut skin_files: BTreeMap<String, std::collections::BTreeSet<&'static str>> = BTreeMap::new();
    for arc in &a[1..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif")).collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            let dec = Decoded::new(&file, &data, &schema);
            let objects = scene_objects(&file, &dec);
            let by_block: HashMap<usize, _> = objects.iter().map(|o| (o.block, o)).collect();
            let listed = detail.as_ref().is_some_and(|d| path.to_ascii_lowercase().contains(d));
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
                if chain.iter().any(|&b| dec.blocks[b].as_ref().ok().and_then(|s| s.int("flags")).unwrap_or(0) & 1 != 0) {
                    continue;
                }
                // The last BSShaderPPLightingProperty on the way down wins.
                let mut shader = None;
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        if file.blocks[r].type_name == "BSShaderPPLightingProperty" {
                            let texture = s
                                .block_ref("texture_set")
                                .and_then(|t| dec.get(t))
                                .and_then(|t| t.list("textures"))
                                .and_then(|l| match l.first() {
                                    Some(fnv_nif::Value::Str { text: Some(t), .. }) => Some(t.clone()),
                                    _ => None,
                                })
                                .unwrap_or_default();
                            shader = Some((s.int("shader_type").unwrap_or(-1), s.int("shader_flags").unwrap_or(0), texture));
                        }
                    }
                }
                let Some((ty, flags, texture)) = shader else { continue };
                let facegen = flags & (1 << 10) != 0;
                if listed {
                    println!("{path} :: {:?}  type {ty}  facegen {facegen}  flags {flags:#010x}  {texture}", o.name);
                }
                if ty == 14 {
                    *skin_textures.entry(texture.to_ascii_lowercase().replace('/', "\\")).or_default() += 1;
                    skin_files.entry(path.clone()).or_default().insert(if texture.is_empty() { "none" } else { class(&texture) });
                }
                let key = format!("type {ty:2}  facegen {facegen:5}  {}", class(&texture));
                let ex = examples.entry(key.clone()).or_default();
                if ex.len() < 4 && (ty == 14 || facegen || class(&texture) != "other") {
                    ex.push(format!("{path} :: {:?} :: {texture}", o.name));
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
    println!("skin shapes (type 14) by diffuse texture:");
    for (t, c) in &skin_textures {
        println!("{c:7}  {}", if t.is_empty() { "(none)" } else { t });
    }
    println!("files with skin shapes, by the classes of their skin shapes' textures:");
    let mut mixes: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for (path, classes) in &skin_files {
        let top = path.split('\\').nth(1).unwrap_or("");
        let key = format!("{top:10}  {}", classes.iter().copied().collect::<Vec<_>>().join(" + "));
        let e = mixes.entry(key).or_insert((0, path.clone()));
        e.0 += 1;
    }
    for (k, (c, example)) in &mixes {
        println!("{c:7}  {k}  (e.g. {example})");
    }
}
