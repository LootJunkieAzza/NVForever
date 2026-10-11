//! Radio programmes (`fnv_game::radio`) against the game's own master (docs/spec-notes/radio.md §5). They
//! need the game data, which is never in the repository: set `FNVLAB_DATA_DIR` to the
//! game's `Data` folder. Without it each test prints why it was skipped and passes.

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

/// The lines a station says in a row from the start of a new game, each begun and ended.
fn programme(l: &Loaded, station: &str, n: usize) -> Vec<(FormId, FormId)> {
    let (data, refs) = (&l.data, &l.refs);
    let world = World::new(&l.forms, refs, SimConfig::default());
    let mut g = Game::new(data, world, GameConfig::default());
    g.new_game();
    let base = data.by_edid(station).expect("station");
    let station = refs.refs.iter().find(|(_, p)| p.base == base).map(|(id, _)| *id).expect("a placed station");
    let mut out = Vec::new();
    let mut last = None;
    for _ in 0..n {
        let Some((topic, info)) = g.radio_next(station, last) else { break };
        g.radio_line_begins(station, info);
        g.radio_line_ends(station, info);
        out.push((topic, info));
        last = Some(info);
    }
    out
}

#[test]
fn radio_programmes() {
    let Some(dir) = data_dir() else { return };
    let l = load(&dir);
    mojave_music_radio_rotates_its_songs(&l);
    radio_new_vegas_goes_from_news_to_songs(&l);
}

/// Mojave Music Radio (`vCountryRadio`) says 30 songs from RadioHello: every line has a
/// song (a sound, no voice), and its quest's conditions with the lines' begin scripts
/// keep a song from coming back within the next three [observed: the conditions name
/// three "last played" variables, radio.md §5].
fn mojave_music_radio_rotates_its_songs(l: &Loaded) {
    let data = &l.data;
    let lines = programme(l, "vCountryRadio", 30);
    assert_eq!(lines.len(), 30);
    let mut said: Vec<FormId> = Vec::new();
    for &(_, info) in &lines {
        let def = &data.infos[&info];
        assert!(def.responses.iter().all(|r| r.sound.is_some()), "{info} has no song");
        if let Some(k) = said.iter().rposition(|&i| i == info) {
            assert!(said.len() - k > 3, "{info} came back after {} lines", said.len() - k);
        }
        said.push(info);
    }
    let distinct: std::collections::BTreeSet<FormId> = said.iter().copied().collect();
    assert!(distinct.len() >= 8, "only {} songs in 30 lines", distinct.len());
}

/// Radio New Vegas (`RadioNewVegasTA`) over 60 lines: Mr. New Vegas's lines (voice, no
/// sound) lead into songs (lines with a sound) and back to the news [observed: the
/// topics' links, radio.md §5].
fn radio_new_vegas_goes_from_news_to_songs(l: &Loaded) {
    let data = &l.data;
    let lines = programme(l, "RadioNewVegasTA", 60);
    assert_eq!(lines.len(), 60);
    let topic = |t: FormId| data.topic_defs.get(&t).and_then(|d| d.edid.clone()).unwrap_or_default();
    let songs = lines.iter().filter(|(_, i)| data.infos[i].responses.iter().any(|r| r.sound.is_some())).count();
    let topics: Vec<String> = lines.iter().map(|(t, _)| topic(*t)).collect();
    eprintln!("Radio New Vegas: {songs} songs in 60 lines: {topics:?}");
    assert!((5..50).contains(&songs), "{songs} songs in 60 lines");
    assert!(topics.iter().filter(|t| *t == "RadioHello").count() >= 2, "the programme did not come round again");
}
