//! Actor templates: which record supplies a group of an actor's data
//! (docs/spec-notes/factions.md §2 and §3).
//!
//! An actor base (NPC_, CREA) with a template (`TPLT`) takes each group of data whose
//! ACBS template flag is set from the template, which may do the same with its own
//! template [documented: xEdit hides a group's fields when its flag is set; GECK wiki
//! GetActorTemplateFlag names the flags]. A template can be a leveled actor list (LVLN,
//! LVLC), and so can the base of a placed actor; the list stands for one of its entries.
//! Which entry is a choice of this engine, see [`Picks`].

use crate::data::{ActorDef, GameData, LeveledActorDef};
use fnv_sim::zones::LevelAt;
use fnv_sim::{leveled, FormId};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// Template chains and nested lists longer than this are cut off (a cycle).
const MAX_DEPTH: usize = 16;

/// The level leveled actor lists are read at where no reference level is given (tools,
/// a base alone): a level-1 player, as the static display's
/// (`fnv_actor::resolve::DISPLAY_LEVEL`; docs/spec-notes/actors.md §2.1). In a game each
/// reference reads them at its own level (`fnv_sim::zones`, [`crate::game::Game::leveled_at`];
/// docs/spec-notes/encounter-zones.md).
pub const LEVELED_ACTOR_LEVEL: u16 = 1;

/// The entries a pick chooses from: those at or under `level`, all of them with
/// "Calculate from all levels", else those of the highest such level
/// (docs/spec-notes/actors.md §2.1, as leveled items in simulation.md §4).
pub fn leveled_pool(list: &LeveledActorDef, level: u16) -> Vec<FormId> {
    leveled_pool_at(list, LevelAt::plain(level))
}

/// [`leveled_pool`] at a reference's level: with the level modifier Easy every eligible
/// entry is in the pool (docs/spec-notes/encounter-zones.md §3).
pub fn leveled_pool_at(list: &LeveledActorDef, at: LevelAt) -> Vec<FormId> {
    let level = at.level;
    let eligible = list.entries.iter().filter(|e| e.0 <= level);
    if at.all(list.all_levels) {
        return eligible.map(|e| e.1).collect();
    }
    let top = eligible.clone().map(|e| e.0).max();
    eligible.filter(|e| Some(e.0) == top).map(|e| e.1).collect()
}

/// The leveled choices of one placed actor: uniform from [`leveled_pool`] at the
/// reference's level, from a random stream seeded by the reference's FormID, each list
/// picked once [hypothesis: an NVForever choice, not the game's roll; the same seed,
/// level, pool and order as the actor drawn for the reference, `fnv_actor::resolve`, so
/// the two name the same entry].
pub struct Picks {
    reference: FormId,
    at: LevelAt,
    /// Seeded at the first list: most chains meet none.
    rng: Option<ChaCha8Rng>,
    picked: Vec<(FormId, FormId)>,
    /// Lists whose Chance none came up: they stand for no actor.
    none: Vec<FormId>,
}

impl Picks {
    pub fn new(reference: FormId) -> Self {
        Self::at(reference, LevelAt::plain(LEVELED_ACTOR_LEVEL))
    }

    /// Picks read at the level `at` (the reference's, `fnv_sim::zones`).
    pub fn at(reference: FormId, at: LevelAt) -> Self {
        Picks { reference, at, rng: None, picked: Vec::new(), none: Vec::new() }
    }

    /// The lists picked so far and their picks, in order.
    pub fn picked(&self) -> &[(FormId, FormId)] {
        &self.picked
    }

    fn pick(&mut self, id: FormId, list: &LeveledActorDef) -> Option<FormId> {
        if let Some(&(_, p)) = self.picked.iter().find(|p| p.0 == id) {
            return Some(p);
        }
        if self.none.contains(&id) {
            return None;
        }
        let pool = leveled_pool_at(list, self.at);
        if pool.is_empty() {
            return None;
        }
        let rng = self.rng.get_or_insert_with(|| ChaCha8Rng::seed_from_u64(u64::from(self.reference.0)));
        // Chance none, rolled only when above 0, so lists without one pick as before, and
        // before the entry, as `fnv_actor::resolve` does (docs/spec-notes/actor-flags.md
        // §6) [documented: GECK wiki LeveledItem, "The chance that the leveled list will
        // not generate an item"; hypothesis: actor lists alike].
        if fnv_sim::leveled::rolls_none(rng, list.chance_none) {
            self.none.push(id);
            return None;
        }
        let p = pool[leveled::below(rng, pool.len() as u32) as usize];
        self.picked.push((id, p));
        Some(p)
    }
}

impl GameData {
    /// The actor base `id` stands for: itself, or the entry picked from a leveled actor
    /// list, through nested lists. `None` for a form that is neither, and for a list with
    /// no entry at the level.
    pub fn actor_base(&self, mut id: FormId, picks: &mut Picks) -> Option<FormId> {
        for _ in 0..MAX_DEPTH {
            if self.actors.contains_key(&id) {
                return Some(id);
            }
            id = picks.pick(id, self.leveled_actors.get(&id)?)?;
        }
        None
    }

    /// The actor base that supplies a group of the data of `reference`, placed with
    /// `base`: follow `TPLT` from the base while the current actor's template flag for
    /// the group (`from_template`) is set. An actor whose template is missing, or a list
    /// with nothing to pick, keeps its own data.
    pub fn template_source(&self, base: FormId, reference: FormId, from_template: fn(&ActorDef) -> bool) -> Option<(FormId, &ActorDef)> {
        self.template_source_at(base, reference, LevelAt::plain(LEVELED_ACTOR_LEVEL), from_template)
    }

    /// [`GameData::template_source`] with leveled lists read at `at` (the reference's
    /// level in a game, [`crate::game::Game::leveled_at`]).
    pub fn template_source_at(&self, base: FormId, reference: FormId, at: LevelAt, from_template: fn(&ActorDef) -> bool) -> Option<(FormId, &ActorDef)> {
        let mut picks = Picks::at(reference, at);
        let mut id = self.actor_base(base, &mut picks)?;
        let mut def = self.actors.get(&id)?;
        for _ in 0..MAX_DEPTH {
            let next = def.template.filter(|_| from_template(def)).and_then(|t| self.actor_base(t, &mut picks));
            match next {
                Some(t) => (id, def) = (t, &self.actors[&t]),
                None => break,
            }
        }
        Some((id, def))
    }

    /// Whether `reference`, placed with `base`, is flagged No VATS Melee on its own record
    /// (docs/spec-notes/actor-flags.md §1; `Game::no_vats_melee`).
    pub fn no_vats_melee(&self, base: FormId, reference: FormId) -> bool {
        self.template_source(base, reference, |_| false).is_some_and(|(_, d)| d.no_vats_melee)
    }

    /// The base data source's (template flag "Base Data") ACBS flags of `reference`,
    /// placed with `base`, for pushing, head tracking and dismemberment (actor-fields.md
    /// §1) [documented: GECK wiki Category:NPC, Template Data: Base Data supplies the
    /// flags but No Knockdowns, No VATS Melee and a few others]. All clear without a
    /// definition.
    pub fn minor_flags(&self, base: FormId, reference: FormId) -> MinorFlags {
        self.template_source(base, reference, |d| d.template_base).map_or(MinorFlags::default(), |(_, d)| MinorFlags {
            not_pushable: d.not_pushable,
            no_head_track: d.no_head_track,
            no_head: d.no_head,
            no_right_arm: d.no_right_arm,
            no_left_arm: d.no_left_arm,
        })
    }

    /// Whether hits on `reference`, placed with `base`, leave no blood spray and no blood
    /// decal (actor-flags.md §2; `Game::no_blood`).
    pub fn no_blood(&self, base: FormId, reference: FormId) -> (bool, bool) {
        self.template_source(base, reference, |d| d.template_model).map_or((false, false), |(_, d)| (d.no_blood_spray, d.no_blood_decal))
    }

    /// The factions and ranks of `reference`, placed with `base`, as the records give
    /// them: the `SNAM` list of the "Factions" source of its template chain.
    pub fn base_factions(&self, base: FormId, reference: FormId) -> &[(FormId, i8)] {
        self.base_factions_at(base, reference, LevelAt::plain(LEVELED_ACTOR_LEVEL))
    }

    /// [`GameData::base_factions`] at the level `at`.
    pub fn base_factions_at(&self, base: FormId, reference: FormId, at: LevelAt) -> &[(FormId, i8)] {
        self.template_source_at(base, reference, at, |d| d.template_factions).map_or(&[], |(_, d)| &d.factions)
    }

    /// The actor base whose name `reference` (placed with `base`) is shown by: the
    /// "Base Data" source of its template chain (the name is base data: an actor with the
    /// flag has no FULL of its own [observed: VSpawnTier2GeckoMed 0x00156777, shown as
    /// "Gecko", the FULL of the entry its template VEncTier2GeckoMed leads to, owner's
    /// playtest list 4 item 20]). `base` itself when the chain cannot be followed.
    pub fn display_base(&self, base: FormId, reference: FormId) -> FormId {
        self.display_base_at(base, reference, LevelAt::plain(LEVELED_ACTOR_LEVEL))
    }

    /// [`GameData::display_base`] at the level `at`.
    pub fn display_base_at(&self, base: FormId, reference: FormId, at: LevelAt) -> FormId {
        self.template_source_at(base, reference, at, |d| d.template_base).map_or(base, |(id, _)| id)
    }

    /// The placed actors whose carried items are another actor base's than their own:
    /// the "Inventory" source of the template chain, or the entry a placed leveled list
    /// stands for ([`GameData::template_source`]), for `fnv_sim::World::set_inventory_sources`.
    pub fn inventory_sources(&self, placed: &fnv_sim::RefTable) -> std::collections::BTreeMap<FormId, FormId> {
        self.inventory_sources_at(placed, |_| LevelAt::plain(LEVELED_ACTOR_LEVEL))
    }

    /// [`GameData::inventory_sources`] with each reference's leveled lists read at
    /// `at(reference)`.
    pub fn inventory_sources_at(&self, placed: &fnv_sim::RefTable, at: impl Fn(FormId) -> LevelAt) -> std::collections::BTreeMap<FormId, FormId> {
        let mut out = std::collections::BTreeMap::new();
        for (&r, p) in &placed.refs {
            if !matches!(&p.sig, b"ACHR" | b"ACRE") {
                continue;
            }
            if let Some((source, _)) = self.template_source_at(p.base, r, at(r), |d| d.template_inventory) {
                if source != p.base {
                    out.insert(r, source);
                }
            }
        }
        out
    }

    /// Every actor base `id` can stand for or take the group from, whatever the picks:
    /// the ends of the template chain through every entry of every list on the way (for
    /// surveys: whether the pick matters).
    pub fn template_sources_any_pick(&self, id: FormId, from_template: fn(&ActorDef) -> bool) -> Vec<FormId> {
        let mut out = Vec::new();
        self.sources_into(id, from_template, 0, &mut out);
        out.sort();
        out.dedup();
        out
    }

    fn sources_into(&self, id: FormId, from_template: fn(&ActorDef) -> bool, depth: usize, out: &mut Vec<FormId>) {
        if depth > 2 * MAX_DEPTH {
            return;
        }
        if let Some(def) = self.actors.get(&id) {
            match def.template.filter(|t| from_template(def) && (self.actors.contains_key(t) || self.leveled_actors.contains_key(t))) {
                Some(t) => self.sources_into(t, from_template, depth + 1, out),
                None => out.push(id),
            }
        } else if let Some(list) = self.leveled_actors.get(&id) {
            for &(_, e) in &list.entries {
                self.sources_into(e, from_template, depth + 1, out);
            }
        }
    }
}

/// An actor's minor ACBS flags ([`GameData::minor_flags`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MinorFlags {
    pub not_pushable: bool,
    pub no_head_track: bool,
    pub no_head: bool,
    pub no_right_arm: bool,
    pub no_left_arm: bool,
}

impl MinorFlags {
    /// Whether a part at body location `location` (BPTD part type: 1–2 head, 13 brain, 3–4
    /// left arm, 5–6 right arm) is one the actor has not got, so it cannot be cut off.
    pub fn lacks(&self, location: i32) -> bool {
        match location {
            1 | 2 | 13 => self.no_head,
            3 | 4 => self.no_left_arm,
            5 | 6 => self.no_right_arm,
            _ => false,
        }
    }
}

#[cfg(test)]
mod minor_flag_tests {
    use super::*;

    #[test]
    fn a_creature_without_a_head_or_arms_lacks_those_parts() {
        let f = MinorFlags { no_head: true, no_left_arm: true, ..MinorFlags::default() };
        assert!(f.lacks(1) && f.lacks(2) && f.lacks(13));
        assert!(f.lacks(3) && f.lacks(4));
        assert!(!f.lacks(5) && !f.lacks(0) && !f.lacks(7));
        assert!(!MinorFlags::default().lacks(1));
    }
}

