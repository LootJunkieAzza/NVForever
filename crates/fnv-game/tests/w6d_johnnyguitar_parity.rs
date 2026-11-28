//! Wave 6D JohnnyGuitar NVSE parity regression tests (Rule A5 feature-scoped suite).
//!
//! Covers consolidated Wave 6D Draft PRs for JohnnyGuitar lighting-template RGB wrap
//! and weapon world-model path `TempCloneForm` isolation.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn run(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default())
        .unwrap_or_else(|e| panic!("{body}: {e}"))
}

#[test]
fn set_lighting_template_rgb_channels_truncate_and_wrap_mod_256() {
    const CELL: FormId = FormId(0xA00);
    const B: FormId = FormId(0xB01);

    let mut lgtm_data = Vec::new();
    lgtm_data.extend([70u8, 80, 90, 0]); // ambient
    lgtm_data.extend([10u8, 20, 30, 0]); // directional
    lgtm_data.extend([40u8, 50, 60, 0]); // fog
    lgtm_data.extend(100.0f32.to_le_bytes()); // fog near
    lgtm_data.extend(20000.0f32.to_le_bytes()); // fog far
    lgtm_data.extend(0i32.to_le_bytes()); // dir xy
    lgtm_data.extend(0i32.to_le_bytes()); // dir z
    lgtm_data.extend(1.0f32.to_le_bytes()); // dir fade
    lgtm_data.extend(20000.0f32.to_le_bytes()); // fog clip dist
    lgtm_data.extend(1.0f32.to_le_bytes()); // fog power
    assert_eq!(lgtm_data.len(), 40);

    let mut xcll = Vec::new();
    xcll.extend([1u8, 2, 3, 0]);
    xcll.extend([4u8, 5, 6, 0]);
    xcll.extend([7u8, 8, 9, 0]);
    xcll.extend(10.0f32.to_le_bytes());
    xcll.extend(1000.0f32.to_le_bytes());
    xcll.extend(0i32.to_le_bytes());
    xcll.extend(0i32.to_le_bytes());
    xcll.extend(1.0f32.to_le_bytes());
    xcll.extend(1000.0f32.to_le_bytes());
    xcll.extend(1.0f32.to_le_bytes());
    assert_eq!(xcll.len(), 40);

    let lgtm = record(b"LGTM", 0, B.0, &[sub(b"EDID", z("OwnedLightB")), sub(b"DATA", lgtm_data)]);
    let cell = record(
        b"CELL",
        0,
        CELL.0,
        &[sub(b"EDID", z("OwnedLightCell")), sub(b"DATA", vec![1]), sub(b"XCLL", xcll), sub(b"LNAM", u32s(&[0]))],
    );
    let bytes = [
        tes4(),
        top(b"LGTM", &lgtm),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat();

    let plugins = vec![Plugin::parse(&bytes, "JgLighting.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });

    // [observed: w6d3p2_27_lighting_template_and_interior_lighting_wrap: trait 1 300->44, 25.5->25, -10->246; traits 2..=9 hypothesis]
    run(
        &mut g,
        "SetLightingTemplateTraitNumeric OwnedLightB 1 300\n\
         SetLightingTemplateTraitNumeric OwnedLightB 2 25.5\n\
         SetLightingTemplateTraitNumeric OwnedLightB 3 -10",
    );
    assert_eq!(run(&mut g, "SetFunctionValue (GetLightingTemplateTraitNumeric OwnedLightB 1)"), XVal::Num(44.0));
    assert_eq!(run(&mut g, "SetFunctionValue (GetLightingTemplateTraitNumeric OwnedLightB 2)"), XVal::Num(25.0));
    assert_eq!(run(&mut g, "SetFunctionValue (GetLightingTemplateTraitNumeric OwnedLightB 3)"), XVal::Num(246.0));
    run(&mut g, "SetCellLightingTemplate OwnedLightCell OwnedLightB\nSetCellFlag OwnedLightCell 8 1");
    assert_eq!(g.session_cell_lighting(&lo, &schema, CELL).unwrap().ambient, [44, 25, 246]);
}

#[test]
fn set_weapon_world_model_path_returns_one_and_isolates_temp_clone() {
    let bytes = [
        tes4(),
        top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))])),
        top(b"MISC", &record(b"MISC", 0, 0xD11, &[sub(b"EDID", z("OwnMiscModel")), sub(b"MODL", z("clutter\\junk.nif"))])),
        top(
            b"WEAP",
            &record(
                b"WEAP",
                0,
                0xD12,
                &[sub(b"EDID", z("OwnWorldWeapon")), sub(b"MOD4", z("Weapons\\1HandMelee\\ShishkebabGO.NIF"))],
            ),
        ),
    ]
    .concat();

    let plugins = vec![Plugin::parse(&bytes, "JgWorldModel.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });

    assert_eq!(
        run(&mut g, "SetFunctionValue (GetWeaponWorldModelPath OwnWorldWeapon)"),
        XVal::Str("Weapons\\1HandMelee\\ShishkebabGO.NIF".into()),
    );
    assert_eq!(
        run(&mut g, r#"SetFunctionValue (SetWeaponWorldModelPath OwnMiscModel "weapons\bad.nif")"#),
        XVal::Num(0.0),
    );
    assert_eq!(
        run(
            &mut g,
            "ref rCl\n\
             let rCl := TempCloneForm OwnWorldWeapon\n\
             string_var sPre\n\
             let sPre := GetWeaponWorldModelPath rCl\n\
             int iRet\n\
             let iRet := SetWeaponWorldModelPath rCl \"weapons\\w6d3_clone2.nif\"\n\
             string_var sPost\n\
             let sPost := GetWeaponWorldModelPath rCl\n\
             string_var sOrig\n\
             let sOrig := GetWeaponWorldModelPath OwnWorldWeapon\n\
             SetFunctionValue (sv_Construct \"%g|%z|%z|%z\" iRet sPre sPost sOrig)"
        ),
        XVal::Str("1|Weapons\\1HandMelee\\ShishkebabGO.NIF|weapons\\w6d3_clone2.nif|Weapons\\1HandMelee\\ShishkebabGO.NIF".into()),
    );
}
