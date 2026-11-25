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
    calling: Option<u32>,
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
            crate::VarType::Other(t) if t == "string_var" => VarKind::String,
            crate::VarType::Other(t) if t == "array_var" => VarKind::Array,
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
