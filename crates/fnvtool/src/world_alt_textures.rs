//! `fnvtool world alt-textures` (fnv.world.alt_textures/1): every record's alternate
//! textures (MODS/MO2S/MO3S/MO4S), with the model shapes each entry goes to by the rule
//! the engine draws with (`fnv_world::alt_textures::targets`) and whether the texture
//! set's files are in the archives (docs/spec-notes/texture-swaps.md).

use crate::esm_cmd::open_load_order;
use crate::WorldArgs;
use anyhow::Context;
use fnv_esm::decode::formid_str;
use fnv_esm::LoadOrder;
use fnv_esm::Schema;
use fnv_world::alt_textures::{alt_textures, targets};
use fnv_world::fields::{winner, Fields};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// A model's shapes as (name, depth-first geometry index).
type Shapes = Vec<(Option<String>, usize)>;

/// A model's shapes; `None` when it cannot be read.
fn model_shapes(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, path: &str) -> Option<Shapes> {
    use fnv_nif::scene::{geometry_order, scene_objects, Decoded};
    let data = vfs.read(path)?.ok()?;
    let f = fnv_nif::NifFile::parse(&data, Some(nif)).ok()?;
    let dec = Decoded::new(&f, &data, nif);
    let objects = scene_objects(&f, &dec);
    let order = geometry_order(&objects);
    let mut out: Shapes = objects.iter().filter(|o| !o.is_node).filter_map(|o| Some((o.name.clone(), *order.get(&o.block)?))).collect();
    out.sort_by_key(|s| s.1);
    Some(out)
}

pub fn survey(w: &WorldArgs, nif_schema: Option<&Path>, edids: &[String]) -> anyhow::Result<Value> {
    let schema = Schema::load_dir(&w.schema).with_context(|| w.schema.display().to_string())?;
    let nif_dir = nif_schema.map(Path::to_path_buf).unwrap_or_else(|| w.schema.parent().map(|p| p.join("nif")).unwrap_or_default());
    let nif = fnv_nif::Schema::load_dir(&nif_dir).with_context(|| nif_dir.display().to_string())?;
    let plugins = open_load_order(&w.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let vfs = fnv_vfs::Vfs::open_default(&w.lo.data_dir).with_context(|| w.lo.data_dir.display().to_string())?;
    let mut fids: Vec<u32> = lo.index.keys().copied().collect();
    fids.sort_unstable();
    let mut by_sub: BTreeMap<String, u64> = BTreeMap::new();
    let mut matched: BTreeMap<&str, u64> = BTreeMap::new();
    let (mut records, mut entries, mut textures_missing, mut models_missing) = (0u64, 0u64, 0u64, 0u64);
    let mut listed = Vec::new();
    let mut shapes_cache: BTreeMap<String, Option<Shapes>> = BTreeMap::new();
    for fid in fids {
        let Some((pi, r)) = winner(&lo, fid) else { continue };
        if !r.subrecords().is_ok_and(|s| s.iter().any(|s| matches!(&s.sig, b"MODS" | b"MO2S" | b"MO3S" | b"MO4S"))) {
            continue;
        }
        let Ok(f) = Fields::new(r, pi, &schema) else { continue };
        let edid = f.string(b"EDID");
        let sig = String::from_utf8_lossy(&r.header.sig).to_string();
        records += 1;
        let mut models = Vec::new();
        for model_sub in [b"MODL", b"MOD2", b"MOD3", b"MOD4"] {
            let list = alt_textures(&lo, &schema, &f, model_sub);
            if list.is_empty() {
                continue;
            }
            let Some(model) = f.string(model_sub).as_deref().and_then(fnv_world::placed::archive_path) else { continue };
            let shapes = shapes_cache.entry(model.clone()).or_insert_with(|| model_shapes(&vfs, &nif, &model)).clone();
            models_missing += u64::from(shapes.is_none());
            let shapes = shapes.unwrap_or_default();
            let named: Vec<(Option<&str>, usize)> = shapes.iter().map(|(n, i)| (n.as_deref(), *i)).collect();
            *by_sub.entry(format!("{sig} {}", String::from_utf8_lossy(fnv_world::alt_textures::alt_sub(model_sub).unwrap()))).or_default() += list.len() as u64;
            let mut out = Vec::new();
            for e in &list {
                entries += 1;
                let t = targets(e, &named);
                let named_here = |i: usize| named[i].0.is_some_and(|n| n.eq_ignore_ascii_case(e.shape.trim()));
                let kind = if t.is_empty() {
                    "none"
                } else if !t.iter().any(|&i| e.index >= 0 && named[i].1 == e.index as usize) {
                    "name_only"
                } else if t.iter().any(|&i| named_here(i)) {
                    "name_and_index"
                } else {
                    "index_only"
                };
                *matched.entry(kind).or_default() += 1;
                let files: Vec<String> = e.textures.iter().flatten().map(|t| fnv_vfs::texture_path(t)).collect();
                let missing: Vec<&String> = files.iter().filter(|t| !vfs.exists(t)).collect();
                textures_missing += missing.len() as u64;
                out.push(json!({
                    "shape": e.shape, "index": e.index, "texture_set": formid_str(e.texture_set),
                    "diffuse": e.textures[0].as_deref().map(fnv_vfs::texture_path),
                    "files": files.len(), "missing": missing, "match": kind,
                    "targets": t.iter().map(|&i| json!([named[i].0, named[i].1])).collect::<Vec<_>>(),
                    "at_index": named.iter().find(|s| e.index >= 0 && s.1 == e.index as usize).map(|s| s.0),
                    "shapes": named.len(),
                }));
            }
            models.push(json!({"sub": String::from_utf8_lossy(model_sub), "model": model, "entries": out}));
        }
        if edids.is_empty() || edid.as_ref().is_some_and(|e| edids.iter().any(|x| x.eq_ignore_ascii_case(e))) {
            listed.push(json!({"form": formid_str(fid), "sig": sig, "edid": edid, "models": models}));
        }
    }
    Ok(json!({
        "schema": "fnv.world.alt_textures/1",
        "summary": {"records": records, "entries": entries, "by_record_and_sub": by_sub, "matched": matched,
            "models_missing": models_missing, "texture_files_missing": textures_missing},
        "records": listed,
    }))
}
