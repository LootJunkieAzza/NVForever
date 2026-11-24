//! Compiler regressions for unnumbered extender `kind: "actor_value"` parameter typing
//! (`GetThresholdAV` / `GetThresholdedActorValue`, `GetAVMod`, `GetAVDamage`, etc.).

use fnv_script::compile::{compile, Compiled, Env, FormInfo};
use fnv_script::ir::XExpr;
use fnv_script::{decode, FunctionTable, Stmt, VarDecl, VarType};
use std::path::Path;

const QUEST: u32 = 0x0100_1000;
const GLOBAL: u32 = 0x0100_2000;
const NIGHT_EYE_MGEF: u32 = 0x0100_5000;

struct TestEnv;

impl Env for TestEnv {
    fn form(&self, editor_id: &str) -> Option<FormInfo> {
        let (form, sig) = match editor_id.to_ascii_lowercase().as_str() {
            "testquest" => (QUEST, *b"QUST"),
            "testglobal" => (GLOBAL, *b"GLOB"),
            "nighteye" => (NIGHT_EYE_MGEF, *b"MGEF"),
            _ => return None,
        };
        Some(FormInfo { form, sig })
    }
    fn info(&self, form: u32) -> Option<FormInfo> {
        [("testquest", QUEST), ("testglobal", GLOBAL), ("nighteye", NIGHT_EYE_MGEF)]
            .iter()
            .find(|(_, f)| *f == form)
            .and_then(|(n, _)| self.form(n))
    }
    fn editor_id(&self, form: u32) -> Option<String> {
        [("TestQuest", QUEST), ("TestGlobal", GLOBAL), ("NightEye", NIGHT_EYE_MGEF)]
            .iter()
            .find(|(_, f)| *f == form)
            .map(|(n, _)| n.to_string())
    }
    fn local_form(&self, local: u32) -> Option<u32> {
        Some(local)
    }
    fn script_vars(&self, form: u32) -> Option<Vec<VarDecl>> {
        (form == QUEST).then(|| vec![VarDecl { index: 1, name: "iStat".into(), ty: VarType::Int }])
    }
}

fn table() -> FunctionTable {
    FunctionTable::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json")).expect("functions.json loads")
}

fn diag_kinds(c: &Compiled) -> Vec<&'static str> {
    c.diags.iter().map(|d| d.kind).collect()
}

fn decoded_body(c: &Compiled, t: &FunctionTable) -> Vec<Stmt> {
    let p = decode(&c.bytes, t);
    assert!(p.report.issues.is_empty(), "decode issues: {:?}", p.report.issues);
    p.program.blocks[0].body.clone()
}

#[test]
fn unnumbered_actor_value_params_resolve_bare_keywords_without_unquoted_warning() {
    let t = table();
    let src = "int iStat\nstring_var sStat\nfloat fOut\nBegin GameMode\n\
               let fOut := PlayerREF.GetThresholdAV Perception\n\
               let fOut := PlayerREF.GetThresholdedActorValue Health\n\
               let fOut := PlayerREF.GetThresholdAV IgnoreCrippledLimbs\n\
               let fOut := PlayerREF.GetThresholdAV CritChance\n\
               let fOut := PlayerREF.GetThresholdAV NightEye\n\
               let fOut := PlayerREF.GetThresholdAV \"Perception\"\n\
               let fOut := PlayerREF.GetThresholdAV 6\n\
               let fOut := PlayerREF.GetThresholdAV iStat\n\
               let fOut := PlayerREF.GetThresholdAV sStat\n\
               let fOut := PlayerREF.GetThresholdAV TestGlobal\n\
               let fOut := PlayerREF.GetThresholdAV TestQuest.iStat\n\
               let fOut := PlayerREF.GetThresholdAV (StringToActorValue \"Perception\")\n\
               let fOut := PlayerREF.GetThresholdAV GetRandomPercent\n\
               let fOut := PlayerREF.GetAVMod Strength\n\
               let fOut := PlayerREF.GetAVDamage Health\n\
               End\n";
    let c = compile(src, &t, &TestEnv, None);
    assert_eq!(c.errors().count(), 0, "errors: {:?}", c.diags);
    assert!(diag_kinds(&c).is_empty(), "expected zero warnings, got {:?}", c.diags);

    let body = decoded_body(&c, &t);
    let first_arg = |i: usize| -> &XExpr {
        match &body[i] {
            Stmt::Let { expr: XExpr::Assign { value, .. } } => match value.as_ref() {
                XExpr::Call { call } => &call.args[0],
                other => panic!("stmt {i}: expected call, got {other:?}"),
            },
            other => panic!("stmt {i}: expected let, got {other:?}"),
        }
    };
    assert_eq!(first_arg(0), &XExpr::Enum { code: 6 }, "GetThresholdAV Perception");
    assert_eq!(first_arg(1), &XExpr::Enum { code: 16 }, "GetThresholdedActorValue Health");
    assert_eq!(first_arg(2), &XExpr::Enum { code: 72 }, "IgnoreCrippledLimbs");
    assert_eq!(first_arg(3), &XExpr::Enum { code: 14 }, "CritChance");
    assert_eq!(first_arg(4), &XExpr::Enum { code: 50 }, "NightEye AV wins over non-GLOB MGEF");
    assert_eq!(first_arg(5), &XExpr::Str { s: "Perception".into() }, "quoted string preserved");
    assert!(matches!(first_arg(6), XExpr::Num { value, .. } if *value == 6.0), "numeric code preserved");
    assert!(matches!(first_arg(7), XExpr::Var { .. }), "local int var preserved");
    assert!(matches!(first_arg(8), XExpr::Var { .. }), "local string_var preserved");
    assert!(matches!(first_arg(9), XExpr::Var { .. }), "global var preserved");
    assert!(matches!(first_arg(10), XExpr::Var { .. }), "dotted quest var preserved");
    assert!(matches!(first_arg(11), XExpr::Call { .. }), "parenthesized call expression preserved");
    assert!(matches!(first_arg(12), XExpr::Call { .. }), "bare zero-arg function call preserved");
    assert_eq!(first_arg(13), &XExpr::Enum { code: 5 }, "GetAVMod Strength");
    assert_eq!(first_arg(14), &XExpr::Enum { code: 16 }, "GetAVDamage Health");

    // A local variable with the same name as an actor value still shadows the keyword.
    let shadowed = compile("int Perception\nfloat f\nBegin GameMode\nlet f := PlayerREF.GetThresholdAV Perception\nEnd\n", &t, &TestEnv, None);
    assert_eq!(shadowed.errors().count(), 0, "{:?}", shadowed.diags);
    assert!(shadowed.diags.is_empty(), "{:?}", shadowed.diags);
    let sbody = decoded_body(&shadowed, &t);
    match &sbody[0] {
        Stmt::Let { expr: XExpr::Assign { value, .. } } => match value.as_ref() {
            XExpr::Call { call } => assert!(matches!(call.args[0], XExpr::Var { .. }), "local var must win: {:?}", call.args[0]),
            other => panic!("expected call, got {other:?}"),
        },
        other => panic!("expected let, got {other:?}"),
    }

    // An unknown bare word still falls back to the existing `unquoted_argument` warning.
    let unknown = compile("float f\nBegin GameMode\nlet f := PlayerREF.GetThresholdAV NotAnActorValue\nEnd\n", &t, &TestEnv, None);
    assert_eq!(unknown.errors().count(), 0, "{:?}", unknown.diags);
    assert!(unknown.diags.iter().any(|d| d.kind == "unquoted_argument" && !d.error), "{:?}", unknown.diags);
}
