//! The soft-lock watchdog (#2365; `fnv_game::softlock`).

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::softlock::{story, Kind, Limits, Outside, Watchdog};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let quest = record(b"QUST", 0, 0x900, &[sub(b"EDID", z("HoldQuest"))]);
    let bytes = [tes4(), top(b"QUST", &quest)].concat();
    let plugins = vec![Plugin::parse(&bytes, "Softlock.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() })
}

fn run(g: &mut Game, body: &str) {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"));
}

/// `seconds` of frames; the findings on the way.
fn frames(dog: &mut Watchdog, g: &Game, seconds: f64) -> Vec<Kind> {
    let n = (seconds * 10.0).round() as usize;
    (0..n).filter_map(|_| dog.step(g, 0.1, Outside::default())).map(|f| f.kind).collect()
}

#[test]
fn controls_held_with_nothing_going_on_is_found_once() {
    let f = fixture();
    let mut g = game(&f);
    let mut dog = Watchdog::default();
    assert_eq!(frames(&mut dog, &g, 60.0), [], "a free player is not held");
    run(&mut g, "DisablePlayerControls");
    assert_eq!(frames(&mut dog, &g, 29.0), [], "under the limit");
    let found = (0..30).find_map(|_| dog.step(&g, 0.1, Outside::default())).expect("found at 30 s");
    assert_eq!(found.kind, Kind::ControlsHeld);
    assert!(found.text.contains("scripts hold the controls (movement, Pip-Boy, fighting, POV)"), "{}", found.text);
    assert_eq!(frames(&mut dog, &g, 120.0), [], "once per hold");
    run(&mut g, "EnablePlayerControls");
    assert_eq!(frames(&mut dog, &g, 0.1), [], "a frame with the controls back");
    run(&mut g, "DisablePlayerControls");
    assert_eq!(frames(&mut dog, &g, 31.0), [Kind::ControlsHeld], "a new hold is found again");
}

#[test]
fn a_story_event_starts_the_wait_again() {
    let f = fixture();
    let mut g = game(&f);
    let mut dog = Watchdog::default();
    run(&mut g, "DisablePlayerControls");
    assert_eq!(frames(&mut dog, &g, 25.0), []);
    g.emit(EventKind::StageSet { quest: FormId(0x900), stage: 10, entries: vec![] });
    assert_eq!(frames(&mut dog, &g, 25.0), [], "the scene went on 25 s in");
    let found = (0..100).find_map(|_| dog.step(&g, 0.1, Outside::default())).expect("found 30 s after the stage");
    assert!(found.text.contains("last stage set: HoldQuest 10"), "{}", found.text);
}

#[test]
fn a_restrained_player_is_held_and_a_dead_one_is_not() {
    let f = fixture();
    let mut g = game(&f);
    let mut dog = Watchdog::new(Limits { controls: 2.0, ..Limits::default() });
    g.extra.entry(FormId::PLAYER_REF).or_default().restrained = true;
    let found = (0..30).find_map(|_| dog.step(&g, 0.1, Outside::default())).expect("found at the shortened limit");
    assert!(found.text.starts_with("the player is restrained"), "{}", found.text);
    g.extra.entry(FormId::PLAYER_REF).or_default().dead = true;
    assert_eq!(frames(&mut dog, &g, 10.0), []);
}

#[test]
fn timers_package_choices_and_hellos_are_not_story() {
    let hello = Some(FormId(0x77));
    let line = |topic| EventKind::LineStarted { speaker: FormId(1), target: Some(FormId::PLAYER_REF), topic: FormId(topic), info: FormId(2), seconds: 1.0 };
    assert!(!story(&line(0x77), hello), "a hello");
    assert!(story(&line(0x78), hello), "a scene's line");
    assert!(!story(&EventKind::PackageChosen { actor: FormId(1), package: None }, hello));
    assert!(story(&EventKind::MenuOpened { menu: 1001 }, hello));
}
