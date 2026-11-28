//! Canonical `Game::projectile_traits`, `Game::projectile_trait_overrides`,
//! `Game::projectile_sound`, non-flag `PROJ` setter validation, and `TempCloneForm`
//! `PROJ` inheritance/snapshotting/isolation in `fnv-game`.

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use std::path::Path;

const PROJ_MAIN: u32 = 0x0000_0F00;
const PROJ_BARE: u32 = 0x0000_0F01;
const EXPL_A: u32 = 0x0000_0F10;
const EXPL_B: u32 = 0x0000_0F11;
const LIGH_A: u32 = 0x0000_0F12;
const LIGH_B: u32 = 0x0000_0F13;
const SOUN_1: u32 = 0x0000_0F14;
const SOUN_2: u32 = 0x0000_0F15;
const SOUN_3: u32 = 0x0000_0F16;
const SOUN_ALT: u32 = 0x0000_0F17;
const STAT_OTHER: u32 = 0x0000_0F18;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// Authored PROJ DATA layout (`schemas/fnv/combat.json` `ProjectileData`, 84 bytes).
fn proj_data_full() -> Vec<u8> {
    let mut b = Vec::with_capacity(84);
    b.extend(0x008Bu16.to_le_bytes()); // flags: Hitscan | Explosion | Muzzle Flash | Supersonic
    b.extend(1u16.to_le_bytes()); // type: 1 (Missile)
    b.extend(f32b(0.25)); // gravity
    b.extend(f32b(2500.0)); // speed
    b.extend(f32b(6000.0)); // range
    b.extend(LIGH_A.to_le_bytes()); // light
    b.extend(0u32.to_le_bytes()); // muzzle_flash_light
    b.extend(f32b(0.2)); // tracer_chance
    b.extend(f32b(45.0)); // explosion_alt_trigger_proximity
    b.extend(f32b(2.5)); // explosion_alt_trigger_timer
    b.extend(EXPL_A.to_le_bytes()); // explosion
    b.extend(SOUN_1.to_le_bytes()); // sound
    b.extend(f32b(0.05)); // muzzle_flash_duration
    b.extend(f32b(0.75)); // fade_duration
    b.extend(f32b(12.0)); // impact_force
    b.extend(SOUN_2.to_le_bytes()); // sound_countdown
    b.extend(SOUN_3.to_le_bytes()); // sound_disable
    b.extend(0u32.to_le_bytes()); // default_weapon_source
    b.extend(f32b(1.0)); // rotation.x
    b.extend(f32b(2.0)); // rotation.y
    b.extend(f32b(3.0)); // rotation.z
    b.extend(f32b(0.6)); // bouncy_mult
    b
}

fn fixture() -> Fixture {
    let mut bytes = tes4();
    bytes.extend(top(
        b"PROJ",
        &[
            record(
                b"PROJ",
                0,
                PROJ_MAIN,
                &[
                    sub(b"EDID", z("OwnMainProj")),
                    sub(b"DATA", proj_data_full()),
                    sub(b"NAM1", z("effects\\owned_flash.nif")),
                    sub(b"VNAM", 1u32.to_le_bytes().to_vec()),
                ],
            ),
            record(b"PROJ", 0, PROJ_BARE, &[sub(b"EDID", z("OwnBareProj"))]),
        ]
        .concat(),
    ));
    bytes.extend(top(
        b"EXPL",
        &[
            record(b"EXPL", 0, EXPL_A, &[sub(b"EDID", z("OwnExplA"))]),
            record(b"EXPL", 0, EXPL_B, &[sub(b"EDID", z("OwnExplB"))]),
        ]
        .concat(),
    ));
    bytes.extend(top(
        b"LIGH",
        &[
            record(b"LIGH", 0, LIGH_A, &[sub(b"EDID", z("OwnLightA"))]),
            record(b"LIGH", 0, LIGH_B, &[sub(b"EDID", z("OwnLightB"))]),
        ]
        .concat(),
    ));
    bytes.extend(top(
        b"SOUN",
        &[
            record(b"SOUN", 0, SOUN_1, &[sub(b"EDID", z("OwnProjSoun1"))]),
            record(b"SOUN", 0, SOUN_2, &[sub(b"EDID", z("OwnProjSoun2"))]),
            record(b"SOUN", 0, SOUN_3, &[sub(b"EDID", z("OwnProjSoun3"))]),
            record(b"SOUN", 0, SOUN_ALT, &[sub(b"EDID", z("OwnProjSounAlt"))]),
        ]
        .concat(),
    ));
    bytes.extend(top(b"STAT", &record(b"STAT", 0, STAT_OTHER, &[sub(b"EDID", z("OwnNonProjStat"))])));

    let plugins = vec![Plugin::parse(&bytes, "OwnedProjTraits.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&functions).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

fn call(g: &mut Game, name: &str, args: &[XVal]) -> XVal {
    fnv_game::nvse::call(g, &RunCtx::default(), name, None, args).unwrap().unwrap()
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{}}\n{body}\nEnd"), RunCtx::default()).unwrap()
}

#[test]
fn all_non_flag_projectile_traits_read_override_and_preserve_data_flags() {
    let f = fixture();
    let mut g = game(&f);
    let p = FormId(PROJ_MAIN);

    // Baseline authored ProjectileTraits.
    let base = g.projectile_traits(p).expect("authored PROJ traits");
    assert_eq!(base.original, p);
    assert_eq!((base.kind_code, base.kind), (1, Some("Missile")));
    assert_eq!((base.gravity, base.speed, base.range, base.tracer_chance), (0.25, 2500.0, 6000.0, 0.2));
    assert_eq!((base.alt_trigger_proximity, base.alt_trigger_timer), (45.0, 2.5));
    assert_eq!((base.muzzle_flash_duration, base.fade_duration, base.impact_force), (0.05, 0.75, 12.0));
    assert_eq!(base.rotation, [1.0, 2.0, 3.0]);
    assert_eq!(base.bouncy_mult, 0.6);
    assert_eq!((base.sound_level_code, base.sound_level), (1, Some("Normal")));
    assert_eq!(base.explosion, Some(FormId(EXPL_A)));
    assert_eq!(base.light, Some(FormId(LIGH_A)));
    assert_eq!(base.muzzle_flash_model.as_deref(), Some("effects\\owned_flash.nif"));
    assert_eq!((base.sound, base.sound_countdown, base.sound_disable), (Some(FormId(SOUN_1)), Some(FormId(SOUN_2)), Some(FormId(SOUN_3))));
    assert_eq!(g.projectile_sound(p, 1), Some(FormId(SOUN_1)));
    assert_eq!(g.projectile_sound(p, 2), Some(FormId(SOUN_2)));
    assert_eq!(g.projectile_sound(p, 3), Some(FormId(SOUN_3)));
    assert_eq!(g.projectile_sound(p, 4), None);

    // Bare PROJ without VNAM has sound_level = None.
    let bare = g.projectile_traits(FormId(PROJ_BARE)).expect("bare PROJ traits");
    assert_eq!((bare.sound_level_code, bare.sound_level), (0, None));
    assert_eq!(bare.muzzle_flash_model, None);

    // Non-PROJ returns None.
    assert!(g.projectile_traits(FormId(STAT_OTHER)).is_none());
    assert!(g.projectile_trait_overrides(FormId(STAT_OTHER)).is_none());

    // Sparse overrides start empty.
    let ov0 = g.projectile_trait_overrides(p).unwrap();
    assert_eq!(ov0.speed, None);
    assert_eq!(ov0.explosion, None);
    assert_eq!(ov0.sound, None);

    // Exercise every non-flag PROJ setter through real NVSE scripts.
    q(
        &mut g,
        "SetProjectileTraitNumeric OwnMainProj 0 2\n\
         SetProjectileTraitNumeric OwnMainProj 1 1.75\n\
         SetProjectileTraitNumeric OwnMainProj 2 900\n\
         SetProjectileTraitNumeric OwnMainProj 3 1800\n\
         SetProjectileTraitNumeric OwnMainProj 4 0.85\n\
         SetProjectileTraitNumeric OwnMainProj 5 140\n\
         SetProjectileTraitNumeric OwnMainProj 6 4.5\n\
         SetProjectileTraitNumeric OwnMainProj 7 0.15\n\
         SetProjectileTraitNumeric OwnMainProj 8 1.25\n\
         SetProjectileTraitNumeric OwnMainProj 9 22\n\
         SetProjectileTraitNumeric OwnMainProj 10 15\n\
         SetProjectileTraitNumeric OwnMainProj 11 30\n\
         SetProjectileTraitNumeric OwnMainProj 12 45\n\
         SetProjectileTraitNumeric OwnMainProj 13 1.25\n\
         SetProjectileTraitNumeric OwnMainProj 14 2\n\
         SetProjectileExplosion OwnMainProj OwnExplB\n\
         SetProjectileLight OwnMainProj OwnLightB\n\
         SetProjectileMuzzleFlash OwnMainProj \"effects\\new_flash.nif\"\n\
         SetProjectileSound OwnMainProj 1 OwnProjSounAlt\n\
         SetProjectileSound OwnMainProj 2 OwnProjSoun3\n\
         SetProjectileSound OwnMainProj 3 OwnProjSoun1",
    );

    let after = g.projectile_traits(p).unwrap();
    assert_eq!((after.kind_code, after.kind), (2, Some("Lobber")));
    assert_eq!((after.gravity, after.speed, after.range, after.tracer_chance), (1.75, 900.0, 1800.0, 0.85));
    assert_eq!((after.alt_trigger_proximity, after.alt_trigger_timer), (140.0, 4.5));
    assert_eq!((after.muzzle_flash_duration, after.fade_duration, after.impact_force), (0.15, 1.25, 22.0));
    assert_eq!(after.rotation, [15.0, 30.0, 45.0]);
    assert_eq!(after.bouncy_mult, 1.25);
    assert_eq!((after.sound_level_code, after.sound_level), (2, Some("Silent")));
    assert_eq!(after.explosion, Some(FormId(EXPL_B)));
    assert_eq!(after.light, Some(FormId(LIGH_B)));
    assert_eq!(after.muzzle_flash_model.as_deref(), Some("effects\\new_flash.nif"));
    assert_eq!(
        (after.sound, after.sound_countdown, after.sound_disable),
        (Some(FormId(SOUN_ALT)), Some(FormId(SOUN_3)), Some(FormId(SOUN_1)))
    );

    // Script getters read back the exact same values.
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileTraitNumeric OwnMainProj 0)"), XVal::Num(2.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileTraitNumeric OwnMainProj 1)"), XVal::Num(1.75));
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileTraitNumeric OwnMainProj 2)"), XVal::Num(900.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileTraitNumeric OwnMainProj 5)"), XVal::Num(140.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileTraitNumeric OwnMainProj 6)"), XVal::Num(4.5));
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileTraitNumeric OwnMainProj 14)"), XVal::Num(2.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileExplosion OwnMainProj)"), XVal::Ref(EXPL_B));
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileLight OwnMainProj)"), XVal::Ref(LIGH_B));
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileMuzzleFlash OwnMainProj)"), XVal::Str("effects\\new_flash.nif".into()));

    // Sparse overrides reflect every change, and DATA/flags was never written.
    let ov = g.projectile_trait_overrides(p).unwrap();
    assert_eq!(ov.kind, Some(Some("Lobber")));
    assert_eq!(ov.speed, Some(900.0));
    assert_eq!(ov.gravity, Some(1.75));
    assert_eq!(ov.explosion, Some(Some(FormId(EXPL_B))));
    assert_eq!(ov.light, Some(Some(FormId(LIGH_B))));
    assert_eq!(ov.sound, Some(Some(FormId(SOUN_ALT))));
    assert_eq!(ov.sound_countdown, Some(Some(FormId(SOUN_3))));
    assert_eq!(ov.sound_disable, Some(Some(FormId(SOUN_1))));
    assert_eq!(g.num_field(p, "DATA", "flags"), f64::from(0x008Bu16));
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileFlag OwnMainProj 1)"), XVal::Num(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetProjectileFlag OwnMainProj 2)"), XVal::Num(0.0));
}

#[test]
fn projectile_setters_validate_targets_slots_and_replacement_forms_without_mutation() {
    let f = fixture();
    let mut g = game(&f);
    let p = FormId(PROJ_MAIN);

    // SetProjectileExplosion rejects non-PROJ targets, non-EXPL forms, and keeps explosion on null [observed: gaps6].
    call(&mut g, "setprojectileexplosion", &[XVal::Ref(STAT_OTHER), XVal::Ref(EXPL_B)]);
    call(&mut g, "setprojectileexplosion", &[XVal::Ref(p.0), XVal::Ref(STAT_OTHER)]);
    call(&mut g, "setprojectileexplosion", &[XVal::Ref(p.0), XVal::Ref(LIGH_B)]);
    call(&mut g, "setprojectileexplosion", &[XVal::Ref(p.0), XVal::Ref(0)]);
    call(&mut g, "setprojectileexplosion", &[XVal::Ref(p.0)]);
    assert_eq!(g.projectile_traits(p).unwrap().explosion, Some(FormId(EXPL_A)));
    assert_eq!(call(&mut g, "getprojectileexplosion", &[XVal::Ref(STAT_OTHER)]), XVal::Ref(0));

    // SetProjectileExplosion accepts TempCloneForm EXPL.
    let XVal::Ref(expl_clone_raw) = call(&mut g, "tempcloneform", &[XVal::Ref(EXPL_B), XVal::Num(1.0)]) else {
        panic!("expected EXPL clone");
    };
    call(&mut g, "setprojectileexplosion", &[XVal::Ref(p.0), XVal::Ref(expl_clone_raw)]);
    assert_eq!(g.projectile_traits(p).unwrap().explosion, Some(FormId(expl_clone_raw)));

    // SetProjectileLight rejects non-LIGH and malformed non-null args without clearing existing light.
    for bad in [XVal::Ref(STAT_OTHER), XVal::Ref(EXPL_B), XVal::Str("bad".into()), XVal::Num(-1.0), XVal::Num(1.5)] {
        call(&mut g, "setprojectilelight", &[XVal::Ref(p.0), bad]);
        assert_eq!(g.projectile_traits(p).unwrap().light, Some(FormId(LIGH_A)));
    }
    // TempCloneForm LIGH is accepted; explicit 0 is a no-op and omitted arg clears light
    // [observed: `w6d2_11_ammo_proj_expl_clone_and_edges`].
    let XVal::Ref(ligh_clone_raw) = call(&mut g, "tempcloneform", &[XVal::Ref(LIGH_B), XVal::Num(1.0)]) else {
        panic!("expected LIGH clone");
    };
    call(&mut g, "setprojectilelight", &[XVal::Ref(p.0), XVal::Ref(ligh_clone_raw)]);
    assert_eq!(g.projectile_traits(p).unwrap().light, Some(FormId(ligh_clone_raw)));
    call(&mut g, "setprojectilelight", &[XVal::Ref(p.0), XVal::Ref(0)]);
    assert_eq!(g.projectile_traits(p).unwrap().light, Some(FormId(ligh_clone_raw)));
    call(&mut g, "setprojectilelight", &[XVal::Ref(p.0)]);
    assert_eq!(g.projectile_traits(p).unwrap().light, None);

    // SetProjectileSound rejects invalid indices, non-PROJ targets, and non-SOUN forms; accepts cloned SOUN.
    for bad_idx in [XVal::Num(0.0), XVal::Num(4.0), XVal::Num(1.5)] {
        call(&mut g, "setprojectilesound", &[XVal::Ref(p.0), bad_idx, XVal::Ref(SOUN_ALT)]);
    }
    call(&mut g, "setprojectilesound", &[XVal::Ref(STAT_OTHER), XVal::Num(1.0), XVal::Ref(SOUN_ALT)]);
    call(&mut g, "setprojectilesound", &[XVal::Ref(p.0), XVal::Num(1.0), XVal::Ref(STAT_OTHER)]);
    assert_eq!(g.projectile_sound(p, 1), Some(FormId(SOUN_1)));
    let XVal::Ref(soun_clone_raw) = call(&mut g, "tempcloneform", &[XVal::Ref(SOUN_ALT), XVal::Num(1.0)]) else {
        panic!("expected SOUN clone");
    };
    call(&mut g, "setprojectilesound", &[XVal::Ref(p.0), XVal::Num(2.0), XVal::Ref(soun_clone_raw)]);
    assert_eq!(g.projectile_sound(p, 2), Some(FormId(soun_clone_raw)));

    // SetProjectileTraitNumeric rejects fractional/out-of-range trait indices and invalid enum codes.
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(p.0), XVal::Num(2.5), XVal::Num(111.0)]);
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(p.0), XVal::Num(15.0), XVal::Num(111.0)]);
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(p.0), XVal::Num(0.0), XVal::Num(3.0)]);
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(p.0), XVal::Num(14.0), XVal::Num(0.0)]);
    assert_eq!(g.projectile_traits(p).unwrap().speed, 2500.0);
    assert_eq!(g.projectile_traits(p).unwrap().kind_code, 1);
    assert_eq!(g.projectile_traits(p).unwrap().sound_level_code, 1);
}

#[test]
fn temp_cloned_projectiles_snapshot_at_creation_and_isolate_post_clone_edits() {
    let f = fixture();
    let mut g = game(&f);
    let orig = FormId(PROJ_MAIN);

    // Clone created BEFORE any edits on orig snapshots the authored state and stays
    // 100% isolated from subsequent edits to orig [observed: w4g_02_tempcloneform_world_proj_csty].
    let XVal::Ref(c_sibling_raw) = call(&mut g, "tempcloneform", &[XVal::Ref(orig.0), XVal::Num(1.0)]) else {
        panic!("expected sibling clone");
    };
    let c_sibling = FormId(c_sibling_raw);

    // Edit orig: speed, gravity, explosion, light, sound 1, muzzle flash, and DATA/flags bit 2.
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(orig.0), XVal::Num(2.0), XVal::Num(800.0)]);
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(orig.0), XVal::Num(1.0), XVal::Num(0.75)]);
    call(&mut g, "setprojectileexplosion", &[XVal::Ref(orig.0), XVal::Ref(EXPL_B)]);
    call(&mut g, "setprojectilelight", &[XVal::Ref(orig.0), XVal::Ref(LIGH_B)]);
    call(&mut g, "setprojectilesound", &[XVal::Ref(orig.0), XVal::Num(1.0), XVal::Ref(SOUN_ALT)]);
    call(&mut g, "setprojectilemuzzleflash", &[XVal::Ref(orig.0), XVal::Str("effects\\orig_edit.nif".into())]);
    call(&mut g, "setprojectileflag", &[XVal::Ref(orig.0), XVal::Num(2.0), XVal::Num(1.0)]);

    // Pre-edit clone c_sibling MUST NOT see orig's post-clone mutations through any reader path.
    assert_eq!(g.projectile_launch_traits(c_sibling).unwrap().speed, None);
    assert_eq!(g.projectile_flags_override(c_sibling), None);
    assert_eq!(g.projectile_flags(c_sibling), Some(0x008B));
    assert_eq!(g.projectile_traits(c_sibling).unwrap().speed, 2500.0);
    assert_eq!(g.projectile_traits(c_sibling).unwrap().gravity, 0.25);
    assert_eq!(g.projectile_traits(c_sibling).unwrap().explosion, Some(FormId(EXPL_A)));
    assert_eq!(g.projectile_traits(c_sibling).unwrap().light, Some(FormId(LIGH_A)));
    assert_eq!(g.projectile_sound(c_sibling, 1), Some(FormId(SOUN_1)));
    assert_eq!(g.projectile_traits(c_sibling).unwrap().muzzle_flash_model.as_deref(), Some("effects\\owned_flash.nif"));
    assert_eq!(q(&mut g, &format!("SetFunctionValue (GetProjectileTraitNumeric {c_sibling_raw:#010X} 2)")), XVal::Num(2500.0));
    assert_eq!(q(&mut g, &format!("SetFunctionValue (GetProjectileTraitNumeric {c_sibling_raw:#010X} 1)")), XVal::Num(0.25));
    assert_eq!(q(&mut g, &format!("SetFunctionValue (GetProjectileLight {c_sibling_raw:#010X})")), XVal::Ref(LIGH_A));
    assert_eq!(q(&mut g, &format!("SetFunctionValue (GetProjectileMuzzleFlash {c_sibling_raw:#010X})")), XVal::Str("effects\\owned_flash.nif".into()));
    assert_eq!(q(&mut g, &format!("SetFunctionValue (GetProjectileFlag {c_sibling_raw:#010X} 2)")), XVal::Num(0.0));

    // Clone c1 created AFTER orig's edits snapshots all active overrides onto c1.
    let XVal::Ref(c1_raw) = call(&mut g, "tempcloneform", &[XVal::Ref(orig.0), XVal::Num(1.0)]) else {
        panic!("expected c1 clone");
    };
    let c1 = FormId(c1_raw);
    assert_eq!(g.projectile_traits(c1).unwrap().speed, 800.0);
    assert_eq!(g.projectile_traits(c1).unwrap().gravity, 0.75);
    assert_eq!(g.projectile_traits(c1).unwrap().explosion, Some(FormId(EXPL_B)));
    assert_eq!(g.projectile_traits(c1).unwrap().light, Some(FormId(LIGH_B)));
    assert_eq!(g.projectile_sound(c1, 1), Some(FormId(SOUN_ALT)));
    assert_eq!(g.projectile_traits(c1).unwrap().muzzle_flash_model.as_deref(), Some("effects\\orig_edit.nif"));
    assert_eq!(g.projectile_flags(c1), Some(0x008B | (1 << 2)));

    // Subsequent edits on orig (both previously edited and previously untouched fields like range/flag 5)
    // do NOT bleed into c1.
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(orig.0), XVal::Num(2.0), XVal::Num(1234.0)]);
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(orig.0), XVal::Num(3.0), XVal::Num(9999.0)]);
    call(&mut g, "setprojectileexplosion", &[XVal::Ref(orig.0), XVal::Ref(EXPL_A)]);
    call(&mut g, "setprojectilesound", &[XVal::Ref(orig.0), XVal::Num(1.0), XVal::Ref(SOUN_1)]);
    call(&mut g, "setprojectileflag", &[XVal::Ref(orig.0), XVal::Num(5.0), XVal::Num(1.0)]);
    assert_eq!(g.projectile_traits(orig).unwrap().speed, 1234.0);
    assert_eq!(g.projectile_traits(orig).unwrap().range, 9999.0);
    assert_eq!(g.projectile_traits(c1).unwrap().speed, 800.0);
    assert_eq!(g.projectile_traits(c1).unwrap().range, 6000.0);
    assert_eq!(g.projectile_traits(c1).unwrap().explosion, Some(FormId(EXPL_B)));
    assert_eq!(g.projectile_sound(c1, 1), Some(FormId(SOUN_ALT)));
    assert_eq!(q(&mut g, &format!("SetFunctionValue (GetProjectileTraitNumeric {c1_raw:#010X} 3)")), XVal::Num(6000.0));
    assert_eq!(q(&mut g, &format!("SetFunctionValue (GetProjectileFlag {c1_raw:#010X} 5)")), XVal::Num(0.0));

    // Edit c1 (including SetProjectileSound on a cloned PROJ!), then clone c1 -> c2 (clone-of-clone).
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(c1.0), XVal::Num(3.0), XVal::Num(4200.0)]);
    call(&mut g, "setprojectilesound", &[XVal::Ref(c1.0), XVal::Num(3.0), XVal::Ref(SOUN_ALT)]);
    call(&mut g, "setprojectilelight", &[XVal::Ref(c1.0)]); // clear light on c1
    let XVal::Ref(c2_raw) = call(&mut g, "tempcloneform", &[XVal::Ref(c1.0), XVal::Num(1.0)]) else {
        panic!("expected c2 clone");
    };
    let c2 = FormId(c2_raw);
    assert_eq!(g.projectile_traits(c2).unwrap().original, orig);
    assert_eq!(g.projectile_traits(c2).unwrap().speed, 800.0);
    assert_eq!(g.projectile_traits(c2).unwrap().range, 4200.0);
    assert_eq!(g.projectile_traits(c2).unwrap().light, None);
    assert_eq!(g.projectile_sound(c2, 3), Some(FormId(SOUN_ALT)));
    assert_eq!(q(&mut g, &format!("SetFunctionValue (GetProjectileTraitNumeric {c2_raw:#010X} 3)")), XVal::Num(4200.0));
    assert_eq!(q(&mut g, &format!("SetFunctionValue (GetProjectileExplosion {c2_raw:#010X})")), XVal::Ref(EXPL_B));

    // Mutating c1 (both previously edited range and untouched tracer_chance) or c2 leaves
    // the others and orig independent.
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(c1.0), XVal::Num(3.0), XVal::Num(7777.0)]);
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(c1.0), XVal::Num(4.0), XVal::Num(0.95)]);
    call(&mut g, "setprojectiletraitnumeric", &[XVal::Ref(c2.0), XVal::Num(2.0), XVal::Num(333.0)]);
    assert_eq!(g.projectile_traits(orig).unwrap().range, 9999.0);
    assert_eq!(g.projectile_traits(c1).unwrap().range, 7777.0);
    assert_eq!(g.projectile_traits(c2).unwrap().range, 4200.0);
    assert_eq!(g.projectile_traits(c2).unwrap().tracer_chance, 0.2);
    assert_eq!(g.projectile_traits(c1).unwrap().speed, 800.0);
    assert_eq!(g.projectile_traits(c2).unwrap().speed, 333.0);
    assert_eq!(g.projectile_sound(orig, 3), Some(FormId(SOUN_3)));
}
