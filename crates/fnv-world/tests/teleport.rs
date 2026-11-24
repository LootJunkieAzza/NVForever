//! Door teleports left stale by the load order (`fnv_world::teleport`), on synthetic
//! plugins built in memory and decoded through the repository schemas (no game data).
//! The case is the owner's (2026-11-05): a mod moves the destination door and rewrites
//! the XTEL of the door one leaves from; a later plugin overrides that door with the
//! master's XTEL.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::teleport::{door_xtel, mend, xtel};

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [sig.as_slice(), &(payload.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [b"GRUP".as_slice(), &((24 + body.len()) as u32).to_le_bytes(), sig, &0i32.to_le_bytes(), &[0; 8], body].concat()
}

fn tes4(masters: &[&str]) -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    let mut subs = vec![sub(b"HEDR", hedr)];
    for m in masters {
        let mut n = m.as_bytes().to_vec();
        n.push(0);
        subs.push(sub(b"MAST", n));
        subs.push(sub(b"DATA", vec![0; 8]));
    }
    record(b"TES4", 0, &subs)
}

fn posrot(p: [f32; 3], r: [f32; 3]) -> Vec<u8> {
    p.iter().chain(r.iter()).flat_map(|f| f.to_le_bytes()).collect()
}

const DOOR_BASE: u32 = 0x800;
/// The door one leaves from (FreesideGateToStripREF's part).
const FROM: u32 = 0x900;
/// Its destination door (the Strip's gate to Freeside).
const TO: u32 = 0x901;

fn door(fid: u32, pos: [f32; 3], to: Option<(u32, [f32; 3], f32)>) -> Vec<u8> {
    let mut subs = vec![sub(b"NAME", DOOR_BASE.to_le_bytes().to_vec())];
    if let Some((door, marker, z)) = to {
        subs.push(sub(b"XTEL", [door.to_le_bytes().to_vec(), posrot(marker, [0.0, 0.0, z]), 0u32.to_le_bytes().to_vec()].concat()));
    }
    subs.push(sub(b"DATA", posrot(pos, [0.0; 3])));
    record(b"REFR", fid, &subs)
}

fn plugin(name: &str, masters: &[&str], refs: &[Vec<u8>]) -> Plugin {
    let bytes = [tes4(masters), top(b"REFR", &refs.concat())].concat();
    Plugin::parse(&bytes, name.into()).unwrap()
}

fn schema() -> Schema {
    Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap()
}

const VANILLA_DOOR: [f32; 3] = [0.0, 0.0, 0.0];
const VANILLA_MARKER: [f32; 3] = [-240.0, -20.0, 20.0];
const MOVED_DOOR: [f32; 3] = [2400.0, 2000.0, 10.0];
const MOVED_MARKER: [f32; 3] = [2050.0, 2020.0, 10.0];

fn door_at(fid: u32, pos: [f32; 3], z: f32, to: Option<(u32, [f32; 3], f32)>) -> Vec<u8> {
    let mut subs = vec![sub(b"NAME", DOOR_BASE.to_le_bytes().to_vec())];
    if let Some((door, marker, rz)) = to {
        subs.push(sub(b"XTEL", [door.to_le_bytes().to_vec(), posrot(marker, [0.0, 0.0, rz]), 0u32.to_le_bytes().to_vec()].concat()));
    }
    subs.push(sub(b"DATA", posrot(pos, [0.0, 0.0, z])));
    record(b"REFR", fid, &subs)
}

fn master() -> Plugin {
    plugin("Master.esm", &[], &[door(FROM, [-7000.0, 0.0, 0.0], Some((TO, VANILLA_MARKER, 2.4))), door(TO, VANILLA_DOOR, Some((FROM, [-6800.0, 0.0, 0.0], 0.6)))])
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
}

/// The owner's case: the Strip mod moves the destination door and rewrites the XTEL;
/// a plugin with only the master overrides the door one leaves from with the master's
/// XTEL. The marker moves with the door: as far from it as the master had it.
#[test]
fn a_marker_from_a_plugin_that_never_saw_the_door_move_moves_with_the_door() {
    let s = schema();
    let p = vec![
        master(),
        plugin("Mover.esm", &["Master.esm"], &[door(FROM, [-7000.0, 0.0, 0.0], Some((TO, MOVED_MARKER, 2.36))), door(TO, MOVED_DOOR, None)]),
        plugin("Opener.esm", &["Master.esm"], &[door(FROM, [-6900.0, 10.0, 0.0], Some((TO, VANILLA_MARKER, 3.9)))]),
    ];
    let lo = LoadOrder::new(&p).unwrap();
    let x = door_xtel(&lo, &s, FROM).unwrap();
    assert_eq!(x.door, TO);
    assert!(close(x.pos, [2400.0 - 240.0, 2000.0 - 20.0, 30.0]), "{:?}", x.pos);
    assert!((x.rot[2] - 3.9).abs() < 1e-6, "the door did not turn: {:?}", x.rot);
    let (pi, r) = fnv_world::fields::winner(&lo, FROM).unwrap();
    let stored = xtel(&lo, &s, pi, r).unwrap();
    assert_eq!(stored.pos, VANILLA_MARKER, "the winner's as stored");
    let m = mend(&lo, &s, pi, &stored).unwrap();
    assert_eq!(p[m.seen_in].name, "Master.esm");
    assert!((m.moved_by - (2400.0f32.powi(2) + 2000.0f32.powi(2) + 100.0).sqrt()).abs() < 0.1);
}

/// A door turned as well as moved: the marker turns round it with the door (clockwise
/// seen from above for a positive z rotation).
#[test]
fn a_marker_turns_with_its_door() {
    let s = schema();
    let quarter = std::f32::consts::FRAC_PI_2;
    let p = vec![
        plugin("Master.esm", &[], &[door(FROM, [-7000.0, 0.0, 0.0], Some((TO, [0.0, 100.0, 0.0], 0.5))), door(TO, VANILLA_DOOR, None)]),
        plugin("Mover.esm", &["Master.esm"], &[door_at(TO, [500.0, 0.0, 0.0], quarter, None)]),
    ];
    let lo = LoadOrder::new(&p).unwrap();
    let x = door_xtel(&lo, &s, FROM).unwrap();
    // 100 north of the door, turned a quarter clockwise: 100 east of it.
    assert!(close(x.pos, [600.0, 0.0, 0.0]), "{:?}", x.pos);
    assert!((x.rot[2] - (0.5 + quarter)).abs() < 1e-5, "{:?}", x.rot);
}

fn tipped_door(fid: u32, pos: [f32; 3], rot: [f32; 3]) -> Vec<u8> {
    let subs = vec![sub(b"NAME", DOOR_BASE.to_le_bytes().to_vec()), sub(b"DATA", posrot(pos, rot))];
    record(b"REFR", fid, &subs)
}

/// Overnight queue item 37 (2026-11-07): a mod opens a gate by tipping the destination
/// door over, turning it and sinking it into the ground (Simple Open Freeside does this to
/// Freeside's gates). The door was not moved for anyone to come out of, so the marker
/// stays as stored: in front of where the gate stood, on the street, not swung round to
/// the far side of the wall and under the ground.
#[test]
fn a_marker_stays_when_its_door_was_tipped_over_to_open_the_way() {
    let s = schema();
    // Simple Open Freeside's VFSEastGateREF: x 5.6295 (-0.65 rad), y 0.4718, z 5.4105 -> 2.1063,
    // 290 units down.
    let p = vec![
        master(),
        plugin("Opener.esm", &["Master.esm"], &[tipped_door(TO, [-48.0, -229.0, -290.0], [5.6295, 0.4718, 2.1063])]),
    ];
    let lo = LoadOrder::new(&p).unwrap();
    assert_eq!(door_xtel(&lo, &s, FROM).unwrap().pos, VANILLA_MARKER);
    // Tipped only a little either way (float noise, a door set on a slope): still moves.
    for tilt in [0.05, std::f32::consts::TAU - 0.05] {
        let p = vec![master(), plugin("Mover.esm", &["Master.esm"], &[tipped_door(TO, MOVED_DOOR, [tilt, 0.0, 0.0])])];
        let lo = LoadOrder::new(&p).unwrap();
        assert!(close(door_xtel(&lo, &s, FROM).unwrap().pos, [2160.0, 1980.0, 30.0]), "tilt {tilt}");
    }
}

/// Markers whose plugin saw the destination door as it wins are used as stored: the
/// master alone, the mover's own XTEL, and a plugin that has the mover as a master.
#[test]
fn markers_whose_plugin_saw_the_door_where_it_is_are_used_as_stored() {
    let s = schema();
    let mover = || plugin("Mover.esm", &["Master.esm"], &[door(FROM, [-7000.0, 0.0, 0.0], Some((TO, MOVED_MARKER, 2.36))), door(TO, MOVED_DOOR, None)]);
    let cases: Vec<(Vec<Plugin>, [f32; 3])> = vec![
        (vec![master()], VANILLA_MARKER),
        (vec![master(), mover()], MOVED_MARKER),
        (vec![master(), mover(), plugin("Patch.esm", &["Master.esm", "Mover.esm"], &[door(FROM, [0.0; 3], Some((TO, [9000.0, 0.0, 0.0], 1.0)))])], [9000.0, 0.0, 0.0]),
    ];
    for (p, want) in cases {
        let lo = LoadOrder::new(&p).unwrap();
        assert_eq!(door_xtel(&lo, &s, FROM).unwrap().pos, want, "{}", p.last().unwrap().name);
    }
}
