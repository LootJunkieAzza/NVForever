//! Dialogue checks against the game's own masters (docs/spec-notes/dialogue.md §2, §9).
//! They need the game data, which is never in the repository: set `FNVLAB_DATA_DIR` (the
//! parity lab's `data_dir`, parity-lab/lab.json) to the game's `Data` folder. Without it,
//! or without the ten masters there, each test prints why it was skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::data::{info_flag, topic_type};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable};
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

/// The game's Data folder, or `None` (with the reason printed) when the checks cannot run.
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

/// One load for all checks (the build takes a few seconds).
#[test]
fn the_masters_dialogue() {
    let Some(dir) = data_dir() else { return };
    let l = load(&dir);
    counts(&l.data);
    doc_mitchells_lines(&l.data);
    the_intro_walk_through_talks_its_way_to_the_end(&l);
}

/// The counts of docs/spec-notes/dialogue.md §2 [observed: these masters].
fn counts(d: &GameData) {
    assert_eq!((d.topic_defs.len(), d.infos.len()), (21_298, 28_896));
    let greeting = d.greeting.expect("GREETING");
    assert_eq!((d.edid_of[&greeting].as_str(), d.topics[&greeting].len()), ("GREETING", 5_579));
    assert_eq!(d.edid_of[&d.goodbye.expect("GOODBYE")], "GOODBYE");
    assert_eq!(d.top_level.len(), 2_536);
    assert_eq!(d.topic_defs.values().filter(|t| t.topic_type == topic_type::TOPIC).count(), 19_285);
    // Every line sits under a topic and names a quest; every topic of a line is read.
    assert!(d.infos.values().all(|i| i.quest.is_some() && i.topic.is_some_and(|t| d.topic_defs.contains_key(&t))));
    let with = |flag: u16| d.infos.values().filter(|i| i.has(flag)).count();
    assert_eq!((with(info_flag::GOODBYE), with(info_flag::SAY_ONCE), with(info_flag::RANDOM)), (9_828, 4_434, 8_001));
    assert_eq!((with(info_flag::RUN_IMMEDIATELY), with(info_flag::SAY_ONCE_A_DAY), with(info_flag::SPEECH_CHALLENGE)), (765, 194, 3));
    assert_eq!(d.infos.values().map(|i| i.responses.len()).sum::<usize>(), 37_403);
    assert!(d.infos.values().all(|i| i.responses.len() == i.response_chars.len()));
    assert_eq!(d.infos.values().filter(|i| !i.choices.is_empty()).count(), 8_114);
    assert_eq!(d.infos.values().filter(|i| !i.follow_ups.is_empty()).count(), 1_759);
    // Shared lines (dialogue.md §1.2): player lines with choices, and those whose choices
    // all lack lines, counting a topic's own lines only and then its shared ones too.
    let player: Vec<_> = d.infos.values().filter(|i| i.info_type == topic_type::TOPIC && !i.choices.is_empty()).collect();
    let dead = |lines: &dyn Fn(FormId) -> Vec<FormId>| player.iter().filter(|i| i.choices.iter().all(|&c| lines(c).is_empty())).count();
    assert_eq!((player.len(), dead(&|t| d.lines_by_priority(t)), dead(&|t| d.lines_tried(t))), (7_325, 257, 7));
    // Red Lucy's "What's this place?" has no line of its own and shares the answer of
    // the topic of the same name.
    let lucy = d.by_edid("VMS38VMS38RedLucyTopic047").unwrap();
    assert_eq!((d.lines_by_priority(lucy), d.lines_tried(lucy)), (vec![], vec![FormId(0x0013_4FC1)]));
    assert_eq!(d.infos[&FormId(0x0013_4FC1)].topic, d.by_edid("VMS38VMS38RedLucyTopic004"));
    // GREETING shares five lines; four of VMS07Greeting go in at indices 11–14.
    let greeting_tried = d.lines_tried(greeting);
    assert_eq!(greeting_tried.len(), 5_579 + 5);
    assert_eq!(d.shared_lines[&greeting][11..15], [0x000F_378E, 0x000F_378F, 0x000F_3791, 0x000F_479B].map(FormId));
    assert_eq!(d.infos.values().filter(|i| i.challenge_form.is_some()).count(), 1_499);
    // Quests whose own conditions gate their lines (dialogue.md §4).
    let gated: Vec<FormId> = d.quests.values().filter(|q| !q.conditions.is_empty()).map(|q| q.form).collect();
    assert_eq!((d.quests.len(), gated.len()), (640, 113));
    assert_eq!(d.infos.values().filter(|i| i.quest.is_some_and(|q| gated.contains(&q))).count(), 8_151);
    // Run Immediately is set on Radio and Conversation lines only.
    assert!(d.infos.values().filter(|i| i.has(info_flag::RUN_IMMEDIATELY)).all(|i| i.info_type == topic_type::RADIO || i.info_type == topic_type::CONVERSATION));
    // Links point at records of the documented kinds.
    for i in d.infos.values() {
        assert!(i.choices.iter().chain(&i.add_topics).chain(&i.link_from).all(|t| d.topic_defs.contains_key(t)));
        assert!(i.follow_ups.iter().all(|f| d.infos.contains_key(f)));
    }
}

/// Doc Mitchell's psych test: the greeting that opens it offers six topics, and each
/// answer leads on [observed: FalloutNV.esm].
fn doc_mitchells_lines(d: &GameData) {
    let open = &d.infos[&FormId(0x0010_558F)];
    assert_eq!(open.topic, d.greeting);
    assert!(open.say_once && open.choices.len() == 6 && open.begin.is_some() && open.end.is_some());
    let farewell = &d.infos[&FormId(0x0010_57E8)];
    assert_eq!(farewell.follow_ups, vec![FormId(0x0010_57E4)]);
    assert_eq!(d.infos[&FormId(0x0010_57E4)].follow_ups.len(), 2);
}

/// The headless intro (docs/spec-notes/scripts.md §9) with the psych test and Doc's
/// farewell as conversations: no step fails and VCG01 ends at stage 200.
fn the_intro_walk_through_talks_its_way_to_the_end(l: &Loaded) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("walkthrough/intro.actions.json");
    let input: fnv_game::runner::Input = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let t = fnv_game::runner::run(&l.data, &l.forms, &l.refs, &input).unwrap();
    let failed: Vec<&Json> = t["steps"].as_array().unwrap().iter().filter(|s| !s["error"].is_null()).collect();
    assert!(failed.is_empty(), "{failed:?}");
    let obs = |label: &str| {
        t["observations"].as_array().unwrap().iter().find(|o| o["label"] == label).cloned().unwrap_or_else(|| panic!("no observation {label:?}"))
    };
    // The couch trigger's script started the conversation (StartConversation), and the
    // opening line's begin script ran.
    assert_eq!(obs("psych test opened by the couch trigger")["choices"], 6);
    assert_eq!(obs("psych test begun")["var.bBeganTest"], 1.0);
    assert_eq!(obs("psych test over")["state"], "ended");
    let tags = obs("tag skills after the psych test");
    assert_eq!((&tags["tag.Guns"], &tags["tag.Science"], &tags["tag.Sneak"], &tags["tag.Speech"]), (&Json::Bool(true), &Json::Bool(true), &Json::Bool(true), &Json::Bool(false)));
    assert_eq!(obs("Doc's farewell greeting")["info"], "0x001057E8");
    assert_eq!(obs("farewell over")["state"], "ended");
    let end = obs("VCG01 at the end");
    assert_eq!((&end["stage"], &end["completed"], &end["running"]), (&Json::from(200), &Json::Bool(true), &Json::Bool(false)));
    assert_eq!(obs("VCG02 after VCG01")["stage"], 5);
    // The two conversations: 15 and 5 lines, both ended by a Goodbye line.
    let ended: Vec<(&str, u64)> = t["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "dialogue_ended")
        .map(|e| (e["reason"].as_str().unwrap(), e["lines"].as_u64().unwrap()))
        .collect();
    assert_eq!(ended, vec![("goodbye", 15), ("goodbye", 5)]);
    // The idles asked of Doc Mitchell at the bedside (docs/spec-notes/idles.md §2): the
    // gestures named on his lines (VCG01DocWhoaThere, SitChairTalkPlayerD and A), the
    // script's `PlayIdle VCG01DocGiveMirror` at stage 30, and the same idle on the next
    // line. (The player's package idles need packages run, which the walk-through's
    // runner does not do.)
    let asked: Vec<(&str, &str, &str)> = t["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "idle_asked")
        .map(|e| (e["actor"].as_str().unwrap(), e["idle"].as_str().unwrap(), e["source"].as_str().unwrap()))
        .collect();
    assert!(asked.iter().all(|(actor, ..)| *actor == "0x00104C0F"), "{asked:?}");
    let bedside: Vec<(&str, &str)> = asked.iter().take(5).map(|(_, idle, source)| (*idle, *source)).collect();
    assert_eq!(
        bedside,
        vec![("0x0012280E", "speaker"), ("0x00024F9B", "speaker"), ("0x00024F98", "speaker"), ("0x0010658C", "script"), ("0x0010658C", "speaker")]
    );
}
