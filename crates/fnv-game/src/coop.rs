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
/// The services a guest may also buy, as a companion's player would (owner, item 130
/// Q1): repair (ShowRepairMenu), crafting at a vendor (ShowRecipeMenu) and a doctor's
/// healing (ResetHealth) [observed: `schemas/script/functions.json` opcodes].
/// Random encounters every player sets off in co-op, not only the host (owner, item 131;
/// the first few, 2026-11-09): their scripts' `player.GetDistance` measures to the nearest
/// player and their `OnTriggerEnter player` takes any player [design]. FalloutNV.esm
/// scripts [observed: `fnvtool script decompile`]:
/// - 0x00168CFD VERT1GeckoAmbushScript (Goodsprings geckos, player within 3000),
/// - 0x00168995 VERT1PrimmJackalAmbushScript (Primm jackals, within 4760),
/// - 0x00168993 VERT2IvanpahJackalAmbushScript (Ivanpah jackals, within 5500),
/// - 0x00156A96 VERT1CoyoteDenGSTriggerScript (Goodsprings coyote den trigger),
/// - 0x0015F18E / 0x0015F18D VERT2ViperSLTrigger01/02Script (Vipers' ambush triggers),
/// - 0x001024C4 FiendAmbushTriggerScript (Fiends' ambush trigger).
pub const COOP_ENCOUNTERS: [u32; 7] = [0x0016_8CFD, 0x0016_8995, 0x0016_8993, 0x0015_6A96, 0x0015_F18E, 0x0015_F18D, 0x0010_24C4];

const SHOW_REPAIR_MENU: u16 = 0x117A;
const SHOW_RECIPE_MENU: u16 = 0x1250;
const RESET_HEALTH: u16 = 0x1150;
/// AddItem and RemoveItem (`crate::functions`).
const ADD_ITEM: u16 = 0x1002;
const REMOVE_ITEM: u16 = 0x1052;

/// Whether a script changes chips for something else: it both adds and removes items, and
/// one of those items is a casino chip (CHIP). That is a casino cashier's exchange, in
/// either direction [observed: FalloutNV.esm vDialogueCasinoCashier, the 17 lines that
/// buy chips for caps or NCR or Legion money and cash them in, at every casino by
/// faction; the lines that only hand out free chips, Benny's and Gomorrah's comps, add
/// without removing]. Mod casinos' chips are CHIP records too.
fn exchanges_chips(data: &GameData, unit: &crate::data::Compiled) -> bool {
    let (mut adds, mut removes, mut chip) = (false, false, false);
    unit.program.walk(&mut |s| {
        let fnv_script::ir::Stmt::Call { call } = s else { return };
        if call.opcode != ADD_ITEM && call.opcode != REMOVE_ITEM {
            return;
        }
        adds |= call.opcode == ADD_ITEM;
        removes |= call.opcode == REMOVE_ITEM;
        let item = match call.args.first() {
            Some(fnv_script::ir::Arg::Ref(r)) => unit.unit.slot(*r),
            _ => None,
        };
        if let Some(fnv_script::unit::RefSlot::Form(f)) = item {
            chip |= data.sig(FormId(f)) == Some(*b"CHIP");
        }
    });
    adds && removes && chip
}

/// [`Game::guest_lines`]: the lines that trade, deal Caravan or exchange a casino's
/// chips, then, until nothing more is added, the lines whose choices (TCLT) offer a topic
/// with such a line.
fn guest_lines(data: &GameData) -> BTreeSet<FormId> {
    let calls = |u: crate::data::UnitId| {
        let mut n = BTreeMap::new();
        crate::intro::calls(&data.unit(u).program, &mut n);
        [SHOW_BARTER_MENU, SHOW_CARAVAN_MENU, SHOW_REPAIR_MENU, SHOW_RECIPE_MENU, RESET_HEALTH].iter().any(|o| n.contains_key(o)) || exchanges_chips(data, data.unit(u))
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
    /// result scripts open the barter menu, deal a game of Caravan or exchange a casino's
    /// chips (a cashier, `exchanges_chips`), and those whose choices lead to one [design:
    /// a guest trades, plays and buys or cashes in chips, and leaves the quests to the
    /// host; owner 2026-11-05: the cashiers, nothing more].
    pub fn guest_lines(&self) -> &BTreeSet<FormId> {
        self.guest_lines.get_or_init(|| guest_lines(self.data))
    }

    /// A guest: `actor` is a target of the host's active quest's open objectives, so the
    /// quest is on it and the guest leaves it to the host entirely ("Wait for <host>";
    /// owner, item 130 Q1) [design: guests are the host's companions].
    pub fn guest_quest_locked(&mut self, actor: FormId) -> bool {
        if !self.coop_guest {
            return false;
        }
        let Some(q) = self.coop_host_quest.as_ref().map(|h| h.quest) else { return false };
        self.check_quest_targets(q).iter().any(|t| t.target == actor)
    }

    /// Whether a guest's conversation with `actor` would offer anything but Goodbye.
    pub fn guest_can_talk(&mut self, actor: FormId) -> bool {
        let Some(greeting) = self.data.greeting else { return false };
        let goodbye = self.data.goodbye;
        self.guest_opening(actor, greeting).iter().any(|c| Some(c.topic) != goodbye)
    }

    /// What a guest's conversation with `actor` opens on: the choices `topic` (the
    /// greeting) offers by itself, and before them those of the line of it the actor would
    /// say, the first it can say (no random draw), whose words the guest does not hear.
    /// Many vendors offer their trade only there: Genaro's "Need some grub?" greeting (INFO
    /// 0012936A) lists the topic whose line opens his barter menu (0012936A's choices,
    /// VFreeformFreesideVFSGenaroTopic000) [observed: FalloutNV.esm], and no top-level topic
    /// does, so a guest was offered nothing (queue 2026-11-07 items 31, 39, 47).
    pub(crate) fn guest_opening(&mut self, actor: FormId, topic: FormId) -> Vec<crate::dialogue::Choice> {
        let data = self.data;
        let own = self.build_choices(actor, topic, &InfoDef::default());
        let said = data.lines_tried(topic).iter().copied().find(|&i| self.can_say(actor, Some(FormId::PLAYER_REF), i));
        let Some(def) = said.and_then(|i| data.infos.get(&i)).filter(|d| !d.choices.is_empty()) else { return own };
        let mut out = self.build_choices(actor, topic, def);
        for c in own {
            if !out.iter().any(|o| o.info == c.info || o.topic == c.topic) {
                out.push(c);
            }
        }
        // Goodbye last.
        let goodbye = data.goodbye;
        out.sort_by_key(|c| Some(c.topic) == goodbye);
        out
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

    /// What scripts put on the player that it still wears ([`Game::script_worn`]), with
    /// whether locked on: a co-op host's story gear, for the guests to wear too.
    pub fn story_gear(&self) -> Vec<(FormId, bool)> {
        let worn = |item: FormId| self.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).is_some_and(|i| i.is_equipped(item));
        self.script_worn.iter().filter(|(item, _)| worn(**item)).map(|(item, locked)| (*item, *locked)).collect()
    }

    /// A co-op guest wears what the host's story put on the host's player (`host`, the
    /// host's [`Game::story_gear`]), and takes off what it no longer wears: a DLC start
    /// dresses every player alike (overnight item 84: Dead Money's collar and jumpsuit,
    /// QUST 0x01000FBF stage 5; Old World Blues' patient gown, QUST 0x03002FCB stage 10)
    /// [observed: `fnvtool script decompile`; design: the owner's rule]. An item the
    /// player lacks is added (as AddItem gives it, 99.9% health); a locked one is locked
    /// on here too. Taken off, an added item is taken away again, so nothing the host's
    /// story gave stays with the guest ([`Game::coop_story_gear`]). `host` empty takes
    /// everything off (the guest left). Returns the items put on and taken off.
    pub fn follow_story_gear(&mut self, host: &[(FormId, bool)]) -> (Vec<FormId>, Vec<FormId>) {
        let me = FormId::PLAYER_REF;
        let mut on = Vec::new();
        for &(item, locked) in host {
            if self.coop_story_gear.contains_key(&item) {
                continue;
            }
            let had = self.world.item_count(me, item).unwrap_or(0) > 0;
            if !had && self.world.add_item_at_condition(me, item, 1, Some(0.999)).is_err() {
                continue;
            }
            let _ = self.world.lock_equip(me, item, None);
            if self.world.equip(me, item).is_ok() {
                if locked {
                    let _ = self.world.lock_equip(me, item, Some(fnv_sim::world::EquipLock::NoUnequip));
                }
                on.push(item);
            }
            self.coop_story_gear.insert(item, !had);
        }
        let gone: Vec<FormId> = self.coop_story_gear.keys().copied().filter(|i| !host.iter().any(|(h, _)| h == i)).collect();
        for &item in &gone {
            let added = self.coop_story_gear.remove(&item).unwrap_or(false);
            let _ = self.world.lock_equip(me, item, None);
            let _ = self.world.unequip(me, item);
            if added {
                let _ = self.world.remove_item(me, item, 1);
            }
        }
        if !on.is_empty() || !gone.is_empty() {
            self.sync_actor_effects(me);
        }
        (on, gone)
    }

    /// Whether `event` on `r`'s own script is left to the host's world in a co-op
    /// guest's game: `r` is an NPC or creature placed in the world (ACHR, ACRE), not the
    /// player's teammate, and the block is not OnActivate (the guest's own use of it).
    /// The host's world runs its NPCs (a guest's actors' AI is off and they follow the
    /// host's, nvforever `coop`); their scripts' GameMode, OnDeath, OnHit and the rest
    /// run there, on the host's story. Run here as well they act on the guest's own
    /// story: Jed's GameMode (Honest Hearts SCPT 0x01009F67) shows "Jed's Dead, Baby!"
    /// and fails Happy Trails (stage 255) when he is dead before its stage 100, and in a
    /// guest's game that has not taken the trip the host's ambush is exactly that
    /// (overnight item 68, 2026-11-07) [observed: `fnvtool script decompile`; design].
    pub fn host_runs_script_of(&self, r: FormId, event: u16) -> bool {
        self.coop_guest
            && event != crate::game::event::ON_ACTIVATE
            && r != FormId::PLAYER_REF
            && self.world.placed.get(r).is_some_and(|p| &p.sig == b"ACHR" || &p.sig == b"ACRE")
            && !self.is_teammate(r)
    }

    /// `r`'s script is one of the random encounters every player sets off ([`COOP_ENCOUNTERS`]),
    /// with other players in the game.
    pub fn coop_encounter(&self, r: FormId) -> bool {
        !self.coop_actors.is_empty() && self.ref_script(r).is_some_and(|s| COOP_ENCOUNTERS.contains(&s.0))
    }

    /// Where `r` is: its cell's space (worldspace or interior) and position.
    fn space_pos(&self, r: FormId) -> Option<(FormId, [f32; 3])> {
        let s = self.world.peek(r)?;
        let c = s.cell?;
        Some((self.world.placed.world_of(c).unwrap_or(c), s.pos))
    }

    /// The distance from `t` to the nearest player, the host's own or another player's
    /// actor (`None`: `t` is nowhere).
    pub fn nearest_player_distance(&self, t: FormId) -> Option<f32> {
        let (space, at) = self.space_pos(t)?;
        std::iter::once(FormId::PLAYER_REF)
            .chain(self.coop_actors.keys().copied())
            .filter_map(|p| self.space_pos(p).filter(|(s, _)| *s == space).map(|(_, q)| (0..3).map(|i| (q[i] - at[i]).powi(2)).sum::<f32>().sqrt()))
            .min_by(f32::total_cmp)
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
