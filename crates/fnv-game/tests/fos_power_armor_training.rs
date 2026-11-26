//! Regression tests for restoring Power Armor Training (`SetPCCanUsePowerArmor`) and worn
//! Power Armor items during `.fos` -> `.nvfsave` conversion.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::fos_convert::{convert_fos, inspect_fos, ConvertOptions};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_save::fos::{Fos, PLAYER_BASE, PLAYER_REF, TYPE_ACHR};
use fnv_save::{PluginEntry, SaveKind};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, World};

const BAR: u8 = 0x7C;
const GLOB_HOUR: u32 = 0x0000_0038;
const GLOB_DAYS: u32 = 0x0000_0039;
const ROOM_ID: u32 = 0x0000_0830;
const RACE_ID: u32 = 0x0000_0019;
const PA_BODY_ID: u32 = 0x0000_0821;
const PA_HELM_ID: u32 = 0x0000_0822;
const NORMAL_ARMOR_ID: u32 = 0x0000_0823;
const PAT_PERK_ID: u32 = 0x0005_8FDF;

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
    ]
    .concat();
    let armo = |id: u32, edid: &str, slots: u32, general_flags: u8| {
        record(
            b"ARMO",
            0,
            id,
            &[
                sub(b"EDID", z(edid)),
                sub(b"BMDT", [slots.to_le_bytes().as_slice(), &[general_flags, 0, 0, 0]].concat()),
                sub(
                    b"DATA",
                    [50i32.to_le_bytes().as_slice(), &100i32.to_le_bytes(), &10.0f32.to_le_bytes()]
                        .concat(),
                ),
            ],
        )
    };
    let armors = [
        // Upper body (0x04) + BMDT general_flags bit 5 (0x20 = Power Armor)
        armo(PA_BODY_ID, "OwnPowerArmorBody", 0x04, 0x20),
        // Head (0x01) + BMDT general_flags bit 5 (0x20 = Power Armor)
        armo(PA_HELM_ID, "OwnPowerArmorHelm", 0x01, 0x20),
        // Eyeglasses (0x40), non-Power-Armor control
        armo(NORMAL_ARMOR_ID, "OwnGlasses", 0x40, 0x00),
    ]
    .concat();
    let perk = record(b"PERK", 0, PAT_PERK_ID, &[sub(b"EDID", z("PowerArmorTraining"))]);
    let race = record(b"RACE", 0, RACE_ID, &[sub(b"EDID", z("OwnRace"))]);
    let npc_player = record(
        b"NPC_",
        0,
        PLAYER_BASE,
        &[sub(b"EDID", z("Player")), sub(b"RNAM", RACE_ID.to_le_bytes().to_vec())],
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
    let children = group(ROOM_ID.to_le_bytes(), 6, &group(ROOM_ID.to_le_bytes(), 9, &player_ref));
    let bytes = [
        tes4(),
        top(b"GLOB", &globals),
        top(b"ARMO", &armors),
        top(b"PERK", &perk),
        top(b"RACE", &race),
        top(b"NPC_", &npc_player),
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
    fn r(&mut self, idx_1based: u32) -> &mut Self {
        self.b.extend(&idx_1based.to_be_bytes()[1..]);
        self
    }
    fn vs(&mut self, n: u32) -> &mut Self {
        assert!(n < 64);
        self.u8((n << 2) as u8)
    }
    fn s_field(&mut self, v: &str, trailing_bar: bool) -> &mut Self {
        self.u16(v.len() as u16).bar();
        self.b.extend(v.as_bytes());
        if trailing_bar && !v.is_empty() {
            self.bar();
        }
        self
    }
}

fn build_fos_with_pa_flag(flag_byte: Option<u8>, trailing_list_len: u32) -> Fos {
    let fids = [
        ROOM_ID,         // 1
        PA_BODY_ID,      // 2
        PA_HELM_ID,      // 3
        NORMAL_ARMOR_ID, // 4
        PAT_PERK_ID,     // 5
        PLAYER_REF,      // 6
    ];

    let mut pl = Build { b: Vec::new() };
    // Place + 232 actor values
    pl.r(1).f32(10.0).f32(20.0).f32(0.0).f32(0.0).f32(0.0).f32(0.0).bar();
    for _ in 0..232 {
        pl.f32(0.0).bar();
    }
    pl.u8(0).bar(); // process_level (bit 4 absent => no scale)
    pl.vs(0).bar(); // 0 actor extras
    // Inventory: 3 worn items (PA body, PA helmet, normal glasses)
    pl.vs(3).bar();
    for fid_idx in [2u32, 3, 4] {
        pl.r(fid_idx).bar().i32(1).bar();
        pl.vs(1).bar(); // 1 entry list
        pl.vs(1).bar(); // 1 extra: 0x16 worn
        pl.u8(0x16).bar();
    }
    // Tail: DEAD_MARK + optional 17-bar prefix with flag_byte + 11-slot ref3| + trailing list + perk
    pl.b.extend_from_slice(b"\xAD\xDE\xAD\xDE\x7C");
    if let Some(flag) = flag_byte {
        pl.u8(0).bar();
        pl.f32(-1.0).bar();
        pl.u8(0).bar().u8(0).bar().u8(0).bar().u8(0).bar().u8(flag).bar().u8(0).bar();
        for _ in 0..7 {
            pl.u32(0).bar();
        }
        pl.u8(0).bar().u32(200).bar();
        for _ in 0..11 {
            pl.r(0).bar();
        }
        pl.vs(trailing_list_len).bar();
        for _ in 0..trailing_list_len {
            pl.r(1).bar();
        }
    }
    // Player perks list (PAT_PERK_ID rank 1) + 4*(u8|) + 5*(u32|)
    pl.u8(0).bar().u8(0).bar();
    pl.vs(1).bar().r(5).bar().u8(1).bar();
    for _ in 0..4 {
        pl.u8(0).bar();
    }
    for _ in 0..5 {
        pl.u32(0).bar();
    }
    // 54-byte end trailer
    for _ in 0..3 {
        pl.u32(0).bar();
    }
    pl.u8(0).bar().u8(1).bar().u8(1).bar();
    for _ in 0..8 {
        pl.u32(0).bar();
    }
    for _ in 0..4 {
        pl.u8(0).bar();
    }

    let mut changed = Vec::new();
    changed.extend(&6u32.to_be_bytes()[1..]); // PLAYER_REF
    changed.extend(((1u32 << 1) | (1 << 5) | (1 << 28) | (1 << 31)).to_le_bytes());
    changed.push(TYPE_ACHR | 0x40);
    changed.push(27);
    changed.extend((pl.b.len() as u16).to_le_bytes());
    changed.extend(&pl.b);

    let mut h = Build { b: Vec::new() };
    h.u32(0x30).bar();
    h.b.extend([0u8; 64]);
    h.bar()
        .u32(1)
        .bar()
        .u32(1)
        .bar()
        .u32(1)
        .bar()
        .s_field("Courier", true)
        .s_field("Neutral", true)
        .u32(20)
        .bar()
        .s_field("OwnTestRoom", true)
        .s_field("001.00.00", false);

    let mut b = b"FO3SAVEGAME".to_vec();
    b.extend((h.b.len() as u32).to_le_bytes());
    b.extend(&h.b);
    b.extend([0u8, 0, 0]); // 1x1 RGB screenshot

    let mut plugins_buf = Build { b: Vec::new() };
    plugins_buf.u8(1).bar().s_field("FalloutNV.esm", true);
    b.push(27);
    b.extend((plugins_buf.b.len() as u32).to_le_bytes());
    b.extend(&plugins_buf.b);

    let tbl_pos = b.len();
    b.extend([0u8; 32 + 78]);

    let g1_off = b.len() as u32;
    let cf_off = b.len() as u32;
    b.extend(&changed);
    let g2_off = b.len() as u32;
    let fid_off = b.len() as u32;
    b.extend((fids.len() as u32).to_le_bytes());
    for fid in fids {
        b.extend(fid.to_le_bytes());
    }
    b.extend(0u32.to_le_bytes()); // worldspaces
    let tail_off = b.len() as u32;
    b.extend([0u8; 9]);

    let table = [fid_off, tail_off, g1_off, cf_off, g2_off, 0, 0, 1];
    for (i, v) in table.iter().enumerate() {
        b[tbl_pos + i * 4..tbl_pos + (i + 1) * 4].copy_from_slice(&v.to_le_bytes());
    }
    Fos::parse(&b).expect("synthetic fos parses")
}

fn opts() -> ConvertOptions {
    ConvertOptions {
        kind: SaveKind::Manual,
        plugins: vec![PluginEntry { name: "FalloutNV.esm".into(), size: 100 }],
        include_thumbnail: false,
        cosave_present: false,
    }
}

#[test]
fn fos_convert_restores_power_armor_training_and_equips_worn_power_armor() {
    let f = fixture();
    let fos = build_fos_with_pa_flag(Some(1), 4);

    let inspect = inspect_fos(&fos, false);
    assert_eq!(inspect.decoded_counts.get("player_power_armor_training"), Some(&1));

    let converted =
        convert_fos(&fos, &f.data, &f.forms, &f.refs, &opts()).expect("convert succeeds");
    assert_eq!(converted.report.converted.get("player_power_armor_training"), Some(&1));
    assert_eq!(converted.report.converted.get("player_equipped"), Some(&3));
    assert!(
        converted
            .report
            .hypothesis_fields
            .contains_key("player_power_armor_training (ACHR 0x14 tail flag) [hypothesis]")
    );
    assert!(
        !converted.report.skipped.keys().any(|k| k.starts_with("player_equipped")),
        "worn Power Armor must not be skipped: {:?}",
        converted.report.skipped
    );
    assert!(converted.body.game.player.power_armor);
    assert!(converted.body.game.sim.power_armor_training);

    // Restore into a live Game, tick once, verify training persists and armor can re-equip.
    let world = World::restore(&f.forms, &f.refs, &converted.body.game.sim).expect("world restore");
    let mut g = Game::restore(&f.data, world, GameConfig::default(), &converted.body.game)
        .expect("game restore");
    assert!(g.player.power_armor);
    assert!(g.world.power_armor_training());
    let inv = g.world.peek(FormId::PLAYER_REF).unwrap().inventory.as_ref().unwrap();
    assert!(inv.is_equipped(FormId(PA_BODY_ID)));
    assert!(inv.is_equipped(FormId(PA_HELM_ID)));
    assert!(inv.is_equipped(FormId(NORMAL_ARMOR_ID)));

    g.tick();
    assert!(g.player.power_armor);
    assert!(g.world.power_armor_training());
    g.world.unequip(FormId::PLAYER_REF, FormId(PA_BODY_ID)).expect("unequip PA body");
    g.world.equip(FormId::PLAYER_REF, FormId(PA_BODY_ID)).expect("re-equip PA body");
}

#[test]
fn fos_convert_refuses_worn_power_armor_when_flag_is_zero_or_corrupt() {
    let f = fixture();

    // Explicit flag = 0 (untrained): refuses Power Armor, equips non-Power-Armor control.
    let fos_zero = build_fos_with_pa_flag(Some(0), 0);
    let inspect_zero = inspect_fos(&fos_zero, false);
    assert_eq!(inspect_zero.decoded_counts.get("player_power_armor_training"), Some(&0));

    let zero =
        convert_fos(&fos_zero, &f.data, &f.forms, &f.refs, &opts()).expect("convert succeeds");
    assert!(!zero.body.game.player.power_armor);
    assert!(!zero.body.game.sim.power_armor_training);
    assert!(!zero.report.converted.contains_key("player_power_armor_training"));
    assert!(
        !zero
            .report
            .hypothesis_fields
            .contains_key("player_power_armor_training (ACHR 0x14 tail flag) [hypothesis]")
    );
    assert_eq!(zero.report.converted.get("player_equipped"), Some(&1));

    let world = World::restore(&f.forms, &f.refs, &zero.body.game.sim).expect("world restore");
    let inv = world.peek(FormId::PLAYER_REF).unwrap().inventory.as_ref().unwrap();
    assert!(!inv.is_equipped(FormId(PA_BODY_ID)));
    assert!(!inv.is_equipped(FormId(PA_HELM_ID)));
    assert!(inv.is_equipped(FormId(NORMAL_ARMOR_ID)));

    // Non-boolean / corrupt flag byte (2) fails closed as untrained (`None`).
    let fos_bad = build_fos_with_pa_flag(Some(2), 0);
    let inspect_bad = inspect_fos(&fos_bad, false);
    assert!(!inspect_bad.decoded_counts.contains_key("player_power_armor_training"));
    let bad =
        convert_fos(&fos_bad, &f.data, &f.forms, &f.refs, &opts()).expect("convert succeeds");
    assert!(!bad.body.game.player.power_armor);
    assert!(!bad.body.game.sim.power_armor_training);
    assert!(!bad.report.converted.contains_key("player_power_armor_training"));
}
