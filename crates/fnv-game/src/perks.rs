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

impl Game<'_> {
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
