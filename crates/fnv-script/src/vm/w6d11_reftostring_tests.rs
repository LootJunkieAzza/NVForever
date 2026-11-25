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
            crate::VarType::Other(t) if t == "string_var" => VarKind::String,
            crate::VarType::Other(t) if t == "array_var" => VarKind::Array,
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
