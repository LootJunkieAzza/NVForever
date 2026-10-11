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

/// One PlayGroup or IsAnimPlaying call found in a script unit.
struct GroupCall {
    /// "PlayGroup" or "IsAnimPlaying".
    func: &'static str,
    /// The animation group code (`None`: IsAnimPlaying without a group).
    group: Option<u16>,
    /// PlayGroup's init flag.
    flag: Option<i32>,
    /// The explicit calling reference (`Ref.PlayGroup`): a form, or a local variable.
    on: Option<fnv_script::unit::RefSlot>,
}

/// The PlayGroup (0x1013) and IsAnimPlaying (0x1128) calls of one decoded unit, in
/// statements and in expressions.
fn group_calls(unit: &fnv_script::ScriptUnit, prog: &fnv_script::ir::Program) -> Vec<GroupCall> {
    use fnv_script::ir::{Arg, Call, Stmt, Token};
    fn of(unit: &fnv_script::ScriptUnit, c: &Call, out: &mut Vec<GroupCall>) {
        let func = match c.opcode {
            0x1013 => "PlayGroup",
            0x1128 => "IsAnimPlaying",
            _ => return,
        };
        let group = c.args.iter().find_map(|a| if let Arg::Enum(g) = a { Some(*g) } else { None });
        let flag = c.args.iter().find_map(|a| match a {
            Arg::Int(n) => Some(*n),
            Arg::Float(x) => Some(*x as i32),
            _ => None,
        });
        out.push(GroupCall { func, group, flag, on: c.on.and_then(|r| unit.slot(r)) });
    }
    fn expr(unit: &fnv_script::ScriptUnit, e: &fnv_script::ir::Expr, out: &mut Vec<GroupCall>) {
        for t in &e.0 {
            if let Token::Call { call } = t {
                of(unit, call, out);
            }
        }
    }
    let mut out = Vec::new();
    prog.walk(&mut |s| match s {
        Stmt::Call { call } => of(unit, call, &mut out),
        Stmt::Set { expr: e, .. } => expr(unit, e, &mut out),
        Stmt::If { arms, .. } => arms.iter().for_each(|a| expr(unit, &a.cond, &mut out)),
        Stmt::Unmatched { cond: Some(e), .. } => expr(unit, e, &mut out),
        _ => {}
    });
    out
}

/// A model's sequences: (name, cycle type, length in seconds).
type SeqList = Vec<(String, i64, f32)>;

/// `fnvtool sim playgroup-calls` (fnv.sim.playgroup_calls/1): which scripts call
/// PlayGroup and IsAnimPlaying, on which objects (the explicit reference's base, else the
/// bases that carry the script), with which groups and flags, and whether the object's
/// model has a sequence of the group's name (docs/spec-notes/activators.md §5).
#[allow(clippy::too_many_arguments)]
pub fn playgroup_calls(
    lo: &LoadOrder,
    schema: &Schema,
    forms: &FormStore,
    refs: &RefTable,
    vfs: &fnv_vfs::Vfs,
    nif: &fnv_nif::Schema,
    table: &fnv_script::FunctionTable,
) -> Value {
    let (units, _) = fnv_script::unit::read_load_order(lo, schema);
    let mut placed: BTreeMap<FormId, Vec<FormId>> = BTreeMap::new();
    for p in refs.refs.values() {
        placed.entry(p.base).or_default().push(p.id);
    }
    // Bases by the script they carry.
    let mut users: BTreeMap<u32, Vec<FormId>> = BTreeMap::new();
    for b in forms.bases.values() {
        if let Some(s) = b.script {
            users.entry(s.0).or_default().push(b.id);
        }
    }
    let group_name = |g: Option<u16>| match g {
        None => "(any)".to_string(),
        Some(g) => table.enum_name("anim_group", g).map(str::to_string).unwrap_or_else(|| format!("code {g}")),
    };
    let mut seq_cache: BTreeMap<String, Option<SeqList>> = BTreeMap::new();
    let mut seqs_of = |path: &str| -> Option<SeqList> {
        seq_cache
            .entry(path.to_string())
            .or_insert_with(|| {
                model(vfs, nif, path).ok().map(|(s, _)| {
                    s.iter()
                        .map(|v| {
                            let len = v["stop"].as_f64().unwrap_or(0.0) - v["start"].as_f64().unwrap_or(0.0);
                            (v["name"].as_str().unwrap_or("?").to_string(), v["cycle_type"].as_i64().unwrap_or(-1), len as f32)
                        })
                        .collect()
                })
            })
            .clone()
    };
    let mut by_group: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut by_target: BTreeMap<String, u64> = BTreeMap::new();
    let mut has_seq: BTreeMap<String, u64> = BTreeMap::new();
    let mut targets: BTreeMap<(u32, String), Value> = BTreeMap::new();
    // Calls whose object the survey cannot name from the unit alone (a reference
    // variable, a result script's subject, a script no base carries): listed for reading.
    let mut unresolved: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut calls_n = 0u64;
    for u in &units {
        let d = fnv_script::decode(&u.bytes, table);
        let calls = group_calls(u, &d.program);
        if calls.is_empty() {
            continue;
        }
        for c in calls {
            calls_n += 1;
            let g = group_name(c.group);
            let flag = c.flag.map(|f| f.to_string()).unwrap_or_else(|| "-".into());
            let note = |why: &str, unresolved: &mut BTreeMap<String, Vec<String>>| {
                let var = match c.on {
                    Some(fnv_script::unit::RefSlot::Local(i)) => u.var(i).map(|v| format!("{}.", v.name)).unwrap_or_else(|| format!("local {i}.")),
                    _ => String::new(),
                };
                let text = format!("{why}: {var}{} {g}{}", c.func, c.flag.map(|f| format!(" {f}")).unwrap_or_default());
                let v = unresolved.entry(u.key()).or_default();
                if !v.contains(&text) {
                    v.push(text);
                }
            };
            *by_group.entry(format!("{} {g}", c.func)).or_default().entry(flag).or_default() += 1;
            // The objects the call runs on: (base, the reference named, if any).
            let on: Vec<(FormId, Option<FormId>)> = match (c.on, &u.owner) {
                (Some(fnv_script::unit::RefSlot::Form(f)), _) => match refs.refs.get(&FormId(f)) {
                    Some(p) => vec![(p.base, Some(p.id))],
                    None => vec![(FormId(f), None)],
                },
                (Some(_), _) => {
                    *by_target.entry("a reference variable".into()).or_default() += 1;
                    note("a reference variable", &mut unresolved);
                    continue;
                }
                (None, fnv_script::unit::UnitOwner::Script { form }) => {
                    users.get(form).map(|v| v.iter().map(|b| (*b, None)).collect()).unwrap_or_default()
                }
                (None, o) => {
                    *by_target.entry(format!("no reference ({})", o.sig())).or_default() += 1;
                    note(&format!("the subject of a {} unit", o.sig()), &mut unresolved);
                    continue;
                }
            };
            if on.is_empty() {
                *by_target.entry("script carried by no base".into()).or_default() += 1;
                note("script carried by no base", &mut unresolved);
            }
            for (base, r) in on {
                let sig = forms.sig(base).map(|s| String::from_utf8_lossy(&s).to_string()).unwrap_or_else(|| "?".into());
                *by_target.entry(sig.clone()).or_default() += 1;
                let obj = fnv_world::placed::base_object(lo, schema, base.0);
                let path = obj.as_ref().and_then(|o| o.model_path());
                let edid = obj.and_then(|o| o.edid);
                let seqs = path.as_deref().and_then(&mut seqs_of);
                let matched = match (&seqs, c.group) {
                    (None, _) => "no model or unreadable",
                    (Some(_), None) => "any group",
                    (Some(s), Some(_)) if s.iter().any(|(n, _, _)| n.eq_ignore_ascii_case(&g)) => "sequence of that name",
                    (Some(s), Some(_)) if s.is_empty() => "model without sequences",
                    _ => "no sequence of that name",
                };
                *has_seq.entry(format!("{sig}: {matched}")).or_default() += 1;
                let row = targets.entry((base.0, u.key())).or_insert_with(|| {
                    let refs_of = placed.get(&base).cloned().unwrap_or_default();
                    json!({
                        "base": base.to_string(), "sig": sig, "edid": edid,
                        "model": path, "unit": u.key(),
                        "refs": refs_of.len(),
                        "ref_samples": refs_of.iter().take(4).map(|r| {
                            let p = &refs.refs[r];
                            json!({"ref": r.to_string(), "edid": p.edid, "cell": p.cell.map(|c| c.to_string())})
                        }).collect::<Vec<_>>(),
                        "sequences": seqs.as_ref().map(|s| s.iter().map(|(n, ct, len)| json!([n, ct, len])).collect::<Vec<_>>()),
                        "calls": [],
                    })
                });
                let text = format!(
                    "{}{} {g}{}",
                    r.map(|r| format!("{r}.")).unwrap_or_default(),
                    c.func,
                    c.flag.map(|f| format!(" {f}")).unwrap_or_default()
                );
                if let Some(a) = row["calls"].as_array_mut() {
                    if !a.iter().any(|v| v == &json!(text)) {
                        a.push(json!(text));
                    }
                }
            }
        }
    }
    json!({
        "schema": "fnv.sim.playgroup_calls/1",
        "summary": {"calls": calls_n, "by_function_group_flag": by_group, "by_target": by_target, "sequence_found": has_seq, "targets": targets.len()},
        "targets": targets.into_values().collect::<Vec<_>>(),
        "unresolved": unresolved,
    })
}
