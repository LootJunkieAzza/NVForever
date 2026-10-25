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
