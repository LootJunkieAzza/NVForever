//! The run's base archive list (`fnv_vfs::set_archive_list`, issue #1142): set once in
//! place of the shipped `SArchiveList`, before `Update.bsa` and the plugins' archives. In
//! its own test binary, as the list is the process's.

use fnv_vfs::{archive_list, archive_list_overridden, set_archive_list, Vfs, DEFAULT_ARCHIVE_LIST};

#[test]
fn a_given_archive_list_replaces_the_shipped_one_once() {
    let dir = std::env::temp_dir().join(format!("nvf-vfs-archive-list-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // The names only matter: archive_order mounts what is present (empty files will do).
    for n in ["Fallout - Textures.bsa", "Fallout - Meshes.bsa", "MyBase.bsa", "Update.bsa", "MyMod - Main.bsa"] {
        std::fs::write(dir.join(n), b"").unwrap();
    }
    assert!(!archive_list_overridden());
    assert_eq!(archive_list(), DEFAULT_ARCHIVE_LIST.map(String::from).to_vec());
    let (default, _) = Vfs::order_for_plugins(&dir, &["FalloutNV.esm", "MyMod.esp"]);
    assert_eq!(default, ["Fallout - Textures.bsa", "Fallout - Meshes.bsa", "Update.bsa", "MyMod - Main.bsa"]);
    // An empty list is refused and changes nothing.
    assert!(set_archive_list(Vec::new()).is_err());
    assert!(!archive_list_overridden());
    // A mod setup's list with a base archive of its own, after the shipped ones.
    let given: Vec<String> = ["Fallout - Textures.bsa", "Fallout - Meshes.bsa", "MyBase.bsa"].map(String::from).to_vec();
    set_archive_list(given.clone()).unwrap();
    assert!(archive_list_overridden());
    let (order, from_env) = Vfs::order_for_plugins(&dir, &["FalloutNV.esm", "MyMod.esp"]);
    assert!(!from_env);
    assert_eq!(order, ["Fallout - Textures.bsa", "Fallout - Meshes.bsa", "MyBase.bsa", "Update.bsa", "MyMod - Main.bsa"]);
    // Once per run.
    assert!(set_archive_list(vec!["Other.bsa".into()]).is_err());
    assert_eq!(archive_list(), given);
    std::fs::remove_dir_all(&dir).unwrap();
}
