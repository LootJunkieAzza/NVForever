//! Regression tests for Game::player_karma / GetPlayerKarmaTitle reading live runtime Karma* globals
//! (Wave 6D-8 probe `w6d8_30_karma_titles_levels_and_tiers`).

use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const G_VE: u32 = 0x100;
const G_E: u32 = 0x101;
const G_G: u32 = 0x102;
const G_VG: u32 = 0x103;
const GMST_N1: u32 = 0x104;
const GMST_G1: u32 = 0x105;

fn glob(fid: u32, name: &str, val: f32) -> Vec<u8> {
    record(b"GLOB", 0, fid, &[sub(b"EDID", z(name)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", val.to_le_bytes().to_vec())])
}

fn gmst_str(fid: u32, name: &str, val: &str) -> Vec<u8> {
    record(b"GMST", 0, fid, &[sub(b"EDID", z(name)), sub(b"DATA", z(val))])
}

fn game_fixture() -> (GameData, FormStore, RefTable) {
    let bytes = [
        tes4(),
        top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))])),
        top(
            b"GMST",
            &[
                gmst_str(GMST_N1, "sKarmicTitleNeutral01", "SynthNeutral1"),
                gmst_str(GMST_G1, "sKarmicTitleGood01", "SynthGood1"),
            ]
            .concat(),
        ),
        top(
            b"GLOB",
            &[
                glob(G_VE, "KarmaVeryEvil", -750.0),
                glob(G_E, "KarmaEvil", -250.0),
                glob(G_G, "KarmaGood", 250.0),
                glob(G_VG, "KarmaVeryGood", 750.0),
            ]
            .concat(),
        ),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d8Ktg.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    (data, forms, refs)
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}
{body}
End
"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

#[test]
fn player_karma_titles_read_live_runtime_globals() {
    let (data, forms, refs) = game_fixture();
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });

    q(&mut g, "RewardKarmaAlt 100");
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerKarmaTitle 0)"), XVal::Str("SynthNeutral1".into()));
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerKarmaTitle 1)"), XVal::Str("Neutral".into()));

    g.world.set_global(FormId(G_G), 50.0).unwrap();
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerKarmaTitle 0)"), XVal::Str("SynthGood1".into()));
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerKarmaTitle 1)"), XVal::Str("Good".into()));
}
