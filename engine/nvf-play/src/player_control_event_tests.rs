//! Authored control-event source handlers through actual binding and player consumers.
//! The movement fixture uses the real collision-free character stepping path; no assets.
use super::*;
use crate::script::ScriptState;
use fnv_game::game::RunCtx;
use fnv_script::vm::XVal;

fn scripts(mut script: ResMut<ScriptState>) {
    script.rt.as_mut().unwrap().game.tick();
}

fn app() -> App {
    let mut app = App::new();
    app.insert_resource(ScriptState::over_no_plugins())
        .insert_resource(State::new(PlayState::Playing))
        .insert_resource(crate::script_draw::tests::play_over(&[]))
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<Bindings>()
        .init_resource::<crate::pad::StickButtons>()
        .init_resource::<PlayerControl>()
        .init_resource::<MoveIntent>()
        // `step` takes the Advanced Movement switches; their default is what ships.
        .init_resource::<crate::inertia::MovementInertia>()
        .init_resource::<crate::furniture::PlayerSeat>()
        .init_resource::<Time>()
        .init_resource::<PhysState>()
        .init_resource::<Status>()
        .init_resource::<Lunge>()
        .insert_resource(NoClip(true))
        .insert_resource(MoveSpeeds { walk: 77.0, run: 308.0 })
        .insert_resource(Player { heading: 0.0, pitch: 0.0, eye_height: 120.0, feet: [0.0; 3], body_eye: None, body_view: None, placed: None })
        .add_message::<crate::rumble::Landed>()
        .add_message::<crate::falls::Fell>()
        .configure_sets(Update, (PlaySet::Input, PlaySet::Physics, PlaySet::Activation, PlaySet::Scripts).chain())
        .add_systems(PreUpdate, crate::pad::update_stick_buttons)
        .add_plugins(crate::script_input::ScriptInputPlugin)
        .add_systems(Update, read_input.in_set(PlaySet::Input))
        .add_systems(Update, step.in_set(PlaySet::Physics))
        .add_systems(Update, crate::control::tell_scripts.after(PlaySet::Input).before(PlaySet::Scripts))
        .add_systems(Update, scripts.in_set(PlaySet::Scripts));
    app.world_mut().resource_mut::<PhysState>().place_player([0.0; 3], 1.0, 1.0);
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let game = &mut script.rt.as_mut().unwrap().game;
        let files = fnv_ui::MemSource::default().with(
            "controls.xml",
            r#"<menu name="HUDMainMenu"><rect name="Controls"><_jd>0</_jd><_ju>0</_ju><_nd>0</_nd><_nu>0</_nu><_other>0</_other><_last>0</_last><string>none</string></rect></menu>"#,
        );
        game.ui_tree.load_menu(&files, "controls.xml").unwrap();
        game.ui_tree.update();
    }
    app
}

fn query(a: &mut App, body: &str) -> XVal {
    a.world_mut().resource_mut::<ScriptState>().rt.as_mut().unwrap().game.run_query(&format!("Begin Function {{}}\n{body}\nEnd"), RunCtx::default()).unwrap()
}

fn count(a: &mut App, field: &str) -> f64 {
    let XVal::Num(n) = query(a, &format!("SetFunctionValue (GetUIFloat \"HUDMainMenu/Controls/{field}\")")) else { panic!("numeric control observation") };
    n
}

fn handlers(a: &mut App, control: u8, feature: bool) {
    for (down, field, jip) in [(true, "_jd", true), (false, "_ju", true), (true, "_nd", false), (false, "_nu", false)] {
        let edge = if down { "Down" } else { "Up" };
        let extra = if feature {
            match (jip, down) {
                (true, true) => "SetAlwaysRun 0 1",
                (true, false) => "SetAlwaysRun 1 1",
                (false, true) => "DisableControl 8",
                (false, false) => "EnableControl 8",
            }
        } else {
            ""
        };
        let context = if jip { "" } else { "SetUIString \"HUDMainMenu/Controls/string\" (GetCurrentEventName)" };
        let lambda = format!(
            "(Begin Function {{int id}}\nSetUIFloat \"HUDMainMenu/Controls/{field}\" ((GetUIFloat \"HUDMainMenu/Controls/{field}\") + 1)\nSetUIFloat \"HUDMainMenu/Controls/_last\" id\n{extra}\n{context}\nEnd)"
        );
        let call = if jip {
            format!("SetOnControl{edge}EventHandler {lambda} 1 {control}")
        } else {
            format!("SetEventHandler \"OnControl{edge}:{control}\" {lambda}")
        };
        query(a, &call);
    }
}

fn tick(a: &mut App) {
    let dt = a.world().resource::<PhysState>().world.params.dt;
    a.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f64(f64::from(dt)));
    a.update();
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
    a.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
}

fn travel(a: &mut App) -> f32 {
    for _ in 0..3 {
        tick(a);
    }
    let y = a.world().resource::<PhysState>().player.as_ref().unwrap().feet[1];
    tick(a);
    a.world().resource::<PhysState>().player.as_ref().unwrap().feet[1] - y
}

#[test]
fn source_control_edges_change_real_walk_run_steps_and_disabled_release_restores_running() {
    let mut a = app();
    handlers(&mut a, 8, true);
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyW);
    let run = travel(&mut a);
    assert!(run > 0.0, "physical character actually travels");
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ControlLeft);
    let walk = travel(&mut a);
    assert!(a.world().resource::<MoveIntent>().walk);
    assert!((run / walk - 4.0).abs() < 0.001, "real run/walk step ratio: {run}/{walk}");
    assert_eq!((count(&mut a, "_jd"), count(&mut a, "_nd"), count(&mut a, "_last")), (1.0, 1.0, 8.0));
    assert!(a.world().resource::<crate::script_input::ScriptBlocked>().keys.contains(&KeyCode::ControlLeft));
    for _ in 0..3 {
        tick(&mut a);
    }
    assert_eq!((count(&mut a, "_jd"), count(&mut a, "_nd")), (1.0, 1.0), "held input is not a new edge");
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::ControlLeft);
    let restored = travel(&mut a);
    assert_eq!((count(&mut a, "_ju"), count(&mut a, "_nu")), (1.0, 1.0));
    assert!(!a.world().resource::<MoveIntent>().walk);
    assert!((restored - run).abs() < 0.001);
    assert!(!a.world().resource::<crate::script_input::ScriptBlocked>().keys.contains(&KeyCode::ControlLeft));
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ControlLeft);
    travel(&mut a);
    assert_eq!((count(&mut a, "_jd"), count(&mut a, "_nd")), (2.0, 2.0), "second entry remains usable");
}

#[test]
fn current_bindings_aggregate_keyboard_mouse_and_rebinding_without_replaying_a_hold() {
    let mut a = app();
    handlers(&mut a, 0, false);
    query(&mut a, "SetEventHandler \"OnControlDown:0\" (Begin Function {int id}\nSetUIFloat \"HUDMainMenu/Controls/_other\" 99\nEnd) \"first\"::1");
    // A different control must not pass the existing JIP filter or colon name.
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyS);
    tick(&mut a);
    assert_eq!((count(&mut a, "_jd"), count(&mut a, "_nd")), (0.0, 0.0));
    query(&mut a, "SetControl 0 48 0\nSetControl 0 257 1\nDisableControl 0");
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyB);
    tick(&mut a);
    assert_eq!((count(&mut a, "_jd"), count(&mut a, "_nd")), (1.0, 1.0));
    a.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Right);
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::KeyB);
    tick(&mut a);
    assert_eq!((count(&mut a, "_ju"), count(&mut a, "_nu")), (0.0, 0.0), "another binding is still held");
    query(&mut a, "SetControl 0 256 1");
    tick(&mut a);
    assert_eq!((count(&mut a, "_ju"), count(&mut a, "_nu")), (1.0, 1.0), "current binding ends the logical hold");
    a.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
    tick(&mut a);
    assert_eq!((count(&mut a, "_jd"), count(&mut a, "_nd")), (2.0, 2.0));
    assert_eq!(count(&mut a, "_other"), 0.0, "named event receives numeric control0 and retains its first-argument filter");
}

#[test]
fn controller_button_and_stick_use_selected_device_and_the_existing_thresholds() {
    let mut a = app();
    let pad = a.world_mut().spawn(Gamepad::default()).id();
    handlers(&mut a, 0, false);
    a.world_mut().resource_mut::<Bindings>().device = crate::bindings::InputDevice::Controller;
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyW);
    tick(&mut a);
    assert_eq!(count(&mut a, "_jd"), 0.0, "unselected keyboard is not a controller press");
    a.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(GamepadAxis::LeftStickY, 1.0);
    tick(&mut a);
    assert_eq!((count(&mut a, "_jd"), count(&mut a, "_nd")), (1.0, 1.0));
    tick(&mut a);
    assert_eq!(count(&mut a, "_jd"), 1.0);
    a.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(GamepadAxis::LeftStickY, 0.0);
    tick(&mut a);
    assert_eq!((count(&mut a, "_ju"), count(&mut a, "_nu")), (1.0, 1.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIString \"HUDMainMenu/Controls/string\")"), XVal::Str("OnControlUp:0".into()));
    assert_eq!(query(&mut a, "SetFunctionValue (GetCurrentEventName)"), XVal::Str(String::new()), "callback context restored");
    // Use the actual configured button for this control, not an invented bit map.
    a.world_mut().resource_mut::<Bindings>().set_pad(Control::Forward, Some(crate::pad::PadInput::Button(GamepadButton::South)));
    a.world_mut().get_mut::<Gamepad>(pad).unwrap().digital_mut().press(GamepadButton::South);
    tick(&mut a);
    assert_eq!(count(&mut a, "_jd"), 2.0);
    a.world_mut().get_mut::<Gamepad>(pad).unwrap().digital_mut().release(GamepadButton::South);
    tick(&mut a);
    assert_eq!(count(&mut a, "_ju"), 2.0);
}

#[test]
fn history_advances_without_handlers_and_both_interfaces_receive_only_the_next_transition() {
    let mut a = app();
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyW);
    tick(&mut a);
    handlers(&mut a, 0, false);
    tick(&mut a);
    assert_eq!((count(&mut a, "_jd"), count(&mut a, "_nd")), (0.0, 0.0));
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::KeyW);
    tick(&mut a);
    assert_eq!((count(&mut a, "_ju"), count(&mut a, "_nu")), (1.0, 1.0));
    a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyW);
    tick(&mut a);
    assert_eq!((count(&mut a, "_jd"), count(&mut a, "_nd")), (1.0, 1.0));
}
