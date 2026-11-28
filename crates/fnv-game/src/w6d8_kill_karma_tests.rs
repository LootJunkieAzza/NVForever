//! Regression tests for kill_karma Good (-50) and VeryEvil (fKarmaModKillingVeryEvilActor = +2)
//! (Wave 6D-8 probes `w6d8p2_31_kill_karma_all_alignments` & `w6d8p5_31d_karma_kill_gmsts_all`).

use crate::common::*;
use crate::crime::{kill_karma, Alignment, Settings};
use crate::game::{Game, GameConfig};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn gmst_f32(fid: u32, name: &str, val: f32) -> Vec<u8> {
    record(b"GMST", 0, fid, &[sub(b"EDID", z(name)), sub(b"DATA", val.to_le_bytes().to_vec())])
}

#[test]
fn kill_karma_reads_very_evil_gmst_and_halves_good_penalty() {
    let d = Settings::default();
    assert_eq!(kill_karma(Alignment::VeryGood, false, true, &d), -100.0);
    assert_eq!(kill_karma(Alignment::Good, false, true, &d), -50.0);
    assert_eq!(kill_karma(Alignment::Evil, false, true, &d), 100.0);
    assert_eq!(kill_karma(Alignment::VeryEvil, false, true, &d), 2.0);
    assert_eq!(kill_karma(Alignment::Evil, true, false, &d), 0.0);
    assert_eq!(kill_karma(Alignment::Neutral, true, true, &d), -25.0);
    assert_eq!(kill_karma(Alignment::Neutral, false, true, &d), 0.0);

    let bytes = [
        tes4(),
        top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))])),
        top(
            b"GMST",
            &[
                gmst_f32(0x100, "fKarmaModKillingEvilActor", 80.0),
                gmst_f32(0x101, "fKarmaModKillingVeryEvilActor", 5.0),
            ]
            .concat(),
        ),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d8Km.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    let world = World::new(&forms, &refs, SimConfig::default());
    let g = Game::new(&data, world, GameConfig::default());
    let s = Settings::read(&g);
    assert_eq!(s.killing_evil, 80.0);
    assert_eq!(s.killing_very_evil, 5.0);
    assert_eq!(kill_karma(Alignment::Good, false, true, &s), -40.0);
    assert_eq!(kill_karma(Alignment::VeryEvil, false, true, &s), 5.0);
}
