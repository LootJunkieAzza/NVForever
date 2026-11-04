//! The other players of a co-op game (docs/design/coop-save.md): what a save keeps of
//! players 2–4. Player 1 is the game's own player (`FormId::PLAYER_REF`, [`PlayerState`],
//! its `RefExtra`); each other player has a [`CoopPlayer`] holding the same kinds of state
//! for that character alone. The quests, the world and everything else the game saves
//! stay shared and are saved once, as before.
//!
//! This is the save format's groundwork: nothing plays these characters yet. The game
//! keeps them ([`Game::coop`]) so that a save loaded and saved again keeps them unchanged.

use crate::data::{GameData, InfoDef};
use crate::game::{ActiveEffect, Game, PlayerState, RefExtra};
use fnv_sim::world::RefState;
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Players in a co-op game, the game's own player included.
pub const MAX_PLAYERS: u8 = 4;
/// The first slot a [`CoopPlayer`] takes (slot 1 is the game's own player).
pub const FIRST_COOP_SLOT: u8 = 2;

/// What a co-op guest's quest list shows: the host's active quest ([`Game::coop_host_quest`]).
#[derive(Debug, Clone, PartialEq)]
pub struct HostQuest {
    pub quest: FormId,
    /// Its objectives shown or done, by index: whether each is done.
    pub objectives: BTreeMap<i32, bool>,
    pub completed: bool,
    pub failed: bool,
}

/// ShowBarterMenu and ShowCaravanMenu (`crate::functions`).
const SHOW_BARTER_MENU: u16 = 0x1173;
const SHOW_CARAVAN_MENU: u16 = 0x125A;

/// [`Game::guest_lines`]: the lines that trade or deal Caravan, then, until nothing more
/// is added, the lines whose choices (TCLT) offer a topic with such a line.
fn guest_lines(data: &GameData) -> BTreeSet<FormId> {
    let calls = |u: crate::data::UnitId| {
        let mut n = BTreeMap::new();
        crate::intro::calls(&data.unit(u).program, &mut n);
        n.contains_key(&SHOW_BARTER_MENU) || n.contains_key(&SHOW_CARAVAN_MENU)
    };
    let mut out: BTreeSet<FormId> = data.infos.iter().filter(|(_, d)| d.begin.into_iter().chain(d.end).any(calls)).map(|(i, _)| *i).collect();
    loop {
        let topics: BTreeSet<FormId> = out.iter().filter_map(|i| data.infos.get(i)?.topic).collect();
        let more: Vec<FormId> = data.infos.iter().filter(|(i, d)| !out.contains(i) && d.choices.iter().any(|t| topics.contains(t))).map(|(i, _)| *i).collect();
        if more.is_empty() {
            return out;
        }
        out.extend(more);
    }
}

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

impl Character {
    /// Put this character on the game's own player (the inverse of [`Character::of`]):
    /// the fields that are the character's; the rest of `p` (reputation, notes read,
    /// achievements and the like) stays as it is.
    pub fn put_on(&self, p: &mut PlayerState) {
        p.name = self.name.clone();
        p.female = self.female;
        p.look = self.look.clone();
        p.race = self.race;
        p.level = self.level;
        p.xp = self.xp;
        p.perks = self.perks.clone();
        p.tag_skills = self.tag_skills.clone();
        p.tag_slots = self.tag_slots.clone();
        p.skill_points = self.skill_points.clone();
        p.need_stages = self.need_stages.clone();
        p.misc_stats = self.misc_stats.clone();
        p.dead_count = self.dead_count.clone();
        p.notes = self.notes.clone();
        p.power_armor = self.power_armor;
        p.caravan = self.caravan.clone();
    }
}

impl crate::game::GameSnapshot {
    /// A co-op guest's save (private fork): this snapshot is the guest's own game as it
    /// was when it joined another player's world, and `now` is the game as it is in that
    /// world. The result is this game with the player's character as `now` has it (the
    /// actor's inventory, equipped items and S.P.E.C.I.A.L., health and every other
    /// actor value, level, XP, perks, skills, needs, statistics, notes, Caravan cards,
    /// the effects on the player, play time), and everything else as this one has it:
    /// the player's cell, position and rotation, the world, quests, globals and time.
    /// So a guest comes back to its own game where it left it, with what it gained.
    pub fn with_character_of(mut self, now: &crate::game::GameSnapshot) -> crate::game::GameSnapshot {
        let p = FormId::PLAYER_REF;
        if let Some(mut actor) = now.sim.refs.get(&p).cloned() {
            if let Some(old) = self.sim.refs.get(&p) {
                actor.cell = old.cell;
                actor.pos = old.pos;
                actor.rot = old.rot;
            }
            self.sim.refs.insert(p, actor);
        }
        match now.extra.get(&p) {
            Some(e) => {
                self.extra.insert(p, e.clone());
            }
            None => {
                self.extra.remove(&p);
            }
        }
        Character::of(&now.player).put_on(&mut self.player);
        // The player's effects as they are now, numbered after this game's own.
        self.effects.retain(|e| e.target != p);
        let mut next = self.next_effect.max(now.next_effect);
        for e in now.effects.iter().filter(|e| e.target == p) {
            let mut e = e.clone();
            e.id = next;
            next += 1;
            self.effects.push(e);
        }
        self.next_effect = next;
        self.time = self.time.max(now.time);
        self.sim.next_dynamic = self.sim.next_dynamic.max(now.sim.next_dynamic);
        self
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

    /// The lines a co-op guest may choose ([`Game::coop_guest`]), found once: those whose
    /// result scripts open the barter menu or deal a game of Caravan, and those whose
    /// choices lead to one [design: a guest trades and plays, and leaves the quests to
    /// the host].
    pub fn guest_lines(&self) -> &BTreeSet<FormId> {
        self.guest_lines.get_or_init(|| guest_lines(self.data))
    }

    /// Whether a guest's conversation with `actor` would offer anything but Goodbye.
    pub fn guest_can_talk(&mut self, actor: FormId) -> bool {
        let Some(greeting) = self.data.greeting else { return false };
        let goodbye = self.data.goodbye;
        self.build_choices(actor, greeting, &InfoDef::default()).iter().any(|c| Some(c.topic) != goodbye)
    }

    /// A guest's choices: only [`Game::guest_lines`], and Goodbye.
    pub(crate) fn guest_choices(&mut self, actor: FormId, mut out: Vec<crate::dialogue::Choice>) -> Vec<crate::dialogue::Choice> {
        let goodbye = self.data.goodbye;
        let lines = self.guest_lines();
        out.retain(|c| lines.contains(&c.info) || Some(c.topic) == goodbye);
        if let Some(bye) = goodbye.filter(|b| !out.iter().any(|c| c.topic == *b)) {
            out.extend(self.offer_topic(actor, bye));
        }
        out
    }

    /// Whether `r` is another player's actor in a co-op game being played
    /// ([`Game::coop_actors`]).
    pub fn is_coop_actor(&self, r: FormId) -> bool {
        self.coop_actors.contains_key(&r)
    }

    /// `r`, or the player for another player's actor: whom the world's actors take it
    /// for in friendship and enmity [design: the players are one side].
    pub fn as_player(&self, r: FormId) -> FormId {
        if self.is_coop_actor(r) {
            FormId::PLAYER_REF
        } else {
            r
        }
    }
}
