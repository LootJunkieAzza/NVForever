//! Wave 6D-5 & 6D-11 `crates/fnv-script/src/vm.rs` parity regression tests.
//!
//! Covers all 26 consolidated VM fixes:
//! - Wave 6D-5 (#4235, #4237, #4239, #4241, #4243, #4245, #4247, #4249,
//!   #4268, #4270, #4272, #4274, #4276, #4278, #4280)
//! - Wave 6D-11 (#4549, #4553, #4555, #4557, #4559, #4561, #4563, #4565,
//!   #4567, #4577, #4579)

use fnv_script::compile::{compile, Compiled, Env, FormInfo};
use fnv_script::ir::VarSlot;
use fnv_script::vm::{form_type_code, ArgValue, ExtStore, Host, RunOpts, Value, VarKind, Vm};
use fnv_script::{decode, FunctionTable, Program, RefIndex, VarDecl, VarRef, VarType};
use std::collections::HashMap;

struct NoForms;

impl Env for NoForms {
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

struct TestHost {
    ext: ExtStore,
    vars: HashMap<u16, Value>,
    kinds: HashMap<u16, VarKind>,
}

impl TestHost {
    fn text(&self, local: u16) -> &str {
        let id = self.vars.get(&local).map_or(0, |v| v.num() as u32);
        self.ext.text(id).unwrap_or("")
    }
}

impl Host for TestHost {
    fn var_kind(&mut self, v: &VarRef) -> VarKind {
        match v.var {
            VarSlot::Local { index, .. } => self.kinds.get(&index).copied().unwrap_or(VarKind::Value),
            _ => VarKind::Value,
        }
    }
    fn ext(&mut self) -> Option<&mut ExtStore> {
        Some(&mut self.ext)
    }
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let VarSlot::Local { index, .. } = var.var else {
            return Ok(Value::Num(0.0));
        };
        Ok(self.vars.get(&index).copied().unwrap_or(Value::Num(0.0)))
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        if let VarSlot::Local { index, .. } = var.var {
            self.vars.insert(index, value);
        }
        Ok(())
    }
    fn ref_value(&mut self, _: RefIndex) -> Result<Value, String> {
        Ok(Value::Ref(0))
    }
    fn call(&mut self, _: u16, _: Option<Value>, _: &[ArgValue]) -> Result<Value, String> {
        Ok(Value::Num(0.0))
    }
}

fn table() -> FunctionTable {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    FunctionTable::load(&p).expect("functions.json loads")
}

fn build(src: &str, t: &FunctionTable) -> (Compiled, Program) {
    let compiled = compile(src, t, &NoForms, None);
    let errors: Vec<_> = compiled.errors().collect();
    assert!(errors.is_empty(), "compile errors: {errors:?}");
    let dec = decode(&compiled.bytes, t);
    assert!(dec.report.issues.is_empty(), "decode issues: {:?}", dec.report.issues);
    (compiled, dec.program)
}

fn host_for(compiled: &Compiled) -> TestHost {
    let mut host = TestHost { ext: ExtStore::default(), vars: HashMap::new(), kinds: HashMap::new() };
    for v in &compiled.variables {
        let k = match &v.ty {
            VarType::Other(t) if t == "string_var" => VarKind::String,
            VarType::Other(t) if t == "array_var" => VarKind::Array,
            _ => VarKind::Value,
        };
        host.kinds.insert(v.index as u16, k);
    }
    host
}

fn run_strict(src: &str) -> TestHost {
    let t = table();
    let (c, p) = build(src, &t);
    let mut host = host_for(&c);
    Vm::new(&t, &mut host, RunOpts { strict: true, ..RunOpts::default() })
        .run(&p.blocks[0].body)
        .expect("runs");
    host
}

fn run_default(src: &str) -> TestHost {
    let t = table();
    let (c, p) = build(src, &t);
    let mut host = host_for(&c);
    Vm::new(&t, &mut host, RunOpts::default())
        .run(&p.blocks[0].body)
        .expect("runs");
    host
}

#[test]
fn testexpr_succeeds_on_valid_string_expressions() {
    let h = run_default(
        "string_var svVal\nstring_var svEmp\nstring_var svUninit\nstring_var svAssign\n\
         array_var aMap\nint tLit\nint tSv\nint tMapS\nint tMapM\nint tAsgn\nint tDiv0\n\
         begin GameMode\n\
         let svVal := \"hello\"\n\
         let svEmp := \"\"\n\
         let svAssign := \"before\"\n\
         let aMap := ar_Map \"strKey\"::\"world\", \"numKey\"::42\n\
         let tLit := TestExpr \"abc\"\n\
         let tSv := TestExpr svVal\n\
         let tMapS := TestExpr aMap[\"strKey\"]\n\
         let tMapM := TestExpr aMap[\"missing\"]\n\
         let tAsgn := TestExpr (svAssign = \"after\")\n\
         let tDiv0 := TestExpr (10 / 0)\n\
         end",
    );
    assert_eq!(h.vars[&6], Value::Num(1.0), "TestExpr string literal");
    assert_eq!(h.vars[&7], Value::Num(1.0), "TestExpr string_var");
    assert_eq!(h.vars[&8], Value::Num(1.0), "TestExpr string map lookup");
    assert_eq!(h.vars[&9], Value::Num(0.0), "TestExpr missing map key");
    assert_eq!(h.vars[&10], Value::Num(1.0), "TestExpr string assignment");
    assert_eq!(h.text(4), "after");
    assert_eq!(h.vars[&11], Value::Num(0.0), "TestExpr div0");
}

#[test]
fn while_condition_error_exits_loop_and_continues_in_non_strict_mode() {
    let src = "array_var aL\nint iIdx\nint iAfter\nbegin GameMode\n\
               let aL := ar_List 10, 20\n\
               while iIdx < 5 && aL[iIdx] > 0\n\
                 let iIdx += 1\n\
               loop\n\
               let iAfter := 1\n\
               end";
    let h = run_default(src);
    assert_eq!(h.vars[&2], Value::Num(2.0), "loop ran twice before OOB");
    assert_eq!(h.vars[&3], Value::Num(1.0), "script continued after while condition error");

    // Strict mode still propagates the while condition error.
    let t = table();
    let (c, p) = build(src, &t);
    let mut strict_h = host_for(&c);
    assert!(Vm::new(&t, &mut strict_h, RunOpts { strict: true, ..RunOpts::default() })
        .run(&p.blocks[0].body)
        .is_err());
}

#[test]
fn arma_reports_armo_form_type_code_24() {
    assert_eq!(form_type_code(b"ARMA"), Some(24));
    assert_eq!(form_type_code(b"ARMO"), Some(24));
    assert_eq!(form_type_code(b"WEAP"), Some(40));
    assert_eq!(form_type_code(b"NPC_"), Some(42));
    assert_eq!(form_type_code(b"DOOR"), Some(28));
}

#[test]
fn printf_number_preserves_negative_zero_across_all_float_codes() {
    let h = run_strict(
        "float nz\nstring_var s\nstring_var fl\nstring_var th\n\
         begin GameMode\n\
         let nz := -1.0 * 0.0\n\
         let s := sv_Construct \"f=%.2f|g=%g|G=%G|e=%.2e|E=%.2E\" nz nz nz nz nz\n\
         let fl := sv_Construct \"p=[%+5.1f]|s=[% 5.1f]|l=[%-5.1f]|z=[%05.1f]|pg=[%+g]|zg=[%06g]\" 3.5 3.5 3.5 3.5 42 42\n\
         let th := sv_Construct \"g0=[%.0g]|g1=[%.1g]|e4=[%g]|e5=[%g]\" 123.4 123.4 0.0001 0.0000999\n\
         end",
    );
    assert_eq!(h.text(2), "f=-0.00|g=-0|G=-0|e=-0.00e+00|E=-0.00E+00");
    assert_eq!(h.text(3), "p=[ +3.5]|s=[  3.5]|l=[3.5  ]|z=[003.5]|pg=[+42]|zg=[000042]");
    assert_eq!(h.text(4), "g0=[1e+02]|g1=[1e+02]|e4=[0.0001]|e5=[9.99e-05]");
}

#[test]
fn dollar_and_tostring_format_numbers_with_c_percent_g() {
    let h = run_strict(
        "float nz\nstring_var a\nstring_var b\nstring_var c\nstring_var d\nstring_var e\nstring_var f\n\
         begin GameMode\n\
         let nz := -1.0 * 0.0\n\
         let a := $42.0 + \"/\" + (ToString 42.0)\n\
         let b := $nz + \"/\" + (ToString nz)\n\
         let c := $3.14159265 + \"/\" + (ToString 3.14159265)\n\
         let d := $0.0000001 + \"/\" + (ToString 0.0000001)\n\
         let e := $1234567.89 + \"/\" + (ToString 1234567.89)\n\
         let f := $2.5\n\
         end",
    );
    assert_eq!(h.text(2), "42/42");
    assert_eq!(h.text(3), "-0/-0");
    assert_eq!(h.text(4), "3.14159/3.14159");
    assert_eq!(h.text(5), "1e-07/1e-07");
    assert_eq!(h.text(6), "1.23457e+06/1.23457e+06");
    assert_eq!(h.text(7), "2.5");
}

#[test]
fn array_equality_and_deepequals_match_list_map_and_case_insensitive_keys() {
    let h = run_strict(
        "array_var aL\narray_var aM\narray_var aS1\narray_var aS2\n\
         int eqLM\nint deqLM\nint eqCase\nint deqCase\nint neDiff\n\
         begin GameMode\n\
         let aL := ar_List 1, 2\n\
         let aM := ar_Map 0::1, 1::2\n\
         let aS1 := ar_Map \"A\"::1\n\
         let aS2 := ar_Map \"a\"::1\n\
         let eqLM := (aL == aM)\n\
         let deqLM := Ar_DeepEquals aL aM\n\
         let eqCase := (aS1 == aS2)\n\
         let deqCase := Ar_DeepEquals aS1 aS2\n\
         let neDiff := (aL != aS1)\n\
         end",
    );
    assert_eq!(h.vars[&5], Value::Num(1.0));
    assert_eq!(h.vars[&6], Value::Num(1.0));
    assert_eq!(h.vars[&7], Value::Num(1.0));
    assert_eq!(h.vars[&8], Value::Num(1.0));
    assert_eq!(h.vars[&9], Value::Num(1.0));
}

#[test]
fn string_subscript_oob_returns_empty_without_error_and_clamps_negative_slice_start() {
    let h = run_strict(
        "string_var s\nstring_var n1\nstring_var n5\nstring_var hi\nstring_var lo\n\
         int teHi\nint teLo\nstring_var slNeg\nstring_var slCl\n\
         begin GameMode\n\
         let s := \"abcde\"\n\
         let n1 := s[-1]\n\
         let n5 := s[-5]\n\
         let hi := s[99]\n\
         let lo := s[-99]\n\
         let teHi := TestExpr (sv_Length s[99])\n\
         let teLo := TestExpr (sv_Length s[-99])\n\
         let slNeg := s[-3:-1]\n\
         let slCl := s[-99:2]\n\
         end",
    );
    assert_eq!(h.text(2), "e");
    assert_eq!(h.text(3), "a");
    assert_eq!(h.text(4), "");
    assert_eq!(h.text(5), "");
    assert_eq!(h.vars[&6], Value::Num(1.0));
    assert_eq!(h.vars[&7], Value::Num(1.0));
    assert_eq!(h.text(8), "cde");
    assert_eq!(h.text(9), "abc");
}

#[test]
fn chartoascii_rejects_multi_character_and_empty_strings() {
    let h = run_strict(
        "int a\nint ab\nint emp\n\
         begin GameMode\n\
         let a := CharToAscii \"A\"\n\
         let ab := CharToAscii \"AB\"\n\
         let emp := CharToAscii \"\"\n\
         end",
    );
    assert_eq!(h.vars[&1], Value::Num(65.0));
    assert_eq!(h.vars[&2], Value::Num(-1.0));
    assert_eq!(h.vars[&3], Value::Num(-1.0));
}

#[test]
fn tonumber_report_error_flag_raises_catchable_error() {
    let h = run_strict(
        "int bad0\nint bad1\nint ok1\nint emp1\nint hexBad1\nfloat mix\n\
         begin GameMode\n\
         let bad0 := TestExpr (ToNumber \"abc\" 0 0)\n\
         let bad1 := TestExpr (ToNumber \"abc\" 0 1)\n\
         let ok1 := TestExpr (ToNumber \"12.5\" 0 1)\n\
         let emp1 := TestExpr (ToNumber \"\" 0 1)\n\
         let hexBad1 := TestExpr (ToNumber \"zz\" 1 1)\n\
         let mix := #\"  -12.5abc\"\n\
         end",
    );
    assert_eq!(h.vars[&1], Value::Num(1.0));
    assert_eq!(h.vars[&2], Value::Num(0.0));
    assert_eq!(h.vars[&3], Value::Num(1.0));
    assert_eq!(h.vars[&4], Value::Num(0.0));
    assert_eq!(h.vars[&5], Value::Num(0.0));
    assert_eq!(h.vars[&6], Value::Num(-12.5));
}

#[test]
fn typeof_returns_bad_array_for_uninitialized_array_and_ar_null() {
    let h = run_strict(
        "array_var aUninit\narray_var aList\narray_var aMap\narray_var aSMap\n\
         string_var tU\nstring_var tN\nstring_var tL\nstring_var tM\nstring_var tS\nstring_var tMiss\n\
         begin GameMode\n\
         let aList := ar_List 1\n\
         let aMap := ar_Map 1::2\n\
         let aSMap := ar_Map \"k\"::3\n\
         let tU := TypeOf aUninit\n\
         let tN := TypeOf Ar_Null\n\
         let tL := TypeOf aList\n\
         let tM := TypeOf aMap\n\
         let tS := TypeOf aSMap\n\
         let tMiss := TypeOf aSMap[\"nope\"]\n\
         end",
    );
    assert_eq!(h.text(5), "<Bad Array>");
    assert_eq!(h.text(6), "<Bad Array>");
    assert_eq!(h.text(7), "Array");
    assert_eq!(h.text(8), "Map");
    assert_eq!(h.text(9), "StringMap");
    assert_eq!(h.text(10), "NULL");
}

#[test]
fn format_v_returns_unknown_for_negative_or_out_of_range_actor_values() {
    let h = run_strict(
        "string_var s\nfloat fNan\nfloat fInf\n\
         begin GameMode\n\
         let fNan := (-1) ^ 0.5\n\
         let fInf := 10 ^ 400\n\
         let s := sv_Construct \"17=[%v]|34=[%v]|n1=[%v]|999=[%v]|nan=[%v]|inf=[%v]\" 17 34 -1 999 fNan fInf\n\
         end",
    );
    assert!(matches!(h.vars[&2], Value::Num(x) if x.is_nan()), "expected NaN, got {:?}", h.vars[&2]);
    assert!(matches!(h.vars[&3], Value::Num(x) if x.is_infinite()), "expected Inf, got {:?}", h.vars[&3]);
    assert_eq!(h.text(1), "17=[MeleeDamage]|34=[EnergyWeapons]|n1=[unknown]|999=[unknown]|nan=[unknown]|inf=[unknown]");
}

#[test]
fn dollar_stringize_formats_arrays_distinctly_from_tostring() {
    let h = run_strict(
        "array_var aL\narray_var aN\narray_var aS\narray_var aZ\n\
         string_var sL\nstring_var sN\nstring_var sS\nstring_var sZ\n\
         string_var tL\nstring_var tN\nstring_var tS\nstring_var tZ\n\
         begin GameMode\n\
         let aL := ar_List 1, 2\n\
         let aN := ar_Map 1::10\n\
         let aS := ar_Map \"k\"::5\n\
         let sL := $aL\nlet sN := $aN\nlet sS := $aS\nlet sZ := $aZ\n\
         let tL := ToString aL\nlet tN := ToString aN\nlet tS := ToString aS\nlet tZ := ToString aZ\n\
         end",
    );
    assert_eq!(h.text(5), "[1, 2]");
    assert_eq!(h.text(6), "[1.000000: 10]");
    assert_eq!(h.text(7), "[\"k\": 5]");
    assert_eq!(h.text(8), "array ID 0 (invalid)");
    assert_eq!(h.text(9), "NULL");
    assert_eq!(h.text(10), "NULL");
    assert_eq!(h.text(11), "NULL");
    assert_eq!(h.text(12), "NULL");
}

#[test]
fn ar_sortalpha_filters_mixed_array_to_first_element_type() {
    let h = run_strict(
        "array_var aMix\narray_var sAlpha\nint sz\nfloat v0\nfloat v1\n\
         begin GameMode\n\
         let aMix := ar_List 10, 2, \"b\", \"a\"\n\
         let sAlpha := ar_SortAlpha aMix\n\
         let sz := ar_Size sAlpha\n\
         let v0 := sAlpha[0]\n\
         let v1 := sAlpha[1]\n\
         end",
    );
    assert_eq!(h.vars[&3], Value::Num(2.0));
    assert_eq!(h.vars[&4], Value::Num(2.0));
    assert_eq!(h.vars[&5], Value::Num(10.0));
}

#[test]
fn bitwise_functions_return_unsigned_u32_and_getbit_wraps_modulo_32() {
    let h = run_strict(
        "float fRs\nfloat fLs\nfloat fNot\nfloat fSb31\nint g32\nint g33\nint g34\nint gNeg5\nint gNegAll\n\
         begin GameMode\n\
         let fRs := RightShift -8 1\n\
         let fLs := LeftShift 1 31\n\
         let fNot := LogicalNot 0\n\
         let fSb31 := SetBit 0 31 1\n\
         let g32 := GetBit 5 32\n\
         let g33 := GetBit 5 33\n\
         let g34 := GetBit 5 34\n\
         let gNeg5 := GetBit 5 -1\n\
         let gNegAll := GetBit -1 -1\n\
         end",
    );
    assert_eq!(h.vars[&1], Value::Num(2_147_483_644.0));
    assert_eq!(h.vars[&2], Value::Num(2_147_483_648.0));
    assert_eq!(h.vars[&3], Value::Num(4_294_967_295.0));
    assert_eq!(h.vars[&4], Value::Num(2_147_483_648.0));
    assert_eq!(h.vars[&5], Value::Num(1.0));
    assert_eq!(h.vars[&6], Value::Num(0.0));
    assert_eq!(h.vars[&7], Value::Num(1.0));
    assert_eq!(h.vars[&8], Value::Num(0.0));
    assert_eq!(h.vars[&9], Value::Num(1.0));
}

#[test]
fn bitwise_functions_return_unsigned_u32_values_and_support_3arg_setbit() {
    let h = run_strict(
        "float fSign\nfloat fAndS\nfloat fXorN\nfloat fNotM1\nfloat fSb3c\nfloat fSb31\nfloat fCb31\nfloat fRs1\n\
         begin GameMode\n\
         let fSign := LeftShift 1 31\n\
         let fAndS := LogicalAnd -1 fSign\n\
         let fXorN := LogicalXor -1 0\n\
         let fNotM1 := LogicalNot -1\n\
         let fSb3c := SetBit 15 1 0\n\
         let fSb31 := SetBit 0 31\n\
         let fCb31 := ClearBit -1 31\n\
         let fRs1 := RightShift fSign 1\n\
         end",
    );
    assert_eq!(h.vars[&1], Value::Num(2_147_483_648.0));
    assert_eq!(h.vars[&2], Value::Num(2_147_483_648.0));
    assert_eq!(h.vars[&3], Value::Num(4_294_967_295.0));
    assert_eq!(h.vars[&4], Value::Num(0.0));
    assert_eq!(h.vars[&5], Value::Num(13.0));
    assert_eq!(h.vars[&6], Value::Num(2_147_483_648.0));
    assert_eq!(h.vars[&7], Value::Num(2_147_483_647.0));
    assert_eq!(h.vars[&8], Value::Num(1_073_741_824.0));
}

#[test]
fn vanilla_div_and_mod_by_zero_return_dividend_while_extended_errors() {
    let h = run_strict(
        "float vDiv\nfloat vMod\nint teD\nint teM\n\
         begin GameMode\n\
         set vDiv to 10 / 0\n\
         set vMod to 10 % 0\n\
         let teD := TestExpr (10 / 0)\n\
         let teM := TestExpr (10 % 0)\n\
         end",
    );
    assert_eq!(h.vars[&1], Value::Num(10.0));
    assert_eq!(h.vars[&2], Value::Num(10.0));
    assert_eq!(h.vars[&3], Value::Num(0.0));
    assert_eq!(h.vars[&4], Value::Num(0.0));
}

#[test]
fn ar_insertrange_rejects_non_list_source_and_keeps_list_source_working() {
    let h = run_strict(
        "array_var aL\narray_var aSrc\narray_var aMap\nfloat fOk\nfloat fMap\nfloat fSz\n\
         begin GameMode\n\
         let aL := Ar_List 10, 20\n\
         let aSrc := Ar_List 100, 200\n\
         let aMap := Ar_Map 5::50, 2::20\n\
         let fOk := Ar_InsertRange aL, 1, aSrc\n\
         let fMap := Ar_InsertRange aL, 0, aMap\n\
         let fSz := Ar_Size aL\n\
         end",
    );
    assert_eq!(h.vars[&4], Value::Num(1.0));
    assert_eq!(h.vars[&5], Value::Num(0.0), "Map source must be rejected");
    assert_eq!(h.vars[&6], Value::Num(4.0));
}

#[test]
fn sv_find_and_sv_count_handle_negative_start_and_negative_length() {
    let h = run_strict(
        "string_var sHay\nfloat fNegS\nfloat fNegL\nfloat fOob\nfloat cNegS\nfloat cNegL\n\
         begin GameMode\n\
         let sHay := \"abABA_42_ab\"\n\
         let fNegS := Sv_Find \"ab\" sHay -4\n\
         let fNegL := Sv_Find \"ab\" sHay 0 -1\n\
         let fOob := Sv_Find \"ab\" sHay 99\n\
         let cNegS := Sv_Count \"ab\" sHay -4\n\
         let cNegL := Sv_Count \"ab\" sHay 0 -1\n\
         end",
    );
    assert_eq!(h.vars[&2], Value::Num(-1.0));
    assert_eq!(h.vars[&3], Value::Num(0.0));
    assert_eq!(h.vars[&4], Value::Num(-1.0));
    assert_eq!(h.vars[&5], Value::Num(0.0));
    assert_eq!(h.vars[&6], Value::Num(3.0));
}

#[test]
fn sv_substring_and_sv_erase_handle_negative_start_and_negative_count() {
    let h = run_strict(
        "string_var sSubNs\nstring_var sSubNl\nstring_var sE1\nstring_var sE2\n\
         begin GameMode\n\
         let sSubNs := Sv_SubString \"abcdef\", -3\n\
         let sSubNl := Sv_SubString \"abcdef\", 2, -1\n\
         let sE1 := \"abcdef\"\n\
         Sv_Erase sE1, -2\n\
         let sE2 := \"abcdef\"\n\
         Sv_Erase sE2, 2, -1\n\
         end",
    );
    assert_eq!(h.text(1), "");
    assert_eq!(h.text(2), "cdef");
    assert_eq!(h.text(3), "abcdef");
    assert_eq!(h.text(4), "ab");
}

#[test]
fn sv_compare_returns_minus2_for_destructed_or_uninitialized_string_var() {
    let h = run_strict(
        "string_var sGone\nstring_var sUninit\nstring_var sBan\nfloat fDestr\nfloat fUninit\nfloat fOrd\n\
         begin GameMode\n\
         let sGone := \"tmp\"\n\
         Sv_Destruct sGone\n\
         let sBan := \"banana\"\n\
         let fDestr := Sv_Compare \"a\" sGone\n\
         let fUninit := Sv_Compare \"a\" sUninit\n\
         let fOrd := Sv_Compare \"apple\" sBan\n\
         end",
    );
    assert_eq!(h.vars[&4], Value::Num(-2.0));
    assert_eq!(h.vars[&5], Value::Num(-2.0));
    assert_eq!(h.vars[&6], Value::Num(-1.0));
}

#[test]
fn sv_getchar_returns_minus1_for_negative_and_oob_indices() {
    let h = run_strict(
        "float f0\nfloat f2\nfloat fM1\nfloat fM10\nfloat fOob\nfloat fEmp\n\
         begin GameMode\n\
         let f0 := Sv_GetChar \"ABC\", 0\n\
         let f2 := Sv_GetChar \"ABC\", 2\n\
         let fM1 := Sv_GetChar \"ABC\", -1\n\
         let fM10 := Sv_GetChar \"ABC\", -10\n\
         let fOob := Sv_GetChar \"ABC\", 99\n\
         let fEmp := Sv_GetChar \"\", 0\n\
         end",
    );
    assert_eq!(h.vars[&1], Value::Num(65.0));
    assert_eq!(h.vars[&2], Value::Num(67.0));
    assert_eq!(h.vars[&3], Value::Num(-1.0));
    assert_eq!(h.vars[&4], Value::Num(-1.0));
    assert_eq!(h.vars[&5], Value::Num(-1.0));
    assert_eq!(h.vars[&6], Value::Num(-1.0));
}

#[test]
fn sin_cos_tan_support_optional_arc_flag_in_degrees() {
    let h = run_strict(
        "float fS\nfloat fC\nfloat fT\nfloat fAs\nfloat fAc\nfloat fAt\n\
         begin GameMode\n\
         let fS := Sin 30\n\
         let fC := Cos 60\n\
         let fT := Tan 45\n\
         let fAs := Sin 0.5 1\n\
         let fAc := Cos 0.5 1\n\
         let fAt := Tan 1 1\n\
         end",
    );
    assert!((h.vars[&1].num() - 0.5).abs() < 1e-6);
    assert!((h.vars[&2].num() - 0.5).abs() < 1e-6);
    assert!((h.vars[&3].num() - 1.0).abs() < 1e-6);
    assert!((h.vars[&4].num() - 30.0).abs() < 1e-6);
    assert!((h.vars[&5].num() - 60.0).abs() < 1e-6);
    assert!((h.vars[&6].num() - 45.0).abs() < 1e-6);
}

#[test]
fn log_supports_optional_second_base_argument() {
    let h = run_strict(
        "float fLn10\nfloat fLog100\nfloat fLog8\n\
         begin GameMode\n\
         let fLn10 := Log 10\n\
         let fLog100 := Log 100 10\n\
         let fLog8 := Log 8 2\n\
         end",
    );
    assert!((h.vars[&1].num() - 10.0_f64.ln()).abs() < 1e-6);
    assert!((h.vars[&2].num() - 2.0).abs() < 1e-6);
    assert!((h.vars[&3].num() - 3.0).abs() < 1e-6);
}

#[test]
fn fmod_uses_floored_modulo_for_negative_divisors_and_returns_nan_for_zero() {
    let h = run_strict(
        "float f1\nfloat f2\nfloat f3\nfloat f4\nfloat f5\nfloat f6\nfloat f7\nfloat f8\nfloat fZero\n\
         begin GameMode\n\
         let f1 := fmod 10 3 0\n\
         let f2 := fmod -10 3 0\n\
         let f3 := fmod 10 -3 0\n\
         let f4 := fmod -10 -3 0\n\
         let f5 := fmod -1 360 0\n\
         let f6 := fmod 370 360 -180\n\
         let f7 := fmod -190 360 -180\n\
         let f8 := fmod 5.5 2 0\n\
         let fZero := fmod 5 0 0\n\
         end",
    );
    assert_eq!(h.vars[&1], Value::Num(1.0));
    assert_eq!(h.vars[&2], Value::Num(2.0));
    assert_eq!(h.vars[&3], Value::Num(-2.0));
    assert_eq!(h.vars[&4], Value::Num(-1.0));
    assert_eq!(h.vars[&5], Value::Num(359.0));
    assert_eq!(h.vars[&6], Value::Num(10.0));
    assert_eq!(h.vars[&7], Value::Num(170.0));
    assert_eq!(h.vars[&8], Value::Num(1.5));
    assert!(h.vars[&9].num().is_nan(), "fmod divisor 0 must return NaN");
}

#[test]
fn sv_replace_honors_search_len_window_and_max_replacements() {
    let h = run_strict(
        "string_var s1\nstring_var s2\nstring_var s3\nstring_var s4\nstring_var s5\nstring_var s6\n\
         float c1\nfloat c2\nfloat c3\nfloat c4\nfloat c5\nfloat c6\n\
         begin GameMode\n\
         let s1 := \"aaaaaa\"\n\
         let c1 := Sv_Replace \"a|X\" s1 0 3\n\
         let s2 := \"aaaaaa\"\n\
         let c2 := Sv_Replace \"a|X\" s2 2 3\n\
         let s3 := \"aaaaaa\"\n\
         let c3 := Sv_Replace \"a|X\" s3 0 6 0 0\n\
         let s4 := \"aaaaaa\"\n\
         let c4 := Sv_Replace \"a|X\" s4 0 6 0 2\n\
         let s5 := \"aaaaaa\"\n\
         let c5 := Sv_Replace \"a|X\" s5 2 -1\n\
         let s6 := \"aaaaaa\"\n\
         let c6 := Sv_Replace \"a|X\" s6 -2 3\n\
         end",
    );
    assert_eq!(h.text(1), "XXXaaa");
    assert_eq!(h.text(2), "aaXXXa");
    assert_eq!(h.text(3), "aaaaaa");
    assert_eq!(h.text(4), "XXaaaa");
    assert_eq!(h.text(5), "aaXXXX");
    assert_eq!(h.text(6), "XXXaaa");
    assert_eq!(
        (h.vars[&7], h.vars[&8], h.vars[&9], h.vars[&10], h.vars[&11], h.vars[&12]),
        (Value::Num(3.0), Value::Num(3.0), Value::Num(0.0), Value::Num(2.0), Value::Num(4.0), Value::Num(3.0))
    );
}

#[test]
fn sv_tonumeric_parses_0x_hex_and_leading_dot_decimals() {
    let h = run_strict(
        "float fHex\nfloat fSignedHex\nfloat fDot\nfloat fExp\nfloat fPrefix\nfloat fBad\n\
         begin GameMode\n\
         let fHex := Sv_ToNumeric \"0x1F\"\n\
         let fSignedHex := Sv_ToNumeric \"-0x10\"\n\
         let fDot := Sv_ToNumeric \".5\"\n\
         let fExp := Sv_ToNumeric \"-1.5e2\"\n\
         let fPrefix := Sv_ToNumeric \"  +3.25xyz\"\n\
         let fBad := Sv_ToNumeric \"abc\"\n\
         end",
    );
    assert_eq!(h.vars[&1], Value::Num(31.0));
    assert_eq!(h.vars[&2], Value::Num(0.0), "unprobed signed -0x hex is not parsed");
    assert_eq!(h.vars[&3], Value::Num(0.5));
    assert_eq!(h.vars[&4], Value::Num(-150.0));
    assert_eq!(h.vars[&5], Value::Num(3.25));
    assert_eq!(h.vars[&6], Value::Num(0.0));
}
