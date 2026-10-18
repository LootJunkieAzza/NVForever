//! Aid items (ingestibles: chems, food, drink, medicine) and what magic effects do to
//! actor values. docs/spec-notes/aid-items.md has the rules and their sources.
//!
//! - Using an ingestible ([`Game::ingest`]): its effects apply to the actor
//!   ([`Game::apply_magic_item_with`]) with the beneficial Value Modifier magnitudes
//!   multiplied by the item's potency (Medicine and Food flags, §4); its usage monitor
//!   counts the use and the addiction roll may add the item's withdrawal spell (§5).
//! - A Value Modifier or Value And Parts effect becomes a [`ValueChange`] (§3): with the
//!   Recover flag a modifier held while the effect lasts; without it a restoration or
//!   damage, at once (duration 0) or by the second while it lasts.
//! - An addiction's effects are held back while an effect of a chem that names it as its
//!   withdrawal is active (§5.3); a Cure Addiction effect removes addictions (§5.4).

use crate::data::{
    EffectItem, IngestibleDef, MagicEffectDef, ARCHETYPE_CHAMELEON, ARCHETYPE_CURE_ADDICTION, ARCHETYPE_INVISIBILITY, ARCHETYPE_LIMB_CONDITION,
    ARCHETYPE_VALUE_AND_PARTS, ARCHETYPE_VALUE_MODIFIER,
};
use crate::functions::actor as fa;
use crate::game::Game;
use crate::trace::EventKind;
use fnv_sim::FormId;
use rand_core::RngCore;
use serde::{Deserialize, Serialize};

/// The usage monitor ("UMON") effect: the EFID 0x0000014F that chems carry with no MGEF
/// record behind it in any master [observed: FalloutNV.esm; documented: GECK wiki
/// Addiction Formula, UMON; hypothesis: that this FormID is the UMON effect, from
/// GetSpellUsageNum's example "the number of times Psycho has been used, multiplied by
/// 30" and the magnitude 30 it has on the chems]. aid-items.md §5.1.
pub const USAGE_MONITOR: FormId = FormId(0x14F);

/// SPEL SPIT type "Addiction" [documented: xEdit SPEL].
pub const SPELL_ADDICTION: i64 = 10;

/// Actor value codes read here [documented: xEdit ActorValue, `schemas/fnv/common.json`].
pub const RAD_LEVEL: u16 = 54;
pub const DEHYDRATION: u16 = 73;
pub const HUNGER: u16 = 74;
pub const SLEEP_DEPRIVATION: u16 = 75;
/// Perception Condition to Brain Condition.
pub const LIMB_CONDITIONS: std::ops::RangeInclusive<u16> = 25..=31;

/// The settings of potency and addiction, with the GECK wiki's defaults [documented:
/// Ingestible Settings, Addiction Formula].
pub const SETTINGS: [(&str, f64); 5] = [
    ("fMagicMedicineSkillBase", 1.0),
    ("fMagicMedicineSkillMult", 2.0),
    ("fMagicSurvivalSkillBase", 1.0),
    ("fMagicSurvivalSkillMult", 2.0),
    ("fAddictionUsageMonitorThreshold", 70.0),
];

/// How a value change is carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    /// Recover: a modifier while the effect lasts, taken away when it ends.
    Modifier,
    /// No Recover, duration 0 (or an ability): restored or damaged once, at the start.
    Once,
    /// No Recover, a duration: `amount` per second while it lasts.
    PerSecond,
}

/// What a Value Modifier (or Value And Parts) effect does to one actor value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ValueChange {
    pub code: u16,
    pub kind: ChangeKind,
    /// Signed: positive raises or restores, negative lowers or damages (per second for
    /// [`ChangeKind::PerSecond`]); potency included.
    pub amount: f64,
    /// Value And Parts: the limb conditions are restored with the value.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub parts: bool,
    /// What is in its pool now (Modifier), or what was applied so far (Once, PerSecond).
    #[serde(default)]
    pub applied: f64,
    /// The modifier sits in the permanent pool (a player's ability without conditions).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub permanent_pool: bool,
    /// Held back: an addiction whose chem is active.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub suspended: bool,
}

/// The values that grow as needs grow (rads, and Hardcore's dehydration, hunger and
/// sleep deprivation): a "Damage" effect raises them and a "Restore" effect lowers them,
/// never below 0 [hypothesis: RadAway's effect is "Restore Rads" and Nuka-Cola's
/// "Damage Rads"; GECK wiki GetRadiationLevel reads the player's rads with GetAV].
pub fn grows(code: u16) -> bool {
    matches!(code, RAD_LEVEL | DEHYDRATION | HUNGER | SLEEP_DEPRIVATION)
}

/// S.P.E.C.I.A.L. and skills: a negative temporary modifier does not take them below 0
/// [documented: GECK wiki ActorValue].
fn floored(code: u16) -> bool {
    (5..=11).contains(&code) || (32..=45).contains(&code)
}

/// The value change of one effect item, or `None` when its base effect changes no actor
/// value (another archetype, or no actor value named). The EFIT's actor value is used,
/// else the MGEF's [hypothesis; aid-items.md §2]. `potency` multiplies the magnitude of
/// an effect that is neither Hostile nor Detrimental [documented: GECK wiki Ingestible,
/// Medicine; Ingestible Settings].
pub fn value_change(m: &MagicEffectDef, e: &EffectItem, potency: f64, permanent: bool) -> Option<ValueChange> {
    let (parts, code) = match m.archetype {
        // Invisibility and Chameleon change their actor value as a Value Modifier does
        // [hypothesis: the Stealth Boy's "Stealth Field" names the actor value Chameleon
        // with Recover; detection reads that value, docs/design/detection.md].
        ARCHETYPE_VALUE_MODIFIER | ARCHETYPE_INVISIBILITY | ARCHETYPE_CHAMELEON => (false, e.actor_value.or(m.actor_value)?),
        ARCHETYPE_VALUE_AND_PARTS => (true, e.actor_value.or(m.actor_value)?),
        // Every limb condition (the first stands for them) [hypothesis: Doctor's Bag's
        // and Hydra's "NVRestoreLimbs" names no actor value; aid-items.md §3].
        ARCHETYPE_LIMB_CONDITION => (true, *LIMB_CONDITIONS.start()),
        _ => return None,
    };
    let harmful = m.has(MagicEffectDef::HOSTILE) || m.has(MagicEffectDef::DETRIMENTAL);
    let sign = if m.has(MagicEffectDef::DETRIMENTAL) { -1.0 } else { 1.0 };
    let amount = sign * e.magnitude as f64 * if harmful { 1.0 } else { potency };
    let kind = if m.has(MagicEffectDef::RECOVER) && m.archetype != ARCHETYPE_LIMB_CONDITION {
        ChangeKind::Modifier
    } else if e.duration == 0 || permanent {
        ChangeKind::Once
    } else {
        ChangeKind::PerSecond
    };
    Some(ValueChange { code, kind, amount, parts, applied: 0.0, permanent_pool: false, suspended: false })
}

/// The potency of an ingestible's beneficial effects for an actor with these Medicine
/// and Survival skills (0–100): `base + mult × skill / 100` per flag [documented: GECK
/// wiki Ingestible Settings]; with both flags the two multiply [hypothesis].
pub fn potency(flags: u8, medicine: f64, survival: f64, setting: impl Fn(&str) -> f64) -> f64 {
    let mut p = 1.0;
    if flags & IngestibleDef::MEDICINE != 0 {
        p *= setting("fMagicMedicineSkillBase") + setting("fMagicMedicineSkillMult") * medicine / 100.0;
    }
    if flags & IngestibleDef::FOOD != 0 {
        p *= setting("fMagicSurvivalSkillBase") + setting("fMagicSurvivalSkillMult") * survival / 100.0;
    }
    p
}

/// The chance (0–1) that this use addicts: 0 without a withdrawal effect or with an
/// Addiction Chance of 0; with a usage monitor `(usage − threshold) / 100` once the
/// usage reaches the threshold, else 0; without one the Addiction Chance itself
/// [documented: GECK wiki Addiction Formula]. `usage` is the monitor's value after this
/// use [hypothesis]. Perks' "Modify Addiction Chance" is applied by [`Game::ingest`].
pub fn addiction_chance(def: &IngestibleDef, usage: Option<f64>, threshold: f64) -> f64 {
    if def.withdrawal.is_none() || def.addiction_chance == 0.0 {
        return 0.0;
    }
    match usage {
        Some(u) if u < threshold => 0.0,
        Some(u) => ((u - threshold) / 100.0).clamp(0.0, 1.0),
        None => (def.addiction_chance as f64).clamp(0.0, 1.0),
    }
}

/// ETYP "Chems" [documented: xEdit EquipType; GECK wiki Equip Type].
pub const EQUIP_CHEMS: i64 = 10;

/// What changes an ingestible's effects for one use (aid-items.md §4, §10).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mods {
    /// Multiplies the beneficial Value Modifier magnitudes (Medicine and Food potency).
    pub potency: f64,
    /// Multiplies the durations of the beneficial effects that have one, the usage
    /// monitor's excepted (perks' "Modify Positive Chem Duration", chems only).
    pub duration: f64,
    /// Multiplies the rads an effect gives (perks' "Modify Radiation Consumed").
    pub rads: f64,
    /// Applied by a hit (a weapon's, a creature's or a blast's effect,
    /// [`Game::apply_hit_effect`]): the target's resistances cut the harmful magnitudes,
    /// and the same item's active effect is refreshed rather than lengthened
    /// (docs/spec-notes/hit-effects.md §2, §3).
    pub hit: bool,
    /// A worn enchantment: the equipped item it comes from (`crate::abilities`).
    pub worn: Option<FormId>,
}

impl Default for Mods {
    fn default() -> Mods {
        Mods { potency: 1.0, duration: 1.0, rads: 1.0, hit: false, worn: None }
    }
}

impl Mods {
    pub fn potency(potency: f64) -> Mods {
        Mods { potency, ..Mods::default() }
    }

    /// The seconds an effect item lasts under these mods: a beneficial effect (neither
    /// Hostile nor Detrimental) with a duration is lengthened by `duration`, the usage
    /// monitor not [hypothesis: "positive" effects; the monitor counts uses, it is no
    /// effect of the chem].
    pub fn seconds(&self, m: Option<&MagicEffectDef>, e: &EffectItem) -> f64 {
        let beneficial = m.is_some_and(|m| !m.has(MagicEffectDef::HOSTILE) && !m.has(MagicEffectDef::DETRIMENTAL));
        let d = e.duration as f64;
        if beneficial && e.effect != USAGE_MONITOR && d > 0.0 {
            d * self.duration
        } else {
            d
        }
    }

    /// A value change with the rads factor: an effect that gives rads (a "Damage Rads",
    /// whose amount lowers the rads' "health") gives `rads` times as many.
    pub fn on_change(&self, mut c: ValueChange) -> ValueChange {
        if c.code == RAD_LEVEL && c.amount < 0.0 {
            c.amount *= self.rads;
        }
        c
    }
}

/// One effect a magic item started, or an active one it extended
/// ([`Game::apply_magic_item_with`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Applied {
    pub id: u32,
    /// The base effect.
    pub effect: FormId,
    /// Its value change as it took hold (a modifier's amount as cut, potency included).
    pub change: Option<ValueChange>,
    /// Seconds: its own duration, or the extended effect's whole duration.
    pub duration: f64,
    /// The same item's effect was active: this use lengthened it.
    pub extended: bool,
}

/// What using an ingestible did ([`Game::ingest`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ingested {
    /// The effects started (an effect merged into an active one is not listed).
    pub effects: Vec<u32>,
    /// Every effect started or extended, with its value change.
    pub applied: Vec<Applied>,
    pub potency: f64,
    /// The factors this use had (potency, perks).
    pub mods: Mods,
    /// The perks whose entry points changed this use: the perk and the entry point.
    pub perks: Vec<(FormId, u8)>,
    /// The usage monitor's value after this use, when the item has one.
    pub usage: Option<f64>,
    /// The addiction roll: its chance and whether it addicted.
    pub addiction: Option<(f64, bool)>,
}

impl Game<'_> {
    fn aid_setting(&self, name: &str) -> f64 {
        let default = SETTINGS.iter().find(|(n, _)| *n == name).map_or(0.0, |s| s.1);
        crate::combat::setting(self, name, default)
    }

    /// An actor uses an ingestible (the caller has taken it from the inventory): its
    /// effects apply with the item's potency, its usage monitor counts the use, and the
    /// addiction roll is made. docs/spec-notes/aid-items.md §4–§5.
    pub fn ingest(&mut self, actor: FormId, item: FormId) -> Ingested {
        let def = self.data.ingestibles.get(&item).cloned().unwrap_or_default();
        let medicine = fa::actor_value_capped(self, actor, 37);
        let survival = fa::actor_value_capped(self, actor, 44);
        let potency = potency(def.flags, medicine, survival, |n| self.aid_setting(n));
        let mut perks: Vec<(FormId, u8)> = Vec::new();
        let mut perk = |g: &mut Game, point: u8, value: f64| -> f64 {
            let (v, by) = g.entry_point(actor, point, value);
            perks.extend(by.iter().map(|a| (a.perk, point)));
            v
        };
        // Chemist and the like lengthen chems only [hypothesis: the perk's "any chems you
        // take"; the GECK wiki's entry point says "any ingestible"].
        let duration = if def.equip_type == EQUIP_CHEMS { perk(self, crate::perks::MODIFY_POSITIVE_CHEM_DURATION, 1.0) } else { 1.0 };
        let rads = perk(self, crate::perks::MODIFY_RADIATION_CONSUMED, 1.0);
        let mods = Mods { potency, duration, rads, ..Mods::default() };
        self.emit(EventKind::Ingested { actor, item, potency });
        let applied = self.apply_magic_item_with(actor, item, Some(actor), false, &mods);
        if actor == FormId::PLAYER_REF {
            for (code, n) in crate::misc_stats::ingest_counts(&def, &applied) {
                self.count_stat(code, n);
            }
        }
        let effects = applied.iter().filter(|a| !a.extended).map(|a| a.id).collect();
        let has_monitor = self.data.effect_items.get(&item).is_some_and(|l| l.iter().any(|e| e.effect == USAGE_MONITOR));
        let usage = has_monitor.then(|| self.spell_usage(actor, item));
        let threshold = self.aid_setting("fAddictionUsageMonitorThreshold");
        let mut chance = addiction_chance(&def, usage, threshold);
        // "Addiction Chance accounts for the Modify Addiction Chance Perk Entry Point"
        // [documented: GECK wiki Addiction Formula].
        if chance > 0.0 {
            chance = perk(self, crate::perks::MODIFY_ADDICTION_CHANCE, chance).clamp(0.0, 1.0);
        }
        let mut addiction = None;
        if let Some(spell) = def.withdrawal.filter(|_| def.addiction_chance != 0.0) {
            let already = self.extra.get(&actor).is_some_and(|e| e.spells.contains(&spell));
            if !already {
                let hit = chance > 0.0 && ((self.rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64) < chance;
                if hit {
                    self.add_spell(actor, spell);
                    if actor == FormId::PLAYER_REF {
                        self.count_stat(crate::misc_stats::TIMES_ADDICTED, 1.0);
                    }
                    self.emit(EventKind::Addicted { actor, item, spell });
                }
                addiction = Some((chance, hit));
            }
        }
        Ingested { effects, applied, potency, mods, perks, usage, addiction }
    }

    /// AddSpell: the spell's effects apply at once and last until removed; false when
    /// the actor already has it.
    pub fn add_spell(&mut self, actor: FormId, spell: FormId) -> bool {
        let e = self.extra.entry(actor).or_default();
        if e.spells.contains(&spell) {
            return false;
        }
        e.spells.push(spell);
        self.apply_magic_item(actor, spell, None, true);
        true
    }

    /// GetSpellUsageNum: the usage monitor's value for an ingestible on the actor, 0
    /// when none is active [documented: GECK wiki GetSpellUsageNum].
    pub fn spell_usage(&self, actor: FormId, item: FormId) -> f64 {
        self.effects.iter().find(|e| e.target == actor && e.source == item && e.effect == USAGE_MONITOR).map_or(0.0, |e| e.usage)
    }

    /// The addictions (spells of type Addiction) an actor has.
    pub fn addictions(&self, actor: FormId) -> Vec<FormId> {
        let Some(e) = self.extra.get(&actor) else { return Vec::new() };
        e.spells.iter().copied().filter(|s| self.data.spell_types.get(s) == Some(&SPELL_ADDICTION)).collect()
    }

    /// Whether an addiction is sated: an effect (not the usage monitor) of an ingestible
    /// whose withdrawal effect is this spell is active on the actor [hypothesis: GECK
    /// wiki IsSpellTarget, an addicted player who has just had the chem "is not presently
    /// undergoing withdrawal"].
    fn sated(&self, actor: FormId, spell: FormId) -> bool {
        self.effects.iter().any(|e| {
            e.target == actor
                && e.effect != USAGE_MONITOR
                && self.data.ingestibles.get(&e.source).is_some_and(|d| d.withdrawal == Some(spell))
        })
    }

    /// Change an actor value at once by `amount` (positive restores, negative damages);
    /// a value that grows (rads) moves the other way (see [`grows`]).
    fn change_value(&mut self, actor: FormId, code: u16, amount: f64, parts: bool, caster: Option<FormId>) {
        if code == crate::actor_values::HEALTH && amount < 0.0 {
            // Health damage can kill (a burn, a poison; hit-effects.md §4).
            self.effect_health_damage(actor, -amount, caster);
        } else if grows(code) {
            let now = fa::actor_value(self, actor, code).unwrap_or(0.0);
            let to = (now - amount).max(0.0);
            let _ = fa::shift_base(self, actor, code, to - now);
        } else if amount >= 0.0 {
            fa::restore(self, actor, code, amount);
            if parts {
                // [hypothesis: the limbs get the same amount; MGEF "Restore Health &
                // Conditions"].
                for limb in LIMB_CONDITIONS.filter(|&l| l != code) {
                    fa::restore(self, actor, limb, amount);
                }
            }
        } else {
            fa::add_damage(self, actor, code, -amount);
        }
    }

    fn change_of(&self, id: u32) -> Option<(FormId, ValueChange)> {
        self.effects.iter().find(|e| e.id == id).and_then(|e| Some((e.target, e.change?)))
    }

    fn set_change(&mut self, id: u32, c: ValueChange) {
        if let Some(e) = self.effects.iter_mut().find(|e| e.id == id) {
            e.change = Some(c);
        }
    }

    /// Put a modifier in its pool; a negative one on S.P.E.C.I.A.L. or a skill is cut so
    /// the value stays at 0 or more [documented: GECK wiki ActorValue].
    fn hold_modifier(&mut self, target: FormId, c: &mut ValueChange) {
        let mut amount = c.amount;
        if amount < 0.0 && floored(c.code) {
            let now = fa::actor_value(self, target, c.code).unwrap_or(0.0).max(0.0);
            amount = amount.max(-now);
        }
        if c.permanent_pool {
            let _ = fa::add_modifier(self, target, c.code, amount);
        } else {
            fa::add_temporary(self, target, c.code, amount);
        }
        c.applied = amount;
    }

    fn release_modifier(&mut self, target: FormId, c: &mut ValueChange) {
        if c.applied != 0.0 {
            if c.permanent_pool {
                let _ = fa::add_modifier(self, target, c.code, -c.applied);
            } else {
                fa::add_temporary(self, target, c.code, -c.applied);
            }
        }
        c.applied = 0.0;
    }

    /// An effect started: its value change takes hold.
    pub(crate) fn start_change(&mut self, id: u32) {
        let Some((target, mut c)) = self.change_of(id) else { return };
        let (permanent, source, index) = match self.effects.iter().find(|e| e.id == id) {
            Some(e) => (e.permanent, e.source, e.index),
            None => return,
        };
        match c.kind {
            ChangeKind::Modifier => {
                // A player's ability without conditions changes the permanent pool
                // [documented: GECK wiki ActorValue, the Life Giver example].
                let unconditional = self.data.effect_items.get(&source).and_then(|l| l.get(index)).is_none_or(|e| e.conditions.is_empty());
                let ability = permanent && self.data.spell_types.get(&source) == Some(&4);
                c.permanent_pool = ability && target == FormId::PLAYER_REF && unconditional;
                c.suspended = self.data.spell_types.get(&source) == Some(&SPELL_ADDICTION) && self.sated(target, source);
                if !c.suspended {
                    self.hold_modifier(target, &mut c);
                }
            }
            ChangeKind::Once => {
                let caster = self.effects.iter().find(|e| e.id == id).and_then(|e| e.caster);
                self.change_value(target, c.code, c.amount, c.parts, caster);
                c.applied = c.amount;
            }
            ChangeKind::PerSecond => {}
        }
        self.set_change(id, c);
    }

    /// An effect is about to end: a modifier leaves its pool.
    pub(crate) fn end_change(&mut self, id: u32) {
        let Some((target, mut c)) = self.change_of(id) else { return };
        if c.kind == ChangeKind::Modifier {
            self.release_modifier(target, &mut c);
            self.set_change(id, c);
        }
    }

    /// One frame of an effect that changes a value by the second: the share of the
    /// seconds from `before` to its elapsed time that lie inside its duration.
    pub(crate) fn tick_change(&mut self, id: u32, before: f64) {
        let Some((target, mut c)) = self.change_of(id) else { return };
        if c.kind != ChangeKind::PerSecond {
            return;
        }
        let Some(e) = self.effects.iter().find(|e| e.id == id) else { return };
        let seconds = e.elapsed.min(e.duration) - before.min(e.duration);
        if seconds <= 0.0 {
            return;
        }
        let caster = e.caster;
        let amount = c.amount * seconds;
        self.change_value(target, c.code, amount, c.parts, caster);
        c.applied += amount;
        self.set_change(id, c);
    }

    /// Addictions are held back while their chem is active, and come back when it ends.
    pub(crate) fn refresh_suspended(&mut self, _dt: f64) {
        let ids: Vec<(u32, FormId, FormId)> = self
            .effects
            .iter()
            .filter(|e| e.permanent && e.change.is_some_and(|c| c.kind == ChangeKind::Modifier))
            .filter(|e| self.data.spell_types.get(&e.source) == Some(&SPELL_ADDICTION))
            .map(|e| (e.id, e.target, e.source))
            .collect();
        for (id, target, spell) in ids {
            let Some((_, mut c)) = self.change_of(id) else { continue };
            let sated = self.sated(target, spell);
            if sated == c.suspended {
                continue;
            }
            if sated {
                self.release_modifier(target, &mut c);
            } else {
                self.hold_modifier(target, &mut c);
            }
            c.suspended = sated;
            self.set_change(id, c);
        }
    }

    /// What an effect's archetype does when it starts, besides value changes and scripts:
    /// Cure Addiction removes the target's addictions [hypothesis from the archetype's
    /// name; aid-items.md §5.4].
    pub(crate) fn effect_archetype_start(&mut self, id: u32) {
        let Some(e) = self.effects.iter().find(|e| e.id == id) else { return };
        let (target, effect) = (e.target, e.effect);
        let archetype = self.data.magic_effects.get(&effect).map(|m| m.archetype);
        if archetype == Some(ARCHETYPE_CURE_ADDICTION) {
            for spell in self.addictions(target) {
                if let Some(x) = self.extra.get_mut(&target) {
                    x.spells.retain(|&s| s != spell);
                }
                self.dispel(target, None, |e| e.source == spell);
                self.emit(EventKind::AddictionCured { actor: target, spell });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mgef(archetype: i64, flags: u32, av: Option<u16>) -> MagicEffectDef {
        MagicEffectDef { flags, archetype, script: None, actor_value: av, shader: None, resistance: None }
    }

    fn item(magnitude: u32, duration: u32, av: Option<u16>) -> EffectItem {
        EffectItem { effect: FormId(0x800), magnitude, area: 0, duration, actor_value: av, conditions: Vec::new() }
    }

    const R: u32 = MagicEffectDef::RECOVER;
    const D: u32 = MagicEffectDef::DETRIMENTAL;
    const H: u32 = MagicEffectDef::HOSTILE;

    #[test]
    fn the_flags_and_duration_pick_the_kind() {
        // Buffout's Strength: Recover, a duration.
        let c = value_change(&mgef(0, R, Some(5)), &item(2, 240, Some(5)), 1.0, false).unwrap();
        assert_eq!((c.kind, c.amount, c.code), (ChangeKind::Modifier, 2.0, 5));
        // Hardcore Stimpak: no Recover, 5 a second for 6 s.
        let c = value_change(&mgef(0, 0, Some(16)), &item(5, 6, Some(16)), 1.3, false).unwrap();
        assert_eq!(c.kind, ChangeKind::PerSecond);
        assert!((c.amount - 6.5).abs() < 1e-9);
        // Value And Parts at once; withdrawal's Reduce Strength is a negative modifier.
        let c = value_change(&mgef(34, 0, Some(16)), &item(30, 0, Some(16)), 1.0, false).unwrap();
        assert_eq!((c.kind, c.parts), (ChangeKind::Once, true));
        let c = value_change(&mgef(0, R | D, Some(5)), &item(1, 0, Some(5)), 2.0, true).unwrap();
        assert_eq!((c.kind, c.amount), (ChangeKind::Modifier, -1.0));
        // Harmful effects ignore potency; other archetypes change no value.
        let c = value_change(&mgef(0, H | D, Some(54)), &item(3, 0, Some(54)), 2.0, false).unwrap();
        assert_eq!(c.amount, -3.0);
        assert!(value_change(&mgef(1, 0, None), &item(0, 0, None), 1.0, false).is_none());
        // Limb Condition: every limb, by the second here (Hydra).
        let c = value_change(&mgef(35, 0, None), &item(10, 60, None), 1.0, false).unwrap();
        assert_eq!((c.code, c.parts, c.kind), (25, true, ChangeKind::PerSecond));
        // The EFIT's actor value, else the MGEF's.
        assert_eq!(value_change(&mgef(0, 0, Some(16)), &item(1, 0, None), 1.0, false).unwrap().code, 16);
        assert_eq!(value_change(&mgef(0, 0, Some(16)), &item(1, 0, Some(20)), 1.0, false).unwrap().code, 20);
    }

    #[test]
    fn potency_follows_the_wiki_examples() {
        let s = |n: &str| SETTINGS.iter().find(|x| x.0 == n).unwrap().1;
        // Ingestible Settings: Survival 55 → 2.1; Medicine 10 → 1.2.
        assert!((potency(IngestibleDef::FOOD, 0.0, 55.0, s) - 2.1).abs() < 1e-9);
        assert!((potency(IngestibleDef::MEDICINE, 10.0, 0.0, s) - 1.2).abs() < 1e-9);
        assert_eq!(potency(0, 100.0, 100.0, s), 1.0);
    }

    #[test]
    fn addiction_chance_follows_the_formula() {
        let def = IngestibleDef { withdrawal: Some(FormId(0x900)), addiction_chance: 0.1, ..Default::default() };
        assert!((addiction_chance(&def, None, 70.0) - 0.1).abs() < 1e-6);
        assert_eq!(addiction_chance(&def, Some(60.0), 70.0), 0.0);
        assert!((addiction_chance(&def, Some(90.0), 70.0) - 0.2).abs() < 1e-9);
        let none = IngestibleDef { addiction_chance: 0.1, ..Default::default() };
        assert_eq!(addiction_chance(&none, Some(300.0), 70.0), 0.0);
        let zero = IngestibleDef { withdrawal: Some(FormId(0x900)), ..Default::default() };
        assert_eq!(addiction_chance(&zero, Some(300.0), 70.0), 0.0);
    }
}
