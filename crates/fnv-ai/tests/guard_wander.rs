//! Guard packages whose location has a radius (#2027): once there, the actor wanders
//! within that radius ("If it has a radius greater than 0, the actor will wander within
//! that radius" [documented: GECK wiki, Guard Package, Guard Location]), as a sandbox
//! that only wanders (`fnv_ai::Sandbox::guard`, `fnv_ai::carry::guard_wander_radius`).
//! A plugin and navmeshes built in memory (no game data): a yard round a post, a yard
//! cut by a wall, and a navmesh too small to wander on. Rules and sources:
//! docs/design/actor-behaviour.md §5.5, docs/spec-notes/packages.md §3.

use fnv_ai::{Activity, Ai, AiConfig, AiEvent, Doing};
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::ai::{AiPhase, SandboxPhase};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::package::type_flags;
use fnv_game::GameData;
use fnv_nav::record::{tri_flags, DoorLink, NavMeshRecord, Triangle};
use fnv_nav::{NavGraph, Space};
use fnv_script::vm::{ArgValue, Value};
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

/// A location or target: (type, form or code, radius or count).
type Loc = (i32, u32, i32);

/// A package without schedule or conditions: type, type-specific flags, location 1 and
/// target.
fn pack(id: u32, edid: &str, ty: u8, specific: u16, loc: Loc, target: Option<Loc>) -> Vec<u8> {
    let three = |l: Loc| [l.0.to_le_bytes(), l.1.to_le_bytes(), l.2.to_le_bytes()].concat();
    let mut subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"PKDT", [&0u32.to_le_bytes()[..], &[ty, 0, 0, 0], &specific.to_le_bytes(), &[0, 0]].concat()),
        sub(b"PLDT", three(loc)),
        sub(b"PSDT", [&[0xFF, 0xFF, 0, 0xFF][..], &0u32.to_le_bytes()].concat()),
    ];
    subs.extend(target.map(|t| sub(b"PTDT", three(t))));
    record(b"PACK", id, &subs)
}

fn npc(fid: u32, edid: &str, packages: &[u32]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    subs.extend(packages.iter().map(|p| sub(b"PKID", p.to_le_bytes().to_vec())));
    record(b"NPC_", fid, &subs)
}

fn placed(sig: &[u8; 4], fid: u32, edid: &str, base: u32, pos: [f32; 3]) -> Vec<u8> {
    let subs = [sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", [pos[0].to_le_bytes(), pos[1].to_le_bytes(), pos[2].to_le_bytes(), [0; 4], [0; 4], [0; 4]].concat())];
    record(sig, fid, &subs)
}

// Package types [documented: xEdit PACK type enum].
const TRAVEL: u8 = 6;
const GUARD: u8 = 14;
// Location types [documented: xEdit PACK PLDT]: near reference 0, near current 2.
const NEAR_REF: i32 = 0;
const NEAR_CURRENT: i32 = 2;
/// "In cell" (location type 1), whose radius the editor greys out.
const IN_CELL: i32 = 1;

const STAT: u32 = 0x810;
const CHAIR: u32 = 0x811;
const BED: u32 = 0x812;
const IDLE_MARKER: u32 = 0x813;

const P_GUARD: u32 = 0x820;
const P_GUARD_ZERO: u32 = 0x821;
const P_GUARD_REMAIN: u32 = 0x822;
const P_TRAVEL_NEAR: u32 = 0x823;
const P_GUARD_WALL: u32 = 0x824;
const P_GUARD_TINY: u32 = 0x825;
const P_GUARD_HERE: u32 = 0x826;
const P_TO_FAR: u32 = 0x827;
const P_GUARD_32: u32 = 0x828;
const P_GUARD_64: u32 = 0x829;
const P_GUARD_IN_CELL: u32 = 0x82A;

const N_GUARD: u32 = 0x840;
const N_ZERO: u32 = 0x841;
const N_REMAIN: u32 = 0x842;
const N_TRAVELLER: u32 = 0x843;
const N_WALL: u32 = 0x844;
const N_TINY: u32 = 0x845;
const N_HERE: u32 = 0x846;
const N_FOE: u32 = 0x847;
const N_SMALL_32: u32 = 0x848;
const N_SMALL_64: u32 = 0x849;
const N_IN_CELL: u32 = 0x84A;

const YARD: u32 = 0x860;
const WALLED: u32 = 0x861;
const TINY: u32 = 0x862;
const SHED: u32 = 0x863;

const MARK_POST: u32 = 0x870;
const MARK_FAR: u32 = 0x871;
const MARK_WALL: u32 = 0x872;
const MARK_TINY: u32 = 0x873;
const R_CHAIR: u32 = 0x874;
const R_BED: u32 = 0x875;
const R_IDLE: u32 = 0x876;

const R_GUARD: u32 = 0x880;
const R_FAR_GUARD: u32 = 0x881;
const R_ZERO: u32 = 0x882;
const R_REMAIN: u32 = 0x883;
const R_TRAVELLER: u32 = 0x884;
const R_WALL: u32 = 0x885;
const R_TINY: u32 = 0x886;
const R_HERE: u32 = 0x887;
const R_FOE: u32 = 0x888;
const R_SMALL_32: u32 = 0x889;
const R_SMALL_64: u32 = 0x88A;
const R_IN_CELL: u32 = 0x88B;
/// The yard's door to the shed (in its far corner) and the shed's back to the yard.
const DOOR_YARD: u32 = 0x8D0;
const DOOR_SHED: u32 = 0x8D1;

const POST: [f32; 3] = [1000.0, 1000.0, 0.0];
const FAR: [f32; 3] = [1800.0, 200.0, 0.0];
/// The location radius of the guard packages round the post.
const RADIUS: f32 = 128.0;
/// The guarded reference's own radius (the target's distance), for intruders: not the
/// area the guard wanders in.
const GUARD_RADIUS: i32 = 600;
const WALL_POST: [f32; 3] = [11050.0, 1000.0, 0.0];
const WALL_RADIUS: f32 = 250.0;
const TINY_POST: [f32; 3] = [20025.0, 20025.0, 0.0];

fn plugin_bytes() -> Vec<u8> {
    let globs = [
        glob(0x801, "GameYear", 2281.0),
        glob(0x802, "GameMonth", 9.0),
        glob(0x803, "GameDay", 19.0),
        // Noon: in a sandbox's lunch window (11 to 14), which a guard's wander ignores.
        glob(0x804, "GameHour", 12.0),
        glob(0x805, "GameDaysPassed", 5.0),
        glob(0x806, "TimeScale", 30.0),
    ]
    .concat();
    let packs = [
        pack(P_GUARD, "GuardThePost", GUARD, 0, (NEAR_REF, MARK_POST, RADIUS as i32), Some((NEAR_REF, MARK_POST, GUARD_RADIUS))),
        pack(P_GUARD_ZERO, "GuardThePostStill", GUARD, 0, (NEAR_REF, MARK_POST, 0), None),
        // Remain Near Reference to Guard, with a location radius of its own.
        pack(P_GUARD_REMAIN, "GuardNearThePost", GUARD, type_flags::REMAIN_NEAR_REFERENCE, (NEAR_CURRENT, 0, 200), Some((NEAR_REF, MARK_POST, 120))),
        pack(P_TRAVEL_NEAR, "TravelNearThePost", TRAVEL, 0, (NEAR_REF, MARK_POST, RADIUS as i32), None),
        pack(P_GUARD_WALL, "GuardTheWall", GUARD, 0, (NEAR_REF, MARK_WALL, WALL_RADIUS as i32), None),
        pack(P_GUARD_TINY, "GuardTheLedge", GUARD, 0, (NEAR_REF, MARK_TINY, RADIUS as i32), None),
        pack(P_GUARD_HERE, "GuardWhereItStands", GUARD, 0, (NEAR_CURRENT, 0, RADIUS as i32), None),
        pack(P_TO_FAR, "TravelFar", TRAVEL, 0, (NEAR_REF, MARK_FAR, 0), None),
        // Small radii, and an "in cell" location with a radius stored all the same.
        pack(P_GUARD_32, "GuardThePostClosely", GUARD, 0, (NEAR_REF, MARK_POST, 32), None),
        pack(P_GUARD_64, "GuardThePostNear", GUARD, 0, (NEAR_REF, MARK_POST, 64), None),
        pack(P_GUARD_IN_CELL, "GuardTheYard", GUARD, 0, (IN_CELL, YARD, 128), None),
    ]
    .concat();
    let npcs = [
        npc(0x7, "Player", &[]),
        npc(N_GUARD, "Guard", &[P_GUARD]),
        npc(N_ZERO, "StillGuard", &[P_GUARD_ZERO]),
        npc(N_REMAIN, "Keeper", &[P_GUARD_REMAIN]),
        npc(N_TRAVELLER, "Traveller", &[P_TRAVEL_NEAR]),
        npc(N_WALL, "WallGuard", &[P_GUARD_WALL]),
        npc(N_TINY, "LedgeGuard", &[P_GUARD_TINY]),
        npc(N_HERE, "Stayer", &[P_GUARD_HERE]),
        npc(N_FOE, "Foe", &[]),
        npc(N_SMALL_32, "CloseGuard", &[P_GUARD_32]),
        npc(N_SMALL_64, "NearGuard", &[P_GUARD_64]),
        npc(N_IN_CELL, "YardGuard", &[P_GUARD_IN_CELL]),
    ]
    .concat();
    // A chair (marker flags bit 30), a bed (bit 31) and an idle marker inside the post's
    // radius: a sandbox would use them, the guard does not.
    let furn = [
        record(b"FURN", CHAIR, &[sub(b"EDID", z("Chair")), sub(b"MNAM", 0x4000_0001u32.to_le_bytes().to_vec())]),
        record(b"FURN", BED, &[sub(b"EDID", z("Bed")), sub(b"MNAM", 0x8000_0001u32.to_le_bytes().to_vec())]),
    ]
    .concat();
    let idlm = record(b"IDLM", IDLE_MARKER, &[sub(b"EDID", z("LeanMarker")), sub(b"IDLF", vec![0]), sub(b"IDLC", vec![0]), sub(b"IDLT", 10f32.to_le_bytes().to_vec())]);
    let stat = record(b"STAT", STAT, &[sub(b"EDID", z("Marker"))]);
    let yard = [
        placed(b"REFR", MARK_POST, "Post", STAT, POST),
        placed(b"REFR", MARK_FAR, "Far", STAT, FAR),
        placed(b"REFR", R_CHAIR, "PostChair", CHAIR, [1060.0, 1000.0, 0.0]),
        placed(b"REFR", R_BED, "PostBed", BED, [1000.0, 940.0, 0.0]),
        placed(b"REFR", R_IDLE, "PostMarker", IDLE_MARKER, [1000.0, 1070.0, 0.0]),
        placed(b"ACHR", R_GUARD, "GuardRef", N_GUARD, [1020.0, 1000.0, 0.0]),
        placed(b"ACHR", R_FAR_GUARD, "FarGuardRef", N_GUARD, [300.0, 300.0, 0.0]),
        placed(b"ACHR", R_ZERO, "StillGuardRef", N_ZERO, [300.0, 1700.0, 0.0]),
        placed(b"ACHR", R_REMAIN, "KeeperRef", N_REMAIN, [1700.0, 300.0, 0.0]),
        placed(b"ACHR", R_TRAVELLER, "TravellerRef", N_TRAVELLER, [1700.0, 1700.0, 0.0]),
        placed(b"ACHR", R_HERE, "StayerRef", N_HERE, [500.0, 1500.0, 0.0]),
        placed(b"ACHR", R_FOE, "FoeRef", N_FOE, [1600.0, 1000.0, 0.0]),
        placed(b"ACHR", R_SMALL_32, "CloseGuardRef", N_SMALL_32, POST),
        placed(b"ACHR", R_SMALL_64, "NearGuardRef", N_SMALL_64, POST),
        placed(b"ACHR", R_IN_CELL, "YardGuardRef", N_IN_CELL, [400.0, 1000.0, 0.0]),
        // The door the graph's door link names, placed so the game knows it.
        placed(b"REFR", DOOR_YARD, "YardDoor", STAT, [1950.0, 1950.0, 0.0]),
    ]
    .concat();
    let walled = [placed(b"REFR", MARK_WALL, "WallPost", STAT, WALL_POST), placed(b"ACHR", R_WALL, "WallGuardRef", N_WALL, [10950.0, 1000.0, 0.0])].concat();
    let tiny = [placed(b"REFR", MARK_TINY, "LedgePost", STAT, TINY_POST), placed(b"ACHR", R_TINY, "LedgeGuardRef", N_TINY, TINY_POST)].concat();
    let cell = |id: u32, edid: &str, refs: &[u8]| [record(b"CELL", id, &[sub(b"EDID", z(edid))]), group(id.to_le_bytes(), 6, &group(id.to_le_bytes(), 9, refs))].concat();
    let cells = [cell(YARD, "Yard", &yard), cell(WALLED, "WalledYard", &walled), cell(TINY, "Ledge", &tiny), cell(SHED, "Shed", &[])].concat();
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

/// A flat navmesh of `n` × `n` squares of side `size` from `origin`, two triangles a
/// square; the squares `wall` names are deleted (no way across them).
fn grid(form_id: u32, origin: [f32; 2], n: usize, size: f32, wall: impl Fn(usize, usize) -> bool) -> NavMeshRecord {
    let vid = |i: usize, j: usize| (j * (n + 1) + i) as u16;
    let mut vertices = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            vertices.push([origin[0] + i as f32 * size, origin[1] + j as f32 * size, 0.0]);
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
            let flags = if wall(i, j) { tri_flags::DELETED } else { 0 };
            triangles.push(Triangle { v: [a, b, c], edges: [tri(ii, jj - 1, 1), tri(ii + 1, jj, 1), tri(ii, jj, 1)], flags, cover: 0 });
            triangles.push(Triangle { v: [a, c, d], edges: [tri(ii, jj, 0), tri(ii, jj + 1, 0), tri(ii - 1, jj, 0)], flags, cover: 0 });
        }
    }
    NavMeshRecord { form_id, vertices, triangles, ..Default::default() }
}

/// The yard: 20 × 20 squares of 100 at the origin. The walled yard: the same at
/// (10000, 0), with a wall one square thick at x 11000 to 11100 from y 0 to 1400, so
/// its post's circle (radius 250 round (11050, 1000)) is cut in two halves joined only
/// round the wall's end, outside the circle. The ledge: one square of 50. The shed: 5 × 5
/// squares of 100 at (30000, 0), a door from the yard's far corner leading to its near
/// corner and back.
fn nav() -> NavGraph {
    let mut yard = grid(1, [0.0, 0.0], 20, 100.0, |_, _| false);
    let walled = grid(2, [10000.0, 0.0], 20, 100.0, |i, j| i == 10 && j < 14);
    let ledge = grid(3, [20000.0, 20000.0], 1, 50.0, |_, _| false);
    let mut shed = grid(4, [30000.0, 0.0], 5, 100.0, |_, _| false);
    let corner = yard.triangles.len() - 2;
    yard.door_links.push(DoorLink { door: DOOR_YARD, triangle: corner as u16 });
    yard.triangles[corner].flags |= tri_flags::DOOR;
    shed.door_links.push(DoorLink { door: DOOR_SHED, triangle: 0 });
    let input = [
        (&yard, Some(YARD), Some(Space::Interior(YARD))),
        (&walled, Some(WALLED), Some(Space::Interior(WALLED))),
        (&ledge, Some(TINY), Some(Space::Interior(TINY))),
        (&shed, Some(SHED), Some(Space::Interior(SHED))),
    ];
    let mut g = NavGraph::assemble(&input);
    g.join_doors(&input, |door| match door {
        DOOR_YARD => Some((DOOR_SHED, [30050.0, 30.0, 0.0], 0.5, Space::Interior(SHED))),
        DOOR_SHED => Some((DOOR_YARD, [1940.0, 1940.0, 0.0], 3.0, Space::Interior(YARD))),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "GuardWander.esm".into()).unwrap()];
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

    /// A script function called on `actor` (GetCurrentAIProcedure, AddScriptPackage).
    fn call(&mut self, actor: u32, opcode: u16, args: &[ArgValue]) -> f64 {
        let ctx = Ctx { unit: "test".into(), opcode, on: Some(FormId(actor)), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
        match self.game.call_function(&ctx, args) {
            Value::Num(x) => x,
            Value::Ref(f) => f as f64,
        }
    }

    /// GetCurrentAIProcedure [documented: GECK wiki; opcode 0x108F, schemas/script/functions.json].
    fn procedure(&mut self, actor: u32) -> f64 {
        self.call(actor, 0x108F, &[])
    }

    /// Whether the actor stands at the end of a walk of its own.
    fn stopped(&self, actor: u32) -> bool {
        let a = &self.ai.agents[&FormId(actor)];
        a.activity == Activity::There && a.walker.follower.is_none() && a.sandbox.is_some()
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn doings(s: &Sim, actor: u32) -> Vec<Doing> {
    s.events(actor)
        .iter()
        .filter_map(|e| match e {
            AiEvent::Sandbox { doing, .. } => Some(*doing),
            _ => None,
        })
        .collect()
}

/// GUARD, the procedure of a Guard package [observed: AI observer samples,
/// `fnv_game::functions` `procedure_code`; the issue's capture read 44 throughout].
const GUARD_PROCEDURE: f64 = 44.0;

// ---- tests -------------------------------------------------------------------------

#[test]
fn the_fixture_reads_without_issues() {
    let f = fixture();
    assert!(f.data.issues.is_empty(), "{:?}", f.data.issues.to_json());
    let radius = |p: u32| fnv_ai::carry::guard_wander_radius(&f.data.package_defs[&FormId(p)]);
    assert_eq!(radius(P_GUARD), Some(RADIUS), "the location's radius, not the guarded reference's {GUARD_RADIUS}");
    assert_eq!(radius(P_GUARD_ZERO), None);
    assert_eq!(radius(P_GUARD_REMAIN), None, "Remain Near Reference to Guard is left as it was");
    assert_eq!(radius(P_TRAVEL_NEAR), None, "a Travel package is no guard");
    assert_eq!(radius(P_GUARD_IN_CELL), None, "an \"in cell\" location has no radius, whatever is stored");
    assert_eq!((radius(P_GUARD_32), radius(P_GUARD_64)), (Some(32.0), Some(64.0)));
}

/// A guard at its post keeps walking about inside the location's radius (128), never
/// out to the guarded reference's 600: positions measured every half second, not only
/// the spots chosen. Nothing in the area is used (chair, bed, idle marker, a meal at
/// noon), and GetCurrentAIProcedure reads GUARD all along.
#[test]
fn a_guard_with_a_location_radius_wanders_inside_it() {
    let f = fixture();
    let mut s = sim(&f, &[R_GUARD]);
    let mut farthest = 0.0f32;
    let mut procedures = std::collections::BTreeSet::new();
    let mut moving = 0;
    for _ in 0..480 {
        s.run(0.5);
        farthest = farthest.max(dist(s.feet(R_GUARD), POST));
        procedures.insert(s.procedure(R_GUARD) as i64);
        if s.ai.agents[&FormId(R_GUARD)].activity == Activity::Travelling {
            moving += 1;
        }
    }
    let did = doings(&s, R_GUARD);
    assert!(did.iter().filter(|d| **d == Doing::Wandering).count() >= 10, "{did:?}");
    assert!(did.iter().all(|d| matches!(d, Doing::Wandering | Doing::Standing)), "{did:?}");
    let a = &s.ai.agents[&FormId(R_GUARD)];
    assert!(a.walker.travelled > 600.0, "walked {}", a.walker.travelled);
    assert!(moving > 10, "moving in {moving} of 480 samples");
    assert!(farthest <= RADIUS + 1.0, "went {farthest} from the post");
    assert_eq!(a.using, None);
    assert!(!s.events(R_GUARD).iter().any(|e| matches!(e, AiEvent::Furniture { furniture: Some(_) })), "{:?}", s.events(R_GUARD));
    let sb = a.sandbox.as_ref().expect("the guard's wander");
    let allowed = sb.allowed;
    assert!(allowed.wandering && !allowed.furniture && !allowed.idle_markers && !allowed.eating && !allowed.sleeping && !allowed.conversation);
    assert_eq!(sb.area(), (fnv_nav::Place { space: Space::Interior(YARD), pos: POST }, RADIUS));
    assert_eq!(procedures.into_iter().collect::<Vec<_>>(), vec![GUARD_PROCEDURE as i64]);
    // The package stays the guard's; its type reads GUARD (35) [observed: the issue's capture].
    assert_eq!(s.call(R_GUARD, 0x106E, &[]), 35.0);
    assert!(matches!(s.game.extra[&FormId(R_GUARD)].ai_phase, Some(AiPhase::Sandbox(SandboxPhase::Wandering | SandboxPhase::Standing))));
}

/// A guard placed away from its post walks there (arriving at the radius's edge, as a
/// Travel would), then goes on moving about inside the radius.
#[test]
fn a_guard_that_starts_outside_walks_in_then_wanders() {
    let f = fixture();
    let mut s = sim(&f, &[R_FAR_GUARD]);
    s.run(30.0);
    assert!(s.events(R_FAR_GUARD).iter().any(|e| matches!(e, AiEvent::Arrived { .. })), "{:?}", s.events(R_FAR_GUARD));
    assert!(dist(s.feet(R_FAR_GUARD), POST) <= RADIUS + 1.0, "at {:?}", s.feet(R_FAR_GUARD));
    let arrived_walk = s.ai.agents[&FormId(R_FAR_GUARD)].walker.travelled;
    let mut farthest = 0.0f32;
    for _ in 0..240 {
        s.run(0.5);
        farthest = farthest.max(dist(s.feet(R_FAR_GUARD), POST));
    }
    let walked = s.ai.agents[&FormId(R_FAR_GUARD)].walker.travelled - arrived_walk;
    assert!(walked > 200.0, "walked {walked} after arriving");
    assert!(farthest <= RADIUS + 1.0, "went {farthest} from the post");
    assert!(doings(&s, R_FAR_GUARD).iter().filter(|d| **d == Doing::Wandering).count() >= 4);
}

/// A radius of 0 keeps the guard where it arrived, as before (#2027 changes only a
/// radius over 0); so does a Travel package with a radius, and a Guard that remains near
/// the reference it guards, whose place is that reference's.
#[test]
fn a_guard_without_a_radius_and_the_other_stays_stand_still() {
    let f = fixture();
    // One at a time: three standing round one post would push each other aside. The
    // keeper stays within its location's radius (200) of the guarded post (`goal::resolve`).
    for (actor, reach) in [(R_ZERO, 33.0), (R_TRAVELLER, RADIUS + 1.0), (R_REMAIN, 201.0)] {
        let mut s = sim(&f, &[actor]);
        s.run(60.0);
        let here = s.feet(actor);
        assert!(dist(here, POST) <= reach, "{actor:X} at {here:?}");
        s.run(120.0);
        assert!(s.ai.agents[&FormId(actor)].sandbox.is_none(), "{actor:X}");
        assert!(doings(&s, actor).is_empty(), "{actor:X}: {:?}", doings(&s, actor));
        assert!(dist(s.feet(actor), here) < 1.0, "{actor:X} moved from {here:?} to {:?}", s.feet(actor));
        if actor != R_TRAVELLER {
            assert_eq!(s.procedure(actor), GUARD_PROCEDURE);
        }
    }
}

/// "Near current location" with a radius: the guard wanders round where it settled.
#[test]
fn a_guard_near_its_current_location_wanders_round_where_it_stood() {
    let f = fixture();
    let mut s = sim(&f, &[R_HERE]);
    let start = s.feet(R_HERE);
    let mut farthest = 0.0f32;
    for _ in 0..240 {
        s.run(0.5);
        farthest = farthest.max(dist(s.feet(R_HERE), start));
    }
    assert!(s.ai.agents[&FormId(R_HERE)].walker.travelled > 200.0);
    assert!(farthest <= RADIUS + 1.0, "went {farthest} from where it stood");
}

/// A navmesh with no spot to go to (one square of 50, every point of it within 64 of
/// the guard): it stands, and asks again now and then, still guarding.
#[test]
fn a_guard_with_nowhere_to_wander_stands() {
    let f = fixture();
    let mut s = sim(&f, &[R_TINY]);
    s.run(60.0);
    let did = doings(&s, R_TINY);
    assert!(did.len() >= 3, "{did:?}");
    assert!(did.iter().all(|d| *d == Doing::Standing), "{did:?}");
    assert!(dist(s.feet(R_TINY), TINY_POST) < 1.0);
    assert_eq!(s.procedure(R_TINY), GUARD_PROCEDURE);
}

/// Another package replaces the guard's: the wander ends with it and the new package is
/// carried out; when that one is done (a script's Travel, dropped at its goal), the
/// guard walks back and wanders again.
#[test]
fn a_new_package_ends_the_guards_wander() {
    let f = fixture();
    let mut s = sim(&f, &[R_GUARD]);
    s.run(30.0);
    assert!(s.ai.agents[&FormId(R_GUARD)].sandbox.is_some());
    s.call(R_GUARD, 0x1097, &[ArgValue::Value(Value::Ref(P_TO_FAR))]);
    s.run(2.0);
    let a = &s.ai.agents[&FormId(R_GUARD)];
    assert!(a.sandbox.is_none());
    assert_eq!(a.activity, Activity::Travelling);
    assert_eq!(s.game.current_package(FormId(R_GUARD)), Some(FormId(P_TO_FAR)));
    s.run(20.0);
    assert!(s.events(R_GUARD).iter().any(|e| matches!(e, AiEvent::Arrived { package, .. } if package.0 == P_TO_FAR)), "{:?}", s.events(R_GUARD));
    let wanders = doings(&s, R_GUARD).len();
    s.run(60.0);
    assert_eq!(s.game.current_package(FormId(R_GUARD)), Some(FormId(P_GUARD)));
    assert!(doings(&s, R_GUARD).len() > wanders, "wanders again: {:?}", s.events(R_GUARD));
    assert!(dist(s.feet(R_GUARD), POST) <= RADIUS + 1.0);
}

/// Combat takes the wander away (`Ai::interrupt_sandbox`) and its end gives it back, from
/// wherever the fight left the guard: back inside the radius, and moving about there,
/// without the package being taken up again.
#[test]
fn combat_interrupts_the_wander_and_it_resumes_afterwards() {
    let f = fixture();
    let mut s = sim(&f, &[R_GUARD]);
    s.run(30.0);
    let packages = s.events(R_GUARD).iter().filter(|e| matches!(e, AiEvent::Package { .. })).count();
    s.game.start_combat(FormId(R_GUARD), FormId(R_FOE));
    s.run(10.0);
    let sb = s.ai.agents[&FormId(R_GUARD)].sandbox.as_ref().expect("kept through the fight");
    assert_eq!(sb.doing, Doing::Standing, "no wander in combat");
    s.game.stop_combat(FormId(R_GUARD));
    let before = doings(&s, R_GUARD).len();
    let mut back = None;
    for i in 0..240 {
        s.run(0.5);
        let d = dist(s.feet(R_GUARD), POST);
        match back {
            None if d <= RADIUS + 1.0 && s.stopped(R_GUARD) => back = Some(i),
            Some(_) => assert!(d <= RADIUS + 1.0, "out again at {d}"),
            None => {}
        }
    }
    assert!(back.is_some(), "never back inside: at {:?}", s.feet(R_GUARD));
    assert!(doings(&s, R_GUARD)[before..].iter().filter(|d| **d == Doing::Wandering).count() >= 3, "{:?}", &doings(&s, R_GUARD)[before..]);
    assert_eq!(s.events(R_GUARD).iter().filter(|e| matches!(e, AiEvent::Package { .. })).count(), packages, "no package replacement");
}

/// A wall across the circle: every spot is inside the radius and every walk ends there,
/// but the way from one half to the other goes round the wall's end, outside the circle.
/// Spots bound where the guard stops, not the way between [design, #2027: paths are
/// not kept inside the radius].
#[test]
fn round_a_wall_the_walks_end_inside_though_the_way_leaves_the_circle() {
    let f = fixture();
    let mut s = sim(&f, &[R_WALL]);
    let (mut farthest_stop, mut farthest_walking) = (0.0f32, 0.0f32);
    let (mut left, mut right) = (0, 0);
    for _ in 0..1200 {
        s.run(0.5);
        let at = s.feet(R_WALL);
        let d = dist(at, WALL_POST);
        if s.stopped(R_WALL) {
            farthest_stop = farthest_stop.max(d);
            if at[0] < 11000.0 {
                left += 1;
            } else {
                right += 1;
            }
        } else {
            farthest_walking = farthest_walking.max(d);
        }
    }
    assert!(farthest_stop <= WALL_RADIUS + 1.0, "stopped {farthest_stop} from the post");
    assert!(left > 0 && right > 0, "stops on both sides: {left} left, {right} right");
    assert!(farthest_walking > WALL_RADIUS, "the way round the wall: {farthest_walking}");
}

/// A small radius still wanders (#2027 review): a guard standing on its post with a
/// location radius of 32 or 64 finds spots nearer than the 64 units an ordinary sandbox
/// keeps between spots, and walks to them.
#[test]
fn a_guard_with_a_small_radius_still_wanders() {
    let f = fixture();
    for (actor, radius) in [(R_SMALL_32, 32.0f32), (R_SMALL_64, 64.0)] {
        let mut s = sim(&f, &[actor]);
        let mut farthest = 0.0f32;
        for _ in 0..240 {
            s.run(0.5);
            farthest = farthest.max(dist(s.feet(actor), POST));
        }
        let did = doings(&s, actor);
        assert!(did.iter().filter(|d| **d == Doing::Wandering).count() >= 6, "radius {radius}: {did:?}");
        let walked = s.ai.agents[&FormId(actor)].walker.travelled;
        assert!(walked > radius * 3.0, "radius {radius}: walked {walked}");
        assert!(farthest <= radius + 1.0, "radius {radius}: went {farthest} from the post");
    }
}

/// An "in cell" location's stored radius is not a wander radius (the editor greys it out
/// [documented: GECK wiki, Category:Packages, Standard Location]): the guard stays where
/// it arrived in the cell, as before.
#[test]
fn an_in_cell_guard_ignores_its_stored_radius() {
    let f = fixture();
    let mut s = sim(&f, &[R_IN_CELL]);
    s.run(30.0);
    let here = s.feet(R_IN_CELL);
    s.run(120.0);
    assert!(s.ai.agents[&FormId(R_IN_CELL)].sandbox.is_none());
    assert!(doings(&s, R_IN_CELL).is_empty(), "{:?}", doings(&s, R_IN_CELL));
    assert!(dist(s.feet(R_IN_CELL), here) < 1.0, "moved from {here:?} to {:?}", s.feet(R_IN_CELL));
}

/// A guard that fled through a door (#2027 review): its wander is round the post in the
/// yard, so in the shed it neither wanders nor stands for ever; when the fight is over it
/// goes back through the door to its post and wanders there again.
#[test]
fn a_guard_that_fled_through_a_door_goes_back_to_its_post() {
    use fnv_game::awareness::{State, Timers};
    use fnv_game::combat_group::{FleeBy, Flight};
    use fnv_game::detection::{Band, Detection};
    let f = fixture();
    let mut s = sim(&f, &[R_GUARD]);
    let guard = FormId(R_GUARD);
    s.run(30.0);
    assert!(s.ai.agents[&guard].sandbox.is_some());
    // In combat with the player at the yard's near corner; the door, in the far corner,
    // is the way away from it.
    let threat = [0.0, 0.0, 0.0];
    let (now, t) = (s.game.time, Timers::default());
    let a = s.game.awareness.actors.entry(guard).or_default();
    a.observe(FormId::PLAYER_REF, &Detection { value: 5.0, band: Band::Detected, ..Default::default() }, threat, now, &t);
    a.settle(guard, now, &t, true);
    assert_eq!(a.state, State::Combat);
    a.combat_began = Some(now - fnv_ai::alert::COMBAT_REACTION_SECONDS - 1.0);
    s.game.awareness.tactics.flights.insert(guard, Flight { from: FormId::PLAYER_REF, from_at: threat, since: now, spot: None, arrived: None });
    s.run(15.0);
    assert!(s.events(R_GUARD).iter().any(|e| matches!(e, AiEvent::Flee { how: FleeBy::Door { door }, .. } if door.0 == DOOR_YARD)), "{:?}", s.events(R_GUARD));
    assert_eq!(s.ai.agents[&guard].walker.space, Space::Interior(SHED), "fled into the shed: {:?}", s.events(R_GUARD));
    // The fight is over.
    s.game.awareness.actors.remove(&guard);
    s.game.awareness.tactics.flights.remove(&guard);
    let before = doings(&s, R_GUARD).len();
    s.run(60.0);
    let ev = s.events(R_GUARD);
    assert!(ev.iter().any(|e| matches!(e, AiEvent::SearchBack { .. })), "{ev:?}");
    assert_eq!(s.ai.agents[&guard].walker.space, Space::Interior(YARD), "back in the yard: {ev:?}");
    let mut farthest = 0.0f32;
    for _ in 0..120 {
        s.run(0.5);
        farthest = farthest.max(dist(s.feet(R_GUARD), POST));
    }
    assert!(farthest <= RADIUS + 1.0, "went {farthest} from the post");
    assert!(doings(&s, R_GUARD)[before..].iter().filter(|d| **d == Doing::Wandering).count() >= 3, "{:?}", &doings(&s, R_GUARD)[before..]);
    assert_eq!(s.ai.agents[&guard].sandbox.as_ref().map(|sb| sb.area().0.space), Some(Space::Interior(YARD)));
}
