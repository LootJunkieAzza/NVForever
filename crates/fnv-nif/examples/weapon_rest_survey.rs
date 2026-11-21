//! Diagnostic: where each clip puts a weapon model's moving parts at its start and its
//! end, against the part's place in the model. For each `##` node of the model, each
//! clip gives its turn (degrees) and offset (units) from the model's pose at the clip's
//! start and end, or `-` where it has no transform track for the node. Says which clip
//! leaves a part away from the model's pose (the 9mm pistol's draw clip cocks its
//! hammer, which the aim clip does not key; docs/spec-notes/animation.md §9.8).
//!
//! cargo run --release -p fnv-nif --example weapon_rest_survey -- <schema dir> <archive>... -- <model.nif> <clip.kf>...
//!
//! Takes archive paths rather than a Data directory so a survey names the archives it
//! reads; later archives override earlier ones.

use fnv_nif::anim::{sequences, TrackData};
use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let split = a.iter().position(|x| x == "--").expect("-- between the archives and the files");
    let mut archives: Vec<fnv_bsa::Archive> = a[2..split].iter().map(|p| fnv_bsa::Archive::open(std::path::Path::new(p)).unwrap()).collect();
    let mut read = |path: &str| {
        let p = path.replace('/', "\\").to_ascii_lowercase();
        let data = archives.iter_mut().rev().find_map(|ar| {
            let (fi, ri) = ar.find(&p)?;
            let r = ar.folders[fi].files[ri].clone();
            ar.extract(&r).ok()
        });
        if data.is_none() {
            println!("{p}: not found");
        }
        data.map(|d| (p, d))
    };
    let files = &a[split + 1..];
    let Some((model, data)) = read(&files[0]) else { return };
    let f = NifFile::parse(&data, Some(&schema)).unwrap();
    let dec = Decoded::new(&f, &data, &schema);
    // The model's `##` nodes: name, translation and rotation (row-major 3×3). Two nodes
    // may share a name (the 9mm's `##nmClip`).
    let parts: Vec<(String, [f32; 3], [f32; 9], usize)> = scene_objects(&f, &dec)
        .into_iter()
        .filter(|o| o.is_node)
        .filter_map(|o| {
            let n = o.name.clone().filter(|n| n.starts_with("##"))?;
            Some((n, o.transform.translation, o.transform.rotation, o.children.len()))
        })
        .collect();
    println!("{model}: {} parts", parts.len());
    for (n, t, _, kids) in &parts {
        println!("  {n:24} at ({:.2}, {:.2}, {:.2}), {kids} child(ren)", t[0], t[1], t[2]);
    }
    for (p, data) in files[1..].iter().filter_map(|f| read(f)) {
        let f = NifFile::parse(&data, Some(&schema)).unwrap();
        let dec = Decoded::new(&f, &data, &schema);
        let Some(s) = sequences(&f, &dec).into_iter().next() else { continue };
        println!("{p} [{:.3}..{:.3}]", s.start_time, s.stop_time);
        let mut seen = Vec::new();
        for (n, bt, br, _) in &parts {
            if seen.contains(n) {
                continue;
            }
            seen.push(n.clone());
            let track = s.tracks.iter().find_map(|t| match &t.data {
                TrackData::Transform(tt) if t.node_name.as_deref() == Some(n) => Some(tt),
                _ => None,
            });
            let Some(tt) = track else {
                println!("  {n:24} -");
                continue;
            };
            let at = |time: f32| {
                let turn = tt.rotation_at(time).map_or(0.0, |q| angle_to(q, br));
                let off = tt.translation.sample(time).map_or(0.0, |v| (0..3).map(|k| (v[k] - bt[k]).powi(2)).sum::<f32>().sqrt());
                format!("{turn:5.1}° {off:5.2} u")
            };
            let keyed = if tt.rotation_at(s.start_time).is_none() { " (rotation not keyed)" } else { "" };
            println!("  {n:24} start {}  end {}{keyed}", at(s.start_time), at(s.stop_time));
        }
    }
}

/// The angle between rotation `q` (w, x, y, z) and the row-major matrix `m`, degrees.
fn angle_to(q: [f32; 4], m: &[f32; 9]) -> f32 {
    let [w, x, y, z] = q;
    // The quaternion's matrix, row-major (column vectors).
    let r = [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y - w * z),
        2.0 * (x * z + w * y),
        2.0 * (x * y + w * z),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z - w * x),
        2.0 * (x * z - w * y),
        2.0 * (y * z + w * x),
        1.0 - 2.0 * (x * x + y * y),
    ];
    // trace(rᵀ m)
    let tr: f32 = (0..9).map(|k| r[k] * m[k]).sum();
    ((tr - 1.0) / 2.0).clamp(-1.0, 1.0).acos().to_degrees()
}
