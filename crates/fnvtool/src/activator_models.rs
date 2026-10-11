//! `fnvtool sim activator-models` (fnv.sim.activator_models/1): every activator and
//! talking activator base with the controller sequences of its model and what their
//! tracks drive (docs/spec-notes/activators.md). Observational: it decides which
//! activators can show an on/off look or play an animation, and how.

use fnv_esm::{LoadOrder, Schema};
use fnv_nif::anim::TrackData;
use fnv_sim::{FormId, FormStore, RefTable};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn bump(m: &mut BTreeMap<String, u64>, k: impl Into<String>, n: u64) {
    *m.entry(k.into()).or_default() += n;
}

/// What one track drives: controller type, controller ID with the texture slot part
/// kept (`0-0-TT_TRANSLATE_U`), and the interpolator kind.
fn track_kind(t: &fnv_nif::anim::Track) -> String {
    let ctl = t.controller_type.as_deref().unwrap_or("-");
    let id = t.controller_id.as_deref().unwrap_or("");
    let interp = match &t.data {
        TrackData::Transform(_) => "transform",
        TrackData::Other(o) => o.as_str(),
        TrackData::None => "none",
    };
    if id.is_empty() {
        format!("{ctl} ({interp})")
    } else {
        format!("{ctl} {id} ({interp})")
    }
}

/// The sequences of one model; also the time controllers of the file outside any
/// controller manager (their own clock, played without a sequence) by type.
fn model(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, path: &str) -> Result<(Vec<Value>, Vec<String>), String> {
    use fnv_nif::scene::Decoded;
    let data = match vfs.read(path) {
        None => return Err("not found".into()),
        Some(Err(e)) => return Err(e.to_string()),
        Some(Ok(d)) => d,
    };
    let f = fnv_nif::NifFile::parse(&data, Some(nif)).map_err(|e| e.to_string())?;
    let dec = Decoded::new(&f, &data, nif);
    let seqs = fnv_nif::anim::sequences(&f, &dec)
        .into_iter()
        .map(|s| {
            let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
            for t in &s.tracks {
                bump(&mut kinds, track_kind(t), 1);
            }
            json!({"name": s.name, "start": s.start_time, "stop": s.stop_time, "cycle_type": s.cycle_type,
                "tracks": kinds, "text_keys": s.text_keys.iter().map(|(t, k)| json!([t, k])).collect::<Vec<_>>()})
        })
        .collect();
    // Controllers not managed by a sequence: those whose flags lack bit 5
    // "ManagerControlled" [documented: nif.xml TimeControllerFlags].
    let mut free = Vec::new();
    for (i, b) in f.blocks.iter().enumerate() {
        if !b.type_name.ends_with("Controller") || b.type_name == "NiControllerManager" || b.type_name == "NiMultiTargetTransformController" {
            continue;
        }
        let Ok(s) = &dec.blocks[i] else { continue };
        if s.int("flags").is_some_and(|fl| fl & 0x20 == 0) {
            free.push(b.type_name.clone());
        }
    }
    Ok((seqs, free))
}

pub fn activator_models(lo: &LoadOrder, schema: &Schema, forms: &FormStore, refs: &RefTable, vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema) -> Value {
    let radio = fnv_world::radio::RadioForms::build(lo, schema);
    let mut placed: BTreeMap<FormId, u64> = BTreeMap::new();
    for p in refs.refs.values() {
        if matches!(forms.sig(p.base), Some(s) if &s == b"ACTI" || &s == b"TACT") {
            *placed.entry(p.base).or_default() += 1;
        }
    }
    let script_name = |id: Option<FormId>| id.and_then(|s| fnv_world::placed::base_object(lo, schema, s.0)).and_then(|b| b.edid);
    // Per kind of base (ACTI, TACT, receiver): by the set of sequence names, bases and refs.
    let mut by_names: BTreeMap<String, BTreeMap<String, [u64; 2]>> = BTreeMap::new();
    // Per sequence name: tracks by what they drive (count of models).
    let mut drives: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut cycles: BTreeMap<String, u64> = BTreeMap::new();
    let mut free_controllers: BTreeMap<String, u64> = BTreeMap::new();
    let mut models_seen: BTreeSet<String> = BTreeSet::new();
    let mut receivers = Vec::new();
    let mut rows = Vec::new();
    for b in forms.bases.values().filter(|b| &b.sig == b"ACTI" || &b.sig == b"TACT") {
        let Some(obj) = fnv_world::placed::base_object(lo, schema, b.id.0) else { continue };
        let refs_n = placed.get(&b.id).copied().unwrap_or(0);
        let path = obj.model_path();
        let (seqs, free, error) = match path.as_deref().map(|m| model(vfs, nif, m)) {
            None => (vec![], vec![], Some("no model".to_string())),
            Some(Ok((s, f))) => (s, f, None),
            Some(Err(e)) => (vec![], vec![], Some(e)),
        };
        let mut names: Vec<String> = seqs.iter().map(|s| s["name"].as_str().unwrap_or("?").to_string()).collect();
        names.sort();
        let key = if error.is_some() { "(no model or error)".to_string() } else if names.is_empty() { "(none)".to_string() } else { names.join("+") };
        let receiver = radio.receivers.get(&b.id.0);
        let kind = if receiver.is_some() { "receiver" } else if &b.sig == b"TACT" { "TACT" } else { "ACTI" };
        for k in [kind, "all"] {
            let e = by_names.entry(k.to_string()).or_default().entry(key.clone()).or_default();
            e[0] += 1;
            e[1] += refs_n;
        }
        // Count each model file once for what its sequences drive.
        if path.as_ref().is_some_and(|p| models_seen.insert(p.clone())) {
            for s in &seqs {
                let name = s["name"].as_str().unwrap_or("?").to_string();
                bump(&mut cycles, format!("{name}: {}", s["cycle_type"]), 1);
                if let Some(t) = s["tracks"].as_object() {
                    for (k, n) in t {
                        bump(drives.entry(name.clone()).or_default(), k.clone(), n.as_u64().unwrap_or(0));
                    }
                }
            }
            for c in &free {
                bump(&mut free_controllers, c.clone(), 1);
            }
        }
        let row = json!({
            "base": b.id.to_string(), "sig": String::from_utf8_lossy(&b.sig), "edid": b.edid, "model": path,
            "script": script_name(b.script), "refs": refs_n, "sequences": seqs,
            "controllers_without_sequence": free, "error": error,
        });
        if let Some(r) = receiver {
            let station = radio.stations.get(&r.station).and_then(|s| s.edid.clone());
            let mut row = row.clone();
            row["station"] = json!(station);
            receivers.push(row);
        }
        if !seqs.is_empty() {
            rows.push(row);
        }
    }
    json!({
        "schema": "fnv.sim.activator_models/1",
        "summary": {
            "by_sequence_names": by_names
                .into_iter()
                .map(|(k, m)| (k, Value::Object(m.into_iter().map(|(n, v)| (n, json!({"bases": v[0], "refs": v[1]}))).collect())))
                .collect::<serde_json::Map<_, _>>(),
            "tracks_by_sequence_name": drives,
            "cycle_types": cycles,
            "models": models_seen.len(),
            "controllers_without_sequence": free_controllers,
        },
        "receivers": receivers,
        "with_sequences": rows,
    })
}
