//! Combat-tab lines (`fnv_game::combat_lines`) against the game's own master
//! (docs/spec-notes/combat-dialogue.md §3). They need the game data, which is never in the
//! repository: set `FNVLAB_DATA_DIR` to the game's `Data` folder. Without it each test
//! prints why it was skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat::HitInfo;
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

fn game(l: &Loaded) -> Game<'_> {
    let world = World::new(&l.forms, &l.refs, SimConfig::default());
    let mut g = Game::new(&l.data, world, GameConfig::default());
    g.new_game();
    g
}

fn placed(l: &Loaded, base: &str) -> FormId {
    let base = l.data.by_edid(base).expect("base form");
    l.refs.refs.iter().find(|(_, p)| p.base == base).map(|(id, _)| *id).expect("a placed reference")
}

fn quest_of(l: &Loaded, info: FormId) -> String {
    l.data.infos[&info].quest.map_or("none".into(), |q| l.data.name(q))
}

#[test]
fn combat_lines() {
    let Some(dir) = data_dir() else { return };
    let l = load(&dir);
    the_player_grunts_when_hit_and_on_a_power_attack(&l);
    a_brahmin_says_its_hit_sound(&l);
    sunny_says_her_death_line_and_it_is_not_cut(&l);
    a_companion_knocked_down_says_knocked_back(&l);
    a_companion_whose_shot_dt_stops_says_weapon_ineffective(&l);
    a_companion_poisoned_says_poisoned(&l);
}

/// The companions whose lines the VNPCFollowers quest holds.
const COMPANIONS: [&str; 6] = ["CraigBoone", "CassREF", "VeronicaREF", "ArcadeGannonREF", "RaulREF", "LilyREF"];

fn companions(l: &Loaded) -> Vec<FormId> {
    let mut out: Vec<FormId> = COMPANIONS.iter().filter_map(|e| l.data.by_edid(e)).collect();
    if let Some(b) = l.data.by_edid("CraigBoone") {
        out.extend(l.refs.refs.iter().filter(|(_, p)| p.base == b).map(|(id, _)| *id));
    }
    out.retain(|r| l.refs.refs.contains_key(r));
    out
}

/// The topic `topic`'s line said by one of the companions under `act`, and by whom.
fn said_by_a_companion(l: &Loaded, topic: &str, act: impl Fn(&mut Game, FormId)) -> (FormId, FormId) {
    for c in companions(l) {
        let mut g = game(l);
        act(&mut g, c);
        if let Some(line) = g.combat_lines.said.iter().find(|s| s.0 == topic && s.1 == c).and_then(|s| s.2) {
            return (c, line);
        }
    }
    panic!("no companion said {topic}");
}

/// KnockedBack: "Said when an actor has been knocked down, but isn't unconscious"
/// (Combat Tab); the masters' lines are the companions'.
fn a_companion_knocked_down_says_knocked_back(l: &Loaded) {
    let (_, line) = said_by_a_companion(l, "KnockedBack", |g, c| {
        g.knock_out(c, fnv_game::unconscious::Cause::KnockedDown, Some(FormId::PLAYER_REF));
    });
    assert_eq!(quest_of(l, line), "VNPCFollowers");
}

/// WeaponIneffective: the companion's hit was cut to DT's floor; once a minute at most.
fn a_companion_whose_shot_dt_stops_says_weapon_ineffective(l: &Loaded) {
    let target = placed(l, "GSSunnySmiles");
    let (c, line) = said_by_a_companion(l, "WeaponIneffective", |g, c| g.armour_lines(Some(c), target, 10.0, 30.0, 2.0, 0.2));
    assert_eq!(quest_of(l, line), "VNPCFollowers");
    let mut g = game(l);
    g.armour_lines(Some(c), target, 10.0, 30.0, 2.0, 0.2);
    g.speech.clear();
    g.armour_lines(Some(c), target, 10.0, 30.0, 2.0, 0.2);
    assert_eq!(g.combat_lines.said.iter().filter(|s| s.0 == "WeaponIneffective").count(), 1, "once a minute");
    // A hit DT did not stop says nothing.
    let mut g = game(l);
    g.armour_lines(Some(c), target, 30.0, 5.0, 25.0, 0.2);
    assert!(g.combat_lines.said.is_empty());
}

/// Poisoned: an effect item whose base effect Poison Resistance resists, applied by a hit.
fn a_companion_poisoned_says_poisoned(l: &Loaded) {
    let poison = l
        .data
        .effect_items
        .iter()
        .filter(|(_, e)| e.iter().any(|x| l.data.magic_effects.get(&x.effect).is_some_and(|m| m.resistance == Some(fnv_game::actor_values::POISON_RESISTANCE))))
        .map(|(id, _)| *id)
        .min()
        .expect("a poison in the masters");
    let (_, line) = said_by_a_companion(l, "Poisoned", |g, c| {
        g.apply_hit_effect(c, poison, None);
    });
    assert_eq!(quest_of(l, line), "VNPCFollowers");
}

/// The player's lines are the GenericPlayer quest's (voice type PlayerVoiceMale).
fn the_player_grunts_when_hit_and_on_a_power_attack(l: &Loaded) {
    let mut g = game(l);
    let r = g.hit(FormId::PLAYER_REF, 20.0, HitInfo { location: -1, ..HitInfo::default() });
    assert!(r.dealt > 0.0 && !r.died);
    let said = std::mem::take(&mut g.combat_lines.said);
    let (topic, who, line) = said.first().copied().expect("a line tried");
    assert_eq!((topic, who), ("Hit", FormId::PLAYER_REF));
    let line = line.expect("the player has Hit lines");
    assert_eq!(quest_of(l, line), "GenericPlayer");
    // Talking: a second hit says nothing, and an attack waits.
    g.hit(FormId::PLAYER_REF, 20.0, HitInfo { location: -1, ..HitInfo::default() });
    assert!(g.combat_lines.said.is_empty());

    let mut g = game(l);
    let line = g.attack_line(FormId::PLAYER_REF, None, true, false).expect("a PowerAttack line");
    assert_eq!(quest_of(l, line), "GenericPlayer");
    assert_eq!(l.data.infos[&line].topic, l.data.by_edid("PowerAttack"));
    // The attack timer holds the next one back, whether the line has ended or not.
    g.speech.clear();
    assert_eq!(g.attack_line(FormId::PLAYER_REF, None, true, false), None);
    assert_eq!(g.combat_lines.said.len(), 1);
}

/// A brahmin's Hit line has no voice: its response is a sound (NPCBrahminInjured). The
/// brahmins are those of the placed creatures whose base's Editor ID names one.
fn a_brahmin_says_its_hit_sound(l: &Loaded) {
    let mut brahmins: Vec<FormId> = l.refs.refs.iter().filter(|(_, p)| l.data.name(p.base).to_ascii_lowercase().contains("brahmin")).map(|(id, _)| *id).collect();
    brahmins.sort();
    let mut tried = Vec::new();
    for brahmin in brahmins {
        let mut g = game(l);
        // A hit says Hit only on a 1 % roll for creatures (combat-dialogue.md §1, since
        // 2026-10-17): the line is asked for directly.
        g.combat_line(brahmin, Some(FormId::PLAYER_REF), "Hit");
        let Some(line) = g.combat_lines.said.iter().find(|s| s.0 == "Hit" && s.1 == brahmin).and_then(|s| s.2) else {
            tried.push(l.data.name(l.refs.refs[&brahmin].base));
            continue;
        };
        let sound = l.data.infos[&line].responses.first().and_then(|r| r.sound).expect("a response sound");
        assert_eq!(l.data.name(sound), "NPCBrahminInjured");
        return;
    }
    panic!("no placed brahmin says a Hit line: {tried:?}");
}

/// Death is said by the dying and goes on although its speaker is dead.
fn sunny_says_her_death_line_and_it_is_not_cut(l: &Loaded) {
    let mut g = game(l);
    let sunny = placed(l, "GSSunnySmiles");
    assert!(g.kill(sunny, Some(FormId::PLAYER_REF)));
    let line = g.combat_lines.said.iter().find(|s| s.0 == "Death" && s.1 == sunny).and_then(|s| s.2).expect("Sunny's Death line");
    assert_eq!(quest_of(l, line), "VFreeformGoodsprings");
    g.tick();
    assert!(g.speech.iter().any(|s| s.speaker == sunny && s.info == line), "the Death line goes on");
}
