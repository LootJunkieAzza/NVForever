//! Owned search controls over the ordinary menu and simulation consumers.
#[path = "search_repair_tests.rs"]
mod repair_tests;

use super::*;
use fnv_sim::inventory::Extra;

#[test]
fn filtering_and_clearing_never_activate_rows_but_explicit_transfer_still_does() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    let original = m.lists.clone();
    let before = w.snapshot();
    assert!(m.set_search(&mut w, "TiN").unwrap());
    assert_eq!(names(m.list(Side::Container)), [("tin can", 2)]);
    assert!(m.list(Side::Player).rows.is_empty());
    assert_eq!((m.search_hidden(Side::Container), m.search_hidden(Side::Player)), (3, 4));
    assert_eq!(m.filtered, [0, 0]);
    assert_eq!(w.snapshot(), before);
    assert_eq!((m.lists[0].weight, m.lists[0].value), (original[0].weight, original[0].value));
    assert!(m.apply(&mut w, Command::One).changed);
    assert_eq!((count(&mut w, BOX_REF, CAN), count(&mut w, 0x14, CAN)), (1, 1));
    assert_eq!(m.search_text(), "TiN");
    assert_eq!(names(m.list(Side::Player)), [("tin can", 1)]);
    let transferred = w.snapshot();
    assert!(m.set_search(&mut w, "").unwrap());
    assert_eq!(w.snapshot(), transferred, "clearing must not reselect/activate the row");
    assert_eq!(m.selected_row(Side::Container).unwrap().item, FormId(CAN));
    assert_eq!(m.search_hidden(Side::Container), 0);
    assert_eq!(m.list(Side::Container).rows.len(), 4);
}

#[test]
fn hidden_barter_offers_keep_quantities_balance_and_actual_acceptance() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut m = barter(&mut w);
    select(&mut m, Side::Player, "Pistol");
    m.apply(&mut w, Command::One);
    let offered = m.trade.as_ref().unwrap().offered.clone();
    let before = w.snapshot();
    m.set_search(&mut w, "tin").unwrap();
    assert_eq!(m.trade.as_ref().unwrap().offered, offered);
    assert_eq!(m.trade.as_ref().unwrap().balance, 90);
    assert_eq!(w.snapshot(), before, "offering or hiding is not a transfer");
    select(&mut m, Side::Container, "tin can");
    m.apply(&mut w, Command::Stack);
    assert_eq!(m.trade.as_ref().unwrap().balance, 86);
    let offered = m.trade.as_ref().unwrap().offered.clone();
    m.set_search(&mut w, "no item matches this").unwrap();
    assert!(m.lists.iter().all(|l| l.rows.is_empty()));
    assert_eq!(m.trade.as_ref().unwrap().offered, offered);
    assert_eq!(m.trade.as_ref().unwrap().balance, 86);
    assert!(m.apply(&mut w, Command::Accept).changed);
    assert_eq!((count(&mut w, 0x14, PISTOL), count(&mut w, BOX_REF, PISTOL)), (0, 1));
    assert_eq!((count(&mut w, 0x14, CAN), count(&mut w, BOX_REF, CAN)), (2, 0));
    assert_eq!((count(&mut w, 0x14, CAPS), count(&mut w, BOX_REF, CAPS)), (91, 114));
    assert!(m.trade.as_ref().unwrap().is_empty());
    assert_eq!(m.trade.as_ref().unwrap().balance, 0);
    assert_eq!(m.search_text(), "no item matches this");
}

#[test]
fn synchronized_categories_keep_search_separate_from_category_counts() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    m.sync_categories = true;
    m.set_search(&mut w, "rifle").unwrap();
    m.apply(&mut w, Command::SetCategory(Side::Container, Category::Weapons));
    assert_eq!(m.category, [Category::Weapons; 2]);
    assert_eq!(m.filtered, [3, 3]);
    assert_eq!(names(m.list(Side::Container)), [("Varmint Rifle", 1)]);
    assert!(m.list(Side::Player).rows.is_empty());
    assert_eq!((m.search_hidden(Side::Container), m.search_hidden(Side::Player)), (0, 1));
    let before = w.snapshot();
    m.set_search(&mut w, "").unwrap();
    assert_eq!(names(m.list(Side::Player)), [("Pistol", 1)]);
    assert_eq!(m.filtered, [3, 3]);
    assert_eq!(m.search_text(), "");
    assert_eq!(w.snapshot(), before);
}

#[test]
fn synchronized_categories_stay_on_emptied_category_after_transfer() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    // Container mode: transferring the sole weapon out of Weapons keeps both columns on Weapons
    // (Stewie's Tweaks bPreventSwitchWhenEmptyingCategory).
    let mut m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    m.sync_categories = true;
    m.apply(&mut w, Command::SetCategory(Side::Player, Category::Weapons));
    assert_eq!(m.category, [Category::Weapons; 2]);
    assert_eq!(names(m.list(Side::Player)), [("Pistol", 1)]);
    select(&mut m, Side::Player, "Pistol");
    assert!(m.apply(&mut w, Command::One).changed);
    assert!(m.list(Side::Player).rows.is_empty(), "player weapons category is now empty");
    assert_eq!(m.category, [Category::Weapons; 2], "emptying a synchronized category must not switch back to All");
    // Barter mode: offering and accepting the sole weapon out of Weapons keeps both columns on Weapons.
    let mut wb = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut b = barter(&mut wb);
    b.sync_categories = true;
    b.apply(&mut wb, Command::SetCategory(Side::Player, Category::Weapons));
    assert_eq!(b.category, [Category::Weapons; 2]);
    assert_eq!(names(b.list(Side::Player)), [("Pistol", 1)]);
    select(&mut b, Side::Player, "Pistol");
    assert!(b.apply(&mut wb, Command::One).changed);
    assert!(b.apply(&mut wb, Command::Accept).changed);
    assert!(b.list(Side::Player).rows.is_empty(), "player weapons category is now empty");
    assert_eq!(b.category, [Category::Weapons; 2], "emptying a synchronized barter category keeps Weapons");
}

fn search_in_app(app: &mut App, text: &str) {
    app.world_mut()
        .resource_scope(|world, mut active: bevy::prelude::Mut<crate::menu::ActiveMenu>| {
            let mut sim = world.resource_mut::<crate::sim::SimState>();
            active.0.as_mut().unwrap().set_search(&mut sim.world, text).unwrap();
        });
}

#[test]
fn pre_filter_painted_contact_cannot_transfer_the_new_item_at_its_old_index() {
    let mut app = app_on_the_box();
    let caps_guard = guard_of(&app, 0, false, 2);
    search_in_app(&mut app, "rifle");
    assert_eq!(row_of(&app, "Varmint Rifle"), 0);
    send(&mut app, &[Command::Expect(caps_guard), Command::Hover(Side::Container, 0), Command::One]);
    assert_eq!(in_box(&mut app, RIFLE), 1, "real apply_input must discard both stale commands");
    assert_eq!(in_box(&mut app, CAPS), 200);
    let current = guard_of(&app, 0, false, 2);
    send(&mut app, &[Command::Expect(current), Command::Hover(Side::Container, 0), Command::One]);
    assert_eq!(in_box(&mut app, RIFLE), 0, "the new painted contact still transfers");
    // Select remains an action on an already-selected row; search did not change that contract.
    search_in_app(&mut app, "tin");
    let current = guard_of(&app, 0, false, 2);
    send(
        &mut app,
        &[
            Command::Expect(current),
            Command::Hover(Side::Container, 0),
            Command::Select(Side::Container, 0),
        ],
    );
    assert_eq!(in_box(&mut app, CAN), 1);
}

#[test]
fn quantity_prompt_keeps_its_row_until_cancel_then_search_can_change() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    m.set_search(&mut w, "cap").unwrap();
    select(&mut m, Side::Container, "Bottle Cap");
    m.apply(&mut w, Command::One);
    assert_eq!(m.quantity.unwrap().max, 200);
    let (before, rows, prompt) = (w.snapshot(), m.lists.clone(), m.quantity);
    assert!(matches!(m.set_search(&mut w, "rifle"), Err(SearchError::QuantityOpen)));
    assert!(!m.set_search(&mut w, "cap").unwrap());
    assert_eq!((m.lists.clone(), m.quantity), (rows, prompt));
    assert_eq!(w.snapshot(), before);
    assert_eq!(m.search_text(), "cap");
    m.apply(&mut w, Command::AmountCancel);
    assert!(m.set_search(&mut w, "rifle").unwrap());
    assert_eq!(m.selected_row(Side::Container).unwrap().item, FormId(RIFLE));
    assert_eq!(w.snapshot(), before);
}

#[test]
fn unicode_bound_refusal_preserves_query_selection_and_world() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    let at_bound = "é".repeat(MAX_SEARCH_CHARS);
    assert!(m.set_search(&mut w, &at_bound).unwrap());
    assert!(m.lists.iter().all(|l| l.rows.is_empty()));
    let (before, rows, selected, scroll) = (w.snapshot(), m.lists.clone(), m.selected, m.scroll);
    assert!(matches!(m.set_search(&mut w, &(at_bound.clone() + "é")), Err(SearchError::TooLong)));
    assert_eq!(m.search_text(), at_bound);
    assert_eq!((m.lists.clone(), m.selected, m.scroll), (rows, selected, scroll));
    assert_eq!(w.snapshot(), before);
    m.set_search(&mut w, "").unwrap();
    assert_eq!(m.list(Side::Container).rows.len(), 4);
}

#[test]
fn changed_condition_refresh_and_model_clone_preserve_query_without_reactivating_items() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let at = |condition| Extra {
        condition: Some(condition),
        ..Extra::default()
    };
    w.add_item_extra(PLAYER, FormId(RIFLE), 1, at(0.4)).unwrap();
    w.add_item_extra(PLAYER, FormId(RIFLE), 1, Extra { mods: 2, ..at(0.6) }).unwrap();
    let mut m = MenuModel::open_inventory(&mut w).unwrap();
    m.set_search(&mut w, "rifle").unwrap();
    m.selected[Side::Player as usize] = m.list(Side::Player).rows.iter().position(|r| r.mods == 2).unwrap();
    // Repair returns through refresh on this same model. Change real inventory state first.
    {
        let inv = w.inventory_mut(PLAYER).unwrap();
        let removed = inv.take_at(FormId(RIFLE), 1, Pick::row(Some(0.6), 2));
        assert_eq!(removed, [(Extra { mods: 2, ..at(0.6) }, 1)]);
        inv.add_extra(FormId(RIFLE), 1, Extra { mods: 2, ..at(0.8) });
    }
    let before_refresh = w.snapshot();
    m.refresh(&mut w).unwrap();
    assert_eq!(w.snapshot(), before_refresh);
    assert_eq!(m.search_text(), "rifle");
    assert_eq!(m.list(Side::Player).rows.len(), 2);
    assert!(m.list(Side::Player).rows.iter().any(|r| r.mods == 2 && r.condition == Some(0.8)));
    assert!(m.list(Side::Player).rows.iter().all(|r| !r.equipped));
    assert_eq!(count(&mut w, 0x14, RIFLE), 2);
    let mut clone = m.clone();
    clone.refresh(&mut w).unwrap();
    assert_eq!(clone.search_text(), "rifle");
    assert_eq!(clone.lists, m.lists);
    assert_eq!(MenuModel::open_inventory(&mut w).unwrap().search_text(), "");
}

#[test]
fn folder_search_keeps_back_navigation_and_does_not_search_hidden_folders() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    for item in [KEY_A, KEY_B] {
        w.add_item(PLAYER, FormId(item), 1).unwrap();
    }
    let mut m = MenuModel::open_inventory(&mut w).unwrap();
    m.apply(&mut w, Command::SetCategory(Side::Player, Category::Misc));
    m.set_search(&mut w, "shack").unwrap();
    assert!(m.list(Side::Player).rows.is_empty(), "a closed keyring is not recursively searched");
    m.set_search(&mut w, "").unwrap();
    select(&mut m, Side::Player, "Keyring");
    m.apply(&mut w, Command::One);
    m.set_search(&mut w, "shack").unwrap();
    assert_eq!(names(m.list(Side::Player)), [("Back", 0), ("Shack Key", 1)]);
    assert_eq!(m.search_hidden(Side::Player), 1);
    select(&mut m, Side::Player, "Back");
    m.apply(&mut w, Command::One);
    assert_eq!(m.folder, None);
    assert_eq!(m.search_text(), "shack");
    m.set_search(&mut w, "").unwrap();
    assert!(names(m.list(Side::Player)).contains(&("Keyring", 2)));
    assert_eq!((count(&mut w, 0x14, KEY_A), count(&mut w, 0x14, KEY_B)), (1, 1));
}

#[test]
fn search_selection_keeps_a_dropped_reference_distinct_from_the_same_carried_units() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let dropped = w.drop_item(FormId(BOX_REF), FormId(RIFLE), 1).unwrap()[0];
    w.get(dropped).unwrap().dropped_by = Some(FormId(BOX_REF));
    w.add_item(FormId(BOX_REF), FormId(RIFLE), 1).unwrap();
    let mut m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    m.selected[Side::Container as usize] = m.list(Side::Container).rows.iter().position(|r| r.reference == Some(dropped)).unwrap();
    m.set_search(&mut w, "rifle").unwrap();
    assert_eq!(m.selected_row(Side::Container).unwrap().reference, Some(dropped));
    m.set_search(&mut w, "").unwrap();
    assert_eq!(m.selected_row(Side::Container).unwrap().reference, Some(dropped));
    let action = m.apply(&mut w, Command::One);
    assert_eq!(action.picked, [dropped]);
    assert!(w.peek(dropped).unwrap().deleted());
    assert_eq!((count(&mut w, BOX_REF, RIFLE), count(&mut w, 0x14, RIFLE)), (1, 1));
}

#[test]
fn search_preserves_the_own_stock_role_beside_an_identical_offered_row() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    w.add_item(FormId(BOX_REF), FormId(PISTOL), 1).unwrap();
    let mut m = barter(&mut w);
    select(&mut m, Side::Player, "Pistol");
    let units = m.selected_row(Side::Player).unwrap().units();
    m.apply(&mut w, Command::One);
    m.focus = Side::Container;
    m.selected[Side::Container as usize] = m
        .list(Side::Container)
        .rows
        .iter()
        .position(|r| r.item == FormId(PISTOL) && !r.bartered)
        .unwrap();
    m.set_search(&mut w, "pistol").unwrap();
    assert_eq!(m.list(Side::Container).rows.len(), 2);
    assert!(!m.selected_row(Side::Container).unwrap().bartered);
    m.set_search(&mut w, "").unwrap();
    assert!(!m.selected_row(Side::Container).unwrap().bartered);
    m.apply(&mut w, Command::One);
    let t = m.trade.as_ref().unwrap();
    for side in [Side::Container, Side::Player] {
        assert_eq!(t.offered[side as usize].get(&units), Some(&1));
    }
    assert_eq!(
        (count(&mut w, BOX_REF, PISTOL), count(&mut w, 0x14, PISTOL)),
        (1, 1),
        "both are offers, not transfers"
    );
}
