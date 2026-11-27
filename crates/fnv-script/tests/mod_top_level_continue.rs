//! Regression tests for top-level `break`/`continue` tolerance in mod runtime
//! compilation (`Seed::unknown_calls`, e.g. `CompileScript` UDFs) vs. strict
//! compilation keeping the fatal `outside_a_loop` error.

use fnv_script::compile::{compile_mod_runtime, compile_runtime, Env, FormInfo};
use fnv_script::decode;
use fnv_script::ir::{RefIndex, VarRef, VarSlot};
use fnv_script::table::FunctionTable;
use fnv_script::unit::VarDecl;
use fnv_script::vm::{ArgValue, Exit, Host, RunOpts, Value, Vm};
use std::collections::BTreeMap;

fn table() -> FunctionTable {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    FunctionTable::load(&p).expect("functions.json loads")
}

struct NoEnv;
impl Env for NoEnv {
    fn form(&self, _editor_id: &str) -> Option<FormInfo> { None }
    fn info(&self, _form: u32) -> Option<FormInfo> { None }
    fn editor_id(&self, _form: u32) -> Option<String> { None }
    fn local_form(&self, _local: u32) -> Option<u32> { None }
    fn script_vars(&self, _form: u32) -> Option<Vec<VarDecl>> { None }
}

#[derive(Default)]
struct MiniHost {
    locals: BTreeMap<u16, Value>,
}

impl Host for MiniHost {
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        match var.var {
            VarSlot::Local { index, .. } => Ok(self.locals.get(&index).copied().unwrap_or_default()),
            _ => Ok(Value::ZERO),
        }
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        if let VarSlot::Local { index, .. } = var.var {
            self.locals.insert(index, value);
        }
        Ok(())
    }
    fn ref_value(&mut self, r: RefIndex) -> Result<Value, String> {
        Err(format!("unexpected ref {r:?}"))
    }
    fn call(&mut self, _opcode: u16, _on: Option<Value>, _args: &[ArgValue]) -> Result<Value, String> {
        Ok(Value::ZERO)
    }
}

#[test]
fn mod_runtime_compilation_tolerates_top_level_continue_and_break_while_strict_rejects() {
    let tbl = table();
    let src = "int bSkip\nint iOut\nBegin Function { bSkip }\nif bSkip == 1\n    continue\nendif\nset iOut to 77\nEnd\n";

    // Strict compilation rejects top-level `continue` with a fatal `outside_a_loop` error.
    let strict = compile_runtime(src, &tbl, &NoEnv);
    assert_eq!(strict.errors().next().map(|e| e.kind), Some("outside_a_loop"));

    // Mod runtime compilation (`Seed::unknown_calls = true`) warns non-fatally and compiles cleanly.
    let compiled = compile_mod_runtime(src, &tbl, &NoEnv);
    assert!(compiled.errors().next().is_none(), "expected no fatal errors, got {:?}", compiled.diags);
    assert!(
        compiled.diags.iter().any(|d| d.kind == "outside_a_loop" && !d.error),
        "expected non-fatal outside_a_loop warning, got {:?}",
        compiled.diags
    );

    let decoded = decode(&compiled.bytes, &tbl);
    let body = &decoded.program.blocks[0].body;
    let skip_idx = compiled.variables.iter().find(|v| v.name.eq_ignore_ascii_case("bSkip")).unwrap().index as u16;
    let out_idx = compiled.variables.iter().find(|v| v.name.eq_ignore_ascii_case("iOut")).unwrap().index as u16;

    // When bSkip == 1, top-level `continue` exits the block early before `set iOut to 77`.
    let mut host = MiniHost::default();
    host.locals.insert(skip_idx, Value::Num(1.0));
    let exit = Vm::new(&tbl, &mut host, RunOpts::default()).run(body).expect("VM executes top-level continue");
    assert!(matches!(exit, Exit::Finished | Exit::Returned), "unexpected exit: {exit:?}");
    assert_eq!(host.locals.get(&out_idx).copied().unwrap_or_default(), Value::ZERO);

    // When bSkip == 0, execution reaches `set iOut to 77`.
    let mut host2 = MiniHost::default();
    host2.locals.insert(skip_idx, Value::Num(0.0));
    let exit2 = Vm::new(&tbl, &mut host2, RunOpts::default()).run(body).expect("VM executes past skipped continue");
    assert!(matches!(exit2, Exit::Finished | Exit::Returned), "unexpected exit: {exit2:?}");
    assert_eq!(host2.locals.get(&out_idx).copied().unwrap_or_default(), Value::Num(77.0));
}
