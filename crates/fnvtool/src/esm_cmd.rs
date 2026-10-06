use crate::{EsmCmd, LoadOrderArgs};
use anyhow::{bail, Context};
use fnv_esm::decode::{decode_record, formid_str, Coverage};
use fnv_esm::inventory::{header_json, inventory_json, stats_json};
use fnv_esm::raw::{visit_records, write_subrecords};
use fnv_esm::survey::Survey;
use fnv_esm::{LoadOrder, Plugin, Schema};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

pub fn run(cmd: EsmCmd) -> anyhow::Result<Value> {
    match cmd {
        EsmCmd::Header { plugin } => Ok(header_json(&Plugin::open(plugin)?)),
        EsmCmd::Stats { plugin } => Ok(stats_json(&Plugin::open(plugin)?)),
        EsmCmd::Inventory { plugin } => Ok(inventory_json(&Plugin::open(plugin)?)),
        EsmCmd::Verify { plugin } => verify(&plugin),
        EsmCmd::Coverage { plugin, schema, max_rows } => {
            let schema = Schema::load_dir(schema)?;
            let p = Plugin::open(plugin)?;
            let mut cov = Coverage::default();
            visit_records(&p.nodes[1..], &mut |r, _| cov.add_record(r, &schema, None));
            let mut v = cov.to_json(max_rows);
            v["schema"] = json!("fnv.esm.coverage/1");
            v["plugin"] = json!(p.name);
            Ok(v)
        }
        EsmCmd::Decode { plugin, schema, formid: want_fid, edid } => {
            let schema = Schema::load_dir(schema)?;
            let p = Plugin::open(plugin)?;
            let want_edid = edid.map(|s| s.to_ascii_lowercase());
            let mut found = Vec::new();
            visit_records(&p.nodes[1..], &mut |r, _| {
                let hit = match (want_fid, &want_edid) {
                    (Some(f), _) => r.header.form_id == f,
                    (None, Some(e)) => fnv_esm::inventory::names(r)
                        .ok()
                        .and_then(|(ed, _)| ed)
                        .is_some_and(|ed| ed.to_ascii_lowercase() == *e),
                    _ => false,
                };
                if hit {
                    found.push(decode_record(r, &schema));
                }
            });
            if found.is_empty() {
                bail!("no matching record");
            }
            Ok(json!({"schema": "fnv.esm.decode/1", "plugin": p.name, "records": found}))
        }
        EsmCmd::Loadorder { lo, schema } => loadorder(&lo, schema.as_deref()),
        EsmCmd::Survey { lo, plugin } => survey(&lo, plugin.as_deref()),
    }
}

/// Structural verification: byte-identical rewrite, every compressed record
/// decompresses, every payload re-serializes identically from its subrecords.
fn verify(path: &Path) -> anyhow::Result<Value> {
    let data = fnv_esm::plugin::map_file(path).with_context(|| path.display().to_string())?;
    let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let p = Plugin::parse_bytes(data.clone(), name)?;
    let out = p.to_bytes();
    let identical = out == data;
    let first_diff = if identical { None } else { out.iter().zip(&data).position(|(a, b)| a != b) };
    let mut records = 0u64;
    let mut compressed = 0u64;
    let mut decompress_errors = Vec::new();
    let mut subrecord_errors = Vec::new();
    let mut payload_mismatch = Vec::new();
    let mut xxxx = 0u64;
    let mut trailer_anomalies = Vec::new();
    visit_records(&p.nodes, &mut |r, _| {
        records += 1;
        if r.is_compressed() {
            compressed += 1;
        }
        let payload = match r.payload_checked() {
            Ok((pl, t)) => {
                if t != fnv_inflate::Trailer::Verified {
                    trailer_anomalies.push(Value::from(format!(
                        "{} {} at 0x{:x}: {:?}",
                        r.sig_str(),
                        formid_str(r.header.form_id),
                        r.offset,
                        t
                    )));
                }
                pl
            }
            Err(e) => {
                decompress_errors.push(Value::from(e.to_string()));
                return;
            }
        };
        match fnv_esm::raw::parse_subrecords(&payload) {
            Ok(subs) => {
                xxxx += subs.iter().filter(|s| s.extended).count() as u64;
                if write_subrecords(&subs) != payload.as_ref() && payload_mismatch.len() < 50 {
                    payload_mismatch.push(Value::from(format!("{} {}", r.sig_str(), formid_str(r.header.form_id))));
                }
            }
            Err((pos, m)) => {
                if subrecord_errors.len() < 50 {
                    subrecord_errors.push(Value::from(format!(
                        "{} {} at +{pos}: {m}",
                        r.sig_str(),
                        formid_str(r.header.form_id)
                    )));
                }
            }
        }
    });
    Ok(json!({
        "schema": "fnv.esm.verify/1",
        "plugin": p.name,
        "roundtrip_identical": identical,
        "first_difference": first_diff,
        "records": records,
        "compressed_records": compressed,
        "extended_size_subrecords": xxxx,
        "decompress_errors": decompress_errors,
        "zlib_trailer_anomalies": trailer_anomalies,
        "subrecord_errors": subrecord_errors,
        "payload_roundtrip_mismatches": payload_mismatch,
    }))
}

fn open_load_order(args: &LoadOrderArgs) -> anyhow::Result<Vec<Plugin>> {
    args.plugins
        .iter()
        .map(|n| {
            let p = args.data_dir.join(n);
            Plugin::open(&p).with_context(|| p.display().to_string())
        })
        .collect()
}

/// Load-order resolution report. With --schema, also resolves every FormID-typed
/// field the schema knows about.
fn loadorder(args: &LoadOrderArgs, schema: Option<&Path>) -> anyhow::Result<Value> {
    let plugins = open_load_order(args)?;
    let lo = LoadOrder::new(&plugins)?;
    let schema = schema.map(Schema::load_dir).transpose()?;
    let mut per = Vec::new();
    let mut ref_total = 0u64;
    let mut ref_null = 0u64;
    let mut ref_ok = 0u64;
    let mut ref_bad_index = 0u64;
    let mut ref_missing: BTreeMap<String, u64> = BTreeMap::new();
    let mut ref_missing_examples: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (pi, p) in plugins.iter().enumerate() {
        let recs = &lo.records[pi];
        let self_idx = p.self_index() as u32;
        let mut new_forms = 0u64;
        let mut overrides = 0u64;
        let mut override_local: HashSet<u32> = HashSet::new();
        let mut override_types: BTreeMap<String, u64> = BTreeMap::new();
        for r in recs {
            if r.header.form_id >> 24 == self_idx {
                new_forms += 1;
            } else if r.header.form_id >> 24 < self_idx {
                overrides += 1;
                override_local.insert(r.header.form_id);
                *override_types.entry(r.sig_str()).or_default() += 1;
            }
        }
        let onam: HashSet<u32> = p.header.overridden_forms.iter().copied().collect();
        let onam_overridden = onam.intersection(&override_local).count();
        let mut not_in_onam: BTreeMap<String, u64> = BTreeMap::new();
        for r in recs {
            if override_local.contains(&r.header.form_id) && !onam.contains(&r.header.form_id) {
                *not_in_onam.entry(r.sig_str()).or_default() += 1;
            }
        }
        let unmappable: Vec<Value> = lo
            .unmappable
            .iter()
            .filter(|(x, _)| *x as usize == pi)
            .map(|(_, f)| Value::from(formid_str(*f)))
            .collect();
        // Overrides placed in cell temporary-children groups, and "injected" forms:
        // records using a master's FormID space whose base does not exist in any
        // earlier plugin.
        let mut temp_overrides: HashSet<u32> = HashSet::new();
        let mut injected: BTreeMap<String, u64> = BTreeMap::new();
        visit_records(&p.nodes[1..], &mut |r, stack| {
            if r.header.form_id >> 24 >= self_idx {
                return;
            }
            if stack.last().is_some_and(|g| g.group_type == fnv_esm::raw::group_type::CELL_TEMPORARY_CHILDREN) {
                temp_overrides.insert(r.header.form_id);
            }
            if let Some(g) = lo.to_global(pi, r.header.form_id) {
                let earlier = lo.index.get(&g).is_some_and(|d| d.iter().any(|x| (x.plugin as usize) < pi));
                if !earlier {
                    *injected.entry(r.sig_str()).or_default() += 1;
                }
            }
        });
        let temp_in_onam = temp_overrides.intersection(&onam).count();
        if let Some(schema) = &schema {
            for r in recs {
                let mut fids: Vec<u32> = Vec::new();
                let mut cov = Coverage::default();
                {
                    let mut sink = |v: u32| fids.push(v);
                    cov.add_record(r, schema, Some(&mut sink));
                }
                for v in fids {
                    ref_total += 1;
                    if v == 0 {
                        ref_null += 1;
                        continue;
                    }
                    match lo.to_global(pi, v) {
                        None => {
                            ref_bad_index += 1;
                            let ex = ref_missing_examples.entry("bad_master_index".into()).or_default();
                            if ex.len() < 5 {
                                ex.push(Value::from(format!(
                                    "{}: {} {} -> {}",
                                    p.name,
                                    r.sig_str(),
                                    formid_str(r.header.form_id),
                                    formid_str(v)
                                )));
                            }
                        }
                        Some(g) if lo.exists(g) => ref_ok += 1,
                        Some(g) => {
                            // FormIDs below 0x800 in the first master are engine
                            // built-ins that need not exist as records [hypothesis].
                            let key = if g & 0x00FF_FFFF < 0x800 && (g >> 24) == 0 { "builtin_range" } else { "missing" };
                            let kind = format!("{key}:{}", r.sig_str());
                            *ref_missing.entry(kind.clone()).or_default() += 1;
                            let ex = ref_missing_examples.entry(kind).or_default();
                            if ex.len() < 5 {
                                ex.push(Value::from(format!(
                                    "{}: {} {} -> {} (global {})",
                                    p.name,
                                    r.sig_str(),
                                    formid_str(r.header.form_id),
                                    formid_str(v),
                                    formid_str(g)
                                )));
                            }
                        }
                    }
                }
            }
        }
        per.push(json!({
            "plugin": p.name,
            "load_index": pi,
            "records": recs.len(),
            "new_forms": new_forms,
            "overrides": overrides,
            "unmappable_formids": unmappable,
            "onam_entries": onam.len(),
            "onam_entries_that_are_overrides": onam_overridden,
            "temporary_group_overrides": temp_overrides.len(),
            "temporary_group_overrides_in_onam": temp_in_onam,
            "injected_forms_by_type": injected,
            "overrides_by_type": override_types,
            "overrides_not_in_onam_by_type": not_in_onam,
        }));
    }
    let mut chains: BTreeMap<usize, u64> = BTreeMap::new();
    for defs in lo.index.values() {
        *chains.entry(defs.len()).or_default() += 1;
    }
    let mut v = json!({
        "schema": "fnv.esm.loadorder/1",
        "plugins": per,
        "unique_global_forms": lo.index.len(),
        "definitions_per_form": chains,
    });
    if schema.is_some() {
        v["references"] = json!({
            "formid_fields": ref_total,
            "null": ref_null,
            "resolved": ref_ok,
            "bad_master_index": ref_bad_index,
            "unresolved_by_kind": ref_missing,
            "unresolved_examples": ref_missing_examples,
        });
    }
    Ok(v)
}

fn survey(args: &LoadOrderArgs, only: Option<&str>) -> anyhow::Result<Value> {
    let plugins = open_load_order(args)?;
    let lo = LoadOrder::new(&plugins)?;
    let only = only.map(|s| s.to_ascii_lowercase());
    let mut s = Survey::default();
    let mut errors = Vec::new();
    for (pi, p) in plugins.iter().enumerate() {
        if let Some(o) = &only {
            if p.name.to_ascii_lowercase() != *o {
                continue;
            }
        }
        for r in &lo.records[pi] {
            match r.subrecords() {
                Ok(subs) => s.add(&r.sig_str(), &subs, &lo, pi),
                Err(e) => errors.push(Value::from(e.to_string())),
            }
        }
    }
    let mut v = s.to_json();
    v["errors"] = Value::Array(errors);
    Ok(v)
}
