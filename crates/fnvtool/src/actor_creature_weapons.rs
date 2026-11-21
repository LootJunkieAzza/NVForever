//! `fnvtool actor creature-weapons`: for every creature base that a placed creature
//! (ACRE) stands for, every weapon it can carry and whether the engine can draw it in its
//! hands: the weapon has a model, is not part of the body ("Embedded Weapon"), and the
//! creature's skeleton folder has that weapon's stance clips and a `Weapon` node.
//! Contract: docs/spec-notes/actors.md §10.6.

use fnv_actor::weapon::{clip_path, AnimType, EquipClip};
use fnv_actor::ActorForms;
use fnv_esm::decode::formid_str;
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::forms::BaseData;
use fnv_sim::{FormId, FormStore};
use fnv_world::placed::placement;
use fnv_world::WorldIndex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_DEPTH: u32 = 16;

/// The ends of the template chain of creature `id` for template group `flag`, through
/// every entry of every leveled creature list on the way.
fn sources(actors: &ActorForms, id: FormId, flag: u16, depth: u32, out: &mut BTreeSet<FormId>) {
    if depth > MAX_DEPTH {
        return;
    }
    if let Some(c) = actors.creatures.get(&id) {
        match c.template.filter(|_| c.template_flags & flag != 0) {
            Some(t) => sources(actors, t, flag, depth + 1, out),
            None => {
                out.insert(id);
            }
        }
    } else if let Some(l) = actors.leveled_creatures.get(&id) {
        for &(_, e) in &l.entries {
            sources(actors, e, flag, depth + 1, out);
        }
    }
}

/// Every item a carried entry `item` can give, leveled item lists followed through all
/// their entries.
fn items(forms: &FormStore, item: FormId, depth: u32, out: &mut BTreeSet<FormId>) {
    if depth > MAX_DEPTH {
        return;
    }
    match forms.leveled(item) {
        Some(l) => {
            for e in &l.entries {
                items(forms, e.item, depth + 1, out);
            }
        }
        None => {
            out.insert(item);
        }
    }
}

/// The folder of a skeleton path (`meshes\creatures\ghoul\skeleton.nif` → `meshes\creatures\ghoul`).
fn folder_of(skeleton: &str) -> String {
    let s = fnv_actor::anims::normalize(skeleton);
    s.rfind('\\').map_or(String::new(), |i| s[..i].to_string())
}

/// Whether a skeleton has a node named `Weapon` (`None`: the file could not be read).
fn has_weapon_node(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, path: &str) -> Option<bool> {
    let data = vfs.read(path)?.ok()?;
    let file = fnv_nif::file::NifFile::parse(&data, Some(nif)).ok()?;
    let dec = fnv_nif::scene::Decoded::new(&file, &data, nif);
    Some(fnv_actor::mesh::analyse(&file, &dec).node_names.contains("Weapon"))
}

fn bump(m: &mut BTreeMap<String, u64>, k: impl Into<String>) {
    *m.entry(k.into()).or_default() += 1;
}

pub fn report(lo: &LoadOrder, schema: &Schema, vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema) -> Value {
    let ix = WorldIndex::build(lo, schema);
    let actors = ActorForms::build(lo, schema);
    let forms = FormStore::build(lo, schema);

    // Placed creatures by base.
    let mut placed: BTreeMap<FormId, u64> = BTreeMap::new();
    for (&r, e) in &ix.refs {
        if &e.sig != b"ACRE" {
            continue;
        }
        let Some(p) = placement(lo, schema, r).filter(|p| !p.deleted()) else { continue };
        if let Some(b) = p.base {
            *placed.entry(FormId(b)).or_default() += 1;
        }
    }

    // Stance and Weapon-node checks, once per (folder, prefix) and per skeleton.
    let mut stance_cache: BTreeMap<(String, String), Value> = BTreeMap::new();
    let mut node_cache: BTreeMap<String, Option<bool>> = BTreeMap::new();
    let mut stance = |folder: &str, prefix: &str| -> Value {
        stance_cache
            .entry((folder.to_string(), prefix.to_string()))
            .or_insert_with(|| {
                let found: Vec<&str> =
                    EquipClip::ALL.iter().filter(|&&c| vfs.exists(&clip_path(folder, prefix, c))).map(|c| c.stem()).collect();
                json!(found)
            })
            .clone()
    };

    let mut rows = Vec::new();
    let mut by_status: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_status_placed: BTreeMap<String, u64> = BTreeMap::new();
    // (kind, folder) → status → placed references.
    let mut by_group: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut not_drawn: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (&base, &count) in &placed {
        let mut models = BTreeSet::new();
        sources(&actors, base, fnv_actor::records::template::MODEL_ANIMATION, 0, &mut models);
        let mut invs = BTreeSet::new();
        sources(&actors, base, fnv_actor::records::template::INVENTORY, 0, &mut invs);
        let skeletons: BTreeSet<(String, String)> = models
            .iter()
            .filter_map(|m| actors.creatures.get(m))
            .filter_map(|c| {
                let s = c.skeleton.as_deref().filter(|s| !fnv_actor::resolve::is_editor_marker(s))?;
                Some((fnv_actor::resolve::mesh_path(s), c.kind.clone().unwrap_or_default()))
            })
            .collect();
        let mut carried = BTreeSet::new();
        for inv in &invs {
            for e in forms.entries(*inv).unwrap_or_default() {
                if e.count > 0 {
                    items(&forms, e.item, 0, &mut carried);
                }
            }
        }
        let weapons: Vec<FormId> = carried.into_iter().filter(|i| actors.weapons.contains_key(i)).collect();
        let edid = actors.creatures.get(&base).and_then(|c| c.edid.clone());
        let mut skel_rows = Vec::new();
        for (skeleton, kind) in &skeletons {
            let folder = folder_of(skeleton);
            let node = *node_cache.entry(skeleton.clone()).or_insert_with(|| has_weapon_node(vfs, nif, skeleton));
            let mut weapon_rows = Vec::new();
            for &wid in &weapons {
                let w = &actors.weapons[&wid];
                let t = w.anim_type.clone().unwrap_or_default();
                let prefix = AnimType::from_name(&t).map(AnimType::prefix);
                let model = w.model.as_deref().map(fnv_actor::resolve::mesh_path);
                let clips = prefix.map_or(json!([]), |p| stance(&folder, p));
                let has_stance = clips.as_array().is_some_and(|a| a.iter().any(|c| c == "holster" || c == "aim"));
                let status = if w.embedded {
                    "embedded (part of the body)"
                } else if w.not_for_npcs {
                    "not for NPCs"
                } else if model.is_none() {
                    "no model"
                } else if prefix.is_none() {
                    "no animation type"
                } else if !has_stance {
                    "carried only: no stance in the skeleton folder"
                } else if node != Some(true) {
                    "carried only: the skeleton has no Weapon node"
                } else if model.as_deref().is_some_and(|m| !vfs.exists(m)) {
                    "model file missing"
                } else {
                    "drawable"
                };
                bump(&mut by_status, status);
                *by_status_placed.entry(status.to_string()).or_default() += count;
                *by_group.entry(format!("{kind} | {folder}")).or_default().entry(status.to_string()).or_default() += count;
                if status != "drawable" && !status.starts_with("embedded") {
                    not_drawn.entry(format!("{kind} | {folder} | {status}")).or_default().insert(format!("{} ({})", w.edid.clone().unwrap_or_default(), prefix.unwrap_or("?")));
                }
                weapon_rows.push(json!({
                    "weapon": formid_str(wid.0),
                    "edid": w.edid,
                    "anim_type": t,
                    "prefix": prefix,
                    "model": model,
                    "embedded": w.embedded,
                    "not_for_npcs": w.not_for_npcs,
                    "damage": w.damage,
                    "stance_clips": clips,
                    "status": status,
                }));
            }
            if weapons.is_empty() {
                *by_group.entry(format!("{kind} | {folder}")).or_default().entry("no weapon".into()).or_default() += count;
            }
            skel_rows.push(json!({"skeleton": skeleton, "kind": kind, "weapon_node": node, "weapons": weapon_rows}));
        }
        let non_weapons = invs.iter().any(|i| matches!(forms.get(*i).map(|b| &b.data), Some(BaseData::Actor(_))));
        rows.push(json!({
            "base": formid_str(base.0),
            "edid": edid,
            "placed": count,
            "inventory_sources": invs.iter().map(|i| formid_str(i.0)).collect::<Vec<_>>(),
            "inventory_source_is_actor": non_weapons,
            "skeletons": skel_rows,
        }));
    }
    json!({
        "schema": "fnv.actor.creature_weapons/1",
        "summary": {
            "bases": rows.len(),
            "placed": placed.values().sum::<u64>(),
            "weapon_rows_by_status": by_status,
            "placed_by_status": by_status_placed,
            "by_kind_and_folder": by_group,
            "not_drawn": not_drawn.into_iter().map(|(k, v)| json!({"group": k, "weapons": v})).collect::<Vec<_>>(),
        },
        "creatures": rows,
    })
}
