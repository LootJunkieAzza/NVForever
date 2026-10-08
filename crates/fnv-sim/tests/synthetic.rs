//! Layer 4 behaviour on a synthetic plugin built in memory and decoded through the
//! repository schemas (no game data).

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_sim::actions::{self, Script};
use fnv_sim::forms::{BaseData, Entry, ItemKind};
use fnv_sim::world::{Outcome, SimError};
use fnv_sim::{leveled, FormId, FormStore, RefTable, SimConfig, World};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

// ---- plugin builders ------------------------------------------------------------

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    cat(&[
        sig,
        &(payload.len() as u32).to_le_bytes(),
        &flags.to_le_bytes(),
        &fid.to_le_bytes(),
        &[0; 4],
        &15u16.to_le_bytes(),
        &[0; 2],
        &payload,
    ])
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    cat(&[b"GRUP", &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body])
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

fn tes4() -> Vec<u8> {
    let hedr = cat(&[&1.34f32.to_le_bytes(), &0i32.to_le_bytes(), &0x800u32.to_le_bytes()]);
    record(b"TES4", 1, 0, &[sub(b"HEDR", hedr)])
}

fn value_weight(value: i32, weight: f32) -> Vec<u8> {
    cat(&[&value.to_le_bytes(), &weight.to_le_bytes()])
}

fn cnto(item: u32, count: i32) -> Subrecord {
    sub(b"CNTO", cat(&[&item.to_le_bytes(), &count.to_le_bytes()]))
}

fn lvlo(level: u16, item: u32, count: u16) -> Subrecord {
    sub(b"LVLO", cat(&[&level.to_le_bytes(), &[0; 2], &item.to_le_bytes(), &count.to_le_bytes(), &[0; 2]]))
}

fn posrot(p: [f32; 3], r: [f32; 3]) -> Vec<u8> {
    p.iter().chain(r.iter()).flat_map(|f| f.to_le_bytes()).collect()
}

fn refr(fid: u32, flags: u32, base: u32, pos: [f32; 3], extra: Vec<Subrecord>) -> Vec<u8> {
    let mut subs = vec![sub(b"NAME", base.to_le_bytes().to_vec())];
    subs.extend(extra);
    subs.push(sub(b"DATA", posrot(pos, [0.0; 3])));
    record(b"REFR", flags, fid, &subs)
}

fn xloc(level: u8, key: u32) -> Subrecord {
    sub(b"XLOC", cat(&[&[level, 0, 0, 0], &key.to_le_bytes(), &[0; 4]]))
}

fn xtel(door: u32, pos: [f32; 3]) -> Subrecord {
    sub(b"XTEL", cat(&[&door.to_le_bytes(), &posrot(pos, [0.0, 0.0, 1.5]), &0u32.to_le_bytes()]))
}

fn cell_children(cell: u32, temp: &[u8], persistent: &[u8]) -> Vec<u8> {
    let label = cell.to_le_bytes();
    let mut body = Vec::new();
    if !persistent.is_empty() {
        body.extend(group(label, 8, persistent));
    }
    body.extend(group(label, 9, temp));
    group(label, 6, &body)
}

// FormIDs of the synthetic plugin (a master with no masters: top byte 00).
const STIMPAK: u32 = 0x100; // MISC, value 25, weight 0.5
const CAPS: u32 = 0x101; // MISC, value 1, weight 0
const KEY: u32 = 0x102; // KEYM
const CHAIR: u32 = 0x103; // FURN
const LL_LOOT: u32 = 0x110; // LVLI: level 1 -> STIMPAK x2, level 5 -> CAPS x10
const LL_NONE: u32 = 0x111; // LVLI: chance none 100
const LL_LOOP: u32 = 0x112; // LVLI naming itself
const SAFE: u32 = 0x120; // CONT: CAPS x3, LL_LOOT x1, count-0 entry
const DOOR: u32 = 0x130;
const CELL_A: u32 = 0x200;
const CELL_B: u32 = 0x201;
const SAFE_REF: u32 = 0x300; // locked (level 50, KEY)
const STIM_REF: u32 = 0x301; // XCNT 3
const DOOR_A: u32 = 0x302; // -> DOOR_B
const DOOR_B: u32 = 0x303; // -> DOOR_A, in CELL_B
const PLAIN_DOOR: u32 = 0x304;
const KEY_REF: u32 = 0x305; // persistent, initially disabled
const CHAIR_REF: u32 = 0x306;

fn plugin_bytes() -> Vec<u8> {
    let misc = cat(&[
        &record(b"MISC", 0, STIMPAK, &[sub(b"EDID", z("TestStim")), sub(b"DATA", value_weight(25, 0.5))]),
        &record(b"MISC", 0, CAPS, &[sub(b"EDID", z("TestCaps")), sub(b"DATA", value_weight(1, 0.0))]),
    ]);
    let keym = record(b"KEYM", 0, KEY, &[sub(b"EDID", z("TestKey")), sub(b"DATA", value_weight(0, 0.0))]);
    let furn = record(b"FURN", 0, CHAIR, &[sub(b"EDID", z("TestChair"))]);
    let lvli = cat(&[
        &record(
            b"LVLI",
            0,
            LL_LOOT,
            &[sub(b"EDID", z("TestLoot")), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0]), lvlo(1, STIMPAK, 2), lvlo(5, CAPS, 10)],
        ),
        &record(b"LVLI", 0, LL_NONE, &[sub(b"EDID", z("TestNothing")), sub(b"LVLD", vec![100]), lvlo(1, STIMPAK, 1)]),
        &record(b"LVLI", 0, LL_LOOP, &[sub(b"EDID", z("TestLoop")), sub(b"LVLD", vec![0]), lvlo(1, LL_LOOP, 1)]),
    ]);
    let cont = record(
        b"CONT",
        0,
        SAFE,
        &[
            sub(b"EDID", z("TestSafe")),
            sub(b"FULL", z("Safe")),
            cnto(CAPS, 3),
            cnto(LL_LOOT, 1),
            cnto(STIMPAK, 0),
            sub(b"DATA", cat(&[&[2u8], &10f32.to_le_bytes()])),
        ],
    );
    let door = record(b"DOOR", 0, DOOR, &[sub(b"EDID", z("TestDoor")), sub(b"FNAM", vec![0])]);
    let npc = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), cnto(CAPS, 5)]);
    let cell_a = record(b"CELL", 0, CELL_A, &[sub(b"EDID", z("TestCellA"))]);
    let cell_b = record(b"CELL", 0, CELL_B, &[sub(b"EDID", z("TestCellB"))]);
    let temp_a = cat(&[
        &refr(SAFE_REF, 0, SAFE, [1.0, 2.0, 3.0], vec![sub(b"EDID", z("TestSafeRef")), xloc(50, KEY)]),
        &refr(STIM_REF, 0, STIMPAK, [4.0, 5.0, 6.0], vec![sub(b"XCNT", 3i32.to_le_bytes().to_vec())]),
        &refr(DOOR_A, 0, DOOR, [0.0; 3], vec![xtel(DOOR_B, [100.0, 0.0, 0.0])]),
        &refr(PLAIN_DOOR, 0, DOOR, [0.0; 3], vec![]),
        &refr(CHAIR_REF, 0, CHAIR, [0.0; 3], vec![]),
    ]);
    let pers_a = refr(KEY_REF, 0x400 | 0x800, KEY, [7.0, 8.0, 9.0], vec![sub(b"EDID", z("TestKeyRef"))]);
    let temp_b = refr(DOOR_B, 0, DOOR, [0.0; 3], vec![xtel(DOOR_A, [-100.0, 0.0, 0.0])]);
    let cells = top(
        b"CELL",
        &group(
            0i32.to_le_bytes(),
            2,
            &group(
                0i32.to_le_bytes(),
                3,
                &cat(&[&cell_a, &cell_children(CELL_A, &temp_a, &pers_a), &cell_b, &cell_children(CELL_B, &temp_b, &[])]),
            ),
        ),
    );
    let glob = record(b"GLOB", 0, 0x150, &[sub(b"EDID", z("TestGlobal")), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 42f32.to_le_bytes().to_vec())]);
    cat(&[
        &tes4(),
        &top(b"GLOB", &glob),
        &top(b"MISC", &misc),
        &top(b"KEYM", &keym),
        &top(b"FURN", &furn),
        &top(b"LVLI", &lvli),
        &top(b"CONT", &cont),
        &top(b"DOOR", &door),
        &top(b"NPC_", &npc),
        &cells,
    ])
}

fn schema() -> Schema {
    Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap()
}

struct Fixture {
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let s = schema();
    Fixture { forms: FormStore::build(&lo, &s), refs: RefTable::build(&lo, &s) }
}

fn id(x: u32) -> FormId {
    FormId(x)
}

// ---- loaders --------------------------------------------------------------------

#[test]
fn forms_load_through_the_schema() {
    let f = fixture();
    assert!(f.forms.issues.is_empty(), "{:?}", f.forms.issues.to_json());
    let stim = f.forms.item(id(STIMPAK)).unwrap();
    assert_eq!((stim.kind, stim.value, stim.weight), (ItemKind::Misc, Some(25), Some(0.5)));
    assert_eq!(f.forms.by_edid("teststim"), Some(id(STIMPAK)));
    assert_eq!(f.forms.globals[&id(0x150)], 42.0);
    let BaseData::Container(c) = &f.forms.get(id(SAFE)).unwrap().data else { panic!() };
    assert!(c.respawns);
    assert_eq!(
        c.entries,
        vec![Entry { item: id(CAPS), count: 3 }, Entry { item: id(LL_LOOT), count: 1 }, Entry { item: id(STIMPAK), count: 0 }]
    );
    let l = f.forms.leveled(id(LL_LOOT)).unwrap();
    assert_eq!(l.entries.len(), 2);
    assert_eq!((l.entries[1].level, l.entries[1].item, l.entries[1].count), (5, id(CAPS), 10));
    assert_eq!(f.forms.leveled(id(LL_NONE)).unwrap().chance_none, 100);
    assert!(matches!(f.forms.get(id(0x7)).unwrap().data, BaseData::Actor(_)));
}

#[test]
fn references_load_with_cells_and_extra_data() {
    let f = fixture();
    assert!(f.refs.issues.is_empty(), "{:?}", f.refs.issues.to_json());
    assert_eq!(f.refs.refs.len(), 7);
    let safe = f.refs.get(id(SAFE_REF)).unwrap();
    assert_eq!((safe.base, safe.cell, safe.pos), (id(SAFE), Some(id(CELL_A)), [1.0, 2.0, 3.0]));
    assert_eq!(safe.lock.unwrap().key, Some(id(KEY)));
    assert_eq!(safe.lock.unwrap().level, 50);
    assert_eq!(f.refs.get(id(STIM_REF)).unwrap().count, Some(3));
    let a = f.refs.get(id(DOOR_A)).unwrap().teleport.unwrap();
    assert_eq!((a.door, a.pos, a.rot), (id(DOOR_B), [100.0, 0.0, 0.0], [0.0, 0.0, 1.5]));
    assert_eq!(f.refs.get(id(DOOR_B)).unwrap().cell, Some(id(CELL_B)));
    let key = f.refs.get(id(KEY_REF)).unwrap();
    assert!(key.in_persistent_group && key.initially_disabled());
    assert_eq!(f.refs.by_edid("TESTSAFEREF"), Some(id(SAFE_REF)));
}

// ---- runtime --------------------------------------------------------------------

#[test]
fn locked_container_needs_its_key() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig { seed: 1, player_level: 1 });
    let p = FormId::PLAYER_REF;
    assert_eq!(w.item_count(p, id(CAPS)).unwrap(), 5, "player starts with the base form's items");
    let r = w.activate(p, id(SAFE_REF)).unwrap();
    assert_eq!(r.outcome, Outcome::Locked { level: 50 });
    w.add_item(p, id(KEY), 1).unwrap();
    let r = w.activate(p, id(SAFE_REF)).unwrap();
    assert_eq!(r.outcome, Outcome::OpenedContainer { container: id(SAFE_REF) });
    assert_eq!(r.unlocked_with, Some(id(KEY)));
    // Player level 1: the leveled list can only give the level-1 entry (2 stimpaks);
    // the count-0 entry adds nothing.
    assert_eq!(w.item_count(id(SAFE_REF), id(CAPS)).unwrap(), 3);
    assert_eq!(w.item_count(id(SAFE_REF), id(STIMPAK)).unwrap(), 2);
    let moved = w.transfer_all(id(SAFE_REF), p).unwrap();
    assert_eq!(moved, vec![(id(STIMPAK), 2), (id(CAPS), 3)]);
    assert_eq!(w.item_count(p, id(CAPS)).unwrap(), 8);
    assert!(w.peek(id(SAFE_REF)).unwrap().inventory.as_ref().unwrap().is_empty());
}

#[test]
fn picking_up_and_dropping_items() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    let r = w.activate(p, id(STIM_REF)).unwrap();
    assert_eq!(r.outcome, Outcome::PickedUp { item: id(STIMPAK), count: 3, owner: None });
    assert_eq!(w.item_count(p, id(STIMPAK)).unwrap(), 3);
    assert_eq!(w.activate(p, id(STIM_REF)).unwrap().outcome, Outcome::Disabled);
    let dropped = w.drop_item(p, id(STIMPAK), 2).unwrap().unwrap();
    assert_eq!(dropped, FormId::FIRST_DYNAMIC);
    assert_eq!(w.item_count(p, id(STIMPAK)).unwrap(), 1);
    assert_eq!(w.activate(p, dropped).unwrap().outcome, Outcome::PickedUp { item: id(STIMPAK), count: 2, owner: None });
    assert_eq!(w.item_count(p, id(STIMPAK)).unwrap(), 3);
    // The initially disabled key cannot be taken until enabled.
    assert_eq!(w.activate(p, id(KEY_REF)).unwrap().outcome, Outcome::Disabled);
    w.set_enabled(id(KEY_REF), true).unwrap();
    assert!(matches!(w.activate(p, id(KEY_REF)).unwrap().outcome, Outcome::PickedUp { .. }));
}

#[test]
fn doors_teleport_or_toggle() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    let r = w.activate(p, id(DOOR_A)).unwrap();
    assert_eq!(
        r.outcome,
        Outcome::Teleported { to_door: id(DOOR_B), cell: Some(id(CELL_B)), pos: [100.0, 0.0, 0.0], rot: [0.0, 0.0, 1.5] }
    );
    let ps = w.peek(p).unwrap();
    assert_eq!((ps.cell, ps.pos), (Some(id(CELL_B)), [100.0, 0.0, 0.0]));
    w.activate(p, id(DOOR_B)).unwrap();
    assert_eq!(w.peek(p).unwrap().cell, Some(id(CELL_A)));
    assert_eq!(w.activate(p, id(PLAIN_DOOR)).unwrap().outcome, Outcome::DoorToggled { open: true });
    assert_eq!(w.activate(p, id(PLAIN_DOOR)).unwrap().outcome, Outcome::DoorToggled { open: false });
    w.lock(id(PLAIN_DOOR), Some(25)).unwrap();
    assert_eq!(w.activate(p, id(PLAIN_DOOR)).unwrap().outcome, Outcome::Locked { level: 25 });
    w.unlock(id(PLAIN_DOOR)).unwrap();
    assert!(matches!(w.activate(p, id(PLAIN_DOOR)).unwrap().outcome, Outcome::DoorToggled { .. }));
    assert_eq!(w.activate(p, id(CHAIR_REF)).unwrap().outcome, Outcome::Activated);
}

#[test]
fn errors_are_reported_not_panics() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    assert_eq!(w.activate(p, id(0xDEAD)), Err(SimError::UnknownRef(id(0xDEAD))));
    assert_eq!(w.add_item(p, id(DOOR), 1), Err(SimError::NotAnItem(id(DOOR))));
    assert_eq!(w.add_item(p, id(0xBEEF), 1), Err(SimError::UnknownForm(id(0xBEEF))));
    assert_eq!(w.add_item(p, id(CAPS), 0), Err(SimError::BadCount(0)));
    assert_eq!(w.add_item(id(DOOR_A), id(CAPS), 1), Err(SimError::NoInventory(id(DOOR_A))));
    assert!(matches!(w.add_item(p, id(LL_LOOP), 1), Err(SimError::Leveled(_))));
    assert_eq!(w.remove_item(p, id(STIMPAK), 4).unwrap(), 0);
}

// ---- leveled lists --------------------------------------------------------------

#[test]
fn leveled_lists_follow_level_and_chance_none() {
    let f = fixture();
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    for _ in 0..200 {
        let mut out = Vec::new();
        leveled::resolve(&f.forms, id(LL_NONE), 1, 99, &mut rng, &mut out).unwrap();
        assert!(out.is_empty(), "chance none 100 never yields");
        let mut out = Vec::new();
        leveled::resolve(&f.forms, id(LL_LOOT), 2, 10, &mut rng, &mut out).unwrap();
        // Without "all levels" only the highest eligible level (5) is chosen; one roll
        // multiplied by the requested count.
        assert_eq!(out, vec![Entry { item: id(CAPS), count: 20 }]);
    }
}

#[test]
fn below_is_uniform_enough() {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    let mut hist = [0u32; 7];
    for _ in 0..70_000 {
        hist[leveled::below(&mut rng, 7) as usize] += 1;
    }
    for h in hist {
        assert!((9_400..10_600).contains(&h), "{hist:?}");
    }
}

// ---- determinism, snapshots, action scripts --------------------------------------

const SCRIPT: &str = r#"{
  "format": "fnv.sim.actions/1", "seed": 3, "player_level": 1,
  "actions": [
    {"op": "activate", "actor": "player", "target": "TestSafeRef"},
    {"op": "add_item", "to": "player", "item": "TestKey", "count": 1},
    {"op": "activate", "actor": "player", "target": "TestSafeRef"},
    {"op": "transfer", "from": "TestSafeRef", "to": "player", "item": "TestCaps", "count": 2},
    {"op": "activate", "actor": "player", "target": "0x00000301"},
    {"op": "remove_item", "from": "player", "item": "NoSuchThing", "count": 1},
    {"op": "get_item_count", "of": "player", "item": "TestStim", "label": "stims"},
    {"op": "activate", "actor": "player", "target": "0x00000302"},
    {"op": "drop_item", "from": "player", "item": "TestStim", "count": 1},
    {"op": "get_pos", "of": "player"},
    {"op": "remove_all_items", "from": "TestSafeRef"},
    {"op": "get_item_count", "of": "TestSafeRef", "item": "TestStim", "label": "safe stims"}
  ],
  "report": ["player", "TestSafeRef"]
}"#;

fn run_script(f: &Fixture) -> serde_json::Value {
    let s: Script = serde_json::from_str(SCRIPT).unwrap();
    let mut w = World::new(&f.forms, &f.refs, actions::config(&s));
    actions::run(&mut w, &s).unwrap()
}

#[test]
fn action_script_trace() {
    let f = fixture();
    let t = run_script(&f);
    assert_eq!(t["format"], "fnv.sim.trace/1");
    let steps = t["steps"].as_array().unwrap();
    assert_eq!(steps[0]["result"]["result"], "locked");
    assert_eq!(steps[2]["result"]["unlocked_with"], "0x00000102");
    assert_eq!(steps[3]["result"]["moved"], 2);
    assert_eq!(steps[4]["result"]["result"], "picked_up");
    assert!(steps[5]["error"].as_str().unwrap().contains("NoSuchThing"));
    assert_eq!(steps[6]["result"]["count"], 3);
    assert_eq!(steps[7]["result"]["cell"], "0x00000201");
    assert_eq!(steps[8]["result"]["reference"], "0xFF000000");
    let obs = t["observations"].as_array().unwrap();
    assert_eq!(obs.len(), 3);
    assert_eq!((obs[0]["label"].as_str(), obs[0]["count"].as_i64()), (Some("stims"), Some(3)));
    assert_eq!((obs[1]["label"].as_str(), obs[1]["cell"].as_str()), (Some("step9"), Some("0x00000201")));
    assert_eq!(obs[1]["pos"], serde_json::json!([100.0, 0.0, 0.0]));
    assert_eq!(obs[2]["count"], 0);
    assert_eq!(steps[10]["result"]["removed"], serde_json::json!([["0x00000100", 2], ["0x00000101", 1]]));
    let player = &t["final"]["0x00000014"];
    assert_eq!(player["inventory"]["0x00000101"], 7);
    assert_eq!(player["inventory"]["0x00000102"], 1);
    assert_eq!(player["inventory"]["0x00000100"], 2);
    assert_eq!(t["final"]["0x00000300"]["lock"]["locked"], false);
    // Same input, same output.
    assert_eq!(run_script(&f), t);
}

#[test]
fn snapshot_resumes_identically() {
    let f = fixture();
    let cfg = SimConfig { seed: 99, player_level: 5 };
    let ops = |w: &mut World| {
        let p = FormId::PLAYER_REF;
        for _ in 0..20 {
            w.add_item(p, id(LL_LOOT), 1).unwrap();
        }
    };
    let mut full = World::new(&f.forms, &f.refs, cfg);
    ops(&mut full);
    ops(&mut full);

    let mut first = World::new(&f.forms, &f.refs, cfg);
    ops(&mut first);
    let text = serde_json::to_string(&first.snapshot()).unwrap();
    let snap = serde_json::from_str(&text).unwrap();
    let mut resumed = World::restore(&f.forms, &f.refs, &snap).unwrap();
    ops(&mut resumed);
    assert_eq!(serde_json::to_value(resumed.snapshot()).unwrap(), serde_json::to_value(full.snapshot()).unwrap());

    let mut bad = snap.clone();
    bad.format = "other".into();
    assert!(World::restore(&f.forms, &f.refs, &bad).is_err());
}
