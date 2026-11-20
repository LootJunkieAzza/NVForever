//! Split screen's view (plan `split-screen.md` §2, §5): the field of view of a half.
//!
//! The game's field of view keeps its vertical angle at every aspect ("Hor+",
//! `nvf_play::free_cam::vertical_fov`), so a top-and-bottom half (32:9 on a 16:9 screen)
//! sees about 128 degrees across at the default 75 and everything in it is drawn small
//! and stretched towards the sides [observed: the first two-window check, 2026-11-20].
//! While split, a half wider than 16:9 keeps the horizontal angle the same setting has at
//! 16:9 instead, so it shows the full-screen view's middle band at the same size; a side-by-
//! side half (8:9) keeps the vertical angle as always [design: plan §2, "top/bottom keeps
//! the player's horizontal FOV; side by side keeps the vertical FOV of the same setting at
//! 16:9"]. Only the player's camera, only while split; the field of view setting stays
//! the base.
//!
//! How: every system sets the camera's field of view as it does without split screen.
//! Just before the camera's matrices are made (`CameraUpdateSystems`) the base is swapped
//! for the half's ([`split_vertical_fov`]), and at the next frame's start it is swapped
//! back, so what reads or eases the field of view during the frame (the sights' zoom,
//! dialogue, photo mode, scripts) sees the base.

use bevy::camera::CameraUpdateSystems;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// The widest aspect whose view keeps the setting's vertical angle while split: 16:9.
pub const WIDEST_HOR_PLUS: f32 = 16.0 / 9.0;

/// The vertical field of view (radians) the player's camera draws a split-screen half of
/// `aspect` (width over height) with, for the vertical field of view `vfov` the game
/// asks for: as asked up to 16:9; wider, the horizontal angle `vfov` has at 16:9.
pub fn split_vertical_fov(vfov: f32, aspect: f32) -> f32 {
    if aspect.is_nan() || aspect <= WIDEST_HOR_PLUS || !vfov.is_finite() {
        return vfov;
    }
    2.0 * ((vfov / 2.0).tan() * WIDEST_HOR_PLUS / aspect).atan()
}

/// On the player's camera while the half's field of view is on it: the game's own (`base`)
/// and the one put on for drawing (`drawn`).
#[derive(Component, Debug, Clone, Copy)]
struct HalfFov {
    base: f32,
    drawn: f32,
}

pub struct SplitViewPlugin;

impl Plugin for SplitViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(First, give_back).add_systems(PostUpdate, put_on.before(CameraUpdateSystems));
    }
}

/// Split screen is on in this game: player 2's always, player 1's while its menus' split
/// screen is on.
fn split_on(p1: Option<&nvf_play::split_screen::SplitScreen>, p2: bool) -> bool {
    p2 || p1.is_some_and(|s| s.on)
}

/// The frame's start: the game's own field of view back on the camera, unless something
/// set another since ([`put_on`] takes that as the base).
fn give_back(mut commands: Commands, mut cams: Query<(Entity, &mut Projection, &HalfFov)>) {
    for (e, mut p, half) in &mut cams {
        if let Projection::Perspective(p) = &mut *p {
            if p.fov == half.drawn {
                p.fov = half.base;
            }
        }
        commands.entity(e).remove::<HalfFov>();
    }
}

/// Before the camera's matrices: the half's field of view on the player's camera.
fn put_on(
    mut commands: Commands,
    p1: Option<Res<nvf_play::split_screen::SplitScreen>>,
    p2: Option<Res<nvf_play::split_screen::PlayerTwo>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cams: Query<(Entity, &mut Projection), With<nvf_play::PlayerCamera>>,
    mut logged: Local<Option<(u32, u32)>>,
) {
    if !split_on(p1.as_deref(), p2.is_some()) {
        *logged = None;
        return;
    }
    let Ok(w) = windows.single() else { return };
    let (width, height) = (w.resolution.physical_width(), w.resolution.physical_height());
    if height == 0 {
        return;
    }
    let aspect = width as f32 / height as f32;
    for (e, mut p) in &mut cams {
        let Projection::Perspective(p) = &mut *p else { continue };
        let base = p.fov;
        let drawn = split_vertical_fov(base, aspect);
        if drawn == base {
            continue;
        }
        if *logged != Some((width, height)) {
            *logged = Some((width, height));
            let across = |v: f32| (2.0 * ((v / 2.0).tan() * aspect).atan()).to_degrees();
            info!("split: the view of a {width}x{height} half: {:.1} degrees across ({:.1} high), not {:.1}", across(drawn), drawn.to_degrees(), across(base));
        }
        p.fov = drawn;
        commands.entity(e).insert(HalfFov { base, drawn });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvf_play::free_cam::vertical_fov;

    fn across(vfov: f32, aspect: f32) -> f32 {
        (2.0 * ((vfov / 2.0).tan() * aspect).atan()).to_degrees()
    }

    #[test]
    fn a_top_and_bottom_half_sees_as_much_across_as_16_9() {
        let v = vertical_fov(75.0);
        let wide = split_vertical_fov(v, 32.0 / 9.0);
        assert!((across(wide, 32.0 / 9.0) - across(v, 16.0 / 9.0)).abs() < 1e-3);
        assert!((across(v, 16.0 / 9.0) - 91.3).abs() < 0.1, "75 is 91.3 across at 16:9");
        assert!((across(v, 32.0 / 9.0) - 127.9).abs() < 0.1, "Hor+ alone: 127.9 across");
        // The full view's middle band: half its height, in the angle's tangent.
        assert!(((wide / 2.0).tan() - (v / 2.0).tan() / 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_side_by_side_half_and_a_16_9_view_keep_the_vertical_angle() {
        let v = vertical_fov(90.0);
        assert_eq!(split_vertical_fov(v, 8.0 / 9.0), v);
        assert_eq!(split_vertical_fov(v, 16.0 / 9.0), v);
        assert_eq!(split_vertical_fov(v, f32::NAN), v);
        assert_eq!(split_vertical_fov(f32::NAN, 4.0).to_bits(), f32::NAN.to_bits());
    }

    /// The base the game sets during a frame comes back at the next frame's start; one set
    /// after the half's was put on is taken as the new base.
    #[test]
    fn the_games_own_field_of_view_is_back_each_frame() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(SplitViewPlugin).insert_resource(nvf_play::split_screen::PlayerTwo);
        app.world_mut().spawn((Window { resolution: bevy::window::WindowResolution::new(2560, 720), ..default() }, PrimaryWindow));
        let v = vertical_fov(75.0);
        let cam = app.world_mut().spawn((nvf_play::PlayerCamera, Projection::Perspective(PerspectiveProjection { fov: v, ..default() }))).id();
        let fov = |app: &App| match app.world().get::<Projection>(cam) {
            Some(Projection::Perspective(p)) => p.fov,
            _ => f32::NAN,
        };
        app.update();
        let drawn = split_vertical_fov(v, 2560.0 / 720.0);
        assert_eq!(fov(&app), drawn);
        app.update();
        assert_eq!(fov(&app), drawn, "the same each frame, not compounded");
        // The setting changes between frames: the new base.
        let zoomed = vertical_fov(55.0);
        if let Some(mut p) = app.world_mut().get_mut::<Projection>(cam) {
            if let Projection::Perspective(p) = &mut *p {
                p.fov = zoomed;
            }
        }
        app.update();
        assert_eq!(fov(&app), split_vertical_fov(zoomed, 2560.0 / 720.0));
        // Split screen over: the base stays.
        app.world_mut().remove_resource::<nvf_play::split_screen::PlayerTwo>();
        app.update();
        assert_eq!(fov(&app), zoomed);
    }
}
