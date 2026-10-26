//! Leveled item lists (LVLI): turning a list into concrete items.
//!
//! The *data* (chance none, the three flags, entries with level and count) is
//! documented by the record layout. The *selection rules* below were derived from the
//! flag names and are **consistent with the game** for the `L4.sim.leveled.basic`
//! cases [observed: 300 in-game `AddItem` draws per case, chi-square p 0.16–1.0 on all
//! seven]: chance none, independent rolls for "each item in count", highest eligible
//! level vs uniform pick, "Use All", and resolution against the player's level; and for
//! the `L4.sim.leveled.b8` cases [observed: 300 draws per case, p 0.16–1.0 on all ten]:
//! the LVLG global replacing chance none (both over LVLD 0 and LVLD 100) at its current
//! value (set from the console), "all levels" with the player's level between entry
//! levels or equal to one (eligible), a count without "each item in count" (one roll
//! times N), and nesting 3–5 deep, including a count carried into an "each item"
//! sublist as that many rolls; and for the `L4.sim.leveled.useall` cases [observed:
//! 300 draws per case, 2026-10-26]: a count given to a list without "each item in
//! count" multiplies what one roll of it gives, nested "each item" sublists included
//! (`LootAmmo10mm` ×10 gave 50–130 rounds, always a multiple of 10).
//!
//! 1. Chance none: roll an integer 0–99; below `chance_none` (or the LVLG global's
//!    value, when set) yields nothing.
//! 2. Eligible entries have `level <= actor level`.
//! 3. "Use All": every eligible entry is produced.
//!    Otherwise one entry is picked uniformly from the eligible entries if "Calculate
//!    from all levels" is set, else from the eligible entries of the highest level.
//! 4. "Calculate for each item in count": a request for N items makes N independent
//!    rolls of one; otherwise one roll whose result is multiplied by N.
//! 5. An entry naming another leveled list is a request for `entry.count` of it (rolls,
//!    if that list is "each item"); the multiplier of the lists above it applies to
//!    everything it gives. A list without "each item" asked for N multiplies by N
//!    whatever its entries give, so `LootAmmo10mm` ×10 rolls its "each item" sublists
//!    12 times each and gives ten times that, not 120 rolls.
//!
//! Entries a script added at run time (AddItemToLeveledList, [`resolve_with`]) count as
//! entries of the list, after the authored ones [documented: GECK wiki
//! AddItemToLeveledList, "adds a form to a Leveled Item list"; hypothesis: they take part
//! in the selection exactly as authored entries do].

use std::collections::BTreeMap;

use rand_core::RngCore;

use crate::form::FormId;
use crate::forms::{Entry, FormStore, LeveledEntry};

/// Nesting deeper than this is reported as a cycle.
pub const MAX_DEPTH: u32 = 32;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LeveledError {
    #[error("leveled list nesting deeper than {MAX_DEPTH} at {0} (cycle?)")]
    TooDeep(FormId),
}

/// Uniform integer in `0..n` (n > 0) without modulo bias.
/// Whether a leveled actor list's Chance none (0–100) comes up: rolled only when above 0,
/// so a list without one draws nothing from `rng` (docs/spec-notes/actor-flags.md §6).
pub fn rolls_none(rng: &mut impl RngCore, chance_none: u8) -> bool {
    chance_none > 0 && below(rng, 100) < u32::from(chance_none)
}

pub fn below(rng: &mut impl RngCore, n: u32) -> u32 {
    debug_assert!(n > 0);
    let zone = u32::MAX - (u32::MAX % n);
    loop {
        let v = rng.next_u32();
        if v < zone {
            return v % n;
        }
    }
}

/// Resolve `count` of `item` (an item or a leveled list) into concrete entries, with
/// every LVLG global at its authored value.
pub fn resolve(
    forms: &FormStore,
    item: FormId,
    count: i32,
    level: u16,
    rng: &mut impl RngCore,
    out: &mut Vec<Entry>,
) -> Result<(), LeveledError> {
    resolve_in(forms, &BTreeMap::new(), item, count, level, rng, out)
}

/// [`resolve`] with the current values of globals that differ from their authored
/// values (`globals`; e.g. set by a script or the console). An LVLG global is read at
/// the moment of resolution.
pub fn resolve_in(
    forms: &FormStore,
    globals: &BTreeMap<FormId, f32>,
    item: FormId,
    count: i32,
    level: u16,
    rng: &mut impl RngCore,
    out: &mut Vec<Entry>,
) -> Result<(), LeveledError> {
    resolve_with(forms, globals, &BTreeMap::new(), item, count, level, rng, out)
}

/// [`resolve_in`] with entries added to lists at run time (`added`, by list).
#[allow(clippy::too_many_arguments)]
pub fn resolve_with(
    forms: &FormStore,
    globals: &BTreeMap<FormId, f32>,
    added: &BTreeMap<FormId, Vec<LeveledEntry>>,
    item: FormId,
    count: i32,
    level: u16,
    rng: &mut impl RngCore,
    out: &mut Vec<Entry>,
) -> Result<(), LeveledError> {
    resolve_at(&Ctx { forms, globals, added }, item, count, 1, None, level, rng, out, 0)
}

/// The most times one "each item in count" list is rolled at once ([`resolve_at`]).
const MAX_LIST_ROLLS: i32 = 10_000;

struct Ctx<'a> {
    forms: &'a FormStore,
    globals: &'a BTreeMap<FormId, f32>,
    added: &'a BTreeMap<FormId, Vec<LeveledEntry>>,
}

/// `count` of `item` is asked for, and everything that gives is multiplied by `mult`
/// (rule 5). `condition`: the condition the entry that led here asked for (a script's added
/// entry, [`LeveledEntry::condition`]); it carries down into nested lists unless one of
/// their entries asks for its own [hypothesis].
#[allow(clippy::too_many_arguments)]
fn resolve_at(
    cx: &Ctx,
    item: FormId,
    count: i32,
    mult: i32,
    condition: Option<f32>,
    level: u16,
    rng: &mut impl RngCore,
    out: &mut Vec<Entry>,
    depth: u32,
) -> Result<(), LeveledError> {
    // A count-0 carried-item entry gives nothing [observed by the owner in game,
    // 2026-10-08: NPCs with such entries carry nothing for them].
    if count <= 0 {
        return Ok(());
    }
    let Some(list) = cx.forms.leveled(item) else {
        let mut e = Entry::new(item, count.saturating_mul(mult));
        e.extra.condition = condition;
        out.push(e);
        return Ok(());
    };
    if depth >= MAX_DEPTH {
        return Err(LeveledError::TooDeep(item));
    }
    let global = list.global.and_then(|g| cx.globals.get(&g).or_else(|| cx.forms.globals.get(&g)));
    let chance_none = match global {
        Some(v) => v.clamp(0.0, 100.0) as u32,
        None => list.chance_none as u32,
    };
    let extra = cx.added.get(&item).map(Vec::as_slice).unwrap_or_default();
    let eligible: Vec<_> = list.entries.iter().chain(extra).filter(|e| e.level <= level).collect();
    // A list rolled for each of a huge count (a broken mod's entry or script) would
    // roll that many times: capped [hypothesis: our limit, far above any shipped count].
    let rolls = if list.each { count.min(MAX_LIST_ROLLS) } else { 1 };
    let mult = if list.each { mult } else { mult.saturating_mul(count) };
    for _ in 0..rolls {
        if below(rng, 100) < chance_none || eligible.is_empty() {
            continue;
        }
        if list.use_all {
            for e in &eligible {
                resolve_at(cx, e.item, e.count as i32, mult, e.condition.or(condition), level, rng, out, depth + 1)?;
            }
            continue;
        }
        let pool: Vec<_> = if list.all_levels {
            eligible.clone()
        } else {
            let top = eligible.iter().map(|e| e.level).max().unwrap_or(0);
            eligible.iter().copied().filter(|e| e.level == top).collect()
        };
        let e = pool[below(rng, pool.len() as u32) as usize];
        resolve_at(cx, e.item, e.count as i32, mult, e.condition.or(condition), level, rng, out, depth + 1)?;
    }
    Ok(())
}

/// The reference whose FormID seeds the leveled actor choices of `reference` after its
/// record's random choices were rolled again `rolls` times (ResurrectActor,
/// `fnv_game::resurrect`): `reference` itself at 0, so an actor never resurrected keeps
/// its picks; another number for each roll. The seeds of `fnv_actor::resolve` (how the
/// actor is drawn) and `fnv_game::templates` (its carried items) go through it, so both
/// name the same entry. [design: the game's own roll depends on the player's level and
/// the encounter zone and is not reproduced, docs/spec-notes/actors.md §2.1]
pub fn roll_seed(reference: FormId, rolls: u32) -> FormId {
    if rolls == 0 {
        return reference;
    }
    // An odd multiplier: every roll number gives a different, non-zero mask.
    FormId(reference.0 ^ rolls.wrapping_mul(0x9E37_79B9).rotate_left(16))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    #[test]
    fn a_leveled_actor_lists_chance_none() {
        // Chance none 0 draws nothing, so such a list picks as before.
        let (mut a, b) = (ChaCha8Rng::seed_from_u64(7), ChaCha8Rng::seed_from_u64(7));
        assert!(!rolls_none(&mut a, 0));
        assert_eq!(a, b);
        // 100 always comes up; 50 about half the time.
        assert!((0..50).all(|_| rolls_none(&mut a, 100)));
        let n = (0..2000).filter(|_| rolls_none(&mut a, 50)).count();
        assert!((900..1100).contains(&n), "{n}");
    }

    /// A count given to a list without "each item" multiplies what one roll gives, an
    /// "each item" sublist included (`LootAmmo10mm` ×10 in game: always a multiple of 10).
    #[test]
    fn a_count_multiplies_one_roll() {
        use crate::forms::{BaseData, BaseForm, LeveledList};
        let mut forms = FormStore::default();
        let lists = [
            (1u32, false, true, 0u8, vec![(2u32, 3u16)]), // Use All, ×3 of the sublist
            (2, true, false, 50, vec![(9, 1)]),           // each item, chance none 50
        ];
        for (id, each, use_all, chance_none, es) in lists {
            let entries = es.into_iter().map(|(item, count)| LeveledEntry { level: 1, item: FormId(item), count, condition: None }).collect();
            let list = LeveledList { chance_none, all_levels: false, each, use_all, global: None, entries };
            let base = BaseForm { id: FormId(id), sig: *b"LVLI", edid: None, name: None, script: None, prompt: None, voice_filter: false, data: BaseData::LeveledItem(list) };
            forms.bases.insert(FormId(id), base);
        }
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..400 {
            let mut out = Vec::new();
            resolve(&forms, FormId(1), 10, 1, &mut rng, &mut out).unwrap();
            seen.insert(out.iter().map(|e| e.count).sum::<i32>());
        }
        // Three rolls of the sublist, times ten: 0, 10, 20 or 30, never 120 rolls' sums.
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), [0, 10, 20, 30]);
    }

    /// A huge count (a broken mod) neither overflows nor rolls billions of times.
    #[test]
    fn huge_counts_are_capped() {
        use crate::forms::{BaseData, BaseForm, LeveledList};
        let mut forms = FormStore::default();
        for (id, each) in [(1u32, true), (2, false)] {
            let entries = vec![LeveledEntry { level: 1, item: FormId(9), count: u16::MAX, condition: None }];
            let list = LeveledList { chance_none: 0, all_levels: false, each, use_all: false, global: None, entries };
            let base = BaseForm { id: FormId(id), sig: *b"LVLI", edid: None, name: None, script: None, prompt: None, voice_filter: false, data: BaseData::LeveledItem(list) };
            forms.bases.insert(FormId(id), base);
        }
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut out = Vec::new();
        resolve(&forms, FormId(1), i32::MAX, 1, &mut rng, &mut out).unwrap();
        assert_eq!(out.len(), MAX_LIST_ROLLS as usize);
        out.clear();
        resolve(&forms, FormId(2), i32::MAX, 1, &mut rng, &mut out).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].count, i32::MAX);
    }
}
