//! Which plugin's record names a model file (#3045), so a `missing_asset` event can say
//! whose file it is, and so a file the base game's own data names and no archive has is
//! not reported as a fault of the player's install.
//!
//! FalloutNV.esm names `MMM_ghoulvariant_v1.nif` in creatures' model lists and no base
//! archive holds it [observed: player reports of build 0.8.0, 11 of 49 installs, unmodded
//! load orders among them; issue #3045]. It is the data's own dangling name, as the
//! starfield cloud texture is (`sky.rs` `official_plugin`, #2408).
//!
//! The list is made once a session, on a thread of its own, from the winning record of
//! every form: its model subrecords, and for a creature the names of its model list in
//! its skeleton's folder. A file named only by other files (a NIF's texture, a script's
//! path) has no entry.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

/// Model files by path (lower case, `meshes\…`), each with the plugin whose winning record
/// names it.
#[derive(Debug, Default)]
pub struct Sources {
    by_path: HashMap<String, String>,
}

/// Whether a plugin is the base game or one of its add-ons, by file name whatever its case.
pub fn official(plugin: &str) -> bool {
    super::OFFICIAL_PLUGINS.iter().any(|o| o.eq_ignore_ascii_case(plugin))
}

/// A path as the lists and the log lines are compared: lower case, backslashes.
pub fn normal(path: &str) -> String {
    path.trim().replace('/', "\\").to_ascii_lowercase()
}

impl Sources {
    /// `plugin`'s record names `path`. When several records name one file, a mod's is
    /// kept over the game's own: the file is then missing from that mod as well.
    pub fn note(&mut self, path: &str, plugin: &str) {
        match self.by_path.entry(normal(path)) {
            Entry::Vacant(e) => {
                e.insert(plugin.to_string());
            }
            Entry::Occupied(mut e) => {
                if official(e.get()) && !official(plugin) {
                    e.insert(plugin.to_string());
                }
            }
        }
    }

    /// The plugin whose record names `path`, if one does.
    pub fn plugin_of(&self, path: &str) -> Option<&str> {
        self.by_path.get(&normal(path)).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    /// From a load order: every form's winning record, deleted ones left out.
    pub fn of(lo: &fnv_esm::LoadOrder) -> Sources {
        let mut out = Sources::default();
        for defs in lo.index.values() {
            let Some(d) = defs.last() else { continue };
            let r = &lo.records[d.plugin as usize][d.ordinal as usize];
            if !may_name_a_model(&r.header.sig) || r.header.flags & 0x20 != 0 {
                continue;
            }
            let Some(plugin) = lo.plugins.get(d.plugin as usize) else { continue };
            let mut subs: Vec<([u8; 4], Vec<u8>)> = Vec::new();
            // A record that does not read to its end still gives the names before the fault.
            let _ = r.visit_subrecords(|sig, data| {
                if MODEL_SUBRECORDS.contains(&sig) || sig == b"NIFZ" {
                    subs.push((*sig, data.to_vec()));
                }
            });
            for path in named_models(&r.header.sig, &subs) {
                out.note(&path, &plugin.name);
            }
        }
        out
    }
}

/// The subrecords that hold a model's path, relative to `meshes\` [documented:
/// `schemas/fnv`: MODL, and the further models of armour, weapons and addons MOD2 to MOD4].
const MODEL_SUBRECORDS: [&[u8; 4]; 4] = [b"MODL", b"MOD2", b"MOD3", b"MOD4"];

/// The record types with too many records and no model of their own, passed over unread.
fn may_name_a_model(sig: &[u8; 4]) -> bool {
    !matches!(sig, b"REFR" | b"ACHR" | b"ACRE" | b"PGRE" | b"PMIS" | b"NAVM" | b"LAND" | b"INFO" | b"DIAL" | b"CELL" | b"WRLD" | b"GMST" | b"GLOB")
}

/// A subrecord's text: bytes up to the first zero, one character each (the paths are
/// compared in lower case ASCII; other bytes only need to stay apart).
fn text(data: &[u8]) -> String {
    data.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect()
}

/// The model files a record names, as archive paths: each model subrecord's, and for a
/// creature each name of its model list (NIFZ: names one after another, each ended by a
/// zero) in its skeleton's folder (`fnv_actor::resolve::creature_part_path`).
pub fn named_models(sig: &[u8; 4], subs: &[([u8; 4], Vec<u8>)]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skeleton = None;
    for (s, data) in subs {
        if !MODEL_SUBRECORDS.contains(&s) {
            continue;
        }
        let stored = text(data);
        if stored.trim().is_empty() {
            continue;
        }
        let path = fnv_actor::idle::meshes_path(stored.trim());
        if s == b"MODL" && skeleton.is_none() {
            skeleton = Some(path.clone());
        }
        out.push(path);
    }
    if let (b"CREA", Some(skeleton)) = (sig, &skeleton) {
        for (_, data) in subs.iter().filter(|(s, _)| s == b"NIFZ") {
            for name in data.split(|&b| b == 0).map(text).filter(|n| !n.trim().is_empty()) {
                out.push(fnv_actor::resolve::creature_part_path(skeleton, &name));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(sig: &[u8; 4], data: &[u8]) -> ([u8; 4], Vec<u8>) {
        (*sig, data.to_vec())
    }

    #[test]
    fn a_records_models_are_archive_paths() {
        assert_eq!(named_models(b"STAT", &[sub(b"MODL", b"Clutter\\Crate01.NIF\0"), sub(b"NIFZ", b"x.nif\0")]), ["meshes\\clutter\\crate01.nif"]);
        assert_eq!(
            named_models(b"WEAP", &[sub(b"MODL", b"weapons\\a.nif\0"), sub(b"MOD4", b"meshes\\weapons\\a_go.nif\0"), sub(b"MOD2", b"\0")]),
            ["meshes\\weapons\\a.nif", "meshes\\weapons\\a_go.nif"]
        );
        // A creature's model list stands in its skeleton's folder.
        assert_eq!(
            named_models(b"CREA", &[sub(b"MODL", b"Creatures\\Ghoul\\skeleton.NIF\0"), sub(b"NIFZ", b"MMM_ghoulvariant_v1.nif\0feralghoul.nif\0\0")]),
            ["meshes\\creatures\\ghoul\\skeleton.nif", "meshes\\creatures\\ghoul\\mmm_ghoulvariant_v1.nif", "meshes\\creatures\\ghoul\\feralghoul.nif"]
        );
        assert!(named_models(b"CREA", &[sub(b"NIFZ", b"a.nif\0")]).is_empty(), "no skeleton, no folder");
    }

    #[test]
    fn a_mods_record_is_kept_over_the_games_own() {
        let mut s = Sources::default();
        s.note("meshes\\A.nif", "FalloutNV.esm");
        s.note("meshes/a.nif", "Mod.esp");
        s.note("meshes\\a.nif", "DeadMoney.esm");
        s.note("meshes\\b.nif", "Mod.esp");
        s.note("meshes\\b.nif", "Other.esp");
        assert_eq!(s.plugin_of("Meshes\\a.NIF"), Some("Mod.esp"));
        assert_eq!(s.plugin_of("meshes\\b.nif"), Some("Mod.esp"));
        assert_eq!(s.plugin_of("meshes\\c.nif"), None);
        assert_eq!(s.len(), 2);
        assert!(official("falloutnv.ESM") && official("CaravanPack.esm"));
        assert!(!official("TaleOfTwoWastelands.esm") && !official("FalloutNV.esm.bak"));
    }

    /// With the game's data (`NVF_DATA=<Data>`): FalloutNV.esm's creatures name the ghoul
    /// variant no base archive has (#3045), and the list is made in well under a second.
    #[test]
    #[ignore = "needs the game's Data folder in NVF_DATA"]
    fn the_base_game_names_the_ghoul_variant() {
        let dir = std::path::PathBuf::from(std::env::var_os("NVF_DATA").expect("NVF_DATA"));
        let mut plugin = fnv_esm::Plugin::open(dir.join("FalloutNV.esm")).unwrap();
        plugin.name = "FalloutNV.esm".into();
        let plugins = [plugin];
        let lo = fnv_esm::LoadOrder::new(&plugins).unwrap();
        let t = std::time::Instant::now();
        let s = Sources::of(&lo);
        eprintln!("{} model files listed in {:.0} ms", s.len(), t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(s.plugin_of("meshes\\creatures\\ghoul\\mmm_ghoulvariant_v1.nif"), Some("FalloutNV.esm"));
        assert!(s.len() > 5000, "{}", s.len());
    }
}
