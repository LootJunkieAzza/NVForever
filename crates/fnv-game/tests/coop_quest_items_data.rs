//! What the host's quests hand a co-op guest, over the masters' own quest records (owner
//! request of 2026-11-14, `fnv_game::coop_rewards`): the host runs each quest of a matrix
//! (every stage's result scripts in order, then the result scripts of the quest's
//! dialogue lines), and two guests take what it handed out:
//! - guest A, a fresh game, gets every item: a gun with its rounds, quest items, keys and
//!   notes (a Caravan deck once);
//! - guest B, a game that played those quests itself (the same stages set in its own
//!   game, as console `SetStage` calls would), gets none of them again and nothing it
//!   holds stacks up.
//!
//! The quests are found by their names in the data (FULL), so the matrix is the masters'
//! own records, not FormIDs typed here: Doc Mitchell's and Sunny Smiles' quests, Ghost
//! Town Gunfight, the Platinum Chip, the Strip, each faction's ending, and each add-on's
//! first quests.
//!
//! Needs the game data, which is never in the repository: set `FNVLAB_DATA_DIR` to the
//! game's `Data` folder with the base game and the add-ons. Without it the test prints
//! why it was skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::coop_rewards::Gift;
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::forms::ItemKind;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;
use std::collections::BTreeMap;
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

/// The matrix, by quest name as the data spells it.
const QUESTS: [&str; 18] = [
    // Doc Mitchell: the Pip-Boy, the Vault 21 jumpsuit and his gifts.
    "Ain't That a Kick in the Head",
    // Sunny Smiles: the varmint rifle and its rounds.
    "Back in the Saddle",
    "Ghost Town Gunfight",
    // Benny and the Platinum Chip.
    "They Went That-a-Way",
    "Ring-a-Ding-Ding!",
    "The House Always Wins, I",
    // Each faction's ending.
    "All or Nothing",
    "Eureka!",
    "Veni, Vidi, Vici",
    "No Gods, No Masters",
    // The add-ons' starts. Dead Money's collar, suit, holorifle and rounds are its
    // master quest's stage 5 (DeadMoney.esm QUST 0x01000FBF) [observed: the data].
    "Sierra Madre Grand Opening!",
    "MQ01 Master Quest",
    "Fires in the Sky",
    "Happy Trails Expedition",
    "Midnight Science-Fiction Feature!",
    // Old World Blues' lab coat: stage 10 (OldWorldBlues.esm QUST 0x01002FCB).
    "Welcome to the Big Empty",
    "The Job",
    "The Reunion",
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

struct Loaded {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn load(dir: &Path) -> Loaded {
    let plugins: Vec<Plugin> = MASTERS.iter().map(|m| Plugin::open(dir.join(m)).unwrap()).collect();
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

fn game(l: &Loaded) -> Game<'_> {
    let world = World::new(&l.forms, &l.refs, SimConfig::default());
    let mut g = Game::new(&l.data, world, GameConfig::default());
    g.new_game();
    g
}

/// The quests of the matrix found in the data, by name (a name two quests share: each).
fn matrix(data: &GameData) -> Vec<(String, FormId)> {
    let mut out = Vec::new();
    for name in QUESTS {
        let found: Vec<FormId> = data.quests.iter().filter(|(_, q)| q.name.as_deref() == Some(name)).map(|(id, _)| *id).collect();
        if found.is_empty() {
            eprintln!("note: no quest named {name:?} in the data");
        }
        out.extend(found.into_iter().map(|q| (name.to_string(), q)));
    }
    out
}

/// A dialogue line: its FormID, fixed speaker and begin and end result scripts.
type Line = (FormId, Option<FormId>, Option<usize>, Option<usize>);

/// Play `quest` in `g`: each stage in order (its result scripts, as SetStage runs them),
/// then each of its dialogue lines' result scripts (begin, end), spoken by the line's
/// speaker when it names one.
fn play(g: &mut Game, quest: FormId) {
    let stages: Vec<i32> = g.data.quests[&quest].stages.keys().copied().collect();
    for s in stages {
        g.set_stage(quest, s);
    }
    let mut lines: Vec<Line> = g.data.infos.iter().filter(|(_, i)| i.quest == Some(quest)).map(|(id, i)| (*id, i.speaker, i.begin, i.end)).collect();
    lines.sort_by_key(|l| l.0);
    for (_, speaker, begin, end) in lines {
        for u in [begin, end].into_iter().flatten() {
            g.run_result(u, RunCtx { self_ref: speaker, action_ref: None, quest: Some(quest) });
        }
    }
}

fn kind(g: &Game, item: FormId) -> Option<ItemKind> {
    g.world.forms.item(item).map(|i| i.kind)
}

fn held(g: &mut Game, item: FormId) -> i32 {
    g.world.item_count(FormId::PLAYER_REF, item).unwrap_or(0)
}

#[test]
fn the_hosts_quest_items_reach_a_fresh_guest_and_not_one_that_played_them() {
    let Some(dir) = data_dir() else { return };
    let l = load(&dir);
    let quests = matrix(&l.data);
    assert!(quests.len() >= 12, "most of the matrix is in the data: {quests:?}");

    let mut host = game(&l);
    host.coop_rewards.on = true;
    let mut by_quest: BTreeMap<FormId, Vec<Gift>> = BTreeMap::new();
    for (_, q) in &quests {
        play(&mut host, *q);
        by_quest.entry(*q).or_default().extend(std::mem::take(&mut host.coop_rewards.gifts));
    }

    // Guest B played the same quests in its own game.
    let mut b = game(&l);
    for (_, q) in &quests {
        play(&mut b, *q);
    }

    let mut a = game(&l);
    let (mut a_last, mut b_last) = (None, None);
    let mut handed = 0;
    let mut gun_and_rounds = Vec::new();
    for (name, q) in &quests {
        let gifts = &by_quest[q];
        let list: Vec<String> = gifts.iter().map(|g| format!("{} x{}{}", l.data.name(g.item), g.count, g.stage.map_or(String::new(), |s| format!(" (stage {s})")))).collect();
        eprintln!("{name} {q}: {} hand-out(s): {}", gifts.len(), list.join(", "));
        for gift in gifts {
            assert_eq!(gift.quest, Some(*q), "{name}: the batch names its quest");
            handed += 1;
            // Guest A: fresh, takes everything (a Caravan card goes to its cards, a note
            // to its notes).
            let before = held(&mut a, gift.item);
            let n = a.take_coop_gift(gift, &mut a_last).unwrap();
            let k = kind(&a, gift.item);
            let card = k == Some(ItemKind::CaravanCard) || l.data.caravan_cards.contains_key(&gift.item);
            if card {
                assert!(n > 0 || !a.player.caravan.cards.is_empty(), "{name}: guest A gets the deck");
            } else if k == Some(ItemKind::Note) {
                assert!(a.player.notes.contains(&gift.item), "{name}: guest A has the note {}", l.data.name(gift.item));
            } else if matches!(k, Some(ItemKind::Ammo | ItemKind::Aid | ItemKind::CasinoChip | ItemKind::CaravanMoney)) || before == 0 {
                assert!(n > 0 && held(&mut a, gift.item) > before, "{name}: guest A gets {} ({:?})", l.data.name(gift.item), k);
            }
            // Guest B: did it in its own game; takes nothing, holds no more than before.
            let b_before = held(&mut b, gift.item);
            assert!(b.coop_gift_done_here(gift), "{name}: guest B's own game did {q}");
            assert_eq!(b.take_coop_gift(gift, &mut b_last).unwrap(), 0, "{name}: guest B keeps its own {}", l.data.name(gift.item));
            assert_eq!(held(&mut b, gift.item), b_before, "{name}: nothing stacks on guest B");
        }
        // A gun and its rounds in one batch: guest A has both.
        let mut batches: BTreeMap<u32, Vec<&Gift>> = BTreeMap::new();
        for g in gifts {
            batches.entry(g.batch).or_default().push(g);
        }
        for items in batches.values() {
            let gun = items.iter().find(|g| kind(&a, g.item) == Some(ItemKind::Weapon));
            let ammo = items.iter().find(|g| kind(&a, g.item) == Some(ItemKind::Ammo));
            if let (Some(gun), Some(ammo)) = (gun, ammo) {
                assert!(held(&mut a, gun.item) > 0 && held(&mut a, ammo.item) >= ammo.count, "{name}: guest A has {} and its rounds", l.data.name(gun.item));
                gun_and_rounds.push(format!("{} + {} x{}", l.data.name(gun.item), l.data.name(ammo.item), ammo.count));
            }
        }
    }
    eprintln!("{handed} hand-outs over {} quests; gun and rounds together: {gun_and_rounds:?}", quests.len());
    assert!(handed > 0, "the matrix hands out items");
    assert!(!gun_and_rounds.is_empty(), "Sunny's varmint rifle and its rounds come as one batch");
}

/// Quest items are hand-outs now (owner, 2026-11-14; item 123 kept them the host's): a
/// quest item a stage's script gives the host is in the batch, and a guest that already
/// holds one gets no second.
#[test]
fn a_quest_item_is_handed_out_once() {
    let Some(dir) = data_dir() else { return };
    let l = load(&dir);
    let mut host = game(&l);
    host.coop_rewards.on = true;
    let mut quest_items = Vec::new();
    for (_, q) in matrix(&l.data) {
        play(&mut host, q);
        quest_items.extend(std::mem::take(&mut host.coop_rewards.gifts).into_iter().filter(|g| host.world.is_quest_item(g.item) && kind(&host, g.item) != Some(ItemKind::Note)));
    }
    eprintln!("quest items handed out: {:?}", quest_items.iter().map(|g| l.data.name(g.item)).collect::<Vec<_>>());
    assert!(!quest_items.is_empty(), "the matrix hands out quest items");
    let mut a = game(&l);
    let mut last = None;
    for g in &quest_items {
        let fresh = Gift { stage: None, quest: None, ..*g };
        a.take_coop_gift(&fresh, &mut last).unwrap();
        let once = held(&mut a, g.item);
        assert_eq!(a.take_coop_gift(&fresh, &mut last).unwrap(), 0, "a second {} is not given", l.data.name(g.item));
        assert_eq!(held(&mut a, g.item), once);
    }
}

/// A guest that only half finished a quest in its own game (the stages before a stage
/// that hands something out set, the quest not completed) still gets what the host's
/// later stage hands out, and nothing of the stages it did (parity, 2026-11-14).
#[test]
fn a_half_finished_quest_gives_the_later_stages_items() {
    let Some(dir) = data_dir() else { return };
    let l = load(&dir);
    let mut host = game(&l);
    host.coop_rewards.on = true;
    let mut checked = Vec::new();
    for (name, q) in matrix(&l.data) {
        play(&mut host, q);
        let gifts: Vec<Gift> = std::mem::take(&mut host.coop_rewards.gifts).into_iter().filter(|g| g.stage.is_some()).collect();
        let mut stages: Vec<i32> = gifts.iter().filter_map(|g| g.stage).collect();
        stages.sort();
        stages.dedup();
        if stages.is_empty() {
            continue;
        }
        // Guest C set the stages before the last stage that hands something out.
        let last = *stages.last().unwrap();
        let mut c = game(&l);
        let done: Vec<i32> = l.data.quests[&q].stages.keys().copied().filter(|s| *s < last).collect();
        for s in &done {
            c.set_stage(q, *s);
        }
        if done.is_empty() || c.quests.get(&q).is_some_and(|s| s.completed) {
            continue;
        }
        let mut lastb = None;
        for g in &gifts {
            let did = g.stage.is_some_and(|s| s < last);
            assert_eq!(c.coop_gift_done_here(g), did, "{name} stage {:?}", g.stage);
            let before = held(&mut c, g.item);
            let n = c.take_coop_gift(g, &mut lastb).unwrap();
            if did {
                assert_eq!(n, 0, "{name}: {} of a stage guest C did is not given again", l.data.name(g.item));
                assert_eq!(held(&mut c, g.item), before);
            } else if before == 0 && !matches!(kind(&c, g.item), Some(ItemKind::Note | ItemKind::CaravanCard)) {
                assert!(n > 0, "{name}: guest C gets {} of stage {last}, which it has not done", l.data.name(g.item));
            }
        }
        checked.push(format!("{name}: did {} stage(s) before {last}", done.len()));
    }
    eprintln!("half-finished quests checked: {checked:?}");
    assert!(!checked.is_empty(), "some quest of the matrix hands out items at a stage after others");
}

/// A guest that completed a quest in its own game still takes the host's completion XP
/// and caps for it (`coop_rewards::REPEAT_DONE_REWARDS` on; owner, 2026-11-14): only
/// items keep the no-repeat rule.
#[test]
fn a_completed_quests_xp_and_caps_still_reach_the_guest() {
    let Some(dir) = data_dir() else { return };
    let l = load(&dir);
    let q = matrix(&l.data).into_iter().find(|(n, _)| n == "Back in the Saddle").expect("Back in the Saddle").1;
    let mut b = game(&l);
    play(&mut b, q);
    assert!(b.quests[&q].completed, "Back in the Saddle completes when all its stages are set");
    assert!(!b.coop_reward_done_here(q), "the reward is taken again");
    let caps = held(&mut b, fnv_game::coop_rewards::CAPS);
    let xp = b.take_coop_reward(100.0, 50).unwrap();
    assert!(xp > 0.0, "guest B gains the XP");
    assert_eq!(held(&mut b, fnv_game::coop_rewards::CAPS), caps + 50, "guest B gains the caps");
}

/// Items the add-ons' object scripts give the player (parity, 2026-11-14: a fresh guest
/// gets every quest item, also those a quest's or a quest object's script gives).
/// Activated by the player on a fresh host, each row's script hands out for the quest it
/// names (Dead Money's Tampico prop, SCPT 0x0100FFD4: two items and NVDLC01MQ03b's
/// objective 15 [observed: the data]) or not at all (a pickup that names no quest).
#[test]
fn quest_objects_scripts_hand_out_for_their_quest() {
    let Some(dir) = data_dir() else { return };
    let l = load(&dir);
    // (script, label) [observed: the add-ons' scripts that AddItem to the player].
    let rows: [(u32, &str); 5] = [
        (0x0100_FFD4, "Dead Money: Tampico props (names NVDLC01MQ03b)"),
        (0x0201_12F7, "Honest Hearts: OnActivate pickup"),
        (0x0301_2F01, "Old World Blues: OnActivate weapon pickup"),
        (0x0400_E106, "Lonesome Road: OnActivate pickup"),
        (0x0400_D792, "Lonesome Road: OnActivate pickup"),
    ];
    let mut out = Vec::new();
    let mut dm = None;
    for (script, label) in rows {
        let script = FormId(script);
        let Some(placed) = l.refs.refs.values().find(|r| l.data.script_of_base(r.base) == Some(script)).map(|r| r.id) else {
            out.push(format!("{label} {script}: no placed reference"));
            continue;
        };
        let mut host = game(&l);
        host.coop_rewards.on = true;
        let ctx = RunCtx { self_ref: Some(placed), action_ref: Some(FormId::PLAYER_REF), quest: None };
        host.run_event(fnv_game::game::InstKey::Ref { reference: placed }, script, fnv_game::game::event::ON_ACTIVATE, Some(fnv_script::vm::Value::Ref(FormId::PLAYER_REF.0)), ctx);
        let gifts = std::mem::take(&mut host.coop_rewards.gifts);
        let list: Vec<String> = gifts.iter().map(|g| format!("{} x{} for {}", l.data.name(g.item), g.count, g.quest.map_or("-".into(), |q| l.data.name(q)))).collect();
        out.push(format!("{label} {script} on {placed}: {} hand-out(s) {list:?}", gifts.len()));
        if script == FormId(0x0100_FFD4) {
            dm = Some(gifts);
        }
    }
    eprintln!("add-on object scripts:\n  {}", out.join("\n  "));
    let dm = dm.expect("Dead Money's Tampico prop is placed");
    assert!(!dm.is_empty() && dm.iter().all(|g| g.quest == Some(FormId(0x0101_3A43))), "the Tampico props hand out for NVDLC01MQ03b: {dm:?}");
    // Guest A takes them; guest B, whose game completed NVDLC01MQ03b, keeps its own.
    let (mut a, mut b) = (game(&l), game(&l));
    b.quest(FormId(0x0101_3A43)).completed = true;
    let (mut la, mut lb) = (None, None);
    for g in &dm {
        assert!(a.take_coop_gift(g, &mut la).unwrap() > 0, "guest A gets {}", l.data.name(g.item));
        assert_eq!(b.take_coop_gift(g, &mut lb).unwrap(), 0, "guest B keeps its own {}", l.data.name(g.item));
    }
}
