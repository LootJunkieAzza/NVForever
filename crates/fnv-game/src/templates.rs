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
use fnv_sim::{leveled, FormId};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// Template chains and nested lists longer than this are cut off (a cycle).
const MAX_DEPTH: usize = 16;

/// The level leveled actor lists are read at: a level-1 player, as the drawn actor's
/// (`fnv_actor::resolve::DISPLAY_LEVEL`; docs/spec-notes/actors.md §2.1).
pub const LEVELED_ACTOR_LEVEL: u16 = 1;

/// The entries a pick chooses from: those at or under `level`, all of them with
/// "Calculate from all levels", else those of the highest such level
/// (docs/spec-notes/actors.md §2.1, as leveled items in simulation.md §4).
pub fn leveled_pool(list: &LeveledActorDef, level: u16) -> Vec<FormId> {
    let eligible = list.entries.iter().filter(|e| e.0 <= level);
    if list.all_levels {
        return eligible.map(|e| e.1).collect();
    }
    let top = eligible.clone().map(|e| e.0).max();
    eligible.filter(|e| Some(e.0) == top).map(|e| e.1).collect()
}

/// The leveled choices of one placed actor: uniform from [`leveled_pool`], from a random
/// stream seeded by the reference's FormID, each list picked once [hypothesis: an
/// NVForever choice, not the game's roll, which depends on the player's level and the
/// encounter zone; the same seed, pool and order as the actor drawn for the reference,
/// `fnv_actor::resolve`, so the two name the same entry].
pub struct Picks {
    reference: FormId,
    /// Seeded at the first list: most chains meet none.
    rng: Option<ChaCha8Rng>,
    picked: Vec<(FormId, FormId)>,
}

impl Picks {
    pub fn new(reference: FormId) -> Self {
        Picks { reference, rng: None, picked: Vec::new() }
    }

    fn pick(&mut self, id: FormId, list: &LeveledActorDef) -> Option<FormId> {
        if let Some(&(_, p)) = self.picked.iter().find(|p| p.0 == id) {
            return Some(p);
        }
        let pool = leveled_pool(list, LEVELED_ACTOR_LEVEL);
        if pool.is_empty() {
            return None;
        }
        let rng = self.rng.get_or_insert_with(|| ChaCha8Rng::seed_from_u64(u64::from(self.reference.0)));
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
        let mut picks = Picks::new(reference);
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

    /// The factions and ranks of `reference`, placed with `base`, as the records give
    /// them: the `SNAM` list of the "Factions" source of its template chain.
    pub fn base_factions(&self, base: FormId, reference: FormId) -> &[(FormId, i8)] {
        self.template_source(base, reference, |d| d.template_factions).map_or(&[], |(_, d)| &d.factions)
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
