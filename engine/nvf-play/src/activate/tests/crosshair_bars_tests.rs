//! #3233: the Vault 3 captives behind the cell bars (JailCellDoorway01, one collision
//! object on layer 3 Transparent) are talked to through them; what lies behind bars
//! otherwise stays out of reach.
use super::*;
use fnv_phys::HitVolume;

/// Bars: a thin slab 30 units ahead of the eye across the view, on `layer`, owned by a
/// static the simulation does not offer (no placed reference in it, as the bars' STAT).
fn bars(app: &mut App, layer: u8) {
    slab(app, BARS, 30.0, layer);
}

const BARS: u32 = 0x7780;

/// A thin slab of reference `form_id` at `x` across the view, on `layer`.
fn slab(app: &mut App, form_id: u32, x: f32, layer: u8) {
    let slab = CollisionObject { layer, shape: Shape::Box { half_extents: [2.0, 40.0, 80.0] }.into(), ..boxed(10.0) };
    app.world_mut().resource_mut::<PhysState>().world.add_ref(&RefPose { form_id, position: [x, 0.0, 100.0], rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], scale: 1.0 }, &[slab]);
}

/// A living actor's hit capsule standing at `x` on the view line (front face at x - 20).
fn actor_at(app: &mut App, x: f32) {
    let v = HitVolume { feet: [x, 0.0, 0.0], radius: 20.0, height: 130.0, dead: false };
    app.world_mut().resource_mut::<PhysState>().world.set_actor_volume(NPC_REF, v);
}

fn targeted(app: &App) -> Option<(u32, Source, Action)> {
    app.world().resource::<Target>().0.as_ref().map(|t| (t.form_id, t.source, t.action.clone()))
}

#[test]
fn an_actor_behind_transparent_bars_is_talked_to() {
    for layer in [layers::TRANSPARENT, layers::TRANSPARENT_SMALL, layers::TRANSPARENT_SMALL_ANIM] {
        let mut app = scene(&[]);
        bars(&mut app, layer);
        actor_at(&mut app, 120.0);
        app.update();
        assert_eq!(targeted(&app), Some((NPC_REF, Source::Collision, Action::Talk)), "layer {layer}");
        let t = app.world().resource::<Target>().0.clone().unwrap();
        assert!((t.distance - 100.0).abs() < 1.0, "the actor's capsule, not the bars: {}", t.distance);
        assert_eq!(t.through, Some(BARS), "picked through the bars");
    }
}

#[test]
fn opaque_bars_reach_and_a_wall_between_still_hide_the_actor() {
    // Bars on a solid layer hide the actor as before.
    let mut app = scene(&[]);
    bars(&mut app, layers::STATIC);
    actor_at(&mut app, 120.0);
    app.update();
    assert_eq!(targeted(&app), None);
    // Beyond reach behind the bars: nothing.
    let mut app = scene(&[]);
    bars(&mut app, layers::TRANSPARENT);
    actor_at(&mut app, 190.0);
    app.update();
    assert_eq!(targeted(&app), None);
    // A wall (any reference's solid box) between the bars and the actor hides it.
    let mut app = scene(&[(0x999, 70.0)]);
    bars(&mut app, layers::TRANSPARENT);
    actor_at(&mut app, 120.0);
    app.update();
    assert_eq!(targeted(&app), None);
}

#[test]
fn items_and_containers_behind_transparent_bars_stay_out_of_reach() {
    // A chest by its collision behind the bars: not opened through them.
    let mut app = scene(&[(CHEST_REF, 100.0)]);
    bars(&mut app, layers::TRANSPARENT);
    app.update();
    assert_eq!(targeted(&app), None);
    // A stimpak by its render box behind the bars: not taken through them.
    let mut app = scene(&[]);
    bars(&mut app, layers::TRANSPARENT);
    visual(&mut app, STIM_REF, [60.0, 0.0, 100.0]);
    app.update();
    assert_eq!(targeted(&app), None);
    // In front of the bars it still is.
    let mut app = scene(&[]);
    bars(&mut app, layers::TRANSPARENT);
    visual(&mut app, STIM_REF, [20.0, 0.0, 100.0]);
    actor_at(&mut app, 120.0);
    app.update();
    assert_eq!(targeted(&app).map(|t| (t.0, t.1)), Some((STIM_REF, Source::Bounds)));
}

#[test]
fn e_on_the_actor_behind_the_bars_talks() {
    let mut app = scene(&[]);
    bars(&mut app, layers::TRANSPARENT);
    actor_at(&mut app, 120.0);
    app.update();
    press_e(&mut app);
    // Without the scripts Talk is a line saying the actor has nothing to say
    // (`outcome::effects`): the activation went to the actor, not the bars.
    assert!(app.world().resource::<HudMessage>().text().contains("Rick Lancer"), "{}", app.world().resource::<HudMessage>().text());
}

#[test]
fn a_closed_door_on_a_see_through_layer_behind_the_bars_hides_the_actor() {
    // 0x777B: a door without a teleport (it opens in place), its slab on layer 28.
    const CELL_DOOR: u32 = 0x777B;
    let mut app = scene(&[]);
    bars(&mut app, layers::TRANSPARENT);
    slab(&mut app, CELL_DOOR, 70.0, layers::TRANSPARENT_SMALL_ANIM);
    actor_at(&mut app, 120.0);
    app.update();
    assert_eq!(targeted(&app), None, "the closed door hides the actor");
    // Opened, it no longer does.
    app.world_mut().resource_mut::<SimState>().world.activate(FormId::PLAYER_REF, FormId(CELL_DOOR)).unwrap();
    assert!(app.world().resource::<SimState>().world.peek(FormId(CELL_DOOR)).is_some_and(|s| s.open));
    app.update();
    assert_eq!(targeted(&app), Some((NPC_REF, Source::Collision, Action::Talk)));
}

/// With the scripts running, E on a teammate opens the companion wheel (the control);
/// through the bars it only tries the conversation: no wheel (nor perks' choices or
/// OnActivate, which go through the same `script::by_hand` path).
#[test]
fn through_the_bars_e_only_talks_even_to_a_teammate() {
    let teammate = |app: &mut App| {
        let mut script = crate::script::ScriptState::over_no_plugins();
        script.rt.as_mut().unwrap().game.extra.entry(FormId(NPC_REF)).or_default().teammate = true;
        app.insert_resource(script);
    };
    let wheel = |app: &App| app.world().resource::<crate::script::ScriptState>().rt.as_ref().unwrap().game.companions.wheel;
    // No bars: the wheel opens.
    let mut app = scene(&[]);
    teammate(&mut app);
    actor_at(&mut app, 120.0);
    app.update();
    assert_eq!(app.world().resource::<Target>().0.as_ref().map(|t| (t.form_id, t.through)), Some((NPC_REF, None)));
    press_e(&mut app);
    assert_eq!(wheel(&app), Some(FormId(NPC_REF)), "control: the scripted activation opens the wheel");
    // Behind the bars: no wheel; with no GREETING in this load order, nothing to say.
    let mut app = scene(&[]);
    teammate(&mut app);
    bars(&mut app, layers::TRANSPARENT);
    actor_at(&mut app, 120.0);
    app.update();
    assert_eq!(app.world().resource::<Target>().0.as_ref().map(|t| (t.form_id, t.action.clone(), t.through)), Some((NPC_REF, Action::Talk, Some(BARS))));
    press_e(&mut app);
    assert_eq!(wheel(&app), None, "through the bars the wheel does not open");
    assert!(app.world().resource::<HudMessage>().text().ends_with("has nothing to say"), "{}", app.world().resource::<HudMessage>().text());
}
