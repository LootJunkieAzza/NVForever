//! Synthetic NIF files (no game data) decoded with the repository schemas.

use fnv_nif::scene::{mesh_data, scene_objects, shape_summary, Decoded};
use fnv_nif::{NifFile, Schema};
use std::path::Path;

fn schema() -> Schema {
    Schema::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/nif")).unwrap()
}

#[derive(Default)]
struct W(Vec<u8>);
impl W {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn f(&mut self, v: &[f32]) -> &mut Self {
        for x in v {
            self.0.extend_from_slice(&x.to_le_bytes());
        }
        self
    }
    fn sized(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
        self
    }
    fn export(&mut self, s: &str) -> &mut Self {
        self.u8(s.len() as u8 + 1);
        self.0.extend_from_slice(s.as_bytes());
        self.u8(0)
    }
}

const IDENTITY: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// Named-object + scene-object fields for 20.2.0.7 / bs 34.
fn av_object(w: &mut W, name_idx: u32, translation: [f32; 3]) {
    w.u32(name_idx).u32(0).i32(-1); // name, no extra data, no controller
    w.u32(14).f(&translation).f(&IDENTITY).f(&[1.0]); // flags (u32 since bs > 26), transform
    w.u32(0).i32(-1); // no properties, no collision object
}

fn node(child: i32) -> Vec<u8> {
    let mut w = W::default();
    av_object(&mut w, 0, [0.0, 0.0, 0.0]);
    w.u32(1).i32(child).u32(0);
    w.0
}

fn tri_strips(data: i32) -> Vec<u8> {
    let mut w = W::default();
    av_object(&mut w, 1, [1.0, 2.0, 3.0]);
    w.i32(data).i32(-1); // data, no skin
    w.u32(0).i32(-1).u8(0); // material data: none, active -1, needs update false
    w.0
}

fn strips_data() -> Vec<u8> {
    let mut w = W::default();
    w.i32(0).u16(4).u8(0).u8(0).u8(1);
    w.f(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, -2.0]);
    w.u16(1); // bs data flags: one UV set, no tangents
    w.u8(0); // no normals
    w.f(&[0.5, 0.5, 0.0, 1.0]); // bounding sphere
    w.u8(0); // no vertex colours
    w.f(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0]); // one UV set
    w.u16(0).i32(-1); // consistency, no additional data
    w.u16(2); // triangles
    w.u16(1).u16(4).u8(1).u16(0).u16(1).u16(2).u16(3); // one strip of four points
    w.0
}

fn build(blocks: &[(&str, Vec<u8>)], strings: &[&str], roots: &[i32]) -> Vec<u8> {
    let mut types: Vec<&str> = Vec::new();
    for (t, _) in blocks {
        if !types.contains(t) {
            types.push(t);
        }
    }
    let mut w = W::default();
    w.0.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    w.u32(0x1402_0007).u8(1).u32(11).u32(blocks.len() as u32);
    w.u32(34).export("tester").export("Do Nothing").export(" PE Static Art");
    w.u16(types.len() as u16);
    for t in &types {
        w.sized(t);
    }
    for (t, _) in blocks {
        w.u16(types.iter().position(|x| x == t).unwrap() as u16);
    }
    for (_, b) in blocks {
        w.u32(b.len() as u32);
    }
    w.u32(strings.len() as u32).u32(strings.iter().map(|s| s.len()).max().unwrap_or(0) as u32);
    for s in strings {
        w.sized(s);
    }
    w.u32(0);
    for (_, b) in blocks {
        w.0.extend_from_slice(b);
    }
    w.u32(roots.len() as u32);
    for r in roots {
        w.i32(*r);
    }
    w.0
}

fn sample() -> Vec<u8> {
    build(
        &[("BSFadeNode", node(1)), ("NiTriStrips", tri_strips(2)), ("NiTriStripsData", strips_data())],
        &["Root", "Shape"],
        &[0],
    )
}

#[test]
fn container_round_trip_and_exact_decode() {
    let s = schema();
    let data = sample();
    let f = NifFile::parse(&data, Some(&s)).unwrap();
    assert_eq!(f.header.bs.as_ref().unwrap().author_text(), "tester");
    assert_eq!(f.blocks.len(), 3);
    assert_eq!(f.rewrite(&data), data);
    for i in 0..3 {
        assert!(f.decode_block(&s, &data, i).is_ok(), "block {i}");
        assert_eq!(f.reencode_block(&s, &data, i), Ok(true), "block {i}");
    }
}

#[test]
fn scene_graph_and_geometry() {
    let s = schema();
    let data = sample();
    let f = NifFile::parse(&data, Some(&s)).unwrap();
    let dec = Decoded::new(&f, &data, &s);
    let objs = scene_objects(&f, &dec);
    assert_eq!(objs.len(), 2);
    assert_eq!(objs[0].name.as_deref(), Some("Root"));
    assert_eq!(objs[0].parent, None);
    assert_eq!(objs[1].parent, Some(0));
    assert_eq!(objs[1].transform.translation, [1.0, 2.0, 3.0]);
    let data_block = dec.blocks[2].as_ref().unwrap();
    let sm = shape_summary(2, "NiTriStripsData", data_block);
    assert_eq!((sm.vertices, sm.triangles, sm.uv_sets, sm.has_normals), (4, 2, 1, false));
    assert_eq!(sm.aabb, Some(([0.0, 0.0, -2.0], [1.0, 1.0, 0.0])));
    let mesh = mesh_data(data_block);
    // Strip 0,1,2,3 → (0,1,2) then (1,3,2) with the winding flipped.
    assert_eq!(mesh.triangles, [[0, 1, 2], [1, 3, 2]]);
    assert_eq!(mesh.uv_sets[0][3], [1.0, 1.0]);
}

#[test]
fn unknown_and_short_blocks_are_reported_not_fatal() {
    let s = schema();
    let mut padded = strips_data();
    padded.extend_from_slice(&[0, 0]);
    let data = build(
        &[("BSFadeNode", node(1)), ("NiMadeUpBlock", vec![1, 2, 3]), ("NiTriStripsData", padded)],
        &["Root"],
        &[0],
    );
    let f = NifFile::parse(&data, Some(&s)).unwrap();
    assert_eq!(f.rewrite(&data), data);
    assert!(f.decode_block(&s, &data, 0).is_ok());
    assert_eq!(f.decode_block(&s, &data, 1).unwrap_err().code(), "unknown");
    assert_eq!(f.decode_block(&s, &data, 2).unwrap_err().code(), "short");
}

#[test]
fn truncation_is_an_error() {
    let s = schema();
    let data = sample();
    for cut in [10, 60, data.len() - 5] {
        assert!(NifFile::parse(&data[..cut], Some(&s)).is_err(), "cut at {cut}");
    }
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(300))]
    #[test]
    fn corrupted_files_never_panic(pos in 0usize..400, byte in proptest::prelude::any::<u8>()) {
        let s = schema();
        let mut data = sample();
        let p = pos % data.len();
        data[p] = byte;
        if let Ok(f) = NifFile::parse(&data, Some(&s)) {
            let dec = Decoded::new(&f, &data, &s);
            let _ = scene_objects(&f, &dec);
            for b in dec.blocks.iter().flatten() {
                let _ = mesh_data(b);
            }
        }
    }
}
