//! Independent synthetic scene placement; no native assets or installed mod text.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    GameData,
    game::{Game, GameConfig},
};
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}
fn fixture() -> Fixture {
    let mut bytes = tes4();
    bytes.extend(top(b"NPC_", &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))])));
    bytes.extend(top(b"STAT", &record(b"STAT", 0, 0xD10, &[sub(b"EDID", z("OwnedSceneBase")), sub(b"MODL", z("owned\\original.nif"))])));
    bytes.extend(top(b"STAT", &record(b"STAT", 0, 0xD13, &[sub(b"EDID", z("OwnedOtherSceneBase")), sub(b"MODL", z("owned\\other.nif"))])));
    bytes.extend(top(b"PROJ", &record(b"PROJ", 0, 0xD20, &[sub(b"EDID", z("OwnedProjectileBase")), sub(b"MODL", z("owned\\projectile.nif"))])));
    bytes.extend(top(b"MISC", &record(b"MISC", 0, 0xD30, &[sub(b"EDID", z("OwnedInventoryBase")), sub(b"DATA", [25u32.to_le_bytes(), 1.0f32.to_le_bytes()].concat())])));
    bytes.extend(top(b"ACTI", &record(b"ACTI", 0, 0xD40, &[sub(b"EDID", z("OwnedActivatorBase"))])));
    let mut script_subs = vec![sub(b"EDID", z("OwnedSceneScript"))];
    script_subs.extend(unit(0, &script(&[(0, None, Vec::new())]), &[], &[]));
    bytes.extend(top(b"SCPT", &record(b"SCPT", 0, 0xD12, &script_subs)));
    bytes.extend(top(b"ACTI", &record(b"ACTI", 0, 0xD11, &[sub(b"EDID", z("OwnedScriptedBase")), sub(b"SCRI", u32s(&[0xD12]))])));
    let plugins = vec![Plugin::parse(&bytes, "OwnedSceneClone.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&functions).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}
fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}
fn place(g: &mut Game, temporary: bool) -> FormId {
    let prepare = if temporary { "let ownBase := TempCloneForm OwnedSceneBase 1\nSetModelPath \"owned\\replacement.nif\" ownBase\n" } else { "let ownBase := OwnedSceneBase\n" };
    let text = format!("ref ownBase\nref ownPlaced\nBegin Function {{}}\n{prepare}let ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd");
    let XVal::Ref(id) = g.run_query(&text, Default::default()).unwrap() else { panic!("expected reference result") };
    assert_ne!(id, 0, "a supported scene base produces a real reference");
    FormId(id)
}

fn save_result(f: &Fixture, script: &str) -> serde_json::Value {
    let input = serde_json::from_value(serde_json::json!({"format":"fnv.script.actions/1","new_game":false,"actions":[
        {"op":"query","label":"place","script":script},
        {"op":"save_load","label":"snapshot"}
    ]}))
    .unwrap();
    let trace = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    trace["observations"].as_array().unwrap().iter().find(|r| r["label"] == "snapshot").unwrap().clone()
}

#[test]
fn temporary_scene_placement_preserves_the_cloned_base_identity() {
    let f = fixture();
    let mut g = game(&f);
    let placed = place(&mut g, true);
    let base = g.world.peek(placed).unwrap().base;
    assert_ne!(base, FormId(0xD10), "silently placing the original loses the clone");
    assert_eq!(g.unclone(base), FormId(0xD10));
    assert_eq!(g.world.peek(placed).unwrap().count, 1);
}

#[test]
fn ordinary_scene_placement_keeps_its_original_base_and_snapshot() {
    let f = fixture();
    let mut g = game(&f);
    let placed = place(&mut g, false);
    assert_eq!(g.world.peek(placed).unwrap().base, FormId(0xD10));
    let snapshot = g.world.snapshot();
    assert_eq!(snapshot.refs.get(&placed).unwrap().base, FormId(0xD10));
    let restored = World::restore(&f.forms, &f.refs, &snapshot).unwrap();
    assert_eq!(restored.peek(placed).unwrap().base, FormId(0xD10));
}

#[test]
fn actual_headless_save_boundary_rejects_scene_clones_and_keeps_ordinary_saves() {
    let f = fixture();
    for temporary in [false, true] {
        let prepare = if temporary { "let ownBase := TempCloneForm OwnedSceneBase 1" } else { "let ownBase := OwnedSceneBase" };
        let script = format!("ref ownBase\nref ownPlaced\nBegin Function {{}}\n{prepare}\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd");
        let result = save_result(&f, &script);
        if temporary {
            assert!(result["errors"]["action"].as_str().is_some_and(|e| e.contains("temporary cloned base")), "{result}");
            assert!(result.get("bytes").is_none(), "no successful save-size answer after refusal");
        } else {
            assert!(result.get("errors").is_none(), "{result}");
            assert!(result["bytes"].as_u64().is_some_and(|n| n > 0));
        }
    }
}

#[test]
fn unsupported_scene_clone_shape_does_not_silently_place_an_original() {
    let f = fixture();
    let mut g = game(&f);
    let source = "ref ownBase\nref ownPlaced\nBegin Function {}\nlet ownBase := TempCloneForm Player 1\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd";
    assert_eq!(g.run_query(source, Default::default()).unwrap(), XVal::Ref(0));
    assert!(g.events.iter().any(|e| matches!(&e.kind,fnv_game::trace::EventKind::CallFailed {error,..} if error.contains("supported temporary scene base"))));
}

#[test]
fn scene_clone_uses_its_preplacement_override_and_refuses_late_replacement() {
    let f = fixture();
    let mut g = game(&f);
    let placed = place(&mut g, true);
    let base = g.world.peek(placed).unwrap().base;
    let read = fnv_game::nvse::call(&mut g, &Default::default(), "getmodelpath", Some(placed), &[]).unwrap().unwrap();
    assert_eq!(read, XVal::Str("owned\\replacement.nif".into()));
    let changed = fnv_game::nvse::call(&mut g, &Default::default(), "setmodelpath", None, &[XVal::Str("owned\\late.nif".into()), XVal::Ref(base.0)]).unwrap();
    assert!(changed.is_err(), "the preview must not claim a reload it cannot apply");
    assert_eq!(fnv_game::nvse::call(&mut g, &Default::default(), "getmodelpath", Some(placed), &[]).unwrap().unwrap(), read);
}

#[test]
fn deleting_a_scene_clone_preserves_its_tombstone_through_the_save_boundary() {
    let f = fixture();
    let mut g = game(&f);
    let placed = place(&mut g, true);
    let base = g.world.peek(placed).unwrap().base;
    g.world.set_deleted(placed, true).unwrap();
    let raw = g.snapshot();
    assert!(!raw.sim.refs.contains_key(&placed));
    assert_eq!(raw.sim.deleted_created.get(&placed), Some(&base));
    let source = "ref ownBase\nref ownPlaced\nBegin Function {}\nlet ownBase := TempCloneForm OwnedSceneBase 1\nlet ownPlaced := player.PlaceAtMe ownBase\nownPlaced.MarkForDelete\nSetFunctionValue ownPlaced\nEnd";
    let result = save_result(&f, source);
    assert!(result.get("errors").is_none(), "{result}");
    assert!(result["bytes"].as_u64().is_some_and(|n| n > 0));
}

#[test]
fn a_script_attached_to_a_scene_clone_is_not_silently_dropped_by_placement() {
    let f = fixture();
    for setup in ["let ownBase := TempCloneForm OwnedSceneBase 1\nSetScript OwnedSceneScript ownBase", "let ownBase := TempCloneForm OwnedScriptedBase 1"] {
        let mut g = game(&f);
        assert_eq!(g.base_script(FormId(0xD11)), Some(FormId(0xD12)), "the inherited-script control must be real");
        let source = format!("ref ownBase\nref ownPlaced\nBegin Function {{}}\n{setup}\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd");
        assert_eq!(g.run_query(&source, Default::default()).unwrap(), XVal::Ref(0));
        assert_eq!(g.world.created().count(), 0);
        assert!(g.events.iter().any(|e| matches!(&e.kind,fnv_game::trace::EventKind::CallFailed {error,..} if error.contains("attached script"))));
    }
}

#[test]
fn an_inherited_original_model_override_is_frozen_at_placement_including_empty() {
    let f = fixture();
    for path in ["owned\\changed.nif", ""] {
        let mut g = game(&f);
        let source =
            format!("ref ownBase\nref ownPlaced\nBegin Function {{}}\nSetModelPath \"{path}\" OwnedSceneBase\nlet ownBase := TempCloneForm OwnedSceneBase 1\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd");
        let XVal::Ref(id) = g.run_query(&source, Default::default()).unwrap() else { panic!("expected reference") };
        assert_ne!(id, 0);
        let placed = FormId(id);
        let base = g.world.peek(placed).unwrap().base;
        let read = fnv_game::nvse::call(&mut g, &Default::default(), "getmodelpath", Some(placed), &[]).unwrap().unwrap();
        assert_eq!(read, XVal::Str(path.into()));
        assert_eq!(g.world.temporary_scene_base(base).unwrap().model_override.as_deref(), Some(path), "the model shared by drawing and physics must match the effective getter");
        let later = fnv_game::nvse::call(&mut g, &Default::default(), "setmodelpath", None, &[XVal::Str("owned\\late.nif".into()), XVal::Ref(0xD10)]).unwrap();
        assert!(later.is_err(), "original mutations cannot silently diverge from its placed aliases");
        assert_eq!(fnv_game::nvse::call(&mut g, &Default::default(), "getmodelpath", Some(placed), &[]).unwrap().unwrap(), read);
        assert!(fnv_game::nvse::call(&mut g, &Default::default(), "setmodelpath", None, &[XVal::Str("owned\\unrelated.nif".into()), XVal::Ref(0xD13)]).unwrap().is_ok());
    }
}

#[test]
fn inventory_clone_and_clone_chain_are_explicitly_refused() {
    let f = fixture();
    assert!(f.forms.item(FormId(0xD30)).is_some(), "the inventory control must really be an item");
    for prepare in ["let ownBase := TempCloneForm OwnedInventoryBase 1", "let ownFirst := TempCloneForm OwnedSceneBase 1\nlet ownBase := TempCloneForm ownFirst 1"] {
        let mut g = game(&f);
        let source = format!("ref ownFirst\nref ownBase\nref ownPlaced\nBegin Function {{}}\n{prepare}\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd");
        assert_eq!(g.run_query(&source, Default::default()).unwrap(), XVal::Ref(0));
        assert_eq!(g.world.created().count(), 0);
        assert!(g.events.iter().any(|e| matches!(&e.kind,fnv_game::trace::EventKind::CallFailed {error,..} if error.contains("supported temporary scene base") || error.contains("clone of a temporary clone"))));
    }
}

#[test]
fn a_projectile_clone_can_be_an_inert_created_model_but_not_an_activator() {
    let f = fixture();
    let mut g = game(&f);
    let source =
        "ref ownBase\nref ownPlaced\nBegin Function {}\nlet ownBase := TempCloneForm OwnedProjectileBase 1\nSetModelPath \"owned\\projectile-visual.nif\" ownBase\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd";
    let XVal::Ref(id) = g.run_query(source, Default::default()).unwrap() else { panic!("expected reference") };
    assert_ne!(id, 0);
    let state = g.world.peek(FormId(id)).unwrap();
    assert!(!state.shot);
    assert_eq!(g.unclone(state.base), FormId(0xD20));
    assert_eq!(g.world.temporary_scene_base(state.base).unwrap().model_override.as_deref(), Some("owned\\projectile-visual.nif"));
    let mut g = game(&f);
    let source = "ref ownBase\nBegin Function {}\nlet ownBase := TempCloneForm OwnedActivatorBase 1\nSetFunctionValue (player.PlaceAtMe ownBase)\nEnd";
    assert_eq!(g.run_query(source, Default::default()).unwrap(), XVal::Num(0.0));
    assert_eq!(g.world.created().count(), 0, "activator behavior is not admitted by a model-only preview");
    assert!(g.events.iter().any(|e| matches!(&e.kind,fnv_game::trace::EventKind::CallFailed {error,..} if error.contains("supported temporary scene base"))));
}

#[test]
fn late_script_changes_to_a_placed_alias_or_original_are_refused() {
    let f = fixture();
    let mut g = game(&f);
    let placed = place(&mut g, true);
    let base = g.world.peek(placed).unwrap().base;
    for target in [base, FormId(0xD10)] {
        let result = fnv_game::nvse::call(&mut g, &Default::default(), "setscript", None, &[XVal::Ref(0xD12), XVal::Ref(target.0)]).unwrap();
        assert!(result.is_err(), "a late attachment must not silently change script behavior");
        assert_eq!(g.base_script(target), None);
    }
    assert!(fnv_game::nvse::call(&mut g, &Default::default(), "setscript", None, &[XVal::Ref(0xD12), XVal::Ref(0xD13)]).unwrap().is_ok());
    assert_eq!(g.base_script(FormId(0xD13)), Some(FormId(0xD12)));
}

fn retired_query(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("ref ownBase\nref ownPlaced\nBegin Function {{}}\n{body}\nEnd"), Default::default()).unwrap()
}

fn retire_scene(g: &mut Game, local: bool, model: &str) -> (FormId, FormId) {
    let XVal::Ref(id) = retired_query(g, &format!(
        "let ownBase := TempCloneForm OwnedSceneBase {}\nSetModelPath \"{model}\" ownBase\nlet ownPlaced := player.PlaceAtMe ownBase\nownPlaced.MarkForDelete\nSetFunctionValue ownPlaced", u8::from(local)
    )) else { panic!("real placed reference") };
    let reference = FormId(id);
    assert_ne!(id, 0);
    let state = g.world.peek(reference).unwrap();
    assert!(state.deleted());
    (reference, state.base)
}

fn restore_scene<'a>(f: &'a Fixture, value: &serde_json::Value) -> Result<Game<'a>, String> {
    let snapshot: fnv_game::game::GameSnapshot = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    let world = World::restore(&f.forms, &f.refs, &snapshot.sim)?;
    Game::restore(&f.data, world, GameConfig { strict: true, ..Default::default() }, &snapshot)
}

#[test]
fn retired_scene_roundtrip_keeps_tombstone_provenance_without_reviving_a_clone() {
    for local in [false, true] {
        let f = fixture();
        let mut g = game(&f);
        let (reference, alias) = retire_scene(&mut g, local, "owned\\retired.nif");
        let before = g.world.snapshot();
        // First changed consumer boundary: checked snapshot must admit this exact
        // retired closure, not erase its reference or replace its base.
        let snapshot = g.checked_snapshot().expect("retired scene provenance is reconstructible");
        assert_eq!(snapshot.sim.deleted_created.get(&reference), Some(&alias));
        assert!(!snapshot.sim.refs.contains_key(&reference));
        assert_eq!(g.world.snapshot(), before, "saving must not mutate the running world");
        assert_eq!(g.unclone(alias), FormId(0xD10), "the original running clone remains valid");
        let json = serde_json::to_value(&snapshot).unwrap();
        let key = alias.to_string();
        assert_eq!(json["format"], "fnv.game.state/2-retired-scenes+lambda-ids");
        assert_eq!(json["retired_scene_bases"][&key]["original"], "0x00000D10");
        assert_eq!(json["retired_scene_bases"][&key]["kind"], serde_json::json!([83, 84, 65, 84]));
        assert_eq!(json["retired_scene_bases"][&key]["model_override"], "owned\\retired.nif");
        let mut back = restore_scene(&f, &json).unwrap();
        assert!(back.world.peek(reference).is_none());
        assert_eq!(back.world.deleted_base(reference), Some(alias));
        assert!(back.world.temporary_scene_base(alias).is_none(), "retired provenance is not a live scene base");
        assert_eq!(back.unclone(alias), alias);
        assert_eq!(retired_query(&mut back, &format!("SetFunctionValue (IsClonedForm (StringToRef \"{:08X}\"))", alias.0)), XVal::Num(0.0));
        let repeated = serde_json::to_value(back.checked_snapshot().unwrap()).unwrap();
        assert_eq!(repeated["retired_scene_bases"], json["retired_scene_bases"]);
        // The earlier positional-lambda-ID retired format still loads.
        let mut legacy = json.clone();
        legacy["format"] = "fnv.game.state/2-retired-scenes".into();
        if let Some(lambdas) = legacy.get_mut("lambdas").and_then(|l| l.as_array_mut()) {
            lambdas.iter_mut().for_each(|l| { l.as_object_mut().unwrap().remove("id"); });
        }
        let old = restore_scene(&f, &legacy).unwrap();
        assert_eq!(old.world.deleted_base(reference), Some(alias));
        assert_eq!(serde_json::to_value(old.checked_snapshot().unwrap()).unwrap()["retired_scene_bases"], json["retired_scene_bases"]);
        let XVal::Ref(fresh) = retired_query(&mut back, &format!("SetFunctionValue (TempCloneForm OwnedSceneBase {})", u8::from(local))) else { panic!("fresh clone") };
        assert!(fresh > alias.0, "the retired alias cannot be reassigned");
        let placed = back.world.place(FormId(0xD13), None, [0.0; 3], [0.0; 3], 1).unwrap();
        assert!(placed.0 > reference.0);
        assert_eq!(back.world.deleted_base(reference), Some(alias));
        assert_eq!(back.world.peek(placed).unwrap().base, FormId(0xD13));
    }
}

#[test]
fn retired_scene_preserves_empty_override_and_independent_original_metadata() {
    let f = fixture(); let mut g = game(&f);
    let (a, alias_a) = retire_scene(&mut g, true, "");
    let (b, alias_b) = retire_scene(&mut g, true, "owned\\second.nif");
    let snapshot = g.checked_snapshot().unwrap();
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["retired_scene_bases"][alias_a.to_string()]["model_override"], "");
    assert_eq!(json["retired_scene_bases"][alias_b.to_string()]["model_override"], "owned\\second.nif");
    let mut back = restore_scene(&f, &json).unwrap();
    assert_eq!(back.world.deleted_base(a), Some(alias_a));
    assert_eq!(back.world.deleted_base(b), Some(alias_b));
    assert_eq!(retired_query(&mut back, "SetFunctionValue (GetModelPath OwnedSceneBase)"), XVal::Str("owned\\original.nif".into()));
    assert_eq!(retired_query(&mut back, "SetFunctionValue (GetModelPath OwnedOtherSceneBase)"), XVal::Str("owned\\other.nif".into()));
}

#[test]
fn retired_scene_metadata_cannot_admit_a_live_or_unresolved_scene_alias() {
    let f = fixture(); let mut g = game(&f);
    let (_, retired) = retire_scene(&mut g, true, "owned\\retired.nif");
    let live = place(&mut g, true);
    assert_ne!(g.world.peek(live).unwrap().base, retired);
    assert!(g.checked_snapshot().unwrap_err().contains("temporary cloned base"));
    let mut g = game(&f);
    let XVal::Ref(alias) = retired_query(&mut g, "SetFunctionValue (TempCloneForm OwnedSceneBase 1)") else { panic!("clone") };
    let reference = g.world.place(FormId(0xD10), None, [0.0; 3], [0.0; 3], 1).unwrap();
    g.world.get(reference).unwrap().base = FormId(alias);
    g.world.set_deleted(reference, true).unwrap();
    assert!(g.checked_snapshot().unwrap_err().contains("temporary cloned base"), "an unadmitted alias must not acquire provenance from its ID alone");
}

#[test]
fn retired_scene_format_and_malformed_provenance_fail_before_restore() {
    let f = fixture(); let mut g = game(&f);
    let (reference, alias) = retire_scene(&mut g, true, "owned\\retired.nif");
    let valid = serde_json::to_value(g.checked_snapshot().unwrap()).unwrap();
    let key = alias.to_string();
    for case in 0..10 {
        let mut value = valid.clone();
        match case {
            0 => value["format"] = "fnv.game.state/1".into(),
            1 => value["clone_sequence_floor"] = 0.into(),
            2 => value["clone_sequence_floor"] = u32::MAX.into(),
            3 => value["retired_scene_bases"][&key]["original"] = key.clone().into(),
            4 => value["retired_scene_bases"][&key]["kind"] = serde_json::json!([78, 80, 67, 95]),
            5 => { value["sim"]["deleted_created"].as_object_mut().unwrap().remove(&reference.to_string()); },
            6 => { value["retired_scene_bases"] = serde_json::json!({}); },
            7 => value["retired_scene_bases"][&key]["original"] = "0x00000007".into(),
            8 => value["sim"]["next_dynamic"] = reference.0.into(),
            9 => value["format"] = fnv_game::game::SNAPSHOT_FORMAT.into(),
            _ => unreachable!(),
        }
        assert!(restore_scene(&f, &value).is_err(), "malformed case {case} must refuse");
    }
    // Ordinary-format readers admit only /1 or /2, so this record cannot be silently ignored.
    assert_ne!(valid["format"], fnv_game::game::SNAPSHOT_FORMAT);
    assert_eq!(serde_json::to_value(game(&f).checked_snapshot().unwrap()).unwrap()["format"], fnv_game::game::SNAPSHOT_FORMAT);
}

#[test]
fn retired_scene_restore_refuses_authored_identity_collision_and_wrong_original() {
    let mut f = fixture();
    let (alias, json) = {
        let mut g = game(&f);
        let (_, alias) = retire_scene(&mut g, true, "owned\\retired.nif");
        (alias, serde_json::to_value(g.checked_snapshot().unwrap()).unwrap())
    };
    // Exercise the actual loaded record-signature admission boundary; this is an
    // adversarial GameData collision, not a claim about a winning plugin importer.
    f.data.sigs.insert(alias, *b"STAT");
    assert!(restore_scene(&f, &json).is_err());
    f.data.sigs.remove(&alias);
    f.data.sigs.insert(FormId(0xD10), *b"NPC_");
    assert!(restore_scene(&f, &json).is_err());
}

#[test]
fn retired_scene_sequence_reservation_and_exhaustion_do_not_reuse_old_ids() {
    let f = fixture(); let mut g = game(&f);
    let (_, alias) = retire_scene(&mut g, true, "owned\\retired.nif");
    let XVal::Ref(later_unused) = retired_query(&mut g, "SetFunctionValue (TempCloneForm OwnedOtherSceneBase 1)") else { panic!("unused clone") };
    let mut json = serde_json::to_value(g.checked_snapshot().unwrap()).unwrap();
    let mut back = restore_scene(&f, &json).unwrap();
    assert_eq!(back.unclone(FormId(later_unused)), FormId(later_unused), "unused temporary clones must not be resurrected");
    let XVal::Ref(next) = retired_query(&mut back, "SetFunctionValue (TempCloneForm OwnedSceneBase 1)") else { panic!("next clone") };
    assert!(next > later_unused && later_unused > alias.0);
    json["clone_sequence_floor"] = (fnv_game::game::COMPILED_BASE - fnv_game::nvse_data::CLONE_BASE - 1).into();
    let mut exhausted = restore_scene(&f, &json).unwrap();
    let result = fnv_game::nvse::call(&mut exhausted, &Default::default(), "tempcloneform", None, &[XVal::Ref(0xD10), XVal::Num(0.0)]).unwrap();
    assert!(result.unwrap_err().contains("identity space exhausted"));
    assert_eq!(serde_json::to_value(exhausted.checked_snapshot().unwrap()).unwrap()["retired_scene_bases"], json["retired_scene_bases"]);
}

#[test]
fn ordinary_unused_clone_carriers_still_restore_as_invalid_without_new_format() {
    use fnv_script::vm::{ArrayKind, XKey};
    let f = fixture(); let mut g = game(&f);
    let alias = retired_query(&mut g, "SetFunctionValue (TempCloneForm OwnedSceneBase 1)");
    let XVal::Ref(id) = alias else { panic!("clone") };
    let array = g.ext.new_array(ArrayKind::List, vec![(XKey::Num(0.0), alias.clone())]);
    g.ext.retain(&XVal::Array(array));
    let json = serde_json::to_value(g.checked_snapshot().unwrap()).unwrap();
    assert_eq!(json["format"], fnv_game::game::SNAPSHOT_FORMAT);
    assert!(json.get("retired_scene_bases").is_none());
    let back = restore_scene(&f, &json).unwrap();
    assert_eq!(back.unclone(FormId(id)), FormId(id));
    assert_eq!(back.ext.array(array).unwrap().get(&XKey::Num(0.0)), Some(&alias));
}

#[test]
fn retired_alias_cannot_also_be_a_live_or_deleted_reference_identity() {
    let f = fixture(); let mut g = game(&f);
    let (_, alias) = retire_scene(&mut g, true, "owned\\retired.nif");
    let ordinary = g.world.place(FormId(0xD13), None, [0.0; 3], [0.0; 3], 1).unwrap();
    assert_eq!(g.world.peek(ordinary).unwrap().base, FormId(0xD13));
    let valid = serde_json::to_value(g.checked_snapshot().unwrap()).unwrap();
    let live = valid["sim"]["refs"][ordinary.to_string()].clone();
    assert_eq!(live["base"], "0x00000D13");
    assert!(restore_scene(&f, &valid).unwrap().world.peek(alias).is_none());
    for deleted in [false, true] {
        let mut malformed = valid.clone();
        if deleted {
            malformed["sim"]["deleted_created"][alias.to_string()] = "0x00000D13".into();
        } else {
            malformed["sim"]["refs"][alias.to_string()] = live.clone();
        }
        assert!(restore_scene(&f, &malformed).is_err(), "retired alias used as a reference key: deleted={deleted}");
    }
}
