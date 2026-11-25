//! Standalone `GetTextureFormat` DDS header integration tests.
//! Dedicated test binary because `mod_files::ROOTS` is process-wide; each test
//! serializes access and restores the previous roots via an RAII guard.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::mod_files::{self, Roots};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

static ROOTS_LOCK: Mutex<()> = Mutex::new(());

struct TempDataRoots {
    _lock: MutexGuard<'static, ()>,
    previous: Option<Roots>,
    root: PathBuf,
    temp: PathBuf,
}

impl TempDataRoots {
    fn new() -> Self {
        let lock = ROOTS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = std::env::temp_dir().canonicalize().unwrap();
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = temp.join(format!("nvf-jg8-texfmt-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let mut out = Self { _lock: lock, previous: None, root, temp };
        std::fs::create_dir_all(out.root.join("textures")).unwrap();
        let read = vec![out.root.clone()];
        mod_files::update_roots(|roots| {
            out.previous = Some(std::mem::replace(roots, Roots { read, ..Default::default() }));
        });
        out
    }

    fn write_texture(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.root.join("textures").join(name), bytes).unwrap();
    }
}

impl Drop for TempDataRoots {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            mod_files::set_roots(previous);
        }
        if self.root.parent() == Some(self.temp.as_path())
            && self.root.file_name().is_some_and(|n| n.to_string_lossy().starts_with("nvf-jg8-texfmt-"))
        {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn with_game(f: impl FnOnce(&mut Game<'_>)) {
    let npc = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let bytes = [tes4(), top(b"NPC_", &npc)].concat();
    let plugins = [Plugin::parse(&bytes, "Jg8TexFmt.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schemas = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(schemas.join("fnv")).unwrap();
    let funcs = schemas.join("script/functions.json");
    let json: Json = serde_json::from_str(&std::fs::read_to_string(&funcs).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&funcs).unwrap(), &json, &refs);
    let mut g = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    f(&mut g);
}

fn eval_num(g: &mut Game<'_>, expr: &str) -> f64 {
    let source = format!("Begin Function {{ }}\nSetFunctionValue ({expr})\nEnd\n");
    match g.run_query(&source, RunCtx::default()).unwrap_or_else(|e| panic!("{expr}: {e}")) {
        XVal::Num(x) => x,
        v => panic!("{expr}: {v:?}"),
    }
}

#[test]
fn get_texture_format_returns_neg_48_for_zero_fourcc_and_checks_prefix() {
    let roots = TempDataRoots::new();

    let mut dxt5 = vec![0u8; 128];
    dxt5[..4].copy_from_slice(b"DDS ");
    dxt5[12..16].copy_from_slice(&256u32.to_le_bytes());
    dxt5[16..20].copy_from_slice(&512u32.to_le_bytes());
    dxt5[28..32].copy_from_slice(&10u32.to_le_bytes());
    dxt5[84..88].copy_from_slice(b"DXT5");

    let mut uncomp = vec![0u8; 128];
    uncomp[..4].copy_from_slice(b"DDS ");
    uncomp[12..16].copy_from_slice(&8u32.to_le_bytes());
    uncomp[16..20].copy_from_slice(&2u32.to_le_bytes());

    roots.write_texture("dxt5.dds", &dxt5);
    roots.write_texture("uncomp.dds", &uncomp);

    with_game(|g| {
        // [observed: w6d3_12_texture_format_dxt1_dxt3_dxt5_uncomp; other zero-FourCC uncompressed DDS hypothesis]
        assert_eq!(eval_num(g, r#"GetTextureFormat "uncomp.dds""#), -48.0);
        assert_eq!(eval_num(g, r#"GetTextureFormat "textures\dxt5.dds""#), 5.0);
        assert_eq!(eval_num(g, r#"GetTextureFormat "textures/dxt5.dds""#), 0.0);
        assert_eq!(eval_num(g, r#"GetTextureFormat "missing.dds""#), 0.0);
    });
}
