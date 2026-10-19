//! Caravan's cards and decks read from the game's own masters (docs/spec-notes/caravan.md
//! §2, §6). They need the game data, which is never in the repository: set
//! `FNVLAB_DATA_DIR` (the parity lab's `data_dir`) to the game's `Data` folder. Without it,
//! or without the ten masters there, the test prints why it was skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::caravan::{self, Rank};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, RefTable};
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

fn load(dir: &Path) -> GameData {
    let plugins: Vec<Plugin> = MASTERS.iter().map(|m| Plugin::open(dir.join(m)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_dir(root.join("schemas/fnv")).unwrap();
    let fp = root.join("schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&fp).unwrap()).unwrap();
    let table = FunctionTable::load(&fp).unwrap();
    let refs = RefTable::build(&lo, &schema);
    GameData::build(&lo, &schema, table, &j, &refs)
}

#[test]
fn the_masters_caravan_cards_and_decks() {
    let Some(dir) = data_dir() else { return };
    let d = load(&dir);
    // FalloutNV.esm alone holds 270 cards and 13 decks [observed: fnvtool esm stats].
    assert!(d.caravan_cards.len() >= 270, "{} cards", d.caravan_cards.len());
    assert!(d.caravan_decks.len() >= 13, "{} decks", d.caravan_decks.len());
    // `CardClubs10b` [observed: INTV 4 then 10, TX00 deck B's 10 of clubs].
    let ten = &d.caravan_cards[&FormId(0x0016_49C0)];
    assert_eq!((ten.suit, ten.rank), (caravan::Suit::Clubs, Rank::Num(10)));
    assert!(ten.face.as_deref().is_some_and(|f| f.to_ascii_lowercase().ends_with("cardb_c10.dds")));
    let mut decks: Vec<_> = d.caravan_decks.iter().collect();
    decks.sort_by_key(|(f, _)| f.0);
    for (form, deck) in decks {
        let cards: Vec<caravan::Card> = deck.cards.iter().filter_map(|f| d.caravan_cards.get(f).map(|c| caravan::Card::new(*f, c.suit, c.rank))).collect();
        assert_eq!(cards.len(), deck.cards.len(), "{form} {:?}: every CARD names a caravan card", deck.name);
        let faces = cards.iter().filter(|c| c.rank.is_face()).count();
        eprintln!("{form} {:?}: {} cards, {faces} face cards, skill {:.2}", deck.name, cards.len(), caravan::skill(&cards));
    }
    // Johnson Nash's deck, the one his lines pass with difficulty 1.
    let nash = &d.caravan_decks[&FormId(0x0016_49D1)];
    assert_eq!(nash.name, "Mojave Express Spectacular");
    // The help messages the menu shows.
    for m in [caravan::HELP_BETTING, caravan::HELP_DECK, caravan::HELP_STARTING, caravan::HELP_WAR] {
        assert!(d.messages.get(&m).and_then(|m| m.text.as_ref()).is_some(), "{m}: no text");
    }
}
