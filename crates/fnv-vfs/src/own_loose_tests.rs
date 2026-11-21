//! Authored filesystem/archive controls through the unchanged public constructors.
use super::Vfs;
use fnv_bsa::{encode_block, flags, write_archive, WriteFile, WriteFolder};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Roots {
    root: PathBuf,
    data: PathBuf,
    mods: PathBuf,
}

impl Roots {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!("nvf-own-loose-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&root).unwrap();
        let (data, mods) = (root.join("Data"), root.join("mods"));
        std::fs::create_dir(&data).unwrap();
        std::fs::create_dir(&mods).unwrap();
        Self { root, data, mods }
    }

    fn open(&self, mods: Option<&Path>) -> Vfs {
        Vfs::open_for_plugins_in(&self.data, mods, &["Low.esp", "High.esp"]).unwrap()
    }
}

impl Drop for Roots {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn loose(root: &Path, path: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

fn archive(root: &Path, name: &str, bytes: &[u8]) {
    let files = ["shared.xml", "archived.xml"].into_iter().map(|name| WriteFile {
        name: name.as_bytes().to_vec(), size_flag_bits: 0, data: encode_block(bytes, false, None),
    }).collect();
    let mut data = Vec::new();
    write_archive(&mut data, flags::DIR_NAMES | flags::FILE_NAMES, 0x1, 0, &[WriteFolder { name: b"menus\\main".to_vec(), files }]).unwrap();
    std::fs::write(root.join(name), data).unwrap();
}

#[test]
fn home_only_mixed_case_asset_has_the_same_read_and_stream_source() {
    let r = Roots::new();
    let file = loose(&r.mods, "Meshes/Own/Clip.KF", b"authored home-only bytes");
    let vfs = r.open(Some(&r.mods));
    assert!(vfs.exists("MESHES/OWN/clip.kf"));
    assert_eq!(vfs.source_of("meshes\\own\\CLIP.KF"), Some("loose"));
    assert_eq!(vfs.loose_path("Meshes/Own/clip.kf"), Some(file.clone()));
    assert_eq!(vfs.read("meshes/own/clip.kf").unwrap().unwrap(), std::fs::read(file).unwrap());
    assert_eq!(vfs.paths(|_| true), ["meshes\\own\\clip.kf"]);
    assert!(vfs.read_archived("meshes/own/clip.kf").is_none());
    assert_eq!(vfs.data_dir(), r.data.as_path());
    assert!(!r.data.join("Meshes/Own/Clip.KF").exists());
}

#[test]
fn two_loose_layers_keep_the_highest_actual_archive_underneath() {
    let r = Roots::new();
    archive(&r.data, "Low.bsa", b"low archive");
    archive(&r.data, "High.bsa", b"hidden Data archive");
    archive(&r.mods, "High.bsa", b"winning home archive");
    loose(&r.data, "Menus/Main/shared.xml", b"Data loose");
    let file = loose(&r.mods, "Menus/Main/Shared.XML", b"home loose");
    let vfs = r.open(Some(&r.mods));
    assert_eq!(vfs.read("menus/main/shared.xml").unwrap().unwrap(), b"home loose");
    assert_eq!(vfs.read_archived("menus/main/shared.xml").unwrap().unwrap(), b"winning home archive");
    assert_eq!(vfs.read_archived("menus/main/archived.xml").unwrap().unwrap(), b"winning home archive");
    assert_eq!(vfs.read("menus/main/archived.xml").unwrap().unwrap(), b"winning home archive");
    assert_eq!(vfs.source_of("menus/main/shared.xml"), Some("loose"));
    assert_eq!(vfs.loose_path("menus/main/shared.xml"), Some(file));
    assert!(vfs.loose_path("menus/main/archived.xml").is_none());
    assert_eq!(vfs.archives().map(str::to_owned).collect::<Vec<_>>(), ["Low.bsa".to_owned(), r.mods.join("High.bsa").to_string_lossy().into_owned()]);
    assert_eq!(vfs.data_dir(), r.data.as_path());
}

#[test]
fn fresh_mounts_reveal_data_then_archive_when_higher_layers_are_removed() {
    let r = Roots::new();
    archive(&r.data, "High.bsa", b"archive");
    let data_file = loose(&r.data, "Menus/Main/shared.xml", b"Data");
    let home_file = loose(&r.mods, "Menus/Main/shared.xml", b"home");
    assert_eq!(r.open(Some(&r.mods)).read("menus/main/shared.xml").unwrap().unwrap(), b"home");
    std::fs::remove_file(home_file).unwrap();
    let data = r.open(Some(&r.mods));
    assert_eq!(data.read("menus/main/shared.xml").unwrap().unwrap(), b"Data");
    assert_eq!(data.loose_path("menus/main/shared.xml"), Some(data_file.clone()));
    std::fs::remove_file(data_file).unwrap();
    let archived = r.open(Some(&r.mods));
    assert_eq!(archived.read("menus/main/shared.xml").unwrap().unwrap(), b"archive");
    assert_eq!(archived.source_of("menus/main/shared.xml"), Some("High.bsa"));
    assert!(archived.read("menus/main/absent.xml").is_none());
}

#[test]
fn optional_roots_preserve_reads_and_existing_archive_name_policy() {
    let r = Roots::new();
    archive(&r.data, "High.bsa", b"archive");
    loose(&r.data, "Menus/Main/shared.xml", b"Data");
    let ordinary = Vfs::open(&r.data, &["High.bsa".to_owned()]).unwrap();
    let missing = r.root.join("not-created");
    for mods in [None, Some(missing.as_path())] {
        let vfs = r.open(mods);
        assert_eq!(vfs.paths(|_| true), ordinary.paths(|_| true));
        assert_eq!(vfs.archives().collect::<Vec<_>>(), ordinary.archives().collect::<Vec<_>>());
        assert_eq!(vfs.data_dir(), ordinary.data_dir());
        assert_eq!(vfs.skipped(), ordinary.skipped());
        for path in ordinary.paths(|_| true) {
            assert_eq!(vfs.read(&path).unwrap().unwrap(), ordinary.read(&path).unwrap().unwrap());
            assert_eq!(vfs.source_of(&path), ordinary.source_of(&path));
            assert_eq!(vfs.loose_path(&path), ordinary.loose_path(&path));
            assert_eq!(vfs.read_archived(&path).unwrap().unwrap(), ordinary.read_archived(&path).unwrap().unwrap());
        }
    }
    // Existing archive discovery names an explicit mods copy by its full path,
    // even if both roots identify Data. Loose indexing must not change that rule.
    let same = r.open(Some(&r.data));
    assert_eq!(same.archives().map(str::to_owned).collect::<Vec<_>>(), [r.data.join("High.bsa").to_string_lossy().into_owned()]);
    assert_eq!(same.paths(|_| true), ordinary.paths(|_| true));
    assert_eq!(same.data_dir(), r.data.as_path());
    assert_eq!(same.read("menus/main/shared.xml").unwrap().unwrap(), b"Data");
    assert_eq!(same.loose_path("menus/main/shared.xml"), ordinary.loose_path("menus/main/shared.xml"));
    assert_eq!(same.read_archived("menus/main/shared.xml").unwrap().unwrap(), b"archive");
}

#[test]
fn root_level_files_are_excluded_while_layer_neighbors_remain() {
    let r = Roots::new();
    loose(&r.data, "Textures/Neighbor.dds", b"Data neighbor");
    loose(&r.mods, "Meshes/Own.nif", b"home neighbor");
    for root in [&r.data, &r.mods] {
        loose(root, "Credits.txt", b"top level");
        loose(root, "Unloaded.esp", b"top level");
        loose(root, "Unloaded.bsa", b"top level");
    }
    let vfs = r.open(Some(&r.mods));
    assert_eq!(vfs.paths(|_| true), ["meshes\\own.nif", "textures\\neighbor.dds"]);
    assert_eq!(vfs.read("textures/neighbor.dds").unwrap().unwrap(), b"Data neighbor");
    assert_eq!(vfs.read("meshes/own.nif").unwrap().unwrap(), b"home neighbor");
    assert_eq!(vfs.archives().count(), 0);
    for path in ["Credits.txt", "Unloaded.esp", "Unloaded.bsa"] {
        assert!(!vfs.exists(path));
        assert!(vfs.loose_path(path).is_none());
        assert!(vfs.read(path).is_none());
    }
}

#[test]
fn deleting_an_indexed_home_winner_keeps_read_error_instead_of_fallback() {
    let r = Roots::new();
    archive(&r.data, "High.bsa", b"archive");
    loose(&r.data, "Menus/Main/shared.xml", b"hidden Data");
    let home = loose(&r.mods, "Menus/Main/shared.xml", b"home");
    let vfs = r.open(Some(&r.mods));
    assert_eq!(vfs.read("menus/main/shared.xml").unwrap().unwrap(), b"home");
    std::fs::remove_file(&home).unwrap();
    assert!(vfs.exists("menus/main/shared.xml"));
    assert_eq!(vfs.source_of("menus/main/shared.xml"), Some("loose"));
    assert_eq!(vfs.loose_path("menus/main/shared.xml"), Some(home));
    assert_eq!(vfs.read("menus/main/shared.xml").unwrap().unwrap_err().kind(), std::io::ErrorKind::NotFound);
    assert_eq!(vfs.read_archived("menus/main/shared.xml").unwrap().unwrap(), b"archive");
}

#[test]
fn the_path_index_requires_reopen_but_indexed_file_bytes_are_not_snapshotted() {
    let r = Roots::new();
    let original = loose(&r.mods, "Meshes/Existing.nif", b"first");
    let vfs = r.open(Some(&r.mods));
    assert_eq!(vfs.read("meshes/existing.nif").unwrap().unwrap(), b"first");
    loose(&r.mods, "Meshes/Added.nif", b"added later");
    std::fs::write(original, b"updated bytes").unwrap();
    assert!(!vfs.exists("meshes/added.nif"));
    assert!(vfs.read("meshes/added.nif").is_none());
    assert_eq!(vfs.read("meshes/existing.nif").unwrap().unwrap(), b"updated bytes");
    let reopened = r.open(Some(&r.mods));
    assert_eq!(reopened.read("meshes/added.nif").unwrap().unwrap(), b"added later");
}

#[test]
fn a_non_directory_layer_keeps_data_and_reports_its_traversal_error() {
    let r = Roots::new();
    loose(&r.data, "Meshes/Neighbor.nif", b"Data neighbor");
    let not_a_directory = loose(&r.root, "not-a-directory", b"file");
    let vfs = r.open(Some(&not_a_directory));
    assert_eq!(vfs.read("meshes/neighbor.nif").unwrap().unwrap(), b"Data neighbor");
    assert_eq!(vfs.skipped().len(), 1);
    assert_eq!(vfs.skipped()[0].0, not_a_directory.display().to_string());
    assert!(!vfs.skipped()[0].1.is_empty());
    assert!(vfs.broken_archives().is_empty());
}
