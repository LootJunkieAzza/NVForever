//! Extender functions that move a container's items (RemoveItemTarget, RemoveMeIRAlt,
//! RemoveMeIR), as Just Assorted Mods' loot menu takes with them: the items arrive as the
//! game's own moves do, OnAdd included, and a note becomes one of the player's notes
//! (`fnv_game::functions::items::script_move_items`).

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const CHIP: u32 = 0x930;
const CHIP_SCRIPT: u32 = 0x931;
const MATCHBOOK: u32 = 0x932;
const CRATE: u32 = 0x940;
const CRATE_REF: u32 = 0x941;
const QUEST: u32 = 0x920;
const QSCRIPT: u32 = 0x921;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn placed(sig: &[u8; 4], fid: u32, edid: &str, base: u32, pos: [f32; 3], extra: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    let data = [f32b(pos[0]), f32b(pos[1]), f32b(pos[2]), vec![0; 12]].concat();
    record(sig, 0, fid, &[vec![sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec())], extra.to_vec(), vec![sub(b"DATA", data)]].concat())
}

/// A script compiled from its source at load (it declares a string variable).
fn source_script(fid: u32, edid: &str, source: &str) -> Vec<u8> {
    let mut subs = unit(0, &script(&[(0, None, Vec::new())]), &[], &[]);
    subs.push(sub(b"SCTX", format!("scn {edid}\n{source}").into_bytes()));
    record(b"SCPT", 0, fid, &[vec![sub(b"EDID", z(edid))], subs].concat())
}


fn plugin_bytes() -> Vec<u8> {
    let npcs = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let scripts = [
        source_script(QSCRIPT, "TestQuestScript", "float fOwn\nstring_var sText\nbegin GameMode\nend\n"),
        // A string variable makes the fixture compile the script from its source.
        source_script(CHIP_SCRIPT, "TestChipScript", "string_var sText\nbegin OnAdd\nSetStage TestQuest 10\nend\n"),
    ]
    .concat();
    let misc = record(b"MISC", 0, CHIP, &[sub(b"EDID", z("TestChip")), sub(b"FULL", z("Test Chip")), sub(b"SCRI", CHIP_SCRIPT.to_le_bytes().to_vec()), sub(b"DATA", [10i32.to_le_bytes().to_vec(), f32b(0.0)].concat())]);
    let note = record(b"NOTE", 0, MATCHBOOK, &[sub(b"EDID", z("TestMatchbook")), sub(b"FULL", z("Test Matchbook")), sub(b"DATA", vec![1])]);
    let cont = record(b"CONT", 0, CRATE, &[sub(b"EDID", z("TestCrate")), sub(b"DATA", [vec![0], f32b(0.0)].concat())]);
    let quest = record(
        b"QUST",
        0,
        QUEST,
        &[
            sub(b"EDID", z("TestQuest")),
            sub(b"SCRI", QSCRIPT.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0, 0, 0, 0, 0, 0, 0, 0]),
            sub(b"INDX", 10i16.to_le_bytes().to_vec()),
            sub(b"QSDT", vec![0]),
        ],
    );
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = placed(b"REFR", CRATE_REF, "CrateRef", CRATE, [0.0, 0.0, 0.0], &[]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"SCPT", &scripts),
        top(b"NPC_", &npcs),
        top(b"MISC", &misc),
        top(b"NOTE", &note),
        top(b"CONT", &cont),
        top(b"QUST", &quest),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "ItemMoves.esm".into()).unwrap()];
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

const VARS: &str = "float fX\nref rItem\narray_var aList\n";

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("{VARS}Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn n(g: &mut Game, expr: &str) -> f64 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        XVal::Ref(r) => f64::from(r),
        v => panic!("{expr}: {v:?}"),
    }
}

fn count(g: &mut Game, holder: u32, item: u32) -> i32 {
    g.world.item_count(FormId(holder), FormId(item)).unwrap()
}

#[test]
fn a_scripted_item_taken_with_remove_item_target_runs_its_on_add() {
    let f = fixture();
    let mut g = game(&f);
    g.world.add_item(FormId(CRATE_REF), FormId(CHIP), 1).unwrap();
    assert_eq!(n(&mut g, "GetStage TestQuest"), 0.0);
    q(&mut g, "CrateRef.RemoveItemTarget TestChip player 1");
    assert_eq!((count(&mut g, CRATE_REF, CHIP), count(&mut g, 0x14, CHIP)), (0, 1));
    // The chip's OnAdd ran in the player's inventory: its quest stage is set.
    assert_eq!(n(&mut g, "GetStage TestQuest"), 10.0);
}

#[test]
fn an_inventory_reference_moved_to_the_player_runs_on_add_and_a_note_is_filed() {
    let f = fixture();
    let mut g = game(&f);
    g.world.add_item(FormId(CRATE_REF), FormId(CHIP), 2).unwrap();
    g.world.add_item(FormId(CRATE_REF), FormId(MATCHBOOK), 1).unwrap();
    q(&mut g, "let aList := CrateRef.GetAllItemRefs 31\nlet rItem := aList[0]\nrItem.RemoveMeIRAlt 0 0 player");
    assert_eq!((count(&mut g, CRATE_REF, CHIP), count(&mut g, 0x14, CHIP)), (0, 2));
    assert_eq!(n(&mut g, "GetStage TestQuest"), 10.0);
    // A note in a container counts there, as the loot menu reads it before taking.
    assert_eq!(n(&mut g, "CrateRef.GetItemCount TestMatchbook"), 1.0);
    // The note leaves the crate and, carried, is one of the player's notes.
    q(&mut g, "CrateRef.RemoveItemTarget TestMatchbook player 0");
    assert_eq!(count(&mut g, CRATE_REF, MATCHBOOK), 0);
    g.file_carried_notes();
    assert_eq!(n(&mut g, "player.GetHasNote TestMatchbook"), 1.0);
}
