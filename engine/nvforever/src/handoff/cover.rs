//! The new run's cover over the change of program (module notes of `crate::handoff`): no
//! gap and no visible switch between the two runs [owner 2026-11-22, a split-screen load:
//! "the closing of the program should be masked now. instead of the program closing and
//! seeing desktop, it should close and show the loading screen hang for a second", then
//! "there should be no gap or visible switch between programs"].
//!
//! Since queue item 132 the new run waits for the old run's process to end before it opens
//! its GPU device, so for a few seconds neither run had a window on the screen: the desktop
//! showed where the old window was [observed: the owner's video of 2026-11-22, 2.5 s of
//! desktop in player 1's half, then 2 s of black before the new loading screen].
//!
//! - **The picture.** Just before it tells the old run to end ([`up_over`], called from
//!   `before_gpu`), the new run copies what the screen shows where the old run's window is
//!   (its loading screen) and puts that picture up in a window of its own at the same place,
//!   over everything. The old window then goes under it, unseen.
//! - **No GPU.** A plain Win32 window on a thread of its own, painted from a GDI copy of
//!   the screen, so the two runs still never have a GPU device each (item 132).
//! - **Down.** Once the new run's window is shown, has drawn its own screen for a few
//!   frames and has a picture in it ([`drawn`]; in split screen, on its half with player
//!   1's black backdrop up under it), the cover goes ([`down`],
//!   `crate::handoff::lower_cover`): the new screen is there under it, so the picture stops
//!   and the screen goes on, with nothing between.
//! - **Over split screen's stack.** Player 1's game keeps its windows' order every frame
//!   (`crate::split::backdrop`); while the cover is up it is the top of that order
//!   ([`hwnd`]).
//! - **What it leaves.** Before it goes, the new run's window is put at the top of the
//!   windows that are not topmost ([`raise`]), where the old window was, so the cover never
//!   goes to show another program.
//!
//! The cover belongs to the new run's process: if that run dies, the cover goes with it.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

/// The cover's window while it is up (0: none).
static COVER: AtomicIsize = AtomicIsize::new(0);
/// Set by [`up_over_screen`]: the cover waits for split screen to end before it may go.
static TILL_SPLIT_ENDS: AtomicBool = AtomicBool::new(false);

/// Whether the cover is up.
pub fn is_up() -> bool {
    COVER.load(Ordering::Relaxed) != 0
}

/// Puts the cover up over the old run's window (process `pid`) and returns once it is on
/// the screen: where it is, or why there is none.
pub fn up_over(pid: u32) -> Result<String, String> {
    imp::up_over(pid)
}

/// Puts a cover up over the whole monitor of `window` (this run's), holding what is on
/// the screen now, for split screen's end on the way to the main menu: player 2's game
/// quit and its window went before player 1's window was back at its own size, and the
/// black backdrop showed over both halves for about 0.1 s [observed: the check of
/// 2026-11-22, item 215, cover6 15.91-15.97 s]. It stays while split screen is on and goes
/// like a load's cover (`crate::handoff::lower_cover`) once this window has drawn at its
/// own size and is in front.
pub fn up_over_screen(window: isize) -> Result<String, String> {
    let r = imp::up_over_screen(window);
    if r.is_ok() {
        TILL_SPLIT_ENDS.store(true, Ordering::Relaxed);
    }
    r
}

/// Whether the cover is [`up_over_screen`]'s, held while split screen is still on.
pub fn till_split_ends() -> bool {
    TILL_SPLIT_ENDS.load(Ordering::Relaxed)
}

/// Takes the cover down.
pub fn down() {
    TILL_SPLIT_ENDS.store(false, Ordering::Relaxed);
    let h = COVER.swap(0, Ordering::Relaxed);
    if h != 0 {
        imp::close(h);
    }
}

/// The cover's window while it is up (split screen's stack keeps it on top).
pub fn hwnd() -> Option<isize> {
    Some(COVER.load(Ordering::Relaxed)).filter(|&h| h != 0)
}

/// Whether `window` (this run's) has a picture in it: what Windows holds of its inside,
/// read even under the cover, is not all black. Frames counted were not enough: the window's
/// first picture at its split-screen half reached the screen 0.8 s after it showed, black
/// where the cover had been [observed: the check of 2026-11-22, item 215].
pub fn drawn(window: isize) -> bool {
    imp::drawn(window)
}

/// The new run's window to the top of the windows that are not topmost, without the focus,
/// so it is what the cover leaves on the screen: a run that Windows keeps from the focus
/// showed its window under other programs, and the cover going showed them [observed: the
/// check of 2026-11-22, item 215].
pub fn raise(window: isize) {
    imp::raise(window);
}

/// Whether `window` (this run's) is what the cover leaves on the screen: the first shown
/// window under the cover over its place, topmost ones aside (they are over any window).
/// The new main menu's window went under another program once with the cover up, and
/// the cover going showed that program [observed: the check of 2026-11-22, item 215,
/// with Windows' firewall question in front].
pub fn in_front(window: isize) -> bool {
    let h = COVER.load(Ordering::Relaxed);
    h == 0 || imp::in_front(h, window)
}

/// [`raise`], harder: through the topmost band and back, which Windows does for a window
/// of a program without the focus too [hypothesis: the usual way; checked by the 50 ms
/// captures of item 215].
pub fn raise_hard(window: isize) {
    imp::raise_hard(window);
}

#[cfg(windows)]
mod imp {
    use std::sync::mpsc;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, EndPaint, GetDC, GetPixel, ReleaseDC, SelectObject, SetStretchBltMode, StretchBlt, UpdateWindow, CAPTUREBLT, HALFTONE, HBITMAP, HGDIOBJ, PAINTSTRUCT, SRCCOPY,
    };
    use windows_sys::Win32::Storage::Xps::{PrintWindow, PW_CLIENTONLY};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows, GetClientRect, GetMessageW, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible, PostMessageW, PostQuitMessage, RegisterClassW, SetWindowPos, ShowWindow, TranslateMessage, GWL_EXSTYLE, HWND_TOP, HWND_TOPMOST, MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, PW_RENDERFULLCONTENT, SW_SHOWNOACTIVATE, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND,
        WM_MOUSEACTIVATE, WM_PAINT, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };

    use super::COVER;

    /// `MA_NOACTIVATE`: a click on the cover never takes the focus.
    const MA_NOACTIVATE: LRESULT = 3;

    /// The screen's picture the cover paints (an HBITMAP; one cover per run).
    static PICTURE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

    /// How long the new run waits for the cover to be painted before telling the old run
    /// anyway [engineering choice: it is painted within a few milliseconds].
    const PAINT_WAIT: Duration = Duration::from_millis(500);
    /// After it is painted, time for Windows to put it on the screen (a frame at 30 Hz,
    /// with room) before the old window may go [engineering choice].
    const COMPOSE_WAIT: Duration = Duration::from_millis(50);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        match msg {
            WM_MOUSEACTIVATE => MA_NOACTIVATE,
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                // SAFETY: this window's own paint, between BeginPaint and EndPaint; the
                // picture is this process's bitmap, selected into a DC made and deleted here.
                unsafe {
                    let mut ps: PAINTSTRUCT = std::mem::zeroed();
                    let dc = BeginPaint(hwnd, &mut ps);
                    let picture = PICTURE.load(std::sync::atomic::Ordering::Relaxed);
                    if picture != 0 {
                        let mem = CreateCompatibleDC(dc);
                        let old = SelectObject(mem, picture as HGDIOBJ);
                        let mut r: RECT = std::mem::zeroed();
                        GetWindowRect(hwnd, &mut r);
                        BitBlt(dc, 0, 0, r.right - r.left, r.bottom - r.top, mem, 0, 0, SRCCOPY);
                        SelectObject(mem, old);
                        DeleteDC(mem);
                    }
                    EndPaint(hwnd, &ps);
                }
                0
            }
            WM_CLOSE => {
                // SAFETY: this thread's own window.
                unsafe { DestroyWindow(hwnd) };
                0
            }
            WM_DESTROY => {
                // SAFETY: ends this thread's message loop.
                unsafe { PostQuitMessage(0) };
                0
            }
            // SAFETY: the default handling of this window's other messages.
            _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
        }
    }

    /// The old run's largest shown window that is not a tool window (split screen's
    /// backdrop is one): its game window.
    fn window_of(pid: u32) -> Option<(HWND, RECT)> {
        struct Find {
            pid: u32,
            best: Option<(HWND, RECT)>,
        }
        unsafe extern "system" fn each(h: HWND, l: LPARAM) -> i32 {
            // SAFETY: `l` is the `Find` below, alive for the whole `EnumWindows` call;
            // the rest only asks about windows.
            unsafe {
                let f = &mut *(l as *mut Find);
                let mut owner = 0u32;
                GetWindowThreadProcessId(h, &mut owner);
                if owner != f.pid || IsWindowVisible(h) == 0 || IsIconic(h) != 0 || GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0 {
                    return 1;
                }
                let mut r: RECT = std::mem::zeroed();
                if GetWindowRect(h, &mut r) == 0 {
                    return 1;
                }
                let area = |r: &RECT| i64::from(r.right - r.left) * i64::from(r.bottom - r.top);
                if f.best.as_ref().is_none_or(|(_, b)| area(&r) > area(b)) {
                    f.best = Some((h, r));
                }
                1
            }
        }
        let mut f = Find { pid, best: None };
        // SAFETY: the callback only reads windows and writes `f`, which outlives the call.
        unsafe { EnumWindows(Some(each), &mut f as *mut Find as LPARAM) };
        f.best.filter(|(_, r)| r.right > r.left && r.bottom > r.top)
    }

    /// A copy of what the screen shows in `r` (an HBITMAP), or none.
    fn copy_screen(r: &RECT) -> Option<HBITMAP> {
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        // SAFETY: the screen's DC, released here; a bitmap and DC of this process, the DC
        // deleted here and the bitmap kept for the cover's paint.
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            if screen.is_null() {
                return None;
            }
            let mem = CreateCompatibleDC(screen);
            let bmp = CreateCompatibleBitmap(screen, w, h);
            let ok = !mem.is_null() && !bmp.is_null() && {
                let old = SelectObject(mem, bmp as HGDIOBJ);
                let ok = BitBlt(mem, 0, 0, w, h, screen, r.left, r.top, SRCCOPY | CAPTUREBLT) != 0;
                SelectObject(mem, old);
                ok
            };
            if !mem.is_null() {
                DeleteDC(mem);
            }
            ReleaseDC(std::ptr::null_mut(), screen);
            if ok {
                Some(bmp)
            } else {
                if !bmp.is_null() {
                    DeleteObject(bmp as HGDIOBJ);
                }
                None
            }
        }
    }

    pub fn up_over(pid: u32) -> Result<String, String> {
        let Some((_, r)) = window_of(pid) else {
            return Err(format!("the old run (process {pid}) has no window showing"));
        };
        put_up(r)
    }

    pub fn up_over_screen(window: isize) -> Result<String, String> {
        use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
        // SAFETY: only asks about this run's window's monitor.
        let r = unsafe {
            let m = MonitorFromWindow(window as HWND, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if m.is_null() || GetMonitorInfoW(m, &mut info) == 0 {
                return Err("its monitor is not known".into());
            }
            info.rcMonitor
        };
        put_up(r)
    }

    fn put_up(r: RECT) -> Result<String, String> {
        if super::is_up() {
            return Err("a cover is up already".into());
        }
        let Some(bmp) = copy_screen(&r) else {
            return Err("the screen could not be copied".into());
        };
        PICTURE.store(bmp as isize, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = mpsc::channel::<Option<isize>>();
        let spawned = std::thread::Builder::new().name("handoff cover".into()).spawn(move || {
            let class = wide("NVForeverHandoffCover");
            let title = wide("NVForever");
            // SAFETY: a window class and a window of this thread, whose messages the loop
            // below dispatches until the window is destroyed; the bitmap is deleted after.
            unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
                RegisterClassW(&wc);
                let h = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST, class.as_ptr(), title.as_ptr(), WS_POPUP, r.left, r.top, r.right - r.left, r.bottom - r.top, std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null());
                if h.is_null() {
                    let _ = tx.send(None);
                } else {
                    ShowWindow(h, SW_SHOWNOACTIVATE);
                    UpdateWindow(h);
                    let _ = tx.send(Some(h as isize));
                    let mut msg: MSG = std::mem::zeroed();
                    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                let picture = PICTURE.swap(0, std::sync::atomic::Ordering::Relaxed);
                if picture != 0 {
                    DeleteObject(picture as HGDIOBJ);
                }
            }
        });
        if let Err(e) = spawned {
            return Err(format!("no thread for it: {e}"));
        }
        match rx.recv_timeout(PAINT_WAIT) {
            Ok(Some(h)) => {
                COVER.store(h, std::sync::atomic::Ordering::Relaxed);
                std::thread::sleep(COMPOSE_WAIT);
                Ok(format!("{}x{} at {},{}", r.right - r.left, r.bottom - r.top, r.left, r.top))
            }
            Ok(None) => Err("its window could not be made".into()),
            Err(_) => Err(format!("not painted within {} ms", PAINT_WAIT.as_millis())),
        }
    }

    pub fn close(h: isize) {
        // SAFETY: the cover's thread destroys its own window on this message.
        unsafe { PostMessageW(h as HWND, WM_CLOSE, 0, 0) };
    }

    /// Samples (16 by 16) of the window's inside that must not be black, and the level of
    /// red, green and blue together that counts as lit [engineering choice].
    const SAMPLES: i32 = 16;
    const LIT_SAMPLES: u32 = 8;
    const LIT_LEVEL: u32 = 48;

    pub fn drawn(window: isize) -> bool {
        let hwnd = window as HWND;
        // SAFETY: this run's own window, copied into bitmaps and DCs of this process, all
        // made, read and deleted here.
        unsafe {
            let mut r: RECT = std::mem::zeroed();
            if GetClientRect(hwnd, &mut r) == 0 || r.right <= 0 || r.bottom <= 0 {
                return false;
            }
            let screen = GetDC(std::ptr::null_mut());
            if screen.is_null() {
                return false;
            }
            let full = CreateCompatibleDC(screen);
            let full_bmp = CreateCompatibleBitmap(screen, r.right, r.bottom);
            let small = CreateCompatibleDC(screen);
            let small_bmp = CreateCompatibleBitmap(screen, SAMPLES, SAMPLES);
            let mut lit = 0;
            if !full.is_null() && !full_bmp.is_null() && !small.is_null() && !small_bmp.is_null() {
                let old_full = SelectObject(full, full_bmp as HGDIOBJ);
                let old_small = SelectObject(small, small_bmp as HGDIOBJ);
                if PrintWindow(hwnd, full, PW_CLIENTONLY | PW_RENDERFULLCONTENT) != 0 {
                    SetStretchBltMode(small, HALFTONE);
                    StretchBlt(small, 0, 0, SAMPLES, SAMPLES, full, 0, 0, r.right, r.bottom, SRCCOPY);
                    for y in 0..SAMPLES {
                        for x in 0..SAMPLES {
                            let c = GetPixel(small, x, y);
                            if (c & 0xFF) + ((c >> 8) & 0xFF) + ((c >> 16) & 0xFF) > LIT_LEVEL {
                                lit += 1;
                            }
                        }
                    }
                }
                SelectObject(full, old_full);
                SelectObject(small, old_small);
            }
            for dc in [full, small] {
                if !dc.is_null() {
                    DeleteDC(dc);
                }
            }
            for bmp in [full_bmp, small_bmp] {
                if !bmp.is_null() {
                    DeleteObject(bmp as HGDIOBJ);
                }
            }
            ReleaseDC(std::ptr::null_mut(), screen);
            lit >= LIT_SAMPLES
        }
    }

    pub fn in_front(cover: isize, window: isize) -> bool {
        use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindow, GW_HWNDNEXT};
        // SAFETY: only asks about windows; every handle comes from Windows' own list.
        unsafe {
            let mut c: RECT = std::mem::zeroed();
            if GetWindowRect(cover as HWND, &mut c) == 0 {
                return true;
            }
            let mut w = cover as HWND;
            for _ in 0..4096 {
                w = GetWindow(w, GW_HWNDNEXT);
                if w.is_null() {
                    return false;
                }
                if w as isize == window {
                    return true;
                }
                if IsWindowVisible(w) == 0 || IsIconic(w) != 0 || GetWindowLongPtrW(w, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0 {
                    continue;
                }
                let mut cloaked: u32 = 0;
                if DwmGetWindowAttribute(w, DWMWA_CLOAKED as u32, (&mut cloaked as *mut u32).cast(), 4) == 0 && cloaked != 0 {
                    continue;
                }
                let mut r: RECT = std::mem::zeroed();
                if GetWindowRect(w, &mut r) == 0 || r.right <= c.left || r.left >= c.right || r.bottom <= c.top || r.top >= c.bottom {
                    continue;
                }
                return false;
            }
            false
        }
    }

    pub fn raise_hard(window: isize) {
        use windows_sys::Win32::UI::WindowsAndMessaging::HWND_NOTOPMOST;
        // SAFETY: this run's own window, moved in the stack only, never activated.
        unsafe {
            SetWindowPos(window as HWND, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            SetWindowPos(window as HWND, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    pub fn raise(window: isize) {
        // SAFETY: this run's own window, moved in the stack only, never activated.
        unsafe {
            SetWindowPos(window as HWND, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn up_over(_pid: u32) -> Result<String, String> {
        Err("not made on this system".into())
    }
    pub fn up_over_screen(_window: isize) -> Result<String, String> {
        Err("not made on this system".into())
    }
    pub fn close(_h: isize) {}
    pub fn raise(_window: isize) {}
    pub fn raise_hard(_window: isize) {}
    pub fn in_front(_cover: isize, _window: isize) -> bool {
        true
    }
    pub fn drawn(_window: isize) -> bool {
        true
    }
}
