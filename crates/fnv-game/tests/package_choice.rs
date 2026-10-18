//! AI package choice (`fnv_game::ai`) on a plugin built in memory (no game data):
//! schedules, list order, the linked-reference rule, keeping the current package, Once
//! Per Day, Must Complete, Continue If PC Near and the periodic choice. The rules and
//! their sources: docs/spec-notes/packages.md §3 and §4.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::package::{flags, PackageCheck};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    [s.as_bytes(), &[0]].concat()
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

fn tes4() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x900u32.to_le_bytes()].concat();
    let payload = write_subrecords(&[sub(b"HEDR", hedr)]);
    [&b"TES4"[..], &(payload.len() as u32).to_le_bytes(), &1u32.to_le_bytes(), &[0; 4], &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn glob(fid: u32, edid: &str, v: f32) -> Vec<u8> {
    record(b"GLOB", fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", v.to_le_bytes().to_vec())])
}

/// A package: general flags, type, location (type, value), schedule (hour, duration,
/// day of week) and one optional condition "GetGlobalValue `global` == 1".
struct Pack {
    id: u32,
    edid: &'static str,
    flags: u32,
    ty: u8,
    location: (i32, u32),
    hour: i8,
    duration: u32,
    dow: i8,
    global: Option<u32>,
}

const GET_GLOBAL_VALUE: u16 = 0x4A;

fn pack(p: &Pack) -> Vec<u8> {
    let mut subs = vec![
        sub(b"EDID", z(p.edid)),
        sub(b"PKDT", [&p.flags.to_le_bytes()[..], &[p.ty, 0], &[0; 6]].concat()),
        sub(b"PLDT", [p.location.0.to_le_bytes(), p.location.1.to_le_bytes(), 0i32.to_le_bytes()].concat()),
        sub(b"PSDT", [&[0xFF, p.dow as u8, 0, p.hour as u8][..], &p.duration.to_le_bytes()].concat()),
    ];
    if let Some(g) = p.global {
        let mut d = vec![0u8, 0, 0, 0];
        d.extend(1.0f32.to_le_bytes());
        d.extend(GET_GLOBAL_VALUE.to_le_bytes());
        d.extend([0, 0]);
        d.extend(g.to_le_bytes());
        d.extend([0u8; 12]);
        subs.push(sub(b"CTDA", d));
    }
    record(b"PACK", p.id, &subs)
}

fn npc(fid: u32, edid: &str, packages: &[u32]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    subs.extend(packages.iter().map(|p| sub(b"PKID", p.to_le_bytes().to_vec())));
    record(b"NPC_", fid, &subs)
}

fn placed(sig: &[u8; 4], fid: u32, edid: &str, base: u32, linked: Option<u32>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec())];
    subs.extend(linked.map(|l| sub(b"XLKR", l.to_le_bytes().to_vec())));
    subs.push(sub(b"DATA", vec![0; 24]));
    record(sig, fid, &subs)
}

// Globals.
const G_HOUR: u32 = 0x804;
const G_DAY: u32 = 0x803;
const G_DAYS_PASSED: u32 = 0x805;
const G_SWITCH: u32 = 0x810;
// Packages.
const P_COND: u32 = 0x820;
const P_MORNING: u32 = 0x821;
const P_NIGHT: u32 = 0x822;
const P_DEFAULT: u32 = 0x823;
const P_PATROL: u32 = 0x824;
const P_NIGHT_COND: u32 = 0x825;
const P_DAY: u32 = 0x826;
const P_ONCE: u32 = 0x827;
const P_MUST: u32 = 0x828;
const P_SHOP: u32 = 0x829;
const P_SUNDAY: u32 = 0x82A;
const P_WEEKEND: u32 = 0x82B;
const P_FIND_2H: u32 = 0x82C;
const P_PATROL_3H: u32 = 0x82D;
// Actor bases and references.
const NPC_A: u32 = 0x840;
const NPC_B: u32 = 0x841;
const NPC_C: u32 = 0x842;
const NPC_D: u32 = 0x843;
const NPC_E: u32 = 0x844;
const NPC_F: u32 = 0x845;
const NPC_G: u32 = 0x846;
const NPC_H: u32 = 0x847;
const STAT: u32 = 0x850;
const CELL: u32 = 0x860;
const CELL_FAR: u32 = 0x861;
const MARKER: u32 = 0x870;
const REF_A: u32 = 0x871;
const REF_B_LINKED: u32 = 0x872;
const REF_B_PLAIN: u32 = 0x873;
const REF_C: u32 = 0x874;
const REF_D: u32 = 0x875;
const REF_E: u32 = 0x876;
const REF_F: u32 = 0x877;
const REF_G: u32 = 0x878;
const REF_FAR: u32 = 0x879;
const REF_H: u32 = 0x87A;

const TRAVEL: u8 = 6;
const SLEEP: u8 = 4;
const SANDBOX: u8 = 12;
const PATROL: u8 = 13;
const NEAR_EDITOR: (i32, u32) = (3, 0);
const NEAR_LINKED: (i32, u32) = (6, 0);

fn plugin_bytes() -> Vec<u8> {
    let globs = [
        glob(0x801, "GameYear", 2281.0),
        glob(0x802, "GameMonth", 9.0),
        glob(G_DAY, "GameDay", 19.0),
        glob(G_HOUR, "GameHour", 12.0),
        glob(G_DAYS_PASSED, "GameDaysPassed", 5.0),
        glob(0x806, "TimeScale", 30.0),
        glob(G_SWITCH, "GSwitch", 0.0),
    ]
    .concat();
    let p = |id, edid, flags, ty, location, hour, duration, dow, global| pack(&Pack { id, edid, flags, ty, location, hour, duration, dow, global });
    let packs = [
        p(P_COND, "Conditional", 0, TRAVEL, NEAR_EDITOR, -1, 0, -1, Some(G_SWITCH)),
        p(P_MORNING, "Morning8x4", 0, TRAVEL, NEAR_EDITOR, 8, 4, -1, None),
        p(P_NIGHT, "Night22x8", 0, SLEEP, NEAR_EDITOR, 22, 8, -1, None),
        p(P_DEFAULT, "Default", 0, SANDBOX, NEAR_EDITOR, -1, 0, -1, None),
        p(P_PATROL, "PatrolLinked", 0, PATROL, NEAR_LINKED, -1, 0, -1, None),
        p(P_NIGHT_COND, "NightCond22x8", 0, SLEEP, NEAR_EDITOR, 22, 8, -1, Some(G_SWITCH)),
        p(P_DAY, "Day6x16", 0, SANDBOX, NEAR_EDITOR, 6, 16, -1, None),
        p(P_ONCE, "OnceADay", flags::ONCE_PER_DAY, TRAVEL, NEAR_EDITOR, -1, 0, -1, Some(G_SWITCH)),
        p(P_MUST, "MustComplete", flags::MUST_COMPLETE, TRAVEL, NEAR_EDITOR, -1, 0, -1, Some(G_SWITCH)),
        p(P_SHOP, "Shop8x12", flags::CONTINUE_IF_PC_NEAR, TRAVEL, NEAR_EDITOR, 8, 12, -1, None),
        p(P_SUNDAY, "Sunday", 0, TRAVEL, NEAR_EDITOR, -1, 0, 0, None),
        p(P_WEEKEND, "Weekend", 0, TRAVEL, NEAR_EDITOR, -1, 0, 8, None),
        p(P_FIND_2H, "FindOnceADay2h", flags::ONCE_PER_DAY, 0, NEAR_EDITOR, -1, 2, -1, None),
        p(P_PATROL_3H, "SandboxOnceADay3h", flags::ONCE_PER_DAY, SANDBOX, NEAR_EDITOR, -1, 3, -1, None),
    ]
    .concat();
    let npcs = [
        npc(0x7, "Player", &[]),
        npc(NPC_A, "A", &[P_COND, P_MORNING, P_NIGHT, P_DEFAULT]),
        npc(NPC_B, "B", &[P_PATROL, P_DEFAULT]),
        npc(NPC_C, "C", &[P_NIGHT_COND, P_DAY]),
        npc(NPC_D, "D", &[P_ONCE, P_DEFAULT]),
        npc(NPC_E, "E", &[P_MUST, P_DEFAULT]),
        npc(NPC_F, "F", &[P_SHOP, P_DEFAULT]),
        npc(NPC_G, "G", &[P_SUNDAY, P_WEEKEND, P_DEFAULT]),
        npc(NPC_H, "H", &[P_FIND_2H, P_PATROL_3H, P_DEFAULT]),
    ]
    .concat();
    let stat = record(b"STAT", STAT, &[sub(b"EDID", z("Marker"))]);
    let refs = [
        placed(b"REFR", MARKER, "MarkerREF", STAT, None),
        placed(b"ACHR", REF_A, "ARef", NPC_A, None),
        placed(b"ACHR", REF_B_LINKED, "BLinkedRef", NPC_B, Some(MARKER)),
        placed(b"ACHR", REF_B_PLAIN, "BPlainRef", NPC_B, None),
        placed(b"ACHR", REF_C, "CRef", NPC_C, None),
        placed(b"ACHR", REF_D, "DRef", NPC_D, None),
        placed(b"ACHR", REF_E, "ERef", NPC_E, None),
        placed(b"ACHR", REF_F, "FRef", NPC_F, None),
        placed(b"ACHR", REF_G, "GRef", NPC_G, None),
        placed(b"ACHR", REF_H, "HRef", NPC_H, None),
    ]
    .concat();
    let cell = |id: u32, edid: &str, refs: &[u8]| {
        let rec = record(b"CELL", id, &[sub(b"EDID", z(edid))]);
        [rec, group(id.to_le_bytes(), 6, &group(id.to_le_bytes(), 9, refs))].concat()
    };
    let cells = [cell(CELL, "TestCell", &refs), cell(CELL_FAR, "FarCell", &placed(b"ACHR", REF_FAR, "FarRef", NPC_A, None))].concat();
    [
        tes4(),
        top(b"GLOB", &globs),
        top(b"STAT", &stat),
        top(b"NPC_", &npcs),
        top(b"PACK", &packs),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Packages.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture, package_interval: Option<f64>) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, package_interval, ..GameConfig::default() });
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(CELL));
    g.player_entered(Some(FormId(CELL)));
    g
}

fn set(g: &mut Game, global: u32, v: f32) {
    g.world.set_global(FormId(global), v).unwrap();
}

fn id(x: u32) -> FormId {
    FormId(x)
}

#[test]
fn the_first_package_in_its_window_whose_conditions_pass_runs() {
    let f = fixture();
    assert!(f.data.issues.is_empty(), "{:?}", f.data.issues.to_json());
    let mut g = game(&f, None);
    let a = id(REF_A);
    // Noon: the conditional package fails, the morning and night ones are outside
    // their windows.
    assert_eq!(g.check_package(a, id(P_COND)), PackageCheck::ConditionsFail);
    assert_eq!(g.check_package(a, id(P_MORNING)), PackageCheck::OutOfSchedule);
    assert_eq!(g.check_package(a, id(P_NIGHT)), PackageCheck::OutOfSchedule);
    assert_eq!(g.check_package(a, id(P_DEFAULT)), PackageCheck::Valid);
    assert_eq!(g.check_package(a, id(MARKER)), PackageCheck::Missing);
    assert_eq!(g.evaluate_package(a), Some(id(P_DEFAULT)));
    for (hour, want) in [(8.0, P_MORNING), (11.9, P_MORNING), (12.0, P_DEFAULT), (22.0, P_NIGHT), (23.5, P_NIGHT), (0.5, P_NIGHT), (5.9, P_NIGHT), (6.0, P_DEFAULT), (7.9, P_DEFAULT)] {
        set(&mut g, G_HOUR, hour);
        assert_eq!(g.evaluate_package(a), Some(id(want)), "at {hour}");
    }
    // A package earlier in the list wins as soon as its conditions pass, whatever the
    // hour.
    set(&mut g, G_SWITCH, 1.0);
    assert_eq!(g.evaluate_package(a), Some(id(P_COND)));
    set(&mut g, G_HOUR, 9.0);
    assert_eq!(g.evaluate_package(a), Some(id(P_COND)));
    set(&mut g, G_SWITCH, 0.0);
    assert_eq!(g.evaluate_package(a), Some(id(P_MORNING)));
    // Each change is traced once.
    let chosen = g.events.iter().filter(|e| matches!(e.kind, EventKind::PackageChosen { actor, .. } if actor == a)).count();
    assert_eq!(chosen, 7);
}

#[test]
fn a_package_at_the_linked_reference_needs_one() {
    let f = fixture();
    let mut g = game(&f, None);
    assert_eq!(g.check_package(id(REF_B_LINKED), id(P_PATROL)), PackageCheck::Valid);
    assert_eq!(g.evaluate_package(id(REF_B_LINKED)), Some(id(P_PATROL)));
    assert_eq!(g.check_package(id(REF_B_PLAIN), id(P_PATROL)), PackageCheck::NoLinkedRef);
    assert_eq!(g.evaluate_package(id(REF_B_PLAIN)), Some(id(P_DEFAULT)));
}

#[test]
fn with_no_valid_package_the_actor_keeps_the_one_it_runs() {
    let f = fixture();
    let mut g = game(&f, None);
    let c = id(REF_C);
    // By day the day package; at night the night package's condition fails and the day
    // package is outside its window: the actor keeps the day package.
    assert_eq!(g.evaluate_package(c), Some(id(P_DAY)));
    set(&mut g, G_HOUR, 23.0);
    assert_eq!(g.check_package(c, id(P_DAY)), PackageCheck::OutOfSchedule);
    assert_eq!(g.evaluate_package(c), Some(id(P_DAY)));
    set(&mut g, G_SWITCH, 1.0);
    assert_eq!(g.evaluate_package(c), Some(id(P_NIGHT_COND)));
    // An actor that never ran a package has none to keep.
    let f2 = fixture();
    let mut g2 = game(&f2, None);
    set(&mut g2, G_HOUR, 23.0);
    assert_eq!(g2.evaluate_package(c), None);
    assert_eq!(g2.current_package(c), None);
}

#[test]
fn once_per_day_is_not_chosen_again_within_a_day() {
    let f = fixture();
    let mut g = game(&f, None);
    let d = id(REF_D);
    assert_eq!(g.evaluate_package(d), Some(id(P_DEFAULT)));
    set(&mut g, G_SWITCH, 1.0);
    assert_eq!(g.evaluate_package(d), Some(id(P_ONCE)));
    // Still running: it may go on.
    assert_eq!(g.check_package(d, id(P_ONCE)), PackageCheck::Valid);
    assert_eq!(g.evaluate_package(d), Some(id(P_ONCE)));
    // It ends; its conditions pass again within the day: not chosen.
    set(&mut g, G_SWITCH, 0.0);
    assert_eq!(g.evaluate_package(d), Some(id(P_DEFAULT)));
    set(&mut g, G_SWITCH, 1.0);
    set(&mut g, G_DAYS_PASSED, 5.9);
    assert_eq!(g.check_package(d, id(P_ONCE)), PackageCheck::OncePerDay);
    assert_eq!(g.evaluate_package(d), Some(id(P_DEFAULT)));
    // A day after it was chosen (at 5.0): chosen again.
    set(&mut g, G_DAYS_PASSED, 6.01);
    assert_eq!(g.evaluate_package(d), Some(id(P_ONCE)));
}

#[test]
fn must_complete_holds_the_periodic_choice_but_not_evaluate_package() {
    let f = fixture();
    let mut g = game(&f, None);
    let e = id(REF_E);
    set(&mut g, G_SWITCH, 1.0);
    assert_eq!(g.update_package(e), Some(id(P_MUST)));
    // Its conditions fail now, but it has not reached its goal.
    set(&mut g, G_SWITCH, 0.0);
    assert_eq!(g.package_held(e), Some("must complete"));
    assert_eq!(g.update_package(e), Some(id(P_MUST)));
    // The goal is met: the next periodic choice replaces it.
    g.package_goal_met(e);
    assert_eq!(g.package_held(e), None);
    assert_eq!(g.update_package(e), Some(id(P_DEFAULT)));
    // EvaluatePackage drops even an unfinished Must Complete package.
    set(&mut g, G_SWITCH, 1.0);
    assert_eq!(g.update_package(e), Some(id(P_MUST)));
    set(&mut g, G_SWITCH, 0.0);
    assert_eq!(g.update_package(e), Some(id(P_MUST)));
    assert_eq!(g.evaluate_package(e), Some(id(P_DEFAULT)));
}

#[test]
fn continue_if_pc_near_keeps_the_package_while_the_player_is_in_the_cell() {
    let f = fixture();
    let mut g = game(&f, None);
    let shop = id(REF_F);
    assert_eq!(g.update_package(shop), Some(id(P_SHOP)));
    // Closing time with the player in the shop: it stays open.
    set(&mut g, G_HOUR, 21.0);
    assert_eq!(g.package_held(shop), Some("continue if PC near"));
    assert_eq!(g.update_package(shop), Some(id(P_SHOP)));
    // The player leaves the cell.
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(CELL_FAR));
    assert_eq!(g.package_held(shop), None);
    assert_eq!(g.update_package(shop), Some(id(P_DEFAULT)));
}

#[test]
fn day_of_week_schedules_follow_the_calendar() {
    let f = fixture();
    let mut g = game(&f, None);
    let a = id(REF_G);
    // Seven days in a row: the Sunday package on one of them, the weekend package on
    // the day before it (Saturday), the default on the other five.
    let mut seen = Vec::new();
    for day in 10..17 {
        set(&mut g, G_DAY, day as f32);
        seen.push(g.evaluate_package(a).unwrap().0);
    }
    assert_eq!(seen.iter().filter(|p| **p == P_SUNDAY).count(), 1, "{seen:x?}");
    assert_eq!(seen.iter().filter(|p| **p == P_WEEKEND).count(), 1, "{seen:x?}");
    assert_eq!(seen.iter().filter(|p| **p == P_DEFAULT).count(), 5, "{seen:x?}");
    let sunday = seen.iter().position(|p| *p == P_SUNDAY).unwrap();
    let saturday = seen.iter().position(|p| *p == P_WEEKEND).unwrap();
    assert_eq!((saturday + 1) % 7, sunday);
    // The weekday agrees with GetDayOfWeek's: 13 October 2281 is a Sunday on the game's
    // calendar (observed, `functions::misc::weekday`), so the Sunday of 10..17 is the 13th.
    assert_eq!(sunday, 3);
}

#[test]
fn the_periodic_choice_follows_the_clock() {
    let f = fixture();
    let mut g = game(&f, Some(10.0));
    let a = id(REF_A);
    // The actors of the loaded cell choose at the first tick; the far cell's do not.
    g.tick();
    assert_eq!(g.extra.get(&a).and_then(|e| e.current_package), Some(id(P_DEFAULT)));
    assert_eq!(g.extra.get(&id(REF_B_LINKED)).and_then(|e| e.current_package), Some(id(P_PATROL)));
    assert!(g.extra.get(&id(REF_FAR)).is_none_or(|e| e.current_package.is_none()));
    // The hour moves into the night window: within one interval the actor switches,
    // not at once.
    set(&mut g, G_HOUR, 22.0);
    g.tick();
    let mut frames = 1u64;
    while g.extra[&a].current_package != Some(id(P_NIGHT)) {
        g.tick();
        frames += 1;
        assert!(frames < 60 * 11, "no switch within the interval");
    }
    // Once a switch has happened the next one is a full interval later.
    set(&mut g, G_HOUR, 7.0);
    g.frames(60 * 9);
    assert_eq!(g.extra[&a].current_package, Some(id(P_NIGHT)));
    g.frames(60 * 2);
    assert_eq!(g.extra[&a].current_package, Some(id(P_DEFAULT)));
    // A dead actor no longer chooses.
    g.extra.entry(a).or_default().dead = true;
    set(&mut g, G_HOUR, 9.0);
    g.frames(60 * 12);
    assert_eq!(g.extra[&a].current_package, Some(id(P_DEFAULT)));
    // An actor that runs a package goes on choosing after the player has left.
    let far = id(REF_FAR);
    g.evaluate_package(far);
    assert_eq!(g.extra[&far].current_package, Some(id(P_MORNING)));
    set(&mut g, G_HOUR, 13.0);
    g.frames(60 * 11);
    assert_eq!(g.extra[&far].current_package, Some(id(P_DEFAULT)));
    // Without an interval nothing chooses by itself.
    let mut still = game(&f, None);
    still.frames(60 * 12);
    assert!(still.extra.get(&a).is_none_or(|e| e.current_package.is_none()));
}

/// ResetAI, GetCurrentAIPackage and GetCurrentAIProcedure (docs/design/script-stubs.md
/// §3; the codes are the GECK wiki's tables).
#[test]
fn reset_ai_drops_the_script_package_and_the_codes_follow_the_package() {
    use fnv_game::functions::Ctx;
    use fnv_game::game::FurnitureUse;
    use fnv_script::vm::{ArgValue, Value};
    let f = fixture();
    let mut g = game(&f, None);
    let a = id(REF_A);
    let call = |g: &mut Game, op: u16, args: &[ArgValue]| -> f64 {
        let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(a), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
        g.call_function(&ctx, args).num()
    };
    let (package, procedure) = (0x106E, 0x108F);
    // Noon: the sandbox package.
    assert_eq!((call(&mut g, package, &[]), call(&mut g, procedure, &[])), (36.0, 45.0));
    // A script's travel package: travelling, then done at its goal.
    call(&mut g, 0x1097, &[ArgValue::Value(Value::Ref(P_MORNING))]);
    assert_eq!((call(&mut g, package, &[]), call(&mut g, procedure, &[])), (14.0, 0.0));
    g.package_goal_met(a);
    assert_eq!(call(&mut g, procedure, &[]), 17.0);
    // ResetAI: the script package is gone and the list is chosen from again.
    call(&mut g, 0x11FA, &[]);
    assert!(g.extra[&a].script_package.is_none());
    assert_eq!((call(&mut g, package, &[]), call(&mut g, procedure, &[])), (36.0, 45.0));
    // Night: the sleep package, walking to the bed, lying on it, getting up; a chair.
    set(&mut g, G_HOUR, 23.0);
    g.evaluate_package(a);
    assert_eq!((call(&mut g, package, &[]), call(&mut g, procedure, &[])), (4.0, 0.0));
    let on = |marker_id, state| Some(FurnitureUse { reference: id(MARKER), marker_id, state });
    g.set_furniture_use(a, on(1, 3));
    assert_eq!(call(&mut g, procedure, &[]), 8.0);
    g.set_furniture_use(a, on(1, 4));
    assert_eq!(call(&mut g, procedure, &[]), 21.0);
    g.set_furniture_use(a, on(20, 3));
    assert_eq!(call(&mut g, procedure, &[]), 48.0);
    // In combat 13; fleeing from it 16 FLEE, the package still 5 COMBAT
    // (docs/spec-notes/idles.md §13); fighting again 13.
    g.set_furniture_use(a, None);
    g.start_combat(a, FormId::PLAYER_REF);
    assert_eq!((call(&mut g, package, &[]), call(&mut g, procedure, &[])), (5.0, 13.0));
    g.start_flight(a, FormId::PLAYER_REF, [0.0; 3]);
    assert_eq!((call(&mut g, package, &[]), call(&mut g, procedure, &[])), (5.0, 16.0));
    g.end_flight(a);
    assert_eq!(call(&mut g, procedure, &[]), 13.0);
}

/// SetPackDuration ends the current package that many game minutes after the call (it
/// is chosen again, here the same one, which starts afresh); SetActorsAI 0 keeps the
/// periodic choice away from the actor until SetActorsAI 1 (docs/design/script-stubs.md
/// §3.2).
#[test]
fn set_pack_duration_ends_the_package_and_set_actors_ai_stops_the_choice() {
    use fnv_game::functions::Ctx;
    use fnv_script::vm::{ArgValue, Value};
    let f = fixture();
    let mut g = game(&f, Some(10.0));
    let a = id(REF_A);
    let call = |g: &mut Game, op: u16, x: f64| {
        let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(a), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
        g.call_function(&ctx, &[ArgValue::Value(Value::Num(x))]);
    };
    g.tick();
    assert_eq!(g.extra[&a].current_package, Some(id(P_DEFAULT)));
    call(&mut g, 0x1122, 30.0);
    let until = g.extra[&a].package_ends;
    assert!(until.is_some_and(|(p, t)| p == id(P_DEFAULT) && (t - (5.0 + 30.0 / 1440.0)).abs() < 1e-4), "{until:?}");
    let let_go = |g: &Game, from: usize| g.events[from..].iter().any(|e| matches!(e.kind, EventKind::PackageChosen { actor, package: None } if actor == a));
    // 14 game minutes on: still running.
    set(&mut g, G_DAYS_PASSED, 5.01);
    let from = g.events.len();
    g.tick();
    assert!(!let_go(&g, from) && g.extra[&a].package_ends.is_some());
    // 43 minutes on: it ends and the actor chooses again (the same package, afresh).
    set(&mut g, G_DAYS_PASSED, 5.03);
    let from = g.events.len();
    g.tick();
    assert!(let_go(&g, from), "the timed package was let go");
    assert_eq!((g.extra[&a].current_package, g.extra[&a].package_ends), (Some(id(P_DEFAULT)), None));

    // AI off: the night window comes and goes without a choice; on again, it chooses.
    call(&mut g, 0x118B, 0.0);
    assert!(g.extra[&a].ai_off);
    set(&mut g, G_HOUR, 22.0);
    g.frames(60 * 12);
    assert_eq!(g.extra[&a].current_package, Some(id(P_DEFAULT)));
    call(&mut g, 0x118B, 1.0);
    g.frames(60 * 12);
    assert_eq!(g.extra[&a].current_package, Some(id(P_NIGHT)));
}

/// A list package with a duration and no starting hour ends that many game hours after
/// it was chosen; Once Per Day keeps it from being chosen again that day, so the next
/// package of the list takes over (the Goodsprings geckos' chain: find food 2 h, patrol
/// 3 h, then a sandbox), and a day later the first one is valid again.
#[test]
fn a_schedule_duration_ends_the_package_and_the_next_one_takes_over() {
    let f = fixture();
    let mut g = game(&f, Some(10.0));
    let h = id(REF_H);
    g.tick();
    assert_eq!(g.extra[&h].current_package, Some(id(P_FIND_2H)));
    assert!(g.extra[&h].package_ends_by_schedule);
    for (days, want) in [(5.05, P_FIND_2H), (5.09, P_PATROL_3H), (5.2, P_PATROL_3H), (5.22, P_DEFAULT), (5.9, P_DEFAULT)] {
        set(&mut g, G_DAYS_PASSED, days);
        g.tick();
        assert_eq!(g.extra[&h].current_package, Some(id(want)), "at day {days}");
    }
    // A day after the first choice the find package is valid again.
    set(&mut g, G_DAYS_PASSED, 6.01);
    assert_eq!(g.evaluate_package(h), Some(id(P_FIND_2H)));
    // Its timer starts afresh.
    assert!(g.extra[&h].package_ends.is_some_and(|(p, t)| p == id(P_FIND_2H) && (t - (6.01 + 2.0 / 24.0)).abs() < 1e-4));
}
