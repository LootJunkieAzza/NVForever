//! Wave 6D JIP LN NVSE & Lutana parity regression tests (Rule A5 feature-scoped suite).
//!
//! Covers consolidated Wave 6D Draft PRs for JIP/Lutana record, lighting, ammo,
//! weapon-model, class, faction-reputation, VATS-effect, and perk-modifier parity.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::data::EntryPointDef;
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::perks;
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn edid(sig: &[u8; 4], fid: u32, name: &str, rest: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    record(sig, 0, fid, &[vec![sub(b"EDID", z(name))], rest.to_vec()].concat())
}

fn with_plugin<F>(name: &str, bytes: &[u8], f: F)
where
    F: FnOnce(&mut Game),
{
    let plugins = vec![Plugin::parse(bytes, name.into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });
    f(&mut g);
}

fn run(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default())
        .unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn num_val(g: &mut Game, expr: &str) -> f64 {
    match run(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        XVal::Ref(r) => f64::from(r),
        v => panic!("{expr}: {v:?}"),
    }
}

fn ref_val(g: &mut Game, expr: &str) -> u32 {
    match run(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Ref(r) => r,
        XVal::Num(x) => x as u32,
        v => panic!("{expr}: {v:?}"),
    }
}

/// Wave 6D-2 `w6d2p5_01_faction_reputation_validate` [observed]: `SetFactionReputationType`
/// treats non-`REPU` and explicit `0` as no-ops and clears `WMI1` only when arg 1 is omitted
/// (`FACT`-target validation and `TempCloneForm` `unclone()` handling are [design]/[hypothesis]).
#[test]
fn set_faction_reputation_type_validates_repu_and_omitted_clear() {
    const FACT_ID: u32 = 0x9A0;
    const REPU_A: u32 = 0x9A1;
    const REPU_B: u32 = 0x9A2;
    const NOT_REPU: u32 = 0x9A3;

    let npc_player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let fact = record(b"FACT", 0, FACT_ID, &[sub(b"EDID", z("TestRepFaction")), sub(b"WMI1", REPU_A.to_le_bytes().to_vec())]);
    let repus = [
        record(b"REPU", 0, REPU_A, &[sub(b"EDID", z("TestRepuA"))]),
        record(b"REPU", 0, REPU_B, &[sub(b"EDID", z("TestRepuB"))]),
    ]
    .concat();
    let misc = record(b"MISC", 0, NOT_REPU, &[sub(b"EDID", z("TestNotRepu"))]);
    let bytes = [tes4(), top(b"NPC_", &npc_player), top(b"FACT", &fact), top(b"REPU", &repus), top(b"MISC", &misc)].concat();

    with_plugin("RepFact.esm", &bytes, |g| {
        assert_eq!(ref_val(g, "GetFactionReputationType TestRepFaction"), REPU_A);
        run(g, "SetFactionReputationType TestRepFaction TestNotRepu");
        assert_eq!(ref_val(g, "GetFactionReputationType TestRepFaction"), REPU_A);
        run(g, "SetFactionReputationType TestRepFaction 0");
        assert_eq!(ref_val(g, "GetFactionReputationType TestRepFaction"), REPU_A);
        run(g, "SetFactionReputationType TestRepFaction TestRepuB");
        assert_eq!(ref_val(g, "GetFactionReputationType TestRepFaction"), REPU_B);
        run(g, "SetFactionReputationType TestRepFaction");
        assert_eq!(ref_val(g, "GetFactionReputationType TestRepFaction"), 0);
        // Non-FACT target is ignored without creating a fake override.
        run(g, "SetFactionReputationType TestNotRepu TestRepuA");
        assert_eq!(ref_val(g, "GetFactionReputationType TestNotRepu"), 0);
    });
}

/// `SetLightTraitNumeric` validates `LIGH`, wraps red (`1`) modulo 256, and preserves signed
/// radius (`0`) [observed: `w6d2_22_light_grass_txst_traits_and_flags`]; channels `2..=3`
/// (`g`/`b`) wrap and `.trunc()` are [hypothesis], and `raw.is_finite()` is [design].
#[test]
fn set_light_trait_numeric_wraps_rgb_and_validates_ligh() {
    const LIGHT: u32 = 0xC00;
    const LGTM: u32 = 0xB00;
    let npc_player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let mut ligh_data = Vec::new();
    ligh_data.extend(0i32.to_le_bytes());
    ligh_data.extend(200u32.to_le_bytes());
    ligh_data.extend([177u8, 100, 50, 0]);
    ligh_data.extend(0u32.to_le_bytes());
    ligh_data.extend(1.0f32.to_le_bytes());
    ligh_data.extend(90.0f32.to_le_bytes());
    ligh_data.extend(0u32.to_le_bytes());
    ligh_data.extend(0.0f32.to_le_bytes());
    let ligh = record(b"LIGH", 0, LIGHT, &[sub(b"EDID", z("OwnedLightForm")), sub(b"DATA", ligh_data)]);
    let lgtm = record(b"LGTM", 0, LGTM, &[sub(b"EDID", z("OwnedLightA")), sub(b"DATA", vec![0u8; 40])]);
    let bytes = [tes4(), top(b"NPC_", &npc_player), top(b"LIGH", &ligh), top(b"LGTM", &lgtm)].concat();

    with_plugin("LightTrait.esm", &bytes, |g| {
        assert_eq!(run(g, "SetFunctionValue (GetLightTraitNumeric OwnedLightForm 0)"), XVal::Num(200.0));
        assert_eq!(run(g, "SetFunctionValue (GetLightTraitNumeric OwnedLightForm 1)"), XVal::Num(177.0));
        run(g, "SetLightTraitNumeric OwnedLightForm 0 -50\nSetLightTraitNumeric OwnedLightForm 1 300");
        assert_eq!(run(g, "SetFunctionValue (GetLightTraitNumeric OwnedLightForm 0)"), XVal::Num(-50.0));
        assert_eq!(run(g, "SetFunctionValue (GetLightTraitNumeric OwnedLightForm 1)"), XVal::Num(44.0));
        run(g, "SetLightTraitNumeric OwnedLightForm 1 -10");
        assert_eq!(run(g, "SetFunctionValue (GetLightTraitNumeric OwnedLightForm 1)"), XVal::Num(246.0));
        // Non-LIGH form (LGTM) is ignored without mutation.
        run(g, "SetLightTraitNumeric OwnedLightA 0 99");
        assert_eq!(run(g, "SetFunctionValue (GetLightTraitNumeric OwnedLightA 0)"), XVal::Num(0.0));
    });
}

/// `SetWeaponModel` accepts `TempCloneForm` `WEAP` clones (`cl=[weapons\w6d2_clone.nif]`)
/// [observed: `w6d2p3_02_weapon_model_paths_and_clone`]; isolating clone overrides from the
/// original weapon is [hypothesis]/[design] (`origEq=0` in the probe reflected literal `"s0"`).
#[test]
fn set_weapon_model_accepts_tempclone_and_isolates_original() {
    const WEAP_ID: u32 = 0x9C0;
    let npc_player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let weap = edid(b"WEAP", WEAP_ID, "TestGun", &[sub(b"MODL", z("weapons\\base_gun.nif"))]);
    let bytes = [tes4(), top(b"NPC_", &npc_player), top(b"WEAP", &weap)].concat();

    with_plugin("WeapModel.esm", &bytes, |g| {
        run(g, r#"SetWeaponModel TestGun 0 "weapons\orig_test.nif""#);
        let XVal::Ref(cl) = run(g, "SetFunctionValue (TempCloneForm TestGun 0)") else { panic!("clone ref") };
        assert_ne!(cl, 0);
        run(g, &format!(r#"SetWeaponModel (StringToRef "{cl:08X}") 0 "weapons\w6d2_clone.nif""#));
        assert_eq!(
            run(g, &format!(r#"SetFunctionValue (GetWeaponModel (StringToRef "{cl:08X}") 0)"#)),
            XVal::Str("weapons\\w6d2_clone.nif".into())
        );
        assert_eq!(run(g, "SetFunctionValue (GetWeaponModel TestGun 0)"), XVal::Str("weapons\\orig_test.nif".into()));
    });
}

/// Wave 6D-2 `w6d2_04_weapon_ammo_and_mod_traits` [observed]: `SetWeaponVATSEffect` leaves
/// the existing VATS effect unchanged on a non-`SPEL` replacement (`rStim`); explicit null
/// `0` no-op, omitted arg 1 clear, and `WEAP`/`SPEL` `unclone()` handling are
/// [hypothesis]/[design].
#[test]
fn set_weapon_vats_effect_validates_spel_and_omitted_clear() {
    const WEAP_ID: u32 = 0x9B0;
    const SPEL_A: u32 = 0x9B1;
    const SPEL_B: u32 = 0x9B2;
    const NOT_SPEL: u32 = 0x9B3;

    let npc_player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let weap = edid(b"WEAP", WEAP_ID, "TestVatsWeap", &[]);
    let spels = [edid(b"SPEL", SPEL_A, "TestSpelA", &[]), edid(b"SPEL", SPEL_B, "TestSpelB", &[])].concat();
    let misc = edid(b"MISC", NOT_SPEL, "TestNotSpel", &[]);
    let bytes = [tes4(), top(b"NPC_", &npc_player), top(b"WEAP", &weap), top(b"SPEL", &spels), top(b"MISC", &misc)].concat();

    with_plugin("VatsEff.esm", &bytes, |g| {
        run(g, "SetWeaponVATSEffect TestVatsWeap TestSpelA");
        assert_eq!(num_val(g, "GetWeaponVATSEffect TestVatsWeap"), f64::from(SPEL_A));
        run(g, "SetWeaponVATSEffect TestVatsWeap TestNotSpel");
        assert_eq!(num_val(g, "GetWeaponVATSEffect TestVatsWeap"), f64::from(SPEL_A));
        run(g, "SetWeaponVATSEffect TestVatsWeap 0");
        assert_eq!(num_val(g, "GetWeaponVATSEffect TestVatsWeap"), f64::from(SPEL_A));
        run(g, "SetWeaponVATSEffect TestVatsWeap TestSpelB");
        assert_eq!(num_val(g, "GetWeaponVATSEffect TestVatsWeap"), f64::from(SPEL_B));
        run(g, "SetWeaponVATSEffect TestVatsWeap");
        assert_eq!(num_val(g, "GetWeaponVATSEffect TestVatsWeap"), 0.0);
    });
}

/// `SetClassTraitNumeric` returns `0` while updating the class trait
/// [observed: `w6d2p3_24_class_and_faction_reputation_and_ranks`].
#[test]
fn set_class_trait_numeric_returns_zero_and_updates_trait() {
    const CLASS_A: u32 = 0x9D0;
    let npc_player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let clas = edid(b"CLAS", CLASS_A, "ClassA", &[]);
    let bytes = [tes4(), top(b"NPC_", &npc_player), top(b"CLAS", &clas)].concat();

    with_plugin("ClassTrait.esm", &bytes, |g| {
        assert_eq!(num_val(g, "SetClassTraitNumeric ClassA 4 8"), 0.0);
        assert_eq!(num_val(g, "GetClassTraitNumeric ClassA 4"), 8.0);
        assert_eq!(num_val(g, "GetClassFlag ClassA 20"), 0.0);
    });
}

#[test]
fn set_ammo_casing_explicit_null_is_noop_and_omitted_arg_clears() {
    const PROJ_A: u32 = 0x801;
    const CASING_A: u32 = 0x803;
    const CASING_B: u32 = 0x804;
    const AMMO_A: u32 = 0x808;

    let npc_player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let proj = edid(b"PROJ", PROJ_A, "OwnedProjA", &[]);
    let casings = [
        record(b"MISC", 0, CASING_A, &[sub(b"EDID", z("OwnedCasingA")), sub(b"DATA", [1i32.to_le_bytes().as_slice(), &0.01f32.to_le_bytes()].concat())]),
        record(b"MISC", 0, CASING_B, &[sub(b"EDID", z("OwnedCasingB")), sub(b"DATA", [1i32.to_le_bytes().as_slice(), &0.01f32.to_le_bytes()].concat())]),
    ]
    .concat();
    let ammo_data = [1.25f32.to_le_bytes().as_slice(), &[0, 0, 0, 0], &10i32.to_le_bytes(), &[8]].concat();
    let ammo_dat2 = [1u32.to_le_bytes().as_slice(), &PROJ_A.to_le_bytes(), &0.02f32.to_le_bytes(), &CASING_A.to_le_bytes(), &1.0f32.to_le_bytes()].concat();
    let ammo = edid(b"AMMO", AMMO_A, "OwnedAmmoA", &[sub(b"DATA", ammo_data), sub(b"DAT2", ammo_dat2)]);
    let bytes = [tes4(), top(b"NPC_", &npc_player), top(b"PROJ", &proj), top(b"MISC", &casings), top(b"AMMO", &ammo)].concat();

    with_plugin("AmmoCasing.esm", &bytes, |g| {
        assert_eq!(run(g, "SetFunctionValue (GetAmmoCasing OwnedAmmoA)"), XVal::Ref(CASING_A));
        assert_eq!(g.ammo_traits(FormId(AMMO_A)).unwrap().casing, Some(FormId(CASING_A)));

        // Explicit null ref (rNull = 0) and direct host Ref(0)/Num(0.0) are no-ops
        // [observed: w6d2_11_ammo_proj_expl_clone_and_edges].
        run(g, "ref rNull\nlet rNull := 0\nSetAmmoCasing OwnedAmmoA rNull");
        assert_eq!(run(g, "SetFunctionValue (GetAmmoCasing OwnedAmmoA)"), XVal::Ref(CASING_A));
        assert_eq!(g.ammo_traits(FormId(AMMO_A)).unwrap().casing, Some(FormId(CASING_A)));
        fnv_game::nvse::call(g, &RunCtx::default(), "setammocasing", None, &[XVal::Ref(AMMO_A), XVal::Ref(0)]).unwrap().unwrap();
        fnv_game::nvse::call(g, &RunCtx::default(), "setammocasing", None, &[XVal::Ref(AMMO_A), XVal::Num(0.0)]).unwrap().unwrap();
        assert_eq!(g.ammo_traits(FormId(AMMO_A)).unwrap().casing, Some(FormId(CASING_A)));

        // Omitting the optional 2nd argument clears the casing to 0.
        run(g, "SetAmmoCasing OwnedAmmoA");
        assert_eq!(run(g, "SetFunctionValue (GetAmmoCasing OwnedAmmoA)"), XVal::Ref(0));
        assert_eq!(g.ammo_traits(FormId(AMMO_A)).unwrap().casing, None);

        // After setting a session casing (OwnedCasingB), explicit null ref leaves it intact and omit clears.
        run(g, "SetAmmoCasing OwnedAmmoA OwnedCasingB\nref rNull\nlet rNull := 0\nSetAmmoCasing OwnedAmmoA rNull");
        assert_eq!(run(g, "SetFunctionValue (GetAmmoCasing OwnedAmmoA)"), XVal::Ref(CASING_B));
        assert_eq!(g.ammo_traits(FormId(AMMO_A)).unwrap().casing, Some(FormId(CASING_B)));
        run(g, "SetAmmoCasing OwnedAmmoA");
        assert_eq!(run(g, "SetFunctionValue (GetAmmoCasing OwnedAmmoA)"), XVal::Ref(0));
        assert_eq!(g.ammo_traits(FormId(AMMO_A)).unwrap().casing, None);
    });
}

#[test]
fn getperkmodifier_passes_target_actor_as_tab0_other() {
    const CELL: u32 = 0x200;
    const NPC: u32 = 0x400;
    const OTHER_NPC: u32 = 0x401;
    const NPC_REF: u32 = 0x500;
    const OTHER_REF: u32 = 0x501;
    const REVOLVER: u32 = 0x600;

    let npc_data = [50i32.to_le_bytes().to_vec(), vec![5; 7]].concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", npc_data.clone())]),
        record(b"NPC_", 0, NPC, &[sub(b"EDID", z("TestNpc")), sub(b"DATA", npc_data.clone())]),
        record(b"NPC_", 0, OTHER_NPC, &[sub(b"EDID", z("OtherNpc")), sub(b"DATA", npc_data)]),
    ]
    .concat();
    let weap = edid(b"WEAP", REVOLVER, "Revolver", &[]);
    let placed = [
        record(b"ACHR", 0, NPC_REF, &[sub(b"EDID", z("NpcRef")), sub(b"NAME", u32s(&[NPC])), sub(b"DATA", vec![0; 24])]),
        record(b"ACHR", 0, OTHER_REF, &[sub(b"EDID", z("OtherRef")), sub(b"NAME", u32s(&[OTHER_NPC])), sub(b"DATA", vec![0; 24])]),
    ]
    .concat();
    let cell = [
        edid(b"CELL", CELL, "PerkRoom", &[sub(b"DATA", vec![1])]),
        group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &placed)),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"NPC_", &npcs),
        top(b"WEAP", &weap),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat();

    with_plugin("PerkMod.esm", &bytes, |g| {
        // NPC_REF has detected the player; OTHER_REF has not.
        g.detection.record(
            FormId(NPC_REF),
            FormId::PLAYER_REF,
            fnv_game::detection::Detection { band: fnv_game::detection::Band::Detected, ..Default::default() },
            0.0,
        );
        let mk_det_perk = |name: &str, rhs: f64| fnv_game::data::PerkDef {
            name: name.into(),
            playable: true,
            min_level: 1,
            ranks: 1,
            entries: vec![fnv_game::data::PerkEntry {
                effect: fnv_game::data::PerkEffect::EntryPoint(EntryPointDef {
                    point: perks::CALCULATE_MY_CRITICAL_HIT_DAMAGE,
                    function: perks::MULTIPLY_VALUE,
                    value: Some(1.2),
                    param_type: Some(1),
                    conditions: Some(vec![(0, fnv_game::cond::Condition {
                        cmp: fnv_game::cond::Cmp::Eq,
                        or: false,
                        run_on: fnv_game::cond::RunOn::Target,
                        function: 45,
                        params: vec![fnv_script::vm::ArgValue::Value(fnv_script::vm::Value::Ref(FormId::PLAYER_REF.0))],
                        rhs: fnv_game::cond::Operand::Value(rhs),
                    })]),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        };

        // Case 1: GetDetected(PlayerREF) == 0 (undetected target gets 1.2x; detected target stays 100.0).
        let p_prof = FormId(0x7B0);
        g.lu3.perk_edits.insert(p_prof, mk_det_perk("ProfTest", 0.0));
        perks::gain_rank(g, p_prof);
        let query_target = |g: &mut Game, target_ref: Option<u32>| {
            let call = match target_ref {
                Some(r) => format!("ref rNpc\nlet rNpc := {r:#x}\nSetFunctionValue (PlayerREF.GetPerkModifier 2 100 Revolver rNpc)"),
                None => "SetFunctionValue (PlayerREF.GetPerkModifier 2 100 Revolver)".into(),
            };
            match run(g, &call) {
                XVal::Num(v) => v,
                other => panic!("expected Num, got {other:?}"),
            }
        };
        assert!((query_target(g, Some(OTHER_REF)) - 120.0).abs() < 1e-4, "undetected target should trigger perk");
        assert!((query_target(g, Some(NPC_REF)) - 100.0).abs() < 1e-4, "detected target must fail GetDetected == 0");

        // Case 2: GetDetected(PlayerREF) == 1 (requires matching detected target ref; fails for
        // undetected target and when target ref is omitted so other == None).
        g.player.perks.clear();
        let p_seen = FormId(0x7B1);
        g.lu3.perk_edits.insert(p_seen, mk_det_perk("SeenTest", 1.0));
        perks::gain_rank(g, p_seen);
        assert!((query_target(g, Some(NPC_REF)) - 120.0).abs() < 1e-4, "detected target should pass GetDetected == 1");
        assert!((query_target(g, Some(OTHER_REF)) - 100.0).abs() < 1e-4, "undetected target must fail GetDetected == 1");
        assert!((query_target(g, None) - 100.0).abs() < 1e-4, "missing target ref must not match RunOn::Target");
    });
}
