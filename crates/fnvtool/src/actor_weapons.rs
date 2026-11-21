//! `fnvtool actor weapons`: the weapon stance clips of a skeleton folder and the weapon
//! each placed NPC draws. Contract: docs/spec-notes/actors.md §9.5.

use fnv_actor::weapon::{clip_path, mount_bones, AnimType, ClipFacts, EquipClip};
use fnv_actor::{ActorForms, Resolver};
use fnv_esm::decode::formid_str;
use fnv_esm::{LoadOrder, Schema};
use fnv_nif::anim::TrackData;
use fnv_sim::{FormId, FormStore};
use fnv_world::placed::placement;
use fnv_world::WorldIndex;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn bump(m: &mut BTreeMap<String, u64>, k: impl Into<String>) {
    *m.entry(k.into()).or_default() += 1;
}

/// The first sequence of a `.kf` with its transform-track priorities.
fn clip_row(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, path: &str) -> (Value, Option<ClipFacts>) {
    let data = match vfs.read(path) {
        Some(Ok(d)) => d,
        Some(Err(e)) => return (json!({"path": path, "error": e.to_string()}), None),
        None => return (json!({"path": path, "error": "not found"}), None),
    };
    let file = match fnv_nif::file::NifFile::parse(&data, Some(nif)) {
        Ok(f) => f,
        Err(e) => return (json!({"path": path, "error": e.to_string()}), None),
    };
    let dec = fnv_nif::scene::Decoded::new(&file, &data, nif);
    let Some(s) = fnv_nif::anim::sequences(&file, &dec).into_iter().next() else {
        return (json!({"path": path, "error": "no sequence"}), None);
    };
    let facts = ClipFacts::from_parts(s.start_time, s.stop_time, &s.text_keys);
    let mut priorities: BTreeMap<String, u64> = BTreeMap::new();
    let mut weapon_track = None;
    for t in &s.tracks {
        if let (Some(n), TrackData::Transform(_)) = (&t.node_name, &t.data) {
            bump(&mut priorities, t.priority.map_or("none".to_string(), |p| p.to_string()));
            if n == "Weapon" {
                weapon_track = t.priority;
            }
        }
    }
    let v = json!({
        "path": path,
        "name": s.name,
        "start_time": s.start_time,
        "stop_time": s.stop_time,
        "cycle_type": s.cycle_type,
        "blend_time": facts.blend_time,
        "prn": facts.prn.iter().map(|(p, b)| json!([p, b])).collect::<Vec<_>>(),
        "text_keys": s.text_keys.iter().map(|(t, k)| json!([t, k.trim()])).collect::<Vec<_>>(),
        "transform_priorities": priorities,
        "weapon_track_priority": weapon_track,
    });
    (v, Some(facts))
}

pub fn report(lo: &LoadOrder, schema: &Schema, vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, folder: &str) -> Value {
    let folder = fnv_actor::anims::normalize(folder).trim_end_matches('\\').to_string();
    let mut prefixes: Vec<&str> = AnimType::ALL.iter().map(|t| t.prefix()).collect();
    prefixes.dedup();
    prefixes.sort_unstable();
    prefixes.dedup();
    let mut stances = Vec::new();
    for prefix in prefixes {
        let mut clips = serde_json::Map::new();
        let mut facts: BTreeMap<EquipClip, ClipFacts> = BTreeMap::new();
        for c in EquipClip::ALL {
            let (row, f) = clip_row(vfs, nif, &clip_path(&folder, prefix, c));
            clips.insert(c.stem().to_string(), row);
            if let Some(f) = f {
                facts.insert(c, f);
            }
        }
        let (holster, drawn) = mount_bones(&facts, "Weapon's skeleton parent");
        let types: Vec<&str> = AnimType::ALL.iter().filter(|t| t.prefix() == prefix).map(|t| t.name()).collect();
        stances.push(json!({"prefix": prefix, "anim_types": types, "holster_bone": holster, "drawn_bone": drawn, "clips": clips}));
    }

    // The weapon of every placed NPC.
    let ix = WorldIndex::build(lo, schema);
    let actors = ActorForms::build(lo, schema);
    let forms = FormStore::build(lo, schema);
    let resolver = Resolver { actors: &actors, forms: &forms };
    let mut refs: Vec<u32> = ix.refs.iter().filter(|(_, e)| &e.sig == b"ACHR").map(|(&r, _)| r).collect();
    refs.sort_unstable();
    let mut by_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_weapon: BTreeMap<String, u64> = BTreeMap::new();
    let mut outcomes: BTreeMap<String, u64> = BTreeMap::new();
    let mut missing_models: BTreeMap<String, u64> = BTreeMap::new();
    let mut actors_out = Vec::new();
    for r in refs {
        let Some(p) = placement(lo, schema, r).filter(|p| !p.deleted()) else { continue };
        let Some(base) = p.base else { continue };
        let a = match resolver.resolve(FormId(base), FormId(r)) {
            Ok(a) => a,
            Err(_) => {
                bump(&mut outcomes, "unresolved");
                continue;
            }
        };
        let Some(wid) = a.weapon else {
            bump(&mut outcomes, "no weapon");
            continue;
        };
        bump(&mut outcomes, if a.random { "weapon (random)" } else { "weapon (deterministic)" });
        let w = &actors.weapons[&wid];
        let t = w.anim_type.clone().unwrap_or_default();
        bump(&mut by_type, t.clone());
        bump(&mut by_weapon, w.edid.clone().unwrap_or_else(|| formid_str(wid.0)));
        let model = w.model.as_deref().map(fnv_actor::resolve::mesh_path).unwrap_or_default();
        if !vfs.exists(&model) {
            bump(&mut missing_models, model.clone());
        }
        actors_out.push(json!({
            "reference": formid_str(r),
            "edid": a.edid,
            "weapon": formid_str(wid.0),
            "weapon_edid": w.edid,
            "anim_type": t,
            "prefix": AnimType::from_name(&t).map(AnimType::prefix),
            "model": model,
            "random": a.random,
        }));
    }
    let mut top: Vec<(String, u64)> = by_weapon.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top.truncate(25);
    json!({
        "schema": "fnv.actor.weapons/1",
        "folder": folder,
        "stances": stances,
        "summary": {
            "weap_records": actors.weapons.len(),
            "drawable": actors.weapons.values().filter(|w| fnv_actor::weapon::drawable(w)).count(),
            "actors": outcomes,
            "by_anim_type": by_type,
            "top_weapons": top,
            "missing_models": missing_models,
        },
        "actors": actors_out,
        "archives": vfs.archives().collect::<Vec<_>>(),
    })
}
