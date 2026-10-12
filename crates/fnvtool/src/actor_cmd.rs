//! `fnvtool actor …`: actor appearance (workstream 5, crate `fnv-actor`). Contract:
//! docs/spec-notes/actors.md §5; `actor anims` (module `actor_anims`): docs/spec-notes/animation.md §6; `actor locomotion`
//! (module `actor_locomotion`): docs/spec-notes/actors.md §8.5; `actor weapons` (module
//! `actor_weapons`): §9.5.

use crate::esm_cmd::open_load_order;
use crate::ActorCmd;
use anyhow::{anyhow, Context};
use fnv_actor::{mesh, mesh::PartMesh, ActorForms, Resolver};
use fnv_esm::decode::formid_str;
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::{FormId, FormStore};
use fnv_world::placed::placement;
use fnv_world::WorldIndex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn run(cmd: ActorCmd) -> anyhow::Result<Value> {
    match cmd {
        ActorCmd::Values { w, functions, actor, survey } => {
            crate::ai_cmd::with_data(&w, functions, |lo, schema, data, refs, _| crate::actor_values::run(lo, schema, data, refs, &actor, survey))
        }
        ActorCmd::Aid { w, functions, item, uses, damage, seconds, hardcore } => {
            let o = crate::actor_aid::Options { items: item, uses, damage, seconds, hardcore };
            crate::ai_cmd::with_data(&w, functions, |lo, schema, data, refs, names| {
                let forms = FormStore::build(lo, schema);
                crate::actor_aid::run(data, &forms, refs, names, &o)
            })
        }
        ActorCmd::Chargen { w, functions, special, tag } => crate::ai_cmd::with_data(&w, functions, |lo, schema, data, refs, _| {
            crate::actor_chargen::run(lo, schema, data, refs, special.as_deref(), &tag)
        }),
        ActorCmd::Factions { w, functions, actor, survey } => {
            crate::ai_cmd::with_data(&w, functions, |lo, schema, data, refs, _| crate::actor_factions::run(lo, schema, data, refs, &actor, survey))
        }
        ActorCmd::Karma { w, functions, actor, cell, survey } => {
            crate::ai_cmd::with_data(&w, functions, |lo, schema, data, refs, _| crate::actor_karma::run(lo, schema, data, refs, &actor, &cell, survey))
        }
        ActorCmd::Detect { w, functions, detector, target, interior, light, dark } => {
            let o = crate::actor_detect::Options { detector, target, interior, light, dark };
            crate::ai_cmd::with_data(&w, functions, |lo, schema, data, refs, _| crate::actor_detect::run(lo, schema, data, refs, &o))
        }
        ActorCmd::Appearance { lo, schema, reference, cell, all, deterministic } => {
            let schema = Schema::load_dir(&schema).with_context(|| schema.display().to_string())?;
            let plugins = open_load_order(&lo)?;
            let lo = LoadOrder::new(&plugins)?;
            let ix = WorldIndex::build(&lo, &schema);
            let actors = ActorForms::build(&lo, &schema);
            let forms = FormStore::build(&lo, &schema);
            let refs: Vec<u32> = match (reference, cell) {
                (Some(r), _) => vec![r],
                (None, Some(c)) => {
                    let cell = ix.cell_by_name(&c).ok_or_else(|| anyhow!("no cell {c}"))?;
                    cell.refs.iter().copied().filter(|r| ix.refs.get(r).is_some_and(|e| &e.sig == b"ACHR")).collect()
                }
                _ if all => ix.refs.iter().filter(|(_, e)| &e.sig == b"ACHR").map(|(&r, _)| r).collect(),
                _ => unreachable!("clap requires --ref, --cell or --all"),
            };
            let mut v = appearance(&lo, &schema, &actors, &forms, refs, deterministic);
            v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
            Ok(v)
        }
        ActorCmd::Creatures { lo: lo_args, schema: schema_dir, reference, cell, all, deterministic, check_meshes, nif_schema } => {
            let schema = Schema::load_dir(&schema_dir).with_context(|| schema_dir.display().to_string())?;
            let plugins = open_load_order(&lo_args)?;
            let lo = LoadOrder::new(&plugins)?;
            let ix = WorldIndex::build(&lo, &schema);
            let actors = ActorForms::build(&lo, &schema);
            let forms = FormStore::build(&lo, &schema);
            let refs: Vec<u32> = match (reference, cell) {
                (Some(r), _) => vec![r],
                (None, Some(c)) => {
                    let cell = ix.cell_by_name(&c).ok_or_else(|| anyhow!("no cell {c}"))?;
                    cell.refs.iter().copied().filter(|r| ix.refs.get(r).is_some_and(|e| &e.sig == b"ACRE")).collect()
                }
                _ if all => ix.refs.iter().filter(|(_, e)| &e.sig == b"ACRE").map(|(&r, _)| r).collect(),
                _ => unreachable!("clap requires --ref, --cell or --all"),
            };
            let check = if check_meshes {
                let nif_dir = nif_schema.unwrap_or_else(|| schema_dir.parent().map(|p| p.join("nif")).unwrap_or_default());
                let nif = fnv_nif::Schema::load_dir(&nif_dir).with_context(|| nif_dir.display().to_string())?;
                let vfs = fnv_vfs::Vfs::open_default(&lo_args.data_dir).with_context(|| lo_args.data_dir.display().to_string())?;
                Some((vfs, nif))
            } else {
                None
            };
            let mut v = creatures(&lo, &schema, &actors, &forms, refs, deterministic, check.as_ref().map(|(v, n)| (v, n)));
            v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
            Ok(v)
        }
        ActorCmd::Forms { lo, schema } => {
            let schema = Schema::load_dir(&schema).with_context(|| schema.display().to_string())?;
            let plugins = open_load_order(&lo)?;
            let lo = LoadOrder::new(&plugins)?;
            Ok(forms_report(&ActorForms::build(&lo, &schema)))
        }
        ActorCmd::Meshes { lo: lo_args, schema: schema_dir, nif_schema } => {
            let schema = Schema::load_dir(&schema_dir).with_context(|| schema_dir.display().to_string())?;
            let nif_dir = nif_schema.unwrap_or_else(|| schema_dir.parent().map(|p| p.join("nif")).unwrap_or_default());
            let nif = fnv_nif::Schema::load_dir(&nif_dir).with_context(|| nif_dir.display().to_string())?;
            let vfs = fnv_vfs::Vfs::open_default(&lo_args.data_dir).with_context(|| lo_args.data_dir.display().to_string())?;
            let plugins = open_load_order(&lo_args)?;
            let lo = LoadOrder::new(&plugins)?;
            let ix = WorldIndex::build(&lo, &schema);
            let actors = ActorForms::build(&lo, &schema);
            let forms = FormStore::build(&lo, &schema);
            let refs: Vec<u32> = ix.refs.iter().filter(|(_, e)| &e.sig == b"ACHR").map(|(&r, _)| r).collect();
            meshes(&lo, &schema, &actors, &forms, &vfs, &nif, refs)
        }
        ActorCmd::Anims { lo: lo_args, schema: schema_dir, nif_schema, functions, summary_only } => {
            let schema = Schema::load_dir(&schema_dir).with_context(|| schema_dir.display().to_string())?;
            let parent = schema_dir.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
            let nif_dir = nif_schema.unwrap_or_else(|| parent.join("nif"));
            let nif = fnv_nif::Schema::load_dir(&nif_dir).with_context(|| nif_dir.display().to_string())?;
            let fn_path = functions.unwrap_or_else(|| parent.join("script").join("functions.json"));
            let fn_names = crate::actor_anims::condition_function_names(&fn_path)?;
            let vfs = fnv_vfs::Vfs::open_default(&lo_args.data_dir).with_context(|| lo_args.data_dir.display().to_string())?;
            let plugins = open_load_order(&lo_args)?;
            let lo = LoadOrder::new(&plugins)?;
            let mut v = crate::actor_anims::report(&lo, &schema, &vfs, &nif, &fn_names, summary_only);
            v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
            v["archives"] = json!(vfs.archives().collect::<Vec<_>>());
            v["condition_function_names"] = json!(fn_names.len());
            Ok(v)
        }
        ActorCmd::Weapons { lo: lo_args, schema: schema_dir, nif_schema, folder } => {
            let schema = Schema::load_dir(&schema_dir).with_context(|| schema_dir.display().to_string())?;
            let nif_dir = nif_schema.unwrap_or_else(|| schema_dir.parent().map(|p| p.join("nif")).unwrap_or_default());
            let nif = fnv_nif::Schema::load_dir(&nif_dir).with_context(|| nif_dir.display().to_string())?;
            let vfs = fnv_vfs::Vfs::open_default(&lo_args.data_dir).with_context(|| lo_args.data_dir.display().to_string())?;
            let plugins = open_load_order(&lo_args)?;
            let lo = LoadOrder::new(&plugins)?;
            let mut v = crate::actor_weapons::report(&lo, &schema, &vfs, &nif, &folder);
            v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
            Ok(v)
        }
        ActorCmd::Furniture { lo: lo_args, schema: schema_dir, nif_schema, summary_only } => {
            let schema = Schema::load_dir(&schema_dir).with_context(|| schema_dir.display().to_string())?;
            let nif_dir = nif_schema.unwrap_or_else(|| schema_dir.parent().map(|p| p.join("nif")).unwrap_or_default());
            let nif = fnv_nif::Schema::load_dir(&nif_dir).with_context(|| nif_dir.display().to_string())?;
            let vfs = fnv_vfs::Vfs::open_default(&lo_args.data_dir).with_context(|| lo_args.data_dir.display().to_string())?;
            let plugins = open_load_order(&lo_args)?;
            let lo = LoadOrder::new(&plugins)?;
            let mut v = crate::actor_furniture::report(&lo, &schema, &vfs, &nif, summary_only);
            v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
            Ok(v)
        }
        ActorCmd::Locomotion { data_dir, nif_schema, folder, samples } => {
            let nif = fnv_nif::Schema::load_dir(&nif_schema).with_context(|| nif_schema.display().to_string())?;
            let vfs = fnv_vfs::Vfs::open_default(&data_dir).with_context(|| data_dir.display().to_string())?;
            Ok(crate::actor_locomotion::report(&vfs, &nif, &folder, samples))
        }
        ActorCmd::Tri { data_dir, nif_schema, path } => {
            let nif = fnv_nif::Schema::load_dir(&nif_schema).with_context(|| nif_schema.display().to_string())?;
            let vfs = fnv_vfs::Vfs::open_default(&data_dir).with_context(|| data_dir.display().to_string())?;
            Ok(crate::actor_tri::report(&vfs, &nif, path.as_deref()))
        }
    }
}

/// `fnvtool actor creatures` (fnv.actor.creatures/1, docs/spec-notes/actors.md §10.4).
#[allow(clippy::too_many_arguments)]
fn creatures(
    lo: &LoadOrder,
    schema: &Schema,
    actors: &ActorForms,
    forms: &FormStore,
    mut refs: Vec<u32>,
    deterministic: bool,
    check: Option<(&fnv_vfs::Vfs, &fnv_nif::Schema)>,
) -> Value {
    refs.sort_unstable();
    let resolver = Resolver { actors, forms };
    let mut out = Vec::new();
    let mut outcomes: BTreeMap<String, u64> = BTreeMap::new();
    let mut base_kinds: BTreeMap<String, u64> = BTreeMap::new();
    // (skeleton, model) -> creatures using it (for --check-meshes)
    let mut pairs: BTreeMap<(String, String), u64> = BTreeMap::new();
    for r in refs {
        let Some(p) = placement(lo, schema, r) else {
            bump(&mut outcomes, "no placement");
            continue;
        };
        if p.deleted() {
            bump(&mut outcomes, "deleted");
            continue;
        }
        let Some(base) = p.base else {
            bump(&mut outcomes, "no base");
            out.push(json!({"reference": formid_str(r), "error": "no base"}));
            continue;
        };
        let kind = actors.sigs.get(&FormId(base)).map_or("missing".to_string(), |s| String::from_utf8_lossy(s).into_owned());
        bump(&mut base_kinds, kind);
        match resolver.resolve_creature(FormId(base), FormId(r)) {
            Ok(a) => {
                bump(&mut outcomes, if a.random { "resolved random" } else { "resolved deterministic" });
                for part in &a.parts {
                    *pairs.entry((a.skeleton.clone(), part.model.clone())).or_default() += 1;
                }
                if deterministic && a.random {
                    continue;
                }
                let mut v = serde_json::to_value(&a).expect("creature serialises");
                v["reference"] = json!(formid_str(r));
                v["base"] = json!(formid_str(base));
                v["scale"] = json!(f64::from(p.scale));
                out.push(v);
            }
            Err(e) => {
                bump(&mut outcomes, format!("{e:?}").split('(').next().unwrap_or("error").to_string());
                out.push(json!({"reference": formid_str(r), "base": formid_str(base), "error": e.to_string()}));
            }
        }
    }
    let mut v = json!({
        "schema": "fnv.actor.creatures/1",
        "summary": {"outcomes": outcomes, "base_kinds": base_kinds},
        "issues": actors.issues.to_json(),
        "creatures": out,
    });
    if let Some((vfs, nif)) = check {
        v["meshes"] = creature_meshes(vfs, nif, &pairs);
    }
    v
}

/// Every (skeleton, model) pair placed creatures use: how the model joins the skeleton.
fn creature_meshes(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, pairs: &BTreeMap<(String, String), u64>) -> Value {
    let mut cache: BTreeMap<String, Result<PartMesh, String>> = BTreeMap::new();
    let mut load = |path: &str| -> Result<PartMesh, String> {
        cache
            .entry(path.to_string())
            .or_insert_with(|| {
                let data = match vfs.read(path) {
                    None => return Err("not found".to_string()),
                    Some(Err(e)) => return Err(e.to_string()),
                    Some(Ok(d)) => d,
                };
                let file = fnv_nif::file::NifFile::parse(&data, Some(nif)).map_err(|e| e.to_string())?;
                let dec = fnv_nif::scene::Decoded::new(&file, &data, nif);
                Ok(mesh::analyse(&file, &dec))
            })
            .clone()
    };
    let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut problems = Vec::new();
    let mut idles: BTreeMap<String, u64> = BTreeMap::new();
    let mut skeletons: BTreeMap<&str, u64> = BTreeMap::new();
    let mut rigid_at_root = Vec::new();
    for ((skel, model), uses) in pairs {
        *skeletons.entry(skel.as_str()).or_default() += uses;
        let skeleton = match load(skel) {
            Ok(s) => s,
            Err(e) => {
                bump(&mut kinds, "skeleton unreadable");
                problems.push(json!({"skeleton": skel, "model": model, "creatures": uses, "problems": [format!("skeleton: {e}")]}));
                continue;
            }
        };
        let part = match load(model) {
            Ok(p) => p,
            Err(e) => {
                bump(&mut kinds, "unreadable");
                problems.push(json!({"skeleton": skel, "model": model, "creatures": uses, "problems": [e]}));
                continue;
            }
        };
        let kind = match (part.skinned_shapes > 0, part.rigid_shapes > 0) {
            (true, false) => "skinned",
            (false, true) => "rigid",
            (true, true) => "mixed",
            (false, false) => "no shapes",
        };
        bump(&mut kinds, kind);
        let at_root = mesh::creature_rigid_at_root(&part, &skeleton);
        if at_root > 0 {
            rigid_at_root.push(json!({"skeleton": skel, "model": model, "shapes": at_root, "of": part.rigid_shapes, "creatures": uses}));
        }
        let p = mesh::creature_attach_problems(&part, &skeleton);
        if !p.is_empty() {
            problems.push(json!({"skeleton": skel, "model": model, "creatures": uses, "problems": p}));
        }
    }
    // The idle each skeleton would play (actors.md §10.3).
    for skel in skeletons.keys() {
        let folder = skel.rsplit_once('\\').map_or("", |(f, _)| f);
        let found = fnv_actor::locomotion::clip_candidates(folder, fnv_actor::locomotion::Body::Male, "mt", fnv_actor::locomotion::Move::Idle)
            .into_iter()
            .find(|p| vfs.exists(p));
        bump(&mut idles, found.map_or("missing".to_string(), |p| p.strip_prefix(folder).unwrap_or(&p).to_string()));
    }
    json!({
        "pairs": pairs.len(),
        "skeletons": skeletons.len(),
        "part_kinds": kinds,
        "idle_clips": idles,
        "rigid_at_root": rigid_at_root,
        "problems": problems,
    })
}

/// Every part file used by placed NPCs, attached to the skeleton it is used with.
fn meshes(
    lo: &LoadOrder,
    schema: &Schema,
    actors: &ActorForms,
    forms: &FormStore,
    vfs: &fnv_vfs::Vfs,
    nif: &fnv_nif::Schema,
    mut refs: Vec<u32>,
) -> anyhow::Result<Value> {
    refs.sort_unstable();
    let resolver = Resolver { actors, forms };
    // (skeleton, part model) -> (role, actors using it)
    let mut pairs: BTreeMap<(String, String), (String, u64)> = BTreeMap::new();
    let mut textures: BTreeMap<String, u64> = BTreeMap::new();
    for r in refs {
        let Some(base) = placement(lo, schema, r).filter(|p| !p.deleted()).and_then(|p| p.base) else { continue };
        let Ok(a) = resolver.resolve(FormId(base), FormId(r)) else { continue };
        for p in &a.parts {
            let e = pairs.entry((a.skeleton.clone(), p.model.clone())).or_insert_with(|| (p.role.clone(), 0));
            e.1 += 1;
            if let Some(t) = &p.texture {
                *textures.entry(t.clone()).or_default() += 1;
            }
        }
    }
    let mut cache: BTreeMap<String, Result<PartMesh, String>> = BTreeMap::new();
    let mut load = |path: &str| -> Result<PartMesh, String> {
        cache
            .entry(path.to_string())
            .or_insert_with(|| {
                let data = match vfs.read(path) {
                    None => return Err("not found".to_string()),
                    Some(Err(e)) => return Err(e.to_string()),
                    Some(Ok(d)) => d,
                };
                let file = fnv_nif::file::NifFile::parse(&data, Some(nif)).map_err(|e| e.to_string())?;
                let dec = fnv_nif::scene::Decoded::new(&file, &data, nif);
                Ok(mesh::analyse(&file, &dec))
            })
            .clone()
    };
    let mut problems = Vec::new();
    let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut variants: BTreeMap<String, u64> = BTreeMap::new();
    for ((skel, model), (role, uses)) in &pairs {
        let skeleton = match load(skel) {
            Ok(s) => s,
            Err(e) => {
                problems.push(json!({"skeleton": skel, "model": model, "role": role, "actors": uses, "problems": [format!("skeleton: {e}")]}));
                continue;
            }
        };
        let part = match load(model) {
            Ok(p) => p,
            Err(e) => {
                bump(&mut kinds, "unreadable");
                problems.push(json!({"skeleton": skel, "model": model, "role": role, "actors": uses, "problems": [e]}));
                continue;
            }
        };
        let kind = match (part.skinned_shapes > 0, part.rigid_shapes > 0) {
            (true, false) => "skinned",
            (false, true) => "rigid",
            (true, true) => "mixed",
            (false, false) => "no shapes",
        };
        bump(&mut kinds, format!("{role} {kind}{}", if part.prn.is_some() { " +Prn" } else { "" }));
        if role == "hair" {
            let has = |n: &str| part.shape_names.iter().any(|s| s.eq_ignore_ascii_case(n));
            bump(&mut variants, format!("NoHat {} / Hat {}", has("NoHat"), has("Hat")));
        }
        let p = mesh::attach_problems(&part, &skeleton);
        if !p.is_empty() {
            problems.push(json!({"skeleton": skel, "model": model, "role": role, "actors": uses, "problems": p}));
        }
    }
    // Every ARMO's worn models (placed or not): the bone their unskinned shapes name,
    // the Pip-Boy sleeve shapes of outfits, and the Pip-Boy light's glow (actors.md §1.2).
    let worn: BTreeSet<String> = actors.armors.values().flat_map(|a| a.worn.iter().flatten()).map(|m| fnv_actor::resolve::mesh_path(m)).collect();
    let mut rigid_bones: BTreeMap<String, u64> = BTreeMap::new();
    let mut off_head = Vec::new();
    let mut sleeves: BTreeMap<String, u64> = BTreeMap::new();
    let mut light_effect = Vec::new();
    for model in &worn {
        let Ok(part) = load(model) else { continue };
        let sleeve = |on: bool| part.shape_names.iter().any(|n| mesh::pipboy_sleeve(n) == Some(on));
        match (sleeve(true), sleeve(false)) {
            (true, true) => bump(&mut sleeves, "on and off"),
            (true, false) => bump(&mut sleeves, "on only"),
            (false, true) => bump(&mut sleeves, "off only"),
            (false, false) => {}
        }
        if part.shape_names.iter().any(|n| mesh::is_light_effect_shape(n)) {
            light_effect.push(model);
        }
        if part.rigid_shapes == 0 {
            continue;
        }
        bump(&mut rigid_bones, part.prn.clone().unwrap_or_else(|| "(no Prn)".to_string()));
        if part.prn.as_deref().is_some_and(|p| p != mesh::DEFAULT_RIGID_BONE) {
            off_head.push(json!({"model": model, "prn": part.prn, "skinned_shapes": part.skinned_shapes, "rigid_shapes": part.rigid_shapes}));
        }
    }
    let missing_textures: Vec<&String> = textures.keys().filter(|t| !vfs.exists(t)).collect();
    Ok(json!({
        "schema": "fnv.actor.meshes/1",
        "pairs": pairs.len(),
        "part_kinds": kinds,
        "armor_models": worn.len(),
        "armor_rigid_bones": rigid_bones,
        "armor_rigid_off_head": off_head,
        "armor_pipboy_sleeves": sleeves,
        "armor_light_effect": light_effect,
        "hair_variants": variants,
        "textures": textures.len(),
        "missing_textures": missing_textures,
        "problems": problems,
    }))
}

/// Observational statistics over every NPC_ record (not only placed ones).
fn forms_report(actors: &ActorForms) -> Value {
    use fnv_actor::records::template;
    let mut by_template: BTreeMap<String, u64> = BTreeMap::new();
    let mut skeletons: BTreeMap<String, u64> = BTreeMap::new();
    let mut own_model_when_traits_only: BTreeMap<String, u64> = BTreeMap::new();
    let mut template_targets: BTreeMap<String, u64> = BTreeMap::new();
    let mut facegen_lengths: BTreeMap<String, u64> = BTreeMap::new();
    for n in actors.npcs.values() {
        let t = n.template.is_some();
        let traits = t && n.template_flags & template::TRAITS != 0;
        let model = t && n.template_flags & template::MODEL_ANIMATION != 0;
        let inv = t && n.template_flags & template::INVENTORY != 0;
        bump(&mut by_template, format!("tplt={t} traits={traits} model={model} inventory={inv}"));
        if let Some(tp) = n.template {
            let k = actors.sigs.get(&tp).map_or("missing".into(), |s| String::from_utf8_lossy(s).into_owned());
            bump(&mut template_targets, k);
        }
        bump(&mut skeletons, n.skeleton.as_deref().map_or("none".into(), |s| s.to_ascii_lowercase()));
        if traits && !model {
            let face = n.facegen.as_ref().is_some_and(|f| f.fggs.iter().any(|v| *v != 0.0));
            bump(
                &mut own_model_when_traits_only,
                format!(
                    "skeleton={} hair={} facegen_nonzero={face}",
                    n.skeleton.as_deref().map_or("none".into(), |s| s.to_ascii_lowercase()),
                    n.hair.is_some()
                ),
            );
        }
        if let Some(f) = &n.facegen {
            bump(&mut facegen_lengths, format!("{}/{}/{}", f.fggs.len(), f.fgga.len(), f.fgts.len()));
        }
    }
    let races: BTreeMap<String, Value> = actors
        .races
        .values()
        .map(|r| {
            let parts = |l: &Vec<fnv_actor::records::RacePart>| {
                l.iter().map(|p| json!({"index": p.index, "model": p.model, "icon": p.icon})).collect::<Vec<_>>()
            };
            (
                format!("{} {}", r.id, r.edid.clone().unwrap_or_default()),
                json!({"height": r.height, "head_male": parts(&r.head[0]).len(), "head_female": parts(&r.head[1]).len(),
                       "body_male": parts(&r.body[0]), "facegen": [r.facegen[0].is_some(), r.facegen[1].is_some()]}),
            )
        })
        .collect();
    json!({
        "schema": "fnv.actor.forms/1",
        "counts": {"npcs": actors.npcs.len(), "races": actors.races.len(), "hairs": actors.hairs.len(), "eyes": actors.eyes.len(),
                   "head_parts": actors.head_parts.len(), "armors": actors.armors.len(), "leveled_npcs": actors.leveled.len()},
        "templates": by_template,
        "template_targets": template_targets,
        "skeletons": skeletons,
        "traits_templated_own_model": own_model_when_traits_only,
        "facegen_lengths": facegen_lengths,
        "races": races,
        "issues": actors.issues.to_json(),
    })
}

fn bump(m: &mut BTreeMap<String, u64>, k: impl Into<String>) {
    *m.entry(k.into()).or_default() += 1;
}

fn appearance(lo: &LoadOrder, schema: &Schema, actors: &ActorForms, forms: &FormStore, mut refs: Vec<u32>, deterministic: bool) -> Value {
    refs.sort_unstable();
    let resolver = Resolver { actors, forms };
    let mut out = Vec::new();
    let mut outcomes: BTreeMap<String, u64> = BTreeMap::new();
    let mut base_kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut worn_counts: BTreeMap<String, u64> = BTreeMap::new();
    for r in refs {
        let Some(p) = placement(lo, schema, r) else {
            bump(&mut outcomes, "no placement");
            continue;
        };
        if p.deleted() {
            bump(&mut outcomes, "deleted");
            continue;
        }
        let Some(base) = p.base else {
            bump(&mut outcomes, "no base");
            out.push(json!({"reference": formid_str(r), "error": "no base"}));
            continue;
        };
        let kind = actors.sigs.get(&FormId(base)).map_or("missing".to_string(), |s| String::from_utf8_lossy(s).into_owned());
        bump(&mut base_kinds, kind);
        match resolver.resolve(FormId(base), FormId(r)) {
            Ok(a) => {
                bump(&mut outcomes, if a.random { "resolved random" } else { "resolved deterministic" });
                if deterministic && a.random {
                    continue;
                }
                bump(&mut worn_counts, a.worn.len().to_string());
                let mut v = serde_json::to_value(&a).expect("appearance serialises");
                v["reference"] = json!(formid_str(r));
                v["base"] = json!(formid_str(base));
                v["scale"] = json!(f64::from(p.scale));
                out.push(v);
            }
            Err(e) => {
                bump(&mut outcomes, format!("{e:?}").split('(').next().unwrap_or("error").to_string());
                out.push(json!({"reference": formid_str(r), "base": formid_str(base), "error": e.to_string()}));
            }
        }
    }
    json!({
        "schema": "fnv.actor.appearance/1",
        "summary": {"outcomes": outcomes, "base_kinds": base_kinds, "worn_items": worn_counts},
        "issues": actors.issues.to_json(),
        "actors": out,
    })
}
