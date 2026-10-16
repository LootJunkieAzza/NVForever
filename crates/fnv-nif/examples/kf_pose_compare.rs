//! Diagnostic: the named KF clips side by side, bone by bone: each clip's track priority
//! and how far its rotation (at the clip's start) is from the first clip's, in degrees,
//! plus the translation where it differs. Used to see what a weapon's `aimup` and
//! `aimdown` clips change against its `aim` clip (docs/spec-notes/animation.md §9.6).
//!
//! cargo run --release -p fnv-nif --example kf_pose_compare -- <schema dir> <archive>... -- <kf>...
//!
//! Takes archive paths rather than a Data directory so a survey names the archives it
//! reads; later archives override earlier ones.

use fnv_nif::anim::{sequences, TrackData};
use fnv_nif::scene::Decoded;
use fnv_nif::{NifFile, Schema};
use std::collections::BTreeMap;

struct Row {
    priority: i64,
    rotation: Option<[f32; 4]>,
    translation: Option<[f32; 3]>,
    animated: bool,
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let split = a.iter().position(|x| x == "--").expect("-- between the archives and the clips");
    let mut archives: Vec<fnv_bsa::Archive> = a[2..split].iter().map(|p| fnv_bsa::Archive::open(std::path::Path::new(p)).unwrap()).collect();
    let mut clips: Vec<(String, BTreeMap<String, Row>)> = Vec::new();
    for kf in &a[split + 1..] {
        let p = kf.replace('/', "\\").to_ascii_lowercase();
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
        let Some(s) = sequences(&f, &dec).into_iter().next() else { continue };
        let keys: Vec<String> = s.text_keys.iter().map(|(t, k)| format!("{t:.2}:{}", k.replace('\n', " | ").replace('\r', ""))).collect();
        println!("{p}: [{:.3}..{:.3}] cycle {} accum {:?}, {} tracks; text keys: {}", s.start_time, s.stop_time, s.cycle_type, s.accum_root_name, s.tracks.len(), keys.join(", "));
        // Tracks that are not transforms (visibility, colours, ...): named only.
        let other: Vec<String> = s
            .tracks
            .iter()
            .filter_map(|t| match &t.data {
                TrackData::Transform(_) => None,
                TrackData::Other(kind) => Some(format!("{} ({kind}, {}, priority {})", t.node_name.as_deref().unwrap_or("-"), t.controller_type.as_deref().unwrap_or("-"), t.priority.unwrap_or(-1))),
                TrackData::None => Some(format!("{} (no data, {})", t.node_name.as_deref().unwrap_or("-"), t.controller_type.as_deref().unwrap_or("-"))),
            })
            .collect();
        if !other.is_empty() && std::env::var_os("KF_OTHER").is_some() {
            println!("  other tracks: {}", other.join("; "));
        }
        let mut rows = BTreeMap::new();
        for t in &s.tracks {
            let TrackData::Transform(tt) = &t.data else { continue };
            let Some(n) = t.node_name.clone() else { continue };
            if std::env::var_os("KF_OTHER").is_some() {
                println!("  {n}: priority {}, translation {:?}, scale {:?}", t.priority.unwrap_or(-1), tt.translation.sample(s.start_time), tt.scale.sample(s.start_time));
            }
            // `KF_TRACK=<node>`: that track over the clip, every 1/30 s: translation and
            // the turn from its pose at the clip's start (degrees), to see where it jumps.
            if std::env::var("KF_TRACK").is_ok_and(|w| w.eq_ignore_ascii_case(&n)) {
                let mut time = s.start_time;
                while time <= s.stop_time + 1e-4 {
                    let turn = match (tt.rotation_at(s.start_time), tt.rotation_at(time)) {
                        (Some(x), Some(y)) => format!("{:6.1}°", angle(x, y)),
                        _ => "     -".to_string(),
                    };
                    let tr = tt.translation.sample(time).map_or("-".to_string(), |v| format!("({:7.2}, {:7.2}, {:7.2})", v[0], v[1], v[2]));
                    println!("    {n} {time:5.3}: {tr} {turn}");
                    time += 1.0 / 30.0;
                }
            }
            // Whether the pose moves over the clip: compare the start with the middle.
            let mid = 0.5 * (s.start_time + s.stop_time);
            let animated = match (tt.rotation_at(s.start_time), tt.rotation_at(mid)) {
                (Some(x), Some(y)) => angle(x, y) > 0.5,
                _ => false,
            };
            rows.insert(n, Row { priority: t.priority.unwrap_or(-1), rotation: tt.rotation_at(s.start_time), translation: tt.translation.sample(s.start_time), animated });
        }
        clips.push((p, rows));
    }
    let Some((_, base)) = clips.first() else { return };
    let mut names: Vec<&String> = clips.iter().flat_map(|(_, r)| r.keys()).collect();
    names.sort();
    names.dedup();
    println!("\nbone: per clip `priority` then, after the first, `rotation from the first clip in degrees` (`~` moves over the clip, `-` no track)");
    for n in names {
        let mut line = format!("{n:28}");
        for (i, (_, rows)) in clips.iter().enumerate() {
            let Some(r) = rows.get(n) else {
                line += "        -      ";
                continue;
            };
            let moves = if r.animated { "~" } else { " " };
            if i == 0 {
                line += &format!(" {:3}{moves}          ", r.priority);
                continue;
            }
            let d = match (base.get(n).and_then(|b| b.rotation), r.rotation) {
                (Some(x), Some(y)) => format!("{:6.1}", angle(x, y)),
                _ => "     ?".to_string(),
            };
            let moved = match (base.get(n).and_then(|b| b.translation), r.translation) {
                (Some(x), Some(y)) if (0..3).any(|k| (x[k] - y[k]).abs() > 0.05) => "t",
                _ => " ",
            };
            line += &format!(" {:3}{moves} {d}°{moved}    ", r.priority);
        }
        println!("{}", line.trim_end());
    }
}

/// The angle between two rotations (w, x, y, z), degrees.
fn angle(a: [f32; 4], b: [f32; 4]) -> f32 {
    let d: f32 = (0..4).map(|k| a[k] * b[k]).sum();
    2.0 * d.abs().min(1.0).acos().to_degrees()
}
