//! Leveled item lists (LVLI): turning a list into concrete items.
//!
//! The *data* (chance none, the three flags, entries with level and count) is
//! documented by the record layout. The *selection rules* below are **[hypothesis]**,
//! derived from the flag names only, until the distribution scenarios in
//! docs/design/layer4.md §4 compare them with samples captured from the original game:
//!
//! 1. Chance none: roll an integer 0–99; below `chance_none` (or the LVLG global's
//!    value, when set) yields nothing.
//! 2. Eligible entries have `level <= actor level`.
//! 3. "Use All": every eligible entry is produced.
//!    Otherwise one entry is picked uniformly from the eligible entries if "Calculate
//!    from all levels" is set, else from the eligible entries of the highest level.
//! 4. "Calculate for each item in count": a request for N items makes N independent
//!    rolls of one; otherwise one roll whose result is multiplied by N.
//! 5. An entry naming another leveled list is resolved recursively with
//!    `entry.count × multiplier`.

use rand_core::RngCore;

use crate::form::FormId;
use crate::forms::{Entry, FormStore};

/// Nesting deeper than this is reported as a cycle.
pub const MAX_DEPTH: u32 = 32;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LeveledError {
    #[error("leveled list nesting deeper than {MAX_DEPTH} at {0} (cycle?)")]
    TooDeep(FormId),
}

/// Uniform integer in `0..n` (n > 0) without modulo bias.
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

/// Resolve `count` of `item` (an item or a leveled list) into concrete entries.
pub fn resolve(
    forms: &FormStore,
    item: FormId,
    count: i32,
    level: u16,
    rng: &mut impl RngCore,
    out: &mut Vec<Entry>,
) -> Result<(), LeveledError> {
    resolve_at(forms, item, count, level, rng, out, 0)
}

fn resolve_at(
    forms: &FormStore,
    item: FormId,
    count: i32,
    level: u16,
    rng: &mut impl RngCore,
    out: &mut Vec<Entry>,
    depth: u32,
) -> Result<(), LeveledError> {
    if count <= 0 {
        return Ok(());
    }
    let Some(list) = forms.leveled(item) else {
        out.push(Entry::new(item, count));
        return Ok(());
    };
    if depth >= MAX_DEPTH {
        return Err(LeveledError::TooDeep(item));
    }
    let chance_none = match list.global.and_then(|g| forms.globals.get(&g)) {
        Some(v) => v.clamp(0.0, 100.0) as u32,
        None => list.chance_none as u32,
    };
    let eligible: Vec<_> = list.entries.iter().filter(|e| e.level <= level).collect();
    let rolls = if list.each { count } else { 1 };
    let mult = if list.each { 1 } else { count };
    for _ in 0..rolls {
        if below(rng, 100) < chance_none || eligible.is_empty() {
            continue;
        }
        if list.use_all {
            for e in &eligible {
                resolve_at(forms, e.item, e.count as i32 * mult, level, rng, out, depth + 1)?;
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
        resolve_at(forms, e.item, e.count as i32 * mult, level, rng, out, depth + 1)?;
    }
    Ok(())
}
