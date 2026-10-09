//! Diagnostic: for each KF named, the sequence's timing and accumulation root, its text
//! keys, and its transform tracks grouped by priority (the bones each layer drives). Used
//! to work out how movement, weapon and sneak clips combine (docs/spec-notes/animation.md
//! §8).
//!
//! cargo run --release -p fnv-nif --example kf_layer_survey -- <Data dir> <schema dir> <kf>...
//!
//! `KF_ROOT=1` also prints the accumulation root's translation at the start and stop
//! times (the distance a movement clip covers in one cycle).

use fnv_nif::anim::{sequences, TrackData};
use fnv_nif::scene::Decoded;
use fnv_nif::{NifFile, Schema};
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let schema = Schema::load_dir(std::path::Path::new(&a[2])).unwrap();
    let mut archives: Vec<fnv_bsa::Archive> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsa")))
        .filter_map(|p| fnv_bsa::Archive::open(&p).ok())
        .collect();
    for kf in &a[3..] {
        let p = kf.replace('/', "\\").to_ascii_lowercase();
        // Later archives (Update.bsa) override earlier ones: search from the end.
        let data = archives.iter_mut().rev().find_map(|ar| {
            let (fi, ri) = ar.find(&p)?;
            let r = ar.folders[fi].files[ri].clone();
            ar.extract(&r).ok()
        });
        let Some(data) = data else {
            println!("{p}: not found");
            continue;
        };
        let f = NifFile::parse(&data, Some(&schema)).unwrap();
        let dec = Decoded::new(&f, &data, &schema);
        for s in sequences(&f, &dec) {
            println!(
                "{p}: sequence {:?} [{:.3}..{:.3}] cycle {} freq {} accum {:?}, {} tracks",
                s.name,
                s.start_time,
                s.stop_time,
                s.cycle_type,
                s.frequency,
                s.accum_root_name,
                s.tracks.len()
            );
            let keys: Vec<String> = s.text_keys.iter().map(|(t, k)| format!("{t:.2}:{}", k.replace('\n', " | ").replace('\r', ""))).collect();
            println!("  text keys: {}", keys.join(", "));
            let mut by_pri: BTreeMap<i64, Vec<String>> = BTreeMap::new();
            for t in &s.tracks {
                let kind = match &t.data {
                    TrackData::Transform(_) => "",
                    TrackData::Other(o) => o.as_str(),
                    TrackData::None => "none",
                };
                let n = t.node_name.clone().unwrap_or("-".into());
                by_pri.entry(t.priority.unwrap_or(-1)).or_default().push(if kind.is_empty() { n } else { format!("{n}({kind})") });
            }
            for (pri, names) in by_pri {
                println!("  priority {pri}: {} tracks: {}", names.len(), names.join(", "));
            }
            if std::env::var_os("KF_ROOT").is_some() {
                for t in &s.tracks {
                    let TrackData::Transform(tt) = &t.data else { continue };
                    let n = t.node_name.as_deref().unwrap_or("");
                    if n == "Bip01" || Some(n) == s.accum_root_name.as_deref() || n == "Bip01 NonAccum" {
                        let q = |time| tt.rotation_at(time).map(|q| q.map(|x| (x * 1000.0).round() / 1000.0));
                        println!(
                            "  {n}: translation start {:?} stop {:?}; rotation start {:?} stop {:?}",
                            tt.translation.sample(s.start_time),
                            tt.translation.sample(s.stop_time),
                            q(s.start_time),
                            q(s.stop_time)
                        );
                    }
                }
            }
        }
    }
}
