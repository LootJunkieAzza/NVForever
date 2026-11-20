//! `fnvtool coop`: co-op data checks (private fork). `coop uniques` lists a load order's
//! unique quest rewards (`fnv_game::coop_uniques`, shared-progression plan §4.1): the
//! items, perks and spells only one quest gives, with that quest. The game computes the
//! same list itself at load, so the list is for reading, not a file the game needs.

use anyhow::Context;
use clap::Subcommand;
use fnv_game::coop_uniques::{compute, Uniques};
use fnv_game::GameData;
use fnv_sim::{FormId, FormStore, RefTable};
use serde_json::{json, Value};

#[derive(Subcommand)]
pub enum CoopCmd {
    /// The unique quest rewards of a load order: plugin, editor ID and quest of each item,
    /// perk and spell that only one quest's scripts give (fnv.coop.uniques/1).
    Uniques {
        #[command(flatten)]
        a: crate::script_cmd::ScriptArgs,
    },
    /// The quests' prerequisites for joint progression as the game reads them from the
    /// starting dialogue lines (`fnv_game::coop_prereqs`, plan §3.3), for review
    /// (fnv.coop.prereqs/1).
    Prereqs {
        #[command(flatten)]
        a: crate::script_cmd::ScriptArgs,
    },
}

/// The load order's data, as the game builds it.
fn load(a: &crate::script_cmd::ScriptArgs) -> anyhow::Result<(GameData, FormStore, RefTable, Vec<String>)> {
    let tp = crate::script_cmd::table_path(a);
    let ftext = std::fs::read_to_string(&tp).with_context(|| tp.display().to_string())?;
    let fjson: Value = serde_json::from_str(&ftext).with_context(|| tp.display().to_string())?;
    let table = fnv_script::FunctionTable::load(&tp).with_context(|| tp.display().to_string())?;
    let plugins = crate::esm_cmd::open_load_order(&a.lo)?;
    let lo = fnv_esm::LoadOrder::new(&plugins)?;
    let schema = fnv_esm::Schema::load_dir(&a.schema)?;
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &fjson, &refs);
    Ok((data, forms, refs, plugins.iter().map(|p| p.name.clone()).collect()))
}

/// One row per quest whose ways in ask something, by editor ID.
pub fn prereq_rows(data: &GameData, p: &fnv_game::coop_prereqs::Prereqs) -> Vec<Value> {
    use fnv_game::coop_prereqs::Need;
    let edid = |f: FormId| data.edid_of.get(&f).cloned().unwrap_or_else(|| format!("{:08X}", f.0));
    let need = |n: &Need| match n {
        Need::StageDone { quest, stage } => format!("{} stage {stage} done", edid(*quest)),
        Need::StageAtLeast { quest, stage } => format!("{} at stage {stage}+", edid(*quest)),
        Need::Completed { quest } => format!("{} completed", edid(*quest)),
    };
    let mut rows: Vec<(String, Value)> = p
        .ways
        .iter()
        .map(|(q, ways)| {
            let ways: Vec<Vec<String>> = ways.iter().map(|w| w.iter().map(|g| g.iter().map(need).collect::<Vec<_>>().join(" or ")).collect()).collect();
            (edid(*q), json!({"quest": edid(*q), "form": format!("{:08X}", q.0), "ways": ways}))
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows.into_iter().map(|(_, v)| v).collect()
}

/// One row per unique, by kind, then quest, then form.
pub fn rows(data: &GameData, u: &Uniques) -> Vec<Value> {
    let plugin = |f: FormId| data.load_order.get((f.0 >> 24) as usize).cloned().unwrap_or_default();
    let edid = |f: FormId| data.edid_of.get(&f).cloned().unwrap_or_default();
    let mut list: Vec<_> = u.by_form.iter().map(|(f, (k, q))| (*k, edid(*q), *f, *q)).collect();
    list.sort();
    list.into_iter()
        .map(|(k, _, f, q)| {
            json!({
                "kind": k.label(),
                "plugin": plugin(f),
                "edid": edid(f),
                "form": format!("{:08X}", f.0),
                "name": data.full_names.get(&f),
                "quest": edid(q),
                "quest_plugin": plugin(q),
                "quest_form": format!("{:08X}", q.0),
            })
        })
        .collect()
}

pub fn run(cmd: CoopCmd) -> anyhow::Result<Value> {
    match cmd {
        CoopCmd::Prereqs { a } => {
            let t0 = std::time::Instant::now();
            let (data, _, _, plugins) = load(&a)?;
            let p = fnv_game::coop_prereqs::compute(&data);
            Ok(json!({
                "schema": "fnv.coop.prereqs/1",
                "load_order": plugins,
                "summary": {"asking": p.ways.len(), "open": p.open.len()},
                "quests": prereq_rows(&data, &p),
                "seconds": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            }))
        }
        CoopCmd::Uniques { a } => {
            let t0 = std::time::Instant::now();
            let (data, forms, refs, plugins) = load(&a)?;
            let u = compute(&data, &forms, &refs);
            let count = |k: fnv_game::coop_uniques::Kind| u.by_form.values().filter(|(x, _)| *x == k).count();
            use fnv_game::coop_uniques::Kind;
            Ok(json!({
                "schema": "fnv.coop.uniques/1",
                "load_order": plugins,
                "summary": {"items": count(Kind::Item), "perks": count(Kind::Perk), "spells": count(Kind::Spell)},
                "uniques": rows(&data, &u),
                "seconds": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            }))
        }
    }
}
