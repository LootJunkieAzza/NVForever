//! Script coverage of headless runs (docs/handoff/mq-walkthrough-2026-10-27.md §5): what
//! a run reached of the load order's quests, scripts and dialogue, and the report of many
//! runs against the masters.
//!
//! A run with [`Game::coverage`] set records the quest stages set, the script blocks run
//! (unit and block), the dialogue lines (INFO) said and the result scripts run (quest
//! stage, INFO, terminal, package and the rest). [`Coverage::log`] writes them as a
//! `fnv.script.coverage-log/1` document whose FormIDs name their plugin
//! (`FalloutNV.esm:10E908`), so that runs over different load orders merge; [`report`]
//! merges any number of logs and compares them with a load order's records.

use crate::data::GameData;
use crate::game::Game;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::UnitOwner;
use fnv_sim::FormId;
use serde_json::{json, Map, Value as Json};
use std::collections::{BTreeMap, BTreeSet};

pub const LOG_FORMAT: &str = "fnv.script.coverage-log/1";
pub const REPORT_FORMAT: &str = "fnv.script.coverage/1";

/// What one run reached, in the run's own FormIDs and unit keys.
#[derive(Debug, Default, Clone)]
pub struct Coverage {
    pub stages: BTreeSet<(FormId, i32)>,
    /// Unit key and the block's place among the unit's blocks.
    pub blocks: BTreeSet<(String, usize)>,
    pub infos: BTreeSet<FormId>,
    /// Unit keys of the result scripts run.
    pub results: BTreeSet<String>,
}

impl Game<'_> {
    pub(crate) fn cover_block(&mut self, key: impl FnOnce() -> String, index: usize) {
        if let Some(c) = self.coverage.as_mut() {
            c.blocks.insert((key(), index));
        }
    }

    pub(crate) fn cover_result(&mut self, key: impl FnOnce() -> String) {
        if let Some(c) = self.coverage.as_mut() {
            c.results.insert(key());
        }
    }

    pub(crate) fn cover_stage(&mut self, quest: FormId, stage: i32) {
        if let Some(c) = self.coverage.as_mut() {
            c.stages.insert((quest, stage));
        }
    }

    pub(crate) fn cover_info(&mut self, info: FormId) {
        if let Some(c) = self.coverage.as_mut() {
            c.infos.insert(info);
        }
    }
}

/// A FormID by its plugin: `Plugin.esm:XXXXXX` (the plugin of the load-order index in
/// the top byte, the object ID below it).
pub fn form_name(load_order: &[String], id: u32) -> String {
    let i = (id >> 24) as usize;
    let plugin = load_order.get(i).map(String::as_str).unwrap_or("?");
    format!("{plugin}:{:06X}", id & 0x00FF_FFFF)
}

/// A unit key without its plugin and with every FormID by its plugin: the winning unit
/// of an owner is the same unit whichever plugin last changed it, so runs over load
/// orders with and without an add-on that overrides it name it alike.
pub fn unit_name(load_order: &[String], key: &str) -> String {
    let owner = key.split_once('|').map(|(_, o)| o).unwrap_or(key);
    let mut out = String::with_capacity(owner.len() + 16);
    let mut rest = owner;
    while let Some(at) = rest.find("0x") {
        let hex = &rest[at + 2..];
        let n = hex.bytes().take_while(u8::is_ascii_hexdigit).count();
        out.push_str(&rest[..at]);
        match (n == 8).then(|| u32::from_str_radix(&hex[..8], 16).ok()).flatten() {
            Some(id) => out.push_str(&form_name(load_order, id)),
            None => out.push_str(&rest[at..at + 2 + n]),
        }
        rest = &hex[n..];
    }
    out.push_str(rest);
    out
}

impl Coverage {
    /// The run's coverage log (`fnv.script.coverage-log/1`); `title` names the run.
    pub fn log(&self, data: &GameData, title: &str) -> Json {
        let lo = &data.load_order;
        let keys: BTreeSet<&str> = self.blocks.iter().map(|(k, _)| k.as_str()).collect();
        let units: BTreeMap<String, &crate::data::Compiled> =
            data.units.iter().map(|u| (u.key(), u)).filter(|(k, _)| keys.contains(k.as_str())).collect();
        let block_name = |key: &str, i: usize| {
            units
                .get(key)
                .and_then(|u| u.program.blocks.get(i))
                .and_then(|b| data.table.block_name(b.event))
                .unwrap_or("?")
                .to_string()
        };
        json!({
            "format": LOG_FORMAT,
            "title": title,
            "load_order": lo,
            "stages": self.stages.iter().map(|(q, s)| json!([form_name(lo, q.0), s])).collect::<Vec<_>>(),
            "blocks": self.blocks.iter().map(|(k, i)| json!([unit_name(lo, k), i, block_name(k, *i)])).collect::<Vec<_>>(),
            "infos": self.infos.iter().map(|i| form_name(lo, i.0)).collect::<Vec<_>>(),
            "results": self.results.iter().map(|k| unit_name(lo, k)).collect::<Vec<_>>(),
        })
    }
}

/// Logs merged: everything any of them reached, in plugin-named form.
#[derive(Debug, Default)]
struct Reached {
    stages: BTreeSet<(String, i64)>,
    blocks: BTreeSet<(String, u64)>,
    infos: BTreeSet<String>,
    results: BTreeSet<String>,
}

impl Reached {
    fn add(&mut self, log: &Json) -> Result<(), String> {
        if log["format"] != LOG_FORMAT {
            return Err(format!("not a {LOG_FORMAT} document"));
        }
        let arr = |k: &str| log[k].as_array().cloned().unwrap_or_default();
        for s in arr("stages") {
            self.stages.insert((s[0].as_str().unwrap_or_default().to_string(), s[1].as_i64().unwrap_or_default()));
        }
        for b in arr("blocks") {
            self.blocks.insert((b[0].as_str().unwrap_or_default().to_string(), b[1].as_u64().unwrap_or_default()));
        }
        self.infos.extend(arr("infos").iter().filter_map(|i| i.as_str().map(str::to_string)));
        self.results.extend(arr("results").iter().filter_map(|i| i.as_str().map(str::to_string)));
        Ok(())
    }
}

/// The main quest's Editor ID prefixes in FalloutNV.esm [observed: the quests the
/// main-quest walkthroughs run: VCG (Goodsprings start), VMQ, VHD (Hoover Dam), VEnding].
const MAIN_PREFIXES: [&str; 4] = ["vmq", "vhd", "vending", "vcg0"];

/// The four story add-ons' plugins.
const STORY_ADDONS: [&str; 4] = ["deadmoney.esm", "honesthearts.esm", "oldworldblues.esm", "lonesomeroad.esm"];

/// Rank of a quest in the untouched list: 0 main quest, 1 faction, 2 add-on, 3 the rest.
/// A faction quest is a base-game quest whose stage scripts change a reputation
/// [hypothesis: a working rule for ranking, not a category of the data].
fn rank(data: &GameData, q: FormId, edid: &str) -> u8 {
    let plugin = data.load_order.get((q.0 >> 24) as usize).map(|p| p.to_ascii_lowercase()).unwrap_or_default();
    let e = edid.to_ascii_lowercase();
    if plugin == "falloutnv.esm" && MAIN_PREFIXES.iter().any(|p| e.starts_with(p)) {
        return 0;
    }
    if STORY_ADDONS.contains(&plugin.as_str()) || plugin != "falloutnv.esm" {
        return 2;
    }
    let reputation = data.quests.get(&q).is_some_and(|d| {
        d.stages.values().flatten().filter_map(|e| e.unit).any(|u| {
            data.unit(u).unit.source.as_deref().is_some_and(|s| s.to_ascii_lowercase().contains("reputation"))
        })
    });
    if reputation {
        1
    } else {
        3
    }
}

const RANKS: [&str; 4] = ["main", "faction", "add-on", "other"];

/// The actor a line belongs to: its fixed speaker, else the actor of its own GetIsID
/// condition (dialogue.rs `can_say_as`).
pub(crate) fn line_actor(data: &GameData, info: &crate::data::InfoDef) -> Option<FormId> {
    info.conditions
        .iter()
        .find(|c| c.function == 0x48 && matches!(c.run_on, crate::cond::RunOn::Subject))
        .and_then(|c| match c.params.first() {
            Some(ArgValue::Value(Value::Ref(f))) => Some(FormId(*f)),
            _ => None,
        })
        .or(info.speaker)
        .filter(|f| data.sig(*f).is_some())
}

/// The kind of a result script, by its owner.
fn result_kind(o: &UnitOwner) -> Option<&'static str> {
    Some(match o {
        UnitOwner::QuestStage { .. } => "stage",
        UnitOwner::Info { .. } => "info",
        UnitOwner::Terminal { .. } => "terminal",
        UnitOwner::Package { .. } => "package",
        UnitOwner::Script { .. } | UnitOwner::Perk { .. } | UnitOwner::Patrol { .. } | UnitOwner::Other { .. } => return None,
    })
}

/// `fnvtool script cover` / `coverage-report`: `logs` merged and compared with `data`'s
/// quests, scripts and lines. The summary holds the totals and the untouched quests,
/// ranked (main quest, factions, add-ons, the rest) and cut to `untouched_max`; the
/// detail holds every quest, script and actor.
pub fn report(data: &GameData, logs: &[Json], untouched_max: usize) -> Result<Json, String> {
    let mut r = Reached::default();
    for (i, l) in logs.iter().enumerate() {
        r.add(l).map_err(|e| format!("log {i}: {e}"))?;
    }
    let lo = &data.load_order;
    let edid = |f: FormId| data.edid_of.get(&f).cloned().unwrap_or_else(|| form_name(lo, f.0));

    // Quests: stages reached, whether a stage that completes the quest was reached.
    let mut quests = Vec::new();
    let mut untouched: Vec<(u8, String, usize)> = Vec::new();
    let (mut stages_total, mut stages_reached, mut quests_touched, mut completed) = (0usize, 0usize, 0usize, 0usize);
    for (q, def) in &data.quests {
        if def.stages.is_empty() {
            continue;
        }
        let name = form_name(lo, q.0);
        let reached: Vec<i32> = def.stages.keys().copied().filter(|s| r.stages.contains(&(name.clone(), *s as i64))).collect();
        let completing: Vec<i32> =
            def.stages.iter().filter(|(_, es)| es.iter().any(|e| e.flags & 1 != 0)).map(|(s, _)| *s).collect();
        let complete = completing.iter().any(|s| reached.contains(s));
        let e = def.edid.clone().unwrap_or_else(|| name.clone());
        stages_total += def.stages.len();
        stages_reached += reached.len();
        quests_touched += usize::from(!reached.is_empty());
        completed += usize::from(complete);
        let rk = rank(data, *q, &e);
        if reached.is_empty() {
            untouched.push((rk, e.clone(), def.stages.len()));
        }
        quests.push(json!({
            "quest": e, "form": name, "rank": RANKS[rk as usize],
            "stages": def.stages.len(), "reached": reached,
            "completing_stages": completing, "completed": complete,
        }));
    }
    untouched.sort();
    let mut untouched_by_rank = Map::new();
    for (i, n) in RANKS.iter().enumerate() {
        untouched_by_rank.insert(n.to_string(), json!(untouched.iter().filter(|u| u.0 as usize == i).count()));
    }

    // Scripts (SCPT units with blocks): blocks run out of all.
    let mut scripts = Vec::new();
    let (mut blocks_total, mut blocks_run, mut scripts_touched) = (0usize, 0usize, 0usize);
    let mut results_total: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for u in &data.units {
        let key = unit_name(lo, &u.key());
        if let Some(kind) = result_kind(&u.unit.owner) {
            // A result script field with no statement (most INFOs' and packages') is
            // nothing to run.
            if u.program.top.is_empty() {
                continue;
            }
            let e = results_total.entry(kind).or_default();
            e.0 += 1;
            e.1 += usize::from(r.results.contains(&key));
            continue;
        }
        if !matches!(u.unit.owner, UnitOwner::Script { .. }) || u.program.blocks.is_empty() {
            continue;
        }
        let n = u.program.blocks.len();
        let run = (0..n).filter(|i| r.blocks.contains(&(key.clone(), *i as u64))).count();
        blocks_total += n;
        blocks_run += run;
        scripts_touched += usize::from(run > 0);
        scripts.push(json!({"script": edid(FormId(u.unit.owner.form())), "unit": key, "blocks": n, "run": run}));
    }

    // Lines per actor.
    let mut actors: BTreeMap<FormId, (usize, usize)> = BTreeMap::new();
    let (mut lines_total, mut lines_said) = (0usize, 0usize);
    for (id, info) in &data.infos {
        let said = r.infos.contains(&form_name(lo, id.0));
        lines_total += 1;
        lines_said += usize::from(said);
        if let Some(a) = line_actor(data, info) {
            let e = actors.entry(a).or_default();
            e.0 += 1;
            e.1 += usize::from(said);
        }
    }
    let actors_json: Vec<Json> = actors
        .iter()
        .map(|(a, (n, s))| json!({"actor": edid(*a), "form": form_name(lo, a.0), "lines": n, "said": s}))
        .collect();
    let actors_touched = actors.values().filter(|(_, s)| *s > 0).count();

    let results: Map<String, Json> =
        results_total.iter().map(|(k, (n, run))| (k.to_string(), json!({"total": n, "run": run}))).collect();
    Ok(json!({
        "format": REPORT_FORMAT,
        "load_order": lo,
        "logs": logs.len(),
        "summary": {
            "quests": {"total": quests.len(), "touched": quests_touched, "completed": completed},
            "stages": {"total": stages_total, "reached": stages_reached},
            "scripts": {"total": scripts.len(), "touched": scripts_touched},
            "blocks": {"total": blocks_total, "run": blocks_run},
            "lines": {"total": lines_total, "said": lines_said},
            "actors": {"with_lines": actors.len(), "touched": actors_touched},
            "results": results,
            "untouched_quests": untouched_by_rank,
        },
        "untouched": untouched
            .iter()
            .take(untouched_max)
            .map(|(rk, e, n)| json!({"quest": e, "rank": RANKS[*rk as usize], "stages": n}))
            .collect::<Vec<_>>(),
        "detail": {"quests": quests, "scripts": scripts, "actors": actors_json},
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_carry_the_plugin() {
        let lo = vec!["FalloutNV.esm".to_string(), "HonestHearts.esm".to_string()];
        assert_eq!(form_name(&lo, 0x0010E908), "FalloutNV.esm:10E908");
        assert_eq!(form_name(&lo, 0x01000ADE), "HonestHearts.esm:000ADE");
        assert_eq!(unit_name(&lo, "HonestHearts.esm|QUST 0x0010E908 stage 100 entry 1"), "QUST FalloutNV.esm:10E908 stage 100 entry 1");
        assert_eq!(unit_name(&lo, "FalloutNV.esm|INFO 0x01000ADE end"), "INFO HonestHearts.esm:000ADE end");
        // Not a FormID: kept as it is.
        assert_eq!(unit_name(&lo, "x|Other 0x12 unit 0"), "Other 0x12 unit 0");
    }
}
