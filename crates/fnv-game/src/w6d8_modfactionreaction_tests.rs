//! Regression tests for ModFactionReaction truncating finite float deltas without aborting
//! (Wave 6D-8 probe `w6d8p2_24_modfactionreaction_float_var`).

use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const F1: u32 = 0x100;
const F2: u32 = 0x101;

fn game_fixture() -> (GameData, FormStore, RefTable) {
    let bytes = [
        tes4(),
        top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))])),
        top(
            b"FACT",
            &[
                record(b"FACT", 0, F1, &[sub(b"EDID", z("FacA"))]),
                record(b"FACT", 0, F2, &[sub(b"EDID", z("FacB"))]),
            ]
            .concat(),
        ),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d8Mfr.esm".into()).unwrap()];
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
fn modfactionreaction_truncates_float_deltas_without_aborting() {
    let (data, forms, refs) = game_fixture();
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });

    q(&mut g, "SetFactionReaction FacA FacB 15");
    q(&mut g, "ModFactionReaction FacA FacB 0.5
ModFactionReaction FacA FacB 1.9");
    assert_eq!(q(&mut g, "SetFunctionValue (GetFactionReaction FacA FacB)"), XVal::Num(16.0));
    q(&mut g, "ModFactionReaction FacA FacB -2.8");
    assert_eq!(q(&mut g, "SetFunctionValue (GetFactionReaction FacA FacB)"), XVal::Num(14.0));
}
