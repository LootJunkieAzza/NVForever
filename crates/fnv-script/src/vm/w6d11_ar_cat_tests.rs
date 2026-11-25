use crate::compile::{compile, Env, FormInfo};
use crate::vm::{ArgValue, ExtStore, Host, RunOpts, Value, VarKind, Vm};
use crate::{decode, FunctionTable, RefIndex, VarDecl, VarRef};
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
    refs: Vec<crate::unit::RefSlot>,
}

impl Host for TestHost {
    fn var_kind(&mut self, v: &VarRef) -> VarKind {
        match v.var {
            crate::ir::VarSlot::Local { index, .. } => self.kinds.get(&index).copied().unwrap_or(VarKind::Value),
            _ => VarKind::Value,
        }
    }
    fn ext(&mut self) -> Option<&mut ExtStore> { Some(&mut self.ext) }
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let crate::ir::VarSlot::Local { index, .. } = var.var else { return Ok(Value::Num(0.0)) };
        Ok(self.vars.get(&index).copied().unwrap_or(Value::Num(0.0)))
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        if let crate::ir::VarSlot::Local { index, .. } = var.var {
            self.vars.insert(index, value);
        }
        Ok(())
    }
    fn ref_value(&mut self, r: RefIndex) -> Result<Value, String> {
        match self.refs.get(r.0.saturating_sub(1) as usize) {
            Some(crate::unit::RefSlot::Local(i)) => Ok(self.vars.get(&(*i as u16)).copied().unwrap_or(Value::Ref(0))),
            Some(crate::unit::RefSlot::Form(f)) => Ok(Value::Ref(*f)),
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
            crate::VarType::Other(t) if t == "string_var" => VarKind::String,
            crate::VarType::Other(t) if t == "array_var" => VarKind::Array,
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
