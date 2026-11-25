//! Independently authored CELL/LGTM records and real compiled setter calls.
//! No installed mod text, native implementation, assets or process-wide settings.
#![allow(dead_code)]
use super::lighting_build::{group, record, sub, tes4, top, u32s, z};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{game::RunCtx, trace::EventKind, Game, GameConfig, GameData};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

pub const CELL: FormId = FormId(0xA00);
pub const OTHER_CELL: FormId = FormId(0xA01);
pub const NO_TEMPLATE: FormId = FormId(0xA02);
pub const EMPTY_CELL: FormId = FormId(0xA03);
pub const A: FormId = FormId(0xB00);
pub const B: FormId = FormId(0xB01);
pub const LIGHT: FormId = FormId(0xC00);

pub struct Fixture {
    pub lo: &'static LoadOrder<'static>,
    pub schema: &'static Schema,
    pub data: GameData,
    pub forms: FormStore,
    pub refs: RefTable,
}

/// The public schema's 40-byte XCLL/LightingData shape; all values are authored.
fn lighting(ambient: [u8; 3], clip: f32) -> Vec<u8> {
    let mut bytes = Vec::new();
    for c in [ambient, [20, 30, 40], [50, 60, 70]] {
        bytes.extend([c[0], c[1], c[2], 0]);
    }
    bytes.extend(11.0_f32.to_le_bytes());
    bytes.extend(22.0_f32.to_le_bytes());
    bytes.extend(33_i32.to_le_bytes());
    bytes.extend(44_i32.to_le_bytes());
    bytes.extend(0.5_f32.to_le_bytes());
    bytes.extend(clip.to_le_bytes());
    bytes.extend(2.0_f32.to_le_bytes());
    assert_eq!(bytes.len(), 40);
    bytes
}

pub fn fixture() -> Fixture {
    let template = |id: FormId, name: &str, color, clip| record(b"LGTM", 0, id.0,
        &[sub(b"EDID", z(name)), sub(b"DATA", lighting(color, clip))]);
    let cell = |id: FormId, name: &str, clip, template: Option<FormId>| {
        let mut fields = vec![sub(b"EDID", z(name)), sub(b"DATA", vec![1]),
            sub(b"XCLL", lighting([1, 2, 3], clip)), sub(b"LNAM", u32s(&[1 << 7]))];
        if let Some(t) = template { fields.push(sub(b"LTMP", u32s(&[t.0]))); }
        record(b"CELL", 0, id.0, &fields)
    };
    let cells = [cell(CELL, "OwnedLightCell", 1000.0, Some(A)),
        cell(OTHER_CELL, "OwnedOtherCell", 2000.0, Some(A)),
        cell(NO_TEMPLATE, "OwnedNoTemplate", 3000.0, None),
        record(b"CELL", 0, EMPTY_CELL.0, &[sub(b"EDID", z("OwnedEmptyCell")), sub(b"DATA", vec![1])])].concat();
    let mut ligh_data = Vec::new();
    ligh_data.extend(0i32.to_le_bytes());
    ligh_data.extend(200u32.to_le_bytes());
    ligh_data.extend([177u8, 100, 50, 0]);
    ligh_data.extend(0u32.to_le_bytes());
    ligh_data.extend(1.0f32.to_le_bytes());
    ligh_data.extend(90.0f32.to_le_bytes());
    ligh_data.extend(0u32.to_le_bytes());
    ligh_data.extend(0.0f32.to_le_bytes());
    let ligh = record(b"LIGH", 0, LIGHT.0, &[sub(b"EDID", z("OwnedLightForm")), sub(b"DATA", ligh_data)]);
    let bytes = [tes4(), top(b"LIGH", &ligh), top(b"LGTM", &[template(A, "OwnedLightA", [7, 8, 9], 10000.0),
        template(B, "OwnedLightB", [70, 80, 90], 20000.0)].concat()),
        top(b"CELL", &group(0_i32.to_le_bytes(), 2, &group(0_i32.to_le_bytes(), 3, &cells)))].concat();
    let plugins: &'static [Plugin] = Box::leak(vec![Plugin::parse(&bytes, "OwnedLighting.esm".into()).unwrap()].into_boxed_slice());
    let lo = Box::leak(Box::new(LoadOrder::new(plugins).unwrap()));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Box::leak(Box::new(Schema::load_dir(root.join("fnv")).unwrap()));
    let path = root.join("script/functions.json");
    let catalogue: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(lo, schema);
    let refs = RefTable::build(lo, schema);
    let data = GameData::build(lo, schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    // Prove actual schema admission before any behavioral oracle.
    assert_eq!(data.sig(CELL), Some(*b"CELL"));
    assert_eq!(data.sig(B), Some(*b"LGTM"));
    assert_eq!(data.record_field(B, "DATA", "fog_clip_dist").and_then(serde_json::Value::as_f64), Some(20000.0));
    assert_eq!(fnv_world::lighting::cell_lighting(lo, schema, CELL.0).unwrap().fog_clip_dist, 10000.0);
    Fixture { lo, schema, data, forms, refs }
}

pub fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

pub fn query(g: &mut Game, body: &str) -> XVal {
    let source = format!("Begin Function {{ }}\n{body}\nEnd\n");
    let value = g.run_query(&source, RunCtx::default()).unwrap_or_else(|e| panic!("source setup: {source}: {e}"));
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::CallFailed { .. } | EventKind::ScriptError { .. })), "compiled setter/query failure is setup failure");
    value
}

pub fn set(g: &mut Game, lines: &str) {
    assert_eq!(query(g, &format!("{lines}\nSetFunctionValue 1")), XVal::Num(1.0));
}

pub fn restore<'a>(f: &'a Fixture, snapshot: &fnv_game::game::GameSnapshot) -> Game<'a> {
    let world = World::restore(&f.forms, &f.refs, &snapshot.sim).unwrap();
    Game::restore(&f.data, world, GameConfig { strict: true, ..Default::default() }, snapshot).unwrap()
}

pub const SHIFTED_CELL: FormId = FormId(0x0200_0A00);
pub const SHIFTED_TEMPLATE: FormId = FormId(0x0200_0B00);

/// The owning plugin has local index1 but global index2 after an unrelated master.
/// A cold reader must resolve its LTMP through LoadOrder, not treat the raw field
/// as a global FormID. This is a supported-input preservation control.
pub fn shifted_fixture() -> Fixture {
    let header = record(b"TES4", 0, 0, &[sub(b"HEDR", [1.34_f32.to_le_bytes(), 0_u32.to_le_bytes(), 0x800_u32.to_le_bytes()].concat()),
        sub(b"MAST", z("OwnedBase.esm")), sub(b"DATA", vec![0; 8])]);
    let cells = record(b"CELL", 0, 0x0100_0A00, &[sub(b"EDID", z("OwnedShiftedCell")), sub(b"DATA", vec![1]),
        sub(b"XCLL", lighting([1, 2, 3], 1000.0)), sub(b"LNAM", u32s(&[1 << 7])), sub(b"LTMP", u32s(&[0x0100_0B00]))]);
    let bytes = [header,
        top(b"LGTM", &record(b"LGTM", 0, 0x0100_0B00, &[sub(b"EDID", z("OwnedShiftedTemplate")), sub(b"DATA", lighting([4, 5, 6], 45000.0))])),
        top(b"CELL", &group(0_i32.to_le_bytes(), 2, &group(0_i32.to_le_bytes(), 3, &cells)))].concat();
    let plugins: &'static [Plugin] = Box::leak(vec![Plugin::parse(&tes4(), "OwnedBase.esm".into()).unwrap(),
        Plugin::parse(&tes4(), "OwnedUnused.esm".into()).unwrap(), Plugin::parse(&bytes, "OwnedShifted.esp".into()).unwrap()].into_boxed_slice());
    let lo = Box::leak(Box::new(LoadOrder::new(plugins).unwrap()));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Box::leak(Box::new(Schema::load_dir(root.join("fnv")).unwrap()));
    let path = root.join("script/functions.json");
    let catalogue: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(lo, schema); let refs = RefTable::build(lo, schema);
    let data = GameData::build(lo, schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    assert_eq!(data.sig(SHIFTED_TEMPLATE), Some(*b"LGTM"));
    assert_eq!(lo.to_global(2, 0x0100_0B00), Some(SHIFTED_TEMPLATE.0));
    assert_eq!(fnv_world::lighting::cell_lighting(lo, schema, SHIFTED_CELL.0).unwrap().fog_clip_dist, 45000.0);
    Fixture { lo, schema, data, forms, refs }
}
