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
        CoopCmd::Uniques { a } => {
            let t0 = std::time::Instant::now();
            let tp = crate::script_cmd::table_path(&a);
            let ftext = std::fs::read_to_string(&tp).with_context(|| tp.display().to_string())?;
            let fjson: Value = serde_json::from_str(&ftext).with_context(|| tp.display().to_string())?;
            let table = fnv_script::FunctionTable::load(&tp).with_context(|| tp.display().to_string())?;
            let plugins = crate::esm_cmd::open_load_order(&a.lo)?;
            let lo = fnv_esm::LoadOrder::new(&plugins)?;
            let schema = fnv_esm::Schema::load_dir(&a.schema)?;
            let forms = FormStore::build(&lo, &schema);
            let refs = RefTable::build(&lo, &schema);
            let data = GameData::build(&lo, &schema, table, &fjson, &refs);
            let u = compute(&data, &forms, &refs);
            let count = |k: fnv_game::coop_uniques::Kind| u.by_form.values().filter(|(x, _)| *x == k).count();
            use fnv_game::coop_uniques::Kind;
            Ok(json!({
                "schema": "fnv.coop.uniques/1",
                "load_order": plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                "summary": {"items": count(Kind::Item), "perks": count(Kind::Perk), "spells": count(Kind::Spell)},
                "uniques": rows(&data, &u),
                "seconds": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            }))
        }
    }
}
