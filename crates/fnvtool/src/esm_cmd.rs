use crate::{Args, CmdError};
use fnv_esm::decode::{decode_record, formid_str, Coverage};
use fnv_esm::inventory::{header_json, inventory_json, stats_json};
use fnv_esm::raw::{visit_records, write_subrecords};
use fnv_esm::survey::Survey;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

pub fn run(sub: &str, rest: &[String]) -> Result<Value, CmdError> {
    let args =
        Args::parse(rest, &["schema", "formid", "edid", "max-rows", "plugin"]).map_err(CmdError::Usage)?;
    let first = || args.positional.first().ok_or_else(|| CmdError::Usage("missing plugin path".into()));
    match sub {
        "header" => Ok(header_json(&Plugin::open(first()?)?)),
        "stats" => Ok(stats_json(&Plugin::open(first()?)?)),
        "inventory" => Ok(inventory_json(&Plugin::open(first()?)?)),
        "verify" => verify(first()?),
        "coverage" => {
            let schema = load_schema(&args)?;
            let p = Plugin::open(first()?)?;
            let mut cov = Coverage::default();
            visit_records(&p.nodes[1..], &mut |r, _| cov.add_record(r, &schema, None));
            let max = args.value("max-rows").and_then(|s| s.parse().ok()).unwrap_or(200);
            let mut v = cov.to_json(max);
            v.set("schema", "fnv.esm.coverage/1").set("plugin", p.name.as_str());
            Ok(v)
        }
        "decode" => {
            let schema = load_schema(&args)?;
            let p = Plugin::open(first()?)?;
            let want_fid = match args.value("formid") {
                Some(s) => Some(
                    u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
                        .map_err(|_| CmdError::Usage(format!("bad --formid {s}")))?,
                ),
                None => None,
            };
            let want_edid = args.value("edid").map(|s| s.to_ascii_lowercase());
            if want_fid.is_none() && want_edid.is_none() {
                return Err(CmdError::Usage("decode needs --formid or --edid".into()));
            }
            let mut found = Vec::new();
            visit_records(&p.nodes[1..], &mut |r, _| {
                let hit = match (want_fid, &want_edid) {
                    (Some(f), _) => r.header.form_id == f,
                    (None, Some(e)) => fnv_esm::inventory::names(r)
                        .ok()
                        .and_then(|(ed, _)| ed)
                        .map_or(false, |ed| ed.to_ascii_lowercase() == *e),
                    _ => false,
                };
                if hit {
                    found.push(decode_record(r, &schema));
                }
            });
            if found.is_empty() {
                return Err(CmdError::Failed("no matching record".into()));
            }
            let mut v = Value::object();
            v.set("schema", "fnv.esm.decode/1").set("plugin", p.name.as_str()).set("records", Value::Array(found));
            Ok(v)
        }
        "loadorder" => loadorder(&args),
        "survey" => survey(&args),
        other => Err(CmdError::Usage(format!("unknown esm subcommand {other:?}"))),
    }
}

fn load_schema(args: &Args) -> Result<Schema, CmdError> {
    let dir = args.value("schema").ok_or_else(|| CmdError::Usage("--schema <dir> is required".into()))?;
    Ok(Schema::load_dir(dir)?)
}

/// Structural verification: byte-identical rewrite, every compressed record
/// decompresses, every payload re-serializes identically from its subrecords.
fn verify(path: &str) -> Result<Value, CmdError> {
    let data = std::fs::read(path)?;
    let name = Path::new(path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let p = Plugin::parse(&data, name)?;
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
    let mut v = Value::object();
    v.set("schema", "fnv.esm.verify/1")
        .set("plugin", p.name.as_str())
        .set("roundtrip_identical", identical)
        .set("first_difference", first_diff)
        .set("records", records)
        .set("compressed_records", compressed)
        .set("extended_size_subrecords", xxxx)
        .set("decompress_errors", Value::Array(decompress_errors))
        .set("zlib_trailer_anomalies", Value::Array(trailer_anomalies))
        .set("subrecord_errors", Value::Array(subrecord_errors))
        .set("payload_roundtrip_mismatches", Value::Array(payload_mismatch));
    Ok(v)
}

fn open_load_order(args: &Args) -> Result<Vec<Plugin>, CmdError> {
    let dir = args.positional.first().ok_or_else(|| CmdError::Usage("missing data directory".into()))?;
    let names = &args.positional[1..];
    if names.is_empty() {
        return Err(CmdError::Usage("list at least one plugin, in load order".into()));
    }
    let mut plugins = Vec::new();
    for n in names {
        let p = Path::new(dir).join(n);
        plugins.push(Plugin::open(&p).map_err(|e| CmdError::Failed(format!("{}: {e}", p.display())))?);
    }
    Ok(plugins)
}

/// Load-order resolution report. With --schema, also resolves every FormID-typed
/// field the schema knows about.
fn loadorder(args: &Args) -> Result<Value, CmdError> {
    let plugins = open_load_order(args)?;
    let lo = LoadOrder::new(&plugins)?;
    let schema = match args.value("schema") {
        Some(d) => Some(Schema::load_dir(d)?),
        None => None,
    };
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
            if stack.last().map_or(false, |g| g.group_type == fnv_esm::raw::group_type::CELL_TEMPORARY_CHILDREN) {
                temp_overrides.insert(r.header.form_id);
            }
            if let Some(g) = lo.to_global(pi, r.header.form_id) {
                let earlier = lo.index.get(&g).map_or(false, |d| d.iter().any(|x| (x.plugin as usize) < pi));
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
        let mut o = Value::object();
        o.set("plugin", p.name.as_str())
            .set("load_index", pi)
            .set("records", recs.len())
            .set("new_forms", new_forms)
            .set("overrides", overrides)
            .set("unmappable_formids", Value::Array(unmappable))
            .set("onam_entries", onam.len())
            .set("onam_entries_that_are_overrides", onam_overridden)
            .set("temporary_group_overrides", temp_overrides.len())
            .set("temporary_group_overrides_in_onam", temp_in_onam)
            .set(
                "injected_forms_by_type",
                Value::Object(injected.into_iter().map(|(k, n)| (k, Value::from(n))).collect()),
            )
            .set(
                "overrides_by_type",
                Value::Object(override_types.into_iter().map(|(k, n)| (k, Value::from(n))).collect()),
            )
            .set(
                "overrides_not_in_onam_by_type",
                Value::Object(not_in_onam.into_iter().map(|(k, n)| (k, Value::from(n))).collect()),
            );
        per.push(o);
    }
    let mut chains: BTreeMap<usize, u64> = BTreeMap::new();
    for defs in lo.index.values() {
        *chains.entry(defs.len()).or_default() += 1;
    }
    let mut v = Value::object();
    v.set("schema", "fnv.esm.loadorder/1")
        .set("plugins", Value::Array(per))
        .set("unique_global_forms", lo.index.len())
        .set(
            "definitions_per_form",
            Value::Object(chains.into_iter().map(|(k, n)| (k.to_string(), Value::from(n))).collect()),
        );
    if schema.is_some() {
        let mut r = Value::object();
        r.set("formid_fields", ref_total)
            .set("null", ref_null)
            .set("resolved", ref_ok)
            .set("bad_master_index", ref_bad_index)
            .set("unresolved_by_kind", Value::Object(ref_missing.into_iter().map(|(k, n)| (k, Value::from(n))).collect()))
            .set(
                "unresolved_examples",
                Value::Object(ref_missing_examples.into_iter().map(|(k, v)| (k, Value::Array(v))).collect()),
            );
        v.set("references", r);
    }
    Ok(v)
}

fn survey(args: &Args) -> Result<Value, CmdError> {
    let plugins = open_load_order(args)?;
    let lo = LoadOrder::new(&plugins)?;
    let only = args.value("plugin").map(|s| s.to_ascii_lowercase());
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
    v.set("errors", Value::Array(errors));
    Ok(v)
}
