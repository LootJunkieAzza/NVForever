//! Authored opaque argument selects the real Auto source compiler; no private bytes.
use super::*;
use fnv_game::game::RunCtx;

#[test]
fn auto_source_recompile_keeps_a_grouped_second_argument_for_real_game_dispatch() {
    assert_eq!(fnv_game::source_compile::mode(), fnv_game::source_compile::Mode::Auto, "the fixture must exercise ordinary Auto admission, not a forced mode");
    const OWNED_SCRIPT: u32 = 0xA40;
    let source = "scn OwnedInlineSource\nBegin GameMode\nset TestGlobal to GetObjectiveCompleted Q (3 + 7)\nEnd\n";
    let (table, _) = catalogue();
    let opcode = table.by_name("GetObjectiveCompleted").unwrap();
    // A synthetic unreadable value, not a native inline encoding. It exists only
    // to trigger the same documented raw>0 source-selection branch.
    let opaque = [le16(2), r(1), vec![0xFF]].concat();
    let bytes = script(&[(0, None, set(var(b'G', 2), ex(&[xcall(opcode, &opaque)])))]);
    assert!(fnv_script::decode(&bytes, &table).report.raw_total() > 0);
    // EDID precedes the contiguous SCHR/SCDA/SCRO/SCTX unit group.
    let mut subs = vec![sub(b"EDID", z("OwnedInlineSource"))];
    subs.extend(unit(0, &bytes, &[], &[QUEST, GLOBAL]));
    subs.push(sub(b"SCTX", source.as_bytes().to_vec()));
    let plugin = [plugin_bytes(), top(b"SCPT", &record(b"SCPT", 0, OWNED_SCRIPT, &subs))].concat();
    let plugins = [Plugin::parse(&plugin, "OwnedInline.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let schema = schema();
    assert_eq!(order.plugins.len(), 1);
    assert_eq!(order.plugins[0].name, "OwnedInline.esm");
    let records: Vec<_> = order.records[0].iter().filter(|r| r.header.sig == *b"SCPT" && r.header.form_id == OWNED_SCRIPT).collect();
    assert_eq!(records.len(), 1, "one exact authored SCPT record");
    let mut read_issues = fnv_script::unit::ReadIssues::default();
    let read_units = fnv_script::unit::read_record(records[0], &order.plugins[0].name, &|form| order.to_global(0, form), &schema, &mut read_issues);
    assert_eq!(read_units.len(), 1, "one exact extracted unit: {read_issues:?}");
    let expected_owner = fnv_script::unit::UnitOwner::Script { form: OWNED_SCRIPT };
    let authored = &read_units[0];
    assert_eq!(authored.owner, expected_owner);
    assert_eq!(authored.plugin, "OwnedInline.esm", "exact authored plugin identity");
    assert!(authored.has_scda, "the real reader must retain the opaque compiled bytes");
    assert_eq!(authored.bytes, bytes, "the real reader retained the exact authored SCDA");
    assert_eq!(authored.source.as_deref(), Some(source), "SCTX belongs to this same unit");
    assert!(fnv_script::decode(&authored.bytes, &table).report.raw_total() > 0, "the extracted unit, not only the byte builder, selects Auto");
    let unit_key = authored.key();
    let (table, catalogue_json) = catalogue();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(&order, &schema, table, &catalogue_json, &refs);
    let unit_id = *data.scripts.get(&FormId(OWNED_SCRIPT)).expect("the exact enabled SCPT has a real dispatch entry");
    let compiled = &data.units[unit_id];
    assert_eq!(compiled.unit.owner, expected_owner, "the dispatch entry owns the authored SCPT");
    assert_eq!(compiled.unit.plugin, "OwnedInline.esm");
    assert_eq!(compiled.key(), unit_key, "compiled identity matches the real reader key");
    assert_eq!(data.source_units.len(), 1, "exact selected source-unit census: {:?}", data.source_units);
    let selected: Vec<_> = data.source_units.iter().filter(|u| u.key == unit_key).collect();
    assert_eq!(selected.len(), 1, "Auto selected this exact authored unit");
    let selected = selected[0];
    assert_eq!(selected.plugin, "OwnedInline.esm");
    assert!(selected.error.is_none(), "source compile setup: {:?}", selected.error);
    let fixture = Fixture { data, forms, refs };
    for completed in [false, true] {
        let mut game = game(&fixture);
        game.new_game();
        game.run_source(&format!("SetObjectiveCompleted Q 10 {}", u8::from(completed)), RunCtx::default()).unwrap();
        assert_ne!(game.world.global(FormId(GLOBAL)).unwrap(), f32::from(u8::from(completed)), "initial sentinel differs from both outcomes");
        assert!(game.run_event(InstKey::Ref { reference: FormId(NPC_REF) }, FormId(OWNED_SCRIPT), 0, None, RunCtx { self_ref: Some(FormId(NPC_REF)), ..Default::default() }));
        assert_eq!(game.world.global(FormId(GLOBAL)).unwrap(), f32::from(u8::from(completed)), "Auto-recompiled grouped argument must reach the actual quest getter and global write");
    }
}
