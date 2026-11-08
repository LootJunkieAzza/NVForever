use super::*;
use bevy::ecs::system::RunSystemOnce;
use fnv_sim::forms::{ActorBase, BaseData, BaseForm, Item, ItemKind};
use fnv_sim::{FormStore, RefTable, SimConfig};

const AID: FormId = FormId(0x100);

fn sim_with_radaway(count: i32) -> crate::sim::SimState {
    let mut forms = FormStore::default();
    let item = Item {
        kind: ItemKind::Aid, value: Some(1), money: None, weight: Some(0.1), slots: None,
        projectile: None, health: None, non_playable: false, cant_drop: false,
        armor_class: Default::default(), power_armor: false, armor_dr: None, armor_dt: None,
        weapon_damage: None, weight_mods: [0.0; 3], drawable: false, not_for_npcs: false,
        thrown: false, quest_item: false, icon: None, placed_count: None, teaches: None,
        cant_be_taken: false, modulates_voice: false, weapon_traits: None,
    };
    for (id, data) in [
        (AID, BaseData::Item(item)),
        (FormId::PLAYER_BASE, BaseData::Actor(ActorBase { entries: Vec::new(), attributes: Some([5; 7]) })),
    ] {
        forms.bases.insert(id, BaseForm {
            id, sig: if id == AID { *b"ALCH" } else { *b"NPC_" },
            edid: None, name: Some("Test aid".into()), script: None,
            prompt: None, voice_filter: false, data,
        });
    }
    // Synthetic forms; only the lookup's Editor ID is the production action's name.
    forms.edids.insert("radaway".into(), AID);
    let forms = Box::leak(Box::new(forms));
    let refs = Box::leak(Box::new(RefTable::default()));
    let mut world = fnv_sim::World::new(forms, refs, SimConfig::default());
    if count > 0 {
        world.add_item(FormId::PLAYER_REF, AID, count).unwrap();
    }
    crate::sim::SimState { forms, refs, world }
}

/// The Pip-Boy's tab keys, then the page's own [`prepare`] (which routes the view's keys),
/// as the plugin chains them; one frame with nothing pressed has shown the page already.
fn input_app(status: usize, tab: usize) -> App {
    let mut app = App::new();
    app.init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<crate::menu::PromptKeys>()
        .init_resource::<crate::menu::ActiveMenu>()
        .insert_resource(Time::<()>::default())
        .init_resource::<crate::pause::PlayClock>()
        .init_resource::<Prepared>()
        .insert_resource(sim_with_radaway(0))
        .insert_resource(PipBoy { page: Page::Stats, phase: super::super::Phase::Up, tabs: [tab, 0, 0], ..default() })
        .insert_resource(StatsView { status, ..default() })
        .add_message::<super::super::PipBoyInput>()
        .add_message::<UseHint>()
        .add_message::<MouseWheel>()
        .add_systems(Update, (super::super::keys, super::super::apply_input, prepare).chain());
    app.update();
    app
}

#[test]
fn rad_a_uses_one_carried_radaway_and_keeps_status_even_when_empty() {
    for count in [0, 2] {
        let mut app = input_app(1, 0);
        app.insert_resource(sim_with_radaway(count))
            .add_message::<crate::ItemUsed>()
            .add_message::<crate::ui_sounds::UiSound>()
            .add_systems(Update, use_hints.after(prepare));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyA);
        app.update();
        assert_eq!(app.world().resource::<PipBoy>().tab(), 0);
        assert_eq!(app.world().resource::<StatsView>().status, 1);
        let sim = &mut *app.world_mut().resource_mut::<crate::sim::SimState>();
        assert_eq!(sim.world.item_count(FormId::PLAYER_REF, AID).unwrap(), (count - 1).max(0));
        let used: Vec<_> = app.world_mut().resource_mut::<Messages<crate::ItemUsed>>().drain().collect();
        assert_eq!(used.len(), usize::from(count > 0));
        if let Some(used) = used.first() {
            assert_eq!((used.item, used.limb), (AID.0, None));
        }
    }
}

#[test]
fn tab_keys_switch_every_stats_view_that_does_not_own_them() {
    use KeyCode::*;
    for status in 0..3 {
        for tab in 0..5 {
            for (key, by) in [(KeyA, 4), (ArrowLeft, 4), (KeyD, 1), (ArrowRight, 1)] {
                let mut app = input_app(status, tab);
                app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
                app.update();
                let rad = status == 1 && tab == 0 && key == KeyA;
                let expected = if rad { 0 } else { (tab + by) % 5 };
                assert_eq!(app.world().resource::<PipBoy>().tab(), expected, "status {status} tab {tab} key {key:?}");
                // A press that switched the tab is spent: it uses nothing on the new tab
                // (A from S.P.E.C.I.A.L. onto a Status left on RAD, #1899).
                let asks: Vec<_> = app.world_mut().resource_mut::<Messages<UseHint>>().drain().collect();
                let used: &[UseHint] = if rad { &[UseHint { hint: model::RADAWAY, limb: None }] } else { &[] };
                assert_eq!(asks, used, "status {status} tab {tab} key {key:?}");
            }
        }
    }
}

#[test]
fn cnd_pad_x_cycles_all_limbs_and_wraps_to_the_body() {
    let at = crate::pad::MenuAt { code: Some(fnv_game::game::menus::STATS), tab: 0 };
    let key = crate::pad::menu_buttons(at).iter().find(|(b, _)| *b == GamepadButton::West).unwrap().1;
    let mut view = StatsView::default();
    for expected in (0..6).map(Some).chain([None, Some(0)]) {
        let mut input = ButtonInput::default();
        input.press(key);
        assert_eq!(keys(&input, &mut view, 0), None);
        assert_eq!(view.limb, expected);
        assert_eq!(view.aimed(), expected);
        assert_eq!(hint_pressed(&view, model::STIMPAK).unwrap().limb, expected);
    }
    // A pointer left on a limb must not mask the new choice.
    view.hover = Some(Spot::Limb(2));
    view.cycle_limb();
    assert_eq!(view.aimed(), Some(3));
    for (tab, status) in [(0, 1), (0, 2), (1, 0), (2, 0), (3, 0), (4, 0)] {
        let mut view = StatsView { status, ..default() };
        let mut input = ButtonInput::default();
        input.press(key);
        keys(&input, &mut view, tab);
        assert_eq!(view.limb, None);
    }
}

#[test]
fn cnd_draws_three_hints_in_the_existing_slots() {
    // The page lays out in the Pip-Boy screen's own units, as `prepare` draws it (#1631).
    let _canvas = crate::interface::original_units();
    let mut world = World::new();
    world.run_system_once(|mut commands: Commands| {
        let mut kit = Kit::default();
        let mut images = Assets::<Image>::default();
        let mut c = Ctx::new(&mut kit, None, &mut images, None);
        let s = Snapshot { switch_limbs: "Test target".into(), ..default() };
        commands.spawn(Node::default()).with_children(|p| status(p, &mut c, &mut StatsView::default(), &s));
    }).unwrap();
    let mut hints: Vec<_> = world.query::<(&Spot, &Node)>().iter(&world).filter_map(|(spot, node)| {
        if let Spot::Hint(i) = spot { Some((*i, node.top)) } else { None }
    }).collect();
    hints.sort_by_key(|(i, _)| *i);
    assert_eq!(hints.len(), 3);
    let top = |i| hints.iter().find(|(hint, _)| *hint == i).unwrap().1;
    // Spots are placed in interface units (`crate::interface::at`, a part of the view's
    // height), not pixels.
    let (Val::Vh(first), Val::Vh(middle), Val::Vh(last)) = (top(0), top(SWITCH_LIMBS), top(1)) else { panic!("hint positions must be interface units") };
    // The first hint sits in the page's first hint slot, the others one slot below each.
    assert_eq!(Val::Vh(first), crate::interface::u(STATUS.min.y + HINT_Y));
    assert!(first < middle && middle < last);
    assert!((middle - first - (last - middle)).abs() < 1e-4, "{first} {middle} {last}");
    for (labels, expected) in [(crate::pad::Labels::Keys, ["S)", "X)", "E)"]), (crate::pad::Labels::Xbox, ["A)", "X)", "Y)"]), (crate::pad::Labels::PlayStation, ["Cross)", "Square)", "Triangle)"])] {
        crate::pad::set_labels(labels);
        assert_eq!([hint_key(&StatsView::default(), 0), hint_key(&StatsView::default(), SWITCH_LIMBS), hint_key(&StatsView::default(), 1)], expected);
    }
    crate::pad::set_labels(crate::pad::Labels::Keys);
}
