//! Diagnostic survey of NiAlphaProperty settings on visible triangle shapes, for deciding
//! how the renderers draw shapes that set both alpha blending and alpha testing. Counts
//! shapes by (blend, test, test function, threshold bucket, shader property type), with
//! example files.
//!
//! cargo run --release -p fnv-nif --example alpha_survey -- <schema dir> <archive.bsa>...

use fnv_nif::scene::{scene_objects, Decoded};
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
                let (mut alpha, mut shader) = (None, "none".to_string());
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        match file.blocks[r].type_name.as_str() {
                            "NiAlphaProperty" => alpha = Some((s.int("flags").unwrap_or(0), s.int("threshold").unwrap_or(0))),
                            t if dec.is_a(r, "BSShaderProperty") => shader = t.to_string(),
                            _ => {}
                        }
                    }
                }
                let Some((flags, threshold)) = alpha else { continue };
                let (blend, test) = (flags & 1 != 0, flags & 0x200 != 0);
                if !(blend && test) {
                    *counts.entry(format!("blend {blend} test {test}")).or_default() += 1;
                    continue;
                }
                let bucket = match threshold {
                    0 => "0",
                    1..=15 => "1-15",
                    16..=63 => "16-63",
                    64..=127 => "64-127",
                    _ => "128+",
                };
                let key = format!(
                    "blend+test func {} dst {} threshold {bucket:6} {shader}",
                    (flags >> 10) & 7,
                    (flags >> 5) & 0xF
                );
                let ex = examples.entry(key.clone()).or_default();
                if ex.len() < 3 {
                    ex.push(format!("{path} :: {:?}", o.name));
                }
                *counts.entry(key).or_default() += 1;
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
}
