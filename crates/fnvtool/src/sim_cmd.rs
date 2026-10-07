//! `fnvtool sim …`: Layer 4 core simulation (crate `fnv-sim`).

use crate::esm_cmd::open_load_order;
use crate::SimCmd;
use anyhow::Context;
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::actions::{self, Script};
use fnv_sim::forms::{BaseData, ItemKind};
use fnv_sim::refs::FLAG_PERSISTENT;
use fnv_sim::{FormId, FormStore, RefTable, World};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn run(cmd: SimCmd) -> anyhow::Result<Value> {
    match cmd {
        SimCmd::Forms { lo, schema } => {
            let plugins = open_load_order(&lo)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema)?;
            Ok(forms_report(&lo, &FormStore::build(&lo, &schema), &RefTable::build(&lo, &schema)))
        }
        SimCmd::Containers { lo, schema } => {
            let plugins = open_load_order(&lo)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema)?;
            Ok(containers(&FormStore::build(&lo, &schema)))
        }
        SimCmd::Run { lo, schema, actions } => {
            let text = std::fs::read_to_string(&actions).with_context(|| actions.display().to_string())?;
            let script: Script = serde_json::from_str(&text).with_context(|| actions.display().to_string())?;
            let plugins = open_load_order(&lo)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema)?;
            let forms = FormStore::build(&lo, &schema);
            let refs = RefTable::build(&lo, &schema);
            let mut world = World::new(&forms, &refs, actions::config(&script));
            actions::run(&mut world, &script).map_err(anyhow::Error::msg)
        }
    }
}

fn sig_str(s: &[u8; 4]) -> String {
    String::from_utf8_lossy(s).into_owned()
}

fn bump(m: &mut BTreeMap<String, u64>, k: impl Into<String>) {
    *m.entry(k.into()).or_default() += 1;
}

/// Classification of a FormID that a carried-item entry names.
fn target_class(forms: &FormStore, id: FormId) -> String {
    match forms.get(id).map(|b| &b.data) {
        Some(BaseData::Item(_)) => "item".into(),
        Some(BaseData::LeveledItem(_)) => "leveled_item".into(),
        _ => match forms.sig(id) {
            Some(s) => format!("other:{}", sig_str(&s)),
            None if id.0 < 0x800 => "engine_builtin".into(),
            None => "missing".into(),
        },
    }
}

/// Summary of what the loaders understood, plus consistency checks on the data
/// (fnv.sim.forms/1). Everything here is observational: it drives the spec notes.
fn forms_report(lo: &LoadOrder, forms: &FormStore, refs: &RefTable) -> Value {
    let mut by_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut item_stats: BTreeMap<String, [u64; 3]> = BTreeMap::new(); // total, value known, weight known
    let mut cnto_targets: BTreeMap<String, u64> = BTreeMap::new();
    let mut cnto_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut lvlo_targets: BTreeMap<String, u64> = BTreeMap::new();
    let mut odd_entries: Vec<String> = Vec::new();
    let (mut lvl_lists, mut lvl_entries, mut lvl_flags) = (0u64, 0u64, BTreeMap::new());
    for b in forms.bases.values() {
        bump(&mut by_kind, sig_str(&b.sig));
        match &b.data {
            BaseData::Item(i) => {
                let e = item_stats.entry(format!("{:?}", i.kind)).or_default();
                e[0] += 1;
                e[1] += i.value.is_some() as u64;
                e[2] += i.weight.is_some() as u64;
            }
            BaseData::Container(c) => {
                for e in &c.entries {
                    let class = target_class(forms, e.item);
                    if (e.count <= 0 || !class.ends_with("item")) && odd_entries.len() < 20 {
                        odd_entries.push(format!("{} {:?} -> {} x{} ({class})", b.id, b.edid, e.item, e.count));
                    }
                    bump(&mut cnto_targets, format!("CONT->{class}"));
                    bump(&mut cnto_counts, if e.count <= 0 { "<=0" } else if e.count == 1 { "1" } else { ">1" });
                }
            }
            BaseData::Actor(a) => {
                for e in &a.entries {
                    let class = target_class(forms, e.item);
                    if (e.count <= 0 || !class.ends_with("item")) && odd_entries.len() < 20 {
                        odd_entries.push(format!("{} {:?} -> {} x{} ({class})", b.id, b.edid, e.item, e.count));
                    }
                    bump(&mut cnto_targets, format!("{}->{class}", sig_str(&b.sig)));
                    bump(&mut cnto_counts, if e.count <= 0 { "<=0" } else if e.count == 1 { "1" } else { ">1" });
                }
            }
            BaseData::LeveledItem(l) => {
                lvl_lists += 1;
                lvl_entries += l.entries.len() as u64;
                for (on, name) in [(l.all_levels, "all_levels"), (l.each, "each"), (l.use_all, "use_all")] {
                    if on {
                        bump(&mut lvl_flags, name);
                    }
                }
                if l.global.is_some() {
                    bump(&mut lvl_flags, "global");
                }
                for e in &l.entries {
                    bump(&mut lvlo_targets, target_class(forms, e.item));
                }
            }
            _ => {}
        }
    }

    // References.
    let mut ref_sigs: BTreeMap<String, u64> = BTreeMap::new();
    let mut ref_base: BTreeMap<String, u64> = BTreeMap::new();
    let mut persist = [[0u64; 2]; 2]; // [flag][group]
    let (mut disabled, mut with_count, mut owned) = (0u64, 0u64, 0u64);
    let mut lock_levels: BTreeMap<String, u64> = BTreeMap::new();
    let mut lock_keys: BTreeMap<String, u64> = BTreeMap::new();
    let mut lock_on: BTreeMap<String, u64> = BTreeMap::new();
    let mut tel: BTreeMap<String, u64> = BTreeMap::new();
    let mut tel_samples: Vec<String> = Vec::new();
    let mut ep: BTreeMap<String, u64> = BTreeMap::new();
    let mut ep_samples: Vec<String> = Vec::new();
    for r in refs.refs.values() {
        bump(&mut ref_sigs, sig_str(&r.sig));
        let base_sig = forms.sig(r.base).map(|s| sig_str(&s));
        bump(
            &mut ref_base,
            base_sig.clone().unwrap_or_else(|| {
                if r.base.0 < 0x800 { format!("engine_builtin:{}", r.base) } else { "missing".into() }
            }),
        );
        persist[(r.flags & FLAG_PERSISTENT != 0) as usize][r.in_persistent_group as usize] += 1;
        disabled += r.initially_disabled() as u64;
        with_count += r.count.is_some() as u64;
        owned += r.owner.is_some() as u64;
        if let Some(l) = r.lock {
            bump(&mut lock_levels, format!("{}{}", l.level, if l.key.is_some() { " with key" } else { "" }));
            bump(&mut lock_on, base_sig.clone().unwrap_or_else(|| "missing".into()));
            bump(
                &mut lock_keys,
                match l.key {
                    None => "none".into(),
                    Some(k) => match forms.get(k).map(|b| &b.data) {
                        Some(BaseData::Item(i)) if i.kind == ItemKind::Key => "KEYM".to_string(),
                        _ => format!("other:{}", forms.sig(k).map(|s| sig_str(&s)).unwrap_or_else(|| "missing".into())),
                    },
                },
            );
        }
        if let Some(e) = r.enable_parent {
            bump(&mut ep, if e.opposite { "opposite" } else { "same" });
            // Depth of the chain, and whether it ends at a missing reference or loops.
            let (mut cur, mut depth, mut seen) = (e.reference, 1u32, vec![r.id]);
            let end = loop {
                if seen.contains(&cur) {
                    break "loop";
                }
                seen.push(cur);
                match refs.get(cur) {
                    None if cur == FormId::PLAYER_REF => break "player",
                    None => break "missing parent",
                    Some(p) => match p.enable_parent {
                        None => break "ok",
                        Some(pp) => {
                            cur = pp.reference;
                            depth += 1;
                        }
                    },
                }
            };
            bump(&mut ep, format!("depth {depth}"));
            if end != "ok" {
                bump(&mut ep, end);
                if ep_samples.len() < 10 {
                    ep_samples.push(format!("{} -> {}: {end}", r.id, e.reference));
                }
            }
        }
        if let Some(t) = r.teleport {
            let class = match refs.get(t.door) {
                None => "destination missing".to_string(),
                Some(d) if forms.sig(d.base) != Some(*b"DOOR") => "destination not a door".into(),
                Some(d) => match d.teleport {
                    Some(back) if back.door == r.id => "reciprocal".into(),
                    Some(_) => "destination teleports elsewhere".into(),
                    None => "destination has no teleport".into(),
                },
            };
            if class != "reciprocal" && tel_samples.len() < 10 {
                tel_samples.push(format!("{} -> {}: {class}", r.id, t.door));
            }
            bump(&mut tel, class);
        }
    }

    let player_base = forms.get(FormId::PLAYER_BASE).map(|b| json!({"sig": sig_str(&b.sig), "edid": b.edid}));
    let player_ref = refs.get(FormId::PLAYER_REF).map(|r| json!({"sig": sig_str(&r.sig), "base": r.base}));
    let mut issues = forms.issues.clone();
    issues.merge(refs.issues.clone());
    json!({
        "format": "fnv.sim.forms/1",
        "plugins": lo.plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        "bases": by_kind,
        "items": item_stats.iter().map(|(k, v)| (k.clone(), json!({"total": v[0], "value_known": v[1], "weight_known": v[2]}))).collect::<serde_json::Map<_, _>>(),
        "carried_items": {"targets": cnto_targets, "counts": cnto_counts, "unusual_samples": odd_entries},
        "leveled_items": {"lists": lvl_lists, "entries": lvl_entries, "flags": lvl_flags, "targets": lvlo_targets},
        "globals": forms.globals.len(),
        "settings": forms.settings.len(),
        "player": {"base_0x7": player_base, "ref_0x14_placed": player_ref},
        "references": {
            "total": refs.refs.len(),
            "deleted": refs.deleted,
            "by_record": ref_sigs,
            "by_base_type": ref_base,
            "persistent_flag_vs_group": {
                "flag_and_persistent_group": persist[1][1],
                "flag_only": persist[1][0],
                "group_only": persist[0][1],
                "neither": persist[0][0],
            },
            "initially_disabled_flag": disabled,
            "with_count": with_count,
            "owned": owned,
            "locks": {"levels": lock_levels, "keys": lock_keys, "on_base_type": lock_on},
            "teleports": {"classes": tel, "samples": tel_samples},
            "enable_parents": {"classes": ep, "samples": ep_samples},
        },
        "issues": issues.to_json(),
    })
}

/// Every container base with its authored contents (fnv.sim.containers/1). Leveled
/// entries are listed as authored, not resolved (their resolution is random).
fn containers(forms: &FormStore) -> Value {
    let mut out = Vec::new();
    for b in forms.bases.values() {
        let BaseData::Container(c) = &b.data else { continue };
        let entries: Vec<_> = c
            .entries
            .iter()
            .map(|e| json!({"item": e.item, "count": e.count, "class": target_class(forms, e.item)}))
            .collect();
        out.push(json!({
            "formid": b.id,
            "edid": b.edid,
            "name": b.name,
            "respawns": c.respawns,
            "script": b.script,
            "entries": entries,
        }));
    }
    json!({"format": "fnv.sim.containers/1", "count": out.len(), "containers": out})
}
