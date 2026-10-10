//! Carrying packages out (`fnv_ai`) on a plugin and navmeshes built in memory (no game
//! data): two interior cells joined by a door, markers, and actors with one package
//! each. The rules and their sources: docs/design/actor-behaviour.md §5.

use fnv_ai::{Activity, Ai, AiConfig, AiEvent};
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::package::{flags, PackageCheck};
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
    let mut subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"PKDT", [&general.to_le_bytes()[..], &[ty, 0], &[0; 6]].concat()),
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
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec())];
    subs.extend(linked.map(|l| sub(b"XLKR", l.to_le_bytes().to_vec())));
    subs.push(sub(b"DATA", [pos[0].to_le_bytes(), pos[1].to_le_bytes(), pos[2].to_le_bytes(), [0; 4], [0; 4], [0; 4]].concat()));
    record(sig, fid, &subs)
}

const G_DAYS: u32 = 0x805;
const STAT: u32 = 0x810;
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
// Bases.
const N_TRAVEL: u32 = 0x840;
const N_IN: u32 = 0x841;
const N_RADIUS: u32 = 0x842;
const N_PATROL: u32 = 0x843;
const N_LOOP: u32 = 0x844;
const N_FOLLOW: u32 = 0x845;
const N_HERE: u32 = 0x846;
const N_RUN: u32 = 0x847;
// Cells and references.
const CELL_A: u32 = 0x860;
const CELL_B: u32 = 0x861;
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
const DOOR_A: u32 = 0x8D1;
const DOOR_B: u32 = 0x8D2;

const TRAVEL: u8 = 6;
const SANDBOX: u8 = 12;
const PATROL: u8 = 13;
const FOLLOW: u8 = 1;
const START: [f32; 3] = [50.0, 50.0, 0.0];

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
        pack(P_DEFAULT, "StayHome", 0, SANDBOX, (3, 0, 64), None),
        pack(P_IN_B, "SandboxInB", 0, SANDBOX, (1, CELL_B, 0), None),
        pack(P_RADIUS, "TravelNearA", 0, TRAVEL, (0, MARK_A, 150), None),
        pack(P_PATROL, "PatrolOnce", 0, PATROL, (0, M1, 0), Some(false)),
        pack(P_LOOP, "PatrolRound", 0, PATROL, (6, 0, 0), Some(true)),
        pack(P_FOLLOW, "FollowSomeone", 0, FOLLOW, (3, 0, 0), None),
        pack(P_HERE, "TravelHere", flags::MUST_COMPLETE, TRAVEL, (2, 0, 0), None),
        pack(P_RUN, "RunToA", flags::ALWAYS_RUN, TRAVEL, (0, MARK_A, 0), None),
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
    ]
    .concat();
    let stat = record(b"STAT", STAT, &[sub(b"EDID", z("Marker"))]);
    let a_refs = [
        placed(b"REFR", MARK_A, "MarkA", STAT, [350.0, 350.0, 0.0], None),
        placed(b"REFR", M1, "M1", STAT, [350.0, 50.0, 0.0], Some(M2)),
        placed(b"REFR", M2, "M2", STAT, [350.0, 350.0, 0.0], Some(M3)),
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
    ]
    .concat();
    let b_refs = placed(b"REFR", MARK_B, "MarkB", STAT, [5250.0, 5250.0, 50.0], None);
    let cell = |id: u32, edid: &str, refs: &[u8]| {
        let rec = record(b"CELL", id, &[sub(b"EDID", z(edid))]);
        [rec, group(id.to_le_bytes(), 6, &group(id.to_le_bytes(), 9, refs))].concat()
    };
    let cells = [cell(CELL_A, "CellA", &a_refs), cell(CELL_B, "CellB", &b_refs)].concat();
    let top = |sig: &[u8; 4], body: &[u8]| group(*sig, 0, body);
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

// ---- navmeshes ---------------------------------------------------------------------

/// A flat navmesh over `n` × `n` squares of side 100 from `origin`, two triangles a
/// square, at height `z`.
fn grid(form_id: u32, origin: [f32; 2], n: usize, z: f32) -> NavMeshRecord {
    let vid = |i: usize, j: usize| (j * (n + 1) + i) as u16;
    let mut vertices = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            vertices.push([origin[0] + i as f32 * 100.0, origin[1] + j as f32 * 100.0, z]);
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
    let input = [(&a, Some(CELL_A), Some(A)), (&b, Some(CELL_B), Some(B))];
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
