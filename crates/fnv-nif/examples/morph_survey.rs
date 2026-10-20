//! Survey of vertex morphs (`NiGeomMorpherController` + `NiMorphData`) for
//! docs/spec-notes/vertex-morphs.md: over every `.nif` and `.kf` in the given archives,
//! one line per controller in a `.nif` (how it runs: "free" Active and not manager
//! controlled, "seq" manager controlled, "off" neither; whether its shape moves; the
//! shape's type and whether it is skinned; morph and vertex counts; whether morph 0
//! equals the shape's stored vertices; the largest later morph vector; the weights'
//! interpolator types; cycle type, span and frequency), then counts, and the controller
//! IDs and interpolator types of sequence tracks that drive a morpher (`.nif` and `.kf`).
//!
//! cargo run --release -p fnv-nif --example morph_survey -- <schema dir> <archive.bsa>...

use fnv_nif::anim::{geom_morphers, sequences};
use fnv_nif::scene::Decoded;
use fnv_nif::{NifFile, Schema};
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut counts: BTreeMap<String, (BTreeSet<String>, usize)> = BTreeMap::new();
    let mut seq_ids: BTreeMap<String, usize> = BTreeMap::new();
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif") || p.ends_with(".kf")).collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            if !data.windows(23).any(|w| w == b"NiGeomMorpherController") {
                continue;
            }
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            let dec = Decoded::new(&file, &data, &schema);
            let kf = path.ends_with(".kf");
            for s in sequences(&file, &dec) {
                for t in s.tracks.iter().filter(|t| t.controller_type.as_deref() == Some("NiGeomMorpherController")) {
                    let interp = match &t.data {
                        fnv_nif::anim::TrackData::Other(x) => x.clone(),
                        fnv_nif::anim::TrackData::None => "none".into(),
                        fnv_nif::anim::TrackData::Transform(_) => "transform".into(),
                    };
                    let key = format!("{} id={:?} interp={interp} values={}", if kf { "kf" } else { "nif" }, t.controller_id, t.values.is_some());
                    *seq_ids.entry(key).or_default() += 1;
                }
            }
            for m in geom_morphers(&file, &dec) {
                let how = if m.flags & 0x20 != 0 { "seq" } else if m.flags & 0x8 != 0 { "free" } else { "off" };
                let moves = m.moves();
                let key = format!("{} {how} {}", if kf { "kf" } else { "nif" }, if moves { "moves" } else { "still" });
                let e = counts.entry(key).or_default();
                e.1 += 1;
                e.0.insert(path.clone());
                if kf {
                    continue;
                }
                let target = usize::try_from(m.target).ok();
                let ty = target.and_then(|t| file.blocks.get(t)).map_or("none", |b| b.type_name.as_str());
                let shape = target.and_then(|t| dec.get(t as i32));
                let skinned = shape.and_then(|s| s.block_ref("skin_instance")).is_some_and(|r| r >= 0);
                let sdata = shape.and_then(|s| s.block_ref("data")).and_then(|r| dec.get(r));
                let sverts: Vec<[f32; 3]> = sdata.and_then(|d| d.floats("vertices")).map(|f| f.as_chunks::<3>().0.to_vec()).unwrap_or_default();
                let base_diff = m.morphs.first().map(|b| {
                    if b.vectors.len() != sverts.len() {
                        f32::INFINITY
                    } else {
                        b.vectors.iter().zip(&sverts).map(|(p, q)| (0..3).map(|k| (p[k] - q[k]).abs()).fold(0.0, f32::max)).fold(0.0, f32::max)
                    }
                });
                let later = m.morphs.iter().skip(1).flat_map(|x| x.vectors.iter()).map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(0.0, f32::max);
                let interps: Vec<String> = m.morphs.iter().map(|x| x.interpolator_type.clone().unwrap_or("-".into())).collect();
                // How far each weight is from its start value at the stop time: a loop
                // without a jump has all of them near 0.
                let seam = (1..m.morphs.len()).map(|i| (m.weight_at(i, m.start_time) - m.weight_at(i, m.stop_time)).abs()).fold(0.0, f32::max);
                let names: Vec<String> = m.morphs.iter().map(|x| x.name.clone().unwrap_or_default()).collect();
                let wts: Vec<String> = m.morphs.iter().map(|x| format!("{:.2}", x.stored_weight)).collect();
                println!(
                    "{path} | {how} {} | {ty}{} | morphs {} verts {} (shape {}) rel {} normals {} | base diff {:?} | later max {later:.2} | interps {:?} names {:?} stored {:?} | cycle {} span {:.2}-{:.2} freq {:.2} phase {:.2} seam {seam:.3}",
                    if moves { "moves" } else { "still" },
                    if skinned { " skinned" } else { "" },
                    m.morphs.len(),
                    m.num_vertices,
                    sverts.len(),
                    m.relative,
                    m.update_normals,
                    base_diff,
                    interps,
                    names,
                    wts,
                    (m.flags >> 1) & 3,
                    m.start_time,
                    m.stop_time,
                    m.frequency,
                    m.phase
                );
            }
        }
    }
    println!("\n## Controllers (files, blocks)");
    for (k, (f, b)) in &counts {
        println!("{:>6} {b:>6}  {k}", f.len());
    }
    println!("\n## Sequence tracks driving a morpher");
    for (k, n) in &seq_ids {
        println!("{n:>6}  {k}");
    }
}
