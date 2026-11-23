//! Integration tests for `SetPCToddler` and `SetPCYoung` (#3025):
//! walk-set selection, appearance rebuild notification, and save/restore persistence
//! (`docs/spec-notes/pc-toddler-young.md`).

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::appearance::PlayerWalkSet;
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let bytes = [
        tes4(),
        top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))])),
        top(
            b"CELL",
            &group(
                0i32.to_le_bytes(),
                2,
                &group(0i32.to_le_bytes(), 3, &record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell")), sub(b"DATA", vec![1])])),
            ),
        ),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "PcToddlerYoung.esm".into()).unwrap()];
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
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g
}

fn run(g: &mut Game, body: &str) {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default())
        .unwrap_or_else(|e| panic!("{body}: {e}"));
}

#[test]
fn set_pc_toddler_and_young_drive_walk_set_appearance_rebuild_and_save_roundtrip() {
    let f = fixture();
    let mut g = game(&f);

    // Baseline: neither flag set, normal walk set, no spurious appearance changes.
    assert!(!g.pc_toddler());
    assert!(!g.pc_young());
    assert_eq!(g.player_walk_set(), PlayerWalkSet::Normal);
    assert!(g.take_appearance_changes().is_empty());

    // SetPCToddler 1 switches walk set to Toddler; SetPCYoung 1 flags young and queues one player body rebuild.
    run(&mut g, "SetPCToddler 1\nSetPCYoung 1");
    assert!(g.pc_toddler());
    assert!(g.pc_young());
    assert_eq!(g.player_walk_set(), PlayerWalkSet::Toddler);
    let changes = g.take_appearance_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].actor, FormId::PLAYER_REF);
    assert!(!changes[0].reset);

    // Repeating SetPCYoung 1 does not queue a redundant rebuild.
    run(&mut g, "SetPCYoung 1");
    assert!(g.take_appearance_changes().is_empty());

    // Save/restore snapshot preserves both flags.
    let snap = g.snapshot();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut restored = Game::restore(&f.data, world, GameConfig { strict: true, ..GameConfig::default() }, &snap).unwrap();
    assert!(restored.pc_toddler());
    assert!(restored.pc_young());
    assert_eq!(restored.player_walk_set(), PlayerWalkSet::Toddler);

    // Clearing both flags restores normal walk set and queues an appearance rebuild to restore adult hair/head_parts.
    run(&mut restored, "SetPCToddler 0\nSetPCYoung 0");
    assert!(!restored.pc_toddler());
    assert!(!restored.pc_young());
    assert_eq!(restored.player_walk_set(), PlayerWalkSet::Normal);
    let cleared = restored.take_appearance_changes();
    assert_eq!(cleared.len(), 1);
    assert_eq!(cleared[0].actor, FormId::PLAYER_REF);
}
