//! Saved Hardcore eligibility and the unchanged survival consumer, issue #2271.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{Game, GameConfig, GameData, game::{GameSnapshot, InstKey, RunCtx}, trace::EventKind};
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormId, FormStore, RefTable, World};
const HARDCORE_SCRIPT: FormId = FormId(0x901);
struct Fixture { data: GameData, forms: FormStore, refs: RefTable }
fn fixture() -> Fixture {
    let mut subs = vec![sub(b"EDID", z("HardcoreTrackingCommand"))];
    subs.extend(unit(0, &script(&[(0, None, st(0x126A, &[]))]), &[], &[]));
    let player = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let bytes = [tes4(), top(b"NPC_", &player), top(b"SCPT", &record(b"SCPT", 0, HARDCORE_SCRIPT.0, &subs))].concat();
    let plugins = [Plugin::parse(&bytes, "HardcoreTracking.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/script/functions.json"));
    let json = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(path).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}
fn game(f: &Fixture) -> Game<'_> { Game::new(&f.data, World::new(&f.forms, &f.refs, Default::default()), GameConfig::default()) }
fn clean(g: &Game) { assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::Unhandled { .. } | EventKind::CallFailed { .. } | EventKind::Stub { .. } | EventKind::ArgUnread { .. } | EventKind::ScriptError { .. })), "{:?}", g.events); }
fn query(g: &mut Game, expr: &str) -> XVal {
    let value = g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue ({expr})\nEnd"), RunCtx::default()).unwrap();
    clean(g); value
}

#[test]
fn disable_tracking_changes_real_eligibility_without_switching_off_hardcore() {
    let f = fixture(); let mut g = game(&f);
    g.set_hardcore(true);
    fnv_game::functions::set_actor_value_current(&mut g, FormId::PLAYER_REF, fnv_game::aid::DEHYDRATION, 17.25).unwrap();
    assert_eq!(query(&mut g, "IsAlwaysHardcore"), XVal::Num(1.0));
    g.run_source("DisableHardcoreTracking", RunCtx::default()).unwrap(); clean(&g);
    assert_eq!(query(&mut g, "IsAlwaysHardcore"), XVal::Num(0.0));
    assert!(g.hardcore() && g.world.config().hardcore);
    assert_eq!(fnv_game::functions::actor_value(&mut g, FormId::PLAYER_REF, fnv_game::aid::DEHYDRATION).unwrap(), 17.25, "the command did not clear survival needs");
    g.frames(60); clean(&g);
    assert!(fnv_game::functions::actor_value(&mut g, FormId::PLAYER_REF, fnv_game::aid::DEHYDRATION).unwrap() > 17.25, "real need accumulation remains enabled");
    g.run_source("babby", RunCtx::default()).unwrap(); clean(&g);
    assert!(g.hardcore() && g.player.hardcore_was_off);
    let snap: GameSnapshot = serde_json::from_slice(&serde_json::to_vec(&g.snapshot()).unwrap()).unwrap();
    let world = World::restore(&f.forms, &f.refs, &snap.sim).unwrap();
    let mut loaded = Game::restore(&f.data, world, g.cfg, &snap).unwrap();
    assert_eq!(query(&mut loaded, "IsAlwaysHardcore"), XVal::Num(0.0));
    assert!(loaded.hardcore());
    loaded.set_hardcore(true);
    assert_eq!(query(&mut loaded, "IsAlwaysHardcore"), XVal::Num(0.0));
}

#[test]
fn authored_hardcore_scda_marks_eligibility_even_while_mode_is_off() {
    let f = fixture(); let mut g = game(&f);
    assert!(!g.hardcore() && !g.player.hardcore_was_off);
    assert!(g.run_event(InstKey::Ref { reference: FormId::PLAYER_REF }, HARDCORE_SCRIPT, 0, None, RunCtx { self_ref: Some(FormId::PLAYER_REF), ..Default::default() })); clean(&g);
    assert!(!g.hardcore() && g.player.hardcore_was_off);
    g.set_hardcore(true);
    assert_eq!(query(&mut g, "IsAlwaysHardcore"), XVal::Num(0.0));
}
