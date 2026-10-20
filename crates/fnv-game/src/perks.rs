//! Perk entry points: a perk the player has changes a number the game is about to use
//! (docs/spec-notes/aid-items.md §10). Only the entry points of aid items and of
//! experience (docs/spec-notes/experience.md §2) are asked for here; the others (combat's, `fnv_combat::damage::PerkHooks`) are not yet read from the
//! records.
//!
//! An entry point effect of a PERK names the entry point, a function (Set, Add,
//! Multiply) and a number; the function is applied to the value the game has calculated
//! [documented: GECK wiki Entry Point, "the Add Value function … modify the current
//! value"]. Which code is which entry point [observed: the one perk of the masters with
//! code 23 is Chem Resistant, "50% less likely to develop an addiction", Multiply 0.5;
//! code 25 is Chemist, "chems … last twice as long", Multiply 2, and the chem challenge
//! perk, Multiply 1.33; code 44 is Lead Belly, "50% less radiation when consuming",
//! Multiply 0.5].
//!
//! Rules that are ours [hypothesis]: of a perk with several ranks only the entries of the
//! highest rank owned apply (the masters' ranked entries each hold the whole bonus,
//! Demolition Expert 1.2, 1.4, 1.6); entries apply in ascending priority, then perk
//! FormID; the perk owner's conditions (tab 0) are evaluated on the owner; an entry with
//! conditions on another tab (the item, the weapon, the target) is not applied, as no
//! aid entry point of the masters has any.

use crate::data::{PerkEffect, PerkEntry};
use crate::game::Game;
use fnv_sim::FormId;

/// "Adjust Limb Damage" [documented: GECK wiki Entry Point, the name; observed: the
/// two perks of FalloutNV.esm with code 6 are the trait Small Frame, "your limbs are
/// more easily crippled", Multiply 1.25, and the challenge perk Tough Guy, "breaking
/// bones has led them to become stronger", Multiply 0.8; hypothesis: it runs for the
/// actor whose limb is hit] (`Game::damage_limb`, docs/spec-notes/crippled-limbs.md).
pub const ADJUST_LIMB_DAMAGE: u8 = 6;
/// "Adjust Experience Points" [documented: GECK wiki Entry Point, "Run when the player
/// gains any experience", the example Swift Learner; observed: Swift Learner's and
/// Skilled's entries have code 9] (`crate::experience::award`).
pub const ADJUST_EXPERIENCE_POINTS: u8 = 9;
/// "Adjust Gained Skill Points" [documented: GECK wiki Entry Point, "Run when the player
/// gains a level; Modifies the skill rate of the player", the example Educated; observed:
/// the two entries of the masters with code 10 are Educated's Add 2, "two more skill
/// points every time you advance in level", and Logan's Loophole's Set 0 under GetLevel >
/// 30, "after 30 you can kiss … skill points goodbye"] (`crate::levelup::points_for`).
pub const ADJUST_GAINED_SKILL_POINTS: u8 = 10;
/// "Adjust Book Skill Points" [documented: GECK wiki Entry Point, "Run when the actor
/// reads a book; Modifies the skill points gained", the example Comprehension; observed:
/// Comprehension's one entry has code 11, Add 1]. Not asked for yet: reading skill
/// books is not built (docs/spec-notes/levelup.md §9).
pub const ADJUST_BOOK_SKILL_POINTS: u8 = 11;
/// "Modify Addiction Chance" [observed: Chem Resistant; documented: GECK wiki Entry
/// Point, "Run when the actor takes a drug"].
pub const MODIFY_ADDICTION_CHANCE: u8 = 23;
/// "Modify Positive Chem Duration" (Vegas) [observed: Chemist; documented: GECK wiki
/// Entry Point, "Modifies the duration of the ingestible"].
pub const MODIFY_POSITIVE_CHEM_DURATION: u8 = 25;
/// "Modify Radiation Consumed" [observed: Lead Belly; documented: GECK wiki Entry
/// Point, "Modifies the radiation from ingestibles"].
pub const MODIFY_RADIATION_CONSUMED: u8 = 44;

/// Entry point functions [observed: see `EntryPointDef::function`; hypothesis for the
/// names].
pub const SET_VALUE: u8 = 1;
pub const ADD_VALUE: u8 = 2;
pub const MULTIPLY_VALUE: u8 = 3;

/// One entry that applied: the perk, and the value after it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Applied {
    pub perk: FormId,
    pub value: f64,
}

/// The function of one entry on a value.
pub fn apply_function(function: u8, number: f64, value: f64) -> Option<f64> {
    match function {
        SET_VALUE => Some(number),
        ADD_VALUE => Some(value + number),
        MULTIPLY_VALUE => Some(value * number),
        _ => None,
    }
}

/// The player gains one rank of `perk` (AddPerk, the level-up menu's Done): the rank is
/// counted, then each Quest + Stage effect of that rank sets its quest to its stage, which
/// runs the stage's result script [hypothesis: when the rank is gained; observed: the
/// stages' scripts say "triggered by Intense Training perk rank 1" for the effect of rank
/// 0, docs/spec-notes/levelup.md §8]. The rank the player has now.
pub fn gain_rank(g: &mut Game, perk: FormId) -> u8 {
    let rank = {
        let r = g.player.perks.entry(perk).or_default();
        *r = r.saturating_add(1);
        *r
    };
    let data = g.data;
    let stages: Vec<(FormId, u8)> = data
        .perks
        .get(&perk)
        .into_iter()
        .flat_map(|p| &p.entries)
        .filter(|e| u16::from(e.rank) + 1 == u16::from(rank))
        .filter_map(|e| match e.effect {
            PerkEffect::QuestStage { quest, stage } => Some((quest, stage)),
            _ => None,
        })
        .collect();
    for (quest, stage) in stages {
        g.set_stage(quest, i32::from(stage));
    }
    rank
}

/// The abilities a perk gives at `rank`: the Ability effects of the highest rank owned
/// that has any [documented: GECK wiki GetNthPerkEntryType, entry type "Ability";
/// observed: Action Boy's rank 1 names PerkActionBoy1, Action Points +15, and its rank 2
/// PerkActionBoy2, +30, the whole bonus, so the ranks do not add up; hypothesis: as the
/// entry points' ranks above].
pub fn rank_abilities(def: &crate::data::PerkDef, rank: u8) -> Vec<FormId> {
    let at = |e: &&PerkEntry| matches!(e.effect, PerkEffect::Ability(s) if s != FormId(0)) && e.rank < rank;
    let Some(top) = def.entries.iter().filter(at).map(|e| e.rank).max() else { return Vec::new() };
    def.entries
        .iter()
        .filter(at)
        .filter(|e| e.rank == top)
        .filter_map(|e| match e.effect {
            PerkEffect::Ability(s) => Some(s),
            _ => None,
        })
        .collect()
}

impl Game<'_> {
    /// The abilities the player's perks give now (traits included), with their perks.
    pub fn perk_abilities(&self) -> Vec<(FormId, FormId)> {
        let data = self.data;
        let mut out = Vec::new();
        for (&perk, &rank) in &self.player.perks {
            let Some(def) = data.perks.get(&perk) else { continue };
            out.extend(rank_abilities(def, rank.max(1)).into_iter().map(|s| (perk, s)));
        }
        out
    }

    /// The player's perks' abilities follow the perks: an ability of a perk the player
    /// has joins the player's spell list (so `crate::abilities` keeps its effects on, as
    /// an authored ability's), and one of a perk the player no longer has, or a rank it
    /// has passed, leaves it and its effects end [documented: GECK wiki GetNthPerkEntryType
    /// "Ability"; observed: Good Natured's PerkGoodNatured, +5 to five non-combat skills
    /// and −5 to five combat ones, the trait's description; hypothesis: on while the perk
    /// is held, whichever way it was gained: AddPerk, the level-up menu, a trait].
    /// Logged `perks: <perk>'s ability <spell> on|off`.
    pub fn sync_perk_abilities(&mut self) {
        let wanted = self.perk_abilities();
        let data = self.data;
        let have: Vec<FormId> = self.extra.get(&FormId::PLAYER_REF).map(|x| x.spells.clone()).unwrap_or_default();
        let stale: Vec<FormId> = have
            .iter()
            .copied()
            .filter(|s| !wanted.iter().any(|w| w.1 == *s))
            .filter(|s| data.perks.values().any(|p| p.entries.iter().any(|e| matches!(e.effect, PerkEffect::Ability(a) if a == *s))))
            .collect();
        let new: Vec<(FormId, FormId)> = wanted.into_iter().filter(|w| !have.contains(&w.1)).collect();
        if stale.is_empty() && new.is_empty() {
            return;
        }
        let x = self.extra.entry(FormId::PLAYER_REF).or_default();
        x.spells.retain(|s| !stale.contains(s));
        x.dispelled.retain(|s| !stale.contains(s));
        for &(_, spell) in &new {
            if !x.spells.contains(&spell) {
                x.spells.push(spell);
            }
        }
        for spell in stale {
            tracing::info!("perks: ability {} off", data.name(spell));
            self.dispel(FormId::PLAYER_REF, None, |e| e.source == spell && e.worn.is_none());
        }
        for (perk, spell) in new {
            tracing::info!("perks: {}'s ability {} on", data.name(perk), data.name(spell));
        }
    }

    /// The entries of `point` that apply to `actor` now, in the order they apply.
    fn entry_point_entries(&mut self, actor: FormId, point: u8) -> Vec<(FormId, PerkEntry)> {
        // Perks are the player's only (`AddPerk` on another actor is refused).
        if actor != FormId::PLAYER_REF {
            return Vec::new();
        }
        let data = self.data;
        let mut found: Vec<(FormId, PerkEntry)> = Vec::new();
        let owned: Vec<(FormId, u8)> = self.player.perks.iter().map(|(&p, &r)| (p, r)).collect();
        for (perk, rank) in owned {
            let Some(def) = data.perks.get(&perk) else { continue };
            let at = |e: &&PerkEntry| matches!(&e.effect, PerkEffect::EntryPoint(ep) if ep.point == point) && e.rank < rank.max(1);
            let Some(top) = def.entries.iter().filter(at).map(|e| e.rank).max() else { continue };
            for e in def.entries.iter().filter(at).filter(|e| e.rank == top) {
                let PerkEffect::EntryPoint(ep) = &e.effect else { continue };
                let Some(conds) = ep.conditions.as_ref() else { continue };
                if conds.iter().any(|(tab, _)| *tab != 0) {
                    continue;
                }
                let owner: Vec<_> = conds.iter().map(|(_, c)| c.clone()).collect();
                let label = format!("{} entry point {point}", data.name(perk));
                if self.conditions_pass(&owner, Some(actor), None, &label) {
                    found.push((perk, e.clone()));
                }
            }
        }
        found.sort_by_key(|(p, e)| (e.priority, *p));
        found
    }

    /// The value after the perk entries of `point` the actor has; the perks that changed
    /// it, in order.
    pub fn entry_point(&mut self, actor: FormId, point: u8, value: f64) -> (f64, Vec<Applied>) {
        let mut v = value;
        let mut applied = Vec::new();
        for (perk, e) in self.entry_point_entries(actor, point) {
            let PerkEffect::EntryPoint(ep) = &e.effect else { continue };
            let Some(number) = ep.value else { continue };
            if let Some(next) = apply_function(ep.function, number as f64, v) {
                v = next;
                applied.push(Applied { perk, value: v });
            }
        }
        (v, applied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_set_add_and_multiply() {
        assert_eq!(apply_function(SET_VALUE, 0.0, 5.0), Some(0.0));
        assert_eq!(apply_function(ADD_VALUE, 2.0, 5.0), Some(7.0));
        assert_eq!(apply_function(MULTIPLY_VALUE, 0.5, 0.3), Some(0.15));
        assert_eq!(apply_function(9, 1.0, 5.0), None);
    }
}
