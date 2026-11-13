//! The unlisted JIP LN commands (docs/spec-notes/ext-unlisted-commands.md) on a synthetic
//! plugin: the equipment lock, teammate ammo, the CCC mod's faction, distance, location
//! and reputation getters, position strings, the centre of the bounds, ListToArray,
//! KillActorAlt, AttachLight on the node mirror and the answers of the rest.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::nif_blocks::{SceneEdit, SceneNode};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const NPC: u32 = 0x900;
const NPC_REF: u32 = 0x901;
const BOX: u32 = 0x902;
const BOX_REF: u32 = 0x903;
const FACT_TOWN: u32 = 0x904;
const FACT_OTHER: u32 = 0x905;
const REPU: u32 = 0x906;
const WEAP: u32 = 0x907;
const LIGH: u32 = 0x908;
const FLST: u32 = 0x909;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn edid(sig: &[u8; 4], fid: u32, name: &str, rest: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    record(sig, 0, fid, &[vec![sub(b"EDID", z(name))], rest.to_vec()].concat())
}

fn placed(sig: &[u8; 4], fid: u32, name: &str, base: u32, pos: [f32; 3]) -> Vec<u8> {
    let data = [f32b(pos[0]), f32b(pos[1]), f32b(pos[2]), vec![0; 12]].concat();
    record(sig, 0, fid, &[sub(b"EDID", z(name)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn i16s(v: &[i16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn plugin_bytes() -> Vec<u8> {
    // The NPC is in the town faction by its record (rank 0).
    let snam = [FACT_TOWN.to_le_bytes().to_vec(), vec![0, 0, 0, 0]].concat();
    let npcs = [record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), edid(b"NPC_", NPC, "TestNpc", &[sub(b"SNAM", snam)])].concat();
    let facts = [
        edid(b"FACT", FACT_TOWN, "TownFaction", &[sub(b"WMI1", REPU.to_le_bytes().to_vec())]),
        edid(b"FACT", FACT_OTHER, "OtherFaction", &[]),
    ]
    .concat();
    let repu = edid(b"REPU", REPU, "RepTown", &[]);
    // A box 22 x 66 x 23 from its foot.
    let cont = edid(b"CONT", BOX, "TestBox", &[sub(b"OBND", i16s(&[-11, -33, 0, 11, 33, 23]))]);
    let weap = edid(b"WEAP", WEAP, "TestGun", &[]);
    let ligh = edid(b"LIGH", LIGH, "TestLight", &[]);
    let flst = edid(b"FLST", FLST, "TestList", &[sub(b"LNAM", WEAP.to_le_bytes().to_vec()), sub(b"LNAM", LIGH.to_le_bytes().to_vec())]);
    // An interior cell with a name.
    let cell = edid(b"CELL", CELL, "TestCell", &[sub(b"FULL", z("Test Room")), sub(b"DATA", vec![1])]);
    let refs = [placed(b"ACHR", NPC_REF, "NpcRef", NPC, [0.0, 0.0, 0.0]), placed(b"REFR", BOX_REF, "BoxRef", BOX, [300.0, 400.0, 100.0])].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"FACT", &facts),
        top(b"REPU", &repu),
        top(b"NPC_", &npcs),
        top(b"CONT", &cont),
        top(b"WEAP", &weap),
        top(b"LIGH", &ligh),
        top(b"FLST", &flst),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Unlisted.esm".into()).unwrap()];
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

const VARS: &str = "float fX\nref rItem\nstring_var sOut\narray_var aList\n";

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

fn s(g: &mut Game, expr: &str) -> String {
    match q(g, &format!("let sOut := {expr}\nSetFunctionValue sOut")) {
        XVal::Str(s) => s,
        v => panic!("{expr}: {v:?}"),
    }
}

#[test]
fn lock_equipment_is_kept_and_equipping_still_works() {
    let f = fixture();
    let mut g = game(&f);
    n(&mut g, "NpcRef.AddItem TestGun 1");
    assert_eq!(n(&mut g, "NpcRef.LockEquipment 1"), 0.0);
    assert!(g.equipment_locked(FormId(NPC_REF)));
    q(&mut g, "NpcRef.EquipItem TestGun");
    assert_eq!(n(&mut g, "NpcRef.GetEquipped TestGun"), 1.0, "a script still equips");
    n(&mut g, "NpcRef.LockEquipment 0");
    assert!(!g.equipment_locked(FormId(NPC_REF)));
}

#[test]
fn teammate_ammo_reads_one_until_set() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "NpcRef.GetTeammateUsingAmmo"), 1.0);
    assert_eq!(n(&mut g, "NpcRef.SetTeammateUsingAmmo 0"), 0.0);
    assert_eq!(n(&mut g, "NpcRef.GetTeammateUsingAmmo"), 0.0);
    n(&mut g, "NpcRef.SetTeammateUsingAmmo 1");
    assert_eq!(n(&mut g, "NpcRef.GetTeammateUsingAmmo"), 1.0);
    assert_eq!(n(&mut g, "PlayerREF.GetTeammateUsingAmmo"), 1.0);
}

#[test]
fn ccc_distance_modes() {
    let f = fixture();
    let mut g = game(&f);
    // 300 400 100 apart: across the ground 500, the height 100, from either end.
    assert!((n(&mut g, "NpcRef.CCCGetDistance BoxRef 3") - 500.0).abs() < 1e-3);
    assert!((n(&mut g, "NpcRef.CCCGetDistance BoxRef 0") - 500.0).abs() < 1e-3);
    assert!((n(&mut g, "NpcRef.CCCGetDistance BoxRef 4") - 100.0).abs() < 1e-3);
    assert!((n(&mut g, "BoxRef.CCCGetDistance NpcRef 4") - 100.0).abs() < 1e-3);
}

#[test]
fn ccc_in_faction_counts_script_ranks_unless_asked_for_the_base() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "NpcRef.GetInFaction TownFaction"), 1.0);
    assert_eq!(n(&mut g, "NpcRef.CCCInFaction TownFaction"), 0.0, "the record's faction alone");
    assert_eq!(n(&mut g, "NpcRef.CCCInFaction TownFaction 1"), 1.0, "with the base");
    assert_eq!(n(&mut g, "NpcRef.CCCInFaction OtherFaction"), 0.0);
    n(&mut g, "NpcRef.SetFactionRank OtherFaction 3");
    assert_eq!(n(&mut g, "NpcRef.CCCInFaction OtherFaction"), 1.0);
    n(&mut g, "NpcRef.SetFactionRank OtherFaction -1");
    assert_eq!(n(&mut g, "NpcRef.CCCInFaction OtherFaction"), 0.0);
    assert_eq!(n(&mut g, "NpcRef.CCCInFaction OtherFaction 1"), 0.0);
}

#[test]
fn ccc_reputation_and_location() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "NpcRef.CCCGetReputation"), f64::from(REPU));
    assert_eq!(n(&mut g, "BoxRef.CCCGetReputation"), 0.0);
    let place = s(&mut g, "NpcRef.CCCLocationName");
    assert!(place.ends_with("Test Room") && place.len() > "Test Room".len(), "{place}");
}

#[test]
fn position_strings_round_trip() {
    let f = fixture();
    let mut g = game(&f);
    // Z one above its whole part.
    assert_eq!(s(&mut g, "BoxRef.RefToPosStr"), format!("{CELL:X} 300 400 101"));
    // Whole numbers, cut toward zero; words after the numbers do not matter.
    assert_eq!(n(&mut g, "NpcRef.MoveToPosStr \"200 10.7 -20.9 30.9 5\""), 1.0);
    assert_eq!(n(&mut g, "NpcRef.GetPos Y"), -20.0);
    assert_eq!(s(&mut g, "NpcRef.RefToPosStr"), format!("{CELL:X} 10 -20 31"));
    assert_eq!(n(&mut g, "NpcRef.MoveToPosStr \"00000200 1 2 3\""), 1.0, "leading zeros");
    // Not a cell, or too few numbers: refused.
    assert_eq!(n(&mut g, "NpcRef.MoveToPosStr \"907 1 2 3\""), 0.0);
    assert_eq!(n(&mut g, "NpcRef.MoveToPosStr \"200 1 2\""), 0.0);
    assert_eq!(n(&mut g, "NpcRef.MoveToPosStr \"nothing here\""), 0.0);
}

#[test]
fn center_of_the_bounds() {
    let f = fixture();
    let mut g = game(&f);
    assert!((n(&mut g, "BoxRef.GetCenterPos Z") - 111.5).abs() < 1e-3);
    assert!((n(&mut g, "BoxRef.GetCenterPos X") - 300.0).abs() < 1e-3);
    assert!((n(&mut g, "BoxRef.GetCenterPos Y") - 400.0).abs() < 1e-3);
}

#[test]
fn list_to_array_in_list_order() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "ar_Size (ListToArray TestList)"), 2.0);
    q(&mut g, "let aList := ListToArray TestList\nlet rItem := aList[1]\nSetFunctionValue rItem");
    assert_eq!(n(&mut g, "ar_Size (ListToArray TestGun)"), -1.0, "not a list: no array");
}

#[test]
fn kill_actor_alt_kills_with_the_killer() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "NpcRef.KillActorAlt PlayerREF"), 0.0);
    assert_eq!(n(&mut g, "NpcRef.GetDead"), 1.0);
}

#[test]
fn attach_light_adds_a_node_and_remove_takes_it() {
    let f = fixture();
    let mut g = game(&f);
    let node = |name: &str, parent| SceneNode { name: name.into(), parent, translation: [0.0; 3], rotation: [0.0; 3], scale: 1.0, flags: 0, material: None, info: None };
    // The engine tells the tree once the reference is asked about.
    n(&mut g, "NpcRef.GetNifBlockScale \"Bip01 Head\"");
    g.scene.set_tree(FormId(NPC_REF), vec![node("Bip01", None), node("Bip01 Head", Some(0))]);
    g.scene.take_edits();
    assert_eq!(n(&mut g, "NpcRef.AttachLight \"NoSuchNode\" TestLight 0 0 0"), 0.0);
    assert_eq!(n(&mut g, "NpcRef.AttachLight \"Bip01 Head\" TestGun 0 0 0"), 0.0, "not a light");
    assert_eq!(n(&mut g, "NpcRef.AttachLight \"Bip01 Head\" TestLight 0 0 10"), 1.0);
    let lname = fnv_game::nvse_unlisted::light_node_name(FormId(LIGH));
    assert_eq!(n(&mut g, &format!("NpcRef.GetNifBlockScale \"{lname}\"")), 1.0, "the light's node is in the mirror");
    let edits = g.scene.take_edits();
    assert!(edits.iter().any(|e| matches!(e, SceneEdit::Light { target, light, offset, .. } if target == "Bip01 Head" && light.0 == LIGH && offset[2] == 10.0)), "{edits:?}");
    // Again on the same node: still 1, one light there.
    assert_eq!(n(&mut g, "NpcRef.AttachLight \"Bip01 Head\" TestLight 0 0 10"), 1.0);
    assert_eq!(n(&mut g, "NpcRef.RemoveLight \"Bip01 Head\""), 0.0);
    assert_eq!(n(&mut g, &format!("NpcRef.GetNifBlockScale \"{lname}\"")), 0.0, "gone from the mirror");
    assert_eq!(n(&mut g, "NpcRef.RemoveLight \"Bip01 Head\""), 0.0);
}

#[test]
fn the_rest_answer_as_observed() {
    let f = fixture();
    let mut g = game(&f);
    assert!((n(&mut g, "GetConditionDamagePenalty") - 0.67).abs() < 1e-9);
    assert_eq!(s(&mut g, "NpcRef.GetHitNode"), "NULL");
    for call in [
        "NpcRef.RefreshAnimData",
        "NpcRef.MoveAwayFromPlayer",
        "NpcRef.ToggleDetectionFix 1",
        "NpcRef.ToggleIgnoreLockedDoors 1",
        "BoxRef.DisableScriptedActivate 1",
        "CCCSetFloat 9 1",
        "CCCSetTrait 40 1",
        "NpcRef.CCCSetEquipped",
        "NpcRef.CCCSetNCCS 1",
        "CCCSMS NpcRef",
        "NpcRef.CCCSetFollowState 1",
        "CCCLoadNCCS",
        "CCCOnLoad",
    ] {
        assert_eq!(n(&mut g, call), 0.0, "{call}");
    }
    assert_eq!(n(&mut g, "CCCSavedForm 0"), 0.0);
    q(&mut g, "CCCSetString 112 \"Attack %n\" NpcRef");
}
