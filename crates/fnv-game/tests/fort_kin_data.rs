//! The Fort's Legion against the game's own master (owner 2026-11-15, Finger of
//! Suspicion): Caesar, Lucius and the Praetorians killed Vulpes Inculta on sight in
//! Caesar's tent. FortVulpesInculta is in VCaesarsLegionFaction, which names itself an
//! ally, and in ArmorNCRFactionNVEnemy, which the Legion names an enemy; in the original
//! nobody there attacks him. Needs the game data, which is never in the repository: set
//! `FNVLAB_DATA_DIR` to the game's `Data` folder. Without it the test prints why it was
//! skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
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

const VULPES: u32 = 0x00122B9A;
const CAESAR: u32 = 0x00121FF0;
const LUCIUS: u32 = 0x0011FC9E;
/// FortLegionaryPraetorianHM, its factions from its template.
const PRAETORIAN: u32 = 0x0011F9D3;

#[test]
fn the_legion_in_caesars_tent_counts_vulpes_as_its_own() {
    let Some(dir) = data_dir() else { return };
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
    let mut g = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default());
    g.new_game();
    let vulpes = FormId(VULPES);
    for other in [CAESAR, LUCIUS, PRAETORIAN].map(FormId) {
        assert!(g.kin(vulpes, other) && g.kin(other, vulpes), "Vulpes and {other:?} are of the Legion");
        assert!(!g.would_attack(other, vulpes) && !g.would_attack(vulpes, other), "{other:?} and Vulpes do not fight");
    }
}
