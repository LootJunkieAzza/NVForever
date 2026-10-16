//! `fnvtool actor fidgets`: the drawn-weapon fidgets the IDLE tree gives actors
//! (docs/spec-notes/idles.md §11). For each actor, its weapon is put out in a game where
//! nothing has happened yet and the tree is asked many times (`Game::weapon_fidget`,
//! without replay delays); the picks are counted per idle (fnv.actor.fidgets/1).
//! `--weapon` puts that weapon in every actor's hands (none is equipped in a fresh game).

use fnv_esm::{LoadOrder, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The people's animation folder below `meshes\` (the engine's `idles::PEOPLE_FOLDER`).
const PEOPLE_FOLDER: &str = "characters\\_male";

pub fn run(lo: &LoadOrder, schema: &Schema, data: &GameData, refs: &RefTable, actors: &[String], weapon: Option<&str>, checks: u32) -> anyhow::Result<Value> {
    let forms = FormStore::build(lo, schema);
    let mut g = Game::new(data, World::new(&forms, refs, SimConfig::default()), GameConfig::default());
    let mut rows = Vec::new();
    for name in actors {
        let actor = crate::actor_factions::resolve(&g, refs, name)?;
        g.set_weapon_out(actor, true);
        // The weapon in the hands, as the engine tells it for a drawn NPC.
        if let Some(w) = weapon {
            let w = data.by_edid(w).or_else(|| u32::from_str_radix(w.trim_start_matches("0x"), 16).ok().map(FormId)).ok_or_else(|| anyhow::anyhow!("no weapon {w}"))?;
            g.set_held_weapon(actor, Some(w));
        }
        let mut picks: BTreeMap<String, u32> = BTreeMap::new();
        for _ in 0..checks {
            if let Some(idle) = g.weapon_fidget(actor, PEOPLE_FOLDER, &|_| false) {
                *picks.entry(data.name(idle)).or_default() += 1;
            }
        }
        let anim_type = g.weapon_anim_type(actor);
        rows.push(json!({
            "actor": data.name(actor),
            "player": actor == FormId::PLAYER_REF,
            "weapon_anim_type": anim_type,
            "checks": checks,
            "fidgets": picks.values().sum::<u32>(),
            "by_idle": picks,
        }));
    }
    Ok(json!({ "schema": "fnv.actor.fidgets/1", "actors": rows }))
}
