//! Crippled limbs against the game's own master (docs/spec-notes/crippled-limbs.md): the
//! player's Perception with a crippled head, which the masters' `GenericScript` gives
//! through `PlayerConcussed` (and the engine's `HeadCrippleEffect` must not add to). They need the game data, which is never
//! in the repository: set `FNVLAB_DATA_DIR` to the game's `Data` folder. Without it each
//! test prints why it was skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::{actor_value, add_actor_value_damage, restore_actor_value_damage, set_actor_value_current};
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

const PERCEPTION: u16 = 6;
const HEAD: u16 = 25;

/// Perception −4 while the player's head is crippled (once, not twice), all of it back
/// once healed.
#[test]
fn a_crippled_head_takes_four_perception_and_gives_it_back() {
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
    let p = FormId::PLAYER_REF;
    let per = |g: &mut Game| actor_value(g, p, PERCEPTION).unwrap();
    let run = |g: &mut Game, frames: usize| {
        for _ in 0..frames {
            g.tick();
        }
    };
    run(&mut g, 120);
    set_actor_value_current(&mut g, p, PERCEPTION, 6.0).unwrap();
    assert_eq!(per(&mut g), 6.0);
    add_actor_value_damage(&mut g, p, HEAD, 1000.0);
    let mut seen = Vec::new();
    for _ in 0..8 {
        run(&mut g, 60);
        seen.push(per(&mut g));
    }
    // Within a few seconds (the script casts the spell, then its condition is checked),
    // and never lower.
    assert!(seen[3..].iter().all(|v| *v == 2.0) && seen.iter().all(|v| *v >= 2.0), "Perception while crippled, each second: {seen:?}");
    restore_actor_value_damage(&mut g, p, HEAD, 1000.0);
    run(&mut g, 180);
    assert_eq!(per(&mut g), 6.0);
}
