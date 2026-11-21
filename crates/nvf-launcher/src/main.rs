//! NVForever's launcher (#2708, docs/design/launcher.md): the project's news, Play/Continue
//! and the settings of the home's `NVForever.ini` in one window, beside `nvforever.exe`.
//! It writes only NVForever's own files (the INI, `plugins.txt`), never the game folder or
//! the original game's (`fnv_install::guard_write`).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod catalog;
mod display;
mod dlss;
mod fomod;
mod fonts;
mod launch;
mod modlist;
mod news;
mod reports;
mod start;
mod theme;
mod update;

const ICON: &[u8] = include_bytes!("../assets/icon-64.png");

fn main() -> eframe::Result {
    let (width, height) = std::env::var("NVF_LAUNCHER_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))
        })
        .filter(|(w, h)| *w >= 640.0 && *h >= 400.0 && *w <= 4000.0 && *h <= 4000.0)
        .unwrap_or((1180.0, 760.0));
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
