//! Quest markers' ways through doors (`fnv_game::quest_targets::DoorGraph`) against the
//! game's own master (#2458; docs/spec-notes/quests-dialogue.md §2). They need the game
//! data, which is never in the repository: set `FNVLAB_DATA_DIR` to the game's `Data`
//! folder. Without it each test prints why it was skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::quest_targets::{outdoor_marker_door, DoorGraph, Place};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;
use std::path::{Path, PathBuf};

fn data_dir() -> Option<PathBuf> {
    let Some(dir) = std::env::var_os("FNVLAB_DATA_DIR").map(PathBuf::from) else {
        eprintln!("skipped: FNVLAB_DATA_DIR is not set (no game data)");
        return None;
    };
    if !dir.join("FalloutNV.esm").is_file() {
        eprintln!("skipped: FalloutNV.esm is not in {}", dir.display());
        return None;
    }
    Some(dir)
}

struct Loaded {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn load(dir: &Path) -> Loaded {
    let plugins = vec![Plugin::open(dir.join("FalloutNV.esm")).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_dir(root.join("schemas/fnv")).unwrap();
    let fp = root.join("schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&fp).unwrap()).unwrap();
    let table = FunctionTable::load(&fp).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Loaded { data, forms, refs }
}

/// The worldspace of the exterior cell `cell` (an Editor ID).
fn world_of_cell(refs: &RefTable, cell: &str) -> Place {
    let c = refs.cell_by_edid(cell).unwrap_or_else(|| panic!("cell {cell}"));
    Place::World(refs.world_of(c).unwrap_or_else(|| panic!("{cell} is not exterior")))
}

/// The doors outdoors straight into the interior `cell` (as `Game::doors_into`).
fn doors_into(refs: &RefTable, cell: FormId) -> Vec<FormId> {
    refs.refs
        .iter()
        .filter(|(_, d)| d.teleport.is_some_and(|t| refs.get(t.door).is_some_and(|o| o.cell == Some(cell))) && d.cell.is_some_and(|c| refs.cell_world.contains_key(&c)))
        .map(|(id, _)| *id)
        .collect()
}

/// For the Republic, Part 2 (VMQ05) objective 10 marks Colonel Moore, in an interior of
/// Hoover Dam's power plant that no door outdoors leads into: the World Map had no place
/// for her (#2458). Now: a door of the dam outdoors in the Mojave; from the Strip the
/// compass's door is the Strip's gate to Freeside, its one way to another worldspace.
#[test]
fn colonel_moore_is_marked_outdoors_at_the_dam_and_from_the_strip_at_its_gate() {
    let Some(dir) = data_dir() else { return };
    let l = load(&dir);
    let refs = &l.refs;
    let world = World::new(&l.forms, refs, SimConfig::default());
    let graph = DoorGraph::build(refs);
    let moore_cell = refs.get(refs.by_edid("VHDColonelMooreREF").expect("Moore")).and_then(|m| m.cell).expect("her cell");
    assert_eq!(Some(moore_cell), refs.cell_by_edid("HooverDamIntPowerPlant01b"));
    assert!(doors_into(refs, moore_cell).is_empty(), "no door outdoors leads straight in");
    let door = outdoor_marker_door(&world, &graph, &[], moore_cell, false).expect("a door outdoors on the way to her");
    let d = refs.get(door).unwrap();
    let mojave = world_of_cell(refs, "FreesideExtEastGate");
    assert_eq!(d.cell.map(|c| Place::of_cell(refs, c)), Some(mojave), "the door stands in the Mojave");
    // On the dam [observed: its doors outdoors stand in x 66000..78000, y 15000..30000].
    assert!((60_000.0..80_000.0).contains(&d.pos[0]) && (10_000.0..32_000.0).contains(&d.pos[1]), "{door} at {:?}", d.pos);
    let there = Place::Interior(moore_cell);
    let strip = world_of_cell(refs, "StripFSGate");
    let gate = refs.by_edid("FreesideGateToStripREF").and_then(|g| refs.get(g)).and_then(|g| g.teleport).map(|t| t.door);
    assert_eq!(graph.first_door(&world, strip, there, false), gate, "from the Strip: its gate to Freeside");
    let path = graph.path(&world, strip, there, false).expect("a way from the Strip");
    assert!(path.contains(&door), "the way from the Strip goes in at the World Map's door: {path:?}");
}

/// Every quest target indoors in the master: how many a door outdoors leads straight to,
/// how many only a way through other interiors does (marked nowhere on the World Map
/// before #2458); a target left without a door outdoors has no way out at all.
#[test]
fn quest_targets_indoors_get_a_door_outdoors() {
    let Some(dir) = data_dir() else { return };
    let l = load(&dir);
    let refs = &l.refs;
    let world = World::new(&l.forms, refs, SimConfig::default());
    let graph = DoorGraph::build(refs);
    let (mut direct, mut deep, mut none) = (0, 0, Vec::new());
    for def in l.data.quests.values() {
        for t in def.objectives.values().flat_map(|o| o.targets.iter()) {
            let Some(cell) = t.target.and_then(|r| refs.get(r)).and_then(|p| p.cell) else { continue };
            if refs.cell_world.contains_key(&cell) {
                continue;
            }
            let straight = doors_into(refs, cell);
            match outdoor_marker_door(&world, &graph, &straight, cell, t.ignores_locks) {
                Some(_) if !straight.is_empty() => direct += 1,
                Some(_) => deep += 1,
                None => none.push(cell),
            }
        }
    }
    none.sort();
    none.dedup();
    eprintln!("quest targets indoors: {direct} with a door straight in, {deep} only through other interiors, {} cells with none: {none:?}", none.len());
    assert!(deep > 0, "Moore and the like");
    for c in none {
        assert!(graph.outdoor_door(&world, Place::Interior(c), true).is_none());
    }
}
