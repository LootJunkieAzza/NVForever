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
fn bitwise_functions_return_unsigned_u32_values_and_support_3arg_setbit() {
    let h = run_script(r#"float fSign
float fAndS
float fXorN
float fNotM1
float fSb3c
float fSb31
float fCb31
float fRs1
begin GameMode
let fSign := LeftShift 1 31
let fAndS := LogicalAnd -1 fSign
let fXorN := LogicalXor -1 0
let fNotM1 := LogicalNot -1
let fSb3c := SetBit 15 1 0
let fSb31 := SetBit 0 31
let fCb31 := ClearBit -1 31
let fRs1 := RightShift fSign 1
end
"#);
    assert_eq!(h.vars[&1], Value::Num(2_147_483_648.0));
    assert_eq!(h.vars[&2], Value::Num(2_147_483_648.0));
    assert_eq!(h.vars[&3], Value::Num(4_294_967_295.0));
    assert_eq!(h.vars[&4], Value::Num(0.0));
    assert_eq!(h.vars[&5], Value::Num(13.0));
    assert_eq!(h.vars[&6], Value::Num(2_147_483_648.0));
    assert_eq!(h.vars[&7], Value::Num(2_147_483_647.0));
    assert_eq!(h.vars[&8], Value::Num(1_073_741_824.0));
}
