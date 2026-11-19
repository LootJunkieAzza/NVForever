//! Authored real Game source/VM controls. No installed script or game data.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{game::RunCtx, trace::EventKind, Game, GameConfig, GameData};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tracing_subscriber::{
    layer::{Context, SubscriberExt},
    Layer,
};

const QUEST: FormId = FormId(0x810);
const ROOM: FormId = FormId(0x900);
const SOURCE: &str = r#"scn OwnedNullReceiver
ref left
ref right
short branch
short before
short after
string_var sourceMarker
Begin GameMode
 set before to before + 1
 if branch == 0
  left.MoveToCell OwnedRoom 1 2 3
 else
  right.MoveToCell OwnedRoom 4 5 6
 endif
 set after to after + 1
End
"#;
const GETTER: &str = r#"scn OwnedReceiverGetter
string_var sourceMarker
Begin Function {}
 set OwnedReads to OwnedReads + 1
 SetFunctionValue 0
End
"#;
struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}
fn scpt(id: u32, name: &str, source: &str, variables: &[(u32, &str, u8)]) -> Vec<u8> {
    let mut fields = vec![sub(b"EDID", z(name))];
    fields.extend(unit(0, &script(&[]), variables, &[]));
    fields.push(sub(b"SCTX", source.as_bytes().to_vec()));
    record(b"SCPT", 0, id, &fields)
}
fn fixture() -> Fixture {
    let player = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]);
    let base = record(b"STAT", 0, 0x800, &[sub(b"EDID", z("OwnedBase"))]);
    let global = record(
        b"GLOB",
        0,
        0x840,
        &[
            sub(b"EDID", z("OwnedReads")),
            sub(b"FNAM", vec![b'f']),
            sub(b"FLTV", 0f32.to_le_bytes().to_vec()),
        ],
    );
    let quest = record(
        b"QUST",
        0,
        QUEST.0,
        &[
            sub(b"EDID", z("OwnedQuest")),
            sub(b"SCRI", u32s(&[0x811])),
            sub(
                b"DATA",
                [vec![0; 4], 0.01f32.to_le_bytes().to_vec()].concat(),
            ),
        ],
    );
    let cell = record(
        b"CELL",
        0,
        ROOM.0,
        &[sub(b"EDID", z("OwnedRoom")), sub(b"DATA", vec![1])],
    );
    let scripts = [
        scpt(
            0x811,
            "OwnedNullReceiver",
            SOURCE,
            &[
                (1, "left", 0),
                (2, "right", 0),
                (3, "branch", 1),
                (4, "before", 1),
                (5, "after", 1),
                (6, "sourceMarker", 0),
            ],
        ),
        scpt(
            0x812,
            "OwnedReceiverGetter",
            GETTER,
            &[(1, "sourceMarker", 0)],
        ),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"NPC_", &player),
        top(b"STAT", &base),
        top(b"GLOB", &global),
        top(b"QUST", &quest),
        top(b"SCPT", &scripts),
        top(
            b"CELL",
            &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)),
        ),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "OwnedNull.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let table_path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&table_path).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(
        &order,
        &schema,
        FunctionTable::load(&table_path).unwrap(),
        &catalogue,
        &refs,
    );
    assert_eq!(
        data.issues.count("script_source_does_not_compile"),
        0,
        "{:?}",
        data.issues.to_json()
    );
    assert_eq!(
        data.issues.count("script_compiled_from_source"),
        2,
        "{:?}",
        data.issues.to_json()
    );
    for id in [0x811, 0x812] {
        let c = data.script(FormId(id)).unwrap();
        assert!(
            !c.program.blocks.is_empty(),
            "source fallback: id={id:X} raw={} program={:?}",
            c.raw,
            c.program
        );
    }
    Fixture { data, forms, refs }
}
fn game(f: &Fixture) -> Game<'_> {
    let mut g = Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, SimConfig::default()),
        GameConfig {
            strict: false,
            frame_seconds: 0.1,
            ..Default::default()
        },
    );
    g.player_entered(Some(ROOM));
    assert!(g.data.quests[&QUEST].script_delay() <= g.cfg.frame_seconds);
    g
}
fn query(g: &mut Game<'_>, body: &str) -> Result<XVal, String> {
    g.run_query(
        &format!("Begin Function {{}}\n{body}\nEnd"),
        RunCtx::default(),
    )
}
fn number(g: &mut Game<'_>, expression: &str) -> f64 {
    match query(g, &format!("SetFunctionValue ({expression})")).unwrap() {
        XVal::Num(n) => n,
        v => panic!("{v:?}"),
    }
}
fn errors(g: &Game<'_>) -> Vec<String> {
    g.events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ScriptError { error, .. } => Some(error.clone()),
            _ => None,
        })
        .collect()
}

type Record = BTreeMap<String, String>;
#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<Record>>>);
struct Fields(Record);
impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        self.0.insert(f.name().into(), format!("{v:?}"));
    }
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
        self.0.insert(f.name().into(), v.to_string());
    }
}
impl<S: tracing::Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() == "fnv_game::call_context" {
            let mut fields = Fields(Record::new());
            event.record(&mut fields);
            self.0.lock().unwrap().push(fields.0);
        }
    }
}
fn captured(f: impl FnOnce(&Arc<Mutex<Vec<Record>>>)) {
    let rows = Arc::new(Mutex::new(Vec::new()));
    let dispatch =
        tracing::Dispatch::new(tracing_subscriber::registry().with(Capture(rows.clone())));
    tracing::dispatcher::with_default(&dispatch, || f(&rows));
}

#[test]
fn actual_quest_zero_receivers_are_distinguished_and_fatal_behavior_is_unchanged() {
    captured(|rows| {
        let f = fixture();
        let mut g = game(&f);
        query(&mut g, "StartQuest OwnedQuest").unwrap();
        g.tick();
        assert_eq!(number(&mut g, "OwnedQuest.before"), 1.0);
        assert_eq!(number(&mut g, "OwnedQuest.after"), 0.0);
        assert_eq!(errors(&g), ["movetocell called on no reference"]);
        let first = rows.lock().unwrap().clone();
        assert_eq!(first.len(), 1, "owned quest receiver diagnostic context missing");
        assert_eq!(first[0]["command"], "movetocell");
        assert_eq!(first[0]["variable"], "left");
        assert_eq!(first[0]["local_index"], "Some(1)");
        assert_eq!(first[0]["receiver_form"], "None");
        assert_eq!(first[0]["receiver_kind"], "number");
        assert_eq!(first[0]["quest"], "Some(2064)");
        assert_eq!(first[0]["registrar"], "unavailable");
        query(&mut g, "set OwnedQuest.branch to 1").unwrap();
        g.tick();
        assert_eq!(number(&mut g, "OwnedQuest.before"), 2.0);
        assert_eq!(number(&mut g, "OwnedQuest.after"), 0.0);
        // Frame events accumulate; each failing run still emits exactly one original error.
        assert_eq!(
            errors(&g),
            [
                "movetocell called on no reference",
                "movetocell called on no reference"
            ]
        );
        let all = rows.lock().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1]["variable"], "right");
        assert_eq!(all[1]["local_index"], "Some(2)");
        assert_ne!(all[0]["frame"], all[1]["frame"]);
    });
}

#[test]
fn actual_valid_neighbor_moves_and_continues_without_failure_context() {
    captured(|rows| {
        let f = fixture();
        let mut g = game(&f);
        // Seed a distinct location so both cell and position must actually change.
        {
            let player = g.world.get(FormId::PLAYER_REF).unwrap();
            player.cell = None;
            player.pos = [-11.0, -22.0, -33.0];
        }
        query(
            &mut g,
            "set OwnedQuest.left to player\nStartQuest OwnedQuest",
        )
        .unwrap();
        {
            let player = g.world.peek(FormId::PLAYER_REF).unwrap();
            assert_eq!(player.cell, None);
            assert_eq!(player.pos, [-11.0, -22.0, -33.0]);
        }
        g.tick();
        assert_eq!(number(&mut g, "OwnedQuest.before"), 1.0);
        assert_eq!(number(&mut g, "OwnedQuest.after"), 1.0);
        assert!(errors(&g).is_empty());
        assert!(rows.lock().unwrap().is_empty());
        // MoveToCell updates World directly; its result is the movement oracle.
        let player = g.world.peek(FormId::PLAYER_REF).unwrap();
        assert_eq!(player.cell, Some(ROOM));
        assert_eq!(player.pos, [1.0, 2.0, 3.0]);
    });
}

#[test]
fn actual_computed_receiver_runs_once_and_host_rejected_base_keeps_its_original_error() {
    captured(|rows| {
        let f = fixture();
        let mut g = game(&f);
        let error = query(
            &mut g,
            "(Call OwnedReceiverGetter).MoveToCell OwnedRoom 1 2 3",
        )
        .unwrap_err();
        assert_eq!(error, "movetocell called on no reference");
        assert_eq!(
            number(&mut g, "OwnedReads"),
            1.0,
            "logging must not reevaluate receiver"
        );
        let first = rows.lock().unwrap().clone();
        assert_eq!(first.len(), 1, "owned computed receiver diagnostic context missing");
        assert_eq!(first[0]["receiver_shape"], "call");
        let error = query(&mut g, "OwnedBase.ClampToGround").unwrap_err();
        assert_eq!(error, "clamptoground called on no reference");
        let all = rows.lock().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1]["command"], "clamptoground");
        assert_eq!(all[1]["receiver_form"], "Some(2048)");
    });
}
