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
            crate::VarType::Other(t) if t == "string_var" => VarKind::String,
            crate::VarType::Other(t) if t == "array_var" => VarKind::Array,
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
fn sv_substring_and_sv_erase_handle_negative_start_and_negative_count() {
    let h = run_script(r#"string_var sSubNs
string_var sSubNl
string_var sE1
string_var sE2
begin GameMode
let sSubNs := Sv_SubString "abcdef", -3
let sSubNl := Sv_SubString "abcdef", 2, -1
let sE1 := "abcdef"
Sv_Erase sE1, -2
let sE2 := "abcdef"
Sv_Erase sE2, 2, -1
end
"#);
    assert_eq!(h.ext.text(h.vars[&1].num() as u32), Some(""));
    assert_eq!(h.ext.text(h.vars[&2].num() as u32), Some("cdef"));
    assert_eq!(h.ext.text(h.vars[&3].num() as u32), Some("abcdef"));
    assert_eq!(h.ext.text(h.vars[&4].num() as u32), Some("ab"));
}
