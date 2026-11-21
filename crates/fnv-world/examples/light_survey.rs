//! Interior lighting evidence (docs/spec-notes/world.md §8): the lights of a cell and
//! the cell's lighting, and a survey of placed lights' XRDS against their base radius.
//!
//! cargo run --release -p fnv-world --example light_survey -- <Data dir> <esm schema dir> cell <cell EDID>
//!     the cell's XCLL, its template (LTMP) and inherit flags (LNAM), the merged lighting,
//!     and every LIGH reference: base, radius, XRDS, colour, fade, flags, position.
//! ... --xrds
//!     interior LIGH references: how many carry XRDS, its range, and per cell the lights
//!     whose XRDS changes the base radius most (candidates for a capture).
//! ... --xemi
//!     what the XEMI of interior and exterior LIGH references names (record type, most
//!     common targets).
//! ... --templates
//!     how interior cells use lighting templates (LTMP present, LNAM bits set).

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::fields::{winner, Fields};
use fnv_world::index::WorldIndex;
use fnv_world::lighting::{cell_lighting, light_data, ref_xrds};
use fnv_world::placed::placement;
use std::path::Path;

const MASTERS: [&str; 10] = [
    "FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm",
    "GunRunnersArsenal.esm", "ClassicPack.esm", "MercenaryPack.esm", "TribalPack.esm", "CaravanPack.esm",
];

fn edid(lo: &LoadOrder, schema: &Schema, fid: u32) -> String {
    winner(lo, fid)
        .and_then(|(pi, r)| Fields::new(r, pi, schema).ok()?.string(b"EDID"))
        .unwrap_or_else(|| format!("{fid:#010x}"))
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let schema = Schema::load_dir(Path::new(&a[2])).unwrap();
    let plugins: Vec<Plugin> = MASTERS.iter().map(|m| Plugin::open(data.join(m)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let index = WorldIndex::build(&lo, &schema);
    match a.get(3).map(String::as_str) {
        Some("cell") => cell(&lo, &schema, &index, &a[4]),
        Some("--xrds") => xrds(&lo, &schema, &index),
        Some("--templates") => templates(&lo, &schema, &index),
        Some("--xemi") => xemi(&lo, &schema, &index),
        _ => eprintln!("usage: light_survey <Data> <schema> cell <EDID> | --xrds | --templates"),
    }
}

fn cell(lo: &LoadOrder, schema: &Schema, index: &WorldIndex, name: &str) {
    let c = index.cell_by_name(name).expect("no such cell");
    let (pi, r) = winner(lo, c.form_id).unwrap();
    let f = Fields::new(r, pi, schema).unwrap();
    println!("cell {:?} {:#010x} plugin {}", c.edid, c.form_id, MASTERS[c.plugin]);
    println!("  XCLL {}", f.get(b"XCLL").map_or("-".into(), |v| v.to_string()));
    if let Some(t) = f.formid(b"LTMP", lo) {
        let (tpi, tr) = winner(lo, t).unwrap();
        let tf = Fields::new(tr, tpi, schema).unwrap();
        println!("  LTMP {} {}", edid(lo, schema, t), tf.get(b"DATA").map_or("-".into(), |v| v.to_string()));
    }
    println!("  LNAM {}", f.get(b"LNAM").map_or("-".into(), |v| v.to_string()));
    println!("  XCIM {}", f.formid(b"XCIM", lo).map_or("-".into(), |i| edid(lo, schema, i)));
    println!("  merged {:?}", cell_lighting(lo, schema, c.form_id));
    println!("  lights: ref base radius xrds colour fade flags falloff fov pos xemi disabled");
    for &rf in &c.refs {
        let Some(p) = placement(lo, schema, rf) else { continue };
        let Some(b) = p.base else { continue };
        let Some(l) = light_data(lo, schema, b) else { continue };
        println!(
            "  {rf:#010x} {:<28} {:>5} {:>8} {:?} {:.2} {:#06x} {:.2} {:.0} ({:.0}, {:.0}, {:.0}) {} {}",
            edid(lo, schema, b),
            l.radius,
            ref_xrds(lo, schema, rf).map_or("-".into(), |x| format!("{x:.1}")),
            l.color,
            l.fade,
            l.flags,
            l.falloff_exponent,
            l.fov,
            p.position[0],
            p.position[1],
            p.position[2],
            p.emittance.map_or("-".into(), |e| edid(lo, schema, e)),
            if p.initially_disabled() || p.enable_parent.is_some() { "disabled/parented" } else { "" }
        );
    }
}

fn xrds(lo: &LoadOrder, schema: &Schema, index: &WorldIndex) {
    let (mut n, mut with) = (0, 0);
    let mut ratios = Vec::new();
    let mut by_cell: Vec<(f32, String, String, f32, f32, u32)> = Vec::new();
    for c in index.cells.values().filter(|c| c.world.is_none()) {
        for &rf in &c.refs {
            let Some(p) = placement(lo, schema, rf) else { continue };
            let Some(l) = p.base.and_then(|b| light_data(lo, schema, b)) else { continue };
            n += 1;
            if let Some(x) = ref_xrds(lo, schema, rf) {
                with += 1;
                ratios.push(x / l.radius.max(1.0));
                let e = c.edid.clone().unwrap_or_default();
                by_cell.push(((l.radius + x) / l.radius.max(1.0), e, edid(lo, schema, p.base.unwrap()), l.radius, x, rf));
            }
        }
    }
    ratios.sort_by(f32::total_cmp);
    println!("interior LIGH refs {n}, with XRDS {with}");
    if !ratios.is_empty() {
        println!("XRDS / base: min {:.3} median {:.3} max {:.3}", ratios[0], ratios[ratios.len() / 2], ratios[ratios.len() - 1]);
    }
    by_cell.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("largest (base + XRDS) / base:");
    for (k, cell, base, r, x, rf) in by_cell.iter().take(40) {
        println!("  {k:6.2} {cell:<32} {base:<28} base {r:5} xrds {x:7.1} ref {rf:#010x}");
    }
    println!("smallest:");
    for (k, cell, base, r, x, rf) in by_cell.iter().rev().take(20) {
        println!("  {k:6.3} {cell:<32} {base:<28} base {r:5} xrds {x:7.1} ref {rf:#010x}");
    }
}

fn templates(lo: &LoadOrder, schema: &Schema, index: &WorldIndex) {
    let (mut cells, mut with_ltmp, mut with_xcll) = (0, 0, 0);
    let mut bits = [0u32; 32];
    for c in index.cells.values().filter(|c| c.world.is_none()) {
        let Some((pi, r)) = winner(lo, c.form_id) else { continue };
        let Ok(f) = Fields::new(r, pi, schema) else { continue };
        cells += 1;
        with_xcll += f.raw(b"XCLL").is_some() as u32;
        if f.formid(b"LTMP", lo).is_some_and(|t| t != 0) {
            with_ltmp += 1;
        }
        let l = f.get(b"LNAM").and_then(|v| v.get("value").and_then(|x| x.as_f64())).unwrap_or(0.0) as u32;
        for (i, b) in bits.iter_mut().enumerate() {
            *b += (l >> i) & 1;
        }
    }
    println!("interior cells {cells}: XCLL {with_xcll}, LTMP {with_ltmp}");
    println!("LNAM bit counts: {:?}", &bits[..12]);
}

fn xemi(lo: &LoadOrder, schema: &Schema, index: &WorldIndex) {
    use std::collections::BTreeMap;
    for interior in [true, false] {
        let (mut n, mut with) = (0, 0);
        let mut sigs: BTreeMap<String, u32> = BTreeMap::new();
        let mut targets: BTreeMap<String, u32> = BTreeMap::new();
        for c in index.cells.values().filter(|c| c.world.is_none() == interior) {
            for &rf in &c.refs {
                let Some(p) = placement(lo, schema, rf) else { continue };
                if p.base.and_then(|b| light_data(lo, schema, b)).is_none() {
                    continue;
                }
                n += 1;
                let Some(e) = p.emittance else { continue };
                with += 1;
                let sig = winner(lo, e).map_or("?".into(), |(_, r)| String::from_utf8_lossy(&r.header.sig).into_owned());
                *sigs.entry(sig.clone()).or_default() += 1;
                let desc = match light_data(lo, schema, e) {
                    Some(l) => format!("{sig} {} colour {:?} fade {:.2} radius {}", edid(lo, schema, e), l.color, l.fade, l.radius),
                    None => format!("{sig} {}", edid(lo, schema, e)),
                };
                *targets.entry(desc).or_default() += 1;
            }
        }
        println!("{} LIGH refs {n}, with XEMI {with}: {sigs:?}", if interior { "interior" } else { "exterior" });
        let mut t: Vec<_> = targets.into_iter().collect();
        t.sort_by_key(|e| std::cmp::Reverse(e.1));
        for (d, k) in t.iter().take(25) {
            println!("  {k:5} {d}");
        }
    }
}
