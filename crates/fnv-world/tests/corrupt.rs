//! Broken and hostile world data on a synthetic plugin built in memory (no game data):
//! a worldspace that names itself as its parent, references to FormID 0 and to
//! themselves, short subrecords, then thousands of byte-mutated copies. Every reader
//! must return an error or a partial result, never panic or hang.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::{audio, land, placed, sky, xrgd, WorldIndex};

const WRLD: u32 = 0x100;
const PCELL: u32 = 0x101;
const CELL: u32 = 0x102;
const REFR: u32 = 0x103;
const SELF_REF: u32 = 0x104;
const LAND: u32 = 0x105;

fn sub(sig: &[u8; 4], data: &[u8]) -> Subrecord {
    Subrecord { sig: *sig, data: data.to_vec(), extended: false }
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [sig.as_slice(), &(payload.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [b"GRUP".as_slice(), &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

fn refr(fid: u32, base: u32) -> Vec<u8> {
    let pos: Vec<u8> = [1.0f32, 2.0, 3.0, 0.0, 0.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
    record(b"REFR", fid, &[sub(b"NAME", &base.to_le_bytes()), sub(b"DATA", &pos), sub(b"XRGD", &[0; 30])])
}

fn plugin_bytes() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    let tes4 = record(b"TES4", 0, &[sub(b"HEDR", &hedr)]);
    // The worldspace is its own parent, climate and water.
    let wrld = record(
        b"WRLD",
        WRLD,
        &[sub(b"EDID", b"Loop\0"), sub(b"WNAM", &WRLD.to_le_bytes()), sub(b"CNAM", &WRLD.to_le_bytes()), sub(b"NAM2", &WRLD.to_le_bytes()), sub(b"DNAM", &[0; 3])],
    );
    let pcell = record(b"CELL", PCELL, &[sub(b"DATA", &[0])]);
    let xclc = [0i32.to_le_bytes(), 0i32.to_le_bytes(), 0u32.to_le_bytes()].concat();
    let cell = record(b"CELL", CELL, &[sub(b"EDID", b"C\0"), sub(b"DATA", &[2]), sub(b"XCLC", &xclc), sub(b"XCLW", &f32::NAN.to_le_bytes())]);
    // VHGT one byte short of its 1,096; LAND as its own base.
    let land = record(b"LAND", LAND, &[sub(b"DATA", &[0; 4]), sub(b"VHGT", &[1; 1095])]);
    let temp = [refr(REFR, 0), refr(SELF_REF, SELF_REF), refr(0, CELL), land].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &temp));
    let sub_block = group([0; 4], 5, &[cell, children].concat());
    let world_children = group(WRLD.to_le_bytes(), 1, &[pcell, group([0; 4], 4, &sub_block)].concat());
    [tes4, group(*b"WRLD", 0, &[wrld, world_children].concat())].concat()
}

fn schema() -> Schema {
    Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap()
}

/// Every reader that takes a load order, on every FormID in it.
fn exercise(p: Plugin, schema: &Schema) {
    let plugins = [p];
    let Ok(lo) = LoadOrder::new(&plugins) else { return };
    let ix = WorldIndex::build(&lo, schema);
    let _ = audio::AudioForms::build(&lo, schema);
    let _ = sky::region_weathers(&lo, schema);
    let ids: Vec<u32> = lo.index.keys().copied().chain([0, u32::MAX]).collect();
    for id in ids {
        let _ = placed::placement(&lo, schema, id);
        let _ = placed::base_object(&lo, schema, id);
        let _ = land::load_land(&lo, schema, id, land::LandParts::ALL);
        let _ = xrgd::stored(&lo, id);
        let _ = audio::cell_audio(&lo, schema, id);
        let _ = sky::climate(&lo, schema, id);
        let _ = sky::weather(&lo, schema, id);
    }
    let _ = audio::markers(&lo, schema, &ix, ix.cells.keys().copied().collect::<Vec<_>>());
    for c in ix.cells.values() {
        let _ = audio::cells_in_earshot(&ix, c.form_id);
        let _ = c.water_surface(None);
    }
}

#[test]
fn self_references_and_null_formids() {
    let schema = schema();
    let p = Plugin::parse(&plugin_bytes(), "Loop.esm".into()).unwrap();
    let plugins = [p];
    let lo = LoadOrder::new(&plugins).unwrap();
    let ix = WorldIndex::build(&lo, &schema);
    let w = &ix.worldspaces[&WRLD];
    assert_eq!((w.parent, w.persistent_cell), (Some(WRLD), Some(PCELL)));
    assert_eq!(ix.cells[&CELL].grid, Some((0, 0)));
    // NaN water height falls back to the (absent) world default instead of drowning everything.
    assert_eq!(ix.cells[&CELL].water_surface(None), None);
    assert_eq!(ix.cells[&CELL].land, Some(LAND));
    // A short VHGT gives no heights rather than a panic.
    assert!(land::load_land(&lo, &schema, LAND, land::LandParts::ALL).is_none_or(|l| l.heights.is_none()));
    let [p] = plugins;
    exercise(p, &schema);
}

#[test]
fn mutated_world_plugins() {
    let schema = schema();
    let base = plugin_bytes();
    // xorshift64: deterministic, so a failure names its iteration.
    let mut s = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let mut parsed = 0;
    for _ in 0..3000 {
        let mut f = base.clone();
        for _ in 0..1 + next() % 4 {
            let at = (next() % f.len() as u64) as usize;
            f[at] = next() as u8;
        }
        if next() % 8 == 0 {
            f.truncate((next() % f.len() as u64) as usize);
        }
        if let Ok(p) = Plugin::parse(&f, "Loop.esm".into()) {
            parsed += 1;
            exercise(p, &schema);
        }
    }
    // Most mutations land in payloads, so most copies still parse and reach the readers.
    assert!(parsed > 1000, "{parsed}");
}
