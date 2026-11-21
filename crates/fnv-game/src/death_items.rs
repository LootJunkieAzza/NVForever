//! Death items (docs/spec-notes/death-items.md): the leveled item list an actor base
//! names (NPC_/CREA `INAM`) is added to the actor's inventory when it dies, so the
//! body holds creature parts, hides, glands, dog tags.
//!
//! - The list comes from the actor that supplies the traits (template flag "Traits")
//!   [documented: xEdit hides INAM with the Traits flag, docs/spec-notes/actors.md
//!   §1.1] (§1).
//! - It is rolled at the reference's leveled-list level (its encounter zone's, fixed
//!   when its cell loaded; `fnv_sim::zones`), as its carried items are (§2).
//! - Once per death, as the actor dies, before its OnDeath blocks run [hypothesis: the
//!   order] (§2). A resurrected actor that dies again gets a new roll [hypothesis]. A
//!   saved game keeps the inventory, so loading adds nothing.
//! - Placed corpses (Base Health 0, `Game::lay_out_corpses`) are owed theirs until their
//!   inventory is first read [hypothesis: a placed corpse holds its death item too] (§2).
//! - A player's kill also adds the lists of the perk entry point "Add Leveled List On
//!   Death" (21) whose conditions pass on the victim [documented: GECK wiki Perk, list of
//!   entry points; GetPerkModifier, entry 21's tab "Target"] (§3).

use crate::game::Game;
use crate::perks::ADD_LEVELED_LIST_ON_DEATH;
use fnv_sim::FormId;

impl<'a> Game<'a> {
    /// The death item list of `r`: the `INAM` of the actor supplying its traits.
    pub fn death_item(&self, r: FormId) -> Option<FormId> {
        self.traits_def(r)?.death_item
    }

    /// `r` has just died (killed by `killer`): add its death item, and a player's kill
    /// the perk lists. Logged as `death item: …`.
    pub(crate) fn give_death_items(&mut self, r: FormId, killer: Option<FormId>) {
        let mut lists: Vec<(FormId, Option<FormId>)> = self.death_item(r).map(|l| (l, None)).into_iter().collect();
        if killer == Some(FormId::PLAYER_REF) {
            for (perk, e) in self.entry_point_entries(FormId::PLAYER_REF, ADD_LEVELED_LIST_ON_DEATH, &[Some(r)], Some(r)) {
                if let crate::data::PerkEffect::EntryPoint(ep) = &e.effect {
                    lists.extend(ep.leveled_list.map(|l| (l, Some(perk))));
                }
            }
        }
        for (list, perk) in lists {
            self.add_death_list(r, list, perk);
        }
    }

    /// Owe the placed corpse `r` its death item; it is rolled when its inventory is first
    /// made (`fnv_sim::World::add_death_item`).
    pub(crate) fn owe_death_item(&mut self, r: FormId) {
        if let Some(list) = self.death_item(r) {
            let _ = self.world.add_death_item(r, list);
        }
    }

    fn add_death_list(&mut self, r: FormId, list: FormId, perk: Option<FormId>) {
        let data = self.data;
        let from = match perk {
            Some(p) => format!(" (perk {})", data.name(p)),
            None => String::new(),
        };
        let who = crate::functions::base_of(self, r).map_or_else(String::new, |b| format!(" ({})", data.name(b)));
        match self.world.add_death_item(r, list) {
            Ok(Some((level, items))) => {
                let what: Vec<String> = items.iter().map(|(i, n)| format!("{} x{n}", data.name(*i))).collect();
                let what = if what.is_empty() { "nothing (chance none)".to_string() } else { what.join(", ") };
                tracing::info!("death item: {r}{who} {}{from} at level {level}: {what}", data.name(list));
            }
            Ok(None) => tracing::info!("death item: {r}{who} {}{from} owed until its inventory is read", data.name(list)),
            Err(e) => tracing::warn!("death item: {r}{who} {}{from} not added: {e}", data.name(list)),
        }
    }
}
