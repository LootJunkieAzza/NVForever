//! Synthetic clean-room round-trip & edge tests for `.fos` -> `.nvfsave` conversion.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::fos_convert::{convert_fos, inspect_fos, ConvertOptions};
use fnv_game::game::{Game, GameConfig, GameSnapshot, InstKey};
use fnv_game::GameData;
use fnv_save::fos::{
    Fos, FORM_DEAD, FORM_DELETED, FORM_DISABLED, FORM_TALKED_TO_PC, PLAYER_BASE, PLAYER_CLASS,
    PLAYER_REF, TYPE_ACHR, TYPE_ACRE, TYPE_CELL, TYPE_CHAL, TYPE_CLAS, TYPE_ECZN, TYPE_FACT,
    TYPE_FLST, TYPE_INFO, TYPE_LVLI, TYPE_NOTE, TYPE_NPC, TYPE_QUST, TYPE_REFR, TYPE_REPU,
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
const RACE_ID: u32 = 0x0000_0019;
const SPEL_ID: u32 = 0x0000_08C0;
const PERK_ID: u32 = 0x0000_08C1;
const TEAMMATE_PERK_ID: u32 = 0x0000_08C2;
const HAIR_ID: u32 = 0x0000_08C3;
const EYES_ID: u32 = 0x0000_08C4;
const HDPT_ID: u32 = 0x0000_08C5;
const KEY_ID: u32 = 0x0000_08C6;
const DOOR_SCRIPT_ID: u32 = 0x0000_08C7;
const NPC_SCRIPT_ID: u32 = 0x0000_08C8;
const DOOR_BASE: u32 = 0x0000_08C9;
const DOOR_REF: u32 = 0x0000_08CA;
const LOCKED_CONT_REF: u32 = 0x0000_08CB;
const ECZN_ID: u32 = 0x0000_08CC;
const COMPANION_BASE: u32 = 0x0000_08CD;
const COMPANION_REF: u32 = 0x0000_08CE;
const DEAD_NPC_REF: u32 = 0x0000_08CF;
const RADIO_REF: u32 = 0x0000_08D0;

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
    let script_rec = record(b"SCPT", 0, SCRIPT_ID, &scpt_subs);
    let mut door_scpt_subs = vec![sub(b"EDID", z("OwnDoorScript"))];
    door_scpt_subs.extend(unit(
        0,
        &script(&[(0, None, Vec::new())]),
        &[(1, "bOpened", 0), (2, "rLinker", 0), (3, "fBad", 0)],
        &[],
    ));
    let door_script = record(b"SCPT", 0, DOOR_SCRIPT_ID, &door_scpt_subs);
    let mut npc_scpt_subs = vec![sub(b"EDID", z("OwnCompanionScript"))];
    npc_scpt_subs.extend(unit(
        0,
        &script(&[(0, None, Vec::new())]),
        &[(1, "bHired", 0), (2, "rBoss", 0), (3, "fBad", 0)],
        &[],
    ));
    let npc_script = record(b"SCPT", 0, NPC_SCRIPT_ID, &npc_scpt_subs);
    let scripts_all = [script_rec, door_script, npc_script].concat();
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
    let npc_player = record(
        b"NPC_",
        0,
        PLAYER_BASE,
        &[sub(b"EDID", z("Player")), sub(b"RNAM", RACE_ID.to_le_bytes().to_vec())],
    );
    let npc_other = record(b"NPC_", 0, NPC_BASE, &[sub(b"EDID", z("OwnNpcBase"))]);
    let npc_companion = record(
        b"NPC_",
        0,
        COMPANION_BASE,
        &[
            sub(b"EDID", z("OwnCompanionBase")),
            sub(b"SCRI", NPC_SCRIPT_ID.to_le_bytes().to_vec()),
        ],
    );
    let race = record(b"RACE", 0, RACE_ID, &[sub(b"EDID", z("OwnRace"))]);
    let spel = record(b"SPEL", 0, SPEL_ID, &[sub(b"EDID", z("OwnAbility"))]);
    let perk = record(b"PERK", 0, PERK_ID, &[sub(b"EDID", z("OwnPerk"))]);
    let tm_perk = record(b"PERK", 0, TEAMMATE_PERK_ID, &[sub(b"EDID", z("OwnTeammatePerk"))]);
    let hair = record(b"HAIR", 0, HAIR_ID, &[sub(b"EDID", z("OwnHair"))]);
    let eyes = record(b"EYES", 0, EYES_ID, &[sub(b"EDID", z("OwnEyes"))]);
    let hdpt = record(b"HDPT", 0, HDPT_ID, &[sub(b"EDID", z("OwnHeadPart"))]);
    let keym = record(b"KEYM", 0, KEY_ID, &[sub(b"EDID", z("OwnKey"))]);
    let door_base = record(
        b"DOOR",
        0,
        DOOR_BASE,
        &[
            sub(b"EDID", z("OwnDoorBase")),
            sub(b"SCRI", DOOR_SCRIPT_ID.to_le_bytes().to_vec()),
        ],
    );
    let eczn = record(b"ECZN", 0, ECZN_ID, &[sub(b"EDID", z("OwnEncounterZone"))]);
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
    let door_ref = record(
        b"REFR",
        0,
        DOOR_REF,
        &[
            sub(b"EDID", z("OwnDoorRef")),
            sub(b"NAME", DOOR_BASE.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let locked_cont_ref = record(
        b"REFR",
        0,
        LOCKED_CONT_REF,
        &[
            sub(b"EDID", z("OwnLockedCrateRef")),
            sub(b"NAME", CONT_BASE.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let radio_ref = record(
        b"REFR",
        0,
        RADIO_REF,
        &[
            sub(b"EDID", z("OwnRadioRef")),
            sub(b"NAME", ITEM_ID.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let companion_ref = record(
        b"ACHR",
        0,
        COMPANION_REF,
        &[
            sub(b"EDID", z("OwnCompanionRef")),
            sub(b"NAME", COMPANION_BASE.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let dead_npc_ref = record(
        b"ACHR",
        0,
        DEAD_NPC_REF,
        &[
            sub(b"EDID", z("OwnDeadNpcRef")),
            sub(b"NAME", NPC_BASE.to_le_bytes().to_vec()),
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
            &[
                prop_ref,
                cont_ref,
                cont28_ref,
                locked_cont_ref,
                door_ref,
                radio_ref,
                marker_vis_ref,
                marker_found_ref,
                npc_ref,
                companion_ref,
                dead_npc_ref,
                player_ref,
            ]
            .concat(),
        ),
    );
    let bytes = [
        tes4(),
        top(b"GLOB", &globals),
        top(b"SCPT", &scripts_all),
        top(b"QUST", &quest),
        top(b"MISC", &item),
        top(b"ARMO", &armor),
        top(b"WEAP", &weap),
        top(b"AMMO", &ammo),
        top(b"KEYM", &keym),
        top(b"CONT", &cont_base),
        top(b"DOOR", &door_base),
        top(b"STAT", &marker_base),
        top(b"RACE", &race),
        top(b"HAIR", &hair),
        top(b"EYES", &eyes),
        top(b"HDPT", &hdpt),
        top(b"SPEL", &spel),
        top(b"PERK", &[perk, tm_perk].concat()),
        top(b"ECZN", &eczn),
        top(b"NPC_", &[npc_player, npc_other, npc_companion].concat()),
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
    assert_eq!(converted.report.hypothesis_fields.get("notes (NOTE bit 31) [hypothesis]"), Some(&1));
    // Confirmed [observed] fields are no longer counted in hypothesis_fields.
    assert!(!converted.report.hypothesis_fields.contains_key("active_quest (global type 1) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_special (NPC_ bit 2) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("quest_flags_objectives (QUST bits 1/29) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_xp (AV2 code 24) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_karma (AV2 code 23) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("reputations (REPU bit 1, fame/infamy) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("form_lists (FLST bit 31, u32 count) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("leveled_lists (LVLI bit 31) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("challenges (CHAL bit 1, progress/flags) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("said_infos (INFO bit 31) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_item_conditions (extra 0x25) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_equipped (extra 0x16) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("reference_form_flags (REFR/ACHR/ACRE bit 0) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("faction_relations (FACT bit 2) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_tag_skills (CLAS bit 1) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("misc_stats (global type 0) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("weather (global type 8) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("thumbnail_rgb_order (header screenshot) [hypothesis]"));
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
    assert_eq!(converted.report.hypothesis_fields.get("player_weapon_magazine (extra 0x6E) [hypothesis]"), Some(&1));
    assert!(!converted.report.hypothesis_fields.contains_key("player_item_owners (extra 0x21) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("dead_counts (global type 2 prefix) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_item_weapon_mods (extra 0x8D) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_item_stack_counts (extra 0x24) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_item_conditions (extra 0x25) [hypothesis]"));
    assert!(!converted.report.hypothesis_fields.contains_key("player_equipped (extra 0x16) [hypothesis]"));
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
    assert!(converted.report.hypothesis_fields.is_empty());
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

#[test]
fn v2_player_skills_limbs_perks_companions_world_and_radio_round_trip() {
    let f = fixture();
    // 1-based form ID array
    let fids = [
        ROOM_ID,          // 1
        PLAYER_REF,       // 2
        PLAYER_BASE,      // 3
        SPEL_ID,          // 4
        PERK_ID,          // 5
        TEAMMATE_PERK_ID, // 6
        HAIR_ID,          // 7
        EYES_ID,          // 8
        HDPT_ID,          // 9
        FACT_A,           // 10
        FACT_B,           // 11
        COMPANION_REF,    // 12
        DEAD_NPC_REF,     // 13
        ITEM_ID,          // 14
        ITEM_WORN_ID,     // 15
        WEAP_ID,          // 16
        KEY_ID,           // 17
        DOOR_REF,         // 18
        DOOR_SCRIPT_ID,   // 19
        NPC_SCRIPT_ID,    // 20
        LOCKED_CONT_REF,  // 21
        ECZN_ID,          // 22
        RADIO_REF,        // 23
    ];

    let mut h = Build { b: Vec::new() };
    h.u32(0x30).bar();
    h.b.extend([0u8; 64]);
    h.bar()
        .u32(0)
        .bar()
        .u32(0)
        .bar()
        .u32(40)
        .bar()
        .s_field("CourierV2", true)
        .s_field("Hero", true)
        .u32(18)
        .bar()
        .s_field("OwnTestRoom", true)
        .s_field("012.34.56", false);

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

    // Global table 1: type 10 (Pip-Boy radio ON tuned to RADIO_REF via observed 2nd active-station list)
    let g1 = b.len();
    let mut t10 = Build { b: Vec::new() };
    t10.u32(0)
        .bar()
        .u8(0)
        .bar()
        .s_field("", true)
        .vs(0)
        .bar()
        .vs(1)
        .bar()
        .r(23)
        .bar()
        .u32(9740)
        .bar()
        .u32(1)
        .bar()
        .u8(0x64)
        .bar()
        .u8(0x64)
        .bar()
        .u32(0x33)
        .bar()
        .u8(0)
        .bar()
        .u8(0)
        .bar();
    b.extend(10u32.to_le_bytes());
    b.extend((t10.b.len() as u32).to_le_bytes());
    b.extend(&t10.b);

    let cf = b.len();

    // 1. NPC_ 0x7 (r(3)) with multi-flag bits 1|2|4|5|11: female, SPEL_ID, SPECIAL after bits 1+4, FaceGen Look
    let mut npc = Build { b: Vec::new() };
    let mut acbs = [0u8; 24];
    acbs[0] = 1; // female
    npc.b.extend(acbs);
    npc.bar();
    npc.vs(1).bar().r(4).bar().vs(0).bar();
    npc.b.extend([6, 5, 7, 4, 8, 6, 9]);
    npc.bar();
    npc.s_field("CourierV2", true);
    npc.u8(1).bar();
    for i in 0..50 {
        npc.f32(if i == 0 { 0.3 } else { 0.0 }).bar();
    }
    for i in 0..30 {
        npc.f32(if i == 1 { -0.2 } else { 0.0 }).bar();
    }
    for i in 0..50 {
        npc.f32(if i == 2 { 0.6 } else { 0.0 }).bar();
    }
    npc.r(7).bar().r(8).bar().f32(0.5).bar().u32(0x0044_5566).bar();
    npc.vs(1).bar().r(9).bar();
    push_record(&mut b, 3, (1 << 1) | (1 << 2) | (1 << 4) | (1 << 5) | (1 << 11), TYPE_NPC, &npc.b);

    // 2. Player ACHR (r(2)): AV1 active mod (must NOT double-count), AV2 permanent mods (STR +1, CarryWeight +25, Melee +4),
    //    AV3 damage/needs (AP -15, limb 26 -30, rads +120, dehydration +200, hunger +150, sleep +100),
    //    AV[231] health damage (-40.0), scale (1.05), 0x5E factions, 0x1D teammate COMPANION_REF,
    //    inventory with 0x30 item scale & 0x17 secondary worn, bit-22 spent skills, perks, teammate perks.
    let mut p = Build { b: Vec::new() };
    p.r(1).f32(5.0).f32(6.0).f32(7.0).f32(0.0).f32(0.0).f32(0.5).bar();
    for i in 0..232 {
        let v = match i {
            38 => 2.0,         // AV1 active effect (must not be added to skill_points/av_permanent)
            82 => 1.0,         // AV2 STR +1 (77+5)
            91 => 25.0,        // AV2 CarryWeight +25 (77+14)
            100 => 300.0,      // AV2 Karma (77+23)
            101 => 9500.0,     // AV2 XP (77+24)
            115 => 4.0,        // AV2 Melee +4 (77+38)
            166 => -15.0,      // AV3 AP damage (154+12)
            180 => -30.0,      // AV3 Limb 26 damage (154+26)
            208 => 120.0,      // AV3 Rads (154+54)
            227 => 200.0,      // AV3 Dehydration (154+73)
            228 => 150.0,      // AV3 Hunger (154+74)
            229 => 100.0,      // AV3 Sleep deprivation (154+75)
            231 => -40.0,      // AV[231] Health damage
            _ => 0.0,
        };
        p.f32(v).bar();
    }
    p.u8(0).bar().f32(1.05).bar();
    p.vs(2).bar();
    p.u8(0x5E).bar().vs(2).bar().r(10).bar().u8(2).bar().r(11).bar().u8((-1i8) as u8).bar();
    p.u8(0x1D).bar().vs(1).bar().r(12).bar();
    // Inventory: 1 suit worn via 0x17 + 0x30 item scale
    p.vs(1).bar();
    p.r(15).bar().i32(1).bar().vs(1).bar().vs(2).bar().u8(0x17).bar().u8(0x30).bar().f32(1.0).bar();
    // Bit-22 spent skills
    p.vs(2).bar().u8(38).bar().f32(25.0).bar().u8(43).bar().f32(18.0).bar();
    p.b.extend_from_slice(b"\xFF\xFF\x7C\xFF\xFF\x7C\x00\x7C\x01\x01\x00\x00\x7C");
    // Perks
    p.b.extend_from_slice(b"\xAD\xDE\xAD\xDE\x7C");
    p.u8(0).bar().u8(0).bar().u8(0).bar();
    p.vs(1).bar().r(5).bar().u8(2).bar();
    for _ in 0..4 {
        p.u8(0).bar();
    }
    for _ in 0..5 {
        p.u32(0).bar();
    }
    // Teammate perks + 54B end trailer
    for _ in 0..3 {
        p.u32(0).bar();
    }
    p.vs(1).bar().r(6).bar().u8(1).bar();
    p.u8(0).bar().u8(1).bar().u8(1).bar();
    for _ in 0..8 {
        p.u32(0).bar();
    }
    for _ in 0..4 {
        p.u8(0).bar();
    }
    push_record(&mut b, 2, (1 << 1) | (1 << 4) | (1 << 5) | (1 << 22), TYPE_ACHR, &p.b);

    // 3. Companion ACHR (r(12)): scale 1.12, 0x0D script vars (bHired=1, rBoss=PLAYER_REF),
    //    0x5E faction FACT_A rank 1, bits 5+27 inventory (ITEM_ID +2, ITEM_WORN_ID +1 worn).
    let mut comp = Build { b: Vec::new() };
    comp.u8(0).bar();
    comp.f32(1.12).bar();
    comp.vs(2).bar();
    comp.u8(0x0D)
        .bar()
        .r(20)
        .bar()
        .vs(2)
        .bar()
        .u32(1)
        .bar()
        .f64(1.0)
        .bar()
        .u32(0x8000_0002)
        .bar()
        .r(2)
        .bar()
        .vs(0)
        .bar()
        .vs(0)
        .bar();
    comp.u8(0x5E).bar().vs(1).bar().r(10).bar().u8(1).bar();
    comp.vs(2).bar();
    comp.r(14).bar().i32(2).bar().vs(0).bar();
    comp.r(15).bar().i32(1).bar().vs(1).bar().vs(1).bar().u8(0x16).bar();
    push_record(&mut b, 12, (1 << 4) | (1 << 5) | (1 << 27) | (1 << 31), TYPE_ACHR, &comp.b);

    // 4. Dead NPC ACHR (r(13)): bit 0 FORM_DEAD without bit 1
    let mut dead_npc = Build { b: Vec::new() };
    dead_npc.u8(0).bar().u32(FORM_DEAD | 0x09).bar();
    push_record(&mut b, 13, 1 << 0, TYPE_ACHR, &dead_npc.b);

    // 5. Door REFR (r(18)): scale 0.9, bit 12 lock (75, locked, KEY_ID), bit 31 owner FACT_A + 0x0D script, bit 28 "Open"
    let mut door = Build { b: Vec::new() };
    door.f32(0.9).bar();
    door.vs(3).bar();
    door.u8(0x2A).bar().u8(75).bar().u8(1).bar().r(17).bar().u32(0).bar().u32(0).bar();
    door.u8(0x21).bar().r(10).bar();
    door.u8(0x0D)
        .bar()
        .r(19)
        .bar()
        .vs(2)
        .bar()
        .u32(1)
        .bar()
        .f64(1.0)
        .bar()
        .u32(0x8000_0002)
        .bar()
        .r(12)
        .bar()
        .vs(0)
        .bar()
        .vs(0)
        .bar();
    door.u8(0x28).bar().s_field("Open", true);
    push_record(&mut b, 18, (1 << 4) | (1 << 12) | (1 << 28) | (1 << 31), TYPE_REFR, &door.b);

    // 6. Locked container REFR (r(21)): bit 12 lock (50, locked) + bits 5+27 inventory (+2 WEAP_ID)
    let mut lcont = Build { b: Vec::new() };
    lcont.vs(1).bar().u8(0x2A).bar().u8(50).bar().u8(1).bar().r(0).bar().u32(0).bar().u32(0).bar();
    lcont.vs(1).bar().r(16).bar().i32(2).bar().vs(0).bar();
    push_record(&mut b, 21, (1 << 5) | (1 << 12) | (1 << 27), TYPE_REFR, &lcont.b);

    // 7. ECZN (r(22)): locked zone level 18
    let mut ez = Build { b: Vec::new() };
    ez.u32(5).u32(5).u32(5).u32(18).bar();
    push_record(&mut b, 22, 1 << 31, TYPE_ECZN, &ez.b);

    // 8. CELL ROOM_ID (r(1)): bits 3 + 30 + 31 (detach 72h = 3.0 days, fog -> visited, owner FACT_A AFTER fog)
    let mut cl = Build { b: Vec::new() };
    cl.u32(72).bar();
    cl.b.extend([0xFFu8; 4]);
    cl.r(10).bar();
    push_record(&mut b, 1, (1 << 3) | (1 << 30) | (1 << 31), TYPE_CELL, &cl.b);

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
    let table = [fid, tail, g1, cf, g2, 1, 1, 8].map(|v| v as u32);
    for (i, v) in table.iter().enumerate() {
        b[table_at + 4 * i..table_at + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }

    let fos = Fos::parse(&b).unwrap();
    let inspect = inspect_fos(&fos, false);
    assert!(inspect.parse_errors.is_empty());
    assert_eq!(inspect.decoded_counts.get("actor_changes"), Some(&2));
    assert_eq!(inspect.decoded_counts.get("ref_extras"), Some(&2));
    assert_eq!(inspect.decoded_counts.get("encounter_zones"), Some(&1));
    assert_eq!(inspect.decoded_counts.get("cells"), Some(&1));
    assert_eq!(inspect.decoded_counts.get("radio_on"), Some(&1));

    let opts = ConvertOptions {
        kind: SaveKind::Manual,
        plugins: vec![PluginEntry { name: "FalloutNV.esm".into(), size: 1234 }],
        include_thumbnail: false,
        cosave_present: false,
    };
    let converted = convert_fos(&fos, &f.data, &f.forms, &f.refs, &opts).unwrap();
    assert!(!converted.report.hypothesis_fields.is_empty());
    assert_eq!(converted.report.converted.get("player_perks"), Some(&1));
    assert_eq!(converted.report.converted.get("player_teammate_perks"), Some(&1));
    assert_eq!(converted.report.converted.get("player_skill_points"), Some(&2));
    assert_eq!(converted.report.converted.get("player_teammates"), Some(&1));
    assert_eq!(converted.report.converted.get("actor_dead"), Some(&1));
    assert_eq!(converted.report.converted.get("locks"), Some(&2));
    assert_eq!(converted.report.converted.get("doors_open_state"), Some(&1));
    assert_eq!(converted.report.converted.get("encounter_zones"), Some(&1));
    assert_eq!(converted.report.converted.get("radio_station"), Some(&1));

    let dir = std::env::temp_dir().join(format!("nvf-fos-convert-v2-{}", std::process::id()));
    let out = dir.join("v2_full.nvfsave");
    let body_bytes = serde_json::to_vec(&converted.body).unwrap();
    fnv_save::write(&out, &converted.header, &converted.shot, &body_bytes, false).unwrap();

    let (_h, _shot, body): (SaveHeader, Vec<u8>, SaveBody<GameSnapshot>) = fnv_save::read_body(&out).unwrap();
    body.game.check().unwrap();
    let restored_world = World::restore(&f.forms, &f.refs, &body.game.sim).unwrap();
    let mut restored = Game::restore(&f.data, restored_world, GameConfig::default(), &body.game).unwrap();

    // Player base: sex, spells, SPECIAL (after bits 1+4), FaceGen appearance
    assert_eq!(restored.player.female, Some(true));
    let pe = restored.extra.get(&FormId::PLAYER_REF).unwrap();
    assert!(pe.spells.contains(&FormId(SPEL_ID)));
    assert_eq!(pe.actor_values.get(&5), Some(&6.0));
    assert_eq!(pe.actor_values.get(&11), Some(&9.0));
    let look = restored.player.look.as_ref().expect("FaceGen look restored");
    assert!(look.female);
    assert_eq!(look.hair, Some(FormId(HAIR_ID)));
    assert_eq!(look.eyes, Some(FormId(EYES_ID)));
    assert_eq!(look.hair_color, Some([0x66, 0x55, 0x44]));
    assert_eq!(look.head_parts, [FormId(HDPT_ID)]);
    assert_eq!(look.fggs[0], 0.3);
    assert_eq!(look.fgga[1], -0.2);
    assert_eq!(look.fgts[2], 0.6);

    // AV2 permanent mods, AV3 damage/needs, AV[231] health damage, no double-count of AV1
    assert_eq!(restored.player.xp, 9500.0);
    assert_eq!(pe.actor_values.get(&23), Some(&300.0));
    assert_eq!(pe.av_permanent.get(&5), Some(&1.0));
    assert_eq!(pe.av_permanent.get(&14), Some(&25.0));
    assert_eq!(pe.av_permanent.get(&38), Some(&4.0));
    assert_eq!(pe.av_damage.get(&12), Some(&15.0));
    assert_eq!(pe.av_damage.get(&26), Some(&30.0));
    assert_eq!(pe.av_damage.get(&16), Some(&40.0));
    assert_eq!(pe.actor_values.get(&54), Some(&120.0));
    assert_eq!(pe.actor_values.get(&73), Some(&200.0));
    assert_eq!(pe.actor_values.get(&74), Some(&150.0));
    assert_eq!(pe.actor_values.get(&75), Some(&100.0));
    assert_eq!(restored.world.peek(FormId::PLAYER_REF).unwrap().scale, Some(1.05));
    assert_eq!(pe.factions.get(&FormId(FACT_A)), Some(&2));
    assert_eq!(pe.factions.get(&FormId(FACT_B)), Some(&-1));
    assert_eq!(restored.player.skill_points.get(&38), Some(&25.0));
    assert_eq!(restored.player.skill_points.get(&43), Some(&18.0));
    assert_eq!(restored.player.perks.get(&FormId(PERK_ID)), Some(&2));
    assert!(restored.player.teammate_perks.contains(&FormId(TEAMMATE_PERK_ID)));
    assert!(restored
        .world
        .peek(FormId::PLAYER_REF)
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .is_equipped(FormId(ITEM_WORN_ID)));

    // Companion + Dead NPC
    let ce = restored.extra.get(&FormId(COMPANION_REF)).unwrap();
    assert!(ce.teammate);
    assert_eq!(ce.factions.get(&FormId(FACT_A)), Some(&1));
    let comp_inst = restored.instances.get(&InstKey::Ref { reference: FormId(COMPANION_REF) }).unwrap();
    assert_eq!(comp_inst.vars.get(&1), Some(&Value::Num(1.0)));
    assert_eq!(comp_inst.vars.get(&2), Some(&Value::Ref(PLAYER_REF)));
    let comp_st = restored.world.peek(FormId(COMPANION_REF)).unwrap();
    assert_eq!(comp_st.scale, Some(1.12));
    assert!(comp_st.outfitted);
    assert!(comp_st.inventory.as_ref().unwrap().is_equipped(FormId(ITEM_WORN_ID)));
    assert_eq!(restored.world.item_count(FormId(COMPANION_REF), FormId(ITEM_ID)).unwrap(), 2);
    assert!(restored.extra.get(&FormId(DEAD_NPC_REF)).unwrap().dead);

    // Door, locked container, encounter zone, cell, radio
    let door_st = restored.world.peek(FormId(DOOR_REF)).unwrap();
    assert_eq!(door_st.scale, Some(0.9));
    assert!(door_st.open);
    assert_eq!(door_st.extra.owner, Some(FormId(FACT_A)));
    let lk = door_st.lock.expect("door lock restored");
    assert_eq!((lk.level, lk.key, lk.locked), (75, Some(FormId(KEY_ID)), true));
    let door_inst = restored.instances.get(&InstKey::Ref { reference: FormId(DOOR_REF) }).unwrap();
    assert_eq!(door_inst.vars.get(&1), Some(&Value::Num(1.0)));
    assert_eq!(door_inst.vars.get(&2), Some(&Value::Ref(COMPANION_REF)));
    let lcont_st = restored.world.peek(FormId(LOCKED_CONT_REF)).unwrap();
    assert_eq!(lcont_st.lock.map(|l| (l.level, l.locked)), Some((50, true)));
    assert_eq!(restored.world.item_count(FormId(LOCKED_CONT_REF), FormId(WEAP_ID)).unwrap(), 2);
    assert_eq!(restored.world.zone_level(FormId(ECZN_ID)), Some(18));
    assert!(restored.forms.visited_cells.contains(&FormId(ROOM_ID)));
    assert_eq!(restored.forms.cell_owners.get(&FormId(ROOM_ID)), Some(&FormId(FACT_A)));
    assert_eq!(restored.world.detached_since(FormId(ROOM_ID)), Some(3.0));
    assert!(restored.radio.pipboy.on);
    assert_eq!(restored.radio.pipboy.station, Some(FormId(RADIO_REF)));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn v2_non_finite_floats_in_new_fields_are_sanitized_cleanly() {
    let f = fixture();
    let fids = [
        ROOM_ID,        // 1
        PLAYER_REF,     // 2
        PLAYER_BASE,    // 3
        COMPANION_REF,  // 4
        NPC_SCRIPT_ID,  // 5
        DOOR_REF,       // 6
        DOOR_SCRIPT_ID, // 7
    ];

    let mut h = Build { b: Vec::new() };
    h.u32(0x30).bar();
    h.b.extend([0u8; 64]);
    h.bar()
        .u32(0)
        .bar()
        .u32(0)
        .bar()
        .u32(41)
        .bar()
        .s_field("CleanV2", true)
        .s_field("Drifter", true)
        .u32(5)
        .bar()
        .s_field("OwnTestRoom", true)
        .s_field("001.00.00", false);

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

    // 1. NPC_ 0x7 with NaN FaceGen morph
    let mut npc = Build { b: Vec::new() };
    npc.u8(1).bar();
    for i in 0..50 {
        npc.f32(if i == 0 { f32::NAN } else { 0.0 }).bar();
    }
    for _ in 0..30 {
        npc.f32(0.0).bar();
    }
    for _ in 0..50 {
        npc.f32(0.0).bar();
    }
    npc.r(0).bar().r(0).bar().f32(0.5).bar().u32(0).bar().vs(0).bar();
    push_record(&mut b, 3, 1 << 11, TYPE_NPC, &npc.b);

    // 2. Player ACHR with NaN AV2 permanent mod, Inf AV3 rads, NaN AV[231] health damage
    let mut p = Build { b: Vec::new() };
    p.r(1).f32(0.0).f32(0.0).f32(0.0).f32(0.0).f32(0.0).f32(0.0).bar();
    for i in 0..232 {
        let v = match i {
            115 => f32::NAN,         // AV2 Melee
            208 => f32::INFINITY,    // AV3 Rads
            231 => f32::NEG_INFINITY, // AV[231] Health damage
            _ => 0.0,
        };
        p.f32(v).bar();
    }
    p.u8(0).bar().f32(1.0).bar().vs(0).bar().vs(0).bar();
    push_record(&mut b, 2, (1 << 1) | (1 << 4) | (1 << 5), TYPE_ACHR, &p.b);

    // 3. Companion ACHR with Inf scale and NaN script var 3
    let mut comp = Build { b: Vec::new() };
    comp.u8(0).bar().f32(f32::INFINITY).bar();
    comp.vs(1).bar();
    comp.u8(0x0D)
        .bar()
        .r(5)
        .bar()
        .vs(1)
        .bar()
        .u32(3)
        .bar()
        .f64(f64::NAN)
        .bar()
        .vs(0)
        .bar()
        .vs(0)
        .bar();
    push_record(&mut b, 4, (1 << 4) | (1 << 31), TYPE_ACHR, &comp.b);

    // 4. Door REFR with NaN scale and -Inf script var 3
    let mut door = Build { b: Vec::new() };
    door.f32(f32::NAN).bar();
    door.vs(1).bar();
    door.u8(0x0D)
        .bar()
        .r(7)
        .bar()
        .vs(1)
        .bar()
        .u32(3)
        .bar()
        .f64(f64::NEG_INFINITY)
        .bar()
        .vs(0)
        .bar()
        .vs(0)
        .bar();
    push_record(&mut b, 6, (1 << 4) | (1 << 31), TYPE_REFR, &door.b);

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
    let table = [fid, tail, g1, cf, g2, 0, 1, 4].map(|v| v as u32);
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
    assert_eq!(converted.report.skipped.get("player_appearance: non-finite FaceGen morph"), Some(&1));
    assert_eq!(converted.report.skipped.get("player_av: non-finite value"), Some(&1));
    assert_eq!(converted.report.skipped.get("player_av_damage: non-finite value"), Some(&1));
    assert_eq!(converted.report.skipped.get("player_health_damage: non-finite value"), Some(&1));
    assert_eq!(converted.report.skipped.get("actor_scale: non-finite value"), Some(&1));
    assert_eq!(converted.report.skipped.get("actor_script_var: non-finite value"), Some(&1));
    assert_eq!(converted.report.skipped.get("ref_scale: non-finite value"), Some(&1));
    assert_eq!(converted.report.skipped.get("ref_script_var: non-finite value"), Some(&1));

    let dir = std::env::temp_dir().join(format!("nvf-fos-convert-v2-nan-{}", std::process::id()));
    let out = dir.join("v2_sanitized.nvfsave");
    let body_bytes = serde_json::to_vec(&converted.body).unwrap();
    fnv_save::write(&out, &converted.header, &converted.shot, &body_bytes, false).unwrap();

    let (_h, _shot, body): (SaveHeader, Vec<u8>, SaveBody<GameSnapshot>) = fnv_save::read_body(&out).unwrap();
    body.game.check().unwrap();
    let restored_world = World::restore(&f.forms, &f.refs, &body.game.sim).unwrap();
    let restored = Game::restore(&f.data, restored_world, GameConfig::default(), &body.game).unwrap();
    assert!(restored.player.look.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn v2_observed_decoder_fixes_round_trip_cleanly() {
    let f = fixture();
    let fids = [
        ROOM_ID,       // 1
        PLAYER_REF,    // 2
        PLAYER_BASE,   // 3
        ITEM_ID,       // 4
        FACT_A,        // 5
        PERK_ID,       // 6
        CONT_REF,      // 7
        RADIO_REF,     // 8
        NOTE_ID,       // 9
        WEAP_ID,       // 10
        HAIR_ID,       // 11
        EYES_ID,       // 12
        HDPT_ID,       // 13
    ];

    let mut h = Build { b: Vec::new() };
    h.u32(0x30).bar();
    h.b.extend([0u8; 64]);
    h.bar()
        .u32(0)
        .bar()
        .u32(0)
        .bar()
        .u32(42)
        .bar()
        .s_field("ObservedV2", true)
        .s_field("Drifter", true)
        .u32(5)
        .bar()
        .s_field("OwnTestRoom", true)
        .s_field("001.00.00", false);

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

    // BUG #3: Global[10] vanilla 1.4.0.525 layout (header 0, active station in 2nd vsval list)
    let g1 = b.len();
    let mut t10 = Build { b: Vec::new() };
    t10.u32(0)
        .bar()
        .u8(0)
        .bar()
        .s_field("", true)
        .vs(0)
        .bar()
        .vs(1)
        .bar()
        .r(8)
        .bar()
        .u32(29077)
        .bar()
        .u32(1)
        .bar()
        .u8(0x64)
        .bar()
        .u8(0x64)
        .bar()
        .u32(0x33)
        .bar()
        .u8(0)
        .bar()
        .u8(0)
        .bar();
    b.extend(10u32.to_le_bytes());
    b.extend((t10.b.len() as u32).to_le_bytes());
    b.extend(&t10.b);

    let cf = b.len();

    // BUG #8 + Round 3 bit-11 appearance: NPC_ 0x7 with bit 11 FaceGen + bit 24 SexChange (`01 7c`)
    let mut npc7 = Build { b: Vec::new() };
    npc7.u8(1).bar();
    for i in 0..50 {
        npc7.f32(if i == 0 { 0.25 } else { 0.0 }).bar();
    }
    for _ in 0..30 {
        npc7.f32(0.0).bar();
    }
    for _ in 0..50 {
        npc7.f32(0.0).bar();
    }
    npc7.r(11).bar().r(12).bar().f32(0.04).bar().u32(0x002E_4654).bar();
    npc7.vs(1).bar().r(13).bar();
    npc7.u8(0x01).bar(); // bit 24 SexChange
    push_record(&mut b, 3, (1 << 11) | (1 << 24), TYPE_NPC, &npc7.b);

    // BUG #1 + #2 + #7 + #12 + #13: Player ACHR WITHOUT bit 4 (no scale float!), AV2 rads (54) &
    // hardcore needs (73..=75), inventory item with 0x16 + 0x3E (no-unequip) & NO 0x6E, High-Process
    // tok[70]=9 & tok[76]=11 active magazine rounds, bit 22 with non-skill SetAV Luck=9 (11) +
    // skill Melee=20 (38), tail notes, and perks.
    let mut p = Build { b: Vec::new() };
    p.r(1).f32(1.0).f32(2.0).f32(3.0).f32(0.0).f32(0.0).f32(0.0).bar();
    for i in 0..232 {
        let v = match i {
            131 => 125.0, // AV2 RadiationRads (77 + 54)
            150 => 150.0, // AV2 Dehydration (77 + 73)
            151 => 175.0, // AV2 Hunger (77 + 74)
            152 => 200.0, // AV2 SleepDeprivation (77 + 75)
            _ => 0.0,
        };
        p.f32(v).bar();
    }
    p.u8(0).bar(); // NO scale float because bit 4 is 0!
    p.vs(1).bar().u8(0x5E).bar().vs(1).bar().r(5).bar().u8(2).bar();
    p.vs(2).bar();
    p.r(4).bar().i32(4).bar().vs(0).bar();
    // Equipped WEAP_ID with 0x25 + 0x16 + 0x3E (no-unequip, BUG #12) and NO 0x6E
    p.r(10).bar().i32(1).bar().vs(1).bar().vs(3).bar();
    p.u8(0x25).bar().f32(150.0).bar().u8(0x16).bar().u8(0x3E).bar();
    // High-Process block tok[0..=76] with tok[70] = 9 (equipped) and tok[76] = 11 (active rounds, BUG #13)
    const PROC_WIDTHS: [usize; 77] = [
        1, 1, 1, 1, 1, 1, 1, 1, 4, 4, 1, 1, 3, 3, 4, 4, 4, 3, 1, 4, 3, 4, 4, 4, 1, 2, 4, 4, 3, 3,
        3, 3, 3, 1, 4, 1, 1, 1, 4, 4, 4, 1, 12, 4, 1, 1, 1, 2, 12, 1, 1, 1, 1, 4, 1, 4, 4, 1, 1, 1,
        2, 4, 1, 4, 4, 1, 1, 4, 4, 4, 4, 1, 3, 3, 3, 3, 4,
    ];
    for (idx, &w) in PROC_WIDTHS.iter().enumerate() {
        if idx == 70 {
            p.u32(9).bar();
        } else if idx == 76 {
            p.u32(11).bar();
        } else {
            p.b.extend(std::iter::repeat_n(0u8, w));
            p.bar();
        }
    }
    p.r(0).bar().r(0).bar().r(0).bar();
    p.vs(2).bar().u8(11).bar().f32(9.0).bar().u8(38).bar().f32(20.0).bar();
    p.b.extend_from_slice(b"\xFF\xFF\x7C\xFF\xFF\x7C\x00\x7C\x01\x01\x00\x00\x7C");
    p.b.extend_from_slice(b"\xAD\xDE\xAD\xDE\x7C");
    p.u8(0).bar();
    for _ in 0..4 {
        p.u32(0).bar();
    }
    p.u8(0).bar().u32(200).bar();
    for _ in 0..11 {
        p.r(0).bar();
    }
    // BUG #9: player tail notes list with NOTE_ID (0 TYPE_NOTE changed forms!)
    p.vs(0).bar();
    p.vs(1).bar().r(9).bar();
    p.vs(0).bar();
    // Known actors: 1 entry
    p.vs(1).bar().r(8).bar().u8(0).bar().u8(0).bar();
    // Perks: 1 entry (PERK_ID, rank 1)
    p.vs(1).bar().r(6).bar().u8(1).bar();
    for _ in 0..4 {
        p.u8(0).bar();
    }
    for _ in 0..5 {
        p.u32(0).bar();
    }
    for _ in 0..3 {
        p.u32(0).bar();
    }
    p.u8(0).bar().u8(1).bar().u8(1).bar();
    for _ in 0..8 {
        p.u32(0).bar();
    }
    for _ in 0..4 {
        p.u8(0).bar();
    }
    push_record(
        &mut b,
        2,
        (1 << 1) | (1 << 5) | (1 << 22) | (1 << 28) | (1 << 31),
        TYPE_ACHR,
        &p.b,
    );

    // BUG #4: Container REFR with bit 6 (`0x40`, SetOwnership 0x21) + bit 4 scale + bits 27+28
    let mut cont = Build { b: Vec::new() };
    cont.f32(1.5).bar();
    cont.vs(1).bar().u8(0x21).bar().r(5).bar();
    cont.vs(1).bar().r(4).bar().i32(2).bar().vs(0).bar();
    cont.u8(0x2C).bar().s_field("Close", true).u8(0).bar();
    push_record(
        &mut b,
        7,
        (1 << 4) | (1 << 6) | (1 << 27) | (1 << 28),
        TYPE_REFR,
        &cont.b,
    );

    // BUG #5: CELL ROOM_ID with flags 0x8000000a (bits 1+3+31): 20 | + 76B fog + trailing owner FACT_A |
    let mut cl = Build { b: Vec::new() };
    cl.u8(0x20).bar();
    cl.b.extend([0x08, BAR, 0x00, BAR, 0xFF, BAR]);
    cl.b.extend([0x00u8; 70]);
    cl.r(5).bar();
    assert_eq!(cl.b.len(), 82);
    push_record(&mut b, 1, (1 << 1) | (1 << 3) | (1 << 31), TYPE_CELL, &cl.b);

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
    let table = [fid, tail, g1, cf, g2, 1, 1, 4].map(|v| v as u32);
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
    // Every field in this save is [observed], so hypothesis_fields and unmapped skips are empty!
    assert!(
        converted.report.hypothesis_fields.is_empty(),
        "unexpected hypothesis_fields: {:?}",
        converted.report.hypothesis_fields
    );
    assert!(
        converted.report.skipped.is_empty(),
        "unexpected skipped: {:?}",
        converted.report.skipped
    );

    let dir = std::env::temp_dir().join(format!("nvf-fos-convert-v2-obs-{}", std::process::id()));
    let out = dir.join("v2_observed_fixes.nvfsave");
    let body_bytes = serde_json::to_vec(&converted.body).unwrap();
    fnv_save::write(&out, &converted.header, &converted.shot, &body_bytes, false).unwrap();

    let (_h, _shot, body): (SaveHeader, Vec<u8>, SaveBody<GameSnapshot>) = fnv_save::read_body(&out).unwrap();
    body.game.check().unwrap();
    let restored_world = World::restore(&f.forms, &f.refs, &body.game.sim).unwrap();
    let mut restored = Game::restore(&f.data, restored_world, GameConfig::default(), &body.game).unwrap();

    // BUG #8: SexChange bit 24
    assert_eq!(restored.player.female, Some(true));
    // BUG #1: bit-4-absent player kept inventory, factions, perks
    assert_eq!(restored.world.peek(FormId::PLAYER_REF).unwrap().scale, None);
    assert_eq!(restored.world.item_count(FormId::PLAYER_REF, FormId(ITEM_ID)).unwrap(), 4);
    let pe = restored.extra.get(&FormId::PLAYER_REF).unwrap();
    assert_eq!(pe.factions.get(&FormId(FACT_A)), Some(&2));
    assert_eq!(restored.player.perks.get(&FormId(PERK_ID)), Some(&1));
    // BUG #2: bit 22 non-skill Luck=9 AND skill Melee=20
    assert_eq!(pe.actor_values.get(&11), Some(&9.0));
    assert_eq!(restored.player.skill_points.get(&38), Some(&20.0));
    // BUG #7: AV2 rads & hardcore needs in actor_values (NOT av_permanent)
    assert_eq!(pe.actor_values.get(&54), Some(&125.0));
    assert_eq!(pe.actor_values.get(&73), Some(&150.0));
    assert_eq!(pe.actor_values.get(&74), Some(&175.0));
    assert_eq!(pe.actor_values.get(&75), Some(&200.0));
    assert!(!pe.av_permanent.contains_key(&54));
    assert!(!pe.av_permanent.contains_key(&73));
    // BUG #3: Global[10] active Pip-Boy radio
    assert!(restored.radio.pipboy.on);
    assert_eq!(restored.radio.pipboy.station, Some(FormId(RADIO_REF)));
    // BUG #4: bit-6 owned container (owner + scale + inventory)
    let cst = restored.world.peek(FormId(CONT_REF)).unwrap();
    assert_eq!(cst.scale, Some(1.5));
    assert_eq!(cst.extra.owner, Some(FormId(FACT_A)));
    assert_eq!(restored.world.item_count(FormId(CONT_REF), FormId(ITEM_ID)).unwrap(), 5);
    // BUG #5: CELL bit 3 owner AFTER bit-31 fog
    assert!(restored.forms.visited_cells.contains(&FormId(ROOM_ID)));
    assert_eq!(restored.forms.cell_owners.get(&FormId(ROOM_ID)), Some(&FormId(FACT_A)));
    // BUG #9: Player tail notes list decoded with zero TYPE_NOTE records (and zero hypothesis_fields!)
    assert!(restored.player.notes.contains(&FormId(NOTE_ID)));
    // Round 3: NPC_ bit 11 appearance, BUG #12 0x3E no-unequip, and BUG #13 tok[76] active magazine (all [observed]!)
    let look = restored.player.look.as_ref().expect("FaceGen look restored with zero hypothesis_fields");
    assert_eq!(look.hair, Some(FormId(HAIR_ID)));
    assert_eq!(look.eyes, Some(FormId(EYES_ID)));
    assert_eq!(look.hair_color, Some([0x54, 0x46, 0x2E]));
    assert_eq!(look.head_parts, [FormId(HDPT_ID)]);
    assert_eq!(body.engine.magazine, Some((WEAP_ID, 11)));
    assert_eq!(body.engine.magazine_ammo, None);

    let _ = std::fs::remove_dir_all(&dir);
}
