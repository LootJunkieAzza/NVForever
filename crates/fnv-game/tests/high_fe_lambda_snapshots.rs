//! Owned CPU inputs for the proposed callable/snapshot policy; no installed data.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{GameSnapshot, InstKey};
use fnv_game::{Game, GameConfig, GameData};
use fnv_script::compile::{Env, FormInfo};
use fnv_script::unit::RefSlot;
use fnv_script::vm::{ArrayKind, Value, XKey, XVal};
use fnv_script::{FunctionTable, VarDecl, VarType};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const MAKER_A: u32 = 0x0000_0901;
const MAKER_B: u32 = 0x0000_0902;
const BOUNDARY: u32 = 0xFE80_0000;
const OCCUPIED_NPC: u32 = BOUNDARY + 2;

struct NoForms;
impl Env for NoForms {
    fn form(&self, _: &str) -> Option<FormInfo> { None }
    fn info(&self, _: u32) -> Option<FormInfo> { None }
    fn editor_id(&self, _: u32) -> Option<String> { None }
    fn local_form(&self, form: u32) -> Option<u32> { Some(form) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

fn function(table: &FunctionTable, id: u32, name: &str, value: u32, maker: bool) -> Vec<u8> {
    let value = if maker {
        format!("(Begin Function {{}}\nSetFunctionValue {value}\nEnd)")
    } else { value.to_string() };
    let source = format!("scn {name}\nref held\nref other\nBegin Function {{}}\nSetFunctionValue {value}\nEnd\n");
    let c = fnv_script::compile::compile(&source, table, &NoForms, None);
    assert!(c.errors().next().is_none(), "authored {name}: {:?}", c.diags);
    assert!(!c.bytes.is_empty());
    let vars: Vec<(u32, &str, u8)> = c.variables.iter().map(|v| (v.index, v.name.as_str(), u8::from(v.ty == VarType::Int))).collect();
    let refs: Vec<u32> = c.refs.iter().map(|r| match r {
        RefSlot::Form(id) => *id,
        _ => panic!("constant authored function has an unexpected reference slot"),
    }).collect();
    assert!(refs.is_empty());
    let mut fields = vec![sub(b"EDID", z(name))];
    fields.extend(unit(0, &c.bytes, &vars, &refs));
    fields.push(sub(b"SCTX", source.into_bytes()));
    record(b"SCPT", 0, id, &fields)
}

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }
fn fixture(occupied: bool) -> Fixture {
    let schemas = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let catalogue_path = schemas.join("script/functions.json");
    let table = FunctionTable::load(&catalogue_path).expect("public catalogue");
    let mut plugins = Vec::new();
    for index in 0..255 {
        let mut bytes = tes4();
        if index == 0 {
            bytes.extend(top(b"NPC_", &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))])));
            let scripts = [function(&table, MAKER_A, "OwnedMakerA", 17, true), function(&table, MAKER_B, "OwnedMakerB", 19, true)].concat();
            bytes.extend(top(b"SCPT", &scripts));
        }
        if index == 254 && occupied {
            bytes.extend(top(b"SCPT", &function(&table, BOUNDARY & 0x00FF_FFFF, "OwnedBoundary", 43, false)));
            bytes.extend(top(b"NPC_", &record(b"NPC_", 0, OCCUPIED_NPC & 0x00FF_FFFF, &[sub(b"EDID", z("OwnedHighNPC"))])));
        }
        plugins.push(Plugin::parse(&bytes, format!("OwnedSnapshotSlot{index:03}.esm")).expect("authored plugin"));
    }
    let order = LoadOrder::new(&plugins).expect("255 owned plugins admitted");
    assert_eq!(order.to_global(254, 0x0080_0000), Some(BOUNDARY));
    let schema = Schema::load_dir(schemas.join("fnv")).expect("public schema");
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let catalogue = serde_json::from_str(&std::fs::read_to_string(catalogue_path).unwrap()).unwrap();
    let data = GameData::build(&order, &schema, table, &catalogue, &refs);
    assert_eq!(data.load_order.len(), 255);
    for id in [MAKER_A, MAKER_B] {
        assert!(data.script(FormId(id)).unwrap().program.blocks.iter().any(|b| b.event == fnv_script::ir::FUNCTION_BLOCK));
    }
    if occupied {
        assert!(data.script(FormId(BOUNDARY)).is_some());
        assert_eq!(data.sig(FormId(OCCUPIED_NPC)), Some(*b"NPC_"));
    }
    Fixture { data, forms, refs }
}
fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}
fn call(g: &mut Game<'_>, target: &XVal) -> Result<XVal, String> {
    g.run_function(target, None, vec![], None, Default::default())
}
fn maker(g: &mut Game<'_>, id: u32, expected: f64) -> XVal {
    let lambda = call(g, &XVal::Ref(id)).expect("record maker compiles and executes");
    assert!(matches!(lambda, XVal::Ref(_)), "real maker must allocate a lambda");
    assert_eq!(call(g, &lambda).unwrap(), XVal::Num(expected), "actual allocated lambda body");
    lambda
}
fn token(value: &XVal) -> u32 {
    let XVal::Ref(id) = value else { panic!("expected actual reference token") };
    *id
}
fn legacy(snapshot: &GameSnapshot) -> GameSnapshot {
    let mut json = serde_json::to_value(snapshot).unwrap();
    json["format"] = "fnv.game.state/1".into();
    for lambda in json["lambdas"].as_array_mut().unwrap() { lambda.as_object_mut().unwrap().remove("id"); }
    serde_json::from_value(json).expect("legacy serde(default) decode")
}
fn restore<'a>(f: &'a Fixture, snapshot: &GameSnapshot) -> Result<Game<'a>, String> {
    let world = World::restore(&f.forms, &f.refs, &snapshot.sim)?;
    Game::restore(&f.data, world, GameConfig { strict: true, ..Default::default() }, snapshot)
}

#[test]
fn sparse_lambda_ids_keep_variable_array_and_record_identity_through_save() {
    let f = fixture(true);
    let mut g = game(&f);
    let a = maker(&mut g, MAKER_A, 17.0);
    let b = maker(&mut g, MAKER_B, 19.0);
    eprintln!("owned sparse witness a={a:?} b={b:?} records={:?}/{:?} events={:?}", f.data.sig(FormId(BOUNDARY)), f.data.sig(FormId(OCCUPIED_NPC)), g.events);
    assert_eq!((token(&a), token(&b)), (BOUNDARY + 1, BOUNDARY + 3), "skip SCPT and non-SCPT occupied records");
    assert_eq!(call(&mut g, &XVal::Ref(BOUNDARY)).unwrap(), XVal::Num(43.0));
    assert_eq!(call(&mut g, &XVal::Num(f64::from(BOUNDARY))).unwrap(), XVal::Num(43.0));
    assert_eq!(call(&mut g, &XVal::Num(f64::from(token(&b)))).unwrap(), XVal::Num(19.0));
    let owner = InstKey::Ref { reference: FormId::PLAYER_REF };
    g.store(owner, Some(FormId(MAKER_A)), 1, Value::Ref(token(&a)), "owned snapshot", false);
    g.store(owner, Some(FormId(MAKER_A)), 2, Value::Ref(token(&b)), "owned snapshot", false);
    let inner = g.ext.new_array(ArrayKind::List, vec![(XKey::Num(0.0), a.clone()), (XKey::Num(1.0), b.clone())]);
    let outer = g.ext.new_array(ArrayKind::List, vec![(XKey::Num(0.0), XVal::Array(inner))]);
    let before = g.checked_snapshot().unwrap();
    let json = serde_json::to_value(&before).unwrap();
    assert_eq!(json["format"], "fnv.game.state/2");
    assert_eq!(json["lambdas"][0]["id"], token(&a));
    assert_eq!(json["lambdas"][1]["id"], token(&b));
    let back: GameSnapshot = serde_json::from_value(json).unwrap();
    let mut r = restore(&f, &back).unwrap();
    assert_eq!(r.instances[&owner].vars[&1], Value::Ref(token(&a)));
    assert_eq!(r.instances[&owner].vars[&2], Value::Ref(token(&b)));
    assert_eq!(r.ext.array(outer).unwrap().entries[0].1, XVal::Array(inner));
    assert_eq!(r.ext.array(inner).unwrap().entries[0].1, a);
    assert_eq!(r.ext.array(inner).unwrap().entries[1].1, b);
    assert_eq!(call(&mut r, &a).unwrap(), XVal::Num(17.0));
    assert_eq!(call(&mut r, &b).unwrap(), XVal::Num(19.0));
    assert_eq!(call(&mut r, &XVal::Ref(MAKER_A)).unwrap(), a, "same live LambdaKey keeps its saved name");
    assert_eq!(r.snapshot().lambdas, before.lambdas);
    assert!(!r.events.iter().any(|e| matches!(e.kind, fnv_game::trace::EventKind::ScriptError { .. } | fnv_game::trace::EventKind::CallFailed { .. } | fnv_game::trace::EventKind::Skipped { .. })));
}

#[test]
fn nonconflicting_legacy_lambdas_keep_ids_and_missing_units_keep_slots() {
    let f = fixture(false);
    let mut g = game(&f);
    let a = maker(&mut g, MAKER_A, 17.0);
    let b = maker(&mut g, MAKER_B, 19.0);
    assert_eq!((token(&a), token(&b)), (BOUNDARY, BOUNDARY + 1));
    let old = legacy(&g.checked_snapshot().unwrap());
    old.check().unwrap();
    let mut r = restore(&f, &old).unwrap();
    assert_eq!(call(&mut r, &a).unwrap(), XVal::Num(17.0));
    assert_eq!(call(&mut r, &b).unwrap(), XVal::Num(19.0));
    assert_eq!(call(&mut r, &XVal::Ref(MAKER_A)).unwrap(), a);
    let mut gone = old.clone();
    gone.lambdas[0].unit = "owned missing unit".into();
    let mut r = restore(&f, &gone).unwrap();
    assert!(call(&mut r, &a).unwrap_err().contains("gone"));
    assert_eq!(call(&mut r, &b).unwrap(), XVal::Num(19.0), "later slot must not shift");
    let remade = maker(&mut r, MAKER_A, 17.0);
    assert_ne!(remade, a, "missing-unit tombstone cannot be reallocated");
    let json = serde_json::to_value(r.snapshot()).unwrap();
    assert_eq!(json["format"], "fnv.game.state/2", "new writer fences readers that assume positional IDs");
}

#[test]
fn ambiguous_and_malformed_snapshot_ids_refuse_without_silent_retargeting() {
    let f = fixture(true);
    let mut g = game(&f);
    let a = maker(&mut g, MAKER_A, 17.0);
    let snapshot = g.checked_snapshot().unwrap();
    let mut explicit = serde_json::to_value(&snapshot).unwrap();
    explicit["format"] = "fnv.game.state/2".into();
    explicit["lambdas"][0]["id"] = BOUNDARY.into();
    let explicit: GameSnapshot = serde_json::from_value(explicit).unwrap();
    let old = legacy(&snapshot);
    let mut errors = Vec::new();
    for mut bad in [old.clone(), explicit.clone()] {
        for missing in [false, true] {
            if missing { bad.lambdas[0].unit = "owned missing unit".into(); }
            errors.push(restore(&f, &bad).err());
        }
    }
    eprintln!("owned conflict refusals={errors:?}; original lambda={a:?}; events={:?}", g.events);
    assert!(errors.iter().all(|e| e.as_ref().is_some_and(|e| e.contains("0xFE800000") && e.contains("conflicts"))));
    assert_eq!(call(&mut g, &a).unwrap(), XVal::Num(17.0), "refusal does not rewrite the original Game");
    for (name, mut json) in [
        ("missing ID", serde_json::to_value(&snapshot).unwrap()),
        ("duplicate ID", serde_json::to_value(&snapshot).unwrap()),
        ("outside range", serde_json::to_value(&snapshot).unwrap()),
    ] {
        match name {
            "missing ID" => { json["lambdas"][0].as_object_mut().unwrap().remove("id"); }
            "duplicate ID" => { let same = json["lambdas"][0].clone(); json["lambdas"].as_array_mut().unwrap().push(same); }
            _ => { json["lambdas"][0]["id"] = 0xFF00_0000u32.into(); }
        }
        let bad: GameSnapshot = serde_json::from_value(json).unwrap();
        assert!(bad.check().is_err(), "{name} must be refused before any Game construction");
    }
}

#[test]
fn restored_last_lambda_id_exhausts_without_wrapping_or_reusing_a_tombstone() {
    let f = fixture(false);
    let mut g = game(&f);
    let _ = maker(&mut g, MAKER_A, 17.0);
    let mut json = serde_json::to_value(g.checked_snapshot().unwrap()).unwrap();
    json["format"] = "fnv.game.state/2".into();
    json["lambdas"][0]["id"] = 0xFEFF_FFFFu32.into();
    let snapshot: GameSnapshot = serde_json::from_value(json).unwrap();
    let mut r = restore(&f, &snapshot).unwrap();
    assert_eq!(call(&mut r, &XVal::Ref(MAKER_A)).unwrap(), XVal::Ref(0xFEFF_FFFF));
    assert_eq!(call(&mut r, &XVal::Ref(0xFEFF_FFFF)).unwrap(), XVal::Num(17.0));
    assert!(call(&mut r, &XVal::Ref(MAKER_B)).unwrap_err().contains("exhausted"));
    assert_eq!(r.snapshot().lambdas.len(), 1);
}
