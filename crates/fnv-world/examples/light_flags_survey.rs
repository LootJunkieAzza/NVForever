//! LIGH flicker and pulse flags (docs/spec-notes/effects-animation-audit.md §4): over the
//! base game's master only (FalloutNV.esm; the add-ons are not read), how many LIGH
//! records set each DATA flag, with examples (EDID, radius, colour, fade), and how many
//! placed references (interior and exterior) use a light with each of the four motion
//! flags (Flicker, Flicker Slow, Pulse, Pulse Slow).
//!
//! cargo run --release -p fnv-world --example light_flags_survey -- <Data dir> <esm schema dir>

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::fields::{winner, Fields};
use fnv_world::index::WorldIndex;
use fnv_world::lighting::light_data;
use fnv_world::placed::placement;
use std::collections::BTreeMap;
use std::path::Path;

const NAMES: [&str; 11] =
    ["Dynamic", "Can be Carried", "Negative", "Flicker", "Unused", "Off By Default", "Flicker Slow", "Pulse", "Pulse Slow", "Spot Light", "Spot Shadow"];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(Path::new(&a[2])).unwrap();
    let plugins = vec![Plugin::open(Path::new(&a[1]).join("FalloutNV.esm")).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let index = WorldIndex::build(&lo, &schema);
    let mut by_bit: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut ligh: Vec<u32> = Vec::new();
    for (&fid, recs) in &lo.index {
        let Some(d) = recs.last() else { continue };
        if &lo.records[d.plugin as usize][d.ordinal as usize].header.sig != b"LIGH" {
            continue;
        }
        ligh.push(fid);
        let Some(l) = light_data(&lo, &schema, fid) else { continue };
        let e = winner(&lo, fid).and_then(|(pi, r)| Fields::new(r, pi, &schema).ok()?.string(b"EDID")).unwrap_or_default();
        for (bit, list) in (0..NAMES.len()).map(|b| (b, l.flags & (1 << b) != 0)).filter(|x| x.1) {
            let _ = list;
            by_bit.entry(bit).or_default().push(format!("{e} r{} {:?} fade {:.2} flags {:#x}", l.radius, l.color, l.fade, l.flags));
        }
    }
    println!("LIGH records: {}", ligh.len());
    for (bit, list) in &by_bit {
        println!("\nbit {bit} {}: {} records", NAMES[*bit], list.len());
        for x in list.iter().take(12) {
            println!("  {x}");
        }
    }
    // Placed references of lights with a motion flag.
    let motion = [3usize, 6, 7, 8];
    let mut placed: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut by_cell: Vec<(usize, String)> = Vec::new();
    for c in index.cells.values() {
        let mut here = 0;
        for &rf in &c.refs {
            let Some(p) = placement(&lo, &schema, rf) else { continue };
            let Some(l) = p.base.and_then(|b| light_data(&lo, &schema, b)) else { continue };
            here += usize::from(motion.iter().any(|b| l.flags & (1 << *b) != 0));
            for b in motion.iter().filter(|b| l.flags & (1 << **b) != 0) {
                let e = placed.entry(*b).or_default();
                if c.world.is_none() {
                    e.0 += 1;
                } else {
                    e.1 += 1;
                }
            }
        }
        if c.world.is_none() && here > 0 {
            by_cell.push((here, c.edid.clone().unwrap_or_default()));
        }
    }
    by_cell.sort_by_key(|c| std::cmp::Reverse(c.0));
    println!("\ninterior cells with the most flickering or pulsing lights:");
    for (n, e) in by_cell.iter().take(15) {
        println!("  {n:4} {e}");
    }
    println!("\nplaced references (interior, exterior):");
    for (b, (i, x)) in &placed {
        println!("  {}: {i} interior, {x} exterior", NAMES[*b]);
    }
}
