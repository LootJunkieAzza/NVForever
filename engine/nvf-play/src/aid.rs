//! Aid items the player uses from the inventory (docs/spec-notes/aid-items.md §7): the
//! menu takes the item and plays its sound (`crate::menu`, `crate::ui_sounds`); here the
//! script runtime uses it (`fnv_game::Game::ingest`: effects with potency, usage
//! monitor, addiction) and the log tells what it did to the player's actor values.
//!
//! Log lines: `aid: the player used <item> (potency p): <changes>; <value> a → b, …`
//! when an item is used, `aid: <item>'s effects on the player ended: <value> v, …` when
//! the last effect of an item ends, `aid: the player is addicted to <item> (<spell>)`,
//! `aid: <spell> cured`, and at exit `aid summary: …`.

use crate::script::{with_world, Runtime, ScriptState};
use crate::sim::SimState;
use bevy::prelude::*;
use fnv_game::aid::{ChangeKind, Ingested, ValueChange};
use fnv_game::functions::{actor_value, AV_NAMES};
use fnv_game::trace::EventKind;
use fnv_sim::FormId;
use std::collections::{BTreeMap, BTreeSet};

const P: FormId = FormId::PLAYER_REF;

/// What the player's aid items did this run.
#[derive(Debug, Default, Clone)]
pub struct AidStats {
    /// Items used, by name.
    pub used: BTreeMap<String, u32>,
    pub effects_started: u32,
    pub effects_ended: u32,
    pub addicted: Vec<String>,
    pub cured: u32,
    /// Actor values the items touched (for the summary).
    pub codes: BTreeSet<u16>,
    /// Items with effects on the player that last (modifiers, by the second), and the
    /// values they change: logged when the last of them ends.
    pub lasting: BTreeMap<FormId, BTreeSet<u16>>,
}

fn av_name(code: u16) -> &'static str {
    AV_NAMES.get(code as usize).copied().unwrap_or("?")
}

fn value(rt: &mut Runtime, code: u16) -> f64 {
    actor_value(&mut rt.game, P, code).unwrap_or(0.0)
}

fn change_text(c: &ValueChange, duration: f64) -> String {
    let how = match c.kind {
        ChangeKind::Modifier => format!("while it lasts ({duration:.0} s)"),
        ChangeKind::Once => "at once".to_string(),
        ChangeKind::PerSecond => format!("a second for {duration:.0} s"),
    };
    let limbs = if c.parts { ", limbs too" } else { "" };
    format!("{} {:+.1} {how}{limbs}", av_name(c.code), c.amount)
}

/// The player uses `item` (already taken from the inventory by the menu). Call with the
/// world swapped in (`with_world`).
pub fn use_item(rt: &mut Runtime, item: FormId) -> Ingested {
    let data = rt.game.data;
    // The values the item's effects may touch, before.
    let codes: BTreeSet<u16> = data
        .effect_items
        .get(&item)
        .into_iter()
        .flatten()
        .filter_map(|e| data.magic_effects.get(&e.effect).and_then(|m| fnv_game::aid::value_change(m, e, 1.0, false)))
        .map(|c| c.code)
        .collect();
    let before: Vec<(u16, f64)> = codes.iter().map(|&c| (c, value(rt, c))).collect();
    let r = rt.game.ingest(P, item);
    let changes: Vec<String> = r
        .applied
        .iter()
        .filter_map(|a| {
            let c = a.change?;
            Some(if a.extended { format!("{} effect lengthened to {:.0} s", av_name(c.code), a.duration) } else { change_text(&c, a.duration) })
        })
        .collect();
    let values: Vec<String> = before
        .iter()
        .map(|&(c, b)| {
            let a = value(rt, c);
            format!("{} {} → {}", av_name(c), whole(b), whole(a))
        })
        .collect();
    let name = data.name(item);
    let mut extra = String::new();
    if let Some(u) = r.usage {
        extra.push_str(&format!("; usage {u:.0}"));
    }
    if let Some((chance, hit)) = r.addiction {
        extra.push_str(&format!("; addiction chance {chance:.2}{}", if hit { ", addicted" } else { "" }));
    }
    info!(
        "aid: the player used {name} (potency {:.2}): {}{extra}; {}",
        r.potency,
        if changes.is_empty() { "no value changes".to_string() } else { changes.join(", ") },
        if values.is_empty() { "no values".to_string() } else { values.join(", ") }
    );
    let s = &mut rt.aid;
    for c in r.applied.iter().filter_map(|a| a.change).filter(|c| c.kind != ChangeKind::Once) {
        s.lasting.entry(item).or_default().insert(c.code);
    }
    *s.used.entry(name).or_default() += 1;
    s.effects_started += r.effects.len() as u32;
    s.codes.extend(codes);
    r
}

fn whole(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{:.0}", v)
    } else {
        format!("{v:.1}")
    }
}

/// After the frame's tick (world swapped in): effects of aid items on the player that
/// ended, addictions gained and cured, from the events since `from`.
pub fn after_tick(rt: &mut Runtime, from: usize) {
    let data = rt.game.data;
    let mut ended: Vec<FormId> = Vec::new();
    let mut lines = Vec::new();
    for e in rt.game.events.get(from..).unwrap_or(&[]) {
        match &e.kind {
            EventKind::EffectFinished { target, source, .. } if *target == P && data.ingestibles.contains_key(source) => {
                rt.aid.effects_ended += 1;
                if !ended.contains(source) {
                    ended.push(*source);
                }
            }
            EventKind::Addicted { actor, item, spell } if *actor == P => {
                lines.push(format!("aid: the player is addicted to {} ({})", data.name(*item), data.name(*spell)));
                rt.aid.addicted.push(data.name(*spell));
            }
            EventKind::AddictionCured { actor, spell } if *actor == P => {
                lines.push(format!("aid: {} cured", data.name(*spell)));
                rt.aid.cured += 1;
            }
            _ => {}
        }
    }
    for l in lines {
        info!("{l}");
    }
    for item in ended {
        // Only once no effect of the item is left on the player.
        if rt.game.effects.iter().any(|e| e.target == P && e.source == item && e.effect != fnv_game::aid::USAGE_MONITOR) {
            continue;
        }
        let Some(codes) = rt.aid.lasting.remove(&item) else { continue };
        let values: Vec<String> = codes.iter().map(|&c| format!("{} {}", av_name(c), whole(value(rt, c)))).collect();
        info!("aid: {}'s effects on the player ended: {}", data.name(item), values.join(", "));
    }
}

/// At exit, when the player used an aid item: `aid summary: …`.
pub fn report(mut exit: MessageReader<AppExit>, mut script: ResMut<ScriptState>, mut sim: ResMut<SimState>) {
    if exit.read().next().is_none() {
        return;
    }
    let Some(rt) = script.rt.as_mut() else { return };
    if rt.aid.used.is_empty() {
        return;
    }
    let used: Vec<String> = rt.aid.used.iter().map(|(n, c)| format!("{n} {c}")).collect();
    let total: u32 = rt.aid.used.values().sum();
    let codes: Vec<u16> = rt.aid.codes.iter().copied().collect();
    let values = with_world(rt, &mut sim, |rt| codes.iter().map(|&c| format!("{} {}", av_name(c), whole(value(rt, c)))).collect::<Vec<_>>());
    let addictions: Vec<String> = rt.game.addictions(P).iter().map(|s| rt.game.data.name(*s)).collect();
    let s = &rt.aid;
    info!(
        "aid summary: {total} items used ({}), {} effects started, {} ended; addictions: {}; {} cured; the player at exit: {}",
        used.join(", "),
        s.effects_started,
        s.effects_ended,
        if addictions.is_empty() { "none".to_string() } else { addictions.join(", ") },
        s.cured,
        values.join(", ")
    );
}
