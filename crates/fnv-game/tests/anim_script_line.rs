//! Unit tests for `Game::run_anim_script_line` (kNVSE `ScriptLine:` KF animation text-key
//! execution, as used by B42 Inject / B42 Interact / B42 Loot).

use crate::common::{record, sub, tes4, top, z};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{Game, GameConfig, GameData};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, World};
use std::path::PathBuf;

const GOO1: u32 = 0x801;

#[test]
fn anim_script_line_executes_b42_inject_auxvars_and_caches_compilation() {
    let bytes = [
        tes4(),
        top(b"MISC", &record(b"MISC", 0, GOO1, &[sub(b"EDID", z("Goo1"))])),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "AnimScriptLine.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let catalogue = root.join("script/functions.json");
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let raw = serde_json::from_slice(&std::fs::read(&catalogue).unwrap()).unwrap();
    let data = GameData::build(&lo, &schema, FunctionTable::load(&catalogue).unwrap(), &raw, &refs);
    let mut g = Game::new(&data, World::new(&forms, &refs, Default::default()), GameConfig::default());

    let q = |g: &mut Game<'_>, expr: &str| {
        g.run_query(&format!("Begin Function {{}}\nSetFunctionValue ({expr})\nEnd"), Default::default()).unwrap()
    };

    g.run_anim_script_line(
        FormId::PLAYER_REF,
        r#"Goo1.AuxVarSetStr "*_InjMP" "B42Inject\Stimpak01.nif""#,
    )
    .expect("AuxVarSetStr ScriptLine compiles and runs");
    g.run_anim_script_line(FormId::PLAYER_REF, r#"Goo1.AuxVarSetFlt "*_InjM" 1"#)
        .expect("AuxVarSetFlt ScriptLine compiles and runs");

    assert_eq!(q(&mut g, r#"Goo1.AuxVarGetStr "*_InjMP""#), XVal::Str(r"B42Inject\Stimpak01.nif".into()));
    assert_eq!(q(&mut g, r#"Goo1.AuxVarGetFlt "*_InjM""#), XVal::Num(1.0));

    // Repeating the same line hits the cache and updates the value cleanly.
    g.run_anim_script_line(FormId::PLAYER_REF, r#"Goo1.AuxVarSetFlt "*_InjM" 3"#)
        .expect("detach ScriptLine runs");
    assert_eq!(q(&mut g, r#"Goo1.AuxVarGetFlt "*_InjM""#), XVal::Num(3.0));
    g.run_anim_script_line(FormId::PLAYER_REF, r#"Goo1.AuxVarSetFlt "*_InjM" 1"#)
        .expect("cached ScriptLine re-runs");
    assert_eq!(q(&mut g, r#"Goo1.AuxVarGetFlt "*_InjM""#), XVal::Num(1.0));
}
