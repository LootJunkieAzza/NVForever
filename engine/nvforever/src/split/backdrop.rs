//! A black backdrop under both halves while split screen is on (split.rs; the owner's idea
//! of 2026-11-22, "player 1 draws a black box under the game so that entering split screen
//! nothing moves around and the desktop never shows behind"; before it, the owner's list of
//! 2026-11-21, "while it's loading there's a blank space where it shows desktop behind it").
//!
//! - **Its time.** Player 1's game makes it the frame split screen starts, before its own
//!   window goes to its half, so the desktop never shows while the windows move. It covers
//!   the whole monitor and stays until split screen ends.
//! - **Under the halves.** Player 1's game keeps the stack every frame: player 2's window
//!   (while it shows), player 1's, then the backdrop, together. A window owned by player
//!   1's would always be over it, so the backdrop has no owner [documented: Win32 "Owned
//!   Windows", an owned window is always above its owner]. All three are topmost while
//!   either half has the focus (the taskbar fix, split.rs `p1_watch_focus`), so the
//!   taskbar stays hidden, and go back under the program that takes the focus. The
//!   backdrop hides while player 1's window is minimised.
//! - **Never in the way.** It is a tool window (no taskbar button, not in Alt+Tab) and is
//!   never activated, not even by a click, so the keyboard stays with player 1's window
//!   and Alt+Tab to another program works as before.
//! - **Player 2 is joining.** While player 2's window isn't showing (its game loading, or
//!   starting again to load a character, `crate::handoff`), the backdrop says so in player
//!   2's half. Once player 2's window shows, it covers that half.
//!
//! A window of this game's own (Win32, the main thread's, whose messages the game's event
//! loop already dispatches), not a Bevy one: it draws nothing but black and a line of
//! text, with no GPU surface.

use std::sync::atomic::{AtomicU32, Ordering};

use bevy::prelude::*;

/// Frames the backdrop has been up and painted ([`ready`]); [`FAILED`] when it could not be made.
static FRAMES_UP: AtomicU32 = AtomicU32::new(0);
const FAILED: u32 = u32::MAX;
/// Frames the backdrop is up before player 1's window may leave the whole screen: one
/// for Windows to compose it on the screen, one to spare [engineering choice].
const READY_AFTER: u32 = 2;

/// Whether player 1's window may go to its half: the backdrop has been on the screen for
/// [`READY_AFTER`] frames (or could not be made), so nothing behind shows as it shrinks
/// [observed: the two-window check of 2026-11-22 and the owner's "start of split", the
/// desktop showed as player 1's window shrank before the backdrop was painted].
pub fn ready() -> bool {
    let n = FRAMES_UP.load(Ordering::Relaxed);
    n == FAILED || n >= READY_AFTER
}

/// What the backdrop says in player 2's half while it waits [design: our own words].
pub const JOINING: &str = "Player 2 is joining\u{2026}";

/// What the backdrop is to look like this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// The monitor, in screen pixels.
    pub screen: (IVec2, UVec2),
    /// Player 2's half, in screen pixels.
    pub p2_half: (IVec2, UVec2),
    /// "Player 2 is joining…" in player 2's half.
    pub joining: bool,
}

/// The backdrop, while it is up.
#[derive(Default)]
pub struct Backdrop {
    hwnd: Option<isize>,
    look: Option<Look>,
    hidden: bool,
    /// Times the stack was put right again (a window came between, or the band changed).
    restacks: u32,
    /// Making it failed (tried once a split screen).
    failed: bool,
}

impl Backdrop {
    /// Up as `look` says, under the halves: `p1` (player 1's window) and `p2` (player 2's,
    /// once known), all three topmost when `on_top`. Made the first time, then kept in place.
    pub fn show(&mut self, p1: isize, p2: Option<isize>, look: Look, on_top: bool) {
        if self.hwnd.is_some_and(|h| !imp::alive(h)) {
            self.hwnd = None;
        }
        let h = match self.hwnd {
            Some(h) => {
                let n = FRAMES_UP.load(Ordering::Relaxed);
                if n < READY_AFTER {
                    FRAMES_UP.store(n + 1, Ordering::Relaxed);
                }
                h
            }
            None if self.failed => return,
            None => {
                let Some(h) = imp::create(look.screen) else {
                    warn!("split: no backdrop: the desktop may show while the windows move and player 2's game loads");
                    self.failed = true;
                    FRAMES_UP.store(FAILED, Ordering::Relaxed);
                    return;
                };
                info!("split: backdrop up under the halves ({}x{} at {:?})", look.screen.1.x, look.screen.1.y, look.screen.0);
                self.hwnd = Some(h);
                self.hidden = false;
                self.look = None;
                h
            }
        };
        if self.look != Some(look) {
            if self.look.is_some_and(|l| l.screen != look.screen) {
                imp::place(h, look.screen);
            }
            if self.look.is_some_and(|l| l.joining != look.joining) {
                info!("split: backdrop: {}", if look.joining { "player 2 is joining (their window isn't showing)" } else { "player 2's window shows over their half" });
            }
            imp::set_text(h, look.joining.then_some(look.p2_half), look.screen.0);
            self.look = Some(look);
        }
        // Player 1's window minimised: its owned windows hide with it, and so does this.
        let iconic = imp::iconic(p1);
        if iconic != self.hidden {
            self.hidden = iconic;
            imp::set_shown(h, !iconic);
        }
        if iconic {
            return;
        }
        // The stack, top down: player 2's window (while it shows), player 1's, the
        // backdrop. Player 1's game keeps all three, since a window owned by another
        // process's window does not follow it into the topmost band [observed: the
        // two-window check of 2026-11-22, player 2's window stayed out of it, under the
        // backdrop].
        let order: Vec<isize> = p2.filter(|&w| imp::shown(w)).into_iter().chain([p1, h]).collect();
        let mut moved = false;
        let mut why = "the band";
        let mut left_top = false;
        // The band, bottom up, so they land in their order.
        for &w in order.iter().rev() {
            if imp::topmost(w) != on_top {
                imp::set_topmost(w, on_top);
                left_top |= !on_top;
                moved = true;
            }
        }
        // Leaving the topmost band puts a window over every one that isn't topmost
        // [documented: SetWindowPos, HWND_NOTOPMOST]: the halves go back under the program
        // that took the focus.
        if left_top {
            if let Some(fg) = imp::foreground().filter(|f| !order.contains(f)) {
                imp::put_under(order[0], fg);
            }
        }
        for pair in order.windows(2) {
            if !imp::just_under(pair[1], pair[0]) {
                imp::put_under(pair[1], pair[0]);
                why = if pair[1] == h { "the backdrop was not under player 1's window" } else { "player 1's window was not under player 2's" };
                moved = true;
            }
        }
        if moved {
            self.restacks += 1;
            if self.restacks <= 2 || self.restacks.is_power_of_two() {
                info!("split: backdrop: the halves and the backdrop stacked again ({} windows, {}; {why}; {} times)", order.len(), if on_top { "over the taskbar" } else { "under the focused program" }, self.restacks);
            }
        }
    }

    /// Gone (split screen ended).
    /// The backdrop's window, while it is made.
    pub fn hwnd(&self) -> Option<isize> {
        self.hwnd
    }

    pub fn hide(&mut self, why: &str) {
        if let Some(h) = self.hwnd.take() {
            imp::destroy(h);
            info!("split: backdrop gone ({why})");
        }
        self.look = None;
        self.hidden = false;
        self.failed = false;
        self.restacks = 0;
        FRAMES_UP.store(0, Ordering::Relaxed);
    }
}

/// Whether window `hwnd` (another game's, player 2's) exists and is shown.
pub fn shown(hwnd: isize) -> bool {
    imp::shown(hwnd)
}

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;

    use bevy::math::{IVec2, UVec2};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, DeleteObject, DrawTextW, EndPaint, FillRect, GetStockObject, InvalidateRect, SelectObject, SetBkMode, SetTextColor, UpdateWindow, BLACK_BRUSH, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL, HBRUSH, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetForegroundWindow, GetWindow, GetWindowLongPtrW, IsIconic, IsWindow, IsWindowVisible, RegisterClassW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GW_HWNDNEXT, HWND_NOTOPMOST, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_PAINT, WNDCLASSW,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };

    /// Where the text goes, in the backdrop's own pixels; `None`, no text. Read by the
    /// window's paint (one backdrop at a time, the main thread's).
    static TEXT: Mutex<Option<RECT>> = Mutex::new(None);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// `MA_NOACTIVATE`: a click on the backdrop never takes the focus.
    const MA_NOACTIVATE: LRESULT = 3;

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        match msg {
            WM_MOUSEACTIVATE => MA_NOACTIVATE,
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let text = TEXT.lock().ok().and_then(|t| *t);
                // SAFETY: this window's own paint, between BeginPaint and EndPaint; the
                // font is made, used and deleted here.
                unsafe {
                    let mut ps: PAINTSTRUCT = std::mem::zeroed();
                    let dc = BeginPaint(hwnd, &mut ps);
                    let mut rect: RECT = std::mem::zeroed();
                    GetClientRect(hwnd, &mut rect);
                    FillRect(dc, &rect, GetStockObject(BLACK_BRUSH) as HBRUSH);
                    if let Some(mut area) = text {
                        let height = ((area.bottom - area.top) / 22).clamp(18, 48);
                        let face = wide("Segoe UI");
                        let font = CreateFontW(height, 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, 0, CLEARTYPE_QUALITY as u32, FF_DONTCARE as u32, face.as_ptr());
                        let old = SelectObject(dc, font);
                        SetBkMode(dc, TRANSPARENT as i32);
                        SetTextColor(dc, 0x00A0_A0A0);
                        let mut line: Vec<u16> = super::JOINING.encode_utf16().collect();
                        DrawTextW(dc, line.as_mut_ptr(), line.len() as i32, &mut area, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                        SelectObject(dc, old);
                        DeleteObject(font);
                    }
                    EndPaint(hwnd, &ps);
                }
                0
            }
            // SAFETY: the default handling of this window's other messages.
            _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
        }
    }

    pub fn create((at, size): (IVec2, UVec2)) -> Option<isize> {
        let class = wide("NVForeverSplitBackdrop");
        let title = wide("NVForever");
        // SAFETY: a window class and a window of this process, made on the main thread
        // (the caller's `NonSendMarker`), whose messages the game's event loop dispatches.
        // Registering the class again fails harmlessly (it is the same class).
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance, lpszClassName: class.as_ptr(), hbrBackground: GetStockObject(BLACK_BRUSH) as HBRUSH, ..std::mem::zeroed() };
            RegisterClassW(&wc);
            let h = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, class.as_ptr(), title.as_ptr(), WS_POPUP, at.x, at.y, size.x as i32, size.y as i32, std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null());
            if h.is_null() {
                return None;
            }
            ShowWindow(h, SW_SHOWNOACTIVATE);
            // Painted now, not when the game's loop next gets to its messages: player 1's
            // game can be busy for seconds as split screen starts [observed: the two-window
            // check of 2026-11-22, the desktop showed through for 3 s].
            UpdateWindow(h);
            Some(h as isize)
        }
    }

    pub fn place(h: isize, (at, size): (IVec2, UVec2)) {
        // SAFETY: this process's own backdrop; fails harmlessly once it is gone.
        unsafe {
            SetWindowPos(h as HWND, std::ptr::null_mut(), at.x, at.y, size.x as i32, size.y as i32, SWP_NOACTIVATE | SWP_NOZORDER);
        }
    }

    /// The text in `half` (screen pixels; `origin` is the backdrop's top left), or none.
    pub fn set_text(h: isize, half: Option<(IVec2, UVec2)>, origin: IVec2) {
        let rect = half.map(|(at, size)| {
            let (x, y) = (at.x - origin.x, at.y - origin.y);
            RECT { left: x, top: y, right: x + size.x as i32, bottom: y + size.y as i32 }
        });
        if let Ok(mut t) = TEXT.lock() {
            *t = rect;
        }
        // SAFETY: this process's own backdrop, painted again.
        unsafe {
            InvalidateRect(h as HWND, std::ptr::null(), 1);
        }
    }

    pub fn set_shown(h: isize, on: bool) {
        // SAFETY: this process's own backdrop.
        unsafe {
            ShowWindow(h as HWND, if on { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    }

    pub fn set_topmost(h: isize, on: bool) {
        // SAFETY: a window of the stack (this process's, or player 2's game's); its place
        // in the stack is set again right after (`put_under`).
        unsafe {
            SetWindowPos(h as HWND, if on { HWND_TOPMOST } else { HWND_NOTOPMOST }, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
        }
    }

    /// Whether the first shown window under `above` is `h`. Hidden windows between are
    /// left where they are: the games' own hidden windows sit there, and putting `h` back
    /// over them every frame moved the stack every frame [observed: the two-window check
    /// of 2026-11-22, 8192 restacks in 4 minutes].
    pub fn just_under(h: isize, above: isize) -> bool {
        let mut w = above as HWND;
        // SAFETY: only asks about windows.
        unsafe {
            for _ in 0..64 {
                w = GetWindow(w, GW_HWNDNEXT);
                if w.is_null() {
                    return false;
                }
                if w as isize == h {
                    return true;
                }
                if IsWindowVisible(w) != 0 {
                    return false;
                }
            }
        }
        false
    }

    /// `h` right under `above`, never activated. Asynchronous for a window of another
    /// thread (player 2's game's), so player 1's game never waits on player 2's while it
    /// loads [documented: SetWindowPos, SWP_ASYNCWINDOWPOS].
    pub fn put_under(h: isize, above: isize) {
        // SAFETY: a window of the stack (this process's, or player 2's game's).
        unsafe {
            SetWindowPos(h as HWND, above as HWND, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
        }
    }

    pub fn foreground() -> Option<isize> {
        // SAFETY: only asks which window has the focus.
        let h = unsafe { GetForegroundWindow() };
        (!h.is_null()).then_some(h as isize)
    }

    pub fn topmost(h: isize) -> bool {
        // SAFETY: only reads the window's extended style.
        unsafe { GetWindowLongPtrW(h as HWND, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0 }
    }

    pub fn iconic(h: isize) -> bool {
        // SAFETY: only asks about the window.
        unsafe { IsIconic(h as HWND) != 0 }
    }

    pub fn destroy(h: isize) {
        // SAFETY: this process's own backdrop, on the thread that made it.
        unsafe {
            DestroyWindow(h as HWND);
        }
        if let Ok(mut t) = TEXT.lock() {
            *t = None;
        }
    }

    pub fn alive(h: isize) -> bool {
        // SAFETY: only asks whether the handle is a window.
        unsafe { IsWindow(h as HWND) != 0 }
    }

    pub fn shown(h: isize) -> bool {
        // SAFETY: only asks about the window (another process's, player 2's).
        unsafe { IsWindow(h as HWND) != 0 && IsWindowVisible(h as HWND) != 0 }
    }
}

#[cfg(not(windows))]
mod imp {
    use bevy::math::{IVec2, UVec2};
    pub fn create(_screen: (IVec2, UVec2)) -> Option<isize> {
        None
    }
    pub fn place(_h: isize, _screen: (IVec2, UVec2)) {}
    pub fn set_text(_h: isize, _half: Option<(IVec2, UVec2)>, _origin: IVec2) {}
    pub fn set_shown(_h: isize, _on: bool) {}
    pub fn set_topmost(_h: isize, _on: bool) {}
    pub fn just_under(_h: isize, _above: isize) -> bool {
        true
    }
    pub fn put_under(_h: isize, _above: isize) {}
    pub fn foreground() -> Option<isize> {
        None
    }
    pub fn topmost(_h: isize) -> bool {
        false
    }
    pub fn iconic(_h: isize) -> bool {
        false
    }
    pub fn destroy(_h: isize) {}
    pub fn alive(_h: isize) -> bool {
        false
    }
    pub fn shown(_h: isize) -> bool {
        false
    }
}
