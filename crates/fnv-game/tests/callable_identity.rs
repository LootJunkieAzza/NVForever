//! Portable loaded records exercise callback identity through source and the real hit recorder.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat::HitInfo;
use fnv_game::game::{event, GameSnapshot, InstKey, RunCtx, COMPILED_BASE, LAMBDA_BASE};
use fnv_game::trace::EventKind;
use fnv_game::{Game, GameConfig, GameData};
use fnv_script::vm::{Value, XVal};
use fnv_script::{FunctionTable, VarType};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const HIT: FormId = FormId(0xFEF0_0440);
const REGISTRAR: FormId = FormId(0xFEF0_0430);
const QUEST: FormId = FormId(0xFEF0_0431);
const MAKER: FormId = FormId(0x0000_0900);
const TARGET: FormId = FormId(0x0000_0903);
const COUNT: FormId = FormId(0x800);
const WHO: FormId = FormId(0x801);
const BY: FormId = FormId(0x802);
const DAMAGE: FormId = FormId(0x803);
const CAPTURED: FormId = FormId(0x804);
const ORDINARY: FormId = FormId(0x905);
const RECEIVER_BODY: FormId = FormId(0x906);
const RECEIVER_DRIVER: FormId = FormId(0x907);
const ARRAY_BODY: FormId = FormId(0xFEF0_0450);
const RECEIVER_HIGH: FormId = FormId(0xFEF0_0451);

struct Env;
impl fnv_script::compile::Env for Env {
    fn form(&self, name: &str) -> Option<fnv_script::compile::FormInfo> {
        let (form, sig) = match name.to_ascii_lowercase().as_str() {
            "ownedhit" => (HIT.0, *b"SCPT"),
            "ownedreceiverbody" => (RECEIVER_BODY.0, *b"SCPT"),
            "hitcount" => (COUNT.0, *b"GLOB"),
            "hitwho" => (WHO.0, *b"GLOB"),
            "hitby" => (BY.0, *b"GLOB"),
            "hitdamage" => (DAMAGE.0, *b"GLOB"),
            "capturedcount" => (CAPTURED.0, *b"GLOB"),
            _ => return None,
        };
        Some(fnv_script::compile::FormInfo { form, sig })
    }
    fn info(&self, _: u32) -> Option<fnv_script::compile::FormInfo> {
        None
    }
    fn editor_id(&self, _: u32) -> Option<String> {
        None
    }
    fn local_form(&self, _: u32) -> Option<u32> {
        None
    }
    fn script_vars(&self, _: u32) -> Option<Vec<fnv_script::VarDecl>> {
        None
    }
}
fn script(table: &FunctionTable, id: u32, name: &str, source: &str, quest: bool) -> Vec<u8> {
    script_source_mode(table, id, name, source, quest, true)
}
fn script_source_mode(
    table: &FunctionTable,
    id: u32,
    name: &str,
    source: &str,
    quest: bool,
    include_source: bool,
) -> Vec<u8> {
    let c = fnv_script::compile::compile(source, table, &Env, None);
    assert!(
        c.errors().next().is_none(),
        "owned script {name}: {:?}",
        c.diags
    );
    let vars: Vec<_> = c
        .variables
        .iter()
        .map(|v| {
            // The accepted SLSD enumeration keeps the disposal control typed as string.
            let kind = match &v.ty {
                VarType::Int => 1,
                VarType::Other(t) if t == "string_var" => 2,
                VarType::Other(t) if t == "array_var" => 3,
                _ => 0,
            };
            (v.index, v.name.as_str(), kind)
        })
        .collect();
    let mut script_unit = unit(u16::from(quest), &c.bytes, &vars, &[]);
    script_unit[0].data[4..8].copy_from_slice(&(c.refs.len() as u32).to_le_bytes());
    // EDID is record metadata, not part of the contiguous SCHR script unit.
    // Keep SCTX with that unit so the real reader can attach its source.
    let mut sr = vec![sub(b"EDID", z(name))];
    sr.extend(script_unit);
    for r in &c.refs {
        sr.push(match r {
            fnv_script::unit::RefSlot::Form(f) | fnv_script::unit::RefSlot::Unmapped(f) => {
                sub(b"SCRO", f.to_le_bytes().to_vec())
            }
            fnv_script::unit::RefSlot::Local(v) => sub(b"SCRV", v.to_le_bytes().to_vec()),
        });
    }
    if include_source {
        sr.push(sub(b"SCTX", source.as_bytes().to_vec()));
    }
    record(b"SCPT", 0, id, &sr)
}
struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}
fn fixture(collisions: bool) -> Fixture {
    fixture_source_mode(collisions, true)
}
fn fixture_source_mode(collisions: bool, include_source: bool) -> Fixture {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let path = root.join("script/functions.json");
    let table = FunctionTable::load(&path).unwrap();
    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let globals = [
        (COUNT, "HitCount"),
        (WHO, "HitWho"),
        (BY, "HitBy"),
        (DAMAGE, "HitDamage"),
        (CAPTURED, "CapturedCount"),
    ]
    .into_iter()
    .flat_map(|(id, name)| {
        record(
            b"GLOB",
            0,
            id.0,
            &[
                sub(b"EDID", z(name)),
                sub(b"FNAM", vec![b'f']),
                sub(b"FLTV", 0f32.to_le_bytes().to_vec()),
            ],
        )
    })
    .collect::<Vec<_>>();
    let npcs = [(FormId::PLAYER_BASE.0, "Player"), (0x901, "OwnedActor")]
        .into_iter()
        .flat_map(|(id, name)| {
            record(
                b"NPC_",
                0,
                id,
                &[
                    sub(b"EDID", z(name)),
                    sub(
                        b"DATA",
                        [500i32.to_le_bytes().to_vec(), vec![5; 7]].concat(),
                    ),
                ],
            )
        })
        .collect::<Vec<_>>();
    let placed = record(
        b"ACHR",
        0,
        TARGET.0,
        &[
            sub(b"EDID", z("OwnedTarget")),
            sub(b"NAME", 0x901u32.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let cell = record(
        b"CELL",
        0,
        0x902,
        &[sub(b"EDID", z("OwnedCell")), sub(b"DATA", vec![1])],
    );
    let cells = group(
        0i32.to_le_bytes(),
        2,
        &group(
            0i32.to_le_bytes(),
            3,
            &[
                cell,
                group(
                    0x902u32.to_le_bytes(),
                    6,
                    &group(0x902u32.to_le_bytes(), 9, &placed),
                ),
            ]
            .concat(),
        ),
    );
    let maker = script(
        &table,
        MAKER.0,
        "OwnedMaker",
        "scn OwnedMaker\nint n\nBegin Function {}\nlet n := 40\nSetFunctionValue (Begin Function {}\nlet n := n + 1\nlet CapturedCount := n\nEnd)\nEnd\n",
        false,
    );
    let ordinary = script(
        &table,
        ORDINARY.0,
        "OwnedEcho",
        "scn OwnedEcho\nint n\nBegin Function {n}\nSetFunctionValue n + GetSelf\nEnd\n",
        false,
    );
    // The unused typed local selects the accepted Auto source route when SCTX
    // exists. The source-less variant keeps the same compiled body and locals.
    let receiver_body = script_source_mode(
        &table,
        RECEIVER_BODY.0,
        "OwnedReceiverBody",
        "scn OwnedReceiverBody\nstring_var ownedSourceAdmission\nBegin Function {}\nlet HitCount := HitCount + 1\nSetFunctionValue GetSelf\nEnd\n",
        false,
        include_source,
    );
    let receiver_driver = script_source_mode(
        &table,
        RECEIVER_DRIVER.0,
        "OwnedReceiverDriver",
        "scn OwnedReceiverDriver\nref cb\nref receiver\nint explicit\nfloat result\nstring_var ownedSourceAdmission\nBegin Function {cb receiver explicit}\nif explicit\nlet result := receiver.Call cb\nelse\nlet result := Call cb\nendif\nlet HitDamage := 99\nSetFunctionValue result + 0\nEnd\n",
        false,
        include_source,
    );
    let first = [
        tes4(),
        top(b"GLOB", &globals),
        top(b"NPC_", &npcs),
        top(b"CELL", &cells),
        top(
            b"SCPT",
            &[maker, ordinary, receiver_body, receiver_driver].concat(),
        ),
    ]
    .concat();
    let mut plugins = vec![Plugin::parse(&first, "OwnedBase.esm".into()).unwrap()];
    for i in 1..254 {
        plugins.push(Plugin::parse(&tes4(), format!("OwnedPadding{i:03}.esm")).unwrap());
    }
    let mut header = vec![sub(
        b"HEDR",
        [
            1.34f32.to_le_bytes(),
            0u32.to_le_bytes(),
            0x00f00500u32.to_le_bytes(),
        ]
        .concat(),
    )];
    for p in &plugins {
        header.extend([sub(b"MAST", z(&p.name)), sub(b"DATA", vec![0; 8])]);
    }
    let mut scripts = vec![
        script_source_mode(
            &table,
            RECEIVER_HIGH.0,
            "OwnedReceiverHigh",
            "scn OwnedReceiverHigh\nstring_var ownedSourceAdmission\nBegin Function {}\nlet HitCount := HitCount + 1\nSetFunctionValue GetSelf\nEnd\n",
            false,
            include_source,
        ),
        script(
            &table,
            ARRAY_BODY.0,
            "OwnedArrayBody",
            "scn OwnedArrayBody\narray_var scratch\nstring_var text\nref empty\nint mode\nBegin Function {mode}\nlet scratch := Ar_List 4, 9\nlet text := sv_Construct \"portable\"\nif mode == 1\nSetFunctionValue scratch\nelseif mode == 2\nempty.Call OwnedReceiverBody\nendif\nEnd\n",
            false,
        ),
        script(
            &table,
            HIT.0,
            "OwnedHit",
            "scn OwnedHit\nstring_var scratch\nBegin Function {}\nlet scratch := sv_Construct \"portable\"\nlet HitCount := HitCount + 1\nlet HitWho := GetSelf\nlet HitBy := GetHitAttacker\nlet HitDamage := GetHitHealthDamage\nEnd\n",
            false,
        ),
        script(
            &table,
            REGISTRAR.0,
            "OwnedRegistrar",
            "scn OwnedRegistrar\nBegin GameMode\nSetOnHitEventHandler OwnedHit 1\nEnd\n",
            true,
        ),
    ];
    if collisions {
        for (id, name, value) in [
            (LAMBDA_BASE, "OwnedLambdaSlot", 71),
            (COMPILED_BASE, "OwnedCompiledSlot", 83),
        ] {
            scripts.push(script(
                &table,
                id,
                name,
                &format!("scn {name}\nBegin Function {{}}\nSetFunctionValue {value}\nEnd\n"),
                false,
            ));
        }
    }
    let quest = record(
        b"QUST",
        0,
        QUEST.0,
        &[
            sub(b"EDID", z("OwnedRegistrarQuest")),
            sub(b"SCRI", REGISTRAR.0.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0, 50]),
        ],
    );
    let last = [
        record(b"TES4", 0, 0, &header),
        top(b"SCPT", &scripts.concat()),
        top(b"QUST", &quest),
    ]
    .concat();
    plugins.push(Plugin::parse(&last, "OwnedCallbacks.esp".into()).unwrap());
    let lo = LoadOrder::new(&plugins).unwrap();
    assert_eq!(lo.to_global(254, HIT.0), Some(HIT.0));
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &json, &refs);
    assert_eq!(data.sig(HIT), Some(*b"SCPT"));
    assert!(data.script(HIT).is_some());
    Fixture { data, forms, refs }
}
fn game(f: &Fixture) -> Game<'_> {
    Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, SimConfig::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    )
}
fn q(g: &mut Game<'_>, body: &str) -> XVal {
    g.run_query(
        &format!("Begin Function {{}}\n{body}\nEnd\n"),
        RunCtx::default(),
    )
    .unwrap_or_else(|e| panic!("owned source {body}: {e}"))
}
fn value(g: &Game<'_>, id: FormId) -> f64 {
    f64::from(g.world.global(id).unwrap())
}
fn hit(g: &mut Game<'_>, on: FormId, damage: f64) {
    let result = g.hit(
        on,
        damage,
        HitInfo {
            attacker: Some(FormId::PLAYER_REF),
            weapon: None,
            location: 0,
            critical: true,
            limb: None,
        },
    );
    assert!(
        (result.dealt - damage).abs() < 0.001,
        "real Game hit must deal its damage"
    );
}
fn no_script_errors(g: &Game<'_>) {
    assert!(
        !g.events
            .iter()
            .any(|e| matches!(e.kind, EventKind::ScriptError { .. })),
        "callback failure: {:?}",
        g.events
    );
}
fn register_record(g: &mut Game<'_>) {
    assert!(g.run_event(
        InstKey::Quest { quest: QUEST },
        REGISTRAR,
        event::GAME_MODE,
        None,
        RunCtx {
            quest: Some(QUEST),
            ..Default::default()
        }
    ));
}

#[test]
fn last_slot_record_registration_runs_on_real_hit_receiver_without_becoming_a_lambda() {
    let f = fixture(true);
    let mut g = game(&f);
    register_record(&mut g);
    for (i, on, damage) in [
        (1, FormId::PLAYER_REF, 17.0),
        (2, TARGET, 23.0),
        (3, TARGET, 11.0),
    ] {
        hit(&mut g, on, damage);
        assert_eq!(value(&g, COUNT), f64::from(i));
        assert_eq!(value(&g, WHO), f64::from(on.0));
        assert_eq!(value(&g, BY), f64::from(FormId::PLAYER_REF.0));
        assert_eq!(value(&g, DAMAGE), damage);
        assert!(
            g.ext.is_empty(),
            "record UDF temporary strings must be disposed, including high IDs"
        );
    }
    no_script_errors(&g);
    assert_eq!(
        g.run_function(
            &XVal::Ref(LAMBDA_BASE),
            None,
            vec![],
            None,
            Default::default()
        )
        .unwrap(),
        XVal::Num(71.0)
    );
    assert_eq!(
        q(&mut g, "SetFunctionValue (EvaluateString \"1 == 1\")"),
        XVal::Num(1.0)
    );
    assert_eq!(
        g.run_function(
            &XVal::Ref(COMPILED_BASE),
            None,
            vec![],
            None,
            Default::default()
        )
        .unwrap(),
        XVal::Num(83.0)
    );
}

#[test]
fn live_lambda_collision_avoidance_keeps_captures_and_saved_handle_identity() {
    let f = fixture(true);
    let mut g = game(&f);
    register_record(&mut g);
    let callback = q(
        &mut g,
        "ref cb\nlet cb := Call OwnedMaker\nSetOnHitEventHandler cb 1\nSetFunctionValue cb",
    );
    let XVal::Ref(handle) = callback else {
        panic!("owned lambda handle")
    };
    assert_ne!(handle, LAMBDA_BASE);
    hit(&mut g, TARGET, 7.0);
    assert_eq!(value(&g, CAPTURED), 41.0);
    hit(&mut g, TARGET, 9.0);
    assert_eq!(value(&g, CAPTURED), 42.0);
    assert_eq!(value(&g, COUNT), 2.0);
    no_script_errors(&g);
    let snap = g.snapshot();
    let json = serde_json::to_value(&snap).unwrap();
    assert_eq!(json["format"], "fnv.game.state/2-lambda-handles");
    assert_eq!(json["lambdas"][0]["handle"], handle);
    let mut restored = Game::restore(
        &f.data,
        World::new(&f.forms, &f.refs, Default::default()),
        Default::default(),
        &snap,
    )
    .unwrap();
    assert_eq!(
        q(&mut restored, "SetFunctionValue (Call OwnedMaker)"),
        XVal::Ref(handle)
    );
    let before = value(&restored, COUNT);
    hit(&mut restored, TARGET, 5.0);
    assert_eq!(
        value(&restored, COUNT),
        before,
        "JIP hit registrations do not persist with the snapshot"
    );
    register_record(&mut restored);
    hit(&mut restored, TARGET, 6.0);
    assert_eq!(value(&restored, COUNT), before + 1.0);
    no_script_errors(&restored);
}

#[test]
fn legacy_lambda_snapshots_and_high_record_session_callbacks_keep_existing_lifetimes() {
    let f = fixture(false);
    let mut controls = game(&f);
    q(&mut controls, "SetGameMainLoopCallback OwnedReceiverBody 1 1\nSetGameMainLoopCallback (Begin Function {}\nlet CapturedCount := 9\nEnd) 1 1 3\nPlayerREF.SetGameMainLoopCallback OwnedReceiverBody 1 1 0\nSetGameMainLoopCallback (Begin Function {}\nlet CapturedCount := 99\nEnd) 1 1 0");
    controls.tick();
    assert_eq!(
        value(&controls, COUNT),
        1.0,
        "default flags run once; explicit disabled flags stay disabled"
    );
    assert_eq!(
        value(&controls, CAPTURED),
        9.0,
        "explicit mode3 runs; disabled lambda does not overwrite it"
    );
    no_script_errors(&controls);
    let mut g = game(&f);
    let callback = q(&mut g, "SetFunctionValue (Call OwnedMaker)");
    assert_eq!(callback, XVal::Ref(LAMBDA_BASE));
    q(
        &mut g,
        "SetEventHandler \"owned_event\" OwnedHit\nSetEventHandler \"owned_lambda\" (Begin Function {}\nlet CapturedCount := 99\nEnd)\nCallFunctionNextFrame OwnedHit 1 0\nSetGameMainLoopCallback OwnedHit 1 1 3\nSetGameMainLoopCallback (Begin Function {}\nlet CapturedCount := 100\nEnd) 1 1 3",
    );
    let session = g.take_session();
    let snap = g.snapshot();
    let mut json = serde_json::to_value(&snap).unwrap();
    json["format"] = "fnv.game.state/1".into();
    for item in json["lambdas"].as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("handle");
    }
    let legacy: GameSnapshot = serde_json::from_value(json).unwrap();
    let mut restored = Game::restore(
        &f.data,
        World::new(&f.forms, &f.refs, Default::default()),
        Default::default(),
        &legacy,
    )
    .unwrap();
    restored.put_session(session);
    assert_eq!(
        q(&mut restored, "SetFunctionValue (Call OwnedMaker)"),
        callback
    );
    assert_eq!(
        restored.dispatch_event("owned_lambda", vec![], Some(TARGET)),
        0,
        "old-game lambda event callbacks are not session functions"
    );
    assert_eq!(
        restored.dispatch_event("owned_event", vec![], Some(TARGET)),
        1
    );
    let before = value(&restored, COUNT);
    restored.tick();
    assert_eq!(
        value(&restored, COUNT),
        before + 2.0,
        "kept deferred and main-loop high-record callbacks run after restore"
    );
    assert_eq!(
        value(&restored, CAPTURED),
        0.0,
        "old-game main-loop lambdas do not transfer with the session"
    );
    no_script_errors(&restored);
}

#[test]
fn record_filter_receiver_and_ordinary_function_neighbors_keep_their_admission() {
    let f = fixture(true);
    let mut g = game(&f);
    q(
        &mut g,
        "ref target\nlet target := StringToRef \"00000903\"\nSetOnHitEventHandler OwnedHit 1 target",
    );
    hit(&mut g, FormId::PLAYER_REF, 3.0);
    assert_eq!(value(&g, COUNT), 0.0);
    hit(&mut g, TARGET, 4.0);
    assert_eq!(value(&g, COUNT), 1.0);
    assert_eq!(value(&g, WHO), f64::from(TARGET.0));
    let ctx = RunCtx {
        self_ref: Some(TARGET),
        ..Default::default()
    };
    assert_eq!(
        g.run_function(
            &XVal::Ref(ORDINARY.0),
            None,
            vec![XVal::Num(2.0)],
            None,
            ctx
        )
        .unwrap(),
        XVal::Num(f64::from(TARGET.0) + 2.0)
    );
    assert_eq!(
        g.run_function(
            &XVal::Ref(ORDINARY.0),
            Some(Value::Ref(0)),
            vec![XVal::Num(2.0)],
            None,
            ctx
        )
        .unwrap_err(),
        "call called on no reference",
        "explicit empty receiver must stop before entering the UDF"
    );
    assert_eq!(
        g.run_function(&XVal::Ref(0xFEF0_0999), None, vec![], None, ctx)
            .unwrap_err(),
        "an unknown lambda",
        "unresolved high handles keep their existing diagnostic"
    );
    no_script_errors(&g);
}

#[test]
fn snapshot_handle_collisions_are_rejected_without_retargeting_loaded_records() {
    let f = fixture(true);
    let mut g = game(&f);
    let callback = q(&mut g, "SetFunctionValue (Call OwnedMaker)");
    let mut json = serde_json::to_value(g.snapshot()).unwrap();
    json["lambdas"][0]["handle"] = serde_json::json!(LAMBDA_BASE);
    let ambiguous: GameSnapshot = serde_json::from_value(json).unwrap();
    let result = Game::restore(
        &f.data,
        World::new(&f.forms, &f.refs, Default::default()),
        Default::default(),
        &ambiguous,
    );
    assert!(matches!(result,Err(ref e) if e.contains("unambiguous runtime callable")));
    assert_eq!(
        g.run_function(
            &XVal::Ref(LAMBDA_BASE),
            None,
            vec![],
            None,
            Default::default()
        )
        .unwrap(),
        XVal::Num(71.0)
    );
    g.run_function(&callback, None, vec![], None, Default::default())
        .unwrap();
    assert_eq!(value(&g, CAPTURED), 41.0);
}

#[test]
fn high_record_and_owned_lambda_arity_are_checked_before_jip_dispatch() {
    let f = fixture(true);
    let mut g = game(&f);
    q(
        &mut g,
        "SetOnMenuClickEventHandler OwnedHit 1 \"portable\"\nSetOnMenuClickEventHandler (Begin Function {}\nlet HitCount := 100\nEnd) 1 \"portable\"",
    );
    assert_eq!(
        g.jip_menu_click("portable", "portable", 1.0, 2.0, "leaf"),
        0
    );
    assert_eq!(value(&g, COUNT), 0.0);
    no_script_errors(&g);
    let error = g
        .run_function(
            &XVal::Ref(REGISTRAR.0),
            None,
            vec![],
            None,
            Default::default(),
        )
        .unwrap_err();
    assert!(
        error.contains("has no Function block"),
        "an ordinary script keeps its non-UDF admission: {error}"
    );
}

#[test]
fn receiver_admission_stops_null_calls_before_mutation_in_source_and_loaded_bytecode() {
    for include_source in [true, false] {
        let f = fixture_source_mode(true, include_source);
        assert_eq!(
            f.data
                .script(RECEIVER_DRIVER)
                .unwrap()
                .unit
                .source
                .is_some(),
            include_source,
            "receiver driver SCTX admission precedes receiver oracles"
        );
        assert_eq!(
            f.data.script(RECEIVER_BODY).unwrap().unit.source.is_some(),
            include_source
        );
        assert!(f.data.script(RECEIVER_DRIVER).unwrap().unit.has_scda);
        assert_eq!(f.data.script(RECEIVER_DRIVER).unwrap().raw, 0);
        for id in [RECEIVER_DRIVER, RECEIVER_BODY, RECEIVER_HIGH] {
            let unit = f.data.script(id).unwrap();
            assert_eq!(unit.unit.source.is_some(), include_source);
            assert!(unit.unit.has_scda);
            assert_eq!(unit.raw, 0);
            let selected = f
                .data
                .source_units
                .iter()
                .find(|entry| entry.key == unit.key());
            if include_source {
                assert!(
                    selected.is_some_and(|entry| entry.error.is_none()),
                    "source receiver unit {id:?} must be selected and compiled: {selected:?}"
                );
            } else {
                assert!(
                    selected.is_none(),
                    "source-less receiver unit {id:?} must retain loaded bytecode"
                );
            }
        }
        let mut g = game(&f);
        let ctx = RunCtx {
            self_ref: Some(TARGET),
            ..Default::default()
        };
        let lambda = q(
            &mut g,
            "SetFunctionValue (Begin Function {}\nlet HitCount := HitCount + 1\nSetFunctionValue GetSelf\nEnd)",
        );
        for callback in [
            XVal::Ref(RECEIVER_BODY.0),
            XVal::Ref(RECEIVER_HIGH.0),
            lambda,
        ] {
            for (receiver, explicit, expected) in [
                (0, false, TARGET.0),
                (FormId::PLAYER_REF.0, true, FormId::PLAYER_REF.0),
                // Existing UDF admission forwards nonempty references; it does not
                // impose an actor/base-form existence policy on GetSelf.
                (0x00AB_CDEF, true, 0x00AB_CDEF),
            ] {
                let before = value(&g, COUNT);
                let result = g
                    .run_function(
                        &XVal::Ref(RECEIVER_DRIVER.0),
                        None,
                        vec![
                            callback.clone(),
                            XVal::Ref(receiver),
                            XVal::Num(f64::from(u8::from(explicit))),
                        ],
                        None,
                        ctx,
                    )
                    .unwrap();
                assert_eq!(result, XVal::Num(f64::from(expected)));
                assert_eq!(value(&g, COUNT), before + 1.0);
                assert_eq!(value(&g, DAMAGE), 99.0);
            }
            let before = (value(&g, COUNT), value(&g, DAMAGE));
            let result = g.run_function(
                &XVal::Ref(RECEIVER_DRIVER.0),
                None,
                vec![callback.clone(), XVal::Ref(0), XVal::Num(1.0)],
                None,
                ctx,
            );
            assert!(
                matches!(result, Err(ref e) if e == "call called on no reference"),
                "nested explicit null: {result:?}"
            );
            assert_eq!(
                (value(&g, COUNT), value(&g, DAMAGE)),
                before,
                "null invokes neither callback body nor caller tail"
            );
            let result = g.run_function(&callback, Some(Value::Ref(0)), vec![], None, ctx);
            assert!(
                matches!(result, Err(ref e) if e == "call called on no reference"),
                "direct explicit null: {result:?}"
            );
            assert_eq!((value(&g, COUNT), value(&g, DAMAGE)), before);
            let result = g.run_function(&callback, Some(Value::Num(0.5)), vec![], None, ctx);
            assert!(
                matches!(result, Err(ref e) if e == "call called on no reference"),
                "non-reference receiver: {result:?}"
            );
            assert_eq!((value(&g, COUNT), value(&g, DAMAGE)), before);
            assert_eq!(
                g.run_function(&callback, None, vec![], None, ctx).unwrap(),
                XVal::Ref(TARGET.0)
            );
        }
        let before = (value(&g, COUNT), value(&g, WHO), value(&g, DAMAGE));
        let result = g.run_query("ref receiver\nBegin Function {}\nlet HitWho := receiver.Call (Call OwnedReceiverBody)\nlet HitDamage := 123\nEnd\n", ctx);
        assert!(
            matches!(result, Err(ref e) if e == "call called on no reference"),
            "source explicit null: {result:?}"
        );
        assert_eq!(
            (value(&g, COUNT), value(&g, WHO), value(&g, DAMAGE)),
            before,
            "null receiver refuses before evaluating a mutating function argument or later statements"
        );
        no_script_errors(&g);
    }
}

fn travel(g: &mut Game<'_>) -> FormId {
    q(
        g,
        "ref actor\nlet actor := StringToRef \"00000903\"\nactor.TravelToRef PlayerREF 1",
    );
    let package = g.extra[&TARGET]
        .script_package
        .expect("real travel package");
    let def = g.package_def(&package).unwrap();
    assert_eq!(def.ty, fnv_game::package::PackageType::Travel);
    assert_eq!(
        def.location.unwrap().kind,
        fnv_game::package::LocationKind::NearReference(Some(FormId::PLAYER_REF))
    );
    assert_ne!(def.flags & fnv_game::package::flags::ALWAYS_RUN, 0);
    package
}

#[test]
fn travel_packages_and_lambdas_keep_distinct_handles_in_both_orders_and_after_restore() {
    for package_first in [true, false] {
        let f = fixture(true);
        let mut g = game(&f);
        let (package, callback) = if package_first {
            let package = travel(&mut g);
            (package, q(&mut g, "SetFunctionValue (Call OwnedMaker)"))
        } else {
            let callback = q(&mut g, "SetFunctionValue (Call OwnedMaker)");
            (travel(&mut g), callback)
        };
        let XVal::Ref(handle) = callback else {
            panic!("owned callback")
        };
        assert_ne!(package.0, handle);
        assert_ne!(package.0, LAMBDA_BASE, "loaded record retains its ID");
        assert_ne!(handle, LAMBDA_BASE);
        g.run_function(&callback, None, vec![], None, Default::default())
            .unwrap();
        assert_eq!(value(&g, CAPTURED), 41.0);
        assert_eq!(
            g.run_function(
                &XVal::Ref(LAMBDA_BASE),
                None,
                vec![],
                None,
                Default::default()
            )
            .unwrap(),
            XVal::Num(71.0)
        );
        assert!(g.package_def(&package).is_some());
        let snapshot = g.snapshot();
        let mut restored = Game::restore(
            &f.data,
            World::new(&f.forms, &f.refs, Default::default()),
            Default::default(),
            &snapshot,
        )
        .unwrap();
        assert!(
            restored.package_def(&package).is_none(),
            "existing transient package definitions are not serialized; no persistence parity claim"
        );
        assert_eq!(
            q(&mut restored, "SetFunctionValue (Call OwnedMaker)"),
            callback
        );
        let next_package = travel(&mut restored);
        assert_ne!(
            next_package.0, handle,
            "post-load package allocation preserves restored lambda identity"
        );
        restored
            .run_function(&callback, None, vec![], None, Default::default())
            .unwrap();
        assert_eq!(value(&restored, CAPTURED), 41.0);
        assert!(restored.package_def(&next_package).is_some());
        no_script_errors(&g);
        no_script_errors(&restored);
    }
}

#[test]
fn high_record_arrays_dispose_on_success_and_error_and_return_live_values_to_the_caller() {
    let f = fixture(true);
    let mut g = game(&f);
    assert_eq!(
        g.run_function(
            &XVal::Ref(ARRAY_BODY.0),
            None,
            vec![XVal::Num(0.0)],
            None,
            Default::default()
        )
        .unwrap(),
        XVal::Num(0.0)
    );
    assert!(
        g.ext.is_empty(),
        "unreturned array and string locals are disposed"
    );
    assert_eq!(
        q(
            &mut g,
            "array_var result\nlet result := Call OwnedArrayBody 1\nSetFunctionValue result[0] + result[1]"
        ),
        XVal::Num(13.0)
    );
    assert!(
        g.ext.is_empty(),
        "returned array remains alive in the caller, then leaves with its local"
    );
    let before = value(&g, COUNT);
    let error = g
        .run_function(
            &XVal::Ref(ARRAY_BODY.0),
            None,
            vec![XVal::Num(2.0)],
            None,
            Default::default(),
        )
        .unwrap_err();
    assert_eq!(error, "call called on no reference");
    assert_eq!(
        value(&g, COUNT),
        before,
        "failed nested receiver never enters the neighbor body"
    );
    assert!(
        g.ext.is_empty(),
        "error unwind releases the allocated array and string"
    );
    assert_eq!(
        g.run_function(
            &XVal::Ref(ORDINARY.0),
            None,
            vec![XVal::Num(2.0)],
            None,
            Default::default()
        )
        .unwrap(),
        XVal::Num(2.0)
    );
    register_record(&mut g);
    hit(&mut g, TARGET, 7.0);
    assert_eq!(value(&g, COUNT), before + 1.0);
    assert!(g.ext.is_empty());
    no_script_errors(&g);
}

#[test]
fn sparse_owned_lambda_updates_its_maker_local_during_the_nested_call() {
    let f = fixture(true);
    let mut g = game(&f);
    let package = travel(&mut g);
    assert_eq!(
        q(
            &mut g,
            "int n\nref cb\nlet n := 40\nlet cb := (Begin Function {}\nlet n := n + 1\nlet CapturedCount := n\nEnd)\nCall cb\nSetFunctionValue n"
        ),
        XVal::Num(41.0),
        "maker observes the nested lambda write before it returns"
    );
    assert_eq!(value(&g, CAPTURED), 41.0);
    let saved = serde_json::to_value(g.snapshot()).unwrap();
    let handle = saved["lambdas"].as_array().unwrap().last().unwrap()["handle"]
        .as_u64()
        .unwrap() as u32;
    assert_ne!(handle, LAMBDA_BASE);
    assert_ne!(handle, package.0);
    g.run_function(&XVal::Ref(handle), None, vec![], None, Default::default())
        .unwrap();
    assert_eq!(
        value(&g, CAPTURED),
        42.0,
        "same captured local remains owned after the maker returns"
    );
    no_script_errors(&g);
}
