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
    calling: Option<u32>,
    plugin: Option<String>,
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
