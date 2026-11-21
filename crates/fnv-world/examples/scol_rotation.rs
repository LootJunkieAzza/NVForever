//! Evidence for the reference rotation convention (docs/spec-notes/world.md §4).
//!
//! A static collection (SCOL) lists its parts as (STAT, position, rotation, scale) and
//! also ships a combined mesh. Comparing the combined mesh's part nodes with the stored
//! placements under every candidate Euler convention shows which one the data uses.
//!
//! cargo run --release -p fnv-world --example scol_rotation -- <Data dir> <esm schema dir> <nif schema dir> [--dump N]

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::NifFile;
use fnv_world::fields::{formid_of, global, num, vec3, Fields};
use fnv_world::placed::base_object;
use fnv_world::transform::{max_diff, Euler};
use std::path::Path;

const MASTERS: [&str; 10] = [
    "FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm",
    "GunRunnersArsenal.esm", "ClassicPack.esm", "MercenaryPack.esm", "TribalPack.esm", "CaravanPack.esm",
];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let schema = Schema::load_dir(Path::new(&a[2])).unwrap();
    let nif_schema = fnv_nif::Schema::load_dir(Path::new(&a[3])).unwrap();
    let dump: usize = a.iter().position(|x| x == "--dump").map_or(0, |i| a[i + 1].parse().unwrap());
    let plugins: Vec<Plugin> = MASTERS.iter().map(|m| Plugin::open(data.join(m)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let vfs = fnv_vfs::Vfs::open_default(data).unwrap();
    let mut shown = 0;
    let mut stats = Stats::default();
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"SCOL") {
            let f = Fields::new(r, pi, &schema).unwrap();
            let me = lo.to_global(pi, r.header.form_id).unwrap();
            let model = f.string(b"MODL").unwrap_or_default();
            let mut parts = Vec::new();
            let mut cur: Option<u32> = None;
            for s in &f.subs {
                match &s.sig {
                    b"ONAM" => cur = f.decode(s).and_then(|v| formid_of(&v)).and_then(|l| global(&lo, pi, l)),
                    b"DATA" => {
                        for p in f.decode(s).and_then(|v| v.as_array().cloned()).unwrap_or_default() {
                            parts.push((cur, vec3(&p["position"]).unwrap(), vec3(&p["rotation_radians"]).unwrap(), num(&p, "scale").unwrap() as f32));
                        }
                    }
                    _ => {}
                }
            }
            // Match each part to the combined mesh's node at the same position and scale.
            let path = format!("meshes\\{}", model.to_ascii_lowercase());
            if let Some(Ok(bytes)) = vfs.read(&path) {
                if let Ok(nf) = NifFile::parse(&bytes, Some(&nif_schema)) {
                    let dec = Decoded::new(&nf, &bytes, &nif_schema);
                    let nodes: Vec<_> = scene_objects(&nf, &dec).into_iter().filter(|o| o.is_node && o.parent.is_some()).collect();
                    for (_, pos, rot, sc) in &parts {
                        // Several parts can share a position; take the best node per candidate.
                        let cands: Vec<_> = nodes
                            .iter()
                            .filter(|n| {
                                n.transform.translation.iter().zip(pos).all(|(a, b)| (a - b).abs() < 0.01)
                                    && (n.transform.scale - sc).abs() < 1e-3
                            })
                            .collect();
                        if cands.is_empty() {
                            stats.unmatched += 1;
                            continue;
                        }
                        stats.matched += 1;
                        // Single-axis (z only) rotations cannot tell the orders apart.
                        let multi = rot[0].abs() > 1e-3 || rot[1].abs() > 1e-3;
                        let errs: Vec<f32> = Euler::ALL
                            .iter()
                            .map(|e| cands.iter().map(|n| max_diff(&e.matrix(*rot), &n.transform.rotation)).fold(f32::MAX, f32::min))
                            .collect();
                        if errs[1] >= 1e-3 && stats.misfit_examples < 8 {
                            stats.misfit_examples += 1;
                            println!("misfit {me:08X} rot {rot:?} err {:.4} nodes {}", errs[1], cands.len());
                        }
                        for (i, &e) in errs.iter().enumerate() {
                            if e < 1e-3 {
                                stats.fits[i] += 1;
                                if multi {
                                    stats.fits_multi[i] += 1;
                                }
                            }
                        }
                        if multi {
                            stats.multi += 1;
                        }
                    }
                }
            }
            if shown < dump {
                shown += 1;
                println!("SCOL {me:08X} {:?} model {model} parts {}", f.string(b"EDID"), parts.len());
                for (b, pos, rot, sc) in parts.iter().take(6) {
                    let bo = b.and_then(|b| base_object(&lo, &schema, b));
                    println!("  part {:?} {:?} pos {pos:?} rot {rot:?} scale {sc}", b.map(|b| format!("{b:08X}")), bo.and_then(|b| b.model_path()));
                }
                match vfs.read(&path) {
                    Some(Ok(bytes)) => {
                        let nf = NifFile::parse(&bytes, Some(&nif_schema)).unwrap();
                        let dec = Decoded::new(&nf, &bytes, &nif_schema);
                        for o in scene_objects(&nf, &dec).iter().take(12) {
                            println!("    {} {:?} parent {:?} t {:?} r {:?} s {}", o.type_name, o.name, o.parent, o.transform.translation, o.transform.rotation, o.transform.scale);
                        }
                    }
                    _ => println!("    mesh {path} not found"),
                }
            }
        }
    }
    println!(
        "parts matched to a mesh node: {} (unmatched {}), with x/y rotation: {}",
        stats.matched, stats.unmatched, stats.multi
    );
    for (i, e) in Euler::ALL.iter().enumerate() {
        println!("  {:8} fits {:5} of {}   multi-axis fits {:5} of {}", e.name(), stats.fits[i], stats.matched, stats.fits_multi[i], stats.multi);
    }
}

#[derive(Default)]
struct Stats {
    matched: usize,
    unmatched: usize,
    multi: usize,
    misfit_examples: usize,
    fits: [usize; 12],
    fits_multi: [usize; 12],
}
