//! `fnvtool actor karma`: karma and crime as the load order's records set them up
//! (docs/spec-notes/crime-karma.md): the settings and globals the rules read, the
//! alignment of actors, faction flags, owned cells and references; for named actors the
//! karma a kill gives, for named cells what is owned in them (fnv.actor.karma/1).

use fnv_esm::{LoadOrder, Schema};
use fnv_game::crime::{faction_flags as ff, Alignment, Settings, NEUTRAL_MURDER_SHARE};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::vm::ArgValue;
use fnv_sim::forms::BaseData;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

const EXAMPLES: usize = 12;
const GET_IS_ALIGNMENT: u16 = 0x1DA;

const SETTINGS: &[&str] = &["fKarmaModKillingEvilActor", "fKarmaModStealing", "iGuardWarnings", "fAITrespassWarningTimer", "iKarmaMax", "iKarmaMin"];
const STRINGS: &[&str] = &[
    "sKarmaMajorGained",
    "sKarmaMajorLost",
    "sKarmaMinorGained",
    "sKarmaMinorLost",
    "sKarmaMajorGainedImage",
    "sKarmaMajorLostImage",
    "sKarmaMinorGainedImage",
    "sKarmaMinorLostImage",
];
const GLOBALS: &[&str] = &["KarmaVeryEvil", "KarmaEvil", "KarmaGood", "KarmaVeryGood", "GenericCrimeKarmaDialogueChance"];

pub fn run(lo: &LoadOrder, schema: &Schema, data: &GameData, refs: &RefTable, actors: &[String], cells: &[String], survey: bool) -> anyhow::Result<Value> {
    let forms = FormStore::build(lo, schema);
    let mut g = Game::new(data, World::new(&forms, refs, SimConfig::default()), GameConfig::default());
    let s = g.crime_settings();
    let mut out = json!({
        "schema": "fnv.actor.karma/1",
        "state": "the authored game: no quest started, no script run",
        "settings": settings(&g, &forms),
        "rules": {
            "stealing": s.stealing,
            "killing_evil": s.killing_evil,
            "killing_good": -s.killing_evil,
            "murdering_neutral": -s.killing_evil * NEUTRAL_MURDER_SHARE,
            "guard_warnings": s.guard_warnings,
            "warning_timer": s.warning_timer,
            "alignment_bounds": { "very_evil": s.bounds[0], "evil": s.bounds[1], "good": s.bounds[2], "very_good": s.bounds[3] },
        },
    });
    if !actors.is_empty() {
        let list: Vec<Value> = actors.iter().map(|n| actor(&mut g, refs, n, &s)).collect::<anyhow::Result<_>>()?;
        out["actors"] = json!(list);
    }
    if !cells.is_empty() {
        let list: Vec<Value> = cells.iter().map(|n| cell(&g, refs, n)).collect::<anyhow::Result<_>>()?;
        out["cells"] = json!(list);
    }
    if survey {
        out["survey"] = json!({
            "actors": actor_survey(&mut g, refs, &s),
            "factions": faction_survey(data),
            "cells": cell_survey(data),
            "references": reference_survey(&g, refs, &forms),
            "get_is_alignment_conditions": alignment_conditions(data),
        });
    }
    Ok(out)
}

fn settings(g: &Game, forms: &FormStore) -> Value {
    let mut m = Map::new();
    for &name in SETTINGS {
        m.insert(name.into(), forms.setting_f32(name).map_or(Value::Null, |v| json!(v)));
    }
    for &name in STRINGS {
        let v = match forms.settings.get(&name.to_ascii_lowercase()) {
            Some(fnv_sim::settings::Setting::Str(s)) => json!(s),
            _ => Value::Null,
        };
        m.insert(name.into(), v);
    }
    for &name in GLOBALS {
        m.insert(name.into(), forms.global_by_edid(name).map_or(Value::Null, |(_, v)| json!(v)));
    }
    for name in ["UIKarmaUp", "UIKarmaDown"] {
        m.insert(name.into(), g.data.by_edid(name).map_or(Value::Null, |f| json!(f.to_string())));
    }
    Value::Object(m)
}

fn kill_karma(a: Alignment, s: &Settings, npc: bool) -> (f64, f64) {
    // (as a kill in a fight the actor started, as a murder)
    match a {
        _ if !npc => (0.0, 0.0),
        a if a.evil() => (s.killing_evil, s.killing_evil),
        a if a.good() => (-s.killing_evil, -s.killing_evil),
        _ if npc => (0.0, -s.killing_evil * NEUTRAL_MURDER_SHARE),
        _ => (0.0, 0.0),
    }
}

fn actor(g: &mut Game, refs: &RefTable, name: &str, s: &Settings) -> anyhow::Result<Value> {
    let r = crate::actor_factions::resolve(g, refs, name)?;
    let base = if r == FormId::PLAYER_REF { FormId::PLAYER_BASE } else { refs.get(r).map_or(r, |p| p.base) };
    let karma = fnv_game::functions::actor_value(g, r, fnv_game::actor_values::KARMA).unwrap_or(0.0);
    let a = g.alignment(r);
    let npc = g.data.actors.get(&base).is_some_and(|d| &d.sig == b"NPC_");
    let (fight, murder) = kill_karma(a, s, npc);
    Ok(json!({
        "actor": name,
        "reference": r.to_string(),
        "base": g.data.name(base),
        "record": if npc { "NPC_" } else { "CREA" },
        "karma": karma,
        "alignment": a.name(),
        "kill_karma_in_a_fight_it_started": fight,
        "kill_karma_as_murder": murder,
        "murder_if_killed_unprovoked": npc && !a.evil(),
        "factions": fnv_game::functions::factions_of(g, r).iter().map(|f| g.data.name(*f)).collect::<Vec<_>>(),
    }))
}

fn cell(g: &Game, refs: &RefTable, name: &str) -> anyhow::Result<Value> {
    let c = refs.cell_by_edid(name).ok_or_else(|| anyhow::anyhow!("no cell named {name}"))?;
    let def = g.data.cells.get(&c).copied().unwrap_or_default();
    let owned: Vec<Value> = refs
        .by_cell
        .get(&c)
        .into_iter()
        .flatten()
        .filter_map(|&r| {
            let o = g.ownership(r)?;
            let p = refs.get(r)?;
            Some(json!({
                "reference": r.to_string(),
                "edid": p.edid,
                "base": g.data.name(p.base),
                "record": String::from_utf8_lossy(&g.world.forms.sig(p.base).unwrap_or(*b"????")).into_owned(),
                "owner": g.data.name(o.owner),
                "rank": o.rank,
                "from_cell": o.from_cell,
                "player_owns": g.counts_as_owner(FormId::PLAYER_REF, &o),
                "pos": p.pos,
            }))
        })
        .collect();
    let mut by_record: BTreeMap<String, usize> = BTreeMap::new();
    for v in &owned {
        *by_record.entry(v["record"].as_str().unwrap_or("").to_string()).or_default() += 1;
    }
    Ok(json!({
        "cell": name,
        "form": c.to_string(),
        "owner": def.owner.map(|o| g.data.name(o)),
        "rank": def.rank,
        "public": def.public,
        "off_limits": def.off_limits,
        "player_trespasses_here": def.owner.is_some() && !def.public,
        "owned_references_by_record": by_record,
        "owned_references": owned,
    }))
}

fn tally(counts: &mut BTreeMap<String, usize>, key: &str) {
    *counts.entry(key.to_string()).or_default() += 1;
}

fn actor_survey(g: &mut Game, refs: &RefTable, s: &Settings) -> Value {
    let data = g.data;
    let mut records: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for d in data.actors.values() {
        let a = Alignment::of(d.karma, s.bounds);
        tally(records.entry(String::from_utf8_lossy(&d.sig).into_owned()).or_default(), a.name());
    }
    let mut placed: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let ids: Vec<FormId> = refs.refs.iter().filter(|(_, p)| matches!(&p.sig, b"ACHR" | b"ACRE")).map(|(r, _)| *r).collect();
    for r in ids {
        let a = g.alignment(r);
        let sig = if &refs.refs[&r].sig == b"ACHR" { "NPC" } else { "creature" };
        tally(placed.entry(sig.into()).or_default(), a.name());
        if a != Alignment::Neutral {
            let list = examples.entry(format!("{sig} {}", a.name())).or_default();
            if list.len() < EXAMPLES {
                list.push(refs.refs[&r].edid.clone().unwrap_or_else(|| data.name(refs.refs[&r].base)));
            }
        }
    }
    json!({ "records_by_own_karma": records, "placed_by_karma": placed, "placed_examples": examples })
}

fn faction_survey(data: &GameData) -> Value {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut evil = Vec::new();
    let mut tracking = Vec::new();
    let mut ids: Vec<(&FormId, &u16)> = data.faction_flags.iter().collect();
    ids.sort();
    for (f, &x) in ids {
        tally(&mut counts, "factions");
        for (bit, name) in [(ff::HIDDEN, "hidden"), (ff::EVIL, "evil"), (ff::SPECIAL_COMBAT, "special_combat"), (ff::TRACK_CRIME, "track_crime"), (ff::ALLOW_SELL, "allow_sell")] {
            if x & bit != 0 {
                tally(&mut counts, name);
            }
        }
        if x & ff::EVIL != 0 && x & ff::TRACK_CRIME != 0 {
            tally(&mut counts, "evil_and_track_crime");
        }
        if x & ff::EVIL != 0 && evil.len() < EXAMPLES {
            evil.push(data.name(*f));
        }
        if x & ff::TRACK_CRIME != 0 && x & ff::EVIL == 0 && tracking.len() < EXAMPLES {
            tracking.push(data.name(*f));
        }
    }
    json!({ "counts": counts, "evil_examples": evil, "track_crime_not_evil_examples": tracking })
}

fn cell_survey(data: &GameData) -> Value {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut trespass = Vec::new();
    let mut ids: Vec<(&FormId, &fnv_game::crime::CellDef)> = data.cells.iter().collect();
    ids.sort_by_key(|(f, _)| **f);
    for (id, c) in ids {
        let place = if c.interior { "interior" } else { "exterior" };
        if let Some(owner) = c.owner {
            tally(&mut counts, &format!("owned_{place}"));
            let by = if data.sig(owner).is_some_and(|s| &s == b"FACT") { "faction" } else { "npc" };
            tally(&mut counts, &format!("owned_by_{by}"));
            if c.rank.is_some() {
                tally(&mut counts, "owned_with_rank");
            }
            if !c.public {
                tally(&mut counts, &format!("owned_not_public_{place}"));
                if trespass.len() < EXAMPLES {
                    trespass.push(data.edid_of.get(id).cloned().unwrap_or_else(|| id.to_string()));
                }
            }
        }
        if c.public {
            tally(&mut counts, &format!("public_{place}"));
        }
        if c.off_limits {
            tally(&mut counts, &format!("off_limits_{place}"));
            if c.owner.is_none() {
                tally(&mut counts, "off_limits_without_owner");
            }
        }
    }
    json!({ "counts": counts, "owned_not_public_examples": trespass })
}

fn reference_survey(g: &Game, refs: &RefTable, forms: &FormStore) -> Value {
    let mut own: BTreeMap<String, usize> = BTreeMap::new();
    let mut inherited: BTreeMap<String, usize> = BTreeMap::new();
    for (&r, p) in &refs.refs {
        let kind = match forms.get(p.base).map(|b| &b.data) {
            Some(BaseData::Item(_)) => "item",
            Some(BaseData::Container(_)) => "container",
            Some(BaseData::Door(_)) => "door",
            Some(BaseData::Actor(_)) => "actor",
            Some(BaseData::Furniture) => "furniture",
            Some(BaseData::Activator) => "activator",
            _ => continue,
        };
        if p.owner.is_some() {
            tally(&mut own, kind);
            if g.data.owner_ranks.contains_key(&r) {
                tally(&mut own, "with_rank");
            }
        } else if g.ownership(r).is_some() {
            tally(&mut inherited, kind);
        }
    }
    json!({ "own_owner_by_kind": own, "owner_from_cell_by_kind": inherited })
}

fn alignment_conditions(data: &GameData) -> Value {
    let mut out = Vec::new();
    let mut ids: Vec<&FormId> = data.infos.keys().collect();
    ids.sort();
    for id in ids {
        for c in data.infos[id].conditions.iter().filter(|c| c.function == GET_IS_ALIGNMENT) {
            let code = match c.params.first() {
                Some(ArgValue::Enum(e)) => json!(e),
                other => json!(format!("{other:?}")),
            };
            out.push(json!({ "info": id.to_string(), "topic_line_of": data.edid_of.get(id), "code": code, "comparison": format!("{:?} {:?}", c.cmp, c.rhs) }));
        }
    }
    let mut packs: Vec<&FormId> = data.package_defs.keys().collect();
    packs.sort();
    for id in packs {
        for c in data.package_defs[id].conditions.iter().filter(|c| c.function == GET_IS_ALIGNMENT) {
            let code = match c.params.first() {
                Some(ArgValue::Enum(e)) => json!(e),
                other => json!(format!("{other:?}")),
            };
            out.push(json!({ "package": data.name(*id), "code": code, "comparison": format!("{:?} {:?}", c.cmp, c.rhs) }));
        }
    }
    json!(out)
}
