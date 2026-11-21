//! Diagnostic: which parts of weapon models the actor clips move. Prints each model's
//! scene objects (type, name, parent, local translation), then for each clip the tracks
//! whose target is not a node of the skeleton: priority, controller type, how far the
//! target moves over the clip, and which of the models have an object of that name
//! (docs/spec-notes/animation.md §9.8).
//!
//! cargo run --release -p fnv-nif --example weapon_parts_survey -- <schema dir> <archive>... -- <skeleton.nif> <model.nif>... <clip.kf>...
//!
//! Takes archive paths rather than a Data directory so a survey names the archives it
//! reads; later archives override earlier ones.

use fnv_nif::anim::{sequences, Channel, TrackData};
use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema};
use std::collections::BTreeSet;

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
    let mut skeleton: Option<BTreeSet<String>> = None;
    let mut models: Vec<(String, BTreeSet<String>)> = Vec::new();
    for (p, data) in files.iter().filter(|f| f.to_ascii_lowercase().ends_with(".nif")).filter_map(|f| read(f)) {
        let f = NifFile::parse(&data, Some(&schema)).unwrap();
        let dec = Decoded::new(&f, &data, &schema);
        let objects = scene_objects(&f, &dec);
        let names: BTreeSet<String> = objects.iter().filter_map(|o| o.name.clone()).collect();
        if skeleton.is_none() {
            println!("{p}: skeleton, {} named objects", names.len());
            skeleton = Some(names);
            continue;
        }
        println!("{p}: {} objects, {} sequences in the file", objects.len(), sequences(&f, &dec).len());
        for o in &objects {
            let parent = o.parent.and_then(|b| objects.iter().find(|x| x.block == b)).and_then(|x| x.name.as_deref()).unwrap_or("-");
            let s = dec.blocks[o.block].as_ref().ok();
            let controller = s.and_then(|s| s.block_ref("controller")).filter(|&c| c >= 0).and_then(|c| f.blocks.get(c as usize)).map(|b| b.type_name.as_str());
            let skinned = s.and_then(|s| s.block_ref("skin_instance")).is_some_and(|r| r >= 0);
            let [x, y, z] = o.transform.translation;
            println!(
                "  {:18} {:28} under {:24} at ({x:.2}, {y:.2}, {z:.2}){}{}",
                o.type_name,
                o.name.as_deref().unwrap_or("-"),
                parent,
                controller.map_or(String::new(), |c| format!(" controller {c}")),
                if skinned { " skinned" } else { "" }
            );
        }
        models.push((p, names));
    }
    let skeleton = skeleton.unwrap_or_default();
    for (p, data) in files.iter().filter(|f| f.to_ascii_lowercase().ends_with(".kf")).filter_map(|f| read(f)) {
        let f = NifFile::parse(&data, Some(&schema)).unwrap();
        let dec = Decoded::new(&f, &data, &schema);
        let Some(s) = sequences(&f, &dec).into_iter().next() else { continue };
        let keys: Vec<String> = s.text_keys.iter().map(|(t, k)| format!("{t:.2}:{}", k.replace('\n', " | ").replace('\r', ""))).collect();
        let bound = s.tracks.iter().filter(|t| t.node_name.as_ref().is_some_and(|n| skeleton.contains(n))).count();
        println!("{p}: [{:.3}..{:.3}] cycle {}, {} tracks, {bound} on skeleton nodes; text keys: {}", s.start_time, s.stop_time, s.cycle_type, s.tracks.len(), keys.join(", "));
        for t in s.tracks.iter().filter(|t| t.node_name.as_ref().is_none_or(|n| !skeleton.contains(n))) {
            let name = t.node_name.as_deref().unwrap_or("-");
            let what = match &t.data {
                TrackData::Transform(tt) => {
                    // The largest move from the start, over 32 samples of the clip.
                    let (t0, r0) = (tt.translation.sample(s.start_time), tt.rotation_at(s.start_time));
                    let (mut dt, mut dr) = (0.0f32, 0.0f32);
                    for k in 1..=32 {
                        let time = s.start_time + (s.stop_time - s.start_time) * k as f32 / 32.0;
                        if let (Some(a), Some(b)) = (t0, tt.translation.sample(time)) {
                            dt = dt.max((0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt());
                        }
                        if let (Some(a), Some(b)) = (r0, tt.rotation_at(time)) {
                            dr = dr.max(angle(a, b));
                        }
                    }
                    let at = t0.map_or("no translation".to_string(), |[x, y, z]| format!("starts at ({x:.2}, {y:.2}, {z:.2})"));
                    format!("transform, {at}, moves {dt:.2} units, turns {dr:.1}°{}", if tt.scale.sample(s.start_time).is_some() { ", scale" } else { "" })
                }
                TrackData::Other(kind) => kind.clone(),
                TrackData::None => "no data".to_string(),
            };
            // `PARTS_DETAIL=<node name>`: that node's track sampled at nine times, or at
            // `PARTS_SAMPLES` + 1.
            if let (TrackData::Transform(tt), true) = (&t.data, std::env::var("PARTS_DETAIL").is_ok_and(|d| d == name)) {
                let kind = |animated: bool, bspline: bool| if bspline { "B-spline" } else if animated { "keys" } else { "static" };
                println!(
                    "    {name}: translation {}, rotation {}, scale {}",
                    kind(tt.translation.is_animated(), matches!(tt.translation, Channel::BSpline { .. })),
                    if tt.euler.is_some() { "Euler keys" } else { kind(tt.rotation.is_animated(), matches!(tt.rotation, Channel::BSpline { .. })) },
                    kind(tt.scale.is_animated(), matches!(tt.scale, Channel::BSpline { .. }))
                );
                let n: usize = std::env::var("PARTS_SAMPLES").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
                for k in 0..=n {
                    let time = s.start_time + (s.stop_time - s.start_time) * k as f32 / n as f32;
                    println!("    {time:.3}: translation {:.2?} rotation (w, x, y, z) {:.3?} scale {:.2?}", tt.translation.sample(time), tt.rotation_at(time), tt.scale.sample(time));
                }
            }
            let on: Vec<&str> = models.iter().filter(|(_, names)| names.contains(name)).map(|(p, _)| p.rsplit('\\').next().unwrap_or(p)).collect();
            println!(
                "  {name:28} priority {:3} {:28} {what}; in: {}",
                t.priority.unwrap_or(-1),
                t.controller_type.as_deref().unwrap_or("-"),
                if on.is_empty() { "none".to_string() } else { on.join(", ") }
            );
        }
    }
}

/// The angle between two rotations (w, x, y, z), degrees.
fn angle(a: [f32; 4], b: [f32; 4]) -> f32 {
    let d: f32 = (0..4).map(|k| a[k] * b[k]).sum();
    2.0 * d.abs().min(1.0).acos().to_degrees()
}
