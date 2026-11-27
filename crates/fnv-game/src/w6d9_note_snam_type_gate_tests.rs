use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NOTE_TXT: u32 = 0x901;
const NOTE_VOX: u32 = 0x902;
const SOUN: u32 = 0x903;
const NPC: u32 = 0x904;

fn make_game() -> (GameData, FormStore, RefTable) {
    let n_txt = record(b"NOTE", 0, NOTE_TXT, &[sub(b"EDID", z("W6D9TxtNote")), sub(b"DATA", vec![1])]);
    let n_vox = record(b"NOTE", 0, NOTE_VOX, &[sub(b"EDID", z("W6D9VoxNote")), sub(b"DATA", vec![3])]);
    let snd = record(b"SOUN", 0, SOUN, &[sub(b"EDID", z("W6D9Snd"))]);
    let npc = record(b"NPC_", 0, NPC, &[sub(b"EDID", z("W6D9Npc"))]);
    let bytes = [tes4(), top(b"SOUN", &snd), top(b"NPC_", &npc), top(b"NOTE", &[n_txt, n_vox].concat())].concat();
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
fn note_sound_and_speaker_require_matching_note_type() {
    let (data, forms, placed) = make_game();
    let world = World::new(&forms, &placed, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());

    // Text note (type 1) rejects both SetNoteSound and SetNoteSpeaker.
    assert_eq!(num_q(&mut g, "SetNoteSound W6D9TxtNote W6D9Snd"), 0.0);
    assert_eq!(num_q(&mut g, "SetNoteSpeaker W6D9TxtNote W6D9Npc"), 0.0);

    // Sound note (type 0) accepts SetNoteSound and returns 0 from GetNoteSpeaker.
    assert_eq!(num_q(&mut g, "SetNoteType W6D9TxtNote 0"), 1.0);
    assert_eq!(num_q(&mut g, "SetNoteSound W6D9TxtNote W6D9Snd"), 1.0);
    assert_eq!(g.run_query("Begin Function { }
SetFunctionValue (GetNoteSound W6D9TxtNote)
End
", RunCtx::default()).unwrap(), XVal::Ref(SOUN));
    assert_eq!(g.run_query("Begin Function { }
SetFunctionValue (GetNoteSpeaker W6D9TxtNote)
End
", RunCtx::default()).unwrap(), XVal::Ref(0));
    // Rejected cross-type SetNoteSpeaker on a Type 0 note must return 0 and leave SNAM unchanged.
    assert_eq!(num_q(&mut g, "SetNoteSpeaker W6D9TxtNote W6D9Npc"), 0.0);
    assert_eq!(g.run_query("Begin Function { }
SetFunctionValue (GetNoteSound W6D9TxtNote)
End
", RunCtx::default()).unwrap(), XVal::Ref(SOUN));

    // Voice note (type 3) accepts SetNoteSpeaker and returns 0 from GetNoteSound.
    assert_eq!(num_q(&mut g, "SetNoteSpeaker W6D9VoxNote W6D9Npc"), 1.0);
    assert_eq!(g.run_query("Begin Function { }
SetFunctionValue (GetNoteSpeaker W6D9VoxNote)
End
", RunCtx::default()).unwrap(), XVal::Ref(NPC));
    assert_eq!(g.run_query("Begin Function { }
SetFunctionValue (GetNoteSound W6D9VoxNote)
End
", RunCtx::default()).unwrap(), XVal::Ref(0));
    // Rejected cross-type SetNoteSound on a Type 3 note must return 0 and leave SNAM unchanged.
    assert_eq!(num_q(&mut g, "SetNoteSound W6D9VoxNote W6D9Snd"), 0.0);
    assert_eq!(g.run_query("Begin Function { }
SetFunctionValue (GetNoteSpeaker W6D9VoxNote)
End
", RunCtx::default()).unwrap(), XVal::Ref(NPC));
}
