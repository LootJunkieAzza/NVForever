//! Synthetic clean-room round-trip & edge tests for `.fos` -> `.nvfsave` conversion.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::fos_convert::{convert_fos, inspect_fos, ConvertOptions};
use fnv_game::game::{Game, GameConfig, GameSnapshot, InstKey};
use fnv_game::GameData;
use fnv_save::fos::{
    Fos, FORM_DELETED, FORM_DISABLED, FORM_TALKED_TO_PC, PLAYER_BASE, PLAYER_CLASS, PLAYER_REF,
    TYPE_ACHR, TYPE_ACRE, TYPE_CHAL, TYPE_CLAS, TYPE_FACT, TYPE_FLST, TYPE_INFO, TYPE_LVLI,
    TYPE_NOTE, TYPE_NPC, TYPE_QUST, TYPE_REFR, TYPE_REPU,
};
use fnv_save::{PluginEntry, SaveBody, SaveHeader, SaveKind, BODY_FORMAT, ENGINE_FORMAT};
use fnv_script::vm::Value;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, World};

const BAR: u8 = 0x7C;
const GLOB_HOUR: u32 = 0x0000_0038;
const GLOB_DAYS: u32 = 0x0000_0039;
const GLOB_CUSTOM: u32 = 0x0000_0801;
const QUEST_ID: u32 = 0x0000_0810;
const SCRIPT_ID: u32 = 0x0000_0811;
const ITEM_ID: u32 = 0x0000_0820;
const ITEM_WORN_ID: u32 = 0x0000_0821;
const WEAP_ID: u32 = 0x0000_0822;
const AMMO_ID: u32 = 0x0000_0823;
const ROOM_ID: u32 = 0x0000_0830;
const PROP_REF: u32 = 0x0000_0831;
const NPC_BASE: u32 = 0x0000_0832;
const NPC_REF: u32 = 0x0000_0833;
const FACT_A: u32 = 0x0000_0840;
const FACT_B: u32 = 0x0000_0841;
const FLST_ID: u32 = 0x0000_0850;
const LVLI_ID: u32 = 0x0000_0851;
const REPU_ID: u32 = 0x0000_0860;
const CHAL_ID: u32 = 0x0000_0861;
const NOTE_ID: u32 = 0x0000_0870;
const DIAL_ID: u32 = 0x0000_0880;
const INFO_ID: u32 = 0x0000_0881;
const WTHR_ID: u32 = 0x0000_0890;
const CONT_BASE: u32 = 0x0000_08A0;
const CONT_REF: u32 = 0x0000_08A1;
const CONT28_REF: u32 = 0x0000_08A2;
const MAP_MARKER_BASE: u32 = 0x0000_0010;
const MARKER_VIS_REF: u32 = 0x0000_08B0;
const MARKER_FOUND_REF: u32 = 0x0000_08B1;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let glob = |id: u32, edid: &str, val: f32| {
        record(
            b"GLOB",
            0,
            id,
            &[
                sub(b"EDID", z(edid)),
                sub(b"FNAM", vec![b'f']),
                sub(b"FLTV", val.to_le_bytes().to_vec()),
            ],
        )
    };
    let globals = [
        glob(0x0000_0035, "GameYear", 2281.0),
        glob(0x0000_0036, "GameMonth", 9.0),
        glob(0x0000_0037, "GameDay", 19.0),
        glob(GLOB_HOUR, "GameHour", 8.0),
        glob(GLOB_DAYS, "GameDaysPassed", 1.0),
        glob(GLOB_CUSTOM, "OwnTestGlobal", 0.0),
    ]
    .concat();
    let mut scpt_subs = vec![sub(b"EDID", z("OwnQuestScript"))];
    scpt_subs.extend(unit(
        1,
        &script(&[(0, None, Vec::new())]),
        &[(1, "fCount", 0), (2, "rTarget", 0), (3, "fInf", 0), (4, "fGood", 0)],
        &[],
    ));
    let script = record(b"SCPT", 0, SCRIPT_ID, &scpt_subs);
    let quest = record(
        b"QUST",
        0,
        QUEST_ID,
        &[
            sub(b"EDID", z("OwnTestQuest")),
            sub(b"SCRI", SCRIPT_ID.to_le_bytes().to_vec()),
        ],
    );
    let item = record(
        b"MISC",
        0,
        ITEM_ID,
        &[
            sub(b"EDID", z("OwnTestMisc")),
            sub(b"DATA", [5i32.to_le_bytes(), 1.0f32.to_le_bytes()].concat()),
        ],
    );
    // Synthetic armor with health = 100 and biped slot upper body so Equip works cleanly.
    let armor = record(
        b"ARMO",
        0,
        ITEM_WORN_ID,
        &[
            sub(b"EDID", z("OwnTestSuit")),
            sub(b"BMDT", [4u32.to_le_bytes().as_slice(), &[0, 0, 0, 0]].concat()),
            sub(
                b"DATA",
                [
                    50i32.to_le_bytes().as_slice(),
                    &100i32.to_le_bytes(),
                    &2.0f32.to_le_bytes(),
                ]
                .concat(),
            ),
        ],
    );
    let weap_data = [
        &100i32.to_le_bytes()[..],
        &200i32.to_le_bytes(),
        &3.0f32.to_le_bytes(),
        &15i16.to_le_bytes(),
        &[12u8],
    ]
    .concat();
    let weap = record(
        b"WEAP",
        0,
        WEAP_ID,
        &[
            sub(b"EDID", z("OwnTestRifle")),
            sub(b"NAM0", AMMO_ID.to_le_bytes().to_vec()),
            sub(b"DATA", weap_data),
        ],
    );
    let ammo = record(b"AMMO", 0, AMMO_ID, &[sub(b"EDID", z("OwnTestAmmo"))]);
    let npc_player = record(b"NPC_", 0, PLAYER_BASE, &[sub(b"EDID", z("Player"))]);
    let npc_other = record(b"NPC_", 0, NPC_BASE, &[sub(b"EDID", z("OwnNpcBase"))]);
    let fact_a = record(b"FACT", 0, FACT_A, &[sub(b"EDID", z("OwnFactA"))]);
    let fact_b = record(b"FACT", 0, FACT_B, &[sub(b"EDID", z("OwnFactB"))]);
    let flst = record(b"FLST", 0, FLST_ID, &[sub(b"EDID", z("OwnFormList"))]);
    let lvli = record(b"LVLI", 0, LVLI_ID, &[sub(b"EDID", z("OwnLeveledList"))]);
    let repu = record(b"REPU", 0, REPU_ID, &[sub(b"EDID", z("OwnReputation"))]);
    let chal = record(
        b"CHAL",
        0,
        CHAL_ID,
        &[
            sub(b"EDID", z("OwnChallenge")),
            sub(
                b"DATA",
                [
                    13u32.to_le_bytes().as_slice(), // type: scripted
                    &5u32.to_le_bytes(),            // threshold: 5
                    &0u32.to_le_bytes(),            // flags
                    &1u32.to_le_bytes(),            // interval
                    &0u32.to_le_bytes(),
                    &0u32.to_le_bytes(),
                ]
                .concat(),
            ),
        ],
    );
    let note = record(b"NOTE", 0, NOTE_ID, &[sub(b"EDID", z("OwnNote"))]);
    let dial = record(b"DIAL", 0, DIAL_ID, &[sub(b"EDID", z("OwnTopic"))]);
    let info = record(
        b"INFO",
        0,
        INFO_ID,
        &[sub(b"DATA", vec![0, 0, 0, 0x04])], // say_once flag
    );
    let dial_grp = [dial, group(DIAL_ID.to_le_bytes(), 7, &info)].concat();
    let wthr = record(b"WTHR", 0, WTHR_ID, &[sub(b"EDID", z("OwnClearWeather"))]);
    let cont_base = record(
        b"CONT",
        0,
        CONT_BASE,
        &[
            sub(b"EDID", z("OwnTestCrate")),
            sub(b"CNTO", [ITEM_ID.to_le_bytes(), 3i32.to_le_bytes()].concat()),
        ],
    );
    let marker_base = record(b"STAT", 0, MAP_MARKER_BASE, &[sub(b"EDID", z("MapMarker"))]);

    let prop_ref = record(
        b"REFR",
        0,
        PROP_REF,
        &[
            sub(b"EDID", z("OwnPropRef")),
            sub(b"NAME", ITEM_ID.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let cont_ref = record(
        b"REFR",
        0,
        CONT_REF,
        &[
            sub(b"EDID", z("OwnCrateRef")),
            sub(b"NAME", CONT_BASE.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let cont28_ref = record(
        b"REFR",
        0,
        CONT28_REF,
        &[
            sub(b"EDID", z("OwnCrate28Ref")),
            sub(b"NAME", CONT_BASE.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let marker_vis_ref = record(
        b"REFR",
        0,
        MARKER_VIS_REF,
        &[
            sub(b"EDID", z("OwnMarkerVis")),
            sub(b"NAME", MAP_MARKER_BASE.to_le_bytes().to_vec()),
            sub(b"XMRK", Vec::new()),
            sub(b"FNAM", vec![0]),
            sub(b"FULL", z("Visible Spot")),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let marker_found_ref = record(
        b"REFR",
        0,
        MARKER_FOUND_REF,
        &[
            sub(b"EDID", z("OwnMarkerFound")),
            sub(b"NAME", MAP_MARKER_BASE.to_le_bytes().to_vec()),
            sub(b"XMRK", Vec::new()),
            sub(b"FNAM", vec![0]),
            sub(b"FULL", z("Found Spot")),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let npc_ref = record(
        b"ACHR",
        0,
        NPC_REF,
        &[
            sub(b"EDID", z("OwnNpcRef")),
            sub(b"NAME", NPC_BASE.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let player_ref = record(
        b"ACHR",
        0,
        PLAYER_REF,
        &[
            sub(b"EDID", z("PlayerRef")),
            sub(b"NAME", PLAYER_BASE.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let room = record(
        b"CELL",
        0,
        ROOM_ID,
        &[sub(b"EDID", z("OwnTestRoom")), sub(b"DATA", vec![1])],
    );
    let children = group(
        ROOM_ID.to_le_bytes(),
        6,
        &group(
            ROOM_ID.to_le_bytes(),
            9,
            &[prop_ref, cont_ref, cont28_ref, marker_vis_ref, marker_found_ref, npc_ref, player_ref].concat(),
        ),
    );
    let bytes = [
        tes4(),
        top(b"GLOB", &globals),
        top(b"SCPT", &script),
        top(b"QUST", &quest),
        top(b"MISC", &item),
        top(b"ARMO", &armor),
        top(b"WEAP", &weap),
        top(b"AMMO", &ammo),
        top(b"CONT", &cont_base),
        top(b"STAT", &marker_base),
        top(b"NPC_", &[npc_player, npc_other].concat()),
        top(b"FACT", &[fact_a, fact_b].concat()),
        top(b"FLST", &flst),
        top(b"LVLI", &lvli),
        top(b"REPU", &repu),
        top(b"CHAL", &chal),
        top(b"NOTE", &note),
        top(b"DIAL", &dial_grp),
        top(b"WTHR", &wthr),
        top(
            b"CELL",
            &group(
                0i32.to_le_bytes(),
                2,
                &group(0i32.to_le_bytes(), 3, &[room, children].concat()),
            ),
        ),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "FalloutNV.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let schema =
        Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/script/functions.json");
    let catalogue = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(
        &order,
        &schema,
        FunctionTable::load(&path).unwrap(),
        &catalogue,
        &refs,
    );
    Fixture { data, forms, refs }
}

struct Build {
    b: Vec<u8>,
}

impl Build {
    fn bar(&mut self) -> &mut Self {
        self.b.push(BAR);
        self
    }
    fn u8(&mut self, v: u8) -> &mut Self {
        self.b.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.b.extend(v.to_le_bytes());
        self
    }
    fn i16(&mut self, v: i16) -> &mut Self {
        self.b.extend(v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.b.extend(v.to_le_bytes());
        self
    }
    fn i32(&mut self, v: i32) -> &mut Self {
        self.b.extend(v.to_le_bytes());
        self
    }
    fn f32(&mut self, v: f32) -> &mut Self {
        self.b.extend(v.to_le_bytes());
        self
    }
    fn f64(&mut self, v: f64) -> &mut Self {
        self.b.extend(v.to_le_bytes());
        self
    }
    fn s_field(&mut self, v: &str, trailing_bar: bool) -> &mut Self {
        self.u16(v.len() as u16).bar();
        self.b.extend(v.as_bytes());
        if trailing_bar && !v.is_empty() {
            self.bar();
        }
        self
    }
    fn r(&mut self, idx_1based: u32) -> &mut Self {
        self.b.extend(&idx_1based.to_be_bytes()[1..]);
        self
    }
    fn vs(&mut self, n: u32) -> &mut Self {
        assert!(n < 64);
        self.u8((n << 2) as u8)
    }
}

fn push_record(out: &mut Vec<u8>, r: u32, flags: u32, kind: u8, data: &[u8]) {
    out.extend(&r.to_be_bytes()[1..]);
    out.extend(flags.to_le_bytes());
    out.push(kind | 0x40);
    out.push(27);
    out.extend((data.len() as u16).to_le_bytes());
    out.extend(data);
}

fn synthetic_fos_bytes() -> Vec<u8> {
    // 1-based form ID array (index 1..=20)
    let fids = [
        GLOB_HOUR,           // 1
        GLOB_DAYS,           // 2
        GLOB_CUSTOM,         // 3
        QUEST_ID,            // 4
        ITEM_ID,             // 5
        ITEM_WORN_ID,        // 6
        ROOM_ID,             // 7
        PROP_REF,            // 8
        NPC_REF,             // 9
        FACT_A,              // 10
        FACT_B,              // 11
        FLST_ID,             // 12
        LVLI_ID,             // 13
        REPU_ID,             // 14
        CHAL_ID,             // 15
        NOTE_ID,             // 16
        INFO_ID,             // 17
        WTHR_ID,             // 18
        PLAYER_REF,          // 19
        PLAYER_BASE,         // 20
        PLAYER_CLASS,        // 21
        0x0100_0999,         // 22: form from an unloaded plugin
        0xFF00_0042,         // 23: created 0xFF form
    ];

    let mut h = Build { b: Vec::new() };
    h.u32(0x30).bar();
    h.b.extend([0u8; 64]);
    h.bar()
        .u32(2)
        .bar()
        .u32(2)
        .bar()
        .u32(7)
        .bar()
        .s_field("Synthetic", true)
        .s_field("Wanderer", true)
        .u32(4)
        .bar()
        .s_field("OwnTestRoom", true)
        .s_field("002.15.30", false);

    let mut b = b"FO3SAVEGAME".to_vec();
    b.extend((h.b.len() as u32).to_le_bytes());
    b.extend(&h.b);
    // 2x2 RGB screenshot (12 bytes)
    b.extend([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0]);

    let mut pl = Build { b: Vec::new() };
    pl.u8(2).bar().s_field("FalloutNV.esm", true).s_field("MissingMod.esp", true);
    b.push(27);
    b.extend((pl.b.len() as u32).to_le_bytes());
    b.extend(&pl.b);

    let table_at = b.len();
    b.extend([0u8; 32 + 78]);

    let g1 = b.len();
    // Global type 0: misc stats (index 0 = 3, index 2 = 11)
    let mut t0 = Build { b: Vec::new() };
    t0.u32(3).bar().u32(3).bar().u32(0).bar().u32(11).bar();
    b.extend(0u32.to_le_bytes());
    b.extend((t0.b.len() as u32).to_le_bytes());
    b.extend(&t0.b);

    // Global type 1: location & active quest (QUEST_ID = r(4))
    let mut t1 = Build { b: Vec::new() };
    t1.u32(0xFF00_0050)
        .bar()
        .r(0)
        .bar()
        .i32(0)
        .bar()
        .i32(0)
        .bar()
        .r(7)
        .bar()
        .f32(10.0)
        .f32(20.0)
        .f32(30.0)
        .bar()
        .r(4)
        .bar();
    b.extend(1u32.to_le_bytes());
    b.extend((t1.b.len() as u32).to_le_bytes());
    b.extend(&t1.b);

    // Global type 3: 5 globals (3 valid, 1 unloaded plugin, 1 created 0xFF)
    let mut t3 = Build { b: Vec::new() };
    t3.vs(5)
        .bar()
        .r(1)
        .bar()
        .f32(14.5)
        .bar()
        .r(2)
        .bar()
        .f32(9.25)
        .bar()
        .r(3)
        .bar()
        .f32(42.0)
        .bar()
        .r(22)
        .bar()
        .f32(1.0)
        .bar()
        .r(23)
        .bar()
        .f32(2.0)
        .bar();
    b.extend(3u32.to_le_bytes());
    b.extend((t3.b.len() as u32).to_le_bytes());
    b.extend(&t3.b);

    // Global type 8: weather (WTHR_ID = r(18))
    let mut t8 = Build { b: Vec::new() };
    t8.r(18).bar().r(0).bar();
    b.extend(8u32.to_le_bytes());
    b.extend((t8.b.len() as u32).to_le_bytes());
    b.extend(&t8.b);

    // Global type 6: unmapped global table entry (should be counted in skipped)
    b.extend(6u32.to_le_bytes());
    b.extend(4u32.to_le_bytes());
    b.extend([1, 2, 3, 4]);

    let cf = b.len();
    // 1. QUST (QUEST_ID = r(4)): running, delay=2.5, stage 20 done, vars (1=7.5, 2=NPC_REF), obj 10 displayed+completed
    let mut q = Build { b: Vec::new() };
    q.u8(0x01).bar().f32(2.5).bar();
    q.vs(1).bar().u8(20).bar().u8(1).bar().vs(0).bar();
    q.vs(2)
        .bar()
        .u32(1)
        .bar()
        .f64(7.5)
        .bar()
        .u32(0x8000_0002)
        .bar()
        .r(9)
        .bar()
        .vs(0)
        .bar()
        .vs(0)
        .bar();
    q.vs(1).bar().u32(10).bar().u32(3).bar();
    push_record(&mut b, 4, (1 << 1) | (1 << 2) | (1 << 31) | (1 << 30) | (1 << 29), TYPE_QUST, &q.b);

    // 2. NPC_ player base (r(20)): SPECIAL
    let mut npc = Build { b: Vec::new() };
    npc.b.extend([6, 7, 5, 4, 8, 9, 10]);
    npc.bar();
    push_record(&mut b, 20, 1 << 2, TYPE_NPC, &npc.b);

    // 3. CLAS PlayerClass (r(21)): tag skills 38, 43, 44
    let mut clas = Build { b: Vec::new() };
    clas.u32(38).bar().u32(43).bar().u32(44).bar().u32(u32::MAX).bar();
    push_record(&mut b, 21, 1 << 1, TYPE_CLAS, &clas.b);

    // 4. Player ACHR (r(19)): place, 232 AV floats (AV2: karma=250.0, xp=1200.0), 2 items (1 worn with condition 50/100)
    let mut pl_rec = Build { b: Vec::new() };
    pl_rec.r(7).f32(12.0).f32(34.0).f32(56.0).f32(0.1).f32(0.0).f32(1.5).bar();
    for i in 0..232 {
        let v = if i == 77 + 23 {
            250.0
        } else if i == 77 + 24 {
            1200.0
        } else {
            0.0
        };
        pl_rec.f32(v).bar();
    }
    pl_rec
        .u8(0)
        .bar()
        .f32(1.0)
        .bar()
        .vs(2)
        .bar()
        .u8(0x5E)
        .bar()
        .vs(0)
        .bar()
        .u8(0x18)
        .bar()
        .r(0)
        .bar()
        .f32(0.0)
        .f32(0.0)
        .f32(0.0)
        .bar()
        .f32(0.0)
        .bar();
    pl_rec.vs(2).bar();
    pl_rec.r(5).bar().i32(3).bar().vs(0).bar();
    pl_rec
        .r(6)
        .bar()
        .i32(1)
        .bar()
        .vs(1)
        .bar()
        .vs(2)
        .bar()
        .u8(0x16)
        .bar()
        .u8(0x25)
        .bar()
        .f32(50.0)
        .bar();
    push_record(&mut b, 19, (1 << 1) | (1 << 4) | (1 << 5), TYPE_ACHR, &pl_rec.b);

    // 5. REFR PROP_REF (r(8)): disabled + deleted
    let mut rf = Build { b: Vec::new() };
    rf.u32(FORM_DISABLED | FORM_DELETED).bar();
    push_record(&mut b, 8, 1 << 0, TYPE_REFR, &rf.b);

    // 6. ACHR NPC_REF (r(9)): moved + talked_to_pc
    let mut nr = Build { b: Vec::new() };
    nr.r(7).f32(100.0).f32(200.0).f32(5.0).f32(0.0).f32(0.0).f32(0.5).bar();
    nr.u8(0).bar().u32(FORM_TALKED_TO_PC).bar();
    push_record(&mut b, 9, 0b11, TYPE_ACHR, &nr.b);

    // 7. FACT FACT_A (r(10)): reaction to FACT_B (r(11)) = 2 (ally)
    let mut fc = Build { b: Vec::new() };
    fc.vs(1).bar().r(11).bar().i32(50).bar().u32(2).bar();
    push_record(&mut b, 10, 1 << 2, TYPE_FACT, &fc.b);

    // 8. FLST (r(12)): u32 count=1 | ITEM_ID
    let mut fl = Build { b: Vec::new() };
    fl.u32(1).bar().r(5).bar();
    push_record(&mut b, 12, 1 << 31, TYPE_FLST, &fl.b);

    // 9. LVLI (r(13)): 1 entry (ITEM_ID, level 2, count 4, cond 1.0)
    let mut lv = Build { b: Vec::new() };
    lv.vs(1).bar().r(5).bar().u16(2).bar().u16(4).bar().f32(1.0).bar();
    push_record(&mut b, 13, 1 << 31, TYPE_LVLI, &lv.b);

    // 10. REPU (r(14)): fame 15.0, infamy 3.0
    let mut rp = Build { b: Vec::new() };
    rp.f32(15.0).bar().f32(3.0).bar();
    push_record(&mut b, 14, 1 << 1, TYPE_REPU, &rp.b);

    // 11. CHAL (r(15)): progress 5 (>= threshold 5 -> completed)
    let mut ch = Build { b: Vec::new() };
    ch.u32(5).bar().u32(4).bar();
    push_record(&mut b, 15, 1 << 1, TYPE_CHAL, &ch.b);

    // 12. NOTE (r(16)) & INFO (r(17))
    push_record(&mut b, 16, 1 << 31, TYPE_NOTE, &[]);
    push_record(&mut b, 17, 1 << 31, TYPE_INFO, &[]);

    // 13. Created 0xFF REFR (r(23)) — must be gracefully skipped & counted
    push_record(&mut b, 23, 1 << 0, TYPE_REFR, &[0, 0, 0, 0, BAR]);

    let g2 = b.len();
    b.extend(1000u32.to_le_bytes());
    b.extend(0u32.to_le_bytes());

    let fid = b.len();
    b.extend((fids.len() as u32).to_le_bytes());
    for f in fids {
        b.extend(f.to_le_bytes());
    }
    b.extend(0u32.to_le_bytes());
    let tail = b.len();
    b.extend([5, 0, 0, 0, 0, 0, 0, 0, BAR]);

    let table = [fid, tail, g1, cf, g2, 5, 1, 14].map(|v| v as u32);
    for (i, v) in table.iter().enumerate() {
        b[table_at + 4 * i..table_at + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }
    b
}

#[test]
fn fos_converts_to_valid_nvfsave_and_restores_all_sections() {
    let f = fixture();
    let raw = synthetic_fos_bytes();
    let fos = Fos::parse(&raw).unwrap();

    let inspect = inspect_fos(&fos, true);
    assert_eq!(inspect.schema, "fnv.save.fos_inspect/1");
    assert!(inspect.cosave_present);
    assert_eq!(inspect.source.player_name, "Synthetic");
    assert_eq!(inspect.decoded_counts.get("quests"), Some(&1));
    assert_eq!(inspect.decoded_counts.get("form_lists"), Some(&1));
    assert_eq!(inspect.decoded_counts.get("leveled_lists"), Some(&1));
    assert_eq!(inspect.decoded_counts.get("reputations"), Some(&1));
    assert_eq!(inspect.decoded_counts.get("challenges"), Some(&1));
    assert_eq!(inspect.decoded_counts.get("notes"), Some(&1));
    assert_eq!(inspect.decoded_counts.get("said_infos"), Some(&1));

    let opts = ConvertOptions {
        kind: SaveKind::Manual,
        plugins: vec![PluginEntry { name: "FalloutNV.esm".into(), size: 1234 }],
        include_thumbnail: true,
        cosave_present: true,
    };
    let converted = convert_fos(&fos, &f.data, &f.forms, &f.refs, &opts).unwrap();
    assert_eq!(converted.report.missing_plugins, ["MissingMod.esp"]);
    assert!(converted.report.cosave_ignored);
    assert!(converted.report.warnings.iter().any(|w| w.contains(".nvse")));
    assert_eq!(converted.report.converted.get("globals"), Some(&3));
    assert_eq!(converted.report.converted.get("quests"), Some(&1));
    assert_eq!(converted.report.converted.get("form_lists"), Some(&1));
    assert_eq!(converted.report.converted.get("leveled_lists"), Some(&1));
    assert_eq!(converted.report.converted.get("reputations"), Some(&1));
    assert_eq!(converted.report.converted.get("challenges"), Some(&1));
    assert_eq!(converted.report.converted.get("notes"), Some(&1));
    assert_eq!(converted.report.converted.get("said_infos"), Some(&1));
    assert_eq!(converted.report.converted.get("weather"), Some(&1));
    assert_eq!(converted.report.converted.get("thumbnail_encoded"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("player_xp (AV2 code 24) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("player_karma (AV2 code 23) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("reputations (REPU bit 1, fame/infamy) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("active_quest (global type 1) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("form_lists (FLST bit 31, u32 count) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("leveled_lists (LVLI bit 31) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("challenges (CHAL bit 1, progress/flags) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("notes (NOTE bit 31) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("said_infos (INFO bit 31) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("player_item_conditions (extra 0x25) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("player_equipped (extra 0x16) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("reference_form_flags (REFR/ACHR/ACRE bit 0) [hypothesis]"), Some(&2));
    assert_eq!(converted.report.hypothesis_fields.get("faction_relations (FACT bit 2) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("player_special (NPC_ bit 2) [hypothesis]"), Some(&7));
    assert_eq!(converted.report.hypothesis_fields.get("player_tag_skills (CLAS bit 1) [hypothesis]"), Some(&3));
    assert_eq!(converted.report.hypothesis_fields.get("misc_stats (global type 0) [hypothesis]"), Some(&2));
    assert_eq!(converted.report.hypothesis_fields.get("weather (global type 8) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("quest_flags_delay_objectives (QUST) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("thumbnail_rgb_order (header screenshot) [hypothesis]"), Some(&1));
    assert!(converted.report.warnings.iter().any(|w| w.contains("[hypothesis]")));
    assert!(converted.report.skipped.contains_key("global: form of a plugin not loaded"));
    assert!(converted.report.skipped.contains_key("global: created form (0xFF)"));
    assert!(converted.report.skipped.contains_key("global_table: type 6 layout unmapped"));
    assert!(converted.report.skipped.contains_key("changed_form: created REFR (0xFF) unsupported"));

    // Write real .nvfsave file and verify via fnv_save::read_header + read_body + GameSnapshot::check + Game::restore.
    let dir = std::env::temp_dir().join(format!("nvf-fos-convert-test-{}", std::process::id()));
    let out = dir.join("converted.nvfsave");
    let body_bytes = serde_json::to_vec(&converted.body).unwrap();
    fnv_save::write(&out, &converted.header, &converted.shot, &body_bytes, false).unwrap();

    let head_only = fnv_save::read_header(&out).unwrap();
    assert_eq!(head_only.header.player_name, "Synthetic");
    assert_eq!(head_only.header.level, 4);
    assert_eq!(head_only.header.location, "OwnTestRoom");
    assert_eq!(head_only.header.play_seconds, 2.0 * 3600.0 + 15.0 * 60.0 + 30.0);
    assert_eq!(head_only.header.game_year, 2281);
    assert_eq!(head_only.header.game_month, 9);
    assert_eq!(head_only.header.game_day, 19);
    assert_eq!(head_only.header.game_hour, 14.5);
    assert_eq!(head_only.header.game_days_passed, Some(9.25));
    assert_eq!(head_only.header.weather.as_deref(), Some("OwnClearWeather"));
    assert_eq!(head_only.header.place.interior, Some(ROOM_ID));
    assert_eq!(head_only.header.place.feet, [12.0, 34.0, 56.0]);
    assert!(!head_only.shot.is_empty());
    assert_eq!(&head_only.shot[..8], b"\x89PNG\r\n\x1a\n");

    let (_h, _shot, body): (SaveHeader, Vec<u8>, SaveBody<GameSnapshot>) = fnv_save::read_body(&out).unwrap();
    assert_eq!(body.format, BODY_FORMAT);
    assert_eq!(body.engine.format, ENGINE_FORMAT);
    assert_eq!(body.engine.active_quest, Some(QUEST_ID));
    body.game.check().unwrap();

    let restored_world = World::restore(&f.forms, &f.refs, &body.game.sim).unwrap();
    let mut restored = Game::restore(&f.data, restored_world, GameConfig::default(), &body.game).unwrap();
    assert_eq!(restored.world.global(FormId(GLOB_CUSTOM)), Ok(42.0));
    assert_eq!(restored.player.name.as_deref(), Some("Synthetic"));
    assert_eq!(restored.player_level(), 4.0);
    assert_eq!(restored.player.xp, 1200.0);
    assert_eq!(restored.extra.get(&FormId::PLAYER_REF).unwrap().actor_values.get(&23), Some(&250.0));
    assert_eq!(restored.extra.get(&FormId::PLAYER_REF).unwrap().actor_values.get(&5), Some(&6.0));
    assert_eq!(restored.extra.get(&FormId::PLAYER_REF).unwrap().actor_values.get(&11), Some(&10.0));
    assert!(restored.player.tag_skills.contains(&38));
    assert_eq!(restored.player.misc_stats.get(&0), Some(&3.0));
    assert_eq!(restored.player.misc_stats.get(&2), Some(&11.0));
    assert_eq!(restored.world.item_count(FormId::PLAYER_REF, FormId(ITEM_ID)).unwrap(), 3);
    let inv = restored.world.peek(FormId::PLAYER_REF).unwrap().inventory.as_ref().unwrap();
    assert!(inv.is_equipped(FormId(ITEM_WORN_ID)));
    let cond = inv
        .groups()
        .find(|(id, _, _)| *id == FormId(ITEM_WORN_ID))
        .and_then(|(_, e, _)| e.condition);
    assert_eq!(cond, Some(0.5));
    let qs = restored.quests.get(&FormId(QUEST_ID)).unwrap();
    assert!(qs.running && !qs.completed);
    assert_eq!(qs.delay, Some(2.5));
    assert!(qs.stages_done.contains(&20));
    assert!(qs.objectives[&10].displayed && qs.objectives[&10].completed);
    let q_inst = restored.instances.get(&InstKey::Quest { quest: FormId(QUEST_ID) }).unwrap();
    assert_eq!(q_inst.vars.get(&1), Some(&Value::Num(7.5)));
    assert_eq!(q_inst.vars.get(&2), Some(&Value::Ref(NPC_REF)));
    assert!(!restored.world.peek(FormId(PROP_REF)).unwrap().enabled());
    assert!(restored.world.peek(FormId(PROP_REF)).unwrap().deleted());
    assert!(restored.extra.get(&FormId(NPC_REF)).unwrap().talked_to_pc);
    assert_eq!(restored.world.peek(FormId(NPC_REF)).unwrap().pos, [100.0, 200.0, 5.0]);
    assert_eq!(restored.forms.faction_reactions[&FormId(FACT_A)][&FormId(FACT_B)], 2);
    assert_eq!(restored.form_lists[&FormId(FLST_ID)], [FormId(ITEM_ID)]);
    assert_eq!(restored.world.leveled_added(FormId(LVLI_ID)).len(), 1);
    assert_eq!(restored.player.reputation[&FormId(REPU_ID)], [3.0, 15.0]);
    assert!(restored.player.challenges[&FormId(CHAL_ID)].completed);
    assert_eq!(restored.player.challenges[&FormId(CHAL_ID)].progress, 5);
    assert!(restored.player.notes.contains(&FormId(NOTE_ID)));
    assert!(restored.said_once(FormId(INFO_ID)));
    assert_eq!(restored.weather.shown, Some(FormId(WTHR_ID)));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn malformed_truncated_and_unknown_extra_data_are_handled_without_panicking() {
    let f = fixture();
    assert!(Fos::parse(b"NOTAFOS").is_err());
    let valid = synthetic_fos_bytes();
    assert!(Fos::parse(&valid[..valid.len() / 2]).is_err());

    // Mutate the player's inventory extra type from 0x25 to an unknown type 0xFE:
    // conversion must succeed, keep the rest of the save, and count inventory as skipped.
    let mut fos = Fos::parse(&valid).unwrap();
    let player_rec = fos.changed.iter_mut().find(|c| c.form == PLAYER_REF).unwrap();
    let last_extra_pos = player_rec.data.len() - 7;
    assert_eq!(player_rec.data[last_extra_pos], 0x25);
    player_rec.data[last_extra_pos] = 0xFE;
    let opts = ConvertOptions {
        plugins: vec![PluginEntry { name: "FalloutNV.esm".into(), size: 1234 }],
        include_thumbnail: false,
        ..ConvertOptions::default()
    };
    let converted = convert_fos(&fos, &f.data, &f.forms, &f.refs, &opts).unwrap();
    assert!(converted.shot.is_empty());
    assert!(converted.report.skipped.contains_key("player_inventory: not read (missing player record or unmapped layout)"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_item_conditions (extra 0x25) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_equipped (extra 0x16) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("thumbnail_rgb_order (header screenshot) [hypothesis]"));
    assert_eq!(converted.report.converted.get("quests"), Some(&1));
    converted.body.game.check().unwrap();

    // Regression: i32::MIN inventory delta must skip cleanly without negation overflow,
    // and non-empty TYPE_INFO/TYPE_NOTE flag-31 records must be reported as skipped.
    let mut edge_fos = Fos::parse(&valid).unwrap();
    let edge_player = edge_fos.changed.iter_mut().find(|c| c.form == PLAYER_REF).unwrap();
    // First item in synthetic_fos_bytes() has count = 3i32 right before its 0-lists vsval.
    let item1_count_pos = edge_player.data.windows(5).position(|w| w == [3, 0, 0, 0, BAR]).unwrap();
    edge_player.data[item1_count_pos..item1_count_pos + 4].copy_from_slice(&i32::MIN.to_le_bytes());
    let info_rec = edge_fos.changed.iter_mut().find(|c| c.kind == TYPE_INFO).unwrap();
    info_rec.data.push(0xAA);
    let note_rec = edge_fos.changed.iter_mut().find(|c| c.kind == TYPE_NOTE).unwrap();
    note_rec.data.push(0xBB);
    let edge_converted = convert_fos(&edge_fos, &f.data, &f.forms, &f.refs, &opts).unwrap();
    assert!(edge_converted.report.skipped.contains_key("player_item: negative item count overflow"));
    assert!(edge_converted.report.skipped.contains_key("changed_form: INFO (kind 8) unmapped"));
    assert!(edge_converted.report.skipped.contains_key("changed_form: NOTE (kind 31) unmapped"));
    assert!(!edge_converted.report.hypothesis_fields.contains_key("notes (NOTE bit 31) [hypothesis]"));
    assert!(!edge_converted.report.hypothesis_fields.contains_key("said_infos (INFO bit 31) [hypothesis]"));
    edge_converted.body.game.check().unwrap();
}

#[test]
fn midgame_player_extras_weapon_magazine_and_dead_counts_convert_and_restore() {
    let f = fixture();
    // 1-based form ID array
    let fids = [
        ROOM_ID,      // 1
        PLAYER_REF,   // 2
        WEAP_ID,      // 3
        AMMO_ID,      // 4
        NPC_BASE,     // 5
        PROP_REF,     // 6
        SCRIPT_ID,    // 7
        NPC_REF,      // 8
    ];

    let mut h = Build { b: Vec::new() };
    h.u32(0x30).bar();
    h.b.extend([0u8; 64]);
    h.bar()
        .u32(0)
        .bar()
        .u32(0)
        .bar()
        .u32(20)
        .bar()
        .s_field("Midgame", true)
        .s_field("Wanderer", true)
        .u32(12)
        .bar()
        .s_field("OwnTestRoom", true)
        .s_field("010.05.00", false);

    let mut b = b"FO3SAVEGAME".to_vec();
    b.extend((h.b.len() as u32).to_le_bytes());
    b.extend(&h.b);

    let mut pl = Build { b: Vec::new() };
    pl.u8(1).bar().s_field("FalloutNV.esm", true);
    b.push(27);
    b.extend((pl.b.len() as u32).to_le_bytes());
    b.extend(&pl.b);

    let table_at = b.len();
    b.extend([0u8; 32 + 78]);

    let g1 = b.len();
    // Global table 1 type 2: 1 dead_count entry (NPC_BASE = r(5), count = 7) + 105 unmapped trailing bytes.
    let mut t2 = Build { b: Vec::new() };
    t2.vs(1).bar().r(5).bar().u16(7).bar();
    t2.b.extend([0u8; 105]);
    b.extend(2u32.to_le_bytes());
    b.extend((t2.b.len() as u32).to_le_bytes());
    b.extend(&t2.b);

    let cf = b.len();
    // Player ACHR with f32 = 1.1 and 5-entry actor extra-data list (0x5E, 0x74, 0x1D, 0x60, 0x75; no 0x18).
    let mut pl_rec = Build { b: Vec::new() };
    pl_rec.r(1).f32(1.0).f32(2.0).f32(3.0).f32(0.0).f32(0.0).f32(0.0).bar();
    for _ in 0..232 {
        pl_rec.f32(0.0).bar();
    }
    pl_rec.u8(0).bar().f32(1.1).bar().vs(5).bar();
    pl_rec.u8(0x5E).bar().vs(0).bar();
    pl_rec.u8(0x74).bar().r(8).bar();
    pl_rec.u8(0x1D).bar().vs(2).bar().r(6).bar().r(8).bar();
    pl_rec.u8(0x60).bar().u32(42).bar();
    pl_rec.u8(0x75).bar().r(6).bar().r(8).bar().u8(1).bar();

    // 1 inventory item (WEAP_ID = r(3)), count = 4, 2 extra lists covering all 9 observed item extra types:
    //   list 1 (worn modded weapon): 0x16, 0x25 (150/200 = 0.75), 0x8D (mods = 3), 0x6E (AMMO_ID = r(4), 9 rounds),
    //                                0x4A (hotkey = 1), 0x0D (script r(7) with 1 num + 1 ref var)
    //   list 2 (unworn owned sub-stack of 2): 0x24 (count = 2), 0x21 (owner NPC_BASE = r(5)),
    //                                         0x1C (orig ref PROP_REF = r(6)), 0x25 (100/200 = 0.5)
    //   + 1 remaining plain unit (total 1 + 2 + 1 = 4)
    pl_rec.vs(1).bar();
    pl_rec.r(3).bar().i32(4).bar().vs(2).bar();
    pl_rec.vs(6).bar();
    pl_rec.u8(0x16).bar();
    pl_rec.u8(0x25).bar().f32(150.0).bar();
    pl_rec.u8(0x8D).bar().u8(0x03).bar();
    pl_rec.u8(0x6E).bar().r(4).bar().u32(9).bar();
    pl_rec.u8(0x4A).bar().u8(1).bar();
    pl_rec
        .u8(0x0D)
        .bar()
        .r(7)
        .bar()
        .vs(2)
        .bar()
        .u32(1)
        .bar()
        .f64(3.25)
        .bar()
        .u32(0x8000_0002)
        .bar()
        .r(8)
        .bar()
        .vs(0)
        .bar()
        .vs(0)
        .bar();
    pl_rec.vs(4).bar();
    pl_rec.u8(0x24).bar().i16(2).bar();
    pl_rec.u8(0x21).bar().r(5).bar();
    pl_rec.u8(0x1C).bar().r(6).bar();
    pl_rec.u8(0x25).bar().f32(100.0).bar();
    push_record(&mut b, 2, (1 << 1) | (1 << 4) | (1 << 5) | (1 << 29), TYPE_ACHR, &pl_rec.b);

    let g2 = b.len();
    b.extend(1000u32.to_le_bytes());
    b.extend(0u32.to_le_bytes());

    let fid = b.len();
    b.extend((fids.len() as u32).to_le_bytes());
    for fid_val in fids {
        b.extend(fid_val.to_le_bytes());
    }
    b.extend(0u32.to_le_bytes());
    let tail = b.len();
    b.extend([5, 0, 0, 0, 0, 0, 0, 0, BAR]);

    let table = [fid, tail, g1, cf, g2, 1, 1, 1].map(|v| v as u32);
    for (i, v) in table.iter().enumerate() {
        b[table_at + 4 * i..table_at + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }

    let fos = Fos::parse(&b).unwrap();
    let inspect = inspect_fos(&fos, false);
    assert_eq!(inspect.decoded_counts.get("dead_counts"), Some(&1));
    assert_eq!(inspect.player_inventory_items, Some(1));

    let opts = ConvertOptions {
        kind: SaveKind::Manual,
        plugins: vec![PluginEntry { name: "FalloutNV.esm".into(), size: 1234 }],
        include_thumbnail: false,
        cosave_present: false,
    };
    let converted = convert_fos(&fos, &f.data, &f.forms, &f.refs, &opts).unwrap();
    assert_eq!(converted.report.converted.get("dead_counts"), Some(&1));
    assert_eq!(converted.report.converted.get("player_items"), Some(&1));
    assert_eq!(converted.report.converted.get("player_equipped"), Some(&1));
    assert_eq!(converted.report.converted.get("player_weapon_magazine"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("dead_counts (global type 2 prefix) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("player_item_weapon_mods (extra 0x8D) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("player_item_owners (extra 0x21) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("player_item_stack_counts (extra 0x24) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("player_weapon_magazine (extra 0x6E) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.hypothesis_fields.get("player_item_conditions (extra 0x25) [hypothesis]"), Some(&2));
    assert_eq!(converted.report.hypothesis_fields.get("player_equipped (extra 0x16) [hypothesis]"), Some(&1));
    assert_eq!(converted.report.skipped.get("player_hotkey: not persisted in NVFSAVE v1"), Some(&1));
    assert_eq!(converted.report.skipped.get("global_table: type 2 trailing bytes unmapped"), Some(&1));

    // Write .nvfsave and verify round-trip through fnv_save::read_body + World::restore + Game::restore.
    let dir = std::env::temp_dir().join(format!("nvf-fos-convert-midgame-{}", std::process::id()));
    let out = dir.join("midgame.nvfsave");
    let body_bytes = serde_json::to_vec(&converted.body).unwrap();
    fnv_save::write(&out, &converted.header, &converted.shot, &body_bytes, false).unwrap();

    let (_h, _shot, body): (SaveHeader, Vec<u8>, SaveBody<GameSnapshot>) = fnv_save::read_body(&out).unwrap();
    assert_eq!(body.engine.magazine, Some((WEAP_ID, 9)));
    assert_eq!(body.engine.magazine_ammo, Some(AMMO_ID));
    body.game.check().unwrap();

    let restored_world = World::restore(&f.forms, &f.refs, &body.game.sim).unwrap();
    let mut restored = Game::restore(&f.data, restored_world, GameConfig::default(), &body.game).unwrap();
    assert_eq!(restored.player.dead_count.get(&FormId(NPC_BASE)), Some(&7));
    assert_eq!(restored.world.item_count(FormId::PLAYER_REF, FormId(WEAP_ID)).unwrap(), 4);

    let inv = restored.world.peek(FormId::PLAYER_REF).unwrap().inventory.as_ref().unwrap();
    assert!(inv.is_equipped(FormId(WEAP_ID)));
    let groups: Vec<_> = inv
        .groups()
        .filter(|(id, _, _)| *id == FormId(WEAP_ID))
        .map(|(_, e, n)| (e, n))
        .collect();
    assert_eq!(groups.len(), 3);
    assert!(groups.iter().any(|(e, n)| e.equipped && e.mods == 3 && e.condition == Some(0.75) && *n == 1));
    assert!(groups
        .iter()
        .any(|(e, n)| !e.equipped && e.owner == Some(FormId(NPC_BASE)) && e.condition == Some(0.5) && *n == 2));
    assert!(groups.iter().any(|(e, n)| e.is_plain() && *n == 1));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn container_stashes_map_markers_and_created_refs_convert_and_restore() {
    let f = fixture();
    // 1-based form ID array
    let fids = [
        ROOM_ID,          // 1
        PLAYER_REF,       // 2
        ITEM_ID,          // 3
        WEAP_ID,          // 4
        CONT_REF,         // 5
        CONT28_REF,       // 6
        MARKER_VIS_REF,   // 7
        MARKER_FOUND_REF, // 8
        PROP_REF,         // 9
        NPC_REF,          // 10
        0xFF00_0099,      // 11: created REFR
        0xFF00_009A,      // 12: created ACRE (still unsupported)
    ];

    let mut h = Build { b: Vec::new() };
    h.u32(0x30).bar();
    h.b.extend([0u8; 64]);
    h.bar()
        .u32(0)
        .bar()
        .u32(0)
        .bar()
        .u32(25)
        .bar()
        .s_field("WorldTest", true)
        .s_field("Wanderer", true)
        .u32(10)
        .bar()
        .s_field("OwnTestRoom", true)
        .s_field("005.00.00", false);

    let mut b = b"FO3SAVEGAME".to_vec();
    b.extend((h.b.len() as u32).to_le_bytes());
    b.extend(&h.b);

    let mut pl = Build { b: Vec::new() };
    pl.u8(1).bar().s_field("FalloutNV.esm", true);
    b.push(27);
    b.extend((pl.b.len() as u32).to_le_bytes());
    b.extend(&pl.b);

    let table_at = b.len();
    b.extend([0u8; 32 + 78]);

    let g1 = b.len();
    let cf = b.len();

    // 1. Container CONT_REF (r(5), bits 5+27): remove 1 ITEM_ID (base 3 -> 2), add 2 WEAP_ID
    //    (1 modded at 100/200 = 0.5 with extra 0x2F, 1 plain).
    let mut c1 = Build { b: Vec::new() };
    c1.vs(2).bar();
    c1.r(3).bar().i32(-1).bar().vs(0).bar();
    c1.r(4).bar().i32(2).bar().vs(1).bar().vs(3).bar();
    c1.u8(0x2F).bar().u32(0x0000_0100).bar().u8(1).bar();
    c1.u8(0x25).bar().f32(100.0).bar();
    c1.u8(0x8D).bar().u8(0x02).bar();
    push_record(&mut b, 5, (1 << 5) | (1 << 27), TYPE_REFR, &c1.b);

    // 2. Container CONT28_REF (r(6), bits 5+27+28): add 4 ITEM_ID + 13-byte bit-28 tail.
    let mut c2 = Build { b: Vec::new() };
    c2.vs(1).bar().r(3).bar().i32(4).bar().vs(0).bar();
    c2.b.extend([0u8; 13]);
    push_record(&mut b, 6, (1 << 5) | (1 << 27) | (1 << 28), TYPE_REFR, &c2.b);

    // 3. Map marker MARKER_VIS_REF (r(7), bit 31): 0x2C = 0x01 (visible only).
    let mut m1 = Build { b: Vec::new() };
    m1.vs(1).bar().u8(0x2C).bar().u8(0x01).bar();
    push_record(&mut b, 7, 1 << 31, TYPE_REFR, &m1.b);

    // 4. Map marker MARKER_FOUND_REF (r(8), bits 29+31): 0x2C = 0x03 + 0x74 companion ref.
    let mut m2 = Build { b: Vec::new() };
    m2.vs(2).bar().u8(0x2C).bar().u8(0x03).bar().u8(0x74).bar().r(10).bar();
    push_record(&mut b, 8, (1 << 29) | (1 << 31), TYPE_REFR, &m2.b);

    // 5. Non-marker bit-31 REFR PROP_REF (r(9)): must NOT error, must stay counted as unmapped.
    let mut nm = Build { b: Vec::new() };
    nm.vs(1).bar().u8(0x56).bar().u32(7).bar();
    push_record(&mut b, 9, 1 << 31, TYPE_REFR, &nm.b);

    // 6. Created 0xFF00_0099 REFR (r(11), bit 1): 32-byte header placing ITEM_ID in ROOM_ID.
    let mut cr = Build { b: Vec::new() };
    cr.r(1);
    for v in [11.0f32, 22.0, 33.0, 0.0, 0.0, 1.0] {
        cr.f32(v);
    }
    cr.u8(0x01).r(3).bar();
    assert_eq!(cr.b.len(), 32);
    push_record(&mut b, 11, 1 << 1, TYPE_REFR, &cr.b);

    // 7. Created 0xFF00_009A ACRE (r(12), bit 1): still reported unsupported.
    push_record(&mut b, 12, 1 << 1, TYPE_ACRE, &[0u8; 8]);

    let g2 = b.len();
    b.extend(1000u32.to_le_bytes());
    b.extend(0u32.to_le_bytes());

    let fid = b.len();
    b.extend((fids.len() as u32).to_le_bytes());
    for fid_val in fids {
        b.extend(fid_val.to_le_bytes());
    }
    b.extend(0u32.to_le_bytes());
    let tail = b.len();
    b.extend([5, 0, 0, 0, 0, 0, 0, 0, BAR]);

    let table = [fid, tail, g1, cf, g2, 0, 1, 7].map(|v| v as u32);
    for (i, v) in table.iter().enumerate() {
        b[table_at + 4 * i..table_at + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }

    let fos = Fos::parse(&b).unwrap();
    let inspect = inspect_fos(&fos, false);
    assert_eq!(inspect.decoded_counts.get("container_inventories"), Some(&2));
    assert_eq!(inspect.decoded_counts.get("map_markers"), Some(&2));
    assert_eq!(inspect.decoded_counts.get("created_refs"), Some(&1));
    assert!(inspect.parse_errors.is_empty());

    let opts = ConvertOptions {
        kind: SaveKind::Manual,
        plugins: vec![PluginEntry { name: "FalloutNV.esm".into(), size: 1234 }],
        include_thumbnail: false,
        cosave_present: false,
    };
    let converted = convert_fos(&fos, &f.data, &f.forms, &f.refs, &opts).unwrap();
    assert_eq!(converted.report.converted.get("containers"), Some(&2));
    assert_eq!(converted.report.converted.get("container_items"), Some(&3));
    assert_eq!(converted.report.converted.get("map_markers"), Some(&2));
    assert_eq!(converted.report.converted.get("created_refs"), Some(&1));
    assert_eq!(
        converted.report.hypothesis_fields.get("container_inventories (REFR bits 5/27) [hypothesis]"),
        Some(&2)
    );
    assert_eq!(
        converted.report.hypothesis_fields.get("map_markers (REFR extra 0x2C) [hypothesis]"),
        Some(&2)
    );
    assert_eq!(
        converted.report.hypothesis_fields.get("created_refs (0xFF REFR 32B header) [hypothesis]"),
        Some(&1)
    );
    // Converted created REFR is NOT double-counted as unsupported; created ACRE and non-marker bit-31 REFR are.
    assert!(!converted.report.skipped.contains_key("changed_form: created REFR (0xFF) unsupported"));
    assert_eq!(converted.report.skipped.get("changed_form: created ACRE (0xFF) unsupported"), Some(&1));
    assert_eq!(converted.report.skipped.get("changed_form: REFR (kind 0) unmapped"), Some(&1));

    // Write .nvfsave and verify round-trip through read_body + World::restore + Game::restore.
    let dir = std::env::temp_dir().join(format!("nvf-fos-convert-world-r4a-{}", std::process::id()));
    let out = dir.join("world_r4a.nvfsave");
    let body_bytes = serde_json::to_vec(&converted.body).unwrap();
    fnv_save::write(&out, &converted.header, &converted.shot, &body_bytes, false).unwrap();

    let (_h, _shot, body): (SaveHeader, Vec<u8>, SaveBody<GameSnapshot>) = fnv_save::read_body(&out).unwrap();
    assert_eq!(body.engine.extra.get("map_markers_found"), Some(&serde_json::json!([MARKER_FOUND_REF])));
    body.game.check().unwrap();

    let restored_world = World::restore(&f.forms, &f.refs, &body.game.sim).unwrap();
    let mut restored = Game::restore(&f.data, restored_world, GameConfig::default(), &body.game).unwrap();

    // Container inventories: CONT_REF has 3 - 1 = 2 ITEM_ID and 2 WEAP_ID (1 modded at 0.5, 1 plain);
    // CONT28_REF has 3 + 4 = 7 ITEM_ID.
    assert_eq!(restored.world.item_count(FormId(CONT_REF), FormId(ITEM_ID)).unwrap(), 2);
    assert_eq!(restored.world.item_count(FormId(CONT_REF), FormId(WEAP_ID)).unwrap(), 2);
    let cont_inv = restored.world.peek(FormId(CONT_REF)).unwrap().inventory.as_ref().unwrap();
    let weap_groups: Vec<_> = cont_inv
        .groups()
        .filter(|(id, _, _)| *id == FormId(WEAP_ID))
        .map(|(_, e, n)| (e, n))
        .collect();
    assert_eq!(weap_groups.len(), 2);
    assert!(weap_groups.iter().any(|(e, n)| !e.equipped && e.mods == 2 && e.condition == Some(0.5) && *n == 1));
    assert!(weap_groups.iter().any(|(e, n)| e.is_plain() && *n == 1));
    assert_eq!(restored.world.item_count(FormId(CONT28_REF), FormId(ITEM_ID)).unwrap(), 7);

    // Map markers: MARKER_VIS_REF visible-only; MARKER_FOUND_REF visible + travel + found.
    assert!(restored.player.map_markers.contains(&FormId(MARKER_VIS_REF)));
    assert!(!restored.player.travel_markers.contains(&FormId(MARKER_VIS_REF)));
    assert!(!restored.player.found_markers.contains(&FormId(MARKER_VIS_REF)));
    assert!(restored.player.map_markers.contains(&FormId(MARKER_FOUND_REF)));
    assert!(restored.player.travel_markers.contains(&FormId(MARKER_FOUND_REF)));
    assert!(restored.player.found_markers.contains(&FormId(MARKER_FOUND_REF)));

    // Created 0xFF REFR was placed into ROOM_ID at [11.0, 22.0, 33.0] with base ITEM_ID.
    let created = restored.world.peek(FormId::FIRST_DYNAMIC).expect("created ref placed");
    assert_eq!(created.base, FormId(ITEM_ID));
    assert_eq!(created.cell, Some(FormId(ROOM_ID)));
    assert_eq!(created.pos, [11.0, 22.0, 33.0]);
    assert_eq!(created.rot, [0.0, 0.0, 1.0]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn non_finite_fos_floats_are_sanitized_and_round_trip_cleanly() {
    let f = fixture();
    // 1-based form ID array
    let fids = [
        GLOB_HOUR,     // 1
        GLOB_CUSTOM,   // 2
        QUEST_ID,      // 3
        ROOM_ID,       // 4
        PLAYER_REF,    // 5
        NPC_REF,       // 6
        ITEM_WORN_ID,  // 7
        WEAP_ID,       // 8
        CONT_REF,      // 9
        LVLI_ID,       // 10
        REPU_ID,       // 11
        ITEM_ID,       // 12
        0xFF00_00AA,   // 13: created REFR with NaN pos
    ];

    let mut h = Build { b: Vec::new() };
    h.u32(0x30).bar();
    h.b.extend([0u8; 64]);
    h.bar()
        .u32(0)
        .bar()
        .u32(0)
        .bar()
        .u32(30)
        .bar()
        .s_field("Sanitized", true)
        .s_field("Wanderer", true)
        .u32(6)
        .bar()
        .s_field("OwnTestRoom", true)
        .s_field("001.02.03", false);

    let mut b = b"FO3SAVEGAME".to_vec();
    b.extend((h.b.len() as u32).to_le_bytes());
    b.extend(&h.b);

    let mut pl = Build { b: Vec::new() };
    pl.u8(1).bar().s_field("FalloutNV.esm", true);
    b.push(27);
    b.extend((pl.b.len() as u32).to_le_bytes());
    b.extend(&pl.b);

    let table_at = b.len();
    b.extend([0u8; 32 + 78]);

    let g1 = b.len();
    // Global type 3: 2 non-finite globals (GLOB_HOUR = -Inf, GLOB_CUSTOM = NaN)
    let mut t3 = Build { b: Vec::new() };
    t3.vs(2)
        .bar()
        .r(1)
        .bar()
        .f32(f32::NEG_INFINITY)
        .bar()
        .r(2)
        .bar()
        .f32(f32::NAN)
        .bar();
    b.extend(3u32.to_le_bytes());
    b.extend((t3.b.len() as u32).to_le_bytes());
    b.extend(&t3.b);

    let cf = b.len();
    // 1. QUST (r(3)): NaN delay + 3 script vars: var 1 = NaN, var 3 = +Inf, var 4 = 12.5 (valid)
    let mut q = Build { b: Vec::new() };
    q.u8(0x01).bar().f32(f32::NAN).bar();
    q.vs(0).bar();
    q.vs(3)
        .bar()
        .u32(1)
        .bar()
        .f64(f64::NAN)
        .bar()
        .u32(3)
        .bar()
        .f64(f64::INFINITY)
        .bar()
        .u32(4)
        .bar()
        .f64(12.5)
        .bar()
        .vs(0)
        .bar()
        .vs(0)
        .bar();
    q.vs(0).bar();
    push_record(&mut b, 3, (1 << 1) | (1 << 2) | (1 << 31) | (1 << 30) | (1 << 29), TYPE_QUST, &q.b);

    // 2. Player ACHR (r(5)): non-finite rot[2] = +Inf, AV2 XP = NaN, worn item with condition (0x25) = +Inf
    let mut pl_rec = Build { b: Vec::new() };
    pl_rec.r(4).f32(1.0).f32(2.0).f32(3.0).f32(0.0).f32(0.0).f32(f32::INFINITY).bar();
    for i in 0..232 {
        let v = if i == 77 + 24 { f32::NAN } else { 0.0 };
        pl_rec.f32(v).bar();
    }
    pl_rec.u8(0).bar().f32(1.0).bar().vs(0).bar();
    pl_rec.vs(1).bar();
    pl_rec
        .r(7)
        .bar()
        .i32(1)
        .bar()
        .vs(1)
        .bar()
        .vs(2)
        .bar()
        .u8(0x16)
        .bar()
        .u8(0x25)
        .bar()
        .f32(f32::INFINITY)
        .bar();
    push_record(&mut b, 5, (1 << 1) | (1 << 4) | (1 << 5), TYPE_ACHR, &pl_rec.b);

    // 3. NPC_REF (r(6)): place with NaN pos
    let mut nr = Build { b: Vec::new() };
    nr.r(4).f32(f32::NAN).f32(0.0).f32(0.0).f32(0.0).f32(0.0).f32(0.0).bar();
    nr.u8(0).bar();
    push_record(&mut b, 6, 1 << 1, TYPE_ACHR, &nr.b);

    // 4. Container CONT_REF (r(9)): item with condition (0x25) = NaN
    let mut cont = Build { b: Vec::new() };
    cont.vs(1).bar();
    cont.r(8).bar().i32(1).bar().vs(1).bar().vs(1).bar().u8(0x25).bar().f32(f32::NAN).bar();
    push_record(&mut b, 9, (1 << 5) | (1 << 27), TYPE_REFR, &cont.b);

    // 5. LVLI (r(10)): entry with condition = NaN
    let mut lv = Build { b: Vec::new() };
    lv.vs(1).bar().r(12).bar().u16(1).bar().u16(1).bar().f32(f32::NAN).bar();
    push_record(&mut b, 10, 1 << 31, TYPE_LVLI, &lv.b);

    // 6. REPU (r(11)): fame = NaN, infamy = 1.0
    let mut rp = Build { b: Vec::new() };
    rp.f32(f32::NAN).bar().f32(1.0).bar();
    push_record(&mut b, 11, 1 << 1, TYPE_REPU, &rp.b);

    // 7. Created 0xFF REFR (r(13)): 32-byte header with NaN pos[0]
    let mut cr = Build { b: Vec::new() };
    cr.r(4);
    for v in [f32::NAN, 2.0, 3.0, 0.0, 0.0, 0.0] {
        cr.f32(v);
    }
    cr.u8(0x01).r(12).bar();
    assert_eq!(cr.b.len(), 32);
    push_record(&mut b, 13, 1 << 1, TYPE_REFR, &cr.b);

    let g2 = b.len();
    b.extend(1000u32.to_le_bytes());
    b.extend(0u32.to_le_bytes());

    let fid = b.len();
    b.extend((fids.len() as u32).to_le_bytes());
    for fid_val in fids {
        b.extend(fid_val.to_le_bytes());
    }
    b.extend(0u32.to_le_bytes());
    let tail = b.len();
    b.extend([5, 0, 0, 0, 0, 0, 0, 0, BAR]);

    let table = [fid, tail, g1, cf, g2, 1, 1, 7].map(|v| v as u32);
    for (i, v) in table.iter().enumerate() {
        b[table_at + 4 * i..table_at + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }

    let fos = Fos::parse(&b).unwrap();
    let opts = ConvertOptions {
        kind: SaveKind::Manual,
        plugins: vec![PluginEntry { name: "FalloutNV.esm".into(), size: 1234 }],
        include_thumbnail: false,
        cosave_present: false,
    };
    let converted = convert_fos(&fos, &f.data, &f.forms, &f.refs, &opts).unwrap();

    // Every non-finite float was sanitized and truthfully reported in report.skipped.
    assert_eq!(converted.report.skipped.get("quest_variable: non-finite value"), Some(&2));
    assert_eq!(converted.report.skipped.get("quest_delay: non-finite value"), Some(&1));
    assert_eq!(converted.report.skipped.get("global: non-finite value"), Some(&2));
    assert_eq!(converted.report.skipped.get("player_av: non-finite value"), Some(&1));
    assert_eq!(converted.report.skipped.get("player_place: non-finite transform"), Some(&1));
    assert_eq!(converted.report.skipped.get("reference_place: non-finite transform"), Some(&1));
    // Fos::created_refs already rejects non-finite 32-byte placement headers.
    assert_eq!(converted.report.skipped.get("created_ref: 32-byte header not understood"), Some(&1));
    assert_eq!(converted.report.skipped.get("player_item_condition: non-finite value"), Some(&1));
    assert_eq!(converted.report.skipped.get("container_item_condition: non-finite value"), Some(&1));
    assert_eq!(converted.report.skipped.get("leveled_list_condition: non-finite value"), Some(&1));
    assert_eq!(converted.report.skipped.get("reputation: non-finite value"), Some(&1));

    // Write .nvfsave and prove full typed SaveBody<GameSnapshot> round-trip succeeds without null floats.
    let dir = std::env::temp_dir().join(format!("nvf-fos-convert-nan-{}", std::process::id()));
    let out = dir.join("sanitized.nvfsave");
    let body_bytes = serde_json::to_vec(&converted.body).unwrap();
    fnv_save::write(&out, &converted.header, &converted.shot, &body_bytes, false).unwrap();

    let (_h, _shot, body): (SaveHeader, Vec<u8>, SaveBody<GameSnapshot>) = fnv_save::read_body(&out).unwrap();
    body.game.check().unwrap();
    let restored_world = World::restore(&f.forms, &f.refs, &body.game.sim).unwrap();
    let restored = Game::restore(&f.data, restored_world, GameConfig::default(), &body.game).unwrap();

    // Valid quest var 4 survived while NaN (1) and +Inf (3) were omitted cleanly.
    let q_inst = restored.instances.get(&InstKey::Quest { quest: FormId(QUEST_ID) }).unwrap();
    assert_eq!(q_inst.vars.get(&1), None);
    assert_eq!(q_inst.vars.get(&3), None);
    assert_eq!(q_inst.vars.get(&4), Some(&Value::Num(12.5)));
    assert_eq!(restored.quests.get(&FormId(QUEST_ID)).unwrap().delay, None);
    // Globals kept their finite ESM defaults instead of -Inf / NaN.
    assert_eq!(restored.world.global(FormId(GLOB_HOUR)), Ok(8.0));
    assert_eq!(restored.world.global(FormId(GLOB_CUSTOM)), Ok(0.0));
    // Worn item & container item survived with condition = None instead of Inf / NaN.
    let p_inv = restored.world.peek(FormId::PLAYER_REF).unwrap().inventory.as_ref().unwrap();
    assert!(p_inv.is_equipped(FormId(ITEM_WORN_ID)));
    assert!(p_inv.groups().all(|(_, e, _)| e.condition.is_none()));
    let c_inv = restored.world.peek(FormId(CONT_REF)).unwrap().inventory.as_ref().unwrap();
    assert!(c_inv.groups().all(|(_, e, _)| e.condition.is_none()));
    assert!(!restored.player.reputation.contains_key(&FormId(REPU_ID)));
    assert!(restored.world.peek(FormId::FIRST_DYNAMIC).is_none());

    let _ = std::fs::remove_dir_all(&dir);
}
