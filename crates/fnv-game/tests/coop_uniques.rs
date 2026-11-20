//! Unique quest rewards in co-op (`fnv_game::coop_uniques`; shared-progression plan
//! §4.1, P5): the list computed from the data, the hand-out filter for guests that
//! claimed no stage of the quest, and the item-81 undo keeping a claimer's uniques, on a
//! synthetic plugin built in memory (no game data). Scripts are compiled from source by
//! NVForever's compiler.

use crate::common::{group, record, sub, tes4, top, z};
use fnv_esm::raw::Subrecord;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::coop_joint::ClaimedStage;
use fnv_game::coop_rewards::Gift;
use fnv_game::coop_uniques::{compute, Kind};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::compile::{compile, Env, FormInfo};
use fnv_script::unit::RefSlot;
use fnv_script::{FunctionTable, VarDecl};
use fnv_sim::inventory::Extra;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const QA: u32 = 0x900;
const QB: u32 = 0x901;
const QB_SCRIPT: u32 = 0x910;
const LEVER_SCRIPT: u32 = 0x911;
const LEVER: u32 = 0x920;
/// Given by QA's stage 10 only.
const GUN: u32 = 0xA00;
/// Given by both quests.
const SHARED: u32 = 0xA01;
/// Given by QB, and in a vendor's container.
const VENDOR: u32 = 0xA02;
/// Given by QB, and in a leveled list.
const LEVELED: u32 = 0xA03;
/// Given by QB, and placed in a cell.
const PLACED: u32 = 0xA04;
/// Given by QB, and by a lever's script.
const SCRIPTED: u32 = 0xA05;
/// Given by QB's quest script only.
const QB_ITEM: u32 = 0xA06;
/// Given by a dialogue line of QB only.
const LINE_ITEM: u32 = 0xA07;
/// Sold for caps by a dialogue line (the clinic's implants).
const SOLD: u32 = 0xA08;
/// Handed out by a background quest's script (a preorder pack).
const PACK: u32 = 0xA09;
const CAPS: u32 = 0xF;
const QC: u32 = 0x902;
const QC_SCRIPT: u32 = 0x912;
const LINE2: u32 = 0xE02;
const QPERK: u32 = 0xB00;
const PLAYABLE_PERK: u32 = 0xB01;
const QSPELL: u32 = 0xC00;
const CONT: u32 = 0xD00;
const LIST: u32 = 0xD01;
const TOPIC: u32 = 0xE00;
const LINE: u32 = 0xE01;
const CELL: u32 = 0x200;
const PLACED_REF: u32 = 0x300;

const NAMES: &[(&str, u32, &[u8; 4])] = &[
    ("player", 0x14, b"REFR"),
    ("TQuestA", QA, b"QUST"),
    ("TQuestB", QB, b"QUST"),
    ("TUniqueGun", GUN, b"MISC"),
    ("TShared", SHARED, b"MISC"),
    ("TVendor", VENDOR, b"MISC"),
    ("TLeveled", LEVELED, b"MISC"),
    ("TPlaced", PLACED, b"MISC"),
    ("TScripted", SCRIPTED, b"MISC"),
    ("TQuestBItem", QB_ITEM, b"MISC"),
    ("TLineItem", LINE_ITEM, b"MISC"),
    ("TSold", SOLD, b"MISC"),
    ("TPack", PACK, b"MISC"),
    ("Caps001", CAPS, b"MISC"),
    ("TPackQuest", QC, b"QUST"),
    ("TQuestPerk", QPERK, b"PERK"),
    ("TPlayablePerk", PLAYABLE_PERK, b"PERK"),
    ("TQuestSpell", QSPELL, b"SPEL"),
];

struct Names;

impl Env for Names {
    fn form(&self, editor_id: &str) -> Option<FormInfo> {
        NAMES.iter().find(|(n, _, _)| n.eq_ignore_ascii_case(editor_id)).map(|(_, f, s)| FormInfo { form: *f, sig: **s })
    }
    fn info(&self, form: u32) -> Option<FormInfo> {
        NAMES.iter().find(|(_, f, _)| *f == form).map(|(_, f, s)| FormInfo { form: *f, sig: **s })
    }
    fn editor_id(&self, form: u32) -> Option<String> {
        NAMES.iter().find(|(_, f, _)| *f == form).map(|(n, _, _)| n.to_string())
    }
    fn local_form(&self, local: u32) -> Option<u32> {
        Some(local)
    }
    fn script_vars(&self, _form: u32) -> Option<Vec<VarDecl>> {
        None
    }
}

/// SCHR, SCDA, SCRO and SCTX of a unit compiled from `src` (`kind`: 0 object, 1 quest).
fn unit(table: &FunctionTable, kind: u16, src: &str) -> Vec<Subrecord> {
    let c = compile(src, table, &Names, None);
    assert!(c.errors().next().is_none(), "{src}: {:?}", c.diags);
    let refs: Vec<u32> = c.refs.iter().filter_map(|r| if let RefSlot::Form(f) = r { Some(*f) } else { None }).collect();
    let mut schr = vec![0u8; 4];
    schr.extend((refs.len() as u32).to_le_bytes());
    schr.extend((c.bytes.len() as u32).to_le_bytes());
    schr.extend(0u32.to_le_bytes());
    schr.extend(kind.to_le_bytes());
    schr.extend(1u16.to_le_bytes());
    let mut out = vec![sub(b"SCHR", schr), sub(b"SCDA", c.bytes.clone())];
    out.extend(refs.iter().map(|r| sub(b"SCRO", r.to_le_bytes().to_vec())));
    out.push(sub(b"SCTX", src.as_bytes().to_vec()));
    out
}

fn misc(fid: u32, edid: &str) -> Vec<u8> {
    record(b"MISC", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", [1i32.to_le_bytes(), 1f32.to_le_bytes()].concat())])
}

fn quest(table: &FunctionTable, fid: u32, edid: &str, script: Option<u32>, stages: &[(i16, &str)]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    if let Some(s) = script {
        subs.push(sub(b"SCRI", s.to_le_bytes().to_vec()));
    }
    subs.push(sub(b"DATA", [vec![0u8, 0, 0, 0], 5.0f32.to_le_bytes().to_vec()].concat()));
    for (s, src) in stages {
        subs.push(sub(b"INDX", s.to_le_bytes().to_vec()));
        subs.push(sub(b"QSDT", vec![0]));
        subs.extend(unit(table, 0, src));
    }
    record(b"QUST", 0, fid, &subs)
}

fn plugin(table: &FunctionTable) -> Vec<u8> {
    let scpt = [
        record(b"SCPT", 0, QB_SCRIPT, &[vec![sub(b"EDID", z("TQuestBScript"))], unit(table, 1, "scn TQuestBScript\nbegin GameMode\n  player.AddItem TQuestBItem 1\nend\n")].concat()),
        record(b"SCPT", 0, QC_SCRIPT, &[vec![sub(b"EDID", z("TPackScript"))], unit(table, 1, "scn TPackScript\nbegin GameMode\n  player.AddItem TPack 1\nend\n")].concat()),
        record(b"SCPT", 0, LEVER_SCRIPT, &[vec![sub(b"EDID", z("TLeverScript"))], unit(table, 0, "scn TLeverScript\nbegin OnActivate\n  player.AddItem TScripted 1\nend\n")].concat()),
    ]
    .concat();
    let items = [
        misc(GUN, "TUniqueGun"),
        misc(SHARED, "TShared"),
        misc(VENDOR, "TVendor"),
        misc(LEVELED, "TLeveled"),
        misc(PLACED, "TPlaced"),
        misc(SCRIPTED, "TScripted"),
        misc(QB_ITEM, "TQuestBItem"),
        misc(LINE_ITEM, "TLineItem"),
        misc(SOLD, "TSold"),
        misc(PACK, "TPack"),
        misc(CAPS, "Caps001"),
    ]
    .concat();
    // DATA: trait, minimum level, ranks, playable, hidden.
    let perks = [
        record(b"PERK", 0, QPERK, &[sub(b"EDID", z("TQuestPerk")), sub(b"DATA", vec![0, 1, 1, 0, 0])]),
        record(b"PERK", 0, PLAYABLE_PERK, &[sub(b"EDID", z("TPlayablePerk")), sub(b"DATA", vec![0, 1, 1, 1, 0])]),
    ]
    .concat();
    let spel = record(b"SPEL", 0, QSPELL, &[sub(b"EDID", z("TQuestSpell")), sub(b"SPIT", [4u32.to_le_bytes(), 0u32.to_le_bytes(), 0u32.to_le_bytes(), 0u32.to_le_bytes()].concat())]);
    let cont = record(
        b"CONT",
        0,
        CONT,
        &[sub(b"EDID", z("TVendorBox")), sub(b"CNTO", [VENDOR.to_le_bytes(), 1i32.to_le_bytes()].concat()), sub(b"DATA", [vec![0], 0f32.to_le_bytes().to_vec()].concat())],
    );
    let lvlo = sub(b"LVLO", [1i16.to_le_bytes().to_vec(), vec![0; 2], LEVELED.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat());
    let lvli = record(b"LVLI", 0, LIST, &[sub(b"EDID", z("TList")), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0]), lvlo]);
    let acti = record(b"ACTI", 0, LEVER, &[sub(b"EDID", z("TLever")), sub(b"SCRI", LEVER_SCRIPT.to_le_bytes().to_vec())]);
    let qust = [
        quest(
            table,
            QA,
            "TQuestA",
            None,
            &[(10, "player.AddItem TUniqueGun 1\nplayer.AddPerk TQuestPerk\nplayer.AddPerk TPlayablePerk\nplayer.AddSpell TQuestSpell\n"), (20, "player.AddItem TShared 1\n")],
        ),
        quest(
            table,
            QB,
            "TQuestB",
            Some(QB_SCRIPT),
            &[(10, "player.AddItem TShared 1\nplayer.AddItem TVendor 1\nplayer.AddItem TLeveled 1\nplayer.AddItemHealthPercent TPlaced 1 0.5\nplayer.AddItem TScripted 1\n")],
        ),
        // A background quest running from the game's start (DATA flags 0x01), no name,
        // stages or objectives, whose script hands out TPack.
        record(b"QUST", 0, QC, &[sub(b"EDID", z("TPackQuest")), sub(b"SCRI", QC_SCRIPT.to_le_bytes().to_vec()), sub(b"DATA", [vec![1u8, 0, 0, 0], 5.0f32.to_le_bytes().to_vec()].concat())]),
    ]
    .concat();
    // A line of TQuestB whose end script gives TLineItem.
    let trdt = [0u32.to_le_bytes().to_vec(), 50u32.to_le_bytes().to_vec(), vec![0; 4], vec![1, 0, 0, 0], 0u32.to_le_bytes().to_vec(), vec![0; 4]].concat();
    let mut info = vec![sub(b"DATA", vec![0, 0, 0, 0]), sub(b"QSTI", QB.to_le_bytes().to_vec()), sub(b"TRDT", trdt.clone()), sub(b"NAM1", z("Take this.")), sub(b"NAM2", z("")), sub(b"NAM3", z(""))];
    info.push(sub(b"NEXT", vec![]));
    info.extend(unit(table, 0, "player.AddItem TLineItem 1\n"));
    let dial_subs = [sub(b"EDID", z("TTopic")), sub(b"FULL", z("TTopic")), sub(b"QSTI", QB.to_le_bytes().to_vec()), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![0, 0])];
    // A second line of TQuestB that sells TSold for caps.
    let mut sale = vec![sub(b"DATA", vec![0, 0, 0, 0]), sub(b"QSTI", QB.to_le_bytes().to_vec()), sub(b"TRDT", trdt.clone()), sub(b"NAM1", z("Sold.")), sub(b"NAM2", z("")), sub(b"NAM3", z(""))];
    sale.push(sub(b"NEXT", vec![]));
    sale.extend(unit(table, 0, "player.RemoveItem Caps001 100\nplayer.AddItem TSold 1\n"));
    let lines = [record(b"INFO", 0, LINE, &info), record(b"INFO", 0, LINE2, &sale)].concat();
    let dial = [record(b"DIAL", 0, TOPIC, &dial_subs), group(TOPIC.to_le_bytes(), 7, &lines)].concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TCell"))]);
    let placed = record(b"REFR", 0, PLACED_REF, &[sub(b"NAME", PLACED.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &placed));
    // The player's base, so the player has an inventory.
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    [
        tes4(),
        top(b"NPC_", &player),
        top(b"SCPT", &scpt),
        top(b"MISC", &items),
        top(b"PERK", &perks),
        top(b"SPEL", &spel),
        top(b"CONT", &cont),
        top(b"LVLI", &lvli),
        top(b"ACTI", &acti),
        top(b"QUST", &qust),
        top(b"DIAL", &dial),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let bytes = plugin(&table);
    let plugins = vec![Plugin::parse(&bytes, "Uniques.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.new_game();
    g
}

fn claim(g: &mut Game, q: u32, stage: i32) {
    g.player.claims.stages.entry(FormId(q)).or_default().push(ClaimedStage { stage, line: None });
}

fn carried(g: &Game, item: u32) -> i32 {
    g.world.peek(FormId::PLAYER_REF).and_then(|r| r.inventory.as_ref()).map_or(0, |i| i.count(FormId(item)))
}

#[test]
fn only_what_one_quests_scripts_alone_give_is_unique() {
    let f = fixture();
    let u = compute(&f.data, &f.forms, &f.refs);
    let got: Vec<(u32, Kind, u32)> = u.by_form.iter().map(|(f, (k, q))| (f.0, *k, q.0)).collect();
    assert_eq!(
        got,
        vec![
            // A stage script's item, perk and spell.
            (GUN, Kind::Item, QA),
            // The quest script's item, and a dialogue line's of the quest.
            (QB_ITEM, Kind::Item, QB),
            (LINE_ITEM, Kind::Item, QB),
            (QPERK, Kind::Perk, QA),
            (QSPELL, Kind::Spell, QA),
        ],
        "not unique: given by two quests, a vendor's container, a leveled list, a placed reference, an object's script, a playable perk, \
         sold for caps, a background quest's hand-out"
    );
    assert!(f.data.infos.contains_key(&FormId(LINE2)) && f.data.quests.contains_key(&FormId(QC)), "the sale line and the pack quest are read");
    // The same list the game keeps, computed once.
    let g = game(&f);
    assert_eq!(g.coop_uniques(), &u);
    assert!(std::ptr::eq(g.coop_uniques(), g.coop_uniques()));
}

#[test]
fn a_unique_goes_only_to_a_guest_that_claimed_a_stage_of_its_quest() {
    let f = fixture();
    let mut g = game(&f);
    let gift = |item: u32, batch: u32| Gift { batch, item: FormId(item), count: 1, quest: Some(FormId(QA)), stage: Some(10) };
    let mut last = None;
    // Not claimed: the unique stays the host's; a shared hand-out still comes.
    assert!(!g.coop_unique_allowed(FormId(GUN)));
    assert_eq!(g.take_coop_gift(&gift(GUN, 1), &mut last), Ok(0));
    assert_eq!(carried(&g, GUN), 0);
    assert_eq!(g.take_coop_gift(&gift(SHARED, 1), &mut last), Ok(1));
    assert_eq!(carried(&g, SHARED), 1);
    // A claim of another quest does not count.
    claim(&mut g, QB, 10);
    assert_eq!(g.take_coop_gift(&gift(GUN, 2), &mut last), Ok(0));
    // A claimed stage of its quest: it comes.
    claim(&mut g, QA, 10);
    assert!(g.coop_unique_allowed(FormId(GUN)));
    assert_eq!(g.take_coop_gift(&gift(GUN, 3), &mut last), Ok(1));
    assert_eq!(carried(&g, GUN), 1);
}

#[test]
fn the_undo_keeps_a_claimers_uniques_and_takes_the_rest() {
    let f = fixture();
    let mut g = game(&f);
    // The host's story gave the guest the gun, the shared item and the quest's perk.
    let none = Extra::default();
    g.world.add_item(FormId::PLAYER_REF, FormId(GUN), 1).unwrap();
    g.world.add_item(FormId::PLAYER_REF, FormId(SHARED), 1).unwrap();
    g.coop_story.record_items(&[], &[(FormId(GUN), none, 1), (FormId(SHARED), none, 1)]);
    g.coop_story.record_perk(FormId(QPERK), 0);
    g.player.perks.insert(FormId(QPERK), 1);
    // Not a claimer: everything is undone in its own save.
    let mut s = g.snapshot();
    g.coop_story_to_undo().undo_on(&mut s);
    let inv = |s: &fnv_game::game::GameSnapshot, item: u32| s.sim.refs.get(&FormId::PLAYER_REF).and_then(|r| r.inventory.as_ref()).map_or(0, |i| i.count(FormId(item)));
    assert_eq!((inv(&s, GUN), inv(&s, SHARED)), (0, 0));
    assert!(!s.player.perks.contains_key(&FormId(QPERK)));
    // A claimer of the quest keeps its uniques; the shared item still goes.
    claim(&mut g, QA, 10);
    let mut s = g.snapshot();
    let undone = g.coop_story_to_undo().undo_on(&mut s);
    assert_eq!((inv(&s, GUN), inv(&s, SHARED)), (1, 0), "{undone:?}");
    assert_eq!(s.player.perks.get(&FormId(QPERK)), Some(&1));
    // The running game is not touched.
    assert_eq!(g.coop_story.items.len(), 2);
}
