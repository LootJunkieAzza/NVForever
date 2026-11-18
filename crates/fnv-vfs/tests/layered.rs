//! Loose files layered over `Data` by mods folders, lowest precedence first (#1526): a
//! Mod Organizer 2 profile's mods from the lowest priority up, then its overwrite folder.

use fnv_vfs::Vfs;
use std::path::PathBuf;

fn file(dir: &std::path::Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

#[test]
fn higher_layers_replace_lower_ones_case_insensitively() {
    let root = std::env::temp_dir().join(format!("nvf-vfs-layered-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let data = root.join("Data");
    let (a, b, overwrite) = (root.join("mods/A"), root.join("mods/B"), root.join("overwrite"));
    for d in [&data, &a, &b, &overwrite] {
        std::fs::create_dir_all(d).unwrap();
    }
    file(&data, "textures/shared.dds", "data");
    file(&data, "textures/only-data.dds", "data");
    file(&a, "textures/shared.dds", "A");
    file(&a, "textures/only-a.dds", "A");
    file(&b, "textures/Shared.DDS", "B");
    file(&b, "meshes/only-B.nif", "B");
    file(&overwrite, "textures/shared.dds", "overwrite");

    let roots: Vec<PathBuf> = vec![a.clone(), b.clone(), overwrite.clone()];
    let vfs = Vfs::open_for_plugins_layered(&data, &roots, &["FalloutNV.esm"]).unwrap();

    // The highest layer that has the file wins; paths need neither case nor slashes.
    assert_eq!(vfs.read("textures\\shared.dds").unwrap().unwrap(), b"overwrite");
    assert_eq!(vfs.read("Textures/Shared.DDS").unwrap().unwrap(), b"overwrite");
    // A file only a lower layer has is still there, and Data's own files are unmoved.
    assert_eq!(vfs.read("textures/only-a.dds").unwrap().unwrap(), b"A");
    assert_eq!(vfs.read("textures/only-data.dds").unwrap().unwrap(), b"data");
    assert_eq!(vfs.read("meshes/only-b.nif").unwrap().unwrap(), b"B");
    assert!(vfs.read("textures/none.dds").is_none());
    // The layers are reported lowest first, Data included.
    assert_eq!(vfs.loose_roots(), [data.clone(), a, b, overwrite].as_slice());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn an_absent_layer_is_skipped() {
    let root = std::env::temp_dir().join(format!("nvf-vfs-layered-absent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let data = root.join("Data");
    std::fs::create_dir_all(&data).unwrap();
    file(&data, "textures/one.dds", "data");
    let gone = root.join("mods/gone");
    let vfs = Vfs::open_for_plugins_layered(&data, &[gone], &["FalloutNV.esm"]).unwrap();
    assert_eq!(vfs.read("textures/one.dds").unwrap().unwrap(), b"data");
    assert_eq!(vfs.loose_roots(), [data].as_slice());
    std::fs::remove_dir_all(&root).unwrap();
}
