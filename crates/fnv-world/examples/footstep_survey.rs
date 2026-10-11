//! Evidence for the sounds animations name (docs/spec-notes/audio.md §10): the Havok
//! material each landscape texture (LTEX HNAM) gives, the armour's animation sounds (ARMO
//! SNAM by type), and the footstep sound records (SOUN `FST…`) by material and gait.
//! With `--keys <file>` (one SOUN editor ID per line, e.g. from the `kf_sound_keys`
//! example), how many of those name a SOUN record.
//!
//! Reads the plugins named on the command line only (pass the base game's master).
//!
//! cargo run --release -p fnv-world --example footstep_survey -- <esm schema dir> <plugin.esm>... [--keys file]

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::fields::{winner, Fields};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut plugins = Vec::new();
    let mut keys = None;
    let mut i = 1;
    while i < a.len() {
        if a[i] == "--keys" {
            keys = Some(a[i + 1].clone());
            i += 2;
            continue;
        }
        plugins.push(Plugin::open(Path::new(&a[i])).unwrap());
        i += 1;
    }
    let schema = Schema::load_dir(Path::new(&a[0])).unwrap();
    let lo = LoadOrder::new(&plugins).unwrap();
    let of_type = |sig: &[u8; 4]| -> Vec<u32> {
        let mut v: Vec<u32> = lo
            .index
            .iter()
            .filter(|(_, d)| d.last().is_some_and(|d| &lo.records[d.plugin as usize][d.ordinal as usize].header.sig == sig))
            .map(|(&id, _)| id)
            .collect();
        v.sort_unstable();
        v
    };
    let fields = |id: u32| winner(&lo, id).and_then(|(pi, r)| Fields::new(r, pi, &schema).ok());

    // SOUN editor IDs.
    let mut soun: HashMap<u32, String> = HashMap::new();
    let mut by_edid: HashMap<String, u32> = HashMap::new();
    for id in of_type(b"SOUN") {
        if let Some(e) = fields(id).and_then(|f| f.string(b"EDID")) {
            by_edid.insert(e.to_ascii_lowercase(), id);
            soun.insert(id, e);
        }
    }
    println!("{} SOUN records", soun.len());

    // Landscape textures' materials.
    let mut materials: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let ltex = of_type(b"LTEX");
    for &id in &ltex {
        let Some(f) = fields(id) else { continue };
        let m = f.get(b"HNAM").and_then(|v| v.get("material_type").cloned()).map_or("(none)".to_string(), |v| match v {
            Value::String(s) => s,
            v => v.to_string(),
        });
        materials.entry(m).or_default().push(f.string(b"EDID").unwrap_or_default());
    }
    println!("\n{} landscape textures by HNAM material type:", ltex.len());
    for (m, v) in &materials {
        println!("  {:>4} {m:<16} e.g. {}", v.len(), v.iter().take(4).cloned().collect::<Vec<_>>().join(", "));
    }

    // Armour animation sounds.
    let armo = of_type(b"ARMO");
    let (mut with, mut overrides, mut templates) = (0, 0, 0);
    let mut by_type: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
    let mut template_use: BTreeMap<String, u32> = BTreeMap::new();
    for &id in &armo {
        let Some(f) = fields(id) else { continue };
        let snams: Vec<Value> = f.subs.iter().filter(|s| &s.sig == b"SNAM").filter_map(|s| f.decode(s)).collect();
        with += usize::from(!snams.is_empty());
        overrides += usize::from(f.get(b"BNAM").is_some_and(|v| v.as_u64().unwrap_or(0) != 0 || v.as_bool().unwrap_or(false)));
        templates += usize::from(f.raw(b"TNAM").is_some());
        if let Some(t) = f.formid(b"TNAM", &lo) {
            let name = fields(t).and_then(|f| f.string(b"EDID")).unwrap_or_default();
            *template_use.entry(name).or_default() += 1;
        }
        if !snams.is_empty() {
            println!("  ARMO {id:08X} {} has {} SNAM", f.string(b"EDID").unwrap_or_default(), snams.len());
        }
        for s in snams {
            let t = s.get("type").map_or("?".to_string(), |v| v.as_str().map_or(v.to_string(), str::to_string));
            let sound = s.get("sound").and_then(fnv_world::fields::formid_of).and_then(|l| fnv_world::fields::global(&lo, f.plugin, l));
            let name = sound.and_then(|s| soun.get(&s).cloned()).unwrap_or_else(|| format!("{sound:08X?}"));
            let chance = s.get("chance").and_then(Value::as_u64).unwrap_or(0);
            *by_type.entry(t).or_default().entry(format!("{name} ({chance}%)")).or_default() += 1;
        }
    }
    println!("\n{} ARMO records: {with} with animation sounds (SNAM), {overrides} with BNAM set, {templates} with a template (TNAM)", armo.len());
    for (t, v) in &by_type {
        println!("  {t}: {}", v.iter().map(|(k, n)| format!("{k} ×{n}")).collect::<Vec<_>>().join(", "));
    }
    println!("  templates named: {}", template_use.iter().map(|(k, n)| format!("{k} ×{n}")).collect::<Vec<_>>().join(", "));

    // Footstep records by family.
    let mut fam: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in soun.values().filter(|e| e.starts_with("FST")) {
        let rest = &e[3..];
        let gait = ["Walk", "Run", "Sneak", "Stairs", "Land", "Jump", "Swim"].iter().find(|g| rest.contains(*g)).copied().unwrap_or("-");
        let material = rest.split(gait).next().unwrap_or(rest);
        fam.entry(material.to_string()).or_default().insert(rest[material.len()..].to_string());
    }
    println!("\nfootstep SOUN records (FST<material><kind>):");
    for (m, kinds) in &fam {
        println!("  {m:<14} {}", kinds.iter().cloned().collect::<Vec<_>>().join(" "));
    }

    if let Some(path) = keys {
        let text = std::fs::read_to_string(path).unwrap();
        let names: BTreeSet<String> = text.lines().map(|l| l.split_whitespace().next().unwrap_or("").to_string()).filter(|l| !l.is_empty()).collect();
        let missing: Vec<&String> = names.iter().filter(|n| !by_edid.contains_key(&n.to_ascii_lowercase())).collect();
        println!("\n{} distinct sound keys: {} name a SOUN record, {} do not: {}", names.len(), names.len() - missing.len(), missing.len(), missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
    }
}
