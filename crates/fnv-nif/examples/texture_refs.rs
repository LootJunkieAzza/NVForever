//! Diagnostic survey: which meshes name a texture, and in which slot of a
//! BSShaderTextureSet. Used to see which normal maps (`…_n.dds`) the game can only reach
//! by their name, because no mesh names them (docs/spec-notes/actors.md §3.1, "Normal
//! maps"). Prints one line per (slot, texture path) whose path contains the text given:
//! the number of texture sets that name it and one example mesh.
//!
//! cargo run --release -p fnv-nif --example texture_refs -- <schema dir> <text> <archive.bsa>...

use fnv_nif::scene::Decoded;
use fnv_nif::{NifFile, Schema, Value};
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[0])).unwrap();
    let text = a[1].to_ascii_lowercase();
    let mut refs: BTreeMap<(String, usize), (usize, String)> = BTreeMap::new();
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif")).collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            let dec = Decoded::new(&file, &data, &schema);
            for (b, block) in dec.blocks.iter().enumerate() {
                let Ok(s) = block else { continue };
                if file.blocks[b].type_name != "BSShaderTextureSet" {
                    continue;
                }
                for (slot, t) in s.list("textures").unwrap_or_default().iter().enumerate() {
                    let Value::Str { text: Some(t), .. } = t else { continue };
                    let t = t.to_ascii_lowercase().replace('/', "\\");
                    if t.contains(&text) {
                        let e = refs.entry((t, slot)).or_insert((0, path.clone()));
                        e.0 += 1;
                    }
                }
            }
        }
    }
    for ((texture, slot), (count, example)) in &refs {
        println!("{count:6}  slot {slot}  {texture}  ({example})");
    }
}
