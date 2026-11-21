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
fn mod_archives_are_mounted_from_the_highest_layer() {
    use fnv_bsa::{encode_block, flags, write_archive, WriteFile, WriteFolder};
    let root = std::env::temp_dir().join(format!("nvf-vfs-layered-bsa-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let data = root.join("Data");
    let (a, b, overwrite) = (root.join("mods/A"), root.join("mods/B"), root.join("overwrite"));
    for d in [&data, &a, &b, &overwrite] {
        std::fs::create_dir_all(d).unwrap();
    }
    let archive = |dir: &std::path::Path, name: &str, folder: &str, file: &str, text: &str| {
        let files = vec![WriteFile { name: file.as_bytes().to_vec(), size_flag_bits: 0, data: encode_block(text.as_bytes(), false, None) }];
        let mut bytes = Vec::new();
        write_archive(&mut bytes, flags::DIR_NAMES | flags::FILE_NAMES, 0x1, 0, &[WriteFolder { name: folder.as_bytes().to_vec(), files }]).unwrap();
        std::fs::write(dir.join(name), &bytes).unwrap();
    };
    // The game's own archive, and a mod that replaces it whole (usvfs shows one file).
    archive(&data, "Fallout - Meshes.bsa", "meshes\\base", "arch.nif", "data-archive");
    archive(&b, "Fallout - Meshes.bsa", "meshes\\base", "b.nif", "mod-archive");
    // A mod's own archive, not in the base list but named by the profile.
    archive(&a, "SomeMod.bsa", "textures\\some", "a.dds", "mod-a");
    file(&a, "textures/loose.dds", "loose");
    // A plugin-owned archive in a mod folder, mounted by load order like Data's.
    archive(&a, "PluginMod.bsa", "meshes\\plug", "p.nif", "plugin-mod");
    file(&data, "vanilla.nif", "vanilla");

    let roots: Vec<PathBuf> = vec![a.clone(), b.clone(), overwrite.clone()];
    let list = ["Fallout - Meshes.bsa", "SomeMod.bsa"];
    let files = Vfs::archive_files_layered(&data, &roots, &list, &["FalloutNV.esm", "PluginMod.esp"]);
    let mounted: Vec<(&str, &std::path::Path)> = files.iter().map(|(n, p)| (n.as_str(), p.as_path())).collect();
    assert_eq!(
        mounted,
        [("Fallout - Meshes.bsa", b.join("Fallout - Meshes.bsa").as_path()), ("SomeMod.bsa", a.join("SomeMod.bsa").as_path()), ("PluginMod.bsa", a.join("PluginMod.bsa").as_path())]
    );
    let vfs = Vfs::open_files(&data, &files).unwrap();
    assert_eq!(vfs.archive_path("Fallout - Meshes.bsa"), Some(b.join("Fallout - Meshes.bsa").as_path()));
    // The mod's replacement archive is read, the Data copy it hides is not.
    assert_eq!(vfs.read("meshes\\base\\b.nif").unwrap().unwrap(), b"mod-archive");
    assert!(vfs.read("meshes\\base\\arch.nif").is_none());
    assert_eq!(vfs.read("textures/some/a.dds").unwrap().unwrap(), b"mod-a");
    assert_eq!(vfs.read("meshes/plug/p.nif").unwrap().unwrap(), b"plugin-mod");
    // Loose files over the mods still come from the layered `Vfs`.
    let vfs = Vfs::open_for_plugins_layered(&data, &roots, &["FalloutNV.esm"]).unwrap();
    assert_eq!(vfs.read("textures/loose.dds").unwrap().unwrap(), b"loose");
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

/// Authored material maps (#2981, docs/spec-notes/pbr-materials.md) resolve per file like
/// any other texture: a mods folder's copy under `textures\pbr\` replaces Data's, through
/// NVForever's own mods folder as through a Mod Organizer 2 layer, and the path is listed
/// once.
#[test]
fn a_pbr_file_in_a_mods_root_beats_data() {
    let root = std::env::temp_dir().join(format!("nvf-vfs-layered-pbr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let data = root.join("Data");
    let mods = root.join("mods");
    for d in [&data, &mods] {
        std::fs::create_dir_all(d).unwrap();
    }
    file(&data, "textures/pbr/walls/brick07_orm.dds", "data-orm");
    file(&data, "textures/pbr/walls/brick07_n.dds", "data-n");
    file(&mods, "Textures/PBR/Walls/Brick07_ORM.dds", "mod-orm");

    let check = |vfs: &Vfs| {
        assert_eq!(vfs.read(r"textures\pbr\walls\brick07_orm.dds").unwrap().unwrap(), b"mod-orm");
        assert_eq!(vfs.source_of(r"textures\pbr\walls\brick07_orm.dds"), Some("loose"));
        assert!(vfs.loose_path(r"textures\pbr\walls\brick07_orm.dds").unwrap().starts_with(&mods));
        // The member the mod does not ship still comes from Data.
        assert_eq!(vfs.read("textures/pbr/walls/brick07_n.dds").unwrap().unwrap(), b"data-n");
        assert!(vfs.loose_path("textures/pbr/walls/brick07_n.dds").unwrap().starts_with(&data));
        assert_eq!(vfs.paths(|k| k.starts_with(r"textures\pbr\")), [r"textures\pbr\walls\brick07_n.dds", r"textures\pbr\walls\brick07_orm.dds"]);
    };
    check(&Vfs::open_for_plugins_in(&data, Some(&mods), &["FalloutNV.esm"]).unwrap());
    check(&Vfs::open_for_plugins_layered(&data, std::slice::from_ref(&mods), &["FalloutNV.esm"]).unwrap());
    std::fs::remove_dir_all(&root).unwrap();
}
