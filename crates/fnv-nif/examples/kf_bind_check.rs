//! Diagnostic: compare a KF's pose at its start time with the bind pose of the nodes it
//! animates in a mesh. Idles start near the bind pose, so large rotation differences
//! point at a decoding or convention error.
//!
//! cargo run --release -p fnv-nif --example kf_bind_check -- <Data dir> <schema dir> <mesh> <kf>
//!
//! `KF_VALUES=1` also prints the start values next to the bind values; `KF_BENCH=1` times
//! sampling every transform track of each sequence (the per-actor cost of one frame).

use fnv_nif::anim::{sequences, Channel, TrackData};
use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema};
use std::collections::HashMap;

fn angle(a: [f32; 4], b: [f32; 4]) -> f32 {
    let d: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum::<f32>().abs().min(1.0);
    2.0 * d.acos().to_degrees()
}

/// Quaternion (w, x, y, z) of a row-major rotation matrix.
fn quat_of(m: &[f32; 9]) -> [f32; 4] {
    let (m00, m01, m02, m10, m11, m12, m20, m21, m22) = (m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8]);
    let tr = m00 + m11 + m22;
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [0.25 * s, (m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [(m21 - m12) / s, 0.25 * s, (m01 + m10) / s, (m02 + m20) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(m02 - m20) / s, (m01 + m10) / s, 0.25 * s, (m12 + m21) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(m10 - m01) / s, (m02 + m20) / s, (m12 + m21) / s, 0.25 * s]
    };
    let n = q.iter().map(|x| x * x).sum::<f32>().sqrt();
    q.map(|x| x / n)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let vfs = fnv_vfs_shim::open(&a[1]);
    let schema = Schema::load_dir(std::path::Path::new(&a[2])).unwrap();
    let load = |p: &str| {
        let data = vfs(p);
        let f = NifFile::parse(&data, Some(&schema)).unwrap();
        (f, data)
    };
    let (mf, md) = load(&a[3]);
    let mdec = Decoded::new(&mf, &md, &schema);
    let objects = scene_objects(&mf, &mdec);
    let bind: HashMap<String, fnv_nif::scene::Transform> = objects.iter().filter_map(|o| Some((o.name.clone()?, o.transform))).collect();
    // The chain from the first animated node's parent up to the mesh root, with bind values.
    let by_block: HashMap<usize, &fnv_nif::scene::SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
    let (kf, kd) = load(&a[4]);
    let kdec = Decoded::new(&kf, &kd, &schema);
    if std::env::var_os("KF_BENCH").is_some() {
        for s in sequences(&kf, &kdec) {
            let tracks: Vec<_> = s.tracks.iter().filter_map(|t| if let TrackData::Transform(tt) = &t.data { Some(tt) } else { None }).collect();
            let frames = 20_000;
            let t0 = std::time::Instant::now();
            let mut sink = 0.0f32;
            for i in 0..frames {
                let time = s.start_time + (s.stop_time - s.start_time) * (i as f32 / frames as f32);
                for tt in &tracks {
                    sink += tt.translation.sample(time).map_or(0.0, |v| v[0]);
                    sink += tt.rotation_at(time).map_or(0.0, |q| q[0]);
                    sink += tt.scale.sample(time).map_or(0.0, |v| v[0]);
                }
            }
            let per = t0.elapsed().as_secs_f64() / frames as f64;
            println!("bench {:?}: {} transform tracks, {:.2} us per pose ({sink:.0})", s.name, tracks.len(), per * 1e6);
        }
    }
    for s in sequences(&kf, &kdec) {
        println!("sequence {:?} [{}..{}] cycle {} frequency {} accum root {:?}", s.name, s.start_time, s.stop_time, s.cycle_type, s.frequency, s.accum_root_name);
        if let Some(first) = s.tracks.iter().find_map(|t| t.node_name.as_deref()) {
            let mut cur = objects.iter().find(|o| o.name.as_deref() == Some(first));
            while let Some(o) = cur {
                println!("  mesh chain: {:28} bind t {:?} q(wxyz) {:?}", o.name.as_deref().unwrap_or("-"), o.transform.translation, quat_of(&o.transform.rotation));
                cur = o.parent.and_then(|p| by_block.get(&p).copied());
            }
        }
        for t in &s.tracks {
            let (Some(name), TrackData::Transform(tt)) = (&t.node_name, &t.data) else {
                println!("{:28} not a transform track: controller {:?}, {:?}", t.node_name.as_deref().unwrap_or("-"), t.controller_type, t.data);
                continue;
            };
            let Some(b) = bind.get(name) else {
                println!("{name:28} not in the mesh");
                continue;
            };
            let qb = quat_of(&b.rotation);
            let kind = match &tt.rotation {
                Channel::BSpline { .. } => "bspline",
                Channel::Keys(_) => "keys",
                Channel::Static(_) => "static",
            };
            let Some(q) = tt.rotation_at(s.start_time) else { continue };
            let alt = [q[3], q[0], q[1], q[2]]; // if stored x, y, z, w
            let conj = [q[0], -q[1], -q[2], -q[3]];
            let tr = tt.translation.sample(s.start_time);
            let dt = tr.map(|v| ((v[0] - b.translation[0]).powi(2) + (v[1] - b.translation[1]).powi(2) + (v[2] - b.translation[2]).powi(2)).sqrt());
            if std::env::var_os("KF_VALUES").is_some() {
                println!("  {name:28} kf t {:?} q(wxyz) {:?} | bind t {:?} q {:?}", tt.translation.sample(s.start_time), q, b.translation, qb);
            }
            println!(
                "{name:28} {kind:7} rot diff as-is {:6.1}°  xyzw {:6.1}°  conj {:6.1}°  trans diff {:?}",
                angle(q, qb),
                angle(alt, qb),
                angle(conj, qb),
                dt
            );
        }
    }
}

/// Minimal archive access for the example (avoids a dependency cycle on fnv-vfs).
mod fnv_vfs_shim {
    pub fn open(data_dir: &str) -> impl Fn(&str) -> Vec<u8> {
        let dir = std::path::PathBuf::from(data_dir);
        move |p: &str| {
            let p = p.replace('/', "\\").to_ascii_lowercase();
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsa")) {
                    let Ok(mut a) = fnv_bsa::Archive::open(&path) else { continue };
                    if let Some((fi, ri)) = a.find(&p) {
                        let r = a.folders[fi].files[ri].clone();
                        return a.extract(&r).unwrap();
                    }
                }
            }
            panic!("{p} not found")
        }
    }
}
