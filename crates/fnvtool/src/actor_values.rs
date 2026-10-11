//! `fnvtool actor values`: actor values as the records and formulas give them, and what
//! the load order's records show about them (docs/spec-notes/actor-values.md).

use crate::ai_cmd::winners;
use fnv_esm::{LoadOrder, Schema};
use fnv_game::actor_values as av;
use fnv_game::functions::{actor_value_parts, av_by_name, AV_NAMES};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_sim::extract::{decode, first};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};

/// The settings the base values read, with the default used when the load order has none
/// (actor-values.md §4).
const SETTINGS: [&str; 30] = [
    "fAVDSkillPrimaryBonusMult",
    "fAVDSkillLuckBonusMult",
    "fAVDTagSkillBonus",
    "fAVDSkillBarterBase",
    "fAVDSkillBigGunsBase",
    "fAVDSkillEnergyWeaponsBase",
    "fAVDSkillExplosivesBase",
    "fAVDSkillLockpickBase",
    "fAVDSkillMedicineBase",
    "fAVDSkillMeleeWeaponsBase",
    "fAVDSkillRepairBase",
    "fAVDSkillScienceBase",
    "fAVDSkillSmallGunsBase",
    "fAVDSkillSneakBase",
    "fAVDSkillSpeechBase",
    "fAVDSkillSurvivalBase",
    "fAVDSkillUnarmedBase",
    "fAVDActionPointsBase",
    "fAVDActionPointsMult",
    "fAVDCarryWeightsBase",
    "fAVDCarryWeightMult",
    "fAVDCritLuckBase",
    "fAVDCritLuckMult",
    "fAVDMeleeDamageStrengthMult",
    "fAVDMeleeDamageStrengthOffset",
    "fAVDUnarmedDamageBase",
    "fAVDUnarmedDamageMult",
    "fAVDHealthEnduranceMult",
    "fAVDNPCHealthEnduranceMult",
    "fAVDNPCHealthLevelMult",
];

pub fn run(lo: &LoadOrder, schema: &Schema, data: &GameData, refs: &RefTable, actors: &[String], survey: bool) -> anyhow::Result<Value> {
    let forms = FormStore::build(lo, schema);
    // A game in its authored state: no quest started, no script run.
    let mut g = Game::new(data, World::new(&forms, refs, SimConfig::default()), GameConfig::default());
    let mut out = json!({
        "schema": "fnv.actor.values/1",
        "state": "the authored game: no quest started, no script run, the player as its record (level 1, nothing tagged)",
    });
    if !actors.is_empty() {
        let list: Vec<Value> = actors.iter().map(|name| actor(&mut g, refs, name)).collect::<anyhow::Result<_>>()?;
        out["actors"] = json!(list);
    }
    if survey {
        out["survey"] = records(lo, schema, &g);
        out["settings"] = SETTINGS.iter().map(|s| (s.to_string(), json!(forms.setting_f32(s)))).collect::<Map<String, Value>>().into();
    }
    Ok(out)
}

/// A placed actor from a name: `player`, a reference, or an actor base (its first
/// placed reference).
fn resolve(g: &Game, refs: &RefTable, name: &str) -> anyhow::Result<FormId> {
    if name.eq_ignore_ascii_case("player") {
        return Ok(FormId::PLAYER_REF);
    }
    let id = crate::ai_packages::find_form(g.data, refs, name).ok_or_else(|| anyhow::anyhow!("no form named {name}"))?;
    if id == FormId::PLAYER_REF || refs.get(id).is_some() {
        return Ok(id);
    }
    if !g.data.actors.contains_key(&id) {
        anyhow::bail!("{name} ({id}) is not an actor or a placed actor");
    }
    refs.refs.iter().filter(|(_, p)| p.base == id).map(|(r, _)| *r).min().ok_or_else(|| anyhow::anyhow!("{name} ({id}) has no placed reference"))
}

fn actor(g: &mut Game, refs: &RefTable, name: &str) -> anyhow::Result<Value> {
    let r = resolve(g, refs, name)?;
    let base = if r == FormId::PLAYER_REF { FormId::PLAYER_BASE } else { refs.get(r).map(|p| p.base).unwrap_or(r) };
    let d = g.data.actors.get(&base).ok_or_else(|| anyhow::anyhow!("{name} ({r}) is not an actor"))?;
    let mut values = Map::new();
    for (code, av_name) in AV_NAMES.iter().enumerate() {
        if let Ok([base, _, _]) = actor_value_parts(g, r, code as u16) {
            values.insert(av_name.to_string(), json!(base));
        }
    }
    Ok(json!({
        "actor": name,
        "reference": r.to_string(),
        "base": g.data.name(base),
        "record": String::from_utf8_lossy(&d.sig),
        "level": g.actor_level(r),
        "auto_calc": d.autocalc,
        "template": d.template.map(|t| g.data.name(t)),
        "stats_from_template": d.template_stats,
        "ai_data_from_template": d.template_ai,
        // The record's own DNAM bytes, Barter to Unarmed (NPCs).
        "stored_skill_values": d.skills,
        "stored_skill_offsets": d.skill_offsets,
        "values": values,
    }))
}

/// What the records hold: how many actors have each field, how the skill formula fits
/// the stored skills of auto-calculated NPCs for each attribute, and the skill offsets.
fn records(lo: &LoadOrder, schema: &Schema, g: &Game) -> Value {
    // Each class's tag skills (CLAS DATA), as actor value codes, and attributes (ATTR).
    let mut tags: HashMap<FormId, Vec<u16>> = HashMap::new();
    let mut class_attributes: HashMap<FormId, Vec<u64>> = HashMap::new();
    for (&id, &(_, r)) in &winners(lo, &[b"CLAS"]) {
        let Ok(d) = decode(r, schema) else { continue };
        let list = first(&d, b"DATA").and_then(|v| v.get("tag_skills")).and_then(Value::as_array).cloned().unwrap_or_default();
        let codes = list.iter().filter_map(|t| t.as_u64().map(|n| n as u16).or_else(|| t.as_str().and_then(av_by_name))).collect();
        tags.insert(FormId(id), codes);
        if let Some(a) = first(&d, b"ATTR") {
            class_attributes.insert(FormId(id), fnv_sim::forms::SPECIAL.iter().filter_map(|n| a.get(n).and_then(Value::as_u64)).collect());
        }
    }
    let special = &AV_NAMES[av::STRENGTH as usize..=av::LUCK as usize];
    let (mut npcs, mut auto, mut creatures, mut attributes, mut skills) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let (mut auto_as_class, mut level_mult, mut auto_level_mult) = (0u64, 0u64, 0u64);
    // Per skill: untagged auto-calculated NPCs, and for how many the formula with each
    // attribute gives the stored value.
    let mut fit = [[0u64; 7]; 14];
    let mut untagged = [0u64; 14];
    let (mut with_offsets, mut auto_with_offsets, mut offset_entries, mut over, mut value_over) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut combat: BTreeMap<u8, u64> = BTreeMap::new();
    let mut stealth: BTreeMap<u8, u64> = BTreeMap::new();
    for d in g.data.actors.values() {
        attributes += d.attributes.is_some() as u64;
        if &d.sig == b"CREA" {
            creatures += 1;
            *combat.entry(d.combat_skill.unwrap_or(0)).or_default() += 1;
            *stealth.entry(d.stealth_skill.unwrap_or(0)).or_default() += 1;
            continue;
        }
        npcs += 1;
        auto += d.autocalc as u64;
        let (Some(a), Some(values)) = (d.attributes, d.skills) else { continue };
        skills += 1;
        let offsets = d.skill_offsets.unwrap_or_default();
        let n = offsets.iter().filter(|o| **o > 0).count() as u64;
        with_offsets += (n > 0) as u64;
        auto_with_offsets += (n > 0 && d.autocalc) as u64;
        offset_entries += n;
        over += (0..14).filter(|&i| offsets[i] > 0 && values[i] as u32 + offsets[i] as u32 > 100).count() as u64;
        value_over += values.iter().filter(|v| **v > 100).count() as u64;
        level_mult += d.pc_level_mult as u64;
        if !d.autocalc {
            continue;
        }
        auto_level_mult += d.pc_level_mult as u64;
        auto_as_class += d.class.and_then(|c| class_attributes.get(&c)).is_some_and(|c| c.iter().copied().eq(a.iter().map(|x| *x as u64))) as u64;
        let class_tags = d.class.and_then(|c| tags.get(&c));
        for i in 0..14 {
            let skill = av::BARTER + i as u16;
            if class_tags.is_some_and(|t| t.contains(&skill)) {
                continue;
            }
            untagged[i] += 1;
            for (j, attribute) in a.iter().enumerate() {
                fit[i][j] += (g.skill_formula(skill, *attribute as f64, a[6] as f64) == values[i] as f64) as u64;
            }
        }
    }
    let mut by_skill = Map::new();
    for i in 0..14 {
        let skill = av::BARTER + i as u16;
        let share = |j: usize| if untagged[i] > 0 { (fit[i][j] as f64 / untagged[i] as f64 * 1000.0).round() / 1000.0 } else { 0.0 };
        let best = (0..7).max_by_key(|&j| fit[i][j]).unwrap_or(0);
        by_skill.insert(
            AV_NAMES[skill as usize].to_string(),
            json!({
                "auto_calc_npcs_whose_class_does_not_tag_it": untagged[i],
                "share_the_formula_fits_by_attribute": special.iter().enumerate().map(|(j, n)| (n.to_string(), json!(share(j)))).collect::<Map<String, Value>>(),
                "best_fit": special[best],
                "used": av::governing_attribute(skill).map(|c| AV_NAMES[c as usize]),
            }),
        );
    }
    json!({
        "npcs": npcs,
        "auto_calc_npcs": auto,
        "auto_calc_npcs_with_their_class_attributes": auto_as_class,
        "pc_level_mult_npcs": level_mult,
        "of_them_auto_calc": auto_level_mult,
        "creatures": creatures,
        "actors_with_attributes": attributes,
        "npcs_with_skills": skills,
        "skill_formula_fit": by_skill,
        "skill_offsets": {
            "npcs_with_an_offset": with_offsets,
            "of_them_auto_calc": auto_with_offsets,
            "skills_with_an_offset": offset_entries,
            "of_them_value_plus_offset_over_100": over,
            "skill_values_over_100": value_over,
        },
        "creature_combat_skill_distinct_values": combat.len(),
        "creature_stealth_skill": stealth.iter().map(|(k, n)| (k.to_string(), json!(n))).collect::<Map<String, Value>>(),
    })
}
