//! Windows SetCursorPos intent/receipt ownership; windows-cursor-ack.md.
//! No input messages are fabricated. Other platform and ordinary Window writers
//! retain their existing behavior.

use bevy::{prelude::*, window::PrimaryWindow, winit::{CursorPositionRequest, CursorPositionResult}};
use fnv_game::{game::{menus, Game}, nvse_jip9::Jip9State};
use crate::{script::ScriptState, tiles::mods::ModsPage};

// Engine policy, not an original-game timing claim: two native attempts at most,
// and at most 120 sync frames awaiting readiness or acknowledgement.
const MAX_PENDING_FRAMES: u16 = 120;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Owner { Hud, Menu(u16), Start }

#[derive(Clone, Copy)]
pub(crate) struct Context { menu: Option<u16>, hud: bool, start_page: bool, generation: u64, frame: u64 }

impl Context {
    pub(crate) fn of(game: &Game<'_>, page: Option<&ModsPage>) -> Self {
        Self { menu: game.menu, hud: game.other.jip9.hud_cursor,
            start_page: page.is_some_and(|p| p.frames > 0 && !p.closed), generation: game.other.jip9.cursor_warp_generation, frame: game.frame }
    }
    fn owns(self, owner: Owner) -> bool {
        match owner {
            Owner::Start => self.menu == Some(menus::START) || self.start_page,
            Owner::Menu(menu) => self.menu == Some(menu),
            Owner::Hud => self.hud && self.menu.is_none(),
        }
    }
}

struct Pending {
    id: u64,
    entity: Entity,
    raw: (f64, f64, bool),
    owner: Owner,
    frames: u16,
    generation: u64,
    last_deferred: Option<&'static str>,
}

#[derive(Resource, Default)]
pub(crate) struct CursorRequests { next: u64, pending: Option<Pending> }

impl CursorRequests {
    fn retire(&mut self, j: Option<&mut Jip9State>, commands: &mut Commands, why: &str) {
        if let Some(old) = self.pending.take() {
            if let Ok(mut entity) = commands.get_entity(old.entity) { entity.remove::<CursorPositionRequest>(); }
            if let Some(j) = j {
                if j.cursor_warp_receipt == Some(old.id) { j.cursor_warp_receipt = None; }
            }
            info!(request = old.id, window = ?old.entity, frames = old.frames, reason = why, "script cursor: retired");
        }
    }

    pub(crate) fn no_window(&mut self, j: &mut Jip9State, commands: &mut Commands) {
        self.retire(Some(j), commands, "primary window unavailable");
        if j.cursor_warp.take().is_some() { warn!("script cursor: explicit request retired without a primary window"); }
        j.cursor_warp_start_menu = false;
        j.cursor_warp_receipt = None;
        j.cursor_px = None;
    }

    fn expire(&mut self, j: &mut Jip9State, commands: &mut Commands) {
        if let Some(pending) = self.pending.as_ref() {
            warn!(request = pending.id, window = ?pending.entity, frames = pending.frames,
                "script cursor: readiness/acknowledgement budget exhausted without native success");
        }
        self.retire(Some(j), commands, "readiness/acknowledgement frame budget exhausted");
    }

    pub(crate) fn sync(&mut self, j: &mut Jip9State, context: Context, entity: Entity,
        window: &Window, mut receipt: Option<&mut CursorPositionRequest>, commands: &mut Commands) {
        if self.pending.as_ref().is_some_and(|p| p.entity != entity || j.cursor_warp_receipt != Some(p.id)
            || p.generation != context.generation || !context.owns(p.owner)) {
            self.retire(Some(j), commands, "window, runtime or menu owner changed");
            receipt = None;
        }
        // Taking the core one-shot transfers it into this owned transaction; it
        // does not imply native success. Every explicit request gets a new identity.
        if let Some(raw) = j.cursor_warp.take() {
            let start = std::mem::take(&mut j.cursor_warp_start_menu);
            let owner = if start { Owner::Start } else if let Some(menu) = context.menu { Owner::Menu(menu) } else { Owner::Hud };
            self.retire(Some(j), commands, "superseded by an explicit request");
            if !context.owns(owner) || j.cursor_warp_requested_generation != context.generation || !raw.0.is_finite() || !raw.1.is_finite() {
                warn!(?owner, x = raw.0, y = raw.1, "script cursor: invalid or closed-owner request retired");
                return;
            }
            self.next = self.next.wrapping_add(1).max(1);
            let pending = Pending { id: self.next, entity, raw, owner, frames: 0, generation: context.generation, last_deferred: None };
            let request = CursorPositionRequest::new(pending.id, position(&pending, window));
            info!(request = pending.id, window = ?entity, frame = context.frame, ?owner, x = raw.0, y = raw.1, xml = raw.2,
                origin = ?j.cursor_warp_origin, generation = pending.generation,
                focused = window.focused, visible = window.visible, "script cursor: requested");
            j.cursor_warp_receipt = Some(pending.id);
            // A queued remove precedes this insert, including when superseding a
            // previous request on the same window in the same frame.
            commands.entity(entity).insert(request);
            self.pending = Some(pending);
            return;
        }
        let Some(pending) = self.pending.as_mut() else { return };
        pending.frames += 1;
        let Some(receipt) = receipt.filter(|r| r.id == pending.id) else {
            if pending.frames >= MAX_PENDING_FRAMES { self.expire(j, commands); }
            return;
        };
        match &receipt.result {
            CursorPositionResult::Applied => self.retire(Some(j), commands, "native acknowledged"),
            CursorPositionResult::Superseded => self.retire(Some(j), commands, "ordinary Window writer superseded script intent"),
            CursorPositionResult::Rejected(error) if receipt.attempts >= 2 => {
                warn!(request = pending.id, error, "script cursor: native request failed; retry budget exhausted");
                self.retire(Some(j), commands, "native rejected twice");
            }
            _ if pending.frames >= MAX_PENDING_FRAMES => self.expire(j, commands),
            CursorPositionResult::Rejected(error) => {
                warn!(request = pending.id, error, "script cursor: native request rejected; one retry scheduled");
                receipt.position = position(pending, window);
                receipt.retry();
            }
            CursorPositionResult::Deferred(reason) => {
                if pending.last_deferred != Some(*reason) {
                    info!(request = pending.id, window = ?entity, reason, "script cursor: deferred without native acknowledgement");
                    pending.last_deferred = Some(*reason);
                }
                receipt.position = position(pending, window);
            }
            CursorPositionResult::Pending => {
                receipt.position = position(pending, window);
            }
        }
    }
}

fn position(pending: &Pending, window: &Window) -> Vec2 {
    let size = Vec2::new(window.width(), window.height());
    let (x, y, xml) = pending.raw;
    let per = f64::from(size.y.max(1.0)) / 960.0;
    let mut at = if xml { Vec2::new((x * per) as f32, (y * per) as f32) } else { Vec2::new(x as f32, y as f32) };
    if xml && pending.owner == Owner::Start {
        at = crate::interface::to_menu_frame(at, size, crate::interface::MIDDLE);
    }
    // Keep the requested point within the owned client area, including its last
    // physical pixel, rather than the first pixel of a neighboring window.
    let edge = (size - Vec2::splat(1.0 / window.scale_factor())).max(Vec2::ZERO);
    at.clamp(Vec2::ZERO, edge)
}

/// Publish matching native acknowledgement before PreUpdate source input callbacks.
/// No pointer event, hover/click pulse or contact ownership is synthesized.
pub(crate) fn refresh(state: Res<CursorRequests>, mut script: Option<ResMut<ScriptState>>,
    page: Option<Res<ModsPage>>, windows: Query<(Entity, &Window, Option<&CursorPositionRequest>), With<PrimaryWindow>>) {
    let Some(game) = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map(|rt| &mut rt.game) else { return };
    let Ok((entity, window, receipt)) = windows.single() else { return };
    let context = Context::of(game, page.as_deref());
    game.other.jip9.window = Some((f64::from(window.width()), f64::from(window.height())));
    game.other.jip9.cursor_px = window.cursor_position().map(|at| (f64::from(at.x), f64::from(at.y)));
    if let (Some(pending), Some(receipt), Some(at)) = (state.pending.as_ref(), receipt, window.cursor_position()) {
        if pending.entity == entity && pending.id == receipt.id && game.other.jip9.cursor_warp_receipt == Some(pending.id)
            && game.other.jip9.cursor_warp_generation == pending.generation && context.owns(pending.owner)
            && game.other.jip9.cursor_warp.is_none() && matches!(receipt.result, CursorPositionResult::Applied) {
            let at = if pending.owner == Owner::Start {
                crate::tiles::input::to_menu_units(at, Vec2::new(window.width(), window.height()), crate::interface::MIDDLE)
            } else { crate::tiles::input::to_units(at, window.height()) };
            game.ui_pointer.at = Some(at);
        }
    }
}

/// Recheck ownership after UI transitions and before the Last-stage native consumer.
/// In particular, a closed menu or replaced game cannot replay an old receipt.
pub(crate) fn guard(mut state: ResMut<CursorRequests>, mut script: Option<ResMut<ScriptState>>,
    page: Option<Res<ModsPage>>, windows: Query<(Entity, Option<&CursorPositionRequest>), With<PrimaryWindow>>, mut commands: Commands) {
    let Some(pending) = state.pending.as_ref() else { return };
    let game = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map(|rt| &mut rt.game);
    let valid = game.as_deref().is_some_and(|g| g.other.jip9.cursor_warp_receipt == Some(pending.id)
        && g.other.jip9.cursor_warp_generation == pending.generation
        && g.other.jip9.cursor_warp.is_none()
        && Context::of(g, page.as_deref()).owns(pending.owner))
        && windows.single().is_ok_and(|(entity, receipt)| entity == pending.entity && receipt.is_some_and(|r| r.id == pending.id));
    if !valid { state.retire(game.map(|g| &mut g.other.jip9), &mut commands, "owner changed before native application"); }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy_winit::{CursorPositionReadiness, CursorPositionTestBackend};
    use fnv_game::game::RunCtx;

    include!("script_cursor_schedule_tests.rs");

    pub(crate) fn backend(app: &App, window: Entity) -> CursorPositionTestBackend {
        CursorPositionTestBackend::new(app.world().get::<Window>(window).unwrap())
    }

    // Runs the actual vendored backend consumer with only its native result and
    // readiness facts injected. No OS window or CursorMoved event is synthesized.
    pub(crate) fn native_result(app: &mut App, window: Entity, backend: &mut CursorPositionTestBackend,
        ready: bool, result: Result<(), &'static str>) -> usize {
        let mut entity = app.world_mut().entity_mut(window);
        let mut receipt = entity.take::<CursorPositionRequest>().expect("production sync submitted a request");
        let mut calls = 0;
        {
            let mut window = entity.get_mut::<Window>().unwrap();
            let facts = CursorPositionReadiness { present: true, focused: ready, visible: true, minimized: false,
                physical_size: UVec2::new(window.physical_width(), window.physical_height()), cursor_inside: Some(true) };
            backend.apply(&mut window, &mut receipt, facts, |_| { calls += 1; result });
        }
        entity.insert(receipt);
        calls
    }

    fn native_if_present(app: &mut App, window: Entity, backend: &mut CursorPositionTestBackend) -> usize {
        if app.world().get::<CursorPositionRequest>(window).is_none() { return 0; }
        native_result(app, window, backend, true, Ok(()))
    }

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.insert_resource(ScriptState::over_no_plugins())
            .init_resource::<crate::script_switches::ScriptSwitches>()
            .init_resource::<CursorRequests>();
        let mut window = Window::default();
        window.resolution.set(1280.0, 720.0);
        window.set_cursor_position(Some(Vec2::new(20.0, 30.0)));
        let entity = app.world_mut().spawn((PrimaryWindow, window)).id();
        app.world_mut().resource_mut::<ScriptState>().rt.as_mut().unwrap().game.open_menu(menus::MESSAGE);
        (app, entity)
    }

    fn query(app: &mut App, source: &str) -> fnv_script::vm::XVal {
        app.world_mut().resource_mut::<ScriptState>().rt.as_mut().unwrap().game
            .run_query(&format!("Begin Function {{}}\n{source}\nEnd"), RunCtx::default()).unwrap()
    }
    fn sync(app: &mut App) { app.world_mut().run_system_once(crate::script_switches::sync).unwrap(); }
    fn guard(app: &mut App) { app.world_mut().run_system_once(super::guard).unwrap(); }

    #[test]
    fn native_rejection_keeps_authoritative_pointer_until_successful_acknowledgement() {
        let (mut app, window) = app();
        let mut native = backend(&app, window);
        assert_eq!(query(&mut app, "SetCursorPos 100 80\nSetFunctionValue (GetCursorPos X)"), fnv_script::vm::XVal::Num(100.0), "same-frame source readback retains accepted intent");
        sync(&mut app);
        assert_eq!(app.world().get::<Window>(window).unwrap().cursor_position(), Some(Vec2::new(20.0, 30.0)), "request submission is not native acknowledgement");
        assert_eq!(native_result(&mut app, window, &mut native, true, Err("native rejection")), 1);
        assert_eq!(native.cached_position(), Some(Vec2::new(20.0, 30.0)));
        sync(&mut app);
        assert_eq!(query(&mut app, "SetFunctionValue (GetCursorPos X)"), fnv_script::vm::XVal::Num(20.0), "subsequent source frames use the authoritative pointer after rejection");
        assert_eq!(native_result(&mut app, window, &mut native, true, Ok(())), 1);
        assert_eq!(native.cached_position(), Some(Vec2::new(100.0, 80.0)));
        app.world_mut().run_system_once(super::refresh).unwrap();
        assert_eq!(query(&mut app, "SetFunctionValue (GetCursorPos X)"), fnv_script::vm::XVal::Num(100.0), "PreUpdate callbacks see acknowledged position before receipt retirement");
        sync(&mut app);
        assert!(app.world().get::<CursorPositionRequest>(window).is_none());
        assert_eq!(query(&mut app, "SetFunctionValue (GetCursorPos X)"), fnv_script::vm::XVal::Num(100.0));
    }

    #[test]
    fn repeated_same_value_has_new_identity_and_stale_receipt_cannot_acknowledge_it() {
        let (mut app, window) = app();
        query(&mut app, "SetCursorPos 100 80"); sync(&mut app);
        let old = app.world().get::<CursorPositionRequest>(window).unwrap().id;
        query(&mut app, "SetCursorPos 100 80"); sync(&mut app);
        let new = app.world().get::<CursorPositionRequest>(window).unwrap().id;
        assert_ne!(old, new);
        let mut stale = CursorPositionRequest::new(old, Vec2::new(100.0, 80.0));
        stale.result = CursorPositionResult::Applied;
        app.world_mut().entity_mut(window).insert(stale);
        guard(&mut app);
        assert!(app.world().get::<CursorPositionRequest>(window).is_none());
        assert_eq!(app.world().get::<Window>(window).unwrap().cursor_position(), Some(Vec2::new(20.0, 30.0)));
        assert!(app.world().resource::<CursorRequests>().pending.is_none());
    }

    #[test]
    fn menu_generation_runtime_and_window_replacement_retire_old_intent() {
        for boundary in 0..3 {
            let (mut app, window) = app();
            let mut native = backend(&app, window);
            query(&mut app, "SetCursorPos 100 80"); sync(&mut app);
            assert_eq!(native_result(&mut app, window, &mut native, false, Ok(())), 0);
            let mut replacement = None;
            match boundary {
                0 => {
                    let mut script = app.world_mut().resource_mut::<ScriptState>();
                    let game = &mut script.rt.as_mut().unwrap().game;
                    game.close_menu(); game.open_menu(menus::MESSAGE);
                }
                1 => { app.insert_resource(ScriptState::over_no_plugins()); }
                _ => {
                    app.world_mut().despawn(window);
                    replacement = Some(app.world_mut().spawn((PrimaryWindow, Window::default())).id());
                }
            }
            guard(&mut app);
            assert!(app.world().resource::<CursorRequests>().pending.is_none(), "boundary {boundary}");
            if boundary != 2 { assert!(app.world().get::<CursorPositionRequest>(window).is_none()); }
            assert_eq!(native_if_present(&mut app, window, &mut native), 0, "a retired owner cannot reach the native warp consumer");
            if let Some(replacement) = replacement {
                assert_ne!(replacement, window);
                assert_eq!(native_if_present(&mut app, replacement, &mut native), 0, "window replacement cannot inherit old cursor intent");
            }
            if boundary == 0 {
                query(&mut app, "SetCursorPos 100 80"); sync(&mut app);
                assert_eq!(native_if_present(&mut app, window, &mut native), 1, "a fresh request for the reopened menu can reach native");
            }
        }
    }

    #[test]
    fn later_ui_request_cancels_the_old_native_attempt_before_application() {
        let (mut app, window) = app();
        let mut native = backend(&app, window);
        query(&mut app, "SetCursorPos 100 80"); sync(&mut app);
        query(&mut app, "SetCursorPos 200 160");
        guard(&mut app);
        assert_eq!(native_if_present(&mut app, window, &mut native), 0);
        assert_eq!(native.cached_position(), Some(Vec2::new(20.0, 30.0)));
        sync(&mut app);
        assert_eq!(native_if_present(&mut app, window, &mut native), 1);
        assert_eq!(native.cached_position(), Some(Vec2::new(200.0, 160.0)));
    }

    #[test]
    fn unavailable_window_expires_without_calling_native_and_rejections_retry_only_once() {
        let (mut app, window) = app();
        let mut native = backend(&app, window);
        query(&mut app, "SetCursorPos 100 80"); sync(&mut app);
        for _ in 0..MAX_PENDING_FRAMES {
            assert_eq!(native_result(&mut app, window, &mut native, false, Ok(())), 0);
            sync(&mut app);
        }
        assert!(app.world().get::<CursorPositionRequest>(window).is_none());
        query(&mut app, "SetCursorPos 100 80"); sync(&mut app);
        for _ in 0..2 {
            assert_eq!(native_result(&mut app, window, &mut native, true, Err("native rejection")), 1);
            sync(&mut app);
        }
        assert!(app.world().get::<CursorPositionRequest>(window).is_none());
        assert_eq!(native.cached_position(), Some(Vec2::new(20.0, 30.0)));
    }

    #[test]
    fn xml_intent_reads_back_over_stale_ui_pointer_and_resolves_again_on_resize() {
        let (mut app, window) = app();
        let mut native = backend(&app, window);
        app.world_mut().resource_mut::<ScriptState>().rt.as_mut().unwrap().game.ui_pointer.at = Some((1.0, 2.0));
        assert_eq!(query(&mut app, "SetCursorPos 100 80 1\nSetFunctionValue (GetCursorPos X 1)"), fnv_script::vm::XVal::Num(100.0));
        sync(&mut app);
        assert_eq!(app.world().get::<CursorPositionRequest>(window).unwrap().position, Vec2::new(75.0, 60.0));
        app.world_mut().get_mut::<Window>(window).unwrap().resolution.set(1600.0, 900.0);
        sync(&mut app);
        assert_eq!(app.world().get::<CursorPositionRequest>(window).unwrap().position, Vec2::new(93.75, 75.0));
        assert_eq!(native_result(&mut app, window, &mut native, true, Ok(())), 1);
        assert_eq!(native.cached_position(), Some(Vec2::new(93.75, 75.0)));
        app.world_mut().run_system_once(super::refresh).unwrap();
        assert_eq!(query(&mut app, "SetFunctionValue (GetCursorPos X 1)"), fnv_script::vm::XVal::Num(100.0));
    }
}
