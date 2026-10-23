//! Carrying packages out (`fnv_ai`) on a plugin and navmeshes built in memory (no game
//! data): two interior cells joined by a door, markers, and actors with one package
//! each. The rules and their sources: docs/design/actor-behaviour.md §5.

use fnv_ai::{Activity, Ai, AiConfig, AiEvent, Doing};
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_game::ai::{AiPhase, SandboxPhase};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::package::{flags, type_flags, PackageCheck};
use fnv_game::GameData;
use fnv_nav::graph::DoorLock;
use fnv_nav::record::{tri_flags, DoorLink, NavMeshRecord, Triangle};
use fnv_nav::{NavGraph, Space};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

// ---- plugin builders ---------------------------------------------------------------

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

fn tes4() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x900u32.to_le_bytes()].concat();
    let payload = write_subrecords(&[sub(b"HEDR", hedr)]);
    [&b"TES4"[..], &(payload.len() as u32).to_le_bytes(), &1u32.to_le_bytes(), &[0; 4], &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn glob(fid: u32, edid: &str, v: f32) -> Vec<u8> {
    record(b"GLOB", fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", v.to_le_bytes().to_vec())])
}

/// A package without schedule or conditions: flags, type, location (type, value,
/// radius) and, for a patrol, whether it repeats.
fn pack(id: u32, edid: &str, general: u32, ty: u8, location: (i32, u32, i32), repeatable: Option<bool>) -> Vec<u8> {
    pack_with(id, edid, general, ty, 0, location, repeatable)
}

/// [`pack`] with the type-specific flags (a sandbox's "No ..." switches).
fn pack_with(id: u32, edid: &str, general: u32, ty: u8, specific: u16, location: (i32, u32, i32), repeatable: Option<bool>) -> Vec<u8> {
    let mut subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"PKDT", [&general.to_le_bytes()[..], &[ty, 0, 0, 0], &specific.to_le_bytes(), &[0, 0]].concat()),
        sub(b"PLDT", [location.0.to_le_bytes(), location.1.to_le_bytes(), location.2.to_le_bytes()].concat()),
        sub(b"PSDT", [&[0xFF, 0xFF, 0, 0xFF][..], &0u32.to_le_bytes()].concat()),
    ];
    subs.extend(repeatable.map(|r| sub(b"PKPT", vec![r as u8, 0])));
    record(b"PACK", id, &subs)
}

/// An Accompany package (type 7) whose target is a reference, kept within `distance`.
fn accompany(id: u32, edid: &str, target: u32, distance: i32) -> Vec<u8> {
    let subs = [
        sub(b"EDID", z(edid)),
        sub(b"PKDT", [&0u32.to_le_bytes()[..], &[7, 0, 0, 0], &[0; 4]].concat()),
        sub(b"PSDT", [&[0xFF, 0xFF, 0, 0xFF][..], &0u32.to_le_bytes()].concat()),
        sub(b"PTDT", [0i32.to_le_bytes(), target.to_le_bytes(), distance.to_le_bytes()].concat()),
    ];
    record(b"PACK", id, &subs)
}

/// A Use Weapon package (type 16): stand near `stand`, shoot at the reference `target`
/// two volleys of one shot, half a second apart, always hitting and doing no damage
/// (PKW3 [documented: xEdit PACK PKW3]); the weapon is the object type "Weapons: None".
fn shoot(id: u32, edid: &str, stand: u32, target: u32) -> Vec<u8> {
    let pkw3 = [&0x0000_0101u32.to_le_bytes()[..], &[1, 0], &2u16.to_le_bytes(), &1u16.to_le_bytes(), &1u16.to_le_bytes(), &0.5f32.to_le_bytes(), &0.5f32.to_le_bytes(), &[0; 4]].concat();
    let subs = [
        sub(b"EDID", z(edid)),
        sub(b"PKDT", [&0u32.to_le_bytes()[..], &[16, 0, 0, 0], &[0; 4]].concat()),
        sub(b"PLDT", [0i32.to_le_bytes(), stand.to_le_bytes(), 0i32.to_le_bytes()].concat()),
        sub(b"PSDT", [&[0xFF, 0xFF, 0, 0xFF][..], &0u32.to_le_bytes()].concat()),
        sub(b"PTDT", [2i32.to_le_bytes(), 23u32.to_le_bytes(), 0i32.to_le_bytes()].concat()),
        sub(b"PKW3", pkw3),
        sub(b"PTD2", [0i32.to_le_bytes(), target.to_le_bytes(), 0i32.to_le_bytes()].concat()),
    ];
    record(b"PACK", id, &subs)
}

fn npc(fid: u32, edid: &str, packages: &[u32]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    subs.extend(packages.iter().map(|p| sub(b"PKID", p.to_le_bytes().to_vec())));
    record(b"NPC_", fid, &subs)
}

fn placed(sig: &[u8; 4], fid: u32, edid: &str, base: u32, pos: [f32; 3], linked: Option<u32>) -> Vec<u8> {
    placed_with(sig, fid, edid, base, pos, 0.0, linked, None)
}

/// [`placed`] with a heading (z rotation, radians) and a patrol idle time (XPRD); a
/// patrol point with an idle time also has patrol data (XPPA) naming [`PATROL_TOPIC`].
#[allow(clippy::too_many_arguments)]
fn placed_with(sig: &[u8; 4], fid: u32, edid: &str, base: u32, pos: [f32; 3], heading: f32, linked: Option<u32>, idle: Option<f32>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec())];
    subs.extend(idle.map(|s| sub(b"XPRD", s.to_le_bytes().to_vec())));
    if idle.is_some() {
        subs.push(sub(b"XPPA", Vec::new()));
        subs.push(sub(b"TNAM", PATROL_TOPIC.to_le_bytes().to_vec()));
    }
    subs.extend(linked.map(|l| sub(b"XLKR", l.to_le_bytes().to_vec())));
    subs.push(sub(b"DATA", [pos[0].to_le_bytes(), pos[1].to_le_bytes(), pos[2].to_le_bytes(), [0; 4], [0; 4], heading.to_le_bytes()].concat()));
    record(sig, fid, &subs)
}

/// A topic no line belongs to: saying it shows as `EventKind::NoLine`.
const PATROL_TOPIC: u32 = 0xF0F;
const G_HOUR: u32 = 0x804;
const G_DAYS: u32 = 0x805;
const STAT: u32 = 0x810;
const CHAIR: u32 = 0x811;
const BED: u32 = 0x812;
const IDLE_MARKER: u32 = 0x813;
// Packages.
const P_TRAVEL_B: u32 = 0x820;
const P_DEFAULT: u32 = 0x821;
const P_IN_B: u32 = 0x822;
const P_RADIUS: u32 = 0x823;
const P_PATROL: u32 = 0x824;
const P_LOOP: u32 = 0x825;
const P_FOLLOW: u32 = 0x826;
const P_HERE: u32 = 0x827;
const P_RUN: u32 = 0x828;
const P_SB_ALL: u32 = 0x829;
const P_SB_WANDER: u32 = 0x82A;
const P_SB_OBJECTS: u32 = 0x82B;
const P_SB_NOTHING: u32 = 0x82C;
const P_TO_CHAIR: u32 = 0x82D;
const P_SLEEP_C: u32 = 0x82E;
const P_STEADY: u32 = 0x82F;
const P_ACCOMPANY: u32 = 0x830;
const P_WITH_PLAYER: u32 = 0x831;
const P_SHOOT: u32 = 0x832;
// Bases.
const N_TRAVEL: u32 = 0x840;
const N_IN: u32 = 0x841;
const N_RADIUS: u32 = 0x842;
const N_PATROL: u32 = 0x843;
const N_LOOP: u32 = 0x844;
const N_FOLLOW: u32 = 0x845;
const N_HERE: u32 = 0x846;
const N_RUN: u32 = 0x847;
const N_SB_ALL: u32 = 0x848;
const N_SB_WANDER: u32 = 0x849;
const N_SB_OBJECTS: u32 = 0x84A;
const N_SB_NOTHING: u32 = 0x84B;
const N_TO_CHAIR: u32 = 0x84C;
const N_SLEEP_C: u32 = 0x84D;
const N_STEADY: u32 = 0x84E;
const N_DOG: u32 = 0x84F;
const N_COMPANION: u32 = 0x850;
const N_LONER: u32 = 0x851;
const N_SHOOTER: u32 = 0x852;
// Cells and references.
const CELL_A: u32 = 0x860;
const CELL_B: u32 = 0x861;
const CELL_C: u32 = 0x862;
const MARK_C: u32 = 0x890;
const CHAIR_C: u32 = 0x891;
const BED_C: u32 = 0x892;
const IDLE_C: u32 = 0x893;
const R_SB_ALL: u32 = 0x894;
const R_SB_WANDER: u32 = 0x895;
const R_SB_OBJECTS: u32 = 0x896;
const R_SB_OBJECTS_2: u32 = 0x897;
const R_SB_NOTHING: u32 = 0x898;
const R_TO_CHAIR: u32 = 0x899;
const R_SLEEP_C: u32 = 0x89A;
const MARK_B: u32 = 0x870;
const MARK_A: u32 = 0x871;
const M1: u32 = 0x872;
const M2: u32 = 0x873;
const M3: u32 = 0x874;
const L1: u32 = 0x875;
const L2: u32 = 0x876;
const L3: u32 = 0x877;
const R_TRAVEL: u32 = 0x880;
const R_IN: u32 = 0x881;
const R_RADIUS: u32 = 0x882;
const R_PATROL: u32 = 0x883;
const R_LOOP: u32 = 0x884;
const R_FOLLOW: u32 = 0x885;
const R_HERE: u32 = 0x886;
const R_RUN: u32 = 0x887;
const R_STEADY: u32 = 0x888;
const R_DOG: u32 = 0x889;
const R_COMPANION: u32 = 0x89B;
const R_LONER: u32 = 0x89C;
const R_SHOOTER: u32 = 0x89D;
const DOOR_A: u32 = 0x8D1;
const DOOR_B: u32 = 0x8D2;

const TRAVEL: u8 = 6;
const SLEEP: u8 = 4;
const SANDBOX: u8 = 12;
const PATROL: u8 = 13;
const FOLLOW: u8 = 1;
const START: [f32; 3] = [50.0, 50.0, 0.0];
/// A sandbox's switches for everything it does with objects and for talking.
const NO_OBJECTS: u16 = type_flags::NO_EATING | type_flags::NO_SLEEPING | type_flags::NO_CONVERSATION | type_flags::NO_IDLE_MARKERS | type_flags::NO_FURNITURE;
const C: Space = Space::Interior(CELL_C);
const MIDDLE_C: [f32; 3] = [10500.0, 10500.0, 0.0];

fn plugin_bytes() -> Vec<u8> {
    let globs = [
        glob(0x801, "GameYear", 2281.0),
        glob(0x802, "GameMonth", 9.0),
        glob(0x803, "GameDay", 19.0),
        glob(0x804, "GameHour", 12.0),
        glob(G_DAYS, "GameDaysPassed", 5.0),
        glob(0x806, "TimeScale", 30.0),
    ]
    .concat();
    let packs = [
        pack(P_TRAVEL_B, "TravelToB", flags::MUST_REACH_LOCATION, TRAVEL, (0, MARK_B, 0), None),
        pack(P_DEFAULT, "StayHome", 0, TRAVEL, (3, 0, 64), None),
        pack(P_IN_B, "TravelIntoB", 0, TRAVEL, (1, CELL_B, 0), None),
        pack(P_RADIUS, "TravelNearA", 0, TRAVEL, (0, MARK_A, 150), None),
        pack(P_PATROL, "PatrolOnce", 0, PATROL, (0, M1, 0), Some(false)),
        pack(P_LOOP, "PatrolRound", 0, PATROL, (6, 0, 0), Some(true)),
        pack(P_FOLLOW, "FollowSomeone", 0, FOLLOW, (3, 0, 0), None),
        pack(P_HERE, "TravelHere", flags::MUST_COMPLETE, TRAVEL, (2, 0, 0), None),
        pack(P_RUN, "RunToA", flags::ALWAYS_RUN, TRAVEL, (0, MARK_A, 0), None),
        // Sandboxes around the middle of cell C, 450 units: everything allowed; only
        // wandering; only furniture and idle markers; nothing.
        pack(P_SB_ALL, "SandboxAll", 0, SANDBOX, (0, MARK_C, 450), None),
        pack_with(P_SB_WANDER, "SandboxWander", 0, SANDBOX, NO_OBJECTS, (0, MARK_C, 450), None),
        pack_with(P_SB_OBJECTS, "SandboxObjects", 0, SANDBOX, type_flags::NO_WANDERING | type_flags::NO_EATING | type_flags::NO_SLEEPING, (0, MARK_C, 450), None),
        pack_with(P_SB_NOTHING, "SandboxNothing", 0, SANDBOX, NO_OBJECTS | type_flags::NO_WANDERING, (0, MARK_C, 450), None),
        pack(P_TO_CHAIR, "TravelToChair", 0, TRAVEL, (0, CHAIR_C, 0), None),
        pack(P_SLEEP_C, "SleepInC", 0, SLEEP, (1, CELL_C, 0), None),
        pack(P_STEADY, "TravelToBInCombat", flags::MUST_REACH_LOCATION | flags::CONTINUE_DURING_COMBAT, TRAVEL, (0, MARK_B, 0), None),
        accompany(P_ACCOMPANY, "AccompanyTheTraveller", R_TRAVEL, 128),
        accompany(P_WITH_PLAYER, "AccompanyThePlayer", 0x14, 256),
        shoot(P_SHOOT, "ShootAtTheLoner", MARK_A, R_LONER),
    ]
    .concat();
    let npcs = [
        npc(0x7, "Player", &[]),
        npc(N_TRAVEL, "Traveller", &[P_TRAVEL_B, P_DEFAULT]),
        npc(N_IN, "Lodger", &[P_IN_B]),
        npc(N_RADIUS, "Visitor", &[P_RADIUS]),
        npc(N_PATROL, "Guard", &[P_PATROL]),
        npc(N_LOOP, "Sentry", &[P_LOOP]),
        npc(N_FOLLOW, "Follower", &[P_FOLLOW]),
        npc(N_HERE, "Stander", &[P_HERE]),
        npc(N_RUN, "Runner", &[P_RUN]),
        npc(N_SB_ALL, "Idler", &[P_SB_ALL]),
        npc(N_SB_WANDER, "Wanderer", &[P_SB_WANDER]),
        npc(N_SB_OBJECTS, "Sitter", &[P_SB_OBJECTS]),
        npc(N_SB_NOTHING, "Statue", &[P_SB_NOTHING]),
        npc(N_TO_CHAIR, "ChairUser", &[P_TO_CHAIR]),
        npc(N_SLEEP_C, "Sleeper", &[P_SLEEP_C]),
        // ACBS flag "No Low Level Processing" (bit 9).
        record(b"NPC_", N_STEADY, &[sub(b"EDID", z("Courier")), sub(b"ACBS", [0x200u32.to_le_bytes().to_vec(), vec![0; 20]].concat()), sub(b"PKID", P_STEADY.to_le_bytes().to_vec())]),
        npc(N_DOG, "Dog", &[P_ACCOMPANY]),
        npc(N_COMPANION, "Companion", &[P_WITH_PLAYER]),
        npc(N_LONER, "Loner", &[]),
        npc(N_SHOOTER, "Shooter", &[P_SHOOT]),
    ]
    .concat();
    // Furniture: a chair (marker flags bit 30) and a bed (bit 31); an idle marker with
    // two idles, ten seconds apart.
    let furn = [
        record(b"FURN", CHAIR, &[sub(b"EDID", z("Chair")), sub(b"MNAM", 0x4000_0001u32.to_le_bytes().to_vec())]),
        record(b"FURN", BED, &[sub(b"EDID", z("Bed")), sub(b"MNAM", 0x8000_0001u32.to_le_bytes().to_vec())]),
    ]
    .concat();
    let idlm = record(
        b"IDLM",
        IDLE_MARKER,
        &[sub(b"EDID", z("LeanMarker")), sub(b"IDLF", vec![0]), sub(b"IDLC", vec![2]), sub(b"IDLT", 10f32.to_le_bytes().to_vec()), sub(b"IDLA", [0x8F0u32.to_le_bytes(), 0x8F1u32.to_le_bytes()].concat())],
    );
    let stat = record(b"STAT", STAT, &[sub(b"EDID", z("Marker"))]);
    let a_refs = [
        placed(b"REFR", MARK_A, "MarkA", STAT, [350.0, 350.0, 0.0], None),
        placed(b"REFR", M1, "M1", STAT, [350.0, 50.0, 0.0], Some(M2)),
        placed_with(b"REFR", M2, "M2", STAT, [350.0, 350.0, 0.0], 0.0, Some(M3), Some(6.0)),
        placed(b"REFR", M3, "M3", STAT, [50.0, 350.0, 0.0], None),
        placed(b"REFR", L1, "L1", STAT, [250.0, 50.0, 0.0], Some(L2)),
        placed(b"REFR", L2, "L2", STAT, [250.0, 250.0, 0.0], Some(L3)),
        placed(b"REFR", L3, "L3", STAT, [50.0, 250.0, 0.0], Some(L1)),
        placed(b"ACHR", R_TRAVEL, "TravellerRef", N_TRAVEL, START, None),
        placed(b"ACHR", R_IN, "LodgerRef", N_IN, START, None),
        placed(b"ACHR", R_RADIUS, "VisitorRef", N_RADIUS, START, None),
        placed(b"ACHR", R_PATROL, "GuardRef", N_PATROL, START, None),
        placed(b"ACHR", R_LOOP, "SentryRef", N_LOOP, [60.0, 240.0, 0.0], Some(L1)),
        placed(b"ACHR", R_FOLLOW, "FollowerRef", N_FOLLOW, START, None),
        placed(b"ACHR", R_HERE, "StanderRef", N_HERE, START, None),
        placed(b"ACHR", R_RUN, "RunnerRef", N_RUN, START, None),
        placed(b"ACHR", R_STEADY, "CourierRef", N_STEADY, START, None),
        placed(b"ACHR", R_DOG, "DogRef", N_DOG, [150.0, 50.0, 0.0], None),
        placed(b"ACHR", R_COMPANION, "CompanionRef", N_COMPANION, [250.0, 50.0, 0.0], None),
        placed(b"ACHR", R_LONER, "LonerRef", N_LONER, [350.0, 50.0, 0.0], None),
        placed(b"ACHR", R_SHOOTER, "ShooterRef", N_SHOOTER, START, None),
    ]
    .concat();
    let b_refs = placed(b"REFR", MARK_B, "MarkB", STAT, [5250.0, 5250.0, 50.0], None);
    let cell = |id: u32, edid: &str, refs: &[u8]| {
        let rec = record(b"CELL", id, &[sub(b"EDID", z(edid))]);
        [rec, group(id.to_le_bytes(), 6, &group(id.to_le_bytes(), 9, refs))].concat()
    };
    // Cell C: 10 x 10 squares from (10000, 10000); the sandboxers stand near its middle.
    let c_refs = [
        placed(b"REFR", MARK_C, "MarkC", STAT, [10500.0, 10500.0, 0.0], None),
        placed(b"REFR", CHAIR_C, "ChairC", CHAIR, [10700.0, 10500.0, 0.0], None),
        placed(b"REFR", BED_C, "BedC", BED, [10300.0, 10700.0, 0.0], None),
        placed_with(b"REFR", IDLE_C, "IdleC", IDLE_MARKER, [10500.0, 10250.0, 0.0], 1.25, None, None),
        placed(b"ACHR", R_SB_ALL, "IdlerRef", N_SB_ALL, [10480.0, 10480.0, 0.0], None),
        placed(b"ACHR", R_SB_WANDER, "WandererRef", N_SB_WANDER, [10480.0, 10520.0, 0.0], None),
        placed(b"ACHR", R_SB_OBJECTS, "SitterRef", N_SB_OBJECTS, [10520.0, 10480.0, 0.0], None),
        placed(b"ACHR", R_SB_OBJECTS_2, "Sitter2Ref", N_SB_OBJECTS, [10520.0, 10520.0, 0.0], None),
        placed(b"ACHR", R_SB_NOTHING, "StatueRef", N_SB_NOTHING, [10450.0, 10450.0, 0.0], None),
        placed(b"ACHR", R_TO_CHAIR, "ChairUserRef", N_TO_CHAIR, [10050.0, 10050.0, 0.0], None),
        placed(b"ACHR", R_SLEEP_C, "SleeperRef", N_SLEEP_C, [10050.0, 10950.0, 0.0], None),
    ]
    .concat();
    let cells = [cell(CELL_A, "CellA", &a_refs), cell(CELL_B, "CellB", &b_refs), cell(CELL_C, "CellC", &c_refs)].concat();
    let top = |sig: &[u8; 4], body: &[u8]| group(*sig, 0, body);
    [
        tes4(),
        top(b"GLOB", &globs),
        top(b"STAT", &stat),
        top(b"FURN", &furn),
        top(b"IDLM", &idlm),
        top(b"NPC_", &npcs),
        top(b"PACK", &packs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells))),
    ]
    .concat()
}

// ---- navmeshes ---------------------------------------------------------------------

/// A flat navmesh over `n` × `n` squares of side 100 from `origin`, two triangles a
/// square, at height `z`.
fn grid(form_id: u32, origin: [f32; 2], n: usize, z: f32) -> NavMeshRecord {
    grid_of(form_id, origin, n, 100.0, z)
}

fn grid_of(form_id: u32, origin: [f32; 2], n: usize, size: f32, z: f32) -> NavMeshRecord {
    let vid = |i: usize, j: usize| (j * (n + 1) + i) as u16;
    let mut vertices = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            vertices.push([origin[0] + i as f32 * size, origin[1] + j as f32 * size, z]);
        }
    }
    let tri = |i: isize, j: isize, h: usize| -> i16 {
        if i < 0 || j < 0 || i >= n as isize || j >= n as isize {
            -1
        } else {
            ((j as usize * n + i as usize) * 2 + h) as i16
        }
    };
    let mut triangles = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let (a, b, c, d) = (vid(i, j), vid(i + 1, j), vid(i + 1, j + 1), vid(i, j + 1));
            let (ii, jj) = (i as isize, j as isize);
            triangles.push(Triangle { v: [a, b, c], edges: [tri(ii, jj - 1, 1), tri(ii + 1, jj, 1), tri(ii, jj, 1)], flags: 0, cover: 0 });
            triangles.push(Triangle { v: [a, c, d], edges: [tri(ii, jj, 0), tri(ii, jj + 1, 0), tri(ii - 1, jj, 0)], flags: 0, cover: 0 });
        }
    }
    NavMeshRecord { form_id, vertices, triangles, ..Default::default() }
}

const A: Space = Space::Interior(CELL_A);
const B: Space = Space::Interior(CELL_B);

/// Cell A: 4 × 4 squares at the origin; cell B: 3 × 3 at (5000, 5000), height 50. A door
/// in A's far corner leads to B's near corner and back.
fn nav() -> NavGraph {
    let mut a = grid(1, [0.0, 0.0], 4, 0.0);
    let mut b = grid(2, [5000.0, 5000.0], 3, 50.0);
    let corner = a.triangles.len() - 2;
    a.door_links.push(DoorLink { door: DOOR_A, triangle: corner as u16 });
    a.triangles[corner].flags |= tri_flags::DOOR;
    b.door_links.push(DoorLink { door: DOOR_B, triangle: 0 });
    let c = grid_of(3, [10000.0, 10000.0], 10, 100.0, 0.0);
    let input = [(&a, Some(CELL_A), Some(A)), (&b, Some(CELL_B), Some(B)), (&c, Some(CELL_C), Some(C))];
    let mut g = NavGraph::assemble(&input);
    g.join_doors(&input, |door| match door {
        DOOR_A => Some((DOOR_B, [5050.0, 5030.0, 50.0], 0.5, B)),
        DOOR_B => Some((DOOR_A, [340.0, 340.0, 0.0], 3.0, A)),
        _ => None,
    });
    g
}

// ---- fixture -----------------------------------------------------------------------

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Carry.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    Fixture { data, forms, refs }
}

struct Sim<'a> {
    game: Game<'a>,
    ai: Ai,
    nav: NavGraph,
}

fn sim<'a>(f: &'a Fixture, actors: &[u32]) -> Sim<'a> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let game = Game::new(&f.data, world, GameConfig { strict: true, frame_seconds: 0.5, package_interval: Some(10.0), ..GameConfig::default() });
    let mut ai = Ai::new(AiConfig { walk_speed: 100.0, run_speed: 400.0, ..AiConfig::default() });
    for a in actors {
        ai.add(&game, FormId(*a)).unwrap();
    }
    Sim { game, ai, nav: nav() }
}

impl Sim<'_> {
    fn run(&mut self, seconds: f64) {
        for _ in 0..(seconds * 2.0) as usize {
            self.game.tick();
            self.ai.update(&mut self.game, &self.nav, 0.5);
        }
    }

    fn events(&self, actor: u32) -> Vec<&AiEvent> {
        self.ai.events.iter().filter(|e| e.actor == FormId(actor)).map(|e| &e.event).collect()
    }

    fn feet(&self, actor: u32) -> [f32; 3] {
        self.ai.agents[&FormId(actor)].walker.feet
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

// ---- tests -------------------------------------------------------------------------

#[test]
fn a_travel_package_walks_through_the_door_and_meets_its_goal() {
    let f = fixture();
    assert!(f.data.issues.is_empty(), "{:?}", f.data.issues.to_json());
    let mut s = sim(&f, &[R_TRAVEL]);
    s.run(1.0);
    let actor = FormId(R_TRAVEL);
    assert_eq!(s.ai.agents[&actor].activity, Activity::Travelling);
    // The package holds the periodic choice until the actor is there.
    assert_eq!(s.game.package_held(actor), Some("must reach location"));
    s.run(30.0);
    let ev = s.events(R_TRAVEL);
    assert!(matches!(ev[0], AiEvent::Package { package: Some(p) } if p.0 == P_TRAVEL_B));
    assert!(matches!(ev[1], AiEvent::Trip { doors, running: false, space: B, .. } if doors == &[FormId(DOOR_A)]), "{:?}", ev[1]);
    assert!(matches!(ev[2], AiEvent::Door { door, to_door } if door.0 == DOOR_A && to_door.0 == DOOR_B));
    assert!(matches!(ev[3], AiEvent::Arrived { travelled, .. } if *travelled > 600.0 && *travelled < 900.0), "{:?}", ev[3]);
    assert_eq!(ev.len(), 4);
    assert_eq!(s.ai.agents[&actor].activity, Activity::There);
    assert!(dist(s.feet(R_TRAVEL), [5250.0, 5250.0, 50.0]) < 5.0);
    // The game knows where the actor is and that the package met its goal.
    let st = s.game.world.peek(actor).unwrap();
    assert_eq!(st.cell, Some(FormId(CELL_B)));
    assert!(dist(st.pos, [5250.0, 5250.0, 50.0]) < 5.0);
    assert!(s.game.extra[&actor].package_goal_met);
    assert_eq!(s.game.package_held(actor), None);
}

#[test]
fn a_restrained_actor_stays_in_place_until_released() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    let start = s.feet(R_TRAVEL);
    s.game.extra.entry(FormId(R_TRAVEL)).or_default().restrained = true;
    s.run(30.0);
    assert!(dist(s.feet(R_TRAVEL), start) < 1.0, "a restrained actor walked");
    s.game.extra.get_mut(&FormId(R_TRAVEL)).unwrap().restrained = false;
    s.run(30.0);
    assert!(dist(s.feet(R_TRAVEL), [5250.0, 5250.0, 50.0]) < 5.0);
}

/// A caller whose character movement carries an actor straight on along its heading
/// (as the engine's character controller does on open ground).
struct StraightOn;

impl fnv_ai::Mover for StraightOn {
    fn coast(&mut self, _actor: FormId, walker: &mut fnv_nav::Walker, dt: f32) -> bool {
        let d = fnv_nav::follow::dir_of(walker.heading);
        let s = walker.moving_speed() * dt;
        walker.feet = [walker.feet[0] + d[0] * s, walker.feet[1] + d[1] * s, walker.feet[2]];
        true
    }
}

#[test]
fn set_actors_ai_0_keeps_a_walking_actor_going_straight_on_until_switched_back_on() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    let actor = FormId(R_TRAVEL);
    s.run(3.0);
    assert!(s.ai.agents[&actor].walker.moving, "walking before the AI goes off");
    s.game.extra.entry(actor).or_default().ai_off = true;
    let (start, heading) = (s.feet(R_TRAVEL), s.ai.agents[&actor].walker.heading);
    for _ in 0..4 {
        s.game.tick();
        s.ai.update_moved(&mut s.game, &s.nav, 0.5, &mut StraightOn);
    }
    // Two seconds at 100 units/s, on its heading, the heading kept, the game told.
    let a = &s.ai.agents[&actor];
    assert!((dist(a.walker.feet, start) - 200.0).abs() < 1.0, "{:?} from {start:?}", a.walker.feet);
    assert_eq!(a.walker.heading, heading);
    assert_eq!(a.activity, Activity::Travelling);
    assert!(dist(s.game.world.peek(actor).unwrap().pos, a.walker.feet) < 1.0);
    // Without character movement (headless) it stands, and stays standing.
    let here = s.feet(R_TRAVEL);
    s.run(2.0);
    assert!(dist(s.feet(R_TRAVEL), here) < 1e-3);
    assert!(!s.ai.agents[&actor].walker.moving);
    s.run(2.0);
    assert!(dist(s.feet(R_TRAVEL), here) < 1e-3);
    // Back on: it walks on to its package's place.
    s.game.extra.get_mut(&actor).unwrap().ai_off = false;
    s.run(40.0);
    assert!(dist(s.feet(R_TRAVEL), [5250.0, 5250.0, 50.0]) < 5.0);
}

#[test]
fn time_passed_in_a_menu_takes_the_actor_to_its_packages_place() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    let actor = FormId(R_TRAVEL);
    // A menu holds the game (the Wait menu): no tick moves anyone, game time stands.
    s.game.open_menu(1012);
    let before = s.game.time;
    s.game.choose_packages_now();
    // Too little time: on its way, not there.
    assert_eq!(s.ai.pass_time(&mut s.game, &s.nav, 2.0, 0.5), 1);
    assert_eq!(s.ai.agents[&actor].activity, Activity::Travelling);
    assert!(dist(s.feet(R_TRAVEL), [5250.0, 5250.0, 50.0]) > 100.0);
    // An hour's worth: through the door and there, and it stops as soon as it is.
    assert_eq!(s.ai.pass_time(&mut s.game, &s.nav, 120.0, 0.5), 1);
    assert_eq!(s.ai.agents[&actor].activity, Activity::There);
    assert!(dist(s.feet(R_TRAVEL), [5250.0, 5250.0, 50.0]) < 5.0);
    let ev = s.events(R_TRAVEL);
    assert!(matches!(ev[0], AiEvent::Package { package: Some(p) } if p.0 == P_TRAVEL_B));
    assert!(matches!(ev[3], AiEvent::Arrived { .. }), "{:?}", ev);
    // The game knows: its place, and the package's goal (its end script would have run).
    assert_eq!(s.game.world.peek(actor).unwrap().cell, Some(FormId(CELL_B)));
    assert!(s.game.extra[&actor].package_goal_met);
    assert_eq!(s.game.time, before);
    // Nobody left to walk: nothing more happens.
    assert_eq!(s.ai.pass_time(&mut s.game, &s.nav, 120.0, 0.5), 0);
    assert_eq!(s.events(R_TRAVEL).len(), 4);
}

#[test]
fn a_place_that_cannot_be_reached_sets_the_package_aside() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    s.nav.set_door_lock(DOOR_A, Some(DoorLock { level: 50, key: Some(0x4B) }));
    s.run(1.0);
    let actor = FormId(R_TRAVEL);
    let ev = s.events(R_TRAVEL);
    assert!(matches!(ev[0], AiEvent::Package { package: Some(p) } if p.0 == P_TRAVEL_B));
    assert!(matches!(ev[1], AiEvent::Unreachable { package, .. } if package.0 == P_TRAVEL_B), "{:?}", ev[1]);
    // The actor took the next package of its list, which keeps it at home.
    assert!(matches!(ev[2], AiEvent::Package { package: Some(p) } if p.0 == P_DEFAULT), "{:?}", ev[2]);
    assert!(matches!(ev[3], AiEvent::AlreadyThere { .. }));
    assert_eq!(s.game.check_package(actor, FormId(P_TRAVEL_B)), PackageCheck::Unreachable);
    assert_eq!(s.feet(R_TRAVEL), START);
    // Within the hour it does not try again.
    s.run(30.0);
    assert_eq!(s.events(R_TRAVEL).len(), 4);
    // After it, it tries again; with the key it gets through.
    s.game.world.set_global(FormId(G_DAYS), 5.0 + 1.1 / 24.0).unwrap();
    s.ai.cfg.params.keys.push(0x4B);
    s.run(40.0);
    let ev = s.events(R_TRAVEL);
    assert!(matches!(ev[4], AiEvent::Package { package: Some(p) } if p.0 == P_TRAVEL_B), "{:?}", ev[4]);
    assert!(matches!(ev[5], AiEvent::Trip { .. }));
    assert!(matches!(ev.last().unwrap(), AiEvent::Arrived { .. }));
    assert_eq!(s.ai.agents[&actor].walker.space, B);
}

#[test]
fn an_in_cell_location_is_met_on_entering_the_cell() {
    let f = fixture();
    let mut s = sim(&f, &[R_IN]);
    s.run(30.0);
    let agent = &s.ai.agents[&FormId(R_IN)];
    assert_eq!(agent.activity, Activity::There);
    assert_eq!(agent.walker.space, B);
    // It stopped where the door let it in, not in the middle of the cell.
    assert!(dist(s.feet(R_IN), [5050.0, 5030.0, 50.0]) < 60.0, "{:?}", s.feet(R_IN));
    assert!(matches!(s.events(R_IN).last().unwrap(), AiEvent::Arrived { .. }));
}

#[test]
fn a_radius_is_reached_at_its_edge() {
    let f = fixture();
    let mut s = sim(&f, &[R_RADIUS]);
    s.run(30.0);
    assert_eq!(s.ai.agents[&FormId(R_RADIUS)].activity, Activity::There);
    let d = dist(s.feet(R_RADIUS), [350.0, 350.0, 0.0]);
    assert!(d <= 150.0 && d > 90.0, "stopped {d} from the marker");
}

#[test]
fn always_run_packages_run() {
    let f = fixture();
    let mut s = sim(&f, &[R_RUN, R_RADIUS]);
    s.run(1.0);
    assert!(matches!(s.events(R_RUN)[1], AiEvent::Trip { running: true, .. }));
    // One second at 400 units a second against 100.
    assert!(dist(s.feet(R_RUN), START) > 3.0 * dist(s.feet(R_RADIUS), START));
}

#[test]
fn a_patrol_walks_its_linked_references_in_order_once() {
    let f = fixture();
    let mut s = sim(&f, &[R_PATROL]);
    s.run(60.0);
    let points: Vec<u32> = s.events(R_PATROL).iter().filter_map(|e| match e {
        AiEvent::Arrived { patrol_point: Some(p), .. } => Some(p.0),
        _ => None,
    }).collect();
    assert_eq!(points, vec![M1, M2, M3]);
    let actor = FormId(R_PATROL);
    assert_eq!(s.ai.agents[&actor].activity, Activity::There);
    assert!(dist(s.feet(R_PATROL), [50.0, 350.0, 0.0]) < 5.0);
    assert!(s.game.extra[&actor].package_goal_met);
}

#[test]
fn a_repeatable_patrol_starts_at_the_nearest_point_and_goes_round() {
    let f = fixture();
    let mut s = sim(&f, &[R_LOOP]);
    s.run(40.0);
    let points: Vec<u32> = s.events(R_LOOP).iter().filter_map(|e| match e {
        AiEvent::Arrived { patrol_point: Some(p), .. } => Some(p.0),
        _ => None,
    }).collect();
    // The sentry stands next to L3: there first, then round the loop, again and again.
    assert!(points.len() >= 6, "{points:x?}");
    assert_eq!(&points[..6], &[L3, L1, L2, L3, L1, L2]);
    let actor = FormId(R_LOOP);
    assert_eq!(s.ai.agents[&actor].activity, Activity::Travelling);
    assert!(!s.game.extra[&actor].package_goal_met);
}

#[test]
fn packages_that_are_not_carried_out_leave_the_actor_where_it_is() {
    let f = fixture();
    let mut s = sim(&f, &[R_FOLLOW, R_HERE]);
    s.run(15.0);
    assert_eq!(s.ai.agents[&FormId(R_FOLLOW)].activity, Activity::Staying);
    assert!(matches!(s.events(R_FOLLOW)[1], AiEvent::NotCarriedOut { .. }));
    assert_eq!(s.events(R_FOLLOW).len(), 2);
    assert!(!s.game.extra[&FormId(R_FOLLOW)].package_goal_met);
    assert_eq!(s.feet(R_FOLLOW), START);
    // "Near current location": there at once, and the goal is met.
    assert_eq!(s.ai.agents[&FormId(R_HERE)].activity, Activity::There);
    assert!(matches!(s.events(R_HERE)[1], AiEvent::AlreadyThere { .. }));
    assert!(s.game.extra[&FormId(R_HERE)].package_goal_met);
}

/// An Accompany package: the actor keeps near its target wherever it goes, through the
/// door into the other cell too, and stands while the target stands (`goal::Follow`;
/// the owner's playtest of 2026-10-11: Sunny Smiles's dog stayed behind).
#[test]
fn an_accompanying_actor_follows_its_target_through_a_door() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL, R_DOG]);
    s.run(1.0);
    // Within its distance of the target at first: it stands.
    assert_eq!(s.ai.agents[&FormId(R_DOG)].activity, Activity::There);
    assert!(matches!(s.events(R_DOG)[..], [AiEvent::Package { .. }]), "{:?}", s.events(R_DOG));
    // The traveller walks off to cell B; the dog sets off after it and goes through.
    s.run(40.0);
    let ev = s.events(R_DOG);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Follow { target, .. } if target.0 == R_TRAVEL)), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Door { door, .. } if door.0 == DOOR_A)), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, AiEvent::CaughtUp { .. })), "{ev:?}");
    assert_eq!(s.ai.agents[&FormId(R_DOG)].walker.space, B);
    assert!(dist(s.feet(R_DOG), s.feet(R_TRAVEL)) <= 128.0 + 64.0 + 1.0, "{:?} {:?}", s.feet(R_DOG), s.feet(R_TRAVEL));
    assert_eq!(s.ai.agents[&FormId(R_DOG)].activity, Activity::There);
    assert_eq!(s.game.world.peek(FormId(R_DOG)).unwrap().cell, Some(FormId(CELL_B)));
    // The traveller stands at its marker: so does the dog, and it does not start over.
    let follows = ev.iter().filter(|e| matches!(e, AiEvent::Follow { .. })).count();
    s.run(20.0);
    assert_eq!(s.events(R_DOG).iter().filter(|e| matches!(e, AiEvent::Follow { .. })).count(), follows);
}

/// The player's teammate is brought along when the player changes cells (SetPlayerTeammate,
/// "Changing cells will teleport the teammate to the player"; docs/design/followers.md), and
/// so is an actor following the player on a Follow package (owner decision 2026-10-23).
#[test]
fn a_teammate_following_the_player_is_brought_along_to_another_cell() {
    let f = fixture();
    let mut s = sim(&f, &[R_COMPANION]);
    let (mate, player) = (FormId(R_COMPANION), FormId::PLAYER_REF);
    {
        let p = s.game.world.get(player).unwrap();
        (p.cell, p.pos, p.rot) = (Some(FormId(CELL_A)), [100.0, 50.0, 0.0], [0.0; 3]);
    }
    s.run(1.0);
    assert!(s.game.follows_player(mate));
    let move_to = |s: &mut Sim, marker: u32| {
        let opcode = s.game.data.table.by_name("MoveTo").unwrap();
        let ctx = fnv_game::functions::Ctx { unit: "test".into(), opcode, on: Some(player), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
        s.game.call_function(&ctx, &[fnv_script::vm::ArgValue::Value(fnv_script::vm::Value::Ref(marker))]);
    };
    // Not a teammate, but following the player on its Follow package from the player's
    // cell: it comes along at once too (owner decision 2026-10-23, Deputy Beagle).
    move_to(&mut s, MARK_B);
    assert_eq!(s.game.world.peek(mate).and_then(|m| m.cell), Some(FormId(CELL_B)));
    assert!(s.game.brought_along.contains(&mate));
    assert!(!s.game.is_teammate(mate));
    s.game.brought_along.clear();
    s.ai.moved_by_scripts(&mut s.game);
    s.run(1.0);
    assert!(s.game.follows_player(mate));
    // A teammate: put beside the player in cell B, and the AI takes it up from there.
    s.game.extra.entry(mate).or_default().teammate = true;
    move_to(&mut s, MARK_B);
    let st = s.game.world.peek(mate).unwrap();
    assert_eq!(st.cell, Some(FormId(CELL_B)));
    assert!((dist(st.pos, [5250.0, 5250.0, 50.0]) - fnv_game::followers::BESIDE).abs() < 1.0, "{:?}", st.pos);
    // On the player's left (the original's Boone through a door, od_03).
    let (pp, h) = s.game.world.peek(player).map(|p| (p.pos, p.rot[2])).unwrap();
    let left = [pp[0] - h.cos() * fnv_game::followers::BESIDE, pp[1] + h.sin() * fnv_game::followers::BESIDE];
    assert!((st.pos[0] - left[0]).hypot(st.pos[1] - left[1]) < 1.0, "{:?} not left of {:?} at {h}", st.pos, pp);
    assert!(s.game.brought_along.contains(&mate));
    s.ai.moved_by_scripts(&mut s.game);
    assert_eq!(s.ai.agents[&mate].walker.space, B);
    s.run(2.0);
    assert!(s.game.follows_player(mate));
    // On a package that does not follow (the original's Boone on his own PATROL), a
    // teammate still comes along (od_03, the `followers` campaign).
    s.game.extra.entry(mate).or_default().script_package = Some(FormId(P_DEFAULT));
    s.game.evaluate_package(mate);
    assert!(!s.game.follows_player(mate) && !s.game.told_to_wait(mate));
    move_to(&mut s, MARK_A);
    assert_eq!(s.game.world.peek(mate).and_then(|m| m.cell), s.game.world.peek(player).and_then(|p| p.cell));
    // Dead, it is no teammate any more and stays where it fell.
    s.game.kill(mate, None);
    assert!(!s.game.is_teammate(mate) && !s.game.extra[&mate].teammate);
}

/// The companion wheel's Back Up (docs/spec-notes/companion-wheel.md §5): the companion
/// walks about `BACK_UP` units straight away from the player and stands; its Follow package
/// does not call it back while it is within the follow distance.
#[test]
fn a_companion_told_to_back_up_steps_away_from_the_player() {
    let f = fixture();
    let mut s = sim(&f, &[R_COMPANION]);
    let (mate, player) = (FormId(R_COMPANION), FormId::PLAYER_REF);
    {
        let p = s.game.world.get(player).unwrap();
        (p.cell, p.pos, p.rot) = (Some(FormId(CELL_A)), [100.0, 50.0, 0.0], [0.0; 3]);
    }
    s.game.extra.entry(mate).or_default().teammate = true;
    s.run(20.0);
    assert!(s.game.follows_player(mate));
    let before = s.ai.agents[&mate].walker.feet;
    let ppos = [100.0, 50.0, 0.0];
    let away = |p: [f32; 3]| (p[0] - ppos[0]).hypot(p[1] - ppos[1]);
    s.game.companions.back_ups.insert(mate, s.game.time);
    s.run(6.0);
    let after = s.ai.agents[&mate].walker.feet;
    assert!(s.ai.events.iter().any(|e| e.actor == mate && matches!(e.event, AiEvent::BackUp { .. })), "backed up");
    assert!(away(after) > away(before) + 100.0, "{before:?} -> {after:?}");
    assert!(s.game.companions.back_ups.is_empty(), "the order was taken");
}

// ---- patrol waits, package furniture, sandboxes -------------------------------------------

#[test]
fn a_patrol_waits_out_a_points_idle_time() {
    let f = fixture();
    let mut s = sim(&f, &[R_PATROL]);
    s.run(60.0);
    let log: Vec<(f64, &AiEvent)> = s.ai.events.iter().filter(|e| e.actor == FormId(R_PATROL)).map(|e| (e.time, &e.event)).collect();
    let wait = log.iter().position(|(_, e)| matches!(e, AiEvent::PatrolWait { patrol_point, seconds } if patrol_point.0 == M2 && *seconds == 6.0)).expect("waits at M2");
    // The walk to M3 is planned six seconds after the wait began.
    let (t0, _) = log[wait];
    let (t1, next) = log[wait + 1];
    assert!(matches!(next, AiEvent::Trip { patrol_point: Some(p), .. } if p.0 == M3), "{next:?}");
    assert!((6.0..7.0).contains(&(t1 - t0)), "waited {}", t1 - t0);
    // M1 and M3 have no idle time: no wait there.
    assert_eq!(log.iter().filter(|(_, e)| matches!(e, AiEvent::PatrolWait { .. })).count(), 1);
    // On arriving at M2 the patroller says M2's topic (docs/spec-notes/packages.md §7.4).
    assert_eq!(s.game.world.placed.get(FormId(M2)).and_then(|p| p.patrol_topic), Some(FormId(PATROL_TOPIC)));
    let said = |k: &fnv_game::trace::EventKind| matches!(k, fnv_game::trace::EventKind::NoLine { speaker, topic } if speaker.0 == R_PATROL && topic.0 == PATROL_TOPIC);
    assert!(s.game.events.iter().any(|e| said(&e.kind)), "says M2's topic");
}

#[test]
fn a_package_whose_place_is_furniture_gives_it_to_the_actor() {
    let f = fixture();
    let mut s = sim(&f, &[R_TO_CHAIR, R_SLEEP_C]);
    s.run(30.0);
    // Travel to a chair: the actor uses it once there.
    let sitter = &s.ai.agents[&FormId(R_TO_CHAIR)];
    assert_eq!(sitter.activity, Activity::There);
    assert_eq!(sitter.using, Some(FormId(CHAIR_C)));
    assert!(matches!(s.events(R_TO_CHAIR).last().unwrap(), AiEvent::Furniture { furniture: Some(c) } if c.0 == CHAIR_C));
    // A Sleep package "in cell": the cell's bed, however far.
    assert_eq!(s.ai.agents[&FormId(R_SLEEP_C)].using, Some(FormId(BED_C)));
    // Nobody sandboxes under these packages.
    assert!(sitter.sandbox.is_none());
}

/// A script's Look turning the body keeps an actor from sitting down until StopLook; the
/// seat stays kept for it meanwhile [observed: RingoRef, campaign `knockdown_look`
/// 2026-10-19, docs/design/script-stubs.md §3].
#[test]
fn a_look_turning_the_body_keeps_an_actor_off_its_seat_until_stop_look() {
    use fnv_game::functions::senses::Look;
    let f = fixture();
    let mut s = sim(&f, &[R_TO_CHAIR]);
    s.game.senses.looks.insert(FormId(R_TO_CHAIR), Look { target: FormId(R_SLEEP_C), rotate_body: true });
    s.run(30.0);
    let sitter = &s.ai.agents[&FormId(R_TO_CHAIR)];
    assert_eq!(sitter.activity, Activity::There);
    assert_eq!(sitter.using, None, "held standing by the chair");
    assert!(!s.events(R_TO_CHAIR).iter().any(|e| matches!(e, AiEvent::Furniture { furniture: Some(_) })));
    // StopLook: it sits at the next social update.
    s.game.senses.looks.remove(&FormId(R_TO_CHAIR));
    s.game.tick();
    s.ai.social(&mut s.game, &s.nav, 0.5);
    assert_eq!(s.ai.agents[&FormId(R_TO_CHAIR)].using, Some(FormId(CHAIR_C)));
}

/// What a sandboxing actor did, in order.
fn doings(s: &Sim, actor: u32) -> Vec<Doing> {
    s.events(actor).iter().filter_map(|e| match e {
        AiEvent::Sandbox { doing, .. } => Some(*doing),
        _ => None,
    }).collect()
}

#[test]
fn a_sandbox_that_only_wanders_walks_about_inside_its_radius() {
    let f = fixture();
    let mut s = sim(&f, &[R_SB_WANDER]);
    let mut farthest = 0.0f32;
    for _ in 0..240 {
        s.run(1.0);
        farthest = farthest.max(dist(s.feet(R_SB_WANDER), MIDDLE_C));
    }
    let did = doings(&s, R_SB_WANDER);
    assert!(did.len() >= 6, "{did:?}");
    assert!(did.iter().all(|d| *d == Doing::Wandering), "{did:?}");
    let agent = &s.ai.agents[&FormId(R_SB_WANDER)];
    assert!(agent.walker.travelled > 500.0, "walked {}", agent.walker.travelled);
    assert!(farthest <= 450.0 + 1.0, "went {farthest} from the middle");
    assert_eq!(agent.using, None);
    let allowed = agent.sandbox.as_ref().unwrap().allowed;
    assert!(allowed.wandering && !allowed.furniture && !allowed.idle_markers && !allowed.eating && !allowed.sleeping && !allowed.conversation);
}

#[test]
fn a_sandbox_without_wandering_takes_turns_at_the_chair_and_the_idle_marker() {
    let f = fixture();
    let mut s = sim(&f, &[R_SB_OBJECTS]);
    s.run(600.0);
    let did = doings(&s, R_SB_OBJECTS);
    assert!(did.len() >= 4, "{did:?}");
    // Only the two kinds the package leaves on, never the same kind twice running.
    assert!(did.iter().all(|d| matches!(d, Doing::Sitting(c) if c.0 == CHAIR_C) || matches!(d, Doing::AtIdleMarker(m) if m.0 == IDLE_C)), "{did:?}");
    assert!(did.windows(2).all(|w| std::mem::discriminant(&w[0]) != std::mem::discriminant(&w[1])), "{did:?}");
    // The chair is taken while sat on and given back afterwards.
    let furniture: Vec<Option<u32>> = s.events(R_SB_OBJECTS).iter().filter_map(|e| match e {
        AiEvent::Furniture { furniture } => Some(furniture.map(|f| f.0)),
        _ => None,
    }).collect();
    assert!(furniture.len() >= 2 && furniture[0] == Some(CHAIR_C) && furniture[1].is_none(), "{furniture:?}");
    assert!(s.ai.agents[&FormId(R_SB_OBJECTS)].walker.travelled > 100.0);
}

#[test]
fn an_idle_marker_turns_the_actor_its_way() {
    let f = fixture();
    let mut s = sim(&f, &[R_SB_OBJECTS]);
    // Run until it stands at the marker.
    let mut at_marker = false;
    for _ in 0..600 {
        s.run(1.0);
        let a = &s.ai.agents[&FormId(R_SB_OBJECTS)];
        if a.activity == Activity::There && matches!(a.sandbox.as_ref().map(|x| x.doing), Some(Doing::AtIdleMarker(_))) {
            assert!(dist(a.walker.feet, [10500.0, 10250.0, 0.0]) < 40.0, "{:?}", a.walker.feet);
            assert_eq!(a.using, None);
            // It turns to the marker's heading on the spot (not at once), within the
            // time a half turn takes at the standing rate.
            assert!(a.face_to == Some(1.25) || (a.walker.heading - 1.25).abs() < 1e-4, "{:?} {}", a.face_to, a.walker.heading);
            s.run(2.0);
            let a = &s.ai.agents[&FormId(R_SB_OBJECTS)];
            assert!((a.walker.heading - 1.25).abs() < 1e-4 && a.face_to.is_none(), "heading {} {:?}", a.walker.heading, a.face_to);
            at_marker = true;
            break;
        }
    }
    assert!(at_marker);
}

#[test]
fn two_sandboxers_do_not_share_a_chair() {
    let f = fixture();
    let mut s = sim(&f, &[R_SB_OBJECTS, R_SB_OBJECTS_2]);
    let mut both_busy = 0;
    for _ in 0..600 {
        s.run(1.0);
        let (a, b) = (&s.ai.agents[&FormId(R_SB_OBJECTS)], &s.ai.agents[&FormId(R_SB_OBJECTS_2)]);
        assert!(!(a.using.is_some() && a.using == b.using), "both on {:?}", a.using);
        both_busy += (a.using.is_some() || b.using.is_some()) as u32;
    }
    assert!(both_busy > 60, "the chair was used for {both_busy} s");
    assert!(!doings(&s, R_SB_OBJECTS).is_empty() && !doings(&s, R_SB_OBJECTS_2).is_empty());
}

#[test]
fn a_sandbox_with_everything_off_stands_still() {
    let f = fixture();
    let mut s = sim(&f, &[R_SB_NOTHING]);
    s.run(120.0);
    let did = doings(&s, R_SB_NOTHING);
    assert!(!did.is_empty() && did.iter().all(|d| *d == Doing::Standing), "{did:?}");
    assert_eq!(s.ai.agents[&FormId(R_SB_NOTHING)].walker.travelled, 0.0);
}

#[test]
fn a_sandboxer_walking_up_to_a_conversation_partner_walks_the_whole_way() {
    // Until 2026-10-17 the walk ended in its first frame: the starter stood inside its
    // sandbox's radius, which counted as arriving, and it talked from where it stood.
    let f = fixture();
    let mut s = sim(&f, &[R_SB_NOTHING, R_SB_WANDER]);
    s.run(5.0);
    let (a, to) = (FormId(R_SB_NOTHING), [10750.0, 10450.0, 0.0]);
    let agent = s.ai.agents.get_mut(&a).unwrap();
    agent.walker.go_to(&s.nav, fnv_nav::path::Place { space: C, pos: to }, &fnv_nav::path::PathParams::default()).unwrap();
    agent.activity = Activity::Travelling;
    agent.chat = Some(fnv_ai::Chat::Going { partner: FormId(R_SB_WANDER), since: 0.0 });
    s.run(1.0);
    assert_eq!(s.ai.agents[&a].activity, Activity::Travelling, "the walk goes on inside the sandbox's radius");
    s.run(6.0);
    assert_eq!(s.ai.agents[&a].activity, Activity::There);
    assert!(dist(s.feet(R_SB_NOTHING), to) < 20.0, "{:?}", s.feet(R_SB_NOTHING));
}

#[test]
fn a_sandboxer_sleeps_at_night_and_eats_at_mealtimes() {
    let f = fixture();
    let mut s = sim(&f, &[R_SB_ALL]);
    // Two in the morning is inside every sleep window (start 19:00 to 01:00, six hours
    // or more): to the bed.
    s.game.world.set_global(FormId(G_HOUR), 2.0).unwrap();
    s.run(30.0);
    let actor = FormId(R_SB_ALL);
    let a = &s.ai.agents[&actor];
    assert_eq!(a.sandbox.as_ref().unwrap().doing, Doing::Sleeping(FormId(BED_C)));
    assert_eq!(a.using, Some(FormId(BED_C)));
    let ((sleep_start, sleep_hours), meals) = a.sandbox.as_ref().unwrap().times();
    assert!((19.0..24.0).contains(&sleep_start) || (0.0..1.0).contains(&sleep_start), "sleep starts {sleep_start}");
    assert!((6.0..=10.0).contains(&sleep_hours));
    assert!((6.0..=9.0).contains(&meals[0].0) && (11.0..=14.0).contains(&meals[1].0) && (17.0..=20.0).contains(&meals[2].0), "{meals:?}");
    assert!(meals.iter().all(|m| (0.5..=2.0).contains(&m.1)));
    // It stays in bed while the night lasts, whatever time passes on the wall.
    s.run(300.0);
    assert_eq!(s.ai.agents[&actor].using, Some(FormId(BED_C)));
    // The middle of the afternoon, between lunch and dinner (15:30 to 16:59 is in
    // neither window): up, and doing the things of the day.
    s.game.world.set_global(FormId(G_HOUR), 16.5).unwrap();
    s.game.world.set_global(FormId(G_DAYS), 5.7).unwrap();
    s.run(20.0);
    let a = &s.ai.agents[&actor];
    assert_ne!(a.using, Some(FormId(BED_C)));
    assert!(!matches!(a.sandbox.as_ref().unwrap().doing, Doing::Sleeping(_) | Doing::Eating(_)), "{:?}", a.sandbox.as_ref().unwrap().doing);
    // Lunch, at the hour rolled for it: a meal on the chair.
    let lunch = meals[1].0 + 0.1;
    s.game.world.set_global(FormId(G_HOUR), lunch).unwrap();
    // Whatever it is doing ends within three minutes.
    s.run(240.0);
    assert_eq!(s.ai.agents[&actor].sandbox.as_ref().unwrap().doing, Doing::Eating(Some(FormId(CHAIR_C))));
    assert_eq!(s.ai.agents[&actor].using, Some(FormId(CHAIR_C)));
}

#[test]
fn a_sandbox_is_the_same_every_time() {
    let f = fixture();
    let run = || {
        let mut s = sim(&f, &[R_SB_ALL]);
        s.run(300.0);
        (doings(&s, R_SB_ALL), s.feet(R_SB_ALL))
    };
    let (a, b) = (run(), run());
    assert!(a.0.len() >= 3, "{:?}", a.0);
    assert_eq!(a, b);
}

#[test]
fn a_standing_actor_is_pushed_over_the_navmesh_and_goes_back() {
    let f = fixture();
    let mut s = sim(&f, &[R_RADIUS, R_TO_CHAIR]);
    let actor = FormId(R_RADIUS);
    // On its way: not pushed.
    s.run(1.0);
    assert!(!s.ai.stands(&s.game, actor));
    assert_eq!(s.ai.push(&mut s.game, &s.nav, actor, [5.0, 0.0]), 0.0);
    s.run(30.0);
    // There (within 150 of the marker at (350, 350)) and standing: a push moves it, and
    // the game's reference with it.
    let at = s.feet(R_RADIUS);
    assert!(s.ai.stands(&s.game, actor));
    let moved = s.ai.push(&mut s.game, &s.nav, actor, [6.0, 8.0]);
    assert!((moved - 10.0).abs() < 1e-3, "{moved}");
    let pushed = s.feet(R_RADIUS);
    assert_eq!(pushed, [at[0] + 6.0, at[1] + 8.0, at[2]]);
    assert_eq!(s.game.world.get(actor).unwrap().pos, pushed);
    // Still where its package has it: it stays there.
    s.run(10.0);
    assert_eq!(s.feet(R_RADIUS), pushed);
    assert!(!s.events(R_RADIUS).iter().any(|e| matches!(e, AiEvent::PushedBack { .. })));
    // Pushed at the navmesh's edge (y 0): as far as the edge, no further, and along it
    // when the push points off it at an angle.
    let x = s.feet(R_RADIUS)[0];
    let moved = s.ai.push(&mut s.game, &s.nav, actor, [0.0, -600.0]);
    assert!((moved - (at[1] + 8.0)).abs() < 1e-2 && s.feet(R_RADIUS)[1].abs() < 1e-3, "{moved} {:?}", s.feet(R_RADIUS));
    assert_eq!(s.ai.push(&mut s.game, &s.nav, actor, [0.0, -10.0]), 0.0);
    let moved = s.ai.push(&mut s.game, &s.nav, actor, [10.0, -10.0]);
    assert!((moved - 10.0).abs() < 1e-2 && (s.feet(R_RADIUS)[0] - (x + 10.0)).abs() < 1e-2, "{moved} {:?}", s.feet(R_RADIUS));
    // Out of its package's place now. Left alone for a while it walks back into it.
    s.run(1.0);
    assert!(s.feet(R_RADIUS)[1].abs() < 1e-3);
    s.run(40.0);
    let events = s.events(R_RADIUS);
    let back = events.iter().position(|e| matches!(e, AiEvent::PushedBack { .. })).expect("goes back");
    assert!(matches!(events[back + 1], AiEvent::Trip { .. }) && matches!(events.last().unwrap(), AiEvent::Arrived { .. }), "{events:?}");
    assert!(dist(s.feet(R_RADIUS), [350.0, 350.0, 0.0]) <= 150.0);
    assert_eq!(s.ai.agents[&actor].activity, Activity::There);
    // Who sits is not pushed.
    assert_eq!(s.ai.agents[&FormId(R_TO_CHAIR)].using, Some(FormId(CHAIR_C)));
    assert!(!s.ai.stands(&s.game, FormId(R_TO_CHAIR)));
    assert_eq!(s.ai.push(&mut s.game, &s.nav, FormId(R_TO_CHAIR), [5.0, 0.0]), 0.0);
}

// ---- combat states (fnv_ai::alert) ----------------------------------------------------

use fnv_game::awareness::{Cause, State, Timers};
use fnv_game::detection::{Band, Detection};

/// Put `actor` in Alert, about something of the player's at `at`.
fn alert(s: &mut Sim, actor: u32, at: [f32; 3]) {
    let (now, t) = (s.game.time, Timers::default());
    let a = s.game.awareness.actors.entry(FormId(actor)).or_default();
    a.alarm(FormId::PLAYER_REF, at, -10.0, Cause::Noticed, now, &t);
    a.settle(FormId(actor), now, &t, true);
    assert_eq!(a.state, State::Alert);
}

#[test]
fn an_alerted_actor_searches_where_the_alarm_is_and_goes_back_after() {
    let f = fixture();
    let mut s = sim(&f, &[R_RADIUS]);
    s.run(30.0);
    assert_eq!(s.ai.agents[&FormId(R_RADIUS)].activity, Activity::There);
    let stood = s.feet(R_RADIUS);
    alert(&mut s, R_RADIUS, [150.0, 100.0, 0.0]);
    s.run(1.0);
    assert!(s.events(R_RADIUS).iter().any(|e| matches!(e, AiEvent::Search { about, .. } if *about == FormId::PLAYER_REF)));
    s.run(8.0);
    // It looked about midway between where it stood and the hostile it only noticed
    // [documented: GECK wiki Combat Search; until 2026-10-12 at the alarm itself], then
    // searched on around there.
    let looked = s.events(R_RADIUS).into_iter().find_map(|e| match e {
        AiEvent::SearchLook { at, .. } => Some(*at),
        _ => None,
    });
    let midway = [(stood[0] + 150.0) / 2.0, (stood[1] + 100.0) / 2.0, 0.0];
    assert!(looked.is_some_and(|at| dist(at, midway) < 40.0), "{looked:?} {midway:?}");
    assert!(s.events(R_RADIUS).iter().filter(|e| matches!(e, AiEvent::Search { .. })).count() >= 2);
    // Back to Normal: the search ends and the actor goes back to its package's place.
    s.game.awareness.actors.remove(&FormId(R_RADIUS));
    s.run(30.0);
    let ev = s.events(R_RADIUS);
    let over = ev.iter().position(|e| matches!(e, AiEvent::SearchOver { .. })).expect("search over");
    assert!(matches!(ev[over + 1], AiEvent::SearchBack { .. }), "{ev:?}");
    assert!(dist(s.feet(R_RADIUS), [350.0, 350.0, 0.0]) <= 150.0);
    assert!(s.ai.agents[&FormId(R_RADIUS)].search.is_none());
}

/// An actor without packages (Ranger Ghost, playtest list 5 item 29) walks back to where
/// it was placed after a search.
#[test]
fn an_actor_without_packages_goes_back_to_where_it_was_placed() {
    let f = fixture();
    let mut s = sim(&f, &[R_LONER]);
    s.run(5.0);
    assert_eq!(s.feet(R_LONER), [350.0, 50.0, 0.0]);
    alert(&mut s, R_LONER, [350.0, 400.0, 0.0]);
    s.run(9.0);
    assert!(dist(s.feet(R_LONER), [350.0, 50.0, 0.0]) > 60.0, "{:?}", s.feet(R_LONER));
    s.game.awareness.actors.remove(&FormId(R_LONER));
    s.run(30.0);
    let ev = s.events(R_LONER);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::BackHome { .. })), "{ev:?}");
    assert!(dist(s.feet(R_LONER), [350.0, 50.0, 0.0]) <= 40.0, "{:?}", s.feet(R_LONER));
    assert_eq!(s.ai.agents[&FormId(R_LONER)].activity, Activity::There);
}

#[test]
fn continue_during_combat_keeps_the_package_going() {
    let f = fixture();
    let mut s = sim(&f, &[R_STEADY, R_TRAVEL]);
    s.run(1.0);
    for a in [R_STEADY, R_TRAVEL] {
        assert_eq!(s.ai.agents[&FormId(a)].activity, Activity::Travelling);
        alert(&mut s, a, [100.0, 100.0, 0.0]);
    }
    s.run(30.0);
    // The courier's package continues during combat: it walks on to cell B.
    let steady = s.events(R_STEADY);
    assert!(!steady.iter().any(|e| matches!(e, AiEvent::Search { .. } | AiEvent::Hold { .. })), "{steady:?}");
    assert!(matches!(steady.last().unwrap(), AiEvent::Arrived { .. }), "{steady:?}");
    assert!(dist(s.feet(R_STEADY), [5250.0, 5250.0, 50.0]) < 5.0);
    // The traveller's does not: it breaks off to search and never gets there.
    let travel = s.events(R_TRAVEL);
    assert!(travel.iter().any(|e| matches!(e, AiEvent::Search { .. })), "{travel:?}");
    assert!(!travel.iter().any(|e| matches!(e, AiEvent::Arrived { .. })), "{travel:?}");
}

#[test]
fn an_actor_in_combat_stops_and_faces_its_target() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    s.run(1.0);
    let (now, t) = (s.game.time, Timers::default());
    let a = s.game.awareness.actors.entry(FormId(R_TRAVEL)).or_default();
    a.observe(FormId::PLAYER_REF, &Detection { value: 5.0, band: Band::Detected, ..Default::default() }, [50.0, 300.0, 0.0], now, &t);
    a.settle(FormId(R_TRAVEL), now, &t, true);
    let at = s.feet(R_TRAVEL);
    s.run(3.0);
    assert!(s.events(R_TRAVEL).iter().any(|e| matches!(e, AiEvent::Hold { target, .. } if *target == FormId::PLAYER_REF)));
    assert_eq!(s.ai.agents[&FormId(R_TRAVEL)].activity, Activity::There);
    assert!(dist(s.feet(R_TRAVEL), at) < 1.0, "it stands");
}

#[test]
fn an_actor_in_combat_closes_in_on_a_target_it_cannot_see() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    s.run(1.0);
    let (now, t) = (s.game.time, Timers::default());
    let at = s.feet(R_TRAVEL);
    let target = [at[0] + 300.0, at[1], at[2]];
    let a = s.game.awareness.actors.entry(FormId(R_TRAVEL)).or_default();
    a.observe(FormId::PLAYER_REF, &Detection { value: 5.0, band: Band::Detected, ..Default::default() }, target, now, &t);
    a.settle(FormId(R_TRAVEL), now, &t, true);
    // Within the distance it fights from, but the target is out of sight now.
    s.game.awareness.engage.insert(FormId(R_TRAVEL), (1000.0, 1200.0));
    let a = s.game.awareness.actors.get_mut(&FormId(R_TRAVEL)).unwrap();
    a.observe(FormId::PLAYER_REF, &Detection { value: -10.0, band: Band::Noticed, ..Default::default() }, target, now, &t);
    s.run(2.0);
    assert!(s.events(R_TRAVEL).iter().any(|e| matches!(e, AiEvent::Close { target, .. } if *target == FormId::PLAYER_REF)), "{:?}", s.events(R_TRAVEL));
    assert!(dist(s.feet(R_TRAVEL), target) < 280.0, "it moved towards the target");
}

/// Put `actor` in Combat with the player at `at`, detected.
fn fighting(s: &mut Sim, actor: u32, at: [f32; 3]) {
    let (now, t) = (s.game.time, Timers::default());
    let a = s.game.awareness.actors.entry(FormId(actor)).or_default();
    a.observe(FormId::PLAYER_REF, &Detection { value: 5.0, band: Band::Detected, ..Default::default() }, at, now, &t);
    a.settle(FormId(actor), now, &t, true);
    assert_eq!(a.state, State::Combat);
}

#[test]
fn a_fleeing_actor_runs_away_and_stays_at_its_spot() {
    // Combat Flee (combat-ai.md §12.2): no door or cover near, so it runs straight away
    // from the player; the flee distance shortened to fit cell C (1,000 units square).
    use fnv_game::combat_group::{FleeBy, Flight, GroupSettings};
    let f = fixture();
    let mut s = sim(&f, &[R_SB_NOTHING]);
    s.run(1.0);
    let threat = [10450.0, 10150.0, 0.0];
    s.game.awareness.tactics.settings = Some(GroupSettings { flee_normal: 400.0, ..GroupSettings::default() });
    fighting(&mut s, R_SB_NOTHING, threat);
    let now = s.game.time;
    s.game.awareness.tactics.flights.insert(FormId(R_SB_NOTHING), Flight { from: FormId::PLAYER_REF, from_at: threat, since: now, spot: None, arrived: None });
    let before = dist(s.feet(R_SB_NOTHING), threat);
    s.run(4.0);
    let ev = s.events(R_SB_NOTHING);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Flee { how: FleeBy::Away, .. })), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, AiEvent::FleeArrived { .. })), "{ev:?}");
    let fled = s.feet(R_SB_NOTHING);
    assert!(dist(fled, threat) > before + 300.0, "{fled:?}");
    assert!(s.game.awareness.tactics.flights[&FormId(R_SB_NOTHING)].arrived.is_some());
    // It stays at its spot.
    s.run(5.0);
    assert!(dist(s.feet(R_SB_NOTHING), fled) < 1.0);
}

#[test]
fn a_fleeing_actor_takes_a_door_near_it() {
    // Combat Flee's first try: a door within fCombatFleeMaxDoorDistance, not nearer the
    // attacker than the actor is. The traveller in cell A has the door to B 400 units off.
    use fnv_game::combat_group::{FleeBy, Flight};
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    s.run(0.5);
    let threat = [0.0, 0.0, 0.0];
    fighting(&mut s, R_TRAVEL, threat);
    let now = s.game.time;
    s.game.awareness.tactics.flights.insert(FormId(R_TRAVEL), Flight { from: FormId::PLAYER_REF, from_at: threat, since: now, spot: None, arrived: None });
    s.run(1.0);
    let ev = s.events(R_TRAVEL);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Flee { how: FleeBy::Door { door }, .. } if door.0 == DOOR_A)), "{ev:?}");
}

#[test]
fn a_fighter_goes_to_its_post_and_faces_its_target() {
    // A post (combat-ai.md §12.1): the combat AI wants it on its target's flank.
    use fnv_game::combat_group::{Post, PostKind};
    let f = fixture();
    let mut s = sim(&f, &[R_SB_NOTHING]);
    s.run(1.0);
    let target = [10500.0, 10900.0, 0.0];
    fighting(&mut s, R_SB_NOTHING, target);
    s.game.awareness.engage.insert(FormId(R_SB_NOTHING), (2000.0, 2500.0));
    let post = [10200.0, 10600.0, 0.0];
    s.game.awareness.tactics.posts.insert(FormId(R_SB_NOTHING), Post { at: post, kind: PostKind::Flank, run: true });
    s.run(3.0);
    let ev = s.events(R_SB_NOTHING);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Post { post: PostKind::Flank, .. })), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, AiEvent::AtPost { post: PostKind::Flank, .. })), "{ev:?}");
    assert!(dist(s.feet(R_SB_NOTHING), post) < 40.0);
    // A post it cannot reach is dropped.
    s.game.awareness.tactics.posts.insert(FormId(R_SB_NOTHING), Post { at: [20000.0, 20000.0, 0.0], kind: PostKind::Cover, run: true });
    s.run(1.0);
    assert!(s.events(R_SB_NOTHING).iter().any(|e| matches!(e, AiEvent::NoPost { post: PostKind::Cover })));
    assert!(!s.game.awareness.tactics.posts.contains_key(&FormId(R_SB_NOTHING)));
}

#[test]
fn a_fighter_running_to_its_post_walks_once_it_raises_its_sights() {
    // Aiming down the sights slows it to a walk (alert.rs module notes), also on a run to
    // a post already under way.
    use fnv_game::combat_group::{Post, PostKind};
    let f = fixture();
    let mut s = sim(&f, &[R_SB_NOTHING]);
    s.run(1.0);
    let target = [10500.0, 10900.0, 0.0];
    fighting(&mut s, R_SB_NOTHING, target);
    s.game.awareness.engage.insert(FormId(R_SB_NOTHING), (2000.0, 2500.0));
    let post = [10200.0, 10600.0, 0.0];
    s.game.awareness.tactics.posts.insert(FormId(R_SB_NOTHING), Post { at: post, kind: PostKind::Flank, run: true });
    s.run(0.5);
    assert!(s.events(R_SB_NOTHING).iter().any(|e| matches!(e, AiEvent::Post { post: PostKind::Flank, .. })), "{:?}", s.events(R_SB_NOTHING));
    assert!(!s.events(R_SB_NOTHING).iter().any(|e| matches!(e, AiEvent::AtPost { .. })), "still on its way");
    s.game.awareness.aiming.insert(FormId(R_SB_NOTHING));
    s.run(0.5);
    let ev = s.events(R_SB_NOTHING);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Pace { aiming: true, .. })), "{ev:?}");
}

#[test]
fn a_search_party_splits_the_circle() {
    // Combat Search (combat-ai.md §12.5): two sitters placed from one base form, alarmed
    // by the same shot, search together, each its own half of the circle round it.
    let f = fixture();
    let mut s = sim(&f, &[R_SB_OBJECTS, R_SB_OBJECTS_2]);
    s.run(1.0);
    let t = Timers::default();
    for a in [R_SB_OBJECTS, R_SB_OBJECTS_2] {
        let now = s.game.time;
        let x = s.game.awareness.actors.entry(FormId(a)).or_default();
        x.alarm(FormId::PLAYER_REF, MIDDLE_C, -5.0, Cause::Event(fnv_game::awareness::EventType::Bullet), now, &t);
        x.settle(FormId(a), now, &t, true);
    }
    assert_eq!(s.game.search_party(FormId(R_SB_OBJECTS), FormId::PLAYER_REF), (0, 2));
    assert_eq!(s.game.search_party(FormId(R_SB_OBJECTS_2), FormId::PLAYER_REF), (1, 2));
    s.run(18.0);
    let points = |a: u32| -> Vec<[f32; 3]> {
        s.events(a)
            .into_iter()
            .filter_map(|e| match e {
                AiEvent::Search { to, .. } => Some(*to),
                _ => None,
            })
            .skip(1)
            .collect()
    };
    let (first, second) = (points(R_SB_OBJECTS), points(R_SB_OBJECTS_2));
    assert!(!first.is_empty() && !second.is_empty(), "{first:?} {second:?}");
    // Index 0 searches the half north of the centre, index 1 the half south.
    assert!(first.iter().all(|p| p[1] >= MIDDLE_C[1] - 1.0), "{first:?}");
    assert!(second.iter().all(|p| p[1] <= MIDDLE_C[1] + 1.0), "{second:?}");
}

#[test]
fn a_rooted_actor_stays_where_it_is() {
    // A creature that cannot walk (a raven) keeps its place, whatever its package says.
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    let start = s.feet(R_TRAVEL);
    s.ai.root(&s.game, FormId(R_TRAVEL));
    s.run(30.0);
    assert_eq!(s.feet(R_TRAVEL), start);
    assert_eq!(s.ai.agents[&FormId(R_TRAVEL)].activity, Activity::None);
    assert!(s.events(R_TRAVEL).is_empty(), "{:?}", s.events(R_TRAVEL));
    assert!(!s.ai.stands(&s.game, FormId(R_TRAVEL)));
}

// ---- process levels (fnv_ai::process) -------------------------------------------------

#[test]
fn an_actor_out_of_the_loaded_area_walks_on_in_coarse_updates() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    s.run(1.0);
    let actor = FormId(R_TRAVEL);
    assert_eq!(s.ai.agents[&actor].activity, Activity::Travelling);
    assert!(s.ai.set_high(&s.game, actor, false));
    assert!(!s.ai.set_high(&s.game, actor, false), "no change");
    assert_eq!(s.ai.process_counts(), (0, 1));
    // It stands between its updates and covers the time owed in one: one move, which
    // takes it all the way (the walk takes about 8 s, an update owes up to 18).
    let mut moves = 0;
    let mut at = s.feet(R_TRAVEL);
    for _ in 0..(2.0 * (fnv_ai::LOW_PROCESS_SECONDS + 1.0)) as usize {
        s.run(0.5);
        if dist(s.feet(R_TRAVEL), at) > 1e-3 {
            moves += 1;
            at = s.feet(R_TRAVEL);
        }
    }
    assert_eq!(moves, 1);
    assert!(dist(s.feet(R_TRAVEL), [5250.0, 5250.0, 50.0]) < 5.0);
    assert!(matches!(s.events(R_TRAVEL).last(), Some(AiEvent::Arrived { .. })), "{:?}", s.events(R_TRAVEL));
    assert_eq!(s.game.world.peek(actor).unwrap().cell, Some(FormId(CELL_B)));
    assert!(s.ai.process_stats.low_updates >= 1);
    // Back in the loaded area: every update again.
    assert!(s.ai.set_high(&s.game, actor, true));
    assert_eq!(s.ai.process_counts(), (1, 0));
    assert_eq!((s.ai.process_stats.left, s.ai.process_stats.came_back), (1, 1));
}

#[test]
fn an_actor_out_of_the_loaded_area_searches_no_more_and_takes_up_its_package() {
    let f = fixture();
    let mut s = sim(&f, &[R_RADIUS]);
    s.run(30.0);
    alert(&mut s, R_RADIUS, [150.0, 100.0, 0.0]);
    s.run(1.0);
    assert!(s.ai.agents[&FormId(R_RADIUS)].search.is_some());
    s.ai.set_high(&s.game, FormId(R_RADIUS), false);
    assert!(s.ai.agents[&FormId(R_RADIUS)].search.is_none());
    assert_eq!(s.ai.process_stats.dropped, 1);
    // Its updates keep to its package: no search starts again while it is out.
    s.run(40.0);
    let ev = s.events(R_RADIUS);
    let searches = ev.iter().filter(|e| matches!(e, AiEvent::Search { .. })).count();
    s.run(40.0);
    assert_eq!(s.events(R_RADIUS).iter().filter(|e| matches!(e, AiEvent::Search { .. })).count(), searches);
    assert!(dist(s.feet(R_RADIUS), [350.0, 350.0, 0.0]) <= 150.0);
}

#[test]
fn a_random_conversation_ends_when_one_of_the_two_leaves_the_loaded_area() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL, R_RADIUS]);
    let (a, b) = (FormId(R_TRAVEL), FormId(R_RADIUS));
    s.ai.agents.get_mut(&a).unwrap().chat = Some(fnv_ai::Chat::Going { partner: b, since: 0.0 });
    s.ai.agents.get_mut(&b).unwrap().chat = Some(fnv_ai::Chat::Waiting { starter: a });
    s.ai.set_high(&s.game, a, false);
    assert_eq!(s.ai.agents[&a].chat, None);
    assert_eq!(s.ai.agents[&b].chat, None, "the partner stops waiting");
    // The waiting one leaves instead: the starter finds nobody waiting and gives up.
    s.ai.set_high(&s.game, a, true);
    s.ai.agents.get_mut(&a).unwrap().chat = Some(fnv_ai::Chat::Going { partner: b, since: 0.0 });
    s.ai.agents.get_mut(&b).unwrap().chat = Some(fnv_ai::Chat::Waiting { starter: a });
    s.ai.set_high(&s.game, b, false);
    assert_eq!(s.ai.agents[&b].chat, None);
    assert_eq!(s.ai.process_stats.dropped, 2);
}

#[test]
fn an_actor_with_no_low_level_processing_stands_still_out_of_the_loaded_area() {
    let f = fixture();
    let mut s = sim(&f, &[R_STEADY]);
    s.run(1.0);
    let actor = FormId(R_STEADY);
    assert!(s.game.no_low_level_processing(actor));
    assert_eq!(s.ai.agents[&actor].activity, Activity::Travelling);
    s.ai.set_high(&s.game, actor, false);
    let at = s.feet(R_STEADY);
    s.run(60.0);
    assert_eq!(s.feet(R_STEADY), at, "no update out of the loaded area");
    // Nor in hours passed in the Wait menu.
    s.game.open_menu(1012);
    assert_eq!(s.ai.pass_time(&mut s.game, &s.nav, 120.0, 0.5), 0);
    assert_eq!(s.feet(R_STEADY), at);
    assert_eq!((s.ai.process_stats.frozen, s.ai.process_stats.low_updates), (1, 0));
    // Back in the loaded area it walks on.
    s.game.close_menu();
    s.ai.set_high(&s.game, actor, true);
    s.run(30.0);
    assert!(dist(s.feet(R_STEADY), [5250.0, 5250.0, 50.0]) < 5.0);
}

#[test]
fn an_actor_out_of_the_cell_buffer_is_updated_every_two_minutes() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    s.run(1.0);
    let actor = FormId(R_TRAVEL);
    assert_eq!(s.ai.agents[&actor].activity, Activity::Travelling);
    assert!(s.ai.set_level(&s.game, actor, fnv_ai::ProcessLevel::Low));
    assert!(!s.ai.set_level(&s.game, actor, fnv_ai::ProcessLevel::Low), "no change");
    assert_eq!(s.ai.level(actor), fnv_ai::ProcessLevel::Low);
    assert_eq!((s.ai.process_counts(), s.ai.far_count()), ((0, 1), 1));
    // Its first update comes within the interval (spread by its share of it), and walks
    // it all the way; the next one an interval later.
    let mut waited = 0;
    while s.ai.process_stats.far_updates == 0 {
        s.run(1.0);
        waited += 1;
        assert!(waited <= 121, "an update within {} s", fnv_ai::FAR_PROCESS_SECONDS);
    }
    assert!(dist(s.feet(R_TRAVEL), [5250.0, 5250.0, 50.0]) < 5.0, "one update took it there");
    s.run(f64::from(fnv_ai::FAR_PROCESS_SECONDS) - 2.0);
    assert_eq!(s.ai.process_stats.far_updates, 1, "not before {} s", fnv_ai::FAR_PROCESS_SECONDS);
    s.run(3.0);
    assert_eq!((s.ai.process_stats.fell_far, s.ai.process_stats.far_updates), (1, 2));
    assert_eq!(s.ai.process_stats.low_updates, 2, "no Mid-High update in between");
    // Back up to Mid-High, then into the loaded area.
    assert!(s.ai.set_level(&s.game, actor, fnv_ai::ProcessLevel::MidHigh));
    assert_eq!((s.ai.level(actor), s.ai.far_count()), (fnv_ai::ProcessLevel::MidHigh, 0));
    assert!(s.ai.set_level(&s.game, actor, fnv_ai::ProcessLevel::High));
    assert_eq!(s.ai.process_counts(), (1, 0));
}

#[test]
fn a_follower_is_processed_at_least_as_often_as_its_leader() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL, R_DOG]);
    s.run(1.0);
    let (leader, dog) = (FormId(R_TRAVEL), FormId(R_DOG));
    // The leader is in the loaded area: the dog beyond the buffer is kept at Mid-High.
    assert!(s.ai.set_level(&s.game, dog, fnv_ai::ProcessLevel::Low));
    assert_eq!(s.ai.level(dog), fnv_ai::ProcessLevel::MidHigh);
    // The leader beyond the buffer too: so is the dog.
    assert!(s.ai.set_level(&s.game, leader, fnv_ai::ProcessLevel::Low));
    assert!(s.ai.set_level(&s.game, dog, fnv_ai::ProcessLevel::Low));
    assert_eq!(s.ai.level(dog), fnv_ai::ProcessLevel::Low);
}

#[test]
fn an_actor_walking_into_the_loaded_area_is_moved_every_frame() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    s.run(1.0);
    let actor = FormId(R_TRAVEL);
    assert_eq!(s.ai.agents[&actor].activity, Activity::Travelling);
    assert!(s.ai.set_level(&s.game, actor, fnv_ai::ProcessLevel::Coming));
    assert_eq!(s.ai.level(actor), fnv_ai::ProcessLevel::Coming);
    assert_eq!(s.ai.process_counts(), (0, 1));
    // Owner's playtest list 4 item 13: it walks on in real time, not in 18 s jumps.
    for _ in 0..5 {
        let at = s.feet(R_TRAVEL);
        s.run(0.5);
        assert!(dist(s.feet(R_TRAVEL), at) > 1.0, "it walked in this half second");
    }
    assert_eq!(s.ai.process_stats.coming, 1);
    assert!(s.ai.process_stats.coming_updates >= 5);
    // Into the loaded area: the high process.
    assert!(s.ai.set_level(&s.game, actor, fnv_ai::ProcessLevel::High));
    assert_eq!((s.ai.level(actor), s.ai.process_counts()), (fnv_ai::ProcessLevel::High, (1, 0)));
}

/// A Use Weapon package (`fnv_game::use_weapon`): the actor walks to the package's
/// location and turns to its target; there its shooting begins, and the package's goal
/// is met only once its shots are spent.
#[test]
fn a_use_weapon_package_stands_at_its_place_and_meets_its_goal_when_its_shots_are_spent() {
    use fnv_game::use_weapon::{After, Source};
    let f = fixture();
    assert!(f.data.issues.is_empty(), "{:?}", f.data.issues.to_json());
    let mut s = sim(&f, &[R_SHOOTER]);
    s.run(15.0);
    let actor = FormId(R_SHOOTER);
    let ev = s.events(R_SHOOTER);
    assert!(matches!(ev[1], AiEvent::Trip { .. }), "{ev:?}");
    assert!(matches!(ev[2], AiEvent::Arrived { .. }), "{ev:?}");
    assert_eq!(s.ai.agents[&actor].activity, Activity::There);
    assert!(dist(s.feet(R_SHOOTER), [350.0, 350.0, 0.0]) < 40.0);
    let w = s.game.weapon_use(actor).expect("a weapon use").clone();
    assert_eq!(w.source, Source::Package(FormId(P_SHOOT)));
    assert_eq!((w.target, w.weapon, w.volleys, w.shots_per_volley, w.pause), (FormId(R_LONER), None, Some(2), [1, 1], Some([0.5, 0.5])));
    assert!(w.always_hit && w.no_damage && !w.crouch_to_reload && !w.hold_fire_when_blocked);
    assert!(w.in_place);
    // Not met on arriving: the shots are still to come.
    assert!(!s.game.extra[&actor].package_goal_met);
    // It faces its target, the loner south of it.
    let feet = s.feet(R_SHOOTER);
    let want = fnv_nav::follow::heading_of([350.0 - feet[0], 50.0 - feet[1]]);
    let have = s.ai.agents[&actor].walker.heading;
    assert!((want - have).sin().abs() < 0.05 && (want - have).cos() > 0.0, "heading {have}, want {want}");
    // Two volleys of a shot each; spent, the goal is met and the use stays, done.
    let u = s.game.weapon_uses.get_mut(&actor).unwrap();
    assert_eq!(u.shot(0.3), After::Pause(0.5));
    assert_eq!(u.shot(0.3), After::Done);
    s.game.end_weapon_use(actor);
    assert!(s.game.extra[&actor].package_goal_met);
    assert!(s.game.weapon_use(actor).is_some_and(|w| w.done));
    assert!(s.game.active_weapon_use(actor).is_none());
}

/// The UseWeapon function (`fnv_game::use_weapon`): the actor walks to where it is to
/// stand, and once the use is over goes back to its package.
#[test]
fn use_weapon_sends_the_actor_to_its_place_and_back_to_its_package_after() {
    use fnv_script::vm::{ArgValue, Value};
    let f = fixture();
    let mut s = sim(&f, &[R_LONER]);
    s.run(1.0);
    let actor = FormId(R_LONER);
    let opcode = s.game.data.table.by_name("UseWeapon").unwrap();
    let ctx = fnv_game::functions::Ctx { unit: "test".into(), opcode, on: Some(actor), self_ref: Some(actor), action_ref: None, quest: None, inst: None, in_condition: false };
    let r = |x: u32| ArgValue::Value(Value::Ref(x));
    let n = |x: f64| ArgValue::Value(Value::Num(x));
    s.game.call_function(&ctx, &[r(STAT), r(MARK_A), r(R_HERE), n(4.0), n(1.0), n(0.0), n(1.0)]);
    let w = s.game.weapon_use(actor).expect("a weapon use").clone();
    assert_eq!((w.stand, w.target, w.volleys, w.shots_per_volley), (Some(FormId(MARK_A)), FormId(R_HERE), Some(1), [4, 4]));
    // The defaults of the arguments left out: crouch to reload and hold fire on unless
    // given, no damage off [documented: GECK wiki UseWeapon].
    assert!(w.crouch_to_reload && !w.hold_fire_when_blocked && w.always_hit && !w.no_damage && w.target_here.is_none());
    s.run(10.0);
    let ev = s.events(R_LONER);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::WeaponUse { target, length } if target.0 == R_HERE && *length > 250.0)), "{ev:?}");
    assert!(dist(s.feet(R_LONER), [350.0, 350.0, 0.0]) < 40.0);
    assert!(s.game.weapon_use(actor).unwrap().in_place);
    // Over: it goes back to where it was placed (it has no package).
    s.game.end_weapon_use(actor);
    assert!(s.game.weapon_use(actor).is_none());
    s.run(10.0);
    let ev = s.events(R_LONER);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::WeaponUseOver)), "{ev:?}");
}

/// A melee use of UseWeapon (docs/design/use-weapon.md §3.1): standing on its own
/// reference, the actor closes in on its target within the reach the engine set, stops
/// there, and gives the closing in up when the use is over.
#[test]
fn a_melee_use_closes_in_on_its_target_within_its_reach() {
    use fnv_script::vm::{ArgValue, Value};
    let f = fixture();
    let mut s = sim(&f, &[R_LONER]);
    s.run(1.0);
    let actor = FormId(R_LONER);
    let opcode = s.game.data.table.by_name("UseWeapon").unwrap();
    let ctx = fnv_game::functions::Ctx { unit: "test".into(), opcode, on: Some(actor), self_ref: Some(actor), action_ref: None, quest: None, inst: None, in_condition: false };
    let r = |x: u32| ArgValue::Value(Value::Ref(x));
    let n = |x: f64| ArgValue::Value(Value::Num(x));
    // Standing on its own reference: in place at once, 300 units from its target.
    s.game.call_function(&ctx, &[r(STAT), r(R_LONER), r(R_HERE), n(2.0)]);
    s.run(0.5);
    assert!(s.game.weapon_use(actor).unwrap().in_place);
    assert_eq!(s.feet(R_LONER), [350.0, 50.0, 0.0], "a gun's use stays where it is");
    // The engine finds a melee weapon: a reach to close within.
    s.game.weapon_uses.get_mut(&actor).unwrap().reach = Some([70.0, 100.0]);
    s.run(10.0);
    let away = dist(s.feet(R_LONER), START);
    assert!(away <= 100.0 && away > 20.0, "{away}");
    let ev = s.events(R_LONER);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Close { target, .. } if target.0 == R_HERE)), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, AiEvent::Hold { target, .. } if target.0 == R_HERE)), "{ev:?}");
    // Over: the closing in ends with it.
    s.game.end_weapon_use(actor);
    s.run(1.0);
    assert!(s.ai.agents[&actor].search.is_none());
    assert!(s.events(R_LONER).iter().any(|e| matches!(e, AiEvent::WeaponUseOver)));
}

/// DisableNavmesh while an actor walks a way over that navmesh: it plans again, finds no
/// way and sets the package aside; enabled again, the way is there (docs/design/
/// script-stubs.md §3.5). The switches are applied once per change.
#[test]
fn a_navmesh_disabled_under_a_walk_sends_the_actor_to_plan_again() {
    let f = fixture();
    let mut s = sim(&f, &[R_TRAVEL]);
    let actor = FormId(R_TRAVEL);
    s.run(1.0);
    assert_eq!(s.ai.agents[&actor].activity, Activity::Travelling);
    assert_eq!(s.game.extra[&actor].ai_phase, Some(AiPhase::Travelling));
    assert_eq!(s.ai.apply_navmesh_switches(&mut s.game, &mut s.nav), None);
    // Cell B's navmesh goes: the way through the door ends on it.
    s.game.nav_switches.navmeshes.insert(FormId(2), false);
    assert_eq!(s.ai.apply_navmesh_switches(&mut s.game, &mut s.nav), Some(vec![actor]));
    assert_eq!(s.ai.apply_navmesh_switches(&mut s.game, &mut s.nav), None, "applied once");
    s.run(5.0);
    assert!(s.events(R_TRAVEL).iter().any(|e| matches!(e, AiEvent::Unreachable { package, .. } if package.0 == P_TRAVEL_B)), "{:?}", s.events(R_TRAVEL));
    s.run(30.0);
    assert_ne!(s.game.world.peek(actor).unwrap().cell, Some(FormId(CELL_B)));
    // Enabled again: chosen afresh, it gets there.
    s.game.nav_switches.navmeshes.insert(FormId(2), true);
    s.ai.apply_navmesh_switches(&mut s.game, &mut s.nav);
    s.game.extra.get_mut(&actor).unwrap().package_blocked.clear();
    s.game.choose_packages_now();
    s.run(40.0);
    assert!(dist(s.feet(R_TRAVEL), [5250.0, 5250.0, 50.0]) < 5.0);
    assert!(s.game.extra[&actor].package_goal_met);
    assert_eq!(s.game.extra[&actor].ai_phase, Some(AiPhase::There));
}

/// What a sandboxer does is told to the game for GetCurrentAIProcedure
/// (`fnv_game::ai::SandboxPhase`; docs/spec-notes/packages.md §4, procedure codes): walking to the
/// chair or marker, sitting, at the marker, and up from the chair until the next thing.
#[test]
fn a_sandboxer_tells_the_game_what_it_does() {
    let f = fixture();
    let mut s = sim(&f, &[R_SB_OBJECTS]);
    let actor = FormId(R_SB_OBJECTS);
    let mut seen = Vec::new();
    for _ in 0..1200 {
        s.run(0.5);
        let Some(AiPhase::Sandbox(p)) = s.game.extra.get(&actor).and_then(|e| e.ai_phase) else { continue };
        if seen.last() != Some(&p) {
            seen.push(p);
        }
        let a = &s.ai.agents[&actor];
        match p {
            SandboxPhase::Sitting => assert!(matches!(a.sandbox.as_ref().unwrap().doing, Doing::Sitting(_))),
            SandboxPhase::ToObject => assert_eq!(a.activity, Activity::Travelling),
            _ => {}
        }
    }
    for p in [SandboxPhase::ToObject, SandboxPhase::Sitting, SandboxPhase::AtIdleMarker, SandboxPhase::GotUp] {
        assert!(seen.contains(&p), "{p:?} not in {seen:?}");
    }
    // Up from the chair, then (after walking to it) the idle marker: never the chair again
    // straight from getting up.
    for w in seen.windows(2) {
        if w[0] == SandboxPhase::GotUp {
            assert_ne!(w[1], SandboxPhase::Sitting, "{seen:?}");
        }
    }
}
