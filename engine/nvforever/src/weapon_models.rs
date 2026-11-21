//! Weapon models scripts changed, drawn (docs/spec-notes/weapon-mods.md §8): SetWeaponModel,
//! SetModelPath and SetWeapon1stPersonModel change a WEAP's MODL, MWD1–MWD7, WNAM and
//! WNM1–WNM7 for the session (`fnv_game::Game::weapon_model_changes`); this copies those
//! changes into `ActorWorld::actors`, the renderer's record view, so the player's and
//! NPCs' weapons are drawn with them. A weapon mod that replaces its models by script
//! (WAP's `gr_` files) then needs no copy of its models over the masters' paths.
//!
//! A change that goes (a load) puts the record's own models back. The player's weapon
//! is drawn again when its model changes (`player_body::ensure_body`); an NPC's when it
//! next takes up a weapon.

use crate::actors::ActorWorld;
use bevy::prelude::*;
use fnv_actor::records::{ActorForms, Weapon};
use fnv_game::nvse::WeaponModelChange;
use fnv_sim::FormId;
use nvf_play::script::ScriptState;
use std::collections::{BTreeMap, HashMap};

pub struct WeaponModelsPlugin;

impl Plugin for WeaponModelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_weapon_models.after(nvf_play::PlaySet::Activation));
    }
}

/// An alternate texture list of `ActorForms::alt_textures`, with its key.
type AltTextures = ((FormId, [u8; 4]), Vec<fnv_world::alt_textures::AltTexture>);

/// The record's own models of a weapon a change touched, and the alternate texture lists
/// that go with them.
struct Authored {
    weapon: Weapon,
    textures: Vec<AltTextures>,
}

/// What the copy was made from.
#[derive(Default)]
struct WeaponModelSync {
    stamp: Option<u64>,
    changes: BTreeMap<FormId, WeaponModelChange>,
    authored: HashMap<FormId, Authored>,
}

impl WeaponModelSync {
    /// Copies `now` into `actors`: each weapon changed before or now gets its record's
    /// models back, then the changes it has now.
    fn accept(&mut self, now: BTreeMap<FormId, WeaponModelChange>, actors: &mut ActorForms) {
        let ids: Vec<FormId> = self.changes.keys().chain(now.keys()).copied().collect();
        for id in ids {
            let Some(w) = actors.weapons.get(&id) else { continue };
            let authored = self.authored.entry(id).or_insert_with(|| Authored {
                textures: alt_texture_keys(id, w).filter_map(|k| actors.alt_textures.get(&k).map(|l| (k, l.clone()))).collect(),
                weapon: w.clone(),
            });
            for k in alt_texture_keys(id, &authored.weapon) {
                actors.alt_textures.remove(&k);
            }
            let change = now.get(&id);
            // A changed model is not the one the record's alternate textures were made for
            // [hypothesis: they name its shapes]; the others keep theirs.
            for (k, l) in &authored.textures {
                if change.is_none_or(|c| !touches(c, id, k, &authored.weapon)) {
                    actors.alt_textures.insert(*k, l.clone());
                }
            }
            if let Some(w) = actors.weapons.get_mut(&id) {
                apply(w, &authored.weapon, change);
            }
        }
        if !(now.is_empty() && self.changes.is_empty()) {
            info!("weapon models: {} weapon(s) drawn with models scripts set", now.len());
        }
        self.changes = now;
    }
}

/// The alternate texture lists drawn with `w` (`ActorForms::weapon_alt_textures`).
fn alt_texture_keys(id: FormId, w: &Weapon) -> impl Iterator<Item = (FormId, [u8; 4])> {
    [Some((id, *b"MODL")), Some((id, *b"MOD4")), w.first_person.map(|f| (f, *b"MODL"))].into_iter().flatten()
}

/// Whether `c` changes the model the alternate texture list `k` goes with.
fn touches(c: &WeaponModelChange, id: FormId, k: &(FormId, [u8; 4]), w: &Weapon) -> bool {
    if k.0 == id {
        c.world[0].is_some()
    } else {
        w.first_person == Some(k.0) && c.first_person[0].is_some()
    }
}

/// `w` with `authored`'s models, then `change`'s.
fn apply(w: &mut Weapon, authored: &Weapon, change: Option<&WeaponModelChange>) {
    w.model.clone_from(&authored.model);
    w.held_world_model.clone_from(&authored.held_world_model);
    w.mod_models.clone_from(&authored.mod_models);
    w.first_person = authored.first_person;
    w.first_person_model.clone_from(&authored.first_person_model);
    w.mod_first_person = authored.mod_first_person;
    w.mod_first_person_models.clone_from(&authored.mod_first_person_models);
    let Some(c) = change else { return };
    if let Some(m) = &c.world[0] {
        w.model.clone_from(m);
        // The MOD4 held in third person was chosen beside the record's MODL
        // (`fnv_actor::records::held_world_model`); a script's model is drawn instead
        // [hypothesis: the mod meant its model to be seen].
        w.held_world_model = None;
    }
    for k in 1..8 {
        if let Some(m) = &c.world[k] {
            w.mod_models[k - 1].clone_from(m);
        }
    }
    if let Some((f, m)) = &c.first_person[0] {
        w.first_person = *f;
        w.first_person_model.clone_from(m);
    }
    for k in 1..8 {
        if let Some((f, m)) = &c.first_person[k] {
            w.mod_first_person[k - 1] = *f;
            w.mod_first_person_models[k - 1].clone_from(m);
        }
    }
}

fn sync_weapon_models(script: Option<Res<ScriptState>>, actor_world: Option<ResMut<ActorWorld>>, mut last: Local<WeaponModelSync>) {
    let Some(g) = script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    let Some(mut aw) = actor_world else { return };
    let stamp = g.weapon_model_stamp();
    if last.stamp == Some(stamp) {
        return;
    }
    last.stamp = Some(stamp);
    last.accept(g.weapon_model_changes(), &mut aw.actors);
}

#[cfg(test)]
mod tests;
