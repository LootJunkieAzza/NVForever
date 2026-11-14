//! Authored controls for the proposed session gate, separate from the frozen five.
//! These exercise public consumers; no native behavior or installed input is assumed.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::compile::{Env, FormInfo};
use fnv_script::unit::RefSlot;
use fnv_script::vm::{EventCall, Value, XKey, XVal};
use fnv_script::{FunctionTable, VarDecl, VarType};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const RECORD_COUNT: u32 = 0x800;
const LAMBDA_COUNT: u32 = 0x801;
const DROP_COUNT: u32 = 0x802;
const LOW_COUNT: u32 = 0x803;
const CONTEXT: u32 = 0x804;
const WEAPON: u32 = 0x810;
const ITEM_MOD: u32 = 0x811;
const OTHER_MOD: u32 = 0x812;
const MAKER: u32 = 0x901;
const LOW: u32 = 0x903;
const HIGH: u32 = 0xFE80_0000;
const OCCUPIED_NPC: u32 = HIGH + 2;
const PP_HIGH: u32 = HIGH + 4;
const DROP_HIGH: u32 = HIGH + 6;
const NON_FUNCTION: u32 = HIGH + 8;
const GLOBALS: [(u32, &str); 5] = [
    (RECORD_COUNT, "OwnedRecordCount"),
    (LAMBDA_COUNT, "OwnedLambdaCount"),
    (DROP_COUNT, "OwnedDropCount"),
    (LOW_COUNT, "OwnedLowCount"),
    (CONTEXT, "OwnedContext"),
];

struct Names;
impl Env for Names {
    fn form(&self, name: &str) -> Option<FormInfo> {
        GLOBALS.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)).map(|(form, _)| FormInfo { form: *form, sig: *b"GLOB" })
    }
    fn info(&self, form: u32) -> Option<FormInfo> { GLOBALS.iter().any(|(f, _)| *f == form).then_some(FormInfo { form, sig: *b"GLOB" }) }
    fn editor_id(&self, form: u32) -> Option<String> { GLOBALS.iter().find(|(f, _)| *f == form).map(|(_, name)| name.to_string()) }
    fn local_form(&self, form: u32) -> Option<u32> { Some(form) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

fn function(table: &FunctionTable, local_id: u32, name: &str, body: &str) -> Vec<u8> {
    let source = format!("scn {name}\nBegin Function {{}}\n{body}\nEnd\n");
    let c = fnv_script::compile::compile(&source, table, &Names, None);
    assert!(c.errors().next().is_none(), "{name}: {:?}", c.diags);
    let vars: Vec<_> = c.variables.iter().map(|v| (v.index, v.name.as_str(), u8::from(v.ty == VarType::Int))).collect();
    let refs: Vec<_> = c.refs.iter().map(|r| match r { RefSlot::Form(id) => *id, _ => panic!("unexpected authored reference slot") }).collect();
    let mut fields = vec![sub(b"EDID", z(name))];
    fields.extend(unit(0, &c.bytes, &vars, &refs));
    fields.push(sub(b"SCTX", source.into_bytes()));
    record(b"SCPT", 0, local_id, &fields)
}

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }
fn fixture() -> Fixture {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let catalogue = root.join("script/functions.json");
    let table = FunctionTable::load(&catalogue).unwrap();
    let mut plugins = Vec::new();
    for index in 0..255 {
        let mut bytes = if index == 254 {
            // The high script's low globals belong to master slot zero. Its own
            // records therefore use local index one, which resolves to global FE.
            let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
            record(b"TES4", 1, 0, &[sub(b"HEDR", hedr), sub(b"MAST", z("OwnedSessionSlot000.esm")), sub(b"DATA", vec![0; 8])])
        } else { tes4() };
        if index == 0 {
            bytes.extend(top(b"NPC_", &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))])));
            let globals: Vec<_> = GLOBALS.iter().flat_map(|(id, name)| record(b"GLOB", 0, *id, &[sub(b"EDID", z(name)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())])).collect();
            bytes.extend(top(b"GLOB", &globals));
            bytes.extend(top(b"WEAP", &record(b"WEAP", 0, WEAPON, &[sub(b"EDID", z("OwnedWeapon")), sub(b"DATA", vec![0; 15])])));
            let mods = [record(b"IMOD", 0, ITEM_MOD, &[sub(b"EDID", z("OwnedMod"))]), record(b"IMOD", 0, OTHER_MOD, &[sub(b"EDID", z("OwnedOtherMod"))])].concat();
            bytes.extend(top(b"IMOD", &mods));
            let scripts = [
                function(&table, MAKER, "OwnedMaker", "SetFunctionValue (Begin Function {}\nlet OwnedLambdaCount := OwnedLambdaCount + 1\nSetFunctionValue 0\nEnd)"),
                function(&table, LOW, "OwnedLow", "let OwnedLowCount := OwnedLowCount + 1\nSetFunctionValue 0"),
            ].concat();
            bytes.extend(top(b"SCPT", &scripts));
        }
        if index == 254 {
            let local = |id: u32| 0x0100_0000 | (id & 0x00FF_FFFF);
            let scripts = [
                function(&table, local(HIGH), "OwnedHigh", "let OwnedRecordCount := OwnedRecordCount + 1\nlet OwnedContext := GetSelf\nSetFunctionValue 0"),
                function(&table, local(PP_HIGH), "OwnedPpHigh", "let OwnedRecordCount := OwnedRecordCount + 1\nSetFunctionValue 0"),
                function(&table, local(DROP_HIGH), "OwnedDropHigh", "let OwnedDropCount := OwnedDropCount + 1\nSetFunctionValue 0"),
            ].concat();
            bytes.extend(top(b"SCPT", &scripts));
            let ordinary = script(&[(0, None, Vec::new())]);
            let mut fields = vec![sub(b"EDID", z("OwnedNonFunction"))];
            fields.extend(unit(0, &ordinary, &[], &[]));
            bytes.extend(top(b"SCPT", &record(b"SCPT", 0, local(NON_FUNCTION), &fields)));
            bytes.extend(top(b"NPC_", &record(b"NPC_", 0, local(OCCUPIED_NPC), &[sub(b"EDID", z("OwnedOccupiedNpc"))])));
        }
        plugins.push(Plugin::parse(&bytes, format!("OwnedSessionSlot{index:03}.esm")).unwrap());
    }
    let order = LoadOrder::new(&plugins).unwrap();
    assert_eq!(order.to_global(254, 0x0180_0000), Some(HIGH));
    assert_eq!(order.to_global(254, RECORD_COUNT), Some(RECORD_COUNT), "low globals resolve through the real master slot");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let json = serde_json::from_str(&std::fs::read_to_string(catalogue).unwrap()).unwrap();
    let data = GameData::build(&order, &schema, table, &json, &refs);
    for id in [HIGH, PP_HIGH, DROP_HIGH] {
        let c = data.script(FormId(id)).expect("the high SCPT has an enabled real dispatch entry");
        assert_eq!(c.unit.owner, fnv_script::UnitOwner::Script { form: id });
        assert_eq!(c.unit.plugin, "OwnedSessionSlot254.esm");
        assert!(c.unit.has_scda && c.unit.source.is_some());
        assert!(c.program.blocks.iter().any(|b| b.event == fnv_script::ir::FUNCTION_BLOCK));
        assert!(c.unit.refs.iter().all(|r| !matches!(r, RefSlot::Form(f) if *f >= 0x0100_0000)), "references still name the low master globals: {:?}", c.unit.refs);
    }
    assert_eq!(data.sig(FormId(OCCUPIED_NPC)), Some(*b"NPC_"));
    assert!(!data.script(FormId(NON_FUNCTION)).unwrap().program.blocks.iter().any(|b| b.event == fnv_script::ir::FUNCTION_BLOCK));
    Fixture { data, forms, refs }
}
fn game(f: &Fixture) -> Game<'_> { Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() }) }
fn invoke(g: &mut Game<'_>, f: &XVal) -> XVal {
    g.run_function(f, Some(Value::Ref(FormId::PLAYER_REF.0)), vec![], None, RunCtx { self_ref: Some(FormId::PLAYER_REF), ..Default::default() }).expect("authored callable executes")
}
fn intrinsic(g: &mut Game<'_>, name: &str, args: &[XVal]) -> XVal {
    fnv_game::nvse::call(g, &RunCtx::default(), name, Some(FormId::PLAYER_REF), args).expect("public intrinsic exists").expect("public intrinsic succeeds")
}
fn count(g: &Game<'_>, id: u32) -> f32 { g.world.global(FormId(id)).unwrap() }
fn reset(g: &mut Game<'_>) { for (id, _) in GLOBALS { g.world.set_global(FormId(id), 0.0).unwrap(); } }
fn admitted(g: &mut Game<'_>, high: u32) -> XVal {
    assert_eq!(invoke(g, &XVal::Ref(high)), XVal::Num(0.0));
    assert_eq!(invoke(g, &XVal::Ref(LOW)), XVal::Num(0.0));
    let lambda = invoke(g, &XVal::Ref(MAKER));
    assert_eq!(lambda, XVal::Ref(HIGH + 1), "actual allocation skips loaded high records");
    assert_eq!(invoke(g, &lambda), XVal::Num(0.0));
    assert_eq!((count(g, RECORD_COUNT), count(g, LAMBDA_COUNT), count(g, LOW_COUNT)), (1.0, 1.0, 1.0));
    reset(g);
    lambda
}
fn transfer<'a>(f: &'a Fixture, g: &mut Game<'_>) -> Game<'a> {
    let snapshot = g.checked_snapshot().unwrap();
    let session = g.take_session();
    let world = World::restore(&f.forms, &f.refs, &snapshot.sim).unwrap();
    let mut r = Game::restore(&f.data, world, GameConfig { strict: true, ..Default::default() }, &snapshot).unwrap();
    r.put_session(session);
    r
}
fn clean(g: &Game<'_>) {
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::ScriptError { .. } | EventKind::CallFailed { .. } | EventKind::Skipped { .. })), "{:?}", g.events);
}
fn set(g: &mut Game<'_>, event: &str, func: XVal, alt: bool, pairs: Vec<(XVal, XVal)>) {
    assert_eq!(g.event_call(EventCall::Set { event: event.into(), func, pairs, alt }, "OwnedSessionSlot000.esm").unwrap(), XVal::Num(1.0));
}
fn listed(g: &mut Game<'_>, event: &str, priority: i32) -> Vec<(XVal, Vec<(XKey, XVal)>)> {
    let XVal::Array(id) = intrinsic(g, "geteventhandlers", &[XVal::Str(event.into()), XVal::Num(0.0), XVal::Num(f64::from(priority))]) else { panic!("handler metadata must be an array") };
    if id == 0 { return Vec::new(); }
    let entries = g.ext.array(id).unwrap().entries.clone();
    entries.into_iter().map(|(_, h)| {
        let XVal::Array(h) = h else { panic!("handler entry") };
        let fields = &g.ext.array(h).unwrap().entries;
        let func = fields[0].1.clone();
        let XVal::Array(filters) = &fields[1].1 else { panic!("filter metadata") };
        (func, g.ext.array(*filters).unwrap().entries.clone())
    }).collect()
}

#[test]
fn ordinary_handlers_keep_high_record_filters_and_drop_restored_lambda_and_alt() {
    // This one leaf changes process-global file roots. Canonical integration
    // runs may have concurrent readers: isolate the unchanged body in one exact
    // child process rather than relying on teardown in the parent's address space.
    const LEAF: &str = "high_fe_session_consumers::ordinary_handlers_keep_high_record_filters_and_drop_restored_lambda_and_alt";
    const CHILD: &str = "NVF_GAME_TEST_HIGH_FE_OWNED_ROOTS_CHILD";
    if std::env::var(CHILD).ok().as_deref() != Some("owned-one-leaf") {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child.args([LEAF, "--exact", "--nocapture", "--test-threads=1"]).env(CHILD, "owned-one-leaf");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            child.creation_flags(0x0800_0000); // CREATE_NO_WINDOW; captured headless test.
        }
        let output = child.output().expect("spawn the isolated owned-roots leaf");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let expected_row = format!("test {LEAF} ... ok");
        let rows: Vec<_> = stdout.lines().filter(|line| line.starts_with("test ") && line.contains(" ... ")).collect();
        let summaries: Vec<_> = stdout.lines().filter(|line| line.starts_with("test result:")).collect();
        let one_pass = summaries.len() == 1 && summaries[0].starts_with("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;");
        let prefixed = |bytes: &[u8]| String::from_utf8_lossy(bytes).lines().map(|line| format!("owned-child | {line}")).collect::<Vec<_>>().join("\n");
        assert!(output.status.success() && rows == [expected_row.as_str()] && one_pass,
            "isolated owned-roots leaf failed or did not execute exactly once: {:?}\n{}\n{}",
            output.status, prefixed(&output.stdout), prefixed(&output.stderr));
        return;
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(args.iter().any(|s| s == LEAF) && args.iter().any(|s| s == "--exact") && args.iter().any(|s| s == "--test-threads=1"),
        "the owned-roots marker is valid only for the exact serial child");
    let f = fixture();
    // An actual CompileScript token is an eligibility neighbor, not a claim that
    // the new Game restores its transient compiled-file cache. Restore prior roots.
    struct RestoreRoots(Option<fnv_game::mod_files::Roots>);
    impl Drop for RestoreRoots {
        fn drop(&mut self) { if let Some(roots) = self.0.take() { fnv_game::mod_files::set_roots(roots); } }
    }
    let reader: fnv_game::mod_files::Reader = std::sync::Arc::new(|path| {
        path.eq_ignore_ascii_case("nvse\\user_defined_functions\\owned-session.txt").then(|| b"Begin Function {}\nSetFunctionValue 0\nEnd\n".to_vec())
    });
    let mut previous = None;
    fnv_game::mod_files::update_roots(|roots| previous = Some(std::mem::replace(roots, fnv_game::mod_files::Roots { reader: Some(reader), ..Default::default() })));
    let _restore_roots = RestoreRoots(previous);
    let mut g = game(&f);
    let lambda = admitted(&mut g, HIGH);
    let compiled = intrinsic(&mut g, "compilescript", &[XVal::Str("owned-session.txt".into())]);
    assert_eq!(compiled, XVal::Ref(0xFE20_0000));
    assert_eq!(invoke(&mut g, &compiled), XVal::Num(0.0), "the token was actually compiled and callable before transfer");
    let pairs = vec![(XVal::Str("priority".into()), XVal::Num(23.0)), (XVal::Str("first".into()), XVal::Num(13.0))];
    set(&mut g, "OwnedKeep", XVal::Ref(HIGH), false, pairs);
    set(&mut g, "OwnedLambda", lambda.clone(), false, vec![]);
    set(&mut g, "OwnedAlt", XVal::Ref(HIGH), true, vec![]);
    set(&mut g, "OwnedLow", XVal::Ref(LOW), false, vec![]);
    // Eligibility is intentionally unchanged for invalid low Ref tokens; no
    // callable success is claimed for this separate metadata-only neighbor.
    assert!(f.data.script(FormId(0x777)).is_none());
    set(&mut g, "OwnedInvalidLow", XVal::Ref(0x777), false, vec![]);
    set(&mut g, "OwnedCompiled", compiled.clone(), false, vec![]);
    set(&mut g, "OwnedNonFunction", XVal::Ref(NON_FUNCTION), false, vec![]);
    set(&mut g, "OwnedHighNpc", XVal::Ref(OCCUPIED_NPC), false, vec![]);
    set(&mut g, "OwnedNumeric", XVal::Num(f64::from(HIGH)), false, vec![]);
    assert_eq!(g.dispatch_event("OwnedKeep", vec![XVal::Num(12.0)], Some(FormId::PLAYER_REF)), 0);
    assert_eq!(g.dispatch_event("OwnedKeep", vec![XVal::Num(13.0)], Some(FormId::PLAYER_REF)), 1);
    assert_eq!(g.dispatch_event("OwnedLambda", vec![], Some(FormId::PLAYER_REF)), 1);
    assert_eq!((count(&g, RECORD_COUNT), count(&g, LAMBDA_COUNT), count(&g, CONTEXT)), (1.0, 1.0, 20.0));
    reset(&mut g);
    let mut r = transfer(&f, &mut g);
    assert_eq!(listed(&mut r, "OwnedKeep", 23), vec![(XVal::Ref(HIGH), vec![(XKey::Str("first".into()), XVal::Num(13.0))])]);
    assert!(listed(&mut r, "OwnedLambda", 1).is_empty());
    assert!(listed(&mut r, "OwnedAlt", 1).is_empty());
    assert!(listed(&mut r, "OwnedNumeric", 1).is_empty());
    assert!(listed(&mut r, "OwnedHighNpc", 1).is_empty());
    assert_eq!(listed(&mut r, "OwnedInvalidLow", 1)[0].0, XVal::Ref(0x777));
    assert_eq!(listed(&mut r, "OwnedCompiled", 1)[0].0, compiled);
    assert_eq!(listed(&mut r, "OwnedNonFunction", 1)[0].0, XVal::Ref(NON_FUNCTION), "eligibility follows real SCPT presence, not Function-block validation");
    assert_eq!(r.dispatch_event("OwnedKeep", vec![XVal::Num(12.0)], Some(FormId::PLAYER_REF)), 0);
    assert_eq!(r.dispatch_event("OwnedKeep", vec![XVal::Num(13.0)], Some(FormId::PLAYER_REF)), 1);
    assert_eq!(r.dispatch_event("OwnedLambda", vec![], Some(FormId::PLAYER_REF)), 0);
    assert_eq!(r.dispatch_event("OwnedAlt", vec![], Some(FormId::PLAYER_REF)), 0);
    assert_eq!(r.dispatch_event("OwnedLow", vec![], Some(FormId::PLAYER_REF)), 1);
    assert_eq!((count(&r, RECORD_COUNT), count(&r, LAMBDA_COUNT), count(&r, LOW_COUNT), count(&r, CONTEXT)), (1.0, 0.0, 1.0, 20.0));
    assert_eq!(invoke(&mut r, &lambda), XVal::Num(0.0), "the excluded lambda is actually restored and callable");
    assert_eq!(count(&r, LAMBDA_COUNT), 1.0);
    clean(&r);
}

#[test]
fn jip_loop_keeps_high_record_delay_mode_and_drops_flag8_and_restored_lambda() {
    let f = fixture();
    let mut g = game(&f);
    let lambda = admitted(&mut g, HIGH);
    assert_eq!(invoke(&mut g, &XVal::Ref(DROP_HIGH)), XVal::Num(0.0));
    reset(&mut g);
    g.frame = 17;
    for (func, flags) in [(XVal::Ref(HIGH), 1), (lambda.clone(), 1), (XVal::Ref(LOW), 1), (XVal::Ref(DROP_HIGH), 9), (XVal::Ref(PP_HIGH), 2)] {
        g.event_call(EventCall::MainLoop { func, add: true, delay: 3, flags, on: Some(Value::Ref(FormId::PLAYER_REF.0)) }, "OwnedSessionSlot000.esm").unwrap();
    }
    let mut r = transfer(&f, &mut g);
    r.tick();
    assert_eq!((r.frame, count(&r, RECORD_COUNT), count(&r, LOW_COUNT), count(&r, DROP_COUNT), count(&r, LAMBDA_COUNT), count(&r, CONTEXT)), (18, 1.0, 1.0, 0.0, 0.0, 20.0), "retained due resets to0; mode2 and flag8 do not run in game mode");
    r.frames(2);
    assert_eq!((count(&r, RECORD_COUNT), count(&r, LOW_COUNT)), (1.0, 1.0), "retained delay is three frames");
    r.menu = Some(1001);
    r.tick();
    assert_eq!((count(&r, RECORD_COUNT), count(&r, LOW_COUNT)), (2.0, 1.0), "only the retained menu-mode record runs");
    r.menu = None;
    r.tick();
    assert_eq!((count(&r, RECORD_COUNT), count(&r, LOW_COUNT), count(&r, DROP_COUNT), count(&r, LAMBDA_COUNT)), (3.0, 2.0, 0.0, 0.0));
    assert_eq!(invoke(&mut r, &lambda), XVal::Num(0.0));
    assert_eq!(count(&r, LAMBDA_COUNT), 1.0, "lambda body was valid but its old loop was excluded");
    clean(&r);
}

#[test]
fn delayed_kept_call_preserves_high_record_context_remaining_frames_and_once_only() {
    let f = fixture();
    let mut g = game(&f);
    let lambda = admitted(&mut g, HIGH);
    g.frame = 40;
    for (func, remove) in [(XVal::Ref(HIGH), 0.0), (lambda.clone(), 0.0), (XVal::Ref(LOW), 0.0), (XVal::Ref(DROP_HIGH), 1.0)] {
        intrinsic(&mut g, "callfunctionnextframe", &[func, XVal::Num(5.0), XVal::Num(remove)]);
    }
    g.frames(2);
    assert_eq!((g.frame, count(&g, RECORD_COUNT), count(&g, LAMBDA_COUNT), count(&g, DROP_COUNT)), (42, 0.0, 0.0, 0.0));
    let mut r = transfer(&f, &mut g);
    r.frames(2);
    assert_eq!((r.frame, count(&r, RECORD_COUNT), count(&r, LOW_COUNT)), (44, 0.0, 0.0));
    r.tick();
    assert_eq!((r.frame, count(&r, RECORD_COUNT), count(&r, LOW_COUNT), count(&r, LAMBDA_COUNT), count(&r, DROP_COUNT), count(&r, CONTEXT)), (45, 1.0, 1.0, 0.0, 0.0, 20.0));
    r.tick();
    assert_eq!((count(&r, RECORD_COUNT), count(&r, LOW_COUNT), count(&r, LAMBDA_COUNT), count(&r, DROP_COUNT)), (1.0, 1.0, 0.0, 0.0), "retained calls run once, removeOnLoad1 and lambdas stay gone");
    assert_eq!(invoke(&mut r, &lambda), XVal::Num(0.0));
    assert_eq!(count(&r, LAMBDA_COUNT), 1.0);
    clean(&r);
}

#[test]
fn pp_reconstruct_and_item_mod_consumers_keep_high_record_and_exact_metadata() {
    let f = fixture();
    let mut g = game(&f);
    let lambda = admitted(&mut g, PP_HIGH);
    let XVal::Ref(instance) = intrinsic(&mut g, "createforminstance", &[XVal::Ref(WEAPON), XVal::Str("owned-key".into())]) else { panic!("real PP instance") };
    assert!(g.pp.instances.contains_key(&FormId(instance)));
    for func in [XVal::Ref(PP_HIGH), lambda.clone()] {
        intrinsic(&mut g, "setoninstancereconstruct", &[XVal::Num(17.0), func.clone(), XVal::Str("owned-key".into())]);
        intrinsic(&mut g, "onattachitemmod", &[XVal::Num(29.0), func, XVal::Num(1.0), XVal::Str("1".into()), XVal::Ref(ITEM_MOD), XVal::Str("2".into()), XVal::Ref(WEAPON)]);
    }
    let expected_instance = vec![g.pp.handlers[0].clone()];
    let expected_mod = vec![g.pp.mod_handlers[0].clone()];
    assert_eq!(expected_instance[0].func, XVal::Ref(PP_HIGH));
    assert_eq!((expected_instance[0].priority, expected_instance[0].reconstruct, expected_instance[0].filter.as_deref()), (17, true, Some("owned-key")));
    assert_eq!((expected_mod[0].priority, expected_mod[0].attach, expected_mod[0].on_rebuild, expected_mod[0].item_mod, expected_mod[0].object), (29, true, true, Some(FormId(ITEM_MOD)), Some(FormId(WEAPON))));
    let mut r = transfer(&f, &mut g);
    assert_eq!(r.pp.handlers, expected_instance);
    assert_eq!(r.pp.mod_handlers, expected_mod);
    assert!(r.pp.instances.contains_key(&FormId(instance)), "callback returned0 and kept the loaded instance");
    assert_eq!((count(&r, RECORD_COUNT), count(&r, LAMBDA_COUNT)), (1.0, 0.0), "put_session ran the actual reconstruct handler, excluding the restored lambda");
    r.pp.slots.insert("owned-slot".into());
    assert_eq!(intrinsic(&mut r, "attachmod", &[XVal::Ref(WEAPON), XVal::Str("owned-slot".into()), XVal::Ref(OTHER_MOD)]), XVal::Num(1.0));
    assert_eq!(count(&r, RECORD_COUNT), 1.0, "wrong item mod is filtered out");
    assert_eq!(intrinsic(&mut r, "attachmod", &[XVal::Ref(WEAPON), XVal::Str("owned-slot".into()), XVal::Ref(ITEM_MOD)]), XVal::Num(1.0));
    assert_eq!((count(&r, RECORD_COUNT), count(&r, LAMBDA_COUNT)), (2.0, 0.0));
    assert_eq!(invoke(&mut r, &lambda), XVal::Num(0.0));
    assert_eq!(count(&r, LAMBDA_COUNT), 1.0);
    clean(&r);
}
