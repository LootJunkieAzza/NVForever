//! `ext queries --save`: the starting state of a probe run from the original's save
//! (issue #1023). The probe sets ran in the game from the owner's save, so their rows
//! read its inventory, skills, game time, quests and place, references' state and
//! faction relations; this turns what `fnv_save::fos` reads into the runner's `given`
//! entries. Whatever cannot be carried
//! over (a record the reader does not understand, a form outside the load order, a form
//! the save created) is left out and counted by reason.

use fnv_game::GameData;
use fnv_save::fos::{Fos, VarValue, FORM_DELETED, FORM_DISABLED, FORM_TALKED_TO_PC, TYPE_REFR};
use fnv_save::{PluginEntry, PluginRemap};
use fnv_sim::{FormId, FormStore, RefTable};
use serde_json::{json, Map, Value};

/// Skill actor value codes, Barter … Unarmed.
const SKILLS: std::ops::RangeInclusive<u32> = 32..=45;
/// Strength's actor value code; SPECIAL follow in order.
const STRENGTH: u32 = 5;

fn hex(id: u32) -> String {
    format!("0x{id:08X}")
}

/// What the load order holds, as `save_given` asks it.
pub(crate) trait Lookup {
    fn is_global(&self, id: u32) -> bool;
    fn is_quest(&self, id: u32) -> bool;
    /// The name of a quest script's variable by its index.
    fn quest_var(&self, quest: u32, index: u32) -> Option<String>;
    /// A placed reference (REFR, ACHR, ACRE) of the load order.
    fn is_reference(&self, id: u32) -> bool;
    /// An interior cell, or a worldspace with a cell at the position.
    fn is_place(&self, at: u32, pos: [f32; 3]) -> bool;
}

/// The load order's data and forms.
pub(crate) struct Loaded<'a> {
    pub data: &'a GameData,
    pub forms: &'a FormStore,
    pub refs: &'a RefTable,
}

impl Lookup for Loaded<'_> {
    fn is_global(&self, id: u32) -> bool {
        self.forms.globals.contains_key(&FormId(id))
    }
    fn is_quest(&self, id: u32) -> bool {
        self.data.quests.contains_key(&FormId(id))
    }
    fn quest_var(&self, quest: u32, index: u32) -> Option<String> {
        let script = self.data.quests.get(&FormId(quest))?.script?;
        self.data.script(script)?.var_name(u16::try_from(index).ok()?).map(str::to_string)
    }
    fn is_reference(&self, id: u32) -> bool {
        self.refs.get(FormId(id)).is_some()
    }
    fn is_place(&self, at: u32, pos: [f32; 3]) -> bool {
        let at = FormId(at);
        self.refs.world_cell_at(at, pos).is_some() || self.refs.by_cell.contains_key(&at) || self.refs.world_of(at).is_some()
    }
}

/// The `given` entries for a save, and what was left out (counts by reason).
pub(crate) fn save_given(fos: &Fos, look: &dyn Lookup, plugins: &[String]) -> anyhow::Result<(Vec<Value>, Value)> {
    let saved: Vec<PluginEntry> = fos.plugins.iter().map(|n| PluginEntry { name: n.clone(), size: 0 }).collect();
    let remap = PluginRemap::from_names(&saved, plugins);
    // A form of the load order, or why not: a form the save created exists only in it.
    let map = |id: u32| -> Result<u32, &'static str> {
        if id >> 24 == 0xFF {
            return Err("created form");
        }
        remap.map(id).ok_or("form of a plugin not loaded")
    };
    let mut given = Vec::new();
    let mut skipped: Map<String, Value> = Map::new();
    let mut skip = |what: &str, why: &str| {
        let n = skipped.entry(format!("{what}: {why}")).or_insert(json!(0));
        *n = json!(n.as_u64().unwrap_or(0) + 1);
    };

    for (id, value) in fos.global_variables()? {
        match map(id) {
            Ok(id) if look.is_global(id) => given.push(json!({"kind": "global", "name": hex(id), "value": value})),
            Ok(_) => skip("global", "no GLOB record"),
            Err(why) => skip("global", why),
        }
    }

    for q in fos.quests() {
        let Ok(q) = q else {
            skip("quest", "record not understood");
            continue;
        };
        let id = match map(q.form) {
            Ok(id) if look.is_quest(id) => id,
            Ok(_) => {
                skip("quest", "no QUST record");
                continue;
            }
            Err(why) => {
                skip("quest", why);
                continue;
            }
        };
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
            let Some(name) = look.quest_var(id, *i) else {
                skip("quest variable", "not declared");
                continue;
            };
            match v {
                VarValue::Num(x) => {
                    vars.insert(name, json!(x));
                }
                VarValue::Ref(None) => {
                    vars.insert(name, json!(0.0));
                }
                VarValue::Ref(Some(r)) => match map(*r) {
                    Ok(r) => {
                        refs.insert(name, json!(hex(r)));
                    }
                    Err(why) => skip("quest reference variable", why),
                },
                VarValue::Unresolved => skip("quest reference variable", "not in the save's form ID array"),
            }
        }
        g["vars"] = Value::Object(vars);
        g["refs"] = Value::Object(refs);
        given.push(g);
    }

    if let Some(q) = fos.active_quest() {
        match map(q) {
            Ok(q) if look.is_quest(q) => given.push(json!({"kind": "active_quest", "quest": hex(q)})),
            Ok(_) => skip("active quest", "no QUST record"),
            Err(why) => skip("active quest", why),
        }
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
            skip("actor value modifier", "not a skill");
        }
    }
    match &p.inventory {
        Some(items) => {
            for it in items.iter().filter(|it| it.count != 0) {
                match it.item.ok_or("not in the save's form ID array").and_then(map) {
                    Ok(id) => given.push(json!({"kind": "item_change", "of": "player", "item": hex(id), "change": it.count})),
                    Err(why) => skip("item", why),
                }
            }
        }
        None => skip("inventory", "not read (no player record, or a layout not understood)"),
    }
    if let Some((at, pos, rot)) = p.place {
        match map(at) {
            Ok(at) => given.push(json!({"kind": "place", "ref": "player", "cell": hex(at), "pos": pos, "rot": rot})),
            Err(why) => skip("place", why),
        }
    }

    // Other references: their form flags (disabled, taken, talked to) and places.
    for r in fos.reference_changes() {
        let r = match r {
            Ok(r) => r,
            Err(_) => {
                skip("reference", "change flags of a layout not known");
                continue;
            }
        };
        let id = match map(r.form) {
            Ok(id) if look.is_reference(id) => id,
            Ok(_) => {
                skip("reference", "not a placed reference of the load order");
                continue;
            }
            Err(why) => {
                skip("reference", why);
                continue;
            }
        };
        if let Some(f) = r.form_flags {
            let mut g = json!({"kind": "ref", "ref": hex(id), "disabled": f & FORM_DISABLED != 0, "deleted": f & FORM_DELETED != 0});
            if r.kind != TYPE_REFR {
                g["talked_to_pc"] = json!(f & FORM_TALKED_TO_PC != 0);
            }
            given.push(g);
        }
        if let Some((at, pos, rot)) = r.place {
            match map(at) {
                Ok(at) if look.is_place(at, pos) => given.push(json!({"kind": "place", "ref": hex(id), "cell": hex(at), "pos": pos, "rot": rot})),
                Ok(_) => skip("reference place", "no cell there in the load order"),
                Err(why) => skip("reference place", why),
            }
        }
    }

    for f in fos.faction_relations() {
        let Ok(f) = f else {
            skip("faction", "record not understood");
            continue;
        };
        let Ok(faction) = map(f.form) else {
            skip("faction", "form of a plugin not loaded");
            continue;
        };
        for (other, _modifier, reaction) in f.relations {
            match other.ok_or("not in the save's form ID array").and_then(map) {
                Ok(o) => given.push(json!({"kind": "faction_reaction", "faction": hex(faction), "other": hex(o), "reaction": reaction})),
                Err(why) => skip("faction relation", why),
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_save::fos::{ChangedForm, TYPE_ACHR, TYPE_FACT, TYPE_QUST};

    const BAR: u8 = 0x7C;

    /// A load order of FalloutNV.esm and Mod.esp: globals 0x10 and 0x01000020, quest 0x30
    /// with variables 1 (`fNum`) and 2 (`rRef`).
    struct Fake;
    impl Lookup for Fake {
        fn is_global(&self, id: u32) -> bool {
            id == 0x10 || id == 0x0100_0020
        }
        fn is_quest(&self, id: u32) -> bool {
            id == 0x30
        }
        fn quest_var(&self, quest: u32, index: u32) -> Option<String> {
            match (quest, index) {
                (0x30, 1) => Some("fNum".into()),
                (0x30, 2) => Some("rRef".into()),
                _ => None,
            }
        }
        fn is_reference(&self, id: u32) -> bool {
            id == 0x40 || id == 0x41
        }
        fn is_place(&self, at: u32, _pos: [f32; 3]) -> bool {
            at == 0x50
        }
    }

    /// A kind-0 reference to the form ID array's 1-based `i`.
    fn r(i: u32) -> [u8; 3] {
        let b = i.to_be_bytes();
        [b[1], b[2], b[3]]
    }

    /// A save made with [Mod.esp, FalloutNV.esm] (the other order than the load order's):
    /// globals 0x01000010 (FalloutNV's 0x10), 0x00000020 (Mod's 0x20), a created form and
    /// one of a plugin not loaded; quest 0x01000030 with a number, a reference to a
    /// created form and one past the form ID array; a quest record the reader does not
    /// understand.
    fn fos() -> Fos {
        let form_ids = vec![0x0100_0010, 0x0000_0020, 0xFF00_0001, 0x0200_0040, 0x0100_0030];
        let mut g3 = vec![4 << 2, BAR];
        for i in 1..=4 {
            g3.extend(r(i));
            g3.push(BAR);
            g3.extend(1.5f32.to_le_bytes());
            g3.push(BAR);
        }
        let mut q = vec![3 << 2, BAR];
        q.extend(1u32.to_le_bytes());
        q.push(BAR);
        q.extend(2.0f64.to_le_bytes());
        q.push(BAR);
        q.extend(0x8000_0002u32.to_le_bytes());
        q.push(BAR);
        q.extend(r(3));
        q.push(BAR);
        q.extend(0x8000_0002u32.to_le_bytes());
        q.push(BAR);
        q.extend(r(99));
        q.push(BAR);
        q.extend([0, BAR, 0, BAR]);
        Fos {
            plugins: vec!["Mod.esp".into(), "FalloutNV.esm".into()],
            form_ids,
            globals_data: vec![(3, g3)],
            changed: vec![
                ChangedForm { form: 0x0100_0030, flags: 1 << 30, kind: TYPE_QUST, data: q },
                ChangedForm { form: 0x0100_0030, flags: 1 << 1, kind: TYPE_QUST, data: vec![1, BAR, 9] },
            ],
            ..Fos::default()
        }
    }

    #[test]
    fn a_save_maps_through_its_plugin_list_and_counts_what_it_leaves_out() {
        let plugins = ["FalloutNV.esm".to_string(), "Mod.esp".to_string()];
        let (given, notes) = save_given(&fos(), &Fake, &plugins).unwrap();
        let globals: Vec<&Value> = given.iter().filter(|g| g["kind"] == "global").map(|g| &g["name"]).collect();
        assert_eq!(globals, [&json!("0x00000010"), &json!("0x01000020")], "both plugins' indices swapped");
        let quest = given.iter().find(|g| g["kind"] == "quest").unwrap();
        assert_eq!(quest["quest"], "0x00000030");
        assert_eq!(quest["vars"], json!({"fNum": 2.0}));
        assert_eq!(quest["refs"], json!({}));
        assert_eq!(
            notes["skipped"],
            json!({
                "global: created form": 1,
                "global: form of a plugin not loaded": 1,
                "inventory: not read (no player record, or a layout not understood)": 1,
                "quest reference variable: created form": 1,
                "quest reference variable: not in the save's form ID array": 1,
                "quest: record not understood": 1,
            })
        );
        assert_eq!(notes["given"], given.len());
    }

    /// One plugin: a taken note (REFR 0x40), an actor moved and talked to (ACHR 0x41), a
    /// reference the load order lacks, and a faction relation.
    #[test]
    fn references_and_factions_carry_over() {
        let mut moved = vec![0, 0, 3];
        for v in [1.0f32, 2.0, 3.0, 0.0, 0.0, 0.5] {
            moved.extend(v.to_le_bytes());
        }
        moved.extend([BAR, 3, BAR]);
        moved.extend(0x449u32.to_le_bytes());
        moved.push(BAR);
        let mut taken = 0x429u32.to_le_bytes().to_vec();
        taken.push(BAR);
        let mut fact = vec![1 << 2, BAR, 0, 0, 2, BAR];
        fact.extend(100i32.to_le_bytes());
        fact.push(BAR);
        fact.extend(3u32.to_le_bytes());
        fact.push(BAR);
        let fos = Fos {
            plugins: vec!["FalloutNV.esm".into()],
            form_ids: vec![0x40, 0x41, 0x50, 0x60],
            changed: vec![
                ChangedForm { form: 0x40, flags: 1, kind: TYPE_REFR, data: taken.clone() },
                ChangedForm { form: 0x41, flags: 0b11, kind: TYPE_ACHR, data: moved },
                ChangedForm { form: 0x42, flags: 1, kind: TYPE_REFR, data: taken },
                ChangedForm { form: 0x60, flags: 1 << 2, kind: TYPE_FACT, data: fact },
            ],
            ..Fos::default()
        };
        let (given, notes) = save_given(&fos, &Fake, &["FalloutNV.esm".to_string()]).unwrap();
        let of = |k: &str| given.iter().filter(|g| g["kind"] == k).cloned().collect::<Vec<_>>();
        assert_eq!(
            of("ref"),
            [
                json!({"kind": "ref", "ref": "0x00000040", "disabled": false, "deleted": true}),
                json!({"kind": "ref", "ref": "0x00000041", "disabled": false, "deleted": false, "talked_to_pc": true}),
            ]
        );
        assert_eq!(of("place"), [json!({"kind": "place", "ref": "0x00000041", "cell": "0x00000050", "pos": [1.0, 2.0, 3.0], "rot": [0.0, 0.0, 0.5]})]);
        assert_eq!(of("faction_reaction"), [json!({"kind": "faction_reaction", "faction": "0x00000060", "other": "0x00000041", "reaction": 3})]);
        assert_eq!(notes["skipped"]["reference: not a placed reference of the load order"], 1);
    }
}
