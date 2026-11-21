// Offscreen controls for the production plugin's actual schedule registration.
// Only the native operation/readiness are injected at the production consumer.

#[derive(Resource)]
struct ScheduledNative {
    backend: CursorPositionTestBackend,
    calls: usize,
    reject_next: bool,
}

#[derive(Resource, Default)]
struct ScheduledControl {
    source: Option<&'static str>,
    ui_transition: u8,
}

#[derive(Debug)]
struct ScheduledObservation {
    pixels: Option<(f64, f64)>,
    ui: Option<(f64, f64)>,
    receipt: Option<u64>,
    source_x: f64,
}

#[derive(Resource, Default)]
struct ScheduledObservations(Vec<ScheduledObservation>);

fn scheduled_source_callback(mut script: ResMut<ScriptState>, mut observations: ResMut<ScheduledObservations>) {
    let game = &mut script.rt.as_mut().unwrap().game;
    let value = game.run_query("Begin Function {}\nSetFunctionValue (GetCursorPos X 1)\nEnd", RunCtx::default()).unwrap();
    let fnv_script::vm::XVal::Num(source_x) = value else { panic!("source cursor query must return a number") };
    observations.0.push(ScheduledObservation { pixels: game.other.jip9.cursor_px,
        ui: game.ui_pointer.at, receipt: game.other.jip9.cursor_warp_receipt, source_x });
}

fn scheduled_scripts(mut script: ResMut<ScriptState>, mut control: ResMut<ScheduledControl>) {
    if let Some(source) = control.source.take() {
        script.rt.as_mut().unwrap().game.run_query(&format!("Begin Function {{}}\n{source}\nEnd"), RunCtx::default()).unwrap();
    }
}

fn scheduled_ui(mut script: ResMut<ScriptState>, mut control: ResMut<ScheduledControl>) {
    let transition = std::mem::take(&mut control.ui_transition);
    let game = &mut script.rt.as_mut().unwrap().game;
    match transition {
        1 => { game.close_menu(); game.open_menu(menus::MESSAGE); }
        2 => { game.run_query("Begin Function {}\nSetCursorPos 200 160 1\nEnd", RunCtx::default()).unwrap(); }
        _ => {}
    }
}

fn scheduled_native(mut native: ResMut<ScheduledNative>,
    mut windows: Query<(&mut Window, &mut CursorPositionRequest), With<PrimaryWindow>>) {
    for (mut window, mut request) in &mut windows {
        let facts = CursorPositionReadiness { present: true, focused: true, visible: true, minimized: false,
            physical_size: UVec2::new(window.physical_width(), window.physical_height()), cursor_inside: Some(true) };
        let result = if native.reject_next { Err("scheduled native rejection") } else { Ok(()) };
        let mut calls = 0;
        native.backend.apply(&mut window, &mut request, facts, |_| { calls += 1; result });
        native.calls += calls;
        if calls > 0 { native.reject_next = false; }
    }
}

fn scheduled_app() -> (App, Entity) {
    let (mut app, window) = app();
    let native = backend(&app, window);
    app.add_plugins((bevy::input::InputPlugin, crate::script_input::ScriptInputPlugin,
            crate::script_switches::ScriptSwitchesPlugin))
        .configure_sets(Update, (crate::PlaySet::Scripts, crate::PlaySet::Ui).chain())
        .insert_resource(ScheduledNative { backend: native, calls: 0, reject_next: false })
        .init_resource::<ScheduledControl>()
        .init_resource::<ScheduledObservations>()
        // This callback shares the actual production input set, so moving refresh
        // after that set makes its next-frame source observation fail.
        .add_systems(PreUpdate, scheduled_source_callback.in_set(crate::script_input::ScriptInputSet))
        .add_systems(Update, scheduled_scripts.in_set(crate::PlaySet::Scripts))
        .add_systems(Update, scheduled_ui.in_set(crate::PlaySet::Ui))
        .add_systems(Last, scheduled_native);
    (app, window)
}

#[test]
fn production_schedule_retires_ui_owner_changes_before_any_native_attempt() {
    for transition in [1, 2] {
        let (mut app, window) = scheduled_app();
        {
            let mut control = app.world_mut().resource_mut::<ScheduledControl>();
            control.source = Some("SetCursorPos 100 80 1");
            control.ui_transition = transition;
        }
        app.update();
        assert_eq!(app.world().resource::<ScheduledNative>().calls, 0,
            "the UI-stage close/reopen or superseding request must retire old intent before Last");
        assert!(app.world().get::<CursorPositionRequest>(window).is_none());
        assert_eq!(app.world().get::<Window>(window).unwrap().cursor_position(), Some(Vec2::new(20.0, 30.0)));
        if transition == 1 {
            app.world_mut().resource_mut::<ScheduledControl>().source = Some("SetCursorPos 200 160 1");
        }
        app.update();
        assert_eq!(app.world().resource::<ScheduledNative>().calls, 1, "fresh intent remains usable after lifecycle cancellation");
        assert_eq!(app.world().get::<Window>(window).unwrap().cursor_position(), Some(Vec2::new(150.0, 120.0)));
    }
}

#[test]
fn production_schedule_exposes_ack_before_source_callbacks_and_update_retirement() {
    let (mut app, window) = scheduled_app();
    app.world_mut().resource_mut::<ScriptState>().rt.as_mut().unwrap().game.ui_pointer.at = Some((1.0, 2.0));
    app.world_mut().resource_mut::<ScheduledControl>().source = Some("SetCursorPos 100 80 1");
    app.world_mut().resource_mut::<ScheduledNative>().reject_next = true;
    app.update();
    assert_eq!(app.world().resource::<ScheduledNative>().calls, 1);
    assert_eq!(app.world().get::<Window>(window).unwrap().cursor_position(), Some(Vec2::new(20.0, 30.0)));
    app.update();
    assert_eq!(app.world().resource::<ScheduledNative>().calls, 2);
    assert_eq!(app.world().get::<Window>(window).unwrap().cursor_position(), Some(Vec2::new(75.0, 60.0)));
    {
        let observations = app.world().resource::<ScheduledObservations>();
        let rejected = observations.0.last().unwrap();
        assert_eq!(rejected.pixels, Some((20.0, 30.0)));
        assert_eq!(rejected.source_x, 1.0, "rejection cannot publish the requested XML hit to the next source callback");
    }
    app.update();
    let observations = app.world().resource::<ScheduledObservations>();
    let acknowledged = observations.0.last().unwrap();
    assert_eq!(acknowledged.pixels, Some((75.0, 60.0)));
    assert_eq!(acknowledged.ui, Some((100.0, 80.0)));
    assert_eq!(acknowledged.source_x, 100.0, "actual PreUpdate source query sees native acknowledgement before Update");
    assert!(acknowledged.receipt.is_some(), "the observation precedes production Update receipt retirement");
    assert!(app.world().get::<CursorPositionRequest>(window).is_none());
    assert_eq!(app.world().resource::<ScheduledNative>().calls, 2, "acknowledgement does not cause a second successful native warp");
}
