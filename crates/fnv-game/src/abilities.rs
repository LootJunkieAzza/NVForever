//! An actor's authored abilities and the enchantments of what it wears, as active
//! effects (docs/spec-notes/aid-items.md §13).
//!
//! - Authored abilities ([`Game::give_abilities`]): the actor effects of type Ability in
//!   the SPLO list of the actor's record (or of the template that supplies its "Actor
//!   Effect List") and of its race are added as AddSpell adds a spell, once per actor,
//!   the first time the actor is processed (loaded, or asked about by a script). They
//!   join the actor's spell list, so RemoveSpell takes them away and they are saved
//!   with it. A race change (the character creator) swaps the race's abilities.
//! - Worn enchantments ([`Game::sync_worn`]): the object effect (ENCH of type Apparel)
//!   of each equipped armour piece is active on the wearer while it is equipped, and
//!   ends when it is unequipped or leaves the inventory.
//! - Effects of abilities and worn enchantments whose conditions fail stay listed but
//!   inactive (no value change, no script) and their conditions are checked again
//!   every second ([`Game::recheck_conditions`]).

use crate::game::Game;
use fnv_sim::FormId;

/// SPEL SPIT type "Ability" [documented: schema `combat.json` SPIT].
pub const SPELL_ABILITY: i64 = 4;
/// ENCH ENIT type "Apparel" [documented: schema `combat.json`; observed: all 186 ARMO EITM of
/// FalloutNV.esm name one of this type, all 59 WEAP EITM one of type 2 Weapon].
pub const ENCHANTMENT_APPAREL: u32 = 3;
/// Seconds between checks of an ability's or a worn enchantment's conditions
/// [hypothesis: no source gives the interval].
pub const CONDITION_INTERVAL: f64 = 1.0;

impl Game<'_> {
    /// The race whose abilities the actor has: the character creator's choice for the
    /// player, else the record's (template flag "Traits").
    pub fn ability_race(&self, r: FormId) -> Option<FormId> {
        let chosen = self.player.look.as_ref().filter(|_| r == FormId::PLAYER_REF).map(|l| l.race);
        chosen.or_else(|| self.traits_def(r).and_then(|d| d.race))
    }

    /// The abilities among a list of actor effects: type Ability only. An SPLO entry of
    /// type Actor Effect (the 26 in FalloutNV.esm: Ruthless, auras with an area) is a
    /// spell the actor could cast, not a constant effect [hypothesis].
    fn abilities_of<'s>(&'s self, spells: &'s [FormId]) -> impl Iterator<Item = FormId> + 's {
        spells.iter().copied().filter(|s| self.data.spell_types.get(s) == Some(&SPELL_ABILITY))
    }

    /// The abilities of the actor's record: the SPLO list of the record that supplies
    /// its actor effects (template flag "Actor Effect List") [documented: schema ACBS
    /// template flags; observed: GSVictorRef's base Victor sets the flag and its
    /// template VSecuritronMk1TEMPLATE lists the same three abilities].
    pub fn record_abilities(&self, r: FormId) -> Vec<FormId> {
        self.def_following(r, |d| d.template_spells).map_or_else(Vec::new, |d| self.abilities_of(&d.spells).collect())
    }

    /// The abilities of a race (RACE SPLO).
    pub fn race_abilities(&self, race: Option<FormId>) -> Vec<FormId> {
        race.and_then(|r| self.data.race_spells.get(&r)).map_or_else(Vec::new, |l| self.abilities_of(l).collect())
    }

    /// Give an actor its authored abilities the first time it is processed, and its
    /// race's again after a race change. Not for references that are no actor.
    pub fn give_abilities(&mut self, r: FormId) {
        if r != FormId::PLAYER_REF && self.def_following(r, |d| d.template_spells).is_none() {
            return;
        }
        let race = self.ability_race(r);
        let (given, had_race) = self.extra.get(&r).map_or((false, None), |e| (e.abilities_given, e.ability_race));
        if given && had_race == race {
            return;
        }
        // Mark first: applying an effect reads actor values, which come back here.
        {
            let e = self.extra.entry(r).or_default();
            e.abilities_given = true;
            e.ability_race = race;
        }
        if given {
            // A new race: the old race's abilities go (unless the record has them too).
            let own = self.record_abilities(r);
            for spell in self.race_abilities(had_race) {
                if own.contains(&spell) {
                    continue;
                }
                if let Some(x) = self.extra.get_mut(&r) {
                    x.spells.retain(|&s| s != spell);
                }
                self.dispel(r, None, |e| e.source == spell && e.worn.is_none());
            }
        } else {
            for spell in self.record_abilities(r) {
                self.add_spell(r, spell);
            }
        }
        for spell in self.race_abilities(race) {
            self.add_spell(r, spell);
        }
    }

    /// The enchanted armour an actor wears: (item, its Apparel enchantment). Reads the
    /// inventory as it is; one not yet made (an actor never loaded) wears nothing.
    pub fn worn_enchantments(&self, r: FormId) -> Vec<(FormId, FormId)> {
        let Some(inv) = self.world.peek(r).and_then(|s| s.inventory.as_ref()) else { return Vec::new() };
        let mut out: Vec<(FormId, FormId)> = inv
            .equipped()
            .into_iter()
            .filter_map(|item| {
                let ench = *self.data.armor_enchantments.get(&item)?;
                (self.data.enchantment_types.get(&ench) == Some(&ENCHANTMENT_APPAREL) && self.data.effect_items.contains_key(&ench)).then_some((item, ench))
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The worn enchantments' effects follow what the actor has equipped: an item's
    /// enchantment starts when it is worn and ends (its finish blocks run) when it is
    /// not [documented: GECK wiki ActorValue, armour's object effects alter stats "until
    /// you take the armor off"; hypothesis: they begin at once].
    pub fn sync_worn(&mut self, r: FormId) {
        let wanted = self.worn_enchantments(r);
        let mut have: Vec<(FormId, FormId)> = self.effects.iter().filter(|e| e.target == r).filter_map(|e| Some((e.worn?, e.source))).collect();
        have.sort();
        have.dedup();
        for &(item, ench) in &have {
            if !wanted.contains(&(item, ench)) {
                self.dispel(r, None, |e| e.worn == Some(item) && e.source == ench);
            }
        }
        for &(item, ench) in &wanted {
            if !have.contains(&(item, ench)) {
                let mods = crate::aid::Mods { worn: Some(item), ..crate::aid::Mods::default() };
                self.apply_magic_item_with(r, ench, Some(r), true, &mods);
            }
        }
    }

    /// An actor's abilities and worn enchantments brought up to date: before a script
    /// asks about its effects or values, and every frame for the loaded actors. Inside
    /// the effects' own start (a value read while a modifier is put in place) it does
    /// nothing.
    pub fn sync_actor_effects(&mut self, r: FormId) {
        if self.syncing_effects {
            return;
        }
        self.syncing_effects = true;
        self.give_abilities(r);
        self.sync_worn(r);
        self.syncing_effects = false;
    }

    /// Every frame: the player and the actors in the loaded cells.
    pub(crate) fn sync_loaded_actor_effects(&mut self) {
        let mut actors = self.package_actors();
        actors.insert(FormId::PLAYER_REF);
        for a in actors {
            self.sync_actor_effects(a);
        }
    }

    /// An inactive effect whose conditions now pass takes hold (value change, archetype,
    /// ScriptEffectStart); an active one whose conditions fail lets go (its modifier
    /// leaves its pool, ScriptEffectFinish runs) and stays listed [hypothesis: the
    /// effect is suspended, not removed; aid-items.md §13].
    pub(crate) fn recheck_conditions(&mut self) {
        if self.time + 1e-9 < self.condition_check_due {
            return;
        }
        self.condition_check_due = self.time + CONDITION_INTERVAL;
        let data = self.data;
        let ids: Vec<(u32, FormId, FormId, usize, Option<FormId>, bool)> = self
            .effects
            .iter()
            .filter(|e| e.permanent)
            .filter(|e| data.effect_items.get(&e.source).and_then(|l| l.get(e.index)).is_some_and(|i| !i.conditions.is_empty()))
            .map(|e| (e.id, e.target, e.source, e.index, e.caster, e.inactive))
            .collect();
        for (id, target, source, index, caster, inactive) in ids {
            let Some(item) = data.effect_items.get(&source).and_then(|l| l.get(index)) else { continue };
            let label = format!("{} effect {index}", data.name(source));
            let pass = self.conditions_pass(&item.conditions, Some(target), caster, &label);
            if pass && inactive {
                self.activate_effect(id);
            } else if !pass && !inactive {
                self.deactivate_effect(id);
            }
        }
    }

    pub(crate) fn activate_effect(&mut self, id: u32) {
        let Some(e) = self.effects.iter_mut().find(|e| e.id == id) else { return };
        e.inactive = false;
        self.start_change(id);
        self.effect_archetype_start(id);
        self.effect_started(id);
    }

    pub(crate) fn deactivate_effect(&mut self, id: u32) {
        self.effect_finished(id);
        self.end_change(id);
        if let Some(e) = self.effects.iter_mut().find(|e| e.id == id) {
            e.inactive = true;
        }
    }
}
