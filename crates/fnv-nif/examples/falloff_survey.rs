//! Diagnostic survey of the view-angle falloff of BSShaderNoLightingProperty (light beams,
//! glows), for deciding how the renderers fade unlit effect geometry. Over visible triangle
//! shapes (not hidden, no hidden ancestor, not under an editor marker) whose nearest shader
//! property is a BSShaderNoLightingProperty, counts:
//! - the falloff fields (start angle, stop angle, start opacity, stop opacity);
//! - the same split by the "Vertex_Alpha" shader flag (bit 3) and by the NiAlphaProperty
//!   blend factors, to see whether falloff goes with blended geometry.
//!
//! cargo run --release -p fnv-nif --example falloff_survey -- <schema dir> <archive.bsa>...

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
    let (mut files, mut shapes) = (0usize, 0usize);
    let mut params: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_alpha: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
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
                let (mut shader, mut alpha) = (None, None);
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        match file.blocks[r].type_name.as_str() {
                            "NiAlphaProperty" => alpha = s.int("flags"),
                            t if dec.is_a(r, "BSShaderProperty") => shader = Some((t.to_string(), r)),
                            _ => {}
                        }
                    }
                }
                let Some((ty, r)) = shader else { continue };
                if ty != "BSShaderNoLightingProperty" {
                    continue;
                }
                shapes += 1;
                let s = dec.blocks[r].as_ref().unwrap();
                let f = |n: &str| s.float(n).map_or("-".to_string(), |v| format!("{v:.3}"));
                let key = format!(
                    "start {} stop {} opacity {} -> {}",
                    f("falloff_start_angle"),
                    f("falloff_stop_angle"),
                    f("falloff_start_opacity"),
                    f("falloff_stop_opacity")
                );
                let ex = examples.entry(key.clone()).or_default();
                if ex.len() < 4 {
                    ex.push(format!("{path} :: {:?}", o.name));
                }
                *params.entry(key.clone()).or_default() += 1;
                let flags = s.int("shader_flags").unwrap_or(0);
                let blend = match alpha {
                    None => "no alpha property".to_string(),
                    Some(a) if a & 1 == 0 => format!("no blend (test {})", a & 0x200 != 0),
                    Some(a) => format!("blend src {} dst {}", (a >> 1) & 0xF, (a >> 5) & 0xF),
                };
                let fields = ["falloff_start_angle", "falloff_stop_angle", "falloff_start_opacity", "falloff_stop_opacity"].map(|n| s.float(n));
                let falloff = match fields {
                    [Some(a), Some(b), Some(c), Some(d)] if a == 1.0 && b.abs() < 1e-6 && c == 1.0 && d == 0.0 => "default",
                    [Some(a), Some(b), ..] if (a - b).abs() < 1e-4 => "empty",
                    _ => "custom",
                };
                *by_alpha.entry(format!("{falloff:7} vertex_alpha {} {blend}", flags & 8 != 0)).or_default() += 1;
            }
        }
    }
    println!("files {files}, visible shapes drawn by BSShaderNoLightingProperty {shapes}");
    top("falloff parameters", &params, 30);
    top("falloff (default = nif.xml defaults, empty = start equals stop) x vertex-alpha flag x NiAlphaProperty", &by_alpha, 100);
    let mut v: Vec<_> = params.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!("\nexamples:");
    for (k, _) in v.iter().take(15) {
        println!("  {k}: {:?}", examples[*k]);
    }
}
