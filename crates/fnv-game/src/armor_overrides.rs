//! Live session overrides for `ARMO` (and shared item) traits/flags/slots
//! (docs/spec-notes/ext-last-core.md §11): keeps `field_overrides`, `World::item_overrides`,
//! `OtherState::power_armor`, and `Game::playable_overrides` synchronized across all
//! setters (`SetEquipmentSlotsMask`, `SetEquipmentBipedMask`, `SetArmorClass`,
//! `SetIsPowerArmor`, `SetIsPlayable`, `LNSetIsPlayable`, `SetArmorDT`,
//! `SetArmorArmorRating`, `SetArmorARAlt`, `SetWeight`, `SetObjectHealth`, `SetRepairList`)
//! and provides bi-directional `TempCloneForm` ARMO snapshot isolation.

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::forms::ArmorClass;
use fnv_sim::world::ItemOverride;
use fnv_sim::FormId;

/// `ARMO` `BMDT/general_flags` (`u8`) bit masks
/// [documented: `schemas/fnv/objects.json` ARMO BMDT; GECK wiki SetArmorClass,
/// SetIsPowerArmor, SetIsPlayable, SetEquipmentBipedMask].
pub(crate) mod bmdt {
    pub const MEDIUM: u8 = 1 << 3;
    pub const POWER_ARMOR: u8 = 1 << 5;
    pub const NON_PLAYABLE: u8 = 1 << 6;
    pub const HEAVY: u8 = 1 << 7;
}

/// Decode `ArmorClass` from `ARMO` `BMDT/general_flags` [documented: SetArmorClass;
/// `schemas/fnv/objects.json` ARMO BMDT]: Heavy overrides Medium; neither is Light.
pub(crate) fn armor_class_from_flags(flags: u8) -> ArmorClass {
    if flags & bmdt::HEAVY != 0 {
        ArmorClass::Heavy
    } else if flags & bmdt::MEDIUM != 0 {
        ArmorClass::Medium
    } else {
        ArmorClass::Light
    }
}

impl Game<'_> {
    /// Whether `f` (or its `TempCloneForm` source) is an `ARMO` record.
    pub(crate) fn is_armo(&self, f: FormId) -> bool {
        self.data.sig(self.unclone(f)) == Some(*b"ARMO")
    }

    /// Current `ARMO` `BMDT/general_flags` byte for `armor` (session override first, then
    /// decoded record field, else reconstructed from `World::resolve_item`).
    pub(crate) fn armo_general_flags(&self, armor: FormId) -> u8 {
        if let Some(XVal::Num(x)) = crate::nvse::record_value(self, armor, "BMDT", "general_flags") {
            return (x.max(0.0) as u32 & 0xFF) as u8;
        }
        let Some(it) = self.world.resolve_item(armor) else { return 0 };
        let mut flags = 0u8;
        match it.armor_class {
            ArmorClass::Medium => flags |= bmdt::MEDIUM,
            ArmorClass::Heavy => flags |= bmdt::HEAVY,
            ArmorClass::Light => {}
        }
        if it.power_armor {
            flags |= bmdt::POWER_ARMOR;
        }
        if it.non_playable {
            flags |= bmdt::NON_PLAYABLE;
        }
        flags
    }

    /// Write `ARMO` `BMDT/general_flags` for this session and synchronize every derived
    /// store (`field_overrides`, `other.power_armor`, `playable_overrides`, `World`).
    pub(crate) fn set_armo_general_flags(&mut self, armor: FormId, flags: u8) {
        self.field_overrides.insert((armor, "BMDT/general_flags".into()), XVal::Num(f64::from(flags)));
        let class = armor_class_from_flags(flags);
        let power_armor = flags & bmdt::POWER_ARMOR != 0;
        let non_playable = flags & bmdt::NON_PLAYABLE != 0;
        self.other.power_armor.insert(armor, power_armor);
        self.playable_overrides.insert(armor, !non_playable);
        self.world.mutate_item_override(armor, |ov| {
            ov.armor_class = Some(class);
            ov.power_armor = Some(power_armor);
            ov.non_playable = Some(non_playable);
        });
    }

    /// Write `ARMO` `BMDT/biped_flags` (slot mask) for this session and synchronize `World`.
    pub(crate) fn set_armo_slots(&mut self, armor: FormId, slots: u32) {
        self.field_overrides.insert((armor, "BMDT/biped_flags".into()), XVal::Num(f64::from(slots)));
        self.world.mutate_item_override(armor, |ov| {
            ov.slots = Some(slots);
        });
    }

    /// Write `ARMO` class (`1` Light, `2` Medium, `3` Heavy) through `BMDT/general_flags`.
    pub(crate) fn set_armo_class(&mut self, armor: FormId, class: u8) {
        let mut flags = self.armo_general_flags(armor) & !(bmdt::MEDIUM | bmdt::HEAVY);
        match class {
            2 => flags |= bmdt::MEDIUM,
            3 => flags |= bmdt::HEAVY,
            _ => {}
        }
        self.set_armo_general_flags(armor, flags);
    }

    /// Write `SetIsPowerArmor` for `f`: updates `BMDT/general_flags` bit 5 on `ARMO` (so
    /// `GetEquipmentBipedMask`, `IsPowerArmor`, `World::equip_check`, `carried_weight`, and
    /// `wears_power_armor` all agree), or `other.power_armor` for non-`ARMO` forms.
    pub(crate) fn set_armo_power_armor(&mut self, f: FormId, on: bool) {
        if self.is_armo(f) {
            let cur = self.armo_general_flags(f);
            let next = if on { cur | bmdt::POWER_ARMOR } else { cur & !bmdt::POWER_ARMOR };
            self.set_armo_general_flags(f, next);
        } else {
            self.other.power_armor.insert(f, on);
        }
    }

    /// Write `SetIsPlayable` / `LNSetIsPlayable` for an item (`ARMO`, `WEAP`, `AMMO`): on
    /// `ARMO`, synchronizes `BMDT/general_flags` bit 6 (`Non-Playable`) and `World`.
    pub(crate) fn set_item_playable(&mut self, f: FormId, playable: bool) {
        if self.is_armo(f) {
            let cur = self.armo_general_flags(f);
            let next = if playable { cur & !bmdt::NON_PLAYABLE } else { cur | bmdt::NON_PLAYABLE };
            self.set_armo_general_flags(f, next);
        } else {
            self.playable_overrides.insert(f, playable);
            self.world.mutate_item_override(f, |ov| {
                ov.non_playable = Some(!playable);
            });
        }
    }

    /// Mirror a generic `nvse.rs` `field_overrides` write (`DNAM/dt`, `DNAM/dr`,
    /// `DATA/weight`, `DAT2/weight`, `DATA/`, `DATA/health`) into `World::item_overrides`
    /// so equip/outfitting/weight/rating consumers see the updated value immediately.
    pub(crate) fn sync_item_field_override(&mut self, f: FormId, sub: &str, path: &str, v: &XVal) {
        let Ok(x) = v.num() else { return };
        if !x.is_finite() {
            return;
        }
        let sig = self.data.sig(self.unclone(f));
        match (sub, path) {
            ("DNAM", "dt") if sig == Some(*b"ARMO") => {
                self.world.mutate_item_override(f, |ov| ov.armor_dt = Some(x as f32));
            }
            ("DNAM", "dr") if sig == Some(*b"ARMO") => {
                self.world.mutate_item_override(f, |ov| ov.armor_dr = Some(x as f32 / 100.0));
            }
            ("DATA", "weight") | ("DAT2", "weight") => {
                self.world.mutate_item_override(f, |ov| ov.weight = Some(x.max(0.0) as f32));
            }
            ("DATA", "") if sig == Some(*b"ALCH") => {
                self.world.mutate_item_override(f, |ov| ov.weight = Some(x.max(0.0) as f32));
            }
            ("DATA", "health") if matches!(sig.as_ref(), Some(b"ARMO" | b"WEAP")) => {
                self.world.mutate_item_override(f, |ov| ov.health = Some(x.max(0.0).trunc() as i32));
            }
            _ => {}
        }
    }

    /// Snapshot `source`'s current live `ARMO` traits/flags/slots/repair-list onto `clone`
    /// at `TempCloneForm` creation time so edits to `clone` and later edits to `source`
    /// stay isolated in both directions.
    pub(crate) fn snapshot_armor_overrides(&mut self, source: FormId, clone: FormId) {
        if !self.is_armo(source) {
            return;
        }
        let slots = match crate::nvse::record_value(self, source, "BMDT", "biped_flags") {
            Some(XVal::Num(x)) => x.max(0.0) as u32,
            _ => self.world.effective_slots(source).unwrap_or(0),
        };
        let general = self.armo_general_flags(source);
        let dt = match crate::nvse::record_value(self, source, "DNAM", "dt") {
            Some(XVal::Num(x)) => Some(x),
            _ => self.world.effective_armor_dt(source).map(f64::from),
        };
        let dr_raw = match crate::nvse::record_value(self, source, "DNAM", "dr") {
            Some(XVal::Num(x)) => Some(x),
            _ => self.world.effective_armor_dr(source).map(|d| f64::from(d) * 100.0),
        };
        let weight = match crate::nvse::record_value(self, source, "DATA", "weight") {
            Some(XVal::Num(x)) => Some(x),
            _ => self.world.effective_weight(source).map(f64::from),
        };
        let health = match crate::nvse::record_value(self, source, "DATA", "health") {
            Some(XVal::Num(x)) => Some(x),
            _ => self.world.effective_health(source).map(f64::from),
        };
        let repair_list = self.effective_repair_list_form(source);

        self.field_overrides.insert((clone, "BMDT/biped_flags".into()), XVal::Num(f64::from(slots)));
        self.field_overrides.insert((clone, "BMDT/general_flags".into()), XVal::Num(f64::from(general)));
        if let Some(v) = dt {
            self.field_overrides.insert((clone, "DNAM/dt".into()), XVal::Num(v));
        }
        if let Some(v) = dr_raw {
            self.field_overrides.insert((clone, "DNAM/dr".into()), XVal::Num(v));
        }
        if let Some(v) = weight {
            self.field_overrides.insert((clone, "DATA/weight".into()), XVal::Num(v));
        }
        if let Some(v) = health {
            self.field_overrides.insert((clone, "DATA/health".into()), XVal::Num(v));
        }
        self.field_overrides.insert((clone, "REPL/".into()), XVal::Ref(repair_list.map_or(0, |l| l.0)));

        let power_armor = general & bmdt::POWER_ARMOR != 0;
        let non_playable = general & bmdt::NON_PLAYABLE != 0;
        self.other.power_armor.insert(clone, power_armor);
        self.playable_overrides.insert(clone, !non_playable);
        self.world.set_item_override(
            clone,
            ItemOverride {
                slots: Some(slots),
                armor_class: Some(armor_class_from_flags(general)),
                power_armor: Some(power_armor),
                non_playable: Some(non_playable),
                armor_dt: dt.map(|v| v as f32),
                armor_dr: dr_raw.map(|v| v as f32 / 100.0),
                weight: weight.map(|v| v.max(0.0) as f32),
                health: health.map(|v| v.max(0.0).trunc() as i32),
            },
        );
    }

    /// Effective repair-list `FLST` FormId (`REPL/`) for `item`, honoring `SetRepairList`
    /// and `TempCloneForm` snapshots.
    pub(crate) fn effective_repair_list_form(&self, item: FormId) -> Option<FormId> {
        if let Some(v) = self.field_overrides.get(&(item, "REPL/".to_string())) {
            return match v {
                XVal::Ref(r) if *r != 0 => Some(FormId(*r)),
                _ => None,
            };
        }
        let orig = self.unclone(item);
        if orig != item {
            if let Some(v) = self.field_overrides.get(&(orig, "REPL/".to_string())) {
                return match v {
                    XVal::Ref(r) if *r != 0 => Some(FormId(*r)),
                    _ => None,
                };
            }
        }
        self.data.repair_lists.get(&orig).copied()
    }
}
