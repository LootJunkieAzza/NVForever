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
}

impl Host for TestHost {
    fn var_kind(&mut self, _: &VarRef) -> VarKind { VarKind::Value }
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
    fn mod_index(&mut self, name: &str) -> Option<u32> {
        if name.eq_ignore_ascii_case("FalloutNV.esm") { Some(0) } else { None }
    }
    fn form_sig(&mut self, form: u32) -> Option<[u8; 4]> { (form == 0xF).then_some(*b"MISC") }
}

#[test]
fn getformfrommod_accepts_0x_hex_prefix_and_padded_forms() {
    let mut host = TestHost { ext: ExtStore::default(), vars: HashMap::new() };
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let table = FunctionTable::load(&p).expect("functions.json loads");
    let src = r#"ref rA
ref rB
ref rC
ref rD
begin GameMode
let rA := GetFormFromMod "FalloutNV.esm" "F"
let rB := GetFormFromMod "FalloutNV.esm" "0xF"
let rC := GetFormFromMod "FalloutNV.esm" "0X00000f"
let rD := GetFormFromMod "FalloutNV.esm" "0000000F"
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
}
