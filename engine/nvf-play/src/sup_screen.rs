//! SUP batch 6's engine side (`fnv_game::nvse_sup6`; docs/spec-notes/ext-sup6.md): what
//! only the engine knows goes into `Game::sup` each frame before the scripts (the mouse,
//! the window's size, the fonts' heights, the lockpick menu, how long the throw is
//! held), and the scripts' requests come back after them: FakeMouseMovement's moves (as
//! mouse movement of the next frame), a bobby pin's new health, the debug texts (drawn
//! as plain text over everything) and the cursor's traits (the arrow redrawn).

use crate::script::ScriptState;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseMotion, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_game::nvse_sup6::LockpickView;

pub struct SupScreenPlugin;

impl Plugin for SupScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DrawnTexts>()
            .add_systems(PreUpdate, tell.after(bevy::input::InputSystems).before(crate::script_input::ScriptInputSet))
            .add_systems(Update, take.after(crate::PlaySet::Scripts).before(crate::script_switches::sync))
            .add_systems(Update, draw_texts.after(crate::PlaySet::Scripts));
    }
}

/// Pixels a wheel notch is when the system gives the wheel in pixels [hypothesis].
const PIXELS_A_NOTCH: f32 = 40.0;

/// Before the scripts: the mouse, the window, the fonts, the lockpick menu, the throw.
#[allow(clippy::too_many_arguments)]
fn tell(
    mut script: Option<ResMut<ScriptState>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    windows: Query<&Window, With<PrimaryWindow>>,
    data: Option<Res<nvf_assets::GameData>>,
    lockpick: Option<Res<crate::lockpick::LockpickScreen>>,
    control: Option<Res<crate::control::PlayerControl>>,
    combat: Option<Res<crate::combat::Combat>>,
    time: Res<Time>,
    mut thrown_held: Local<bool>,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let thrown = combat.is_some_and(|c| c.player_weapon_is_throwable());
    let sup = &mut rt.game.sup;
    if let Ok(w) = windows.single() {
        sup.window = (f64::from(w.physical_width()), f64::from(w.physical_height()));
        if let Some(p) = w.physical_cursor_position() {
            sup.mouse.pos = (f64::from(p.x), f64::from(p.y));
        }
    }
    let d = motion.delta;
    sup.mouse.moved = (f64::from(d.x), f64::from(d.y));
    if d != Vec2::ZERO {
        sup.mouse.last_moved = sup.mouse.moved;
    }
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / PIXELS_A_NOTCH,
    };
    sup.mouse.wheel = f64::from(notches);
    if notches != 0.0 {
        sup.mouse.last_wheel = sup.mouse.wheel;
    }
    // The fonts' line heights, once (fonts 1-9, `crate::interface::FONT_FILES`, the
    // install's own files).
    if sup.fonts.is_empty() {
        if let Some(data) = data.as_deref() {
            sup.fonts = crate::interface::FONT_FILES
                .iter()
                .map(|f| {
                    let font = data.vfs.read(&crate::interface::font_file(f)).and_then(|r| r.ok()).and_then(|b| fnv_font::Font::parse(&b).ok());
                    font.map_or(0.0, |f| f64::from(f.size))
                })
                .collect();
        }
    }
    sup.lockpick = lockpick.and_then(|l| {
        let s = l.0.as_ref()?;
        let p = &s.game;
        Some(LockpickView { angle: p.pin, level: p.level.required_skill().unwrap_or(100.0), skill: p.skill, health: p.health })
    });
    // The throw: from the attack's press with a throwable in hand to its release; the
    // time stays until the next press [documented: GetGrenadeTimeHeld].
    let pressed = control.is_some_and(|c| c.trigger) && thrown;
    if pressed {
        if !*thrown_held {
            sup.grenade_held = 0.0;
        }
        sup.grenade_held += time.delta_secs_f64();
    }
    *thrown_held = pressed;
}

/// After the scripts: the fake moves and the pin's health. In menus or HUD-cursor mode,
/// fake moves also queue a relative pixel cursor warp (unless a script already issued
/// `SetCursorPos`) so `FakeMouseMovement` moves the menu cursor as well as feeding
/// `MouseMotion` [documented: FakeMouseMovement, "Will move cursor\camera by 1 pixel";
/// design: routed through `script_switches::sync` / `script_cursor`].
fn take(
    mut script: Option<ResMut<ScriptState>>,
    mut moves: MessageWriter<MouseMotion>,
    mut lockpick: Option<ResMut<crate::lockpick::LockpickScreen>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    page: Option<Res<crate::tiles::mods::ModsPage>>,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let mut sum = Vec2::ZERO;
    for (x, y) in std::mem::take(&mut rt.game.sup.fake_moves) {
        if x.is_finite() && y.is_finite() {
            moves.write(MouseMotion { delta: Vec2::new(x, y) });
            sum += Vec2::new(x, y);
        }
    }
    let start_page = page.as_deref().is_some_and(|p| p.frames > 0 && !p.closed);
    if sum != Vec2::ZERO
        && (rt.game.menu.is_some() || rt.game.other.jip9.hud_cursor || start_page)
        && rt.game.other.jip9.cursor_warp.is_none()
    {
        if let Ok(w) = windows.single() {
            if let Some(pos) = w.cursor_position() {
                let scale = w.scale_factor().max(1e-3);
                let tx = (f64::from(pos.x) + f64::from(sum.x / scale)).clamp(0.0, f64::from(w.width()));
                let ty = (f64::from(pos.y) + f64::from(sum.y / scale)).clamp(0.0, f64::from(w.height()));
                let j = &mut rt.game.other.jip9;
                j.cursor_warp = Some((tx, ty, false));
                j.cursor_warp_start_menu = start_page;
                j.cursor_warp_requested_generation = j.cursor_warp_generation;
                j.cursor_px = Some((tx, ty));
            }
        }
    }
    if let Some(h) = rt.game.sup.pin_health.take() {
        if let Some(s) = lockpick.as_deref_mut().and_then(|l| l.0.as_mut()) {
            s.game.health = h;
            info!("sup: the bobby pin's health set to {h}");
        }
    }
}

/// The debug texts drawn, by name.
#[derive(Resource, Default)]
struct DrawnTexts(std::collections::HashMap<String, (Entity, fnv_game::nvse_sup6::DebugText)>);

#[derive(Component)]
struct DebugTextNode;

/// The debug texts as plain white text over everything, at their pixel places
/// [hypothesis: the page shows no picture of one].
fn draw_texts(mut commands: Commands, script: Option<Res<ScriptState>>, mut drawn: ResMut<DrawnTexts>, windows: Query<&Window, With<PrimaryWindow>>) {
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else { return };
    let texts = &rt.game.sup.debug_texts;
    let scale = windows.single().map_or(1.0, |w| w.scale_factor());
    drawn.0.retain(|name, (e, _)| {
        let keep = texts.contains_key(name);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    for (name, t) in texts {
        if drawn.0.get(name).is_some_and(|(_, old)| old == t) {
            continue;
        }
        if let Some((e, _)) = drawn.0.remove(name) {
            commands.entity(e).despawn();
        }
        let node = Node { position_type: PositionType::Absolute, left: Val::Px(t.x / scale), top: Val::Px(t.y / scale), ..default() };
        let e = commands
            .spawn((node, Text::new(t.text.clone()), TextFont { font_size: bevy::text::FontSize::Px(t.zoom / scale), ..default() }, TextColor(Color::WHITE), GlobalZIndex(i32::MAX - 1), DebugTextNode))
            .id();
        drawn.0.insert(name.clone(), (e, t.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use fnv_game::game::{menus, RunCtx};
    use fnv_script::vm::XVal;

    fn run_q(app: &mut App, body: &str) -> XVal {
        app.world_mut()
            .resource_mut::<ScriptState>()
            .rt
            .as_mut()
            .unwrap()
            .game
            .run_query(&format!("float fX\nfloat fY\nstring_var sOut\nBegin Function {{}}\n{body}\nEnd"), RunCtx::default())
            .unwrap()
    }

    #[test]
    fn startup_window_sync_and_preupdate_tell_populate_sup_and_jip_before_input_callbacks() {
        #[derive(Resource, Default)]
        struct PreUpdateRead {
            screen_w: f64,
            mouse_pos: String,
            mouse_move: String,
            wheel: f64,
            cursor_x: f64,
        }

        fn input_callback(mut script: ResMut<ScriptState>, mut read: ResMut<PreUpdateRead>) {
            let g = &mut script.rt.as_mut().unwrap().game;
            let q = |g: &mut fnv_game::Game<'_>, body: &str| {
                g.run_query(&format!("float fX\nfloat fY\nstring_var sOut\nBegin Function {{}}\n{body}\nEnd"), RunCtx::default()).unwrap()
            };
            if let XVal::Num(v) = q(g, "SetFunctionValue (GetScreenTrait 2)") {
                read.screen_w = v;
            }
            if let XVal::Str(s) = q(g, "GetMousePosition fX fY\nlet sOut := sv_Construct \"%g %g\" fX fY\nSetFunctionValue sOut") {
                read.mouse_pos = s;
            }
            if let XVal::Str(s) = q(g, "GetMouseMovement fX fY 0\nlet sOut := sv_Construct \"%g %g\" fX fY\nSetFunctionValue sOut") {
                read.mouse_move = s;
            }
            if let XVal::Num(v) = q(g, "SetFunctionValue (GetMouseWheelMovement 0)") {
                read.wheel = v;
            }
            if let XVal::Num(v) = q(g, "SetFunctionValue (GetCursorPos X)") {
                read.cursor_x = v;
            }
        }

        let mut app = App::new();
        app.add_plugins((bevy::input::InputPlugin, crate::script_input::ScriptInputPlugin, crate::script_switches::ScriptSwitchesPlugin, SupScreenPlugin))
            .configure_sets(Update, (crate::PlaySet::Scripts, crate::PlaySet::Ui).chain())
            .init_resource::<Time>()
            .init_resource::<PreUpdateRead>()
            .insert_resource(ScriptState::over_no_plugins())
            .add_systems(PreUpdate, input_callback.in_set(crate::script_input::ScriptInputSet));
        let mut window = Window::default();
        window.resolution.set(1600.0, 900.0);
        window.set_cursor_position(Some(Vec2::new(400.0, 225.0)));
        app.world_mut().spawn((PrimaryWindow, window));

        // Before any PreUpdate runs, MenuFiles::sync_window_and_cursor seeds startup scripts.
        app.world_mut()
            .run_system_once(|files: crate::tiles::MenuFiles, mut script: ResMut<ScriptState>| {
                files.sync_window_and_cursor(&mut script.rt.as_mut().unwrap().game);
            })
            .unwrap();
        assert_eq!(run_q(&mut app, "SetFunctionValue (GetScreenTrait 2)"), XVal::Num(1600.0));
        assert_eq!(run_q(&mut app, "SetFunctionValue (GetScreenTrait 3)"), XVal::Num(900.0));
        assert_eq!(run_q(&mut app, "GetMousePosition fX fY\nlet sOut := sv_Construct \"%g %g\" fX fY\nSetFunctionValue sOut"), XVal::Str("400 225".into()));
        assert_eq!(run_q(&mut app, "SetFunctionValue (GetCursorPos X)"), XVal::Num(400.0));

        // Next frame: move window cursor, send MouseMotion + MouseWheel, open a menu, and
        // verify PreUpdate ScriptInputSet callbacks see the fresh values from sup_screen::tell.
        app.world_mut().resource_mut::<ScriptState>().rt.as_mut().unwrap().game.open_menu(menus::MESSAGE);
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
            let mut w = q.single_mut(world).unwrap();
            w.set_cursor_position(Some(Vec2::new(640.0, 360.0)));
        }
        app.world_mut().write_message(MouseMotion { delta: Vec2::new(12.0, -5.0) });
        app.world_mut().write_message(bevy::input::mouse::MouseWheel { unit: MouseScrollUnit::Line, x: 0.0, y: 2.0, window: Entity::PLACEHOLDER, phase: bevy::input::touch::TouchPhase::Moved });
        app.update();
        let read = app.world().resource::<PreUpdateRead>();
        assert_eq!(read.screen_w, 1600.0);
        assert_eq!(read.mouse_pos, "640 360");
        assert_eq!(read.mouse_move, "12 -5");
        assert_eq!(read.wheel, 2.0);
        assert_eq!(read.cursor_x, 640.0);
    }

    #[test]
    fn fake_mouse_movement_moves_menu_cursor_without_clobbering_set_cursor_pos() {
        let mut app = App::new();
        app.add_message::<MouseMotion>()
            .insert_resource(ScriptState::over_no_plugins());
        let mut window = Window::default();
        window.resolution.set(1280.0, 720.0);
        window.set_cursor_position(Some(Vec2::new(100.0, 100.0)));
        app.world_mut().spawn((PrimaryWindow, window));

        // Outside menus and without HUD cursor: MouseMotion emitted, no cursor_warp queued.
        run_q(&mut app, "FakeMouseMovement 10 -4");
        app.world_mut().run_system_once(take).unwrap();
        assert_eq!(app.world().resource::<ScriptState>().rt.as_ref().unwrap().game.other.jip9.cursor_warp, None);
        let deltas: Vec<Vec2> = app.world_mut().resource_mut::<Messages<MouseMotion>>().drain().map(|m| m.delta).collect();
        assert_eq!(deltas, vec![Vec2::new(10.0, -4.0)]);

        // In a menu: FakeMouseMovement queues a relative pixel warp.
        app.world_mut().resource_mut::<ScriptState>().rt.as_mut().unwrap().game.open_menu(menus::MESSAGE);
        run_q(&mut app, "FakeMouseMovement 15 -10");
        app.world_mut().run_system_once(take).unwrap();
        assert_eq!(app.world().resource::<ScriptState>().rt.as_ref().unwrap().game.other.jip9.cursor_warp, Some((115.0, 90.0, false)));

        // An explicit SetCursorPos in the same frame takes precedence over FakeMouseMovement.
        run_q(&mut app, "SetCursorPos 300 200\nFakeMouseMovement 15 -10");
        app.world_mut().run_system_once(take).unwrap();
        assert_eq!(app.world().resource::<ScriptState>().rt.as_ref().unwrap().game.other.jip9.cursor_warp, Some((300.0, 200.0, false)));
    }
}
