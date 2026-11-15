//! Mod scripts the engine runs from their source text compile (`fnv_game::source_compile`):
//! the three that failed in the laptop's new-mod smoke runs (2026-11-15, ship19):
//! - MoreMojave.esm SCPT 0x..006D04 (MoreMojaveExplodingBoatFlySCRIPT): a condition
//!   broken over lines inside its parentheses ("line 19: syntax missing )");
//! - DryWellsReloaded.esm INFO 0x..004955, end script: `if getquestfailed Q 0 &&
//!   getobjectivedisplayed Q 10 1`, values beside the calls ("syntax unexpected 0 && ...");
//! - NovaArizona.esm SCPT 0x..008E6A (AZWhiteHillsMineDialogueScript):
//!   `getobjectivecompleted Q ( 7 && 35 ) == 1`, an expression argument ("2 values left on
//!   the stack" at run time).
//!
//! And three Nova Arizona units whose source names forms the mod renamed after compiling
//! (`fnv_script::compile` matches the old name to the reference list's form): SCPT
//! 0x..00CE4A (AZWPNTommyTroutCritical), QUST 0x..008E4A (AZLegionWhiteHillsMineFaction,
//! stage 10), INFO 0x..021079 (AZFreeformChloride01).
//!
//! Needs the game data and the mods, never in the repository: `FNVLAB_DATA_DIR` (the
//! game's Data folder with the ten masters) and `NVF_TEST_MODS_DIR` (the MO2 mods folder
//! holding "Dry Wells - Legion Expansion", "MoreMojave" and "Nova Arizona"). Without
//! them the test prints why it was skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::RefTable;
use serde_json::Value as Json;
use std::path::{Path, PathBuf};

const MASTERS: [&str; 10] = [
    "FalloutNV.esm",
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
    "CaravanPack.esm",
];

/// The mod plugins, by folder and file.
const MODS: [(&str, &str); 3] = [("Dry Wells - Legion Expansion", "DryWellsReloaded.esm"), ("MoreMojave", "MoreMojave.esm"), ("Nova Arizona", "NovaArizona.esm")];

/// The units that must compile, by plugin and label text.
const UNITS: [(&str, &str); 6] = [
    ("DryWellsReloaded.esm", "004955 end"),
    ("MoreMojave.esm", "006D04"),
    ("NovaArizona.esm", "008E6A"),
    ("NovaArizona.esm", "00CE4A"),
    ("NovaArizona.esm", "008E4A"),
    ("NovaArizona.esm", "021079"),
];

fn plugins() -> Option<Vec<PathBuf>> {
    let Some(dir) = std::env::var_os("FNVLAB_DATA_DIR").map(PathBuf::from) else {
        eprintln!("skipped: FNVLAB_DATA_DIR is not set (no game data)");
        return None;
    };
    let Some(mods) = std::env::var_os("NVF_TEST_MODS_DIR").map(PathBuf::from) else {
        eprintln!("skipped: NVF_TEST_MODS_DIR is not set (no mods folder)");
        return None;
    };
    let mut out: Vec<PathBuf> = MASTERS.iter().map(|m| dir.join(m)).collect();
    out.extend(MODS.iter().map(|(folder, file)| mods.join(folder).join(file)));
    if let Some(missing) = out.iter().find(|p| !p.is_file()) {
        eprintln!("skipped: {} is missing", missing.display());
        return None;
    }
    Some(out)
}

#[test]
fn the_smoke_runs_failing_mod_scripts_compile_from_source() {
    let Some(paths) = plugins() else { return };
    let plugins: Vec<Plugin> = paths.iter().map(|p| Plugin::open(p).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_dir(root.join("schemas/fnv")).unwrap();
    let fp = root.join("schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&fp).unwrap()).unwrap();
    let table = FunctionTable::load(&fp).unwrap();
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    eprintln!("sources that do not compile: {:?}", data.issues.samples("script_source_does_not_compile"));
    for (file, label) in UNITS {
        let units: Vec<_> = data.units.iter().filter(|c| c.unit.plugin.eq_ignore_ascii_case(file) && c.unit.owner.label().contains(label)).collect();
        assert!(!units.is_empty(), "{file} {label}: no such unit");
        for c in units {
            let key = c.unit.key();
            let unknown = fnv_game::source_compile::calls_unknown(&c.program, &data.table);
            assert!(c.raw == 0 && !unknown, "{key}: kept its bytes (raw {}, unknown opcodes {unknown})", c.raw);
            assert!(!data.issues.samples("script_source_does_not_compile").iter().any(|s| s.starts_with(&key)), "{key}");
        }
    }
}
