//! Hit effects against the game's own master (docs/spec-notes/hit-effects.md §1): the
//! unarmed attack effects of placed creatures, through their templates and leveled
//! lists. They need the game data, which is never in the repository: set
//! `FNVLAB_DATA_DIR` to the game's `Data` folder. Without it each test prints why it was
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

#[test]
fn placed_creatures_carry_their_poison() {
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
    let poison = |g: &Game, r: u32| g.unarmed_effect(FormId(r)).map(|(e, anim)| (data.name(e), anim.map(str::to_string)));
    // VERT3NovacCazador01REF and 02 (leveled young cazadors), VERGSRadscorpion5REF.
    for (r, want) in [(0x00168CC2, "CazadorPoison"), (0x00168CC1, "CazadorPoison"), (0x00168CBE, "CazadorPoison"), (0x00168CBF, "CazadorPoison"), (0x00157B4D, "RadScorpion")] {
        let got = poison(&g, r);
        assert!(got.as_ref().is_some_and(|(e, _)| e.starts_with(want)), "{r:08X}: {got:?}");
    }

    // A cazador's sting on the player: its player lines (5 health a second for 30 s, 25
    // Action Points), not the 18 a second of its other line (hit-effects.md §2).
    let item = data.by_edid("CazadorPoison").expect("CazadorPoison");
    let applied = g.apply_hit_effect(FormId::PLAYER_REF, item, Some(FormId(0x00168CC2)));
    let names: Vec<String> = applied.iter().map(|a| format!("{} {:?} {}", data.name(a.effect), a.change.map(|c| c.amount), a.duration)).collect();
    assert_eq!(applied.len(), 3, "{names:?}");
    assert!(names[0].starts_with("Poison") && names[0].contains("30"), "{names:?}");

    // A cazador a script placed (PlaceAtMe) carries the same poison.
    let base = data.by_edid("NVCrCazador").expect("NVCrCazador");
    let placed = g.world.place(base, None, [0.0; 3], [0.0; 3], 1).expect("placed");
    let got = poison(&g, placed.0);
    assert!(got.as_ref().is_some_and(|(e, _)| e == "CazadorPoison"), "{got:?}");
}
