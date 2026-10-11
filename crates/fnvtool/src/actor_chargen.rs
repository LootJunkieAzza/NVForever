//! `fnvtool actor chargen`: what the character-creation menus offer a new player
//! (docs/spec-notes/chargen.md §6): the name, the S.P.E.C.I.A.L. sheet, the tag-skill
//! list with each skill's value, and the traits with their requirement lines. With
//! `--special` and `--tag` the sheets are filled in and applied first, as Done would.

use fnv_esm::{LoadOrder, Schema};
use fnv_game::chargen::{self, SpecialSheet, TagSheet, TraitSheet};
use fnv_game::functions::av_by_name;
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::{json, Value};

pub fn run(lo: &LoadOrder, schema: &Schema, data: &GameData, refs: &RefTable, special: Option<&str>, tags: &[String]) -> anyhow::Result<Value> {
    let forms = FormStore::build(lo, schema);
    let mut g = Game::new(data, World::new(&forms, refs, SimConfig::default()), GameConfig::default());
    let mut sheet = SpecialSheet::of_player(&mut g);
    let start = sheet;
    if let Some(s) = special {
        let v: Vec<i32> = s.split(',').map(|x| x.trim().parse()).collect::<Result<_, _>>()?;
        anyhow::ensure!(v.len() == 7, "--special takes seven numbers, Strength to Luck");
        sheet.values.copy_from_slice(&v);
        anyhow::ensure!(sheet.complete(), "--special must give out every point ({} remaining)", sheet.remaining());
        sheet.apply(&mut g);
    }
    let mut tag = TagSheet::of_player(&g);
    for t in tags {
        let code = av_by_name(t).ok_or_else(|| anyhow::anyhow!("no skill {t}"))?;
        anyhow::ensure!(tag.toggle(code), "cannot tag {t}");
    }
    let skills: Vec<Value> = tag
        .skills
        .clone()
        .into_iter()
        .map(|c| json!({"code": c, "name": chargen::skill_name(&g, c), "tagged": tag.tagged.contains(&c), "value": tag.value(&mut g, c)}))
        .collect();
    let traits = TraitSheet::of_player(&g);
    let list: Vec<Value> = traits
        .traits
        .iter()
        .map(|&t| {
            let p = &data.perks[&t];
            json!({
                "form": t.to_string(),
                "edid": data.edid_of.get(&t),
                "name": p.name,
                "requirements": chargen::requirement_line(&g, t),
                "allowed": chargen::requirements_met(&mut g, t),
                "ranks": p.ranks,
                "icon": p.icon,
            })
        })
        .collect();
    Ok(json!({
        "schema": "fnv.actor.chargen/1",
        "name_offered": chargen::name_offered(&g),
        "special": {"start": start.values, "remaining_at_start": start.remaining(), "applied": special.is_some().then_some(sheet.values)},
        "tag_skills": {"tagged": tag.tagged.len(), "complete": tag.complete(), "skills": skills},
        "traits": {"max": traits.max, "count": list.len(), "list": list},
    }))
}
