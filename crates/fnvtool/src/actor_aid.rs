//! `fnvtool actor aid`: the aid items (ALCH) of a load order with their decoded effects,
//! counts over them, and, with `--use`, what using some of them does to the player in the
//! authored game (docs/spec-notes/aid-items.md §8).

use fnv_game::aid::{self, ChangeKind};
use fnv_game::data::{MagicEffectDef, ARCHETYPE_SCRIPT};
use fnv_game::functions::{actor_value, add_actor_value_damage, AV_NAMES};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub struct Options {
    pub items: Vec<String>,
    pub uses: Vec<String>,
    pub damage: Vec<String>,
    pub seconds: f64,
    pub hardcore: bool,
}

/// MGEF archetype names [documented: xEdit wbArchtypeEnum, `schemas/fnv/gameplay.json`].
fn archetype_name(a: i64) -> String {
    match a {
        0 => "Value Modifier",
        1 => "Script",
        2 => "Dispel",
        3 => "Cure Disease",
        11 => "Invisibility",
        12 => "Chameleon",
        13 => "Light",
        16 => "Lock",
        17 => "Open",
        18 => "Bound Item",
        19 => "Summon Creature",
        24 => "Paralysis",
        30 => "Cure Paralysis",
        31 => "Cure Addiction",
        32 => "Cure Poison",
        33 => "Concussion",
        34 => "Value And Parts",
        35 => "Limb Condition",
        36 => "Turbo",
        _ => return format!("archetype {a}"),
    }
    .to_string()
}

/// ETYP names [documented: xEdit EquipType, GECK wiki Equip Type].
fn equip_type_name(t: i64) -> String {
    const NAMES: [&str; 14] = [
        "Big Guns", "Energy Weapons", "Small Guns", "Melee Weapons", "Unarmed Weapon", "Thrown Weapons", "Mine", "Body Wear", "Head Wear",
        "Hand Wear", "Chems", "Stimpak", "Food", "Alcohol",
    ];
    usize::try_from(t).ok().and_then(|i| NAMES.get(i)).map_or_else(|| "None".to_string(), |n| n.to_string())
}

fn av_name(code: Option<u16>) -> Value {
    code.and_then(|c| AV_NAMES.get(c as usize)).map_or(Value::Null, |n| json!(n))
}

fn kind_name(k: ChangeKind) -> &'static str {
    match k {
        ChangeKind::Modifier => "modifier while it lasts",
        ChangeKind::Once => "at once",
        ChangeKind::PerSecond => "per second",
    }
}

fn flag_names(m: &MagicEffectDef) -> Vec<&'static str> {
    [(MagicEffectDef::HOSTILE, "Hostile"), (MagicEffectDef::RECOVER, "Recover"), (MagicEffectDef::DETRIMENTAL, "Detrimental")]
        .into_iter()
        .filter(|(f, _)| m.has(*f))
        .map(|(_, n)| n)
        .collect()
}

fn find(data: &GameData, name: &str) -> anyhow::Result<FormId> {
    let id = data
        .edids
        .get(&name.to_ascii_lowercase())
        .copied()
        .or_else(|| u32::from_str_radix(name.trim_start_matches("0x"), 16).ok().map(FormId))
        .ok_or_else(|| anyhow::anyhow!("no form named {name}"))?;
    if !data.ingestibles.contains_key(&id) {
        anyhow::bail!("{name} ({id}) is not an ingestible (ALCH)");
    }
    Ok(id)
}

/// One effect item of a magic item, decoded.
fn effect(data: &GameData, e: &fnv_game::data::EffectItem, names: &HashMap<u32, String>) -> Value {
    let m = data.magic_effects.get(&e.effect);
    let conditions: Vec<String> =
        e.conditions.iter().map(|c| names.get(&(c.function as u32)).cloned().unwrap_or_else(|| format!("0x{:04X}", c.opcode()))).collect();
    let mut v = json!({
        "effect": e.effect.to_string(),
        "edid": data.edid_of.get(&e.effect),
        "magnitude": e.magnitude,
        "duration": e.duration,
        "actor_value": av_name(e.actor_value),
        "conditions": conditions,
    });
    match m {
        None if e.effect == aid::USAGE_MONITOR => v["archetype"] = json!("usage monitor (no record)"),
        None => v["archetype"] = json!("no record"),
        Some(m) => {
            v["archetype"] = json!(archetype_name(m.archetype));
            v["mgef_actor_value"] = av_name(m.actor_value);
            v["flags"] = json!(flag_names(m));
            if let Some(c) = aid::value_change(m, e, 1.0, false) {
                v["change"] = json!({
                    "actor_value": av_name(Some(c.code)),
                    "how": kind_name(c.kind),
                    "amount": c.amount,
                    "limbs_too": c.parts,
                    "grows": aid::grows(c.code),
                });
            }
            if m.archetype == ARCHETYPE_SCRIPT {
                v["script"] = json!(m.script.map(|s| data.name(s)));
            }
        }
    }
    v
}

pub fn run(data: &GameData, forms: &FormStore, refs: &RefTable, names: &HashMap<u32, String>, o: &Options) -> anyhow::Result<Value> {
    let wanted: Option<BTreeSet<FormId>> = if o.items.is_empty() { None } else { Some(o.items.iter().map(|n| find(data, n)).collect::<anyhow::Result<_>>()?) };
    let mut ids: Vec<FormId> = data.ingestibles.keys().copied().filter(|id| wanted.as_ref().is_none_or(|w| w.contains(id))).collect();
    ids.sort();
    let mut items = Vec::new();
    // Counts over every item listed.
    let mut by_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_archetype: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_condition: BTreeMap<String, u64> = BTreeMap::new();
    let mut monitors: BTreeMap<String, u64> = BTreeMap::new();
    let mut no_record: BTreeMap<String, u64> = BTreeMap::new();
    let mut withdrawals: BTreeSet<FormId> = BTreeSet::new();
    let (mut food, mut medicine, mut with_withdrawal, mut with_chance, mut av_differs, mut effects_n) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    for &id in &ids {
        let d = &data.ingestibles[&id];
        let list = data.effect_items.get(&id).map(Vec::as_slice).unwrap_or(&[]);
        *by_type.entry(equip_type_name(d.equip_type)).or_default() += 1;
        food += (d.flags & fnv_game::data::IngestibleDef::FOOD != 0) as u64;
        medicine += (d.flags & fnv_game::data::IngestibleDef::MEDICINE != 0) as u64;
        with_withdrawal += d.withdrawal.is_some() as u64;
        with_chance += (d.addiction_chance != 0.0) as u64;
        withdrawals.extend(d.withdrawal);
        let mut monitor = Value::Null;
        for e in list {
            effects_n += 1;
            for c in &e.conditions {
                *by_condition.entry(names.get(&(c.function as u32)).cloned().unwrap_or_else(|| format!("0x{:04X}", c.opcode()))).or_default() += 1;
            }
            match data.magic_effects.get(&e.effect) {
                Some(m) => {
                    *by_archetype.entry(archetype_name(m.archetype)).or_default() += 1;
                    if let Some(c) = aid::value_change(m, e, 1.0, false) {
                        *by_kind.entry(kind_name(c.kind).to_string()).or_default() += 1;
                        av_differs += (e.actor_value.is_some() && m.actor_value.is_some() && e.actor_value != m.actor_value) as u64;
                    }
                }
                None => {
                    *no_record.entry(e.effect.to_string()).or_default() += 1;
                    if e.effect == aid::USAGE_MONITOR {
                        *monitors.entry(format!("magnitude {}, duration {}", e.magnitude, e.duration)).or_default() += 1;
                        monitor = json!({"magnitude": e.magnitude, "duration": e.duration});
                    }
                }
            }
        }
        items.push(json!({
            "formid": id.to_string(),
            "edid": data.edid_of.get(&id),
            "name": forms.get(id).and_then(|b| b.name.clone()),
            "equip_type": equip_type_name(d.equip_type),
            "food": d.flags & fnv_game::data::IngestibleDef::FOOD != 0,
            "medicine": d.flags & fnv_game::data::IngestibleDef::MEDICINE != 0,
            "value": d.value,
            "withdrawal": d.withdrawal.map(|w| data.name(w)),
            "addiction_chance": d.addiction_chance,
            "consume_sound": d.consume_sound.map(|s| data.name(s)),
            "usage_monitor": monitor,
            "effects": list.iter().map(|e| effect(data, e, names)).collect::<Vec<_>>(),
        }));
    }
    let withdrawal_spells: Vec<Value> = withdrawals
        .iter()
        .map(|&w| {
            json!({
                "spell": w.to_string(),
                "edid": data.edid_of.get(&w),
                "type": data.spell_types.get(&w),
                "effects": data.effect_items.get(&w).map(|l| l.iter().map(|e| effect(data, e, names)).collect::<Vec<_>>()).unwrap_or_default(),
            })
        })
        .collect();
    let mut out = json!({
        "schema": "fnv.actor.aid/1",
        "summary": {
            "items": ids.len(),
            "by_equip_type": by_type,
            "food_flag": food,
            "medicine_flag": medicine,
            "with_withdrawal": with_withdrawal,
            "with_addiction_chance": with_chance,
            "with_usage_monitor": monitors.values().sum::<u64>(),
            "usage_monitors": monitors,
            "effects": effects_n,
            "effects_by_archetype": by_archetype,
            "value_changes_by_how": by_kind,
            "effects_without_a_record": no_record,
            "value_changes_whose_efit_actor_value_differs_from_the_mgef": av_differs,
            "condition_functions": by_condition,
        },
        "withdrawal_spells": withdrawal_spells,
        "items": items,
    });
    if !o.uses.is_empty() {
        out["use"] = simulate(data, forms, refs, o)?;
    }
    Ok(out)
}

/// The player in the authored game: damage, then each item used in turn, then
/// `seconds` of play; the actor values the items touch at each step.
fn simulate(data: &GameData, forms: &FormStore, refs: &RefTable, o: &Options) -> anyhow::Result<Value> {
    let mut g = Game::new(data, World::new(forms, refs, SimConfig::default()), GameConfig::default());
    let p = FormId::PLAYER_REF;
    g.player.hardcore = Some(o.hardcore);
    let mut codes: BTreeSet<u16> = BTreeSet::new();
    let mut damaged = Vec::new();
    for d in &o.damage {
        let (name, amount) = d.split_once(':').ok_or_else(|| anyhow::anyhow!("--damage {d:?}: expected <actor value>:<amount>"))?;
        let code = fnv_game::functions::av_by_name(name).ok_or_else(|| anyhow::anyhow!("no actor value {name:?}"))?;
        let amount: f64 = amount.parse()?;
        add_actor_value_damage(&mut g, p, code, amount);
        codes.insert(code);
        damaged.push(json!({"actor_value": AV_NAMES[code as usize], "amount": amount}));
    }
    let items: Vec<FormId> = o.uses.iter().map(|n| find(data, n)).collect::<anyhow::Result<_>>()?;
    for &item in &items {
        for e in data.effect_items.get(&item).into_iter().flatten() {
            if let Some(c) = data.magic_effects.get(&e.effect).and_then(|m| aid::value_change(m, e, 1.0, false)) {
                codes.insert(c.code);
            }
        }
    }
    let values = |g: &mut Game| -> Map<String, Value> { codes.iter().map(|&c| (AV_NAMES[c as usize].to_string(), json!(actor_value(g, p, c).ok()))).collect() };
    let before = values(&mut g);
    let mut steps = Vec::new();
    for &item in &items {
        let r = g.ingest(p, item);
        let active: Vec<Value> = r
            .applied
            .iter()
            .filter_map(|a| {
                let c = a.change?;
                Some(json!({"actor_value": AV_NAMES[c.code as usize], "how": kind_name(c.kind), "amount": c.amount, "applied": c.applied, "duration": a.duration, "extended": a.extended}))
            })
            .collect();
        steps.push(json!({
            "item": data.name(item),
            "potency": r.potency,
            "usage": r.usage,
            "addiction": r.addiction.map(|(chance, hit)| json!({"chance": chance, "addicted": hit})),
            "changes": active,
            "values": values(&mut g),
        }));
    }
    let frames = (o.seconds / g.cfg.frame_seconds).round() as u64;
    g.frames(frames);
    let after = values(&mut g);
    let still: Vec<Value> = g
        .effects
        .iter()
        .filter(|e| e.target == p)
        .map(|e| json!({"source": data.name(e.source), "effect": data.name(e.effect), "elapsed": e.elapsed, "duration": e.duration, "usage": e.usage}))
        .collect();
    Ok(json!({
        "hardcore": o.hardcore,
        "damage": damaged,
        "values_before": before,
        "uses": steps,
        "seconds_after": o.seconds,
        "values_after": after,
        "effects_active_after": still,
        "addictions": g.addictions(p).iter().map(|s| data.name(*s)).collect::<Vec<_>>(),
    }))
}
