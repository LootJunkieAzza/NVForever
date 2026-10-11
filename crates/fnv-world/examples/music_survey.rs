//! Evidence for the music the place does not choose (docs/spec-notes/audio.md §13):
//! battle, dungeon and incidental media sets with their fields, the controllers with
//! their delays, faction lists, battle lists and the raw bytes of the fields xEdit does
//! not name, how many audio markers (and in interiors) use each kind, the music types,
//! and the game settings whose names speak of music or battle.
//!
//! Reads the plugins named on the command line only (pass the base game's master).
//!
//! cargo run --release -p fnv-world --example music_survey -- <esm schema dir> <plugin.esm>...

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::audio::{self, AudioForms, MediaSetKind};
use fnv_world::fields::{winner, Fields};
use fnv_world::WorldIndex;
use std::collections::BTreeMap;
use std::path::Path;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let plugins: Vec<Plugin> = a[1..].iter().map(|p| Plugin::open(Path::new(p)).unwrap()).collect();
    let schema = Schema::load_dir(Path::new(&a[0])).unwrap();
    let lo = LoadOrder::new(&plugins).unwrap();
    let fields = |id: u32| winner(&lo, id).and_then(|(pi, r)| Fields::new(r, pi, &schema).ok());
    let edid = |id: u32| fields(id).and_then(|f| f.string(b"EDID")).unwrap_or_else(|| format!("{id:08X}"));
    let forms = AudioForms::build(&lo, &schema);

    println!("== music types ({})", forms.music.len());
    for m in forms.music.values() {
        println!("{:08X} {:<28} ANAM {:>7.2} {}", m.form_id, m.edid.as_deref().unwrap_or("?"), m.db, m.file.as_deref().unwrap_or("-"));
    }

    for kind in [MediaSetKind::Battle, MediaSetKind::Dungeon, MediaSetKind::Incidental, MediaSetKind::None] {
        let sets: Vec<_> = forms.sets.values().filter(|s| s.kind == kind).collect();
        println!("== {kind:?} sets ({})", sets.len());
        for s in sets {
            let tracks: Vec<String> = (0..6).filter_map(|k| s.tracks[k].as_ref().map(|t| format!("[{k}{}] {t} {:.1} dB", if s.enabled & (1 << k) != 0 { "" } else { " off" }, s.db[k]))).collect();
            println!(
                "{:08X} {:<36} times {:?} intro {} outro {} enabled {:06b}\n    {}",
                s.form_id,
                s.edid.as_deref().unwrap_or("?"),
                s.times,
                s.intro.map_or("-".into(), edid),
                s.outro.map_or("-".into(), edid),
                s.enabled,
                tracks.join("\n    ")
            );
        }
    }
    let loc_times: BTreeMap<String, usize> = forms.sets.values().filter(|s| s.kind == MediaSetKind::Location).fold(BTreeMap::new(), |mut m, s| {
        *m.entry(format!("{:?}", s.times)).or_default() += 1;
        m
    });
    println!("== location sets' times (DNAM ENAM FNAM GNAM): {loc_times:?}");

    // Markers by controller.
    let ix = WorldIndex::build(&lo, &schema);
    let (am, _) = audio::markers(&lo, &schema, &ix, ix.cells.keys().copied());
    let mut by_ctrl: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for m in &am {
        let e = by_ctrl.entry(m.controller.unwrap_or(0)).or_default();
        e.0 += 1;
        if ix.cells.get(&m.cell).is_some_and(|c| c.world.is_none()) {
            e.1 += 1;
        }
    }
    let kind_of = |id: &u32| forms.sets.get(id).map_or("missing".to_string(), |s| format!("{:?}", s.kind));
    println!("== controllers ({})", forms.controllers.len());
    let mut delays: BTreeMap<String, usize> = BTreeMap::new();
    let mut nam1: BTreeMap<String, usize> = BTreeMap::new();
    let mut list_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let (mut with_battle, mut markers_battle, mut interior_battle) = (0, 0, 0);
    for c in forms.controllers.values() {
        let f = fields(c.form_id).unwrap();
        let raw = |s: &[u8; 4]| f.raw(s).map_or("-".to_string(), |r| hex(&r.data));
        *delays.entry(format!("location {} retrigger {}", c.location_delay, c.retrigger_delay)).or_default() += 1;
        *nam1.entry(raw(b"NAM1")).or_default() += 1;
        for (name, list) in [("neutral", &c.neutral), ("ally", &c.ally), ("friend", &c.friend), ("enemy", &c.enemy), ("location", &c.location), ("battle", &c.battle)] {
            for id in list {
                *list_kinds.entry(format!("{name}: {}", kind_of(id))).or_default() += 1;
            }
        }
        let (n, ni) = by_ctrl.get(&c.form_id).copied().unwrap_or_default();
        if !c.battle.is_empty() {
            with_battle += 1;
            markers_battle += n;
            interior_battle += ni;
        }
        let names = |v: &[u32]| v.iter().map(|&i| edid(i)).collect::<Vec<_>>().join(",");
        println!(
            "{:08X} {:<34} markers {n} ({ni} interior) day {:.2} night {:.2} delays {} {} NAM1 {} NAM2 {} NAM3 {} FNAM {} faction {}\n    loc [{}] neutral [{}] ally [{}] friend [{}] enemy [{}] battle [{}]",
            c.form_id,
            c.edid.as_deref().unwrap_or("?"),
            c.day_start_hour,
            c.night_start_hour,
            c.location_delay,
            c.retrigger_delay,
            raw(b"NAM1"),
            raw(b"NAM2"),
            raw(b"NAM3"),
            raw(b"FNAM"),
            c.faction.map_or("-".into(), edid),
            names(&c.location),
            names(&c.neutral),
            names(&c.ally),
            names(&c.friend),
            names(&c.enemy),
            names(&c.battle),
        );
    }
    println!("== delays: {delays:?}");
    println!("== NAM1 values: {nam1:?}");
    println!("== kinds of set in each list: {list_kinds:?}");
    println!("== controllers with a battle list: {with_battle}, their markers {markers_battle} ({interior_battle} interior); markers {} ({} interior)", am.len(), by_ctrl.values().map(|v| v.1).sum::<usize>());

    // Regions' music, and the exterior cells in them.
    let mut cells_in: BTreeMap<u32, usize> = BTreeMap::new();
    for (&cell, c) in &ix.cells {
        if c.world.is_some() {
            for r in audio::cell_regions(&lo, &schema, cell) {
                *cells_in.entry(r).or_default() += 1;
            }
        }
    }
    println!("== regions naming music ({})", forms.regions.len());
    let mut combos: BTreeMap<String, usize> = BTreeMap::new();
    for r in forms.regions.values() {
        let names = |v: &[u32]| v.iter().map(|&i| edid(i)).collect::<Vec<_>>().join(",");
        *combos.entry(format!("music {} incidental {} battle {}", r.music.is_some(), r.incidental.map_or("-".into(), edid), r.battle.len())).or_default() += 1;
        println!(
            "{:08X} {:<36} cells {:>5} music {} incidental {} battle [{}]",
            r.form_id,
            r.edid.as_deref().unwrap_or("?"),
            cells_in.get(&r.form_id).copied().unwrap_or(0),
            r.music.map_or("-".into(), edid),
            r.incidental.map_or("-".into(), edid),
            names(&r.battle)
        );
    }
    println!("== region combinations: {combos:#?}");
    // Acoustic spaces pointing at a region (ASPC RDAT), and the cells naming them (XCAS).
    let mut cells_of_space: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for (&cell, c) in &ix.cells {
        if let Some(s) = audio::cell_audio(&lo, &schema, cell).acoustic_space {
            let e = cells_of_space.entry(s).or_default();
            if c.world.is_some() {
                e.0 += 1;
            } else {
                e.1 += 1;
            }
        }
    }
    let mut via_space: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    for s in forms.spaces.values() {
        let Some(r) = s.region else { continue };
        let (ext, int) = cells_of_space.get(&s.form_id).copied().unwrap_or_default();
        let e = via_space.entry(format!("{} (music: {})", edid(r), forms.regions.contains_key(&r))).or_default();
        e.0 += 1;
        e.1 += ext;
        e.2 += int;
    }
    println!("== regions named by acoustic spaces: region -> (spaces, exterior cells, interior cells)");
    for (k, v) in &via_space {
        println!("{k}: {v:?}");
    }
    let gs = ix.cell_by_name("Goodsprings").map(|c| c.form_id);
    if let Some(gs) = gs {
        println!("== Goodsprings cell {gs:08X} regions: {:?}", audio::cell_regions(&lo, &schema, gs).iter().map(|&r| edid(r)).collect::<Vec<_>>());
    }

    println!("== game settings");
    for (&id, defs) in &lo.index {
        let Some(d) = defs.last() else { continue };
        let r = lo.records[d.plugin as usize][d.ordinal as usize];
        if &r.header.sig != b"GMST" {
            continue;
        }
        let Some(f) = fields(id) else { continue };
        let Some(e) = f.string(b"EDID") else { continue };
        let l = e.to_ascii_lowercase();
        if l.contains("music") || l.contains("battle") || l.contains("combatsound") || l.contains("incidental") || l.contains("mediaset") || l.contains("stinger") {
            println!("{e} = {}", f.get(b"DATA").map_or("?".into(), |v| v.to_string()));
        }
    }
}
