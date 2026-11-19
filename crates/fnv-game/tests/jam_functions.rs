//! Just Assorted Mods' extender functions and events (fnv_game::nvse_jam), on an empty
//! plugin (no game data).

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::nvse_jam::ViewTold;
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&tes4(), "Empty.esm".into()).unwrap()];
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
    Game::new(&f.data, world, GameConfig::default())
}

fn ctx() -> RunCtx {
    RunCtx { self_ref: Some(FormId::PLAYER_REF), ..Default::default() }
}

fn aux(g: &mut Game, name: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue (AuxVarGetFlt \"{name}\" 0)\nEnd\n"), ctx()).unwrap()
}

#[test]
fn key_down_and_up_reach_jip_handlers_with_the_code() {
    let f = fixture();
    let mut g = game(&f);
    let reg = "Begin Function { }\nSetOnKeyDownEventHandler (Begin Function {int k}\nPlayerRef.AuxVarSetFlt \"*d\" k 0\nEnd) 1 30\nSetOnKeyUpEventHandler (Begin Function {int k}\nPlayerRef.AuxVarSetFlt \"*u\" k 0\nEnd) 1 30\nSetOnKeyDownEventHandler (Begin Function {int k}\nPlayerRef.AuxVarSetFlt \"*w\" k 0\nEnd) 1 264\nEnd\n";
    g.run_query(reg, ctx()).unwrap();
    // A key other than the filter's runs nothing.
    g.input.held.insert(31);
    g.tick();
    assert_eq!(aux(&mut g, "*d"), XVal::Num(0.0));
    // A disabled key still sends it (JLM disables its keys and reads them here).
    g.input.disabled_keys.insert(30);
    g.input.held.insert(30);
    g.tick();
    assert_eq!(aux(&mut g, "*d"), XVal::Num(30.0));
    assert_eq!(aux(&mut g, "*u"), XVal::Num(0.0));
    // Held on: no second key down.
    g.run_query("Begin Function { }\nPlayerRef.AuxVarSetFlt \"*d\" 0 0\nEnd\n", ctx()).unwrap();
    g.tick();
    assert_eq!(aux(&mut g, "*d"), XVal::Num(0.0));
    g.input.held.clear();
    g.tick();
    assert_eq!(aux(&mut g, "*u"), XVal::Num(30.0));
    // The wheel (264 up) as a key.
    g.input.held.insert(264);
    g.tick();
    assert_eq!(aux(&mut g, "*w"), XVal::Num(264.0));
}

/// One press is one key-down, whichever sender sees it first: the engine's own
/// (`nvf_play::script_input`, before the frame) or the frame's. A toggle handler (JAM's
/// bullet time, JBTToggle 1 on V) went on and off in one press when two senders each sent it.
#[test]
fn one_press_is_one_key_down() {
    let f = fixture();
    let mut g = game(&f);
    let reg = "Begin Function { }\nSetOnKeyDownEventHandler (Begin Function {int k}\nPlayerRef.AuxVarSetFlt \"*n\" ((PlayerRef.AuxVarGetFlt \"*n\" 0) + 1) 0\nEnd) 1 47\nEnd\n";
    g.run_query(reg, ctx()).unwrap();
    // Sent by the frame alone.
    g.input.held.insert(47);
    g.tick();
    g.tick();
    assert_eq!(aux(&mut g, "*n"), XVal::Num(1.0));
    g.input.held.clear();
    g.tick();
    // Sent before the frame (as the engine does), then the frame.
    g.input.held.insert(47);
    g.jam_key_events();
    g.tick();
    assert_eq!(aux(&mut g, "*n"), XVal::Num(2.0));
}

#[test]
#[ignore = "release 0.7.0: main's JIP 9 / JohnnyGuitar 7, 10 and NIF modules answer these names first (owner, 1110 + mod support: their version wins); tested in jip9.rs, jg7.rs, jg10.rs, jg_nif.rs"]
fn toggles_and_highlights() {
    let f = fixture();
    let mut g = game(&f);
    let q = |g: &mut Game, s: &str| g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue ({s})\nEnd\n"), ctx()).unwrap();
    assert_eq!(q(&mut g, "ToggleVanityWheel"), XVal::Num(1.0));
    assert_eq!(q(&mut g, "ToggleVanityWheel 0"), XVal::Num(0.0));
    assert!(g.jam.vanity_wheel_off);
    assert_eq!(q(&mut g, "ToggleVanityWheel 1"), XVal::Num(1.0));
    q(&mut g, "ToggleDisableSaves 1");
    assert!(g.jam.saves_disabled);
    q(&mut g, "ToggleDisableSaves 0");
    assert!(!g.jam.saves_disabled);
    assert_eq!(q(&mut g, "PlayerRef.HighlightBodyPartAlt 2"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "PlayerRef.DeactivateAllHighlightsAlt"), XVal::Num(0.0));
}

/// A camera at the origin looking north (game +Y), 90° each way, Bevy's reverse-Z
/// perspective (w = view depth).
fn north() -> ViewTold {
    ViewTold { clip_from_world: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0] }
}

#[test]
#[ignore = "release 0.7.0: main's JIP 9 / JohnnyGuitar 7, 10 and NIF modules answer these names first (owner, 1110 + mod support: their version wins); tested in jip9.rs, jg7.rs, jg10.rs, jg_nif.rs"]
fn world_to_screen_writes_the_screen_fractions() {
    let f = fixture();
    let mut g = game(&f);
    let src = |args: &str| format!("float x\nfloat y\nfloat z\nint on\nBegin Function {{ }}\nlet on := WorldToScreen x y z {args}\nSetFunctionValue Sv_Construct \"%g %g %g %g\" on x y z\nEnd\n");
    // No camera told: off screen at -1.
    assert_eq!(g.run_query(&src("0 100 0 0"), ctx()), Ok(XVal::Str("0 -1 -1 0".into())));
    g.jam.view = Some(north());
    assert_eq!(g.run_query(&src("0 100 0 0"), ctx()), Ok(XVal::Str("1 0.5 0.5 100".into())));
    // A quarter of the way right: 50 east at 100 ahead.
    assert_eq!(g.run_query(&src("50 100 -50 0"), ctx()), Ok(XVal::Str("1 0.75 0.75 100".into())));
    // Off to the right: 0 with handle 0, kept on the edge with handle 1.
    assert_eq!(g.run_query(&src("300 100 0 0"), ctx()), Ok(XVal::Str("0 2 0.5 100".into())));
    assert_eq!(g.run_query(&src("300 100 0 1"), ctx()), Ok(XVal::Str("0 1 0.5 100".into())));
}

#[test]
fn set_default_open_sets_the_open_flag() {
    let f = fixture();
    let mut g = game(&f);
    g.run_query("Begin Function { }\nPlayerRef.SetDefaultOpen 1\nEnd\n", ctx()).unwrap();
    assert!(g.world.get(FormId::PLAYER_REF).unwrap().open);
    g.run_query("Begin Function { }\nPlayerRef.SetDefaultOpen 0\nEnd\n", ctx()).unwrap();
    assert!(!g.world.get(FormId::PLAYER_REF).unwrap().open);
}
