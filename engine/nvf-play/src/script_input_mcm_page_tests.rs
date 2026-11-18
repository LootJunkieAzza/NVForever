//! Authored page input consumers; docs/spec-notes/mcm-page-key-events.md.
use super::*;
use bevy::input::mouse::MouseScrollUnit;
use crate::script::ScriptState;
use crate::tiles::mods::{self, ModsPage};
use fnv_game::game::{menus, RunCtx};
use fnv_script::vm::XVal;

fn page(mut state: ResMut<ScriptState>, mut page: ResMut<ModsPage>) {
    mods::tick(&mut state.rt.as_mut().unwrap().game, &mut page);
}

fn app() -> (App, Entity) {
    let mut app = App::new();
    app.init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ScriptBlocked>()
        .init_resource::<ModsPage>()
        .add_message::<MouseWheel>()
        .insert_resource(ScriptState::over_no_plugins())
        .add_systems(Update, (script_input, page).chain());
    let window = app.world_mut().spawn(PrimaryWindow).id();
    {
        let mut state = app.world_mut().resource_mut::<ScriptState>();
        let game = &mut state.rt.as_mut().unwrap().game;
        let files = fnv_ui::MemSource::default().with("owned.xml", r#"
      <menu name="StartMenu"><rect name="MCM"><_Menu>1</_Menu>
        <rect name="MCM_Options"/><rect name="MCM_Scale"/>
        <rect name="MCM_List"/><rect name="MCM_Trigger"/>
        <rect name="MCM_ModList"><_offset>1</_offset><_visible>3</_visible>
          <_count>6</_count><_searchKeys>0</_searchKeys><_released>0</_released><_allDown>0</_allDown><_warpSelections>0</_warpSelections>
          <x>50</x><y>20</y><width>150</width><height>200</height>
        </rect></rect></menu>"#);
        game.ui_tree.load_menu(&files, "owned.xml").unwrap();
        let source = "Begin Function {}\nSetFunctionValue (GetMCMFloat 5 0 \"_offset\")\nEnd";
        assert_eq!(game.run_query(source, RunCtx::default()).unwrap(), XVal::Num(1.0), "the source API reaches the fifth child before any input");
        // Independent consumers of the same documented key-event and MCM tile APIs;
        // none of the installed mod's scripts or XML are test fixtures.
        let handler = r#"Begin Function {int key}
      if MenuMode 1013 == 0
        return
      endif
      if key == 264
        if GetMCMFloat 5 0 "_offset" > 0
          SetMCMFloat 5 0 "_offset" ((GetMCMFloat 5 0 "_offset") - 1)
        endif
      elseif key == 265
        if (GetMCMFloat 5 0 "_offset") + (GetMCMFloat 5 0 "_visible") < GetMCMFloat 5 0 "_count"
          SetMCMFloat 5 0 "_offset" ((GetMCMFloat 5 0 "_offset") + 1)
        endif
      else
        SetMCMFloat 5 0 "_searchKeys" ((GetMCMFloat 5 0 "_searchKeys") + 1)
      endif
    End"#;
        for code in [264, 265, 33] {
            let source = format!("Begin Function {{}}\nSetFunctionValue (SetOnKeyDownEventHandler ({handler}) 1 {code})\nEnd");
            assert_eq!(game.run_query(&source, RunCtx::default()).unwrap(), XVal::Num(1.0));
        }
        let source = r#"Begin Function {}
      SetFunctionValue (SetOnKeyUpEventHandler (Begin Function {int key}
        if MenuMode 1013
          SetMCMFloat 5 0 "_released" key
        endif
      End) 1 33)
    End"#;
        assert_eq!(game.run_query(source, RunCtx::default()).unwrap(), XVal::Num(1.0));
        let source = r#"Begin Function {}
      SetFunctionValue (SetOnKeyDownEventHandler (Begin Function {int key}
        SetMCMFloat 5 0 "_allDown" ((GetMCMFloat 5 0 "_allDown") + 1)
      End) 1 33)
    End"#;
        assert_eq!(game.run_query(source, RunCtx::default()).unwrap(), XVal::Num(1.0));
    }
    (app, window)
}

fn wheel(app: &mut App, window: Entity, y: f32) {
    app.world_mut().write_message(MouseWheel {
        unit: MouseScrollUnit::Line, x: 0.0, y, window,
        phase: bevy::input::touch::TouchPhase::Moved,
    });
}

fn value(app: &App, field: &str) -> f64 {
    app.world().resource::<ScriptState>().rt.as_ref().unwrap().game.ui_tree
        .get_float(&format!("StartMenu/MCM/MCM_ModList/{field}")).unwrap()
}

#[test]
fn mcm_page_wheel_reaches_source_registered_list_handler_and_respects_edges() {
    let (mut app, window) = app();
    wheel(&mut app, window, 1.0);
    app.update();
    assert_eq!(value(&app, "_offset"), 0.0);
    // A fresh down direction is an edge even without an empty frame between.
    wheel(&mut app, window, -1.0);
    app.update();
    assert_eq!(value(&app, "_offset"), 1.0);
    wheel(&mut app, window, -1.0);
    app.update();
    assert_eq!(value(&app, "_offset"), 1.0, "held direction is not another down edge");
    for expected in [2.0, 3.0, 3.0] {
        app.update();
        wheel(&mut app, window, -1.0);
        app.update();
        assert_eq!(value(&app, "_offset"), expected, "list bounds stay consumer-owned");
    }
    let game = &app.world().resource::<ScriptState>().rt.as_ref().unwrap().game;
    assert_eq!(game.time, 0.0, "page input does not run the world");
    assert!(!game.menu_open(menus::START), "temporary page context is restored");
}

#[test]
fn mcm_page_search_gets_disabled_key_down_up_and_no_replay_after_reentry() {
    let (mut app, _) = app();
    app.world_mut().resource_mut::<ScriptState>().rt.as_mut().unwrap().game.input.disabled_keys.insert(33);
    // A box already over the held world stays there through the page's temporary
    // start-menu context and input dispatch.
    app.world_mut().resource_mut::<ScriptState>().rt.as_mut().unwrap().game.menu = Some(menus::MESSAGE);
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyF);
    app.update();
    assert_eq!(value(&app, "_searchKeys"), 1.0);
    app.update();
    assert_eq!(value(&app, "_searchKeys"), 1.0);
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::KeyF);
    app.update();
    assert_eq!(value(&app, "_released"), 33.0);
    app.world_mut().insert_resource(ModsPage::default());
    app.update();
    assert_eq!(value(&app, "_searchKeys"), 1.0);
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyF);
    app.update();
    assert_eq!(value(&app, "_searchKeys"), 2.0);
    assert_eq!(value(&app, "_allDown"), 2.0);
    // The regular frame uses the same edge tracker, so leaving the page while held
    // cannot redeliver the already-consumed key to a handler in the next frame.
    let mut state = app.world_mut().resource_mut::<ScriptState>();
    let game = &mut state.rt.as_mut().unwrap().game;
    game.tick();
    assert_eq!(game.ui_tree.get_float("StartMenu/MCM/MCM_ModList/_searchKeys"), Some(2.0));
    assert_eq!(game.ui_tree.get_float("StartMenu/MCM/MCM_ModList/_allDown"), Some(2.0));
    assert_eq!(game.menu, Some(menus::MESSAGE));
}

#[test]
fn mcm_page_wheel_routes_by_menu_coordinates_and_admits_callback_cursor_writes() {
    let (mut app, window) = app();
    {
        let mut state = app.world_mut().resource_mut::<ScriptState>();
        let game = &mut state.rt.as_mut().unwrap().game;
        // Raw cursor outside the list; actual menu pointer inside, after the menu
        // frame/UI-scale transform performed by tile input.
        game.other.jip9.window = Some((1280.0, 960.0));
        game.other.jip9.cursor_px = Some((700.0, 500.0));
        game.ui_pointer.at = Some((100.0, 80.0));
        let source = r#"Begin Function {}
          SetFunctionValue (SetOnKeyDownEventHandler (Begin Function {int key}
            if MenuMode 1013 && GetActiveMenuMode == 1013
              if GetCursorPos X 1 > GetMCMFloat 5 0 "x" && GetCursorPos X 1 < (GetMCMFloat 5 0 "x") + (GetMCMFloat 5 0 "width")
                if GetCursorPos Y 1 > GetMCMFloat 5 0 "y" && GetCursorPos Y 1 < (GetMCMFloat 5 0 "y") + (GetMCMFloat 5 0 "height")
                  SetMCMFloat 5 0 "_offset" ((GetMCMFloat 5 0 "_offset") + 1)
                  SetCursorPos 110 90 1
                endif
              endif
            endif
          End) 1 265)
        End"#;
        assert_eq!(game.run_query(source, RunCtx::default()).unwrap(), XVal::Num(1.0));
    }
    wheel(&mut app, window, -1.0);
    app.update();
    assert_eq!(value(&app, "_offset"), 3.0, "both source callbacks routed to the list");
    {
        let state = app.world().resource::<ScriptState>();
        let game = &state.rt.as_ref().unwrap().game;
        assert_eq!(game.other.jip9.cursor_warp, Some((110.0, 90.0, true)));
        assert_eq!(game.ui_pointer.at, Some((100.0, 80.0)), "the engine consumes the pending warp later");
        assert_eq!(game.menu, None);
    }
    // Leaving the callback's page context restores plain-play cursor policy. A
    // stale menu pointer cannot replace raw pixels or authorize a new warp.
    {
        let mut state = app.world_mut().resource_mut::<ScriptState>();
        let game = &mut state.rt.as_mut().unwrap().game;
        let query = "Begin Function {}\nSetCursorPos 1 2 1\nSetFunctionValue (GetCursorPos X 1)\nEnd";
        assert_eq!(game.run_query(query, RunCtx::default()).unwrap(), XVal::Num(110.0));
        assert_eq!(game.other.jip9.cursor_warp, Some((110.0, 90.0, true)));
        let query = "Begin Function {}\nSetFunctionValue (GetActiveMenuMode)\nEnd";
        assert_eq!(game.run_query(query, RunCtx::default()).unwrap(), XVal::Num(0.0));
    }
    app.update(); // release the wheel before another edge under an overlay
    {
        let mut state = app.world_mut().resource_mut::<ScriptState>();
        let game = &mut state.rt.as_mut().unwrap().game;
        game.menu = Some(menus::MESSAGE);
        game.other.jip9.cursor_warp = None;
    }
    wheel(&mut app, window, -1.0);
    app.update();
    assert_eq!(value(&app, "_offset"), 3.0, "active-menu-gated list handler stays held under a message");
    let state = app.world().resource::<ScriptState>();
    let game = &state.rt.as_ref().unwrap().game;
    assert_eq!(game.menu, Some(menus::MESSAGE));
    assert_eq!(game.other.jip9.cursor_warp, None);
}

#[test]
fn mcm_page_warp_reaches_window_then_menu_input_and_source_list_routing() {
    use bevy::ecs::system::RunSystemOnce;
    for (scale, expected) in [(0.5, Vec2::new(357.5, 210.0)), (0.75, Vec2::new(216.25, 135.0)), (1.0, Vec2::new(75.0, 60.0))] {
        let _units = crate::interface::units_at(scale);
        let (mut app, window) = app();
        app.init_resource::<crate::script_switches::ScriptSwitches>();
        let mut actual = Window::default();
        actual.resolution.set(1280.0, 720.0);
        app.world_mut().entity_mut(window).insert(actual);
        {
            let mut state = app.world_mut().resource_mut::<ScriptState>();
            let game = &mut state.rt.as_mut().unwrap().game;
            let source = r#"Begin Function {}
              SetFunctionValue (SetOnKeyDownEventHandler (Begin Function {int key}
                if GetActiveMenuMode == 1013
                  SetCursorPos 100 80 1
                endif
              End) 1 265)
            End"#;
            assert_eq!(game.run_query(source, RunCtx::default()).unwrap(), XVal::Num(1.0));
            let source = r#"Begin Function {}
              SetFunctionValue (SetOnKeyDownEventHandler (Begin Function {int key}
                if GetActiveMenuMode == 1013
                  if GetCursorPos X 1 > GetMCMFloat 5 0 "x" && GetCursorPos X 1 < (GetMCMFloat 5 0 "x") + (GetMCMFloat 5 0 "width")
                    if GetCursorPos Y 1 > GetMCMFloat 5 0 "y" && GetCursorPos Y 1 < (GetMCMFloat 5 0 "y") + (GetMCMFloat 5 0 "height")
                      SetMCMFloat 5 0 "_warpSelections" ((GetMCMFloat 5 0 "_warpSelections") + 1)
                    endif
                  endif
                endif
              End) 1 33)
            End"#;
            assert_eq!(game.run_query(source, RunCtx::default()).unwrap(), XVal::Num(1.0));
        }
        wheel(&mut app, window, -1.0);
        app.update();
        assert_eq!(app.world().resource::<ScriptState>().rt.as_ref().unwrap().game.menu, None);
        // Execute this same production consumer on the scope's thread, so the
        // menu scale is isolated from other tests without mutating global settings.
        app.world_mut().run_system_once(crate::script_switches::sync).unwrap();
        let w = app.world().get::<Window>(window).unwrap();
        let at = w.cursor_position().expect("actual window cursor warp consumed");
        assert!((at - expected).length() < 0.01, "scale {scale}: {at:?}");
        let menu_at = crate::tiles::input::to_menu_units(at, Vec2::new(w.width(), w.height()), crate::interface::MIDDLE);
        {
            let mut state = app.world_mut().resource_mut::<ScriptState>();
            let game = &mut state.rt.as_mut().unwrap().game;
            let menu = game.ui_tree.menu("StartMenu").unwrap();
            fnv_game::ui_input::step(game, &[menu], &fnv_ui::input::Input { at: Some(menu_at), ..Default::default() }, &fnv_ui::input::no_measure);
            assert!(game.other.jip9.cursor_warp.is_none());
            assert!(!game.other.jip9.cursor_warp_start_menu);
        }
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyF);
        app.update();
        assert_eq!(value(&app, "_warpSelections"), 1.0, "the subsequent physical handler reaches the list at scale {scale}");
        // Raw coordinates in StartMenu still use window pixels.
        {
            let mut state = app.world_mut().resource_mut::<ScriptState>();
            let game = &mut state.rt.as_mut().unwrap().game;
            game.menu = Some(menus::START);
            game.run_query("Begin Function {}\nSetCursorPos 300 200 0\nEnd", RunCtx::default()).unwrap();
        }
        app.world_mut().run_system_once(crate::script_switches::sync).unwrap();
        assert_eq!(app.world().get::<Window>(window).unwrap().cursor_position(), Some(Vec2::new(300.0, 200.0)));
        // The HUD cursor has no menu frame, including under a smaller menu scale.
        {
            let mut state = app.world_mut().resource_mut::<ScriptState>();
            let game = &mut state.rt.as_mut().unwrap().game;
            game.menu = None;
            game.other.jip9.hud_cursor = true;
            game.run_query("Begin Function {}\nSetCursorPos 100 80 1\nEnd", RunCtx::default()).unwrap();
        }
        app.world_mut().run_system_once(crate::script_switches::sync).unwrap();
        assert_eq!(app.world().get::<Window>(window).unwrap().cursor_position(), Some(Vec2::new(75.0, 60.0)));
    }
}
