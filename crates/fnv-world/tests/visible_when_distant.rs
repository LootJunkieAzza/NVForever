//! "Visible when distant" over every plugin's version of a reference and of its base
//! (#3014), and a base's OBND bounds, on two synthetic plugins built in memory and
//! decoded through the repository schemas (no game data). The second plugin overrides
//! records of the first with different header flags, as a LOD plugin flags bases another
//! plugin leaves plain, or a later plugin's copy of a record drops a flag.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::placed::{any_version_flag, base_flags, base_object, placement, placement_visible_when_distant, ref_flags, visible_when_distant};

const VWD: u32 = 0x8000;

fn sub(sig: &[u8; 4], data: &[u8]) -> Subrecord {
    Subrecord { sig: *sig, data: data.to_vec(), extended: false }
}

fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [sig.as_slice(), &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [b"GRUP".as_slice(), &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

/// An interior cell holding `refs` as its temporary children, as a top CELL group.
fn cell_with(refs: &[u8]) -> Vec<u8> {
    let cell = record(b"CELL", 0, CELL, &[text(b"EDID", "TestCell"), sub(b"DATA", &[1])]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, refs));
    top(b"CELL", &[cell, children].concat())
}

fn tes4(masters: &[&str]) -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    let mut subs = vec![sub(b"HEDR", &hedr)];
    for m in masters {
        let mut n = m.as_bytes().to_vec();
        n.push(0);
        subs.push(sub(b"MAST", &n));
        subs.push(sub(b"DATA", &[0; 8]));
    }
    record(b"TES4", if masters.is_empty() { 1 } else { 0 }, 0, &subs)
}

fn text(sig: &[u8; 4], s: &str) -> Subrecord {
    let mut d = s.as_bytes().to_vec();
    d.push(0);
    sub(sig, &d)
}

/// OBND: min then max, three i16 each.
fn obnd(min: [i16; 3], max: [i16; 3]) -> Subrecord {
    sub(b"OBND", &min.iter().chain(max.iter()).flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>())
}

/// A REFR placing `base` (a FormID local to the plugin holding it).
fn refr(fid: u32, flags: u32, base: u32) -> Vec<u8> {
    let data: Vec<u8> = [0f32; 6].iter().flat_map(|v| v.to_le_bytes()).collect();
    record(b"REFR", flags, fid, &[sub(b"NAME", &base.to_le_bytes()), sub(b"DATA", &data)])
}

const CELL: u32 = 0x300;
// Base records in the master.
const ROCK: u32 = 0x100; // flagged in the master only
const SHACK: u32 = 0x101; // plain in the master, flagged by the second plugin
const SIGN: u32 = 0x102; // plain everywhere
// References in the master.
const R_ROCK: u32 = 0x200; // plain reference of ROCK
const R_SHACK: u32 = 0x201; // plain reference of SHACK
const R_SIGN_OWN: u32 = 0x202; // flagged in the master, the override drops it
const R_SIGN: u32 = 0x203; // plain everywhere

fn master() -> Vec<u8> {
    let stat = [
        record(b"STAT", VWD, ROCK, &[text(b"EDID", "Rock"), obnd([-30, -40, 0], [30, 40, 0]), text(b"MODL", "Rocks\\Rock01.NIF")]),
        record(b"STAT", 0, SHACK, &[text(b"EDID", "Shack"), text(b"MODL", "Architecture\\Shack01.NIF")]),
        record(b"STAT", 0, SIGN, &[text(b"EDID", "Sign"), text(b"MODL", "Signs\\Sign01.NIF")]),
    ]
    .concat();
    let refs = [refr(R_ROCK, 0, ROCK), refr(R_SHACK, 0, SHACK), refr(R_SIGN_OWN, VWD | 0x400, SIGN), refr(R_SIGN, 0, SIGN)].concat();
    [tes4(&[]), top(b"STAT", &stat), cell_with(&refs)].concat()
}

/// Overrides from a plugin with the master as its master 0 (local FormIDs 0x00xxxxxx).
fn overrides() -> Vec<u8> {
    let stat = [
        // ROCK's override drops the flag; SHACK's adds it.
        record(b"STAT", 0, ROCK, &[text(b"EDID", "Rock"), text(b"MODL", "Rocks\\Rock01.NIF")]),
        record(b"STAT", VWD, SHACK, &[text(b"EDID", "Shack"), text(b"MODL", "Architecture\\Shack01.NIF")]),
    ]
    .concat();
    let refs = refr(R_SIGN_OWN, 0x400, SIGN);
    [tes4(&["Test.esm"]), top(b"STAT", &stat), cell_with(&refs)].concat()
}

#[test]
fn any_plugins_version_of_the_reference_or_its_base_counts() {
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let plugins = vec![Plugin::parse(&master(), "Test.esm".into()).unwrap(), Plugin::parse(&overrides(), "LodFlags.esp".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    assert_eq!((ref_flags::VISIBLE_WHEN_DISTANT, base_flags::VISIBLE_WHEN_DISTANT), (VWD, VWD));

    // The winning base records say what the last plugin says...
    assert!(!base_object(&lo, &schema, ROCK).unwrap().visible_when_distant(), "the override dropped the flag");
    assert!(base_object(&lo, &schema, SHACK).unwrap().visible_when_distant(), "the override added it");
    // ...but any version counts.
    assert!(any_version_flag(&lo, ROCK, VWD) && any_version_flag(&lo, SHACK, VWD));
    assert!(!any_version_flag(&lo, SIGN, VWD));
    assert!(!any_version_flag(&lo, 0x0FFF_FFFF, VWD), "no such record");

    // References: the base's flag in any version, or the reference's own in any version.
    assert!(visible_when_distant(&lo, R_ROCK, Some(ROCK)), "base flagged in the master only");
    assert!(visible_when_distant(&lo, R_SHACK, Some(SHACK)), "base flagged by the later plugin only");
    assert!(visible_when_distant(&lo, R_SIGN_OWN, Some(SIGN)), "the reference flagged in the master, dropped by the override");
    assert!(!visible_when_distant(&lo, R_SIGN, Some(SIGN)));
    assert!(!visible_when_distant(&lo, R_SIGN, None));

    // Through the placement: its winning record is plain, its base resolved.
    let p = placement(&lo, &schema, R_SIGN_OWN).unwrap();
    assert_eq!(p.flags & VWD, 0, "the winning reference is plain");
    assert_eq!(p.base, Some(SIGN));
    assert!(placement_visible_when_distant(&lo, &p));
    let p = placement(&lo, &schema, R_SHACK).unwrap();
    assert!(placement_visible_when_distant(&lo, &p));
    let p = placement(&lo, &schema, R_SIGN).unwrap();
    assert!(!placement_visible_when_distant(&lo, &p));
}

#[test]
fn a_base_carries_its_object_bounds() {
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let plugins = vec![Plugin::parse(&master(), "Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let rock = base_object(&lo, &schema, ROCK).unwrap();
    assert_eq!(rock.bounds, Some(([-30.0, -40.0, 0.0], [30.0, 40.0, 0.0])));
    assert_eq!(rock.bound_radius(), Some(50.0));
    assert!(rock.visible_when_distant(), "the master alone: flagged");
    let sign = base_object(&lo, &schema, SIGN).unwrap();
    assert_eq!((sign.bounds, sign.bound_radius()), (None, None));
}
