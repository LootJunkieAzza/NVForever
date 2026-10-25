//! Kill cameras' choice of camera path on a synthetic plugin (no game data): the tree's
//! order, GetVATSValue answering about the landed shot, the VATS area functions and the
//! fallbacks. docs/design/vats-mode.md §19, `fnv_game::camera_paths`.

mod common;

use common::*;
use fnv_esm::raw::Subrecord;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::camera_paths::{hit_shots, ShotAction};
use fnv_game::game::{Game, GameConfig};
use fnv_game::vats::{CameraRoom, VatsOutcome, VatsShot};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const NPC: u32 = 0x400;
const NPC_REF: u32 = 0x500;
const GUN: u32 = 0x600;
// Paths: MISS (Is Success == 0) > MISS_CAM; FATAL (Is Fatal == 1) > RIGHT (right room
// ≥ 60), PLAIN; HITS (no conditions, no shots) > EMPTY (never), then FALLBACK.
const MISS: u32 = 0x700;
const MISS_CAM: u32 = 0x701;
const FATAL: u32 = 0x702;
const RIGHT: u32 = 0x703;
const PLAIN: u32 = 0x704;
const HITS: u32 = 0x705;
const EMPTY: u32 = 0x706;
const FALLBACK: u32 = 0x707;
const SHOOT: u32 = 0x800;
const HIT: u32 = 0x801;
const HIT2: u32 = 0x802;

const GET_VATS_VALUE: u16 = 0x198;
const GET_RANDOM_PERCENT: u16 = 77;
const RIGHT_AREA_FREE: u16 = 518;
const EQ: u8 = 0x00;
const GE: u8 = 0x60;
const LT: u8 = 0x80;

fn ctda(op: u8, value: f32, function: u16, p1: u32, p2: u32) -> Subrecord {
    let mut d = vec![op, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(p1.to_le_bytes());
    d.extend(p2.to_le_bytes());
    d.extend([0; 8]);
    sub(b"CTDA", d)
}

fn path(id: u32, edid: &str, parent: u32, previous: u32, shots: &[u32], conds: Vec<Subrecord>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    subs.extend(conds);
    subs.push(sub(b"ANAM", u32s(&[parent, previous])));
    subs.push(sub(b"DATA", vec![1]));
    subs.extend(shots.iter().map(|s| sub(b"SNAM", s.to_le_bytes().to_vec())));
    record(b"CPTH", 0, id, &subs)
}

fn shot(id: u32, edid: &str, action: u32) -> Vec<u8> {
    let mut data = Vec::new();
    for v in [action, 2, 2, 7] {
        data.extend(v.to_le_bytes());
    }
    for v in [4.0f32, 3.0, 0.25, 3.0, 1.0, 0.0] {
        data.extend(v.to_le_bytes());
    }
    record(b"CAMS", 0, id, &[sub(b"EDID", z(edid)), sub(b"MODL", z("vatscameras\\test.nif")), sub(b"DATA", data)])
}

fn plugin_bytes() -> Vec<u8> {
    // Stored out of order: the tree comes from the links, not the file.
    let paths = [
        path(FALLBACK, "Fallback", 0, HITS, &[SHOOT], vec![]),
        path(MISS, "Miss", 0, 0, &[], vec![ctda(EQ, 0.0, GET_VATS_VALUE, 7, 0)]),
        path(MISS_CAM, "MissCam", MISS, 0, &[SHOOT], vec![]),
        path(PLAIN, "Plain", FATAL, RIGHT, &[SHOOT, HIT, HIT2], vec![]),
        path(RIGHT, "Right", FATAL, 0, &[HIT], vec![ctda(GE, 60.0, RIGHT_AREA_FREE, 0, 0)]),
        path(FATAL, "Fatal", 0, MISS, &[], vec![ctda(EQ, 1.0, GET_VATS_VALUE, 11, 0)]),
        path(HITS, "Hits", 0, FATAL, &[], vec![]),
        path(EMPTY, "Empty", HITS, 0, &[HIT], vec![ctda(LT, 0.0, GET_RANDOM_PERCENT, 0, 0)]),
    ]
    .concat();
    let shots = [shot(SHOOT, "Shoot", 0), shot(HIT, "Hit", 2), shot(HIT2, "Hit2", 2)].concat();
    let npcs = [record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), record(b"NPC_", 0, NPC, &[sub(b"EDID", z("Target"))])].concat();
    let weapons = record(b"WEAP", 0, GUN, &[sub(b"EDID", z("Gun"))]);
    let achr = record(b"ACHR", 0, NPC_REF, &[sub(b"NAME", NPC.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &achr));
    [
        tes4(),
        top(b"NPC_", &npcs),
        top(b"WEAP", &weapons),
        top(b"CAMS", &shots),
        top(b"CPTH", &paths),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "CameraPaths.esm".into()).unwrap()];
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
    g.new_game();
    g.player_entered(Some(FormId(CELL)));
    g
}

fn shot_of(success: bool) -> VatsShot {
    VatsShot { weapon: FormId(GUN), target: FormId(NPC_REF), distance: 400.0, condition: None, success, height: 0.0 }
}

fn room(right: f32) -> CameraRoom {
    CameraRoom { area_free: [right, 0.0, 0.0, 0.0], target_visible: [0.0; 4] }
}

#[test]
fn the_records_load_with_their_links_and_shots() {
    let f = fixture();
    assert_eq!(f.data.camera_paths.len(), 8);
    assert_eq!(f.data.camera_shots.len(), 3);
    let plain = &f.data.camera_paths[&FormId(PLAIN)];
    assert_eq!((plain.parent, plain.previous), (Some(FormId(FATAL)), Some(FormId(RIGHT))));
    assert_eq!(plain.shots, vec![FormId(SHOOT), FormId(HIT), FormId(HIT2)]);
    assert_eq!(f.data.camera_paths[&FormId(RIGHT)].conditions.len(), 1);
    assert_eq!(f.data.camera_shots[&FormId(HIT)].action, ShotAction::Hit);
    // From the hit, a kill camera plays the hit shots.
    assert_eq!(hit_shots(plain, &f.data.camera_shots), vec![FormId(HIT), FormId(HIT2)]);
}

#[test]
fn the_tree_chooses_by_the_landed_shot_and_the_room() {
    let f = fixture();
    let mut g = game(&f);
    let fatal = VatsOutcome { fatal: true, ..Default::default() };
    // A miss: the miss path's child.
    assert_eq!(g.pick_camera_path(shot_of(false), fatal, room(500.0)), Some(FormId(MISS_CAM)));
    // A kill with room on the right, then without: the first passing child.
    assert_eq!(g.pick_camera_path(shot_of(true), fatal, room(500.0)), Some(FormId(RIGHT)));
    assert_eq!(g.pick_camera_path(shot_of(true), fatal, room(10.0)), Some(FormId(PLAIN)));
    // Not a kill: Hits passes but gives nothing (no shots, its child never passes), so
    // the next root.
    assert_eq!(g.pick_camera_path(shot_of(true), VatsOutcome::default(), room(500.0)), Some(FormId(FALLBACK)));
    // The VATS state is left as it was.
    assert_eq!(g.vats.shot, None);
    assert_eq!(g.vats.outcome, None);
}

/// The masters' own tree (needs `FNVLAB_DATA_DIR`; skipped and passing without it): a
/// landed killing shot of the 10mm pistol picks a path under `DefaultHits`, a miss one
/// under `DefaultMissCams` [observed: FalloutNV.esm's CPTH tree].
#[test]
fn the_masters_tree_picks_hit_and_miss_cameras() {
    let Some(dir) = std::env::var_os("FNVLAB_DATA_DIR").map(std::path::PathBuf::from) else {
        eprintln!("skipped: FNVLAB_DATA_DIR is not set (no game data)");
        return;
    };
    if !dir.join("FalloutNV.esm").is_file() {
        eprintln!("skipped: no FalloutNV.esm in {}", dir.display());
        return;
    }
    let plugins = vec![Plugin::open(dir.join("FalloutNV.esm")).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    // 276 shots and 418 paths [observed: docs/spec-notes/default-objects.md §3].
    assert_eq!((data.camera_shots.len(), data.camera_paths.len()), (276, 418));
    let read: usize = data.camera_paths.values().map(|p| p.conditions.len()).sum();
    assert!(read > 500, "{read} conditions read");
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());
    g.new_game();
    let under = |g: &Game, mut p: FormId, root: u32| loop {
        if p == FormId(root) {
            return true;
        }
        match g.data.camera_paths.get(&p).and_then(|d| d.parent) {
            Some(q) => p = q,
            None => return false,
        }
    };
    // The player stands in for the target: who it is matters only to Target Is.
    let shot = |success| VatsShot { weapon: FormId(0x0000_434F), target: FormId::PLAYER_REF, distance: 600.0, condition: Some(26), success, height: 0.0 };
    let open = CameraRoom { area_free: [500.0; 4], target_visible: [500.0; 4] };
    let fatal = VatsOutcome { fatal: true, ..Default::default() };
    for _ in 0..20 {
        let hit = g.pick_camera_path(shot(true), fatal, open).expect("a path for a kill");
        assert!(under(&g, hit, 0x0004_0C22), "{hit} {}", g.data.name(hit));
        assert!(!hit_shots(&g.data.camera_paths[&hit], &g.data.camera_shots).is_empty());
        let miss = g.pick_camera_path(shot(false), VatsOutcome::default(), open).expect("a path for a miss");
        assert!(under(&g, miss, 0x0001_864C), "{miss} {}", g.data.name(miss));
    }
}
