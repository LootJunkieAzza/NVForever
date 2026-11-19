//! Actual public repair-key consumer over a filtered native inventory model.
use super::*;
use crate::pipboy::{
    repair::{PipRepair, RepairInput},
    Page, PipBoy, PipBoyInput,
};
use bevy::prelude::{ButtonInput, KeyCode};

fn filtered_repair_app(worn: bool) -> App {
    let f: &'static Fixture = Box::leak(Box::new(fixture()));
    let mut world = World::new(&f.forms, &f.refs, SimConfig::default());
    world
        .add_item_extra(
            PLAYER,
            FormId(RIFLE),
            1,
            Extra {
                condition: Some(0.4),
                mods: 2,
                equipped: worn,
                ..Extra::default()
            },
        )
        .unwrap();
    world
        .add_item_extra(
            PLAYER,
            FormId(RIFLE),
            1,
            Extra {
                condition: Some(0.3),
                ..Extra::default()
            },
        )
        .unwrap();
    let mut model = MenuModel::open_inventory(&mut world).unwrap();
    model.set_search(&mut world, "+").unwrap();
    assert_eq!(model.list(Side::Player).rows.len(), 1);
    assert_eq!(model.selected_row(Side::Player).unwrap().mods, 2);
    let mut pip = PipBoy::default();
    pip.apply(PipBoyInput::Page(Page::Items));
    pip.raise();
    assert!(pip.open());
    let mut app = App::new();
    app.init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<crate::menu::PromptKeys>()
        .init_resource::<crate::pipboy::KeysTaken>()
        .init_resource::<crate::pipboy::item_mod::PipItemMod>()
        .init_resource::<PipRepair>()
        .insert_resource(pip)
        .insert_resource(crate::menu::ActiveMenu(Some(model)))
        .insert_resource(crate::sim::SimState {
            forms: &f.forms,
            refs: &f.refs,
            world,
        })
        .add_message::<RepairInput>()
        .add_message::<crate::ui_sounds::UiSound>()
        .add_systems(Update, crate::pipboy::repair::keys);
    app
}

fn press_repair_key(app: &mut App, key: KeyCode) {
    {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(key);
    }
    app.update();
}

#[test]
fn filtered_inventory_repair_uses_a_hidden_part_and_refreshes_the_same_model() {
    for worn in [false, true] {
        let mut app = filtered_repair_app(worn);
        press_repair_key(&mut app, KeyCode::KeyR);
        {
            let session = app.world().resource::<PipRepair>().0.as_ref().expect("real R opened repair");
            assert_eq!(session.offer.extra.mods, 2);
            assert_eq!(session.offer.extra.equipped, worn);
            assert_eq!(session.offer.parts.len(), 1);
            assert_eq!(session.offer.parts[0].extra.mods, 0, "the query hides this part only from the item list");
            assert_eq!(session.offer.parts[0].condition, 0.3);
        }
        press_repair_key(&mut app, KeyCode::ArrowDown);
        assert_eq!(app.world().resource::<PipRepair>().0.as_ref().unwrap().selected, 1);
        press_repair_key(&mut app, KeyCode::Enter);
        assert!(!app.world().resource::<PipRepair>().is_open(), "the one real part was consumed");
        assert!(app.world().resource::<crate::pipboy::KeysTaken>().taken());
        let shown_condition = {
            let active = app.world().resource::<crate::menu::ActiveMenu>();
            let model = active.0.as_ref().unwrap();
            assert_eq!(model.search_text(), "+");
            assert_eq!(model.list(Side::Player).rows.len(), 1);
            let row = model.selected_row(Side::Player).unwrap();
            assert_eq!((row.item, row.mods, row.equipped), (FormId(RIFLE), 2, worn));
            assert!(row.condition.unwrap() > 0.4);
            row.condition
        };
        let mut sim = app.world_mut().resource_mut::<crate::sim::SimState>();
        assert_eq!(count(&mut sim.world, 0x14, RIFLE), 1);
        assert_eq!(count(&mut sim.world, 0x14, PISTOL), 1, "unrelated inventory stays carried");
        let groups: Vec<_> = sim.world.inventory_mut(PLAYER).unwrap().groups().filter(|g| g.0 == FormId(RIFLE)).collect();
        assert_eq!(groups.len(), 1);
        assert_eq!((groups[0].1.mods, groups[0].1.equipped, groups[0].1.condition), (2, worn, shown_condition));
    }
}

#[test]
fn cancelling_real_repair_keeps_search_inventory_and_empty_search_selection_safe() {
    let mut app = filtered_repair_app(true);
    let before = app.world().resource::<crate::sim::SimState>().world.snapshot();
    press_repair_key(&mut app, KeyCode::KeyR);
    assert!(app.world().resource::<PipRepair>().is_open());
    press_repair_key(&mut app, KeyCode::Escape);
    assert!(!app.world().resource::<PipRepair>().is_open());
    assert_eq!(app.world().resource::<crate::sim::SimState>().world.snapshot(), before);
    assert_eq!(app.world().resource::<crate::menu::ActiveMenu>().0.as_ref().unwrap().search_text(), "+");
    search_in_app(&mut app, "no item matches");
    press_repair_key(&mut app, KeyCode::KeyR);
    assert!(
        !app.world().resource::<PipRepair>().is_open(),
        "no filtered selection cannot repair another carried item"
    );
    assert_eq!(app.world().resource::<crate::sim::SimState>().world.snapshot(), before);
}
