//! Wave 6D JIP/Lutana VM parity regression tests (consolidated from Wave 6D-11 Draft PRs).

mod w6d11_ar_cat_tests {
use fnv_script::compile::{compile, Env, FormInfo};
use fnv_script::vm::{ArgValue, ExtStore, Host, RunOpts, Value, VarKind, Vm};
use fnv_script::{decode, FunctionTable, RefIndex, VarDecl, VarRef};
use std::collections::HashMap;

struct NoForms;
impl Env for NoForms {
    fn form(&self, _: &str) -> Option<FormInfo> { None }
    fn info(&self, _: u32) -> Option<FormInfo> { None }
    fn editor_id(&self, _: u32) -> Option<String> { None }
    fn local_form(&self, local: u32) -> Option<u32> { Some(local) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

struct TestHost {
    ext: ExtStore,
    vars: HashMap<u16, Value>,
    kinds: HashMap<u16, VarKind>,
    refs: Vec<fnv_script::unit::RefSlot>,
}

impl Host for TestHost {
    fn var_kind(&mut self, v: &VarRef) -> VarKind {
        match v.var {
            fnv_script::ir::VarSlot::Local { index, .. } => self.kinds.get(&index).copied().unwrap_or(VarKind::Value),
            _ => VarKind::Value,
        }
    }
    fn ext(&mut self) -> Option<&mut ExtStore> { Some(&mut self.ext) }
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let fnv_script::ir::VarSlot::Local { index, .. } = var.var else { return Ok(Value::Num(0.0)) };
        Ok(self.vars.get(&index).copied().unwrap_or(Value::Num(0.0)))
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        if let fnv_script::ir::VarSlot::Local { index, .. } = var.var {
            self.vars.insert(index, value);
        }
        Ok(())
    }
    fn ref_value(&mut self, r: RefIndex) -> Result<Value, String> {
        match self.refs.get(r.0.saturating_sub(1) as usize) {
            Some(fnv_script::unit::RefSlot::Local(i)) => Ok(self.vars.get(&(*i as u16)).copied().unwrap_or(Value::Ref(0))),
            Some(fnv_script::unit::RefSlot::Form(f)) => Ok(Value::Ref(*f)),
            _ => Ok(Value::Ref(0)),
        }
    }
    fn call(&mut self, _: u16, _: Option<Value>, _: &[ArgValue]) -> Result<Value, String> { Ok(Value::Num(0.0)) }
}

#[test]
fn ar_cat_checks_stringmap_keys_case_insensitively_and_rejects_cross_kind() {
    let mut host = TestHost { ext: ExtStore::default(), vars: HashMap::new(), kinds: HashMap::new(), refs: Vec::new() };
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let table = FunctionTable::load(&p).expect("functions.json loads");
    let src = r#"array_var aSm1
array_var aSm2
array_var aLst
array_var aMap
float fA0
float fSz0
float fA1
float fCrossSz
begin GameMode
let aSm1 := Ar_Map "A"::1, "b"::2
let aSm2 := Ar_Map "a"::99, "c"::3
Ar_Cat aSm1, aSm2, 0
let fA0 := aSm1["A"]
let fSz0 := Ar_Size aSm1
Ar_Cat aSm1, aSm2, 1
let fA1 := aSm1["A"]
let aLst := Ar_List 1, 2
let aMap := Ar_Map 5::50
Ar_Cat aLst, aMap
let fCrossSz := Ar_Size aLst
end
"#;
    let compiled = compile(src, &table, &NoForms, None);
    assert!(compiled.errors().next().is_none(), "{:?}", compiled.diags);
    let dec = decode(&compiled.bytes, &table);
    for v in &compiled.variables {
        let k = match &v.ty {
            fnv_script::VarType::Other(t) if t == "string_var" => VarKind::String,
            fnv_script::VarType::Other(t) if t == "array_var" => VarKind::Array,
            _ => VarKind::Value,
        };
        host.kinds.insert(v.index as u16, k);
    }
    Vm::new(&table, &mut host, RunOpts { strict: true, ..RunOpts::default() })
        .run(&dec.program.blocks[0].body)
        .expect("runs");
    assert_eq!(host.vars[&5], Value::Num(1.0), "override=0 preserves existing 'A' against 'a'");
    assert_eq!(host.vars[&6], Value::Num(3.0), "size is 3 (A, b, c)");
    assert_eq!(host.vars[&7], Value::Num(99.0), "override=1 replaces 'A' with 99");
    assert_eq!(host.vars[&8], Value::Num(2.0), "cross-kind Map into List is a no-op");
}

}

mod w6d11_auxvar_asarray_tests {
use fnv_script::compile::{compile, Env, FormInfo};
use fnv_script::vm::{ArgValue, ExtStore, Host, RunOpts, Value, VarKind, Vm};
use fnv_script::{decode, FunctionTable, RefIndex, VarDecl, VarRef};
use std::collections::HashMap;

struct NoForms;
impl Env for NoForms {
    fn form(&self, _: &str) -> Option<FormInfo> { None }
    fn info(&self, _: u32) -> Option<FormInfo> { None }
    fn editor_id(&self, _: u32) -> Option<String> { None }
    fn local_form(&self, local: u32) -> Option<u32> { Some(local) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

struct TestHost {
    ext: ExtStore,
    vars: HashMap<u16, Value>,
    kinds: HashMap<u16, VarKind>,
    calling: Option<u32>,
}

impl Host for TestHost {
    fn var_kind(&mut self, v: &VarRef) -> VarKind {
        match v.var {
            fnv_script::ir::VarSlot::Local { index, .. } => self.kinds.get(&index).copied().unwrap_or(VarKind::Value),
            _ => VarKind::Value,
        }
    }
    fn ext(&mut self) -> Option<&mut ExtStore> { Some(&mut self.ext) }
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let fnv_script::ir::VarSlot::Local { index, .. } = var.var else { return Ok(Value::Num(0.0)) };
        Ok(self.vars.get(&index).copied().unwrap_or(Value::Num(0.0)))
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        if let fnv_script::ir::VarSlot::Local { index, .. } = var.var {
            self.vars.insert(index, value);
        }
        Ok(())
    }
    fn ref_value(&mut self, _: RefIndex) -> Result<Value, String> { Ok(Value::Ref(0)) }
    fn call(&mut self, _: u16, _: Option<Value>, _: &[ArgValue]) -> Result<Value, String> { Ok(Value::Num(0.0)) }
    fn calling_ref(&mut self) -> Option<u32> { self.calling }
}

fn run_script(host: &mut TestHost, decls: &str, body: &str) {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let table = FunctionTable::load(&p).expect("functions.json loads");
    let src = format!("{decls}\nbegin GameMode\n{body}\nend\n");
    let compiled = compile(&src, &table, &NoForms, None);
    let errors: Vec<_> = compiled.errors().collect();
    assert!(errors.is_empty(), "compile errors: {errors:?}");
    let dec = decode(&compiled.bytes, &table);
    assert!(dec.report.issues.is_empty(), "decode issues: {:?}", dec.report.issues);
    host.vars.clear();
    host.kinds.clear();
    for v in &compiled.variables {
        let k = match &v.ty {
            fnv_script::VarType::Other(t) if t == "string_var" => VarKind::String,
            fnv_script::VarType::Other(t) if t == "array_var" => VarKind::Array,
            _ => VarKind::Value,
        };
        host.kinds.insert(v.index as u16, k);
    }
    Vm::new(&table, host, RunOpts { strict: true, ..RunOpts::default() })
        .run(&dec.program.blocks[0].body)
        .expect("runs");
}

#[test]
fn auxvar_get_as_array_returns_ar_null_for_missing_var_and_list_when_present() {
    let mut host = TestHost { ext: ExtStore::default(), vars: HashMap::new(), kinds: HashMap::new(), calling: Some(0x14) };
    let decls = "float fMis\nfloat fHit";
    let body = r#"
let fMis := Ar_Size (AuxVarGetAsArr "*w6d11_missing")
AuxVarSetFlt "*w6d11_present" 10 0
AuxVarSetFlt "*w6d11_present" 20 -1
let fHit := Ar_Size (AuxVarGetAsArr "*w6d11_present")
"#;
    run_script(&mut host, decls, body);
    assert_eq!(host.vars[&1], Value::Num(-1.0), "missing AuxVar returns ar_Null (size -1)");
    assert_eq!(host.vars[&2], Value::Num(2.0), "populated AuxVar returns 2-element list");

    host.calling = None;
    run_script(&mut host, "float fNoOwn\nref rEmpty", r#"let fNoOwn := Ar_Size (AuxVarGetAsArr "*w6d11_present" rEmpty)"#);
    assert_eq!(host.vars[&1], Value::Num(-1.0), "no calling ref and null owner returns ar_Null (size -1)");
}

}

mod w6d11_auxvar_eraseall_tests {
use fnv_script::compile::{compile, Env, FormInfo};
use fnv_script::vm::{ArgValue, ExtStore, Host, RunOpts, Value, VarKind, Vm};
use fnv_script::{decode, FunctionTable, RefIndex, VarDecl, VarRef};
use std::collections::HashMap;

struct NoForms;
impl Env for NoForms {
    fn form(&self, _: &str) -> Option<FormInfo> { None }
    fn info(&self, _: u32) -> Option<FormInfo> { None }
    fn editor_id(&self, _: u32) -> Option<String> { None }
    fn local_form(&self, local: u32) -> Option<u32> { Some(local) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

struct TestHost {
    ext: ExtStore,
    vars: HashMap<u16, Value>,
    calling: Option<u32>,
    plugin: Option<String>,
}

impl Host for TestHost {
    fn var_kind(&mut self, _: &VarRef) -> VarKind { VarKind::Value }
    fn ext(&mut self) -> Option<&mut ExtStore> { Some(&mut self.ext) }
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let fnv_script::ir::VarSlot::Local { index, .. } = var.var else { return Ok(Value::Num(0.0)) };
        Ok(self.vars.get(&index).copied().unwrap_or(Value::Num(0.0)))
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        if let fnv_script::ir::VarSlot::Local { index, .. } = var.var {
            self.vars.insert(index, value);
        }
        Ok(())
    }
    fn ref_value(&mut self, _: RefIndex) -> Result<Value, String> { Ok(Value::Ref(0)) }
    fn call(&mut self, _: u16, _: Option<Value>, _: &[ArgValue]) -> Result<Value, String> { Ok(Value::Num(0.0)) }
    fn calling_ref(&mut self) -> Option<u32> { self.calling }
    fn calling_plugin(&mut self) -> Option<String> { self.plugin.clone() }
    fn mod_index(&mut self, name: &str) -> Option<u32> { name.eq_ignore_ascii_case("FalloutNV.esm").then_some(0) }
    fn form_sig(&mut self, form: u32) -> Option<[u8; 4]> { (form == 0xF || form == 0x14).then_some(*b"MISC") }
}

fn run_script(host: &mut TestHost, decls: &str, body: &str) {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let table = FunctionTable::load(&p).expect("functions.json loads");
    let src = format!("{decls}\nbegin GameMode\n{body}\nend\n");
    let compiled = compile(&src, &table, &NoForms, None);
    let errors: Vec<_> = compiled.errors().collect();
    assert!(errors.is_empty(), "compile errors: {errors:?}");
    let dec = decode(&compiled.bytes, &table);
    assert!(dec.report.issues.is_empty(), "decode issues: {:?}", dec.report.issues);
    host.vars.clear();
    Vm::new(&table, host, RunOpts { strict: true, ..RunOpts::default() })
        .run(&dec.program.blocks[0].body)
        .expect("runs");
}

#[test]
fn auxvar_erase_all_erases_selected_class_and_keeps_other_classes() {
    let mut host = TestHost { ext: ExtStore::default(), vars: HashMap::new(), calling: Some(0x14), plugin: None };
    let decls = "ref rCaps\nfloat fRemT\nfloat fRemP\nfloat fPlrT";
    let body = r#"
let rCaps := GetFormFromMod "FalloutNV.esm" "F"
AuxVarSetFlt "*w6d11_ea1" 1 0 rCaps
AuxVarSetFlt "w6d11_ea0" 2 0 rCaps
AuxVarSetFlt "*w6d11_plr" 3 0
AuxVarEraseAll 1 rCaps
let fRemT := AuxVarSize "*w6d11_ea1" rCaps
let fRemP := AuxVarSize "w6d11_ea0" rCaps
let fPlrT := AuxVarSize "*w6d11_plr"
"#;
    run_script(&mut host, decls, body);
    assert_eq!(host.vars[&2], Value::Num(0.0), "temp var on target erased");
    assert_eq!(host.vars[&3], Value::Num(1.0), "permanent var on target preserved");
    assert_eq!(host.vars[&4], Value::Num(1.0), "temp var on different owner preserved");
}

#[test]
fn auxvar_erase_all_respects_private_vs_public_plugin_scope() {
    let mut host = TestHost { ext: ExtStore::default(), vars: HashMap::new(), calling: Some(0x14), plugin: Some("ModA.esp".into()) };
    run_script(&mut host, "", r#"AuxVarSetFlt "*priv_a" 1 0
AuxVarSetFlt "*_pub" 2 0"#);
    host.plugin = Some("ModB.esp".into());
    run_script(&mut host, "", r#"AuxVarSetFlt "*priv_b" 3 0
AuxVarEraseAll 1"#);
    let decls = "float fB\nfloat fPub";
    run_script(&mut host, decls, r#"let fB := AuxVarSize "*priv_b"
let fPub := AuxVarSize "*_pub""#);
    assert_eq!(host.vars[&1], Value::Num(0.0), "ModB private temp erased");
    assert_eq!(host.vars[&2], Value::Num(1.0), "public temp preserved when erasing class 1");
    host.plugin = Some("ModA.esp".into());
    run_script(&mut host, "float fA", r#"let fA := AuxVarSize "*priv_a""#);
    assert_eq!(host.vars[&1], Value::Num(1.0), "ModA private temp preserved");
    run_script(&mut host, "float fPub2", r#"AuxVarEraseAll 3
let fPub2 := AuxVarSize "*_pub""#);
    assert_eq!(host.vars[&1], Value::Num(0.0), "class 3 erases public temp");
}

}

mod w6d11_gffm_tests {
use fnv_script::compile::{compile, Env, FormInfo};
use fnv_script::vm::{ArgValue, ExtStore, Host, RunOpts, Value, VarKind, Vm};
use fnv_script::{decode, FunctionTable, RefIndex, VarDecl, VarRef};
use std::collections::HashMap;

struct NoForms;
impl Env for NoForms {
    fn form(&self, _: &str) -> Option<FormInfo> { None }
    fn info(&self, _: u32) -> Option<FormInfo> { None }
    fn editor_id(&self, _: u32) -> Option<String> { None }
    fn local_form(&self, local: u32) -> Option<u32> { Some(local) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

struct TestHost {
    ext: ExtStore,
    vars: HashMap<u16, Value>,
}

impl Host for TestHost {
    fn var_kind(&mut self, _: &VarRef) -> VarKind { VarKind::Value }
    fn ext(&mut self) -> Option<&mut ExtStore> { Some(&mut self.ext) }
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let fnv_script::ir::VarSlot::Local { index, .. } = var.var else { return Ok(Value::Num(0.0)) };
        Ok(self.vars.get(&index).copied().unwrap_or(Value::Num(0.0)))
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        if let fnv_script::ir::VarSlot::Local { index, .. } = var.var {
            self.vars.insert(index, value);
        }
        Ok(())
    }
    fn ref_value(&mut self, _: RefIndex) -> Result<Value, String> { Ok(Value::Ref(0)) }
    fn call(&mut self, _: u16, _: Option<Value>, _: &[ArgValue]) -> Result<Value, String> { Ok(Value::Num(0.0)) }
    fn mod_index(&mut self, name: &str) -> Option<u32> {
        if name.eq_ignore_ascii_case("FalloutNV.esm") { Some(0) } else { None }
    }
    fn form_sig(&mut self, form: u32) -> Option<[u8; 4]> { (form == 0xF).then_some(*b"MISC") }
}

#[test]
fn getformfrommod_accepts_0x_hex_prefix_without_widening_stringtoref() {
    let mut host = TestHost { ext: ExtStore::default(), vars: HashMap::new() };
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let table = FunctionTable::load(&p).expect("functions.json loads");
    let src = r#"ref rA
ref rB
ref rC
ref rD
ref rS2rBare
ref rS2rOx
begin GameMode
let rA := GetFormFromMod "FalloutNV.esm" "F"
let rB := GetFormFromMod "FalloutNV.esm" "0xF"
let rC := GetFormFromMod "FalloutNV.esm" "00000f"
let rD := GetFormFromMod "FalloutNV.esm" "0000000F"
let rS2rBare := StringToRef "FalloutNV.esm:F"
let rS2rOx := StringToRef "FalloutNV.esm:0xF"
end
"#;
    let compiled = compile(src, &table, &NoForms, None);
    assert!(compiled.errors().next().is_none(), "{:?}", compiled.diags);
    let dec = decode(&compiled.bytes, &table);
    Vm::new(&table, &mut host, RunOpts { strict: true, ..RunOpts::default() })
        .run(&dec.program.blocks[0].body)
        .expect("runs");
    assert_eq!(host.vars[&1], Value::Ref(0xF));
    assert_eq!(host.vars[&2], Value::Ref(0xF));
    assert_eq!(host.vars[&3], Value::Ref(0xF));
    assert_eq!(host.vars[&4], Value::Ref(0xF));
    assert_eq!(host.vars[&5], Value::Ref(0xF));
    assert_eq!(host.vars[&6], Value::Ref(0), "StringToRef must keep bare-hex-only Vm::plugin_form");
}

}

mod w6d11_refmap_getall_tests {
use fnv_script::compile::{compile, Env, FormInfo};
use fnv_script::vm::{ArgValue, ExtStore, Host, RunOpts, Value, VarKind, Vm};
use fnv_script::{decode, FunctionTable, RefIndex, VarDecl, VarRef};
use std::collections::HashMap;

struct NoForms;
impl Env for NoForms {
    fn form(&self, _: &str) -> Option<FormInfo> { None }
    fn info(&self, _: u32) -> Option<FormInfo> { None }
    fn editor_id(&self, _: u32) -> Option<String> { None }
    fn local_form(&self, local: u32) -> Option<u32> { Some(local) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

struct TestHost {
    ext: ExtStore,
    vars: HashMap<u16, Value>,
    kinds: HashMap<u16, VarKind>,
    calling: Option<u32>,
}

impl Host for TestHost {
    fn var_kind(&mut self, v: &VarRef) -> VarKind {
        match v.var {
            fnv_script::ir::VarSlot::Local { index, .. } => self.kinds.get(&index).copied().unwrap_or(VarKind::Value),
            _ => VarKind::Value,
        }
    }
    fn ext(&mut self) -> Option<&mut ExtStore> { Some(&mut self.ext) }
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let fnv_script::ir::VarSlot::Local { index, .. } = var.var else { return Ok(Value::Num(0.0)) };
        Ok(self.vars.get(&index).copied().unwrap_or(Value::Num(0.0)))
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        if let fnv_script::ir::VarSlot::Local { index, .. } = var.var {
            self.vars.insert(index, value);
        }
        Ok(())
    }
    fn ref_value(&mut self, _: RefIndex) -> Result<Value, String> { Ok(Value::Ref(0)) }
    fn call(&mut self, _: u16, _: Option<Value>, _: &[ArgValue]) -> Result<Value, String> { Ok(Value::Num(0.0)) }
    fn calling_ref(&mut self) -> Option<u32> { self.calling }
}

fn run_script(host: &mut TestHost, decls: &str, body: &str) {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let table = FunctionTable::load(&p).expect("functions.json loads");
    let src = format!("{decls}\nbegin GameMode\n{body}\nend\n");
    let compiled = compile(&src, &table, &NoForms, None);
    let errors: Vec<_> = compiled.errors().collect();
    assert!(errors.is_empty(), "compile errors: {errors:?}");
    let dec = decode(&compiled.bytes, &table);
    assert!(dec.report.issues.is_empty(), "decode issues: {:?}", dec.report.issues);
    host.vars.clear();
    host.kinds.clear();
    for v in &compiled.variables {
        let k = match &v.ty {
            fnv_script::VarType::Other(t) if t == "string_var" => VarKind::String,
            fnv_script::VarType::Other(t) if t == "array_var" => VarKind::Array,
            _ => VarKind::Value,
        };
        host.kinds.insert(v.index as u16, k);
    }
    Vm::new(&table, host, RunOpts { strict: true, ..RunOpts::default() })
        .run(&dec.program.blocks[0].body)
        .expect("runs");
}

#[test]
fn refmap_get_all_accepts_class_int_and_returns_stringmap() {
    let mut host = TestHost { ext: ExtStore::default(), vars: HashMap::new(), kinds: HashMap::new(), calling: Some(0x14) };
    let decls = "array_var aAll\nstring_var sType\nfloat fSz\nfloat fHas\nfloat fPermSz";
    let body = r#"
RefMapSetFlt "*w6d11_it" 3
let aAll := RefMapGetAll 1
let sType := TypeOf aAll
let fSz := Ar_Size aAll
let fHas := Ar_HasKey aAll "*w6d11_it"
let fPermSz := Ar_Size (RefMapGetAll 0)
"#;
    run_script(&mut host, decls, body);
    let sid = host.vars[&2].num() as u32;
    assert_eq!(host.ext.text(sid), Some("StringMap"));
    assert_eq!(host.vars[&3], Value::Num(1.0));
    assert_eq!(host.vars[&4], Value::Num(1.0));
    assert_eq!(host.vars[&5], Value::Num(0.0));
}

}

mod w6d11_reftostring_tests {
use fnv_script::compile::{compile, Env, FormInfo};
use fnv_script::vm::{ArgValue, ExtStore, Host, RunOpts, Value, VarKind, Vm};
use fnv_script::{decode, FunctionTable, RefIndex, VarDecl, VarRef};
use std::collections::HashMap;

struct NoForms;
impl Env for NoForms {
    fn form(&self, _: &str) -> Option<FormInfo> { None }
    fn info(&self, _: u32) -> Option<FormInfo> { None }
    fn editor_id(&self, _: u32) -> Option<String> { None }
    fn local_form(&self, local: u32) -> Option<u32> { Some(local) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

struct TestHost {
    ext: ExtStore,
    vars: HashMap<u16, Value>,
    kinds: HashMap<u16, VarKind>,
    refs: Vec<fnv_script::unit::RefSlot>,
}

impl Host for TestHost {
    fn var_kind(&mut self, v: &VarRef) -> VarKind {
        match v.var {
            fnv_script::ir::VarSlot::Local { index, .. } => self.kinds.get(&index).copied().unwrap_or(VarKind::Value),
            _ => VarKind::Value,
        }
    }
    fn ext(&mut self) -> Option<&mut ExtStore> { Some(&mut self.ext) }
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let fnv_script::ir::VarSlot::Local { index, .. } = var.var else { return Ok(Value::Num(0.0)) };
        Ok(self.vars.get(&index).copied().unwrap_or(Value::Num(0.0)))
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        if let fnv_script::ir::VarSlot::Local { index, .. } = var.var {
            self.vars.insert(index, value);
        }
        Ok(())
    }
    fn ref_value(&mut self, r: RefIndex) -> Result<Value, String> {
        match self.refs.get(r.0.saturating_sub(1) as usize) {
            Some(fnv_script::unit::RefSlot::Local(i)) => Ok(self.vars.get(&(*i as u16)).copied().unwrap_or(Value::Ref(0))),
            Some(fnv_script::unit::RefSlot::Form(f)) => Ok(Value::Ref(*f)),
            _ => Ok(Value::Ref(0)),
        }
    }
    fn call(&mut self, _: u16, _: Option<Value>, _: &[ArgValue]) -> Result<Value, String> { Ok(Value::Num(0.0)) }
    fn mod_index(&mut self, name: &str) -> Option<u32> {
        if name.eq_ignore_ascii_case("FalloutNV.esm") { Some(0) } else { None }
    }
    fn mod_name(&mut self, index: u32) -> Option<String> { (index == 0).then(|| "FalloutNV.esm".into()) }
    fn form_sig(&mut self, form: u32) -> Option<[u8; 4]> {
        match form {
            0x0010_4C0F => Some(*b"ACHR"),
            0x0010_4C0C => Some(*b"NPC_"),
            _ => None,
        }
    }
    fn base_form(&mut self, form: u32) -> Option<u32> {
        (form == 0x0010_4C0F).then_some(0x0010_4C0C)
    }
}

#[test]
fn reftostring_resolves_base_when_passed_as_arg_and_keeps_ref_as_caller() {
    let mut host = TestHost { ext: ExtStore::default(), vars: HashMap::new(), kinds: HashMap::new(), refs: Vec::new() };
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let table = FunctionTable::load(&p).expect("functions.json loads");
    let src = r#"ref rDocR
string_var sOn
string_var sArg
string_var sNul
ref rNull
begin GameMode
let rDocR := GetFormFromMod "FalloutNV.esm" "104C0F"
let sOn := rDocR.RefToString
let sArg := RefToString rDocR
let sNul := RefToString rNull
end
"#;
    let compiled = compile(src, &table, &NoForms, None);
    assert!(compiled.errors().next().is_none(), "{:?}", compiled.diags);
    host.refs = compiled.refs.clone();
    let dec = decode(&compiled.bytes, &table);
    for v in &compiled.variables {
        let k = match &v.ty {
            fnv_script::VarType::Other(t) if t == "string_var" => VarKind::String,
            fnv_script::VarType::Other(t) if t == "array_var" => VarKind::Array,
            _ => VarKind::Value,
        };
        host.kinds.insert(v.index as u16, k);
    }
    Vm::new(&table, &mut host, RunOpts { strict: true, ..RunOpts::default() })
        .run(&dec.program.blocks[0].body)
        .expect("runs");
    assert_eq!(host.ext.text(host.vars[&2].num() as u32), Some("FalloutNV.esm:104C0F"));
    assert_eq!(host.ext.text(host.vars[&3].num() as u32), Some("FalloutNV.esm:104C0C"));
    assert_eq!(host.ext.text(host.vars[&4].num() as u32), Some(""));
}

}

mod w6d11_validateregex_tests {
use fnv_script::compile::{compile, Env, FormInfo};
use fnv_script::vm::{ArgValue, ExtStore, Host, RunOpts, Value, VarKind, Vm};
use fnv_script::{decode, FunctionTable, RefIndex, VarDecl, VarRef};
use std::collections::HashMap;

struct NoForms;
impl Env for NoForms {
    fn form(&self, _: &str) -> Option<FormInfo> { None }
    fn info(&self, _: u32) -> Option<FormInfo> { None }
    fn editor_id(&self, _: u32) -> Option<String> { None }
    fn local_form(&self, local: u32) -> Option<u32> { Some(local) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

struct TestHost {
    ext: ExtStore,
    vars: HashMap<u16, Value>,
    kinds: HashMap<u16, VarKind>,
}

impl Host for TestHost {
    fn var_kind(&mut self, v: &VarRef) -> VarKind {
        match v.var {
            fnv_script::ir::VarSlot::Local { index, .. } => self.kinds.get(&index).copied().unwrap_or(VarKind::Value),
            _ => VarKind::Value,
        }
    }
    fn ext(&mut self) -> Option<&mut ExtStore> { Some(&mut self.ext) }
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let fnv_script::ir::VarSlot::Local { index, .. } = var.var else { return Ok(Value::Num(0.0)) };
        Ok(self.vars.get(&index).copied().unwrap_or(Value::Num(0.0)))
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        if let fnv_script::ir::VarSlot::Local { index, .. } = var.var {
            self.vars.insert(index, value);
        }
        Ok(())
    }
    fn ref_value(&mut self, _: RefIndex) -> Result<Value, String> { Ok(Value::Ref(0)) }
    fn call(&mut self, _: u16, _: Option<Value>, _: &[ArgValue]) -> Result<Value, String> { Ok(Value::Num(0.0)) }
}

fn run_script(src: &str) -> TestHost {
    let mut host = TestHost { ext: ExtStore::default(), vars: HashMap::new(), kinds: HashMap::new() };
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let table = FunctionTable::load(&p).expect("functions.json loads");
    let compiled = compile(src, &table, &NoForms, None);
    let errors: Vec<_> = compiled.errors().collect();
    assert!(errors.is_empty(), "compile errors: {errors:?}");
    let dec = decode(&compiled.bytes, &table);
    assert!(dec.report.issues.is_empty(), "decode issues: {:?}", dec.report.issues);
    for v in &compiled.variables {
        let k = match &v.ty {
            fnv_script::VarType::Other(t) if t == "string_var" => VarKind::String,
            fnv_script::VarType::Other(t) if t == "array_var" => VarKind::Array,
            _ => VarKind::Value,
        };
        host.kinds.insert(v.index as u16, k);
    }
    Vm::new(&table, &mut host, RunOpts { strict: true, ..RunOpts::default() })
        .run(&dec.program.blocks[0].body)
        .expect("runs");
    host
}

#[test]
fn validateregex_vm_returns_observed_codes_and_populates_error_outvar() {
    let h = run_script(
        r#"string_var sErr
float vOk
float lOk
float vBrk
float lBrk
float vPar
float lPar
float vBrc
float lBrc
float vRng
float lRng
float vRep
float lRep
float vEsc
float lEsc
begin GameMode
let sErr := ""
let vOk := ValidateRegex "^[a-z]+$", sErr
let lOk := sv_Length sErr
let sErr := ""
let vBrk := ValidateRegex "[abc", sErr
let lBrk := sv_Length sErr
let sErr := ""
let vPar := ValidateRegex "(abc", sErr
let lPar := sv_Length sErr
let sErr := ""
let vBrc := ValidateRegex "a{3,1}", sErr
let lBrc := sv_Length sErr
let sErr := ""
let vRng := ValidateRegex "[z-a]", sErr
let lRng := sv_Length sErr
let sErr := ""
let vRep := ValidateRegex "*abc", sErr
let lRep := sv_Length sErr
let sErr := ""
let vEsc := ValidateRegex "\x", sErr
let lEsc := sv_Length sErr
end
"#,
    );
    assert_eq!(h.vars[&2], Value::Num(0.0), "valid ^[a-z]+$");
    assert_eq!(h.vars[&3], Value::Num(0.0), "valid pattern leaves sErr empty");
    assert_eq!(h.vars[&4], Value::Num(5.0), "[abc -> 5");
    assert!(h.vars[&5].num() > 0.0, "[abc populates sErr");
    assert_eq!(h.vars[&6], Value::Num(6.0), "(abc -> 6");
    assert!(h.vars[&7].num() > 0.0, "(abc populates sErr");
    assert_eq!(h.vars[&8], Value::Num(8.0), "a{{3,1}} -> 8");
    assert!(h.vars[&9].num() > 0.0, "a{{3,1}} populates sErr");
    assert_eq!(h.vars[&10], Value::Num(9.0), "[z-a] -> 9");
    assert!(h.vars[&11].num() > 0.0, "[z-a] populates sErr");
    assert_eq!(h.vars[&12], Value::Num(11.0), "*abc -> 11");
    assert!(h.vars[&13].num() > 0.0, "*abc populates sErr");
    assert_eq!(h.vars[&14], Value::Num(3.0), "\\x -> 3");
    assert!(h.vars[&15].num() > 0.0, "\\x populates sErr");
}

}
