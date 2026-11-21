//! A JIP handler removed with fewer filters than it was set with (#2952): Just Assorted
//! Mods' sprint sets its key handler once per key and removes it with no key.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{game::RunCtx, Game, GameConfig, GameData};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const KEY_HITS: u32 = 0xA00;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let globals = record(b"GLOB", 0, KEY_HITS, &[sub(b"EDID", z("KeyHits")), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())]);
    let mut fields = vec![sub(b"EDID", z("OwnedKeyHandler"))];
    fields.extend(unit(0, &script(&[(0, None, Vec::new())]), &[], &[]));
    fields.push(sub(b"SCTX", b"scn OwnedKeyHandler\nint iKey\nstring_var sourceMarker\nBegin Function {iKey}\n  set KeyHits to KeyHits + 1\nEnd\n".to_vec()));
    let handler = record(b"SCPT", 0, 0xB00, &fields);
    let bytes = [tes4(), top(b"GLOB", &globals), top(b"SCPT", &handler)].concat();
    let plugins = [Plugin::parse(&bytes, "OwnedKeyRemoval.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let catalogue = serde_json::from_slice(&std::fs::read(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(&order, &schema, FunctionTable::load(&functions).unwrap(), &catalogue, &refs);
    Fixture { data, forms, refs }
}

fn run(g: &mut Game<'_>, line: &str) {
    g.run_query(&format!("Begin Function {{}}\n{line}\nEnd\n"), RunCtx::default()).unwrap();
}

fn hits(g: &mut Game<'_>, key: u32) -> f32 {
    g.jip_event("keydown", None, vec![XVal::Num(f64::from(key))]);
    g.world.global(FormId(KEY_HITS)).unwrap()
}

#[test]
fn a_removal_with_no_key_removes_the_handler_for_every_key() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, frame_seconds: 0.02, ..Default::default() });
    run(&mut g, "SetOnKeyDownEventHandler OwnedKeyHandler 1 42");
    run(&mut g, "SetOnKeyDownEventHandler OwnedKeyHandler 1 17");
    assert_eq!((hits(&mut g, 42), hits(&mut g, 17)), (1.0, 2.0), "set for both keys");
    // The same key removes that registration only.
    run(&mut g, "SetOnKeyDownEventHandler OwnedKeyHandler 0 17");
    assert_eq!((hits(&mut g, 17), hits(&mut g, 42)), (2.0, 3.0), "key 17 removed, 42 kept");
    // No key removes every registration of the handler (JAM's JVSScript).
    run(&mut g, "SetOnKeyDownEventHandler OwnedKeyHandler 1 17");
    run(&mut g, "SetOnKeyDownEventHandler OwnedKeyHandler 0");
    assert_eq!((hits(&mut g, 42), hits(&mut g, 17)), (3.0, 3.0), "no handler left for either key");
}
