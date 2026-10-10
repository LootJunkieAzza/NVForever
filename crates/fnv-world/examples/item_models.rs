//! Evidence for "which model an item shows lying in the world"
//! (docs/spec-notes/simulation.md §3 "Item models"): the model paths of every item record
//! type, and for ARMO and WEAP what their extra model subrecords name.
//!
//! Reads the plugins named on the command line only (pass the base game's master, not a
//! whole install).
//!
//! cargo run --release -p fnv-world --example item_models -- <esm schema dir> <plugin.esm>... [--list SIG]
//!     per record type: how many winning records carry each model subrecord; for ARMO and
//!     WEAP, how the paths end and how they relate to MODL.
//! ... --list ARMO | WEAP
//!     every record of that type with its model subrecords.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::fields::{winner, Fields};
use std::collections::BTreeMap;
use std::path::Path;

/// Record types an inventory can hold [documented: xEdit wbDefinitionsFNV.pas, the
/// "Item" choices of CNTO], and ARMA (the add-on shapes ARMO names).
const ITEMS: [&[u8; 4]; 13] = [b"ARMO", b"WEAP", b"AMMO", b"MISC", b"ALCH", b"BOOK", b"KEYM", b"NOTE", b"IMOD", b"CHIP", b"CMNY", b"CCRD", b"ARMA"];
const MODELS: [&[u8; 4]; 11] = [b"MODL", b"MOD2", b"MOD3", b"MOD4", b"MWD1", b"MWD2", b"MWD3", b"MWD4", b"MWD5", b"MWD6", b"MWD7"];

fn name(sig: &[u8; 4]) -> &str {
    std::str::from_utf8(sig).unwrap()
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut plugins = Vec::new();
    let mut list = None;
    let mut i = 1;
    while i < a.len() {
        match a[i].as_str() {
            "--list" => {
                list = Some(a[i + 1].clone());
                i += 1;
            }
            p => plugins.push(Plugin::open(Path::new(p)).unwrap()),
        }
        i += 1;
    }
    let schema = Schema::load_dir(Path::new(&a[0])).unwrap();
    let lo = LoadOrder::new(&plugins).unwrap();

    // Every record of a type, by winning definition, with its model subrecords.
    let of_type = |sig: &[u8; 4]| -> Vec<(u32, String, BTreeMap<&str, String>)> {
        let mut v: Vec<u32> = lo
            .index
            .iter()
            .filter(|(_, d)| d.last().is_some_and(|d| &lo.records[d.plugin as usize][d.ordinal as usize].header.sig == sig))
            .map(|(&id, _)| id)
            .collect();
        v.sort_unstable();
        v.into_iter()
            .filter_map(|id| {
                let (pi, r) = winner(&lo, id)?;
                let f = Fields::new(r, pi, &schema).ok()?;
                let models = MODELS.iter().filter_map(|m| Some((name(m), f.string(m).filter(|s| !s.trim().is_empty())?))).collect();
                Some((id, f.string(b"EDID").unwrap_or_default(), models))
            })
            .collect()
    };

    if let Some(sig) = list {
        let sig: [u8; 4] = sig.as_bytes().try_into().expect("--list takes a four-letter record type");
        for (id, edid, models) in of_type(&sig) {
            println!("{id:08X} {edid}");
            for (m, p) in models {
                println!("    {m} {p}");
            }
        }
        return;
    }

    println!("winning records carrying each model subrecord:");
    for sig in ITEMS {
        let recs = of_type(sig);
        let counts: Vec<String> = MODELS
            .iter()
            .filter_map(|m| {
                let n = recs.iter().filter(|r| r.2.contains_key(name(m))).count();
                (n > 0).then(|| format!("{} {n}", name(m)))
            })
            .collect();
        println!("  {} {:>4}: {}", name(sig), recs.len(), counts.join(", "));
    }

    let lower = |s: &String| s.to_ascii_lowercase();
    let ends = |s: &String, e: &str| lower(s).ends_with(e);
    for sig in [b"ARMO", b"WEAP"] {
        let recs = of_type(sig);
        println!("\n{}:", name(sig));
        for m in ["MODL", "MOD2", "MOD3", "MOD4"] {
            let with: Vec<&String> = recs.iter().filter_map(|r| r.2.get(m)).collect();
            if with.is_empty() {
                continue;
            }
            let go = with.iter().filter(|p| ends(p, "go.nif")).count();
            let casing = with.iter().filter(|p| lower(p).contains("casing") || lower(p).contains("shell")).count();
            let hud = with.iter().filter(|p| lower(p).starts_with("interface\\")).count();
            let same = recs.iter().filter(|r| m != "MODL" && r.2.get(m).is_some_and(|p| r.2.get("MODL").is_some_and(|l| lower(l) == lower(p)))).count();
            println!("  {m}: {} paths; {go} end in go.nif, {casing} name a casing or shell, {hud} are under interface\\, {same} equal MODL", with.len());
        }
        let no_modl = recs.iter().filter(|r| !r.2.contains_key("MODL")).count();
        let only_mod2 = recs.iter().filter(|r| !r.2.contains_key("MODL") && r.2.contains_key("MOD2")).count();
        let mod4_differs = recs.iter().filter(|r| r.2.get("MOD4").is_some_and(|p| r.2.get("MOD2").is_none_or(|q| lower(q) != lower(p)))).count();
        println!("  {no_modl} without MODL ({only_mod2} of them with MOD2); {mod4_differs} with a MOD4 that is not their MOD2");
    }
}
