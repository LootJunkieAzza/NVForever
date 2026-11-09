//! Diagnostic survey for overnight queue 2026-11-09 item 107 (the tumbleweed drawn with
//! see-through gaps and speckles): the visible triangle shapes whose nearest
//! NiAlphaProperty blends SRC_ALPHA / INV_SRC_ALPHA and also tests alpha with a threshold
//! of 1 to 63 (drawn blended by the renderers, docs/spec-notes/nif.md §6 "Alpha"). Counts
//! them by shader property type, whether the shader sets BSShaderFlags2 bit 0
//! "ZBuffer_Write" and whether bit 3 "Vertex_Alpha" is set [documented: nif.xml flag
//! names], and prints every lit one with its base texture, for the alpha shares of those
//! textures to be read separately. `clutter\tumbleweednv.nif` (`TumbleweedNV:0`, flags
//! 0x12ED, threshold 50, BSShaderPPLightingProperty, ZBuffer_Write) is one.
//!
//! cargo run --release -p fnv-nif --example blend_test_lit_survey -- <schema dir> <archive.bsa>...

use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::value::Value;
use fnv_nif::{NifFile, Schema};
use std::collections::{BTreeMap, HashMap};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut lit: Vec<String> = Vec::new();
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
                let (mut alpha, mut shader) = (None, None);
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        if file.blocks[r].type_name == "NiAlphaProperty" {
                            alpha = Some((s.int("flags").unwrap_or(0), s.int("threshold").unwrap_or(0)));
                        } else if dec.is_a(r, "BSShaderProperty") {
                            shader = Some(r);
                        }
                    }
                }
                let Some((flags, threshold)) = alpha else { continue };
                let source_over = flags & 1 != 0 && (flags >> 1) & 0xF == 6 && (flags >> 5) & 0xF == 7;
                if !source_over || flags & 0x200 == 0 || !(1..64).contains(&threshold) {
                    continue;
                }
                let Some(sh) = shader else {
                    *counts.entry("no shader property".into()).or_default() += 1;
                    continue;
                };
                let Ok(s) = dec.blocks[sh].as_ref() else { continue };
                let ty = file.blocks[sh].type_name.as_str();
                let (f1, f2) = (s.int("shader_flags").unwrap_or(0), s.int("shader_flags_2").unwrap_or(0));
                let zwrite = f2 & 1 != 0;
                let vertex_alpha = f1 & (1 << 3) != 0;
                let tex = s
                    .block_ref("texture_set")
                    .and_then(|t| dec.get(t))
                    .and_then(|t| t.list("textures"))
                    .and_then(|t| match t.first() {
                        Some(Value::Str { text: Some(x), .. }) => Some(x.to_string()),
                        _ => None,
                    })
                    .or_else(|| s.text("file_name").map(str::to_string))
                    .unwrap_or_default();
                let key = format!("{ty:28} zwrite {zwrite:5} vertex_alpha {vertex_alpha:5} threshold {}", if threshold < 16 { "1-15" } else { "16-63" });
                *counts.entry(key).or_default() += 1;
                if ty == "BSShaderPPLightingProperty" {
                    lit.push(format!("{path}\t{:?}\t{flags:#x}\t{threshold}\tzwrite {zwrite}\tvertex_alpha {vertex_alpha}\t{tex}", o.name.as_deref().unwrap_or("")));
                }
            }
        }
    }
    println!("source-over blend + test, threshold 1-63, visible shapes:");
    for (k, c) in &counts {
        println!("{c:7}  {k}");
    }
    println!("\nlit ones:");
    for l in &lit {
        println!("{l}");
    }
}
