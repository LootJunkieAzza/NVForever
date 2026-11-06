//! Diagnostic survey of BSShaderFlags2 bit 0 `zbuffer_write` [documented: nif.xml] on
//! visible blended triangle shapes (NiAlphaProperty blend on, SRC_ALPHA / INV_SRC_ALPHA,
//! drawn blended by the renderers' rule: no test, or a test threshold below 64), for
//! deciding which blended shapes write depth. Counts shapes by shader property type and
//! the bit, with example files; with `--list` first, prints every shape with the bit and
//! the names it carries (glass, windows ...).
//!
//! cargo run --release -p fnv-nif --example zwrite_survey -- [--list] <schema dir> <archive.bsa>...

use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema};
use std::collections::{BTreeMap, HashMap};

fn main() {
    let mut a: Vec<String> = std::env::args().collect();
    let list = a.get(1).is_some_and(|x| x == "--list");
    if list {
        a.remove(1);
    }
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut listed: Vec<String> = Vec::new();
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
                let (mut alpha, mut shader) = (None, None);
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        match file.blocks[r].type_name.as_str() {
                            "NiAlphaProperty" => alpha = Some((s.int("flags").unwrap_or(0), s.int("threshold").unwrap_or(0))),
                            t if dec.is_a(r, "BSShaderProperty") => shader = Some((t.to_string(), r)),
                            _ => {}
                        }
                    }
                }
                let Some((flags, threshold)) = alpha else { continue };
                let (src, dst) = ((flags >> 1) & 0xF, (flags >> 5) & 0xF);
                let test = flags & 0x200 != 0;
                if flags & 1 == 0 || src != 6 || dst != 7 || (test && threshold >= 64) {
                    continue;
                }
                let Some((ty, r)) = shader else { continue };
                let s = dec.blocks[r].as_ref().ok();
                let f2 = s.and_then(|s| s.int("shader_flags_2")).unwrap_or(0);
                let f1 = s.and_then(|s| s.int("shader_flags")).unwrap_or(0);
                let zw = f2 & 1 != 0;
                let key = format!("{ty:32} zbuffer_write {zw}");
                let ex = examples.entry(key.clone()).or_default();
                if ex.len() < 6 {
                    ex.push(format!("{path} :: {:?}", o.name));
                }
                *counts.entry(key).or_default() += 1;
                if list {
                    listed.push(format!("{ty} zw {zw} f1 {f1:#010x} f2 {f2:#010x} thr {threshold} test {test} | {path} :: {:?}", o.name));
                }
            }
        }
    }
    for (k, c) in &counts {
        println!("{c:7}  {k}");
        for e in examples.get(k).into_iter().flatten() {
            println!("           {e}");
        }
    }
    for l in &listed {
        println!("  {l}");
    }
}
