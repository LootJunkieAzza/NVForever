use crate::{
    common::*,
    game::{GameConfig, GameSnapshot, RunCtx},
    Game, GameData,
};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, World};

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}
fn fixture() -> Fixture {
    let bytes = [
        tes4(),
        top(
            b"NPC_",
            &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
        ),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "Filtering.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(
        &lo,
        &schema,
        FunctionTable::load(&path).unwrap(),
        &catalogue,
        &refs,
    );
    Fixture { data, forms, refs }
}
fn game(f: &Fixture) -> Game<'_> {
    Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, Default::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    )
}
fn source(g: &mut Game<'_>, body: &str, receiver: Option<FormId>) -> Result<XVal, String> {
    g.run_query(
        &format!("ref empty\nBegin Function {{}}\n{body}\nEnd"),
        RunCtx {
            self_ref: receiver,
            ..Default::default()
        },
    )
}

#[test]
fn source_levels_reach_one_bounded_consumable_request_and_preserve_world() {
    let f = fixture();
    let mut g = game(&f);
    let before = g.world.snapshot();
    for level in [1, 2, 4, 8, 16] {
        assert_eq!(
            source(&mut g, &format!("SetMaxAniso {level}"), None),
            Ok(XVal::Num(0.0))
        );
        assert_eq!(g.take_texture_filter_level(), Some(level));
        assert_eq!(g.take_texture_filter_level(), None);
    }
    source(
        &mut g,
        "SetMaxAniso 1\nSetMaxAniso 16",
        Some(FormId::PLAYER_REF),
    )
    .unwrap();
    assert_eq!(g.take_texture_filter_level(), Some(16));
    assert_eq!(g.world.snapshot(), before);
}

#[test]
fn invalid_source_and_empty_receiver_do_not_replace_a_pending_level() {
    let f = fixture();
    let mut g = game(&f);
    let before = g.world.snapshot();
    for bad in [
        "SetMaxAniso 0",
        "SetMaxAniso 17",
        "SetMaxAniso 7",
        "SetMaxAniso 1.5",
        "SetMaxAniso (Pow -1 0.5)",
        "SetMaxAniso (Pow 10 400)",
        "empty.SetMaxAniso 16",
    ] {
        source(&mut g, "SetMaxAniso 8", None).unwrap();
        assert!(
            source(&mut g, bad, Some(FormId::PLAYER_REF)).is_err(),
            "{bad}"
        );
        assert_eq!(g.take_texture_filter_level(), Some(8), "{bad}");
        assert_eq!(g.world.snapshot(), before);
    }
    // Ordinary non-strict extended statement recovery is not a new global guard.
    g.cfg.strict = false;
    assert!(source(&mut g, "SetMaxAniso 1.5\nSetMaxAniso 16", None).is_ok());
    assert_eq!(g.take_texture_filter_level(), Some(16));
}

#[test]
fn pending_renderer_intent_is_not_saved_or_replayed_by_restoration() {
    let f = fixture();
    let mut g = game(&f);
    source(&mut g, "SetMaxAniso 16", None).unwrap();
    let bytes = serde_json::to_vec(&g.checked_snapshot().unwrap()).unwrap();
    let saved: GameSnapshot = serde_json::from_slice(&bytes).unwrap();
    let world = World::restore(&f.forms, &f.refs, &saved.sim).unwrap();
    let mut back = Game::restore(&f.data, world, g.cfg, &saved).unwrap();
    assert_eq!(back.take_texture_filter_level(), None);
    assert_eq!(g.take_texture_filter_level(), Some(16));
    source(&mut back, "SetMaxAniso 8", None).unwrap();
    assert_eq!(back.take_texture_filter_level(), Some(8));
}
