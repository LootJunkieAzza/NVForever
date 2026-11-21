//! Authored model-query records shared by scene and process-isolated file controls.
use super::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{Game, GameConfig, GameData};
use fnv_game::game::RunCtx;
use fnv_game::nif_blocks::SceneNode;
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

pub const FIRST: FormId = FormId(0x701);
pub const SECOND: FormId = FormId(0x702);
const CELL: u32 = 0x700;

pub fn with_game(f: impl FnOnce(&mut Game<'_>)) {
    let base = record(b"ACTI", 0, 0x703, &[sub(b"EDID", z("OwnedModel")), sub(b"MODL", z("Owned/base.nif"))]);
    let actor = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]);
    let placed = |id: FormId, name| record(b"REFR", 0, id.0, &[
        sub(b"EDID", z(name)), sub(b"NAME", 0x703u32.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24]),
    ]);
    let refs = [placed(FIRST, "OwnedRefA"), placed(SECOND, "OwnedRefB")].concat();
    let cell = [record(b"CELL", 0, CELL, &[sub(b"EDID", z("OwnedModelRoom")), sub(b"DATA", vec![1])]),
        group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs))].concat();
    let bytes = [tes4(), top(b"ACTI", &base), top(b"NPC_", &actor),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)))].concat();
    let plugins = [Plugin::parse(&bytes, "OwnedModelQueries.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let schemas = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(schemas.join("fnv")).unwrap();
    let functions = schemas.join("script/functions.json");
    let catalogue = serde_json::from_slice(&std::fs::read(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(&order, &schema, FunctionTable::load(&functions).unwrap(), &catalogue, &refs);
    let mut game = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    game.player_entered(Some(FormId(CELL)));
    f(&mut game);
}

pub fn tree(name: &str) -> Vec<SceneNode> {
    vec![SceneNode { name: name.into(), parent: None, translation: [0.0; 3], rotation: [0.0; 3],
        scale: 1.0, flags: 0, material: None, info: None }]
}

pub fn query(game: &mut Game<'_>, body: &str) -> XVal {
    let source = format!("ref empty\nBegin Function {{}}\n{body}\nEnd\n");
    game.run_query(&source, RunCtx { self_ref: Some(FormId::PLAYER_REF), ..Default::default() })
        .unwrap_or_else(|error| panic!("{source}: {error}"))
}

pub fn has(game: &mut Game<'_>, object: &str, block: &str) -> XVal {
    query(game, &format!("SetFunctionValue (ModelHasBlock {object} \"{block}\")"))
}
