//! Authored receiver spellings at the ordinary Auto source compilation boundary.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{source_compile::Mode, GameData};
use fnv_script::compile::{compile, compile_mod_runtime, env::LoadOrderIndex, Env};
use fnv_script::ir::{Arg, Call, Stmt, Token};
use fnv_script::unit::{read_load_order, RefSlot};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, RefTable};

const FIVE: u32 = 0xC1234;
const SIX: u32 = 0xC12345;
const TOPIC: u32 = 0xD11111;
const TARGET: u32 = 0xF0100;
const SHADOW: u32 = 0xE0001;
const SCRIPT: u32 = 0x0100_0A00;
// An owned, unregistered opcode only selects Auto. It is not native encoding evidence.
const OPAQUE: u16 = 0x7FFE;

fn schema() -> Schema {
    Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap()
}

fn catalogue() -> (FunctionTable, serde_json::Value) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    (FunctionTable::load(&path).unwrap(), serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap())
}

fn header(masters: &[&str]) -> Vec<u8> {
    let mut fields = vec![sub(b"HEDR", [1.34f32.to_le_bytes(), 0u32.to_le_bytes(), 0x800u32.to_le_bytes()].concat())];
    for master in masters {
        fields.extend([sub(b"MAST", z(master)), sub(b"DATA", vec![0; 8])]);
    }
    record(b"TES4", 1, 0, &fields)
}

fn forms(hex_edid: bool) -> Vec<u8> {
    let refs: Vec<_> = [(FIVE, "FiveRef"), (SIX, "SixRef"), (0xC123, "FourRef"), (0x12345, "12345"), (TARGET, "TargetActorRef"), (SHADOW, if hex_edid { "C1234" } else { "ShadowRef" })]
        .into_iter()
        .map(|(form, name)| record(b"ACHR", 0x400, form, &[sub(b"EDID", z(name)), sub(b"NAME", 0xF0000u32.to_le_bytes().to_vec())]))
        .collect();
    [
        top(b"NPC_", &record(b"NPC_", 0, 0xF0000, &[sub(b"EDID", z("OwnedActor"))])),
        top(b"ACHR", &refs.concat()),
        top(b"DIAL", &record(b"DIAL", 0, TOPIC, &[sub(b"EDID", z("OwnedTopic")), sub(b"DATA", vec![0, 0])])),
    ].concat()
}

/// Master-backed: local index0 means Base. Otherwise Caller has no masters and its
/// local index0 means Caller, despite Base's same-numbered records loading first.
fn fixture(head: Option<&str>, master_backed: bool, hex_edid: bool, local_ref: bool) -> (Vec<Plugin>, Vec<u8>, u32) {
    let base = [header(&[]), forms(hex_edid)].concat();
    let masters = if master_backed { vec!["Base.esm"] } else { vec![] };
    let original = script(&[(0, None, st(OPAQUE, &[]))]);
    let declaration = if local_ref { "ref C1234\n" } else { "" };
    let on = head.map(|s| format!("{s}.")).unwrap_or_default();
    let source = format!("scn OwnedReceiver\n{declaration}begin GameMode\n{on}StartConversation TargetActorRef D11111\nend\n");
    let mut fields = vec![sub(b"EDID", z("OwnedReceiver"))];
    fields.extend(unit(0, &original, &[], &[FIVE, TARGET, TOPIC, SIX]));
    fields.push(sub(b"SCTX", source.into_bytes()));
    let caller = [
        header(&masters),
        if master_backed { vec![] } else { forms(hex_edid) },
        top(b"SCPT", &record(b"SCPT", 0, ((masters.len() as u32) << 24) | 0xA00, &fields)),
    ].concat();
    let plugins = vec![Plugin::parse(&base, "Base.esm".into()).unwrap(), Plugin::parse(&caller, "Caller.esp".into()).unwrap()];
    (plugins, original, if master_backed { 0 } else { 0x0100_0000 })
}

fn build(plugins: &[Plugin]) -> GameData {
    assert_eq!(fnv_game::source_compile::mode(), Mode::Auto);
    let lo = LoadOrder::new(plugins).unwrap();
    let schema = schema();
    let (table, json) = catalogue();
    assert!(table.get(OPAQUE).is_none(), "the owned bytecode must select Auto");
    let refs = RefTable::build(&lo, &schema);
    GameData::build(&lo, &schema, table, &json, &refs)
}

fn call(data: &GameData) -> (&Call, &fnv_script::ScriptUnit) {
    assert_eq!(data.source_units.len(), 1, "one exact Auto-selected unit");
    assert_eq!(data.source_units[0].key, "Caller.esp|SCPT 0x01000A00");
    assert!(data.source_units[0].error.is_none(), "{:?}", data.source_units[0].error);
    let compiled = data.unit(data.scripts[&FormId(SCRIPT)]);
    assert_eq!(compiled.raw, 0, "the source call is completely decoded");
    assert_eq!(compiled.program.blocks.len(), 1);
    assert_eq!(compiled.program.blocks[0].body.len(), 1);
    let Stmt::Call { call } = &compiled.program.blocks[0].body[0] else { panic!("expected vanilla call") };
    assert_eq!(call.opcode, 0x1056);
    (call, &compiled.unit)
}

fn assert_arguments(call: &Call, unit: &fnv_script::ScriptUnit, namespace: u32) {
    assert_eq!(call.args.len(), 2);
    for (arg, form) in call.args.iter().zip([TARGET, TOPIC]) {
        let Arg::Ref(index) = arg else { panic!("expected form argument: {arg:?}") };
        assert_eq!(unit.slot(*index), Some(RefSlot::Form(namespace | form)));
    }
}

#[test]
fn auto_recompile_maps_bare_five_digit_receivers_in_the_plugins_namespace() {
    for master_backed in [false, true] {
        let (plugins, original, namespace) = fixture(Some("C1234"), master_backed, false, false);
        let data = build(&plugins);
        let (call, unit) = call(&data);
        assert_ne!(unit.bytes, original);
        assert_eq!(call.on.map(|r| r.0), Some(1), "reuse the exact seeded receiver slot");
        assert_eq!(unit.slot(call.on.unwrap()), Some(RefSlot::Form(namespace | FIVE)));
        assert_eq!(unit.refs.len(), 4, "no guessed or duplicate receiver appended");
        assert_arguments(call, unit, namespace);
    }
}

#[test]
fn existing_full_width_quoted_short_and_omitted_receivers_keep_their_admission() {
    for head in [Some("000C1234"), Some("\"C1234\""), Some("\"C12345\""), None] {
        let (plugins, _, namespace) = fixture(head, false, false, false);
        let data = build(&plugins);
        let (call, unit) = call(&data);
        let form = if head == Some("\"C12345\"") { SIX } else { FIVE };
        assert_eq!(call.on.map(|r| unit.slot(r)), head.map(|_| Some(RefSlot::Form(namespace | form))));
        assert_arguments(call, unit, namespace);
    }
}

#[test]
fn editor_ids_and_local_variables_take_precedence_over_short_hex_receivers() {
    for local_ref in [false, true] {
        let (plugins, _, namespace) = fixture(Some("C1234"), true, true, local_ref);
        let data = build(&plugins);
        let (call, unit) = call(&data);
        let expected = if local_ref { RefSlot::Local(1) } else { RefSlot::Form(namespace | SHADOW) };
        assert_eq!(unit.slot(call.on.unwrap()), Some(expected));
        assert_arguments(call, unit, namespace);
    }
}

#[test]
fn invalid_explicit_receivers_keep_original_scda_instead_of_becoming_implicit() {
    for head in ["CFFFF", "CFFFFF", "C00000000", "G1234", "0"] {
        let (plugins, original, _) = fixture(Some(head), true, false, false);
        let data = build(&plugins);
        assert_eq!(data.source_units.len(), 1);
        assert!(data.source_units[0].error.is_some(), "invalid explicit receiver {head}");
        assert_eq!(data.unit(data.scripts[&FormId(SCRIPT)]).unit.bytes, original, "preserve bytecode on source failure: {head}");
    }
}

#[test]
fn fresh_strict_compilation_keeps_its_existing_short_receiver_policy() {
    let (plugins, _, _) = fixture(None, true, false, false);
    let lo = LoadOrder::new(&plugins).unwrap();
    let (units, _) = read_load_order(&lo, &schema());
    let ix = LoadOrderIndex::build(&lo, &units);
    let (table, _) = catalogue();
    for (head, accepted) in [("C1234", false), ("000C1234", true), ("\"C1234\"", true)] {
        let source = format!("scn Strict\nbegin GameMode\n{head}.StartConversation TargetActorRef OwnedTopic\nend\n");
        let c = compile(&source, &table, &ix.env(1), None);
        assert_eq!(c.errors().count() == 0, accepted, "{head}: {:?}", c.diags);
    }
}

#[test]
fn unobserved_bare_widths_keep_their_previous_source_refusal() {
    // These local forms exist. Only the observed five-digit receiver spelling is
    // newly admitted; other widths still need their own permitted evidence.
    for head in ["C123", "C12345", "0C12345"] {
        let (plugins, original, _) = fixture(Some(head), true, false, false);
        let data = build(&plugins);
        assert!(data.source_units[0].error.is_some(), "unobserved bare width: {head}");
        assert_eq!(data.unit(data.scripts[&FormId(SCRIPT)]).unit.bytes, original);
    }
}

#[test]
fn mod_runtime_inline_receiver_and_decimal_scalar_keep_their_distinct_roles() {
    let (plugins, _, _) = fixture(None, true, false, false);
    let lo = LoadOrder::new(&plugins).unwrap();
    let (units, _) = read_load_order(&lo, &schema());
    let ix = LoadOrderIndex::build(&lo, &units);
    let env = ix.env(1);
    let (table, _) = catalogue();
    assert_eq!(env.form("12345").map(|f| f.form), Some(0x12345), "a conflicting form name really exists");

    let scalar = compile_mod_runtime("scn Scalar\nfloat result\nbegin GameMode\nset result to 12345\nend\n", &table, &env);
    assert_eq!(scalar.errors().count(), 0, "{:?}", scalar.diags);
    let decoded = fnv_script::decode(&scalar.bytes, &table);
    assert_eq!(decoded.report.raw_total(), 0);
    let Stmt::Set { expr, .. } = &decoded.program.blocks[0].body[0] else { panic!("expected scalar assignment") };
    assert_eq!(expr.0.len(), 1);
    assert!(matches!(&expr.0[0], Token::Num { value, .. } if *value == 12345.0), "decimal scalar must stay a number");
    assert!(scalar.refs.is_empty(), "a scalar must not acquire the conflicting form reference");

    // This caller also enables unknown_calls. Exercise the actual inline receiver
    // consumer, rather than assigning it the Auto test's result.
    let inline = compile_mod_runtime("scn Inline\nfloat result\nbegin GameMode\nset result to C1234.GetDisabled\nend\n", &table, &env);
    assert_eq!(inline.errors().count(), 0, "{:?}", inline.diags);
    let decoded = fnv_script::decode(&inline.bytes, &table);
    assert_eq!(decoded.report.raw_total(), 0);
    let Stmt::Set { expr, .. } = &decoded.program.blocks[0].body[0] else { panic!("expected inline assignment") };
    let [Token::Call { call }] = expr.0.as_slice() else { panic!("expected inline call: {expr:?}") };
    assert_eq!(call.opcode, table.by_name("GetDisabled").unwrap());
    assert!(call.args.is_empty());
    assert_eq!(inline.refs[call.on.expect("explicit receiver retained").0 as usize - 1], RefSlot::Form(FIVE));
}
