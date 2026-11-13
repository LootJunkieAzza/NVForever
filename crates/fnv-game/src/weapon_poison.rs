//! Weapon poisons: the player puts a poison on the weapon in hand, and the weapon's next
//! hits give the poison's effects to whoever they strike.
//!
//! - **A poison** is an ingestible all of whose effects are Detrimental and Hostile
//!   [documented: GECK wiki Ingestible, Notes]. Used from the inventory it is not drunk:
//!   it goes on the weapon in hand ([`Game::use_poison`]).
//! - **Which weapons take one**: those of the Melee Weapons or Unarmed skill [hypothesis:
//!   the master's only setting on poisons, `sPoisonNoWeaponMessage`, says "a weapon that
//!   can be poisoned" and names none; the masters' poisons are blade venoms].
//! - **No such weapon in hand**: the poison stays in the inventory and the setting's own
//!   words say why [observed: FalloutNV.esm `sPoisonNoWeaponMessage`].
//! - **Asking first**: the original asks before it poisons the weapon [documented: Stewie's
//!   Tweaks' readme, bNoPoisonConfirm, "Remove the confirmation prompt when poisoning a
//!   weapon"]. NVForever's question is a second use: the first use names the weapon and keeps
//!   the poison, the same poison used again puts it on [design: our own way and words, as
//!   NVForever's other confirmations]. The player's No Poison Confirmation tweak puts it on at
//!   the first use.
//! - **How long it lasts**: [`POISON_HITS`] hits that land [hypothesis: not in the data;
//!   observer check 107]. A second poison on the same weapon replaces the first.
//! - Kept with the save, for each weapon (`PlayerState::weapon_poisons`); the scripts'
//!   GetEquippedWeaponPoison answers it for the player's weapon in hand.

use crate::game::Game;
use crate::data::MagicEffectDef;
use fnv_sim::FormId;

/// Hits a poison lasts [hypothesis: observer check 107].
pub const POISON_HITS: u32 = 5;

/// The poison on one of the player's weapons and the hits it has left (named fields, so a
/// save's remap reads the poison as a form and the hits as a number).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WeaponPoison {
    pub poison: FormId,
    pub hits: u32,
}

/// What a use of a poison did ([`Game::use_poison`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoisonUse {
    /// Put on the weapon in hand; the item is used up.
    Applied { weapon: FormId },
    /// The first use asks: the weapon it would go on. The item goes back.
    Asks { weapon: FormId },
    /// No weapon in hand takes a poison. The item goes back.
    NoWeapon,
}

impl Game<'_> {
    /// Whether `item` is a poison: an ingestible with effects, all of them Detrimental and
    /// Hostile [documented: GECK wiki Ingestible].
    pub fn is_poison(&self, item: FormId) -> bool {
        if !self.data.ingestibles.contains_key(&item) {
            return false;
        }
        let Some(effects) = self.data.effect_items.get(&item).filter(|e| !e.is_empty()) else { return false };
        effects.iter().all(|e| self.data.magic_effects.get(&e.effect).is_some_and(|m| m.has(MagicEffectDef::DETRIMENTAL) && m.has(MagicEffectDef::HOSTILE)))
    }

    /// Whether `weapon` takes a poison: a Melee Weapons or Unarmed weapon [hypothesis, module notes].
    pub fn poisonable(&self, weapon: FormId) -> bool {
        self.data.weapon_skills.get(&weapon).is_some_and(|&s| s == i64::from(crate::actor_values::MELEE_WEAPONS) || s == i64::from(crate::actor_values::UNARMED))
    }

    /// The player used the poison `item` (already taken from the inventory): it goes on the
    /// weapon in hand, or asks first, or there is no weapon for it (module notes). The
    /// caller gives the item back unless it was [`PoisonUse::Applied`].
    pub fn use_poison(&mut self, item: FormId) -> PoisonUse {
        let p = FormId::PLAYER_REF;
        let Some(weapon) = crate::functions::items::equipped_weapon(self, p).filter(|&w| self.poisonable(w)) else {
            self.player.poison_asked = None;
            return PoisonUse::NoWeapon;
        };
        let asked = self.player.poison_asked.take() == Some((item, weapon));
        if !asked && !self.tweaks.no_poison_confirm {
            self.player.poison_asked = Some((item, weapon));
            return PoisonUse::Asks { weapon };
        }
        self.player.weapon_poisons.insert(weapon, WeaponPoison { poison: item, hits: POISON_HITS });
        tracing::info!("poison: {} on the player's {} for {POISON_HITS} hits", self.data.name(item), self.data.name(weapon));
        PoisonUse::Applied { weapon }
    }

    /// The poison on the player's `weapon`, and the hits it has left.
    pub fn poison_on(&self, weapon: FormId) -> Option<(FormId, u32)> {
        self.player.weapon_poisons.get(&weapon).map(|w| (w.poison, w.hits))
    }

    /// A hit by the player with `weapon` landed: the poison on it, if any, for the hit to
    /// give, one hit fewer left; the last one takes it off.
    pub fn take_poison_hit(&mut self, weapon: FormId) -> Option<FormId> {
        let w = self.player.weapon_poisons.get_mut(&weapon)?;
        let poison = w.poison;
        w.hits = w.hits.saturating_sub(1);
        if w.hits == 0 {
            self.player.weapon_poisons.remove(&weapon);
            tracing::info!("poison: the player's {} has used its {}", self.data.name(weapon), self.data.name(poison));
        }
        Some(poison)
    }

    /// Any other use of the inventory forgets a pending question (a poison used once).
    pub fn forget_poison_question(&mut self) {
        self.player.poison_asked = None;
    }
}
