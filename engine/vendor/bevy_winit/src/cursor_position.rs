//! Acknowledged cursor requests. Ordinary `Window` setters keep their existing path.

use bevy_ecs::{change_detection::DetectChangesMut, entity::Entity, prelude::Component, system::{NonSendMarker, Query}};
use bevy_math::{UVec2, Vec2};
use bevy_window::Window;
use winit::dpi::PhysicalPosition;

use crate::{system::CachedWindow, WINIT_WINDOWS};

/// Result of the last native attempt; requesting a position alone never acknowledges it.
#[derive(Debug, Clone, PartialEq)]
pub enum CursorPositionResult {
    /// The owner has submitted or explicitly retried this request.
    Pending,
    /// No native call was made because the owned window was unavailable.
    Deferred(&'static str),
    /// The native call returned success and both window representations were updated.
    Applied,
    /// The native call failed. The previous window position is retained.
    Rejected(String),
    /// An ordinary window writer superseded this request.
    Superseded,
}

/// Explicit cursor intent and its receipt, attached to the owned window entity.
#[derive(Component, Debug)]
pub struct CursorPositionRequest {
    /// Owner-assigned identity, including repeated requests for identical coordinates.
    pub id: u64,
    /// Position in the Bevy window's logical pixels, resolved again by its owner on resize.
    pub position: Vec2,
    /// Native acknowledgement, or the reason no acknowledgement exists.
    pub result: CursorPositionResult,
    /// Number of native calls for this intent. At most two are permitted.
    pub attempts: u8,
}

impl CursorPositionRequest {
    /// Submit an intent without changing the window's authoritative cursor position.
    pub fn new(id: u64, position: Vec2) -> Self {
        Self { id, position, result: CursorPositionResult::Pending, attempts: 0 }
    }

    /// Permit one bounded retry after a rejected native call.
    pub fn retry(&mut self) {
        if self.attempts < 2 && matches!(self.result, CursorPositionResult::Rejected(_)) {
            self.result = CursorPositionResult::Pending;
        }
    }
}

/// Native window facts captured immediately before applying a request.
#[derive(Debug, Clone, Copy)]
pub struct CursorPositionReadiness {
    /// A native window and its cache exist for this entity.
    pub present: bool,
    /// The native window reports focus.
    pub focused: bool,
    /// The native window reports visibility.
    pub visible: bool,
    /// The native window reports minimization.
    pub minimized: bool,
    /// Current native client area; a pending resize must settle before positioning.
    pub physical_size: UVec2,
    /// Current native cursor containment; `None` means ownership could not be queried.
    pub cursor_inside: Option<bool>,
}

impl CursorPositionReadiness {
    fn unavailable(self, window: &Window) -> Option<&'static str> {
        if !self.present { return Some("native window unavailable"); }
        if !self.focused || !window.focused { return Some("window unfocused"); }
        if !self.visible || !window.visible || self.minimized { return Some("window hidden or minimized"); }
        match self.cursor_inside {
            None => return Some("native cursor ownership unavailable"),
            Some(false) => return Some("native cursor outside client area"),
            Some(true) => {}
        }
        let size = Vec2::new(window.physical_width() as f32, window.physical_height() as f32);
        if size.min_element() <= 0.0 { return Some("window has no client area"); }
        if self.physical_size != UVec2::new(window.physical_width(), window.physical_height()) {
            return Some("native client resize pending");
        }
        None
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code, reason = "Win32 public APIs query the owned live winit handle and a stack POINT")]
fn native_cursor_owner(window: &winit::window::Window) -> (bool, Option<bool>) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::{Foundation::POINT, Graphics::Gdi::ScreenToClient,
        UI::WindowsAndMessaging::{GetCursorPos, GetForegroundWindow}};
    let Ok(handle) = window.window_handle() else { return (false, None) };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else { return (false, None) };
    let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: winit owns this live window for the duration of this main-thread
    // borrow. Both coordinate APIs receive a valid, writable stack POINT. No
    // desktop, focus, input ownership or cursor position is changed here.
    unsafe {
        if GetForegroundWindow() != hwnd { return (false, Some(false)); }
        if GetCursorPos(&mut point) == 0 || ScreenToClient(hwnd, &mut point) == 0 { return (true, None); }
    }
    let size = window.inner_size();
    (true, Some(point.x >= 0 && point.y >= 0 && (point.x as u32) < size.width && (point.y as u32) < size.height))
}

// This is the production consumer. Test support injects only the native call and
// readiness facts here; it does not reproduce the acknowledgement implementation.
fn apply_request<E: core::fmt::Display>(
    window: &mut Window,
    cache: &mut CachedWindow,
    request: &mut CursorPositionRequest,
    readiness: CursorPositionReadiness,
    native: impl FnOnce(PhysicalPosition<f64>) -> Result<(), E>,
) {
    if !matches!(request.result, CursorPositionResult::Pending | CursorPositionResult::Deferred(_)) { return; }
    if window.physical_cursor_position() != cache.physical_cursor_position() {
        request.result = CursorPositionResult::Superseded;
        return;
    }
    if let Some(reason) = readiness.unavailable(window) {
        request.result = CursorPositionResult::Deferred(reason);
        return;
    }
    if !request.position.is_finite() || request.attempts >= 2 {
        request.result = CursorPositionResult::Rejected("invalid position or native attempt budget exhausted".into());
        return;
    }
    let position = request.position.as_dvec2() * f64::from(window.scale_factor());
    if position.x < 0.0 || position.y < 0.0 || position.x.round() >= f64::from(readiness.physical_size.x)
        || position.y.round() >= f64::from(readiness.physical_size.y) {
        request.result = CursorPositionResult::Rejected("target outside owned client area".into());
        return;
    }
    request.attempts += 1;
    match native(PhysicalPosition::new(position.x, position.y)) {
        Ok(()) => {
            window.set_cursor_position(Some(request.position));
            cache.set_physical_cursor_position(window.physical_cursor_position().map(|at| at.as_dvec2()));
            request.result = CursorPositionResult::Applied;
        }
        Err(error) => request.result = CursorPositionResult::Rejected(error.to_string()),
    }
}

pub(crate) fn apply_cursor_position_requests(
    mut windows: Query<(Entity, &mut Window, Option<&mut CachedWindow>, &mut CursorPositionRequest)>,
    _main_thread: NonSendMarker,
) {
    WINIT_WINDOWS.with_borrow(|native_windows| {
        for (entity, mut window, cache, mut request) in &mut windows {
            let (Some(mut cache), Some(native_window)) = (cache, native_windows.get_window(entity)) else {
                if matches!(request.result, CursorPositionResult::Pending | CursorPositionResult::Deferred(_)) {
                    request.result = CursorPositionResult::Deferred("native window unavailable");
                }
                continue;
            };
            #[cfg(target_os = "windows")]
            let (foreground, cursor_inside) = native_cursor_owner(native_window);
            #[cfg(not(target_os = "windows"))]
            let (foreground, cursor_inside) = (true, Some(window.physical_cursor_position().is_some()));
            let readiness = CursorPositionReadiness {
                present: true,
                focused: native_window.has_focus() && foreground,
                visible: native_window.is_visible() == Some(true),
                minimized: native_window.is_minimized() == Some(true),
                physical_size: { let size = native_window.inner_size(); UVec2::new(size.width, size.height) },
                cursor_inside,
            };
            let attempts = request.attempts;
            apply_request(window.bypass_change_detection(), cache.bypass_change_detection(), &mut request, readiness,
                |position| native_window.set_cursor_position(position));
            if matches!(request.result, CursorPositionResult::Applied) && request.attempts != attempts {
                window.set_changed();
                cache.set_changed();
            }
            if request.attempts != attempts && let CursorPositionResult::Rejected(error) = &request.result {
                tracing::error!(?entity, request = request.id, position = ?request.position, attempt = request.attempts,
                    ?readiness, "could not set cursor position: {error}");
            }
        }
    });
}

/// Offscreen fixture for the actual consumer, available only to regression tests.
#[cfg(feature = "cursor-test-support")]
pub struct CursorPositionTestBackend { cache: CachedWindow }

#[cfg(feature = "cursor-test-support")]
impl CursorPositionTestBackend {
    /// Start from the backend's last confirmed window state.
    pub fn new(window: &Window) -> Self { Self { cache: CachedWindow::cursor_test_cache(window) } }

    /// Execute the same consumer used by the native backend with an injected native result.
    pub fn apply<E: core::fmt::Display>(&mut self, window: &mut Window, request: &mut CursorPositionRequest,
        readiness: CursorPositionReadiness, native: impl FnOnce(PhysicalPosition<f64>) -> Result<(), E>) {
        apply_request(window, &mut self.cache, request, readiness, native);
    }

    /// The cache position ordinary window synchronization will compare against.
    pub fn cached_position(&self) -> Option<Vec2> { self.cache.cursor_position() }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> CursorPositionReadiness {
        CursorPositionReadiness { present: true, focused: true, visible: true, minimized: false,
            physical_size: UVec2::new(1280, 720), cursor_inside: Some(true) }
    }
    fn setup() -> (Window, CachedWindow, CursorPositionRequest) {
        let mut window = Window::default();
        window.set_cursor_position(Some(Vec2::new(20.0, 30.0)));
        let cache = CachedWindow::cursor_test_cache(&window);
        (window, cache, CursorPositionRequest::new(7, Vec2::new(80.0, 90.0)))
    }

    #[test]
    fn rejected_native_warp_does_not_publish_and_success_updates_cache_once() {
        let (mut window, mut cache, mut request) = setup();
        apply_request(&mut window, &mut cache, &mut request, ready(), |_| Err("native failure"));
        assert_eq!(request.result, CursorPositionResult::Rejected("native failure".into()));
        assert_eq!(window.cursor_position(), Some(Vec2::new(20.0, 30.0)));
        assert_eq!(cache.cursor_position(), window.cursor_position());
        request.retry();
        apply_request(&mut window, &mut cache, &mut request, ready(), |_| Ok::<_, &str>(()));
        assert_eq!(request.result, CursorPositionResult::Applied);
        assert_eq!(window.cursor_position(), Some(Vec2::new(80.0, 90.0)));
        assert_eq!(cache.cursor_position(), window.cursor_position(), "ordinary changed_windows must not send a duplicate warp");
        apply_request(&mut window, &mut cache, &mut request, ready(), |_| -> Result<(), &str> { panic!("acknowledged request must not run twice") });
        assert_eq!(request.attempts, 2);
    }

    #[test]
    fn unavailable_window_and_ordinary_writers_do_not_issue_script_warps() {
        for facts in [CursorPositionReadiness { present: false, ..ready() },
            CursorPositionReadiness { focused: false, ..ready() },
            CursorPositionReadiness { visible: false, ..ready() },
            CursorPositionReadiness { minimized: true, ..ready() },
            CursorPositionReadiness { cursor_inside: None, ..ready() },
            CursorPositionReadiness { cursor_inside: Some(false), ..ready() }] {
            let (mut window, mut cache, mut request) = setup();
            apply_request(&mut window, &mut cache, &mut request, facts, |_| -> Result<(), &str> { panic!("unavailable window must not warp") });
            assert!(matches!(request.result, CursorPositionResult::Deferred(_)));
            assert_eq!(request.attempts, 0);
        }
        let (mut window, mut cache, mut request) = setup();
        window.set_cursor_position(Some(Vec2::new(40.0, 50.0)));
        apply_request(&mut window, &mut cache, &mut request, ready(), |_| -> Result<(), &str> { panic!("ordinary writer has priority") });
        assert_eq!(request.result, CursorPositionResult::Superseded);
        assert_eq!(window.cursor_position(), Some(Vec2::new(40.0, 50.0)));
        assert_eq!(cache.cursor_position(), Some(Vec2::new(20.0, 30.0)));
    }

    #[test]
    fn two_native_rejections_exhaust_the_explicit_retry_budget() {
        let (mut window, mut cache, mut request) = setup();
        for _ in 0..2 {
            apply_request(&mut window, &mut cache, &mut request, ready(), |_| Err("native failure"));
            request.retry();
        }
        apply_request(&mut window, &mut cache, &mut request, ready(), |_| -> Result<(), &str> { panic!("native retry budget exceeded") });
        assert_eq!(request.attempts, 2);
        assert_eq!(window.cursor_position(), Some(Vec2::new(20.0, 30.0)));
    }

    #[test]
    fn native_cursor_ownership_allows_initial_warp_without_a_motion_event() {
        let (mut window, _, mut request) = setup();
        window.set_cursor_position(None);
        let mut cache = CachedWindow::cursor_test_cache(&window);
        apply_request(&mut window, &mut cache, &mut request, ready(), |_| Err("native rejection"));
        assert_eq!(window.cursor_position(), None);
        assert_eq!(cache.cursor_position(), None);
        request.retry();
        apply_request(&mut window, &mut cache, &mut request, ready(), |_| Ok::<_, &str>(()));
        assert_eq!(window.cursor_position(), Some(Vec2::new(80.0, 90.0)));
        assert_eq!(cache.cursor_position(), window.cursor_position());
    }

    #[test]
    fn resize_and_cursor_absence_defer_and_dpi_is_applied_at_the_consumer() {
        let (mut window, mut cache, mut request) = setup();
        apply_request(&mut window, &mut cache, &mut request,
            CursorPositionReadiness { physical_size: UVec2::new(640, 480), ..ready() },
            |_| -> Result<(), &str> { panic!("native resize has not settled") });
        assert_eq!(request.result, CursorPositionResult::Deferred("native client resize pending"));
        window.set_cursor_position(None);
        cache.set_cursor_position(None);
        apply_request(&mut window, &mut cache, &mut request, CursorPositionReadiness { cursor_inside: None, ..ready() },
            |_| -> Result<(), &str> { panic!("cursor not observed in owned window") });
        assert!(matches!(request.result, CursorPositionResult::Deferred(_)));
        window.resolution.set_scale_factor_override(Some(2.0));
        window.set_cursor_position(Some(Vec2::new(20.0, 30.0)));
        cache = CachedWindow::cursor_test_cache(&window);
        apply_request(&mut window, &mut cache, &mut request, ready(), |at| {
            assert_eq!((at.x, at.y), (160.0, 180.0));
            Ok::<_, &str>(())
        });
        assert_eq!(request.result, CursorPositionResult::Applied);
        assert_eq!(cache.cursor_position(), Some(Vec2::new(80.0, 90.0)));
    }
}
