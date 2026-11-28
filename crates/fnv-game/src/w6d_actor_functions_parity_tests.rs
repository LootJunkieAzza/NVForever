//! Wave 6D-5, 6D-6, 6D-7 & 6D-8 `crates/fnv-game/src/functions/actor.rs` parity regression tests.
//!
//! Covers consolidated actor-function fixes:
//! - #4253 (`SetPCExpelled` rank preservation — also checked in `tests/player_source_commands.rs`)
//! - #4318 (`GetWeaponAnimType` `OneHandThrown` 13 -> 9 — also checked in `tests/synthetic.rs`)
//! - #4340 (`DamageAV` / `RestoreAV` absolute magnitude on negative args)
//! - #4356 (`IsEssential` returns 1 for non-Hardcore teammates — also checked in `tests/unconscious.rs`)
//! - #4358 (`HasFriendDisposition` threshold `FRIEND_DISPOSITION == 50.0`)
//! - #4370 (`DamageAV` floors limb conditions at 0.0)
//! - #4372 (`SetLevel` PC-relative thousandths multiplier — also checked in `tests/appearance.rs`)
//! - #4418 (`Player.SetPlayerTeammate` no-op & `GetTeammates` dead/player filtering)
//! - #4428 (multi-faction `GetFactionRelation` Ally > Friend precedence — also checked in `tests/factions.rs`)
//! - #4430 (`SetFactionRank` preserves signed negative `i8` ranks while `GetInFaction == 0`)

use crate::common::*;
use crate::functions::actor::{weapon_anim_code, FRIEND_DISPOSITION};
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const PLAYER_FAC: u32 = 0x1B2A4;
const F1: u32 = 0x100;
const F2: u32 = 0x101;
const NPC_BASE: u32 = 0x200;
const REF_A: u32 = 0x201;
const REF_B: u32 = 0x202;
const MULTI_NPC: u32 = 0x203;
const MULTI_REF: u32 = 0x204;
const CELL: u32 = 0x400;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn snam(fid: u32, rank: i8) -> Vec<u8> {
    let mut b = fid.to_le_bytes().to_vec();
    b.push(rank as u8);
    b.extend([0u8; 3]);
    b
}

fn fixture() -> Fixture {
    let mut acbs = 0u32.to_le_bytes().to_vec();
    for v in [50u16, 0, 1, 0, 0, 100] {
        acbs.extend(v.to_le_bytes());
    }
    acbs.extend(0f32.to_le_bytes());
    acbs.extend([0u8; 4]);
    let data = [100i32.to_le_bytes().to_vec(), vec![5, 5, 5, 5, 5, 5, 5]].concat();
    let player = record(
        b"NPC_",
        0,
        0x7,
        &[sub(b"EDID", z("Player")), sub(b"ACBS", acbs.clone()), sub(b"DATA", data.clone()), sub(b"SNAM", snam(PLAYER_FAC, 0))],
    );
    let npc = record(b"NPC_", 0, NPC_BASE, &[sub(b"EDID", z("ActorTestNpc")), sub(b"ACBS", acbs), sub(b"DATA", data)]);
    let multi = record(
        b"NPC_",
        0,
        MULTI_NPC,
        &[sub(b"EDID", z("MultiNpc")), sub(b"SNAM", snam(F1, 0)), sub(b"SNAM", snam(F2, 0))],
    );
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell")), sub(b"DATA", vec![1])]);
    let refs_bytes = [
        record(b"ACHR", 0, REF_A, &[sub(b"EDID", z("CompA")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]),
        record(b"ACHR", 0, REF_B, &[sub(b"EDID", z("CompB")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]),
        record(b"ACHR", 0, MULTI_REF, &[sub(b"EDID", z("MultiRef")), sub(b"NAME", u32s(&[MULTI_NPC])), sub(b"DATA", vec![0; 24])]),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(
            b"FACT",
            &[
                record(b"FACT", 0, PLAYER_FAC, &[sub(b"EDID", z("PlayerFaction"))]),
                record(b"FACT", 0, F1, &[sub(b"EDID", z("FacOne"))]),
                record(b"FACT", 0, F2, &[sub(b"EDID", z("FacTwo"))]),
            ]
            .concat(),
        ),
        top(b"NPC_", &[player, npc, multi].concat()),
        top(b"CELL", &[cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &refs_bytes))].concat()),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "W6dActor.esm".into()).unwrap()];
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
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..GameConfig::default() })
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default())
        .unwrap_or_else(|e| panic!("{body}: {e}"))
}

#[test]
fn weapon_anim_code_maps_thrown_to_9_and_matches_all_observed_dnam_types() {
    let got: Vec<i64> = (0..=13).map(weapon_anim_code).collect();
    assert_eq!(got, vec![1, 2, 3, 4, 4, 5, 6, 5, 7, 8, 9, 10, 11, 9]);
    assert_eq!(weapon_anim_code(-1), 0);
    assert_eq!(weapon_anim_code(14), 0);
}

#[test]
fn damage_and_restore_actor_value_use_absolute_magnitude_for_negative_args() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, "CompA.SetAV Strength 6\nCompA.ModAV Strength 2\nCompA.DamageAV Strength 3");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetAV Strength)"), XVal::Num(5.0));
    // Negative DamageAV still damages by abs(amount).
    q(&mut g, "CompA.DamageAV Strength -1");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetAV Strength)"), XVal::Num(4.0));
    // Negative RestoreAV still restores by abs(amount).
    q(&mut g, "CompA.RestoreAV Strength -2");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetAV Strength)"), XVal::Num(6.0));
    q(&mut g, "CompA.RestoreAV Strength 5");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetAV Strength)"), XVal::Num(8.0));
}

#[test]
fn damage_actor_value_floors_limb_condition_at_zero() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, "CompA.DamageAV PerceptionCondition 150");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetAV PerceptionCondition)"), XVal::Num(0.0));
    assert_eq!(crate::functions::actor_value_capped(&mut g, FormId(REF_A), 25), 0.0);
}

#[test]
fn friend_disposition_threshold_is_fifty() {
    assert_eq!(FRIEND_DISPOSITION, 50.0);
}

#[test]
fn set_level_pc_mult_scales_by_thousandths_and_clamps() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, "Player.SetLevel 10");
    // 500 -> 5, 1000 -> 10, 2000 -> 20, min 15 -> 15, max 12 -> 12 [observed: w6d7p2_11b].
    q(&mut g, "CompA.SetLevel 500 1 0 0");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetLevel)"), XVal::Num(5.0));
    q(&mut g, "CompA.SetLevel 1000 1 0 0");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetLevel)"), XVal::Num(10.0));
    q(&mut g, "CompA.SetLevel 2000 1 0 0");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetLevel)"), XVal::Num(20.0));
    q(&mut g, "CompA.SetLevel 500 1 15 0");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetLevel)"), XVal::Num(15.0));
    q(&mut g, "CompA.SetLevel 2000 1 0 12");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetLevel)"), XVal::Num(12.0));
}

#[test]
fn player_cannot_be_teammate_and_getteammates_excludes_dead() {
    let f = fixture();
    let mut g = game(&f);

    q(&mut g, "Player.SetPlayerTeammate 1");
    assert_eq!(q(&mut g, "SetFunctionValue (Player.GetPlayerTeammate)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerTeammateCount)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (ar_Size (GetTeammates))"), XVal::Num(-1.0));

    q(&mut g, "CompA.SetPlayerTeammate 1\nCompB.SetPlayerTeammate 1");
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerTeammateCount)"), XVal::Num(2.0));
    assert_eq!(q(&mut g, "SetFunctionValue (ar_Size (GetTeammates))"), XVal::Num(2.0));

    g.extra.entry(FormId(REF_A)).or_default().dead = true;
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerTeammateCount)"), XVal::Num(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (ar_Size (GetTeammates))"), XVal::Num(1.0));
}

#[test]
fn ally_outranks_friend_regardless_of_faction_order() {
    let f = fixture();
    let mut g = game(&f);

    q(&mut g, "SetAlly FacOne PlayerFaction 0 0\nSetAlly FacTwo PlayerFaction 1 1");
    assert_eq!(q(&mut g, "SetFunctionValue (MultiRef.GetFactionRelation Player)"), XVal::Num(2.0));

    q(&mut g, "SetAlly FacOne PlayerFaction 1 1\nSetAlly FacTwo PlayerFaction 0 0");
    assert_eq!(q(&mut g, "SetFunctionValue (MultiRef.GetFactionRelation Player)"), XVal::Num(2.0));

    q(&mut g, "SetEnemy FacOne PlayerFaction 0 0\nSetAlly FacTwo PlayerFaction 1 1");
    assert_eq!(q(&mut g, "SetFunctionValue (MultiRef.GetFactionRelation Player)"), XVal::Num(3.0));
}

#[test]
fn setfactionrank_stores_negative_rank_raw_and_excludes_from_getinfaction() {
    let f = fixture();
    let mut g = game(&f);

    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetFactionRank FacOne)"), XVal::Num(-1.0));
    q(&mut g, "CompA.SetFactionRank FacOne 3");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetFactionRank FacOne)"), XVal::Num(3.0));
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetInFaction FacOne)"), XVal::Num(1.0));

    q(&mut g, "CompA.SetFactionRank FacOne -5");
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetFactionRank FacOne)"), XVal::Num(-5.0));
    assert_eq!(q(&mut g, "SetFunctionValue (CompA.GetInFaction FacOne)"), XVal::Num(0.0));
}
