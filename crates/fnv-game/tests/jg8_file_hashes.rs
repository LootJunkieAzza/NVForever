//! Standalone `MD5File` and `SHA1File` loose-file integration tests.
//! Dedicated test binary because `mod_files::ROOTS` is process-wide; each test
//! serializes access and restores the previous roots via an RAII guard.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::mod_files::{self, Reader, Roots};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

static ROOTS_LOCK: Mutex<()> = Mutex::new(());

struct TempHashRoots {
    _lock: MutexGuard<'static, ()>,
    previous: Option<Roots>,
    root: PathBuf,
    temp: PathBuf,
}

impl TempHashRoots {
    fn new(reader: Option<Reader>) -> Self {
        let lock = ROOTS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = std::env::temp_dir().canonicalize().unwrap();
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = temp.join(format!("nvf-jg8-hash-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let mut out = Self { _lock: lock, previous: None, root, temp };
        let read = vec![out.root.clone()];
        mod_files::update_roots(|roots| {
            out.previous = Some(std::mem::replace(roots, Roots { read, reader, ..Default::default() }));
        });
        out
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.root.join(name), bytes).unwrap();
    }
}

impl Drop for TempHashRoots {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            mod_files::set_roots(previous);
        }
        if self.root.parent() == Some(self.temp.as_path())
            && self.root.file_name().is_some_and(|n| n.to_string_lossy().starts_with("nvf-jg8-hash-"))
        {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn with_game(f: impl FnOnce(&mut Game<'_>)) {
    let npc = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let bytes = [tes4(), top(b"NPC_", &npc)].concat();
    let plugins = [Plugin::parse(&bytes, "Jg8Hashes.esm".into()).unwrap()];
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

fn eval_str(g: &mut Game<'_>, body: &str) -> String {
    let source = format!("Begin Function {{ }}\n{body}\nEnd\n");
    match g.run_query(&source, RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}")) {
        XVal::Str(s) => s,
        v => panic!("{body}: {v:?}"),
    }
}

#[test]
fn md5_and_sha1_hash_loose_files_only_not_archives() {
    // MD5File / SHA1File hash loose files on disk only, never falling back to BSA archives
    // [observed: w6d3p2_16_md5_sha1_bsa_and_sleepwait_state].
    let reader: Reader = Arc::new(|p: &str| {
        if p.eq_ignore_ascii_case("menus\\falloutdict.txt") {
            Some(b"archive-only".to_vec())
        } else {
            None
        }
    });
    let roots = TempHashRoots::new(Some(reader));
    roots.write("loose.txt", b"abc");

    with_game(|g| {
        assert_eq!(eval_str(g, r#"SetFunctionValue (MD5File "Data\loose.txt")"#), "900150983CD24FB0D6963F7D28E17F72");
        assert_eq!(eval_str(g, r#"SetFunctionValue (SHA1File "Data\loose.txt")"#), "A9993E364706816ABA3E25717850C26C9CD0D89D");
        assert_eq!(eval_str(g, r#"SetFunctionValue (MD5File "Data\menus\falloutdict.txt")"#), "");
        assert_eq!(eval_str(g, r#"SetFunctionValue (SHA1File "Data\menus\falloutdict.txt")"#), "");
        assert_eq!(eval_str(g, r#"SetFunctionValue (MD5File "Data\W6D3_NoSuch.txt")"#), "");
        assert_eq!(eval_str(g, r#"SetFunctionValue (SHA1File "Data\W6D3_NoSuch.txt")"#), "");
    });
}
