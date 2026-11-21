//! Live consumers of `SetActorBaseFlagsLow` and `SetActorBaseFlagsHigh` (`ACBS/flags`,
//! docs/spec-notes/ext-last-core.md §10): record-type guards (`NPC_` vs `CREA`), low/high
//! half preservation, template inheritance boundaries, and placed `LVLN`/`LVLC` resolution.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const CLASS_VENDOR: u32 = 0x700;
const NPC_BASE: u32 = 0x800;
const CREA_BASE: u32 = 0x801;
const CREA_TEMPLATE: u32 = 0x802;
const CREA_CHILD: u32 = 0x803;
const CREA_LEVELED_MEMBER: u32 = 0x804;
const CREA_LVLC: u32 = 0x805;

const NPC_REF: FormId = FormId(0x300);
const CREA_REF: FormId = FormId(0x301);
const CREA_CHILD_REF: FormId = FormId(0x302);
const CREA_LEVELED_REF: FormId = FormId(0x303);

const TMPL_MODEL: u16 = 1 << 6;
const TMPL_BASE: u16 = 1 << 7;

fn acbs_full(flags: u32, level_raw: u16, calc_min: u16, calc_max: u16, template_flags: u16) -> fnv_esm::raw::Subrecord {
    let mut d = Vec::with_capacity(24);
    d.extend(flags.to_le_bytes()); // flags (4)
    d.extend(50u16.to_le_bytes()); // fatigue (2)
    d.extend(0u16.to_le_bytes()); // barter_gold (2)
    d.extend(level_raw.to_le_bytes()); // level_or_level_mult_raw (2)
    d.extend(calc_min.to_le_bytes()); // calc_min (2)
    d.extend(calc_max.to_le_bytes()); // calc_max (2)
    d.extend(100u16.to_le_bytes()); // speed_multiplier (2)
    d.extend(0f32.to_le_bytes()); // karma (4)
    d.extend(0i16.to_le_bytes()); // disposition_base (2)
    d.extend(template_flags.to_le_bytes()); // template_flags (2)
    sub(b"ACBS", d)
}

fn aidt(aggression: u8, assistance: u8, services: u32) -> fnv_esm::raw::Subrecord {
    let mut d = vec![0u8; 20];
    d[0] = aggression;
    d[1] = 2; // confidence
    d[2] = 50; // energy
    d[3] = 50; // responsibility
    d[4] = 0; // mood
    d[8..12].copy_from_slice(&services.to_le_bytes());
    d[15] = assistance;
    sub(b"AIDT", d)
}

fn clas(fid: u32, edid: &str, services: u32) -> Vec<u8> {
    // CLAS DATA: tag_skills[4] (16), flags (4), buys_sells_services (4), teaches (1), max_training (1), unused (2) = 28 bytes
    let mut d = vec![0u8; 28];
    d[20..24].copy_from_slice(&services.to_le_bytes());
    record(b"CLAS", 0, fid, &[sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"DATA", d)])
}

fn npc(fid: u32, edid: &str, flags: u32, level_raw: u16, class_id: u32, ai_services: u32) -> Vec<u8> {
    let subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"FULL", z(edid)),
        acbs_full(flags, level_raw, 1, 20, 0),
        sub(b"CNAM", class_id.to_le_bytes().to_vec()),
        sub(b"DATA", [50i32.to_le_bytes().to_vec(), vec![5; 7]].concat()),
        aidt(0, 0, ai_services),
    ];
    record(b"NPC_", 0, fid, &subs)
}

fn crea(fid: u32, edid: &str, flags: u32, level_raw: u16, template: Option<(u32, u16)>) -> Vec<u8> {
    let mut subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"FULL", z(edid)),
        acbs_full(flags, level_raw, 1, 20, template.map_or(0, |(_, tf)| tf)),
        aidt(0, 0, 0),
    ];
    if let Some((t, _)) = template {
        subs.push(sub(b"TPLT", t.to_le_bytes().to_vec()));
    }
    record(b"CREA", 0, fid, &subs)
}

fn placed(sig: &[u8; 4], fid: FormId, base: u32, x: f32) -> Vec<u8> {
    let data = [x, 0.0, 0.0, 0.0, 0.0, 1.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    record(sig, 0, fid.0, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn plugin_bytes() -> Vec<u8> {
    let classes = clas(CLASS_VENDOR, "TestVendorClass", 0x0002); // Apparel service bit
    let npcs = npc(NPC_BASE, "TestNpcBase", 0, 5, CLASS_VENDOR, 0x0001); // Weapons service bit
    let creas = [
        crea(CREA_BASE, "TestCreaBase", 0, 1000, None),
        crea(CREA_TEMPLATE, "TestCreaTemplate", 0, 5, None),
        crea(CREA_CHILD, "TestCreaChild", 0, 5, Some((CREA_TEMPLATE, TMPL_BASE | TMPL_MODEL))),
        crea(CREA_LEVELED_MEMBER, "TestCreaLeveledMember", 0, 5, None),
    ]
    .concat();
    let lvlo = [1i16.to_le_bytes().to_vec(), vec![0; 2], CREA_LEVELED_MEMBER.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat();
    let lvlc = record(b"LVLC", 0, CREA_LVLC, &[sub(b"EDID", z("TestCreaLvlc")), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0]), sub(b"LVLO", lvlo)]);

    let refs = [
        placed(b"ACHR", NPC_REF, NPC_BASE, 100.0),
        placed(b"ACRE", CREA_REF, CREA_BASE, 200.0),
        placed(b"ACRE", CREA_CHILD_REF, CREA_CHILD, 300.0),
        placed(b"ACRE", CREA_LEVELED_REF, CREA_LVLC, 400.0),
    ]
    .concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestFlagsCell"))]);
    let cells = [cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs))].concat();
    [
        tes4(),
        top(b"CLAS", &classes),
        top(b"NPC_", &npcs),
        top(b"CREA", &creas),
        top(b"LVLC", &lvlc),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "ActorBaseFlags.esm".into()).unwrap()];
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
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.new_game();
    g.player_entered(Some(FormId(CELL)));
    g
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap()
}

fn q_on(g: &mut Game, on: FormId, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx { self_ref: Some(on), ..RunCtx::default() }).unwrap()
}

fn is_essential(g: &mut Game, on: FormId) -> bool {
    q_on(g, on, "SetFunctionValue (IsEssential)") == XVal::Num(1.0)
}

#[test]
fn high_flags_drive_consumers_and_respect_record_type_guards() {
    let f = fixture();
    let mut g = game(&f);

    // Baseline creature: no high flags set.
    assert!(g.can_knock_down(CREA_REF));
    assert!(!g.no_vats_melee(CREA_REF));
    assert!(!g.allows_pc_dialogue(CREA_REF));
    assert!(!g.cant_open_doors(CREA_REF));
    assert!(!g.is_fixture(CREA_REF));
    assert!(!g.is_ghost(CREA_REF));
    assert!(!g.is_immune(CREA_REF));
    assert!(!g.no_combat_in_water(CREA_REF));
    assert!(g.pickpocket_refusal(CREA_REF).is_some());
    assert_eq!(q(&mut g, "SetFunctionValue (GetIsImmobile TestCreaBase)"), XVal::Num(0.0));

    // Set a low bit first (Essential = bit 1) to verify SetActorBaseFlagsHigh preserves low 16 bits.
    q(&mut g, "SetActorBaseFlagsLow 2 TestCreaBase");
    assert!(is_essential(&mut g, CREA_REF));

    // High bit 10 = No Knockdowns (ACBS bit 26), matching SetBit (GetActorBaseFlagsHigh r) 10.
    q(&mut g, "SetActorBaseFlagsHigh (SetBit (GetActorBaseFlagsHigh TestCreaBase) 10) TestCreaBase");
    assert_eq!(q(&mut g, "SetFunctionValue (GetActorBaseFlagsHigh TestCreaBase)"), XVal::Num(1024.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetActorBaseFlagsLow TestCreaBase)"), XVal::Num(2.0));
    assert!(!g.can_knock_down(CREA_REF));
    assert!(!g.push_actor_away(None, CREA_REF, 10.0));
    assert!(is_essential(&mut g, CREA_REF), "low half preserved");

    // Set remaining creature high flags: No Right Arm (0), No Left Arm (1), No Combat in Water (2),
    // No VATS Melee (4), Allow PC Dialogue (5), Can't Open Doors (6), Immobile (7),
    // No Knockdowns (10), Not Pushable (11), Allow Pickpocket (12), Is Ghost (13),
    // No Rotating To Head-track (14), Invulnerable (15).
    let high_all: u32 = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 4) | (1 << 5) | (1 << 6) | (1 << 7) | (1 << 10) | (1 << 11) | (1 << 12) | (1 << 13) | (1 << 14) | (1 << 15);
    q(&mut g, &format!("SetActorBaseFlagsHigh {high_all} TestCreaBase"));
    assert!(g.no_vats_melee(CREA_REF));
    assert!(g.allows_pc_dialogue(CREA_REF));
    assert!(g.cant_open_doors(CREA_REF));
    assert!(g.is_fixture(CREA_REF));
    assert!(g.is_ghost(CREA_REF));
    assert!(g.is_immune(CREA_REF));
    assert!(g.no_combat_in_water(CREA_REF));
    assert_eq!(q(&mut g, "SetFunctionValue (GetIsImmobile TestCreaBase)"), XVal::Num(1.0));
    let at = g.leveled_at(CREA_REF);
    let minor = g.runtime_minor_flags(FormId(CREA_BASE), CREA_REF, at);
    assert!(minor.no_right_arm && minor.no_left_arm && minor.not_pushable && minor.no_head_track);

    // Clear Is Ghost so pickpocket checks Allow Pickpocket (bit 28 / high bit 12) cleanly.
    let without_ghost = high_all & !(1 << 13);
    q(&mut g, &format!("SetActorBaseFlagsHigh {without_ghost} TestCreaBase"));
    assert!(!g.is_ghost(CREA_REF));
    assert!(g.is_immune(CREA_REF), "Invulnerable bit 31 still active");
    assert_eq!(g.pickpocket_refusal(CREA_REF), None, "Allow Pickpocket allows creature pickpocket");

    // Clear all high flags: consumers revert immediately.
    q(&mut g, "SetActorBaseFlagsHigh 0 TestCreaBase");
    assert!(g.can_knock_down(CREA_REF));
    assert!(!g.no_vats_melee(CREA_REF));
    assert!(!g.allows_pc_dialogue(CREA_REF));
    assert!(!g.cant_open_doors(CREA_REF));
    assert!(!g.is_fixture(CREA_REF));
    assert!(!g.is_immune(CREA_REF));
    assert!(g.pickpocket_refusal(CREA_REF).is_some());
    assert_eq!(q(&mut g, "SetFunctionValue (GetIsImmobile TestCreaBase)"), XVal::Num(0.0));

    // NPC_ record-type guards: CREA-only bits must NOT leak onto NPC_, while shared bits and
    // NPC_-only Autocalc Service (bit 23 / high bit 7) DO take effect.
    assert_eq!(g.services(NPC_REF), 0x0001, "authored AI services before Autocalc Service");
    q(&mut g, &format!("SetActorBaseFlagsHigh {high_all} TestNpcBase"));
    assert!(!g.can_knock_down(NPC_REF), "NPC No Knockdowns");
    assert!(g.no_vats_melee(NPC_REF), "NPC No VATS Melee");
    assert_eq!(g.services(NPC_REF), 0x0002, "NPC Autocalc Service (bit 23) switches to class services");
    assert!(!g.is_ghost(NPC_REF), "CREA bit 29 ignored on NPC_");
    assert!(!g.is_immune(NPC_REF), "CREA bit 31 ignored on NPC_");
    assert!(!g.is_fixture(NPC_REF), "CREA bit 23 Immobile ignored on NPC_");
    assert!(!g.allows_pc_dialogue(NPC_REF), "CREA bit 21 ignored on NPC_");
    assert!(!g.cant_open_doors(NPC_REF), "CREA bit 22 ignored on NPC_");
    assert!(!g.no_combat_in_water(NPC_REF), "CREA bit 18 ignored on NPC_");
    assert_eq!(q(&mut g, "SetFunctionValue (GetIsImmobile TestNpcBase)"), XVal::Num(0.0));
    let npc_minor = g.runtime_minor_flags(FormId(NPC_BASE), NPC_REF, g.leveled_at(NPC_REF));
    assert!(npc_minor.not_pushable && npc_minor.no_head_track);
    assert!(!npc_minor.no_right_arm && !npc_minor.no_left_arm, "CREA limb bits ignored on NPC_");
}

#[test]
fn low_flags_drive_consumers_and_preserve_high_half() {
    let f = fixture();
    let mut g = game(&f);

    // Set a high bit first (No VATS Melee = high bit 4) to verify SetActorBaseFlagsLow preserves it.
    q(&mut g, "SetActorBaseFlagsHigh 16 TestNpcBase");
    assert!(g.no_vats_melee(NPC_REF));

    let hp_before = g.max_health(NPC_REF);
    assert!(!g.actor_female(NPC_REF));
    assert!(!is_essential(&mut g, NPC_REF));
    assert!(!g.respawns(NPC_REF));
    assert!(!g.no_low_level_processing(NPC_REF));
    assert_eq!(g.no_blood(NPC_REF), (false, false));

    // NPC low flags: Female (0), Essential (1), Respawn (3), Auto-calc stats (4),
    // No Low Level Processing (9), No Blood Spray (11), No Blood Decal (12), plus bit 15 (CREA No Head).
    let npc_low: u32 = (1 << 0) | (1 << 1) | (1 << 3) | (1 << 4) | (1 << 9) | (1 << 11) | (1 << 12) | (1 << 15);
    q(&mut g, &format!("SetActorBaseFlagsLow {npc_low} TestNpcBase"));
    assert_eq!(q(&mut g, "SetFunctionValue (GetActorBaseFlagsHigh TestNpcBase)"), XVal::Num(16.0));
    assert!(g.no_vats_melee(NPC_REF), "high half preserved");
    assert!(g.actor_female(NPC_REF));
    assert!(is_essential(&mut g, NPC_REF));
    assert!(g.respawns(NPC_REF));
    assert!(g.no_low_level_processing(NPC_REF));
    assert_eq!(g.no_blood(NPC_REF), (true, true));
    assert!(g.max_health(NPC_REF) > hp_before, "Auto-calc stats adds level health bonus");
    let npc_minor = g.runtime_minor_flags(FormId(NPC_BASE), NPC_REF, g.leveled_at(NPC_REF));
    assert!(!npc_minor.no_head, "CREA bit 15 No Head ignored on NPC_");

    // Saved SetEssential override still takes precedence over session ACBS Essential bit.
    q(&mut g, "SetEssential TestNpcBase 0");
    assert!(!is_essential(&mut g, NPC_REF), "explicit SetEssential 0 wins over ACBS bit 1");

    // Creature low flags: bit 0 (Biped, NOT Female), Flies (5), Walks (6), PC Level Mult (7), No Head (15).
    assert_eq!(g.actor_level(CREA_REF), 1000.0);
    assert!(!g.keeps_out_of_water(CREA_REF));
    assert_eq!(g.vats_untargetable(CREA_REF), None);
    let crea_low: u32 = (1 << 0) | (1 << 5) | (1 << 6) | (1 << 7) | (1 << 15);
    q(&mut g, &format!("SetActorBaseFlagsLow {crea_low} TestCreaBase"));
    assert!(!g.actor_female(CREA_REF), "bit 0 is Biped on CREA, not Female");
    assert!(g.keeps_out_of_water(CREA_REF), "Walks/Flies without Swims keeps out of water");
    assert_eq!(g.vats_untargetable(CREA_REF), Some("an unaggressive flying creature"));
    assert_eq!(g.actor_level(CREA_REF), 1.0, "PC Level Mult 1000 at player level 1 => level 1");
    let crea_minor = g.runtime_minor_flags(FormId(CREA_BASE), CREA_REF, g.leveled_at(CREA_REF));
    assert!(crea_minor.no_head, "CREA bit 15 No Head active");

    // Setting Swims (bit 4) clears keeps_out_of_water.
    q(&mut g, &format!("SetActorBaseFlagsLow {} TestCreaBase", crea_low | (1 << 4)));
    assert!(!g.keeps_out_of_water(CREA_REF), "Swims allows water triangles");
}

#[test]
fn template_inheritance_and_leveled_actor_resolution() {
    let f = fixture();
    let mut g = game(&f);

    // Child uses template for Base Data + Model/Animation.
    // 1) Own-record flags (No Knockdowns = high bit 10, No VATS Melee = high bit 4) set on the
    //    template must NOT affect child, while Base Data flags (Invulnerable = high bit 15) DO.
    q(&mut g, &format!("SetActorBaseFlagsHigh {} TestCreaTemplate", (1 << 4) | (1 << 10) | (1 << 15)));
    assert!(g.can_knock_down(CREA_CHILD_REF), "Base Data template does not supply No Knockdowns");
    assert!(!g.no_vats_melee(CREA_CHILD_REF), "Base Data template does not supply No VATS Melee");
    assert!(g.is_immune(CREA_CHILD_REF), "Base Data template supplies Invulnerable");

    // Setting No Knockdowns + No VATS Melee on the child's own base DOES take effect.
    q(&mut g, &format!("SetActorBaseFlagsHigh {} TestCreaChild", (1 << 4) | (1 << 10)));
    assert!(!g.can_knock_down(CREA_CHILD_REF));
    assert!(g.no_vats_melee(CREA_CHILD_REF));

    // Base Data + Model/Animation low flags (Essential = 1, Respawn = 3, Walks = 6, No Blood Spray = 11)
    // on the template propagate to child.
    q(&mut g, &format!("SetActorBaseFlagsLow {} TestCreaTemplate", (1 << 1) | (1 << 3) | (1 << 6) | (1 << 11)));
    assert!(is_essential(&mut g, CREA_CHILD_REF));
    assert!(g.respawns(CREA_CHILD_REF));
    assert!(g.keeps_out_of_water(CREA_CHILD_REF));
    assert_eq!(g.no_blood(CREA_CHILD_REF), (true, false));

    // 2) Placed leveled creature (ACRE -> LVLC -> CREA_LEVELED_MEMBER): calling SetActorBaseFlagsHigh
    //    with the placed reference as receiver resolves the LVLC carrier to the spawned CREA base.
    assert!(g.can_knock_down(CREA_LEVELED_REF));
    q_on(&mut g, CREA_LEVELED_REF, "SetActorBaseFlagsHigh 1024");
    assert_eq!(q_on(&mut g, CREA_LEVELED_REF, "SetFunctionValue (GetActorBaseFlagsHigh)"), XVal::Num(1024.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetActorBaseFlagsHigh TestCreaLeveledMember)"), XVal::Num(1024.0));
    assert!(!g.can_knock_down(CREA_LEVELED_REF));
}
