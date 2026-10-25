//! The compiler without game data: source → bytes → the decoder's IR
//! (docs/spec-notes/script-compiler.md). The parity over the shipped units is the lab
//! scenario L5.script.compile.vanilla.

use fnv_script::compile::{compile, Compiled, Env, FormInfo, Seed, PLAYER};
use fnv_script::unit::RefSlot;
use fnv_script::{decode, Arg, Expr, FunctionTable, Op, Program, Stmt, Token, VarDecl, VarType};

fn table() -> FunctionTable {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    FunctionTable::load(&p).expect("functions.json loads")
}

const QUEST: u32 = 0x0100_1000;
const CAPS: u32 = 0x0000_000F;
const GLOBAL: u32 = 0x0100_2000;
const DOOR: u32 = 0x0100_3000;

struct World;

impl Env for World {
    fn form(&self, editor_id: &str) -> Option<FormInfo> {
        let (form, sig) = match editor_id.to_ascii_lowercase().as_str() {
            "testquest" => (QUEST, *b"QUST"),
            "caps001" => (CAPS, *b"MISC"),
            "testglobal" => (GLOBAL, *b"GLOB"),
            "testdoorref" => (DOOR, *b"REFR"),
            _ => return None,
        };
        Some(FormInfo { form, sig })
    }
    fn info(&self, form: u32) -> Option<FormInfo> {
        [("testquest", QUEST), ("caps001", CAPS), ("testglobal", GLOBAL), ("testdoorref", DOOR)]
            .iter()
            .find(|(_, f)| *f == form)
            .and_then(|(n, _)| self.form(n))
    }
    fn editor_id(&self, form: u32) -> Option<String> {
        [("TestQuest", QUEST), ("Caps001", CAPS), ("TestGlobal", GLOBAL), ("TestDoorREF", DOOR)]
            .iter()
            .find(|(_, f)| *f == form)
            .map(|(n, _)| n.to_string())
    }
    fn local_form(&self, local: u32) -> Option<u32> {
        Some(local)
    }
    fn script_vars(&self, form: u32) -> Option<Vec<VarDecl>> {
        (form == QUEST).then(|| {
            vec![
                VarDecl { index: 1, name: "bDone".into(), ty: VarType::Int },
                VarDecl { index: 2, name: "fTimer".into(), ty: VarType::FloatOrRef },
            ]
        })
    }
}

fn build(src: &str) -> (Compiled, Program) {
    let t = table();
    let c = compile(src, &t, &World, None);
    let errors: Vec<_> = c.errors().collect();
    assert!(errors.is_empty(), "compile errors: {errors:?}");
    let d = decode(&c.bytes, &t);
    assert!(d.report.issues.is_empty(), "decoder issues: {:?}", d.report.issues);
    assert_eq!(d.report.raw_total(), 0);
    (c, d.program)
}

fn body(p: &Program) -> &[Stmt] {
    &p.blocks[0].body
}

fn ops(e: &Expr) -> Vec<String> {
    e.0.iter()
        .map(|t| match t {
            Token::Num { text, .. } => text.clone(),
            Token::Op { op } => op.symbol().to_string(),
            Token::Var { .. } => "var".into(),
            Token::Ref { .. } => "ref".into(),
            Token::Call { call } => format!("call{:04X}", call.opcode),
            Token::X { .. } => "x".into(),
            Token::Raw { .. } => "raw".into(),
        })
        .collect()
}

#[test]
fn a_small_script_round_trips_through_the_decoder() {
    let (c, p) = build(
        "scn TestScript\nshort a\nref r\n\nbegin GameMode\n  set a to 1 + 2 * 3\n  if a == 7 && GetStage TestQuest >= 10\n    player.AddItem Caps001 5\n  elseif a\n    r.Activate player 1\n  else\n    set TestQuest.bDone to 1\n  endif\nend\n",
    );
    assert!(p.script_name);
    assert_eq!(p.blocks.len(), 1);
    assert_eq!(p.blocks[0].params, None, "GameMode has no parameter block");
    let Stmt::Set { expr, .. } = &body(&p)[0] else { panic!("set first") };
    assert_eq!(ops(expr), ["1", "2", "3", "*", "+"]);
    let Stmt::If { arms, otherwise } = &body(&p)[1] else { panic!("if second") };
    assert_eq!(arms.len(), 2);
    assert!(otherwise.is_some());
    assert_eq!(c.variables.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["a", "r"]);
    // The quest (GetStage), the player, the caps, the local `r`, in order of first use.
    assert_eq!(c.refs, [RefSlot::Form(QUEST), RefSlot::Form(PLAYER), RefSlot::Form(CAPS), RefSlot::Local(2)]);
}

#[test]
fn or_binds_tighter_than_and_and_minus_is_unary() {
    let (_, p) = build("short a\nshort b\nshort c\nbegin GameMode\nif a && b || c\nset a to -1\nendif\nend");
    let Stmt::If { arms, .. } = &body(&p)[0] else { panic!() };
    assert_eq!(ops(&arms[0].cond), ["var", "var", "var", "||", "&&"]);
    let Stmt::Set { expr, .. } = &arms[0].body[0] else { panic!() };
    assert_eq!(ops(expr), ["1", "~"]);
    assert!(matches!(expr.0[1], Token::Op { op: Op::Neg }));
}

#[test]
fn parameter_types_decide_how_values_are_stored() {
    let (c, p) = build(
        "ref r\nfloat f\nbegin GameMode\nSetQuestDelay TestQuest 3\nset f to GetGlobalValue TestGlobal\nset f to TestGlobal\nplayer.GetDistance r\nend",
    );
    let b = body(&p);
    // A number in a float parameter is a float, even written without a point.
    let Stmt::Call { call } = &b[0] else { panic!() };
    assert_eq!(call.args[1], Arg::Float(3.0));
    // A global in a form parameter is the form; in an expression it is the value.
    let Stmt::Set { expr, .. } = &b[1] else { panic!() };
    let Token::Call { call } = &expr.0[0] else { panic!() };
    assert!(matches!(call.args[0], Arg::Ref(_)));
    let Stmt::Set { expr, .. } = &b[2] else { panic!() };
    assert!(matches!(expr.0[0], Token::Var { .. }));
    // A `ref` local as a parameter is its reference list entry.
    let Stmt::Call { call } = &b[3] else { panic!() };
    let Arg::Ref(r) = call.args[0] else { panic!("ref local passed as a reference: {:?}", call.args) };
    assert_eq!(c.refs[r.0 as usize - 1], RefSlot::Local(1));
}

#[test]
fn player_spellings_get_their_own_entries() {
    let (c, _) = build("begin GameMode\nplayer.Disable\nPlayerREF.Enable\nplayer.Enable\nend");
    assert_eq!(c.refs, [RefSlot::Form(PLAYER), RefSlot::Form(PLAYER)]);
}

#[test]
fn surplus_arguments_are_dropped() {
    let (_, p) = build("begin GameMode\nStopCombat player\nend");
    let Stmt::Call { call } = &body(&p)[0] else { panic!() };
    assert!(!call.has_params, "StopCombat takes no parameters: no parameter block");
}

#[test]
fn a_seed_keeps_the_units_indices() {
    let t = table();
    let vars = vec![VarDecl { index: 7, name: "a".into(), ty: VarType::Int }];
    let refs = vec![RefSlot::Form(CAPS)];
    let c = compile("short a\nbegin GameMode\nset a to 1\nplayer.AddItem Caps001 a\nend", &t, &World, Some(Seed { variables: &vars, refs: &refs }));
    assert_eq!(c.variables, vars);
    assert_eq!(c.refs, [RefSlot::Form(CAPS), RefSlot::Form(PLAYER)]);
    assert!(c.diags.iter().any(|d| d.kind == "reference_added"));
    let p = decode(&c.bytes, &t).program;
    let Stmt::Call { call } = &body(&p)[1] else { panic!() };
    assert_eq!(call.on.map(|r| r.0), Some(2));
    assert_eq!(call.args[0], Arg::Ref(fnv_script::RefIndex(1)));
}

#[test]
fn unknown_names_are_errors() {
    let t = table();
    let c = compile("begin GameMode\nNoSuchFunction 1\nset x to 2\nend", &t, &World, None);
    let kinds: Vec<&str> = c.errors().map(|d| d.kind).collect();
    assert_eq!(kinds, ["unknown_function", "bad_set_target"]);
    assert_eq!(c.errors().next().map(|d| d.line), Some(2));
}

#[test]
fn begin_blocks_with_parameters() {
    let (c, p) = build("begin OnTrigger player\nend\nbegin OnLoad\nend");
    assert_eq!(p.blocks[0].params.as_ref().map(Vec::len), Some(1));
    assert_eq!(p.blocks[1].params, None);
    assert_eq!(c.refs, [RefSlot::Form(PLAYER)]);
}

// ---- the extender's expressions (task A7) --------------------------------------

use fnv_script::ir::{FuncId, VarSlot, XAssign, XExpr, XOp};
use fnv_script::vm::{ArgValue, Host, RunOpts, Value, Vm};

/// The tree as text, every operation in parentheses.
fn x(e: &XExpr) -> String {
    match e {
        XExpr::Num { text, .. } => text.clone(),
        XExpr::Str { s } => format!("{s:?}"),
        XExpr::Var { var } => match var.var {
            VarSlot::Local { index, .. } => format!("v{index}"),
            VarSlot::Global { global } => format!("g{}", global.0),
        },
        XExpr::Ref { r } => format!("r{}", r.0),
        XExpr::Enum { code } => format!("#{code}"),
        XExpr::Axis { c } => c.to_string(),
        XExpr::Call { call } => {
            let f = match &call.func {
                FuncId::Opcode(o) => format!("{o:04X}"),
                FuncId::Name(n) => n.clone(),
            };
            let on = call.on.as_ref().map(|o| format!("{}.", x(o))).unwrap_or_default();
            format!("{on}{f}({})", call.args.iter().map(x).collect::<Vec<_>>().join(" "))
        }
        XExpr::Unary { op, e } => format!("({op:?} {})", x(e)),
        XExpr::Binary { op, l, r } => format!("({} {op:?} {})", x(l), x(r)),
        XExpr::Member { l, key } => format!("{}->{key}", x(l)),
        XExpr::Assign { op, target, value } => format!("({} {op:?}= {})", x(target), x(value)),
        XExpr::Lambda { params, body, value } => format!(
            "lambda[{}]({})",
            params.len(),
            value.as_ref().map(|v| x(v)).unwrap_or_else(|| format!("{} statements", body.len()))
        ),
    }
}

fn lets(src: &str) -> Vec<String> {
    let (_, p) = build(src);
    let mut out = Vec::new();
    p.walk(&mut |s| match s {
        Stmt::Let { expr } => out.push(x(expr)),
        Stmt::If { arms, .. } => {
            for a in arms {
                if let [Token::X { e }] = &a.cond.0[..] {
                    out.push(format!("if {}", x(e)));
                }
            }
        }
        _ => {}
    });
    out
}

#[test]
fn extender_precedence_follows_the_operator_table() {
    let l = lets("int a\nint b\nbegin GameMode\nlet a := 1 + 2 * 3 ^ 2\nlet a := b := 2\na += 1\nlet a := 1 || 0 && 2\nlet a := !-b\nend");
    assert_eq!(l[0], "(v1 Set= (1 Add (2 Mul (3 Pow 2))))");
    assert_eq!(l[1], "(v1 Set= (v2 Set= 2))", ":= groups right to left");
    assert_eq!(l[2], "(v1 Add= 1)", "the Let macro");
    assert_eq!(l[3], "(v1 Set= (1 Or (0 And 2)))", "&& binds tighter than ||");
    assert_eq!(l[4], "(v1 Set= (Not (Neg v2)))");
}

#[test]
fn a_functions_argument_is_an_operand() {
    // [documented: Inline Expression] `Player.GetActorValue Health + 50` is
    // (GetAV Health) + 50.
    let l = lets("int a\nbegin GameMode\nif eval GetStage TestQuest >= 10\nendif\nlet a := player.GetAV Luck\nlet a := GetRandomPercent * 2\nlet a := Abs -3 + 1\nend");
    assert_eq!(l[0], "if (103A(r1) Ge 10)");
    assert_eq!(l[3], "(v1 Set= (1221((Neg 3)) Add 1))", "a minus starts the first argument");
    assert!(l[1].starts_with("(v1 Set= r2.100E(#"), "an actor value name is its code: {}", l[1]);
    assert_eq!(l[2], "(v1 Set= (104D() Mul 2))", "a function without parameters takes no operand");
}

#[test]
fn extender_functions_are_named() {
    let t = table();
    let c = compile("int a\nbegin GameMode\nlet a := Ar_Size b\nend", &t, &World, None);
    assert_eq!(c.errors().map(|d| d.kind).collect::<Vec<_>>(), ["unresolved_name"], "b is not declared");
    let c = compile("int a\nint b\nbegin GameMode\nlet a := Ar_Size b\nend", &t, &World, None);
    assert!(c.errors().next().is_none(), "{:?}", c.diags);
    let p = decode(&c.bytes, &t).program;
    let Stmt::Let { expr: XExpr::Assign { op: XAssign::Set, value, .. } } = &body(&p)[0] else { panic!() };
    assert!(matches!(value.as_ref(), XExpr::Call { call } if call.func == FuncId::Name("Ar_Size".into())));
}

#[test]
fn bad_extended_lines_are_errors() {
    let t = table();
    for (src, kind) in [
        ("begin GameMode\nlet 5 := 1\nend", "bad_assignment_target"),
        ("int a\nbegin GameMode\nlet a + 1\nend", "let_without_assignment"),
        ("int a\nbegin GameMode\nlet a := (1 + 2\nend", "syntax"),
        ("int a\nbegin GameMode\nlet a := 1 2\nend", "syntax"),
    ] {
        let c = compile(src, &t, &World, None);
        assert_eq!(c.errors().map(|d| d.kind).collect::<Vec<_>>(), [kind], "{src}");
    }
}

#[derive(Default)]
struct Locals(
    std::collections::BTreeMap<u16, Value>,
    fnv_script::vm::ExtStore,
    std::collections::BTreeSet<u16>,
    std::collections::BTreeSet<u16>,
    Vec<String>,
);

impl Host for Locals {
    fn var_kind(&mut self, var: &fnv_script::VarRef) -> fnv_script::vm::VarKind {
        match var.var {
            VarSlot::Local { index, .. } if self.2.contains(&index) => fnv_script::vm::VarKind::String,
            VarSlot::Local { index, .. } if self.3.contains(&index) => fnv_script::vm::VarKind::Array,
            _ => fnv_script::vm::VarKind::Value,
        }
    }
    fn ext(&mut self) -> Option<&mut fnv_script::vm::ExtStore> {
        Some(&mut self.1)
    }
    fn print(&mut self, text: &str) {
        self.4.push(text.to_string());
    }
    fn get_var(&mut self, var: &fnv_script::VarRef) -> Result<Value, String> {
        let VarSlot::Local { index, .. } = var.var else { return Err("global".into()) };
        Ok(self.0.get(&index).copied().unwrap_or_default())
    }
    fn set_var(&mut self, var: &fnv_script::VarRef, value: Value) -> Result<(), String> {
        let VarSlot::Local { index, .. } = var.var else { return Err("global".into()) };
        self.0.insert(index, value);
        Ok(())
    }
    fn ref_value(&mut self, _r: fnv_script::RefIndex) -> Result<Value, String> {
        Ok(Value::Ref(PLAYER))
    }
    fn call(&mut self, _opcode: u16, _on: Option<Value>, _args: &[ArgValue]) -> Result<Value, String> {
        Ok(Value::Num(4.0))
    }
}

/// A host whose string variables are the compiled unit's `string_var`s.
fn host_for(c: &Compiled) -> Locals {
    let of = |t: &str| c.variables.iter().filter(|v| v.ty == VarType::Other(t.into())).map(|v| v.index as u16).collect();
    Locals(Default::default(), Default::default(), of("string_var"), of("array_var"), Vec::new())
}

fn run(src: &str) -> Locals {
    let t = table();
    let (c, p) = build(src);
    let mut h = host_for(&c);
    Vm::new(&t, &mut h, RunOpts { strict: true, ..RunOpts::default() }).run(&p.blocks[0].body).expect("runs");
    h
}

#[test]
fn let_runs_on_numbers() {
    let h = run("float a\nfloat b\nfloat c\nbegin GameMode\nlet a := 2 + 3 * 4\nlet b := c := 7\nb += 1\nlet c := 0 || 5\nlet a := (1 << 3) | 1\nend");
    assert_eq!(h.0[&1], Value::Num(9.0));
    assert_eq!(h.0[&2], Value::Num(8.0));
    // `||` returns the deciding operand's value [documented: NVSE Expressions].
    assert_eq!(h.0[&3], Value::Num(5.0));
}

#[test]
fn if_eval_tests_an_extended_condition() {
    let h = run("float a\nbegin GameMode\nif eval 2 ^ 3 == 8 && GetRandomPercent\nlet a := 1\nelseif eval 1\nlet a := 2\nendif\nend");
    assert_eq!(h.0[&1], Value::Num(1.0));
}

#[test]
fn not_built_yet_stops_the_statement() {
    let t = table();
    // foreach over a container is B5.
    let (_, p) = build("ref r\nbegin GameMode\nforeach r <- player\nloop\nend");
    let mut h = Locals::default();
    let r = Vm::new(&t, &mut h, RunOpts { strict: true, ..RunOpts::default() }).run(&p.blocks[0].body);
    assert!(matches!(r, Err(fnv_script::vm::VmError::NotBuilt(_))), "{r:?}");
    let _ = XOp::Add;
}

// ---- loops (task A8) ------------------------------------------------------------

#[test]
fn while_loops_run_with_break_and_continue() {
    // Sum 1..=10 skipping 5, stopping after 8: 1+2+3+4+6+7+8 = 31.
    let h = run(
        "float i\nfloat s\nbegin GameMode\nwhile i < 10\n  i += 1\n  if eval i == 5\n    continue\n  endif\n  if eval i > 8\n    break\n  endif\n  s += i\nloop\nend",
    );
    assert_eq!(h.0[&2], Value::Num(31.0));
    assert_eq!(h.0[&1], Value::Num(9.0));
}

#[test]
fn nested_loops_and_their_structure() {
    let (_, p) = build("float i\nfloat j\nfloat n\nbegin GameMode\nwhile i < 3\n  i += 1\n  let j := 0\n  while j < 2\n    j += 1\n    n += 1\n  loop\nloop\nend");
    let Stmt::While { body, .. } = &body(&p)[0] else { panic!("outer while") };
    assert!(matches!(body.last(), Some(Stmt::While { .. })), "the inner loop is in the outer body");
    let h = run("float i\nfloat j\nfloat n\nbegin GameMode\nwhile i < 3\n  i += 1\n  let j := 0\n  while j < 2\n    j += 1\n    n += 1\n  loop\nloop\nend");
    assert_eq!(h.0[&3], Value::Num(6.0));
}

#[test]
fn a_runaway_loop_stops_at_the_step_budget() {
    let t = table();
    let (_, p) = build("float i\nbegin GameMode\nwhile 1\n  i += 1\nloop\nend");
    let mut h = Locals::default();
    let r = Vm::new(&t, &mut h, RunOpts { strict: true, max_steps: 1000 }).run(&p.blocks[0].body);
    assert!(matches!(r, Err(fnv_script::vm::VmError::Budget(1000))), "{r:?}");
}

#[test]
fn foreach_compiles_and_waits_for_arrays() {
    let (_, p) = build("float e\nfloat a\nbegin GameMode\nforeach e <- a\n  break\nloop\nend");
    assert!(matches!(&body(&p)[0], Stmt::ForEach { body, .. } if body == &[Stmt::Break]));
}

#[test]
fn loop_mistakes_are_errors() {
    let t = table();
    for (src, kind) in [
        ("begin GameMode\nloop\nend", "loop_without_while"),
        ("begin GameMode\nbreak\nend", "outside_a_loop"),
        ("float i\nbegin GameMode\nwhile i\nend", "while_without_loop"),
        ("float e\nbegin GameMode\nforeach e a\nloop\nend", "syntax"),
    ] {
        let c = compile(src, &t, &World, None);
        assert_eq!(c.errors().map(|d| d.kind).collect::<Vec<_>>(), [kind], "{src}");
    }
}

// ---- inline expressions, new variable types (tasks A9, A10) -----------------------

#[test]
fn inline_expressions_as_arguments_in_an_extended_unit() {
    // [documented: Inline Expression] `Player.SetActorValue Health (Player.GetActorValue
    // Health + 50)`; the unit is extended (it has a let), so the call line is one too.
    let (_, p) = build("float f\nbegin GameMode\nlet f := 1\nplayer.SetActorValue Health (player.GetActorValue Health + 50)\nend");
    let Stmt::Eval { expr } = &body(&p)[1] else { panic!("an expression statement: {:?}", body(&p)[1]) };
    let s = x(expr);
    assert!(s.starts_with("r1.100F(#"), "{s}");
    assert!(s.contains("(r1.100E(#") && s.ends_with("Add 50))"), "{s}");
}

#[test]
fn a_vanilla_unit_keeps_its_parentheses_dropped() {
    // Without any extender syntax, parentheses among arguments are dropped as the editor
    // drops them (scripts.md §3.4): a plain call statement.
    let (_, p) = build("begin GameMode\nplayer.AddItem (Caps001) 5\nend");
    assert!(matches!(&body(&p)[0], Stmt::Call { .. }));
}

#[test]
fn extender_functions_as_statements_make_the_unit_extended() {
    let t = table();
    let c = compile("array_var a\nbegin GameMode\nAr_Append a 5\nend", &t, &World, None);
    assert!(c.errors().next().is_none(), "{:?}", c.diags);
    let p = decode(&c.bytes, &t).program;
    assert!(matches!(&body(&p)[0], Stmt::Eval { expr: XExpr::Call { call } } if call.func == FuncId::Name("Ar_Append".into())));
    assert_eq!(c.variables[0].ty, VarType::Other("array_var".into()));
}

#[test]
fn inline_declarations_hex_and_binary_and_block_comments() {
    let h = run("begin GameMode\nint i = 0x10 + 0b101\n/* a comment\nint j = 99\n*/\nfloat f = i * 2 ; 42\nend");
    assert_eq!(h.0[&1], Value::Num(21.0));
    assert_eq!(h.0[&2], Value::Num(42.0), "j was inside the comment, so f is variable 2");
}

#[test]
fn calls_on_results_chain() {
    let l = lets("ref r\nfloat f\nbegin GameMode\nlet f := (GetSelf).GetAV Luck\nend");
    assert!(l[0].starts_with("(v2 Set= "), "{}", l[0]);
    assert!(l[0].contains(".100E(#"), "a call on the result of GetSelf: {}", l[0]);
}

#[test]
fn compiler_override_blocks_read_every_line_with_the_extender_grammar() {
    // [documented: Script Compiler Override] under `Begin _GameMode`, `if Sin 14 == 1`
    // evaluates 14 == 1 first; `set` takes extended expressions too.
    let (_, p) = build("float f\nbegin _GameMode\nif Sin 14 == 1\nset f to f + 1\nendif\nend");
    assert_eq!(p.blocks[0].event, 0, "the block is GameMode");
    let Stmt::If { arms, .. } = &body(&p)[0] else { panic!() };
    let [Token::X { e }] = &arms[0].cond.0[..] else { panic!("an extended condition: {:?}", arms[0].cond) };
    assert_eq!(x(e), "Sin((14 Eq 1))", "Sin is the extender's (no opcode)");
    assert!(matches!(&arms[0].body[0], Stmt::Let { .. }));
    // The same line outside an override block: (Sin 14) == 1.
    let l = lets("float f\nbegin GameMode\nif eval Sin 14 == 1\nendif\nend");
    assert_eq!(l[0], "if (Sin(14) Eq 1)");
}

// ---- user-defined functions and lambdas (task A11) --------------------------------

#[test]
fn a_function_script_has_a_function_block_with_its_parameters() {
    let (c, p) = build("scn TestUDF
int a
int b
Begin Function {a, b}
  SetFunctionValue a + b
End
");
    assert_eq!(p.blocks[0].event, fnv_script::ir::FUNCTION_BLOCK);
    assert_eq!(p.blocks[0].params.as_ref().map(Vec::len), Some(2));
    assert!(matches!(&p.blocks[0].body[0], Stmt::Eval { .. }), "SetFunctionValue is the extender's");
    assert_eq!(c.variables.len(), 2);
}

#[test]
fn call_takes_the_function_and_up_to_fifteen_arguments() {
    let l = lets("int r
ref f
begin GameMode
let r := Call f 1 2 3
let r := Call f, 1, (2 + 3)
end");
    // A `ref` local holding the function is the variable [documented: Lambda, `Call
    // rCallback 10`].
    assert_eq!(l[0], "(v1 Set= Call(v2 1 2 3))");
    assert_eq!(l[1], "(v1 Set= Call(v2 1 (2 Add 3)))");
}

#[test]
fn lambdas_in_both_forms() {
    let src = "int r
int x
ref f
begin GameMode
  let f := (begin function {x}
    SetFunctionValue x * 2
    let r := 1
  end)
  let f := ({x} => x + 1)
end";
    let l = lets(src);
    assert_eq!(l[0], "(v3 Set= lambda[1](2 statements))");
    assert_eq!(l[1], "(v3 Set= lambda[1]((v2 Add 1)))");
}

// ---- strings (task B1) ---------------------------------------------------------------

impl Locals {
    fn text(&self, index: u16) -> String {
        let id = self.0.get(&index).copied().unwrap_or_default().num() as u32;
        self.1.text(id).unwrap_or_default().to_string()
    }
}

#[test]
fn strings_join_copy_and_convert() {
    let h = run("string_var a\nstring_var b\nfloat n\nbegin GameMode\nlet a := \"Hello\"\nlet b := a\nlet a += \", world\"\nlet n := #\"0x10\" + #\" 2.5 \"\nlet b := b + $n + $3 + $1.25\nend");
    assert_eq!(h.text(1), "Hello, world");
    // Assignment copies: b kept "Hello" when a changed [documented: NVSE Expressions].
    assert_eq!(h.text(2), "Hello18.531.25");
    assert_eq!(h.0[&3], Value::Num(18.5));
}

#[test]
fn strings_compare_without_case_and_slice() {
    let h = run("string_var s\nstring_var t\nfloat a\nfloat b\nbegin GameMode\nlet s := \"Abcdef\"\nlet t := s[1:3] + s[-1] + s[0]\nif eval s == \"ABCDEF\" && \"apple\" < \"Banana\"\n  let a := 1\nendif\nlet b := s != \"x\"\nend");
    assert_eq!(h.text(2), "bcdfA");
    assert_eq!(h.0[&3], Value::Num(1.0));
    assert_eq!(h.0[&4], Value::Num(1.0));
}

#[test]
fn a_type_error_is_reported_and_the_script_goes_on() {
    // [documented: Let] a failing let reports and the script carries on.
    let t = table();
    let (c, p) = build("string_var s\nfloat a\nbegin GameMode\nlet a := \"x\" + 1\nlet a := 1 / 0\nlet a := 7\nend");
    let mut h = host_for(&c);
    Vm::new(&t, &mut h, RunOpts::default()).run(&p.blocks[0].body).expect("non-strict runs go on");
    assert_eq!(h.0[&2], Value::Num(7.0));
    let r = Vm::new(&t, &mut h, RunOpts { strict: true, ..RunOpts::default() }).run(&p.blocks[0].body);
    assert!(matches!(r, Err(fnv_script::vm::VmError::Type(_))), "{r:?}");
}

#[test]
fn tostring_and_tonumber() {
    let h = run("string_var s\nfloat n\nbegin GameMode\nlet s := ToString 42\nlet n := (ToNumber \"12\") * 2\nend");
    // ToNumber is NVSE-aware, so without the parentheses its argument would be "12" * 2.
    assert_eq!(h.text(1), "42");
    assert_eq!(h.0[&2], Value::Num(24.0));
}

// ---- arrays (task B3, the D2 functions) -------------------------------------------------

impl Locals {
    fn array_id(&self, index: u16) -> u32 {
        self.0.get(&index).copied().unwrap_or_default().num() as u32
    }
}

#[test]
fn arrays_share_on_assignment_and_index() {
    let h = run("array_var a\narray_var b\nfloat n\nfloat m\nbegin GameMode\nlet a := Ar_List 10, 20, 30\nlet b := a\nlet b[1] := 99\nlet n := a[1] + Ar_Size a\nlet a[3] := 5\nlet m := a[-0] + a[3]\nend");
    assert_eq!(h.array_id(1), h.array_id(2), "assignment shares [documented: NVSE Expressions]");
    assert_eq!(h.0[&3], Value::Num(102.0));
    assert_eq!(h.0[&4], Value::Num(15.0), "setting the key equal to the size appends");
}

#[test]
fn maps_keep_their_keys_sorted() {
    let h = run("array_var m\narray_var k\nstring_var s\nfloat f\nbegin GameMode\nlet m := Ar_Map \"b\"::2, \"A\"::1, \"c\"::3\nlet k := Ar_Keys m\nlet s := k[0] + k[1] + k[2]\nlet f := m->b + m[\"c\"]\nend");
    let text = { let id = h.0[&3].num() as u32; h.1.text(id).unwrap_or_default().to_string() };
    assert_eq!(text, "Abc");
    assert_eq!(h.0[&4], Value::Num(5.0));
}

#[test]
fn foreach_walks_an_array_and_a_string() {
    let h = run("array_var a\narray_var e\nfloat sum\nstring_var t\nstring_var c\nbegin GameMode\nlet a := Ar_Range 1, 5\nforeach e <- a\n  if eval *e == 4\n    break\n  endif\n  let sum += e[\"value\"] * 10 + e[\"key\"]\nloop\nforeach c <- \"abc\"\n  let t := c + t\nloop\nend");
    // 1..3 counted: values 10+20+30, keys 0+1+2.
    assert_eq!(h.0[&3], Value::Num(63.0));
    let id = h.0[&4].num() as u32;
    assert_eq!(h.1.text(id), Some("cba"));
}

#[test]
fn array_functions() {
    let h = run(
        "array_var a\narray_var b\nfloat f\nfloat g\nfloat k\nstring_var t\nbegin GameMode\nlet a := Ar_List 3, 1, 2\nlet b := Ar_Sort a\nlet f := b[0] * 100 + b[2]\nAr_Append a, 7\nAr_Insert a, 0, 9\nlet g := Ar_Erase a, 1:2\nlet k := Ar_Find 7, a\nlet t := TypeOf (Ar_Construct \"stringmap\")\nAr_Dump a\nend",
    );
    assert_eq!(h.0[&3], Value::Num(103.0));
    assert_eq!(h.0[&4], Value::Num(2.0), "two erased");
    assert_eq!(h.0[&5], Value::Num(2.0), "[9, 2, 7]: 7 is at key 2");
    let id = h.0[&6].num() as u32;
    assert_eq!(h.1.text(id), Some("StringMap"));
    assert_eq!(h.4, ["array #1 [0: 9, 1: 2, 2: 7]"]);
}

#[test]
fn temporary_arrays_are_freed() {
    let h = run("array_var a\nfloat n\nbegin GameMode\nlet n := Ar_Size (Ar_List 1, 2, 3)\nlet a := Ar_List (Ar_List 1), 2\nlet a := Ar_Null\nend");
    assert_eq!(h.0[&2], Value::Num(3.0));
    assert!(h.1.arrays.is_empty(), "nothing holds any array at the end: {:?}", h.1.arrays.keys().collect::<Vec<_>>());
}
