//! Evidence for creatures' sounds (docs/spec-notes/audio.md §14): how many CREA records
//! list sounds (`CSDT`/`CSDI`/`CSDC`), inherit them (`CSCR`) or only have a template
//! (`TPLT`); the types listed and their chances; with `--show <edid>` (repeatable) one
//! creature's list with the SOUN editor IDs and files.
//!
//! Reads the plugins named on the command line only (pass the base game's master).
//!
//! cargo run --release -p fnv-actor --example creature_sound_survey -- <esm schema dir> <plugin.esm>... [--show EDID]...

use fnv_actor::anim_sounds::{creature_sound_type_name, creature_sounds, own_creature_sounds};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::fields::{winner, Fields};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut plugins = Vec::new();
    let mut show = Vec::new();
    let mut i = 1;
    while i < a.len() {
        if a[i] == "--near" || a[i] == "--soun" {
            i += 2;
            continue;
        }
        if a[i] == "--show" {
            show.push(a[i + 1].to_ascii_lowercase());
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
    let edid = |id: u32| fields(id).and_then(|f| f.string(b"EDID")).unwrap_or_else(|| format!("{id:08X}"));
    let mut soun_file: HashMap<u32, String> = HashMap::new();
    for id in of_type(b"SOUN") {
        if let Some(f) = fields(id) {
            let file = f.string(b"FNAM").unwrap_or_default();
            soun_file.insert(id, file);
        }
    }

    // `--soun <prefix>`: SOUN records whose editor ID starts with it, their files and flags.
    if let Some(prefix) = a.iter().position(|x| x == "--soun").and_then(|i| a.get(i + 1)).map(|p| p.to_ascii_lowercase()) {
        for id in of_type(b"SOUN") {
            let Some(f) = fields(id) else { continue };
            let e = f.string(b"EDID").unwrap_or_default();
            if e.to_ascii_lowercase().starts_with(&prefix) {
                let flags = f.get(b"SNDD").and_then(|v| v.get("flags").and_then(|f| f.get("set")).cloned()).map(|v| v.to_string()).unwrap_or_default();
                println!("SOUN {id:08X} {e}: {} {flags}", soun_file.get(&id).map(String::as_str).unwrap_or("-"));
            }
        }
    }

    // Landscape textures' Havok materials (LTEX HNAM's first byte): any stairs (64–127)?
    let mut ltex: BTreeMap<u8, u32> = BTreeMap::new();
    for id in of_type(b"LTEX") {
        if let Some(b) = fields(id).and_then(|f| f.raw(b"HNAM").and_then(|s| s.data.first().copied())) {
            *ltex.entry(b).or_default() += 1;
        }
    }
    println!("landscape textures by Havok material code: {ltex:?}");
    // Acoustic spaces' reverb environments (ASPC ANAM).
    let mut env: BTreeMap<String, u32> = BTreeMap::new();
    for id in of_type(b"ASPC") {
        let e = fields(id).and_then(|f| f.get(b"ANAM")).map(|v| v.to_string()).unwrap_or_else(|| "-".into());
        *env.entry(e).or_default() += 1;
    }
    println!("acoustic spaces by environment (ANAM): {env:?}");

    // Shell casing models (WEAP MOD2) by how many weapons name them.
    let mut casings: BTreeMap<String, u32> = BTreeMap::new();
    for id in of_type(b"WEAP") {
        if let Some(m) = fields(id).and_then(|f| f.string(b"MOD2")) {
            *casings.entry(m.to_ascii_lowercase()).or_default() += 1;
        }
    }
    println!("shell casing models (WEAP MOD2): {casings:?}");

    let crea = of_type(b"CREA");
    let (mut own, mut inherit, mut inherit_and_own, mut template_only, mut none, mut resolved) = (0, 0, 0, 0, 0, 0);
    let mut by_type: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut chances: BTreeMap<u8, u32> = BTreeMap::new();
    let mut per_type_count: BTreeMap<usize, u32> = BTreeMap::new();
    let mut by_edid: HashMap<String, u32> = HashMap::new();
    for &id in &crea {
        let Some(f) = fields(id) else { continue };
        by_edid.insert(f.string(b"EDID").unwrap_or_default().to_ascii_lowercase(), id);
        let list = own_creature_sounds(&lo, &f);
        let cscr = f.formid(b"CSCR", &lo);
        let tplt = f.formid(b"TPLT", &lo);
        match (!list.is_empty(), cscr.is_some()) {
            (true, true) => inherit_and_own += 1,
            (true, false) => own += 1,
            (false, true) => inherit += 1,
            (false, false) if tplt.is_some() => template_only += 1,
            _ => none += 1,
        }
        if !creature_sounds(&lo, &schema, id).0.is_empty() {
            resolved += 1;
        }
        let mut kinds: BTreeMap<u32, usize> = BTreeMap::new();
        for s in &list {
            *kinds.entry(s.kind).or_default() += 1;
            *chances.entry(s.chance).or_default() += 1;
        }
        for (&k, &n) in &kinds {
            let e = by_type.entry(k).or_default();
            e.0 += 1;
            e.1 += n as u32;
            *per_type_count.entry(n).or_default() += 1;
        }
    }
    println!("{} CREA: {own} list sounds, {inherit} inherit them (CSCR) only, {inherit_and_own} both, {template_only} only a template (TPLT), {none} neither", crea.len());
    println!("{resolved} have sounds after following CSCR");
    println!("types listed (creatures, sounds):");
    for (k, (c, n)) in &by_type {
        println!("  {k:>2} {:<22} {c:>4} {n:>5}", creature_sound_type_name(*k));
    }
    println!("sounds per type and creature: {per_type_count:?}");
    println!("chances: {chances:?}");

    // Templates: what a creature with neither list nor CSCR would take its sounds from.
    let mut tpl_kinds: BTreeMap<String, u32> = BTreeMap::new();
    for &id in &crea {
        let Some(f) = fields(id) else { continue };
        if !own_creature_sounds(&lo, &f).is_empty() || f.formid(b"CSCR", &lo).is_some() {
            continue;
        }
        if let Some(t) = f.formid(b"TPLT", &lo) {
            let sig = winner(&lo, t).map(|(_, r)| String::from_utf8_lossy(&r.header.sig).to_string()).unwrap_or_default();
            *tpl_kinds.entry(sig).or_default() += 1;
        }
    }
    println!("template-only creatures' templates by record type: {tpl_kinds:?}");

    // The creatures with lists of their own (the ones the others inherit from), and what
    // the rarer types name.
    let mut owners = Vec::new();
    for &id in &crea {
        let Some(f) = fields(id) else { continue };
        let list = own_creature_sounds(&lo, &f);
        if list.is_empty() {
            continue;
        }
        owners.push(edid(id));
        for s in list.iter().filter(|s| s.kind >= 20 || s.kind == 9) {
            println!("  {} type {} ({}): {} {}", edid(id), s.kind, creature_sound_type_name(s.kind), edid(s.sound), soun_file.get(&s.sound).map(String::as_str).unwrap_or("-"));
        }
    }
    println!("creatures with their own lists: {}", owners.join(", "));

    // Objects' looping sounds: ACTI SNAM, CONT RNAM, DOOR BNAM [documented: xEdit], and
    // how many references place them.
    let mut loop_bases: HashMap<u32, (&str, u32)> = HashMap::new();
    for (sig, sub, what) in [(b"ACTI", b"SNAM", "activator SNAM"), (b"CONT", b"RNAM", "container RNAM"), (b"DOOR", b"BNAM", "door BNAM")] {
        let ids = of_type(sig);
        let mut with = 0;
        let mut flags: BTreeMap<String, u32> = BTreeMap::new();
        for &id in &ids {
            if let Some(s) = fields(id).and_then(|f| f.formid(sub, &lo)) {
                with += 1;
                loop_bases.insert(id, (what, s));
                let sf = fields(s).and_then(|f| f.get(b"SNDD")).and_then(|v| v.get("flags").cloned());
                *flags.entry(sf.map(|v| v.to_string()).unwrap_or_else(|| "-".into())).or_default() += 1;
            }
        }
        println!("{what}: {with} of {} records name one; their sounds' flags: {flags:?}", ids.len());
    }

    let near = a.iter().position(|x| x == "--near").and_then(|i| a.get(i + 1)).map(|s| s.split(',').filter_map(|v| v.parse::<f32>().ok()).collect::<Vec<_>>());
    {
        let ix = fnv_world::WorldIndex::build(&lo, &schema);
        let refs = fnv_sim::refs::RefTable::from_index(&lo, &schema, &ix);
        let mut placed: BTreeMap<&str, u32> = BTreeMap::new();
        for p in refs.refs.values() {
            if let Some((what, _)) = loop_bases.get(&p.base.0) {
                *placed.entry(what).or_default() += 1;
            }
        }
        println!("references placing them: {placed:?}");
        if let Some(n) = near.filter(|n| n.len() == 3) {
            for p in refs.refs.values().filter(|p| &p.sig == b"ACRE") {
                let d = ((p.pos[0] - n[0]).powi(2) + (p.pos[1] - n[1]).powi(2)).sqrt();
                if d <= n[2] && p.cell.is_some_and(|c| refs.cell_world.contains_key(&c)) {
                    let (list, from) = creature_sounds(&lo, &schema, p.base.0);
                    println!("  {:08X} {} at {:.0} units ({:.0}, {:.0}, {:.0}), {} sounds from {}", p.id.0, edid(p.base.0), d, p.pos[0], p.pos[1], p.pos[2], list.len(), edid(from));
                }
            }
            for p in refs.refs.values() {
                let d = ((p.pos[0] - n[0]).powi(2) + (p.pos[1] - n[1]).powi(2)).sqrt();
                if let (true, Some((what, s))) = (d <= n[2], loop_bases.get(&p.base.0)) {
                    println!("  {what} {:08X} {} at {:.0} units: {}", p.id.0, edid(p.base.0), d, edid(*s));
                }
            }
        }
    }

    for s in &show {
        let Some(&id) = by_edid.get(s) else {
            println!("{s}: no CREA");
            continue;
        };
        let (list, from) = creature_sounds(&lo, &schema, id);
        println!("{} ({id:08X}), sounds from {}:", edid(id), edid(from));
        for c in list {
            println!("  {:<18} {:<36} {:>3}%  {}", creature_sound_type_name(c.kind), edid(c.sound), c.chance, soun_file.get(&c.sound).map(String::as_str).unwrap_or("-"));
        }
    }
}
