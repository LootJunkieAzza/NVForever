//! Authored title-start quest consumers through the ordinary source compiler.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{game::{menus, GameSnapshot, RunCtx}, trace::EventKind, Game, GameConfig, GameData};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const TITLE_VISITS: u32 = 0xA00;
const TITLE_BOOTS: u32 = 0xA01;
const TITLE_QUERY: u32 = 0xA02;
const LEGACY_VISITS: u32 = 0xA03;
const ANY_VISITS: u32 = 0xA04;
const LEGACY_BOOTS: u32 = 0xA05;
const LEGACY_TITLE_QUERY: u32 = 0xA06;
const AFTER_LOAD: u32 = 0xA07;
const GAME_VISITS: u32 = 0xA08;
const TITLE_LEGACY_QUERY: u32 = 0xA09;
const ANY_TITLE_QUERY: u32 = 0xA0A;
const UNFILTERED_QUERY: u32 = 0xA0B;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn source_record(id: u32, name: &str, kind: u16, body: &str) -> Vec<u8> {
    let mut fields = vec![sub(b"EDID", z(name))];
    fields.extend(unit(kind, &script(&[(0, None, Vec::new())]), &[], &[]));
    // An extender variable selects the real Auto source path, without inventing
    // extender opcode/argument bytes or changing process-wide compiler mode.
    fields.push(sub(b"SCTX", format!("scn {name}\nstring_var sourceMarker\n{body}\n").into_bytes()));
    record(b"SCPT", 0, id, &fields)
}

fn fixture() -> Fixture {
    let globals = [
        (TITLE_VISITS, "TitleVisits"), (TITLE_BOOTS, "TitleBoots"), (TITLE_QUERY, "TitleQuery"),
        (LEGACY_VISITS, "LegacyVisits"), (ANY_VISITS, "AnyVisits"), (LEGACY_BOOTS, "LegacyBoots"),
        (LEGACY_TITLE_QUERY, "LegacyTitleQuery"), (AFTER_LOAD, "AfterLoad"), (GAME_VISITS, "GameVisits"),
        (TITLE_LEGACY_QUERY, "TitleLegacyQuery"), (ANY_TITLE_QUERY, "AnyTitleQuery"), (UNFILTERED_QUERY, "UnfilteredQuery"),
    ].into_iter().map(|(id, name)| record(b"GLOB", 0, id, &[
        sub(b"EDID", z(name)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec()),
    ])).collect::<Vec<_>>().concat();
    let sources = [
        source_record(0xB00, "OwnedTitleBoot", 1, r#"
Begin MenuMode 4
  set TitleVisits to TitleVisits + 1
  set TitleQuery to MenuMode 4
  set TitleLegacyQuery to MenuMode 1013
  if GetGameRestarted
    set TitleBoots to TitleBoots + 1
    SetEventHandler "PostLoadGame" OwnedAfterLoad
  endif
End"#),
        source_record(0xB01, "OwnedLegacyBlocks", 1, r#"
Begin MenuMode 1013
  set LegacyVisits to LegacyVisits + 1
  set LegacyTitleQuery to MenuMode 4
  set UnfilteredQuery to MenuMode
End
Begin MenuMode
  set AnyVisits to AnyVisits + 1
  set AnyTitleQuery to MenuMode 4
End
Begin GameMode
  set GameVisits to GameVisits + 1
End"#),
        source_record(0xB02, "OwnedLegacyBoot", 1, r#"
Begin MenuMode 1013
  if GetGameRestarted
    set LegacyBoots to LegacyBoots + 1
  endif
End"#),
        source_record(0xB03, "OwnedAfterLoad", 0, r#"
float status
Begin Function {status}
  set AfterLoad to AfterLoad + status
End"#),
    ].concat();
    let quests = (0..3u32).map(|i| record(b"QUST", 0, 0xC00 + i, &[
        sub(b"EDID", z(&format!("OwnedTitleQuest{i}"))), sub(b"SCRI", (0xB00 + i).to_le_bytes().to_vec()),
        sub(b"DATA", [vec![fnv_game::quest::START_GAME_ENABLED, 0, 0, 0], 0.01f32.to_le_bytes().to_vec()].concat()),
    ])).collect::<Vec<_>>().concat();
    let player = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player")),
        sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let bytes = [tes4(), top(b"NPC_", &player), top(b"GLOB", &globals), top(b"SCPT", &sources), top(b"QUST", &quests)].concat();
    let plugins = [Plugin::parse(&bytes, "OwnedTitleStartup.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let catalogue = serde_json::from_slice(&std::fs::read(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(&order, &schema, FunctionTable::load(&functions).unwrap(), &catalogue, &refs);
    assert_eq!(data.source_units.len(), 4, "all four authored SCPT units must select source compilation");
    for source in &data.source_units {
        assert!(source.error.is_none(), "source setup: {source:?}");
        assert!(source.unknown_functions.is_empty() && source.unknown_enums.is_empty(), "source setup: {source:?}");
    }
    for i in 0..3 {
        assert_eq!(data.quests[&FormId(0xC00 + i)].script, Some(FormId(0xB00 + i)));
        assert!(data.script(FormId(0xB00 + i)).is_some(), "attached source unit has a real dispatch entry");
    }
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, frame_seconds: 0.02, ..Default::default() });
    g.new_game();
    g
}
fn value(g: &Game<'_>, id: u32) -> f32 { g.world.global(FormId(id)).unwrap() }
fn query(g: &mut Game<'_>, code: u16) -> XVal {
    g.run_query(&format!("Begin Function {{}}\nSetFunctionValue (MenuMode {code})\nEnd\n"), RunCtx::default()).unwrap()
}
fn clean(g: &Game<'_>) {
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::Stub { .. } | EventKind::Unhandled { .. }
        | EventKind::Skipped { .. } | EventKind::ScriptError { .. } | EventKind::CallFailed { .. } | EventKind::ArgUnread { .. })), "{:?}", g.events);
}

#[test]
fn source_title_start_registers_the_recipient_before_the_load_event() {
    let f = fixture(); let mut g = game(&f);
    g.came_through_start_menu(3, 0.02);
    assert_eq!(value(&g, TITLE_VISITS), 3.0, "the real startup consumer must admit the source group-4 block");
    assert_eq!((value(&g, TITLE_BOOTS), value(&g, LEGACY_BOOTS)), (1.0, 1.0));
    assert_eq!((value(&g, TITLE_QUERY), value(&g, TITLE_LEGACY_QUERY)), (1.0, 1.0));
    assert_eq!((value(&g, LEGACY_VISITS), value(&g, ANY_VISITS), value(&g, GAME_VISITS)), (3.0, 3.0, 0.0));
    assert_eq!(g.dispatch_event("PostLoadGame", vec![XVal::Num(1.0)], None), 1);
    assert_eq!(value(&g, AFTER_LOAD), 1.0, "the registered source UDF must receive the load event");
    assert_eq!(query(&mut g, 4), XVal::Num(0.0), "the title context ends before the load recipient runs");
    clean(&g);
}

#[test]
fn ordinary_pause_pages_and_gameplay_keep_their_existing_admission() {
    let f = fixture(); let mut g = game(&f);
    g.open_menu(menus::START);
    g.frames(2);
    assert_eq!((value(&g, TITLE_VISITS), value(&g, LEGACY_VISITS), value(&g, ANY_VISITS)), (0.0, 2.0, 2.0));
    assert_eq!((query(&mut g, 4), query(&mut g, menus::START)), (XVal::Num(0.0), XVal::Num(1.0)));
    assert_eq!(value(&g, UNFILTERED_QUERY), 1.0, "the unfiltered query still sees an ordinary open pause menu");
    g.close_menu();
    g.open_menu(menus::MESSAGE);
    let clock = g.time;
    g.page_tick(menus::START);
    assert_eq!(g.menu, Some(menus::MESSAGE));
    assert_eq!(g.time, clock);
    assert_eq!((value(&g, TITLE_VISITS), value(&g, LEGACY_VISITS), value(&g, ANY_VISITS)), (0.0, 3.0, 2.0));
    assert_eq!(value(&g, LEGACY_TITLE_QUERY), 0.0, "a held pause page is not a title frame");
    g.close_menu();
    g.page_tick(menus::INVENTORY);
    assert_eq!((value(&g, TITLE_VISITS), value(&g, LEGACY_VISITS), value(&g, ANY_VISITS)), (0.0, 3.0, 2.0));
    g.tick();
    assert_eq!((value(&g, TITLE_VISITS), value(&g, GAME_VISITS)), (0.0, 1.0));
    assert_eq!(query(&mut g, 4), XVal::Num(0.0));
    clean(&g);
}

#[test]
fn title_queries_work_inside_retained_legacy_and_unfiltered_blocks() {
    let f = fixture(); let mut g = game(&f);
    g.came_through_start_menu(2, 0.02);
    assert_eq!(value(&g, LEGACY_TITLE_QUERY), 1.0, "group-4 function query must share the admitted startup context");
    assert_eq!(value(&g, ANY_TITLE_QUERY), 1.0);
    // The unfiltered function's existing held-page semantics are not changed here.
    assert_eq!(value(&g, UNFILTERED_QUERY), 0.0);
    assert_eq!((value(&g, LEGACY_VISITS), value(&g, ANY_VISITS)), (2.0, 2.0), "each retained block runs once per startup frame");
    g.page_tick(menus::START);
    assert_eq!(value(&g, LEGACY_TITLE_QUERY), 0.0);
    assert_eq!((value(&g, LEGACY_VISITS), value(&g, ANY_VISITS)), (3.0, 2.0));
    clean(&g);
}

#[test]
fn repeated_start_frames_keep_restart_consumption_and_restore_has_no_title_context() {
    let f = fixture(); let mut g = game(&f);
    g.came_through_start_menu(1, 0.02);
    assert_eq!(value(&g, TITLE_VISITS), 1.0);
    g.came_through_start_menu(2, 0.02);
    assert_eq!((value(&g, TITLE_VISITS), value(&g, TITLE_BOOTS), value(&g, LEGACY_BOOTS)), (3.0, 1.0, 1.0));
    assert_eq!(g.dispatch_event("PostLoadGame", vec![XVal::Num(1.0)], None), 1);
    let bytes = serde_json::to_vec(&g.checked_snapshot().unwrap()).unwrap();
    let snapshot: GameSnapshot = serde_json::from_slice(&bytes).unwrap();
    let world = World::restore(g.world.forms, g.world.placed, &snapshot.sim).unwrap();
    let mut back = Game::restore(g.data, world, g.cfg, &snapshot).unwrap();
    assert_eq!(query(&mut back, 4), XVal::Num(0.0));
    back.page_tick(menus::START);
    assert_eq!(value(&back, TITLE_VISITS), 3.0, "restoring does not retain the temporary startup phase");
    back.came_through_start_menu(1, 0.02);
    assert_eq!(value(&back, TITLE_VISITS), 4.0);
    assert_eq!(query(&mut back, 4), XVal::Num(0.0));
    clean(&g); clean(&back);
}
