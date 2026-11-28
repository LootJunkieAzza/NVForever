//! Partial source disposition-modifier mapping, separate from combat (factions.md §8).
use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::FormId;

fn faction(g: &Game<'_>, value: Option<&XVal>) -> Option<FormId> {
    match value {
        Some(XVal::Ref(id)) if g.data.sig(FormId(*id)).as_ref() == Some(b"FACT") => Some(FormId(*id)),
        _ => None,
    }
}

fn modifier(g: &Game<'_>, from: FormId, to: FormId) -> i64 {
    g.forms
        .faction_modifiers
        .get(&from)
        .and_then(|pairs| pairs.get(&to))
        .copied()
        .or_else(|| {
            g.data
                .faction_modifiers
                .get(&from)?
                .iter()
                .find(|(other, _)| *other == to)
                .map(|(_, value)| *value)
        })
        .unwrap_or(0)
}

fn change(g: &mut Game<'_>, args: &[XVal]) -> Result<(), String> {
    let (from, to) = faction(g, args.first())
        .zip(faction(g, args.get(1)))
        .ok_or_else(|| "ModFactionReaction requires two faction forms".to_string())?;
    // Exclusive upper bound: i64::MAX as f64 rounds up to 2^63. Never saturate a cast.
    let storage_range = (i64::MIN as f64)..-(i64::MIN as f64);
    // Fractional finite deltas truncate toward zero without aborting the script
    // [observed: 2026-11-25, FNV 1.4.0.525, w6d8p2_24_modfactionreaction_float_var:
    // from 15, +0.5 then +1.9 completed (st=2) with r1=16].
    let delta = match args.get(2) {
        Some(XVal::Num(n)) if n.is_finite() && storage_range.contains(&n.trunc()) => n.trunc() as i64,
        _ => return Err("ModFactionReaction requires a finite delta representable in i64 storage".into()),
    };
    if delta == 0 {
        return Ok(());
    }
    let next = modifier(g, from, to)
        .checked_add(delta)
        .ok_or_else(|| "ModFactionReaction exceeds the existing integer storage range".to_string())?;
    g.forms.faction_modifiers.entry(from).or_default().insert(to, next);
    Ok(())
}

pub(crate) fn call(g: &mut Game<'_>, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    match name {
        "getfactionreaction" => {
            let n = faction(g, args.first())
                .zip(faction(g, args.get(1)))
                .map_or(0, |(from, to)| modifier(g, from, to));
            Some(Ok(XVal::Num(n as f64)))
        }
        "modfactionreaction" => Some(change(g, args).map(|()| XVal::Num(0.0))),
        _ => None,
    }
}

#[cfg(test)]
#[path = "w6d8_modfactionreaction_tests.rs"]
mod w6d8_modfactionreaction_tests;
