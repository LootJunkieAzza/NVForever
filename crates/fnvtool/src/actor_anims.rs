//! `fnvtool actor anims`: the animation data catalogue (`.kf` files by skeleton folder,
//! IDLE records, creature KFFZ/NIFZ lists). Contract: docs/spec-notes/animation.md §6.

use anyhow::Context;
use fnv_actor::idle::{Idles, ModelKind};
use fnv_actor::{anims, mesh};
use fnv_esm::{LoadOrder, Schema};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Condition function index → name, from the script function catalogue (opcode =
/// 0x1000 + index, docs/spec-notes/script-functions.md §4). Empty when the file is absent.
pub fn condition_function_names(path: &Path) -> anyhow::Result<BTreeMap<u16, String>> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else { return Ok(out) };
    let v: Value = serde_json::from_str(&text).with_context(|| path.display().to_string())?;
    for f in v["functions"].as_array().into_iter().flatten() {
        let Some(name) = f["name"].as_str() else { continue };
        for op in f["opcodes"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            let Ok(op) = u32::from_str_radix(op.trim_start_matches("0x"), 16) else { continue };
            if let Some(ix) = op.checked_sub(0x1000).and_then(|i| u16::try_from(i).ok()) {
                out.entry(ix).or_insert_with(|| name.to_string());
            }
        }
    }
    Ok(out)
}

#[derive(Default)]
struct Group {
    skeleton: Option<String>,
    rule: Option<anims::SkeletonRule>,
    skeleton_error: Option<String>,
    skeleton_nodes: usize,
    files: u64,
    unreadable: u64,
    sequences: u64,
    tracks: u64,
    bound: u64,
    unbound: BTreeMap<String, u64>,
    subfolders: BTreeMap<String, u64>,
    named_by_idle: u64,
    named_by_kffz: u64,
}

struct KfRow {
    path: String,
    group: anims::SkeletonMatch,
    sequences: Result<Vec<Value>, String>,
    binding: Option<anims::Binding>,
    count: u64,
    named_like_file: u64,
    text_keys: BTreeMap<String, u64>,
    names: Vec<String>,
    /// File stem minus a sequence name it ends with (`1hpattack5down` − `Attack5Down`).
    prefix: Option<String>,
}

/// Sequence names with `SpecialIdle_…`-style tails collapsed (`SpecialIdle_*`).
fn name_kind(name: &str) -> String {
    match name.split_once('_') {
        Some((head, _)) => format!("{head}_*"),
        None => name.to_string(),
    }
}

fn bump(m: &mut BTreeMap<String, u64>, k: &str) {
    *m.entry(k.to_string()).or_default() += 1;
}

fn read_nif(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, path: &str) -> Result<(fnv_nif::file::NifFile, Vec<u8>), String> {
    let data = match vfs.read(path) {
        None => return Err("not found".to_string()),
        Some(Err(e)) => return Err(e.to_string()),
        Some(Ok(d)) => d,
    };
    let f = fnv_nif::file::NifFile::parse(&data, Some(nif)).map_err(|e| e.to_string())?;
    Ok((f, data))
}

/// Sequences of one `.kf`, bound against the skeleton's node names when known.
fn kf_row(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, path: &str, group: anims::SkeletonMatch, nodes: Option<&BTreeSet<String>>) -> KfRow {
    let mut row = KfRow {
        path: path.to_string(),
        group,
        sequences: Ok(Vec::new()),
        binding: None,
        count: 0,
        named_like_file: 0,
        text_keys: BTreeMap::new(),
        names: Vec::new(),
        prefix: None,
    };
    let (file, data) = match read_nif(vfs, nif, path) {
        Ok(x) => x,
        Err(e) => {
            row.sequences = Err(e);
            return row;
        }
    };
    let dec = fnv_nif::scene::Decoded::new(&file, &data, nif);
    let stem = anims::file_name(path).trim_end_matches(".kf");
    let mut total = anims::Binding::default();
    let mut seqs = Vec::new();
    for s in fnv_nif::anim::sequences(&file, &dec) {
        row.count += 1;
        if let Some(n) = &s.name {
            let lower = n.to_ascii_lowercase();
            if lower == stem {
                row.named_like_file += 1;
            } else if row.prefix.is_none() && !lower.is_empty() {
                row.prefix = stem.strip_suffix(lower.as_str()).map(str::to_string);
            }
            row.names.push(n.clone());
        }
        for (_, k) in &s.text_keys {
            // The word before any ':' or space (`Sound: …` → `sound`); exporter option
            // strings (`-globalRatio …`) count as `-options`.
            let word = k.split(|c: char| c == ':' || c.is_whitespace()).next().unwrap_or_default().to_ascii_lowercase();
            let kind = if word.starts_with('-') { "-options".to_string() } else { word };
            *row.text_keys.entry(kind).or_default() += 1;
        }
        let b = nodes.map(|n| anims::bind(s.tracks.iter().map(|t| t.node_name.as_deref()), n));
        if let Some(b) = &b {
            total.tracks += b.tracks;
            total.bound += b.bound;
            for (k, v) in &b.unbound {
                *total.unbound.entry(k.clone()).or_default() += v;
            }
        }
        seqs.push(json!({"name": s.name, "start_time": f64::from(s.start_time), "stop_time": f64::from(s.stop_time),
            "cycle_type": s.cycle_type, "text_keys": s.text_keys.len(), "tracks": s.tracks.len(),
            "bound": b.map(|b| b.bound)}));
    }
    row.binding = nodes.map(|_| total);
    row.sequences = Ok(seqs);
    row
}

pub fn report(
    lo: &LoadOrder,
    schema: &Schema,
    vfs: &fnv_vfs::Vfs,
    nif: &fnv_nif::Schema,
    fn_names: &BTreeMap<u16, String>,
    summary_only: bool,
) -> Value {
    let mesh_paths = vfs.paths(|p| p.starts_with("meshes\\") && (p.ends_with(".kf") || p.ends_with(".nif")));
    let skels = anims::SkeletonIndex::from_paths(mesh_paths.iter().map(String::as_str));
    let kfs: Vec<&String> = mesh_paths.iter().filter(|p| p.ends_with(".kf")).collect();
    let kf_dirs: BTreeSet<&str> = kfs.iter().filter_map(|p| anims::parent(p)).collect();

    // Skeleton node names, once per skeleton.
    let wanted: BTreeSet<String> = kfs.iter().filter_map(|k| skels.skeleton_for(k).skeleton).collect();
    let skeleton_nodes: BTreeMap<String, Result<BTreeSet<String>, String>> = wanted
        .into_par_iter()
        .map(|s| {
            let r = read_nif(vfs, nif, &s)
                .map(|(f, data)| mesh::analyse(&f, &fnv_nif::scene::Decoded::new(&f, &data, nif)).node_names);
            (s, r)
        })
        .collect();
    let rows: Vec<KfRow> = kfs
        .par_iter()
        .map(|&path| {
            let group = skels.skeleton_for(path);
            let nodes = group.skeleton.as_ref().and_then(|s| skeleton_nodes.get(s)).and_then(|r| r.as_ref().ok());
            kf_row(vfs, nif, path, group, nodes)
        })
        .collect();

    // IDLE records.
    let idles = Idles::build(lo, schema);
    let mut idle_kfs: BTreeSet<String> = BTreeSet::new();
    let mut by_section: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut idle_groups: BTreeMap<String, u64> = BTreeMap::new();
    let mut cond_functions: BTreeMap<String, u64> = BTreeMap::new();
    let mut run_on: BTreeMap<String, u64> = BTreeMap::new();
    let (mut conditions, mut idle_missing, mut idle_rows) = (0u64, Vec::new(), Vec::new());
    for i in idles.idles.values() {
        let section = match (i.section(), i.section_name()) {
            (_, Some(n)) => n.to_string(),
            (Some(s), None) => format!("unknown {s}"),
            (None, None) => "no DATA".to_string(),
        };
        let e = by_section.entry(section.clone()).or_default();
        bump(e, "records");
        if let Some(h) = i.group_high_bits() {
            bump(e, &format!("high_bits_0x{h:02X}"));
        }
        let found = match (i.model_kind(), i.model_path()) {
            (ModelKind::Kf, Some(p)) => {
                let found = vfs.exists(&p);
                bump(e, if found { "kf_found" } else { "kf_missing" });
                if found {
                    bump(&mut idle_groups, &skels.skeleton_for(&p).folder);
                    idle_kfs.insert(p);
                } else {
                    idle_missing.push(json!({"form": i.id.to_string(), "edid": i.edid, "section": section, "model": i.model}));
                }
                Some(found)
            }
            (ModelKind::Other, Some(p)) => {
                let prefix = format!("{p}\\");
                let found = kf_dirs.contains(p.as_str()) || kf_dirs.iter().any(|d| d.starts_with(&prefix));
                bump(e, if found { "folder_with_kf" } else { "other_model" });
                Some(found)
            }
            _ => {
                bump(e, "no_model");
                None
            }
        };
        for c in &i.conditions {
            conditions += 1;
            bump(&mut cond_functions, &fn_names.get(&c.function).cloned().unwrap_or_else(|| format!("index {}", c.function)));
            bump(&mut run_on, &c.run_on.map_or("absent".to_string(), |r| r.to_string()));
        }
        if !summary_only {
            idle_rows.push(json!({
                "form": i.id.to_string(), "edid": i.edid, "plugin": lo.plugins[i.plugin].name, "model": i.model,
                "model_kind": i.model_kind(), "found": found, "group_raw": i.group_raw, "section": section,
                "loop": [i.loop_min, i.loop_max], "replay_delay": i.replay_delay, "flags": i.flags,
                "parent": i.parent.map(|p| p.to_string()), "previous": i.previous.map(|p| p.to_string()),
                "conditions": i.conditions.iter().map(|c| json!({
                    "function": c.function, "name": fn_names.get(&c.function), "operator": c.operator(), "or": c.or(),
                    "type_raw": c.type_raw, "comparison": c.comparison_f32().map(f64::from),
                    "comparison_raw": c.comparison_raw, "param1_raw": c.param1_raw, "param2_raw": c.param2_raw,
                    "run_on": c.run_on, "reference_raw": c.reference_raw})).collect::<Vec<_>>(),
            }));
        }
    }

    // Actor-base KFFZ / NIFZ lists.
    let mut base_issues = fnv_sim::extract::Issues::default();
    let lists = anims::actor_anim_lists(lo, schema, &mut base_issues);
    let mut kffz_kfs: BTreeSet<String> = BTreeSet::new();
    let (mut kffz, mut nifz) = (BTreeMap::new(), BTreeMap::new());
    let (mut kffz_missing, mut nifz_missing) = (Vec::new(), Vec::new());
    for a in &lists {
        if !a.kffz.is_empty() {
            bump(&mut kffz, &format!("{} with KFFZ", a.sig));
        }
        if !a.nifz.is_empty() {
            bump(&mut nifz, &format!("{} with NIFZ", a.sig));
        }
        let Some(skel) = &a.skeleton else {
            if !a.kffz.is_empty() || !a.nifz.is_empty() {
                bump(&mut kffz, "lists without MODL");
            }
            continue;
        };
        for n in &a.kffz {
            let c = anims::kffz_candidates(skel, n);
            match c.iter().position(|p| vfs.exists(p)) {
                Some(h) => {
                    bump(&mut kffz, if h == 0 { "names found in specialanims" } else { "names found in skeleton folder" });
                    kffz_kfs.insert(c[h].clone());
                }
                None => {
                    bump(&mut kffz, "names missing");
                    kffz_missing.push(json!({"form": a.id.to_string(), "edid": a.edid, "skeleton": skel, "name": n}));
                }
            }
        }
        for n in &a.nifz {
            let p = format!("{}\\{}", anims::parent(skel).unwrap_or_default(), fnv_vfs::normalize(n));
            if vfs.exists(&p) {
                bump(&mut nifz, "names found in skeleton folder");
            } else {
                bump(&mut nifz, "names missing");
                nifz_missing.push(json!({"form": a.id.to_string(), "edid": a.edid, "skeleton": skel, "name": n}));
            }
        }
    }

    // The .kf rows per skeleton folder.
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    let mut by_rule: BTreeMap<String, u64> = BTreeMap::new();
    let mut text_keys: BTreeMap<String, u64> = BTreeMap::new();
    let (mut sequences, mut tracks, mut bound, mut unreadable, mut named_like_file, mut not_one) = (0, 0, 0, 0, 0, 0);
    let (mut name_kinds, mut prefixes): (BTreeMap<String, u64>, BTreeMap<String, u64>) = Default::default();
    let mut files = Vec::new();
    for r in &rows {
        let g = groups.entry(r.group.folder.clone()).or_default();
        if g.rule.is_none() {
            g.rule = Some(r.group.rule);
            g.skeleton = r.group.skeleton.clone();
            match r.group.skeleton.as_ref().and_then(|s| skeleton_nodes.get(s)) {
                Some(Ok(n)) => g.skeleton_nodes = n.len(),
                Some(Err(e)) => g.skeleton_error = Some(e.clone()),
                None => {}
            }
        }
        bump(&mut by_rule, &format!("{:?}", r.group.rule).to_ascii_lowercase());
        g.files += 1;
        let base = if r.path.starts_with(&format!("{}\\", r.group.folder)) { r.group.folder.clone() } else { anims::actor_folder(&r.path) };
        bump(&mut g.subfolders, &anims::subfolder(&base, &r.path));
        for n in &r.names {
            bump(&mut name_kinds, &name_kind(n));
        }
        bump(&mut prefixes, r.prefix.as_deref().unwrap_or(if r.named_like_file > 0 { "(named like file)" } else { "(no suffix match)" }));
        g.named_by_idle += u64::from(idle_kfs.contains(&r.path));
        g.named_by_kffz += u64::from(kffz_kfs.contains(&r.path));
        if r.sequences.is_err() {
            g.unreadable += 1;
            unreadable += 1;
        }
        g.sequences += r.count;
        sequences += r.count;
        named_like_file += r.named_like_file;
        not_one += u64::from(r.count != 1);
        for (k, v) in &r.text_keys {
            *text_keys.entry(k.clone()).or_default() += v;
        }
        if let Some(b) = &r.binding {
            g.tracks += b.tracks as u64;
            g.bound += b.bound as u64;
            tracks += b.tracks as u64;
            bound += b.bound as u64;
            for (k, v) in &b.unbound {
                *g.unbound.entry(k.clone()).or_default() += *v as u64;
            }
        }
        if !summary_only {
            files.push(match &r.sequences {
                Ok(seqs) => json!({"path": r.path, "group": r.group.folder, "sequences": seqs}),
                Err(e) => json!({"path": r.path, "group": r.group.folder, "error": e}),
            });
        }
    }
    let group_rows: Vec<Value> = groups
        .iter()
        .map(|(folder, g)| {
            let mut unbound: Vec<(&String, &u64)> = g.unbound.iter().collect();
            unbound.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            json!({"folder": folder, "skeleton": g.skeleton, "skeleton_rule": g.rule, "skeleton_error": g.skeleton_error,
                "skeleton_nodes": g.skeleton_nodes, "skeletons_in_folder": skels.skeletons_in(folder).len(),
                "files": g.files, "unreadable": g.unreadable, "sequences": g.sequences, "tracks": g.tracks, "bound": g.bound,
                "subfolders": g.subfolders, "named_by_idle": g.named_by_idle, "named_by_kffz": g.named_by_kffz,
                "unbound_names": g.unbound.len(),
                "unbound_top": unbound.iter().take(12).map(|(k, v)| json!([k, v])).collect::<Vec<_>>()})
        })
        .collect();
    let mut issues = idles.issues.to_json();
    if !base_issues.is_empty() {
        issues["actor_bases"] = base_issues.to_json();
    }
    let mut out = json!({
        "schema": "fnv.actor.anims/1",
        "summary": {
            "kf_files": rows.len(), "unreadable": unreadable, "sequences": sequences,
            "files_not_one_sequence": not_one, "sequences_named_like_file": named_like_file,
            "groups": groups.len(), "files_by_skeleton_rule": by_rule, "tracks_checked": tracks, "tracks_bound": bound,
            "text_key_kinds": text_keys, "sequence_name_kinds": name_kinds, "file_prefixes": prefixes,
            "idles": {"records": idles.idles.len(), "conditions": conditions, "by_section": by_section,
                      "found_kf_by_group": idle_groups, "tree": idles.tree_stats(),
                      "condition_functions": cond_functions, "condition_run_on": run_on},
            "kffz": kffz, "nifz": nifz,
        },
        "groups": group_rows,
        "idle_missing": idle_missing,
        "kffz_missing": kffz_missing,
        "nifz_missing": nifz_missing,
        "issues": issues,
    });
    if !summary_only {
        out["files"] = json!(files);
        out["idles"] = json!(idle_rows);
    }
    out
}
