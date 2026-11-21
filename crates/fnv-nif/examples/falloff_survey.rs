//! Diagnostic survey of the view-angle falloff of BSShaderNoLightingProperty (light beams,
//! glows), for deciding how the renderers fade unlit effect geometry. Over visible triangle
//! shapes (not hidden, no hidden ancestor, not under an editor marker) whose nearest shader
//! property is a BSShaderNoLightingProperty, counts:
//! - the falloff fields (start angle, stop angle, start opacity, stop opacity);
//! - the same split by the "Vertex_Alpha" shader flag (bit 3) and by the NiAlphaProperty
//!   blend factors, to see whether falloff goes with blended geometry;
//! - per bit of the shader flags, how many shapes of each falloff kind set it, to see
//!   whether a flag goes with the shapes whose falloff was authored;
//! - for bit 6 ("empty" in schemas/nif/scene.json), which goes with authored falloff, the
//!   four groups (authored or default values, with or without the bit) by falloff values,
//!   by blending, by folder, by base texture and by the other flag bits. With `--list`,
//!   every shape of the two groups that go against the rule (authored values without the
//!   bit, default values with it) is printed with its values, flags, NiAlphaProperty and
//!   texture; with `--list-all`, every shape of all four groups.
//!
//! Falloff kinds: "default" (the nif.xml defaults), "empty" (start angle equals stop
//! angle), "absent" (the property has no falloff fields: stream version 26 or lower) and
//! "custom" (anything else: authored values).
//!
//! cargo run --release -p fnv-nif --example falloff_survey -- <schema dir> [--list | --list-all] <archive.bsa>...

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

/// One shape of the bit 6 comparison.
struct Row {
    /// "custom", "default", "empty" or "absent".
    falloff: &'static str,
    bit6: bool,
    line: String,
    params: String,
    blend: String,
    folder: String,
    texture: String,
    flags: i64,
}

/// The first `depth` folders of an archive path.
fn folder(path: &str, depth: usize) -> String {
    let parts: Vec<&str> = path.split('\\').collect();
    parts[..parts.len().saturating_sub(1).min(depth)].join("\\")
}

fn main() {
    let list_all = std::env::args().any(|x| x == "--list-all");
    let list = list_all || std::env::args().any(|x| x == "--list");
    let a: Vec<String> = std::env::args().filter(|x| !x.starts_with("--list")).collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let (mut files, mut shapes) = (0usize, 0usize);
    let mut params: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_alpha: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_bit: BTreeMap<&str, [usize; 33]> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut rows: Vec<Row> = Vec::new();
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
                let (mut shader, mut alpha, mut threshold) = (None, None, None);
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        match file.blocks[r].type_name.as_str() {
                            "NiAlphaProperty" => (alpha, threshold) = (s.int("flags"), s.int("threshold")),
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
                    [None, None, None, None] => "absent",
                    [Some(a), Some(b), Some(c), Some(d)] if a == 1.0 && b.abs() < 1e-6 && c == 1.0 && d == 0.0 => "default",
                    [Some(a), Some(b), ..] if (a - b).abs() < 1e-4 => "empty",
                    _ => "custom",
                };
                *by_alpha.entry(format!("{falloff:7} vertex_alpha {} {blend}", flags & 8 != 0)).or_default() += 1;
                let row = by_bit.entry(falloff).or_insert([0usize; 33]);
                row[32] += 1;
                for (bit, count) in row.iter_mut().take(32).enumerate() {
                    *count += (flags >> bit & 1) as usize;
                }
                let texture = s.text("file_name").unwrap_or("").to_ascii_lowercase();
                rows.push(Row {
                    falloff,
                    bit6: flags & (1 << 6) != 0,
                    params: key.clone(),
                    line: format!(
                        "{path} :: {:?} | {key} | flags {flags:#010x} flags2 {:#010x} | alpha {} threshold {} | {texture:?}",
                        o.name,
                        s.int("shader_flags_2").unwrap_or(0),
                        alpha.map_or("-".to_string(), |a| format!("{a:#06x}")),
                        threshold.map_or("-".to_string(), |t| t.to_string()),
                    ),
                    blend,
                    folder: folder(&path, 3),
                    texture,
                    flags,
                });
            }
        }
    }
    println!("files {files}, visible shapes drawn by BSShaderNoLightingProperty {shapes}");
    top("falloff parameters", &params, 30);
    top("falloff kind x vertex-alpha flag x NiAlphaProperty", &by_alpha, 100);
    println!("\nshapes that set each shader flag bit, by falloff kind (bits no shape sets are left out):");
    for bit in 0..32 {
        if by_bit.values().all(|r| r[bit] == 0) {
            continue;
        }
        let cells: Vec<String> = by_bit.iter().map(|(k, r)| format!("{k} {} of {}", r[bit], r[32])).collect();
        println!("  bit {bit:2}: {}", cells.join(", "));
    }
    for (falloff, bit6) in [("custom", true), ("custom", false), ("default", true), ("default", false)] {
        let group: Vec<&Row> = rows.iter().filter(|r| r.falloff == falloff && r.bit6 == bit6).collect();
        let title = format!("{falloff} falloff {} bit 6 ({} shapes)", if bit6 { "with" } else { "without" }, group.len());
        let count = |key: &dyn Fn(&Row) -> String| {
            let mut m: BTreeMap<String, usize> = BTreeMap::new();
            for r in &group {
                *m.entry(key(r)).or_default() += 1;
            }
            m
        };
        if falloff == "custom" {
            top(&format!("{title} by falloff values"), &count(&|r| r.params.clone()), 60);
        }
        top(&format!("{title} by NiAlphaProperty"), &count(&|r| r.blend.clone()), 12);
        top(&format!("{title} by folder"), &count(&|r| r.folder.clone()), 25);
        top(&format!("{title} by base texture"), &count(&|r| r.texture.clone()), 15);
        let bits: Vec<String> = (0..32)
            .map(|bit| (bit, group.iter().filter(|r| r.flags >> bit & 1 != 0).count()))
            .filter(|&(_, n)| n > 0)
            .map(|(bit, n)| format!("{bit}: {n}"))
            .collect();
        println!("\n{title}, shapes per shader flag bit:\n  {}", bits.join(", "));
        if list_all || (list && bit6 == (falloff == "default")) {
            println!("\n{title}, every shape:");
            let mut lines: Vec<&str> = group.iter().map(|r| r.line.as_str()).collect();
            lines.sort();
            for l in lines {
                println!("  {l}");
            }
        }
    }
    let mut v: Vec<_> = params.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!("\nexamples:");
    for (k, _) in v.iter().take(15) {
        println!("  {k}: {:?}", examples[*k]);
    }
}
