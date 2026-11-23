//! NVForever's launcher (#2708, docs/design/launcher.md): the project's news, Play/Continue
//! and the settings of the home's `NVForever.ini` in one window, beside `nvforever.exe`.
//! It writes only NVForever's own files (the INI, `plugins.txt`), never the game folder or
//! the original game's (`fnv_install::guard_write`).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod catalog;
mod display;
mod dlss;
mod fonts;
mod launch;
mod news;
mod reports;
mod start;
mod theme;
mod update;

const ICON: &[u8] = include_bytes!("../assets/icon-64.png");

/// The window's first size: `NVF_LAUNCHER_SIZE=<width>x<height>` when it is a size from
/// 640x400 to 4000x4000 (a test aid, for screenshots at a fixed size), else 1180x760.
fn first_size(asked: Option<&str>) -> (f32, f32) {
    asked
        .and_then(|s| {
            let (w, h) = s.trim().split_once('x')?;
            Some((w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?))
        })
        .filter(|(w, h)| (640.0..=4000.0).contains(w) && (400.0..=4000.0).contains(h))
        .unwrap_or((1180.0, 760.0))
}

fn main() -> eframe::Result {
    let (width, height) = first_size(std::env::var("NVF_LAUNCHER_SIZE").ok().as_deref());
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("NVForever Launcher")
        .with_inner_size([width, height])
        .with_min_inner_size([900.0, 560.0]);

    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("NVForever Launcher", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_first_size_takes_only_a_sensible_size() {
        use super::first_size;
        assert_eq!(first_size(Some("1600x900")), (1600.0, 900.0));
        assert_eq!(first_size(Some(" 800 x 600 ")), (800.0, 600.0));
        for bad in [None, Some(""), Some("big"), Some("100x100"), Some("9000x900"), Some("1600"), Some("x900")] {
            assert_eq!(first_size(bad), (1180.0, 760.0), "{bad:?}");
        }
    }
}
