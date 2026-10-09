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

fn xesp(parent: u32, opposite: bool) -> Subrecord {
    sub(b"XESP", cat(&[&parent.to_le_bytes(), &[opposite as u8, 0, 0, 0]]))
}

fn glob_rec(fid: u32, edid: &str, ty: u8, v: f32) -> Vec<u8> {
    record(b"GLOB", 0, fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![ty]), sub(b"FLTV", v.to_le_bytes().to_vec())])
}

fn weap(fid: u32, edid: &str, value: i32, weight: f32) -> Vec<u8> {
    let data = cat(&[&value.to_le_bytes(), &100i32.to_le_bytes(), &weight.to_le_bytes(), &10i16.to_le_bytes(), &[12u8]]);
    record(b"WEAP", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
}

fn armo(fid: u32, edid: &str, slots: u32, weight: f32) -> Vec<u8> {
    armo_flags(fid, edid, slots, 0, weight)
}

/// An ARMO with BMDT general flags (0x40: Non-Playable).
fn armo_flags(fid: u32, edid: &str, slots: u32, general: u8, weight: f32) -> Vec<u8> {
    let bmdt = cat(&[&slots.to_le_bytes(), &[general, 0, 0, 0]]);
    let data = cat(&[&10i32.to_le_bytes(), &50i32.to_le_bytes(), &weight.to_le_bytes()]);
    record(b"ARMO", 0, fid, &[sub(b"EDID", z(edid)), sub(b"BMDT", bmdt), sub(b"DATA", data)])
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
const LL_ODDS: u32 = 0x113; // LVLI: chance none 0, LVLG TestOdds (100), STIMPAK x1
const ODDS: u32 = 0x151; // GLOB TestOdds = 100
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
const CHILD_SAME: u32 = 0x307; // XESP -> KEY_REF (initially disabled)
const CHILD_OPPOSITE: u32 = 0x308; // XESP -> KEY_REF, opposite
const GRANDCHILD: u32 = 0x309; // XESP -> CHILD_OPPOSITE
const GUN_REF: u32 = 0x30A; // PISTOL with XHLP 0.25
const PISTOL: u32 = 0x170; // WEAP, weight 3
const RIFLE: u32 = 0x171; // WEAP, weight 8
const HELMET: u32 = 0x172; // ARMO, slots Head|Hair, weight 2
const HAT: u32 = 0x173; // ARMO, slot Hair, weight 1
const SUIT: u32 = 0x174; // ARMO, slot Upper Body, weight 15
const AID: u32 = 0x175; // ALCH, weight 0.5
const BULLETS: u32 = 0x176; // AMMO, weight 0.1 (Hardcore only)
const NOTE: u32 = 0x178; // NOTE: no weight field
const CHIP: u32 = 0x179; // CHIP: no weight field
const CMNY: u32 = 0x17A; // CMNY: no weight field
const CARD: u32 = 0x17B; // CCRD: value only
const GLOVE: u32 = 0x17C; // ARMO, slot Left Hand, Non-Playable, weight 0
const LOCKER: u32 = 0x121; // CONT: PISTOL x1 with COED condition 0.75
const MINE: u32 = 0x177; // WEAP firing MINE_PROJ
const MINE_PROJ: u32 = 0x180; // PROJ
const MINE_REF: u32 = 0x30B; // PGRE of MINE_PROJ
const WORLD: u32 = 0x400; // WRLD
const WORLD_PERSISTENT: u32 = 0x401; // the worldspace's persistent cell
const WORLD_CELL: u32 = 0x402; // exterior cell at grid (-18, 0)
const EXT_DOOR: u32 = 0x30C; // persistent door in WORLD_PERSISTENT
const IN_DOOR: u32 = 0x30D; // door in CELL_B -> EXT_DOOR, destination inside grid (-18, 0)
const WORLD2: u32 = 0x410; // a second WRLD, a child of WORLD (WNAM, PNAM "Use Map Data")
const WORLD2_PERSISTENT: u32 = 0x411;
const WORLD2_CELL: u32 = 0x412; // exterior cell at grid (2, 1)
const GATE_OUT: u32 = 0x30E; // persistent door in WORLD_PERSISTENT -> GATE_IN, in WORLD2
const GATE_IN: u32 = 0x30F; // persistent door in WORLD2_PERSISTENT -> GATE_OUT, in WORLD

fn plugin_bytes() -> Vec<u8> {
    let misc = cat(&[
        &record(b"MISC", 0, STIMPAK, &[sub(b"EDID", z("TestStim")), sub(b"DATA", value_weight(25, 0.5))]),
        &record(b"MISC", 0, CAPS, &[sub(b"EDID", z("TestCaps")), sub(b"DATA", value_weight(1, 0.0))]),
    ]);
    let weightless = [
        top(b"NOTE", &record(b"NOTE", 0, NOTE, &[sub(b"EDID", z("TestNote"))])),
        top(b"CHIP", &record(b"CHIP", 0, CHIP, &[sub(b"EDID", z("TestChip"))])),
        top(b"CCRD", &record(b"CCRD", 0, CARD, &[sub(b"EDID", z("TestCard")), sub(b"DATA", 2u32.to_le_bytes().to_vec())])),
        top(b"CMNY", &record(b"CMNY", 0, CMNY, &[sub(b"EDID", z("TestMoney"))])),
    ]
    .concat();
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
        &record(
            b"LVLI",
            0,
            LL_ODDS,
            &[sub(b"EDID", z("TestOddsList")), sub(b"LVLD", vec![0]), sub(b"LVLG", ODDS.to_le_bytes().to_vec()), lvlo(1, STIMPAK, 1)],
        ),
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
    // S.P.E.C.I.A.L. 5,6,7,4,3,2,1 after base health.
    let npc_data = cat(&[&100i32.to_le_bytes(), &[5u8, 6, 7, 4, 3, 2, 1]]);
    let npc = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), cnto(CAPS, 5), cnto(GLOVE, 1), sub(b"DATA", npc_data)]);
    let mut dnam = vec![0u8; 120]; // required part of WEAP DNAM; projectile at +36
    dnam[36..40].copy_from_slice(&MINE_PROJ.to_le_bytes());
    let mine = {
        let mut m = weap(MINE, "TestMine", 25, 0.5);
        let extra = write_subrecords(&[sub(b"DNAM", dnam)]);
        let size = u32::from_le_bytes(m[4..8].try_into().unwrap()) + extra.len() as u32;
        m[4..8].copy_from_slice(&size.to_le_bytes());
        m.extend(extra);
        m
    };
    let weaps = cat(&[&weap(PISTOL, "TestPistol", 100, 3.0), &weap(RIFLE, "TestRifle", 300, 8.0), &mine]);
    let proj = record(b"PROJ", 0, MINE_PROJ, &[sub(b"EDID", z("TestMineProjectile"))]);
    let armos = cat(&[&armo(HELMET, "TestHelmet", 0b11, 2.0), &armo(HAT, "TestHat", 0b10, 1.0), &armo(SUIT, "TestSuit", 0b100, 15.0), &armo_flags(GLOVE, "TestGlove", 0b1000, 0x40, 0.0)]);
    let enit = cat(&[&20i32.to_le_bytes(), &[0u8, 0, 0, 0], &0u32.to_le_bytes(), &0f32.to_le_bytes(), &0u32.to_le_bytes()]);
    let alch = record(b"ALCH", 0, AID, &[sub(b"EDID", z("TestAid")), sub(b"DATA", 0.5f32.to_le_bytes().to_vec()), sub(b"ENIT", enit)]);
    let ammo_data = cat(&[&1f32.to_le_bytes(), &[0u8, 0, 0, 0], &2i32.to_le_bytes(), &[10u8]]);
    let ammo_dat2 = cat(&[&1u32.to_le_bytes(), &0u32.to_le_bytes(), &0.1f32.to_le_bytes()]);
    let ammo = record(b"AMMO", 0, BULLETS, &[sub(b"EDID", z("TestBullets")), sub(b"DATA", ammo_data), sub(b"DAT2", ammo_dat2)]);
    let coed = cat(&[&0u32.to_le_bytes(), &0u32.to_le_bytes(), &0.75f32.to_le_bytes()]);
    let locker = record(
        b"CONT",
        0,
        LOCKER,
        &[sub(b"EDID", z("TestLocker")), cnto(PISTOL, 1), sub(b"COED", coed), sub(b"DATA", cat(&[&[0u8], &1f32.to_le_bytes()]))],
    );
    let cell_a = record(b"CELL", 0, CELL_A, &[sub(b"EDID", z("TestCellA"))]);
    let cell_b = record(b"CELL", 0, CELL_B, &[sub(b"EDID", z("TestCellB"))]);
    let temp_a = cat(&[
        &refr(SAFE_REF, 0, SAFE, [1.0, 2.0, 3.0], vec![sub(b"EDID", z("TestSafeRef")), xloc(50, KEY)]),
        &refr(STIM_REF, 0, STIMPAK, [4.0, 5.0, 6.0], vec![sub(b"XCNT", 3i32.to_le_bytes().to_vec())]),
        &refr(DOOR_A, 0, DOOR, [0.0; 3], vec![xtel(DOOR_B, [100.0, 0.0, 0.0])]),
        &refr(PLAIN_DOOR, 0, DOOR, [0.0; 3], vec![]),
        &refr(CHAIR_REF, 0, CHAIR, [0.0; 3], vec![sub(b"XMRC", SAFE_REF.to_le_bytes().to_vec())]),
        &refr(CHILD_SAME, 0, STIMPAK, [0.0; 3], vec![xesp(KEY_REF, false)]),
        &refr(CHILD_OPPOSITE, 0, STIMPAK, [0.0; 3], vec![xesp(KEY_REF, true)]),
        &refr(GRANDCHILD, 0, STIMPAK, [0.0; 3], vec![xesp(CHILD_OPPOSITE, false)]),
        &refr(GUN_REF, 0, PISTOL, [0.0; 3], vec![sub(b"XHLP", 0.25f32.to_le_bytes().to_vec())]),
        &{
            // A placed explosive: same subrecords as REFR, record type PGRE.
            let mut v = refr(MINE_REF, 0, MINE_PROJ, [0.0; 3], vec![]);
            v[..4].copy_from_slice(b"PGRE");
            v
        },
    ]);
    let pers_a = refr(KEY_REF, 0x400 | 0x800, KEY, [7.0, 8.0, 9.0], vec![sub(b"EDID", z("TestKeyRef"))]);
    let temp_b = cat(&[
        &refr(DOOR_B, 0, DOOR, [0.0; 3], vec![xtel(DOOR_A, [-100.0, 0.0, 0.0])]),
        &refr(IN_DOOR, 0, DOOR, [0.0; 3], vec![xtel(EXT_DOOR, [-73162.9, 1312.4, 8686.4])]),
    ]);
    let wrld = {
        let wrec = record(b"WRLD", 0, WORLD, &[sub(b"EDID", z("TestWorld"))]);
        let pcell = record(b"CELL", 0, WORLD_PERSISTENT, &[sub(b"DATA", vec![0])]);
        let pdoor = cat(&[
            &refr(EXT_DOOR, 0x400, DOOR, [-73100.0, 1300.0, 8680.0], vec![xtel(IN_DOOR, [0.0; 3])]),
            &refr(GATE_OUT, 0x400, DOOR, [-73300.0, 1500.0, 8690.0], vec![xtel(GATE_IN, [8288.0, 6896.0, 1008.0])]),
        ]);
        let xclc = cat(&[&(-18i32).to_le_bytes(), &0i32.to_le_bytes()]);
        let ecell = record(b"CELL", 0, WORLD_CELL, &[sub(b"EDID", z("TestGrid")), sub(b"DATA", vec![0]), sub(b"XCLC", xclc)]);
        let block = group(0i32.to_le_bytes(), 4, &group(0i32.to_le_bytes(), 5, &cat(&[&ecell, &cell_children(WORLD_CELL, &[], &[])])));
        let children = cat(&[&pcell, &cell_children(WORLD_PERSISTENT, &[], &pdoor), &block]);
        let world = cat(&[&wrec, &group(WORLD.to_le_bytes(), 1, &children)]);
        // A second worldspace, a child of the first that uses only its map data, like
        // FreesideNorthWorld (PNAM 4) [observed in FalloutNV.esm].
        let wrec2 = record(
            b"WRLD",
            0,
            WORLD2,
            &[sub(b"EDID", z("TestWorld2")), sub(b"WNAM", WORLD.to_le_bytes().to_vec()), sub(b"PNAM", 4u16.to_le_bytes().to_vec())],
        );
        let pcell2 = record(b"CELL", 0, WORLD2_PERSISTENT, &[sub(b"DATA", vec![0])]);
        let gate_in = refr(GATE_IN, 0x400, DOOR, [8600.0, 7000.0, 1024.0], vec![xtel(GATE_OUT, [-73250.0, 1450.0, 8690.0])]);
        let xclc2 = cat(&[&2i32.to_le_bytes(), &1i32.to_le_bytes()]);
        let ecell2 = record(b"CELL", 0, WORLD2_CELL, &[sub(b"EDID", z("TestGrid2")), sub(b"DATA", vec![0]), sub(b"XCLC", xclc2)]);
        let block2 = group(0i32.to_le_bytes(), 4, &group(0i32.to_le_bytes(), 5, &cat(&[&ecell2, &cell_children(WORLD2_CELL, &[], &[])])));
        let children2 = cat(&[&pcell2, &cell_children(WORLD2_PERSISTENT, &[], &gate_in), &block2]);
        let world2 = cat(&[&wrec2, &group(WORLD2.to_le_bytes(), 1, &children2)]);
        top(b"WRLD", &cat(&[&world, &world2]))
    };
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
    let glob = cat(&[
        &glob_rec(0x35, "GameYear", b's', 2281.0),
        &glob_rec(0x36, "GameMonth", b's', 9.0),
        &glob_rec(0x37, "GameDay", b's', 31.0),
        &glob_rec(0x38, "GameHour", b's', 12.0),
        &glob_rec(0x39, "GameDaysPassed", b'f', 5.0),
        &glob_rec(0x3A, "TimeScale", b's', 30.0),
        &glob_rec(0x150, "TestGlobal", b'f', 42.0),
        &glob_rec(ODDS, "TestOdds", b's', 100.0),
    ]);
    let gmst = cat(&[
        &record(b"GMST", 0, 0x160, &[sub(b"EDID", z("fAVDCarryWeightsBase")), sub(b"DATA", 150f32.to_le_bytes().to_vec())]),
        &record(b"GMST", 0, 0x161, &[sub(b"EDID", z("sOverEncumbered")), sub(b"DATA", z("Too heavy"))]),
        &record(b"GMST", 0, 0x162, &[sub(b"EDID", z("fAVDCarryWeightMult")), sub(b"DATA", 10f32.to_le_bytes().to_vec())]),
    ]);
    cat(&[
        &tes4(),
        &top(b"GLOB", &glob),
        &top(b"GMST", &gmst),
        &top(b"MISC", &misc),
        &top(b"KEYM", &keym),
        &weightless,
        &top(b"FURN", &furn),
        &top(b"LVLI", &lvli),
        &top(b"CONT", &cat(&[&cont, &locker])),
        &top(b"WEAP", &weaps),
        &top(b"ARMO", &armos),
        &top(b"ALCH", &alch),
        &top(b"AMMO", &ammo),
        &top(b"PROJ", &proj),
        &top(b"DOOR", &door),
        &top(b"NPC_", &npc),
        &cells,
        &wrld,
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
        vec![Entry::new(id(CAPS), 3), Entry::new(id(LL_LOOT), 1), Entry::new(id(STIMPAK), 0)]
    );
    let l = f.forms.leveled(id(LL_LOOT)).unwrap();
    assert_eq!(l.entries.len(), 2);
    assert_eq!((l.entries[1].level, l.entries[1].item, l.entries[1].count), (5, id(CAPS), 10));
    assert_eq!(f.forms.leveled(id(LL_NONE)).unwrap().chance_none, 100);
    assert!(matches!(f.forms.get(id(0x7)).unwrap().data, BaseData::Actor(_)));
    assert_eq!(f.forms.setting_f32("FAVDCARRYWEIGHTSBASE"), Some(150.0));
    assert_eq!(f.forms.settings["soverencumbered"], fnv_sim::settings::Setting::Str("Too heavy".into()));
}

#[test]
fn references_load_with_cells_and_extra_data() {
    let f = fixture();
    assert!(f.refs.issues.is_empty(), "{:?}", f.refs.issues.to_json());
    assert_eq!(f.refs.refs.len(), 16);
    // Stored in the persistent cell, parented to the grid cell containing it.
    assert_eq!(f.refs.get(id(EXT_DOOR)).unwrap().stored_in, Some(id(WORLD_PERSISTENT)));
    assert_eq!(f.refs.get(id(EXT_DOOR)).unwrap().cell, Some(id(WORLD_CELL)));
    assert!(f.refs.by_cell[&id(WORLD_CELL)].contains(&id(EXT_DOOR)));
    assert_eq!(f.refs.exterior_cell_at(id(WORLD_PERSISTENT), [-73162.9, 1312.4, 0.0]), Some(id(WORLD_CELL)));
    assert_eq!(f.refs.exterior_cell_at(id(WORLD_PERSISTENT), [0.0, 0.0, 0.0]), None);
    assert_eq!(f.refs.exterior_cell_at(id(CELL_A), [-73162.9, 1312.4, 0.0]), None);
    assert!(f.refs.issues.is_empty(), "{:?}", f.refs.issues.to_json());
    assert_eq!(f.refs.cell_by_edid("testcella"), Some(id(CELL_A)));
    assert!(f.refs.by_cell[&id(CELL_A)].contains(&id(SAFE_REF)));
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
    // XMRC (decoded through the common layout) marks the merchant's stock container.
    assert_eq!(f.refs.get(id(CHAIR_REF)).unwrap().merchant_container, Some(id(SAFE_REF)));
    assert!(f.refs.is_merchant_container(id(SAFE_REF)));
    assert!(!f.refs.is_merchant_container(id(CHAIR_REF)));
}

// ---- runtime --------------------------------------------------------------------

#[test]
fn locked_container_needs_its_key() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig { seed: 1, ..SimConfig::default() });
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
    // Activating the taken reference again gives its stack again (observed in game).
    assert_eq!(w.activate(p, id(STIM_REF)).unwrap().outcome, Outcome::PickedUp { item: id(STIMPAK), count: 3, owner: None });
    assert_eq!(w.item_count(p, id(STIMPAK)).unwrap(), 6);
    w.remove_item(p, id(STIMPAK), 3).unwrap();
    let dropped = w.drop_item(p, id(STIMPAK), 2).unwrap()[0];
    assert_eq!(dropped, FormId::FIRST_DYNAMIC);
    assert_eq!(w.item_count(p, id(STIMPAK)).unwrap(), 1);
    assert_eq!(w.activate(p, dropped).unwrap().outcome, Outcome::PickedUp { item: id(STIMPAK), count: 2, owner: None });
    assert_eq!(w.item_count(p, id(STIMPAK)).unwrap(), 3);
    // Scripted activation of a disabled item still gives it and leaves it disabled
    // (observed in game).
    assert!(matches!(w.activate(p, id(KEY_REF)).unwrap().outcome, Outcome::PickedUp { .. }));
    assert!(!w.is_enabled(id(KEY_REF)).unwrap());
    assert_eq!(w.item_count(p, id(KEY)).unwrap(), 1);
    // A disabled door does nothing (hypothesis).
    w.set_enabled(id(PLAIN_DOOR), false).unwrap();
    assert_eq!(w.activate(p, id(PLAIN_DOOR)).unwrap().outcome, Outcome::Disabled);
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

/// A stand-in for the physics placement `fnvtool sim run` supplies.
struct Lift;

impl actions::Settle for Lift {
    fn settle(&mut self, _: &World, cell: Option<FormId>, pos: [f32; 3]) -> Option<([f32; 3], serde_json::Value)> {
        (cell == Some(id(CELL_B))).then(|| ([pos[0], pos[1], pos[2] + 3.5], serde_json::json!({"shell": 3.5})))
    }
}

#[test]
fn teleported_actors_are_settled_by_the_caller() {
    let f = fixture();
    let text = format!(
        r#"{{"format": "fnv.sim.actions/1", "actions": [
            {{"op": "activate", "actor": "player", "target": "0x{DOOR_A:08X}"}},
            {{"op": "get_pos", "of": "player", "label": "after A"}},
            {{"op": "activate", "actor": "player", "target": "0x{DOOR_B:08X}"}},
            {{"op": "get_pos", "of": "player", "label": "after B"}}]}}"#
    );
    let script: Script = serde_json::from_str(&text).unwrap();
    let mut w = World::new(&f.forms, &f.refs, actions::config(&script));
    let t = actions::run_with(&mut w, &script, Some(&mut Lift)).unwrap();
    assert_eq!(t["steps"][0]["result"]["pos"], serde_json::json!([100.0, 0.0, 0.0]));
    assert_eq!(t["steps"][0]["result"]["settled"]["pos"], serde_json::json!([100.0, 0.0, 3.5]));
    assert_eq!(t["observations"][0]["pos"], serde_json::json!([100.0, 0.0, 3.5]));
    // No settling offered (another cell): the stored destination stands.
    assert!(t["steps"][2]["result"].get("settled").is_none());
    assert_eq!(t["observations"][1]["pos"], serde_json::json!([-100.0, 0.0, 0.0]));
    // Without a settler, `run` keeps the stored destination.
    let mut w = World::new(&f.forms, &f.refs, actions::config(&script));
    let t = actions::run(&mut w, &script).unwrap();
    assert_eq!(t["observations"][0]["pos"], serde_json::json!([100.0, 0.0, 0.0]));
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
        assert_eq!(out, vec![Entry::new(id(CAPS), 20)]);
    }
}

#[test]
fn lvlg_global_replaces_chance_none_at_its_current_value() {
    let f = fixture();
    let mut rng = ChaCha8Rng::seed_from_u64(3);
    let set = |v: f32| std::collections::BTreeMap::from([(id(ODDS), v)]);
    for _ in 0..100 {
        let mut out = Vec::new();
        // Authored value 100 overrides the list's own chance none 0.
        leveled::resolve(&f.forms, id(LL_ODDS), 1, 1, &mut rng, &mut out).unwrap();
        assert!(out.is_empty());
        leveled::resolve_in(&f.forms, &set(0.0), id(LL_ODDS), 1, 1, &mut rng, &mut out).unwrap();
        assert_eq!(out, vec![Entry::new(id(STIMPAK), 1)]);
    }
    let mut n = 0;
    for _ in 0..4000 {
        let mut out = Vec::new();
        leveled::resolve_in(&f.forms, &set(25.0), id(LL_ODDS), 1, 1, &mut rng, &mut out).unwrap();
        n += out.len();
    }
    assert!((2850..3150).contains(&n), "{n}");
}

#[test]
fn world_add_item_reads_lvlg_globals_set_at_run_time() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    assert!(w.add_item(p, id(LL_ODDS), 1).unwrap().is_empty());
    w.set_global(id(ODDS), 0.0).unwrap();
    assert_eq!(w.add_item(p, id(LL_ODDS), 1).unwrap(), vec![(id(STIMPAK), 1)]);
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
    // The script's `activate` is the scripted Activate: one item from the XCNT 3 stack.
    assert_eq!(steps[4]["result"]["count"], 1);
    assert_eq!(steps[6]["result"]["count"], 1);
    assert_eq!(steps[7]["result"]["cell"], "0x00000201");
    assert_eq!(steps[8]["result"]["references"], serde_json::json!(["0xFF000000"]));
    let obs = t["observations"].as_array().unwrap();
    assert_eq!(obs.len(), 3);
    assert_eq!((obs[0]["label"].as_str(), obs[0]["count"].as_i64()), (Some("stims"), Some(1)));
    assert_eq!((obs[1]["label"].as_str(), obs[1]["cell"].as_str()), (Some("step9"), Some("0x00000201")));
    assert_eq!(obs[1]["pos"], serde_json::json!([100.0, 0.0, 0.0]));
    assert_eq!(obs[2]["count"], 0);
    assert_eq!(steps[10]["result"]["removed"], serde_json::json!([["0x00000100", 2], ["0x00000101", 1]]));
    let player = &t["final"]["0x00000014"];
    assert_eq!(player["inventory"]["0x00000101"], 7);
    assert_eq!(player["inventory"]["0x00000102"], 1);
    assert!(player["inventory"].get("0x00000100").is_none(), "the one stimpak was dropped");
    assert_eq!(t["final"]["0x00000300"]["lock"]["locked"], false);
    // Same input, same output.
    assert_eq!(run_script(&f), t);
}

#[test]
fn snapshot_resumes_identically() {
    let f = fixture();
    let cfg = SimConfig { seed: 99, player_level: 5, ..SimConfig::default() };
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

#[test]
fn enable_parent_chains() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    // KEY_REF starts disabled: the same-state child is disabled, the opposite child
    // enabled, and the grandchild follows the opposite child.
    assert!(!w.is_enabled(id(CHILD_SAME)).unwrap());
    assert!(w.is_enabled(id(CHILD_OPPOSITE)).unwrap());
    assert!(w.is_enabled(id(GRANDCHILD)).unwrap());
    // Item activation ignores the enabled state (observed in game), the state stays.
    assert!(matches!(w.activate(p, id(CHILD_SAME)).unwrap().outcome, Outcome::PickedUp { .. }));
    assert!(!w.is_enabled(id(CHILD_SAME)).unwrap());
    w.set_enabled(id(KEY_REF), true).unwrap();
    assert!(w.is_enabled(id(CHILD_SAME)).unwrap());
    assert!(!w.is_enabled(id(CHILD_OPPOSITE)).unwrap());
    assert!(!w.is_enabled(id(GRANDCHILD)).unwrap());
    // A child's own flag does not matter while it has a parent.
    w.set_enabled(id(CHILD_SAME), false).unwrap();
    assert!(w.is_enabled(id(CHILD_SAME)).unwrap());
    // Queries do not materialise references.
    let fresh = World::new(&f.forms, &f.refs, SimConfig::default());
    fresh.is_enabled(id(GRANDCHILD)).unwrap();
    assert_eq!(fresh.changed().count(), 0);
}

#[test]
fn clock_lives_in_the_time_globals() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let c = w.advance_hours(13.0).unwrap();
    assert_eq!((c.month, c.day, c.hour), (10.0, 1.0, 1.0));
    assert_eq!(w.global(id(0x38)).unwrap(), 1.0);
    // 120 real seconds at TimeScale 30 = 1 game hour.
    let c = w.advance_real_seconds(120.0).unwrap();
    assert_eq!(c.hour, 2.0);
    w.set_global(id(0x150), 7.0).unwrap();
    assert_eq!(w.global(id(0x150)).unwrap(), 7.0);
    assert!(w.set_global(id(STIMPAK), 1.0).is_err());
    // Changed globals are part of the snapshot.
    let snap = w.snapshot();
    assert_eq!(snap.globals[&id(0x150)], 7.0);
    let back = World::restore(&f.forms, &f.refs, &snap).unwrap();
    assert_eq!(back.calendar().unwrap(), w.calendar().unwrap());
}

#[test]
fn condition_and_ownership_travel_with_items() {
    use fnv_sim::inventory::Extra;
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    // COED on the container entry.
    let BaseData::Container(c) = &f.forms.get(id(LOCKER)).unwrap().data else { panic!() };
    assert_eq!(c.entries[0].extra.condition, Some(0.75));
    // The locker is not placed; give the player its pistol through a placed gun (XHLP).
    assert_eq!(f.refs.get(id(GUN_REF)).unwrap().health, Some(0.25));
    w.activate(p, id(GUN_REF)).unwrap();
    w.add_item(p, id(PISTOL), 1).unwrap();
    let inv = w.peek(p).unwrap().inventory.clone().unwrap();
    let groups: Vec<_> = inv.groups().filter(|g| g.0 == id(PISTOL)).map(|g| (g.1, g.2)).collect();
    assert_eq!(groups, vec![(Extra::default(), 1), (Extra { condition: Some(0.25), ..Extra::default() }, 1)]);
    // Dropping both makes one reference per group, keeping the condition.
    let refs = w.drop_item(p, id(PISTOL), 2).unwrap();
    assert_eq!(refs.len(), 2);
    assert_eq!(w.peek(refs[1]).unwrap().extra.condition, Some(0.25));
    w.activate(p, refs[1]).unwrap();
    let inv = w.peek(p).unwrap().inventory.clone().unwrap();
    assert_eq!(inv.groups().find(|g| g.0 == id(PISTOL)).unwrap().1.condition, Some(0.25));
    // Snapshots keep extra data.
    let snap = serde_json::to_string(&w.snapshot()).unwrap();
    let back = World::restore(&f.forms, &f.refs, &serde_json::from_str(&snap).unwrap()).unwrap();
    assert_eq!(back.peek(p), w.peek(p));
}

#[test]
fn equipping_follows_slots() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    for it in [PISTOL, RIFLE, HELMET, HAT, SUIT, AID, CAPS] {
        w.add_item(p, id(it), 1).unwrap();
    }
    assert!(w.equip(p, id(PISTOL)).unwrap().unequipped.is_empty());
    assert_eq!(w.equip(p, id(RIFLE)).unwrap().unequipped, vec![id(PISTOL)]);
    w.equip(p, id(HAT)).unwrap();
    w.equip(p, id(SUIT)).unwrap();
    // The helmet covers Head and Hair: it replaces the hat, not the suit.
    assert_eq!(w.equip(p, id(HELMET)).unwrap().unequipped, vec![id(HAT)]);
    let inv = w.peek(p).unwrap().inventory.clone().unwrap();
    // The player's glove (non-playable, worn from the start) stays on.
    assert_eq!(inv.equipped(), vec![id(RIFLE), id(HELMET), id(SUIT), id(GLOVE)]);
    // Aid items are consumed; other items cannot be equipped.
    assert!(w.equip(p, id(AID)).unwrap().consumed);
    assert_eq!(w.item_count(p, id(AID)).unwrap(), 0);
    assert_eq!(w.equip(p, id(CAPS)), Err(SimError::NotEquippable(id(CAPS))));
    assert_eq!(w.equip(p, id(AID)), Err(SimError::NotCarried { actor: p, item: id(AID) }));
    // Giving away an equipped item unequips it.
    w.transfer(p, id(SAFE_REF), id(RIFLE), 1).unwrap();
    assert!(!w.peek(id(SAFE_REF)).unwrap().inventory.as_ref().unwrap().is_equipped(id(RIFLE)));
    assert!(w.unequip(p, id(SUIT)).unwrap());
    assert!(!w.unequip(p, id(SUIT)).unwrap());
}

#[test]
fn encumbrance_uses_strength_and_settings() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    assert_eq!(w.actor_value(p, "Strength").unwrap(), 5.0);
    assert_eq!(w.actor_value(p, "luck").unwrap(), 1.0);
    assert!(matches!(w.actor_value(p, "Speechcraft"), Err(SimError::UnknownActorValue(_))));
    w.add_item(p, id(SUIT), 1).unwrap();
    w.add_item(p, id(BULLETS), 100).unwrap();
    let e = w.encumbrance(p).unwrap();
    // 150 + 10 x 5; suit 15; ammo weightless outside Hardcore; caps weigh 0.
    assert_eq!((e.capacity, e.weight, e.over), (200.0, 15.0, false));
    w.add_item(p, id(RIFLE), 24).unwrap();
    assert!(w.encumbrance(p).unwrap().over);
    w.set_actor_value(p, "strength", 10.0).unwrap();
    assert!(!w.encumbrance(p).unwrap().over);
    let mut hc = World::new(&f.forms, &f.refs, SimConfig { hardcore: true, ..SimConfig::default() });
    hc.add_item(p, id(BULLETS), 100).unwrap();
    assert!((hc.encumbrance(p).unwrap().weight - 10.0).abs() < 1e-4);
    assert!(matches!(w.encumbrance(id(SAFE_REF)), Err(SimError::NoActorValues(_))));
}

#[test]
fn notes_chips_and_caravan_items_weigh_nothing() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    w.add_item(p, id(SUIT), 1).unwrap();
    for item in [NOTE, CHIP, CMNY, CARD] {
        w.add_item(p, id(item), 50).unwrap();
    }
    let e = w.encumbrance(p).unwrap();
    assert_eq!((e.weight, e.unknown_weight), (15.0, vec![]));
}

#[test]
fn placed_mines_disarm_into_their_weapon() {
    let f = fixture();
    assert_eq!(f.forms.item(id(MINE)).unwrap().projectile, Some(id(MINE_PROJ)));
    assert_eq!(f.forms.get(id(MINE_PROJ)).unwrap().data, BaseData::Projectile { item: Some(id(MINE)) });
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    assert_eq!(w.activate(p, id(MINE_REF)).unwrap().outcome, Outcome::Disarmed { item: id(MINE) });
    assert_eq!(w.item_count(p, id(MINE)).unwrap(), 1);
    assert_eq!(w.activate(p, id(MINE_REF)).unwrap().outcome, Outcome::Disabled);
}

#[test]
fn scripted_activation_leaves_placed_mines_alone() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    for _ in 0..2 {
        assert_eq!(w.activate_scripted(p, id(MINE_REF)).unwrap().outcome, Outcome::NoDefault { sig: "PROJ".into() });
    }
    assert_eq!(w.item_count(p, id(MINE)).unwrap(), 0);
    assert!(w.is_enabled(id(MINE_REF)).unwrap());
    // The player's own activation still disarms it afterwards.
    assert_eq!(w.activate(p, id(MINE_REF)).unwrap().outcome, Outcome::Disarmed { item: id(MINE) });
}

#[test]
fn respawning_containers_refill_after_the_delay() {
    // The cell respawn alone: the safe is also a merchant container in the fixture, and
    // merchant restock days would refill it too.
    let mut f = fixture();
    f.refs.merchant_containers.clear();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    w.unlock(id(SAFE_REF)).unwrap();
    w.transfer_all(id(SAFE_REF), p).unwrap();
    w.add_item(id(SAFE_REF), id(PISTOL), 1).unwrap(); // left behind by the player
    let cell = id(CELL_A);
    // Not long enough: nothing resets.
    w.detach_cell(cell).unwrap();
    w.advance_hours(71.0).unwrap();
    assert!(w.attach_cell(cell).unwrap().is_empty());
    assert_eq!(w.item_count(id(SAFE_REF), id(CAPS)).unwrap(), 0);
    // A second absence long enough: the safe (Respawns flag) is back to its authored
    // state, the pistol is gone, the lock is back.
    w.detach_cell(cell).unwrap();
    w.advance_hours(72.0).unwrap();
    assert_eq!(w.attach_cell(cell).unwrap(), vec![id(SAFE_REF)]);
    assert!(w.peek(id(SAFE_REF)).is_none());
    assert_eq!(w.item_count(id(SAFE_REF), id(CAPS)).unwrap(), 3);
    assert_eq!(w.item_count(id(SAFE_REF), id(PISTOL)).unwrap(), 0);
    assert!(w.peek(id(SAFE_REF)).unwrap().lock.unwrap().locked);
    // Attaching a cell that was never detached does nothing; the config is honoured.
    assert!(w.attach_cell(id(CELL_B)).unwrap().is_empty());
    let mut quick = World::new(&f.forms, &f.refs, SimConfig { respawn_hours: Some(1.0), ..SimConfig::default() });
    quick.get(id(SAFE_REF)).unwrap();
    quick.detach_cell(cell).unwrap();
    quick.advance_hours(1.0).unwrap();
    assert_eq!(quick.attach_cell(cell).unwrap(), vec![id(SAFE_REF)]);
    // Detached cells survive a snapshot.
    w.detach_cell(cell).unwrap();
    let snap = w.snapshot();
    assert!(snap.detached_cells.contains_key(&cell));
}

#[test]
fn leveled_sampling_histograms() {
    let f = fixture();
    let cases: fnv_sim::sample::Cases = serde_json::from_str(
        r#"{"format": "fnv.sim.cases/1", "seed": 4, "cases": [
            {"label": "loot at 10", "list": "TestLoot", "level": 10, "count": 2, "samples": 50},
            {"label": "never", "list": "0x00000111", "samples": 10},
            {"label": "not a list", "list": "TestCaps", "samples": 10},
            {"label": "loop", "list": "TestLoop", "samples": 10},
            {"label": "odds 0", "list": "TestOddsList", "globals": {"TestOdds": 0}, "samples": 10},
            {"label": "bad global", "list": "TestOddsList", "globals": {"TestCaps": 0}, "samples": 10}]}"#,
    )
    .unwrap();
    let out = fnv_sim::sample::run(&f.forms, &cases).unwrap();
    assert_eq!(out["format"], "fnv.sim.samples/1");
    let c = out["cases"].as_array().unwrap();
    assert_eq!(c[0]["list"], "0x00000110");
    assert_eq!(c[0]["outcomes"], serde_json::json!({"0x00000101 x20": 50}));
    assert_eq!(c[1]["outcomes"], serde_json::json!({"nothing": 10}));
    assert!(c[2]["error"].as_str().unwrap().contains("not a leveled item list"));
    assert!(c[3]["error"].as_str().unwrap().contains("nesting"));
    assert_eq!(c[4]["outcomes"], serde_json::json!({"0x00000100 x1": 10}));
    assert_eq!(c[4]["globals"], serde_json::json!({"TestOdds": 0.0}));
    assert!(c[5]["error"].as_str().unwrap().contains("not a global"));
    assert_eq!(fnv_sim::sample::run(&f.forms, &cases).unwrap(), out);
}

#[test]
fn teleporting_into_a_worldspace_lands_in_the_grid_cell() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    let r = w.activate(p, id(IN_DOOR)).unwrap();
    // The destination door is stored in the persistent cell; the player lands in the
    // exterior cell containing the destination position.
    assert_eq!(
        r.outcome,
        Outcome::Teleported {
            to_door: id(EXT_DOOR),
            cell: Some(id(WORLD_CELL)),
            pos: [-73162.9, 1312.4, 8686.4],
            rot: [0.0, 0.0, 1.5]
        }
    );
    assert_eq!(w.peek(p).unwrap().cell, Some(id(WORLD_CELL)));
    // Back inside: an interior door keeps its own cell.
    w.activate(p, id(EXT_DOOR)).unwrap();
    assert_eq!(w.peek(p).unwrap().cell, Some(id(CELL_B)));
}

#[test]
fn teleporting_into_another_worldspace_moves_cell_and_worldspace_together() {
    let f = fixture();
    assert_eq!(f.refs.world_of(id(WORLD2_CELL)), Some(id(WORLD2)));
    assert_eq!(f.refs.world_of(id(CELL_B)), None);
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    // Out of the interior into the first worldspace.
    w.activate(p, id(IN_DOOR)).unwrap();
    assert_eq!(w.worldspace_of(p), Some(id(WORLD)));
    // Through the gate: the destination door is persistent in the other worldspace; the
    // player lands in that worldspace's exterior cell containing the destination.
    let r = w.activate(p, id(GATE_OUT)).unwrap();
    assert_eq!(
        r.outcome,
        Outcome::Teleported { to_door: id(GATE_IN), cell: Some(id(WORLD2_CELL)), pos: [8288.0, 6896.0, 1008.0], rot: [0.0, 0.0, 1.5] }
    );
    let ps = w.peek(p).unwrap();
    assert_eq!((ps.cell, ps.pos), (Some(id(WORLD2_CELL)), [8288.0, 6896.0, 1008.0]));
    assert_eq!(w.worldspace_of(p), Some(id(WORLD2)));
    // The game moves the player on (the physics arrival, walking): cell and position
    // as the caller writes them, still in that worldspace.
    let s = w.get(p).unwrap();
    s.pos = [8300.0, 6900.0, 1010.5];
    // A snapshot taken here resumes in the same worldspace, cell and position.
    let text = serde_json::to_string(&w.snapshot()).unwrap();
    let mut resumed = World::restore(&f.forms, &f.refs, &serde_json::from_str(&text).unwrap()).unwrap();
    let rs = resumed.peek(p).unwrap();
    assert_eq!((rs.cell, rs.pos), (Some(id(WORLD2_CELL)), [8300.0, 6900.0, 1010.5]));
    assert_eq!(resumed.worldspace_of(p), Some(id(WORLD2)));
    // The door back returns to the first worldspace's grid cell (-18, 0).
    resumed.activate(p, id(GATE_IN)).unwrap();
    let rs = resumed.peek(p).unwrap();
    assert_eq!((rs.cell, rs.pos), (Some(id(WORLD_CELL)), [-73250.0, 1450.0, 8690.0]));
    assert_eq!(resumed.worldspace_of(p), Some(id(WORLD)));
    // Indoors there is no worldspace.
    resumed.activate(p, id(EXT_DOOR)).unwrap();
    assert_eq!(resumed.worldspace_of(p), None);
    // References not yet materialised answer from their placement.
    assert_eq!(resumed.worldspace_of(id(GATE_IN)), Some(id(WORLD2)));
    assert_eq!(resumed.worldspace_of(id(SAFE_REF)), None);
}

#[test]
fn respawn_delay_comes_from_config_then_setting_then_default() {
    use fnv_sim::world::DEFAULT_RESPAWN_HOURS;
    let f = fixture();
    // The fixture defines no iHoursToRespawnCell: the default applies.
    assert_eq!(World::new(&f.forms, &f.refs, SimConfig::default()).respawn_hours(), DEFAULT_RESPAWN_HOURS);
    let cfg = SimConfig { respawn_hours: Some(5.0), ..SimConfig::default() };
    assert_eq!(World::new(&f.forms, &f.refs, cfg).respawn_hours(), 5.0);
    // A plugin setting is used when the config does not override it.
    let mut forms = FormStore::default();
    forms.settings.insert("ihourstorespawncell".into(), fnv_sim::settings::Setting::Int(24));
    let refs = RefTable::default();
    assert_eq!(World::new(&forms, &refs, SimConfig::default()).respawn_hours(), 24.0);
    // Older snapshots stored a plain number.
    let old: SimConfig = serde_json::from_str(r#"{"seed": 0, "player_level": 1, "respawn_hours": 72.0}"#).unwrap();
    assert_eq!(old.respawn_hours, Some(72.0));
}

#[test]
fn merchant_restock_days_are_0_and_3_modulo_7() {
    use fnv_sim::world::is_merchant_restock_day;
    // Every day the in-game captures restocked on, and every day they passed over.
    for d in [7, 10, 14, 17, 21, 24, 28] {
        assert!(is_merchant_restock_day(d), "{d}");
    }
    for d in [8, 9, 13, 15, 16, 18, 19, 20, 22, 23, 25, 26, 27] {
        assert!(!is_merchant_restock_day(d), "{d}");
    }
    assert!(is_merchant_restock_day(0) && is_merchant_restock_day(-4));
}

#[test]
fn merchant_containers_restock_on_restock_days() {
    // CHAIR_REF names SAFE_REF in XMRC: the safe is a merchant container, and its base
    // has the Respawns flag. The fixture's clock starts at GameDaysPassed 5.0; restock
    // days are 7, 10, 14, 17, 21, 24, ...
    let mut f = fixture();
    let p = FormId::PLAYER_REF;
    let safe = id(SAFE_REF);
    {
        let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
        assert_eq!(w.item_count(safe, id(CAPS)).unwrap(), 3);
        w.unlock(safe).unwrap();
        w.transfer_all(safe, p).unwrap();
        w.add_item(safe, id(AID), 1).unwrap(); // "sold" to the merchant
        // Midnight 6 is no restock day.
        w.advance_hours(47.0).unwrap();
        assert_eq!((w.item_count(safe, id(CAPS)).unwrap(), w.item_count(safe, id(AID)).unwrap()), (0, 1));
        // Midnight 7: restocked, contents replaced, without any cell event or access.
        w.advance_hours(1.0).unwrap();
        assert_eq!(w.peek(safe).unwrap().last_restock, Some(7.0));
        assert_eq!((w.item_count(safe, id(CAPS)).unwrap(), w.item_count(safe, id(AID)).unwrap()), (3, 0));
        // Emptied at once: 3 days to the next restock (10), then 4 (14).
        w.transfer_all(safe, p).unwrap();
        w.advance_hours(71.0).unwrap();
        assert_eq!(w.item_count(safe, id(CAPS)).unwrap(), 0);
        w.advance_hours(1.0).unwrap();
        assert_eq!(w.item_count(safe, id(CAPS)).unwrap(), 3);
        w.transfer_all(safe, p).unwrap();
        w.advance_hours(72.0).unwrap();
        assert_eq!(w.item_count(safe, id(CAPS)).unwrap(), 0);
        w.advance_hours(24.0).unwrap();
        assert_eq!(w.item_count(safe, id(CAPS)).unwrap(), 3);
        // The schedule does not follow accesses or the last restock: emptied a day
        // before a restock day, the box is full again a day later.
        w.advance_hours(48.0).unwrap(); // day 16
        w.transfer_all(safe, p).unwrap();
        assert!(matches!(w.activate(p, safe).unwrap().outcome, Outcome::OpenedContainer { .. }));
        w.advance_hours(24.0).unwrap();
        assert_eq!(w.item_count(safe, id(CAPS)).unwrap(), 3);
        assert_eq!(w.peek(safe).unwrap().last_restock, Some(17.0));
        // Several restock days in one advance (21, 24) restock once, at the last.
        w.advance_hours(24.0 * 9.0).unwrap();
        assert_eq!(w.peek(safe).unwrap().last_restock, Some(24.0));
        assert_eq!(w.item_count(id(GUN_REF), id(CAPS)), Err(SimError::NoInventory(id(GUN_REF))));
        // The restock time is part of the snapshot, and a restored world continues the
        // same way.
        w.transfer_all(safe, p).unwrap();
        let snap = w.snapshot();
        assert_eq!(snap.refs[&safe].last_restock, Some(24.0));
        let mut r = World::restore(&f.forms, &f.refs, &snap).unwrap();
        w.advance_hours(24.0 * 4.0).unwrap();
        r.advance_hours(24.0 * 4.0).unwrap();
        assert_eq!(r.snapshot(), w.snapshot());
        assert_eq!(r.item_count(safe, id(CAPS)).unwrap(), 3);
    }
    // A merchant container whose base lacks the Respawns flag never restocks.
    if let BaseData::Container(c) = &mut f.forms.bases.get_mut(&id(SAFE)).unwrap().data {
        c.respawns = false;
    }
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    w.unlock(safe).unwrap();
    w.transfer_all(safe, p).unwrap();
    w.advance_hours(24.0 * 10.0).unwrap();
    assert_eq!(w.item_count(safe, id(CAPS)).unwrap(), 0);
    assert_eq!(w.peek(safe).unwrap().last_restock, None);
}

#[test]
fn get_locked_reads_zero_or_one() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    let safe = id(SAFE_REF);
    assert_eq!(w.lock_status(safe).unwrap(), 1);
    assert_eq!(w.lock_status(id(CHAIR_REF)).unwrap(), 0, "no lock data");
    // Level 255 still reads 1 in game, not the wiki's 2 (simulation.md §5).
    w.lock(safe, Some(255)).unwrap();
    assert_eq!(w.lock_status(safe).unwrap(), 1);
    assert_eq!(w.activate(p, safe).unwrap().outcome, Outcome::Locked { level: 255 });
    // The key opens it for good.
    w.add_item(p, id(KEY), 1).unwrap();
    assert_eq!(w.activate(p, safe).unwrap().unlocked_with, Some(id(KEY)));
    assert_eq!(w.lock_status(safe).unwrap(), 0);
    // Lock without a level keeps the old one.
    w.lock(safe, None).unwrap();
    assert_eq!(w.peek(safe).unwrap().lock.unwrap().level, 255);
}

#[test]
fn drops_land_ahead_of_and_above_the_holder() {
    use fnv_sim::world::{DROP_ABOVE, DROP_AHEAD};
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    w.add_item(p, id(STIMPAK), 2).unwrap();
    // Facing north (z rotation 0): ahead is +Y.
    let r = w.drop_item(p, id(STIMPAK), 1).unwrap()[0];
    assert_eq!(w.peek(r).unwrap().pos, [0.0, DROP_AHEAD, DROP_ABOVE]);
    // Facing east (z = pi/2, clockwise from north): ahead is +X.
    w.get(p).unwrap().rot[2] = std::f32::consts::FRAC_PI_2;
    w.get(p).unwrap().pos = [10.0, 20.0, 30.0];
    let r = w.drop_item(p, id(STIMPAK), 1).unwrap()[0];
    let pos = w.peek(r).unwrap().pos;
    assert!((pos[0] - (10.0 + DROP_AHEAD)).abs() < 1e-3 && (pos[1] - 20.0).abs() < 1e-3, "{pos:?}");
    assert_eq!(pos[2], 30.0 + DROP_ABOVE);
}

#[test]
fn created_lists_drops_taken_ones_included() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    w.add_item(p, id(STIMPAK), 2).unwrap();
    assert_eq!(w.created().count(), 0);
    let a = w.drop_item(p, id(STIMPAK), 1).unwrap()[0];
    let b = w.drop_item(p, id(STIMPAK), 1).unwrap()[0];
    w.activate(p, a).unwrap();
    let listed: Vec<(FormId, bool)> = w.created().map(|(id, s)| (id, s.deleted)).collect();
    assert_eq!(listed, vec![(a, true), (b, false)]);
    // The player (materialised, below the created range) is not listed.
    assert!(w.peek(p).is_some());
}

const CREATED_SCRIPT: &str = r#"{
  "format": "fnv.sim.actions/1",
  "actions": [
    {"op": "get_locked", "of": "TestSafeRef", "label": "locked"},
    {"op": "activate", "actor": "player", "target": "TestSafeRef", "label": "no key"},
    {"op": "add_item", "to": "player", "item": "TestKey", "count": 1},
    {"op": "activate", "actor": "player", "target": "TestSafeRef", "label": "key"},
    {"op": "get_locked", "of": "TestSafeRef", "label": "unlocked"},
    {"op": "activate", "actor": "player", "target": "created:TestStim", "label": "none yet"},
    {"op": "add_item", "to": "player", "item": "TestStim", "count": 2},
    {"op": "drop_item", "from": "player", "item": "TestStim", "count": 1},
    {"op": "drop_item", "from": "player", "item": "TestStim", "count": 1},
    {"op": "get_pos", "of": "created:TestStim", "label": "newest"},
    {"op": "activate", "actor": "player", "target": "created:TestStim", "label": "item"},
    {"op": "get_pos", "of": "created:0x00000100", "label": "older"}
  ]
}"#;

#[test]
fn created_names_and_labelled_activations() {
    let f = fixture();
    let s: Script = serde_json::from_str(CREATED_SCRIPT).unwrap();
    let mut w = World::new(&f.forms, &f.refs, actions::config(&s));
    let t = actions::run(&mut w, &s).unwrap();
    let obs: Vec<_> = t["observations"].as_array().unwrap().iter().map(|o| {
        let mut o = o.clone();
        o.as_object_mut().unwrap().remove("step");
        o
    }).collect();
    use serde_json::json;
    assert_eq!(obs[0], json!({"label": "locked", "locked": 1}));
    assert_eq!(obs[1], json!({"label": "no key", "opened": false}));
    assert_eq!(obs[2], json!({"label": "key", "opened": true}));
    assert_eq!(obs[3], json!({"label": "unlocked", "locked": 0}));
    assert!(obs[4]["error"].as_str().unwrap().contains("no created reference"));
    // Two drops: the newest (second) reference is named first; once it is taken, the
    // older one.
    assert_eq!(t["steps"][9]["result"]["cell"], serde_json::Value::Null);
    assert_eq!(t["steps"][8]["result"]["references"], json!(["0xFF000001"]));
    assert_eq!(obs[6], json!({"label": "item", "opened": false}));
    assert_eq!(t["steps"][10]["result"]["result"], "picked_up");
    assert_eq!(t["final"]["0xFF000001"]["deleted"], true);
    assert_eq!(t["final"]["0xFF000000"]["deleted"], false);
    assert!(obs[7].get("error").is_none(), "{}", obs[7]);
}

#[test]
fn scripted_activation_takes_one_item_of_a_stack() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    let stack = id(STIM_REF); // XCNT 3
    let r = w.activate_scripted(p, stack).unwrap();
    assert_eq!(r.outcome, Outcome::PickedUp { item: id(STIMPAK), count: 1, owner: None });
    assert_eq!(w.peek(stack).unwrap().count, 1, "the rest of the stack is lost");
    w.activate_scripted(p, stack).unwrap();
    assert_eq!(w.item_count(p, id(STIMPAK)).unwrap(), 2);
    // A dropped stack likewise.
    w.add_item(p, id(STIMPAK), 1).unwrap();
    let dropped = w.drop_item(p, id(STIMPAK), 3).unwrap()[0];
    assert_eq!(w.peek(dropped).unwrap().count, 3);
    w.activate_scripted(p, dropped).unwrap();
    assert_eq!(w.item_count(p, id(STIMPAK)).unwrap(), 1);
    // The player's own activation takes the whole stack (hypothesis).
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    assert_eq!(w.activate(p, stack).unwrap().outcome, Outcome::PickedUp { item: id(STIMPAK), count: 3, owner: None });
}

#[test]
fn non_playable_armour_is_flagged() {
    let f = fixture();
    assert!(f.forms.item(id(GLOVE)).unwrap().non_playable);
    assert!(!f.forms.item(id(SUIT)).unwrap().non_playable);
    assert!(!f.forms.item(id(PISTOL)).unwrap().non_playable);
}

#[test]
fn the_player_starts_wearing_non_playable_armour_and_keeps_it() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let p = FormId::PLAYER_REF;
    assert!(w.inventory_mut(p).unwrap().is_equipped(id(GLOVE)));
    assert!(!w.inventory_mut(p).unwrap().is_equipped(id(CAPS)));
    // RemoveAllItems takes everything else and leaves the glove worn.
    w.add_item(p, id(SUIT), 1).unwrap();
    w.equip(p, id(SUIT)).unwrap();
    let removed = w.remove_all_items(p).unwrap();
    assert!(removed.iter().all(|&(i, _)| i != id(GLOVE)));
    assert_eq!(w.item_count(p, id(GLOVE)).unwrap(), 1);
    assert_eq!(w.item_count(p, id(SUIT)).unwrap(), 0);
    assert_eq!(w.item_count(p, id(CAPS)).unwrap(), 0);
    assert!(w.inventory_mut(p).unwrap().is_equipped(id(GLOVE)));
}
