//! `fnvtool actor furniture`: every furniture base (FURN) with the entry points of its
//! model, how many references place it, and the clips the IDLE tree names for using
//! it. Contract: docs/spec-notes/furniture.md §6.

use fnv_actor::furniture::{self, state, UseIdles, User};
use fnv_actor::idle::Idles;
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::extract::Issues;
use fnv_sim::{FormId, RefTable};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn bump(m: &mut BTreeMap<String, u64>, k: impl Into<String>, n: u64) {
    *m.entry(k.into()).or_default() += n;
}

/// The markers of a model, or why they could not be read.
fn model_markers(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, path: &str) -> Result<Vec<furniture::Marker>, String> {
    let data = match vfs.read(path) {
        Some(Ok(d)) => d,
        Some(Err(e)) => return Err(e.to_string()),
        None => return Err("not found".into()),
    };
    let file = fnv_nif::file::NifFile::parse(&data, Some(nif)).map_err(|e| e.to_string())?;
    let dec = fnv_nif::scene::Decoded::new(&file, &data, nif);
    Ok(furniture::markers(&file, &dec))
}

/// The first sequence of a `.kf`: name, length, cycle type and text keys.
fn clip_row(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, path: &str) -> Value {
    let data = match vfs.read(path) {
        Some(Ok(d)) => d,
        Some(Err(e)) => return json!({"path": path, "error": e.to_string()}),
        None => return json!({"path": path, "error": "not found"}),
    };
    let file = match fnv_nif::file::NifFile::parse(&data, Some(nif)) {
        Ok(f) => f,
        Err(e) => return json!({"path": path, "error": e.to_string()}),
    };
    let dec = fnv_nif::scene::Decoded::new(&file, &data, nif);
    let Some(s) = fnv_nif::anim::sequences(&file, &dec).into_iter().next() else {
        return json!({"path": path, "error": "no sequence"});
    };
    json!({
        "path": path,
        "name": s.name,
        "start_time": s.start_time,
        "stop_time": s.stop_time,
        "cycle_type": s.cycle_type,
        "accum_root": s.accum_root_name,
        "tracks": s.tracks.len(),
        "body": furniture::ClipEnds::of(&s),
        "text_keys": s.text_keys.iter().map(|(t, k)| json!([t, k.trim()])).collect::<Vec<_>>(),
    })
}

pub fn report(lo: &LoadOrder, schema: &Schema, vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, summary_only: bool) -> Value {
    let mut issues = Issues::default();
    let bases = furniture::furniture(lo, schema, &mut issues);
    let refs = RefTable::build(lo, schema);
    let mut placed: BTreeMap<FormId, u64> = BTreeMap::new();
    for r in refs.refs.values() {
        *placed.entry(r.base).or_default() += 1;
    }

    let mut rows = Vec::new();
    let mut by_ref_pair: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_orientation: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_flags: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_marker_count: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut placed_by_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut flag_bits_vs_markers: BTreeMap<String, u64> = BTreeMap::new();
    let mut heights: BTreeMap<String, u64> = BTreeMap::new();
    let mut placed_by_id: BTreeMap<String, u64> = BTreeMap::new();
    let mut type_by_id: BTreeMap<String, u64> = BTreeMap::new();
    let mut none_enabled = 0u64;
    let mut seen_ids: std::collections::BTreeSet<u8> = Default::default();
    let (mut no_model, mut unreadable, mut no_markers) = (0u64, 0u64, 0u64);
    for f in &bases {
        let n_placed = placed.get(&f.id).copied().unwrap_or(0);
        let markers = match &f.model {
            None => {
                no_model += 1;
                Err("no model".to_string())
            }
            Some(m) => model_markers(vfs, nif, m),
        };
        match &markers {
            Err(_) if f.model.is_some() => unreadable += 1,
            Ok(m) if m.is_empty() => no_markers += 1,
            _ => {}
        }
        let list = markers.as_deref().unwrap_or_default();
        bump(&mut by_marker_count, list.len().to_string(), 1);
        bump(&mut by_flags, f.marker_flags.map_or("none".to_string(), |v| format!("0x{v:08X}")), 1);
        if let Some(flags) = f.marker_flags {
            // How the low bits compare with the markers the model has.
            let low = flags & 0x00FF_FFFF;
            let all = if list.is_empty() { 0 } else { (1u32 << list.len()) - 1 };
            let rel = if low == all {
                "low bits = one per marker"
            } else if low & !all == 0 {
                "low bits ⊂ markers"
            } else {
                "low bits beyond the markers"
            };
            bump(&mut flag_bits_vs_markers, rel, 1);
        }
        let type_bits = match (f.sit(), f.sleep()) {
            (true, false) => "sit",
            (false, true) => "sleep",
            (true, true) => "sit+sleep",
            (false, false) => "neither",
        };
        if !list.is_empty() && !(0..list.len()).any(|i| f.marker_enabled(i)) {
            none_enabled += 1;
        }
        let mut ids: Vec<u8> = Vec::new();
        for m in list {
            bump(&mut by_ref_pair, format!("{}/{}", m.ref_1, m.ref_2), 1);
            bump(&mut by_orientation, m.orientation.to_string(), 1);
            bump(&mut heights, format!("id {} z {:.0}", m.ref_1, m.offset[2]), 1);
            bump(&mut type_by_id, format!("{type_bits}: id {}", m.ref_1), 1);
            if !ids.contains(&m.ref_1) {
                ids.push(m.ref_1);
            }
        }
        ids.sort_unstable();
        seen_ids.extend(ids.iter().copied());
        let kind = if ids.is_empty() { "none".to_string() } else { ids.iter().map(u8::to_string).collect::<Vec<_>>().join("+") };
        bump(&mut by_kind, kind.clone(), 1);
        bump(&mut placed_by_kind, kind.clone(), n_placed);
        for &id in &ids {
            bump(&mut placed_by_id, id.to_string(), n_placed);
        }
        if !summary_only {
            rows.push(json!({
                "form": f.id.to_string(),
                "edid": f.edid,
                "name": f.name,
                "model": f.model,
                "marker_flags": f.marker_flags.map(|v| format!("0x{v:08X}")),
                "sit": f.sit(),
                "sleep": f.sleep(),
                "enabled_markers": (0..list.len()).filter(|&i| f.marker_enabled(i)).collect::<Vec<_>>(),
                "has_script": f.has_script,
                "placed": n_placed,
                "marker_ids": ids,
                "markers": match &markers {
                    Ok(m) => json!(m),
                    Err(e) => json!({"error": e}),
                },
            }));
        }
    }

    // The clips the IDLE tree names for each marker ID in use, for a grown man who is not
    // the player (and the loop the player's first-person view gets).
    let idles = Idles::build(lo, schema);
    let tree = UseIdles::build(&idles, lo);
    let mut clips = serde_json::Map::new();
    for &id in &seen_ids {
        let user = |state: u8| User {
            state,
            marker_id: id,
            furniture_ref: FormId(0),
            furniture_base: FormId(0),
            actor_base: FormId(0),
            first_person: false,
            female: false,
            child: false,
            alerted: false,
            attacked: false,
            random_percent: 99,
        };
        let row = |u: User| match tree.pick(&u) {
            Some(p) => {
                let mut v = clip_row(vfs, nif, &p.clip);
                v["idle"] = json!(p.edid);
                v
            }
            None => Value::Null,
        };
        clips.insert(
            id.to_string(),
            json!({
                "loop": row(user(state::LOOP)),
                "loop_first_person": row(User { first_person: true, actor_base: FormId(7), ..user(state::LOOP) }),
                "get_on": row(user(state::GETTING_ON)),
                "get_on_female": row(User { female: true, ..user(state::GETTING_ON) }),
                "get_off": row(user(state::GETTING_OFF)),
                "get_off_alerted": row(User { alerted: true, ..user(state::GETTING_OFF) }),
            }),
        );
    }

    json!({
        "schema": "fnv.actor.furniture/1",
        "summary": {
            "furniture": bases.len(),
            "placed_references": bases.iter().map(|f| placed.get(&f.id).copied().unwrap_or(0)).sum::<u64>(),
            "no_model": no_model,
            "model_unreadable": unreadable,
            "model_without_markers": no_markers,
            "with_script": bases.iter().filter(|f| f.has_script).count(),
            "with_name": bases.iter().filter(|f| f.name.is_some()).count(),
            "by_marker_count": by_marker_count,
            "markers_by_ref_pair": by_ref_pair,
            "markers_by_orientation": by_orientation,
            "markers_by_kind_and_height": heights,
            "by_marker_flags": by_flags,
            "marker_flags_vs_markers": flag_bits_vs_markers,
            "by_kind": by_kind,
            "placed_by_kind": placed_by_kind,
            "placed_by_marker_id": placed_by_id,
            "use_idles": tree.len(),
            "markers_by_type_bits_and_id": type_by_id,
            "with_markers_but_none_enabled": none_enabled,
        },
        "clips": clips,
        "furniture": rows,
        "issues": issues.to_json(),
    })
}
