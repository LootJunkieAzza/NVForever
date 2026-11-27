//! Clean-room offline `.nvse` co-save inspection and `.fos` + `.nvse` -> `.nvfsave`
//! conversion integration tests (`docs/spec-notes/nvse-cosaves.md`, #4747).

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::fos_convert::{
    convert_fos, convert_fos_with_cosave, inspect_fos_with_cosave, ConvertOptions,
};
use fnv_game::game::{Game, GameConfig, GameSnapshot, InstKey};
use fnv_game::nvse_jip10::ADDED_VAR_BASE;
use fnv_game::GameData;
use fnv_save::fos::{Fos, PLAYER_BASE, PLAYER_REF, TYPE_QUST, TYPE_REFR};
use fnv_save::nvse::{
    NvseCosave, PLUGIN_JIP, PLUGIN_JOHNNYGUITAR, PLUGIN_NVSE, PLUGIN_SHOWOFF,
};
use fnv_save::{PluginEntry, SaveBody, SaveKind};
use fnv_script::vm::{ArrayKind, Value, XKey, XVal};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, World};

const BAR: u8 = 0x7C;
const ROOM_ID: u32 = 0x0000_0830;
const QUEST_ID: u32 = 0x0000_0810;
const SCRIPT_ID: u32 = 0x0000_0811;
const ITEM_BASE: u32 = 0x0000_0820;
const PROP_REF: u32 = 0x0000_0831;
const MODA_ITEM_LOCAL: u32 = 0x0000_0901;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
    plugin_entries: Vec<PluginEntry>,
}

fn build_fixture() -> Fixture {
    let mut scpt_subs = vec![sub(b"EDID", z("CosaveTestScript"))];
    scpt_subs.extend(unit(
        1,
        &script(&[(0, None, Vec::new())]),
        &[(1, "fBaseVar", 0)],
        &[],
    ));
    let scpt = record(b"SCPT", 0, SCRIPT_ID, &scpt_subs);
    let quest = record(
        b"QUST",
        0,
        QUEST_ID,
        &[
            sub(b"EDID", z("CosaveTestQuest")),
            sub(b"SCRI", SCRIPT_ID.to_le_bytes().to_vec()),
        ],
    );
    let item = record(
        b"MISC",
        0,
        ITEM_BASE,
        &[
            sub(b"EDID", z("CosavePen")),
            sub(b"DATA", [5i32.to_le_bytes(), 1.0f32.to_le_bytes()].concat()),
        ],
    );
    let npc_player = record(b"NPC_", 0, PLAYER_BASE, &[sub(b"EDID", z("Player"))]);
    let room = record(
        b"CELL",
        0,
        ROOM_ID,
        &[sub(b"EDID", z("CosaveRoom")), sub(b"DATA", vec![1])],
    );
    let prop_ref = record(
        b"REFR",
        0,
        PROP_REF,
        &[
            sub(b"NAME", ITEM_BASE.to_le_bytes().to_vec()),
            sub(b"DATA", [0.0f32; 6].iter().flat_map(|f| f.to_le_bytes()).collect()),
        ],
    );
    let player_ref = record(
        b"ACHR",
        0,
        PLAYER_REF,
        &[
            sub(b"NAME", PLAYER_BASE.to_le_bytes().to_vec()),
            sub(b"DATA", [0.0f32; 6].iter().flat_map(|f| f.to_le_bytes()).collect()),
        ],
    );
    let children = group(
        ROOM_ID.to_le_bytes(),
        6,
        &group(ROOM_ID.to_le_bytes(), 9, &[prop_ref, player_ref].concat()),
    );
    let base_bytes = [
        tes4(),
        top(b"SCPT", &scpt),
        top(b"QUST", &quest),
        top(b"MISC", &item),
        top(b"NPC_", &npc_player),
        top(
            b"CELL",
            &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[room, children].concat())),
        ),
    ]
    .concat();

    // Extra.esp sits at index 1 in the target load order; ModA.esp shifts from index 1 -> index 2.
    let extra_bytes = tes4();
    let moda_item = record(
        b"MISC",
        0,
        MODA_ITEM_LOCAL,
        &[
            sub(b"EDID", z("ModAItem")),
            sub(b"DATA", [10i32.to_le_bytes(), 0.5f32.to_le_bytes()].concat()),
        ],
    );
    let moda_bytes = [tes4(), top(b"MISC", &moda_item)].concat();

    let plugins = vec![
        Plugin::parse(&base_bytes, "FalloutNV.esm".into()).unwrap(),
        Plugin::parse(&extra_bytes, "Extra.esp".into()).unwrap(),
        Plugin::parse(&moda_bytes, "ModA.esp".into()).unwrap(),
    ];
    let plugin_entries = vec![
        PluginEntry { name: "FalloutNV.esm".into(), size: base_bytes.len() as u64 },
        PluginEntry { name: "Extra.esp".into(), size: extra_bytes.len() as u64 },
        PluginEntry { name: "ModA.esp".into(), size: moda_bytes.len() as u64 },
    ];
    let order = LoadOrder::new(&plugins).unwrap();
    let schema =
        Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/script/functions.json");
    let catalogue = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let table = FunctionTable::load(&path).unwrap();
    let data = GameData::build(&order, &schema, table, &catalogue, &refs);
    Fixture { data, forms, refs, plugin_entries }
}

fn minimal_fos_with_created_ref(created_ff: u32) -> Fos {
    // Saved load order: [FalloutNV.esm (0), ModA.esp (1), Gone.esp (2)]
    let form_ids = vec![ITEM_BASE, ROOM_ID, QUEST_ID, created_ff];

    let mut h = Vec::new();
    h.extend(0x30u32.to_le_bytes());
    h.push(BAR);
    h.extend([0u8; 64]);
    h.push(BAR);
    for v in [0u32, 0, 1] {
        h.extend(v.to_le_bytes());
        h.push(BAR);
    }
    for s in ["Courier", "Drifter"] {
        h.extend((s.len() as u16).to_le_bytes());
        h.push(BAR);
        h.extend(s.as_bytes());
        h.push(BAR);
    }
    h.extend(2u32.to_le_bytes());
    h.push(BAR);
    h.extend(4u16.to_le_bytes());
    h.push(BAR);
    h.extend(b"Room");
    h.push(BAR);
    h.extend(9u16.to_le_bytes());
    h.push(BAR);
    h.extend(b"000.01.00");

    let mut b = b"FO3SAVEGAME".to_vec();
    b.extend((h.len() as u32).to_le_bytes());
    b.extend(&h);

    let saved_plugins = ["FalloutNV.esm", "ModA.esp", "Gone.esp"];
    let mut pl = vec![saved_plugins.len() as u8, BAR];
    for name in saved_plugins {
        pl.extend((name.len() as u16).to_le_bytes());
        pl.push(BAR);
        pl.extend(name.as_bytes());
        pl.push(BAR);
    }
    b.push(27);
    b.extend((pl.len() as u32).to_le_bytes());
    b.extend(&pl);

    let table_at = b.len();
    b.extend([0u8; 32 + 78]);
    let g1 = b.len();
    let cf = b.len();

    // Changed form: created 0xFF REFR (index 4 in form_ids, bit 1):
    // 32-byte header: cell ref3 (ROOM_ID = index 2) + 6*f32 + u8(0x01) + base ref3 (ITEM_BASE = index 1) + BAR.
    let mut cr = Vec::new();
    cr.extend([0x00, 0x00, 0x02]);
    for f in [10.0f32, 20.0, 30.0, 0.0, 0.0, 0.0] {
        cr.extend(f.to_le_bytes());
    }
    cr.push(0x01);
    cr.extend([0x00, 0x00, 0x01]);
    cr.push(BAR);
    assert_eq!(cr.len(), 32);
    b.extend([0x00, 0x00, 0x04]); // created_ff (index 4 in form_ids)
    b.extend((1u32 << 1).to_le_bytes());
    b.push(TYPE_REFR | 0x40);
    b.push(27);
    b.extend((cr.len() as u16).to_le_bytes());
    b.extend(&cr);

    // Changed form: QUEST_ID running (flags = bit 1) so it's present cleanly
    let qdata = vec![0x01, BAR];
    b.extend([0x00, 0x00, 0x03]); // QUEST_ID (index 3 in form_ids)
    b.extend((1u32 << 1).to_le_bytes());
    b.push(TYPE_QUST | 0x40);
    b.push(27);
    b.extend((qdata.len() as u16).to_le_bytes());
    b.extend(&qdata);

    let g2 = b.len();
    b.extend(1000u32.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    let fid = b.len();
    b.extend((form_ids.len() as u32).to_le_bytes());
    for id in &form_ids {
        b.extend(id.to_le_bytes());
    }
    b.extend(0u32.to_le_bytes());
    let tail = b.len();
    b.extend([5, 0, 0, 0, 0, 0, 0, 0, BAR]);

    for (i, v) in [fid, tail, g1, cf, g2, 0, 1, 2].iter().enumerate() {
        b[table_at + 4 * i..table_at + 4 * i + 4].copy_from_slice(&(*v as u32).to_le_bytes());
    }
    Fos::parse(&b).expect("synthetic fos parses")
}

fn chunk(tag: &[u8; 4], ver: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + payload.len());
    out.extend_from_slice(tag);
    out.extend_from_slice(&ver.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn plugin_block(opcode: u32, chunks: &[Vec<u8>]) -> Vec<u8> {
    let total: usize = chunks.iter().map(Vec::len).sum();
    let mut out = Vec::with_capacity(12 + total);
    out.extend_from_slice(&opcode.to_le_bytes());
    out.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    for c in chunks {
        out.extend_from_slice(c);
    }
    out
}

fn build_synthetic_cosave(created_ff: u32) -> NvseCosave {
    // Saved mod list matches FOS: [0: FalloutNV.esm, 1: ModA.esp, 2: Gone.esp]
    let saved_mods = ["FalloutNV.esm", "ModA.esp", "Gone.esp"];
    let mut sdom = vec![saved_mods.len() as u8];
    for m in saved_mods {
        sdom.extend_from_slice(&(m.len() as u16).to_le_bytes());
        sdom.extend_from_slice(m.as_bytes());
    }

    let moda_saved_form: u32 = 0x0100_0000 | MODA_ITEM_LOCAL;
    let gone_saved_form: u32 = 0x0200_0555;

    // RVRA #1: child list owned by ModA (index 1), holding float 777.0 + created 0xFF ref + ModA form
    let mut arr1 = vec![0x01u8];
    arr1.extend_from_slice(&1u32.to_le_bytes());
    arr1.extend_from_slice(&[1, 1]); // numeric packed list
    arr1.extend_from_slice(&2u32.to_le_bytes());
    arr1.extend_from_slice(&[0x01, 0x01]);
    arr1.extend_from_slice(&3u32.to_le_bytes());
    arr1.push(1);
    arr1.extend_from_slice(&777.0f64.to_le_bytes());
    arr1.push(2);
    arr1.extend_from_slice(&created_ff.to_le_bytes());
    arr1.push(2);
    arr1.extend_from_slice(&moda_saved_form.to_le_bytes());

    // RVRA #2: stringmap owned by runtime (0xFF) pointing to child array #1
    let mut arr2 = vec![0xFFu8];
    arr2.extend_from_slice(&2u32.to_le_bytes());
    arr2.extend_from_slice(&[3, 0]); // stringmap
    arr2.extend_from_slice(&1u32.to_le_bytes());
    arr2.push(0xFF);
    arr2.extend_from_slice(&2u32.to_le_bytes());
    arr2.extend_from_slice(&4u16.to_le_bytes());
    arr2.extend_from_slice(b"kArr");
    arr2.push(4);
    arr2.extend_from_slice(&1u32.to_le_bytes());
    arr2.extend_from_slice(&4u16.to_le_bytes());
    arr2.extend_from_slice(b"kStr");
    arr2.push(3);
    arr2.extend_from_slice(&5u16.to_le_bytes());
    arr2.extend_from_slice(b"Hello");

    // RVRA #3: owned by Gone.esp (index 2) -> must be skipped!
    let mut arr3 = vec![0x02u8];
    arr3.extend_from_slice(&3u32.to_le_bytes());
    arr3.extend_from_slice(&[1, 1]);
    arr3.extend_from_slice(&1u32.to_le_bytes());
    arr3.push(0x02);
    arr3.extend_from_slice(&1u32.to_le_bytes());
    arr3.push(1);
    arr3.extend_from_slice(&1.0f64.to_le_bytes());

    // JPAV: 3 mod buckets: ModA (1), runtime (0xFF), Gone (2)
    let mut jpav = Vec::new();
    jpav.extend_from_slice(&3u16.to_le_bytes());
    // ModA (index 1): 1 owner (Player 0x14), 1 private var "privModA" with [ModA form, "ok"]
    jpav.push(0x01);
    jpav.extend_from_slice(&1u16.to_le_bytes());
    jpav.extend_from_slice(&PLAYER_REF.to_le_bytes());
    jpav.extend_from_slice(&1u16.to_le_bytes());
    jpav.push(8);
    jpav.extend_from_slice(b"privModA");
    jpav.extend_from_slice(&2u16.to_le_bytes());
    jpav.push(2);
    jpav.extend_from_slice(&moda_saved_form.to_le_bytes());
    jpav.push(4);
    jpav.extend_from_slice(&2u16.to_le_bytes());
    jpav.extend_from_slice(b"ok");
    // Runtime (0xFF): 2 owners: Player (public "_pubFF" -> created_ff) & created_ff ("_ownFF" -> 77.0)
    jpav.push(0xFF);
    jpav.extend_from_slice(&2u16.to_le_bytes());
    jpav.extend_from_slice(&PLAYER_REF.to_le_bytes());
    jpav.extend_from_slice(&1u16.to_le_bytes());
    jpav.push(6);
    jpav.extend_from_slice(b"_pubFF");
    jpav.extend_from_slice(&1u16.to_le_bytes());
    jpav.push(2);
    jpav.extend_from_slice(&created_ff.to_le_bytes());
    jpav.extend_from_slice(&created_ff.to_le_bytes());
    jpav.extend_from_slice(&1u16.to_le_bytes());
    jpav.push(6);
    jpav.extend_from_slice(b"_ownFF");
    jpav.extend_from_slice(&1u16.to_le_bytes());
    jpav.push(1);
    jpav.extend_from_slice(&77.0f64.to_le_bytes());
    // Gone (index 2): 1 owner (Player), 1 private var "gonePriv" -> must be skipped!
    jpav.push(0x02);
    jpav.extend_from_slice(&1u16.to_le_bytes());
    jpav.extend_from_slice(&PLAYER_REF.to_le_bytes());
    jpav.extend_from_slice(&1u16.to_le_bytes());
    jpav.push(8);
    jpav.extend_from_slice(b"gonePriv");
    jpav.extend_from_slice(&1u16.to_le_bytes());
    jpav.push(1);
    jpav.extend_from_slice(&5.0f64.to_le_bytes());

    // JPRM: 1 mod (0xFF), 1 map "_rm" with 3 entries (ModA form -> 12.5, Player base -> ModA form, Gone form -> skipped)
    let mut jprm = Vec::new();
    jprm.extend_from_slice(&1u16.to_le_bytes());
    jprm.push(0xFF);
    jprm.extend_from_slice(&1u16.to_le_bytes());
    jprm.push(3);
    jprm.extend_from_slice(b"_rm");
    jprm.extend_from_slice(&3u16.to_le_bytes());
    jprm.extend_from_slice(&moda_saved_form.to_le_bytes());
    jprm.push(1);
    jprm.extend_from_slice(&12.5f64.to_le_bytes());
    jprm.extend_from_slice(&PLAYER_BASE.to_le_bytes());
    jprm.push(2);
    jprm.extend_from_slice(&moda_saved_form.to_le_bytes());
    jprm.extend_from_slice(&gone_saved_form.to_le_bytes());
    jprm.push(1);
    jprm.extend_from_slice(&9.0f64.to_le_bytes());

    // JPLR: 1 link (decoded for inspect, skipped during convert)
    let mut jplr = Vec::new();
    jplr.extend_from_slice(&1u16.to_le_bytes());
    jplr.extend_from_slice(&PROP_REF.to_le_bytes());
    jplr.extend_from_slice(&PLAYER_REF.to_le_bytes());
    jplr.push(0xFF);

    // JPSV: 1 owner (QUEST_ID), 2 added vars ("cvNum" = -12.5, "cvRef" = moda_saved_form)
    let mut jpsv = Vec::new();
    jpsv.extend_from_slice(&1u16.to_le_bytes());
    jpsv.extend_from_slice(&QUEST_ID.to_le_bytes());
    jpsv.extend_from_slice(&2u16.to_le_bytes());
    jpsv.push(0xFF);
    jpsv.push(5);
    jpsv.extend_from_slice(b"cvNum");
    jpsv.extend_from_slice(&(-12.5f64).to_le_bytes());
    jpsv.push(0xFF);
    jpsv.push(5);
    jpsv.extend_from_slice(b"cvRef");
    jpsv.extend_from_slice(&moda_saved_form.to_le_bytes());
    jpsv.extend_from_slice(&0u32.to_le_bytes());

    // SMGJ: 2 JohnnyGuitar extra misc stats
    let mut smgj = Vec::new();
    smgj.extend_from_slice(&2u16.to_le_bytes());
    smgj.extend_from_slice(&10u16.to_le_bytes());
    smgj.extend_from_slice(b"CustomStat");
    smgj.extend_from_slice(&17i32.to_le_bytes());
    smgj.extend_from_slice(&7u16.to_le_bytes());
    smgj.extend_from_slice(b"NegStat");
    smgj.extend_from_slice(&(-5i32).to_le_bytes());

    // OSMS: ShowOff AuxStringMapArray
    let mut osms = Vec::new();
    osms.extend_from_slice(&1u16.to_le_bytes());
    osms.push(0xFF);
    osms.extend_from_slice(&1u16.to_le_bytes());
    osms.push(6);
    osms.extend_from_slice(b"_soMap");
    osms.extend_from_slice(&2u16.to_le_bytes());
    osms.push(4);
    osms.extend_from_slice(b"KeyF");
    osms.push(1);
    osms.extend_from_slice(&19.25f64.to_le_bytes());
    osms.push(4);
    osms.extend_from_slice(b"KeyR");
    osms.push(2);
    osms.extend_from_slice(&moda_saved_form.to_le_bytes());

    // SOAT: 1 ShowOff AuxTimer (decoded for inspect, skipped during convert)
    let mut soat = Vec::new();
    soat.extend_from_slice(&1u16.to_le_bytes());
    soat.push(0xFF);
    soat.extend_from_slice(&1u16.to_le_bytes());
    soat.extend_from_slice(&PLAYER_REF.to_le_bytes());
    soat.extend_from_slice(&1u16.to_le_bytes());
    soat.push(4);
    soat.extend_from_slice(b"_tmr");
    soat.extend_from_slice(&60.0f64.to_le_bytes());
    soat.extend_from_slice(&60.0f64.to_le_bytes());
    soat.extend_from_slice(&4u32.to_le_bytes());

    let mut raw = Vec::new();
    let plugins = [
        plugin_block(
            PLUGIN_NVSE,
            &[
                chunk(b"SDOM", 0, &sdom),
                chunk(b"SVRA", 2, &[]),
                chunk(b"RVRA", 2, &arr1),
                chunk(b"RVRA", 2, &arr2),
                chunk(b"RVRA", 2, &arr3),
                chunk(b"EVRA", 2, &[]),
                chunk(b"SVTS", 0, &[]),
                chunk(b"EVTS", 0, &[]),
            ],
        ),
        plugin_block(
            PLUGIN_JIP,
            &[
                chunk(b"JPAV", 10, &jpav),
                chunk(b"JPRM", 10, &jprm),
                chunk(b"JPLR", 9, &jplr),
                chunk(b"JPSV", 9, &jpsv),
                chunk(b"JPGF", 1, &[0, 0, 0, 0, 0, 0, 0x80, 0x3F]),
            ],
        ),
        plugin_block(PLUGIN_JOHNNYGUITAR, &[chunk(b"SMGJ", 1, &smgj)]),
        plugin_block(PLUGIN_SHOWOFF, &[chunk(b"OSMS", 10, &osms), chunk(b"SOAT", 1, &soat)]),
        plugin_block(0x0000_7777, &[chunk(b"FOOB", 2, &[9, 8, 7])]),
    ];
    raw.extend_from_slice(b"NVSE");
    raw.extend_from_slice(&1u32.to_le_bytes());
    raw.extend_from_slice(&0x0004_0006u32.to_le_bytes());
    raw.extend_from_slice(&0x0400_20D0u32.to_le_bytes());
    raw.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
    for p in &plugins {
        raw.extend_from_slice(p);
    }
    NvseCosave::parse(&raw).expect("synthetic cosave parses")
}

#[test]
fn cosave_inspect_and_convert_remaps_plugins_and_0xff_and_roundtrips() {
    let fx = build_fixture();
    let created_ff = 0xFF00_1321;
    let fos = minimal_fos_with_created_ref(created_ff);
    let cosave = build_synthetic_cosave(created_ff);

    // 1. Inspect report includes full cosave summary & unknown chunks.
    let inspect = inspect_fos_with_cosave(&fos, true, Some(&cosave));
    assert!(inspect.cosave_present);
    let cs = inspect.cosave.expect("cosave inspect summary present");
    assert_eq!(cs.format_version, 1);
    assert_eq!(cs.decoded_counts["arrays"], 3);
    assert_eq!(cs.decoded_counts["jip_aux_vars"], 4);
    assert_eq!(cs.decoded_counts["jip_refmaps"], 1);
    assert_eq!(cs.decoded_counts["jip_linked_refs"], 1);
    assert_eq!(cs.decoded_counts["jip_added_script_vars"], 2);
    assert_eq!(cs.decoded_counts["jg_extra_misc_stats"], 2);
    assert_eq!(cs.decoded_counts["showoff_strmaps"], 1);
    assert_eq!(cs.decoded_counts["showoff_aux_timers"], 1);
    assert_eq!(cs.unknown_chunks.len(), 2);

    // 2. Convert with cosave: ModA shifted from 0x01 -> 0x02, Gone.esp missing, 0xFF remapped.
    let opts = ConvertOptions {
        kind: SaveKind::Manual,
        plugins: fx.plugin_entries.clone(),
        include_thumbnail: false,
        cosave_present: true,
    };
    let converted = convert_fos_with_cosave(&fos, Some(&cosave), &fx.data, &fx.forms, &fx.refs, &opts)
        .expect("convert succeeds");
    assert!(!converted.report.cosave_ignored);
    assert!(converted.report.cosave_imported);
    assert_eq!(converted.report.converted["cosave_arrays"], 2);
    assert_eq!(converted.report.converted["cosave_jip_aux_vars"], 3);
    assert_eq!(converted.report.converted["cosave_jip_refmaps"], 1);
    assert_eq!(converted.report.converted["cosave_jip_refmap_entries"], 2);
    assert_eq!(converted.report.converted["cosave_showoff_strmaps"], 1);
    assert_eq!(converted.report.converted["cosave_jg_extra_misc_stats"], 2);
    assert_eq!(converted.report.converted["cosave_jip_added_script_vars"], 2);

    // Verify skipped counts for unloaded plugin + unpersisted JPLR/SOAT + unknown chunks.
    assert_eq!(converted.report.skipped["cosave_array: owning plugin not loaded"], 1);
    assert_eq!(converted.report.skipped["cosave_jip_aux: owning plugin not loaded"], 1);
    assert_eq!(converted.report.skipped["cosave_jip_refmap_entry: form of a plugin not loaded"], 1);
    assert_eq!(
        converted.report.skipped["cosave_jip_linked_ref: GameSnapshot does not persist linked references"],
        1
    );
    assert_eq!(
        converted.report.skipped["cosave_showoff_aux_timer: GameSnapshot does not persist aux timers"],
        1
    );
    assert_eq!(converted.report.skipped["cosave_chunk: 0x00002200/JPGF v1 unmapped"], 1);
    assert_eq!(converted.report.skipped["cosave_chunk: 0x00007777/FOOB v2 unmapped"], 1);

    // 3. Round-trip through NVFSAVE binary container and Game::restore!
    let body_bytes = serde_json::to_vec(&converted.body).expect("serialize body");
    let encoded = fnv_save::encode(&converted.header, &converted.shot, &body_bytes);
    let decoded = fnv_save::decode(std::path::Path::new("synthetic.nvfsave"), &encoded, true)
        .expect("decode nvfsave");
    let body: SaveBody<GameSnapshot> =
        serde_json::from_slice(&decoded.body).expect("deserialize body");
    let snap = &body.game;

    let moda_remapped = 0x0200_0000 | MODA_ITEM_LOCAL;
    // Discover the newly assigned dynamic 0xFF FormId from Player's `_pubff` aux var.
    let new_ff = match snap.ext.jip.aux[&PLAYER_REF]["_pubff"][0] {
        XVal::Ref(r) => r,
        ref other => panic!("expected remapped 0xFF ref, got {other:?}"),
    };
    assert_eq!(new_ff >> 24, 0xFF);
    assert!(snap.sim.refs.contains_key(&FormId(new_ff)));

    // Arrays preserved with original IDs and remapped ModA + 0xFF forms.
    let a1 = &snap.ext.arrays[&1];
    assert_eq!(a1.kind, ArrayKind::List);
    assert_eq!(a1.entries[0], (XKey::Num(0.0), XVal::Num(777.0)));
    assert_eq!(a1.entries[1], (XKey::Num(1.0), XVal::Ref(new_ff)));
    assert_eq!(a1.entries[2], (XKey::Num(2.0), XVal::Ref(moda_remapped)));
    let a2 = &snap.ext.arrays[&2];
    assert_eq!(a2.kind, ArrayKind::StringMap);
    assert_eq!(a2.get(&XKey::Str("kArr".into())), Some(&XVal::Array(1)));
    assert_eq!(a2.get(&XKey::Str("kStr".into())), Some(&XVal::Str("Hello".into())));
    assert!(!snap.ext.arrays.contains_key(&3));

    // JIP AuxVars: private ModA key carries `\u{1}moda.esp`, 0xFF owner remapped to `new_ff`.
    assert_eq!(
        snap.ext.jip.aux[&PLAYER_REF]["privmoda\u{1}moda.esp"],
        vec![XVal::Ref(moda_remapped), XVal::Str("ok".into())]
    );
    assert_eq!(snap.ext.jip.aux[&new_ff]["_ownff"], vec![XVal::Num(77.0)]);

    // JIP RefMaps sorted ascending by FormID with Gone.esp entry dropped.
    assert_eq!(
        snap.ext.jip.refmaps["_rm"],
        vec![(PLAYER_BASE, XVal::Ref(moda_remapped)), (moda_remapped, XVal::Num(12.5))]
    );

    // ShowOff AuxStringMapArray preserved with original key casing.
    assert_eq!(
        snap.ext.jip.strmaps["_somap"]["keyf"],
        ("KeyF".into(), XVal::Num(19.25))
    );
    assert_eq!(
        snap.ext.jip.strmaps["_somap"]["keyr"],
        ("KeyR".into(), XVal::Ref(moda_remapped))
    );

    // JohnnyGuitar extra misc stats (lower-cased).
    assert_eq!(snap.extra_misc_stats["customstat"], 17.0);
    assert_eq!(snap.extra_misc_stats["negstat"], -5.0);

    // JIP added script variables attached to QUEST_ID's script instance.
    assert_eq!(snap.jip10.added_vars[&FormId(SCRIPT_ID)], vec!["cvNum".to_string(), "cvRef".to_string()]);
    let q_inst = snap
        .instances
        .iter()
        .find(|(k, _)| *k == InstKey::Quest { quest: FormId(QUEST_ID) })
        .map(|(_, inst)| inst)
        .expect("quest instance present");
    assert_eq!(q_inst.vars[&ADDED_VAR_BASE], Value::Num(-12.5));
    assert_eq!(q_inst.vars[&(ADDED_VAR_BASE + 1)], Value::Ref(moda_remapped));

    // Restoring into a live Game succeeds and preserves imported extender state.
    let world = World::new(&fx.forms, &fx.refs, fnv_sim::SimConfig::default());
    let mut restored = Game::restore(&fx.data, world, GameConfig::default(), snap)
        .expect("Game::restore succeeds");
    // New array allocated after restore must not collide with imported IDs 1 and 2.
    let fresh_id = restored.ext.new_array(ArrayKind::List, Vec::new());
    assert!(fresh_id > 2, "fresh array ID {fresh_id} must exceed imported max ID 2");
}

#[test]
fn fos_without_cosave_remains_completely_unchanged() {
    let fx = build_fixture();
    let fos = minimal_fos_with_created_ref(0xFF00_1321);
    let opts = ConvertOptions {
        kind: SaveKind::Manual,
        plugins: fx.plugin_entries.clone(),
        include_thumbnail: false,
        cosave_present: false,
    };
    let converted = convert_fos(&fos, &fx.data, &fx.forms, &fx.refs, &opts).expect("convert ok");
    assert!(!converted.report.cosave_ignored);
    assert!(!converted.report.cosave_imported);
    assert!(converted.body.game.ext.is_empty());
    assert!(converted.body.game.extra_misc_stats.is_empty());
    assert!(converted.body.game.jip10.is_empty());
}
