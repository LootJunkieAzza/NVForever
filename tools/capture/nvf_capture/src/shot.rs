//! Screenshots for `screenshot` steps (plugin version 7; docs/spec-notes/xnvse-capture.md §4).
//!
//! The picture is copied from the screen where the game's window is, through Windows'
//! own GDI calls (the same pixels any screen-capture tool sees); like the rest of the
//! plugin it reads no game memory and does not touch Direct3D. This works for a window
//! the desktop compositor draws (windowed or borderless; the owner's install runs
//! windowed at 1280 × 720) and the window must be in front: whatever covers it is in the
//! picture. The game stops running when it loses the focus anyway
//! (docs/spec-notes/physics.md §6.6), so a capture that is running is in front; the
//! output line still says whether it was (`foreground`), and its mean brightness, so a
//! black or covered picture can be told apart without opening it.

use std::fs;
use std::io::BufWriter;
use std::path::Path;

/// A picture: `width × height` pixels, rows top to bottom, 3 bytes (R, G, B) each.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
    /// Whether the window was the foreground window when it was copied.
    pub foreground: bool,
}

impl Picture {
    /// Mean of all channels, 0 (black) to 1 (white).
    pub fn mean(&self) -> f64 {
        if self.rgb.is_empty() {
            return 0.0;
        }
        self.rgb.iter().map(|&b| u64::from(b)).sum::<u64>() as f64 / (self.rgb.len() as f64 * 255.0)
    }
}

/// Converts 32-bit B, G, R, x rows (what a 32-bit GDI DIB holds) to R, G, B.
pub fn bgrx_to_rgb(bgrx: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(bgrx.len() / 4 * 3);
    for px in bgrx.as_chunks::<4>().0 {
        rgb.extend_from_slice(&[px[2], px[1], px[0]]);
    }
    rgb
}

/// Writes the picture as an 8-bit RGB PNG.
pub fn write_png(pic: &Picture, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let f = fs::File::create(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(BufWriter::new(f), pic.width, pic.height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    w.write_image_data(&pic.rgb).map_err(|e| e.to_string())?;
    w.finish().map_err(|e| e.to_string())
}

/// Copies the client area of this process's main window (the largest visible one).
#[cfg(windows)]
pub fn grab_game_window() -> Result<Picture, String> {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, GetDC,
        ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClientRect, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    };

    struct Best {
        pid: u32,
        hwnd: HWND,
        area: i64,
    }

    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let best = &mut *(lparam as *mut Best);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != best.pid || IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return 1;
        }
        let mut r: RECT = zeroed();
        if GetClientRect(hwnd, &mut r) != 0 {
            let area = i64::from(r.right - r.left) * i64::from(r.bottom - r.top);
            if area > best.area {
                best.area = area;
                best.hwnd = hwnd;
            }
        }
        1
    }

    // SAFETY: plain Win32 calls on the game's main thread; every handle created here is
    // released before returning; the DIB's bits are read only while it is alive.
    unsafe {
        let mut best = Best { pid: GetCurrentProcessId(), hwnd: std::ptr::null_mut(), area: 0 };
        EnumWindows(Some(visit), &mut best as *mut Best as LPARAM);
        if best.hwnd.is_null() {
            return Err("no visible game window".into());
        }
        let hwnd = best.hwnd;
        let mut r: RECT = zeroed();
        GetClientRect(hwnd, &mut r);
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        if w <= 0 || h <= 0 {
            return Err("game window has an empty client area".into());
        }
        let mut origin = POINT { x: 0, y: 0 };
        ClientToScreen(hwnd, &mut origin);

        let screen = GetDC(std::ptr::null_mut());
        if screen.is_null() {
            return Err("GetDC failed".into());
        }
        let mem = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(mem, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        let result = if mem.is_null() || dib.is_null() || bits.is_null() {
            Err("cannot create a bitmap".to_string())
        } else {
            let old = SelectObject(mem, dib);
            let ok = BitBlt(mem, 0, 0, w, h, screen, origin.x, origin.y, SRCCOPY) != 0;
            GdiFlush();
            let r = if ok {
                let n = (w as usize) * (h as usize) * 4;
                let bgrx = std::slice::from_raw_parts(bits as *const u8, n);
                Ok(Picture { width: w as u32, height: h as u32, rgb: bgrx_to_rgb(bgrx), foreground: GetForegroundWindow() == hwnd })
            } else {
                Err("BitBlt failed".to_string())
            };
            SelectObject(mem, old);
            r
        };
        if !dib.is_null() {
            DeleteObject(dib);
        }
        if !mem.is_null() {
            DeleteDC(mem);
        }
        ReleaseDC(std::ptr::null_mut(), screen);
        result
    }
}

#[cfg(not(windows))]
pub fn grab_game_window() -> Result<Picture, String> {
    Err("screenshots need Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_bgrx_and_measures_brightness() {
        let rgb = bgrx_to_rgb(&[1, 2, 3, 0xFF, 10, 20, 30, 0]);
        assert_eq!(rgb, [3, 2, 1, 30, 20, 10]);
        let black = Picture { width: 2, height: 1, rgb: vec![0; 6], foreground: true };
        assert_eq!(black.mean(), 0.0);
        let white = Picture { rgb: vec![255; 6], ..black };
        assert_eq!(white.mean(), 1.0);
    }

    #[test]
    fn writes_a_png_that_reads_back() {
        let dir = std::env::temp_dir().join(format!("nvf_capture_shot_{}", std::process::id()));
        let path = dir.join("shots").join("a.png");
        let pic = Picture { width: 2, height: 2, rgb: vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9], foreground: false };
        write_png(&pic, &path).unwrap();
        let dec = png::Decoder::new(fs::File::open(&path).unwrap());
        let mut reader = dec.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height, info.color_type), (2, 2, png::ColorType::Rgb));
        assert_eq!(&buf[..info.buffer_size()], &pic.rgb[..]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
