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

/// Stages done a [`HostQuestState`] carries at most: the highest ones.
pub const HOST_STAGES_DONE: usize = 32;

/// The host's story as a co-op guest's world reads it ([`Game::coop_host_story`]).
///
/// A guest leaves the story to the host: its own game's quests stay where its save had
/// them (it never talks to the quest NPCs), so a door or activator whose script asks
/// the story found the guest's own quest instead of the host's. Long 15's gate at the
/// Mojave Outpost opens only once `ALLong15EntranceQuest.iPapersPlease` is 1, set by
/// talking to the guard; the host's was, the guest's never is, and the guest's game ran
/// the script's other branch, the guard's conversation, which no script starts with a
/// guest: the gate did nothing [observed: owner's ship17 co-op, "guest cant use gate to
/// walk i15", host log 2026-11-13 03:39:17; the script's text in Long15Reloaded.esm
/// SCPT 0x1E0025C2].
///
/// - **What it holds.** Each quest the host's game has a state for (its stage, the
///   highest [`HOST_STAGES_DONE`] stages done, running, ended), and every variable of the
///   quests' scripts, as the host's game has them now ([`Game::story_view`]). nvforever's
///   `coop` sends them; a guest's game keeps the newest of each here.
/// - **Who reads it.** A script on a placed reference (a door, an activator, a trigger:
///   [`InstKey::Ref`]): `Quest.var`, GetStage, GetStageDone, GetQuestRunning and
///   GetQuestCompleted give the host's when it sent that quest [design]. The world's
///   references are the host's world's; a quest's own script (a mod's settings, the
///   guest's own story), a result script and a condition read the guest's own game, so
///   a guest's mod settings kept in quest variables stay its own [design].
/// - Nothing is written into the guest's quests: its own game, and its saves, keep them
///   [design: `save::KeepSolo`]. Not saved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostStory {
    pub quests: BTreeMap<FormId, HostQuestState>,
    /// Quest script variables: (quest, variable index) to value.
    pub vars: BTreeMap<(FormId, u16), fnv_script::vm::Value>,
}

/// One quest of the [`HostStory`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostQuestState {
    /// GetStage.
    pub stage: i32,
    pub running: bool,
    /// Completed or failed (GetQuestCompleted).
    pub ended: bool,
    /// The highest stages done, at most [`HOST_STAGES_DONE`], in order.
    pub done: Vec<i32>,
    /// Failed (a Fail Quest stage, FailQuest); `ended` too.
    pub failed: bool,
    /// Its objectives shown or done, by index: whether each is done. A guest's quest list
    /// and objectives are the host's from these (item 205).
    pub objectives: BTreeMap<i32, bool>,
}

impl HostQuestState {
    /// GetStageDone: from the stages carried; a stage below the lowest carried, when some
    /// were left out, counts as done if it is not above the stage reached [hypothesis:
    /// close enough for a gate's check; the cap only bites on quests of more than
    /// [`HOST_STAGES_DONE`] stages].
    pub fn stage_done(&self, stage: i32) -> bool {
        if self.done.contains(&stage) {
            return true;
        }
        self.done.len() >= HOST_STAGES_DONE && self.done.first().is_some_and(|&low| stage < low)
    }
}

/// ShowBarterMenu and ShowCaravanMenu (`crate::functions`).
const SHOW_BARTER_MENU: u16 = 0x1173;
const SHOW_CARAVAN_MENU: u16 = 0x125A;
/// The services a guest may also buy, as a companion's player would (owner, item 130
/// Q1): repair (ShowRepairMenu), crafting at a vendor (ShowRecipeMenu) and a doctor's
/// healing (ResetHealth) [observed: `schemas/script/functions.json` opcodes].
/// Random encounters every player sets off in co-op, not only the host (owner, item 131:
/// the first few, then "add all" 2026-11-09). In their scripts the player is any player
/// [design]:
/// - `player.GetDistance X` and `GetDistance player`: the nearest player's distance,
/// - `player.GetInCell X`: whether any player is in that cell,
/// - `StartCombat player`: the fight is with the player nearest the attacker,
/// - `OnTriggerEnter player`, `OnHit player`, `OnDeath player`: any player's.
///
/// The encounter still runs once, in the host's world (a guest's game runs no world
/// scripts), so each fires once per world. `player.HasPerk` and `player.GetItemCount` stay
/// the host's (VERT5AlienWWScript's Wild Wasteland check). FalloutNV.esm scripts
/// [observed: `fnvtool script decompile`, `fnvtool esm inventory`]:
/// - 0x00168CFD VERT1GeckoAmbushScript (Goodsprings geckos, player within 3000),
/// - 0x00168995 VERT1PrimmJackalAmbushScript (Primm jackals, within 4760),
/// - 0x00168993 VERT2IvanpahJackalAmbushScript (Ivanpah jackals, within 5500),
/// - 0x00156A96 VERT1CoyoteDenGSTriggerScript (Goodsprings coyote den trigger),
/// - 0x0015F18E / 0x0015F18D VERT2ViperSLTrigger01/02Script (Vipers' ambush triggers),
/// - 0x001024C4 FiendAmbushTriggerScript (Fiends' ambush trigger),
/// - 0x00168CFE VERT1GSRoachHuntedScript (OnDeath),
/// - 0x00165077 VERT2NiptonCaravanScript (player within 5000 of the caravan),
/// - 0x0016122A / 0x00164A2B / 0x0016122F VERT2ViperNipton, ViperCampNipton,
///   ViperNiptonTrigger01,
/// - 0x00173F94 / 0x00173F93 VERT2PrimmDenEnterTrigger, PrimmProspectorDenQuest
///   (player.GetInCell, player within 15000),
/// - 0x0015E8F9 / 0x0015E8FA / 0x0015E8FB VERT3BarkScorpionGulch and its Trigger01/02
///   (GetDistance player 4000, StartCombat player),
/// - 0x0015E8F6 / 0x0015E8F8 / 0x0015E8F7 VERT3CentaurBM and its Trigger01/02,
/// - 0x0015E8F4 / 0x0015E8F5 VERT4AntSV and its trigger,
/// - 0x0015675C / 0x0015675D / 0x0015675E / 0x0015675F VERT4NightStalkerDen01, its
///   Trigger1/2 and BabyScript (OnHit player, OnDeath player, StartCombat player),
/// - 0x00168994 VERT4BloodborneStalkerScript, 0x00166DDD VERT4WalkingBoxStalkerScript,
/// - 0x00166DDF / 0x00166DDE VERT5SilverPeakCazador and its Trigger1,
/// - 0x001672DE VERT5NopahFireGeckoScript,
/// - 0x00174364 VERT5AlienWWScript (GetDistance player 6000; Wild Wasteland only).
pub const COOP_ENCOUNTERS: [u32; 32] = [
    0x0016_8CFD, 0x0016_8995, 0x0016_8993, 0x0015_6A96, 0x0015_F18E, 0x0015_F18D, 0x0010_24C4,
    0x0016_8CFE,
    0x0016_5077,
    0x0016_122A, 0x0016_4A2B, 0x0016_122F,
    0x0017_3F94, 0x0017_3F93,
    0x0015_E8F9, 0x0015_E8FA, 0x0015_E8FB,
    0x0015_E8F6, 0x0015_E8F8, 0x0015_E8F7,
    0x0015_E8F4, 0x0015_E8F5,
    0x0015_675C, 0x0015_675D, 0x0015_675E, 0x0015_675F,
    0x0016_8994, 0x0016_6DDD,
    0x0016_6DDF, 0x0016_6DDE,
    0x0016_72DE,
    0x0017_4364,
];

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
        self.menu_time = self.menu_time.max(now.menu_time);
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
    /// On a guest: the host's quest view is `new` now (nvforever `coop`). The quest it
    /// showed before keeps its objectives, for that quest's page once it ended.
    pub fn coop_set_host_quest(&mut self, new: Option<HostQuest>) {
        if self.coop_host_quest == new {
            return;
        }
        if let Some(old) = self.coop_host_quest.take() {
            self.coop_host_objectives.insert(old.quest, old.objectives);
        }
        self.coop_host_quest = new;
    }

    /// On a guest: the host's `quest` ended, failed or completed (item 205). The guest's
    /// quest list shows it as done, as the host's own does ([`HostQuest`]). Whether this
    /// is news: a quest failed once is not told again; a completed quest that fails after
    /// is (a Fail Quest stage also reads as completed, `Game::set_stage`).
    pub fn coop_host_quest_ended(&mut self, quest: FormId, failed: bool) -> bool {
        match self.coop_host_ended.iter().position(|(q, _)| *q == quest) {
            Some(i) if self.coop_host_ended[i].1 || !failed => false,
            Some(i) => {
                self.coop_host_ended[i].1 = true;
                true
            }
            None => {
                self.coop_host_ended.push((quest, failed));
                true
            }
        }
    }

    /// The host's quest view and ended quests are forgotten: this game is no guest now.
    pub fn coop_forget_host_quests(&mut self) {
        self.coop_host_quest = None;
        self.coop_host_ended.clear();
        self.coop_host_objectives.clear();
    }

    /// The host's: its story now, for the guests ([`HostStory`]): each quest with a state
    /// (running, ended or with a stage done) and every variable of the quests' scripts.
    pub fn story_view(&self) -> HostStory {
        let quests = self
            .quests
            .iter()
            .filter(|(_, s)| s.running || s.completed || s.failed || !s.stages_done.is_empty())
            .map(|(q, s)| {
                let skip = s.stages_done.len().saturating_sub(HOST_STAGES_DONE);
                let objectives = s.objectives.iter().filter(|(_, o)| o.displayed || o.completed).map(|(i, o)| (*i, o.completed)).collect();
                (*q, HostQuestState { stage: s.stage(), running: s.running, ended: s.completed || s.failed, done: s.stages_done.iter().skip(skip).copied().collect(), failed: s.failed, objectives })
            })
            .collect();
        let vars = self
            .instances
            .iter()
            .filter_map(|(k, i)| match k {
                crate::game::InstKey::Quest { quest } => Some((*quest, i)),
                _ => None,
            })
            .flat_map(|(q, i)| i.vars.iter().map(move |(ix, v)| ((q, *ix), *v)))
            .collect();
        HostStory { quests, vars }
    }

    /// A guest's: the host's story for a script running as `inst`, when it reads the
    /// host's ([`HostStory`]: a guest's game, a placed reference's script).
    pub fn host_story(&self, inst: Option<crate::game::InstKey>) -> Option<&HostStory> {
        (self.coop_guest && matches!(inst, Some(crate::game::InstKey::Ref { .. }))).then_some(&self.coop_host_story)
    }

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

    /// `actor` is in a conversation with another player of the co-op game, not with this
    /// one ([`Game::coop_talking`] holds it, this game's own conversation does not): it
    /// offers no second conversation, "Wait for <host>" as a story NPC does (parity,
    /// 2026-11-14: guest B was offered "Talk" on Sarah Weintraub while the host talked
    /// to her) [design: one conversation per NPC].
    pub fn in_another_players_talk(&self, actor: FormId) -> bool {
        let actor = self.talker(actor).0;
        let own = self.dialogue.as_ref().map(|c| self.talker(c.actor).0);
        own != Some(actor) && self.coop_talking.iter().any(|t| self.talker(*t).0 == actor)
    }

    /// A guest: `actor`, dead in this game and alive in the host's world, stands up again
    /// here (nvforever `coop::place_like_host`), and its death is taken back while in the
    /// host's world ([`Game::coop_deaths_taken_back`]), so GetDeadCount reads as it does
    /// in the world being played: guest B, whose own save had killed the Goodsprings
    /// Powder Gangers, marked none of VMS16 objective 70's targets, each conditioned on
    /// GetDeadCount of its ganger's base being 0 [observed: FalloutNV.esm QUST VMS16
    /// QSTA/CTDA; run 2026-11-14]. The count itself stays, for the guest's own game. A
    /// script's ResurrectActor keeps the count [hypothesis].
    pub fn coop_revive(&mut self, actor: FormId) {
        if let Some(b) = crate::functions::base_of(self, actor) {
            let counted = self.player.dead_count.get(&b).copied().unwrap_or(0);
            let back = self.coop_deaths_taken_back.entry(b).or_default();
            if *back < counted {
                *back += 1;
            }
        }
        self.resurrect(actor, false);
    }

    /// The deaths of `base` GetDeadCount reads: the player's count less those a co-op
    /// guest took back ([`Game::coop_revive`]).
    pub fn dead_count(&self, base: FormId) -> u32 {
        let counted = self.player.dead_count.get(&base).copied().unwrap_or(0);
        counted.saturating_sub(self.coop_deaths_taken_back.get(&base).copied().unwrap_or(0))
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
    /// story gave stays with the guest ([`Game::coop_story_gear`]). An item the guest
    /// already wears is its own and is left as it is, on and off ([`Game::coop_story_kept`]).
    /// `host` empty takes everything off (the guest left). Returns the items put on and
    /// taken off.
    pub fn follow_story_gear(&mut self, host: &[(FormId, bool)]) -> (Vec<FormId>, Vec<FormId>) {
        let me = FormId::PLAYER_REF;
        // An add-on checkpoint's gear only while this player's items are in it (item 211).
        let host: Vec<(FormId, bool)> = host.iter().copied().filter(|&(item, _)| self.coop_checkpoint_of(item) != CheckpointGear::Out).collect();
        let host = &host[..];
        let mut on = Vec::new();
        let wears = |g: &Self, item: FormId| g.world.peek(me).and_then(|s| s.inventory.as_ref()).is_some_and(|i| i.is_equipped(item));
        for &(item, locked) in host {
            if self.coop_story_gear.contains_key(&item) || self.coop_story_kept.contains(&item) {
                continue;
            }
            // Worn already (the guest's own Pip-Boy): its own, nothing to put on and
            // nothing to take off later.
            if wears(self, item) {
                self.coop_story_kept.insert(item);
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
        // The guest's own copy of an add-on checkpoint's gear the host's story took off
        // inside it goes too: NVDLC01MQ03 stage 100 takes the player's collar, whoever's
        // it is (item 211; tri9: guest B kept its own collar from its save).
        let dropped: Vec<FormId> = self.coop_story_kept.iter().copied().filter(|i| !host.iter().any(|(h, _)| h == i)).collect();
        for item in dropped {
            self.coop_story_kept.remove(&item);
            if matches!(self.coop_checkpoint_of(item), CheckpointGear::In(_)) {
                let _ = self.world.lock_equip(me, item, None);
                let _ = self.world.unequip(me, item);
                let _ = self.world.remove_item(me, item, 1);
                tracing::info!("co-op: {} taken from this player: the host's story took its own off inside the checkpoint", self.data.name(item));
            }
        }
        let gone: Vec<FormId> = self.coop_story_gear.keys().copied().filter(|i| !host.iter().any(|(h, _)| h == i)).collect();
        for &item in &gone {
            let added = self.coop_story_gear.remove(&item).unwrap_or(false);
            // An add-on checkpoint's gear the host's story took off inside it (Dead
            // Money's collar, NVDLC01MQ03 stage 100) goes from this player too (item 211).
            let mut checkpoint = false;
            for held in self.coop_checkpoint_gear.values_mut() {
                if let Some(i) = held.iter().position(|(h, _)| *h == item) {
                    held.remove(i);
                    checkpoint = true;
                }
            }
            let _ = self.world.lock_equip(me, item, None);
            let _ = self.world.unequip(me, item);
            if added || checkpoint {
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

    /// The player nearest `t`, the host's own or another player's actor, and its
    /// distance (`None`: `t` is nowhere, or no player is in its space).
    pub fn nearest_player(&self, t: FormId) -> Option<(FormId, f32)> {
        let (space, at) = self.space_pos(t)?;
        std::iter::once(FormId::PLAYER_REF)
            .chain(self.coop_actors.keys().copied())
            .filter_map(|p| self.space_pos(p).filter(|(s, _)| *s == space).map(|(_, q)| (p, (0..3).map(|i| (q[i] - at[i]).powi(2)).sum::<f32>().sqrt())))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The distance from `t` to the nearest player ([`Game::nearest_player`]).
    pub fn nearest_player_distance(&self, t: FormId) -> Option<f32> {
        self.nearest_player(t).map(|(_, d)| d)
    }

    /// Whether `is_in` holds for any player, the host's own or another player's actor.
    pub fn any_player(&self, mut is_in: impl FnMut(&Self, FormId) -> bool) -> bool {
        std::iter::once(FormId::PLAYER_REF).chain(self.coop_actors.keys().copied()).collect::<Vec<_>>().into_iter().any(|p| is_in(self, p))
    }

    /// Whether `actor` is a story NPC: one with a line of a story quest not yet completed
    /// or failed, or a line whose scripts set a stage of (or start) one ([`story_quest`]).
    /// Its speakers are found once, by the line's fixed speaker or its own GetIsID
    /// condition, as the coverage report finds them.
    pub fn story_npc(&mut self, actor: FormId) -> bool {
        let data = self.data;
        let base = crate::functions::base_of(self, actor);
        let speakers = self.story_speakers.get_or_insert_with(|| story_speakers(data));
        let quests: Vec<FormId> = [Some(actor), base].into_iter().flatten().filter_map(|k| speakers.get(&k)).flatten().copied().collect();
        quests.into_iter().any(|q| !self.quests.get(&q).is_some_and(|s| s.completed || s.failed))
    }

    /// The other players not within [`STORY_NEAR`] units of `actor` in its space.
    pub fn players_far_from(&self, actor: FormId) -> Vec<FormId> {
        let Some((space, at)) = self.space_pos(actor) else { return Vec::new() };
        self.coop_actors
            .keys()
            .copied()
            .filter(|p| !self.space_pos(*p).is_some_and(|(s, q)| s == space && (0..3).map(|i| (q[i] - at[i]).powi(2)).sum::<f32>() <= STORY_NEAR * STORY_NEAR))
            .collect()
    }

    /// A co-op host's talk with a story NPC waits until every other player is near it
    /// (owner, 2026-11-09: "testing host not being able to talk to important NPCs when
    /// guests are too far"; story NPCs, within sight): true while one is not, with the
    /// wait put in [`Game::coop_story_wait`] for the host's screen and the far players'
    /// prompts. Asked when the host's player starts a conversation and when an NPC's
    /// Dialogue package would force-greet it (`fnv_ai`); a script's StartConversation is
    /// not held (its scene goes on). No gate for a guest's game or a game alone; never a
    /// choice for the host (owner 2026-11-09: "It shouldn't be a choice the host should
    /// have to wait").
    pub fn story_gate(&mut self, actor: FormId) -> bool {
        if self.coop_guest || self.coop_actors.is_empty() {
            self.coop_story_wait = None;
            return false;
        }
        let (actor, _) = self.talker(actor);
        let far = self.players_far_from(actor);
        if far.is_empty() || !self.story_npc(actor) {
            if self.coop_story_wait.as_ref().is_some_and(|w| w.actor == actor) {
                self.coop_story_wait = None;
            }
            return false;
        }
        self.coop_story_wait = Some(StoryWait { actor, far, at: self.time });
        true
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

/// How near a story NPC every player must be for the host to talk to it, and how near the
/// host a guest must be for a script's move of the host to take it along (owner,
/// 2026-11-09: "within sight") [design: about the distance the HUD's compass shows].
pub const STORY_NEAR: f32 = 2000.0;

/// A co-op host's talk held for the other players ([`Game::story_gate`]).
#[derive(Debug, Clone, PartialEq)]
pub struct StoryWait {
    /// The story NPC.
    pub actor: FormId,
    /// The other players' actors that are not near it.
    pub far: Vec<FormId>,
    /// When the talk was last asked for ([`Game::time`]).
    pub at: f64,
}

/// A story quest: the main quest's (by its Editor ID prefixes in FalloutNV.esm, as the
/// coverage report ranks them) or a DLC's main quest (NVDLC01MQ… to NVDLC04MQ…)
/// [observed: the Editor IDs of the four story add-ons' main quests].
pub(crate) fn story_quest(data: &GameData, q: FormId) -> bool {
    let Some(e) = data.edid_of.get(&q) else { return false };
    let e = e.to_ascii_lowercase();
    let plugin = data.load_order.get((q.0 >> 24) as usize).map(|p| p.to_ascii_lowercase()).unwrap_or_default();
    (plugin == "falloutnv.esm" && crate::coverage::MAIN_PREFIXES.iter().any(|p| e.starts_with(p))) || dlc_main_quest(&e)
}

/// NVDLC0nMQ…: an add-on's main quest.
fn dlc_main_quest(edid: &str) -> bool {
    edid.len() > 9 && edid.starts_with("nvdlc0") && edid.as_bytes()[6].is_ascii_digit() && &edid[7..9] == "mq"
}

/// The quests a compiled script names: the quests in its reference list (SetStage,
/// StartQuest, a quest variable set or read). A unit without compiled data falls back on
/// its source, the Editor ID after a SetStage or StartQuest [observed: Honest Hearts' Jed
/// greeting, line 0x0100F195, starts NVDLC02MQ01 by its reference list; the source scan
/// alone left Jed out, laptop run story-far 2026-11-09].
fn unit_quests(data: &GameData, unit: &fnv_script::unit::ScriptUnit) -> Vec<FormId> {
    if unit.has_scda {
        return unit
            .refs
            .iter()
            .copied()
            .filter_map(|r| match r {
                fnv_script::unit::RefSlot::Form(f) if data.is_quest(FormId(f)) => Some(FormId(f)),
                _ => None,
            })
            .collect();
    }
    unit.source.as_deref().map(|src| quests_named(data, src)).unwrap_or_default()
}

/// The quests a script's source sets a stage of or starts (SetStage, StartQuest), by the
/// Editor ID after the call.
fn quests_named(data: &GameData, source: &str) -> Vec<FormId> {
    let low = source.to_ascii_lowercase();
    let mut out = Vec::new();
    for call in ["setstage", "startquest"] {
        for (i, _) in low.match_indices(call) {
            let rest = low[i + call.len()..].trim_start();
            let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            if let Some(q) = data.by_edid(&name) {
                out.push(q);
            }
        }
    }
    out
}

/// Every story quest's speakers ([`Game::story_npc`]).
fn story_speakers(data: &GameData) -> std::collections::HashMap<FormId, Vec<FormId>> {
    let mut out: std::collections::HashMap<FormId, Vec<FormId>> = std::collections::HashMap::new();
    for info in data.infos.values() {
        let mut quests: Vec<FormId> = info.quest.into_iter().filter(|q| story_quest(data, *q)).collect();
        for u in [info.begin, info.end].into_iter().flatten() {
            quests.extend(unit_quests(data, &data.unit(u).unit).into_iter().filter(|q| story_quest(data, *q)));
        }
        if quests.is_empty() {
            continue;
        }
        let Some(speaker) = info.speaker.or_else(|| crate::coverage::line_actor(data, info)) else { continue };
        let list = out.entry(speaker).or_default();
        for q in quests {
            if !list.contains(&q) {
                list.push(q);
            }
        }
    }
    out
}

/// A co-op guest at the host's checkpoint (playtest 119 item 149; `Game::coop_checked`):
/// a Strip casino's weapon check took the host's player's weapons into a container, all
/// but those in an exception list (the guard's dialogue result, `RemoveAllTypedItems`
/// with type 40), and the door that lets the player out gives them back (the container's
/// `RemoveAllItems player`) [observed: FalloutNV.esm INFO 0x0012793B / 0x0012793F and
/// the Gomorrah door script SCPT 0x00127959, fnvtool decompile]. The Fort's gate takes
/// them by several such calls (types 40 and 47, each with its own list) and gives them
/// back by the container's `RemoveAllTypedItems player` [observed: SCPT 0x00164763]
/// (item 177). A guest's own game runs
/// neither, so it hands over and takes back its own as the host's player does.
impl Game<'_> {
    /// Hand over this player's items of form type `code` (-1: all), but those in
    /// `except` (a form list), to checkpoint `container`; what it took, counted.
    pub fn coop_check_in(&mut self, container: FormId, code: i32, except: Option<FormId>) -> i32 {
        if code == 0 {
            return 0;
        }
        let except = except.map(|l| crate::functions::refs::list_members(self, l)).unwrap_or_default();
        // In the checkpoint even with nothing to hand over (`Game::coop_checkpoint_of`).
        self.coop_checked.entry(container).or_default();
        let forms = self.world.forms;
        let stacks: Vec<(FormId, i32)> = self.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map(|i| i.iter().collect()).unwrap_or_default();
        let mut took = 0;
        for (item, n) in stacks {
            let Some(it) = forms.item(item) else { continue };
            if it.non_playable_biped() || except.contains(&item) || self.world.is_quest_item(item) {
                continue;
            }
            if code != -1 && forms.sig(item).as_ref().and_then(crate::functions::items::form_type_code) != Some(code) {
                continue;
            }
            let Ok(inv) = self.world.inventory_mut(FormId::PLAYER_REF) else { continue };
            let groups = fnv_sim::inventory::unequipped(inv.take(item, n));
            let held = self.coop_checked.entry(container).or_default();
            for (extra, c) in groups {
                took += c;
                held.push((item, c, extra));
            }
        }
        took
    }

    /// Give this player back what checkpoint `container` holds of its; how many. The
    /// checkpoint's gear it was handed meanwhile goes first ([`Game::coop_checkpoint_of`]).
    pub fn coop_check_out(&mut self, container: FormId) -> i32 {
        let me = FormId::PLAYER_REF;
        // What the checkpoint's story handed this player and the host's story did not take
        // back is its own now, as the original leaves it with the host (item 211).
        let gear = self.coop_checkpoint_gear.remove(&container).unwrap_or_default();
        for (item, _) in gear {
            if self.coop_story_gear.remove(&item).is_some() {
                let _ = self.world.lock_equip(me, item, None);
            }
            if self.world.item_count(me, item).unwrap_or(0) > 0 {
                tracing::info!("co-op: {} is this player's now: the checkpoint {container} gives its items back", self.data.name(item));
            }
        }
        let Some(held) = self.coop_checked.remove(&container) else { return 0 };
        let mut gave = 0;
        for (item, c, extra) in held {
            if self.world.add_item_extra(FormId::PLAYER_REF, item, c, extra).is_ok() {
                gave += c;
            }
        }
        gave
    }
}

/// Where an add-on checkpoint's gear may go (item 211; owner, 2026-11-14, via parity:
/// guests were seen in Dead Money's gear).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointGear {
    /// Not an add-on checkpoint's: it goes anywhere.
    Free,
    /// The checkpoint of this container holds this player's items now: it goes, and is
    /// taken away again when the checkpoint gives them back.
    In(FormId),
    /// An add-on checkpoint's, and this player's items are not in it: it does not go.
    Out,
}

/// The containers the load order's scripts put the player's whole inventory in: a
/// `player.RemoveAllItems <container>` call (Dead Money's bunker, SCPT 0x0100C59E into
/// NVDLC01PlayerEquipmentContainerRef 0x0100EFCC; FalloutNV.esm SCPT 0x0015E57A and INFO
/// 0x000E2849 into 0x000E3669) [observed: `fnvtool script decompile`].
pub fn strip_containers_of(data: &GameData) -> BTreeSet<FormId> {
    use fnv_script::ir::{Arg, Stmt};
    use fnv_script::unit::RefSlot;
    let remove_all = data.table.by_name("RemoveAllItems");
    let mut out = BTreeSet::new();
    for c in &data.units {
        c.program.walk(&mut |s| {
            let Stmt::Call { call } = s else { return };
            if Some(call.opcode) != remove_all || !matches!(call.on.and_then(|r| c.unit.slot(r)), Some(RefSlot::Form(f)) if f == FormId::PLAYER_REF.0) {
                return;
            }
            if let Some(Arg::Ref(r)) = call.args.first() {
                if let Some(RefSlot::Form(f)) = c.unit.slot(*r) {
                    out.insert(FormId(f));
                }
            }
        });
    }
    out
}

/// An add-on whose story takes the player's whole inventory into a container (Dead
/// Money's Sierra Madre) owns the gear its story hands out there: its quests' hand-outs
/// and the story gear of its own forms (the collar, the jumpsuit, the holorifle and its
/// cells, QUST 0x01000FBF stage 5) [design, item 211]. In the original that gear is the
/// player's from the bunker on, where the player's own items are taken (SCPT
/// 0x0100C59E), to the way out, where NVDLC01MQ03 stage 100 takes the collar and the
/// container gives the player's items back (SCPT 0x01013FB4) [observed: `fnvtool script
/// decompile`]; nothing else on the way out takes the jumpsuit, the holorifle, the cells
/// or the Sierra Madre chips (0x01011236; only dialogue lines that exchange chips take
/// them) [observed: the same]. So a co-op guest gets that gear only while that
/// checkpoint holds its own items (`Game::coop_check_in`, item 182), loses what the
/// host's story takes off inside (the collar), and keeps the rest as its own when the
/// checkpoint gives its items back, as the host does (owner, 2026-11-14, via parity: the
/// original wins). A guest that joins later, or a host that set the stage outside (a
/// console `SetStage NVDLC01MQ01 5` in Goodsprings), hands it to no guest.
/// The base game's own such container (0x000E3669) makes no gear of its quests.
impl Game<'_> {
    /// Where `form` (a hand-out's quest, or an item of the host's story gear) may go on
    /// this player ([`CheckpointGear`]).
    pub fn coop_checkpoint_of(&mut self, form: FormId) -> CheckpointGear {
        let data = self.data;
        let plugin = form.0 >> 24;
        if plugin == 0 || form.0 >= 0xFF00_0000 {
            return CheckpointGear::Free;
        }
        let strip = self.strip_containers.get_or_insert_with(|| strip_containers_of(data));
        if !strip.iter().any(|c| c.0 >> 24 == plugin) {
            return CheckpointGear::Free;
        }
        match self.coop_checked.keys().find(|c| c.0 >> 24 == plugin && strip.contains(c)) {
            Some(c) => CheckpointGear::In(*c),
            None => CheckpointGear::Out,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn each_coop_encounter_script_is_listed_once() {
        let mut v = super::COOP_ENCOUNTERS.to_vec();
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), super::COOP_ENCOUNTERS.len());
    }

    #[test]
    fn the_hosts_stages_done_are_read_from_what_came() {
        use super::{HostQuestState, HOST_STAGES_DONE};
        let few = HostQuestState { stage: 20, running: true, ended: false, done: vec![10, 20], ..Default::default() };
        assert!(few.stage_done(10) && few.stage_done(20));
        assert!(!few.stage_done(5) && !few.stage_done(30));
        // A quest of more stages than came: the ones below the lowest carried were done.
        let many = HostQuestState { stage: 400, running: true, ended: false, done: (0..HOST_STAGES_DONE as i32).map(|i| 100 + 10 * i).collect(), ..Default::default() };
        assert!(many.stage_done(50));
        assert!(many.stage_done(110));
        assert!(!many.stage_done(105));
    }
}
