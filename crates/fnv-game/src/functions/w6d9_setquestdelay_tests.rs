use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const Q_TMR: u32 = 0x901;
const Q_ZERO: u32 = 0x902;

fn make_game() -> (GameData, FormStore, RefTable) {
    let mut d_tmr = vec![0u8; 8];
    d_tmr[4..8].copy_from_slice(&0.5f32.to_le_bytes());
    let d_zero = vec![0u8; 8];
    let q1 = record(b"QUST", 0, Q_TMR, &[sub(b"EDID", z("W6D9Tmr")), sub(b"DATA", d_tmr)]);
    let q2 = record(b"QUST", 0, Q_ZERO, &[sub(b"EDID", z("W6D9Zero")), sub(b"DATA", d_zero)]);
    let bytes = [tes4(), top(b"QUST", &[q1, q2].concat())].concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d9Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let placed = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &placed);
    (data, forms, placed)
}

fn num_q(g: &mut Game, body: &str) -> f64 {
    let src = format!("Begin Function {{ }}
SetFunctionValue ({body})
End
");
    match g.run_query(&src, RunCtx::default()).unwrap() {
        XVal::Num(x) => x,
        other => panic!("expected Num, got {other:?}"),
    }
}

#[test]
fn setquestdelay_preserves_raw_zero_and_negative_for_getquestdelay() {
    let (data, forms, placed) = make_game();
    let world = World::new(&forms, &placed, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());

    assert_eq!(num_q(&mut g, "GetQuestDelay W6D9Tmr"), 0.5);
    assert_eq!(num_q(&mut g, "GetQuestDelay W6D9Zero"), 0.0);

    g.run_query("Begin Function { }
SetQuestDelay W6D9Tmr 2.5
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "GetQuestDelay W6D9Tmr"), 2.5);

    g.run_query("Begin Function { }
SetQuestDelay W6D9Tmr 0
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "GetQuestDelay W6D9Tmr"), 0.0);
    assert_eq!(g.quests.get(&FormId(Q_TMR)).and_then(|s| s.delay), Some(0.0));
    // Delay <= 0 falls back to DEFAULT_QUEST_DELAY (5.0 s) in GetDelayElapsed / SetDelayElapsed.
    assert_eq!(num_q(&mut g, "GetDelayElapsed W6D9Tmr"), 5.0);
    g.run_query("Begin Function { }
SetDelayElapsed W6D9Tmr 1.0
End
", RunCtx::default()).unwrap();
    assert_eq!(g.quests.get(&FormId(Q_TMR)).map(|s| s.since_run), Some(4.0));
    assert_eq!(num_q(&mut g, "GetDelayElapsed W6D9Tmr"), 1.0);

    g.run_query("Begin Function { }
SetQuestDelay W6D9Tmr -1.0
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "GetQuestDelay W6D9Tmr"), -1.0);
    assert_eq!(num_q(&mut g, "GetDelayElapsed W6D9Tmr"), 1.0);
    g.run_query("Begin Function { }
SetDelayElapsed W6D9Tmr 2.0
End
", RunCtx::default()).unwrap();
    assert_eq!(g.quests.get(&FormId(Q_TMR)).map(|s| s.since_run), Some(3.0));
    assert_eq!(num_q(&mut g, "GetDelayElapsed W6D9Tmr"), 2.0);

    g.run_query("Begin Function { }
SetQuestDelay W6D9Zero 0
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "GetQuestDelay W6D9Zero"), 0.0);
    assert_eq!(num_q(&mut g, "GetDelayElapsed W6D9Zero"), 5.0);
}
