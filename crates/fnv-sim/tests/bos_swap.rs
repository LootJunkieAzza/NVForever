//! Clean-room integration tests for Base Object Swapper (`BosTable`) wired through
//! `WorldIndex` and `RefTable::from_index` into `fnv_sim::World`.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_sim::world::Outcome;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_world::{BosTable, WorldIndex};
use std::path::PathBuf;

fn schema() -> Schema {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv");
    Schema::load_dir(&dir).expect("load fnv schemas")
}

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

fn tes4(masters: &[&str]) -> Vec<u8> {
    let hedr = cat(&[&1.34f32.to_le_bytes(), &0i32.to_le_bytes(), &0x800u32.to_le_bytes()]);
    let mut subs = vec![sub(b"HEDR", hedr)];
    for m in masters {
        subs.push(sub(b"MAST", z(m)));
        subs.push(sub(b"DATA", 0u64.to_le_bytes().to_vec()));
    }
    record(b"TES4", 1, 0, &subs)
}

fn value_weight(value: i32, weight: f32) -> Vec<u8> {
    cat(&[&value.to_le_bytes(), &weight.to_le_bytes()])
}

fn cnto(item: u32, count: i32) -> Subrecord {
    sub(b"CNTO", cat(&[&item.to_le_bytes(), &count.to_le_bytes()]))
}

fn posrot(p: [f32; 3], r: [f32; 3]) -> Vec<u8> {
    p.iter().chain(r.iter()).flat_map(|f| f.to_le_bytes()).collect()
}

fn refr(fid: u32, base: u32, pos: [f32; 3]) -> Vec<u8> {
    record(b"REFR", 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", posrot(pos, [0.0; 3]))])
}

const ITEM_A: u32 = 0x0000_1001;
const ITEM_B: u32 = 0x0000_1002;
const ITEM_C: u32 = 0x0000_1003;
const CONT_ORIG: u32 = 0x0000_1100;
const CONT_SWAP_LOCAL: u32 = 0x0100_1101;
const CELL_A: u32 = 0x0000_2000;
const REF_ITEM_FORM_SWAP: u32 = 0x0000_3001;
const REF_ITEM_REF_OVERRIDE: u32 = 0x0000_3002;
const REF_ITEM_UNTOUCHED: u32 = 0x0000_3003;
const REF_CONT_SWAPPED: u32 = 0x0000_3004;

fn build_plugins() -> Vec<Plugin> {
    let player_base = record(b"NPC_", 0, 0x0000_0007, &[sub(b"EDID", z("Player"))]);
    let misc = cat(&[
        &record(b"MISC", 0, ITEM_A, &[sub(b"EDID", z("SynItemOrig")), sub(b"DATA", value_weight(5, 1.0))]),
        &record(b"MISC", 0, ITEM_B, &[sub(b"EDID", z("SynItemFormSwap")), sub(b"DATA", value_weight(10, 1.0))]),
        &record(b"MISC", 0, ITEM_C, &[sub(b"EDID", z("SynItemRefOverride")), sub(b"DATA", value_weight(15, 1.0))]),
    ]);
    let cont_orig = record(
        b"CONT",
        0,
        CONT_ORIG,
        &[sub(b"EDID", z("SynContOrig")), cnto(ITEM_A, 2), sub(b"DATA", cat(&[&[0u8], &50f32.to_le_bytes()]))],
    );
    let cell = record(b"CELL", 0, CELL_A, &[sub(b"EDID", z("SynBosCell")), sub(b"DATA", vec![1])]);
    let refs = cat(&[
        &refr(REF_ITEM_FORM_SWAP, ITEM_A, [10.0, 0.0, 0.0]),
        &refr(REF_ITEM_REF_OVERRIDE, ITEM_A, [20.0, 0.0, 0.0]),
        &refr(REF_ITEM_UNTOUCHED, ITEM_B, [30.0, 0.0, 0.0]),
        &refr(REF_CONT_SWAPPED, CONT_ORIG, [40.0, 0.0, 0.0]),
    ]);
    let label = CELL_A.to_le_bytes();
    let children = group(label, 6, &group(label, 9, &refs));
    let cell_top = top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat())));
    let master_bytes = cat(&[&tes4(&[]), &top(b"NPC_", &player_base), &top(b"MISC", &misc), &top(b"CONT", &cont_orig), &cell_top]);
    let master = Plugin::parse(&master_bytes, "SynMaster.esm".into()).expect("parse master");

    let cont_swap = record(
        b"CONT",
        0,
        CONT_SWAP_LOCAL,
        &[sub(b"EDID", z("SynContSwapped")), cnto(ITEM_C, 4), sub(b"DATA", cat(&[&[0u8], &50f32.to_le_bytes()]))],
    );
    let patch_bytes = cat(&[&tes4(&["SynMaster.esm"]), &top(b"CONT", &cont_swap)]);
    let patch = Plugin::parse(&patch_bytes, "SynBosPatch.esp".into()).expect("parse patch");
    vec![master, patch]
}

#[test]
fn bos_swaps_align_world_placement_and_sim_refs_and_preserve_snapshots() {
    let schema = schema();
    let plugins = build_plugins();
    let lo = LoadOrder::new(&plugins).expect("load order");
    let forms = FormStore::build(&lo, &schema);

    // Baseline without BOS: untouched original bases.
    let plain_ix = WorldIndex::build(&lo, &schema);
    let plain_refs = RefTable::from_index(&lo, &schema, &plain_ix);
    assert_eq!(plain_ix.placement(&lo, &schema, REF_ITEM_FORM_SWAP).unwrap().base, Some(ITEM_A));
    assert_eq!(plain_refs.get(FormId(REF_ITEM_FORM_SWAP)).unwrap().base, FormId(ITEM_A));
    assert_eq!(plain_refs.get(FormId(REF_CONT_SWAPPED)).unwrap().base, FormId(CONT_ORIG));

    // Apply BOS rules mixing [Forms], [References], EditorIDs, and cross-plugin FormIDs.
    let ini = r#"
[Forms]
SynItemOrig|SynItemFormSwap
0x001100~SynMaster.esm|0x001101~SynBosPatch.esp

[References]
0x003002~SynMaster.esm|SynItemRefOverride
"#;
    let bos = BosTable::load_from_sources(&lo, [("synthetic_SWAP.ini", ini)]);
    assert_eq!(bos.report().form_rules, 2);
    assert_eq!(bos.report().reference_rules, 1);

    let mut ix = WorldIndex::build(&lo, &schema);
    ix.apply_bos(bos);
    let refs = RefTable::from_index(&lo, &schema, &ix);

    // Placement and RefTable agree on every reference.
    for &(r, expected_base) in &[
        (REF_ITEM_FORM_SWAP, ITEM_B),
        (REF_ITEM_REF_OVERRIDE, ITEM_C),
        (REF_ITEM_UNTOUCHED, ITEM_B),
        (REF_CONT_SWAPPED, CONT_SWAP_LOCAL),
    ] {
        assert_eq!(ix.placement(&lo, &schema, r).unwrap().base, Some(expected_base), "WorldIndex::placement mismatch for {r:#X}");
        assert_eq!(refs.get(FormId(r)).unwrap().base, FormId(expected_base), "RefTable mismatch for {r:#X}");
    }

    // Simulation runtime sees the swapped base forms (item pickup + container contents)
    // and keeps save/snapshot runtime state intact over swapped references.
    let mut w = World::new(&forms, &refs, SimConfig { seed: 7, ..SimConfig::default() });
    let player = FormId::PLAYER_REF;

    // Picking up REF_ITEM_REF_OVERRIDE grants ITEM_C (the [References]-swapped base).
    let act = w.activate(player, FormId(REF_ITEM_REF_OVERRIDE)).unwrap();
    assert_eq!(act.outcome, Outcome::PickedUp { item: FormId(ITEM_C), count: 1, owner: None });
    assert_eq!(w.item_count(player, FormId(ITEM_C)).unwrap(), 1);
    assert_eq!(w.item_count(player, FormId(ITEM_A)).unwrap(), 0);

    // REF_CONT_SWAPPED uses the swapped container's authored inventory (4x ITEM_C, 0x ITEM_A).
    assert_eq!(w.item_count(FormId(REF_CONT_SWAPPED), FormId(ITEM_C)).unwrap(), 4);
    assert_eq!(w.item_count(FormId(REF_CONT_SWAPPED), FormId(ITEM_A)).unwrap(), 0);

    // Runtime mutation + save/restore snapshot roundtrip remains intact on BOS-swapped refs.
    w.add_item(FormId(REF_CONT_SWAPPED), FormId(ITEM_B), 2).unwrap();
    let snap_json = serde_json::to_string(&w.snapshot()).unwrap();
    let snap: fnv_sim::Snapshot = serde_json::from_str(&snap_json).unwrap();
    let mut restored = World::restore(&forms, &refs, &snap).unwrap();

    assert_eq!(restored.item_count(player, FormId(ITEM_C)).unwrap(), 1);
    assert!(restored.peek(FormId(REF_ITEM_REF_OVERRIDE)).unwrap().deleted());
    assert_eq!(restored.item_count(FormId(REF_CONT_SWAPPED), FormId(ITEM_C)).unwrap(), 4);
    assert_eq!(restored.item_count(FormId(REF_CONT_SWAPPED), FormId(ITEM_B)).unwrap(), 2);
}
