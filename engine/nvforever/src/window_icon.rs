//! The program icon on the game window (title bar, taskbar, Alt+Tab). The executable carries
//! the same picture as a resource (build.rs), which Windows shows before the window opens.

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::WINIT_WINDOWS;

const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

pub struct WindowIconPlugin;

impl Plugin for WindowIconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_icon);
    }
}

/// Tries each frame until the winit window exists, then sets the icon once.
fn set_icon(mut done: Local<bool>, primary: Query<Entity, With<PrimaryWindow>>, _main_thread: NonSendMarker) {
    if *done {
        return;
    }
    let Ok(entity) = primary.single() else { return };
    WINIT_WINDOWS.with_borrow(|windows| {
        let Some(window) = windows.get_window(entity) else { return };
        *done = true;
        match icon() {
            Ok(icon) => window.set_window_icon(Some(icon)),
            Err(e) => warn!("window icon: {e}"),
        }
    });
}

fn icon() -> Result<winit::window::Icon, String> {
    let image = bevy::image::Image::from_buffer(
        ICON_PNG,
        bevy::image::ImageType::Extension("png"),
        default(),
        true,
        default(),
        default(),
    )
    .map_err(|e| e.to_string())?;
    let (w, h) = (image.width(), image.height());
    let rgba = image.data.ok_or("no pixels")?;
    winit::window::Icon::from_rgba(rgba, w, h).map_err(|e| e.to_string())
}
