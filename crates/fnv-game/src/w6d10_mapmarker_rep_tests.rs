//! Regression tests for `SetMapMarkerRep` REPU validation and omitted-arg clear (`w6d10_37`).

use crate::common::*;
use crate::game::{GameConfig, RunCtx};
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const STAT: u32 = 0x800;
const CELL: u32 = 0x900;
const MARKER: u32 = 0x910;
const REPU_GS: u32 = 0x920;
const REPU_NOV: u32 = 0x921;
const ALCH_STIM: u32 = 0x922;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let room = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestRoom")), sub(b"DATA", vec![1])]);
    let marker = record(
        b"REFR",
        0,
        MARKER,
        &[
            sub(b"EDID", z("TestMarkerRef")),
            sub(b"NAME", STAT.to_le_bytes().to_vec()),
            sub(b"XMRK", vec![]),
            sub(b"FNAM", vec![0]),
            sub(b"FULL", z("Test Place")),
            sub(b"TNAM", vec![3, 0]),
            sub(b"WMI1", REPU_GS.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let cell_block = [room, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &marker))].concat();
    let bytes = [
        tes4(),
        top(b"STAT", &record(b"STAT", 0, STAT, &[sub(b"EDID", z("TestStatic"))])),
        top(
            b"REPU",
            &[
                record(b"REPU", 0, REPU_GS, &[sub(b"EDID", z("TestRepuGS"))]),
                record(b"REPU", 0, REPU_NOV, &[sub(b"EDID", z("TestRepuNov"))]),
            ]
            .concat(),
        ),
        top(b"ALCH", &record(b"ALCH", 0, ALCH_STIM, &[sub(b"EDID", z("TestStimpak"))])),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell_block))),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "W6D10MarkerRep.esm".into()).unwrap()];
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

#[test]
fn setmapmarkerrep_rejects_non_repu_and_zero_and_clears_when_omitted() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let ctx = RunCtx::default();

    // Swap to Novac REPU.
    crate::nvse_plugins::call(&mut g, &ctx, "setmapmarkerrep", Some(FormId(MARKER)), &[XVal::Ref(REPU_NOV)])
        .unwrap()
        .unwrap();
    assert_eq!(
        crate::nvse_plugins::call(&mut g, &ctx, "getmapmarkerrep", Some(FormId(MARKER)), &[])
            .unwrap()
            .unwrap(),
        XVal::Ref(REPU_NOV)
    );

    // Non-REPU (Stimpak) and explicit 0 are no-ops.
    crate::nvse_plugins::call(&mut g, &ctx, "setmapmarkerrep", Some(FormId(MARKER)), &[XVal::Ref(ALCH_STIM)])
        .unwrap()
        .unwrap();
    assert_eq!(
        crate::nvse_plugins::call(&mut g, &ctx, "getmapmarkerrep", Some(FormId(MARKER)), &[])
            .unwrap()
            .unwrap(),
        XVal::Ref(REPU_NOV)
    );
    crate::nvse_plugins::call(&mut g, &ctx, "setmapmarkerrep", Some(FormId(MARKER)), &[XVal::Ref(0)])
        .unwrap()
        .unwrap();
    assert_eq!(
        crate::nvse_plugins::call(&mut g, &ctx, "getmapmarkerrep", Some(FormId(MARKER)), &[])
            .unwrap()
            .unwrap(),
        XVal::Ref(REPU_NOV)
    );

    // Omitting the argument clears the reputation.
    crate::nvse_plugins::call(&mut g, &ctx, "setmapmarkerrep", Some(FormId(MARKER)), &[])
        .unwrap()
        .unwrap();
    assert_eq!(
        crate::nvse_plugins::call(&mut g, &ctx, "getmapmarkerrep", Some(FormId(MARKER)), &[])
            .unwrap()
            .unwrap(),
        XVal::Ref(0)
    );
}
