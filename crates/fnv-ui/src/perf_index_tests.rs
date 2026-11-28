use super::*;
use crate::MemSource;

#[test]
fn incremental_update_and_value_gen_track_real_changes_only() {
    let xml = r#"<menu name="M">
      <rect name="A"><user0>10</user0><user1><copy src="me()" trait="user0"/><add>5</add></user1></rect>
      <rect name="B"><user0><copy src="sibling(A)" trait="user1"/><mul>2</mul></user0><string>hello</string></rect>
    </menu>"#;
    let src = MemSource::default().with("m.xml", xml);
    let mut ui = Ui::new();
    let _menu = ui.load_menu(&src, "m.xml").unwrap();
    // First update has reshaped_run == 1; second idle update clears reshaped_run,
    // and next value change builds/uses incremental dependents.
    ui.update();
    assert_eq!(ui.get_float("M/A/user1"), Some(15.0));
    assert_eq!(ui.get_float("M/B/user0"), Some(30.0));

    // Reset reshaped_run on a clean tick, then trigger one update that finishes with reshaped_run == 0.
    ui.update();
    assert!(ui.set_float("M/A/user0", 20.0));
    ui.update();
    assert_eq!(ui.get_float("M/A/user1"), Some(25.0));
    assert_eq!(ui.get_float("M/B/user0"), Some(50.0));
    assert!(!ui.dirty_all);

    // No-op writes leave value_gen unchanged.
    let vgen_before = ui.value_gen();
    assert!(ui.set_float("M/A/user0", 20.0));
    assert!(ui.set_string("M/B/string", "hello"));
    assert_eq!(ui.value_gen(), vgen_before);
    assert_eq!(ui.update(), 0);

    // Incremental update propagates A/user0 -> A/user1 -> B/user0 without full scan.
    ui.evaluations = 0;
    assert!(ui.set_float("M/A/user0", 100.0));
    assert!(ui.value_gen() > vgen_before);
    let changed = ui.update();
    assert_eq!(changed, 2);
    assert_eq!(ui.evaluations, 2);
    assert_eq!(ui.get_float("M/A/user1"), Some(105.0));
    assert_eq!(ui.get_float("M/B/user0"), Some(210.0));
}

#[test]
fn trait_path_cache_clears_on_shape_change_and_keeps_missing_visible_semantics() {
    let xml = r#"<menu name="M"><rect name="Box"><x>1</x></rect></menu>"#;
    let src = MemSource::default().with("m.xml", xml);
    let mut ui = Ui::new();
    let menu = ui.load_menu(&src, "m.xml").unwrap();
    ui.update();

    // Missing <visible> trait still returns None (vanilla behavior preserved).
    assert_eq!(ui.get_float("M/Box/visible"), None);
    assert!(!ui.set_float("M/Box/visible", 0.0));

    // Cached path lookup works and invalidates when tile is unloaded.
    assert!(ui.set_float("M/Box/x", 42.0));
    assert_eq!(ui.get_float("M/Box/x"), Some(42.0));
    assert!(!ui.trait_path_cache_last.is_empty());

    let box_id = ui.find_tile("M/Box").unwrap();
    ui.unload_tile(box_id);
    assert!(ui.trait_path_cache_last.is_empty());
    assert_eq!(ui.get_float("M/Box/x"), None);
    assert!(ui.is_live(menu));
}
