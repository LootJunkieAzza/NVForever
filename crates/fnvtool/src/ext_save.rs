//! `ext queries --save`: the starting state of a probe run from the original's save
//! (issue #1023). The probe sets ran in the game from the owner's save, so their rows
//! read its inventory, skills, game time, quests and place; this turns what
//! `fnv_save::fos` reads into the runner's `given` entries.

use fnv_game::GameData;
use fnv_save::fos::{Fos, VarValue};
use fnv_save::{PluginEntry, PluginRemap};
use fnv_sim::{FormId, FormStore};
use serde_json::{json, Map, Value};

/// Skill actor value codes, Barter … Unarmed.
const SKILLS: std::ops::RangeInclusive<u32> = 32..=45;
/// Strength's actor value code; SPECIAL follow in order.
const STRENGTH: u32 = 5;

fn hex(id: u32) -> String {
    format!("0x{id:08X}")
}

/// The `given` entries for a save, and what was left out (counts by reason).
pub(crate) fn save_given(fos: &Fos, data: &GameData, forms: &FormStore, plugins: &[String]) -> anyhow::Result<(Vec<Value>, Value)> {
    let saved: Vec<PluginEntry> = fos.plugins.iter().map(|n| PluginEntry { name: n.clone(), size: 0 }).collect();
    let remap = PluginRemap::from_names(&saved, plugins);
    let map = |id: u32| remap.map(id);
    let mut given = Vec::new();
    let mut skipped: Map<String, Value> = Map::new();
    let mut skip = |why: &str| {
        let n = skipped.entry(why.to_string()).or_insert(json!(0));
        *n = json!(n.as_u64().unwrap_or(0) + 1);
    };

    for (id, value) in fos.global_variables()? {
        match map(id) {
            Some(id) if forms.globals.contains_key(&FormId(id)) => given.push(json!({"kind": "global", "name": hex(id), "value": value})),
            _ => skip("global not in the load order"),
        }
    }

    for q in fos.quests()? {
        let Some(id) = map(q.form).filter(|id| data.quests.contains_key(&FormId(*id))) else {
            skip("quest not in the load order");
            continue;
        };
        let script = data.quests[&FormId(id)].script.and_then(|s| data.script(s));
        let mut g = json!({"kind": "quest", "quest": hex(id)});
        if let Some(r) = q.running() {
            g["running"] = json!(r);
        }
        if let Some(c) = q.completed() {
            g["completed"] = json!(c);
        }
        let done: Vec<i32> = q.stages.iter().filter(|s| s.1).map(|s| s.0 as i32).collect();
        let not_done: Vec<i32> = q.stages.iter().filter(|s| !s.1).map(|s| s.0 as i32).collect();
        g["done"] = json!(done);
        g["not_done"] = json!(not_done);
        g["objectives"] = q.objectives.iter().map(|(i, s)| json!({"index": i, "displayed": s & 1 != 0, "completed": s & 2 != 0})).collect();
        let (mut vars, mut refs) = (Map::new(), Map::new());
        for (i, v) in &q.vars {
            let Some(name) = script.and_then(|s| u16::try_from(*i).ok().and_then(|i| s.var_name(i))) else {
                skip("quest variable not declared");
                continue;
            };
            match v {
                VarValue::Num(x) => {
                    vars.insert(name.to_string(), json!(x));
                }
                VarValue::Ref(0) => {
                    vars.insert(name.to_string(), json!(0.0));
                }
                VarValue::Ref(r) => match map(*r) {
                    Some(r) => {
                        refs.insert(name.to_string(), json!(hex(r)));
                    }
                    None => skip("quest reference variable not in the load order"),
                },
            }
        }
        g["vars"] = Value::Object(vars);
        g["refs"] = Value::Object(refs);
        given.push(g);
    }

    let p = fos.player()?;
    if let Some(special) = p.special {
        for (i, v) in special.iter().enumerate() {
            given.push(json!({"kind": "actor_value", "of": "player", "av": (STRENGTH + i as u32).to_string(), "value": *v as f64}));
        }
    }
    if !p.tag_skills.is_empty() {
        given.push(json!({"kind": "tag_skills", "skills": p.tag_skills.iter().map(u32::to_string).collect::<Vec<_>>()}));
    }
    for (code, v) in &p.av_modifiers {
        if SKILLS.contains(code) {
            given.push(json!({"kind": "skill_points", "skill": code.to_string(), "value": *v as f64}));
        } else {
            skip("actor value modifier that is not a skill");
        }
    }
    match &p.inventory {
        Some(items) => {
            for it in items {
                match map(it.item) {
                    Some(id) if it.count != 0 => given.push(json!({"kind": "item_change", "of": "player", "item": hex(id), "change": it.count})),
                    Some(_) => {}
                    None => skip("item not in the load order"),
                }
            }
        }
        None => skip("inventory layout not understood"),
    }
    if let Some((at, pos, rot)) = p.place {
        match map(at) {
            Some(at) => given.push(json!({"kind": "place", "ref": "player", "cell": hex(at), "pos": pos, "rot": rot})),
            None => skip("place not in the load order"),
        }
    }
    let notes = json!({
        "save": {"name": fos.header.player_name, "level": fos.header.level, "location": fos.header.location, "play_time": fos.header.play_time},
        "missing_plugins": remap.missing(),
        "given": given.len(),
        "skipped": skipped,
    });
    Ok((given, notes))
}
