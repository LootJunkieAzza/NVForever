//! Carrying packages out (`fnv_ai`) on a plugin and navmeshes built in memory (no game
//! data): two interior cells joined by a door, markers, and actors with one package
//! each. The rules and their sources: docs/design/actor-behaviour.md §5.

use fnv_ai::{Activity, Ai, AiConfig, AiEvent, Doing};
use fnv_esm::raw::{write_subrecords, Subrecord};
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

fn npc(fid: u32, edid: &str, packages: &[u32]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    subs.extend(packages.iter().map(|p| sub(b"PKID", p.to_le_bytes().to_vec())));
    record(b"NPC_", fid, &subs)
}

fn placed(sig: &[u8; 4], fid: u32, edid: &str, base: u32, pos: [f32; 3], linked: Option<u32>) -> Vec<u8> {
    placed_with(sig, fid, edid, base, pos, 0.0, linked, None)
}

/// [`placed`] with a heading (z rotation, radians) and a patrol idle time (XPRD).
#[allow(clippy::too_many_arguments)]
fn placed_with(sig: &[u8; 4], fid: u32, edid: &str, base: u32, pos: [f32; 3], heading: f32, linked: Option<u32>, idle: Option<f32>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec())];
    subs.extend(idle.map(|s| sub(b"XPRD", s.to_le_bytes().to_vec())));
    subs.extend(linked.map(|l| sub(b"XLKR", l.to_le_bytes().to_vec())));
    subs.push(sub(b"DATA", [pos[0].to_le_bytes(), pos[1].to_le_bytes(), pos[2].to_le_bytes(), [0; 4], [0; 4], heading.to_le_bytes()].concat()));
    record(sig, fid, &subs)
}

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
const N_FAST: u32 = 0x8E0;
const R_FAST: u32 = 0x8E1;
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
        // The Visitor's package at Speed Multiplier 200 (ACBS: flags, fatigue, barter
        // gold, level, calc min and max, speed multiplier, karma, disposition, template
        // flags).
        record(
            b"NPC_",
            N_FAST,
            &[
                sub(b"EDID", z("FastVisitor")),
                sub(b"ACBS", [vec![0u8; 14], 200u16.to_le_bytes().to_vec(), vec![0; 8]].concat()),
                sub(b"PKID", P_RADIUS.to_le_bytes().to_vec()),
            ],
        ),
        npc(N_SB_ALL, "Idler", &[P_SB_ALL]),
        npc(N_SB_WANDER, "Wanderer", &[P_SB_WANDER]),
        npc(N_SB_OBJECTS, "Sitter", &[P_SB_OBJECTS]),
        npc(N_SB_NOTHING, "Statue", &[P_SB_NOTHING]),
        npc(N_TO_CHAIR, "ChairUser", &[P_TO_CHAIR]),
        npc(N_SLEEP_C, "Sleeper", &[P_SLEEP_C]),
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
        placed(b"ACHR", R_FAST, "FastVisitorRef", N_FAST, START, None),
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

/// An actor travels at the standard speed times its Speed Multiplier over 100
/// (actor-values.md §6), as the value is when it sets off.
#[test]
fn the_speed_multiplier_scales_the_walk() {
    let f = fixture();
    let mut s = sim(&f, &[R_FAST, R_RADIUS]);
    s.run(1.0);
    assert_eq!(s.ai.agents[&FormId(R_RADIUS)].walker.speed, 100.0);
    assert_eq!(s.ai.agents[&FormId(R_FAST)].walker.speed, 200.0);
    let (slow, fast) = (dist(s.feet(R_RADIUS), START), dist(s.feet(R_FAST), START));
    assert!(slow > 10.0 && (fast - 2.0 * slow).abs() < 0.05 * fast, "{slow} {fast}");
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
            assert!((a.walker.heading - 1.25).abs() < 1e-4, "heading {}", a.walker.heading);
            assert_eq!(a.using, None);
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
