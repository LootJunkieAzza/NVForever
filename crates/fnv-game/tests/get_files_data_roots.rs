//! Authored public file queries in owned temporary directories, no installed data.
//! A dedicated target because Roots is process-wide; each case is also serialized.
mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{GameData, game::{Game, GameConfig}, mod_files::{self, Roots}};
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use std::{collections::BTreeMap, path::{Path, PathBuf}, sync::{Mutex, MutexGuard}, time::{SystemTime, UNIX_EPOCH}};

static ROOTS_LOCK: Mutex<()> = Mutex::new(());

struct Files {
    _lock: MutexGuard<'static, ()>,
    root: PathBuf,
    temp: PathBuf,
    previous: Option<Roots>,
    inputs: BTreeMap<PathBuf, Vec<u8>>,
}

impl Files {
    fn new() -> Self {
        let lock = ROOTS_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = std::env::temp_dir().canonicalize().unwrap();
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = temp.join(format!("nvf-owned-getfiles-{}-{nonce}", std::process::id()));
        // Exclusive creation establishes cleanup ownership, even if setup later fails.
        std::fs::create_dir(&root).unwrap();
        let mut out = Self { _lock: lock, root, temp, previous: None, inputs: BTreeMap::new() };
        for dir in ["game/Data/Allowed/Child", "game/Data/Allowed/Nested", "game/Data/Allowed/.settings", "game/Allowed/ParentFolder", "game/Outside/OutsideFolder", "layer/Allowed"] {
            std::fs::create_dir_all(out.root.join(dir)).unwrap();
        }
        for (path, body) in [
            ("game/Data/root.ini", "17\t29\n"),
            ("game/Data/Allowed/valid.ini", "17\t29\n"),
            ("game/Data/Allowed/extensionless_valid", "$Owned\t$Value\n"),
            ("game/Data/Allowed/__folder_managed_by_vortex", "$Owned metadata\n"),
            ("game/Data/Allowed/shared.ini", "31\t43\n"),
            ("game/Data/Allowed/Nested/deep.ini", "37\t41\n"),
            ("game/Data/Allowed/.settings/inside.ini", "43\t47\n"),
            ("game/Allowed/parent_only.ini", "47\t59\n"),
            ("game/Outside/outside.ini", "61\t73\n"),
            ("layer/Allowed/SHARED.INI", "79\t83\n"),
            ("layer/Allowed/layer.ini", "89\t97\n"),
        ] {
            let path = out.root.join(path);
            std::fs::write(&path, body).unwrap();
            out.inputs.insert(path, body.as_bytes().to_vec());
        }
        let read = vec![out.root.join("layer"), out.root.join("game/Data")];
        mod_files::update_roots(|roots| out.previous = Some(std::mem::replace(roots, Roots { read, ..Default::default() })));
        out
    }

    fn unchanged(&self) {
        for (path, expected) in &self.inputs {
            assert_eq!(std::fs::read(path).unwrap(), *expected, "{}", path.display());
        }
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            mod_files::set_roots(previous);
        }
        if self.root.parent() == Some(self.temp.as_path())
            && self.root.file_name().is_some_and(|n| n.to_string_lossy().starts_with("nvf-owned-getfiles-")) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn with_game(f: impl FnOnce(&mut Game<'_>)) {
    let bytes = tes4();
    let plugins = [Plugin::parse(&bytes, "OwnedFileQuery.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let schemas = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(schemas.join("fnv")).unwrap();
    let functions = schemas.join("script/functions.json");
    let catalogue = serde_json::from_slice(&std::fs::read(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(&order, &schema, FunctionTable::load(&functions).unwrap(), &catalogue, &refs);
    let mut game = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    f(&mut game);
}

fn query(game: &mut Game<'_>, call: &str, result: &str) -> XVal {
    let source = format!("array_var a\nBegin Function {{}}\nlet a := {call}\nSetFunctionValue ({result})\nEnd\n");
    game.run_query(&source, Default::default()).unwrap_or_else(|error| panic!("{source}: {error}"))
}

#[test]
fn files_only_come_from_data_roots_not_parent_namesakes() {
    let files = Files::new();
    with_game(|game| {
        assert_eq!(query(game, "GetFiles \"Allowed\"", "(ar_Find \"valid.ini\" a) >= 0"), XVal::Num(1.0));
        assert_eq!(query(game, "GetFiles \"Allowed\"", "(ar_Find \"parent_only.ini\" a) >= 0"), XVal::Num(0.0));
        assert_eq!(query(game, "GetFilesInFolder \"Allowed\"", "ar_Size a"), XVal::Num(5.0));
        // This separate API is documented game-relative; its parent lookup survives.
        assert_eq!(query(game, "ReadArrayFromFile \"Allowed/parent_only.ini\"", "a[0] * 100 + a[1]"), XVal::Num(4759.0));
    });
    files.unchanged();
}

#[test]
fn folders_only_come_from_data_roots_not_parent_namesakes() {
    let files = Files::new();
    with_game(|game| {
        assert_eq!(query(game, "GetFolders \"Allowed\"", "(ar_Find \"Child\" a) >= 0"), XVal::Num(1.0));
        assert_eq!(query(game, "GetFolders \"Allowed\"", "(ar_Find \"ParentFolder\" a) >= 0"), XVal::Num(0.0));
        assert_eq!(query(game, "GetFoldersInFolder \"Allowed\" \"Ch*\"", "ar_Size a"), XVal::Num(1.0));
    });
    files.unchanged();
}

#[test]
fn enumeration_refuses_paths_outside_the_lexical_data_root() {
    let files = Files::new();
    // Keep every old parent-fallback join inside this owned fixture too. Never
    // enumerate a drive root while comparing the unfixed implementation.
    mod_files::update_roots(|roots| roots.read = vec![files.root.join("game/Data")]);
    with_game(|game| {
        assert_eq!(query(game, "GetFiles \"Allowed\"", "(ar_Find \"valid.ini\" a) >= 0"), XVal::Num(1.0));
        let absolute = files.root.join("game/Outside").to_string_lossy().replace('\\', "/");
        for path in ["../Outside", "..\\Outside", "Allowed/../../Outside", absolute.as_str()] {
            for command in ["GetFiles", "GetFolders"] {
                assert_eq!(query(game, &format!("{command} \"{path}\""), "ar_Size a"), XVal::Num(0.0), "{command} {path}");
            }
        }
    });
    files.unchanged();
}

#[test]
fn ordinary_filters_layers_root_and_marker_neighbors_remain_visible() {
    let files = Files::new();
    with_game(|game| {
        let all = "GetFiles \"aLLoWeD\"";
        assert_eq!(query(game, all, "ar_Size a"), XVal::Num(5.0));
        for name in ["__folder_managed_by_vortex", "extensionless_valid", "layer.ini", "SHARED.INI", "valid.ini"] {
            assert_eq!(query(game, all, &format!("(ar_Find \"{name}\" a) >= 0")), XVal::Num(1.0), "{name}");
        }
        assert_eq!(query(game, all, "a[0]"), XVal::Str("__folder_managed_by_vortex".into()));
        assert_eq!(query(game, all, "a[3]"), XVal::Str("SHARED.INI".into()));
        assert_eq!(query(game, "GetFiles \"Allowed\" \"*.ini\"", "ar_Size a"), XVal::Num(3.0));
        assert_eq!(query(game, "GetFiles \"Allowed\" \"__folder_managed_by_vortex\"", "ar_Size a"), XVal::Num(1.0));
        assert_eq!(query(game, "GetFiles \"aLLoWeD/nEsTeD\"", "a[0]"), XVal::Str("deep.ini".into()));
        assert_eq!(query(game, "GetFiles \"Allowed/.settings\"", "a[0]"), XVal::Str("inside.ini".into()));
        assert_eq!(query(game, "GetFiles \"\"", "ar_Size a"), XVal::Num(1.0));
        assert_eq!(query(game, "GetFiles \"\"", "a[0]"), XVal::Str("root.ini".into()));
        assert_eq!(query(game, "GetFolders \"\"", "ar_Size a"), XVal::Num(1.0));
        assert_eq!(query(game, "GetFiles \"Absent\"", "ar_Size a"), XVal::Num(0.0));
    });
    files.unchanged();
}
