//! `fnvtool ai day` and the replay part of `ai observed`: the game's package choice
//! (`fnv_game::ai`) hour by hour for an actor, and against AI observer samples. JSON
//! schemas: docs/spec-notes/packages.md §5.

use fnv_game::game::{Game, GameConfig};
use fnv_game::package::PackageCheck;
use fnv_game::GameData;
use fnv_sim::{clock, FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// One observer sample of an actor running a package of the load order.
pub struct Seen {
    pub file: usize,
    pub actor: FormId,
    pub package: FormId,
    /// GameHour.
    pub hour: f64,
    /// GameDaysPassed.
    pub days: f64,
    pub timescale: f64,
}

/// A game in its authored state: no quest started, no script run.
fn blank_game<'a>(data: &'a GameData, forms: &'a FormStore, refs: &'a RefTable) -> Game<'a> {
    Game::new(data, World::new(forms, refs, SimConfig::default()), GameConfig::default())
}

fn set_global(g: &mut Game, edid: &str, v: f64) -> anyhow::Result<()> {
    let (id, _) = g.world.forms.global_by_edid(edid).ok_or_else(|| anyhow::anyhow!("the load order has no {edid} global"))?;
    g.world.set_global(id, v as f32).map_err(|e| anyhow::anyhow!("{e}"))
}

fn letter(c: PackageCheck) -> char {
    match c {
        PackageCheck::Valid => 'V',
        PackageCheck::Missing => '?',
        PackageCheck::OutOfSchedule => '.',
        PackageCheck::NoLinkedRef => 'l',
        PackageCheck::OncePerDay => 'o',
        PackageCheck::Unreachable => 'u',
        PackageCheck::ConditionsFail => 'c',
    }
}

/// `ai day`: for each actor, the package it chooses at every hour of the authored
/// calendar day in a game where nothing has happened yet (no quest started), and each
/// list package's verdict per hour.
pub fn day(data: &GameData, forms: &FormStore, refs: &RefTable, actors: &[String], find: &dyn Fn(&str) -> Option<FormId>) -> anyhow::Result<Value> {
    let mut out = Vec::new();
    for name in actors {
        let actor = find(name).filter(|a| refs.get(*a).is_some()).ok_or_else(|| anyhow::anyhow!("{name} is not a placed reference"))?;
        let mut g = blank_game(data, forms, refs);
        let list = g.package_list(actor);
        if list.is_empty() {
            anyhow::bail!("{name} ({actor}) has no AI packages");
        }
        let mut chosen: Vec<Option<FormId>> = Vec::new();
        let mut verdicts: BTreeMap<FormId, String> = BTreeMap::new();
        // Two days: the first gives the actor a package to keep where none is valid.
        for pass in 0..2 {
            for hour in 0..24 {
                set_global(&mut g, clock::GAME_HOUR, hour as f64)?;
                let p = g.evaluate_package(actor);
                if pass == 1 {
                    chosen.push(p);
                    for &l in &list {
                        let c = g.check_package(actor, l);
                        verdicts.entry(l).or_default().push(letter(c));
                    }
                }
                g.events.clear();
            }
        }
        // Runs of hours with the same package.
        let mut runs: Vec<Value> = Vec::new();
        let mut from = 0usize;
        for h in 1..=24 {
            if h == 24 || chosen[h] != chosen[from] {
                runs.push(json!({"from_hour": from, "to_hour": h, "package": chosen[from].map(|p| data.name(p))}));
                from = h;
            }
        }
        let clock = g.package_clock();
        out.push(json!({
            "actor": data.edid_of.get(&actor).cloned().or_else(|| refs.get(actor).and_then(|r| r.edid.clone())).unwrap_or_else(|| actor.to_string()),
            "reference": actor.to_string(),
            "day_of_week": clock.map(|c| c.today.day_of_week),
            "chosen": runs,
            "by_hour": chosen.iter().map(|p| p.map(|p| data.name(p))).collect::<Vec<_>>(),
            "packages": list.iter().map(|l| json!({"package": data.name(*l), "hours_0_to_23": verdicts.get(l)})).collect::<Vec<_>>(),
        }));
    }
    Ok(json!({
        "schema": "fnv.ai.day/1",
        "legend": "per hour: V valid, . outside its schedule, c conditions fail, l needs a linked reference, o once per day, ? not a package",
        "state": "the authored game: no quest started, no script run; the calendar day as authored",
        "actors": out,
    }))
}

/// The game's choice code against the samples: for each sample of a list package, the
/// actor is given the package it ran at its previous sample and chooses at the sample's
/// hour, in a game where nothing has happened. A different choice counts as explained
/// by conditions when a package with conditions lies between the two in the list (the
/// capture's quest state is not known), else as a real difference.
pub fn replay(data: &GameData, forms: &FormStore, refs: &RefTable, rows: &[Seen]) -> anyhow::Result<Value> {
    let mut g = blank_game(data, forms, refs);
    if g.package_clock().is_none() {
        return Ok(Value::Null);
    }
    let mut prev: BTreeMap<(usize, FormId), FormId> = BTreeMap::new();
    let mut lists: BTreeMap<FormId, Vec<FormId>> = BTreeMap::new();
    let mut tally: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_actor: BTreeMap<FormId, [u64; 3]> = BTreeMap::new();
    let mut differences: BTreeMap<String, u64> = BTreeMap::new();
    let mut memo: BTreeMap<(FormId, Option<FormId>, i64), Option<FormId>> = BTreeMap::new();
    for r in rows {
        let list = lists.entry(r.actor).or_insert_with(|| g.package_list(r.actor)).clone();
        let key = (r.file, r.actor);
        let Some(at) = list.iter().position(|p| *p == r.package) else {
            prev.remove(&key);
            continue;
        };
        let before = prev.insert(key, r.package).filter(|p| list.contains(p));
        // Hours to a hundredth: the choice only changes on schedule boundaries.
        let bucket = (r.hour * 100.0).floor() as i64;
        let predicted = match memo.get(&(r.actor, before, bucket)) {
            Some(p) => *p,
            None => {
                set_global(&mut g, clock::GAME_HOUR, r.hour)?;
                set_global(&mut g, clock::GAME_DAYS_PASSED, r.days)?;
                let e = g.extra.entry(r.actor).or_default();
                e.current_package = before;
                e.packages.clear();
                e.package_chosen_at.clear();
                let p = g.select_package(r.actor);
                g.events.clear();
                memo.insert((r.actor, before, bucket), p);
                p
            }
        };
        let conditions = |a: usize, b: usize| list[a.min(b)..=a.max(b)].iter().any(|p| data.package_defs.get(p).is_some_and(|d| !d.conditions.is_empty()));
        let class = match predicted {
            Some(p) if p == r.package => 0,
            Some(p) if conditions(at, list.iter().position(|x| *x == p).unwrap_or(at)) => 1,
            None if conditions(0, at) => 1,
            _ => 2,
        };
        by_actor.entry(r.actor).or_default()[class] += 1;
        match class {
            0 => *tally.entry("same package".into()).or_default() += 1,
            1 => *tally.entry("differs, a package with conditions lies between (quest state unknown)".into()).or_default() += 1,
            _ => {
                *tally.entry("differs with no conditions involved".into()).or_default() += 1;
                *differences
                    .entry(format!("{}: ran {}, chose {}", data.name(r.actor), data.name(r.package), predicted.map(|p| data.name(p)).unwrap_or_else(|| "none".into())))
                    .or_default() += 1;
            }
        }
    }
    let total: u64 = tally.values().sum();
    let mut diffs: Vec<(String, u64)> = differences.into_iter().collect();
    diffs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let actors: BTreeSet<FormId> = by_actor.keys().copied().collect();
    Ok(json!({
        "what": "fnv_game's package choice at each sample's hour, the actor keeping the package of its previous sample, in a game where no quest has started",
        "samples": total,
        "result": tally,
        "differences_with_no_conditions_involved": diffs.iter().take(30).map(|(k, n)| json!([k, n])).collect::<Vec<_>>(),
        "by_actor": actors.iter().map(|a| {
            let c = by_actor[a];
            json!({"actor": data.edid_of.get(a).cloned().or_else(|| refs.get(*a).and_then(|r| r.edid.clone())).unwrap_or_else(|| a.to_string()),
                   "same": c[0], "conditions": c[1], "differs": c[2]})
        }).collect::<Vec<_>>(),
    }))
}
