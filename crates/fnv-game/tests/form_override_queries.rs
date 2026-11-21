//! Authored multi-plugin tests for `GetFormOverrideIndex` (JohnnyGuitar) and
//! `IsFormOverridden` (JIP LN / Lutana).

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    game::{Game, GameConfig, RunCtx},
    trace::EventKind,
    GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

fn tes4_with_masters(masters: &[&str]) -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    let mut subs = vec![sub(b"HEDR", hedr)];
    for m in masters {
        subs.push(sub(b"MAST", z(m)));
        subs.push(sub(b"DATA", 0u64.to_le_bytes().to_vec()));
    }
    record(b"TES4", 1, 0, &subs)
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let misc = |id: u32, edid: &str| {
        let data = [1i32.to_le_bytes().to_vec(), 0.5f32.to_le_bytes().to_vec()].concat();
        record(b"MISC", 0, id, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
    };
    let placed = |id: u32, edid: &str, base: u32| {
        record(b"REFR", 0, id, &[sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
    };

    // Plugin 0: Base.esm
    let player = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]);
    let base_cell = record(b"CELL", 0, 0x200, &[sub(b"EDID", z("OverrideCell"))]);
    let base_children = group(
        0x200u32.to_le_bytes(),
        6,
        &group(
            0x200u32.to_le_bytes(),
            9,
            &[placed(0x300, "RefUntouched", 0x800), placed(0x301, "RefOverridden", 0x800)].concat(),
        ),
    );
    let p0 = [
        tes4_with_masters(&[]),
        top(b"NPC_", &player),
        top(
            b"MISC",
            &[
                misc(0x800, "FormUntouched"),
                misc(0x801, "FormOverriddenOnce"),
                misc(0x802, "FormOverriddenTwice"),
            ]
            .concat(),
        ),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[base_cell, base_children].concat()))),
    ]
    .concat();

    // Plugin 1: PatchA.esp (master 0 = Base.esm; own local forms use prefix 0x01)
    let p1 = [
        tes4_with_masters(&["Base.esm"]),
        top(
            b"MISC",
            &[
                misc(0x0000_0801, "FormOverriddenOnce"),
                misc(0x0000_0802, "FormOverriddenTwice"),
                misc(0x0100_0900, "FormFromPatchA"),
            ]
            .concat(),
        ),
    ]
    .concat();

    // Plugin 2: PatchB.esp (master 0 = Base.esm; overrides FormOverriddenTwice & RefOverridden)
    let p2_cell = record(b"CELL", 0, 0x0000_0200, &[sub(b"EDID", z("OverrideCell"))]);
    let p2_children = group(
        0x200u32.to_le_bytes(),
        6,
        &group(0x200u32.to_le_bytes(), 9, &placed(0x0000_0301, "RefOverridden", 0x800)),
    );
    let p2 = [
        tes4_with_masters(&["Base.esm"]),
        top(b"MISC", &misc(0x0000_0802, "FormOverriddenTwice")),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[p2_cell, p2_children].concat()))),
    ]
    .concat();

    let plugins = vec![
        Plugin::parse(&p0, "Base.esm".into()).unwrap(),
        Plugin::parse(&p1, "PatchA.esp".into()).unwrap(),
        Plugin::parse(&p2, "PatchB.esp".into()).unwrap(),
    ];
    let order = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(&order, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    Fixture { data, forms, refs }
}

fn eval(g: &mut Game<'_>, body: &str) -> XVal {
    let before = g.events.len();
    let v = g
        .run_query(&format!("Begin Function {{}}\n{body}\nEnd\n"), RunCtx::default())
        .unwrap_or_else(|e| panic!("{body}: {e}"));
    assert!(
        !g.events[before..].iter().any(|e| matches!(
            e.kind,
            EventKind::Stub { .. }
                | EventKind::Unhandled { .. }
                | EventKind::CallFailed { .. }
                | EventKind::ScriptError { .. }
        )),
        "{body}: {:?}",
        &g.events[before..]
    );
    v
}

#[test]
fn override_queries_report_load_order_winners_for_base_forms_refs_clones_and_missing() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });

    // Untouched form in plugin 0 (matches plugins1 Stimpak observation: 0 and 0).
    assert_eq!(eval(&mut g, "SetFunctionValue (IsFormOverridden FormUntouched)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (GetFormOverrideIndex FormUntouched)"), XVal::Num(0.0));

    // Untouched form defined in plugin 1: not overridden, index 1.
    assert_eq!(eval(&mut g, "SetFunctionValue (IsFormOverridden FormFromPatchA)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (GetFormOverrideIndex FormFromPatchA)"), XVal::Num(1.0));

    // Overridden once (0 -> 1): overridden, winning index 1.
    assert_eq!(eval(&mut g, "SetFunctionValue (IsFormOverridden FormOverriddenOnce)"), XVal::Num(1.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (GetFormOverrideIndex FormOverriddenOnce)"), XVal::Num(1.0));

    // Overridden twice (0 -> 1 -> 2): overridden, winning index 2 (agrees with GetFormMods).
    assert_eq!(eval(&mut g, "SetFunctionValue (IsFormOverridden FormOverriddenTwice)"), XVal::Num(1.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (GetFormOverrideIndex FormOverriddenTwice)"), XVal::Num(2.0));
    assert_eq!(eval(&mut g, "array_var m\nlet m := GetFormMods FormOverriddenTwice\nSetFunctionValue (ar_Size m)"), XVal::Num(3.0));
    assert_eq!(eval(&mut g, "array_var m\nlet m := GetFormMods FormOverriddenTwice\nSetFunctionValue (m[2])"), XVal::Str("PatchB.esp".into()));

    // Placed references: untouched (0, index 0) and overridden in PatchB (1, index 2),
    // both as explicit argument and as dot-receiver.
    assert_eq!(eval(&mut g, "SetFunctionValue (IsFormOverridden RefUntouched)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (GetFormOverrideIndex RefUntouched)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (IsFormOverridden RefOverridden)"), XVal::Num(1.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (GetFormOverrideIndex RefOverridden)"), XVal::Num(2.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (RefOverridden.IsFormOverridden)"), XVal::Num(1.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (RefOverridden.GetFormOverrideIndex)"), XVal::Num(2.0));

    // Runtime-cloned form (0xFE...): not overridden, index 255 (matching GetSourceModIndex).
    assert_eq!(eval(&mut g, "ref c\nlet c := TempCloneForm FormOverriddenTwice\nSetFunctionValue (IsFormOverridden c)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "ref c\nlet c := TempCloneForm FormOverriddenTwice\nSetFunctionValue (GetFormOverrideIndex c)"), XVal::Num(255.0));

    // Null / omitted / unknown forms: not overridden (0), index -1.
    assert_eq!(eval(&mut g, "ref empty\nSetFunctionValue (IsFormOverridden empty)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "ref empty\nSetFunctionValue (GetFormOverrideIndex empty)"), XVal::Num(-1.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (IsFormOverridden)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (GetFormOverrideIndex)"), XVal::Num(-1.0));
    assert_eq!(eval(&mut g, "ref unk\nlet unk := 0x05123456\nSetFunctionValue (IsFormOverridden unk)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "ref unk\nlet unk := 0x05123456\nSetFunctionValue (GetFormOverrideIndex unk)"), XVal::Num(-1.0));
}
