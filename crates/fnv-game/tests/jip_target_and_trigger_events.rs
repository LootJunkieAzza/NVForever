//! JIP LN `SetOnPCTargetChangeEventHandler` (`pctargetchange`),
//! `SetOnTriggerDownEventHandler` (`triggerdown`), and
//! `SetOnTriggerUpEventHandler` (`triggerup`) (Part of #2272;
//! docs/spec-notes/jip-events.md §2, §5) on a synthetic plugin.

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::senses::Reticle;
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL_A: u32 = 0x200;
const GUARD_REF: FormId = FormId(0x300);
const RAIDER_REF: FormId = FormId(0x301);
const DOOR_REF: FormId = FormId(0x302);
const GUARD: u32 = 0x800;
const RAIDER: u32 = 0x801;
const DOOR: u32 = 0x802;
const GMST_AUTOAIM: u32 = 0x805;

const TARGET_HITS: u32 = 0xA00;
const LT_DOWN: u32 = 0xA01;
const RT_DOWN: u32 = 0xA02;
const LT_UP: u32 = 0xA03;
const RT_UP: u32 = 0xA04;
const ANY_DOWN: u32 = 0xA05;
const ANY_UP: u32 = 0xA06;

const TARGET_UDF: u32 = 0xB00;
const LT_DOWN_UDF: u32 = 0xB01;
const RT_DOWN_UDF: u32 = 0xB02;
const LT_UP_UDF: u32 = 0xB03;
const RT_UP_UDF: u32 = 0xB04;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn glob(id: u32, edid: &str) -> Vec<u8> {
    record(b"GLOB", 0, id, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())])
}

fn udf(id: u32, edid: &str, source: &str) -> Vec<u8> {
    let mut fields = vec![sub(b"EDID", z(edid))];
    fields.extend(unit(0, &script(&[(0, None, Vec::new())]), &[], &[]));
    fields.push(sub(b"SCTX", source.as_bytes().to_vec()));
    record(b"SCPT", 0, id, &fields)
}

fn npc(id: u32, edid: &str) -> Vec<u8> {
    let mut acbs = vec![0u8; 20];
    acbs.extend(0i16.to_le_bytes());
    acbs.extend(0u16.to_le_bytes());
    let data = [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat();
    record(b"NPC_", 0, id, &[sub(b"EDID", z(edid)), sub(b"ACBS", acbs), sub(b"DATA", data)])
}

fn placed(sig: &[u8; 4], id: FormId, edid: &str, base: u32, x: f32) -> Vec<u8> {
    let data = [x, 0.0, 0.0, 0.0, 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    record(sig, 0, id.0, &[sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn fixture() -> Fixture {
    let globs = [
        glob(TARGET_HITS, "TargetHits"),
        glob(LT_DOWN, "LtDown"),
        glob(RT_DOWN, "RtDown"),
        glob(LT_UP, "LtUp"),
        glob(RT_UP, "RtUp"),
        glob(ANY_DOWN, "AnyDown"),
        glob(ANY_UP, "AnyUp"),
    ]
    .concat();
    let scpts = [
        udf(
            TARGET_UDF,
            "TargetUdf",
            "scn TargetUdf\nref rTarget\nstring_var sMarker\nBegin Function {rTarget}\n  set TargetHits to TargetHits + 1\nEnd\n",
        ),
        udf(
            LT_DOWN_UDF,
            "LtDownUdf",
            "scn LtDownUdf\nint iTrig\nstring_var sMarker\nBegin Function {iTrig}\n  set LtDown to LtDown + 1\nEnd\n",
        ),
        udf(
            RT_DOWN_UDF,
            "RtDownUdf",
            "scn RtDownUdf\nint iTrig\nstring_var sMarker\nBegin Function {iTrig}\n  set RtDown to RtDown + 1\nEnd\n",
        ),
        udf(
            LT_UP_UDF,
            "LtUpUdf",
            "scn LtUpUdf\nint iTrig\nstring_var sMarker\nBegin Function {iTrig}\n  set LtUp to LtUp + 1\nEnd\n",
        ),
        udf(
            RT_UP_UDF,
            "RtUpUdf",
            "scn RtUpUdf\nint iTrig\nstring_var sMarker\nBegin Function {iTrig}\n  set RtUp to RtUp + 1\nEnd\n",
        ),
    ]
    .concat();
    let gmst = record(b"GMST", 0, GMST_AUTOAIM, &[sub(b"EDID", z("fAutoAimMaxDistance")), sub(b"DATA", 1800f32.to_le_bytes().to_vec())]);
    let npcs = [npc(0x7, "Player"), npc(GUARD, "TestGuard"), npc(RAIDER, "TestRaider")].concat();
    let door = record(b"DOOR", 0, DOOR, &[sub(b"EDID", z("TestDoor"))]);
    let refs = [
        placed(b"ACHR", FormId::PLAYER_REF, "PlayerREF", 0x7, 0.0),
        placed(b"ACHR", GUARD_REF, "GuardRef", GUARD, 200.0),
        placed(b"ACHR", RAIDER_REF, "RaiderRef", RAIDER, 2200.0),
        placed(b"REFR", DOOR_REF, "DoorRef", DOOR, 100.0),
    ]
    .concat();
    let cell = [
        record(b"CELL", 0, CELL_A, &[sub(b"EDID", z("TestCellA"))]),
        group(CELL_A.to_le_bytes(), 6, &group(CELL_A.to_le_bytes(), 9, &refs)),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"GMST", &gmst),
        top(b"GLOB", &globs),
        top(b"SCPT", &scpts),
        top(b"DOOR", &door),
        top(b"NPC_", &npcs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "JipTargetTrigger.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    assert!(data.source_units.iter().all(|u| u.error.is_none()), "UDF source compile errors: {:?}", data.source_units);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let mut world = World::new(&f.forms, &f.refs, SimConfig::default());
    if let Ok(p) = world.get(FormId::PLAYER_REF) {
        p.cell = Some(FormId(CELL_A));
        p.pos = [0.0, 0.0, 0.0];
    }
    Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() })
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx { self_ref: Some(FormId::PLAYER_REF), ..Default::default() })
        .unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn aux_size(g: &mut Game, key: &str) -> f64 {
    match q(g, &format!("SetFunctionValue (PlayerREF.AuxVarSize \"{key}\")")) {
        XVal::Num(n) => n,
        v => panic!("AuxVarSize {key}: {v:?}"),
    }
}

fn aux_flt(g: &mut Game, key: &str, idx: u32) -> f64 {
    match q(g, &format!("SetFunctionValue (PlayerREF.AuxVarGetFlt \"{key}\" {idx})")) {
        XVal::Num(n) => n,
        v => panic!("AuxVarGetFlt {key}[{idx}]: {v:?}"),
    }
}

fn aux_ref(g: &mut Game, key: &str, idx: u32) -> FormId {
    match q(g, &format!("SetFunctionValue (PlayerREF.AuxVarGetRef \"{key}\" {idx})")) {
        XVal::Ref(r) => FormId(r),
        v => panic!("AuxVarGetRef {key}[{idx}]: {v:?}"),
    }
}

fn global(g: &Game, id: u32) -> f32 {
    g.world.global(FormId(id)).unwrap()
}

#[test]
fn pc_target_change_fires_on_actor_transitions_respects_range_and_removes() {
    let f = fixture();
    let mut g = game(&f);

    q(
        &mut g,
        "SetOnPCTargetChangeEventHandler TargetUdf 1\n\
         SetOnPCTargetChangeEventHandler (Begin Function {ref rTarget}\n\
           PlayerREF.AuxVarSetRef \"*pt\" rTarget -1\n\
           PlayerREF.AuxVarSetFlt \"*pt_null\" (rTarget == 0) -1\n\
         End) 1\n\
         SetOnPCTargetChangeEventHandler (Begin Function {}\n\
           PlayerREF.AuxVarSetFlt \"*pt_under\" 1 -1\n\
         End) 1",
    );

    // Non-actor or empty crosshair does not fire when target is already None.
    g.set_crosshair_ref(None);
    g.set_crosshair_ref(Some(DOOR_REF));
    assert_eq!(aux_size(&mut g, "*pt"), 0.0);
    assert_eq!(global(&g, TARGET_HITS), 0.0);

    // None -> GuardRef fires once; repeating GuardRef does not re-fire; tick does not double-fire.
    g.set_crosshair_ref(Some(GUARD_REF));
    g.set_crosshair_ref(Some(GUARD_REF));
    g.tick();
    assert_eq!(aux_size(&mut g, "*pt"), 1.0);
    assert_eq!(aux_ref(&mut g, "*pt", 0), GUARD_REF);
    assert_eq!(aux_flt(&mut g, "*pt_null", 0), 0.0);
    assert_eq!(global(&g, TARGET_HITS), 1.0);
    assert_eq!(aux_size(&mut g, "*pt_under"), 0.0, "0-parameter handler is skipped");

    // GuardRef -> RaiderRef (old -> new) via set_crosshair_ref fires once with RaiderRef.
    g.set_crosshair_ref(Some(RAIDER_REF));
    assert_eq!(aux_size(&mut g, "*pt"), 2.0);
    assert_eq!(aux_ref(&mut g, "*pt", 1), RAIDER_REF);
    assert_eq!(global(&g, TARGET_HITS), 2.0);

    // Aiming at a non-actor (DoorRef) clears target to 0 (null); clearing again to None does not re-fire.
    // Note: AuxVarSetRef ignores null refs [observed: jip-events.md s2], so *pt stays at 2 while *pt_null records 1.
    g.set_crosshair_ref(Some(DOOR_REF));
    g.set_crosshair_ref(None);
    assert_eq!(aux_size(&mut g, "*pt_null"), 3.0);
    assert_eq!(aux_flt(&mut g, "*pt_null", 2), 1.0, "clearing to non-actor passes null (0)");
    assert_eq!(aux_size(&mut g, "*pt"), 2.0, "null ref leaves no AuxVarSetRef element");
    assert_eq!(global(&g, TARGET_HITS), 3.0);

    // Engine crosshair_far + reticle.hit range check against fAutoAimMaxDistance (1800):
    // RaiderRef at 2200 units is out of range -> stays None.
    g.senses.reticle = Some(Reticle { start: [0.0; 3], dir: [1.0, 0.0, 0.0], hit: Some(2200.0) });
    g.senses.crosshair_far = Some(RAIDER_REF);
    g.tick();
    assert_eq!(aux_size(&mut g, "*pt_null"), 3.0, "actor beyond fAutoAimMaxDistance does not trigger target");

    // Raising fAutoAimMaxDistance to 2500 brings RaiderRef into range on the next tick.
    q(&mut g, "SetNumericGameSetting fAutoAimMaxDistance 2500");
    g.tick();
    assert_eq!(aux_size(&mut g, "*pt_null"), 4.0);
    assert_eq!(aux_flt(&mut g, "*pt_null", 3), 0.0);
    assert_eq!(aux_ref(&mut g, "*pt", 2), RAIDER_REF);

    // Leaving RaiderRef to sky (crosshair_far = None) clears back to null once.
    g.senses.crosshair_far = None;
    g.senses.reticle = None;
    g.tick();
    assert_eq!(aux_size(&mut g, "*pt_null"), 5.0);
    assert_eq!(aux_flt(&mut g, "*pt_null", 4), 1.0);

    // Explicit set_pc_target is not overwritten on subsequent ticks when Senses is unchanged.
    assert_eq!(g.set_pc_target(Some(GUARD_REF)), 2);
    g.tick();
    assert_eq!(aux_size(&mut g, "*pt_null"), 6.0);
    assert_eq!(aux_ref(&mut g, "*pt", 3), GUARD_REF);

    // Removing TargetUdf stops it while the inline handler continues firing.
    q(&mut g, "SetOnPCTargetChangeEventHandler TargetUdf 0");
    assert_eq!(g.set_pc_target(None), 1);
    assert_eq!(aux_size(&mut g, "*pt_null"), 7.0);
    assert_eq!(aux_flt(&mut g, "*pt_null", 6), 1.0);
    assert_eq!(global(&g, TARGET_HITS), 6.0, "removed TargetUdf no longer runs");
}

#[test]
fn trigger_down_and_up_events_match_exact_codes_edges_and_removal() {
    let f = fixture();
    let mut g = game(&f);

    q(
        &mut g,
        "SetOnTriggerDownEventHandler LtDownUdf 1 0\n\
         SetOnTriggerDownEventHandler RtDownUdf 1 1\n\
         SetOnTriggerUpEventHandler LtUpUdf 1 0\n\
         SetOnTriggerUpEventHandler RtUpUdf 1 1\n\
         SetOnTriggerDownEventHandler (Begin Function {int iTrig}\n\
           set AnyDown to AnyDown + 1\n\
           PlayerREF.AuxVarSetFlt \"*td\" iTrig -1\n\
         End) 1\n\
         SetOnTriggerUpEventHandler (Begin Function {int iTrig}\n\
           set AnyUp to AnyUp + 1\n\
           PlayerREF.AuxVarSetFlt \"*tu\" iTrig -1\n\
         End) 1\n\
         SetOnTriggerDownEventHandler (Begin Function {}\n\
           PlayerREF.AuxVarSetFlt \"*td_under\" 1 -1\n\
         End) 1 0",
    );

    // Below-threshold analog noise does not fire.
    g.jip2.told.pad.triggers = [0.05, 0.05];
    g.tick();
    assert_eq!((global(&g, LT_DOWN), global(&g, RT_DOWN), global(&g, ANY_DOWN)), (0.0, 0.0, 0.0));

    // Pressing Left Trigger (0) fires ONLY LtDown + AnyDown (NOT RtDown); holding across ticks does not re-fire.
    g.jip2.told.pad.triggers = [0.8, 0.0];
    g.tick();
    g.tick();
    assert_eq!((global(&g, LT_DOWN), global(&g, RT_DOWN), global(&g, ANY_DOWN)), (1.0, 0.0, 1.0));
    assert_eq!(aux_flt(&mut g, "*td", 0), 0.0);
    assert_eq!(aux_size(&mut g, "*td_under"), 0.0, "0-parameter trigger handler is skipped");

    // Pressing Right Trigger (1) fires ONLY RtDown + AnyDown — proving filter 0 is exact, NOT a wildcard!
    g.jip2.told.pad.triggers = [0.8, 1.0];
    g.tick();
    assert_eq!((global(&g, LT_DOWN), global(&g, RT_DOWN), global(&g, ANY_DOWN)), (1.0, 1.0, 2.0), "filter 0 must not match right trigger 1");
    assert_eq!(aux_flt(&mut g, "*td", 1), 1.0);

    // Releasing Left Trigger (0) fires ONLY LtUp + AnyUp (NOT RtUp).
    g.jip2.told.pad.triggers = [0.0, 1.0];
    g.tick();
    assert_eq!((global(&g, LT_UP), global(&g, RT_UP), global(&g, ANY_UP)), (1.0, 0.0, 1.0));
    assert_eq!(aux_flt(&mut g, "*tu", 0), 0.0);

    // Releasing Right Trigger (1) via set_pad_triggers fires RtUp + AnyUp immediately and does not double-fire on tick.
    g.set_pad_triggers(0.0, 0.0);
    g.tick();
    assert_eq!((global(&g, LT_UP), global(&g, RT_UP), global(&g, ANY_UP)), (1.0, 1.0, 2.0));
    assert_eq!(aux_flt(&mut g, "*tu", 1), 1.0);

    // Disabled trigger still fires; script HoldTrigger/ReleaseTrigger also drive edges (including in page_tick).
    q(&mut g, "DisableTrigger 0\nHoldTrigger 0");
    g.page_tick(fnv_game::game::menus::INVENTORY);
    assert_eq!((global(&g, LT_DOWN), global(&g, ANY_DOWN)), (2.0, 3.0));
    q(&mut g, "ReleaseTrigger 0");
    g.page_tick(fnv_game::game::menus::INVENTORY);
    assert_eq!((global(&g, LT_UP), global(&g, ANY_UP)), (2.0, 3.0));

    // Removing LtDownUdf (filter 0) and RtUpUdf (filter 1) stops those handlers while others still run.
    q(&mut g, "SetOnTriggerDownEventHandler LtDownUdf 0 0\nSetOnTriggerUpEventHandler RtUpUdf 0 1");
    g.set_pad_triggers(1.0, 1.0);
    g.set_pad_triggers(0.0, 0.0);
    assert_eq!((global(&g, LT_DOWN), global(&g, RT_DOWN), global(&g, ANY_DOWN)), (2.0, 2.0, 5.0));
    assert_eq!((global(&g, LT_UP), global(&g, RT_UP), global(&g, ANY_UP)), (3.0, 1.0, 5.0));
}

#[test]
fn target_and_trigger_events_run_newest_first() {
    let f = fixture();
    let mut g = game(&f);

    q(
        &mut g,
        "SetOnPCTargetChangeEventHandler (Begin Function {ref rTarget}\n\
           PlayerREF.AuxVarSetFlt \"*ord_t\" 1 -1\n\
         End) 1\n\
         SetOnPCTargetChangeEventHandler (Begin Function {ref rTarget}\n\
           PlayerREF.AuxVarSetFlt \"*ord_t\" 2 -1\n\
         End) 1\n\
         SetOnTriggerDownEventHandler (Begin Function {int iTrig}\n\
           PlayerREF.AuxVarSetFlt \"*ord_g\" 1 -1\n\
         End) 1 1\n\
         SetOnTriggerDownEventHandler (Begin Function {int iTrig}\n\
           PlayerREF.AuxVarSetFlt \"*ord_g\" 2 -1\n\
         End) 1 1",
    );

    g.set_crosshair_ref(Some(GUARD_REF));
    g.set_pad_triggers(0.0, 1.0);
    assert_eq!((aux_flt(&mut g, "*ord_t", 0), aux_flt(&mut g, "*ord_t", 1)), (2.0, 1.0));
    assert_eq!((aux_flt(&mut g, "*ord_g", 0), aux_flt(&mut g, "*ord_g", 1)), (2.0, 1.0));
}
