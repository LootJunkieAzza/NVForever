//! Diagnostic: what projectile and muzzle flash models are made of, for deciding how
//! shots are drawn (docs/spec-notes/combat-rules.md §15). For each model named: its
//! nodes with their type and, for a NiBillboardNode, its billboard mode and where its
//! own axes point in its parent's frame; its shapes with
//! the box their vertices fill in the model's space (the root's transform left out) and
//! their triangle count; and the box of the whole model. With `--folder <prefix>`
//! instead of model names, one line per `.nif` under that folder: the whole model's
//! box, its shape count and its billboard modes.
//!
//! cargo run --release -p fnv-nif --example effect_model_survey -- <schema dir> <archive>... -- <model.nif>...
//! cargo run --release -p fnv-nif --example effect_model_survey -- <schema dir> <archive>... --folder <prefix>
//!
//! Takes archive paths rather than a Data directory so a survey names the archives it
//! reads; later archives override earlier ones.

use fnv_nif::scene::{mesh_data, scene_objects, Decoded, SceneObject};
use fnv_nif::{NifFile, Schema};
use std::collections::BTreeMap;

type Box3 = ([f32; 3], [f32; 3]);

fn apply(m: &[f32; 9], v: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| (0..3).map(|k| m[i * 3 + k] * v[k]).sum())
}

/// A point of the object's own space in the model's space (the root's transform left
/// out).
fn in_model(objects: &[SceneObject], o: &SceneObject, mut p: [f32; 3]) -> [f32; 3] {
    let mut at = Some(o);
    while let Some(n) = at.filter(|n| n.parent.is_some()) {
        let r = apply(&n.transform.rotation, p);
        p = std::array::from_fn(|i| n.transform.translation[i] + r[i] * n.transform.scale);
        at = n.parent.and_then(|b| objects.iter().find(|x| x.block == b));
    }
    p
}

fn grow(b: &mut Option<Box3>, p: [f32; 3]) {
    let (lo, hi) = b.get_or_insert((p, p));
    for i in 0..3 {
        lo[i] = lo[i].min(p[i]);
        hi[i] = hi[i].max(p[i]);
    }
}

fn show(b: &Option<Box3>) -> String {
    match b {
        Some((lo, hi)) => format!("x {:.1}..{:.1}, y {:.1}..{:.1}, z {:.1}..{:.1}", lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]),
        None => "no vertices".to_string(),
    }
}

struct Model {
    whole: Option<Box3>,
    shapes: usize,
    modes: BTreeMap<String, usize>,
    lines: Vec<String>,
}

fn survey(data: &[u8], schema: &Schema) -> Option<Model> {
    let f = NifFile::parse(data, Some(schema)).ok()?;
    let dec = Decoded::new(&f, data, schema);
    let objects = scene_objects(&f, &dec);
    let mut m = Model { whole: None, shapes: 0, modes: BTreeMap::new(), lines: Vec::new() };
    for o in &objects {
        let parent = o.parent.and_then(|b| objects.iter().find(|x| x.block == b)).and_then(|x| x.name.as_deref()).unwrap_or("-");
        let s = dec.blocks[o.block].as_ref().ok();
        let name = o.name.as_deref().unwrap_or("-");
        if o.is_node {
            let mode = s.filter(|_| o.type_name == "NiBillboardNode").and_then(|s| s.int("billboard_mode"));
            if let Some(mode) = mode {
                *m.modes.entry(mode.to_string()).or_default() += 1;
            }
            let [x, y, z] = o.transform.translation;
            // A billboard's own axes in its parent's frame: the columns of its rotation.
            let r = &o.transform.rotation;
            let axes = (0..3).map(|k| format!("({:.2}, {:.2}, {:.2})", r[k], r[3 + k], r[6 + k])).collect::<Vec<_>>().join(" ");
            m.lines.push(format!(
                "  {:16} {name:24} under {parent:22} at ({x:.2}, {y:.2}, {z:.2}){}",
                o.type_name,
                mode.map_or(String::new(), |b| format!(" billboard mode {b}, its x y z axes {axes}"))
            ));
            continue;
        }
        m.shapes += 1;
        let mesh = s.and_then(|s| s.block_ref("data")).and_then(|r| dec.get(r)).map(mesh_data);
        let mut b = None;
        for p in mesh.iter().flat_map(|d| &d.positions) {
            let q = in_model(&objects, o, *p);
            grow(&mut b, q);
            grow(&mut m.whole, q);
        }
        m.lines.push(format!("  {:16} {name:24} under {parent:22} {} triangles, {}", o.type_name, mesh.map_or(0, |d| d.triangles.len()), show(&b)));
    }
    Some(m)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let split = a.iter().position(|x| x == "--" || x == "--folder").expect("-- or --folder after the archives");
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let folder = (a[split] == "--folder").then(|| a[split + 1].replace('/', "\\").to_ascii_lowercase());
    let wanted: Vec<String> = a[split + 1..].iter().map(|p| p.replace('/', "\\").to_ascii_lowercase()).collect();
    for path in &a[2..split] {
        let mut ar = fnv_bsa::Archive::open(std::path::Path::new(path)).unwrap();
        for dir in ar.folders.clone() {
            let d = String::from_utf8_lossy(&dir.name).to_ascii_lowercase();
            for r in &dir.files {
                let p = format!("{d}\\{}", String::from_utf8_lossy(&r.name).to_ascii_lowercase());
                let take = match &folder {
                    Some(f) => p.starts_with(f.as_str()) && p.ends_with(".nif"),
                    None => wanted.contains(&p),
                };
                if take {
                    if let Ok(data) = ar.extract(r) {
                        files.insert(p, data);
                    }
                }
            }
        }
    }
    for (p, data) in &files {
        let Some(m) = survey(data, &schema) else {
            println!("{p}: not parsed");
            continue;
        };
        println!("{p}: {} shapes, billboard modes {:?}, {}", m.shapes, m.modes, show(&m.whole));
        if folder.is_none() {
            for l in &m.lines {
                println!("{l}");
            }
        }
    }
}
