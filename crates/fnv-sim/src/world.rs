//! Save-agnostic runtime state: references as they are now, stored as a delta over the
//! authored placement data.
//!
//! A reference that nothing has touched has no entry: its state is derived from its
//! `PlacedRef` when first needed ("materialised"). Only materialised references are
//! written to a [`Snapshot`], so a snapshot is the set of changes against the plugins,
//! independent of the original game's savegame format (spec Layer 4 "save-agnostic
//! game state"; savegame compatibility is Layer 7).
//!
//! Everything is deterministic: ordered maps, and one seeded RNG whose position is part
//! of the snapshot, so a run continued from a snapshot equals an uninterrupted run.

use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::clock::{self, Calendar};
use crate::form::FormId;
use crate::forms::{BaseData, FormStore, ItemKind, SPECIAL};
use crate::inventory::{unequipped, Extra, Inventory};
use crate::leveled::{self, LeveledError};
use crate::refs::{PlacedRef, RefTable};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SimConfig {
    pub seed: u64,
    /// Level used for leveled lists (the player's level). [observed in game: `AddItem` of
    /// a list resolves against the player's level (`L4.sim.leveled.basic`); that
    /// containers do too is hypothesis]
    pub player_level: u16,
    /// Hardcore mode: ammunition has weight. (Absent in older snapshots.)
    #[serde(default)]
    pub hardcore: bool,
    /// Game hours a cell must stay unloaded before its respawning containers refill.
    /// `None`: the `iHoursToRespawnCell` game setting when a plugin defines it, else
    /// [`DEFAULT_RESPAWN_HOURS`] (see [`World::respawn_hours`]).
    #[serde(default)]
    pub respawn_hours: Option<f32>,
}

/// Respawn delay when neither the config nor a plugin sets one: the engine's built-in
/// value of `iHoursToRespawnCell` [observed in game: `GetGameSetting
/// iHoursToRespawnCell` = 72.0; no master defines the setting]. Merchant containers
/// restock on a schedule of their own ([`is_merchant_restock_day`]).
pub const DEFAULT_RESPAWN_HOURS: f32 = 72.0;

/// The game setting that holds the cell-respawn delay [observed in game: the running
/// game answers `GetGameSetting iHoursToRespawnCell`].
pub const RESPAWN_SETTING: &str = "iHoursToRespawnCell";

/// Whether merchant containers restock at the midnight that starts GameDaysPassed `day`:
/// days that are 0 or 3 modulo 7, i.e. twice a week, 3 and 4 days apart.
/// [observed in game, Ringo's cash box 0x001629C0 in two saves
/// (docs/spec-notes/simulation.md §3): refills only ever came just after midnights 7, 10,
/// 14, 17, 21, 24 and 28, and an emptied box was passed over at midnights 8, 9, 13, 15,
/// 16, 18, 19, 20, 22, 23, 25, 26 and 27 — every other residue — whenever it had been
/// emptied and whatever its last restock. That the schedule is global (GameDaysPassed
/// modulo 7, the same for every merchant) rather than per container is hypothesis: both
/// saves agree, other merchants are not captured]
pub fn is_merchant_restock_day(day: i64) -> bool {
    matches!(day.rem_euclid(7), 0 | 3)
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig { seed: 0, player_level: 1, hardcore: false, respawn_hours: None }
    }
}

/// How far in front of the holder, along its heading, a dropped item is placed (game
/// units). [observed: one capture, the player dropping stimpaks]
pub const DROP_AHEAD: f32 = 50.0;
/// How far above the holder's position (its feet) a dropped item is placed. [observed:
/// same capture]
pub const DROP_ABOVE: f32 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockState {
    pub level: u8,
    pub key: Option<FormId>,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefState {
    pub base: FormId,
    pub cell: Option<FormId>,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub enabled: bool,
    /// Removed from the world (picked up, or a created reference deleted).
    pub deleted: bool,
    /// Stack size for item references; 1 otherwise.
    pub count: i32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub inventory: Option<Inventory>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub lock: Option<LockState>,
    pub open: bool,
    /// Item references: condition (XHLP on placed weapons and armour) and owner.
    #[serde(skip_serializing_if = "Extra::is_plain", default)]
    pub extra: Extra,
    /// Actor values changed at run time, by lower-case name (S.P.E.C.I.A.L. so far).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub actor_values: BTreeMap<String, f32>,
    /// Merchant containers: GameDaysPassed of the last restock, a restock day (`None`:
    /// not restocked since the simulation started).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_restock: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SimError {
    #[error("unknown reference {0}")]
    UnknownRef(FormId),
    #[error("unknown base form {0}")]
    UnknownForm(FormId),
    #[error("{0} has no inventory")]
    NoInventory(FormId),
    #[error("{0} is not an inventory item")]
    NotAnItem(FormId),
    #[error("count must be positive, got {0}")]
    BadCount(i32),
    #[error("{0} is not in the world (deleted)")]
    Deleted(FormId),
    #[error("{0} cannot be equipped")]
    NotEquippable(FormId),
    #[error("{actor} has no unequipped {item}")]
    NotCarried { actor: FormId, item: FormId },
    #[error("unknown actor value {0:?}")]
    UnknownActorValue(String),
    #[error("{0} has no actor values")]
    NoActorValues(FormId),
    #[error("the game setting {0} is not defined by any plugin")]
    MissingSetting(String),
    #[error("the global {0} is not defined by any plugin")]
    MissingGlobal(String),
    /// The player tried to drop a quest item from the inventory menu
    /// ([`World::player_drop_item`]) [documented: GECK wiki Category:Items and
    /// SetQuestObject: the player cannot drop or sell one].
    #[error("{0} is a quest item: the player cannot drop it")]
    QuestItem(FormId),
    /// Every unequipped unit of the weapon is at zero condition
    /// ([`World::equip`]) [design: the owner's rule, 2026-10-11].
    #[error("{0} is broken: it cannot be equipped until it is repaired")]
    Broken(FormId),
    #[error(transparent)]
    Leveled(#[from] LeveledError),
}

/// What activating a reference did. [hypothesis for the dispatch by base type:
/// follows the GECK wiki's description of default activation per object type;
/// scripted OnActivate blocks are Layer 5 and reported in `script` only]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Outcome {
    PickedUp { item: FormId, count: i32, owner: Option<FormId> },
    OpenedContainer { container: FormId },
    Locked { level: u8 },
    Teleported { to_door: FormId, cell: Option<FormId>, pos: [f32; 3], rot: [f32; 3] },
    Disarmed { item: FormId },
    DoorToggled { open: bool },
    Talk { actor: FormId },
    Activated,
    Disabled,
    NoDefault { sig: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Activation {
    #[serde(flatten)]
    pub outcome: Outcome,
    /// The lock was opened with this key from the actor's inventory first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlocked_with: Option<FormId>,
    /// The base form's script, which would receive OnActivate (Layer 5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<FormId>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Equip {
    /// Aid items are used up instead of worn.
    pub consumed: bool,
    /// Items unequipped to make room.
    pub unequipped: Vec<FormId>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Encumbrance {
    pub weight: f64,
    pub capacity: f64,
    pub over: bool,
    /// Carried items whose weight is unknown (not counted).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unknown_weight: Vec<FormId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub format: String,
    pub config: SimConfig,
    /// ChaCha8 word position, as a decimal string (u128 does not fit JSON numbers).
    pub rng_word_pos: String,
    pub next_dynamic: u32,
    pub refs: BTreeMap<FormId, RefState>,
    /// Globals whose value differs from the plugins (absent in older snapshots).
    #[serde(default)]
    pub globals: BTreeMap<FormId, f32>,
    /// Unloaded cells: GameDaysPassed when each was detached (absent in older snapshots).
    #[serde(default)]
    pub detached_cells: BTreeMap<FormId, f32>,
    /// Quest-item flags set by scripts (`SetQuestObject`), by base form; absent when
    /// none (and in older snapshots).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub quest_objects: BTreeMap<FormId, bool>,
}

pub const SNAPSHOT_FORMAT: &str = "fnv.sim.state/1";

pub struct World<'a> {
    pub forms: &'a FormStore,
    pub placed: &'a RefTable,
    cfg: SimConfig,
    refs: BTreeMap<FormId, RefState>,
    /// Globals set at run time (scripts, the clock); others keep their authored value.
    globals: BTreeMap<FormId, f32>,
    /// Unloaded cells and the GameDaysPassed at which they were unloaded.
    detached_cells: BTreeMap<FormId, f32>,
    /// Quest-item flags set at run time (`set_quest_object`); they replace the record's.
    quest_objects: BTreeMap<FormId, bool>,
    next_dynamic: u32,
    rng: ChaCha8Rng,
}

impl<'a> World<'a> {
    pub fn new(forms: &'a FormStore, placed: &'a RefTable, cfg: SimConfig) -> World<'a> {
        World {
            forms,
            placed,
            cfg,
            refs: BTreeMap::new(),
            globals: BTreeMap::new(),
            detached_cells: BTreeMap::new(),
            quest_objects: BTreeMap::new(),
            next_dynamic: FormId::FIRST_DYNAMIC.0,
            rng: ChaCha8Rng::seed_from_u64(cfg.seed),
        }
    }

    pub fn config(&self) -> SimConfig {
        self.cfg
    }

    /// State of a reference that has already been materialised.
    pub fn peek(&self, id: FormId) -> Option<&RefState> {
        self.refs.get(&id)
    }

    /// Materialised references in FormID order.
    pub fn changed(&self) -> impl Iterator<Item = (FormId, &RefState)> {
        self.refs.iter().map(|(&k, v)| (k, v))
    }

    /// The current state of `id`, creating it from the placement data on first use.
    pub fn get(&mut self, id: FormId) -> Result<&mut RefState, SimError> {
        if !self.refs.contains_key(&id) {
            let s = self.initial(id)?;
            self.refs.insert(id, s);
        }
        Ok(self.refs.get_mut(&id).expect("inserted above"))
    }

    fn initial(&mut self, id: FormId) -> Result<RefState, SimError> {
        let (base, placed): (FormId, Option<&PlacedRef>) = match self.placed.get(id) {
            Some(p) => (p.base, Some(p)),
            // The player is not placed by any shipped plugin. [hypothesis; reported
            // by `fnvtool sim forms`] It starts from its base form.
            None if id == FormId::PLAYER_REF => (FormId::PLAYER_BASE, None),
            None => return Err(SimError::UnknownRef(id)),
        };
        let mut inventory = self.roll_inventory(base)?;
        if id == FormId::PLAYER_REF {
            // The player wears the non-playable armour it carries: the Pip-Boy Glove and
            // the Pip-Boy 3000 that the base form NPC_ 0x7 lists. [observed in game,
            // docs/spec-notes/simulation.md §2: `GetEquipped` 1 for both in the saves
            // nvftest and nvfdoc (`cl_a_pipboy_*`); the owner never saw the glove
            // unequipped. A new game before the intro (nvfnew) holds neither: handing them
            // over is the intro's script, Layer 5]
            if let Some(inv) = inventory.as_mut() {
                for item in inv.iter().map(|(i, _)| i).collect::<Vec<_>>() {
                    if self.forms.item(item).is_some_and(|it| it.kind == ItemKind::Armor && it.non_playable) {
                        inv.set_equipped(item);
                    }
                }
            }
        }
        Ok(RefState {
            base,
            cell: placed.and_then(|p| p.cell),
            pos: placed.map(|p| p.pos).unwrap_or_default(),
            rot: placed.map(|p| p.rot).unwrap_or_default(),
            enabled: !placed.is_some_and(|p| p.initially_disabled()),
            deleted: false,
            count: placed.and_then(|p| p.count).unwrap_or(1),
            inventory,
            // A reference with lock data starts locked. [hypothesis]
            lock: placed.and_then(|p| p.lock).map(|l| LockState { level: l.level, key: l.key, locked: true }),
            // A door flagged Open by Default starts open. [hypothesis from the flag's
            // name; a gate so flagged let the player drop through it, physics.md §4]
            open: placed.is_some_and(|p| p.open_by_default),
            extra: Extra {
                condition: placed.and_then(|p| p.health),
                owner: placed.and_then(|p| p.owner),
                equipped: false,
            },
            actor_values: BTreeMap::new(),
            last_restock: None,
        })
    }

    /// A fresh inventory for a container or actor base: its carried items with leveled
    /// lists resolved. `None` for bases without carried items.
    fn roll_inventory(&mut self, base: FormId) -> Result<Option<Inventory>, SimError> {
        let Some(entries) = self.forms.entries(base) else { return Ok(None) };
        let mut inv = Inventory::new();
        let mut out = Vec::new();
        for e in entries {
            let from = out.len();
            // LVLG globals at their current values [observed: leveled_b8 captures].
            leveled::resolve_in(self.forms, &self.globals, e.item, e.count, self.cfg.player_level, &mut self.rng, &mut out)?;
            // COED on an entry applies to whatever it resolves to. [hypothesis]
            for r in &mut out[from..] {
                r.extra = e.extra;
            }
        }
        for e in out {
            if e.count > 0 {
                inv.add_extra(e.item, e.count, e.extra);
            }
        }
        Ok(Some(inv))
    }

    /// Whether `id` is a merchant container that restocks: an actor names it in XMRC and
    /// its base has the Respawns flag. [observed in game: Primm's merchant box 0x0017409C,
    /// whose base lacks the flag, never refilled in 7 days of daily reads
    /// (`bl_primm_restock`); Ringo's 0x001629C0, with the flag, did;
    /// docs/spec-notes/simulation.md §3]
    fn restocks(&self, id: FormId, base: FormId) -> bool {
        self.placed.is_merchant_container(id)
            && matches!(self.forms.get(base).map(|b| &b.data), Some(BaseData::Container(c)) if c.respawns)
    }

    /// Merchant restocks at the midnights in (`from`, `to`] (GameDaysPassed crossing a
    /// whole number): on a restock day ([`is_merchant_restock_day`]) every restocking
    /// merchant container ([`World::restocks`]) gets a fresh roll of its base's carried
    /// items (anything sold into it is lost). Contents do not change while time passes, so
    /// several restock days in one advance restock once, at the last of them. Containers
    /// never materialised are still as authored and need no visit.
    /// [observed in game, docs/spec-notes/simulation.md §3: refills land just after
    /// midnights, need no cell event, menu or access, and replace the contents (a stimpak
    /// put in was gone). Hypothesis: untouched containers are restocked too (invisible
    /// with Ringo's fixed 200 caps; a merchant with leveled stock would tell), and the
    /// day is the one GameDaysPassed starts — the authored clock's GameDaysPassed (5.0)
    /// and GameHour (12) disagree by half a day, while saves keep them together]
    fn restock_merchants(&mut self, from: f64, to: f64) -> Result<(), SimError> {
        let (first, last) = (from.floor() as i64 + 1, to.floor() as i64);
        let Some(day) = (first..=last).rev().take(7).find(|&d| is_merchant_restock_day(d)) else {
            return Ok(());
        };
        let due: Vec<(FormId, FormId)> = self
            .refs
            .iter()
            .filter(|&(&id, s)| !s.deleted && self.restocks(id, s.base))
            .map(|(&id, s)| (id, s.base))
            .collect();
        for (id, base) in due {
            let inv = self.roll_inventory(base)?;
            let s = self.get(id)?;
            s.inventory = inv;
            s.last_restock = Some(day as f32);
        }
        Ok(())
    }

    fn inventory(&mut self, id: FormId) -> Result<&mut Inventory, SimError> {
        let s = self.get(id)?;
        if s.deleted {
            return Err(SimError::Deleted(id));
        }
        s.inventory.as_mut().ok_or(SimError::NoInventory(id))
    }

    fn check_item(&self, item: FormId) -> Result<(), SimError> {
        match self.forms.item(item) {
            Some(_) => Ok(()),
            None if self.forms.sig(item).is_some() => Err(SimError::NotAnItem(item)),
            None => Err(SimError::UnknownForm(item)),
        }
    }

    pub fn item_count(&mut self, of: FormId, item: FormId) -> Result<i32, SimError> {
        Ok(self.inventory(of)?.count(item))
    }

    /// Add `count` of an item or leveled list; returns the items actually added.
    pub fn add_item(&mut self, to: FormId, item: FormId, count: i32) -> Result<Vec<(FormId, i32)>, SimError> {
        if count <= 0 {
            return Err(SimError::BadCount(count));
        }
        if self.forms.leveled(item).is_none() {
            self.check_item(item)?;
        }
        self.inventory(to)?;
        let mut out = Vec::new();
        leveled::resolve_in(self.forms, &self.globals, item, count, self.cfg.player_level, &mut self.rng, &mut out)?;
        let inv = self.inventory(to)?;
        Ok(out
            .into_iter()
            .filter(|e| e.count > 0)
            .map(|e| {
                inv.add(e.item, e.count);
                (e.item, e.count)
            })
            .collect())
    }

    /// Remove up to `count`; returns how many were removed.
    pub fn remove_item(&mut self, from: FormId, item: FormId, count: i32) -> Result<i32, SimError> {
        if count <= 0 {
            return Err(SimError::BadCount(count));
        }
        Ok(self.inventory(from)?.remove(item, count))
    }

    /// Move up to `count` from one inventory to another; returns how many moved. Quest
    /// items move like any other: the pages that forbid dropping and selling them say
    /// nothing about containers, so storing one is allowed [hypothesis; not captured].
    pub fn transfer(&mut self, from: FormId, to: FormId, item: FormId, count: i32) -> Result<i32, SimError> {
        if count <= 0 {
            return Err(SimError::BadCount(count));
        }
        self.inventory(to)?;
        let groups = unequipped(self.inventory(from)?.take(item, count));
        let dest = self.inventory(to)?;
        let mut n = 0;
        for (e, c) in groups {
            dest.add_extra(item, c, e);
            n += c;
        }
        Ok(n)
    }

    /// Whether `item` is a non-playable item (ARMO BMDT "Non-Playable").
    fn non_playable(forms: &FormStore, item: FormId) -> bool {
        forms.item(item).is_some_and(|it| it.non_playable)
    }

    /// Move everything but non-playable items (as [`World::remove_all_items`], the same
    /// script function with a target); returns the stacks moved. Extra data (condition,
    /// owner) moves with the items; equipped items are unequipped.
    pub fn transfer_all(&mut self, from: FormId, to: FormId) -> Result<Vec<(FormId, i32)>, SimError> {
        self.inventory(to)?;
        let forms = self.forms;
        let stacks: Vec<_> = self.inventory(from)?.iter().filter(|&(i, _)| !Self::non_playable(forms, i)).collect();
        for &(item, n) in &stacks {
            self.transfer(from, to, item, n)?;
        }
        Ok(stacks)
    }

    /// Empty an inventory except for non-playable items, which stay as they are
    /// (equipped ones stay equipped); returns the stacks removed. [observed in game,
    /// docs/spec-notes/simulation.md §3: `player.RemoveAllItems` left the Pip-Boy Glove
    /// and the Pip-Boy 3000, both still equipped, and nothing else (`cl_a_pipboy_nvftest`);
    /// that other non-playable items and other holders behave alike is hypothesis]
    pub fn remove_all_items(&mut self, from: FormId) -> Result<Vec<(FormId, i32)>, SimError> {
        let forms = self.forms;
        let inv = self.inventory(from)?;
        let keep: Vec<_> = inv.groups().filter(|&(i, _, _)| Self::non_playable(forms, i)).collect();
        let mut kept = Inventory::new();
        for &(item, extra, n) in &keep {
            kept.add_extra(item, n, extra);
        }
        let stacks: Vec<_> = inv.iter().filter(|&(i, _)| kept.count(i) == 0).collect();
        *inv = kept;
        Ok(stacks)
    }

    /// Whether `item` is a quest item now: the value a script last set with
    /// [`World::set_quest_object`], else the record's "Quest Item" header flag
    /// (`Item::quest_item`). One flag per base form: SetQuestObject sets "the" quest-item
    /// flag of the form [documented: GECK wiki SetQuestObject], the one GetFormFlag reads
    /// as bit 10 [documented: GECK wiki GetFormFlag]; that a script's value replaces the
    /// authored one for every reference is [hypothesis: the change is to the base form].
    pub fn is_quest_item(&self, item: FormId) -> bool {
        self.quest_objects.get(&item).copied().unwrap_or_else(|| self.forms.is_quest_item(item))
    }

    /// `SetQuestObject item flag`: set or clear the quest-item flag of a base form. It is
    /// kept in snapshots.
    pub fn set_quest_object(&mut self, item: FormId, quest: bool) {
        self.quest_objects.insert(item, quest);
    }

    /// Drop items from an inventory into the world, one new reference per group of
    /// identical extra data, in the holder's cell, [`DROP_AHEAD`] units in front of the
    /// holder along its heading and [`DROP_ABOVE`] units above its position. Returns the
    /// new references. [observed in game, docs/spec-notes/simulation.md §5: the player
    /// dropping 1, 2 and 3 stimpaks made one reference each (count 1, 2, 3) at exactly
    /// that offset, which did not move for 10 s; hypothesis for other actors and items]
    ///
    /// This is the mechanism scripts use (`Drop`, `DropMe`, the action scripts'
    /// `drop_item`): it ignores the quest-item flag, as the game's scripted `player.Drop`
    /// does [observed 2026-10-09, `qi_drop_quest_item`: a quest key and a stimpak marked
    /// with SetQuestObject both left the player's inventory]. The player's own drop is
    /// [`World::player_drop_item`].
    pub fn drop_item(&mut self, from: FormId, item: FormId, count: i32) -> Result<Vec<FormId>, SimError> {
        if count <= 0 {
            return Err(SimError::BadCount(count));
        }
        let groups = unequipped(self.inventory(from)?.take(item, count));
        self.drop_groups(from, item, groups)
    }

    /// Drop the equipped unit of `item` (a weapon that broke in an actor's hand), as
    /// [`World::drop_item`]: the new reference keeps its condition. Nothing is dropped
    /// when none is equipped.
    pub fn drop_equipped(&mut self, from: FormId, item: FormId) -> Result<Vec<FormId>, SimError> {
        let groups = unequipped(self.inventory(from)?.take_equipped(item));
        self.drop_groups(from, item, groups)
    }

    fn drop_groups(&mut self, from: FormId, item: FormId, groups: Vec<(Extra, i32)>) -> Result<Vec<FormId>, SimError> {
        let holder = self.get(from)?.clone();
        // A z rotation turns clockwise seen from above and 0 faces +Y (world.md §4), so
        // the heading's direction is (sin z, cos z).
        let (sin, cos) = holder.rot[2].sin_cos();
        let pos = [holder.pos[0] + DROP_AHEAD * sin, holder.pos[1] + DROP_AHEAD * cos, holder.pos[2] + DROP_ABOVE];
        let mut out = Vec::new();
        for (extra, n) in groups {
            let id = FormId(self.next_dynamic);
            self.next_dynamic += 1;
            self.refs.insert(
                id,
                RefState {
                    base: item,
                    cell: holder.cell,
                    pos,
                    rot: [0.0; 3],
                    enabled: true,
                    deleted: false,
                    count: n,
                    inventory: None,
                    lock: None,
                    open: false,
                    extra,
                    actor_values: BTreeMap::new(),
                    last_restock: None,
                },
            );
            out.push(id);
        }
        Ok(out)
    }

    /// The player drops items by hand (the inventory menu): as [`World::drop_item`], but a
    /// quest item ([`World::is_quest_item`]) is refused with [`SimError::QuestItem`] and
    /// nothing moves [documented: GECK wiki Category:Items and SetQuestObject: the player
    /// cannot drop one]. The count is checked first.
    pub fn player_drop_item(&mut self, item: FormId, count: i32) -> Result<Vec<FormId>, SimError> {
        if count > 0 && self.is_quest_item(item) {
            return Err(SimError::QuestItem(item));
        }
        self.drop_item(FormId::PLAYER_REF, item, count)
    }

    /// Equip one carried `item` on `actor`. Weapons replace the equipped weapon; armour
    /// replaces equipped armour that shares a body slot (BMDT biped flags); aid items
    /// are consumed (their effects are Layer 6). [hypothesis: in-game equip rules as the
    /// GECK wiki describes `EquipItem`; to be captured] A weapon at zero condition is
    /// broken and is refused with [`SimError::Broken`] until it is repaired [design: the
    /// owner's rule, 2026-10-11]; a unit that is not broken is taken before a broken one.
    pub fn equip(&mut self, actor: FormId, item: FormId) -> Result<Equip, SimError> {
        let it = self.forms.item(item).ok_or(SimError::NotAnItem(item))?.clone();
        let forms = self.forms;
        let inv = self.inventory(actor)?;
        if it.kind == ItemKind::Aid {
            return match inv.remove(item, 1) {
                1 => Ok(Equip { consumed: true, unequipped: Vec::new() }),
                _ => Err(SimError::NotCarried { actor, item }),
            };
        }
        if !matches!(it.kind, ItemKind::Weapon | ItemKind::Armor) {
            return Err(SimError::NotEquippable(item));
        }
        if inv.count(item) == 0 {
            return Err(SimError::NotCarried { actor, item });
        }
        if it.kind == ItemKind::Weapon && !inv.has_sound(item) && !inv.is_equipped(item) {
            return Err(SimError::Broken(item));
        }
        let clashes = |other: FormId| -> bool {
            let Some(o) = forms.item(other) else { return false };
            match (it.kind, o.kind) {
                (ItemKind::Weapon, ItemKind::Weapon) => true,
                (ItemKind::Armor, ItemKind::Armor) => it.slots.unwrap_or(0) & o.slots.unwrap_or(0) != 0,
                _ => false,
            }
        };
        let mut unequipped = Vec::new();
        for other in inv.equipped() {
            if clashes(other) && inv.clear_equipped(other) {
                unequipped.push(other);
            }
        }
        if !inv.set_equipped(item) {
            return Err(SimError::NotCarried { actor, item });
        }
        Ok(Equip { consumed: false, unequipped })
    }

    /// Unequip `item`; returns whether it was equipped.
    pub fn unequip(&mut self, actor: FormId, item: FormId) -> Result<bool, SimError> {
        Ok(self.inventory(actor)?.clear_equipped(item))
    }

    /// An actor value: S.P.E.C.I.A.L. from the base NPC_ record unless changed at run
    /// time. (Template NPCs take stats from their template: not followed yet.)
    pub fn actor_value(&mut self, actor: FormId, name: &str) -> Result<f32, SimError> {
        let key = name.to_ascii_lowercase();
        let idx = SPECIAL.iter().position(|n| *n == key).ok_or_else(|| SimError::UnknownActorValue(name.into()))?;
        let s = self.get(actor)?;
        if let Some(v) = s.actor_values.get(&key) {
            return Ok(*v);
        }
        let base = s.base;
        match self.forms.get(base).map(|b| &b.data) {
            Some(BaseData::Actor(a)) => a.attributes.map(|a| a[idx] as f32).ok_or(SimError::NoActorValues(actor)),
            _ => Err(SimError::NoActorValues(actor)),
        }
    }

    pub fn set_actor_value(&mut self, actor: FormId, name: &str, value: f32) -> Result<(), SimError> {
        let key = name.to_ascii_lowercase();
        self.actor_value(actor, &key)?;
        self.get(actor)?.actor_values.insert(key, value);
        Ok(())
    }

    fn setting(&self, edid: &str) -> Result<f32, SimError> {
        self.forms.setting_f32(edid).ok_or_else(|| SimError::MissingSetting(edid.into()))
    }

    /// Carried weight against capacity. Capacity = fAVDCarryWeightsBase +
    /// fAVDCarryWeightMult × Strength (150 + 10 × Strength in the masters); the actor is
    /// over-encumbered when the whole part of the weight exceeds it. [observed in game:
    /// capacity 200 at Strength 5 and 160 at Strength 1, equipped items counted — xNVSE
    /// capture, `L4.sim.trace.goodsprings_weight_and_time`; with capacity 200 the player
    /// still ran (343 units/s) carrying 199.5, 200.0 and 200.5 and walked (85 units/s,
    /// `IsRunning` 0) carrying 201.0, 201.5, 202, 203, 205 and 210.5 —
    /// `cl_b_encumbrance_speed`, `cl_b2_encumbrance_speed`, docs/spec-notes/simulation.md
    /// §3. Whole part vs. weight ≥ capacity + 1 differ only for fractional capacities:
    /// hypothesis]
    pub fn encumbrance(&mut self, actor: FormId) -> Result<Encumbrance, SimError> {
        let strength = self.actor_value(actor, "strength")?;
        let capacity = self.setting("fAVDCarryWeightsBase")? + self.setting("fAVDCarryWeightMult")? * strength;
        let (forms, hardcore) = (self.forms, self.cfg.hardcore);
        let t = self.inventory(actor)?.totals(forms, hardcore);
        Ok(Encumbrance {
            weight: t.weight,
            capacity: capacity as f64,
            over: t.weight.floor() > capacity as f64,
            unknown_weight: t.unknown_weight,
        })
    }

    /// The worldspace a reference is in now: that of its cell (`None` in an interior).
    /// The cell is the truth: a teleport into another worldspace moves the actor into an
    /// exterior cell of that worldspace, so position, cell and worldspace change
    /// together and a snapshot keeps them (it stores the cell). Does not materialise
    /// anything.
    pub fn worldspace_of(&self, id: FormId) -> Option<FormId> {
        let cell = match self.peek(id) {
            Some(s) => s.cell,
            None => self.placed.get(id)?.cell,
        };
        self.placed.world_of(cell?)
    }

    /// Whether a reference is enabled, following its enable-parent chain. A reference
    /// with an enable parent takes the parent's effective state (inverted when the
    /// "opposite" flag is set) and its own flag is ignored; a chain that ends at a
    /// reference that does not exist, or loops, falls back to the reference's own flag.
    /// [hypothesis: GECK wiki "Enable Parent"; to be captured] Does not materialise
    /// anything, so it never consumes randomness.
    pub fn is_enabled(&self, id: FormId) -> Result<bool, SimError> {
        let own = |w: &World, id: FormId| -> Option<bool> {
            match w.refs.get(&id) {
                Some(s) => Some(s.enabled),
                None => w.placed.get(id).map(|p| !p.initially_disabled()),
            }
        };
        let start = own(self, id).ok_or(SimError::UnknownRef(id))?;
        let mut invert = false;
        let mut cur = id;
        let mut seen = vec![id];
        loop {
            let Some(ep) = self.placed.get(cur).and_then(|p| p.enable_parent) else {
                return Ok(own(self, cur).unwrap_or(start) ^ invert);
            };
            if seen.contains(&ep.reference) || own(self, ep.reference).is_none() {
                return Ok(start);
            }
            invert ^= ep.opposite;
            seen.push(ep.reference);
            cur = ep.reference;
        }
    }

    /// Current value of a global.
    pub fn global(&self, id: FormId) -> Result<f32, SimError> {
        match self.globals.get(&id) {
            Some(v) => Ok(*v),
            None => self.forms.globals.get(&id).copied().ok_or(SimError::UnknownForm(id)),
        }
    }

    pub fn set_global(&mut self, id: FormId, value: f32) -> Result<(), SimError> {
        if !self.forms.globals.contains_key(&id) {
            return Err(SimError::UnknownForm(id));
        }
        self.globals.insert(id, value);
        Ok(())
    }

    fn time_global(&self, edid: &str) -> Result<(FormId, f32), SimError> {
        let (id, _) = self.forms.global_by_edid(edid).ok_or_else(|| SimError::MissingGlobal(edid.to_string()))?;
        Ok((id, self.global(id)?))
    }

    /// The clock as stored in the time globals.
    pub fn calendar(&self) -> Result<Calendar, SimError> {
        Ok(Calendar {
            year: self.time_global(clock::GAME_YEAR)?.1,
            month: self.time_global(clock::GAME_MONTH)?.1,
            day: self.time_global(clock::GAME_DAY)?.1,
            hour: self.time_global(clock::GAME_HOUR)?.1,
            days_passed: self.time_global(clock::GAME_DAYS_PASSED)?.1,
        })
    }

    /// Advance game time by `hours` (waiting, sleeping, fast travel). Merchant
    /// containers restock at the midnights passed ([`World::restock_merchants`]).
    pub fn advance_hours(&mut self, hours: f64) -> Result<Calendar, SimError> {
        let before = self.calendar()?;
        let c = before.advance(hours);
        for (edid, v) in [
            (clock::GAME_YEAR, c.year),
            (clock::GAME_MONTH, c.month),
            (clock::GAME_DAY, c.day),
            (clock::GAME_HOUR, c.hour),
            (clock::GAME_DAYS_PASSED, c.days_passed),
        ] {
            let (id, _) = self.time_global(edid)?;
            self.globals.insert(id, v);
        }
        self.restock_merchants(before.days_passed as f64, c.days_passed as f64)?;
        Ok(c)
    }

    /// Advance game time by `seconds` of real time at the current TimeScale.
    pub fn advance_real_seconds(&mut self, seconds: f64) -> Result<Calendar, SimError> {
        let scale = self.time_global(clock::TIME_SCALE)?.1 as f64;
        self.advance_hours(seconds * scale / 3600.0)
    }

    /// The respawn delay in game hours: the config's, else the `iHoursToRespawnCell`
    /// setting from the plugins, else [`DEFAULT_RESPAWN_HOURS`].
    pub fn respawn_hours(&self) -> f32 {
        self.cfg
            .respawn_hours
            .or_else(|| self.forms.setting_f32(RESPAWN_SETTING))
            .unwrap_or(DEFAULT_RESPAWN_HOURS)
    }

    /// The engine unloads `cell` (the player left it).
    pub fn detach_cell(&mut self, cell: FormId) -> Result<(), SimError> {
        let now = self.calendar()?.days_passed;
        self.detached_cells.insert(cell, now);
        Ok(())
    }

    /// The engine loads `cell` again. If it stayed unloaded for at least
    /// `respawn_hours`, the containers in it whose base has the Respawns flag go back
    /// to their authored state: their next use rolls new contents, and anything left in
    /// them is lost. [hypothesis: the cell-reset behaviour commonly described for this
    /// engine generation; other references are not reset yet] Returns the containers
    /// reset.
    pub fn attach_cell(&mut self, cell: FormId) -> Result<Vec<FormId>, SimError> {
        let Some(since) = self.detached_cells.remove(&cell) else { return Ok(Vec::new()) };
        let now = self.calendar()?.days_passed;
        // GameDaysPassed is an f32 global, so an exact delay can come out a hair short
        // (72 h measured as 71.99999 h): compare with a thousandth of an hour of slack.
        let elapsed_hours = (now as f64 - since as f64) * 24.0;
        if elapsed_hours + 1e-3 < self.respawn_hours() as f64 {
            return Ok(Vec::new());
        }
        let forms = self.forms;
        let respawns = |base: FormId| matches!(forms.get(base).map(|b| &b.data), Some(BaseData::Container(c)) if c.respawns);
        let reset: Vec<FormId> = self
            .placed
            .by_cell
            .get(&cell)
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| self.refs.get(id).is_some_and(|s| respawns(s.base)))
            .collect();
        for id in &reset {
            self.refs.remove(id);
        }
        Ok(reset)
    }

    pub fn set_enabled(&mut self, id: FormId, enabled: bool) -> Result<(), SimError> {
        self.get(id)?.enabled = enabled;
        Ok(())
    }

    pub fn lock(&mut self, id: FormId, level: Option<u8>) -> Result<(), SimError> {
        let s = self.get(id)?;
        match &mut s.lock {
            Some(l) => {
                l.locked = true;
                if let Some(v) = level {
                    l.level = v;
                }
            }
            None => s.lock = Some(LockState { level: level.unwrap_or(0), key: None, locked: true }),
        }
        Ok(())
    }

    pub fn unlock(&mut self, id: FormId) -> Result<(), SimError> {
        if let Some(l) = &mut self.get(id)?.lock {
            l.locked = false;
        }
        Ok(())
    }

    /// The access state of a door or container lock, as the in-game `GetLocked` reports
    /// it: 0 not locked, 1 locked. [documented: GECK wiki `GetLocked`, which also lists 2
    /// "locked out" for an impossible level or a broken lock; observed in game,
    /// docs/spec-notes/simulation.md §5: a level-255 lock reads 1, both an authored one
    /// and one set with `Lock 255`. Broken locks (failed lockpicking) are Layer 6.]
    pub fn lock_status(&mut self, id: FormId) -> Result<u8, SimError> {
        Ok(match self.get(id)?.lock {
            Some(l) if l.locked => 1,
            _ => 0,
        })
    }

    /// Create a reference of any base form at run time (the script function PlaceAtMe,
    /// Layer 5): enabled, at `pos`/`rot` in `cell`, with a fresh inventory for a
    /// container or actor base. Returns its FormID (the next dynamic one, as drops).
    /// Where the game puts it exactly is the caller's choice [hypothesis].
    pub fn place(&mut self, base: FormId, cell: Option<FormId>, pos: [f32; 3], rot: [f32; 3], count: i32) -> Result<FormId, SimError> {
        if self.forms.sig(base).is_none() {
            return Err(SimError::UnknownForm(base));
        }
        let inventory = self.roll_inventory(base)?;
        let id = FormId(self.next_dynamic);
        self.next_dynamic += 1;
        self.refs.insert(
            id,
            RefState {
                base,
                cell,
                pos,
                rot,
                enabled: true,
                deleted: false,
                count: count.max(1),
                inventory,
                lock: None,
                open: false,
                extra: Extra::default(),
                actor_values: BTreeMap::new(),
                last_restock: None,
            },
        );
        Ok(id)
    }

    /// The inventory of a reference, for callers that change extra data in place
    /// (Layer 5 functions such as ModWeaponHealthPerc).
    pub fn inventory_mut(&mut self, id: FormId) -> Result<&mut Inventory, SimError> {
        self.inventory(id)
    }

    /// Add `count` of an item (not a leveled list) with the given extra data.
    pub fn add_item_extra(&mut self, to: FormId, item: FormId, count: i32, extra: Extra) -> Result<i32, SimError> {
        if count <= 0 {
            return Err(SimError::BadCount(count));
        }
        self.check_item(item)?;
        Ok(self.inventory(to)?.add_extra(item, count, extra))
    }

    /// A fresh roll of the base's carried items replaces the reference's inventory
    /// (ResetInventory, Layer 5). `false` when the base carries no items.
    pub fn reset_inventory(&mut self, id: FormId) -> Result<bool, SimError> {
        let base = self.get(id)?.base;
        let inv = self.roll_inventory(base)?;
        let s = self.get(id)?;
        let had = inv.is_some();
        if had {
            s.inventory = inv;
        }
        Ok(had)
    }

    /// References created at run time (drops), in FormID order, taken ones included
    /// (`RefState::deleted`).
    pub fn created(&self) -> impl Iterator<Item = (FormId, &RefState)> {
        self.refs.range(FormId::FIRST_DYNAMIC..).map(|(&k, v)| (k, v))
    }

    /// The newest reference created at run time (a drop) whose base is `base` and that
    /// is still present (not taken, enabled). Scenarios name it `created:<item>`
    /// because the FormID the game gives a dropped item is not known in advance.
    pub fn newest_created(&self, base: FormId) -> Option<FormId> {
        self.refs
            .range(FormId::FIRST_DYNAMIC..)
            .rev()
            .find(|(_, s)| s.base == base && !s.deleted && s.enabled)
            .map(|(&id, _)| id)
    }

    /// `actor` activates `target` itself (the player's own activation): an item
    /// reference gives its whole stack [hypothesis; not captured]. Interactive
    /// activation can only reach what the player can see, so keeping taken or disabled
    /// references out of reach is the engine's job, not this function's. Otherwise the
    /// same as [`World::activate_scripted`].
    pub fn activate(&mut self, actor: FormId, target: FormId) -> Result<Activation, SimError> {
        self.activate_as(actor, target, false)
    }

    /// `target` is activated by the scripted `Activate` function with `actor` as the
    /// activator (`<ref>.Activate player`, how the parity traces are captured). An item
    /// reference gives **one** item, whatever its count, and keeps a count of 1 (the
    /// rest of the stack is lost). [observed in game, docs/spec-notes/simulation.md §5:
    /// a placed stack of 12 BB rounds gave 1 and then counted 1 (`GetRefCount`), a second
    /// activation gave 1 more; dropped stacks of 2 and 3 stimpaks gave 1 each;
    /// documented: GECK wiki Activate notes the loss of a stack]
    pub fn activate_scripted(&mut self, actor: FormId, target: FormId) -> Result<Activation, SimError> {
        self.activate_as(actor, target, true)
    }

    fn activate_as(&mut self, actor: FormId, target: FormId, scripted: bool) -> Result<Activation, SimError> {
        self.get(actor)?;
        let t = self.get(target)?.clone();
        let base = self.forms.get(t.base);
        let script = base.and_then(|b| b.script);
        let done = |outcome, unlocked_with| Activation { outcome, unlocked_with, script };
        // Scripted activation of an item gives its stack on every call, whether the
        // reference was already taken or is disabled, and leaves the reference's enabled
        // state alone. [observed in game, docs/spec-notes/simulation.md §5: one stimpak
        // reference activated three times gave 1, 2, 3 stimpaks; a hand-taken one gave
        // another; a never-taken cap disabled with `Disable` still gave a cap and stayed
        // disabled; GetDisabled and GetParentCell do not change after a pick-up]
        if let Some(BaseData::Item(_)) = base.map(|b| &b.data) {
            let count = if scripted { t.count.min(1) } else { t.count };
            self.inventory(actor)?.add_extra(t.base, count, t.extra);
            let s = self.get(target)?;
            s.deleted = true;
            if scripted {
                s.count = count;
            }
            return Ok(done(Outcome::PickedUp { item: t.base, count, owner: t.extra.owner }, None));
        }
        // Other types: a taken or disabled reference does nothing. [observed by the
        // owner in game, 2026-10-08: disabled references cannot be interacted with; not
        // captured by script for doors, containers and activators]
        if t.deleted || !self.is_enabled(target)? {
            return Ok(done(Outcome::Disabled, None));
        }
        let Some(base) = base else {
            let sig = self.forms.sig(t.base).ok_or(SimError::UnknownForm(t.base))?;
            return Ok(done(Outcome::NoDefault { sig: String::from_utf8_lossy(&sig).into_owned() }, None));
        };
        // Locks apply to doors and containers. A locked target opens if the actor
        // carries its key, without a prompt. [observed by the owner in game,
        // 2026-10-08, for a container; doors per the GECK wiki's description of keys;
        // lockpicking is Layer 6]
        let mut unlocked_with = None;
        if matches!(base.data, BaseData::Door(_) | BaseData::Container(_)) {
            if let Some(l) = t.lock.filter(|l| l.locked) {
                match l.key.filter(|&k| self.inventory(actor).is_ok_and(|inv| inv.count(k) > 0)) {
                    Some(k) => {
                        self.unlock(target)?;
                        unlocked_with = Some(k);
                    }
                    None => return Ok(done(Outcome::Locked { level: l.level }, None)),
                }
            }
        }
        let outcome = match &base.data {
            BaseData::Item(_) => unreachable!("items are handled above"),
            BaseData::Container(_) => Outcome::OpenedContainer { container: target },
            BaseData::Door(_) => match self.placed.get(target).and_then(|p| p.teleport) {
                Some(tp) => {
                    let door_cell = match self.placed.get(tp.door) {
                        Some(d) => d.cell,
                        None => self.peek(tp.door).and_then(|d| d.cell),
                    };
                    // Into a worldspace, the actor ends up in the exterior cell that
                    // contains the destination, not necessarily the destination door's.
                    // [observed in game: leaving Doc Mitchell's house puts the player in
                    // Goodsprings, WastelandNV grid -18,0]
                    let cell = door_cell.map(|c| self.placed.exterior_cell_at(c, tp.pos).unwrap_or(c));
                    let a = self.get(actor)?;
                    a.cell = cell;
                    a.pos = tp.pos;
                    a.rot = tp.rot;
                    Outcome::Teleported { to_door: tp.door, cell, pos: tp.pos, rot: tp.rot }
                }
                None => {
                    let s = self.get(target)?;
                    s.open = !s.open;
                    Outcome::DoorToggled { open: s.open }
                }
            },
            BaseData::Actor(_) => Outcome::Talk { actor: target },
            BaseData::Activator | BaseData::Furniture => Outcome::Activated,
            BaseData::LeveledItem(_) => Outcome::NoDefault { sig: "LVLI".into() },
            // A placed explosive activated by script: nothing happens. [observed in
            // game, docs/spec-notes/simulation.md §1: `<mine>.Activate player` on two
            // placed frag mines in Nipton left them enabled and in place and gave no Frag
            // Mine (`bl_placed_mines`, `L4.sim.trace.nipton_placed_mines`)]
            BaseData::Projectile { .. } if scripted => Outcome::NoDefault { sig: "PROJ".into() },
            // The player's own activation disarms it and gives the weapon that fires it.
            // [hypothesis: in-game disarming of placed mines; the Explosives skill check
            // and the chance to trigger are Layer 6]
            BaseData::Projectile { item: Some(item) } => {
                self.inventory(actor)?.add(*item, 1);
                self.get(target)?.deleted = true;
                Outcome::Disarmed { item: *item }
            }
            BaseData::Projectile { item: None } => Outcome::NoDefault { sig: "PROJ".into() },
        };
        Ok(done(outcome, unlocked_with))
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            format: SNAPSHOT_FORMAT.into(),
            config: self.cfg,
            rng_word_pos: self.rng.get_word_pos().to_string(),
            next_dynamic: self.next_dynamic,
            refs: self.refs.clone(),
            globals: self.globals.clone(),
            detached_cells: self.detached_cells.clone(),
            quest_objects: self.quest_objects.clone(),
        }
    }

    pub fn restore(forms: &'a FormStore, placed: &'a RefTable, s: &Snapshot) -> Result<World<'a>, String> {
        if s.format != SNAPSHOT_FORMAT {
            return Err(format!("unsupported snapshot format {:?}", s.format));
        }
        let pos: u128 = s.rng_word_pos.parse().map_err(|_| format!("bad rng_word_pos {:?}", s.rng_word_pos))?;
        let mut w = World::new(forms, placed, s.config);
        w.rng.set_word_pos(pos);
        w.next_dynamic = s.next_dynamic;
        w.refs = s.refs.clone();
        w.globals = s.globals.clone();
        w.detached_cells = s.detached_cells.clone();
        w.quest_objects = s.quest_objects.clone();
        Ok(w)
    }
}
