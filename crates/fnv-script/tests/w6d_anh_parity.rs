//! Scoped Wave 6D regression tests for `fnv-script/src/vm/anh.rs`
//! (`ScancodeToChar` and `V3CrossproductAlt`).

use std::collections::{BTreeMap, BTreeSet};

use fnv_script::compile::{compile, Compiled, Env, FormInfo, PLAYER};
use fnv_script::vm::{ArgValue, ExtStore, Host, RunOpts, Value, VarKind, Vm};
use fnv_script::{decode, FunctionTable, RefIndex, VarDecl, VarRef, VarSlot, VarType};

struct EmptyEnv;
impl Env for EmptyEnv {
    fn form(&self, _: &str) -> Option<FormInfo> {
        None
    }
    fn info(&self, _: u32) -> Option<FormInfo> {
        None
    }
    fn editor_id(&self, _: u32) -> Option<String> {
        None
    }
    fn local_form(&self, local: u32) -> Option<u32> {
        Some(local)
    }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> {
        None
    }
}

#[derive(Default)]
struct Locals {
    vars: BTreeMap<u16, Value>,
    ext: ExtStore,
    strings: BTreeSet<u16>,
    arrays: BTreeSet<u16>,
}

impl Locals {
    fn text(&self, idx: u16) -> String {
        let id = self.vars.get(&idx).copied().unwrap_or_default().num() as u32;
        self.ext.text(id).unwrap_or_default().to_string()
    }
}

impl Host for Locals {
    fn var_kind(&mut self, var: &VarRef) -> VarKind {
        match var.var {
            VarSlot::Local { index, .. } if self.strings.contains(&index) => VarKind::String,
            VarSlot::Local { index, .. } if self.arrays.contains(&index) => VarKind::Array,
            _ => VarKind::Value,
        }
    }
    fn ext(&mut self) -> Option<&mut ExtStore> {
        Some(&mut self.ext)
    }
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let VarSlot::Local { index, .. } = var.var else { return Err("global".into()) };
        Ok(self.vars.get(&index).copied().unwrap_or_default())
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        let VarSlot::Local { index, .. } = var.var else { return Err("global".into()) };
        self.vars.insert(index, value);
        Ok(())
    }
    fn ref_value(&mut self, _: RefIndex) -> Result<Value, String> {
        Ok(Value::Ref(PLAYER))
    }
    fn call(&mut self, _: u16, _: Option<Value>, _: &[ArgValue]) -> Result<Value, String> {
        Ok(Value::Num(0.0))
    }
}

fn host_for(c: &Compiled) -> Locals {
    let of = |t: &str| {
        c.variables
            .iter()
            .filter(|v| v.ty == VarType::Other(t.into()))
            .map(|v| v.index as u16)
            .collect()
    };
    Locals {
        vars: BTreeMap::new(),
        ext: ExtStore::default(),
        strings: of("string_var"),
        arrays: of("array_var"),
    }
}

fn run(src: &str) -> Locals {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let t = FunctionTable::load(&p).expect("functions.json loads");
    let c = compile(src, &t, &EmptyEnv, None);
    let errors: Vec<_> = c.errors().collect();
    assert!(errors.is_empty(), "compile errors: {errors:?}");
    let d = decode(&c.bytes, &t);
    assert!(d.report.issues.is_empty(), "decode issues: {:?}", d.report.issues);
    let mut h = host_for(&c);
    Vm::new(&t, &mut h, RunOpts { strict: true, ..RunOpts::default() })
        .run(&d.program.blocks[0].body)
        .expect("runs");
    h
}

#[test]
fn anh_scancode_to_char_matches_131_modifiers_numpad_nav_and_mouse() {
    // Wave 6D-4 AnhNVSE 131 ScancodeToChar modifiers, punctuation, numpad, nav, mouse,
    // and unmapped 0x1D codes [observed: w6d4_37, w6d4_38, w6d4p2_37b].
    let cases: &[(i32, &str)] = &[
        (12, "-"), (13, "="), (26, "Left Bracket"), (27, "Right Bracket"),
        (29, "Left Control"), (39, ";"), (40, "'"), (41, "~"),
        (42, "Left Shift"), (43, "Back Slash"), (51, "Comma"), (52, "Period"),
        (53, "Forward Slash"), (54, "Right Shift"), (55, "NUM *"), (56, "Left Alt"),
        (58, "Caps Lock"), (69, "Num Lock"), (70, "Scroll Lock"),
        (71, "NUM7"), (72, "NUM8"), (73, "NUM9"), (74, "NUM Minus"),
        (75, "NUM4"), (76, "NUM5"), (77, "NUM6"), (78, "NUM Plus"),
        (79, "NUM1"), (80, "NUM2"), (81, "NUM3"), (82, "NUM0"), (83, "NUM Period"),
        (156, "NUM Enter"), (157, "Right Control"), (181, "NUM_Slash"),
        (183, "\x1d"), (184, "Right Alt"), (197, "\x1d"),
        (199, "Home"), (201, "PgUp"), (203, "Left Arrow"), (205, "Right Arrow"),
        (207, "End"), (208, "Down Arrow"), (209, "PgDown"), (210, "Insert"), (211, "Delete"),
        (219, "\x1d"), (256, "Left Mouse Button"), (257, "Right Mouse Button"),
        (258, "Middle Mouse Button"), (259, "Mouse Button 3"), (260, "Mouse Button 4"),
        (261, "Mouse Button 5"), (262, "Mouse Button 6"), (263, "Mouse Button 7"),
        (264, "Mouse Wheel Up"), (265, "Mouse Wheel Down"), (266, "\x1d"),
    ];
    for &(code, want) in cases {
        let r = run(&format!("string_var s\nbegin GameMode\nlet s := ScancodeToChar {code}\nend"));
        assert_eq!(r.text(1), want, "ScancodeToChar {code}");
    }
}

#[test]
fn anh_v3_crossproduct_alt_computes_b_cross_a() {
    // V3CrossproductAlt computes B x A [observed: w6d4p2_39_anh_vectors_fixed,
    // w6d4p6_21_stewie_sup_anh_hnvse].
    let c = run(
        "array_var aA\narray_var aB\narray_var aC\nfloat c0\nfloat c1\nfloat c2\nfloat z0\nbegin GameMode\nlet aA := ar_List 1, 0, 0\nlet aB := ar_List 0, 1, 0\nlet aC := V3CrossproductAlt aA aB\nlet z0 := aC[2]\nlet aA := ar_List 1, 2, 3\nlet aB := ar_List 4, 5, 6\nlet aC := V3CrossproductAlt aA aB\nlet c0 := aC[0]\nlet c1 := aC[1]\nlet c2 := aC[2]\nend",
    );
    assert_eq!(c.vars[&7], Value::Num(-1.0), "[1,0,0] x [0,1,0] -> z=-1");
    assert_eq!(
        (c.vars[&4], c.vars[&5], c.vars[&6]),
        (Value::Num(3.0), Value::Num(-6.0), Value::Num(3.0)),
        "[1,2,3] x [4,5,6] -> [3,-6,3]"
    );
}
