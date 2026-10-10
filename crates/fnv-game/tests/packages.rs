//! The typed AI package reader (`fnv_game::package`) on packages built in memory (no
//! game data), in the layout of docs/spec-notes/packages.md §1.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::package::{flags, type_flags, LocationKind, PackageType, Schedule, TargetKind};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, RefTable};
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

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &sig[..], &0i32.to_le_bytes(), &[0; 8], body].concat()
}

fn tes4() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    let payload = write_subrecords(&[sub(b"HEDR", hedr)]);
    [&b"TES4"[..], &(payload.len() as u32).to_le_bytes(), &1u32.to_le_bytes(), &[0; 4], &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn pkdt(general: u32, ty: u8, fallout: u16, specific: u16) -> Subrecord {
    sub(b"PKDT", [&general.to_le_bytes()[..], &[ty, 0], &fallout.to_le_bytes(), &specific.to_le_bytes(), &[0, 0]].concat())
}

fn location(sig: &[u8; 4], ty: i32, value: u32, radius: i32) -> Subrecord {
    sub(sig, [ty.to_le_bytes(), value.to_le_bytes(), radius.to_le_bytes()].concat())
}

fn target(sig: &[u8; 4], ty: i32, value: u32, count: i32) -> Subrecord {
    sub(sig, [ty.to_le_bytes(), value.to_le_bytes(), count.to_le_bytes(), 0f32.to_le_bytes()].concat())
}

fn psdt(month: i8, dow: i8, date: i8, hour: i8, duration: u32) -> Subrecord {
    sub(b"PSDT", [&[month as u8, dow as u8, date as u8, hour as u8][..], &duration.to_le_bytes()].concat())
}

fn ctda(value: f32, function: u16, p1: u32, p2: u32) -> Subrecord {
    let mut d = vec![0u8, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(p1.to_le_bytes());
    d.extend(p2.to_le_bytes());
    d.extend(0u32.to_le_bytes());
    d.extend(0u32.to_le_bytes());
    sub(b"CTDA", d)
}

/// The empty result script of a begin / end / change section.
fn schr() -> Subrecord {
    sub(b"SCHR", vec![0; 20])
}

fn section(marker: &[u8; 4], idle: u32, topic: u32) -> Vec<Subrecord> {
    vec![sub(marker, vec![]), sub(b"INAM", idle.to_le_bytes().to_vec()), schr(), sub(b"TNAM", topic.to_le_bytes().to_vec())]
}

const MARKER: u32 = 0x801;
const CELL: u32 = 0x802;
const FOOD: u32 = 0x803;
const STYLE: u32 = 0x804;
const IDLE_A: u32 = 0x805;
const IDLE_B: u32 = 0x806;
const TOPIC: u32 = 0x807;
const QUEST: u32 = 0x808;
const TRAVEL: u32 = 0x810;
const SANDBOX: u32 = 0x811;
const PATROL: u32 = 0x812;
const EAT: u32 = 0x813;
const SHOOT: u32 = 0x814;
const TALK: u32 = 0x815;
const SHORT: u32 = 0x816;
const FOLLOW: u32 = 0x817;
const GET_STAGE_DONE: u16 = 0x3B;

fn plugin_bytes() -> Vec<u8> {
    let mut travel = vec![
        sub(b"EDID", z("TravelToMarker")),
        pkdt(flags::MUST_REACH_LOCATION | flags::ALWAYS_RUN | flags::ONCE_PER_DAY, 6, 0x0011, 0x0007),
        location(b"PLDT", 0, MARKER, 256),
        psdt(-1, 7, 0, 8, 4),
        ctda(1.0, GET_STAGE_DONE, QUEST, 20),
        ctda(1.0, GET_STAGE_DONE, QUEST, 30),
        sub(b"IDLF", vec![0b101]),
        sub(b"IDLC", vec![2]),
        sub(b"IDLT", 12.5f32.to_le_bytes().to_vec()),
        sub(b"IDLA", [IDLE_A.to_le_bytes(), IDLE_B.to_le_bytes()].concat()),
        sub(b"CNAM", STYLE.to_le_bytes().to_vec()),
    ];
    travel.extend(section(b"POBA", IDLE_A, 0));
    travel.extend(section(b"POEA", 0, TOPIC));
    travel.extend(section(b"POCA", 0, 0));
    let sandbox = vec![
        sub(b"EDID", z("SandboxInCell")),
        pkdt(0, 12, 0, type_flags::NO_SLEEPING | type_flags::NO_WANDERING),
        location(b"PLDT", 1, CELL, 0),
        psdt(-1, -1, 0, -1, 0),
    ];
    let patrol = vec![
        sub(b"EDID", z("PatrolLinked")),
        pkdt(flags::WEAPON_DRAWN, 13, 0, 0),
        location(b"PLDT", 6, 0, 0),
        psdt(-1, -1, 0, 22, 8),
        sub(b"PKPT", vec![1, 0]),
    ];
    let eat = vec![
        sub(b"EDID", z("EatAtMarker")),
        pkdt(0, 3, 0, type_flags::ALLOW_BUYING),
        location(b"PLDT", 3, 0, 512),
        location(b"PLD2", 7, 0, 0),
        psdt(-1, -1, 0, 12, 2),
        target(b"PTDT", 2, 18, 1),
        sub(b"PKED", vec![]),
        // The second copy of location 2 (packages.md §2); a differing one must not win.
        location(b"PLD2", 3, 0, 99),
    ];
    let mut pkw3 = vec![];
    pkw3.extend((1u32 | 1 << 16).to_le_bytes());
    pkw3.extend([1u8, 0]);
    pkw3.extend(3u16.to_le_bytes());
    pkw3.extend(2u16.to_le_bytes());
    pkw3.extend(5u16.to_le_bytes());
    pkw3.extend(0.5f32.to_le_bytes());
    pkw3.extend(1.5f32.to_le_bytes());
    pkw3.extend([0u8; 4]);
    let shoot = vec![
        sub(b"EDID", z("ShootAtPlayer")),
        pkdt(0, 16, 0, 0),
        location(b"PLDT", 2, 0, 0),
        psdt(-1, -1, 0, -1, 0),
        target(b"PTDT", 1, FOOD, 1),
        sub(b"PKW3", pkw3),
        target(b"PTD2", 0, 0x14, 0),
    ];
    let mut pkdd = vec![];
    pkdd.extend(90f32.to_le_bytes());
    pkdd.extend(TOPIC.to_le_bytes());
    pkdd.extend((1u32 << 8).to_le_bytes());
    pkdd.extend([0u8; 4]);
    pkdd.extend(1u32.to_le_bytes());
    pkdd.extend([0u8; 4]);
    let talk = vec![
        sub(b"EDID", z("TalkToPlayer")),
        pkdt(flags::MUST_COMPLETE, 15, 0, 0),
        psdt(-1, -1, 0, -1, 0),
        target(b"PTDT", 0, 0x14, 0),
        sub(b"PKDD", pkdd),
    ];
    // The 8-byte PKDT ends after the Fallout behaviour flags [observed: 13 master
    // packages].
    let short = vec![sub(b"EDID", z("ShortData")), sub(b"PKDT", [&flags::OFFERS_SERVICES.to_le_bytes()[..], &[5, 0, 0x80, 0]].concat())];
    let follow = vec![
        sub(b"EDID", z("FollowLinked")),
        pkdt(flags::DEFENSIVE_COMBAT, 1, 0, 0),
        psdt(-1, -1, 0, -1, 0),
        target(b"PTDT", 3, 0, 128),
        sub(b"PKFD", 300f32.to_le_bytes().to_vec()),
    ];
    let packs = [
        record(b"PACK", TRAVEL, &travel),
        record(b"PACK", SANDBOX, &sandbox),
        record(b"PACK", PATROL, &patrol),
        record(b"PACK", EAT, &eat),
        record(b"PACK", SHOOT, &shoot),
        record(b"PACK", TALK, &talk),
        record(b"PACK", SHORT, &short),
        record(b"PACK", FOLLOW, &follow),
    ]
    .concat();
    [tes4(), top(b"QUST", &record(b"QUST", QUEST, &[sub(b"EDID", z("Q"))])), top(b"PACK", &packs)].concat()
}

fn data() -> GameData {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let refs = RefTable::build(&lo, &schema);
    GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs)
}

#[test]
fn travel_package_reads_flags_location_schedule_conditions_idles_and_sections() {
    let d = data();
    assert!(d.issues.is_empty(), "{:?}", d.issues.to_json());
    let p = &d.package_defs[&FormId(TRAVEL)];
    assert_eq!(p.ty, PackageType::Travel);
    assert_eq!(p.ty.runtime_code(), Some(14));
    assert!(p.has(flags::MUST_REACH_LOCATION) && p.has(flags::ALWAYS_RUN) && p.has(flags::ONCE_PER_DAY));
    assert!(!p.has(flags::MUST_COMPLETE));
    assert_eq!(p.flag_names(), vec!["Must Reach Location", "Once Per Day", "Always Run"]);
    assert_eq!(p.fallout_flags, 0x0011);
    // Sandbox's bits on a Travel package mean nothing.
    assert_eq!(p.type_flags, 7);
    assert!(p.type_flag_names().is_empty());
    let l = p.location.unwrap();
    assert_eq!(l.kind, LocationKind::NearReference(Some(FormId(MARKER))));
    assert_eq!(l.radius, 256);
    assert!(p.location2.is_none() && p.target.is_none());
    assert_eq!(p.schedule, Schedule { month: -1, day_of_week: 7, date: 0, hour: 8, duration: 4 });
    assert!(!p.schedule.is_any());
    // Only the conditions before the sections.
    assert_eq!(p.conditions.len(), 2);
    assert_eq!(p.conditions[0].opcode(), 0x1000 + GET_STAGE_DONE);
    let idles = p.idles.as_ref().unwrap();
    assert!(idles.run_in_sequence && idles.do_once);
    assert_eq!(idles.timer, 12.5);
    assert_eq!(idles.idles, vec![FormId(IDLE_A), FormId(IDLE_B)]);
    assert_eq!(p.combat_style, Some(FormId(STYLE)));
    assert_eq!(p.events[0].idle, Some(FormId(IDLE_A)));
    assert_eq!(p.events[0].topic, None);
    assert_eq!(p.events[1].topic, Some(FormId(TOPIC)));
    assert_eq!(p.events[2], Default::default());
    assert!(!p.uses_linked_ref());
}

#[test]
fn locations_targets_and_type_specific_data() {
    let d = data();
    let sandbox = &d.package_defs[&FormId(SANDBOX)];
    assert_eq!(sandbox.ty, PackageType::Sandbox);
    assert_eq!(sandbox.location.unwrap().kind, LocationKind::InCell(Some(FormId(CELL))));
    assert!(sandbox.schedule.is_any());
    assert!(sandbox.sandbox_flag(type_flags::NO_SLEEPING) && !sandbox.sandbox_flag(type_flags::NO_EATING));
    assert_eq!(sandbox.type_flag_names(), vec!["No Sleeping", "No Wandering"]);

    let patrol = &d.package_defs[&FormId(PATROL)];
    assert_eq!(patrol.ty, PackageType::Patrol);
    assert_eq!(patrol.location.unwrap().kind, LocationKind::NearLinked);
    assert_eq!(patrol.patrol_repeatable, Some(true));
    assert!(patrol.uses_linked_ref());
    assert_eq!((patrol.schedule.hour, patrol.schedule.duration), (22, 8));

    let eat = &d.package_defs[&FormId(EAT)];
    assert_eq!(eat.ty, PackageType::Eat);
    assert_eq!(eat.location.unwrap().kind, LocationKind::NearEditor);
    assert_eq!(eat.location.unwrap().radius, 512);
    // The first of the two copies of location 2.
    assert_eq!(eat.location2.unwrap().kind, LocationKind::AtPackageLocation);
    let t = eat.target.unwrap();
    assert_eq!(t.kind, TargetKind::ObjectType(18));
    assert_eq!(fnv_game::package::object_type_name(18), Some("Food"));
    assert_eq!(t.count_distance, 1);
    assert!(eat.search_flag(type_flags::ALLOW_BUYING));

    let shoot = &d.package_defs[&FormId(SHOOT)];
    assert_eq!(shoot.ty, PackageType::UseWeapon);
    assert_eq!(shoot.location.unwrap().kind, LocationKind::NearCurrent);
    assert_eq!(shoot.target.unwrap().kind, TargetKind::ObjectId(Some(FormId(FOOD))));
    // The player's reference is a form of the game itself.
    assert_eq!(shoot.target2.unwrap().kind, TargetKind::Reference(Some(FormId::PLAYER_REF)));
    let w = shoot.use_weapon.unwrap();
    assert!(w.always_hit && w.crouch_to_reload && !w.do_no_damage && !w.hold_fire_when_blocked);
    assert!(w.volley_fire && !w.repeat_fire);
    assert_eq!((w.bursts, w.shots_per_volley, w.pause_between_volleys), (3, [2, 5], [0.5, 1.5]));

    let talk = &d.package_defs[&FormId(TALK)];
    assert_eq!(talk.ty, PackageType::Dialogue);
    assert!(talk.has(flags::MUST_COMPLETE));
    assert_eq!(talk.target.unwrap().kind, TargetKind::Reference(Some(FormId::PLAYER_REF)));
    let dd = talk.dialogue.unwrap();
    assert_eq!(dd.fov, 90.0);
    assert_eq!(dd.topic, Some(FormId(TOPIC)));
    assert!(dd.dont_control_target_movement && !dd.no_headtracking && dd.say_to);

    let follow = &d.package_defs[&FormId(FOLLOW)];
    assert_eq!(follow.ty, PackageType::Follow);
    assert_eq!(follow.target.unwrap().kind, TargetKind::LinkedReference);
    assert_eq!(follow.target.unwrap().count_distance, 128);
    assert_eq!(follow.follow_trigger_radius, Some(300.0));
    assert!(follow.uses_linked_ref() && follow.has(flags::DEFENSIVE_COMBAT));
}

#[test]
fn the_short_data_form_reads_flags_and_type() {
    let d = data();
    let p = &d.package_defs[&FormId(SHORT)];
    assert_eq!(p.ty, PackageType::Wander);
    assert!(p.has(flags::OFFERS_SERVICES));
    assert_eq!((p.fallout_flags, p.type_flags), (fnv_game::package::fallout::ALLOW_IDLE_CHATTER, 0));
    assert!(p.schedule.is_any());
}
