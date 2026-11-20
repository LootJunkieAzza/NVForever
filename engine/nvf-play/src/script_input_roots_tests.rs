use super::read_roots;
use std::path::PathBuf;

#[test]
fn mods_files_come_from_the_own_folder_then_the_highest_layer_then_data() {
    let data = PathBuf::from("game/Data");
    let loose = [data.clone(), PathBuf::from("mo2/mods/Low"), PathBuf::from("mo2/mods/High"), PathBuf::from("mo2/overwrite")];
    let read = read_roots(Some(PathBuf::from("home/mods")), &loose, &data);
    assert_eq!(read, ["home/mods", "mo2/overwrite", "mo2/mods/High", "mo2/mods/Low", "game/Data"].map(PathBuf::from));
}

#[test]
fn without_mod_layers_it_is_the_own_folder_and_data() {
    let data = PathBuf::from("game/Data");
    assert_eq!(read_roots(Some(PathBuf::from("home/mods")), std::slice::from_ref(&data), &data), ["home/mods", "game/Data"].map(PathBuf::from));
    assert_eq!(read_roots(None, &[], &data), [data]);
}
