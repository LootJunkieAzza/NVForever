//! IDLE records and actor-base animation lists on synthetic plugins built in memory and
//! decoded through the repository schemas (no game data): a master and a plugin that
//! overrides one IDLE, deletes nothing, adds one and links it into the master's tree.

use fnv_actor::anims::actor_anim_lists;
use fnv_actor::idle::{Idles, ModelKind};
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_sim::extract::Issues;
use fnv_sim::FormId;

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [sig.as_slice(), &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload]
        .concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [b"GRUP".as_slice(), &((24 + body.len()) as u32).to_le_bytes(), sig, &0i32.to_le_bytes(), &[0; 8], body].concat()
}

fn tes4(masters: &[&str]) -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    let mut subs = vec![sub(b"HEDR", hedr)];
    for m in masters {
        subs.push(sub(b"MAST", z(m)));
        subs.push(sub(b"DATA", vec![0; 8]));
    }
    record(b"TES4", u32::from(masters.is_empty()), 0, &subs)
}

fn anam(parent: u32, previous: u32) -> Subrecord {
    sub(b"ANAM", [parent.to_le_bytes(), previous.to_le_bytes()].concat())
}

/// DATA: group byte, loop min/max, unused, replay delay; with `flags`, the 8-byte form.
fn data(group: u8, loop_: (u8, u8), replay: i16, flags: Option<u8>) -> Subrecord {
    let mut d = vec![group, loop_.0, loop_.1, 0];
    d.extend(replay.to_le_bytes());
    if let Some(f) = flags {
        d.extend([f, 0]);
    }
    sub(b"DATA", d)
}

/// CTDA in its 20-, 24- or 28-byte form (`tail` = run on, then reference).
fn ctda(type_raw: u8, comparison: u32, function: u16, p1: u32, tail: &[u32]) -> Subrecord {
    let mut d = vec![type_raw, 0, 0, 0];
    d.extend(comparison.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(p1.to_le_bytes());
    d.extend(0u32.to_le_bytes());
    for t in tail {
        d.extend(t.to_le_bytes());
    }
    sub(b"CTDA", d)
}

fn idle(flags: u32, fid: u32, edid: &str, model: Option<&str>, rest: Vec<Subrecord>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    if let Some(m) = model {
        subs.push(sub(b"MODL", z(m)));
    }
    subs.extend(rest);
    record(b"IDLE", flags, fid, &subs)
}

fn master() -> Vec<u8> {
    let idles = [
        idle(0, 0x100, "GroupRoot", Some("Characters\\_Male\\IdleAnims"), vec![
            ctda(0x00, 1.0f32.to_bits(), 72, 0, &[0, 0]),
            anam(0, 0),
            data(0x87, (0, 0), 0, Some(0)),
        ]),
        idle(0, 0x101, "LeafA", Some("Characters\\_Male\\IdleAnims\\A.kf"), vec![anam(0x100, 0), data(7, (2, 5), 10, None)]),
        idle(0, 0x102, "LeafB", Some("Characters\\_Male\\IdleAnims\\B.kf"), vec![
            ctda(0x80 | 0x01, 25.0f32.to_bits(), 77, 3, &[2]),
            ctda(0x60 | 0x04, 0x0000_0300, 14, 0, &[]),
            anam(0x100, 0x101),
            data(0x40 | 21, (255, 255), 0, Some(1)),
        ]),
        idle(0x20, 0x103, "Deleted", None, vec![]),
    ]
    .concat();
    let crea = record(b"CREA", 0, 0x200, &[
        sub(b"EDID", z("Dog")),
        sub(b"MODL", z("Creatures\\Dog\\Skeleton.NIF")),
        sub(b"NIFZ", z("Dog.NIF").into_iter().chain([0]).collect()),
        sub(b"KFFZ", b"SpecialIdle_Bark.kf\0b.kf\0\0".to_vec()),
    ]);
    [tes4(&[]), top(b"IDLE", &idles), top(b"CREA", &crea)].concat()
}

fn plugin() -> Vec<u8> {
    let idles = [
        // Override: new model, links kept, no conditions.
        idle(0, 0x101, "LeafA", Some("Characters\\_Male\\IdleAnims\\A2.kf"), vec![anam(0x100, 0), data(7, (0, 0), 0, Some(0))]),
        // New, under the master's LeafB; previous names an IDLE that does not exist.
        idle(0, 0x0100_0300, "Child", Some("Characters\\_Male\\IdleAnims\\C.kf"), vec![anam(0x102, 0x0100_0301), data(7, (0, 0), 0, None)]),
    ]
    .concat();
    [tes4(&["Test.esm"]), top(b"IDLE", &idles)].concat()
}

fn load() -> (Vec<Plugin>, Schema) {
    let plugins = vec![
        Plugin::parse(&master(), "Test.esm".into()).unwrap(),
        Plugin::parse(&plugin(), "Mod.esp".into()).unwrap(),
    ];
    (plugins, Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap())
}

#[test]
fn idles_read_with_overrides_links_and_raw_conditions() {
    let (plugins, schema) = load();
    let lo = LoadOrder::new(&plugins).unwrap();
    let t = Idles::build(&lo, &schema);
    assert!(t.issues.is_empty(), "{:?}", t.issues.to_json());
    let ids: Vec<u32> = t.idles.keys().map(|f| f.0).collect();
    assert_eq!(ids, [0x100, 0x101, 0x102, 0x0100_0300], "deleted IDLE skipped");

    let root = &t.idles[&FormId(0x100)];
    assert_eq!((root.model_kind(), root.section_name(), root.group_high_bits()), (ModelKind::Other, Some("Special Idle"), Some(0x80)));
    assert_eq!((root.parent, root.previous, root.flags), (None, None, Some(0)));
    let c = &root.conditions[0];
    assert_eq!((c.function, c.operator(), c.comparison_f32(), c.run_on, c.reference_raw), (72, Some("=="), Some(1.0), Some(0), Some(0)));

    let a = &t.idles[&FormId(0x101)];
    assert_eq!(a.model.as_deref(), Some("Characters\\_Male\\IdleAnims\\A2.kf"), "the override wins");
    assert_eq!((a.plugin, a.conditions.len(), a.loop_min, a.loop_max), (1, 0, Some(0), Some(0)));

    let b = &t.idles[&FormId(0x102)];
    assert_eq!((b.parent, b.previous), (Some(FormId(0x100)), Some(FormId(0x101))));
    assert_eq!((b.section_name(), b.group_high_bits(), b.flags, b.loop_min), (Some("Upper Body"), Some(0x40), Some(1), Some(255)));
    assert_eq!(b.conditions.len(), 2);
    let (c1, c2) = (&b.conditions[0], &b.conditions[1]);
    assert_eq!((c1.operator(), c1.or(), c1.param1_raw, c1.run_on, c1.reference_raw), (Some("<"), true, 3, Some(2), None));
    assert_eq!((c2.operator(), c2.use_global(), c2.comparison_raw, c2.run_on), (Some(">="), true, 0x300, None));
    assert_eq!(b.model_path().as_deref(), Some("meshes\\characters\\_male\\idleanims\\b.kf"));

    let child = &t.idles[&FormId(0x0100_0300)];
    assert_eq!((child.plugin, child.parent, child.previous), (1, Some(FormId(0x102)), Some(FormId(0x0100_0301))));
    assert_eq!((child.flags, child.replay_delay), (None, Some(0)), "6-byte DATA has no flags");

    assert_eq!(t.children(Some(FormId(0x100))), [FormId(0x101), FormId(0x102)]);
    assert_eq!(t.children(None), [FormId(0x100)]);
    assert_eq!(t.depth(FormId(0x0100_0300)), Some(2));
    let st = t.tree_stats();
    assert_eq!((st.idles, st.roots, st.dangling_parent, st.dangling_previous), (4, 1, 0, 1));
    assert_eq!((st.sibling_groups, st.sibling_groups_one_chain), (3, 3));
}

#[test]
fn creature_kffz_and_nifz_lists() {
    let (plugins, schema) = load();
    let lo = LoadOrder::new(&plugins).unwrap();
    let mut issues = Issues::default();
    let lists = actor_anim_lists(&lo, &schema, &mut issues);
    assert!(issues.is_empty());
    assert_eq!(lists.len(), 1);
    let d = &lists[0];
    assert_eq!((d.id, d.sig.as_str(), d.edid.as_deref()), (FormId(0x200), "CREA", Some("Dog")));
    assert_eq!(d.skeleton.as_deref(), Some("meshes\\creatures\\dog\\skeleton.nif"));
    assert_eq!(d.kffz, ["SpecialIdle_Bark.kf", "b.kf"]);
    assert_eq!(d.nifz, ["Dog.NIF"]);
}
