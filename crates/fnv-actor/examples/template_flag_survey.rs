//! Evidence for actor templates (docs/spec-notes/actors.md §1.3): for NPC_ and CREA, how
//! many records set each ACBS template flag with a template (`TPLT`) that is an actor,
//! one that is a leveled list (LVLN, LVLC), or no template; and, per flag, how many
//! actors have a field of their own that differs from what one of their template sources
//! gives (the cases where reading the placed or own record instead of the source shows).
//!
//! Reads the plugins named on the command line only (pass the base game's master first).
//!
//! cargo run --release -p fnv-actor --example template_flag_survey -- <esm schema dir> <plugin.esm>...

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::fields::{formid_of, global, winner, Fields};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The ten flags, by bit [documented: GECK wiki GetActorTemplateFlag, flag IDs 1 … 512].
/// A field compared with the template sources: the flag bit, its label, and how it reads.
type Check<'a> = (u16, &'a str, Box<dyn Fn(u32) -> Vec<u8> + 'a>);

const FLAGS: [&str; 10] = ["Traits", "Stats", "Factions", "Actor Effect List", "AI Data", "AI Packages", "Model/Animation", "Base Data", "Inventory", "Script"];

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let plugins: Vec<Plugin> = a[1..].iter().map(|p| Plugin::open(Path::new(p)).unwrap()).collect();
    let schema = Schema::load_dir(Path::new(&a[0])).unwrap();
    let lo = LoadOrder::new(&plugins).unwrap();
    let sig_of = |id: u32| winner(&lo, id).map(|(_, r)| r.header.sig);
    let fields = |id: u32| winner(&lo, id).and_then(|(pi, r)| Fields::new(r, pi, &schema).ok());
    let of_type = |sig: &[u8; 4]| -> Vec<u32> {
        let mut v: Vec<u32> = lo.index.keys().copied().filter(|&id| sig_of(id).as_ref() == Some(sig)).collect();
        v.sort_unstable();
        v
    };
    let acbs = |f: &Fields| f.get(b"ACBS");
    let flag_value = |v: &Value, key: &str| v.get(key).and_then(|f| f.get("value").and_then(Value::as_u64).or_else(|| f.as_u64())).unwrap_or(0);
    let template_flags = |id: u32| fields(id).and_then(|f| acbs(&f)).map_or(0, |v| flag_value(&v, "template_flags") as u16);
    let tplt = |id: u32| fields(id).and_then(|f| f.formid(b"TPLT", &lo));
    let entries = |list: u32| -> Vec<u32> {
        let Some(f) = fields(list) else { return Vec::new() };
        f.subs
            .iter()
            .filter(|s| &s.sig == b"LVLO")
            .filter_map(|s| {
                let v = f.decode(s)?;
                let r = v.get("reference").or_else(|| v.get("ref")).or_else(|| v.get("form"))?;
                global(&lo, f.plugin, formid_of(r)?)
            })
            .collect()
    };
    let is_list = |id: u32| matches!(sig_of(id).as_ref(), Some(b"LVLN") | Some(b"LVLC"));
    let is_actor = |id: u32| matches!(sig_of(id).as_ref(), Some(b"NPC_") | Some(b"CREA"));

    // Every actor the group `bit` of `id` can come from, whatever the picks.
    fn sources(id: u32, bit: u16, depth: u32, seen: &mut BTreeSet<u32>, out: &mut BTreeSet<u32>, ctx: &dyn Fn(u32) -> (bool, bool, Option<u32>, u16, Vec<u32>)) {
        if depth > 32 || !seen.insert(id) {
            return;
        }
        let (actor, list, t, flags, list_entries) = ctx(id);
        if actor {
            match t.filter(|_| flags & bit != 0) {
                Some(t) => sources(t, bit, depth + 1, seen, out, ctx),
                None => {
                    out.insert(id);
                }
            }
        } else if list {
            for e in list_entries {
                sources(e, bit, depth + 1, seen, out, ctx);
            }
        }
    }
    let ctx = |id: u32| (is_actor(id), is_list(id), tplt(id), template_flags(id), if is_list(id) { entries(id) } else { Vec::new() });
    let all_sources = |id: u32, bit: u16| {
        let mut out = BTreeSet::new();
        sources(id, bit, 0, &mut BTreeSet::new(), &mut out, &ctx);
        out
    };

    // One field of a record, raw bytes (absent: empty).
    let raw = |id: u32, sig: &[u8; 4]| fields(id).and_then(|f| f.raw(sig).map(|s| s.data.clone())).unwrap_or_default();
    let female = |id: u32| fields(id).and_then(|f| acbs(&f)).map_or(0, |v| flag_value(&v, "flags") & 1);

    for sig in [b"NPC_", b"CREA"] {
        let ids = of_type(sig);
        let mut by_kind: BTreeMap<&str, u32> = BTreeMap::new();
        // flag -> (with actor template, with list template, no template)
        let mut per_flag = [(0u32, 0u32, 0u32); 10];
        for &id in &ids {
            let t = tplt(id);
            let kind = match t {
                None => "none",
                Some(t) if is_actor(t) => "actor",
                Some(t) if is_list(t) => "list",
                Some(_) => "other",
            };
            *by_kind.entry(kind).or_default() += 1;
            let fl = template_flags(id);
            for (b, c) in per_flag.iter_mut().enumerate() {
                if fl & (1 << b) != 0 {
                    match kind {
                        "actor" => c.0 += 1,
                        "list" => c.1 += 1,
                        _ => c.2 += 1,
                    }
                }
            }
        }
        println!("== {} {} records; TPLT: {by_kind:?}", String::from_utf8_lossy(sig), ids.len());
        for (b, c) in per_flag.iter().enumerate() {
            println!("  {:<18} actor template {:>5}  list template {:>5}  no template {:>4}", FLAGS[b], c.0, c.1, c.2);
        }

        // Own field vs the sources the flag points to: (flag bit, field, label).
        let checks: Vec<Check> = vec![
            (0, "VTCK voice", Box::new(|id| raw(id, b"VTCK"))),
            (0, "ACBS Female", Box::new(|id| vec![female(id) as u8])),
            (0, "RNAM race", Box::new(|id| raw(id, b"RNAM"))),
            (0, "CNAM class/impact set", Box::new(|id| raw(id, b"CNAM"))),
            (0, "INAM death item", Box::new(|id| raw(id, b"INAM"))),
            (6, "NAM4 impact material", Box::new(|id| raw(id, b"NAM4"))),
            (6, "CNAM class/impact set", Box::new(|id| raw(id, b"CNAM"))),
            (1, "CNAM class/impact set", Box::new(|id| raw(id, b"CNAM"))),
            (6, "CSDT sound list present", Box::new(|id| vec![fields(id).is_some_and(|f| f.raw(b"CSDT").is_some()) as u8])),
            (6, "CSCR inherits sounds", Box::new(|id| raw(id, b"CSCR"))),
            (9, "SCRI script", Box::new(|id| raw(id, b"SCRI"))),
        ];
        for (bit, label, field) in &checks {
            let (mut with_flag, mut differs_actor, mut differs_list, mut list_varies) = (0, 0, 0, 0);
            let mut examples = Vec::new();
            for &id in &ids {
                let Some(t) = tplt(id) else { continue };
                if template_flags(id) & (1 << bit) == 0 {
                    continue;
                }
                with_flag += 1;
                let own = field(id);
                let src = all_sources(id, 1 << bit);
                let values: BTreeSet<Vec<u8>> = src.iter().map(|&s| field(s)).collect();
                if values.len() > 1 {
                    list_varies += 1;
                }
                if values.iter().any(|v| *v != own) {
                    if is_list(t) {
                        differs_list += 1;
                    } else {
                        differs_actor += 1;
                    }
                    if examples.len() < 3 {
                        examples.push(fields(id).and_then(|f| f.string(b"EDID")).unwrap_or_else(|| format!("{id:08X}")));
                    }
                }
            }
            println!(
                "  [{}] {label}: {with_flag} with the flag and a TPLT; own value differs from a source: {differs_actor} (actor template), {differs_list} (list template); sources disagree among picks: {list_varies}; e.g. {examples:?}",
                FLAGS[*bit as usize]
            );
        }
    }

    // Placed actors whose base is a leveled list.
    for place in [b"ACHR", b"ACRE"] {
        let mut counts: BTreeMap<String, u32> = BTreeMap::new();
        for id in of_type(place) {
            let base = fields(id).and_then(|f| f.formid(b"NAME", &lo));
            let s = base.and_then(sig_of).map_or("-".to_string(), |s| String::from_utf8_lossy(&s).into_owned());
            *counts.entry(s).or_default() += 1;
        }
        println!("placed {} by base type: {counts:?}", String::from_utf8_lossy(place));
    }
}
