//! Regression tests for `GetStartingPos` on dynamic `PlaceAtMe` references (`w6d10_04`).

use crate::common::*;
use crate::functions::{Args, Ctx};
use crate::game::GameConfig;
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const CELL: u32 = 0x200;
const BASE_MISC: u32 = 0x800;
const REF_PLACED: u32 = 0x900;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let data_bytes: Vec<u8> = [2288.8f32, 2244.0, 7360.0, 0.0, 0.0, 0.4]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let placed = record(
        b"REFR",
        0,
        REF_PLACED,
        &[sub(b"NAME", BASE_MISC.to_le_bytes().to_vec()), sub(b"DATA", data_bytes)],
    );
    let cell = [
        record(b"CELL", 0, CELL, &[sub(b"DATA", vec![1])]),
        group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &placed)),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"MISC", &record(b"MISC", 0, BASE_MISC, &[sub(b"EDID", z("TestMisc"))])),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "W6D10StartPos.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    Fixture { data, forms, refs }
}

fn call_start_pos(g: &mut Game, r: FormId, axis: char) -> f64 {
    let ctx = Ctx {
        unit: std::sync::Arc::from("test"),
        opcode: 0,
        on: Some(r),
        self_ref: None,
        action_ref: None,
        quest: None,
        inst: None,
        in_condition: false,
    };
    let v = super::get_starting_pos(g, &ctx, &Args(&[ArgValue::Axis(axis)])).expect("ok");
    match v {
        Value::Num(n) => n,
        other => panic!("expected Num, got {other:?}"),
    }
}

#[test]
fn get_starting_pos_returns_spawn_coords_for_dynamic_refs_and_zero_for_player() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    {
        let pc = g.world.get(FormId::PLAYER_REF).unwrap();
        pc.cell = Some(FormId(CELL));
        pc.pos = [2352.0, 1603.6, 7360.4];
    }

    // Player always returns (0, 0, 0).
    assert_eq!(call_start_pos(&mut g, FormId::PLAYER_REF, 'X'), 0.0);
    assert_eq!(call_start_pos(&mut g, FormId::PLAYER_REF, 'Y'), 0.0);
    assert_eq!(call_start_pos(&mut g, FormId::PLAYER_REF, 'Z'), 0.0);

    // Placed ref returns placement DATA even after moving.
    g.world.get(FormId(REF_PLACED)).unwrap().pos[0] = 9999.0;
    assert!((call_start_pos(&mut g, FormId(REF_PLACED), 'X') - 2288.8).abs() < 1e-3);

    // Dynamic ref returns its creation position (RefState::made_at) even after moving.
    let dyn_ref = g
        .world
        .place(FormId(BASE_MISC), Some(FormId(CELL)), [2352.0, 1603.6, 7360.4], [0.0, 0.0, 0.0], 1)
        .unwrap();
    assert!((call_start_pos(&mut g, dyn_ref, 'X') - 2352.0).abs() < 1e-3);
    assert!((call_start_pos(&mut g, dyn_ref, 'Y') - 1603.6).abs() < 1e-3);
    assert!((call_start_pos(&mut g, dyn_ref, 'Z') - 7360.4).abs() < 1e-3);
    g.world.get(dyn_ref).unwrap().pos[0] += 50.0;
    assert!((call_start_pos(&mut g, dyn_ref, 'X') - 2352.0).abs() < 1e-3);
}
