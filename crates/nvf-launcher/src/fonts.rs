//! The launcher's fonts: the system's own interface font when it has one (Windows: Segoe UI;
//! Linux: Noto Sans or DejaVu Sans), read from the system's font folder at start, else egui's
//! built-in ones. Nothing is shipped or downloaded: the font files stay the system's. These
//! fonts have taller lines than egui's Ubuntu-Light, which leaves the news room to breathe
//! (contributors' feedback 2026-11-15: "very squished together").

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::path::PathBuf;
use std::sync::Arc;

/// The family for titles and headings (a semibold face when the system has one).
pub const TITLE: &str = "title";

/// Candidate (regular, semibold) files, first found wins.
fn candidates() -> Vec<(PathBuf, Option<PathBuf>)> {
    let mut v = Vec::new();
    if cfg!(windows) {
        let dir = std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from).join("Fonts");
        v.push((dir.join("segoeui.ttf"), Some(dir.join("seguisb.ttf"))));
    } else {
        for dir in ["/usr/share/fonts/truetype/noto", "/usr/share/fonts/noto", "/usr/share/fonts/google-noto"] {
            let d = PathBuf::from(dir);
            v.push((d.join("NotoSans-Regular.ttf"), Some(d.join("NotoSans-SemiBold.ttf"))));
        }
        for dir in ["/usr/share/fonts/truetype/dejavu", "/usr/share/fonts/dejavu", "/usr/share/fonts/TTF"] {
            let d = PathBuf::from(dir);
            v.push((d.join("DejaVuSans.ttf"), Some(d.join("DejaVuSans-Bold.ttf"))));
        }
    }
    v
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let found = candidates().into_iter().find_map(|(regular, title)| {
        let r = std::fs::read(&regular).ok()?;
        Some((r, title.and_then(|t| std::fs::read(t).ok())))
    });
    if let Some((regular, title)) = found {
        fonts.font_data.insert("system".into(), Arc::new(FontData::from_owned(regular)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "system".into());
        let mut title_family = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
        if let Some(t) = title {
            fonts.font_data.insert("system-title".into(), Arc::new(FontData::from_owned(t)));
            title_family.insert(0, "system-title".into());
        }
        fonts.families.insert(FontFamily::Name(TITLE.into()), title_family);
    } else {
        let p = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
        fonts.families.insert(FontFamily::Name(TITLE.into()), p);
    }
    ctx.set_fonts(fonts);
}

/// A title font of `size` points.
pub fn title(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name(TITLE.into()))
}
