//! Player 2's half while player 2's window isn't showing (split.rs; the owner's list of
//! 2026-11-21, "while it's loading there's a blank space where it shows desktop behind
//! it"): player 1's game covers that half with a plain black panel saying player 2 is
//! joining. It is owned by player 1's window, so it has no taskbar button, is not in
//! Alt+Tab, never takes the focus and stays over player 1's window; it goes as soon as
//! player 2's window shows (`IsWindowVisible`), and comes back while player 2's game
//! starts again to load a character (its window is gone between the two runs,
//! `crate::handoff`). The screen still splits at once [owner], and the desktop never shows.
//!
//! A window of this game's own (Win32, the main thread's, whose messages the game's event
//! loop already dispatches), not a Bevy one: it draws nothing but black and a line of
//! text, with no GPU surface.

use bevy::prelude::*;

/// What the panel says [design: our own words].
pub const JOINING: &str = "Player 2 is joining\u{2026}";

/// The panel, while it is up.
#[derive(Default)]
pub struct Backdrop {
    hwnd: Option<isize>,
    at: Option<(IVec2, UVec2)>,
}

impl Backdrop {
    /// Shown over `half`, owned by `owner` (player 1's window): made the first time, moved
    /// when the half moves (the layout changed).
    pub fn show(&mut self, owner: isize, half: (IVec2, UVec2)) {
        if self.hwnd.is_some_and(|h| !imp::alive(h)) {
            self.hwnd = None;
        }
        match self.hwnd {
            None => {
                self.hwnd = imp::create(owner, half);
                self.at = Some(half);
                match self.hwnd {
                    Some(_) => info!("split: player 2's half covered until player 2's window shows ({}x{} at {:?})", half.1.x, half.1.y, half.0),
                    None => warn!("split: no panel over player 2's half: the desktop shows there until player 2's window does"),
                }
            }
            Some(h) if self.at != Some(half) => {
                imp::place(h, half);
                self.at = Some(half);
            }
            Some(_) => {}
        }
    }

    /// Gone (player 2's window shows, or split screen ended).
    pub fn hide(&mut self, why: &str) {
        if let Some(h) = self.hwnd.take() {
            imp::destroy(h);
            info!("split: player 2's half uncovered ({why})");
        }
        self.at = None;
    }
}

/// Whether window `hwnd` (another game's, player 2's) exists and is shown.
pub fn shown(hwnd: isize) -> bool {
    imp::shown(hwnd)
}

#[cfg(windows)]
mod imp {
    use bevy::math::{IVec2, UVec2};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, DeleteObject, DrawTextW, EndPaint, FillRect, GetStockObject, SelectObject, SetBkMode, SetTextColor, BLACK_BRUSH, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL, HBRUSH, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, IsWindow, IsWindowVisible, RegisterClassW, SetWindowPos, ShowWindow, SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOWNOACTIVATE, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_PAINT, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// `MA_NOACTIVATE`: a click on the panel never takes the focus from player 1's window.
    const MA_NOACTIVATE: LRESULT = 3;

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        match msg {
            WM_MOUSEACTIVATE => MA_NOACTIVATE,
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                // SAFETY: this window's own paint, between BeginPaint and EndPaint; the
                // font is made, used and deleted here.
                unsafe {
                    let mut ps: PAINTSTRUCT = std::mem::zeroed();
                    let dc = BeginPaint(hwnd, &mut ps);
                    let mut rect: RECT = std::mem::zeroed();
                    GetClientRect(hwnd, &mut rect);
                    FillRect(dc, &rect, GetStockObject(BLACK_BRUSH) as HBRUSH);
                    let height = ((rect.bottom - rect.top) / 22).clamp(18, 48);
                    let face = wide("Segoe UI");
                    let font = CreateFontW(height, 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, 0, CLEARTYPE_QUALITY as u32, FF_DONTCARE as u32, face.as_ptr());
                    let old = SelectObject(dc, font);
                    SetBkMode(dc, TRANSPARENT as i32);
                    SetTextColor(dc, 0x00A0_A0A0);
                    let mut text: Vec<u16> = super::JOINING.encode_utf16().collect();
                    DrawTextW(dc, text.as_mut_ptr(), text.len() as i32, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                    SelectObject(dc, old);
                    DeleteObject(font);
                    EndPaint(hwnd, &ps);
                }
                0
            }
            // SAFETY: the default handling of this window's other messages.
            _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
        }
    }

    pub fn create(owner: isize, (at, size): (IVec2, UVec2)) -> Option<isize> {
        let class = wide("NVForeverSplitBackdrop");
        let title = wide("NVForever");
        // SAFETY: a window class and a window of this process, made on the main thread
        // (the caller's `NonSendMarker`), whose messages the game's event loop dispatches.
        // Registering the class again fails harmlessly (it is the same class).
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance, lpszClassName: class.as_ptr(), hbrBackground: GetStockObject(BLACK_BRUSH) as HBRUSH, ..std::mem::zeroed() };
            RegisterClassW(&wc);
            let h = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, class.as_ptr(), title.as_ptr(), WS_POPUP, at.x, at.y, size.x as i32, size.y as i32, owner as HWND, std::ptr::null_mut(), instance, std::ptr::null());
            if h.is_null() {
                return None;
            }
            ShowWindow(h, SW_SHOWNOACTIVATE);
            Some(h as isize)
        }
    }

    pub fn place(h: isize, (at, size): (IVec2, UVec2)) {
        // SAFETY: this process's own panel; fails harmlessly once it is gone.
        unsafe {
            SetWindowPos(h as HWND, std::ptr::null_mut(), at.x, at.y, size.x as i32, size.y as i32, SWP_NOACTIVATE | SWP_NOZORDER);
        }
    }

    pub fn destroy(h: isize) {
        // SAFETY: this process's own panel, on the thread that made it.
        unsafe {
            DestroyWindow(h as HWND);
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
    pub fn create(_owner: isize, _half: (IVec2, UVec2)) -> Option<isize> {
        None
    }
    pub fn place(_h: isize, _half: (IVec2, UVec2)) {}
    pub fn destroy(_h: isize) {}
    pub fn alive(_h: isize) -> bool {
        false
    }
    pub fn shown(_h: isize) -> bool {
        false
    }
}
