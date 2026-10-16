//! The function families of the 2026-10-09 coverage round on a synthetic plugin (no
//! game data): reputations, effect scripts on a consumed ingestible, placed references,
//! typed item removal, actor flags from records, factions, ownership, destruction,
//! regions, challenges and the calendar. docs/spec-notes/scripts.md §7.6.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig, InstKey};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const G_YEAR: u32 = 0x801;
const G_EFF: u32 = 0x807;
const G_TIME: u32 = 0x808;
const G_DONE: u32 = 0x809;
const REP: u32 = 0x820;
const CHAL: u32 = 0x821;
const RACE_CHILD: u32 = 0x822;
const F1: u32 = 0x823;
const F2: u32 = 0x824;
const NPC_CHILD: u32 = 0x830;
const NPC_ADULT: u32 = 0x831;
const CREA: u32 = 0x832;
const MISC: u32 = 0x840;
const WEAP: u32 = 0x841;
const FLST: u32 = 0x842;
const MGEF: u32 = 0x850;
const EFFECT_SCRIPT: u32 = 0x851;
const ALCH: u32 = 0x852;
const ACTI: u32 = 0x860;
const CONT: u32 = 0x861;
const REGN: u32 = 0x870;
const IMAD_FADE: u32 = 0x880;
const IMAD_WHITE: u32 = 0x881;
const WRLD: u32 = 0x900;
const EXT_CELL: u32 = 0x910;
const CELL: u32 = 0x200;
const ACTI_REF: u32 = 0x300;
const CONT_REF: u32 = 0x301;
const CHILD_REF: u32 = 0x320;
const ADULT_REF: u32 = 0x321;
const CREA_REF: u32 = 0x322;
const TACT_ON: u32 = 0x890;
const TACT_OFF: u32 = 0x891;
const STATION_ON: u32 = 0x330;
const STATION_OFF: u32 = 0x331;

const SCRIPT_EFFECT_ELAPSED_SECONDS: u16 = 0x10BA;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn glob(fid: u32, edid: &str, v: f32) -> Vec<u8> {
    record(b"GLOB", 0, fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", f32b(v))])
}

fn placed(sig: &[u8; 4], fid: u32, base: u32, pos: [f32; 3]) -> Vec<u8> {
    let data = [f32b(pos[0]), f32b(pos[1]), f32b(pos[2]), vec![0; 12]].concat();
    record(sig, 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn npc(fid: u32, edid: &str, extra: Vec<fnv_esm::raw::Subrecord>) -> Vec<u8> {
    record(b"NPC_", 0, fid, &[vec![sub(b"EDID", z(edid))], extra].concat())
}

/// DSTD: health percent, index, damage stage, flags, then 16 bytes of zeros.
fn dstd(pct: u8, index: u8, flags: u8) -> fnv_esm::raw::Subrecord {
    sub(b"DSTD", [vec![pct, index, index, flags], vec![0; 16]].concat())
}

fn plugin_bytes() -> Vec<u8> {
    let globs = [
        glob(G_YEAR, "GameYear", 2281.0),
        glob(0x802, "GameMonth", 9.0),
        glob(0x803, "GameDay", 18.0),
        glob(0x804, "GameHour", 12.0),
        glob(0x805, "GameDaysPassed", 5.0),
        glob(0x806, "TimeScale", 30.0),
        glob(G_EFF, "GEff", 0.0),
        glob(G_TIME, "GTime", 0.0),
        glob(G_DONE, "GDone", 0.0),
    ]
    .concat();
    let repu = record(b"REPU", 0, REP, &[sub(b"EDID", z("RepTest")), sub(b"DATA", f32b(20.0))]);
    let chal = record(b"CHAL", 0, CHAL, &[sub(b"EDID", z("ChalTest")), sub(b"DATA", [u32s(&[13, 1, 1, 0]), vec![0; 8]].concat())]);
    let race = record(b"RACE", 0, RACE_CHILD, &[sub(b"EDID", z("ChildRace")), sub(b"DATA", [vec![0; 16], vec![0; 16], u32s(&[4])].concat())]);
    let fact = [
        record(b"FACT", 0, F1, &[sub(b"EDID", z("F1")), sub(b"XNAM", u32s(&[F2, 0, 1])), sub(b"WMI1", REP.to_le_bytes().to_vec())]),
        record(b"FACT", 0, F2, &[sub(b"EDID", z("F2"))]),
    ]
    .concat();
    let snam = |f: u32| sub(b"SNAM", [f.to_le_bytes().to_vec(), vec![0; 4]].concat());
    let npcs = [
        npc(0x7, "Player", vec![]),
        npc(NPC_CHILD, "ChildNpc", vec![sub(b"RNAM", RACE_CHILD.to_le_bytes().to_vec()), snam(F1)]),
        npc(NPC_ADULT, "AdultNpc", vec![snam(F2)]),
    ]
    .concat();
    let crea_data = [vec![6u8, 0, 0, 0], vec![0; 6], vec![0; 7]].concat();
    let crea = record(b"CREA", 0, CREA, &[sub(b"EDID", z("Robot")), sub(b"DATA", crea_data)]);
    let misc = record(b"MISC", 0, MISC, &[sub(b"EDID", z("Junk")), sub(b"DATA", [1i32.to_le_bytes().to_vec(), f32b(1.0)].concat())]);
    let weap_data = [10i32.to_le_bytes().to_vec(), 100i32.to_le_bytes().to_vec(), f32b(2.0), 5i16.to_le_bytes().to_vec(), vec![6]].concat();
    let weap = record(b"WEAP", 0, WEAP, &[sub(b"EDID", z("Gun")), sub(b"DATA", weap_data)]);
    let flst = record(b"FLST", 0, FLST, &[sub(b"EDID", z("GunList")), sub(b"LNAM", WEAP.to_le_bytes().to_vec())]);
    // The base effect: archetype 1 (Script), its script 0x851 (scripts.md §7.6).
    let mgef_data = [u32s(&[0]), f32b(0.0), u32s(&[EFFECT_SCRIPT]), vec![0xFF; 8], vec![0; 4], vec![0; 4], f32b(1.0), vec![0; 24], vec![0; 8], u32s(&[1]), vec![0xFF; 4]].concat();
    assert_eq!(mgef_data.len(), 72);
    let mgef = record(b"MGEF", 0, MGEF, &[sub(b"EDID", z("TestEffect")), sub(b"DATA", mgef_data)]);
    // Effect script: Start sets GEff 1; Update adds ScriptEffectElapsedSeconds to GTime;
    // Finish sets GDone 1.
    let body_start = set(var(b'G', 1), ex(&[a("1")]));
    let body_update = set(var(b'G', 2), ex(&[var(b'G', 2), xcall(SCRIPT_EFFECT_ELAPSED_SECONDS, &[]), a("+")]));
    let body_finish = set(var(b'G', 3), ex(&[a("1")]));
    let escript = [
        vec![sub(b"EDID", z("TestEffectScript"))],
        unit(0x100, &script(&[(0x11, None, body_start), (0x13, None, body_update), (0x12, None, body_finish)]), &[], &[G_EFF, G_TIME, G_DONE]),
    ]
    .concat();
    let scpt = record(b"SCPT", 0, EFFECT_SCRIPT, &escript);
    let enit = [u32s(&[5]), vec![0; 4], u32s(&[0]), f32b(0.0), u32s(&[0])].concat();
    let efit = [u32s(&[0, 0, 1, 0]), (-1i32).to_le_bytes().to_vec()].concat();
    let alch = record(
        b"ALCH",
        0,
        ALCH,
        &[sub(b"EDID", z("TestChem")), sub(b"DATA", f32b(0.5)), sub(b"ENIT", enit), sub(b"EFID", MGEF.to_le_bytes().to_vec()), sub(b"EFIT", efit)],
    );
    let acti = record(
        b"ACTI",
        0,
        ACTI,
        &[
            sub(b"EDID", z("Crate")),
            sub(b"DEST", [100i32.to_le_bytes().to_vec(), vec![2, 0, 0, 0]].concat()),
            dstd(50, 1, 0),
            sub(b"DSTF", vec![]),
            dstd(0, 2, 4),
            sub(b"DSTF", vec![]),
        ],
    );
    let cnto = |item: u32, n: i32| sub(b"CNTO", [item.to_le_bytes().to_vec(), n.to_le_bytes().to_vec()].concat());
    let cont = record(b"CONT", 0, CONT, &[sub(b"EDID", z("Box")), cnto(MISC, 3), cnto(WEAP, 1), sub(b"DATA", [vec![0], f32b(0.0)].concat())]);
    let rpld = [f32b(0.0), f32b(0.0), f32b(4096.0), f32b(0.0), f32b(4096.0), f32b(4096.0), f32b(0.0), f32b(4096.0)].concat();
    let regn = record(
        b"REGN",
        0,
        REGN,
        &[sub(b"EDID", z("TestRegion")), sub(b"WNAM", WRLD.to_le_bytes().to_vec()), sub(b"RPLI", u32s(&[1024])), sub(b"RPLD", rpld)],
    );
    // IMAD DNAM: animatable, duration, then the key counts (not read here).
    let imad_dnam = |animatable: u32, seconds: f32| sub(b"DNAM", [u32s(&[animatable]), f32b(seconds), vec![0; 236]].concat());
    let imad = [
        record(b"IMAD", 0, IMAD_FADE, &[sub(b"EDID", z("FadeTest")), imad_dnam(1, 0.5)]),
        record(b"IMAD", 0, IMAD_WHITE, &[sub(b"EDID", z("WhiteTest")), imad_dnam(0, 1.0)]),
    ]
    .concat();
    // Radio stations: Radio Station and Continuous Broadcast, and Radio Station with
    // Non-Pipboy (the header bits of radio.md §2).
    let tact = [
        record(b"TACT", 0x4002_0000, TACT_ON, &[sub(b"EDID", z("StationOn"))]),
        record(b"TACT", 0x1002_0000, TACT_OFF, &[sub(b"EDID", z("StationOff"))]),
    ]
    .concat();
    let wrld = record(b"WRLD", 0, WRLD, &[sub(b"EDID", z("TestWorld"))]);
    let ext = record(b"CELL", 0, EXT_CELL, &[sub(b"EDID", z("TestExterior")), sub(b"XCLC", [0i32.to_le_bytes(), 0i32.to_le_bytes()].concat())]);
    let world_children = group(WRLD.to_le_bytes(), 1, &group(0i32.to_le_bytes(), 4, &group(0i32.to_le_bytes(), 5, &ext)));
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [
        placed(b"REFR", ACTI_REF, ACTI, [0.0; 3]),
        placed(b"REFR", CONT_REF, CONT, [0.0; 3]),
        placed(b"ACHR", CHILD_REF, NPC_CHILD, [0.0; 3]),
        placed(b"ACHR", ADULT_REF, NPC_ADULT, [0.0; 3]),
        placed(b"ACRE", CREA_REF, CREA, [0.0; 3]),
        placed(b"REFR", STATION_ON, TACT_ON, [0.0; 3]),
        placed(b"REFR", STATION_OFF, TACT_OFF, [0.0; 3]),
    ]
    .concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"GLOB", &globs),
        top(b"REPU", &repu),
        top(b"CHAL", &chal),
        top(b"RACE", &race),
        top(b"FACT", &fact),
        top(b"NPC_", &npcs),
        top(b"CREA", &crea),
        top(b"MISC", &misc),
        top(b"WEAP", &weap),
        top(b"FLST", &flst),
        top(b"MGEF", &mgef),
        top(b"SCPT", &scpt),
        top(b"ALCH", &alch),
        top(b"ACTI", &acti),
        top(b"CONT", &cont),
        top(b"REGN", &regn),
        top(b"IMAD", &imad),
        top(b"TACT", &tact),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
        top(b"WRLD", &[wrld, world_children].concat()),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Functions.esm".into()).unwrap()];
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
    g
}

fn r(x: u32) -> ArgValue {
    ArgValue::Value(Value::Ref(x))
}
fn n(x: f64) -> ArgValue {
    ArgValue::Value(Value::Num(x))
}

/// Call a function by opcode on `on` (or nothing), failing the test on a call error.
fn call(g: &mut Game, op: u16, on: Option<u32>, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: on.map(FormId), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
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

const PLAYER: u32 = 0x14;

#[test]
fn records_read_for_the_new_functions() {
    let f = fixture();
    let d = &f.data;
    assert_eq!(d.reputations[&FormId(REP)], 20.0);
    assert_eq!(d.challenges[&FormId(CHAL)], 1);
    assert_eq!(d.challenge_thresholds[&FormId(CHAL)], 1);
    assert!(d.child_races.contains(&FormId(RACE_CHILD)));
    assert_eq!(d.faction_reactions[&FormId(F1)], vec![(FormId(F2), 1)]);
    assert_eq!(d.actors[&FormId(CREA)].creature_type, Some(6));
    let m = &d.magic_effects[&FormId(MGEF)];
    assert_eq!((m.archetype, m.script), (1, Some(FormId(EFFECT_SCRIPT))));
    assert_eq!(d.effect_items[&FormId(ALCH)][0].duration, 1);
    let dd = &d.destructibles[&FormId(ACTI)];
    let stages: Vec<(u8, u8, u8)> = dd.stages.iter().map(|s| (s.health_percent, s.index, s.flags)).collect();
    assert_eq!((dd.health, stages), (100, vec![(50, 1, 0), (0, 2, 4)]));
    let rg = &d.regions[&FormId(REGN)];
    assert_eq!(rg.world, Some(FormId(WRLD)));
    assert!(rg.contains(100.0, 100.0) && !rg.contains(5000.0, 100.0) && !rg.contains(-1.0, 2000.0));
}

#[test]
fn reputation_points_tiers_and_percentages() {
    let f = fixture();
    let mut g = game(&f);
    // Neutral: every axis 1.
    for axis in 0..3 {
        assert_eq!(call(&mut g, 0x123F, None, &[r(REP), n(axis as f64)]), 1.0);
    }
    // Deed size 3 = 4 points of fame (the documented default); twice = 8 of 20 = level 1.
    call(&mut g, 0x1239, None, &[r(REP), n(1.0), n(3.0)]);
    call(&mut g, 0x1239, None, &[r(REP), n(1.0), n(3.0)]);
    assert_eq!(g.player.reputation[&FormId(REP)], [0.0, 8.0]);
    assert_eq!(call(&mut g, 0x123E, None, &[r(REP), n(1.0)]), 0.4);
    assert_eq!(call(&mut g, 0x123F, None, &[r(REP), n(1.0)]), 4.0); // Accepted
    assert_eq!(call(&mut g, 0x123F, None, &[r(REP), n(0.0)]), 0.0);
    assert_eq!(call(&mut g, 0x123F, None, &[r(REP), n(2.0)]), 0.0);
    // Infamy at level 1 too: equal levels are Mixed (mixed axis 3).
    call(&mut g, 0x123B, None, &[r(REP), n(0.0), n(5.0)]);
    assert_eq!(call(&mut g, 0x123F, None, &[r(REP), n(0.0)]), 3.0);
    assert_eq!(call(&mut g, 0x123F, None, &[r(REP), n(1.0)]), 0.0);
    // RemoveReputation takes the same points back and does not go below 0.
    call(&mut g, 0x123A, None, &[r(REP), n(0.0), n(5.0)]);
    assert_eq!(g.player.reputation[&FormId(REP)][0], 0.0);
    // The grid: each name on its axis.
    use fnv_game::functions::reputation_tiers as t;
    assert_eq!(t(3, 0), [0, 6, 0]);
    assert_eq!(t(0, 2), [0, 0, 5]);
    assert_eq!(t(2, 1), [0, 2, 0]);
    assert_eq!(t(1, 3), [0, 0, 3]);
    assert_eq!(t(3, 3), [5, 0, 0]);
    assert_eq!(t(2, 3), [2, 0, 0]);
    assert_eq!(t(1, 1), [3, 0, 0]);
    assert_eq!(t(0, 0), [1, 1, 1]);
}

/// The round 3 rules [observed: layer5-captures.md §9.3 a] on a scale of 20.
#[test]
fn reputation_limits_levels_and_ignored_sizes() {
    let f = fixture();
    let mut g = game(&f);
    let set = |g: &mut Game, axis: f64, v: f64| {
        call(g, 0x1238, None, &[r(REP), n(axis), n(v)]);
    };
    let fame = |g: &mut Game| g.player.reputation[&FormId(REP)][1];
    // AddReputation stops at the scale, also from above it.
    set(&mut g, 1.0, 19.0);
    call(&mut g, 0x1239, None, &[r(REP), n(1.0), n(5.0)]);
    assert_eq!(fame(&mut g), 20.0);
    set(&mut g, 1.0, 40.0);
    call(&mut g, 0x1239, None, &[r(REP), n(1.0), n(1.0)]);
    assert_eq!(fame(&mut g), 20.0);
    // RemoveReputation stops at 0 and leaves a value above the scale above it.
    set(&mut g, 1.0, 40.0);
    call(&mut g, 0x123A, None, &[r(REP), n(1.0), n(1.0)]);
    assert_eq!(fame(&mut g), 39.0);
    set(&mut g, 1.0, 1.0);
    call(&mut g, 0x123A, None, &[r(REP), n(1.0), n(5.0)]);
    assert_eq!(fame(&mut g), 0.0);
    // Sizes other than 1-5 change nothing.
    set(&mut g, 1.0, 5.0);
    for size in [0.0, 6.0, -2.0] {
        call(&mut g, 0x1239, None, &[r(REP), n(1.0), n(size)]);
        call(&mut g, 0x123A, None, &[r(REP), n(1.0), n(size)]);
    }
    assert_eq!(fame(&mut g), 5.0);
    // SetReputation keeps anything; the percentage is not capped.
    set(&mut g, 1.0, -3.0);
    assert_eq!(call(&mut g, 0x123D, None, &[r(REP), n(1.0)]), -3.0);
    assert_eq!(call(&mut g, 0x123E, None, &[r(REP), n(1.0)]), -0.15);
    set(&mut g, 1.0, 40.0);
    assert_eq!(call(&mut g, 0x123E, None, &[r(REP), n(1.0)]), 2.0);
    // Levels at 15 %, 50 %, 100 %, a boundary counting as the higher level.
    for (v, good) in [(2.9, 1.0), (3.0, 4.0), (9.9, 4.0), (10.0, 5.0), (19.9, 5.0), (20.0, 6.0)] {
        set(&mut g, 1.0, v);
        assert_eq!(call(&mut g, 0x123F, None, &[r(REP), n(1.0)]), good, "fame {v}");
    }
}

/// Damage and restore [documented: catalogue; observed: layer5-captures.md §9.3 b].
#[test]
fn restore_heals_damage_only() {
    let f = fixture();
    let mut g = game(&f);
    let av = |c: u16| ArgValue::Enum(c);
    // Perception condition (25) starts at 100.
    call(&mut g, 0x1181, Some(PLAYER), &[av(25), n(40.0)]);
    assert_eq!(call(&mut g, 0x100E, Some(PLAYER), &[av(25)]), 60.0);
    call(&mut g, 0x1182, Some(PLAYER), &[av(25), n(100.0)]);
    assert_eq!(call(&mut g, 0x100E, Some(PLAYER), &[av(25)]), 100.0);
    call(&mut g, 0x1182, Some(PLAYER), &[av(25), n(100.0)]);
    assert_eq!(call(&mut g, 0x100E, Some(PLAYER), &[av(25)]), 100.0);
    // ForceActorValue moves the permanent modifier so the current value is the target;
    // the base stays.
    call(&mut g, 0x1181, Some(PLAYER), &[av(25), n(10.0)]);
    call(&mut g, 0x110E, Some(PLAYER), &[av(25), n(50.0)]);
    assert_eq!(call(&mut g, 0x100E, Some(PLAYER), &[av(25)]), 50.0);
    assert_eq!(call(&mut g, 0x1115, Some(PLAYER), &[av(25)]), 100.0);
    assert_eq!(call(&mut g, 0x11EF, Some(PLAYER), &[av(25)]), 60.0);
    call(&mut g, 0x1182, Some(PLAYER), &[av(25), n(100.0)]);
    assert_eq!(call(&mut g, 0x100E, Some(PLAYER), &[av(25)]), 60.0);
    // ModActorValue is permanent.
    call(&mut g, 0x1010, Some(PLAYER), &[av(25), n(5.0)]);
    assert_eq!(call(&mut g, 0x100E, Some(PLAYER), &[av(25)]), 65.0);
}

#[test]
fn a_consumed_ingestible_runs_its_effect_script_for_its_duration() {
    let f = fixture();
    let mut g = game(&f);
    let p = FormId::PLAYER_REF;
    g.world.add_item(p, FormId(ALCH), 2).unwrap();
    let ids = g.consume(p, FormId(ALCH)).unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(g.world.item_count(p, FormId(ALCH)).unwrap(), 1);
    // ScriptEffectStart ran inside the call.
    assert_eq!(g.world.global(FormId(G_EFF)).unwrap(), 1.0);
    assert_eq!(call(&mut g, 0x10DF, Some(PLAYER), &[r(ALCH)]), 1.0);
    assert_eq!(call(&mut g, 0x10D6, Some(PLAYER), &[r(MGEF)]), 1.0);
    // ScriptEffectElapsedSeconds is 0 outside an update.
    assert_eq!(call(&mut g, 0x10BA, None, &[]), 0.0);
    g.frames(30);
    let t = g.world.global(FormId(G_TIME)).unwrap();
    assert!((t - 0.5).abs() < 1e-3, "{t}");
    assert_eq!(g.world.global(FormId(G_DONE)).unwrap(), 0.0);
    // Menus stop the effect clock.
    g.menu = Some(1002);
    g.frames(120);
    g.menu = None;
    assert_eq!(g.world.global(FormId(G_TIME)).unwrap(), t);
    g.frames(30);
    assert_eq!(g.world.global(FormId(G_DONE)).unwrap(), 1.0);
    assert!(g.effects.is_empty());
    assert!(!g.instances.keys().any(|k| matches!(k, InstKey::Effect { .. })));
    assert_eq!(call(&mut g, 0x10DF, Some(PLAYER), &[r(ALCH)]), 0.0);
    // EquipItem of an aid item consumes it the same way.
    g.world.set_global(FormId(G_EFF), 0.0).unwrap();
    call(&mut g, 0x10EE, Some(PLAYER), &[r(ALCH)]);
    assert_eq!(g.world.item_count(p, FormId(ALCH)).unwrap(), 0);
    assert_eq!(g.world.global(FormId(G_EFF)).unwrap(), 1.0);
    // Dispel ends it at once, with its finish block.
    g.world.set_global(FormId(G_DONE), 0.0).unwrap();
    call(&mut g, 0x1142, Some(PLAYER), &[r(ALCH)]);
    assert!(g.effects.is_empty());
    assert_eq!(g.world.global(FormId(G_DONE)).unwrap(), 1.0);
}

/// The same item again while its effect is active adds its duration: one effect, one
/// Finish [observed: a second Cateye, layer5-captures.md §9.3 b].
#[test]
fn the_same_item_again_extends_the_active_effect() {
    let f = fixture();
    let mut g = game(&f);
    let p = FormId::PLAYER_REF;
    g.world.add_item(p, FormId(ALCH), 2).unwrap();
    g.consume(p, FormId(ALCH)).unwrap();
    g.frames(30);
    g.world.set_global(FormId(G_EFF), 0.0).unwrap();
    assert!(g.consume(p, FormId(ALCH)).unwrap().is_empty());
    assert_eq!(g.effects.len(), 1);
    assert_eq!(g.effects[0].duration, 2.0);
    // No second start.
    assert_eq!(g.world.global(FormId(G_EFF)).unwrap(), 0.0);
    // 2 s from the first dose: not done after 1.5 s, done after 2 s.
    g.frames(59);
    assert_eq!(g.world.global(FormId(G_DONE)).unwrap(), 0.0);
    g.frames(31);
    assert_eq!(g.world.global(FormId(G_DONE)).unwrap(), 1.0);
    assert!(g.effects.is_empty());
}

#[test]
fn place_at_me_drop_and_typed_removal() {
    let f = fixture();
    let mut g = game(&f);
    // An item makes one reference with the count; another form one per count.
    let junk = call(&mut g, 0x1025, Some(ACTI_REF), &[r(MISC), n(5.0)]) as u32;
    let s = g.world.peek(FormId(junk)).unwrap().clone();
    assert_eq!((s.base, s.count, s.cell), (FormId(MISC), 5, Some(FormId(CELL))));
    call(&mut g, 0x1025, Some(ACTI_REF), &[r(CONT), n(2.0)]);
    let boxes = g.world.created().filter(|(_, s)| s.base == FormId(CONT)).count();
    assert_eq!(boxes, 2);
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::Placed { .. })));
    // Weapons (type 40) move to the player; the junk stays.
    call(&mut g, 0x1249, Some(CONT_REF), &[r(PLAYER), n(0.0), n(0.0), n(40.0)]);
    assert_eq!(g.world.item_count(FormId::PLAYER_REF, FormId(WEAP)).unwrap(), 1);
    assert_eq!(g.world.item_count(FormId(CONT_REF), FormId(MISC)).unwrap(), 3);
    // With no type everything goes, destroyed without a target.
    call(&mut g, 0x1249, Some(CONT_REF), &[]);
    assert_eq!(g.world.item_count(FormId(CONT_REF), FormId(MISC)).unwrap(), 0);
    // Drop: the player's items fall into the world.
    g.world.add_item(FormId::PLAYER_REF, FormId(MISC), 2).unwrap();
    call(&mut g, 0x1057, Some(PLAYER), &[r(MISC), n(2.0)]);
    assert_eq!(g.world.item_count(FormId::PLAYER_REF, FormId(MISC)).unwrap(), 0);
    assert!(g.world.newest_created(FormId(MISC)).is_some());
    // AddItemHealthPercent and the equipped weapon's condition.
    call(&mut g, 0x11BC, Some(PLAYER), &[r(WEAP), n(1.0), n(0.5)]);
    call(&mut g, 0x10EE, Some(PLAYER), &[r(WEAP)]);
    let hp = call(&mut g, 0x11F4, Some(PLAYER), &[]);
    assert!(hp == 100.0 || hp == 50.0);
    call(&mut g, 0x11F5, Some(PLAYER), &[n(40.0)]);
    assert!((call(&mut g, 0x11F4, Some(PLAYER), &[]) - 40.0).abs() < 1e-3);
    call(&mut g, 0x11F6, Some(PLAYER), &[n(-15.0)]);
    assert!((call(&mut g, 0x11F4, Some(PLAYER), &[]) - 25.0).abs() < 1e-3);
    assert_eq!(call(&mut g, 0x118F, Some(PLAYER), &[r(FLST)]), 1.0);
}

#[test]
fn actor_flags_factions_and_essential_state() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(call(&mut g, 0x116D, Some(CHILD_REF), &[]), 1.0);
    assert_eq!(call(&mut g, 0x116D, Some(ADULT_REF), &[]), 0.0);
    assert_eq!(call(&mut g, 0x11B6, Some(CREA_REF), &[n(6.0)]), 1.0);
    assert_eq!(call(&mut g, 0x11B6, Some(CREA_REF), &[n(5.0)]), 0.0);
    // F1 lists F2 as an enemy (FACT XNAM); SetAlly changes both directions.
    assert_eq!(call(&mut g, 0x11C2, Some(CHILD_REF), &[r(ADULT_REF)]), 1.0);
    assert_eq!(call(&mut g, 0x11C2, Some(ADULT_REF), &[r(CHILD_REF)]), 0.0);
    call(&mut g, 0x1079, None, &[r(F1), r(F2), n(1.0)]);
    assert_eq!(call(&mut g, 0x11C2, Some(CHILD_REF), &[r(ADULT_REF)]), 3.0);
    assert_eq!(call(&mut g, 0x119B, None, &[r(F2), r(F1)]), 2.0);
    // The player as a crime enemy of F2.
    call(&mut g, 0x10C6, None, &[r(F2), n(1.0)]);
    assert_eq!(call(&mut g, 0x11CC, Some(ADULT_REF), &[]), 1.0);
    assert_eq!(call(&mut g, 0x11C2, Some(ADULT_REF), &[r(PLAYER)]), 1.0);
    call(&mut g, 0x11C9, None, &[r(F2)]);
    assert_eq!(call(&mut g, 0x10C5, None, &[r(F2)]), 0.0);
    call(&mut g, 0x1214, Some(ADULT_REF), &[]);
    assert_eq!(call(&mut g, 0x1047, Some(ADULT_REF), &[r(F2)]), 0.0);
    // Essential: the base flag covers every reference; the reference flag adds to it.
    assert_eq!(call(&mut g, 0x1162, Some(ADULT_REF), &[]), 0.0);
    call(&mut g, 0x1273, Some(ADULT_REF), &[n(1.0)]);
    assert_eq!(call(&mut g, 0x1162, Some(ADULT_REF), &[]), 1.0);
    call(&mut g, 0x1003, None, &[r(NPC_CHILD), n(1.0)]);
    assert_eq!(call(&mut g, 0x1162, Some(CHILD_REF), &[]), 1.0);
    // KillActor keeps the killer.
    call(&mut g, 0x108B, Some(CHILD_REF), &[r(PLAYER)]);
    assert_eq!(call(&mut g, 0x1199, Some(CHILD_REF), &[r(PLAYER)]), 1.0);
    assert_eq!(call(&mut g, 0x1199, Some(CHILD_REF), &[r(ADULT_REF)]), 0.0);
}

/// Seen thefts and murders against a faction's people add infamy with its reputation
/// (FACT WMI1), up to the scale; unseen ones and other factions' people none.
#[test]
fn crimes_against_a_factions_people_add_infamy() {
    use fnv_game::crime::Kind;
    use fnv_game::functions::{crime_infamy, reputations_of};
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(reputations_of(&g, FormId(F1)), vec![FormId(REP)]);
    assert_eq!(reputations_of(&g, FormId(CHILD_REF)), vec![FormId(REP)]);
    assert!(reputations_of(&g, FormId(ADULT_REF)).is_empty());
    assert!(crime_infamy(&mut g, Kind::Theft, FormId(F1), false).is_empty());
    assert_eq!(crime_infamy(&mut g, Kind::Theft, FormId(F1), true), vec![(FormId(REP), 0.0, 2.0)]);
    assert!(crime_infamy(&mut g, Kind::Trespass, FormId(F1), true).is_empty());
    assert!(crime_infamy(&mut g, Kind::Murder, FormId(ADULT_REF), true).is_empty());
    // 2 + 30 stops at the scale, 20.
    assert_eq!(crime_infamy(&mut g, Kind::Murder, FormId(CHILD_REF), true), vec![(FormId(REP), 2.0, 20.0)]);
    assert_eq!(g.player.reputation[&FormId(REP)], [20.0, 0.0]);
}

#[test]
fn ownership_destruction_regions_challenges_and_calendar() {
    let f = fixture();
    let mut g = game(&f);
    // Ownership: the player by default.
    assert_eq!(call(&mut g, 0x1116, Some(ACTI_REF), &[]), 0.0);
    call(&mut g, 0x1117, Some(ACTI_REF), &[]);
    assert_eq!(call(&mut g, 0x1116, Some(ACTI_REF), &[]), 1.0);
    call(&mut g, 0x1117, Some(ACTI_REF), &[r(F1)]);
    assert_eq!(call(&mut g, 0x1116, Some(ACTI_REF), &[r(F1)]), 1.0);
    call(&mut g, 0x118C, Some(ACTI_REF), &[]);
    assert_eq!(call(&mut g, 0x1116, Some(ACTI_REF), &[r(F1)]), 0.0);
    // Destruction: no damage −1; 60 of 100 → stage 1; 40 more → stage 2, destroyed.
    assert_eq!(call(&mut g, 0x11D7, Some(ACTI_REF), &[]), -1.0);
    call(&mut g, 0x116C, Some(ACTI_REF), &[n(10.0)]);
    assert_eq!(call(&mut g, 0x11D7, Some(ACTI_REF), &[]), 0.0);
    call(&mut g, 0x116C, Some(ACTI_REF), &[n(50.0)]);
    assert_eq!(call(&mut g, 0x11D7, Some(ACTI_REF), &[]), 1.0);
    call(&mut g, 0x116C, Some(ACTI_REF), &[n(40.0)]);
    assert_eq!(call(&mut g, 0x11D7, Some(ACTI_REF), &[]), 2.0);
    assert_eq!(call(&mut g, 0x10CB, Some(ACTI_REF), &[]), 1.0);
    call(&mut g, 0x11D8, Some(ACTI_REF), &[]);
    assert_eq!(call(&mut g, 0x11D7, Some(ACTI_REF), &[]), -1.0);
    assert_eq!(call(&mut g, 0x10CB, Some(ACTI_REF), &[]), 0.0);
    // Regions: never in an interior; in the exterior area, by position.
    assert_eq!(call(&mut g, 0x1260, None, &[r(REGN)]), 0.0);
    {
        let p = g.world.get(FormId::PLAYER_REF).unwrap();
        p.cell = Some(FormId(EXT_CELL));
        p.pos = [100.0, 200.0, 0.0];
    }
    assert_eq!(call(&mut g, 0x1260, None, &[r(REGN)]), 1.0);
    g.world.get(FormId::PLAYER_REF).unwrap().pos = [5000.0, 200.0, 0.0];
    assert_eq!(call(&mut g, 0x1264, None, &[r(REGN)]), 0.0);
    // Challenges and fast travel.
    call(&mut g, 0x125E, None, &[r(CHAL)]);
    assert!(g.player.challenges[&FormId(CHAL)].unlocked);
    assert_eq!(call(&mut g, 0x1266, None, &[r(CHAL)]), 0.0);
    call(&mut g, 0x1111, None, &[n(0.0), n(0.0), n(0.0)]);
    assert!(g.player.fast_travel.travel_disabled && g.player.fast_travel.wait_disabled);
    // From the interior back outside, without the keep flag: travel allowed again.
    g.player_entered(Some(FormId(CELL)));
    g.player_entered(Some(FormId(EXT_CELL)));
    assert!(!g.player.fast_travel.travel_disabled);
    // 18 October 2281 (month 9 counted from 0) is a Tuesday on the Gregorian calendar.
    assert_eq!(call(&mut g, 0x10AA, None, &[]), 2.0);
    // GetHeadingAngle: a target due east of a caller facing north is 90° to the right.
    g.world.get(FormId(ADULT_REF)).unwrap().pos = [0.0, 0.0, 0.0];
    g.world.get(FormId(CHILD_REF)).unwrap().pos = [100.0, 0.0, 0.0];
    assert!((call(&mut g, 0x1063, Some(ADULT_REF), &[r(CHILD_REF)]) - 90.0).abs() < 1e-3);
}

/// Combat C7: object damage through `Game::damage_object` (DamageObject's rule).
#[test]
fn destruction_stages_from_combat_damage() {
    let f = fixture();
    let mut g = game(&f);
    let acti = FormId(ACTI_REF);
    assert!(g.is_destructible(acti));
    // 60 of 100: 40 % left, at or below the 50 % stage.
    let c = g.damage_object(acti, 60.0).unwrap();
    assert_eq!((c.before, c.after, c.destroyed), (-1, 1, false));
    // A small hit changes nothing.
    let c = g.damage_object(acti, 5.0).unwrap();
    assert_eq!((c.before, c.after), (1, 1));
    // The rest reaches the 0 % stage, which destroys it.
    let c = g.damage_object(acti, 100.0).unwrap();
    assert_eq!((c.after, c.destroyed), (2, true));
    assert!(g.damage_object(acti, 10.0).is_none());
    assert_eq!(call(&mut g, 0x11D7, Some(ACTI_REF), &[]), 2.0);
}

/// Image space modifiers (docs/spec-notes/image-modifiers.md §3): an animatable one
/// ends by itself after its duration, the other stays until removed; menus hold their
/// time; a snapshot keeps them.
#[test]
fn image_space_modifiers_play_end_and_are_removed() {
    const APPLY: u16 = 0x1191;
    const REMOVE: u16 = 0x1192;
    const ACTIVE: u16 = 0x1232;
    let f = fixture();
    assert_eq!(f.data.image_modifiers[&FormId(IMAD_FADE)], fnv_game::data::ImageModifierDef { animatable: true, duration: 0.5 });
    assert!(!f.data.image_modifiers[&FormId(IMAD_WHITE)].animatable);
    let mut g = game(&f);
    assert_eq!(call(&mut g, ACTIVE, None, &[r(IMAD_FADE)]), 0.0);
    call(&mut g, APPLY, None, &[r(IMAD_WHITE)]);
    call(&mut g, APPLY, None, &[r(IMAD_FADE)]);
    assert_eq!(g.image_modifiers.iter().map(|m| m.modifier.0).collect::<Vec<_>>(), [IMAD_WHITE, IMAD_FADE]);
    assert_eq!(call(&mut g, ACTIVE, None, &[r(IMAD_FADE)]), 1.0);
    g.frames(12);
    assert!((g.image_modifiers[1].elapsed - 0.2).abs() < 1e-9);
    // A menu holds the modifiers' time.
    g.menu = Some(1002);
    g.frames(120);
    g.menu = None;
    assert!((g.image_modifiers[1].elapsed - 0.2).abs() < 1e-9);
    let snap = g.snapshot();
    assert_eq!(snap.image_modifiers.len(), 2);
    // Applied again, it starts again.
    call(&mut g, APPLY, None, &[r(IMAD_FADE)]);
    assert_eq!(g.image_modifiers[1].elapsed, 0.0);
    g.frames(29);
    assert_eq!(call(&mut g, ACTIVE, None, &[r(IMAD_FADE)]), 1.0);
    g.frames(1);
    // The animated one has ended; the other is still on the screen.
    assert_eq!(call(&mut g, ACTIVE, None, &[r(IMAD_FADE)]), 0.0);
    assert_eq!(call(&mut g, ACTIVE, None, &[r(IMAD_WHITE)]), 1.0);
    assert!(g.events.iter().any(|e| e.kind == EventKind::ImageModifierRemoved { modifier: FormId(IMAD_FADE), ended: true }));
    g.frames(600);
    assert_eq!(call(&mut g, ACTIVE, None, &[r(IMAD_WHITE)]), 1.0);
    call(&mut g, REMOVE, None, &[r(IMAD_WHITE)]);
    assert!(g.image_modifiers.is_empty());
    assert!(g.events.iter().any(|e| e.kind == EventKind::ImageModifierRemoved { modifier: FormId(IMAD_WHITE), ended: false }));
    // Removing one that is not on the screen does nothing.
    let n = g.events.len();
    call(&mut g, REMOVE, None, &[r(IMAD_WHITE)]);
    assert_eq!(g.events.len(), n);
    // A form that is no image space modifier fails the call.
    let ctx = Ctx { unit: "test".into(), opcode: APPLY, on: None, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[r(MISC)]);
    assert!(matches!(&g.events.last().unwrap().kind, EventKind::CallFailed { name, .. } if name == "ApplyImageSpaceModifier"));
    assert!(g.image_modifiers.is_empty());
}

fn s(text: &str) -> ArgValue {
    ArgValue::Str(text.into())
}

/// Radio functions (docs/spec-notes/radio.md §5, §8, §9): the broadcast state from the
/// Continuous Broadcast bit until set, conversations kept for the engine, the Pip-Boy
/// radio's commands and SetNPCRadio's on, off and "turn off first" rule.
#[test]
fn radio_functions() {
    const GET_BROADCAST: u16 = 0x11D4;
    const SET_BROADCAST: u16 = 0x11D5;
    const START_CONVERSATION: u16 = 0x11D6;
    const PIPBOY_RADIO: u16 = 0x11A7;
    const PIPBOY_RADIO_OFF: u16 = 0x1224;
    const SET_NPC_RADIO: u16 = 0x11C4;
    const FORCE_UPDATE: u16 = 0x122C;
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(call(&mut g, GET_BROADCAST, Some(STATION_ON), &[]), 1.0);
    assert_eq!(call(&mut g, GET_BROADCAST, Some(STATION_OFF), &[]), 0.0);
    call(&mut g, SET_BROADCAST, Some(STATION_ON), &[n(0.0)]);
    call(&mut g, SET_BROADCAST, Some(STATION_OFF), &[n(1.0)]);
    assert_eq!(call(&mut g, GET_BROADCAST, Some(STATION_ON), &[]), 0.0);
    assert!(g.broadcasting(FormId(STATION_OFF)));

    call(&mut g, START_CONVERSATION, Some(STATION_OFF), &[r(0x126)]);
    call(&mut g, START_CONVERSATION, Some(STATION_ON), &[]);
    assert_eq!(g.take_radio_conversations(), vec![(FormId(STATION_OFF), Some(FormId(0x126))), (FormId(STATION_ON), None)]);
    assert!(g.take_radio_conversations().is_empty());

    // Tune does nothing while the radio is off; 1 with a station turns it on and asks for it.
    call(&mut g, PIPBOY_RADIO, None, &[s("tune"), r(STATION_ON)]);
    assert_eq!((g.radio.pipboy.on, g.radio.pipboy.tune), (false, None));
    call(&mut g, PIPBOY_RADIO, None, &[s("1"), r(STATION_ON)]);
    assert_eq!((g.radio.pipboy.on, g.radio.pipboy.tune), (true, Some(FormId(STATION_ON))));
    g.radio.pipboy.tune = None;
    call(&mut g, PIPBOY_RADIO, None, &[s("tune"), r(STATION_OFF)]);
    assert_eq!(g.radio.pipboy.tune, Some(FormId(STATION_OFF)));
    call(&mut g, PIPBOY_RADIO, None, &[n(0.0)]);
    assert!(!g.radio.pipboy.on);
    call(&mut g, PIPBOY_RADIO, None, &[n(1.0)]);
    assert!(g.radio.pipboy.on);
    call(&mut g, PIPBOY_RADIO_OFF, None, &[]);
    assert!(!g.radio.pipboy.on);
    assert_eq!(g.radio.pipboy.script_calls, 6);

    call(&mut g, SET_NPC_RADIO, Some(ADULT_REF), &[n(1.0), r(STATION_ON)]);
    // A second station is not taken until the first is turned off.
    call(&mut g, SET_NPC_RADIO, Some(ADULT_REF), &[n(1.0), r(STATION_OFF)]);
    assert_eq!(g.radio.npc_radios.get(&FormId(ADULT_REF)), Some(&FormId(STATION_ON)));
    call(&mut g, SET_NPC_RADIO, Some(ADULT_REF), &[n(0.0), r(STATION_OFF)]);
    assert!(g.radio.npc_radios.is_empty());
    call(&mut g, FORCE_UPDATE, None, &[]);
    assert_eq!(g.radio.force_updates, 1);
}

/// The stubs implemented by the audit of 2026-10-12 (docs/design/script-stubs.md §3):
/// what the engine tells (movement, sight, the camera), heads held by Look, subtitles,
/// the weather, scripted challenges and the menus' requests.
#[test]
fn audited_stubs_answer_from_the_engine_state_and_leave_requests() {
    use fnv_game::chargen::{SpecialSheet, TagSheet};
    use fnv_game::functions::senses::{Look, Motion};
    use fnv_game::functions::ui::SpecialMenu;
    let f = fixture();
    let mut g = game(&f);
    let (adult, child) = (FormId(ADULT_REF), FormId(CHILD_REF));

    // Look holds the head on the target until StopLook; the body flag is kept.
    call(&mut g, 0x105C, Some(ADULT_REF), &[r(CHILD_REF), n(1.0)]);
    assert_eq!(g.script_look(adult), Some(Look { target: child, rotate_body: true }));
    call(&mut g, 0x105D, Some(ADULT_REF), &[]);
    assert_eq!(g.script_look(adult), None);

    // Movement and sneaking as the engine told them; nothing told is standing still.
    assert_eq!(call(&mut g, 0x111E, None, &[]), 0.0);
    g.set_motion(FormId::PLAYER_REF, Motion { moving: true, running: false, sneaking: true, turning: 0 });
    assert_eq!(call(&mut g, 0x111E, None, &[]), 1.0);
    assert_eq!(call(&mut g, 0x1019, None, &[]), 1.0);
    assert_eq!(call(&mut g, 0x111F, None, &[]), 0.0);
    g.set_motion(adult, Motion { moving: true, running: true, sneaking: false, turning: 0 });
    assert_eq!(call(&mut g, 0x111F, Some(ADULT_REF), &[]), 1.0);
    g.forget_motion(adult);
    assert_eq!(call(&mut g, 0x1019, Some(ADULT_REF), &[]), 0.0);

    // Line of sight from the detection the engine measured for the pair.
    assert_eq!(call(&mut g, 0x101B, Some(ADULT_REF), &[r(CHILD_REF)]), 0.0);
    let seen = fnv_game::detection::Detection { seen: true, ..Default::default() };
    g.detection.record(adult, child, seen, g.time);
    assert_eq!(call(&mut g, 0x101B, Some(ADULT_REF), &[r(CHILD_REF)]), 1.0);

    // The camera: first person until the engine says otherwise.
    assert_eq!(call(&mut g, 0x1188, None, &[]), 1.0);
    g.set_first_person(false);
    assert_eq!(call(&mut g, 0x1188, None, &[]), 0.0);

    // Subtitles shown out of hearing too.
    call(&mut g, 0x1277, Some(ADULT_REF), &[n(1.0)]);
    assert!(g.always_subtitles(adult));
    call(&mut g, 0x1277, Some(ADULT_REF), &[n(0.0)]);
    assert!(!g.always_subtitles(adult));

    // The weather: a request for the sky, current from the call; then what the sky shows.
    let (rain, clear) = (0x999u32, 0x998u32);
    call(&mut g, 0x1104, None, &[r(rain), n(1.0)]);
    assert_eq!(call(&mut g, 0x1095, None, &[r(rain)]), 1.0);
    let req = g.take_weather_request().unwrap();
    assert_eq!((req.weather, req.instant, req.hold), (FormId(rain), false, true));
    call(&mut g, 0x112D, None, &[r(clear)]);
    assert!(g.take_weather_request().is_some_and(|r| r.instant && !r.hold));
    g.set_shown_weather(FormId(rain), true);
    assert_eq!(call(&mut g, 0x103E, None, &[]), 1.0);
    assert_eq!(call(&mut g, 0x1095, None, &[r(clear)]), 0.0);
    call(&mut g, 0x1158, None, &[]);
    assert!(g.take_weather_release() && !g.take_weather_release());

    // A Start Disabled challenge with threshold 1: no progress until unlocked, then done
    // and, not recurring, held at its threshold.
    call(&mut g, 0x1275, None, &[r(CHAL)]);
    assert_eq!(call(&mut g, 0x1266, None, &[r(CHAL)]), 0.0);
    call(&mut g, 0x125E, None, &[r(CHAL)]);
    call(&mut g, 0x1275, None, &[r(CHAL)]);
    assert_eq!(call(&mut g, 0x1266, None, &[r(CHAL)]), 1.0);
    call(&mut g, 0x1275, None, &[r(CHAL)]);
    assert_eq!(g.player.challenges[&FormId(CHAL)].progress, 1);

    // The S.P.E.C.I.A.L. menu with two points to add, the old values its floor.
    call(&mut g, 0x1219, None, &[n(2.0)]);
    assert_eq!((g.menu, g.ui.special), (Some(1074), Some(SpecialMenu::Add(2))));
    let s = SpecialSheet::for_menu(&mut g);
    assert_eq!((s.remaining(), s.floor), (2, s.values));
    assert_eq!(g.ui.special, None, "used up by the opening");
    call(&mut g, 0x1218, None, &[n(30.0)]);
    let s = SpecialSheet::for_menu(&mut g);
    assert_eq!(s.total, 30);
    // The tag-skill menu with one more to tag.
    g.player.tag_skills.extend([41u16, 43, 44]);
    call(&mut g, 0x121B, None, &[n(1.0)]);
    assert_eq!(g.menu, Some(1048));
    let t = TagSheet::for_menu(&mut g);
    assert_eq!((t.count, t.remaining(), t.locked.len()), (4, 1, 3));

    // A terminal's result script asks to go back; a quest that is no quest is refused.
    call(&mut g, 0x1223, None, &[]);
    assert!(g.terminals.back);
    let ctx = Ctx { unit: "test".into(), opcode: 0x11DD, on: None, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[r(CHAL)]);
    assert!(matches!(&g.events.last().unwrap().kind, EventKind::CallFailed { .. }));
    assert_eq!(g.ui.active_quest, None);

    // The deliberate no-ops: handled, no stub recorded.
    let stubbed = g.stubbed.len();
    call(&mut g, 0x11FB, None, &[n(1.0), n(1.0), n(0.5)]);
    call(&mut g, 0x1189, None, &[]);
    assert_eq!(g.stubbed.len(), stubbed);
}

/// The stubs implemented on 2026-10-12 by claude/script-stubs-2
/// (docs/design/script-stubs.md §3.2).
#[test]
fn second_round_stubs() {
    use fnv_game::functions::senses::Motion;
    let f = fixture();
    let mut g = game(&f);
    let adult = FormId(ADULT_REF);

    // GetXPForNextLevel: level 2 needs 200 in all (iXPBase, the wiki's defaults here);
    // what the player has counts.
    g.player.level = Some(1);
    assert_eq!(call(&mut g, 0x1215, None, &[]), 200.0);
    call(&mut g, 0x1177, None, &[n(50.0)]);
    assert_eq!(call(&mut g, 0x1215, None, &[]), 150.0);
    g.player.level = Some(2);
    assert_eq!(call(&mut g, 0x1215, None, &[]), 500.0, "level 3 at 550");

    // IsDLCInstalled: a plugin of the load order by name, spaces and case ignored.
    assert_eq!(call(&mut g, 0x126F, None, &[s("functions")]), 1.0);
    assert_eq!(call(&mut g, 0x126F, None, &[s("Tribal Pack")]), 0.0);

    // ShowAllMapMarkers 1 0 0 (the Explorer perk): every marker, the hidden ones too.
    let marker = FormId(ACTI_REF);
    assert_eq!(call(&mut g, 0x11EC, Some(ACTI_REF), &[]), 0.0);
    call(&mut g, 0x11F8, None, &[n(1.0), n(0.0), n(0.0)]);
    assert!(g.map_marker_shown(marker, true));
    assert_eq!(call(&mut g, 0x11EC, Some(ACTI_REF), &[]), 1.0);
    call(&mut g, 0x11F8, None, &[n(1.0)]);
    assert!(g.map_marker_shown(marker, false) && !g.map_marker_shown(marker, true), "skips hidden by default");
    call(&mut g, 0x11F8, None, &[n(0.0)]);
    assert!(!g.map_marker_shown(marker, false));

    // SetGlobalTimeMultiplier: kept for the engine; 0 refused.
    call(&mut g, 0x1186, None, &[n(0.5), n(1.0)]);
    assert_eq!(g.ui.time_multiplier, Some(0.5));

    // IsTurning and IsPlayerMovingIntoNewSpace from what the engine tells.
    assert_eq!(call(&mut g, 0x101A, Some(ADULT_REF), &[]), 0.0);
    g.set_motion(adult, Motion { turning: 2, ..Motion::default() });
    assert_eq!(call(&mut g, 0x101A, Some(ADULT_REF), &[]), 2.0);
    assert_eq!(call(&mut g, 0x1166, None, &[]), 0.0);
    g.loading = true;
    assert_eq!(call(&mut g, 0x1166, None, &[]), 1.0);
    g.loading = false;

    // Rotate: the frame's last call turns the reference by speed × frame time, once.
    g.cfg.frame_seconds = 0.5;
    let z0 = g.world.get(FormId(ACTI_REF)).unwrap().rot[2];
    call(&mut g, 0x1004, Some(ACTI_REF), &[ArgValue::Axis('Z'), n(10.0)]);
    call(&mut g, 0x1004, Some(ACTI_REF), &[ArgValue::Axis('Z'), n(90.0)]);
    g.tick();
    let z1 = g.world.get(FormId(ACTI_REF)).unwrap().rot[2];
    assert!((z1 - z0 - 45f32.to_radians()).abs() < 1e-5, "{z0} -> {z1}");
    assert!(g.turned.contains(&FormId(ACTI_REF)));
    g.tick();
    assert_eq!(g.world.get(FormId(ACTI_REF)).unwrap().rot[2], z1, "no call, no turn");
    assert!(g.turned.is_empty());
}

/// SwapTexture (playtest list 5 item 38): the shape's name and the texture's path kept
/// per reference for the engine, the last call per shape winning. The reference is the
/// first parameter, the calls having no calling reference (Doc Mitchell's lines:
/// `SwapTexture TempRorschachTest01REF "PipBoyCard01:0" "…"`).
#[test]
fn swap_texture_keeps_the_last_texture_per_shape() {
    let f = fixture();
    let mut g = game(&f);
    let card = FormId(ADULT_REF);
    call(&mut g, 0x1269, None, &[r(ADULT_REF), s("pipboycard01:0"), s(r"clutter\rorchachtest\inkblot03")]);
    call(&mut g, 0x1269, None, &[r(ADULT_REF), s("##PipBoyCard01:0"), s(r"clutter\rorchachtest\inkblot01")]);
    assert_eq!(g.texture_swaps.generation, 2);
    assert_eq!(g.texture_swaps.by_ref[&card], vec![("pipboycard01:0".to_string(), r"textures\clutter\rorchachtest\inkblot01.dds".to_string())]);
}

/// PlayMagicShaderVisuals / StopMagicShaderVisuals: one play per (reference, shader), a
/// second call starting it over; a duration kept; no calling reference plays it on the
/// player; Stop takes it away (docs/spec-notes/effect-shaders.md §3).
#[test]
fn magic_shader_visuals_play_and_stop() {
    let f = fixture();
    let mut g = game(&f);
    const SHADER: u32 = 0x0007_EA60;
    call(&mut g, 0x1123, Some(ADULT_REF), &[r(SHADER)]);
    call(&mut g, 0x1123, Some(ADULT_REF), &[r(SHADER), n(5.0)]);
    call(&mut g, 0x1123, None, &[r(SHADER)]);
    let plays = &g.script_looks.shaders;
    assert_eq!(plays.len(), 2);
    assert_eq!((plays[0].on, plays[0].duration), (FormId(ADULT_REF), Some(5.0)));
    assert_eq!((plays[1].on, plays[1].duration), (FormId::PLAYER_REF, None));
    assert!(plays[1].serial > plays[0].serial);
    call(&mut g, 0x1125, Some(ADULT_REF), &[r(SHADER)]);
    assert_eq!(g.script_looks.shaders.len(), 1);
    let serial = g.script_looks.shaders[0].serial;
    g.script_looks.end(serial);
    assert!(g.script_looks.shaders.is_empty());
}

/// SetActorAlpha: kept per actor below 1, held to 0..1; 1 takes it away.
#[test]
fn set_actor_alpha_is_kept_below_one() {
    let f = fixture();
    let mut g = game(&f);
    let who = FormId(ADULT_REF);
    call(&mut g, 0x1129, Some(ADULT_REF), &[n(0.5)]);
    assert_eq!(g.script_looks.alpha.get(&who), Some(&0.5));
    call(&mut g, 0x1129, Some(ADULT_REF), &[n(-2.0)]);
    assert_eq!(g.script_looks.alpha.get(&who), Some(&0.0));
    call(&mut g, 0x1129, Some(ADULT_REF), &[n(1.0)]);
    assert!(g.script_looks.alpha.is_empty());
}

/// SetSecuritronExpression: the face and expression name the screen's texture, swapped on
/// the screen shape of the target (effect-shaders.md §5).
#[test]
fn securitron_expression_swaps_the_screen() {
    let f = fixture();
    let mut g = game(&f);
    call(&mut g, 0x1248, None, &[r(ADULT_REF), s("Victor"), s("Alert")]);
    assert_eq!(
        g.texture_swaps.by_ref[&FormId(ADULT_REF)],
        vec![("screen01:0".to_string(), r"textures\creatures\securitron\victor_alert.dds".to_string())]
    );
}

/// Reset3DState: the reference's group no longer counts as playing, its waiting requests
/// are dropped and the engine is told once.
#[test]
fn reset_3d_state_forgets_the_animation() {
    let f = fixture();
    let mut g = game(&f);
    let on = FormId(ADULT_REF);
    g.anims.driven = true;
    g.ask_anim(fnv_game::anims::AnimRequest { on, group: 1, name: Some("Forward".into()), start: fnv_game::anims::Start::Normal });
    assert_eq!(g.anims.playing(on), Some(1));
    call(&mut g, 0x1146, Some(ADULT_REF), &[]);
    call(&mut g, 0x1146, Some(ADULT_REF), &[]);
    assert_eq!(g.anims.playing(on), None);
    assert!(g.anims.requests.is_empty());
    assert_eq!(g.anims.resets, vec![on]);
}
