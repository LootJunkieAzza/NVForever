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
    /// Level used for leveled lists (the player's level). [hypothesis: containers
    /// resolve against the player's level]
    pub player_level: u16,
    /// Hardcore mode: ammunition has weight. (Absent in older snapshots.)
    #[serde(default)]
    pub hardcore: bool,
    /// Game hours a cell must stay unloaded before its respawning containers refill.
    /// No game setting holds it in the masters; 72 is the commonly cited engine
    /// default [hypothesis] (absent in older snapshots).
    #[serde(default = "default_respawn_hours")]
    pub respawn_hours: f32,
}

fn default_respawn_hours() -> f32 {
    72.0
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig { seed: 0, player_level: 1, hardcore: false, respawn_hours: default_respawn_hours() }
    }
}

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
        let inventory = match self.forms.entries(base) {
            Some(entries) => {
                let mut inv = Inventory::new();
                let mut out = Vec::new();
                for e in entries {
                    let from = out.len();
                    leveled::resolve(self.forms, e.item, e.count, self.cfg.player_level, &mut self.rng, &mut out)?;
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
                Some(inv)
            }
            None => None,
        };
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
            open: false,
            extra: Extra {
                condition: placed.and_then(|p| p.health),
                owner: placed.and_then(|p| p.owner),
                equipped: false,
            },
            actor_values: BTreeMap::new(),
        })
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
        leveled::resolve(self.forms, item, count, self.cfg.player_level, &mut self.rng, &mut out)?;
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

    /// Move up to `count` from one inventory to another; returns how many moved.
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

    /// Move everything; returns the stacks moved. Extra data (condition, owner) moves
    /// with the items; equipped items are unequipped.
    pub fn transfer_all(&mut self, from: FormId, to: FormId) -> Result<Vec<(FormId, i32)>, SimError> {
        self.inventory(to)?;
        let stacks: Vec<_> = self.inventory(from)?.iter().collect();
        for &(item, n) in &stacks {
            self.transfer(from, to, item, n)?;
        }
        Ok(stacks)
    }

    /// Empty an inventory; returns the stacks removed.
    pub fn remove_all_items(&mut self, from: FormId) -> Result<Vec<(FormId, i32)>, SimError> {
        let inv = self.inventory(from)?;
        let stacks: Vec<_> = inv.iter().collect();
        *inv = Inventory::new();
        Ok(stacks)
    }

    /// Drop items from an inventory into the world at the holder's position, one new
    /// reference per group of identical extra data [hypothesis]. Returns the new
    /// references.
    pub fn drop_item(&mut self, from: FormId, item: FormId, count: i32) -> Result<Vec<FormId>, SimError> {
        if count <= 0 {
            return Err(SimError::BadCount(count));
        }
        let groups = unequipped(self.inventory(from)?.take(item, count));
        let holder = self.get(from)?.clone();
        let mut out = Vec::new();
        for (extra, n) in groups {
            let id = FormId(self.next_dynamic);
            self.next_dynamic += 1;
            self.refs.insert(
                id,
                RefState {
                    base: item,
                    cell: holder.cell,
                    pos: holder.pos,
                    rot: [0.0; 3],
                    enabled: true,
                    deleted: false,
                    count: n,
                    inventory: None,
                    lock: None,
                    open: false,
                    extra,
                    actor_values: BTreeMap::new(),
                },
            );
            out.push(id);
        }
        Ok(out)
    }

    /// Equip one carried `item` on `actor`. Weapons replace the equipped weapon; armour
    /// replaces equipped armour that shares a body slot (BMDT biped flags); aid items
    /// are consumed (their effects are Layer 6). [hypothesis: in-game equip rules as the
    /// GECK wiki describes `EquipItem`; to be captured]
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
    /// over-encumbered when the weight exceeds it. [hypothesis: the formula the setting
    /// names describe; equipped items count]
    pub fn encumbrance(&mut self, actor: FormId) -> Result<Encumbrance, SimError> {
        let strength = self.actor_value(actor, "strength")?;
        let capacity = self.setting("fAVDCarryWeightsBase")? + self.setting("fAVDCarryWeightMult")? * strength;
        let (forms, hardcore) = (self.forms, self.cfg.hardcore);
        let t = self.inventory(actor)?.totals(forms, hardcore);
        Ok(Encumbrance {
            weight: t.weight,
            capacity: capacity as f64,
            over: t.weight > capacity as f64,
            unknown_weight: t.unknown_weight,
        })
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

    /// Advance game time by `hours` (waiting, sleeping, fast travel).
    pub fn advance_hours(&mut self, hours: f64) -> Result<Calendar, SimError> {
        let c = self.calendar()?.advance(hours);
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
        Ok(c)
    }

    /// Advance game time by `seconds` of real time at the current TimeScale.
    pub fn advance_real_seconds(&mut self, seconds: f64) -> Result<Calendar, SimError> {
        let scale = self.time_global(clock::TIME_SCALE)?.1 as f64;
        self.advance_hours(seconds * scale / 3600.0)
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
        if elapsed_hours + 1e-3 < self.cfg.respawn_hours as f64 {
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

    /// `actor` activates `target`.
    pub fn activate(&mut self, actor: FormId, target: FormId) -> Result<Activation, SimError> {
        self.get(actor)?;
        let t = self.get(target)?.clone();
        let base = self.forms.get(t.base);
        let script = base.and_then(|b| b.script);
        let done = |outcome, unlocked_with| Activation { outcome, unlocked_with, script };
        if t.deleted || !self.is_enabled(target)? {
            return Ok(done(Outcome::Disabled, None));
        }
        let Some(base) = base else {
            let sig = self.forms.sig(t.base).ok_or(SimError::UnknownForm(t.base))?;
            return Ok(done(Outcome::NoDefault { sig: String::from_utf8_lossy(&sig).into_owned() }, None));
        };
        // Locks apply to doors and containers. A locked target opens if the actor
        // carries its key. [hypothesis: matches the GECK wiki's description of
        // keys; lockpicking is Layer 6]
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
            BaseData::Item(_) => {
                self.inventory(actor)?.add_extra(t.base, t.count, t.extra);
                self.get(target)?.deleted = true;
                Outcome::PickedUp { item: t.base, count: t.count, owner: t.extra.owner }
            }
            BaseData::Container(_) => Outcome::OpenedContainer { container: target },
            BaseData::Door(_) => match self.placed.get(target).and_then(|p| p.teleport) {
                Some(tp) => {
                    let cell = match self.placed.get(tp.door) {
                        Some(d) => d.cell,
                        None => self.peek(tp.door).and_then(|d| d.cell),
                    };
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
            // A placed explosive: disarming it gives the weapon that fires it.
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
        Ok(w)
    }
}
