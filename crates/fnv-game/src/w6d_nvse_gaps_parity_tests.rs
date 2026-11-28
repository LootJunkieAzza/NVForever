//! Wave 6D-6, 6D-7, 6D-8, 6D-9 & 6D-12 `crates/fnv-game/src/nvse_gaps.rs` parity regression tests.
//!
//! Covers consolidated `nvse_gaps.rs` fixes alongside existing-test updates for #4109, #4205, #4643:
//! - #4300 (`CrippleLimb` accepts limb index 6 `BrainCondition`)
//! - #4322 (`GetCalculatedWeaponDamage` GMSTs, `TempCloneForm` unclone, EP 0 perks)
//! - #4324 (`GetCalculatedWeaponDPS` automatic crit mult division by fire rate)
//! - #4368 (`GetActorValueModifier` JIP filter codes 0/1/2/3)
//! - #4420 (`IsInCombatWith` checks all active combat targets)
//! - #4464 (`GetAvailableRadios` works without explicit calling reference)
//! - #4478 (`SetValueAlt` writes `ENIT/value` for `ALCH` ingestibles)
//! - #4626 (cloned `SPEL`/`ENCH` Nth-effect queries and Add/RemoveNthEffect isolation)
//! - #4630 (`SetImpactDataSetForm` validates `IPDS`/`IPCT`, null no-op, omitted clears)
//! - #4637 (`GetBaseEffectArchetype` returns `-1` for non-`MGEF` and null forms)
//! - #4643 (`SetProjectileExplosion` clears on omitted or non-`EXPL`, keeps on explicit `0`)
//! - #4649 (`GetIsPoison` requires `ALCH` and reads dynamic `effect_list`)

use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::nvse_gaps::RadioSite;
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn build_fixture(bytes: &[u8], name: &str) -> Fixture {
    let plugins = [Plugin::parse(bytes, name.into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue: Json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

fn q(g: &mut Game<'_>, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{}}\n{body}\nEnd\n"), RunCtx::default())
        .unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn qn(g: &mut Game<'_>, expr: &str) -> f64 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(n) => n,
        other => panic!("expected Num for {expr}, got {other:?}"),
    }
}

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn edid(sig: &[u8; 4], fid: u32, name: &str, rest: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    record(sig, 0, fid, &[vec![sub(b"EDID", z(name))], rest.to_vec()].concat())
}

fn mgef_rec(fid: u32, name: &str, flags: u32, arch: u32, av: i32) -> Vec<u8> {
    let mut d = vec![0u8; 72];
    d[0..4].copy_from_slice(&flags.to_le_bytes());
    d[64..68].copy_from_slice(&arch.to_le_bytes());
    d[68..72].copy_from_slice(&av.to_le_bytes());
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(name)), sub(b"DATA", d)])
}

#[test]
fn cripple_limb_covers_indices_0_through_6_including_brain_condition() {
    let bytes = [tes4(), top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]))].concat();
    let f = build_fixture(&bytes, "W6d6Cripple.esm");
    let mut g = game(&f);
    let avs = [
        "PerceptionCondition",
        "EnduranceCondition",
        "LeftAttackCondition",
        "RightAttackCondition",
        "LeftMobilityCondition",
        "RightMobilityCondition",
        "BrainCondition",
    ];
    q(&mut g, "player.CrippleLimb -1\nplayer.CrippleLimb 7");
    for av in avs {
        assert!(qn(&mut g, &format!("player.GetActorValue {av}")) > 0.0, "{av} untouched by OOB");
    }
    q(&mut g, "player.CrippleLimb 6");
    assert_eq!(qn(&mut g, "player.GetActorValue BrainCondition"), 0.0);
}

fn calc_weapon_fixture() -> Fixture {
    let calc_data = [100i32.to_le_bytes().to_vec(), 100i32.to_le_bytes().to_vec(), f32b(2.0), 20i16.to_le_bytes().to_vec(), vec![10]].concat();
    let mut calc_dnam = vec![0u8; 204];
    calc_dnam[0..4].copy_from_slice(&3u32.to_le_bytes());
    calc_dnam[88..92].copy_from_slice(&2.0f32.to_le_bytes());
    calc_dnam[104..108].copy_from_slice(&41u32.to_le_bytes());
    let calc_crdt = [30u16.to_le_bytes().to_vec(), vec![0, 0], f32b(2.0), vec![0; 4], u32s(&[0])].concat();
    let bytes = [
        tes4(),
        top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))])),
        top(b"WEAP", &edid(b"WEAP", 0xA19, "CalcGun", &[sub(b"DATA", calc_data), sub(b"DNAM", calc_dnam), sub(b"CRDT", calc_crdt)])),
        top(
            b"PERK",
            &record(
                b"PERK",
                0,
                0xA20,
                &[
                    sub(b"EDID", z("CalcDmgPerk")),
                    sub(b"FULL", z("CalcDmgPerk")),
                    sub(b"DATA", vec![0, 1, 1, 1, 0]),
                    sub(b"PRKE", vec![2, 0, 0]),
                    sub(b"DATA", vec![crate::perks::CALCULATE_WEAPON_DAMAGE, crate::perks::MULTIPLY_VALUE, 1]),
                    sub(b"EPFT", vec![1]),
                    sub(b"EPFD", f32b(1.05)),
                    sub(b"PRKF", vec![]),
                ],
            ),
        ),
        top(
            b"GMST",
            &[
                edid(b"GMST", 0xA1E, "fDamageWeaponMult", &[sub(b"DATA", f32b(1.0))]),
                edid(b"GMST", 0xA1F, "fDamageSkillBase", &[sub(b"DATA", f32b(0.5))]),
                edid(b"GMST", 0xA21, "fDamageSkillMult", &[sub(b"DATA", f32b(0.5))]),
            ]
            .concat(),
        ),
    ]
    .concat();
    build_fixture(&bytes, "W6d6CalcWeap.esm")
}

#[test]
fn calc_weapon_damage_reads_gmsts_unclones_and_applies_weapon_damage_perks() {
    let f = calc_weapon_fixture();
    let mut g = game(&f);
    q(&mut g, "PlayerREF.ForceAV Guns 100\nPlayerREF.ForceAV CritChance 0");
    let cloned = match q(&mut g, "ref rCl\nlet rCl := TempCloneForm CalcGun\nSetFunctionValue (PlayerREF.GetCalculatedWeaponDamage rCl)") {
        XVal::Num(v) => v,
        other => panic!("expected Num, got {other:?}"),
    };
    assert!((cloned - 20.0).abs() < 1e-5, "cloned gun uses Guns=100 -> 20.0, got {cloned}");
    assert!((qn(&mut g, "PlayerREF.GetCalculatedWeaponDamage CalcGun") - 20.0).abs() < 1e-5);
    q(&mut g, "PlayerREF.AddPerk CalcDmgPerk");
    assert!((qn(&mut g, "PlayerREF.GetCalculatedWeaponDamage CalcGun") - 21.0).abs() < 1e-5);
    assert!((qn(&mut g, "PlayerREF.GetCalculatedWeaponDPS CalcGun") - 40.0).abs() < 1e-5);
    q(&mut g, "PlayerREF.RemovePerk CalcDmgPerk");
    assert!((qn(&mut g, "PlayerREF.GetCalculatedWeaponDamage CalcGun") - 20.0).abs() < 1e-5);
    q(&mut g, "SetNumericGameSetting \"fDamageWeaponMult\" 2.0");
    assert!((qn(&mut g, "PlayerREF.GetCalculatedWeaponDamage CalcGun") - 40.0).abs() < 1e-5);
    q(&mut g, "SetNumericGameSetting \"fDamageWeaponMult\" 1.0");
    q(&mut g, "PlayerREF.ForceAV Guns 0\nSetNumericGameSetting \"fDamageSkillBase\" 0.25");
    assert!((qn(&mut g, "PlayerREF.GetCalculatedWeaponDamage CalcGun") - 5.0).abs() < 1e-5);
    q(&mut g, "PlayerREF.ForceAV Guns 100\nSetNumericGameSetting \"fDamageSkillBase\" 0.5\nSetNumericGameSetting \"fDamageSkillMult\" 1.0");
    assert!((qn(&mut g, "PlayerREF.GetCalculatedWeaponDamage CalcGun") - 30.0).abs() < 1e-5);
}

#[test]
fn calc_weapon_dps_divides_crit_mult_by_fire_rate_for_automatic_weapons() {
    let f = calc_weapon_fixture();
    let mut g = game(&f);
    q(&mut g, "PlayerREF.ForceAV Guns 100\nPlayerREF.ForceAV CritChance 50");
    q(&mut g, "SetWeaponFlags1 2 CalcGun");
    let dps_no_fr = qn(&mut g, "PlayerREF.GetCalculatedWeaponDPS CalcGun");
    assert!((dps_no_fr - 100.0).abs() < 1e-4, "fire_rate=0 fallback: expected 100.0, got {dps_no_fr}");

    q(&mut g, "SetWeaponFireRate 9.0 CalcGun");
    let dps_auto = qn(&mut g, "PlayerREF.GetCalculatedWeaponDPS CalcGun");
    assert!((dps_auto - 210.0).abs() < 1e-4, "auto fire_rate=9: expected 210.0, got {dps_auto}");

    g.tweaks.crit_ignores_fire_rate = true;
    let dps_ignore = qn(&mut g, "PlayerREF.GetCalculatedWeaponDPS CalcGun");
    assert!((dps_ignore - 450.0).abs() < 1e-4, "crit_ignores_fire_rate: expected 450.0, got {dps_ignore}");
    g.tweaks.crit_ignores_fire_rate = false;

    q(&mut g, "PlayerREF.ForceAV Guns 0");
    let dps_clone = match q(
        &mut g,
        "ref rCl\nlet rCl := TempCloneForm CalcGun\nSetWeaponFlags1 2 rCl\nSetWeaponFireRate 4.0 rCl\nSetFunctionValue (PlayerREF.GetCalculatedWeaponDPS rCl)",
    ) {
        XVal::Num(v) => v,
        other => panic!("expected Num, got {other:?}"),
    };
    assert!((dps_clone - 70.0).abs() < 1e-4, "clone fire_rate=4: expected 70.0, got {dps_clone}");
}

#[test]
fn jip_get_actor_value_modifier_filter_codes_0_1_2_3() {
    let mut acbs = 0u32.to_le_bytes().to_vec();
    for v in [50u16, 0, 1, 0, 0, 100] {
        acbs.extend(v.to_le_bytes());
    }
    acbs.extend(0f32.to_le_bytes());
    acbs.extend([0u8; 4]);
    let data = [100i32.to_le_bytes().to_vec(), vec![5, 5, 5, 5, 5, 5, 5]].concat();
    let bytes = [
        tes4(),
        top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"ACBS", acbs), sub(b"DATA", data)])),
    ]
    .concat();
    let f = build_fixture(&bytes, "W6d7AvMod.esm");
    let mut g = game(&f);
    q(&mut g, "player.ModAV Strength 2");
    assert_eq!(qn(&mut g, "player.GetActorValueModifier Strength"), 2.0);
    assert_eq!(qn(&mut g, "player.GetActorValueModifier Strength 0"), 0.0);
    assert_eq!(qn(&mut g, "player.GetActorValueModifier Strength 1"), 0.0);
    assert_eq!(qn(&mut g, "player.GetActorValueModifier Strength 2"), 2.0);
    assert_eq!(qn(&mut g, "player.GetActorValueModifier Strength 3"), 2.0);
    g.extra.entry(FormId::PLAYER_REF).or_default().av_temporary.insert(crate::actor_values::STRENGTH, 3.0);
    assert_eq!(qn(&mut g, "player.GetActorValueModifier Strength 0"), 0.0);
    assert_eq!(qn(&mut g, "player.GetActorValueModifier Strength 1"), 3.0);
    assert_eq!(qn(&mut g, "player.GetActorValueModifier Strength 2"), 2.0);
    assert_eq!(qn(&mut g, "player.GetActorValueModifier Strength 3"), 5.0);
    assert_eq!(qn(&mut g, "player.GetActorValueModifier Strength"), 5.0);
}

#[test]
fn isincombatwith_returns_1_for_both_primary_and_secondary_targets() {
    const NPC_BASE: u32 = 0x100;
    const REF_A: u32 = 0x101;
    const REF_B: u32 = 0x102;
    const REF_C: u32 = 0x103;
    const REF_D: u32 = 0x104;
    const CELL: u32 = 0x400;
    let npc = record(b"NPC_", 0, NPC_BASE, &[sub(b"EDID", z("FighterNpc"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell")), sub(b"DATA", vec![1])]);
    let refs = [
        record(b"ACHR", 0, REF_A, &[sub(b"EDID", z("RefA")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]),
        record(b"ACHR", 0, REF_B, &[sub(b"EDID", z("RefB")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]),
        record(b"ACHR", 0, REF_C, &[sub(b"EDID", z("RefC")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]),
        record(b"ACHR", 0, REF_D, &[sub(b"EDID", z("RefD")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"NPC_", &[record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), npc].concat()),
        top(b"CELL", &[cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &refs))].concat()),
    ]
    .concat();
    let f = build_fixture(&bytes, "W6d8Cmb.esm");
    let mut g = game(&f);

    q(&mut g, "RefA.StartCombat RefB\nRefA.StartCombat RefC");
    assert_eq!(qn(&mut g, "RefA.IsInCombatWith RefB"), 1.0);
    assert_eq!(qn(&mut g, "RefA.IsInCombatWith RefC"), 1.0);
    assert_eq!(qn(&mut g, "RefA.IsInCombatWith RefD"), 0.0);
    assert_eq!(qn(&mut g, "ar_Size (RefA.GetCombatTargets)"), 2.0);
}

#[test]
fn getavailableradios_returns_same_array_for_bare_and_player_calls() {
    let bytes = [tes4(), top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]))].concat();
    let f = build_fixture(&bytes, "W6d9Radio.esm");
    let mut g = game(&f);
    g.gaps.radio_sites = Some(vec![RadioSite {
        station: FormId(0x0014_DF03),
        on_pipboy: true,
        initially_disabled: false,
        broadcast: fnv_world::radio::Broadcast {
            range: fnv_world::radio::BroadcastRange::Everywhere,
            radius: 0.0,
            static_percent: 0.0,
            cell: 0,
            world: None,
            exterior: None,
        },
    }]);
    let res = q(
        &mut g,
        "array_var aPl\nlet aPl := Player.GetAvailableRadios\narray_var aBare\nlet aBare := GetAvailableRadios\nSetFunctionValue ((Ar_Size aPl) * 10 + (Ar_Size aBare))",
    );
    assert_eq!(res, XVal::Num(11.0));
}

#[test]
fn setvaluealt_writes_enit_value_for_alch_and_data_value_for_other_items() {
    let mut enit = 75i32.to_le_bytes().to_vec();
    enit.extend([0u8; 16]);
    let alch = record(b"ALCH", 0, 0x901, &[sub(b"EDID", z("W6D9Stim")), sub(b"ENIT", enit)]);
    let misc = record(b"MISC", 0, 0x902, &[sub(b"EDID", z("W6D9Misc")), sub(b"DATA", [10i32.to_le_bytes(), 1f32.to_le_bytes()].concat())]);
    let bytes = [tes4(), top(b"ALCH", &alch), top(b"MISC", &misc)].concat();
    let f = build_fixture(&bytes, "W6d9Val.esm");
    let mut g = game(&f);
    let res = q(
        &mut g,
        "int s0\nlet s0 := GetValueAlt W6D9Stim\nSetValueAlt W6D9Stim 123\nint s1\nlet s1 := GetValueAlt W6D9Stim\nSetValueAlt W6D9Misc 444\nint m1\nlet m1 := GetValueAlt W6D9Misc\nSetFunctionValue (s0 * 1000000 + s1 * 1000 + m1)",
    );
    assert_eq!(res, XVal::Num(75_123_444.0));
}

#[test]
fn nth_effect_queries_add_remove_and_traits_work_on_cloned_spel_and_ench_isolated_from_originals() {
    const MGEF1: u32 = 0x1101;
    const MGEF2: u32 = 0x1102;
    const SPEL: u32 = 0x1103;
    const ENCH: u32 = 0x1104;
    let efit_sp = [u32s(&[10, 0, 0, 0]), 16i32.to_le_bytes().to_vec()].concat();
    let efit_en = [u32s(&[5, 0, 0, 0]), 16i32.to_le_bytes().to_vec()].concat();
    let spit = [u32s(&[4, 0, 0]), vec![0u8; 4]].concat();
    let enit = [u32s(&[3, 0, 0]), vec![0u8; 4]].concat();
    let bytes = [
        tes4(),
        top(b"MGEF", &[mgef_rec(MGEF1, "ClEff1", 0x2, 0, 16), mgef_rec(MGEF2, "ClEff2", 0x2, 0, 16)].concat()),
        top(b"SPEL", &record(b"SPEL", 0, SPEL, &[sub(b"EDID", z("ClSpel")), sub(b"SPIT", spit), sub(b"EFID", u32s(&[MGEF1])), sub(b"EFIT", efit_sp)])),
        top(b"ENCH", &record(b"ENCH", 0, ENCH, &[sub(b"EDID", z("ClEnch")), sub(b"ENIT", enit), sub(b"EFID", u32s(&[MGEF1])), sub(b"EFIT", efit_en)])),
    ]
    .concat();
    let f = build_fixture(&bytes, "W6d12ClonedEff.esm");
    let mut g = game(&f);
    let src = r#"
ref rSp = TempCloneForm ClSpel
int n0 = GetNumEffects rSp
ref b0 = GetNthEffectBase rSp 0
float m0 = GetNthEffectTraitNumeric rSp 0 0
int remL = RemoveNthEffect rSp 0
int addP = AddNewEffect rSp ClEff2 30 25 0 0 0
int n1 = GetNumEffects rSp
ref e0 = GetNthEffectBase rSp 0
float em0 = GetNthEffectTraitNumeric rSp 0 0
int addA = AddNewEffect rSp ClEff2 10 5 0 0 1
int n2 = GetNumEffects rSp
int rem0 = RemoveNthEffect rSp 0
int origN = GetNumEffects ClSpel
ref rEn = TempCloneForm ClEnch
int en0 = GetNumEffects rEn
int enAdd = AddNewEffect rEn ClEff2 0 42 0 0 1
int en1 = GetNumEffects rEn
float enM1 = GetNthEffectTraitNumeric rEn 1 0
float origM = GetNthEffectTraitNumeric ClEnch 0 0
int origEnN = GetNumEffects ClEnch
string_var s = sv_Construct "%.0f|%i|%.0f|%.0f|%.0f|%.0f|%i|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f" n0 b0 m0 remL addP n1 e0 em0 addA n2 rem0 origN en0 enAdd en1 enM1 origM origEnN
SetFunctionValue s
"#;
    let out = q(&mut g, src);
    let expected = format!("1|{MGEF1:08X}|10|-1|2|2|{MGEF2:08X}|25|2|3|2|1|1|1|2|42|5|1");
    assert_eq!(out, XVal::Str(expected));
}

#[test]
fn setimpactdatasetform_validates_ipct_keeps_on_bad_or_null_and_clears_on_omitted() {
    const IPDS: u32 = 0x1601;
    const IPCT1: u32 = 0x1602;
    const IPCT2: u32 = 0x1603;
    const MISC: u32 = 0x1604;
    let mut data = vec![0u8; 48];
    data[0..4].copy_from_slice(&IPCT1.to_le_bytes());
    let bytes = [
        tes4(),
        top(b"IPCT", &[record(b"IPCT", 0, IPCT1, &[sub(b"EDID", z("Ipct1"))]), record(b"IPCT", 0, IPCT2, &[sub(b"EDID", z("Ipct2"))])].concat()),
        top(b"IPDS", &record(b"IPDS", 0, IPDS, &[sub(b"EDID", z("Ipds1")), sub(b"DATA", data)])),
        top(b"MISC", &record(b"MISC", 0, MISC, &[sub(b"EDID", z("IpMisc"))])),
    ]
    .concat();
    let f = build_fixture(&bytes, "W6d12Ipds.esm");
    let mut g = game(&f);
    assert_eq!(q(&mut g, "SetFunctionValue (GetImpactDataSetForm Ipds1 0)"), XVal::Ref(IPCT1));
    q(&mut g, "SetImpactDataSetForm Ipds1 0 Ipct2");
    assert_eq!(q(&mut g, "SetFunctionValue (GetImpactDataSetForm Ipds1 0)"), XVal::Ref(IPCT2));
    q(&mut g, "SetImpactDataSetForm Ipds1 0 IpMisc");
    assert_eq!(q(&mut g, "SetFunctionValue (GetImpactDataSetForm Ipds1 0)"), XVal::Ref(IPCT2));
    q(&mut g, "SetImpactDataSetForm Ipds1 0 0");
    assert_eq!(q(&mut g, "SetFunctionValue (GetImpactDataSetForm Ipds1 0)"), XVal::Ref(IPCT2));
    q(&mut g, "SetImpactDataSetForm Ipds1 0");
    assert_eq!(q(&mut g, "SetFunctionValue (GetImpactDataSetForm Ipds1 0)"), XVal::Ref(0));
}

#[test]
fn getbaseeffectarchetype_returns_minus_one_for_non_mgef_and_null() {
    let bytes = [
        tes4(),
        top(b"MGEF", &[mgef_rec(0xE01, "TestArchVal", 0, 0, -1), mgef_rec(0xE02, "TestArchScr", 0, 1, -1)].concat()),
        top(b"MISC", &record(b"MISC", 0, 0xE03, &[sub(b"EDID", z("TestArchMisc"))])),
    ]
    .concat();
    let f = build_fixture(&bytes, "W6d12Arch.esm");
    let mut g = game(&f);
    assert_eq!(qn(&mut g, "GetBaseEffectArchetype TestArchVal"), 0.0);
    assert_eq!(qn(&mut g, "GetBaseEffectArchetype TestArchScr"), 1.0);
    assert_eq!(qn(&mut g, "GetBaseEffectArchetype TestArchMisc"), -1.0);
    assert_eq!(qn(&mut g, "GetBaseEffectArchetype 0"), -1.0);
}

#[test]
fn setprojectileexplosion_clears_on_wrong_type_and_omitted_but_keeps_on_null() {
    const PROJ: u32 = 0x1201;
    const EXPL: u32 = 0x1202;
    const MISC: u32 = 0x1203;
    let bytes = [
        tes4(),
        top(b"PROJ", &record(b"PROJ", 0, PROJ, &[sub(b"EDID", z("PeProj")), sub(b"DATA", vec![0u8; 84])])),
        top(b"EXPL", &record(b"EXPL", 0, EXPL, &[sub(b"EDID", z("PeExpl"))])),
        top(b"MISC", &record(b"MISC", 0, MISC, &[sub(b"EDID", z("PeMisc"))])),
    ]
    .concat();
    let f = build_fixture(&bytes, "W6d12ProjExpl.esm");
    let mut g = game(&f);
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileExplosion PeProj)"), XVal::Ref(0));
    q(&mut g, "SetProjectileExplosion PeProj PeExpl");
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileExplosion PeProj)"), XVal::Ref(EXPL));
    q(&mut g, "SetProjectileExplosion PeProj PeMisc");
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileExplosion PeProj)"), XVal::Ref(0));
    q(&mut g, "SetProjectileExplosion PeProj PeExpl\nSetProjectileExplosion PeProj 0");
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileExplosion PeProj)"), XVal::Ref(EXPL));
    q(&mut g, "SetProjectileExplosion PeProj");
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileExplosion PeProj)"), XVal::Ref(0));
}

#[test]
fn getispoison_rejects_non_alch_and_tracks_dynamic_addneweffect() {
    const MGEF_HARM: u32 = 0x1701;
    const MGEF_HEAL: u32 = 0x1702;
    const ALCH_POIS: u32 = 0x1703;
    const SPEL_HARM: u32 = 0x1704;
    const ENCH_HARM: u32 = 0x1705;
    let efit = [u32s(&[10, 0, 5, 0]), 16i32.to_le_bytes().to_vec()].concat();
    let enit_alch = [u32s(&[0]), vec![0u8; 16]].concat();
    let spit = [u32s(&[5, 0, 0]), vec![0u8; 4]].concat();
    let enit_ench = [u32s(&[2, 0, 0]), vec![0u8; 4]].concat();
    let bytes = [
        tes4(),
        top(b"MGEF", &[mgef_rec(MGEF_HARM, "GpHarm", 0x5, 0, 16), mgef_rec(MGEF_HEAL, "GpHeal", 0x0, 0, 16)].concat()),
        top(b"ALCH", &record(b"ALCH", 0, ALCH_POIS, &[sub(b"EDID", z("GpPois")), sub(b"ENIT", enit_alch), sub(b"EFID", u32s(&[MGEF_HARM])), sub(b"EFIT", efit.clone())])),
        top(b"SPEL", &record(b"SPEL", 0, SPEL_HARM, &[sub(b"EDID", z("GpSpel")), sub(b"SPIT", spit), sub(b"EFID", u32s(&[MGEF_HARM])), sub(b"EFIT", efit.clone())])),
        top(b"ENCH", &record(b"ENCH", 0, ENCH_HARM, &[sub(b"EDID", z("GpEnch")), sub(b"ENIT", enit_ench), sub(b"EFID", u32s(&[MGEF_HARM])), sub(b"EFIT", efit)])),
    ]
    .concat();
    let f = build_fixture(&bytes, "W6d12Pois.esm");
    let mut g = game(&f);
    assert_eq!(qn(&mut g, "GetIsPoison GpPois"), 1.0);
    assert_eq!(qn(&mut g, "GetIsPoison GpSpel"), 0.0);
    assert_eq!(qn(&mut g, "GetIsPoison GpEnch"), 0.0);
    let cl_flip = q(
        &mut g,
        "ref rCl\nlet rCl := TempCloneForm GpPois\nint a0\nlet a0 := GetIsPoison rCl\nAddNewEffect rCl GpHeal 5 10 0 0 1\nint a1\nlet a1 := GetIsPoison rCl\nSetFunctionValue (a0 * 10 + a1)",
    );
    assert_eq!(cl_flip, XVal::Num(10.0));
    assert_eq!(qn(&mut g, "GetIsPoison GpPois"), 1.0);
    q(&mut g, "AddNewEffect GpPois GpHeal 5 10 0 0 1");
    assert_eq!(qn(&mut g, "GetIsPoison GpPois"), 0.0);
}
