//! Which base record types are placed in the world, and how many of those references name
//! a base with a model: the evidence for the renderer's list of drawn types
//! (engine/nvforever/src/world.rs `DRAWN`).
//!
//! cargo run --release -p fnv-world --example placed_types -- <esm schema dir> <plugin.esm>...
//!     per base type: placed references (winning REFR), how many of them have a base with
//!     MODL, and how many base records of the type exist and carry MODL.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::fields::{winner, Fields};
use std::collections::BTreeMap;
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let schema = Schema::load_dir(Path::new(&a[0])).unwrap();
    let plugins: Vec<Plugin> = a[1..].iter().map(|p| Plugin::open(Path::new(p)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();

    let has_model = |id: u32| -> Option<([u8; 4], bool)> {
        let (pi, r) = winner(&lo, id)?;
        let f = Fields::new(r, pi, &schema).ok()?;
        let m = [b"MODL", b"MOD2", b"MOD4"].iter().any(|s| f.string(s).is_some_and(|s| !s.trim().is_empty()));
        Some((r.header.sig, m))
    };

    // sig -> (placed refs, placed refs with a model, base records, base records with a model)
    let mut t: BTreeMap<[u8; 4], [u32; 4]> = BTreeMap::new();
    let mut ids: Vec<u32> = lo.index.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let Some((pi, r)) = winner(&lo, id) else { continue };
        if &r.header.sig == b"REFR" {
            let Ok(f) = Fields::new(r, pi, &schema) else { continue };
            let Some((sig, m)) = f.formid(b"NAME", &lo).and_then(has_model) else { continue };
            let e = t.entry(sig).or_default();
            e[0] += 1;
            e[1] += m as u32;
        } else if let Some((sig, m)) = has_model(id) {
            let e = t.entry(sig).or_default();
            e[2] += 1;
            e[3] += m as u32;
        }
    }
    println!("{:<5} {:>8} {:>10} {:>8} {:>10}", "type", "placed", "w/ model", "bases", "w/ model");
    for (sig, [p, pm, b, bm]) in t {
        if p + bm > 0 {
            println!("{:<5} {p:>8} {pm:>10} {b:>8} {bm:>10}", String::from_utf8_lossy(&sig));
        }
    }
}
