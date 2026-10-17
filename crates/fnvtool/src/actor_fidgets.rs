//! `fnvtool actor fidgets`: the fidgets the IDLE tree gives actors, drawn-weapon ones
//! (docs/spec-notes/idles.md §11) or, with `--furniture`, the ones on furniture
//! (docs/spec-notes/furniture.md §8). For each actor, in a game where nothing has
//! happened yet, its weapon is put out (or it is put on the furniture, in the state of
//! being on it) and the tree is asked many times (`Game::fidget`, without replay
//! delays); the picks are counted per idle (fnv.actor.fidgets/1).
//! `--weapon` puts that weapon in every actor's hands (none is equipped in a fresh game).

use fnv_esm::{LoadOrder, Schema};
use fnv_game::game::{FurnitureUse, Game, GameConfig};
use fnv_game::idles::FidgetKind;
use fnv_game::GameData;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The people's animation folder below `meshes\` (the engine's `idles::PEOPLE_FOLDER`).
const PEOPLE_FOLDER: &str = "characters\\_male";

/// What the actors are asked in.
pub struct How<'a> {
    pub weapon: Option<&'a str>,
    /// `REF:MARKER_ID`: on this furniture.
    pub furniture: Option<&'a str>,
    pub first_person: bool,
    pub checks: u32,
}

pub fn run(lo: &LoadOrder, schema: &Schema, data: &GameData, refs: &RefTable, actors: &[String], how: &How) -> anyhow::Result<Value> {
    let forms = FormStore::build(lo, schema);
    let mut g = Game::new(data, World::new(&forms, refs, SimConfig::default()), GameConfig::default());
    g.set_first_person(how.first_person);
    // On furniture: the furniture reference and the marker sat down by, in the state of
    // being on it (`GetSitting` / `GetSleeping` 3).
    let seat = match how.furniture {
        Some(f) => {
            let (r, m) = f.rsplit_once(':').ok_or_else(|| anyhow::anyhow!("--furniture wants REF:MARKER_ID, got {f}"))?;
            let reference = crate::actor_factions::resolve(&g, refs, r)?;
            Some(FurnitureUse { reference, marker_id: m.parse()?, state: 3 })
        }
        None => None,
    };
    let kind = if seat.is_some() { FidgetKind::OnFurniture } else { FidgetKind::WeaponOut };
    let mut rows = Vec::new();
    for name in actors {
        let actor = crate::actor_factions::resolve(&g, refs, name)?;
        match seat {
            Some(s) => g.set_furniture_use(actor, Some(s)),
            None => g.set_weapon_out(actor, true),
        }
        // The weapon in the hands, as the engine tells it for a drawn NPC.
        if let Some(w) = how.weapon {
            let w = data.by_edid(w).or_else(|| u32::from_str_radix(w.trim_start_matches("0x"), 16).ok().map(FormId)).ok_or_else(|| anyhow::anyhow!("no weapon {w}"))?;
            g.set_held_weapon(actor, Some(w));
        }
        let mut picks: BTreeMap<String, u32> = BTreeMap::new();
        for _ in 0..how.checks {
            if let Some(idle) = g.fidget(actor, PEOPLE_FOLDER, kind, &|_| false) {
                *picks.entry(data.name(idle)).or_default() += 1;
            }
        }
        let anim_type = g.weapon_anim_type(actor);
        rows.push(json!({
            "actor": data.name(actor),
            "player": actor == FormId::PLAYER_REF,
            "weapon_anim_type": anim_type,
            "furniture": seat.map(|s| json!({ "reference": data.name(s.reference), "marker_id": s.marker_id })),
            "checks": how.checks,
            "fidgets": picks.values().sum::<u32>(),
            "by_idle": picks,
        }));
    }
    Ok(json!({ "schema": "fnv.actor.fidgets/1", "kind": kind, "actors": rows }))
}
