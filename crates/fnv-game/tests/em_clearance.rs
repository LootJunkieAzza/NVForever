//! Named clearance -> live source callback -> existing first-person transform request.
//! Portable authored records; actual engine collision is covered separately.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    GameData,
    game::{Game, GameConfig, GameEnv, RunCtx},
    nif_blocks::{ModelView, SceneEdit, SceneNode},
    ray::{self, CastKind, Hit},
    vats::{self, CameraRoom},
};
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use std::cell::RefCell;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}
fn fixture() -> Fixture {
    let actors = [
        record(
            b"NPC_",
            0,
            7,
            &[sub(b"EDID", z("Player")), sub(b"ACBS", vec![0; 24])],
        ),
        record(
            b"NPC_",
            0,
            0xE10,
            &[
                sub(b"EDID", z("OwnedLeanActorBase")),
                sub(b"ACBS", vec![0; 24]),
            ],
        ),
    ]
    .concat();
    let actor = record(
        b"ACHR",
        0,
        0xE20,
        &[
            sub(b"EDID", z("OwnedLeanActor")),
            sub(b"NAME", 0xE10u32.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let cell = [
        record(
            b"CELL",
            0,
            0xE00,
            &[sub(b"EDID", z("OwnedLeanCell")), sub(b"DATA", vec![1])],
        ),
        group(
            0xE00u32.to_le_bytes(),
            6,
            &group(0xE00u32.to_le_bytes(), 9, &actor),
        ),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"NPC_", &actors),
        top(
            b"CELL",
            &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)),
        ),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "OwnedLeanClearance.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(
        &lo,
        &schema,
        FunctionTable::load(&functions).unwrap(),
        &json,
        &refs,
    );
    Fixture { data, forms, refs }
}
fn game(f: &Fixture) -> Game<'_> {
    let mut g = Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, SimConfig::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    );
    g.player_entered(Some(FormId(0xE00)));
    let p = g.world.get(FormId::PLAYER_REF).unwrap();
    p.pos = [10.0, 20.0, 30.0];
    p.rot = [0.0; 3];
    g
}
fn query(g: &mut Game, declarations: &str, body: &str, ctx: RunCtx) -> XVal {
    let source = format!("{declarations}\nBegin Function {{}}\n{body}\nEnd\n");
    let compiled =
        fnv_script::compile::compile_runtime(&source, &g.data.table, &GameEnv { g: &*g });
    assert!(
        compiled.errors().next().is_none(),
        "em-clearance-compile: {:?}",
        compiled.diags
    );
    g.run_query(&source, ctx)
        .unwrap_or_else(|e| panic!("em-clearance-source: {body}: {e}"))
}
fn hit(distance: f32, from: [f32; 3], to: [f32; 3]) -> Hit {
    let dx = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let length = (dx[0] * dx[0] + dx[1] * dx[1] + dx[2] * dx[2]).sqrt();
    Hit {
        distance,
        point: [
            from[0] + dx[0] * distance / length,
            from[1] + dx[1] * distance / length,
            from[2] + dx[2] * distance / length,
        ],
        form: Some(0xE90),
        material: None,
    }
}
#[test]
fn ordinary_queries_use_the_current_caster_without_overwriting_cpth_or_world() {
    let f = fixture();
    let mut g = game(&f);
    g.vats.room = Some(CameraRoom {
        area_free: [7.0, 9.0, 11.0, 13.0],
        target_visible: [17.0; 4],
    });
    let before = serde_json::to_value(g.world.snapshot()).unwrap();
    let room = g.vats.room;
    let casts = RefCell::new(Vec::new());
    let caster = |from: [f32; 3], to: [f32; 3], kind: CastKind| {
        assert_eq!(kind, CastKind::CameraClearance);
        assert_eq!(from, [10.0, 20.0, 94.0]);
        casts.borrow_mut().push((from, to));
        Some(hit(if to[0] > from[0] { 30.0 } else { 120.0 }, from, to))
    };
    ray::with_caster(&caster, || {
        assert_eq!(
            query(
                &mut g,
                "",
                "SetFunctionValue (Player.GetVATSRightAreaFree)",
                Default::default()
            ),
            XVal::Num(30.0)
        );
        assert_eq!(
            query(
                &mut g,
                "",
                "SetFunctionValue (Player.GetVATSLeftAreaFree)",
                Default::default()
            ),
            XVal::Num(120.0)
        );
    });
    assert_eq!(casts.borrow().len(), 2);
    assert_eq!(g.vats.room, room);
    assert_eq!(vats::area_value(&g, vats::RIGHT_AREA_FREE), 7.0);
    assert_eq!(
        serde_json::to_value(g.world.snapshot()).unwrap(),
        before,
        "queries must not materialize/mutate World state"
    );
}
#[test]
fn registered_source_uses_clearance_for_real_named_transform_requests_and_fresh_heading() {
    let f = fixture();
    let mut g = game(&f);
    g.scene.use_separate_player_models();
    g.scene.set_tree_for(
        FormId::PLAYER_REF,
        ModelView::FirstPerson,
        vec![SceneNode {
            name: "OwnedLeanNode".into(),
            parent: None,
            translation: [0.0; 3],
            rotation: [0.0; 3],
            scale: 1.0,
            flags: 0,
            material: None,
            info: None,
        }],
    );
    query(
        &mut g,
        "",
        r#"SetEventHandler "OwnedLean" (begin function {}
      Player.SetNifBlockTranslation "OwnedLeanNode" ((Player.GetVATSRightAreaFree) / 10) 0 0 2
    end)"#,
        Default::default(),
    );
    let caster = |from: [f32; 3], to: [f32; 3], kind: CastKind| {
        assert_eq!(kind, CastKind::CameraClearance);
        Some(hit(
            if to[0] - from[0] > 100.0 { 30.0 } else { 40.0 },
            from,
            to,
        ))
    };
    ray::with_caster(&caster, || {
        assert_eq!(
            g.dispatch_event("OwnedLean", vec![], Some(FormId::PLAYER_REF)),
            1
        );
        let queued = g.scene.take_edits();
        assert_eq!(
            queued.len(),
            1,
            "registered source must queue exactly one edit: {queued:?}"
        );
        let (view, edit) = queued.into_iter().next().unwrap().into_model();
        assert_eq!(
            view,
            ModelView::FirstPerson,
            "the source explicitly selected player model 2"
        );
        assert!(
            matches!(&edit, SceneEdit::Translation{r,node,v} if *r==FormId::PLAYER_REF && node=="OwnedLeanNode" && *v==[3.0,0.0,0.0]),
            "em-area-transform: registered source must queue the clearance-dependent node request: {edit:?}"
        );
        query(&mut g, "", "Player.SetAngle Z 90", Default::default());
        assert_eq!(
            g.dispatch_event("OwnedLean", vec![], Some(FormId::PLAYER_REF)),
            1
        );
        assert_eq!(
            g.scene
                .node_for_model(FormId::PLAYER_REF, ModelView::FirstPerson, "OwnedLeanNode")
                .unwrap()
                .translation,
            [4.0, 0.0, 0.0],
            "updated source heading must change the existing feature consumer without a room cache"
        );
        assert_eq!(g.scene.take_edits().len(), 1);
    });
}
#[test]
fn unavailable_and_invalid_contexts_do_not_cast_or_mutate_and_unused_calls_are_free() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(
        query(
            &mut g,
            "",
            "SetFunctionValue (Player.GetVATSLeftAreaFree)",
            Default::default()
        ),
        XVal::Num(0.0)
    );
    let calls = RefCell::new(0);
    let caster = |from, to, _kind| {
        *calls.borrow_mut() += 1;
        Some(hit(22.0, from, to))
    };
    ray::with_caster(&caster, || {
        assert_eq!(
            query(&mut g, "", "SetFunctionValue 19", Default::default()),
            XVal::Num(19.0)
        );
        assert_eq!(*calls.borrow(), 0, "no unused per-frame producer/casts");
        assert_eq!(
            query(
                &mut g,
                "",
                "SetFunctionValue (OwnedLeanActorBase.GetVATSRightAreaFree)",
                Default::default()
            ),
            XVal::Num(0.0)
        );
        let before = serde_json::to_value(g.world.snapshot()).unwrap();
        let source = "ref invalid\nBegin Function {}\nlet invalid := 0\nSetFunctionValue (invalid.GetVATSRightAreaFree)\nEnd\n";
        let e = g
            .run_query(
                source,
                RunCtx {
                    self_ref: Some(FormId::PLAYER_REF),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(
            e.contains("getvatsrightareafree called on no reference"),
            "existing VM receiver boundary: {e}"
        );
        assert_eq!(serde_json::to_value(g.world.snapshot()).unwrap(), before);
        assert_eq!(*calls.borrow(), 0);
        assert_eq!(
            query(
                &mut g,
                "",
                "SetFunctionValue (OwnedLeanActor.GetVATSLeftAreaFree)",
                Default::default()
            ),
            XVal::Num(22.0)
        );
        assert_eq!(*calls.borrow(), 1);
        assert_eq!(
            query(
                &mut g,
                "",
                "SetFunctionValue (GetVATSLeftAreaFree)",
                RunCtx {
                    self_ref: Some(FormId(0xE20)),
                    ..Default::default()
                }
            ),
            XVal::Num(22.0),
            "an omitted receiver retains its calling actor"
        );
        assert_eq!(*calls.borrow(), 2);
        assert_eq!(
            query(
                &mut g,
                "",
                "SetFunctionValue (GetVATSLeftAreaFree)",
                Default::default()
            ),
            XVal::Num(0.0)
        );
        assert_eq!(
            *calls.borrow(),
            2,
            "a missing caller must not invent a player cast"
        );
    });
    assert_eq!(
        query(
            &mut g,
            "",
            "SetFunctionValue (Player.GetVATSLeftAreaFree)",
            Default::default()
        ),
        XVal::Num(0.0),
        "physical context must not survive its lifetime"
    );
}
