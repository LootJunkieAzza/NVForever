//! What magic effects do to actor values (docs/spec-notes/actor-values.md §9).
//!
//! A base effect of the "Value Modifier" archetype (or "Value And Parts") changes one
//! actor value by the effect item's magnitude [documented: GECK wiki ActorValue]:
//!
//! - with the **Recover** flag the change lasts while the effect is active and goes
//!   when it ends: the **temporary modifier**. For the player, an effect without
//!   conditions of an actor effect of type Ability goes to the **permanent modifier**
//!   instead [documented: ActorValue, "but only for the player character"];
//! - without Recover the change stays: it goes to the **damage** pool, which a
//!   detrimental effect fills and any other effect empties (`Game::apply_value_effect`).
//!
//! A base effect with the Detrimental flag lowers the value, any other raises it
//! [hypothesis: the flag's name (xEdit); the wiki page of base effects is not in the
//! owner's export].
//!
//! The modifiers are not stored: they are read off what is active on the actor when a
//! value is asked for ([`Game::effect_modifiers`]). Three things are active on an actor:
//!
//! 1. the abilities of its record's actor effect list (SPLO; the template's list with
//!    the ACBS template flag "Actor Effect List") [documented: xEdit]. This is where a
//!    creature's Damage Threshold and Damage Resistance come from [observed: `fnvtool
//!    actor values --survey`, "ability_effects"];
//! 2. the object effect (ARMO EITM) of each armour or clothing item marked as equipped
//!    in its inventory;
//! 3. the effects started on it by a consumed ingestible, an added or a cast spell
//!    (`Game::effects`), while they last.
//!
//! An effect item's conditions are evaluated with the actor as their subject: for 1
//! and 2 each time the value is read [hypothesis: the game re-evaluates them from time
//! to time while the effect is on the actor], for 3 when the effect starts (scripts.md
//! §7.6).

use crate::actor_values::{SKILLS, SPECIALS};
use crate::data::{
    EffectItem, GameData, MagicEffectDef, ARCHETYPE_VALUE_AND_PARTS, ARCHETYPE_VALUE_MODIFIER, MGEF_DETRIMENTAL, MGEF_RECOVER, MGEF_USE_ATTRIBUTE,
    MGEF_USE_SKILL, SPELL_ABILITY,
};
use crate::functions::base_of;
use crate::game::Game;
use fnv_sim::FormId;

/// The actor value an effect item changes: the base effect's, or the effect item's own
/// when the base effect leaves the choice to it (flags "Use skill", "Use attribute")
/// [documented: xEdit MGEF DATA and EFIT fields; hypothesis: that those flags hand the
/// choice to the item; no effect of FalloutNV.esm sets them, observed]. `None` for an
/// effect that is no value modifier.
pub fn effect_value(def: &MagicEffectDef, item: &EffectItem) -> Option<u16> {
    if def.archetype != ARCHETYPE_VALUE_MODIFIER && def.archetype != ARCHETYPE_VALUE_AND_PARTS {
        return None;
    }
    let own = (def.flags & (MGEF_USE_SKILL | MGEF_USE_ATTRIBUTE) != 0).then_some(item.actor_value).flatten();
    own.or(def.actor_value)
}

/// What one effect item does to an actor value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueChange {
    pub code: u16,
    /// Added to the value: negative for a detrimental effect.
    pub amount: f64,
    /// Recover: the change goes when the effect ends.
    pub recover: bool,
}

/// The change an effect item makes at the given magnitude, if it is a value modifier.
pub fn value_change(data: &GameData, item: &EffectItem, magnitude: f64) -> Option<ValueChange> {
    let def = data.magic_effects.get(&item.effect)?;
    let code = effect_value(def, item)?;
    let amount = if def.flags & MGEF_DETRIMENTAL != 0 { -magnitude } else { magnitude };
    Some(ValueChange { code, amount, recover: def.flags & MGEF_RECOVER != 0 })
}

/// The modifiers effects add to an actor value.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EffectModifiers {
    /// The temporary modifier.
    pub temporary: f64,
    /// The share that counts as permanent: the player's abilities without conditions.
    pub permanent: f64,
}

impl<'a> Game<'a> {
    /// The actor effect list of `r`'s record (the template's with the template flag
    /// "Actor Effect List").
    pub fn actor_effect_list(&self, r: FormId) -> &'a [FormId] {
        self.def_following(r, |d| d.template_effects).map_or(&[], |d| d.actor_effects.as_slice())
    }

    /// The object effects of what `r` wears: the ARMO EITM of each item marked as
    /// equipped in its inventory. An actor that was never given an inventory wears
    /// nothing here.
    fn worn_object_effects(&self, r: FormId) -> Vec<FormId> {
        let data = self.data;
        let Some(inv) = self.world.peek(r).and_then(|s| s.inventory.as_ref()) else { return Vec::new() };
        inv.equipped().into_iter().filter_map(|i| data.enchantments.get(&i).copied()).collect()
    }

    /// What effects add to `code` of `actor` now (see the module notes). Effects whose
    /// conditions read an actor value are evaluated without the conditioned effects, so
    /// that a condition cannot depend on itself.
    pub fn effect_modifiers(&mut self, actor: FormId, code: u16) -> EffectModifiers {
        let data = self.data;
        let mut out = EffectModifiers::default();
        if base_of(self, actor).is_none_or(|b| !data.actors.contains_key(&b)) {
            return out;
        }
        let player = actor == FormId::PLAYER_REF;
        // 3. Started effects: their conditions passed when they started.
        let mut started: Vec<FormId> = Vec::new();
        for e in self.effects.iter().filter(|e| e.target == actor) {
            started.push(e.source);
            let Some(item) = data.effect_items.get(&e.source).and_then(|l| l.get(e.index)) else { continue };
            let Some(c) = value_change(data, item, e.magnitude as f64).filter(|c| c.recover && c.code == code) else { continue };
            if player && item.conditions.is_empty() && data.spell_types.get(&e.source) == Some(&SPELL_ABILITY) {
                out.permanent += c.amount;
            } else {
                out.temporary += c.amount;
            }
        }
        // 1. and 2. The record's abilities and worn object effects. An ability that was
        // also added by a script counts once.
        let abilities = self.actor_effect_list(actor).iter().copied().filter(|s| data.spell_types.get(s) == Some(&SPELL_ABILITY) && !started.contains(s));
        let sources: Vec<(FormId, bool)> = abilities.map(|s| (s, true)).chain(self.worn_object_effects(actor).into_iter().map(|s| (s, false))).collect();
        for (source, ability) in sources {
            for (index, item) in data.effect_items.get(&source).into_iter().flatten().enumerate() {
                let Some(c) = value_change(data, item, item.magnitude as f64).filter(|c| c.recover && c.code == code) else { continue };
                if !item.conditions.is_empty() {
                    if self.reading_effect_conditions {
                        continue;
                    }
                    self.reading_effect_conditions = true;
                    let label = format!("{} effect {index}", data.name(source));
                    let pass = self.conditions_pass(&item.conditions, Some(actor), None, &label);
                    self.reading_effect_conditions = false;
                    if !pass {
                        continue;
                    }
                }
                if player && ability && item.conditions.is_empty() {
                    out.permanent += c.amount;
                } else {
                    out.temporary += c.amount;
                }
            }
        }
        out
    }

    /// A value modifier without Recover takes effect on `target`: a detrimental one adds
    /// `amount` to the value's damage, any other takes it off the damage and never past
    /// zero [documented: GECK wiki ActorValue, the damage pool "can only hold a negative
    /// modifier" and is fed by such effects].
    pub(crate) fn apply_value_effect(&mut self, target: FormId, c: ValueChange, amount: f64) {
        if c.recover || amount <= 0.0 {
            return;
        }
        let d = crate::functions::actor::damage(self, target, c.code);
        let d = if c.amount < 0.0 { d + amount } else { (d - amount).max(0.0) };
        crate::functions::actor::set_damage(self, target, c.code, d);
    }
}

/// A negative temporary modifier cannot take a skill or a S.P.E.C.I.A.L. value below 0
/// [documented: GECK wiki ActorValue]: the modifier as it counts on top of `rest`, the
/// value without it.
pub fn limited_temporary(code: u16, temporary: f64, rest: f64) -> f64 {
    if temporary < 0.0 && (SKILLS.contains(&code) || SPECIALS.contains(&code)) {
        temporary.max(-rest.max(0.0))
    } else {
        temporary
    }
}
