//! Quest markers and disabled load doors, against the game's own master (issue #2566):
//! Nellis's hangar has two versions, NellisHangar1 and NellisHangar1b (the B-29 inside),
//! entered through two pairs of doors on the same spots outdoors; the pair into 1b is
//! "Initially Disabled" with its enable parent the other pair, opposite, and the scripts
//! swap them once the B-29 is up [observed: FalloutNV.esm REFR 0x001093DB/0x001093DC,
//! XESP 0x00104A3F/0x00104A3E "Set Enable State to Opposite of Parent"; SCPT 0x001093CB].
//! A marker must not lead through the disabled pair (it pointed at the hangar door
//! outdoors and back out of it indoors). Needs the game data, never in the repository:
//! set `FNVLAB_DATA_DIR` to the game's `Data` folder; without it the test prints why it
//! was skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::quest_targets::{marker_door, DoorGraph, Place};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
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

const HANGAR1: FormId = FormId(0x000FFCAF);
const HANGAR1B: FormId = FormId(0x00108CBA);
const WASTELAND: FormId = FormId(0x000DA726);
/// Outdoors: the doors into NellisHangar1, and those into NellisHangar1b on the same spots.
const INTO_1: [FormId; 2] = [FormId(0x00104A3E), FormId(0x00104A3F)];
const INTO_1B: [FormId; 2] = [FormId(0x001093DB), FormId(0x001093DC)];

#[test]
fn a_marker_never_leads_through_a_disabled_door() {
    let Some(dir) = data_dir() else { return };
    let plugins = vec![Plugin::open(dir.join("FalloutNV.esm")).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_dir(root.join("schemas/fnv")).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let mut world = World::new(&forms, &refs, SimConfig::default());
    let graph = DoorGraph::build(&refs);
    let (inside, b29, outside) = (Place::Interior(HANGAR1), Place::Interior(HANGAR1B), Place::World(WASTELAND));
    let into_1b: Vec<FormId> = INTO_1B.to_vec();
    // As placed: the way into 1b is only through its disabled doors, so no marker leads
    // there, from the hangar the player can enter nor from outdoors; the world map has no
    // door for it either.
    assert!(INTO_1B.iter().all(|&d| !world.is_enabled(d).unwrap()));
    assert_eq!(graph.first_door(&world, inside, b29, false), None);
    assert_eq!(graph.first_door(&world, outside, b29, false), None);
    assert_eq!(graph.path(&world, outside, b29, false), None);
    assert_eq!(marker_door(&world, &into_1b, false), None);
    // The hangar the player can enter is still marked through its own doors.
    assert!(graph.first_door(&world, outside, inside, false).is_some_and(|d| INTO_1.contains(&d)));
    // The scripts' swap once the B-29 is up (SCPT 0x001093CB): the 1b doors are the way.
    for d in INTO_1 {
        world.set_enabled(d, false).unwrap();
    }
    for d in INTO_1B {
        world.set_enabled(d, true).unwrap();
    }
    assert!(graph.first_door(&world, outside, b29, false).is_some_and(|d| INTO_1B.contains(&d)));
    assert!(marker_door(&world, &into_1b, false).is_some_and(|d| INTO_1B.contains(&d)));
    assert_eq!(graph.first_door(&world, outside, inside, false), None);
}
