//! Scoped Wave 6D regression tests for `nvse_items.rs` (`GetHeadingAngleX`,
//! `GetEquippedCurrentHealth`, `GetAgeClass`, `RemoveMeIR`) and `nvse_rest.rs`
//! (`SetEyes`, `SetNthPackage`).

use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn build_fixture(bytes: Vec<u8>, name: &str) -> Fixture {
    let plugins = [Plugin::parse(&bytes, name.into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

#[test]
fn get_heading_angle_x_returns_vertical_pitch_independent_of_caller_angle() {
    const NPC_ID: u32 = 0x901;
    const NPC_REF: u32 = 0x902;
    const CELL_ID: u32 = 0x903;
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, NPC_ID, &[sub(b"EDID", z("TestNpc"))]),
    ]
    .concat();
    let cell = record(b"CELL", 0, CELL_ID, &[sub(b"EDID", z("PitchCell")), sub(b"DATA", vec![1])]);
    let npc_ref = record(
        b"ACHR",
        0,
        NPC_REF,
        &[sub(b"EDID", z("TestNpcREF")), sub(b"NAME", NPC_ID.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
    );
    let children = group(CELL_ID.to_le_bytes(), 6, &group(CELL_ID.to_le_bytes(), 9, &npc_ref));
    let raw = [
        tes4(),
        top(b"NPC_", &npcs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat();
    let f = build_fixture(raw, "HeadingPitch.esm");
    let mut g = game(&f);
    g.player_entered(Some(FormId(CELL_ID)));
    let q = |g: &mut Game, body: &str| match g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), Default::default()).unwrap() {
        XVal::Num(x) => x,
        v => panic!("{body}: {v:?}"),
    };
    // Place PlayerREF at (0, 0, 0) and TestNpcREF at (100, 0, 100) -> -45 deg.
    q(&mut g, "PlayerREF.SetPos X 0\nPlayerREF.SetPos Y 0\nPlayerREF.SetPos Z 0\nTestNpcREF.SetPos X 100\nTestNpcREF.SetPos Y 0\nTestNpcREF.SetPos Z 100");
    for pitch in [0.0, 30.0, -30.0] {
        q(&mut g, &format!("PlayerREF.SetAngle X {pitch}"));
        let a = q(&mut g, "SetFunctionValue (PlayerREF.GetHeadingAngleX TestNpcREF)");
        assert!((a - (-45.0)).abs() < 1e-3, "above at X={pitch}: {a}");
    }
    // Reverse (target below caller) -> +45 deg.
    let rev = q(&mut g, "SetFunctionValue (TestNpcREF.GetHeadingAngleX PlayerREF)");
    assert!((rev - 45.0).abs() < 1e-3, "below: {rev}");
    // Level -> 0 deg; self-target -> safe 0.0 [design].
    q(&mut g, "TestNpcREF.SetPos Z 0");
    assert!(q(&mut g, "SetFunctionValue (PlayerREF.GetHeadingAngleX TestNpcREF)").abs() < 1e-6);
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.GetHeadingAngleX PlayerREF)"), 0.0);
}

#[test]
fn get_equipped_current_health_returns_100000_on_oob_slots() {
    const CELL_ID: u32 = 0x908;
    let npc = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let cell = record(b"CELL", 0, CELL_ID, &[sub(b"EDID", z("HealthCell")), sub(b"DATA", vec![1])]);
    let raw = [
        tes4(),
        top(b"NPC_", &npc),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat();
    let f = build_fixture(raw, "EqHealth.esm");
    let mut g = game(&f);
    g.player_entered(Some(FormId(CELL_ID)));
    let q = |g: &mut Game, expr: &str| g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue ({expr})\nEnd\n"), Default::default()).unwrap();
    assert_eq!(q(&mut g, "PlayerREF.GetEquippedCurrentHealth 0"), XVal::Num(0.0), "valid empty slot 0");
    assert_eq!(q(&mut g, "PlayerREF.GetEquippedCurrentHealth 5"), XVal::Num(0.0), "valid empty weapon slot 5");
    assert_eq!(q(&mut g, "PlayerREF.GetEquippedCurrentHealth -1"), XVal::Num(100_000.0), "negative slot");
    assert_eq!(q(&mut g, "PlayerREF.GetEquippedCurrentHealth 20"), XVal::Num(100_000.0), "slot 20 oob");
    assert_eq!(q(&mut g, "PlayerREF.GetEquippedCurrentHealth 99"), XVal::Num(100_000.0), "slot 99 oob");
}

#[test]
fn get_age_class_resolves_race_onam_ynam_links() {
    const R_CHILD: u32 = 0x910;
    const R_ADULT: u32 = 0x911;
    const R_OLD: u32 = 0x912;
    const R_AGELESS: u32 = 0x913;
    const N_CHILD: u32 = 0x914;
    const N_ADULT: u32 = 0x915;
    const N_OLD: u32 = 0x916;
    const N_AGELESS: u32 = 0x917;
    const REF_CHILD: u32 = 0x918;
    const REF_ADULT: u32 = 0x919;
    const REF_OLD: u32 = 0x91A;
    const REF_AGELESS: u32 = 0x91B;
    const CONT_BASE: u32 = 0x91C;
    const CONT_REF: u32 = 0x91D;
    const CELL_ID: u32 = 0x91E;
    let races = [
        record(b"RACE", 0, R_CHILD, &[sub(b"EDID", z("RaceChild")), sub(b"ONAM", R_ADULT.to_le_bytes().to_vec())]),
        record(b"RACE", 0, R_ADULT, &[sub(b"EDID", z("RaceAdult")), sub(b"ONAM", R_OLD.to_le_bytes().to_vec()), sub(b"YNAM", R_CHILD.to_le_bytes().to_vec())]),
        record(b"RACE", 0, R_OLD, &[sub(b"EDID", z("RaceOld")), sub(b"YNAM", R_ADULT.to_le_bytes().to_vec())]),
        record(b"RACE", 0, R_AGELESS, &[sub(b"EDID", z("RaceAgeless"))]),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"RNAM", R_ADULT.to_le_bytes().to_vec())]),
        record(b"NPC_", 0, N_CHILD, &[sub(b"EDID", z("NpcChild")), sub(b"RNAM", R_CHILD.to_le_bytes().to_vec())]),
        record(b"NPC_", 0, N_ADULT, &[sub(b"EDID", z("NpcAdult")), sub(b"RNAM", R_ADULT.to_le_bytes().to_vec())]),
        record(b"NPC_", 0, N_OLD, &[sub(b"EDID", z("NpcOld")), sub(b"RNAM", R_OLD.to_le_bytes().to_vec())]),
        record(b"NPC_", 0, N_AGELESS, &[sub(b"EDID", z("NpcAgeless")), sub(b"RNAM", R_AGELESS.to_le_bytes().to_vec())]),
    ]
    .concat();
    let cont = record(b"CONT", 0, CONT_BASE, &[sub(b"EDID", z("TestDresser"))]);
    let cell = record(b"CELL", 0, CELL_ID, &[sub(b"EDID", z("AgeCell")), sub(b"DATA", vec![1])]);
    let mk_ref = |sig: &[u8; 4], fid: u32, name: &str, base: u32| {
        record(sig, 0, fid, &[sub(b"EDID", z(name)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
    };
    let refs_raw = [
        mk_ref(b"ACHR", REF_CHILD, "RefChild", N_CHILD),
        mk_ref(b"ACHR", REF_ADULT, "RefAdult", N_ADULT),
        mk_ref(b"ACHR", REF_OLD, "RefOld", N_OLD),
        mk_ref(b"ACHR", REF_AGELESS, "RefAgeless", N_AGELESS),
        mk_ref(b"REFR", CONT_REF, "RefDresser", CONT_BASE),
    ]
    .concat();
    let children = group(CELL_ID.to_le_bytes(), 6, &group(CELL_ID.to_le_bytes(), 9, &refs_raw));
    let raw = [
        tes4(),
        top(b"RACE", &races),
        top(b"NPC_", &npcs),
        top(b"CONT", &cont),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat();
    let f = build_fixture(raw, "AgeClass.esm");
    let mut g = game(&f);
    g.player_entered(Some(FormId(CELL_ID)));
    let q = |g: &mut Game, expr: &str| g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue ({expr})\nEnd\n"), Default::default()).unwrap();
    assert_eq!(q(&mut g, "RefChild.GetAgeClass"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "PlayerREF.GetAgeClass"), XVal::Num(1.0));
    assert_eq!(q(&mut g, "RefAdult.GetAgeClass"), XVal::Num(1.0));
    assert_eq!(q(&mut g, "RefOld.GetAgeClass"), XVal::Num(2.0));
    assert_eq!(q(&mut g, "RefAgeless.GetAgeClass"), XVal::Num(-1.0));
    assert_eq!(q(&mut g, "RefDresser.GetAgeClass"), XVal::Num(-1.0));
}

#[test]
fn remove_me_ir_returns_one_with_or_without_target() {
    const GUN_ID: u32 = 0x950;
    const NPC_ID: u32 = 0x951;
    const NPC_REF: u32 = 0x952;
    const CELL_ID: u32 = 0x953;
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, NPC_ID, &[sub(b"EDID", z("IrTargetNpc"))]),
    ]
    .concat();
    let weap = record(b"WEAP", 0, GUN_ID, &[sub(b"EDID", z("IrGun")), sub(b"DATA", vec![0; 15])]);
    let cell = record(b"CELL", 0, CELL_ID, &[sub(b"EDID", z("IrCell")), sub(b"DATA", vec![1])]);
    let npc_ref = record(b"ACHR", 0, NPC_REF, &[sub(b"EDID", z("IrTargetRef")), sub(b"NAME", NPC_ID.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let children = group(CELL_ID.to_le_bytes(), 6, &group(CELL_ID.to_le_bytes(), 9, &npc_ref));
    let raw = [
        tes4(),
        top(b"NPC_", &npcs),
        top(b"WEAP", &weap),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat();
    let f = build_fixture(raw, "RemoveMeIr.esm");
    let mut g = game(&f);
    g.player_entered(Some(FormId(CELL_ID)));
    g.world.add_item(FormId::PLAYER_REF, FormId(GUN_ID), 2).unwrap();
    let XVal::Str(out1) = g
        .run_query(
            "ref rIr\nint rDel\nint cLeft\nBegin Function { }\nlet rIr := PlayerREF.GetFirstRefForItem IrGun\nlet rDel := rIr.RemoveMeIR\nlet cLeft := PlayerREF.GetItemCount IrGun\nSetFunctionValue (sv_Construct \"%g|%g\" rDel cLeft)\nEnd\n",
            Default::default(),
        )
        .unwrap()
    else {
        panic!("expected string")
    };
    assert_eq!(out1, "1|0", "delete without target returns 1 and removes stack");
    g.world.add_item(FormId::PLAYER_REF, FormId(GUN_ID), 3).unwrap();
    let XVal::Str(out2) = g
        .run_query(
            "ref rIr\nint rMov\nint cNpc\nBegin Function { }\nlet rIr := PlayerREF.GetFirstRefForItem IrGun\nlet rMov := rIr.RemoveMeIR IrTargetRef\nlet cNpc := IrTargetRef.GetItemCount IrGun\nSetFunctionValue (sv_Construct \"%g|%g\" rMov cNpc)\nEnd\n",
            Default::default(),
        )
        .unwrap()
    else {
        panic!("expected string")
    };
    assert_eq!(out2, "1|3", "move to target returns 1 and transfers stack");
}

#[test]
fn set_eyes_accepts_cloned_npc_and_rejects_non_eyes_form() {
    const EYE_ID: u32 = 0x940;
    const HAIR_ID: u32 = 0x941;
    const NPC_ID: u32 = 0x942;
    const CELL_ID: u32 = 0x944;
    let eyes = record(b"EYES", 0, EYE_ID, &[sub(b"EDID", z("W6d4Eye"))]);
    let hair = record(b"HAIR", 0, HAIR_ID, &[sub(b"EDID", z("W6d4Hair"))]);
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, NPC_ID, &[sub(b"EDID", z("W6d4Npc"))]),
    ]
    .concat();
    let cell = record(b"CELL", 0, CELL_ID, &[sub(b"EDID", z("W6d4Cell")), sub(b"DATA", vec![1])]);
    let raw = [
        tes4(),
        top(b"EYES", &eyes),
        top(b"HAIR", &hair),
        top(b"NPC_", &npcs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat();
    let f = build_fixture(raw, "SetEyes.esm");
    let mut g = game(&f);
    g.player_entered(Some(FormId(CELL_ID)));
    let q = |g: &mut Game, body: &str, ctx: crate::game::RunCtx| {
        g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue ({body})\nEnd\n"), ctx).unwrap()
    };
    let dflt = crate::game::RunCtx::default;
    assert_eq!(q(&mut g, "PlayerREF.SetEyes W6d4Eye", dflt()), XVal::Num(1.0));
    assert_eq!(q(&mut g, "PlayerREF.GetEyes", dflt()), XVal::Ref(EYE_ID));
    assert_eq!(q(&mut g, "PlayerREF.SetEyes W6d4Hair", dflt()), XVal::Num(0.0));
    assert_eq!(q(&mut g, "PlayerREF.GetEyes", dflt()), XVal::Ref(EYE_ID));
    let XVal::Ref(cl) = q(&mut g, "TempCloneForm W6d4Npc", dflt()) else { panic!("clone ref") };
    let r_ref = g.world.place(FormId(NPC_ID), Some(FormId(CELL_ID)), [0.0; 3], [0.0; 3], 1).unwrap();
    g.world.get(r_ref).unwrap().base = FormId(cl);
    let cctx = || crate::game::RunCtx { self_ref: Some(r_ref), ..Default::default() };
    assert_eq!(q(&mut g, "SetEyes W6d4Eye", cctx()), XVal::Num(1.0));
    assert_eq!(q(&mut g, "GetEyes", cctx()), XVal::Ref(EYE_ID));
    assert_eq!(q(&mut g, "SetEyes W6d4Hair", cctx()), XVal::Num(0.0));
    assert_eq!(q(&mut g, "GetEyes", cctx()), XVal::Ref(EYE_ID));
}

#[test]
fn set_nth_package_validates_pack_and_supports_neg_one() {
    const P1: u32 = 0x930;
    const P2: u32 = 0x931;
    const P3: u32 = 0x932;
    const NPC_ID: u32 = 0x933;
    const NPC_REF: u32 = 0x934;
    const NOT_PACK: u32 = 0x935;
    const CELL_ID: u32 = 0x936;
    let packs = [
        record(b"PACK", 0, P1, &[sub(b"EDID", z("PkgOne"))]),
        record(b"PACK", 0, P2, &[sub(b"EDID", z("PkgTwo"))]),
        record(b"PACK", 0, P3, &[sub(b"EDID", z("PkgThree"))]),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, NPC_ID, &[sub(b"EDID", z("PkgNpc")), sub(b"PKID", P1.to_le_bytes().to_vec()), sub(b"PKID", P2.to_le_bytes().to_vec())]),
    ]
    .concat();
    let misc = record(b"MISC", 0, NOT_PACK, &[sub(b"EDID", z("NotAPkg"))]);
    let cell = record(b"CELL", 0, CELL_ID, &[sub(b"EDID", z("PkgCell")), sub(b"DATA", vec![1])]);
    let npc_ref = record(b"ACHR", 0, NPC_REF, &[sub(b"EDID", z("PkgNpcRef")), sub(b"NAME", NPC_ID.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let children = group(CELL_ID.to_le_bytes(), 6, &group(CELL_ID.to_le_bytes(), 9, &npc_ref));
    let raw = [
        tes4(),
        top(b"PACK", &packs),
        top(b"NPC_", &npcs),
        top(b"MISC", &misc),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat();
    let f = build_fixture(raw, "SetNthPkg.esm");
    let mut g = game(&f);
    g.player_entered(Some(FormId(CELL_ID)));
    let q = |g: &mut Game, body: &str| g.run_query(&format!("ref r\nBegin Function {{ }}\n{body}\nEnd\n"), Default::default()).unwrap();
    assert_eq!(q(&mut g, "let r := PkgNpcRef.SetNthPackage PkgThree 99\nSetFunctionValue r"), XVal::Ref(0));
    assert_eq!(q(&mut g, "let r := PkgNpcRef.SetNthPackage NotAPkg 0\nSetFunctionValue r"), XVal::Ref(0));
    assert_eq!(q(&mut g, "SetFunctionValue (PkgNpcRef.GetNthPackage 0)"), XVal::Ref(P1));
    assert_eq!(q(&mut g, "let r := PkgNpcRef.SetNthPackage PkgThree -1\nSetFunctionValue r"), XVal::Ref(P2));
    assert_eq!(q(&mut g, "SetFunctionValue (PkgNpcRef.GetNthPackage 1)"), XVal::Ref(P3));
    assert_eq!(q(&mut g, "let r := PlayerREF.SetNthPackage PkgThree -1\nSetFunctionValue r"), XVal::Ref(0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerREF.GetPackageCount)"), XVal::Num(0.0));
    let nan_res = g
        .run_query(
            "float f\nref r\nBegin Function { }\nlet f := IsNumberNAN\nlet r := PkgNpcRef.SetNthPackage PkgOne f\nSetFunctionValue r\nEnd\n",
            Default::default(),
        )
        .unwrap();
    assert_eq!(nan_res, XVal::Ref(0));
    assert_eq!(q(&mut g, "SetFunctionValue (PkgNpcRef.GetNthPackage 0)"), XVal::Ref(P1));
}
