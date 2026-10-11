//! Actor values from records and formulas, and their pools, on a synthetic plugin (no
//! game data). docs/spec-notes/actor-values.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::actor_values as av;
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NPC_GUARD: u32 = 0x830;
const NPC_COPY: u32 = 0x831;
const CREA: u32 = 0x832;
const CONT: u32 = 0x861;
const CELL: u32 = 0x200;
const GUARD_REF: u32 = 0x320;
const COPY_REF: u32 = 0x321;
const CREA_REF: u32 = 0x322;
const CONT_REF: u32 = 0x323;
const PLAYER: u32 = 0x14;
// Base effects: value modifiers of one actor value each.
const FX_DT: u32 = 0x840;
const FX_DR: u32 = 0x841;
const FX_LOWER_SPEECH: u32 = 0x842;
const FX_FIRE: u32 = 0x843;
const FX_HEALTH: u32 = 0x844;
const FX_GUNS: u32 = 0x845;
const FX_CHARISMA: u32 = 0x846;
const FX_LOWER_AGILITY: u32 = 0x847;
const FX_STRENGTH: u32 = 0x848;
const FX_DAMAGE_HEALTH: u32 = 0x849;
const FX_RESTORE_HEALTH: u32 = 0x84A;
const SPEL_HIDE: u32 = 0x850;
const SPEL_TOUCH: u32 = 0x851;
const SPEL_GUARD: u32 = 0x852;
const SPEL_PERK: u32 = 0x853;
const ENCH_HAT: u32 = 0x858;
const ARMO_HAT: u32 = 0x859;
const ALCH_CHEM: u32 = 0x85A;
// Actors whose template is a leveled list.
const NPC_A: u32 = 0x870;
const NPC_B: u32 = 0x871;
const NPC_C: u32 = 0x872;
const NPC_SPAWN: u32 = 0x873;
const NPC_LATE: u32 = 0x874;
const NPC_NESTED: u32 = 0x875;
const LVLN_AB: u32 = 0x878;
const LVLN_LATE: u32 = 0x879;
const LVLN_OUTER: u32 = 0x87A;
const SPAWN_REFS: std::ops::Range<u32> = 0x330..0x340;
const LATE_REF: u32 = 0x340;
const NESTED_REF: u32 = 0x341;

const GET: u16 = 0x100E;
const SET: u16 = 0x100F;
const MOD: u16 = 0x1010;
const FORCE: u16 = 0x110E;
const DAMAGE: u16 = 0x1181;
const RESTORE: u16 = 0x1182;
const GET_BASE: u16 = 0x1115;
const GET_PERMANENT: u16 = 0x11EF;
const RESET_HEALTH: u16 = 0x1150;
const GET_HEALTH_PERCENTAGE: u16 = 0x11AF;
const REWARD_KARMA: u16 = 0x1204;
const ADD_SPELL: u16 = 0x101C;
const REMOVE_SPELL: u16 = 0x101D;

const RECOVER: u32 = 1 << 1;
const DETRIMENTAL: u32 = 1 << 2;
const ABILITY: u32 = 4;

/// MGEF: a "Value Modifier" (archetype 0) of one actor value, with its flags.
fn mgef(fid: u32, edid: &str, flags: u32, value: u16) -> Vec<u8> {
    let f32b = |v: f32| v.to_le_bytes().to_vec();
    let data = [u32s(&[flags]), f32b(0.0), u32s(&[0]), vec![0xFF; 8], vec![0; 8], f32b(1.0), vec![0; 32], u32s(&[0, value as u32])].concat();
    assert_eq!(data.len(), 72);
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
}

/// One effect of a magic item: EFID and EFIT (magnitude, area, duration, range Self, no
/// actor value of its own).
fn effect(base: u32, magnitude: u32, duration: u32) -> Vec<fnv_esm::raw::Subrecord> {
    vec![sub(b"EFID", base.to_le_bytes().to_vec()), sub(b"EFIT", [u32s(&[magnitude, 0, duration, 0]), (-1i32).to_le_bytes().to_vec()].concat())]
}

/// CTDA on the subject: `GetActorValue value >= at_least`.
fn value_at_least(value: u16, at_least: f32) -> fnv_esm::raw::Subrecord {
    let mut d = vec![0x60u8, 0, 0, 0];
    d.extend(at_least.to_le_bytes());
    d.extend((GET - 0x1000).to_le_bytes());
    d.extend([0, 0]);
    d.extend(u32s(&[value as u32, 0, 0, 0]));
    sub(b"CTDA", d)
}

/// LVLN: entries of (level, actor base or list), picked from the highest level.
fn lvln(fid: u32, edid: &str, entries: &[(u16, u32)]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0])];
    for &(level, to) in entries {
        subs.push(sub(b"LVLO", [level.to_le_bytes().to_vec(), vec![0; 2], to.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), vec![0; 2]].concat()));
    }
    record(b"LVLN", 0, fid, &subs)
}

/// An NPC_ of the given Strength (the other attributes 1), with a template and its flags.
fn leveled_npc(fid: u32, edid: &str, strength: u8, template: Option<(u32, u16)>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), acbs(0, 1, 100, 0.0, template.map_or(0, |t| t.1))];
    subs.extend(template.map(|t| sub(b"TPLT", t.0.to_le_bytes().to_vec())));
    subs.extend([npc_data(10, [strength, 1, 1, 1, 1, 1, 1]), dnam([strength; 14], [0; 14])]);
    record(b"NPC_", 0, fid, &subs)
}

/// SPEL: an actor effect of the given SPIT type.
fn spel(fid: u32, edid: &str, ty: u32, effects: Vec<fnv_esm::raw::Subrecord>) -> Vec<u8> {
    let spit = [u32s(&[ty, 0, 0]), vec![0; 4]].concat();
    record(b"SPEL", 0, fid, &[vec![sub(b"EDID", z(edid)), sub(b"SPIT", spit)], effects].concat())
}

/// ACBS: flags, fatigue, barter gold, level, calc min/max, speed multiplier, karma,
/// disposition, template flags.
fn acbs(fatigue: u16, level: u16, speed: u16, karma: f32, template_flags: u16) -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    for v in [fatigue, 0, level, 0, 0, speed] {
        d.extend(v.to_le_bytes());
    }
    d.extend(karma.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    d.extend(template_flags.to_le_bytes());
    sub(b"ACBS", d)
}

fn npc_data(health: i32, special: [u8; 7]) -> fnv_esm::raw::Subrecord {
    sub(b"DATA", [health.to_le_bytes().to_vec(), special.to_vec()].concat())
}

fn dnam(values: [u8; 14], offsets: [u8; 14]) -> fnv_esm::raw::Subrecord {
    sub(b"DNAM", [values, offsets].concat())
}

/// AIDT: aggression, confidence, energy, responsibility, mood, …, assistance (byte 14).
fn aidt(aggression: u8, confidence: u8, energy: u8, responsibility: u8, mood: u8, assistance: u8) -> fnv_esm::raw::Subrecord {
    let mut d = vec![aggression, confidence, energy, responsibility, mood, 0, 0, 0, 0, 0, 0, 0, 0, 0, assistance, 0];
    d.extend(0i32.to_le_bytes());
    sub(b"AIDT", d)
}

fn gmst(fid: u32, edid: &str, v: f32) -> Vec<u8> {
    record(b"GMST", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", v.to_le_bytes().to_vec())])
}

fn placed(sig: &[u8; 4], fid: u32, base: u32) -> Vec<u8> {
    record(sig, 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
}

const GUARD_SKILLS: [u8; 14] = [10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23];
const GUARD_OFFSETS: [u8; 14] = [0, 0, 30, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0];

fn plugin_bytes() -> Vec<u8> {
    // The masters hold the skill bases and no primary or Luck multiplier; here one base
    // differs from its default so the setting is seen to be read.
    let gmsts = [gmst(0x801, "fAVDActionPointsBase", 65.0), gmst(0x802, "fAVDActionPointsMult", 3.0), gmst(0x803, "fAVDSkillSmallGunsBase", 4.0)].concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), acbs(50, 1, 100, 0.0, 0), npc_data(100, [5, 6, 7, 4, 8, 9, 3]), dnam([99; 14], [0; 14])]),
        record(
            b"NPC_",
            0,
            NPC_GUARD,
            &[
                sub(b"EDID", z("Guard")),
                acbs(150, 4, 110, -250.0, 0),
                sub(b"SPLO", SPEL_GUARD.to_le_bytes().to_vec()),
                aidt(1, 3, 40, 70, 5, 2),
                npc_data(50, [6, 5, 4, 3, 7, 8, 2]),
                dnam(GUARD_SKILLS, GUARD_OFFSETS),
            ],
        ),
        // Stats (bit 1), Actor Effect List (bit 3) and AI Data (bit 4) from the template;
        // its own are not read.
        record(
            b"NPC_",
            0,
            NPC_COPY,
            &[
                sub(b"EDID", z("GuardCopy")),
                acbs(20, 9, 90, 300.0, 0x1A),
                sub(b"TPLT", NPC_GUARD.to_le_bytes().to_vec()),
                aidt(0, 0, 5, 5, 0, 0),
                npc_data(10, [1; 7]),
                dnam([1; 14], [0; 14]),
            ],
        ),
        leveled_npc(NPC_A, "RaiderA", 9, None),
        leveled_npc(NPC_B, "RaiderB", 7, None),
        leveled_npc(NPC_C, "RaiderC", 3, None),
        // Traits (bit 0) and Stats (bit 1) from a list of A and B at level 1, C at 5.
        leveled_npc(NPC_SPAWN, "RaiderSpawn", 5, Some((LVLN_AB, 0x3))),
        // A list with nothing at level 1.
        leveled_npc(NPC_LATE, "RaiderLate", 5, Some((LVLN_LATE, 0x2))),
        // A list of a list.
        leveled_npc(NPC_NESTED, "RaiderNested", 5, Some((LVLN_OUTER, 0x2))),
    ]
    .concat();
    let lists = [lvln(LVLN_AB, "RaidersAB", &[(1, NPC_A), (1, NPC_B), (5, NPC_C)]), lvln(LVLN_LATE, "RaidersLate", &[(5, NPC_C)]), lvln(LVLN_OUTER, "RaidersOuter", &[(1, LVLN_AB)])]
        .concat();
    // CREA DATA: type, combat, magic and stealth skill, health, unused, damage, attributes.
    let crea_data = [vec![0u8, 65, 50, 40], 120i16.to_le_bytes().to_vec(), vec![0; 2], 9i16.to_le_bytes().to_vec(), vec![8, 7, 6, 2, 3, 4, 5]].concat();
    let crea = record(
        b"CREA",
        0,
        CREA,
        &[
            sub(b"EDID", z("Gecko")),
            acbs(80, 3, 150, 0.0, 0),
            sub(b"SPLO", SPEL_HIDE.to_le_bytes().to_vec()),
            sub(b"SPLO", SPEL_TOUCH.to_le_bytes().to_vec()),
            sub(b"DATA", crea_data),
        ],
    );
    let mgefs = [
        mgef(FX_DT, "IncreaseDamageThreshold", RECOVER, 76),
        mgef(FX_DR, "IncreaseDamageResistance", RECOVER, 18),
        mgef(FX_LOWER_SPEECH, "DecreaseSpeech", RECOVER | DETRIMENTAL, av::SPEECH),
        mgef(FX_FIRE, "IncreaseFireResistance", RECOVER, 52),
        mgef(FX_HEALTH, "IncreaseHealth", RECOVER, av::HEALTH),
        mgef(FX_GUNS, "IncreaseGuns", RECOVER, av::GUNS),
        mgef(FX_CHARISMA, "IncreaseCharisma", RECOVER, av::CHARISMA),
        mgef(FX_LOWER_AGILITY, "DecreaseAgility", RECOVER | DETRIMENTAL, av::AGILITY),
        mgef(FX_STRENGTH, "IncreaseStrength", RECOVER, av::STRENGTH),
        mgef(FX_DAMAGE_HEALTH, "DamageHealth", DETRIMENTAL, av::HEALTH),
        mgef(FX_RESTORE_HEALTH, "RestoreHealth", 0, av::HEALTH),
    ]
    .concat();
    let spels = [
        // The Gecko: Damage Threshold 5; Damage Resistance 10 from Strength 9 up; Speech
        // lowered by 60.
        spel(
            SPEL_HIDE,
            "GeckoHide",
            ABILITY,
            [effect(FX_DT, 5, 0), effect(FX_DR, 10, 0), vec![value_at_least(av::STRENGTH, 9.0)], effect(FX_LOWER_SPEECH, 60, 0)].concat(),
        ),
        // An actor effect that is no ability changes nothing by being listed.
        spel(SPEL_TOUCH, "GeckoTouch", 0, effect(FX_DT, 100, 0)),
        spel(SPEL_GUARD, "GuardTraining", ABILITY, effect(FX_FIRE, 20, 0)),
        // A perk: Health 30 without a condition, Guns 10 with one.
        spel(SPEL_PERK, "PerkAbility", ABILITY, [effect(FX_HEALTH, 30, 0), effect(FX_GUNS, 10, 0), vec![value_at_least(av::STRENGTH, 1.0)]].concat()),
    ]
    .concat();
    let enit = [u32s(&[3, 0, 0]), vec![0; 4]].concat();
    let ench = record(
        b"ENCH",
        0,
        ENCH_HAT,
        &[vec![sub(b"EDID", z("HatEffect")), sub(b"ENIT", enit)], effect(FX_CHARISMA, 1, 0), effect(FX_LOWER_AGILITY, 20, 0)].concat(),
    );
    let armo = record(b"ARMO", 0, ARMO_HAT, &[sub(b"EDID", z("Hat")), sub(b"EITM", ENCH_HAT.to_le_bytes().to_vec())]);
    // A chem: Strength 2 for two seconds; 10 Health damage at once; 4 Health a second
    // restored for two seconds.
    let alch_enit = [u32s(&[5]), vec![0; 4], u32s(&[0]), 0f32.to_le_bytes().to_vec(), u32s(&[0])].concat();
    let alch = record(
        b"ALCH",
        0,
        ALCH_CHEM,
        &[
            vec![sub(b"EDID", z("Chem")), sub(b"DATA", 0.5f32.to_le_bytes().to_vec()), sub(b"ENIT", alch_enit)],
            effect(FX_STRENGTH, 2, 2),
            effect(FX_DAMAGE_HEALTH, 10, 0),
            effect(FX_RESTORE_HEALTH, 4, 2),
        ]
        .concat(),
    );
    let cont = record(b"CONT", 0, CONT, &[sub(b"EDID", z("Box")), sub(b"DATA", [vec![0], 0f32.to_le_bytes().to_vec()].concat())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let mut refs = [placed(b"ACHR", GUARD_REF, NPC_GUARD), placed(b"ACHR", COPY_REF, NPC_COPY), placed(b"ACRE", CREA_REF, CREA), placed(b"REFR", CONT_REF, CONT)].concat();
    for r in SPAWN_REFS {
        refs.extend(placed(b"ACHR", r, NPC_SPAWN));
    }
    refs.extend([placed(b"ACHR", LATE_REF, NPC_LATE), placed(b"ACHR", NESTED_REF, NPC_NESTED)].concat());
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"GMST", &gmsts),
        top(b"NPC_", &npcs),
        top(b"LVLN", &lists),
        top(b"CREA", &crea),
        top(b"CONT", &cont),
        top(b"MGEF", &mgefs),
        top(b"SPEL", &spels),
        top(b"ENCH", &ench),
        top(b"ARMO", &armo),
        top(b"ALCH", &alch),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    plugins: Vec<Plugin>,
    schema: Schema,
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "ActorValues.esm".into()).unwrap()];
    let (data, forms, refs, schema) = {
        let lo = LoadOrder::new(&plugins).unwrap();
        let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
        let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let table = FunctionTable::load(&p).unwrap();
        let forms = FormStore::build(&lo, &schema);
        let refs = RefTable::build(&lo, &schema);
        let data = GameData::build(&lo, &schema, table, &j, &refs);
        (data, forms, refs, schema)
    };
    Fixture { plugins, schema, data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g
}

fn n(x: f64) -> ArgValue {
    ArgValue::Value(Value::Num(x))
}

fn code(c: u16) -> ArgValue {
    ArgValue::Enum(c)
}

/// Call a function by opcode on `on`, failing the test on a call error.
fn call(g: &mut Game, op: u16, on: u32, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(FormId(on)), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    let before = g.events.len();
    let v = g.call_function(&ctx, args);
    for e in &g.events[before..] {
        if let EventKind::CallFailed { name, error, .. } | EventKind::Unhandled { name, unit: error, .. } = &e.kind {
            panic!("0x{op:04X} {name}: {error}");
        }
    }
    match v {
        Value::Num(x) => x,
        Value::Ref(f) => f as f64,
    }
}

fn get(g: &mut Game, on: u32, c: u16) -> f64 {
    call(g, GET, on, &[code(c)])
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn the_records_are_read() {
    let f = fixture();
    let d = &f.data.actors[&FormId(NPC_GUARD)];
    assert_eq!(d.attributes, Some([6, 5, 4, 3, 7, 8, 2]));
    assert_eq!((d.skills, d.skill_offsets), (Some(GUARD_SKILLS), Some(GUARD_OFFSETS)));
    assert_eq!((d.fatigue, d.speed_mult, d.karma), (150, 110, -250.0));
    assert_eq!((d.aggression, d.confidence, d.energy, d.responsibility, d.mood, d.assistance), (Some(1), Some(3), Some(40), Some(70), Some(5), Some(2)));
    let c = &f.data.actors[&FormId(NPC_COPY)];
    assert!(c.template_stats && c.template_ai && !c.template_packages);
    let c = &f.data.actors[&FormId(CREA)];
    assert_eq!((c.attributes, c.combat_skill, c.stealth_skill, c.skills), (Some([8, 7, 6, 2, 3, 4, 5]), Some(65), Some(40), None));
}

#[test]
fn an_npc_starts_at_its_record_and_the_formulas() {
    let f = fixture();
    let mut g = game(&f);
    for (i, want) in [6.0, 5.0, 4.0, 3.0, 7.0, 8.0, 2.0].into_iter().enumerate() {
        assert_eq!(get(&mut g, GUARD_REF, av::STRENGTH + i as u16), want);
    }
    // Skill value plus skill offset.
    for i in 0..14 {
        assert_eq!(get(&mut g, GUARD_REF, av::BARTER + i as u16), (GUARD_SKILLS[i] + GUARD_OFFSETS[i]) as f64);
    }
    assert_eq!(get(&mut g, GUARD_REF, av::ENERGY_WEAPONS), 42.0);
    // ACBS and AIDT.
    assert_eq!(get(&mut g, GUARD_REF, av::FATIGUE), 150.0);
    assert_eq!(get(&mut g, GUARD_REF, av::SPEED_MULTIPLIER), 110.0);
    assert_eq!(get(&mut g, GUARD_REF, av::KARMA), -250.0);
    let ai = [av::AGGRESSION, av::CONFIDENCE, av::ENERGY, av::RESPONSIBILITY, av::MOOD, av::ASSISTANCE];
    for (c, want) in ai.into_iter().zip([1.0, 3.0, 40.0, 70.0, 5.0, 2.0]) {
        assert_eq!(get(&mut g, GUARD_REF, c), want);
    }
    // Derived statistics: Agility 8, Strength 6, Luck 2, Endurance 4, Unarmed 23, level 4.
    assert_eq!(get(&mut g, GUARD_REF, av::ACTION_POINTS), 65.0 + 3.0 * 8.0);
    assert_eq!(get(&mut g, GUARD_REF, av::CARRY_WEIGHT), 150.0 + 10.0 * 6.0);
    assert_eq!(get(&mut g, GUARD_REF, av::CRITICAL_CHANCE), 2.0);
    assert_eq!(get(&mut g, GUARD_REF, av::MELEE_DAMAGE), 3.0);
    assert!(close(get(&mut g, GUARD_REF, av::UNARMED_DAMAGE), 0.5 + 0.05 * 23.0));
    assert_eq!(get(&mut g, GUARD_REF, av::POISON_RESISTANCE), 15.0);
    assert_eq!(get(&mut g, GUARD_REF, av::RAD_RESISTANCE), 6.0);
    assert_eq!(get(&mut g, GUARD_REF, av::HEALTH), 50.0 + 4.0 * 5.0 + 4.0 * 5.0);
    // Limb conditions start whole, the rest at nothing.
    for c in av::LIMB_CONDITIONS {
        assert_eq!(get(&mut g, GUARD_REF, c), 100.0);
    }
    for c in [15, 18, 24, 47, 54, 62, 71, 72, 76] {
        assert_eq!(get(&mut g, GUARD_REF, c), 0.0, "code {c}");
    }
}

#[test]
fn a_template_supplies_the_stats_and_ai_data_it_is_flagged_for() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(get(&mut g, COPY_REF, av::STRENGTH), 6.0);
    assert_eq!(get(&mut g, COPY_REF, av::ENERGY_WEAPONS), 42.0);
    assert_eq!(get(&mut g, COPY_REF, av::FATIGUE), 150.0);
    assert_eq!(get(&mut g, COPY_REF, av::SPEED_MULTIPLIER), 110.0);
    assert_eq!(get(&mut g, COPY_REF, av::CONFIDENCE), 3.0);
    assert_eq!(get(&mut g, COPY_REF, av::ENERGY), 40.0);
    // Karma is its own.
    assert_eq!(get(&mut g, COPY_REF, av::KARMA), 300.0);
}

#[test]
fn a_creature_has_attributes_and_its_combat_skill() {
    let f = fixture();
    let mut g = game(&f);
    for (i, want) in [8.0, 7.0, 6.0, 2.0, 3.0, 4.0, 5.0].into_iter().enumerate() {
        assert_eq!(get(&mut g, CREA_REF, av::STRENGTH + i as u16), want);
    }
    for c in [av::BIG_GUNS, av::ENERGY_WEAPONS, av::EXPLOSIVES, av::MELEE_WEAPONS, av::GUNS] {
        assert_eq!(get(&mut g, CREA_REF, c), 65.0);
    }
    assert_eq!(get(&mut g, CREA_REF, av::SNEAK), 40.0);
    // No field: the formula. Barter from Charisma 2 and Luck 5: 2 + 4 + ⌈2.5⌉.
    assert_eq!(get(&mut g, CREA_REF, av::BARTER), 9.0);
    assert_eq!(get(&mut g, CREA_REF, av::HEALTH), 120.0);
    assert_eq!(get(&mut g, CREA_REF, av::FATIGUE), 80.0);
    assert_eq!(get(&mut g, CREA_REF, av::SPEED_MULTIPLIER), 150.0);
    assert_eq!(get(&mut g, CREA_REF, av::CARRY_WEIGHT), 230.0);
    // Its attributes take the script functions too.
    call(&mut g, SET, CREA_REF, &[code(av::STRENGTH), n(3.0)]);
    call(&mut g, MOD, CREA_REF, &[code(av::STRENGTH), n(2.0)]);
    assert_eq!(get(&mut g, CREA_REF, av::STRENGTH), 5.0);
    assert_eq!(call(&mut g, GET_BASE, CREA_REF, &[code(av::STRENGTH)]), 3.0);
    assert_eq!(call(&mut g, GET_PERMANENT, CREA_REF, &[code(av::STRENGTH)]), 5.0);
    assert_eq!(get(&mut g, CREA_REF, av::CARRY_WEIGHT), 200.0);
}

#[test]
fn the_players_skills_come_from_the_formula_and_follow_the_attributes() {
    let f = fixture();
    let mut g = game(&f);
    // S 5, P 6, E 7, C 4, I 8, A 9, L 3: ⌈3 × 0.5⌉ = 2. The record's DNAM (99s) is not read.
    assert_eq!(get(&mut g, PLAYER, av::BARTER), 2.0 + 2.0 * 4.0 + 2.0);
    assert_eq!(get(&mut g, PLAYER, av::MELEE_WEAPONS), 2.0 + 2.0 * 5.0 + 2.0);
    assert_eq!(get(&mut g, PLAYER, av::UNARMED), 2.0 + 2.0 * 7.0 + 2.0);
    // Guns reads fAVDSkillSmallGunsBase, 4 in this plugin.
    assert_eq!(get(&mut g, PLAYER, av::GUNS), 4.0 + 2.0 * 9.0 + 2.0);
    for (skill, attribute) in [(32, av::CHARISMA), (34, av::PERCEPTION), (37, av::INTELLIGENCE), (42, av::AGILITY), (44, av::ENDURANCE)] {
        assert_eq!(av::governing_attribute(skill), Some(attribute));
    }
    assert_eq!(av::governing_attribute(av::STRENGTH), None);
    // A tagged skill.
    g.player.tag_skills.insert(av::BARTER);
    assert_eq!(get(&mut g, PLAYER, av::BARTER), 27.0);
    // The formula reads the attribute as it is now.
    call(&mut g, DAMAGE, PLAYER, &[code(av::CHARISMA), n(2.0)]);
    assert_eq!(get(&mut g, PLAYER, av::BARTER), 23.0);
    call(&mut g, RESTORE, PLAYER, &[code(av::CHARISMA), n(2.0)]);
    call(&mut g, MOD, PLAYER, &[code(av::LUCK), n(2.0)]);
    assert_eq!(get(&mut g, PLAYER, av::BARTER), 28.0);
    assert_eq!(get(&mut g, PLAYER, av::CRITICAL_CHANCE), 5.0);
    // Within its range: Charisma 4 + 20 counts as 10.
    call(&mut g, MOD, PLAYER, &[code(av::CHARISMA), n(20.0)]);
    assert_eq!(get(&mut g, PLAYER, av::CHARISMA), 24.0);
    assert_eq!(get(&mut g, PLAYER, av::SPEECH), 2.0 + 20.0 + 3.0);
    // SetActorValue puts a number in the formula's place.
    call(&mut g, SET, PLAYER, &[code(av::SPEECH), n(50.0)]);
    assert_eq!(get(&mut g, PLAYER, av::SPEECH), 50.0);
    assert_eq!(get(&mut g, PLAYER, av::SPEED_MULTIPLIER), 100.0);
}

#[test]
fn the_pools_are_kept_apart() {
    let f = fixture();
    let mut g = game(&f);
    let c = av::BARTER;
    // Base 10.
    call(&mut g, MOD, GUARD_REF, &[code(c), n(5.0)]);
    assert_eq!(get(&mut g, GUARD_REF, c), 15.0);
    assert_eq!(call(&mut g, GET_BASE, GUARD_REF, &[code(c)]), 10.0);
    assert_eq!(call(&mut g, GET_PERMANENT, GUARD_REF, &[code(c)]), 15.0);
    // SetActorValue replaces the base; the modifier stays on top.
    call(&mut g, SET, GUARD_REF, &[code(c), n(40.0)]);
    assert_eq!(get(&mut g, GUARD_REF, c), 45.0);
    call(&mut g, DAMAGE, GUARD_REF, &[code(c), n(10.0)]);
    assert_eq!(get(&mut g, GUARD_REF, c), 35.0);
    // ForceActorValue moves the permanent modifier.
    call(&mut g, FORCE, GUARD_REF, &[code(c), n(30.0)]);
    assert_eq!(get(&mut g, GUARD_REF, c), 30.0);
    assert_eq!(call(&mut g, GET_BASE, GUARD_REF, &[code(c)]), 40.0);
    assert_eq!(call(&mut g, GET_PERMANENT, GUARD_REF, &[code(c)]), 40.0);
    call(&mut g, RESTORE, GUARD_REF, &[code(c), n(100.0)]);
    assert_eq!(get(&mut g, GUARD_REF, c), 40.0);
    let parts = |g: &mut Game, r: FormId, c: u16| {
        let p = fnv_game::functions::actor_value_parts(g, r, c).unwrap();
        [p.base, p.permanent, p.temporary, p.damage]
    };
    assert_eq!(parts(&mut g, FormId(GUARD_REF), c), [40.0, 0.0, 0.0, 0.0]);

    // An attribute: the world holds the permanent value, so its carry weight rule sees it.
    let p = FormId::PLAYER_REF;
    call(&mut g, MOD, PLAYER, &[code(av::STRENGTH), n(2.0)]);
    assert_eq!(get(&mut g, PLAYER, av::STRENGTH), 7.0);
    assert_eq!(call(&mut g, GET_BASE, PLAYER, &[code(av::STRENGTH)]), 5.0);
    assert_eq!(g.world.actor_value(p, "strength").unwrap(), 7.0);
    call(&mut g, SET, PLAYER, &[code(av::STRENGTH), n(8.0)]);
    assert_eq!(get(&mut g, PLAYER, av::STRENGTH), 10.0);
    assert_eq!(call(&mut g, GET_BASE, PLAYER, &[code(av::STRENGTH)]), 8.0);
    assert_eq!(g.world.actor_value(p, "strength").unwrap(), 10.0);
    // A value written to the world directly (the menus) is the permanent value.
    g.world.set_actor_value(p, "strength", 6.0).unwrap();
    assert_eq!(get(&mut g, PLAYER, av::STRENGTH), 6.0);
    assert_eq!(call(&mut g, GET_BASE, PLAYER, &[code(av::STRENGTH)]), 4.0);
    // The runner's `given` leaves exactly the value.
    fnv_game::functions::set_actor_value_current(&mut g, p, av::STRENGTH, 3.0).unwrap();
    assert_eq!(parts(&mut g, p, av::STRENGTH), [3.0, 0.0, 0.0, 0.0]);
    assert_eq!(fnv_game::functions::actor_value_capped(&mut g, FormId(COPY_REF), av::ENERGY_WEAPONS), 42.0);
}

#[test]
fn reset_health_heals_and_cures_limbs_and_karma_keeps_its_modifier() {
    let f = fixture();
    let mut g = game(&f);
    call(&mut g, DAMAGE, GUARD_REF, &[code(av::HEALTH), n(30.0)]);
    call(&mut g, DAMAGE, GUARD_REF, &[code(27), n(100.0)]);
    assert_eq!(get(&mut g, GUARD_REF, av::HEALTH), 60.0);
    assert!(close(call(&mut g, GET_HEALTH_PERCENTAGE, GUARD_REF, &[]), 60.0 / 90.0));
    call(&mut g, RESET_HEALTH, GUARD_REF, &[]);
    assert_eq!(get(&mut g, GUARD_REF, av::HEALTH), 90.0);
    assert_eq!(get(&mut g, GUARD_REF, 27), 100.0);
    // Health's maximum follows Endurance.
    call(&mut g, MOD, GUARD_REF, &[code(av::ENDURANCE), n(2.0)]);
    assert_eq!(get(&mut g, GUARD_REF, av::HEALTH), 100.0);

    call(&mut g, MOD, PLAYER, &[code(av::KARMA), n(10.0)]);
    call(&mut g, REWARD_KARMA, PLAYER, &[n(25.0)]);
    assert_eq!(get(&mut g, PLAYER, av::KARMA), 35.0);
    assert_eq!(call(&mut g, GET_BASE, PLAYER, &[code(av::KARMA)]), 25.0);
}

#[test]
fn something_that_is_no_actor_has_no_attributes() {
    let f = fixture();
    let mut g = game(&f);
    let chest = FormId(CONT_REF);
    assert!(fnv_game::functions::actor_value(&mut g, chest, av::STRENGTH).is_err());
    assert_eq!(fnv_game::functions::actor_value(&mut g, chest, av::BARTER).unwrap(), 0.0);
    assert_eq!(fnv_game::functions::actor_value(&mut g, chest, 25).unwrap(), 100.0);
}

fn r(x: u32) -> ArgValue {
    ArgValue::Value(Value::Ref(x))
}

fn parts(g: &mut Game, on: u32, c: u16) -> [f64; 4] {
    let p = fnv_game::functions::actor_value_parts(g, FormId(on), c).unwrap();
    [p.base, p.permanent, p.temporary, p.damage]
}

#[test]
fn the_effect_records_are_read() {
    let f = fixture();
    let d = &f.data;
    assert_eq!(d.actors[&FormId(CREA)].actor_effects, vec![FormId(SPEL_HIDE), FormId(SPEL_TOUCH)]);
    assert!(d.actors[&FormId(NPC_COPY)].template_effects && !d.actors[&FormId(NPC_GUARD)].template_effects);
    assert_eq!((d.spell_types[&FormId(SPEL_HIDE)], d.spell_types[&FormId(SPEL_TOUCH)]), (4, 0));
    assert_eq!(d.enchantments[&FormId(ARMO_HAT)], FormId(ENCH_HAT));
    let fx = &d.magic_effects[&FormId(FX_LOWER_SPEECH)];
    assert_eq!((fx.archetype, fx.actor_value, fx.flags), (0, Some(av::SPEECH), RECOVER | DETRIMENTAL));
    let items = &d.effect_items[&FormId(SPEL_HIDE)];
    assert_eq!(items.iter().map(|e| (e.magnitude, e.actor_value, e.conditions.len())).collect::<Vec<_>>(), vec![(5, None, 0), (10, None, 1), (60, None, 0)]);
    let c = fnv_game::effects::value_change(d, &items[2], 60.0).unwrap();
    assert_eq!((c.code, c.amount, c.recover), (av::SPEECH, -60.0, true));
}

/// The abilities of an actor's record are its temporary modifier [documented: GECK wiki
/// ActorValue]: the base and the permanent value do not move.
#[test]
fn a_records_abilities_are_the_temporary_modifier() {
    let f = fixture();
    let mut g = game(&f);
    // Damage Threshold 5 from the ability; the listed effect that is no ability adds nothing.
    assert_eq!(get(&mut g, CREA_REF, 76), 5.0);
    assert_eq!(call(&mut g, GET_BASE, CREA_REF, &[code(76)]), 0.0);
    assert_eq!(call(&mut g, GET_PERMANENT, CREA_REF, &[code(76)]), 0.0);
    assert_eq!(parts(&mut g, CREA_REF, 76), [0.0, 0.0, 5.0, 0.0]);
    // The other pools stay on top of it.
    call(&mut g, MOD, CREA_REF, &[code(76), n(2.0)]);
    call(&mut g, DAMAGE, CREA_REF, &[code(76), n(1.0)]);
    assert_eq!(get(&mut g, CREA_REF, 76), 6.0);
    assert_eq!(parts(&mut g, CREA_REF, 76), [0.0, 2.0, 5.0, 1.0]);
    // An effect with a condition counts while the condition holds: Strength 8, then 9.
    assert_eq!(get(&mut g, CREA_REF, 18), 0.0);
    call(&mut g, MOD, CREA_REF, &[code(av::STRENGTH), n(1.0)]);
    assert_eq!(get(&mut g, CREA_REF, 18), 10.0);
    call(&mut g, DAMAGE, CREA_REF, &[code(av::STRENGTH), n(1.0)]);
    assert_eq!(get(&mut g, CREA_REF, 18), 0.0);
    // A negative modifier does not take a skill below 0: Speech 9 lowered by 60.
    assert_eq!(call(&mut g, GET_BASE, CREA_REF, &[code(av::SPEECH)]), 9.0);
    assert_eq!(get(&mut g, CREA_REF, av::SPEECH), 0.0);
    assert_eq!(parts(&mut g, CREA_REF, av::SPEECH), [9.0, 0.0, -9.0, 0.0]);
    call(&mut g, MOD, CREA_REF, &[code(av::SPEECH), n(70.0)]);
    assert_eq!(get(&mut g, CREA_REF, av::SPEECH), 19.0);
    // Another value is not held back.
    assert_eq!(fnv_game::effects::limited_temporary(76, -60.0, 9.0), -60.0);
    // The list of a template flagged for it.
    assert_eq!(get(&mut g, GUARD_REF, 52), 20.0);
    assert_eq!(get(&mut g, COPY_REF, 52), 20.0);
    assert_eq!(g.actor_effect_list(FormId(COPY_REF)), [FormId(SPEL_GUARD)].as_slice());
    assert_eq!(get(&mut g, PLAYER, 52), 0.0);
    // Reading changes nothing: no effect was started, no event sent.
    assert!(g.effects.is_empty());
}

/// For the player an ability's effect without conditions is permanent modifier, one
/// with conditions temporary [documented: GECK wiki ActorValue].
#[test]
fn the_players_abilities_without_conditions_are_permanent() {
    let f = fixture();
    let mut g = game(&f);
    let health = get(&mut g, PLAYER, av::HEALTH);
    let guns = get(&mut g, PLAYER, av::GUNS);
    call(&mut g, ADD_SPELL, PLAYER, &[r(SPEL_PERK)]);
    assert_eq!(get(&mut g, PLAYER, av::HEALTH), health + 30.0);
    assert_eq!(call(&mut g, GET_PERMANENT, PLAYER, &[code(av::HEALTH)]), health + 30.0);
    assert_eq!(call(&mut g, GET_BASE, PLAYER, &[code(av::HEALTH)]), health);
    assert_eq!(parts(&mut g, PLAYER, av::HEALTH), [health, 30.0, 0.0, 0.0]);
    assert_eq!(get(&mut g, PLAYER, av::GUNS), guns + 10.0);
    assert_eq!(call(&mut g, GET_PERMANENT, PLAYER, &[code(av::GUNS)]), guns);
    assert_eq!(parts(&mut g, PLAYER, av::GUNS), [guns, 0.0, 10.0, 0.0]);
    // On another actor both are temporary.
    call(&mut g, ADD_SPELL, GUARD_REF, &[r(SPEL_PERK)]);
    assert_eq!(parts(&mut g, GUARD_REF, av::HEALTH), [90.0, 0.0, 30.0, 0.0]);
    // An ability the record lists and a script adds counts once.
    call(&mut g, ADD_SPELL, GUARD_REF, &[r(SPEL_GUARD)]);
    assert_eq!(get(&mut g, GUARD_REF, 52), 20.0);
    call(&mut g, REMOVE_SPELL, PLAYER, &[r(SPEL_PERK)]);
    assert_eq!(get(&mut g, PLAYER, av::HEALTH), health);
    assert_eq!(get(&mut g, PLAYER, av::GUNS), guns);
}

/// The object effect of an equipped item counts while the item is equipped.
#[test]
fn a_worn_object_effect_lasts_while_the_item_is_equipped() {
    let f = fixture();
    let mut g = game(&f);
    let p = FormId::PLAYER_REF;
    g.world.add_item(p, FormId(ARMO_HAT), 1).unwrap();
    assert_eq!(get(&mut g, PLAYER, av::CHARISMA), 4.0);
    g.world.equip(p, FormId(ARMO_HAT)).unwrap();
    assert_eq!(get(&mut g, PLAYER, av::CHARISMA), 5.0);
    assert_eq!(call(&mut g, GET_PERMANENT, PLAYER, &[code(av::CHARISMA)]), 4.0);
    // Agility 9 lowered by 20 stops at 0; the formulas follow (Action Points count it as 1).
    assert_eq!(get(&mut g, PLAYER, av::AGILITY), 0.0);
    assert_eq!(get(&mut g, PLAYER, av::ACTION_POINTS), 65.0 + 3.0);
    // Speech follows Charisma: 2 + 2 × 5 + 2.
    assert_eq!(get(&mut g, PLAYER, av::SPEECH), 14.0);
    g.world.unequip(p, FormId(ARMO_HAT)).unwrap();
    assert_eq!(get(&mut g, PLAYER, av::CHARISMA), 4.0);
    assert_eq!(get(&mut g, PLAYER, av::AGILITY), 9.0);
}

/// A consumed ingestible: a Recover effect is temporary modifier for its duration; one
/// without Recover damages or restores the value and that stays.
#[test]
fn an_ingestible_changes_values_for_its_duration_or_for_good() {
    let f = fixture();
    let mut g = game(&f);
    let p = FormId::PLAYER_REF;
    let health = get(&mut g, PLAYER, av::HEALTH);
    g.world.add_item(p, FormId(ALCH_CHEM), 2).unwrap();
    g.consume(p, FormId(ALCH_CHEM)).unwrap();
    assert_eq!(parts(&mut g, PLAYER, av::STRENGTH), [5.0, 0.0, 2.0, 0.0]);
    assert_eq!(get(&mut g, PLAYER, av::CARRY_WEIGHT), 150.0 + 10.0 * 7.0);
    // The damage is done at once.
    assert_eq!(get(&mut g, PLAYER, av::HEALTH), health - 10.0);
    g.frames(60);
    assert!(close(get(&mut g, PLAYER, av::HEALTH), health - 6.0), "{}", get(&mut g, PLAYER, av::HEALTH));
    assert_eq!(get(&mut g, PLAYER, av::STRENGTH), 7.0);
    g.frames(120);
    // The effects ended: Strength is back, 8 of the 10 Health were restored.
    assert!(g.effects.is_empty());
    assert_eq!(get(&mut g, PLAYER, av::STRENGTH), 5.0);
    assert!(close(get(&mut g, PLAYER, av::HEALTH), health - 2.0));
    assert!(close(parts(&mut g, PLAYER, av::HEALTH)[3], 2.0));
    // Restoring stops at the undamaged value.
    call(&mut g, RESTORE, PLAYER, &[code(av::HEALTH), n(100.0)]);
    g.consume(p, FormId(ALCH_CHEM)).unwrap();
    g.frames(180);
    assert!(close(get(&mut g, PLAYER, av::HEALTH), health - 2.0));
    call(&mut g, RESTORE, PLAYER, &[code(av::HEALTH), n(1.0)]);
    g.apply_magic_item(p, FormId(ALCH_CHEM), Some(p), false);
    call(&mut g, RESTORE, PLAYER, &[code(av::HEALTH), n(9.5)]);
    g.frames(180);
    assert!(close(get(&mut g, PLAYER, av::HEALTH), health));
}

/// An actor whose template is a leveled list has the stats of the entry picked for its
/// reference: an entry of the highest level at or under 1, the same for every reading
/// (actor-values.md §10).
#[test]
fn a_leveled_template_gives_the_stats_of_the_actor_picked_for_the_reference() {
    let f = fixture();
    let mut g = game(&f);
    let lists = &f.data.leveled_actors;
    assert_eq!(lists[&FormId(LVLN_AB)].entries, vec![(1, FormId(NPC_A)), (1, FormId(NPC_B)), (5, FormId(NPC_C))]);
    assert_eq!(f.data.actors[&FormId(NPC_SPAWN)].template_flags, 0x3);
    let mut seen = std::collections::BTreeSet::new();
    for r in SPAWN_REFS {
        let source = g.template_source(FormId(r), av::template::STATS).unwrap();
        // Never C (level 5), never its own placeholder record.
        assert!(source == FormId(NPC_A) || source == FormId(NPC_B), "{r:X}: {source}");
        let strength = if source == FormId(NPC_A) { 9.0 } else { 7.0 };
        assert_eq!(get(&mut g, r, av::STRENGTH), strength);
        // Skills (DNAM), health (Endurance 1, level 1) and the level are the picked actor's.
        assert_eq!(get(&mut g, r, av::GUNS), strength);
        assert_eq!(get(&mut g, r, av::HEALTH), 10.0 + 5.0 + 5.0);
        // The same again, and for the other groups that reach the list.
        assert_eq!(g.template_source(FormId(r), av::template::STATS), Some(source));
        assert_eq!(g.template_source(FormId(r), av::template::TRAITS), Some(source));
        // A group the template does not supply stays with the record.
        assert_eq!(g.template_source(FormId(r), av::template::AI_DATA), Some(FormId(NPC_SPAWN)));
        seen.insert(source);
    }
    // The pick is per reference: sixteen references of one base do not all get the same.
    assert_eq!(seen.len(), 2);
    // A list with no entry at the level: the actor's own record.
    assert_eq!(g.template_source(FormId(LATE_REF), av::template::STATS), Some(FormId(NPC_LATE)));
    assert_eq!(get(&mut g, LATE_REF, av::STRENGTH), 5.0);
    // A list of a list.
    let nested = g.template_source(FormId(NESTED_REF), av::template::STATS).unwrap();
    assert!(nested == FormId(NPC_A) || nested == FormId(NPC_B));
    // Something that is no actor has no source.
    assert_eq!(g.template_source(FormId(CONT_REF), av::template::STATS), None);
}

/// The stats come from the actor base the appearance is made from (`fnv_actor`).
#[test]
fn the_leveled_pick_is_the_one_the_appearance_makes() {
    let f = fixture();
    let g = game(&f);
    let lo = LoadOrder::new(&f.plugins).unwrap();
    let actors = fnv_actor::ActorForms::build(&lo, &f.schema);
    let resolver = fnv_actor::Resolver { actors: &actors, forms: &f.forms };
    for r in SPAWN_REFS.chain([NESTED_REF, GUARD_REF, COPY_REF]) {
        let base = f.refs.get(FormId(r)).unwrap().base;
        for group in [av::template::TRAITS, av::template::STATS, av::template::AI_DATA, av::template::MODEL_ANIMATION, av::template::INVENTORY] {
            assert_eq!(g.template_source(FormId(r), group), resolver.group_source(base, FormId(r), group).ok(), "{r:X} group {group}");
        }
    }
    // The appearance of an actor whose list is empty at level 1 cannot be made.
    assert!(resolver.group_source(FormId(NPC_LATE), FormId(LATE_REF), av::template::STATS).is_err());
}
