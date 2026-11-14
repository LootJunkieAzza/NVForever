//! Fixtures that never fight (`Game::is_fixture`, issue #2431) against the game's own
//! masters. They need the game data, which is never in the repository: set
//! `FNVLAB_DATA_DIR` (the parity lab's `data_dir`) to the game's `Data` folder. Without
//! it, or without the ten masters there, the test prints why it was skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_script::vm::{ArgValue, Value};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MASTERS: [&str; 10] = [
    "FalloutNV.esm",
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
    "CaravanPack.esm",
];

fn data_dir() -> Option<PathBuf> {
    let Some(dir) = std::env::var_os("FNVLAB_DATA_DIR").map(PathBuf::from) else {
        eprintln!("skipped: FNVLAB_DATA_DIR is not set (no game data)");
        return None;
    };
    if let Some(missing) = MASTERS.iter().find(|m| !dir.join(m).is_file()) {
        eprintln!("skipped: {missing} is not in {}", dir.display());
        return None;
    }
    Some(dir)
}

/// Mr. House's screens and his chamber are fixtures; the turrets are not.
/// A fixture refuses StartCombat, keeps no combat state after the awareness settles, is
/// never hostile to the player even with its faction an enemy of the player's, and the
/// screens take no damage.
#[test]
fn the_masters_fixtures_never_fight() {
    let Some(dir) = data_dir() else { return };
    let plugins: Vec<Plugin> = MASTERS.iter().map(|m| Plugin::open(dir.join(m)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_dir(root.join("schemas/fnv")).unwrap();
    let fp = root.join("schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&fp).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&fp).unwrap(), &j, &refs);
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());
    g.new_game();

    // Every placed fixture of the masters, by its base's Editor ID.
    let mut bases: BTreeSet<String> = BTreeSet::new();
    let placed: Vec<FormId> = refs.refs.keys().copied().collect();
    for r in placed {
        if g.is_fixture(r) {
            let base = refs.refs[&r].base;
            bases.insert(data.name(base));
        }
    }
    // House's two screens, his chamber and NVCRFestus (a ghosted, Invulnerable robot);
    // the turrets carry their guns; nothing else of the masters is one [observed: survey of
    // the masters, 2026-11-14].
    let want: BTreeSet<String> = ["MrHouse", "MrHouseOFFLINE", "NVCRFestus", "NVCRMrHouse"].map(String::from).into();
    assert_eq!(bases, want);

    let screen = refs.by_edid("Lucky38MrHouseREF").expect("House's penthouse screen");
    let chamber = refs.by_edid("NVCRMrHouse01REF").expect("House's chamber");
    assert!(g.is_immune(screen), "the screen is Invulnerable");
    // His factions turned against the player (the elevator Victor's OnDeath and OnHit).
    let player_faction = data.by_edid("PlayerFaction").unwrap();
    for f in ["Lucky38Faction", "Lucky38MrHouseFaction"] {
        let f = data.by_edid(f).unwrap();
        let ctx = Ctx { unit: "test".into(), opcode: 0x1078, on: None, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
        g.call_function(&ctx, &[ArgValue::Value(Value::Ref(f.0)), ArgValue::Value(Value::Ref(player_faction.0))]);
    }
    for fixture in [screen, chamber] {
        // The player's crimes against it (House murdered before its eyes) too.
        g.player.crime_enemies.insert(fixture);
        g.start_combat(fixture, FormId::PLAYER_REF);
        g.settle_awareness();
        assert!(!g.would_attack(fixture, FormId::PLAYER_REF), "{fixture} is not hostile");
        assert!(!g.awareness.state(fixture).in_combat(), "{fixture} is not in combat");
    }
}
