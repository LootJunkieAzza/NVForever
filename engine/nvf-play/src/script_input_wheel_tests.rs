//! Authored physical-message -> published-script-API controls. No OS input/window.
use super::*;
use bevy::input::mouse::MouseScrollUnit;
use crate::script::ScriptState;
use fnv_game::game::RunCtx;
use fnv_script::vm::XVal;

fn tick(mut state: Option<ResMut<ScriptState>>) {
    if let Some(rt) = state.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        rt.game.tick();
    }
}

fn app() -> (App, Entity) {
    let mut app = App::new();
    app.init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ScriptBlocked>()
        .add_message::<MouseWheel>()
        .insert_resource(ScriptState::over_no_plugins())
        .add_systems(Update, (script_input, tick).chain());
    let window = app.world_mut().spawn(PrimaryWindow).id();
    (app, window)
}

fn wheel(app: &mut App, window: Entity, unit: MouseScrollUnit, x: f32, y: f32) {
    app.world_mut().write_message(MouseWheel { unit, x, y, window, phase: bevy::input::touch::TouchPhase::Moved });
}

fn call(app: &mut App, name: &str, args: &[f64]) -> XVal {
    let mut state = app.world_mut().resource_mut::<ScriptState>();
    let game = &mut state.rt.as_mut().unwrap().game;
    let args: Vec<_> = args.iter().map(|&n| XVal::Num(n)).collect();
    fnv_game::nvse::call(game, &RunCtx::default(), name, None, &args)
        .expect("published extender input API")
        .expect("valid synthetic input arguments")
}

fn pressed(app: &mut App, code: u16) -> XVal {
    call(app, "iskeypressed", &[f64::from(code)])
}

#[test]
fn actual_line_and_pixel_messages_publish_direction_for_one_frame() {
    let (mut app, window) = app();
    wheel(&mut app, window, MouseScrollUnit::Line, 0.0, 3.0);
    app.update();
    assert_eq!(pressed(&mut app, 264), XVal::Num(1.0));
    assert_eq!(pressed(&mut app, 265), XVal::Num(0.0));
    app.update();
    assert_eq!(pressed(&mut app, 264), XVal::Num(0.0));
    wheel(&mut app, window, MouseScrollUnit::Pixel, 0.0, -0.25);
    app.update();
    assert_eq!(pressed(&mut app, 264), XVal::Num(0.0));
    assert_eq!(pressed(&mut app, 265), XVal::Num(1.0));
    app.update();
    assert_eq!(pressed(&mut app, 265), XVal::Num(0.0));
}

#[test]
fn wheel_direction_changes_preserve_keyboard_and_all_eight_physical_buttons() {
    let (mut app, window) = app();
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyW);
    for b in [MouseButton::Left, MouseButton::Right, MouseButton::Middle,
        MouseButton::Back, MouseButton::Forward, MouseButton::Other(5),
        MouseButton::Other(6), MouseButton::Other(7)] {
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(b);
    }
    for (delta, up, down) in [(1.0, 1.0, 0.0), (-1.0, 0.0, 1.0), (0.0, 0.0, 0.0)] {
        wheel(&mut app, window, MouseScrollUnit::Line, 0.0, delta);
        app.update();
        assert_eq!(pressed(&mut app, 17), XVal::Num(1.0));
        for code in 256..264 { assert_eq!(pressed(&mut app, code), XVal::Num(1.0)); }
        assert_eq!(pressed(&mut app, 264), XVal::Num(up));
        assert_eq!(pressed(&mut app, 265), XVal::Num(down));
    }
}

fn install_edges(app: &mut App) {
    let mut state = app.world_mut().resource_mut::<ScriptState>();
    let game = &mut state.rt.as_mut().unwrap().game;
    let files = fnv_ui::MemSource::default().with("owned.xml", r#"<menu name="HUDMainMenu"><rect name="Probe"><_downs>0</_downs><_ups>0</_ups><_last>0</_last></rect></menu>"#);
    game.ui_tree.load_menu(&files, "owned.xml").unwrap();
    for (event, field) in [("SetOnKeyDownEventHandler", "_downs"), ("SetOnKeyUpEventHandler", "_ups")] {
        let source = format!("Begin Function {{}}\nSetFunctionValue ({event} (Begin Function {{int key}}\nSetUIFloat \"HUDMainMenu/Probe/{field}\" ((GetUIFloat \"HUDMainMenu/Probe/{field}\") + 1)\nSetUIFloat \"HUDMainMenu/Probe/_last\" key\nEnd) 1 264)\nEnd");
        assert_eq!(game.run_query(&source, RunCtx::default()).unwrap(), XVal::Num(1.0));
    }
}

fn ui_count(app: &mut App, field: &str) -> XVal {
    let mut state = app.world_mut().resource_mut::<ScriptState>();
    let source = format!("Begin Function {{}}\nSetFunctionValue (GetUIFloat \"HUDMainMenu/Probe/{field}\")\nEnd");
    state.rt.as_mut().unwrap().game.run_query(&source, RunCtx::default()).unwrap()
}

#[test]
fn disabled_getter_policy_and_real_tick_key_edges_keep_their_existing_semantics() {
    let (mut app, window) = app();
    install_edges(&mut app);
    call(&mut app, "disablekey", &[264.0]);
    wheel(&mut app, window, MouseScrollUnit::Line, 0.0, 1.0);
    app.update();
    assert_eq!(pressed(&mut app, 264), XVal::Num(0.0));
    assert_eq!(call(&mut app, "iskeypressed", &[264.0, 2.0]), XVal::Num(1.0));
    assert_eq!(ui_count(&mut app, "_downs"), XVal::Num(1.0));
    assert_eq!(ui_count(&mut app, "_ups"), XVal::Num(0.0));
    // A consecutive same-direction frame remains held, not a fabricated new edge.
    wheel(&mut app, window, MouseScrollUnit::Line, 0.0, 2.0);
    app.update();
    assert_eq!(ui_count(&mut app, "_downs"), XVal::Num(1.0));
    app.update();
    assert_eq!(ui_count(&mut app, "_ups"), XVal::Num(1.0));
    assert_eq!(call(&mut app, "iskeypressed", &[264.0, 2.0]), XVal::Num(0.0));
    wheel(&mut app, window, MouseScrollUnit::Line, 0.0, 1.0);
    app.update();
    assert_eq!(ui_count(&mut app, "_downs"), XVal::Num(2.0));
    assert_eq!(ui_count(&mut app, "_last"), XVal::Num(264.0));
}

#[test]
fn hostile_or_foreign_deltas_and_pre_runtime_messages_do_not_replay() {
    let (mut app, window) = app();
    let foreign = app.world_mut().spawn_empty().id();
    for (target, x, y) in [(window, 9.0, 0.0), (window, 0.0, f32::NAN),
        (window, 0.0, f32::INFINITY), (foreign, 0.0, 1.0)] {
        wheel(&mut app, target, MouseScrollUnit::Pixel, x, y);
    }
    app.update();
    assert_eq!(pressed(&mut app, 264), XVal::Num(0.0));
    assert_eq!(pressed(&mut app, 265), XVal::Num(0.0));
    let state = app.world_mut().remove_resource::<ScriptState>().unwrap();
    wheel(&mut app, window, MouseScrollUnit::Line, 0.0, 1.0);
    app.update();
    app.world_mut().insert_resource(state);
    app.update();
    assert_eq!(pressed(&mut app, 264), XVal::Num(0.0));
    app.world_mut().entity_mut(window).remove::<PrimaryWindow>();
    wheel(&mut app, window, MouseScrollUnit::Line, 0.0, -1.0);
    app.update();
    app.world_mut().entity_mut(window).insert(PrimaryWindow);
    app.update();
    assert_eq!(pressed(&mut app, 265), XVal::Num(0.0));
    wheel(&mut app, window, MouseScrollUnit::Line, 0.0, 1.0);
    wheel(&mut app, window, MouseScrollUnit::Line, 0.0, -1.0);
    app.update();
    assert_eq!(pressed(&mut app, 264), XVal::Num(1.0));
    assert_eq!(pressed(&mut app, 265), XVal::Num(1.0));
}
