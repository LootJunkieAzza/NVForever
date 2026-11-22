//! Live session overrides for SPEL, ALCH, and MGEF traits & flags (Part of #2272;
//! docs/spec-notes/jip-records-8.md): canonical helpers, magic/aid/area/poison/service
//! consumers, and TempCloneForm snapshot isolation.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::data::{ARCHETYPE_SCRIPT, ARCHETYPE_VALUE_MODIFIER, IngestibleDef, MagicEffectDef};
use fnv_game::detection::{Band, Detection};
use fnv_game::game::{event::SCRIPT_EFFECT_START, Game, GameConfig, RunCtx};
use fnv_game::{GameData, services};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0xB00;
const NPC_BASE: u32 = 0xB01;
const NPC_REF: u32 = 0xB02;
const VENDOR_BASE: u32 = 0xB03;
const VENDOR_REF: u32 = 0xB04;
const GLOB_A: u32 = 0xB05;
const GLOB_B: u32 = 0xB06;
const SCPT_A: u32 = 0xB07;
const SCPT_B: u32 = 0xB08;
const MGEF_VAL: u32 = 0xB09;
const MGEF_SCRIPT: u32 = 0xB0A;
const MGEF_SCRIPT_EMPTY: u32 = 0xB0B;
const MGEF_TIMED: u32 = 0xB0C;
const SPEL_AREA: u32 = 0xB0D;
const SPEL_SCRIPT: u32 = 0xB0E;
const SPEL_TIMED: u32 = 0xB0F;
const SPEL_ADDICT_1: u32 = 0xB10;
const SPEL_ADDICT_2: u32 = 0xB11;
const ALCH_ITEM: u32 = 0xB12;
const ALCH_PLAIN: u32 = 0xB13;
const ENCH_ITEM: u32 = 0xB14;
const SOUN_1: u32 = 0xB15;
const SOUN_2: u32 = 0xB16;
const SPEL_ABILITY: u32 = 0xB17;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn edid(sig: &[u8; 4], fid: u32, name: &str, rest: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    record(sig, 0, fid, &[vec![sub(b"EDID", z(name))], rest.to_vec()].concat())
}

fn mgef(fid: u32, name: &str, flags: u32, item: u32, resist: i32, archetype: u32, av: i32) -> Vec<u8> {
    let data = [
        u32s(&[flags]),
        f32b(0.0),
        u32s(&[item]),
        (-1i32).to_le_bytes().to_vec(),
        resist.to_le_bytes().to_vec(),
        vec![0; 4],
        vec![0; 4],
        f32b(1.0),
        vec![0; 24],
        vec![0; 8],
        u32s(&[archetype]),
        av.to_le_bytes().to_vec(),
    ]
    .concat();
    assert_eq!(data.len(), 72);
    edid(b"MGEF", fid, name, &[sub(b"DATA", data)])
}

fn efit(mag: u32, area: u32, dur: u32, range: u32, av: i32) -> fnv_esm::raw::Subrecord {
    sub(b"EFIT", [u32s(&[mag, area, dur, range]), av.to_le_bytes().to_vec()].concat())
}

fn enit_alch(value: i32, flags: u8, withdrawal: u32, chance: f32, sound: u32) -> fnv_esm::raw::Subrecord {
    sub(b"ENIT", [value.to_le_bytes().to_vec(), vec![flags, 0, 0, 0], u32s(&[withdrawal]), f32b(chance), u32s(&[sound])].concat())
}

fn spit(kind: u32, flags: u8) -> fnv_esm::raw::Subrecord {
    sub(b"SPIT", [u32s(&[kind, 0, 0]), vec![flags, 0, 0, 0]].concat())
}

fn acbs(flags: u32, level: u16) -> Vec<u8> {
    [u32s(&[flags]), le16(0), le16(0), le16(level), le16(1), le16(1), le16(100), f32b(0.0), le16(0), le16(0)].concat()
}

fn aidt(services: u32) -> fnv_esm::raw::Subrecord {
    sub(b"AIDT", [vec![0, 2, 50, 50, 0, 0, 0, 0], u32s(&[services]), vec![0xFF, 0, 0, 0], u32s(&[0])].concat())
}

fn scpt_inc_glob(fid: u32, name: &str, glob_fid: u32) -> Vec<u8> {
    let body = set(var(b'G', 1), ex(&[var(b'G', 1), a("1"), a("+")]));
    record(b"SCPT", 0, fid, &[vec![sub(b"EDID", z(name))], unit(0x100, &script(&[(SCRIPT_EFFECT_START, None, body)]), &[], &[glob_fid])].concat())
}

fn placed_achr(fid: u32, name: &str, base: u32, x: f32) -> Vec<u8> {
    let data = [f32b(x), f32b(0.0), f32b(0.0), vec![0; 12]].concat();
    record(b"ACHR", 0, fid, &[sub(b"EDID", z(name)), sub(b"NAME", u32s(&[base])), sub(b"DATA", data)])
}

fn blocked_detection() -> Detection {
    Detection { value: -50.0, band: Band::Hidden, attenuation: 0.0, sound: 0.0, visual: 0.0, detector_skill: 0.0, target_skill: 0.0, seen: false, clear: false }
}

fn plugin_bytes() -> Vec<u8> {
    let globs = [
        edid(b"GLOB", GLOB_A, "TestGlobA", &[sub(b"FNAM", vec![b'f']), sub(b"FLTV", f32b(0.0))]),
        edid(b"GLOB", GLOB_B, "TestGlobB", &[sub(b"FNAM", vec![b'f']), sub(b"FLTV", f32b(0.0))]),
    ]
    .concat();
    let scpts = [scpt_inc_glob(SCPT_A, "TestEffectScriptA", GLOB_A), scpt_inc_glob(SCPT_B, "TestEffectScriptB", GLOB_B)].concat();
    let souns = [edid(b"SOUN", SOUN_1, "TestSound1", &[]), edid(b"SOUN", SOUN_2, "TestSound2", &[])].concat();
    let mgefs = [
        // Value Modifier on Health (16), Poison Resist (19), initially beneficial (flags=0).
        mgef(MGEF_VAL, "TestValEffect", 0, 0, 19, ARCHETYPE_VALUE_MODIFIER as u32, 16),
        // Script effect initially pointing to SCPT_A, Display Name Only initially off.
        mgef(MGEF_SCRIPT, "TestScriptMgef", 0, SCPT_A, -1, ARCHETYPE_SCRIPT as u32, -1),
        // Script effect with no initial script.
        mgef(MGEF_SCRIPT_EMPTY, "TestScriptMgefEmpty", 0, 0, -1, ARCHETYPE_SCRIPT as u32, -1),
        // Timed Value Modifier on Strength (5) with Recover (bit 1), no NoDeathDispel.
        mgef(MGEF_TIMED, "TestTimedMgef", MagicEffectDef::RECOVER, 0, -1, ARCHETYPE_VALUE_MODIFIER as u32, 5),
    ]
    .concat();
    let spels = [
        // Area spell: Actor Effect (0), SPIT flags = 0 (does NOT ignore LOS initially).
        edid(b"SPEL", SPEL_AREA, "TestAreaSpell", &[spit(0, 0), sub(b"EFID", u32s(&[MGEF_VAL])), efit(10, 20, 0, 0, 16)]),
        edid(b"SPEL", SPEL_SCRIPT, "TestScriptSpell", &[spit(0, 0), sub(b"EFID", u32s(&[MGEF_SCRIPT])), efit(1, 0, 0, 0, -1)]),
        edid(b"SPEL", SPEL_TIMED, "TestTimedSpell", &[spit(0, 0), sub(b"EFID", u32s(&[MGEF_TIMED])), efit(2, 0, 30, 0, 5)]),
        edid(b"SPEL", SPEL_ADDICT_1, "TestAddict1", &[spit(10, 0), sub(b"EFID", u32s(&[MGEF_TIMED])), efit(2, 0, 0, 0, 5)]),
        edid(b"SPEL", SPEL_ADDICT_2, "TestAddict2", &[spit(10, 0), sub(b"EFID", u32s(&[MGEF_TIMED])), efit(3, 0, 0, 0, 5)]),
        edid(b"SPEL", SPEL_ABILITY, "TestAbilitySpell", &[spit(4, 0), sub(b"EFID", u32s(&[MGEF_TIMED])), efit(2, 0, 0, 0, 5)]),
    ]
    .concat();
    let enchs = edid(
        b"ENCH",
        ENCH_ITEM,
        "TestEnch",
        &[sub(b"ENIT", [u32s(&[3, 0, 0]), vec![1, 0, 0, 0]].concat()), sub(b"EFID", u32s(&[MGEF_VAL])), efit(5, 0, 0, 0, 16)],
    );
    let alchs = [
        // Ingestible: flags=0, withdrawal=SPEL_ADDICT_1, chance=0.0, sound=SOUN_1, ETYP=10 (Chems), weight=1.0.
        edid(
            b"ALCH",
            ALCH_ITEM,
            "TestAid",
            &[
                sub(b"DATA", f32b(1.0)),
                enit_alch(25, 0, SPEL_ADDICT_1, 0.0, SOUN_1),
                sub(b"ETYP", 10i32.to_le_bytes().to_vec()),
                sub(b"EFID", u32s(&[MGEF_VAL])),
                efit(10, 0, 10, 0, 16),
                sub(b"EFID", u32s(&[MGEF_SCRIPT])),
                efit(1, 0, 0, 0, -1),
            ],
        ),
        // Plain ingestible with no ETYP, flags=0, no withdrawal, single MGEF_VAL effect.
        edid(
            b"ALCH",
            ALCH_PLAIN,
            "TestPlainAid",
            &[sub(b"DATA", f32b(0.5)), enit_alch(10, 0, 0, 0.0, 0), sub(b"EFID", u32s(&[MGEF_VAL])), efit(20, 0, 0, 0, 16)],
        ),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat()), sub(b"ACBS", acbs(0, 1))]),
        edid(b"NPC_", NPC_BASE, "TestNpc", &[sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat()), sub(b"ACBS", acbs(0, 1))]),
        // Vendor selling only STIMPAKS (1 << 6) and FOOD (1 << 4).
        edid(b"NPC_", VENDOR_BASE, "TestVendor", &[sub(b"ACBS", acbs(0, 1)), aidt(services::STIMPAKS | services::FOOD)]),
    ]
    .concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [placed_achr(NPC_REF, "TestNpcRef", NPC_BASE, 100.0), placed_achr(VENDOR_REF, "TestVendorRef", VENDOR_BASE, 200.0)].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"GLOB", &globs),
        top(b"SCPT", &scpts),
        top(b"SOUN", &souns),
        top(b"MGEF", &mgefs),
        top(b"SPEL", &spels),
        top(b"ENCH", &enchs),
        top(b"ALCH", &alchs),
        top(b"NPC_", &npcs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "MagicOverrides.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    let ps = g.world.get(FormId::PLAYER_REF).unwrap();
    ps.cell = Some(FormId(CELL));
    ps.pos = [0.0, 0.0, 0.0];
    g
}

fn q(g: &mut Game<'_>, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn n(g: &mut Game<'_>, expr: &str) -> f64 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        XVal::Ref(r) => f64::from(r),
        other => panic!("{expr}: {other:?}"),
    }
}

fn r(g: &mut Game<'_>, expr: &str) -> u32 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Ref(id) => id,
        other => panic!("{expr}: {other:?}"),
    }
}

#[test]
fn live_mgef_flag_and_script_overrides_reach_magic_consumers() {
    let f = fixture();
    let mut g = game(&f);

    // 1. Live SetBaseEffectScript: swap TestScriptMgef from SCPT_A -> SCPT_B before cast.
    assert_eq!(n(&mut g, "GetBaseEffectScript TestScriptMgef"), f64::from(SCPT_A));
    q(&mut g, "SetBaseEffectScript TestScriptMgef TestEffectScriptB");
    assert_eq!(n(&mut g, "GetBaseEffectScript TestScriptMgef"), f64::from(SCPT_B));
    g.apply_magic_item(FormId::PLAYER_REF, FormId(SPEL_SCRIPT), Some(FormId::PLAYER_REF), false);
    assert_eq!(g.world.global(FormId(GLOB_A)), Ok(0.0));
    assert_eq!(g.world.global(FormId(GLOB_B)), Ok(1.0), "live SetBaseEffectScript must execute on apply");

    // 2. Live SetBaseEffectFlag bit 13 (Display Name Only) -> aid_preview_names.
    assert!(g.aid_preview_names(FormId::PLAYER_REF, FormId(ALCH_ITEM)).is_empty());
    q(&mut g, "SetBaseEffectFlag TestScriptMgef 13 1");
    assert_eq!(g.aid_preview_names(FormId::PLAYER_REF, FormId(ALCH_ITEM)), vec![FormId(MGEF_SCRIPT)]);
    q(&mut g, "SetBaseEffectFlag TestScriptMgef 13 0");
    assert!(g.aid_preview_names(FormId::PLAYER_REF, FormId(ALCH_ITEM)).is_empty());

    // 3. Live SetBaseEffectFlag bits 0 (Hostile) & 2 (Detrimental) -> poison detection,
    //    aid_preview sign/potency, item_text::magic_changes, and hit resistance.
    assert!(!g.is_poison(FormId(ALCH_PLAIN)));
    assert_eq!(n(&mut g, "GetIsPoison TestPlainAid"), 0.0);
    assert_eq!(g.aid_preview(FormId::PLAYER_REF, FormId(ALCH_PLAIN)), vec![(16, 20.0)]);
    assert!(!g.magic_changes(FormId(ALCH_PLAIN))[0].detrimental);

    // Give NPC_REF Poison Resist (AV 19). While MGEF_VAL is beneficial, resisted == 0.
    q(&mut g, "TestNpcRef.SetAV PoisonResist 50");
    let expected_resist = (fnv_game::functions::actor_value(&mut g, FormId(NPC_REF), 19).unwrap() / 100.0).clamp(0.0, 1.0);
    assert!(expected_resist > 0.0);
    let m_before = g.magic_effect_def(FormId(MGEF_VAL));
    assert_eq!(g.resisted(FormId(NPC_REF), m_before.as_ref()), 0.0);

    // Flip MGEF_VAL to Hostile (bit 0) + Detrimental (bit 2).
    q(&mut g, "SetBaseEffectFlag TestValEffect 0 1");
    q(&mut g, "SetBaseEffectFlag TestValEffect 2 1");
    assert!(g.is_poison(FormId(ALCH_PLAIN)), "is_poison must read live SetBaseEffectFlag");
    assert_eq!(n(&mut g, "GetIsPoison TestPlainAid"), 1.0);
    assert_eq!(g.aid_preview(FormId::PLAYER_REF, FormId(ALCH_PLAIN)), vec![(16, -20.0)]);
    assert!(g.magic_changes(FormId(ALCH_PLAIN))[0].detrimental);
    let m_after = g.magic_effect_def(FormId(MGEF_VAL));
    assert!((g.resisted(FormId(NPC_REF), m_after.as_ref()) - expected_resist).abs() < 1e-9);

    // Flip bit 1 (Recover) on MGEF_VAL: timed ALCH_ITEM (10/sec for 10s = -100) becomes Modifier (-10).
    assert_eq!(g.aid_preview(FormId::PLAYER_REF, FormId(ALCH_ITEM)), vec![(16, -100.0)]);
    q(&mut g, "SetBaseEffectFlag TestValEffect 1 1");
    assert_eq!(g.aid_preview(FormId::PLAYER_REF, FormId(ALCH_ITEM)), vec![(16, -10.0)]);

    // GetIsPoisoned on an actor carrying an active effect from a now-poisonous ALCH.
    g.apply_magic_item(FormId(NPC_REF), FormId(ALCH_ITEM), None, false);
    assert_eq!(n(&mut g, "TestNpcRef.GetIsPoisoned"), 0.0);
    q(&mut g, "SetBaseEffectFlag TestScriptMgef 0 1");
    q(&mut g, "SetBaseEffectFlag TestScriptMgef 2 1");
    assert_eq!(n(&mut g, "TestNpcRef.GetIsPoisoned"), 1.0, "GetIsPoisoned must read live SetBaseEffectFlag");

    // 4. Live SetBaseEffectFlag bit 28 (No Death Dispel) -> kill keeps effect alive.
    q(&mut g, "SetBaseEffectFlag TestTimedMgef 28 1");
    g.apply_magic_item(FormId(NPC_REF), FormId(SPEL_TIMED), None, false);
    assert!(g.effects.iter().any(|e| e.target == FormId(NPC_REF) && e.source == FormId(SPEL_TIMED)));
    g.kill(FormId(NPC_REF), None);
    assert!(
        g.effects.iter().any(|e| e.target == FormId(NPC_REF) && e.source == FormId(SPEL_TIMED)),
        "No Death Dispel flag set via SetBaseEffectFlag must survive kill"
    );
}

#[test]
fn live_alch_and_spel_overrides_reach_aid_area_and_service_consumers() {
    let f = fixture();
    let mut g = game(&f);

    // 1. Live SetIngestibleFlag / SetEffectFlag -> aid_preview AND ingest_on potency.
    q(&mut g, "PlayerREF.SetAV Medicine 50");
    let med = fnv_game::functions::actor_value_capped(&mut g, FormId::PLAYER_REF, 37);
    let expected_potency = 1.0 + 2.0 * med / 100.0;
    let expected_heal = (20.0 * expected_potency + 1e-9).floor();
    assert_eq!(g.aid_preview(FormId::PLAYER_REF, FormId(ALCH_PLAIN)), vec![(16, 20.0)]);
    // Set Medicine flag (mask 4 / bit 2) via SetIngestibleFlag -> preview scales by Medicine potency.
    q(&mut g, "SetIngestibleFlag TestPlainAid 4 1");
    assert_eq!(n(&mut g, "GetEffectFlag TestPlainAid 2"), 1.0);
    assert_eq!(g.aid_preview(FormId::PLAYER_REF, FormId(ALCH_PLAIN)), vec![(16, expected_heal)]);
    let ing = g.ingest(FormId::PLAYER_REF, FormId(ALCH_PLAIN));
    assert!((ing.potency - expected_potency).abs() < 1e-9);
    // Clear via SetEffectFlag -> preview drops back to 20.
    q(&mut g, "SetEffectFlag TestPlainAid 2 0");
    assert_eq!(g.aid_preview(FormId::PLAYER_REF, FormId(ALCH_PLAIN)), vec![(16, 20.0)]);

    // 2. Live SetAddictionChance + SetAddictionEffect -> ingest_on addiction roll & sated().
    let ing0 = g.ingest(FormId::PLAYER_REF, FormId(ALCH_ITEM));
    assert!(ing0.addiction.is_none());
    q(&mut g, "SetAddictionChance TestAid 1.0");
    q(&mut g, "SetAddictionEffect TestAid TestAddict2");
    assert_eq!(n(&mut g, "GetAddictionChance TestAid"), 1.0);
    assert_eq!(n(&mut g, "GetAddictionEffect TestAid"), f64::from(SPEL_ADDICT_2));
    let ing1 = g.ingest(FormId::PLAYER_REF, FormId(ALCH_ITEM));
    assert_eq!(ing1.addiction, Some((1.0, true)));
    assert!(g.addictions(FormId::PLAYER_REF).contains(&FormId(SPEL_ADDICT_2)), "ingest_on must roll live withdrawal_effect");
    let addict_eff = g.effects.iter().find(|e| e.target == FormId::PLAYER_REF && e.source == FormId(SPEL_ADDICT_2)).unwrap();
    assert!(addict_eff.change.unwrap().suspended, "sated() must read live SetAddictionEffect");

    // 3. Live SetEquipType + SetIngestibleFlag -> services::sells, wheel_stimpak, carried_stimpaks, GetIsUsedItemEquipType.
    g.extra.entry(FormId(NPC_REF)).or_default().used_item = Some(FormId(ALCH_ITEM));
    assert_eq!(n(&mut g, "TestNpcRef.GetIsUsedItemEquipType 10"), 1.0);
    assert_eq!(n(&mut g, "TestNpcRef.GetIsUsedItemEquipType 11"), 0.0);
    assert!(!g.sells(FormId(VENDOR_REF), FormId(ALCH_ITEM)));
    q(&mut g, "SetEquipType TestAid 11");
    assert_eq!(n(&mut g, "GetEquipType TestAid"), 11.0);
    assert_eq!(n(&mut g, "TestNpcRef.GetIsUsedItemEquipType 11"), 1.0, "GetIsUsedItemEquipType must read live SetEquipType");
    assert_eq!(n(&mut g, "TestNpcRef.GetIsUsedItemEquipType 10"), 0.0);
    assert!(g.sells(FormId(VENDOR_REF), FormId(ALCH_ITEM)), "vendor service check must read live SetEquipType");
    g.world.add_item(FormId::PLAYER_REF, FormId(ALCH_ITEM), 2).unwrap();
    assert_eq!(g.wheel_stimpak(), Some((FormId(ALCH_ITEM), 2)));
    assert!(g.carried_stimpaks(FormId::PLAYER_REF).iter().any(|(id, _)| *id == FormId(ALCH_ITEM)));

    // Plain aid has no ETYP (-1): setting Food flag (mask 2) switches its vendor category from CHEMS -> FOOD.
    assert!(!g.sells(FormId(VENDOR_REF), FormId(ALCH_PLAIN)));
    q(&mut g, "SetIngestibleFlag TestPlainAid 2 1");
    assert!(g.sells(FormId(VENDOR_REF), FormId(ALCH_PLAIN)), "vendor service check must read live SetIngestibleFlag");

    // 4. Live SetEffectFlag bit 4 ("Area Effect Ignores LOS") on SPEL -> area_targets.
    g.detection.record(FormId::PLAYER_REF, FormId(NPC_REF), blocked_detection(), 0.0);
    assert!(
        !g.area_targets(FormId::PLAYER_REF, FormId(SPEL_AREA)).iter().any(|(id, _)| *id == FormId(NPC_REF)),
        "blocked LOS must exclude NPC_REF while SPIT bit 4 is clear"
    );
    q(&mut g, "SetEffectFlag TestAreaSpell 4 1");
    assert_eq!(n(&mut g, "GetEffectFlag TestAreaSpell 4"), 1.0);
    assert!(
        g.area_targets(FormId::PLAYER_REF, FormId(SPEL_AREA)).iter().any(|(id, _)| *id == FormId(NPC_REF)),
        "live SetEffectFlag bit 4 must let area_targets ignore LOS"
    );
    q(&mut g, "SetEffectFlag TestAreaSpell 4 0");
    assert!(!g.area_targets(FormId::PLAYER_REF, FormId(SPEL_AREA)).iter().any(|(id, _)| *id == FormId(NPC_REF)));
}

#[test]
fn temp_clone_form_isolates_spel_alch_and_mgef_overrides() {
    let f = fixture();
    let mut g = game(&f);
    g.detection.record(FormId::PLAYER_REF, FormId(NPC_REF), blocked_detection(), 0.0);

    // --- SPEL TempCloneForm isolation ---
    q(&mut g, "SetEffectFlag TestAreaSpell 4 1");
    let spel_c1 = r(&mut g, "TempCloneForm TestAreaSpell 1");
    assert_ne!(spel_c1, 0);
    assert_eq!(n(&mut g, &format!("GetEffectFlag {spel_c1:#010X} 4")), 1.0);
    assert_eq!(n(&mut g, &format!("GetActorEffectType {spel_c1:#010X}")), 0.0);
    assert!(g.area_targets(FormId::PLAYER_REF, FormId(spel_c1)).iter().any(|(id, _)| *id == FormId(NPC_REF)));

    // Post-clone edit on original does NOT bleed into spel_c1.
    q(&mut g, "SetEffectFlag TestAreaSpell 4 0");
    assert_eq!(n(&mut g, "GetEffectFlag TestAreaSpell 4"), 0.0);
    assert_eq!(n(&mut g, &format!("GetEffectFlag {spel_c1:#010X} 4")), 1.0);
    assert!(g.area_targets(FormId::PLAYER_REF, FormId(spel_c1)).iter().any(|(id, _)| *id == FormId(NPC_REF)));

    // Clone-of-clone inherits spel_c1 and stays isolated when spel_c1 changes.
    let spel_c2 = match q(&mut g, &format!("ref s1\nref s2\nlet s1 := {spel_c1:#010X}\nlet s2 := TempCloneForm s1 1\nSetFunctionValue s2")) {
        XVal::Ref(id) if id != 0 => id,
        other => panic!("clone-of-clone SPEL failed: {other:?}"),
    };
    q(&mut g, &format!("SetEffectFlag {spel_c1:#010X} 4 0"));
    assert_eq!(n(&mut g, &format!("GetEffectFlag {spel_c1:#010X} 4")), 0.0);
    assert_eq!(n(&mut g, &format!("GetEffectFlag {spel_c2:#010X} 4")), 1.0);

    // Cloned Ability SPEL: AddSpell admits permanent effect, and Dispel removes it consistently.
    let abil_c1 = r(&mut g, "TempCloneForm TestAbilitySpell 1");
    assert_ne!(abil_c1, 0);
    q(&mut g, &format!("ref ab\nlet ab := {abil_c1:#010X}\nPlayerREF.AddSpell ab"));
    assert_eq!(n(&mut g, &format!("PlayerREF.IsSpellTarget {abil_c1:#010X}")), 1.0);
    assert!(g.effects.iter().any(|e| e.target == FormId::PLAYER_REF && e.source == FormId(abil_c1) && e.permanent));
    q(&mut g, &format!("ref ab\nlet ab := {abil_c1:#010X}\nPlayerREF.Dispel ab"));
    assert_eq!(n(&mut g, &format!("PlayerREF.IsSpellTarget {abil_c1:#010X}")), 0.0, "Dispel must remove cloned Ability effect");
    assert!(!g.effects.iter().any(|e| e.target == FormId::PLAYER_REF && e.source == FormId(abil_c1)));
    assert!(g.extra.get(&FormId::PLAYER_REF).is_some_and(|x| x.dispelled.contains(&FormId(abil_c1))));

    // --- ALCH TempCloneForm isolation ---
    q(&mut g, "SetIngestibleFlag TestAid 4 1");
    q(&mut g, "SetAddictionChance TestAid 0.75");
    q(&mut g, "SetAddictionEffect TestAid TestAddict2");
    q(&mut g, "SetIngestibleConsumeSound TestSound2 TestAid");
    q(&mut g, "SetEquipType TestAid 11");
    let alch_c1 = r(&mut g, "TempCloneForm TestAid 1");
    assert_ne!(alch_c1, 0);
    assert_eq!(n(&mut g, &format!("GetEffectFlag {alch_c1:#010X} 2")), 1.0);
    assert_eq!(n(&mut g, &format!("GetAddictionChance {alch_c1:#010X}")), 0.75);
    assert_eq!(n(&mut g, &format!("GetAddictionEffect {alch_c1:#010X}")), f64::from(SPEL_ADDICT_2));
    assert_eq!(n(&mut g, &format!("GetIngestibleConsumeSound {alch_c1:#010X}")), f64::from(SOUN_2));
    assert_eq!(n(&mut g, &format!("GetEquipType {alch_c1:#010X}")), 11.0);
    assert_eq!(g.ingestible_def(FormId(alch_c1)).unwrap().flags, IngestibleDef::MEDICINE);

    // Post-clone edits on original do NOT affect alch_c1.
    q(&mut g, "SetIngestibleFlag TestAid 4 0");
    q(&mut g, "SetAddictionChance TestAid 0.1");
    q(&mut g, "SetAddictionEffect TestAid TestAddict1");
    q(&mut g, "SetIngestibleConsumeSound TestSound1 TestAid");
    q(&mut g, "SetEquipType TestAid 10");
    assert_eq!(n(&mut g, &format!("GetEffectFlag {alch_c1:#010X} 2")), 1.0);
    assert_eq!(n(&mut g, &format!("GetAddictionChance {alch_c1:#010X}")), 0.75);
    assert_eq!(n(&mut g, &format!("GetAddictionEffect {alch_c1:#010X}")), f64::from(SPEL_ADDICT_2));
    assert_eq!(n(&mut g, &format!("GetIngestibleConsumeSound {alch_c1:#010X}")), f64::from(SOUN_2));
    assert_eq!(n(&mut g, &format!("GetEquipType {alch_c1:#010X}")), 11.0);
    assert!(g.sells(FormId(VENDOR_REF), FormId(alch_c1)));
    assert!(!g.sells(FormId(VENDOR_REF), FormId(ALCH_ITEM)));
    g.extra.entry(FormId(NPC_REF)).or_default().used_item = Some(FormId(alch_c1));
    assert_eq!(n(&mut g, "TestNpcRef.GetIsUsedItemEquipType 11"), 1.0, "GetIsUsedItemEquipType must read cloned ALCH ETYP");
    assert_eq!(n(&mut g, "TestNpcRef.GetIsUsedItemEquipType 10"), 0.0);
    assert_eq!(n(&mut g, "TestNpcRef.GetIsUsedItemType 47"), 1.0);

    // Clone an ETYP-less ALCH, then set ETYP on original -> clone stays isolated.
    let plain_c1 = r(&mut g, "TempCloneForm TestPlainAid 1");
    assert_ne!(plain_c1, 0);
    q(&mut g, "SetEquipType TestPlainAid 11");
    assert_eq!(n(&mut g, "GetEquipType TestPlainAid"), 11.0);
    assert_eq!(n(&mut g, &format!("GetEquipType {plain_c1:#010X}")), 0.0);
    assert_eq!(g.ingestible_def(FormId(plain_c1)).unwrap().equip_type, -1);

    // --- MGEF TempCloneForm isolation ---
    q(&mut g, "SetBaseEffectFlag TestScriptMgef 13 1");
    q(&mut g, "SetBaseEffectScript TestScriptMgef TestEffectScriptB");
    let mgef_c1 = r(&mut g, "TempCloneForm TestScriptMgef 1");
    assert_ne!(mgef_c1, 0);
    assert_eq!(n(&mut g, &format!("GetBaseEffectArchetype {mgef_c1:#010X}")), ARCHETYPE_SCRIPT as f64);
    assert_eq!(n(&mut g, &format!("GetBaseEffectAV {mgef_c1:#010X}")), -1.0);
    assert_eq!(n(&mut g, &format!("GetBaseEffectFlag {mgef_c1:#010X} 13")), 1.0);
    assert_eq!(n(&mut g, &format!("GetBaseEffectScript {mgef_c1:#010X}")), f64::from(SCPT_B));

    // Post-clone edits on original do NOT bleed into mgef_c1.
    q(&mut g, "SetBaseEffectFlag TestScriptMgef 13 0");
    q(&mut g, "SetBaseEffectScript TestScriptMgef TestEffectScriptA");
    assert_eq!(n(&mut g, &format!("GetBaseEffectFlag {mgef_c1:#010X} 13")), 1.0);
    assert_eq!(n(&mut g, &format!("GetBaseEffectScript {mgef_c1:#010X}")), f64::from(SCPT_B));

    // Clone a scriptless MGEF, then assign script to original -> clone keeps none.
    let empty_c1 = r(&mut g, "TempCloneForm TestScriptMgefEmpty 1");
    assert_ne!(empty_c1, 0);
    q(&mut g, "SetBaseEffectScript TestScriptMgefEmpty TestEffectScriptA");
    assert_eq!(n(&mut g, "GetBaseEffectScript TestScriptMgefEmpty"), f64::from(SCPT_A));
    assert_eq!(n(&mut g, &format!("GetBaseEffectScript {empty_c1:#010X}")), 0.0);

    // --- ENCH TempCloneForm isolation ---
    let ench_c1 = r(&mut g, "TempCloneForm TestEnch 1");
    assert_ne!(ench_c1, 0);
    assert_eq!(n(&mut g, &format!("GetEffectFlag {ench_c1:#010X} 0")), 1.0);
    q(&mut g, "SetEffectFlag TestEnch 2 1");
    assert_eq!(n(&mut g, "GetEffectFlag TestEnch 2"), 1.0);
    assert_eq!(n(&mut g, &format!("GetEffectFlag {ench_c1:#010X} 2")), 0.0);
}
