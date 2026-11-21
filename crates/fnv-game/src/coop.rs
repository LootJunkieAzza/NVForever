//! The other players of a co-op game (docs/design/coop-save.md): what a save keeps of
//! players 2–4. Player 1 is the game's own player (`FormId::PLAYER_REF`, [`PlayerState`],
//! its `RefExtra`); each other player has a [`CoopPlayer`] holding the same kinds of state
//! for that character alone. The quests, the world and everything else the game saves
//! stay shared and are saved once, as before.
//!
//! This is the save format's groundwork: nothing plays these characters yet. The game
//! keeps them ([`Game::coop`]) so that a save loaded and saved again keeps them unchanged.

use crate::game::{ActiveEffect, Game, PlayerState, RefExtra};
use fnv_sim::world::RefState;
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Players in a co-op game, the game's own player included.
pub const MAX_PLAYERS: u8 = 4;
/// The first slot a [`CoopPlayer`] takes (slot 1 is the game's own player).
pub const FIRST_COOP_SLOT: u8 = 2;

/// One of players 2–4 as saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoopPlayer {
    /// The player's number, [`FIRST_COOP_SLOT`]..=[`MAX_PLAYERS`].
    pub slot: u8,
    /// The character's actor as the simulation keeps the player's: base form, cell,
    /// position and rotation, inventory (equipped items and their condition), the
    /// S.P.E.C.I.A.L. the world holds, scale, equip locks.
    pub actor: RefState,
    /// Heading and view pitch, degrees (the player's are in the save header's place).
    #[serde(default)]
    pub heading_deg: f32,
    #[serde(default)]
    pub pitch_deg: f32,
    /// Health and every other actor value (base, permanent and temporary modifiers,
    /// damage), dead, unconscious, factions, level: what `Game::extra` holds for the
    /// player's reference.
    #[serde(default, skip_serializing_if = "is_default_extra")]
    pub extra: RefExtra,
    /// Name, look and progression ([`Character`]).
    pub character: Character,
    /// Magic effects active on the character (aid items, abilities, worn enchantments,
    /// radiation and need stages' effects). `target` is written as the game's own player
    /// was when the effect was taken; it names this character, not that player.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<ActiveEffect>,
}

fn is_default_extra(e: &RefExtra) -> bool {
    *e == RefExtra::default()
}

/// What [`PlayerState`] holds that belongs to one character and not to the shared game:
/// who the character is and how it has grown. The fields have [`PlayerState`]'s names
/// and meanings. Left shared (in [`PlayerState`] only): map markers and fast travel,
/// dialogue topics, reputation and crimes, factions the player is an enemy of,
/// challenges, achievements, casino winnings and bans, the controls scripts disabled,
/// hardcore, the character-generation flag, Pip-Boy display switches.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Character {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub female: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub look: Option<crate::chargen::Look>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub race: Option<FormId>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub level: Option<i32>,
    #[serde(default)]
    pub xp: f64,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub perks: BTreeMap<FormId, u8>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub tag_skills: BTreeSet<u16>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub tag_slots: BTreeMap<u8, u16>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub skill_points: BTreeMap<u16, f64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub need_stages: BTreeMap<u16, FormId>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub misc_stats: BTreeMap<u16, f64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub dead_count: BTreeMap<FormId, u32>,
    /// Notes and holotapes the character carries in its Pip-Boy.
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub notes: BTreeSet<FormId>,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub power_armor: bool,
    #[serde(skip_serializing_if = "crate::caravan::CaravanState::is_empty", default)]
    pub caravan: crate::caravan::CaravanState,
}

impl Character {
    /// The character part of the game's own player.
    pub fn of(p: &PlayerState) -> Character {
        Character {
            name: p.name.clone(),
            female: p.female,
            look: p.look.clone(),
            race: p.race,
            level: p.level,
            xp: p.xp,
            perks: p.perks.clone(),
            tag_skills: p.tag_skills.clone(),
            tag_slots: p.tag_slots.clone(),
            skill_points: p.skill_points.clone(),
            need_stages: p.need_stages.clone(),
            misc_stats: p.misc_stats.clone(),
            dead_count: p.dead_count.clone(),
            notes: p.notes.clone(),
            power_armor: p.power_armor,
            caravan: p.caravan.clone(),
        }
    }
}

impl CoopPlayer {
    /// Player `slot` as a copy of the game's own player now (its actor, values, effects
    /// and character); `None` before the player's actor exists in the world.
    pub fn copy_of_player(g: &Game, slot: u8) -> Option<CoopPlayer> {
        let actor = g.world.peek(FormId::PLAYER_REF)?.clone();
        Some(CoopPlayer {
            slot,
            actor,
            heading_deg: 0.0,
            pitch_deg: 0.0,
            extra: g.extra.get(&FormId::PLAYER_REF).cloned().unwrap_or_default(),
            character: Character::of(&g.player),
            effects: g.effects.iter().filter(|e| e.target == FormId::PLAYER_REF).cloned().collect(),
        })
    }
}

/// Why a list of co-op players cannot be kept.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoopError {
    #[error("co-op player slot {0}: slots are {FIRST_COOP_SLOT} to {MAX_PLAYERS}")]
    BadSlot(u8),
    #[error("co-op player slot {0} given twice")]
    Twice(u8),
}

/// Check a list of co-op players and put it in slot order.
pub fn checked(mut players: Vec<CoopPlayer>) -> Result<Vec<CoopPlayer>, CoopError> {
    players.sort_by_key(|p| p.slot);
    for (i, p) in players.iter().enumerate() {
        if !(FIRST_COOP_SLOT..=MAX_PLAYERS).contains(&p.slot) {
            return Err(CoopError::BadSlot(p.slot));
        }
        if i > 0 && players[i - 1].slot == p.slot {
            return Err(CoopError::Twice(p.slot));
        }
    }
    Ok(players)
}

impl Game<'_> {
    /// Player `slot` (2–4), when the game has one.
    pub fn coop_player(&self, slot: u8) -> Option<&CoopPlayer> {
        self.coop.iter().find(|p| p.slot == slot)
    }

    /// Put in (or replace) the player of `p.slot`.
    pub fn set_coop_player(&mut self, p: CoopPlayer) -> Result<(), CoopError> {
        let mut all: Vec<CoopPlayer> = self.coop.iter().filter(|q| q.slot != p.slot).cloned().collect();
        all.push(p);
        self.coop = checked(all)?;
        Ok(())
    }

    /// Take player `slot` out of the game.
    pub fn remove_coop_player(&mut self, slot: u8) -> Option<CoopPlayer> {
        let i = self.coop.iter().position(|p| p.slot == slot)?;
        Some(self.coop.remove(i))
    }
}
