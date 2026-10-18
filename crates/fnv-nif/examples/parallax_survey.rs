//! Diagnostic survey of parallax height maps, for checking how the renderers draw them
//! (docs/spec-notes/nif.md §6 "Parallax"). Over visible triangle shapes (not hidden, no
//! hidden ancestor, not under an editor marker) whose nearest shader property is a
//! BSShaderPPLightingProperty, counts:
//! - texture-set slot 3 by file-name suffix (`_p`, other, empty) × the two parallax shader
//!   flags, bit 11 "Parallax_Shader_Index_15" and bit 28 "Parallax_Occlusion" [documented:
//!   nif.xml BSShaderFlags names], and whether the slot-3 file exists in the archives given;
//! - the stored `parallax_max_passes` and `parallax_scale` of the shapes with a slot-3
//!   texture, and of the rest;
//! - for each height map that exists: its DDS format, size, mean and spread of the red
//!   channel, and whether its colour channels differ (a grey map has R = G = B).
//!
//! Pass the mesh and texture archives together:
//! cargo run --release -p fnv-nif --example parallax_survey -- <schema dir> <archive.bsa>...

use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn top(title: &str, m: &BTreeMap<String, usize>, n: usize) {
    println!("\n{title}:");
    let mut v: Vec<_> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, c) in v.iter().take(n) {
        println!("  {c:7}  {k}");
    }
}

/// A texture path as the archives index it: lower case, backslashes, from `textures\`.
fn norm(t: &str) -> String {
    let t = t.to_ascii_lowercase().replace('/', "\\");
    match t.find("textures\\") {
        Some(i) => t[i..].to_string(),
        None => format!("textures\\{t}"),
    }
}

fn suffix(t: &str) -> &'static str {
    let t = t.to_ascii_lowercase();
    let stem = t.strip_suffix(".dds").unwrap_or(&t);
    if t.is_empty() {
        "(empty)"
    } else if stem.ends_with("_p") {
        "_p"
    } else {
        "other"
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut archives: Vec<fnv_bsa::Archive> = a[2..].iter().map(|p| fnv_bsa::Archive::open(p).unwrap()).collect();
    // Every DDS in the archives given: path → (archive index, record).
    let mut dds: HashMap<String, (usize, fnv_bsa::FileRecord)> = HashMap::new();
    for (i, bsa) in archives.iter().enumerate() {
        for e in bsa.entries() {
            let p = e.path();
            if p.ends_with(".dds") {
                dds.entry(p).or_insert((i, e.record.clone()));
            }
        }
    }
    let (mut files, mut shapes) = (0usize, 0usize);
    let mut slot3: BTreeMap<String, usize> = BTreeMap::new();
    let mut fields: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut maps: BTreeSet<String> = BTreeSet::new();
    for i in 0..archives.len() {
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            archives[i].entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif")).collect();
        for (path, rec) in entries {
            let Ok(data) = archives[i].extract(&rec) else { continue };
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
                let mut shader = None;
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        if dec.is_a(r, "BSShaderProperty") {
                            shader = Some((file.blocks[r].type_name.clone(), r));
                        }
                    }
                }
                let Some((ty, r)) = shader else { continue };
                if ty != "BSShaderPPLightingProperty" {
                    continue;
                }
                shapes += 1;
                let s = dec.blocks[r].as_ref().unwrap();
                let flags = s.int("shader_flags").unwrap_or(0);
                let tex3 = s
                    .block_ref("texture_set")
                    .and_then(|t| dec.get(t))
                    .and_then(|ts| ts.list("textures"))
                    .and_then(|l| l.get(3).cloned())
                    .and_then(|v| match v {
                        Value::Str { text: Some(t), .. } => Some(t),
                        _ => None,
                    })
                    .unwrap_or_default();
                let exists = !tex3.is_empty() && dds.contains_key(&norm(&tex3));
                let key = format!(
                    "slot3 {:7} {:8} bit11 {} bit28 {}",
                    suffix(&tex3),
                    if tex3.is_empty() { "" } else if exists { "exists" } else { "missing" },
                    u8::from(flags & (1 << 11) != 0),
                    u8::from(flags & (1 << 28) != 0),
                );
                let ex = examples.entry(key.clone()).or_default();
                if ex.len() < 4 {
                    ex.push(format!("{path} :: {:?} {tex3} flags {flags:#010x}", o.name));
                }
                *slot3.entry(key).or_default() += 1;
                let f = format!(
                    "{} passes {:?} scale {:?}",
                    if tex3.is_empty() { "no slot3" } else { "slot3   " },
                    s.float("parallax_max_passes"),
                    s.float("parallax_scale")
                );
                *fields.entry(f).or_default() += 1;
                if exists {
                    maps.insert(norm(&tex3));
                }
            }
        }
    }
    println!("files {files}, visible shapes lit by BSShaderPPLightingProperty {shapes}");
    top("slot 3 suffix x file x parallax flags", &slot3, 40);
    top("stored parallax fields", &fields, 30);
    println!("\nheight maps that exist ({}):", maps.len());
    for m in &maps {
        let (i, rec) = &dds[m];
        let Ok(bytes) = archives[*i].extract(rec) else { continue };
        let Ok(d) = fnv_dds::Dds::parse(&bytes) else {
            println!("  {m}: unreadable");
            continue;
        };
        let (w, h) = (d.header.width, d.header.height);
        let level = &d.levels[0][0];
        let rgba = fnv_dds::decode_blocks(d.format, w, h, level).or_else(|| fnv_dds::to_rgba8(d.format, level));
        let Some(rgba) = rgba else {
            println!("  {m}: {} {w}x{h} (not decoded)", d.format.name());
            continue;
        };
        let n = (rgba.len() / 4).max(1) as f64;
        let red: Vec<f64> = rgba.chunks_exact(4).map(|p| f64::from(p[0])).collect();
        let mean = red.iter().sum::<f64>() / n;
        let sd = (red.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
        let grey = rgba.chunks_exact(4).all(|p| p[0].abs_diff(p[1]) <= 8 && p[0].abs_diff(p[2]) <= 8);
        println!(
            "  {m}: {} {w}x{h} mips {} red mean {mean:.1} sd {sd:.1} {}",
            d.format.name(),
            d.levels[0].len(),
            if grey { "grey" } else { "channels differ" }
        );
    }
    println!("\nexamples:");
    for (k, v) in &examples {
        println!("  {k}");
        for e in v {
            println!("    {e}");
        }
    }
}
