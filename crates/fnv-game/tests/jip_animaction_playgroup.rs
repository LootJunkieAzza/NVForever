//! Live JIP `SetOnAnimActionEventHandler` (`animaction`) and
//! `SetOnPlayGroupEventHandler` (`playgroup`) firing, keyword/numeric filtering
//! (including literal 0), edge deduplication, and handler removal (Part of #2272;
//! docs/spec-notes/jip-events.md, docs/spec-notes/jip-required-animation-codes.md).

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::anim_state::{AnimAction, AnimState, MoveDir};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_ui::MemSource;
use serde_json::Value as Json;

const CELL: u32 = 0xC00;
const NPC_BASE: u32 = 0xC01;
const ACTOR_A: u32 = 0xC02;
const ACTOR_B: u32 = 0xC03;
const EMPTY_LIST: u32 = 0xC04;
const ACTOR_A_LIST: u32 = 0xC05;
const ACTI_BASE: u32 = 0xC06;
const ACTI_REF: u32 = 0xC07;
const GROUP_GLOB: u32 = 0xC08;

fn edid(sig: &[u8; 4], fid: u32, name: &str, rest: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    record(sig, 0, fid, &[vec![sub(b"EDID", z(name))], rest.to_vec()].concat())
}

fn placed(sig: &[u8; 4], fid: u32, name: &str, base: u32) -> Vec<u8> {
    record(sig, 0, fid, &[sub(b"EDID", z(name)), sub(b"NAME", u32s(&[base])), sub(b"DATA", vec![0; 24])])
}

fn plugin_bytes() -> Vec<u8> {
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        edid(b"NPC_", NPC_BASE, "TestNpcBase", &[]),
    ]
    .concat();
    let actis = edid(b"ACTI", ACTI_BASE, "TestDoorActi", &[]);
    let globs = edid(
        b"GLOB",
        GROUP_GLOB,
        "TestGroupGlobal",
        &[sub(b"FNAM", vec![b'l']), sub(b"FLTV", 4.0f32.to_le_bytes().to_vec())],
    );
    let lists = [
        edid(b"FLST", EMPTY_LIST, "EmptyActorList", &[]),
        edid(b"FLST", ACTOR_A_LIST, "OnlyActorAList", &[sub(b"LNAM", u32s(&[ACTOR_A]))]),
    ]
    .concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [
        placed(b"ACHR", ACTOR_A, "TestActorA", NPC_BASE),
        placed(b"ACHR", ACTOR_B, "TestActorB", NPC_BASE),
        placed(b"REFR", ACTI_REF, "TestActiRef", ACTI_BASE),
    ]
    .concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"GLOB", &globs),
        top(b"NPC_", &npcs),
        top(b"ACTI", &actis),
        top(b"FLST", &lists),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "JipAnimEvents.esm".into()).unwrap()];
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
    let mut xml = String::from("<menu name=\"HUDMainMenu\"><rect name=\"Probe\">");
    for key in [
        "equip_count", "equip_code", "equip_live", "equip_a", "equip_b",
        "reload_a", "reload_code", "attack_b",
        "idle_n", "idle_code", "idle_a", "idle_b",
        "fwd_a", "fwd_code", "eq_n", "eq_code",
        "svar_n", "svar_code", "glob_n", "glob_code", "num0_n",
    ] {
        xml.push_str(&format!("<_{key}>0</_{key}>"));
    }
    xml.push_str("</rect></menu>");
    let src = MemSource::default().with("owned.xml", &xml);
    g.ui_tree.load_menu(&src, "owned.xml").unwrap();
    g.ui_tree.update();
    g
}

fn run_src(g: &mut Game<'_>, src: &str) -> XVal {
    g.run_query(src, RunCtx::default()).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn q(g: &mut Game<'_>, body: &str) -> XVal {
    run_src(g, &format!("Begin Function {{ }}\n{body}\nEnd\n"))
}

fn ui(g: &Game<'_>, key: &str) -> f64 {
    g.ui_tree.get_float(&format!("HUDMainMenu/Probe/_{key}")).unwrap()
}

#[test]
fn animaction_fires_on_state_transitions_with_exact_codes_filters_and_removal() {
    let f = fixture();
    let mut g = game(&f);

    // Register literal action 0 (Equip) for any actor via EmptyActorList, recording count,
    // code, live GetAnimAction seen inside the callback, and receiver.
    run_src(
        &mut g,
        "ref fn0\nBegin Function { }\n\
         let fn0 := (Begin Function {int action}\n\
           SetUIFloat \"HUDMainMenu/Probe/_equip_count\" ((GetUIFloat \"HUDMainMenu/Probe/_equip_count\") + 1)\n\
           SetUIFloat \"HUDMainMenu/Probe/_equip_code\" action\n\
           SetUIFloat \"HUDMainMenu/Probe/_equip_live\" (GetAnimAction)\n\
           SetUIFloat \"HUDMainMenu/Probe/_equip_a\" (GetSelf == TestActorA)\n\
           SetUIFloat \"HUDMainMenu/Probe/_equip_b\" (GetSelf == TestActorB)\n\
         End)\n\
         SetOnAnimActionEventHandler fn0 1 EmptyActorList 0\n\
         PlayerREF.AuxVarSetRef \"*fn_equip\" fn0\nEnd\n",
    );

    // Register action 9 (Reload) filtered specifically to OnlyActorAList (contains TestActorA).
    q(
        &mut g,
        "SetOnAnimActionEventHandler (Begin Function {int action}\n\
           SetUIFloat \"HUDMainMenu/Probe/_reload_a\" ((GetUIFloat \"HUDMainMenu/Probe/_reload_a\") + 1)\n\
           SetUIFloat \"HUDMainMenu/Probe/_reload_code\" action\n\
         End) 1 OnlyActorAList 9",
    );

    // Register action 2 (Attack) filtered specifically to TestActorB.
    q(
        &mut g,
        "SetOnAnimActionEventHandler (Begin Function {int action}\n\
           SetUIFloat \"HUDMainMenu/Probe/_attack_b\" ((GetUIFloat \"HUDMainMenu/Probe/_attack_b\") + 1)\n\
         End) 1 TestActorB 2",
    );

    // Enter Equip (0) on ACTOR_A -> fires equip handler once; live GetAnimAction == 0.
    g.set_anim_state(FormId(ACTOR_A), AnimState { action: Some(AnimAction::Equip), ..Default::default() });
    assert_eq!(ui(&g, "equip_count"), 1.0);
    assert_eq!(ui(&g, "equip_code"), 0.0);
    assert_eq!(ui(&g, "equip_live"), 0.0, "callback must see new live GetAnimAction");
    assert_eq!((ui(&g, "equip_a"), ui(&g, "equip_b")), (1.0, 0.0));
    assert_eq!(ui(&g, "reload_a"), 0.0);

    // Holding Equip across another frame does NOT re-fire.
    g.set_anim_state(FormId(ACTOR_A), AnimState { action: Some(AnimAction::Equip), ..Default::default() });
    assert_eq!(ui(&g, "equip_count"), 1.0, "steady state must not re-fire");

    // Transition to Reload (9) on ACTOR_A -> fires reload_a, does NOT fire equip (literal 0 is not wildcard!).
    g.set_anim_state(FormId(ACTOR_A), AnimState { action: Some(AnimAction::Reload), ..Default::default() });
    assert_eq!(ui(&g, "equip_count"), 1.0, "literal action 0 must not match action 9");
    assert_eq!(ui(&g, "reload_a"), 1.0);
    assert_eq!(ui(&g, "reload_code"), 9.0);

    // Transition to Reload (9) on ACTOR_B -> filtered out by OnlyActorAList.
    g.set_anim_state(FormId(ACTOR_B), AnimState { action: Some(AnimAction::Reload), ..Default::default() });
    assert_eq!(ui(&g, "reload_a"), 1.0);

    // Transition to Attack (2) on ACTOR_B -> fires attack_b; on ACTOR_A -> does not.
    g.set_anim_state(FormId(ACTOR_A), AnimState { action: Some(AnimAction::Attack), ..Default::default() });
    assert_eq!(ui(&g, "attack_b"), 0.0);
    g.set_anim_state(FormId(ACTOR_B), AnimState { action: Some(AnimAction::Attack), ..Default::default() });
    assert_eq!(ui(&g, "attack_b"), 1.0);

    // Clearing to None and re-entering Equip (0) on ACTOR_B fires equip handler again.
    g.set_anim_state(FormId(ACTOR_B), AnimState { action: None, ..Default::default() });
    g.set_anim_state(FormId(ACTOR_B), AnimState { action: Some(AnimAction::Equip), ..Default::default() });
    assert_eq!(ui(&g, "equip_count"), 2.0);
    assert_eq!((ui(&g, "equip_a"), ui(&g, "equip_b")), (0.0, 1.0));

    // Remove Equip handler and verify Equip no longer fires.
    run_src(&mut g, "ref fn0\nBegin Function { }\nlet fn0 := PlayerREF.AuxVarGetRef \"*fn_equip\"\nSetOnAnimActionEventHandler fn0 0 EmptyActorList 0\nEnd\n");
    g.set_anim_state(FormId(ACTOR_A), AnimState { action: None, ..Default::default() });
    g.set_anim_state(FormId(ACTOR_A), AnimState { action: Some(AnimAction::Equip), ..Default::default() });
    assert_eq!(ui(&g, "equip_count"), 2.0, "removed handler must not fire");
}

#[test]
fn playgroup_fires_from_script_and_locomotion_with_keywords_and_literal_zero() {
    let f = fixture();
    let mut g = game(&f);
    g.anims.driven = true;

    // Register with bare keyword `Idle` (0) on EmptyActorList -> literal 0, not wildcard!
    run_src(
        &mut g,
        "ref fnIdle\nBegin Function { }\n\
         let fnIdle := (Begin Function {int grp}\n\
           SetUIFloat \"HUDMainMenu/Probe/_idle_n\" ((GetUIFloat \"HUDMainMenu/Probe/_idle_n\") + 1)\n\
           SetUIFloat \"HUDMainMenu/Probe/_idle_code\" grp\n\
           SetUIFloat \"HUDMainMenu/Probe/_idle_a\" (GetSelf == TestActorA)\n\
           SetUIFloat \"HUDMainMenu/Probe/_idle_b\" (GetSelf == TestActorB)\n\
         End)\n\
         SetOnPlayGroupEventHandler fnIdle 1 EmptyActorList Idle\n\
         PlayerREF.AuxVarSetRef \"*fn_idle\" fnIdle\nEnd\n",
    );

    // Register with bare keyword `Forward` (3) on TestActorA.
    q(
        &mut g,
        "SetOnPlayGroupEventHandler (Begin Function {int grp}\n\
           SetUIFloat \"HUDMainMenu/Probe/_fwd_a\" ((GetUIFloat \"HUDMainMenu/Probe/_fwd_a\") + 1)\n\
           SetUIFloat \"HUDMainMenu/Probe/_fwd_code\" grp\n\
         End) 1 TestActorA Forward",
    );

    // Register with quoted string `"Equip"` (24) on EmptyActorList -> stays XVal::Str at compile
    // time and is normalized to numeric 24 at runtime in Game::jip_set.
    run_src(
        &mut g,
        "ref fnEq\nBegin Function { }\n\
         let fnEq := (Begin Function {int grp}\n\
           SetUIFloat \"HUDMainMenu/Probe/_eq_n\" ((GetUIFloat \"HUDMainMenu/Probe/_eq_n\") + 1)\n\
           SetUIFloat \"HUDMainMenu/Probe/_eq_code\" grp\n\
         End)\n\
         SetOnPlayGroupEventHandler fnEq 1 EmptyActorList \"Equip\"\n\
         PlayerREF.AuxVarSetRef \"*fn_eq\" fnEq\nEnd\n",
    );

    // Register with runtime string_var `"FastForward"` (7) -> exercises Game::jip_set XVal::Str
    // normalization for dynamic strings too.
    run_src(
        &mut g,
        "string_var sGrp\nBegin Function { }\n\
         let sGrp := \"FastForward\"\n\
         SetOnPlayGroupEventHandler (Begin Function {int grp}\n\
           SetUIFloat \"HUDMainMenu/Probe/_svar_n\" ((GetUIFloat \"HUDMainMenu/Probe/_svar_n\") + 1)\n\
           SetUIFloat \"HUDMainMenu/Probe/_svar_code\" grp\n\
         End) 1 EmptyActorList sGrp\nEnd\n",
    );

    // Register with non-local global variable `TestGroupGlobal` (value 4 = Backward) and
    // explicit numeric code `0` (Idle).
    q(
        &mut g,
        "SetOnPlayGroupEventHandler (Begin Function {int grp}\n\
           SetUIFloat \"HUDMainMenu/Probe/_glob_n\" ((GetUIFloat \"HUDMainMenu/Probe/_glob_n\") + 1)\n\
           SetUIFloat \"HUDMainMenu/Probe/_glob_code\" grp\n\
         End) 1 EmptyActorList TestGroupGlobal",
    );
    q(
        &mut g,
        "SetOnPlayGroupEventHandler (Begin Function {int grp}\n\
           SetUIFloat \"HUDMainMenu/Probe/_num0_n\" ((GetUIFloat \"HUDMainMenu/Probe/_num0_n\") + 1)\n\
         End) 1 TestActorB 0",
    );

    // Script `PlayGroup Forward 1` on TestActorA -> fires Forward (3), does NOT fire Idle (0).
    q(&mut g, "TestActorA.PlayGroup Forward 1");
    assert_eq!(ui(&g, "fwd_a"), 1.0);
    assert_eq!(ui(&g, "fwd_code"), 3.0);
    assert_eq!(ui(&g, "idle_n"), 0.0, "Idle (0) must not act as wildcard for Forward (3)");
    assert_eq!(ui(&g, "num0_n"), 0.0, "numeric 0 must not act as wildcard");

    // Repeating `PlayGroup Forward 0` (Start::Normal) while Forward is already playing does not spam.
    q(&mut g, "TestActorA.PlayGroup Forward 0");
    assert_eq!(ui(&g, "fwd_a"), 1.0);
    // `PlayGroup Forward 1` (Start::Immediate) restarts and fires again.
    q(&mut g, "TestActorA.PlayGroup Forward 1");
    assert_eq!(ui(&g, "fwd_a"), 2.0);

    // `PlayGroupAlt Equip 1` on TestActorB -> fires Equip (24) handler registered via quoted string.
    q(&mut g, "TestActorB.PlayGroupAlt Equip 1");
    assert_eq!(ui(&g, "eq_n"), 1.0);
    assert_eq!(ui(&g, "eq_code"), 24.0);

    // `PlayGroup FastForward 1` -> fires string_var handler (7); `PlayGroup Backward 1` -> fires
    // global-variable handler (4).
    q(&mut g, "TestActorA.PlayGroup FastForward 1");
    assert_eq!((ui(&g, "svar_n"), ui(&g, "svar_code")), (1.0, 7.0));
    q(&mut g, "TestActorA.PlayGroup Backward 1");
    assert_eq!((ui(&g, "glob_n"), ui(&g, "glob_code")), (1.0, 4.0));

    // `PlayGroup Idle 1` on TestActorB -> fires Idle (0) handlers registered via bare keyword and numeric 0.
    q(&mut g, "TestActorB.PlayGroup Idle 1");
    assert_eq!(ui(&g, "idle_n"), 1.0);
    assert_eq!(ui(&g, "idle_code"), 0.0);
    assert_eq!(ui(&g, "num0_n"), 1.0);
    assert_eq!((ui(&g, "idle_a"), ui(&g, "idle_b")), (0.0, 1.0));

    // Locomotion group transitions via set_anim_state: None -> Idle (0) -> Idle (no re-fire) -> Forward (3).
    g.set_anim_state(FormId(ACTOR_A), AnimState { moving: MoveDir::None, group: Some("Idle"), ..Default::default() });
    assert_eq!(ui(&g, "idle_n"), 2.0);
    assert_eq!((ui(&g, "idle_a"), ui(&g, "idle_b")), (1.0, 0.0));
    g.set_anim_state(FormId(ACTOR_A), AnimState { moving: MoveDir::None, group: Some("Idle"), ..Default::default() });
    assert_eq!(ui(&g, "idle_n"), 2.0, "holding same locomotion group must not re-fire");
    g.set_anim_state(FormId(ACTOR_A), AnimState { moving: MoveDir::Forward, group: Some("Forward"), ..Default::default() });
    assert_eq!(ui(&g, "fwd_a"), 3.0);

    // Sentinel groups (PlayAnimSequence, activator set_ref_open) must NOT fire playgroup.
    q(&mut g, "TestActorA.PlayAnimSequence \"SomeSequence\"");
    let _ = g.set_ref_open(FormId(ACTI_REF), true);
    assert_eq!(ui(&g, "idle_n"), 2.0);
    assert_eq!(ui(&g, "fwd_a"), 3.0);
    assert_eq!(ui(&g, "eq_n"), 1.0);

    // Remove Equip handler using numeric code 24 (matching the string `"Equip"` registration)
    // and Idle handler using bare keyword `Idle`, verifying both stop firing.
    run_src(&mut g, "ref fnEq\nBegin Function { }\nlet fnEq := PlayerREF.AuxVarGetRef \"*fn_eq\"\nSetOnPlayGroupEventHandler fnEq 0 EmptyActorList 24\nEnd\n");
    run_src(&mut g, "ref fnIdle\nBegin Function { }\nlet fnIdle := PlayerREF.AuxVarGetRef \"*fn_idle\"\nSetOnPlayGroupEventHandler fnIdle 0 EmptyActorList Idle\nEnd\n");
    q(&mut g, "TestActorB.PlayGroup Equip 1");
    q(&mut g, "TestActorB.PlayGroup Idle 1");
    assert_eq!(ui(&g, "eq_n"), 1.0, "removed Equip handler must not fire");
    assert_eq!(ui(&g, "idle_n"), 2.0, "removed Idle handler must not fire");
}
