//! JohnnyGuitar GetPipBoyMode live Pip-Boy menu-state tests.
//! Independently authored records and tests; no game data or process-global roots.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    game::{menus, Game, GameConfig, GameSnapshot, RunCtx},
    GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const TUT_MSG: FormId = FormId(0x0000_0810);

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let npc = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]);
    let mesg = record(
        b"MESG",
        0,
        TUT_MSG.0,
        &[sub(b"EDID", z("OwnTutorialMsg")), sub(b"DESC", z("Tip"))],
    );
    let bytes = [tes4(), top(b"NPC_", &npc), top(b"MESG", &mesg)].concat();
    let plugins = [Plugin::parse(&bytes, "OwnedPipBoyMode.esm".into()).unwrap()];
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
    Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, SimConfig::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    )
}

fn mode(g: &mut Game<'_>) -> f64 {
    match g
        .run_query(
            "Begin Function {}\nSetFunctionValue (GetPipBoyMode)\nEnd",
            RunCtx::default(),
        )
        .unwrap()
    {
        XVal::Num(n) => n,
        other => panic!("GetPipBoyMode must return a number, got {other:?}"),
    }
}

#[test]
fn get_pipboy_mode_tracks_pipboy_pages_overlays_other_menus_and_restore() {
    let f = fixture();
    let mut g = game(&f);

    // Closed initially.
    assert_eq!(mode(&mut g), 0.0);

    // Every Pip-Boy page reports 3 (Opened), and page switches keep 3.
    g.open_menu(menus::INVENTORY);
    assert_eq!(mode(&mut g), 3.0);
    for page in [menus::STATS, menus::MAP, menus::REPAIR, menus::ITEM_MOD] {
        assert!(g.set_pipboy_menu(page));
        assert_eq!(mode(&mut g), 3.0, "Pip-Boy page {page} must report mode 3");
    }

    // A tutorial overlay opened over the Pip-Boy keeps the Pip-Boy open underneath (mode 3).
    g.show_tutorial(TUT_MSG);
    assert_eq!(mode(&mut g), 3.0);

    // Snapshot & restore preserves open Pip-Boy mode (including under tutorial overlay).
    let snap: GameSnapshot =
        serde_json::from_slice(&serde_json::to_vec(&g.snapshot()).unwrap()).unwrap();
    let world = World::restore(&f.forms, &f.refs, &snap.sim).unwrap();
    let mut restored = Game::restore(&f.data, world, g.cfg, &snap).unwrap();
    assert_eq!(mode(&mut restored), 3.0);

    // Closing the Pip-Boy returns to 0 (Closed).
    assert!(g.engine_menu_closed(menus::INVENTORY));
    assert_eq!(mode(&mut g), 0.0);

    // Non-Pip-Boy menus report 0 (Closed).
    for other in [menus::BARTER, menus::START, menus::LOCKPICK, menus::SLEEP_WAIT] {
        g.open_menu(other);
        assert_eq!(
            mode(&mut g),
            0.0,
            "non-Pip-Boy menu {other} must not report Pip-Boy open"
        );
        g.close_menu();
    }
}
