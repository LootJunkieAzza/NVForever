//! Diagnostic: the `ProjectileNode` of weapon models, for deciding where a shot is seen
//! to leave the weapon (docs/spec-notes/animation.md §9.9). Over every `.nif` under
//! `meshes\weapons\` in the archives named, counts the files with a node of that name
//! (with or without `##` before it), what it hangs from, whether it is turned, where its
//! axes point and how far it lies from the model's origin; the same counts for
//! `ShellCasingNode`. With `--list`, one line per file.
//!
//! cargo run --release -p fnv-nif --example muzzle_node_survey -- <schema dir> [--list] <archive.bsa>...
//!
//! Takes archive paths rather than a Data directory so a survey names the archives it
//! reads; a path in a later archive replaces the same path in an earlier one.

use fnv_nif::scene::{scene_objects, Decoded, SceneObject, Transform};
use fnv_nif::{NifFile, Schema};
use std::collections::BTreeMap;

/// `a × b` for row-major 3×3 matrices.
fn mul(a: &[f32; 9], b: &[f32; 9]) -> [f32; 9] {
    std::array::from_fn(|i| (0..3).map(|k| a[i / 3 * 3 + k] * b[k * 3 + i % 3]).sum())
}

fn apply(m: &[f32; 9], v: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| (0..3).map(|k| m[i * 3 + k] * v[k]).sum())
}

/// The object's place in the model: its translation and rotation with every parent's
/// applied (the root's own transform included).
fn in_model(objects: &[SceneObject], o: &SceneObject) -> ([f32; 3], [f32; 9]) {
    let mut t = o.transform.translation;
    let mut r = o.transform.rotation;
    let mut parent = o.parent;
    while let Some(p) = parent.and_then(|b| objects.iter().find(|x| x.block == b)) {
        let s = apply(&p.transform.rotation, t);
        t = std::array::from_fn(|i| p.transform.translation[i] + s[i] * p.transform.scale);
        r = mul(&p.transform.rotation, &r);
        parent = p.parent;
    }
    (t, r)
}

fn turned(r: &[f32; 9]) -> bool {
    r.iter().zip(&Transform::IDENTITY.rotation).any(|(a, b)| (a - b).abs() > 1e-3)
}

#[derive(Default)]
struct Count {
    files: usize,
    /// Named with `##` before the name.
    hashed: usize,
    under_root: usize,
    parents: BTreeMap<String, usize>,
    turned_local: usize,
    turned_in_model: usize,
    /// Smallest and largest distance from the model's origin, with the file.
    near: Option<(f32, String)>,
    far: Option<(f32, String)>,
    /// The axis of the model the node lies furthest along.
    axis: BTreeMap<&'static str, usize>,
    /// Where the node's own x, y and z axes point in the model (`+y -x +z`: its x along
    /// the model's y, its y against the model's x), for nodes turned by right angles;
    /// `other` for the rest.
    facing: BTreeMap<String, usize>,
}

/// The model axis a unit vector lies along, if it does within about 8 degrees.
fn along(v: [f32; 3]) -> Option<String> {
    let k = (0..3).max_by(|&i, &j| v[i].abs().total_cmp(&v[j].abs())).unwrap();
    (v[k].abs() > 0.99).then(|| format!("{}{}", if v[k] < 0.0 { '-' } else { '+' }, ["x", "y", "z"][k]))
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let list = a.iter().any(|x| x == "--list");
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for path in a[2..].iter().filter(|x| !x.starts_with("--")) {
        let mut ar = fnv_bsa::Archive::open(std::path::Path::new(path)).unwrap();
        for folder in ar.folders.clone() {
            let dir = String::from_utf8_lossy(&folder.name).to_ascii_lowercase();
            if !dir.starts_with("meshes\\weapons") {
                continue;
            }
            for r in &folder.files {
                let name = String::from_utf8_lossy(&r.name).to_ascii_lowercase();
                if name.ends_with(".nif") {
                    if let Ok(data) = ar.extract(r) {
                        files.insert(format!("{dir}\\{name}"), data);
                    }
                }
            }
        }
    }
    let names = ["ProjectileNode", "ShellCasingNode"];
    let mut counts = [Count::default(), Count::default()];
    let (mut parsed, mut failed, mut first_person) = (0, 0, [0usize; 2]);
    for (path, data) in &files {
        let Ok(f) = NifFile::parse(data, Some(&schema)) else {
            failed += 1;
            continue;
        };
        parsed += 1;
        let dec = Decoded::new(&f, data, &schema);
        let objects = scene_objects(&f, &dec);
        let is_first = path.rsplit('\\').next().is_some_and(|n| n.starts_with("1st"));
        first_person[0] += is_first as usize;
        let mut line = Vec::new();
        for (name, c) in names.iter().zip(&mut counts) {
            let Some(o) = objects.iter().find(|o| o.is_node && o.name.as_deref().is_some_and(|n| n.trim_start_matches("##").eq_ignore_ascii_case(name))) else { continue };
            c.files += 1;
            c.hashed += o.name.as_deref().is_some_and(|n| n.starts_with("##")) as usize;
            if *name == "ProjectileNode" {
                first_person[1] += is_first as usize;
            }
            let parent = o.parent.and_then(|b| objects.iter().find(|x| x.block == b));
            if parent.is_some_and(|p| p.parent.is_none()) {
                c.under_root += 1;
            } else {
                *c.parents.entry(parent.and_then(|p| p.name.clone()).unwrap_or_else(|| "-".into())).or_default() += 1;
            }
            let (t, r) = in_model(&objects, o);
            c.turned_local += turned(&o.transform.rotation) as usize;
            c.turned_in_model += turned(&r) as usize;
            let d = t.iter().map(|x| x * x).sum::<f32>().sqrt();
            if c.near.as_ref().is_none_or(|n| d < n.0) {
                c.near = Some((d, path.clone()));
            }
            if c.far.as_ref().is_none_or(|n| d > n.0) {
                c.far = Some((d, path.clone()));
            }
            let k = (0..3).max_by(|&i, &j| t[i].abs().total_cmp(&t[j].abs())).unwrap();
            *c.axis.entry(["x", "y", "z"][k]).or_default() += 1;
            // The node's axes in the model: the columns of its rotation.
            let axes: Option<Vec<String>> = (0..3).map(|k| along([r[k], r[3 + k], r[6 + k]])).collect();
            let facing = axes.map_or("other".to_string(), |a| a.join(" "));
            *c.facing.entry(facing.clone()).or_default() += 1;
            line.push(format!("{} at ({:.2}, {:.2}, {:.2}) axes {facing}", o.name.as_deref().unwrap_or(name), t[0], t[1], t[2]));
        }
        if list {
            println!("{path}: {}", if line.is_empty() { "neither".to_string() } else { line.join("; ") });
        }
    }
    println!("{parsed} weapon models read ({failed} not parsed); {} of them first-person (`1st…`), {} of those with a ProjectileNode", first_person[0], first_person[1]);
    for (name, c) in names.iter().zip(&counts) {
        println!("\n{name}: in {} of {parsed} models ({} of them as `##{name}`)", c.files, c.hashed);
        println!("  directly under the root: {}; under another node: {:?}", c.under_root, c.parents);
        println!("  with a rotation of its own: {}; turned against the model's axes: {}", c.turned_local, c.turned_in_model);
        println!("  furthest along the model's axis: {:?}", c.axis);
        println!("  its x, y and z axes in the model: {:?}", c.facing);
        if let (Some(n), Some(f)) = (&c.near, &c.far) {
            println!("  distance from the model's origin: {:.2} ({}) to {:.2} ({})", n.0, n.1, f.0, f.1);
        }
    }
}
