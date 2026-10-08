//! Diagnostic: sample every transform track of a KF over its whole time range and report
//! values a renderer cannot use (non-finite numbers, translations far outside the
//! skeleton, near-zero or huge scales, rotations far from unit length).
//!
//! cargo run --release -p fnv-nif --example kf_sample_check -- <Data dir> <schema dir> <kf>...

use fnv_nif::anim::{sequences, TrackData};
use fnv_nif::scene::Decoded;
use fnv_nif::{NifFile, Schema};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let schema = Schema::load_dir(std::path::Path::new(&a[2])).unwrap();
    let read = |p: &str| -> Vec<u8> {
        let p = p.replace('/', "\\").to_ascii_lowercase();
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsa")) {
                let Ok(mut arc) = fnv_bsa::Archive::open(&path) else { continue };
                if let Some((fi, ri)) = arc.find(&p) {
                    let r = arc.folders[fi].files[ri].clone();
                    return arc.extract(&r).unwrap();
                }
            }
        }
        panic!("{p} not found")
    };
    for kf in &a[3..] {
        let data = read(kf);
        let file = NifFile::parse(&data, Some(&schema)).unwrap();
        let dec = Decoded::new(&file, &data, &schema);
        for s in sequences(&file, &dec) {
            println!("{kf}: sequence {:?} [{}..{}] {} tracks", s.name, s.start_time, s.stop_time, s.tracks.len());
            for t in &s.tracks {
                let TrackData::Transform(tt) = &t.data else { continue };
                let name = t.node_name.as_deref().unwrap_or("?");
                let (mut bad, mut first) = (0, None);
                let (mut tmax, mut smin, mut smax, mut qdev) = (0f32, f32::MAX, 0f32, 0f32);
                let steps = 400;
                for i in 0..=steps {
                    let time = s.start_time + (s.stop_time - s.start_time) * i as f32 / steps as f32;
                    let tr = tt.translation.sample(time);
                    let q = tt.rotation_at(time);
                    let sc = tt.scale.sample(time);
                    let finite = tr.is_none_or(|v| v.iter().all(|x| x.is_finite()))
                        && q.is_none_or(|v| v.iter().all(|x| x.is_finite()))
                        && sc.is_none_or(|v| v[0].is_finite());
                    if let Some(v) = tr {
                        tmax = tmax.max(v.iter().map(|x| x.abs()).fold(0.0, f32::max));
                    }
                    if let Some([v]) = sc {
                        smin = smin.min(v);
                        smax = smax.max(v);
                    }
                    if let Some(q) = q {
                        qdev = qdev.max((q.iter().map(|x| x * x).sum::<f32>().sqrt() - 1.0).abs());
                    }
                    let suspicious = !finite || tr.is_some_and(|v| v.iter().any(|x| x.abs() > 1.0e5)) || sc.is_some_and(|[v]| !(1.0e-3..=1.0e3).contains(&v));
                    if suspicious {
                        bad += 1;
                        first.get_or_insert((time, tr, q, sc));
                    }
                }
                if bad > 0 {
                    println!("  {name:28} BAD {bad}/{} first {:?}", steps + 1, first);
                } else {
                    println!("  {name:28} ok  |t|max {tmax:9.2}  scale {smin:.3}..{smax:.3}  |q|-1 max {qdev:.2e}");
                }
            }
        }
    }
}
