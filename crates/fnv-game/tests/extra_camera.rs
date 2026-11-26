//! Clean-room unit and lifecycle tests for JIP LN `AttachExtraCamera` and
//! `ProjectExtraCamera` (`crates/fnv-game/src/nvse_jip14.rs`).

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::nvse_jip14::{
    ExtraCameraTarget, EXTRA_CAMERA_DEFAULT_PX, EXTRA_CAMERA_MAX, EXTRA_CAMERA_MAX_FOV_DEG,
    EXTRA_CAMERA_MAX_PX, EXTRA_CAMERA_MIN_FOV_DEG, EXTRA_CAMERA_MIN_PX,
};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use std::path::PathBuf;
use std::sync::Arc;

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

fn tes4() -> Vec<u8> {
    let hedr = cat(&[&1.34f32.to_le_bytes(), &0i32.to_le_bytes(), &0x800u32.to_le_bytes()]);
    record(b"TES4", 1, 0, &[sub(b"HEDR", hedr)])
}

fn refr(fid: u32, edid: &str, base: u32) -> Vec<u8> {
    let data: Vec<u8> = [0.0f32; 6].iter().flat_map(|f| f.to_le_bytes()).collect();
    record(b"REFR", 0x400, fid, &[sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

const STAT_BASE: u32 = 0x1001;
const CELL_A: u32 = 0x2001;
const CELL_B: u32 = 0x2002;
const REF_A: u32 = 0x3001;
const REF_B: u32 = 0x3002;

struct Fixture {
    forms: FormStore,
    refs: RefTable,
    data: GameData,
}

impl Fixture {
    fn build() -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv");
        let schema = Schema::load_dir(&dir).expect("load schemas");
        let stat = record(b"STAT", 0, STAT_BASE, &[sub(b"EDID", z("SynStat"))]);
        let cell_a = record(b"CELL", 0, CELL_A, &[sub(b"EDID", z("SynCellA")), sub(b"DATA", vec![1])]);
        let cell_b = record(b"CELL", 0, CELL_B, &[sub(b"EDID", z("SynCellB")), sub(b"DATA", vec![1])]);
        let children_a = group(CELL_A.to_le_bytes(), 6, &group(CELL_A.to_le_bytes(), 8, &refr(REF_A, "CamRefA", STAT_BASE)));
        let children_b = group(CELL_B.to_le_bytes(), 6, &group(CELL_B.to_le_bytes(), 8, &refr(REF_B, "CamRefB", STAT_BASE)));
        let cells = top(
            b"CELL",
            &group(
                0i32.to_le_bytes(),
                2,
                &group(0i32.to_le_bytes(), 3, &cat(&[&cell_a, &children_a, &cell_b, &children_b])),
            ),
        );
        let bytes = cat(&[&tes4(), &top(b"STAT", &stat), &cells]);
        let plugins = [Plugin::parse(&bytes, "SynExtraCam.esm".into()).expect("parse plugin")];
        let lo = LoadOrder::new(&plugins).expect("load order");
        let fn_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
        let fn_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&fn_path).unwrap()).unwrap();
        let table = FunctionTable::load(&fn_path).expect("load functions.json");
        let forms = FormStore::build(&lo, &schema);
        let refs = RefTable::build(&lo, &schema);
        let data = GameData::build(&lo, &schema, table, &fn_json, &refs);
        Self { forms, refs, data }
    }

    fn game(&self) -> Game<'_> {
        let world = World::new(&self.forms, &self.refs, SimConfig::default());
        let mut g = Game::new(&self.data, world, GameConfig::default());
        g.player_entered(Some(FormId(CELL_A)));
        g
    }
}

fn eval_num(g: &mut Game<'_>, expr: &str) -> f64 {
    let src = format!("Begin Function {{ }}\nSetFunctionValue ({expr})\nEnd\n");
    match g.run_query(&src, RunCtx::default()).unwrap() {
        XVal::Num(x) => x,
        XVal::Ref(r) => f64::from(r),
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn attach_project_reattach_detach_and_missing_camera_refusal() {
    let fx = Fixture::build();
    let mut g = fx.game();

    // Projecting before attach returns 0 (real-world lazy-attach idiom).
    assert_eq!(eval_num(&mut g, "Player.ProjectExtraCamera \"ScopeCam\" \"LensMesh\" 12 512"), 0.0);
    assert!(g.extra_cameras().is_empty());

    // Empty name or missing reference on attach returns 0.
    assert_eq!(eval_num(&mut g, "Player.AttachExtraCamera \"\" 1 \"AnchorNode\""), 0.0);
    assert_eq!(eval_num(&mut g, "AttachExtraCamera \"NoRef\" 1 \"AnchorNode\""), 0.0);

    // Attach succeeds and stores typed state.
    assert_eq!(eval_num(&mut g, "Player.AttachExtraCamera \"ScopeCam\" 1 \"AnchorNode\""), 1.0);
    let cam = g.extra_cameras().get("scopecam").expect("attached");
    assert_eq!(cam.name, "ScopeCam");
    assert_eq!(cam.reference, FormId::PLAYER_REF);
    assert_eq!(cam.parent_node.as_deref(), Some("AnchorNode"));
    assert!(cam.projection.is_none());

    // Case-insensitive camera lookup + default pixel size (256) when omitted.
    assert_eq!(eval_num(&mut g, "Player.ProjectExtraCamera \"scopecam\" \"LensMesh\" 8.5"), 1.0);
    let proj = g.extra_cameras().get("scopecam").unwrap().projection.as_ref().unwrap();
    assert_eq!(proj.target, ExtraCameraTarget::Geometry { reference: FormId::PLAYER_REF, node: "LensMesh".into() });
    assert!((proj.fov_deg - 8.5).abs() < 1e-5);
    assert_eq!(proj.pixel_size, EXTRA_CAMERA_DEFAULT_PX);
    assert!(!proj.grayscale);

    // Re-attach moves anchor node cleanly without duplicating the entry.
    assert_eq!(eval_num(&mut g, "CamRefA.AttachExtraCamera \"ScopeCam\" 1 \"AltAnchor\""), 1.0);
    assert_eq!(g.extra_cameras().len(), 1);
    let cam = g.extra_cameras().get("scopecam").unwrap();
    assert_eq!(cam.reference, FormId(REF_A));
    assert_eq!(cam.parent_node.as_deref(), Some("AltAnchor"));

    // UI tile target works without a calling reference.
    assert_eq!(eval_num(&mut g, "ProjectExtraCamera \"ScopeCam\" \"*HUDMainMenu/PipTile\" 20 512 1"), 1.0);
    let proj = g.extra_cameras().get("scopecam").unwrap().projection.as_ref().unwrap();
    assert_eq!(proj.target, ExtraCameraTarget::UiTile { path: "HUDMainMenu/PipTile".into() });
    assert_eq!(proj.pixel_size, 512);
    assert!(proj.grayscale);

    // Detach removes camera; second detach and subsequent projection return 0.
    assert_eq!(eval_num(&mut g, "Player.AttachExtraCamera \"ScopeCam\" 0"), 1.0);
    assert!(g.extra_cameras().is_empty());
    assert_eq!(eval_num(&mut g, "Player.AttachExtraCamera \"ScopeCam\" 0"), 0.0);
    assert_eq!(eval_num(&mut g, "Player.ProjectExtraCamera \"ScopeCam\" \"LensMesh\" 8.5"), 0.0);
}

#[test]
fn clamping_caps_cross_interior_refusal_and_restore_cleanup() {
    let fx = Fixture::build();
    let mut g = fx.game();

    // Clamp extreme FoV and pixelSize; reject non-positive FoV and empty target.
    assert_eq!(eval_num(&mut g, "Player.AttachExtraCamera \"ClampCam\" 1"), 1.0);
    assert_eq!(eval_num(&mut g, "Player.ProjectExtraCamera \"ClampCam\" \"Lens\" 0 512"), 0.0);
    assert_eq!(eval_num(&mut g, "Player.ProjectExtraCamera \"ClampCam\" \"\" 10 512"), 0.0);
    assert_eq!(eval_num(&mut g, "Player.ProjectExtraCamera \"ClampCam\" \"*\" 10 512"), 0.0);

    assert_eq!(eval_num(&mut g, "Player.ProjectExtraCamera \"ClampCam\" \"Lens\" 0.01 4"), 1.0);
    let proj = g.extra_cameras().get("clampcam").unwrap().projection.as_ref().unwrap();
    assert_eq!(proj.fov_deg, EXTRA_CAMERA_MIN_FOV_DEG);
    assert_eq!(proj.pixel_size, EXTRA_CAMERA_MIN_PX);

    assert_eq!(eval_num(&mut g, "Player.ProjectExtraCamera \"ClampCam\" \"Lens\" 999 8192"), 1.0);
    let proj = g.extra_cameras().get("clampcam").unwrap().projection.as_ref().unwrap();
    assert_eq!(proj.fov_deg, EXTRA_CAMERA_MAX_FOV_DEG);
    assert_eq!(proj.pixel_size, EXTRA_CAMERA_MAX_PX);

    // Camera attached in another interior cell cannot project while player is in CELL_A.
    assert_eq!(eval_num(&mut g, "CamRefB.AttachExtraCamera \"OtherCellCam\" 1 \"Root\""), 1.0);
    assert_eq!(eval_num(&mut g, "Player.ProjectExtraCamera \"OtherCellCam\" \"Lens\" 15 256"), 0.0);

    // Bounded camera cap: fill up to EXTRA_CAMERA_MAX, next distinct name returns 0.
    g.clear_extra_cameras();
    for i in 0..EXTRA_CAMERA_MAX {
        let call = format!("Player.AttachExtraCamera \"Cam{i}\" 1 \"Node{i}\"");
        assert_eq!(eval_num(&mut g, &call), 1.0, "slot {i}");
    }
    assert_eq!(g.extra_cameras().len(), EXTRA_CAMERA_MAX);
    assert_eq!(eval_num(&mut g, "Player.AttachExtraCamera \"OverflowCam\" 1 \"NodeX\""), 0.0);
    assert_eq!(g.extra_cameras().len(), EXTRA_CAMERA_MAX);
    // Re-attaching an existing camera name still succeeds at the cap.
    assert_eq!(eval_num(&mut g, "Player.AttachExtraCamera \"Cam0\" 1 \"MovedNode\""), 1.0);

    // Save/restore starts a fresh session token with an empty extra-camera table.
    let old_token = g.extra_camera_session();
    let snap = g.snapshot();
    let restored_world = World::new(&fx.forms, &fx.refs, SimConfig::default());
    let restored = Game::restore(&fx.data, restored_world, GameConfig::default(), &snap).expect("restore");
    assert!(restored.extra_cameras().is_empty());
    assert!(!Arc::ptr_eq(&old_token, &restored.extra_camera_session()));
}
