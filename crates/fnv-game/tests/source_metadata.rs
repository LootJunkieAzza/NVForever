//! Synthetic plugin/master visibility at GameData's ordinary Auto source boundary.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{source_compile::Mode, GameData};
use fnv_script::ir::{RefIndex, Stmt, VarLetter, VarSlot};
use fnv_script::unit::RefSlot;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, RefTable};

const TARGET_SCRIPT: u32 = 0x900;
const QUEST: u32 = 0x901;
const ACTOR: u32 = 0x902;
const ACTOR_REF: u32 = 0x903;
const VIEWER_SCRIPT: u32 = 0x0300_0A00;

fn header(masters: &[&str]) -> Vec<u8> {
    let mut fields = vec![sub(b"HEDR", [1.34f32.to_le_bytes(), 0u32.to_le_bytes(), 0x800u32.to_le_bytes()].concat())];
    for master in masters {
        fields.extend([sub(b"MAST", z(master)), sub(b"DATA", vec![0; 8])]);
    }
    record(b"TES4", 1, 0, &fields)
}

fn target(vars: &[(u32, &str, u8)]) -> Vec<u8> {
    let mut fields = vec![sub(b"EDID", z("TargetScript"))];
    fields.extend(unit(0, &script(&[(0, None, vec![])]), vars, &[]));
    record(b"SCPT", 0, TARGET_SCRIPT, &fields)
}

fn plugins(source: &str, sees_override: bool, override_has_unit: bool) -> (Vec<Plugin>, Vec<u8>) {
    let base = [
        header(&[]),
        top(b"SCPT", &target(&[(1, "shared", 0)])),
        top(b"QUST", &record(b"QUST", 0, QUEST, &[sub(b"EDID", z("TargetQuest")), sub(b"SCRI", TARGET_SCRIPT.to_le_bytes().to_vec())])),
        top(b"NPC_", &record(b"NPC_", 0, ACTOR, &[sub(b"EDID", z("TargetActor")), sub(b"SCRI", TARGET_SCRIPT.to_le_bytes().to_vec())])),
        top(b"ACHR", &record(b"ACHR", 0x400, ACTOR_REF, &[sub(b"EDID", z("TargetRef")), sub(b"NAME", ACTOR.to_le_bytes().to_vec())])),
    ].concat();
    let master = [header(&["Base.esm"]), top(b"SCPT", &target(&[(1, "shared", 0), (7, "masterFlag", 1)]))].concat();
    let replacement = if override_has_unit {
        target(&[(1, "shared", 0), (3, "overrideFlag", 1)])
    } else {
        record(b"SCPT", 0, TARGET_SCRIPT, &[sub(b"EDID", z("TargetScript"))])
    };
    let later = [header(&["Base.esm", "Master.esm"]), top(b"SCPT", &replacement)].concat();
    let masters = if sees_override { vec!["Base.esm", "Master.esm", "Later.esp"] } else { vec!["Base.esm", "Master.esm"] };
    let original = script(&[(0, None, vec![])]);
    let mut fields = vec![sub(b"EDID", z("ViewerScript"))];
    fields.extend(unit(0, &original, &[], &[QUEST, ACTOR_REF]));
    fields.push(sub(b"SCTX", format!("scn ViewerScript\nstring_var unused\nbegin GameMode\n{source}\nend\n").into_bytes()));
    let viewer = [header(&masters), top(b"SCPT", &record(b"SCPT", 0, ((masters.len() as u32) << 24) | 0xA00, &fields))].concat();
    let bytes = [base, master, later, viewer];
    let names = ["Base.esm", "Master.esm", "Later.esp", "Viewer.esp"];
    (bytes.iter().zip(names).map(|(bytes, name)| Plugin::parse(bytes, name.into()).unwrap()).collect(), original)
}

fn build(plugins: &[Plugin]) -> GameData {
    assert_eq!(fnv_game::source_compile::mode(), Mode::Auto, "this regression exercises ordinary Auto selection");
    let lo = LoadOrder::new(plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let refs = RefTable::build(&lo, &schema);
    GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &json, &refs)
}

fn source_result(data: &GameData) -> &fnv_game::source_compile::SourceUnit {
    assert_eq!(data.source_units.len(), 1, "only the viewer's extender declaration selects Auto");
    let result = &data.source_units[0];
    assert_eq!(result.key, "Viewer.esp|SCPT 0x03000A00");
    result
}

fn assert_targets(data: &GameData, expected: &[(u16, u16, VarLetter)]) {
    let compiled = data.unit(data.scripts[&FormId(VIEWER_SCRIPT)]);
    assert_eq!(compiled.unit.refs, [RefSlot::Form(QUEST), RefSlot::Form(ACTOR_REF)], "seeded receiver indices stay intact");
    let body = &compiled.program.blocks[0].body;
    assert_eq!(body.len(), expected.len());
    for (stmt, &(receiver, index, letter)) in body.iter().zip(expected) {
        let Stmt::Set { target, .. } = stmt else { panic!("expected remote assignment: {stmt:?}") };
        assert_eq!(target.on, Some(RefIndex(receiver)));
        assert_eq!(target.var, VarSlot::Local { index, letter });
    }
}

#[test]
fn auto_compile_uses_the_masters_remote_variables_without_replacing_the_winning_script() {
    let (plugins, _) = plugins("set TargetQuest.masterFlag to 1\nset TargetRef.masterFlag to 2\nset TargetQuest.shared to 3", false, true);
    let data = build(&plugins);
    assert!(source_result(&data).error.is_none(), "{:?}", source_result(&data).error);
    assert_targets(&data, &[(1, 7, VarLetter::Short), (2, 7, VarLetter::Short), (1, 1, VarLetter::Float)]);
    let winning = data.unit(data.scripts[&FormId(TARGET_SCRIPT)]);
    assert_eq!(winning.unit.plugin, "Later.esp", "metadata indexing must not change execution's winner");
    assert_eq!(winning.unit.variables.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["shared", "overrideFlag"]);
}

#[test]
fn a_visible_override_uses_its_own_remote_index() {
    let (plugins, _) = plugins("set TargetQuest.overrideFlag to 1\nset TargetRef.overrideFlag to 2", true, true);
    let data = build(&plugins);
    assert!(source_result(&data).error.is_none(), "{:?}", source_result(&data).error);
    assert_targets(&data, &[(1, 3, VarLetter::Short), (2, 3, VarLetter::Short)]);
}

#[test]
fn stale_or_nonvisible_remote_names_keep_the_original_bytes() {
    for (sees_override, source, override_has_unit) in [
        (true, "set TargetQuest.masterFlag to 1", true),
        (false, "set TargetRef.overrideFlag to 1", true),
        (false, "set TargetQuest.neverDeclared to 1", true),
        (true, "set TargetQuest.shared to 1", false),
    ] {
        let (plugins, original) = plugins(source, sees_override, override_has_unit);
        let data = build(&plugins);
        assert_eq!(source_result(&data).error.as_ref().map(|e| e.kind), Some("unresolved_remote_variable"), "{source}");
        assert_eq!(data.unit(data.scripts[&FormId(VIEWER_SCRIPT)]).unit.bytes, original, "a genuine source error retains SCDA: {source}");
    }
}

#[test]
fn metadata_survives_when_a_nonvisible_override_empties_the_script() {
    let (plugins, _) = plugins("set TargetRef.masterFlag to 1", false, false);
    let data = build(&plugins);
    assert!(source_result(&data).error.is_none(), "{:?}", source_result(&data).error);
    assert_targets(&data, &[(2, 7, VarLetter::Short)]);
    assert!(!data.scripts.contains_key(&FormId(TARGET_SCRIPT)), "an empty winning override remains nonexecutable");
}

#[test]
fn auto_compile_resolves_remote_variables_on_template_scripted_actor_refs() {
    const TPL_ACTOR: u32 = 0x910;
    const CHILD_ACTOR: u32 = 0x911;
    const TPL_REF: u32 = 0x912;
    const VIEWER: u32 = 0x0100_0A10;

    let mut acbs = vec![0u8; 24];
    acbs[22..24].copy_from_slice(&(1u16 << 9).to_le_bytes());
    let base = [
        header(&[]),
        top(b"SCPT", &target(&[(1, "shared", 0), (4, "tplFlag", 1)])),
        top(b"QUST", &record(b"QUST", 0, QUEST, &[sub(b"EDID", z("TargetQuest")), sub(b"SCRI", TARGET_SCRIPT.to_le_bytes().to_vec())])),
        top(
            b"NPC_",
            &[
                record(b"NPC_", 0, TPL_ACTOR, &[sub(b"EDID", z("TemplateActor")), sub(b"SCRI", TARGET_SCRIPT.to_le_bytes().to_vec())]),
                record(b"NPC_", 0, CHILD_ACTOR, &[sub(b"EDID", z("ChildActor")), sub(b"ACBS", acbs), sub(b"TPLT", TPL_ACTOR.to_le_bytes().to_vec())]),
            ]
            .concat(),
        ),
        top(b"ACHR", &record(b"ACHR", 0x400, TPL_REF, &[sub(b"EDID", z("TemplatedRef")), sub(b"NAME", CHILD_ACTOR.to_le_bytes().to_vec())])),
    ]
    .concat();
    let original = script(&[(0, None, vec![])]);
    let mut fields = vec![sub(b"EDID", z("ViewerScript"))];
    fields.extend(unit(0, &original, &[], &[QUEST, TPL_REF]));
    fields.push(sub(
        b"SCTX",
        b"scn ViewerScript\nstring_var unused\nbegin GameMode\nif TemplatedRef.tplFlag == 1\nset TemplatedRef.tplFlag to 2\nendif\nset TargetQuest.shared to 3\nend\n"
            .to_vec(),
    ));
    let viewer = [header(&["Base.esm"]), top(b"SCPT", &record(b"SCPT", 0, VIEWER, &fields))].concat();
    let plugins = vec![
        Plugin::parse(&base, "Base.esm".into()).unwrap(),
        Plugin::parse(&viewer, "Viewer.esp".into()).unwrap(),
    ];
    let data = build(&plugins);
    assert_eq!(data.source_units.len(), 1);
    assert!(data.source_units[0].error.is_none(), "{:?}", data.source_units[0].error);
    assert!(data.source_units[0].unknown_functions.is_empty(), "{:?}", data.source_units[0].unknown_functions);
    let compiled = data.unit(data.scripts[&FormId(VIEWER)]);
    assert_eq!(compiled.unit.refs, [RefSlot::Form(QUEST), RefSlot::Form(TPL_REF)]);
}
