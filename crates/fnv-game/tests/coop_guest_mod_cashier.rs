//! A co-op guest at a mod casino's cashier: the Black Chip's in Long 15
//! (`Long15Reloaded.esm`, overnight queue 2026-11-14 19:08, the owner: "guests cant buy
//! chips or say goodbye to this guuy"). The cashier sells its chips by result script
//! (`player.additem ALBakerCasinoChip 100` / `player.removeitem caps001 100`) and ends the
//! conversation with a topic of its own, ALBakerCashierGoodbye, whose line is flagged
//! Goodbye, not with the game's GOODBYE topic [observed: Long15Reloaded.esm INFO 06004662,
//! 06004664, 0600466D, 06004667].
//!
//! Needs the game data and the plugin, never in the repository: `FNVLAB_DATA_DIR` (the
//! game's Data folder with the ten masters) and `NVF_TEST_LONG15_ESM` (the plugin's path).
//! Without them the test prints why it was skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;
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

fn plugins() -> Option<Vec<PathBuf>> {
    let Some(dir) = std::env::var_os("FNVLAB_DATA_DIR").map(PathBuf::from) else {
        eprintln!("skipped: FNVLAB_DATA_DIR is not set (no game data)");
        return None;
    };
    let Some(long15) = std::env::var_os("NVF_TEST_LONG15_ESM").map(PathBuf::from).filter(|p| p.is_file()) else {
        eprintln!("skipped: NVF_TEST_LONG15_ESM is not set to Long15Reloaded.esm");
        return None;
    };
    if let Some(missing) = MASTERS.iter().find(|m| !dir.join(m).is_file()) {
        eprintln!("skipped: {missing} is not in {}", dir.display());
        return None;
    }
    Some(MASTERS.iter().map(|m| dir.join(m)).chain([long15]).collect())
}

/// Long 15's own forms load at index 10, after the ten masters.
fn own(id: u32) -> FormId {
    FormId(0x0A00_0000 | id)
}

fn listen(g: &mut Game) {
    for _ in 0..20_000 {
        if g.dialogue.as_ref().is_none_or(|c| !c.choices.is_empty()) {
            return;
        }
        g.tick();
    }
    panic!("the cashier never stopped speaking");
}

fn offered(g: &Game) -> Vec<u32> {
    g.dialogue_choices().iter().map(|c| c.info.0 & 0xFF_FFFF).collect()
}

fn pick(g: &mut Game, info: u32) {
    let i = g.dialogue_choices().iter().position(|c| c.info.0 & 0xFF_FFFF == info).unwrap_or_else(|| panic!("0x{info:X} is not offered: {:X?}", offered(g)));
    g.choose(i).unwrap();
    listen(g);
}

#[test]
fn a_guest_buys_black_chip_chips_and_says_goodbye() {
    let Some(paths) = plugins() else { return };
    let plugins: Vec<Plugin> = paths.iter().map(|p| Plugin::open(p).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_dir(root.join("schemas/fnv")).unwrap();
    let fp = root.join("schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&fp).unwrap()).unwrap();
    let table = FunctionTable::load(&fp).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);

    let (cashier, chip, caps) = (own(0x45E9), own(0x45EE), FormId(0xF));
    let (greeting, menu, buy100, bye) = (0x4662, 0x4664, 0x466D, 0x4667);
    let lines = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default()).guest_lines().clone();
    eprintln!("guest lines of Long 15: {:X?}", lines.iter().filter(|i| i.0 >> 24 == 0x0A).map(|i| i.0).collect::<Vec<_>>());

    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());
    g.new_game();
    g.coop_guest = true;
    g.world.add_item(FormId::PLAYER_REF, caps, 300).unwrap();
    assert!(g.guest_can_talk(cashier), "a guest is offered Talk on the cashier");
    g.start_dialogue(cashier, None).unwrap();
    listen(&mut g);
    let first = offered(&g);
    eprintln!("guest's first choices: {first:X?}");
    // Its greeting's own choices only: not the vanilla cashiers' top-level exchange topics
    // its cashier faction would also pass (laptop co-op run 2026-11-15, six choices).
    assert_eq!(first, vec![menu, 0x4052, bye], "the greeting's caps, NCR and Goodbye choices only");
    assert!(lines.contains(&own(greeting)) || first.contains(&menu), "{first:X?}");
    assert!(first.contains(&bye), "the cashier's own Goodbye line is offered: {first:X?}");
    pick(&mut g, menu);
    pick(&mut g, buy100);
    assert_eq!(g.world.item_count(FormId::PLAYER_REF, chip).unwrap(), 100);
    assert_eq!(g.world.item_count(FormId::PLAYER_REF, caps).unwrap(), 200);
    assert!(offered(&g).contains(&bye), "{:X?}", offered(&g));
    pick(&mut g, bye);
    assert!(g.dialogue.is_none(), "Goodbye ends the conversation");
}
