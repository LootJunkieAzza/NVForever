//! Independently authored tests for JIP LN `IsInKillCam` live kill-camera state.

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    GameData,
    game::{Game, GameConfig, RunCtx},
    trace::EventKind,
};
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let leading = [tes4(), top(b"NPC_", &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]))].concat();
    let plugins = [Plugin::parse(&leading, "OwnedLeading.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&functions).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

fn eval(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{}}\n{body}\nEnd"), RunCtx::default()).unwrap()
}

#[test]
fn is_in_kill_cam_tracks_live_player_told_state() {
    let f = fixture();
    let mut g = game(&f);

    // Defaults to 0 when no kill camera is playing.
    assert_eq!(eval(&mut g, "SetFunctionValue (IsInKillCam)"), XVal::Num(0.0));

    // Reports 1 when the engine tells us a kill camera is playing.
    g.jip2.told.kill_cam = true;
    assert_eq!(eval(&mut g, "SetFunctionValue (IsInKillCam)"), XVal::Num(1.0));

    // Unrelated PlayerTold flags do not affect IsInKillCam.
    g.jip2.told.auto_move = true;
    g.jip2.told.aiming = true;
    g.jip2.told.kill_cam = false;
    assert_eq!(eval(&mut g, "SetFunctionValue (IsInKillCam)"), XVal::Num(0.0));

    // Clean query: no CallFailed diagnostics emitted.
    assert!(
        !g.events.iter().any(|e| matches!(e.kind, EventKind::CallFailed { .. })),
        "IsInKillCam must not emit CallFailed"
    );

    // Independent Game instances keep no leaked state.
    let mut fresh = game(&f);
    assert_eq!(eval(&mut fresh, "SetFunctionValue (IsInKillCam)"), XVal::Num(0.0));
}
