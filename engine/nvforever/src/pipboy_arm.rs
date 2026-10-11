//! The Pip-Boy on the first-person arms (docs/spec-notes/pipboy.md §1): the model's
//! screen shows the menus' image while the Pip-Boy is up (`nvf_play::pipboy`), the
//! button of the page shown is lit, and the window's pointer is followed onto the screen
//! and the three buttons.
//!
//! The raise itself is the arms rig's clip (`player_body.rs`, `ClipKey::PipBoy`).
//!
//! - The parts, by shape name in `pipboy3000\pipboyarm.nif` [observed]: the screen
//!   `pipboyscreen:0`; the buttons `PipBoyButton01`, `02` and `03` (each two shapes), at
//!   the glows `StatsGlow:0`, `ItemsGlow:0` and `DataGlow:0` (the same places, a little
//!   above them): STATS, ITEMS, DATA from the elbow out.
//! - The screen's texture coordinates run from 0 to about 0.75 both ways (u −0.010 to
//!   0.753, v 0.002 to 0.762) over a 4:3 shape [observed]. The canvas fills 0.8 of the
//!   texture each way, as a 1024 × 768 canvas in the top left of a 1280 × 960 image
//!   [observed: fitted to the owner's ITEMS screenshot, where the menus are 1.07 times
//!   as wide and 1.10 times as high as a fill of 0.75 gave]; so the screen shows the
//!   canvas up to about 964 across and 731 down, and the image the menus are drawn into
//!   (`nvf_play::pipboy::TARGET`) is laid out the same way.
//! - The screen with the menus is drawn unlit, as a lit display [hypothesis], in the
//!   original's amber ([`SCREEN_TINT`], fitted). The game's `fBrightenPipboy=1.3`
//!   [documented: `Fallout_default.ini` `[InterfaceFX]`] is not applied as such: what it
//!   brightens is not known, and the fit already matches the screenshot.
//! - The model's glass is not drawn while the menus are on the screen: `ScreenLit:8`, a
//!   lit and glossy copy of the screen in the same place, and `glare:0`, a sheet added
//!   over it with `ScreenGlare.dds` [observed: their properties]. Drawn, the glare lay
//!   over the menus as a pale grey-blue haze the owner's screenshots do not have
//!   [observed]; those show a faint lighter patch at the top right and an empty screen
//!   of about (31, 27, 15) where ours is (15, 9, 0), so the original draws the glare far
//!   more weakly than NVForever does [hypothesis; open, docs/owner-review.md]. The
//!   screen's own material and the glass come back when the Pip-Boy is down.
//! - While the Pip-Boy is up only the page's button glows [observed: the owner's
//!   screenshots: the button of the page shown is bright, the other two dimmer]; down,
//!   all three glow, as before (owner, 2026-10-10).

use crate::player_body::PlayerBody;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use nvf_assets::nif_material::NifMaterial;
use nvf_play::pipboy::{Page, PipBoy, PipBoyInput, PipBoyScreen, CANVAS, TARGET};
use nvf_play::PlayerCamera;

/// The part of the screen's texture the canvas fills [observed, module docs].
pub const UV_EXTENT: f32 = 0.8;

/// What the screen multiplies the menus' picture by (linear): so that their brightest
/// amber comes out as the original's on a 1280 × 720 frame at noon in Goodsprings
/// [observed: fitted; the owner's ITEMS screenshot's solid CND bar is about (220, 180,
/// 64), and with this the menus' brightest text is (221, 179, 59); unscaled it was
/// (237, 167, 53), the view's tone mapping squeezing the brightest values].
const SCREEN_TINT: [f32; 3] = [0.83, 1.17, 1.27];

/// A part of the Pip-Boy model the menus use.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PipBoyPart {
    Screen,
    /// The screen's glass (`ScreenLit:8`, `glare:0`).
    Glass,
    Glow(Page),
    Button(Page),
}

/// The part a shape of the Pip-Boy model is, by its name.
pub fn part(name: &str) -> Option<PipBoyPart> {
    let n = name.trim().to_ascii_lowercase();
    let page = |s: &str| match s {
        "01" => Some(Page::Stats),
        "02" => Some(Page::Items),
        "03" => Some(Page::Data),
        _ => None,
    };
    match n.as_str() {
        "pipboyscreen:0" => Some(PipBoyPart::Screen),
        "screenlit:8" | "glare:0" => Some(PipBoyPart::Glass),
        "statsglow:0" => Some(PipBoyPart::Glow(Page::Stats)),
        "itemsglow:0" => Some(PipBoyPart::Glow(Page::Items)),
        "dataglow:0" => Some(PipBoyPart::Glow(Page::Data)),
        _ => n.strip_prefix("pipboybutton").and_then(|r| page(r.get(..2)?)).map(PipBoyPart::Button),
    }
}

/// Where a spot of the screen's texture lies on the canvas.
pub fn canvas_point(uv: Vec2) -> Option<Vec2> {
    let p = uv / UV_EXTENT * CANVAS;
    (p.x >= 0.0 && p.y >= 0.0 && p.x < CANVAS.x && p.y < CANVAS.y).then_some(p)
}

pub struct PipBoyArmPlugin;

impl Plugin for PipBoyArmPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (show_screen, follow_pointer).chain().after(nvf_play::player::BodySet).before(nvf_play::PlaySet::Ui));
    }
}

/// The screen's own material while the menus are on it.
#[derive(Default)]
struct Swapped {
    screen: Option<(Entity, Handle<NifMaterial>)>,
    menus: Option<Handle<NifMaterial>>,
}

#[allow(clippy::too_many_arguments)]
fn show_screen(
    mut commands: Commands,
    pip: Res<PipBoy>,
    screen: Option<ResMut<PipBoyScreen>>,
    body: Res<PlayerBody>,
    parts: Query<(Entity, &PipBoyPart, &MeshMaterial3d<NifMaterial>)>,
    mut visibility: Query<&mut Visibility>,
    mut materials: ResMut<Assets<NifMaterial>>,
    mut swapped: Local<Swapped>,
) {
    let Some(mut screen) = screen else { return };
    let shown = pip.shown() && body.first_person_drawn();
    let model_screen = parts.iter().find(|(_, p, _)| **p == PipBoyPart::Screen).map(|(e, _, m)| (e, m.0.clone()));
    // Give back the screen's own material when the menus leave it (or the rig was rebuilt).
    if let Some((e, own)) = swapped.screen.clone() {
        if !shown || model_screen.as_ref().is_none_or(|(s, _)| *s != e) {
            if let Ok(mut ec) = commands.get_entity(e) {
                ec.insert(MeshMaterial3d(own));
            }
            swapped.screen = None;
        }
    }
    screen.on_model = shown && model_screen.is_some();
    if let (true, Some((e, own))) = (shown, model_screen) {
        if swapped.screen.is_none() {
            let menus = swapped.menus.get_or_insert_with(|| {
                let mut m = materials.get(&own).cloned().unwrap_or_default();
                let scale = (CANVAS / TARGET.as_vec2()) / UV_EXTENT;
                let [r, g, b] = SCREEN_TINT;
                m.base.base_color = Color::LinearRgba(LinearRgba::rgb(r, g, b));
                m.base.base_color_texture = Some(screen.image.clone());
                m.base.emissive_texture = None;
                m.base.normal_map_texture = None;
                m.base.unlit = true;
                m.base.alpha_mode = AlphaMode::Opaque;
                m.base.uv_transform = bevy::math::Affine2::from_scale(scale);
                materials.add(m)
            });
            commands.entity(e).insert(MeshMaterial3d(menus.clone()));
            swapped.screen = Some((e, own));
            info!("pipboy: the menus are on the model's screen");
        }
    }
    // The glows: the page's alone while the Pip-Boy shows, all three otherwise. The
    // glass is not drawn while the menus are on the screen.
    for (e, p, _) in &parts {
        let shown = match *p {
            PipBoyPart::Glow(page) => !pip.shown() || page == pip.page,
            PipBoyPart::Glass => !screen.on_model,
            _ => continue,
        };
        let want = if shown { Visibility::Inherited } else { Visibility::Hidden };
        if let Ok(mut v) = visibility.get_mut(e) {
            if *v != want {
                *v = want;
            }
        }
    }
}

/// Follow the window's pointer onto the model: where it lies on the screen goes to the
/// menus (`PipBoyScreen::pointer`); a click on a button shows its page.
#[allow(clippy::too_many_arguments)]
fn follow_pointer(
    pip: Res<PipBoy>,
    screen: Option<ResMut<PipBoyScreen>>,
    cams: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    parts: Query<&PipBoyPart>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut ray_cast: MeshRayCast,
    mut out: MessageWriter<PipBoyInput>,
) {
    let Some(mut screen) = screen else { return };
    if !screen.on_model {
        return;
    }
    screen.pointer = None;
    if !pip.open() {
        return;
    }
    let (Ok((cam, at)), Ok(window)) = (cams.single(), windows.single()) else { return };
    let Some(ray) = window.cursor_position().and_then(|c| cam.viewport_to_world(at, c).ok()) else { return };
    let ours = |e: Entity| parts.contains(e);
    let settings = MeshRayCastSettings::default().with_visibility(RayCastVisibility::Any).with_filter(&ours);
    let Some((e, hit)) = ray_cast.cast_ray(ray, &settings).first() else { return };
    match parts.get(*e) {
        Ok(PipBoyPart::Screen) => screen.pointer = hit.uv.and_then(canvas_point),
        Ok(PipBoyPart::Button(page)) if mouse.just_pressed(MouseButton::Left) => {
            out.write(PipBoyInput::Page(*page));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parts_are_known_by_their_shapes() {
        assert_eq!(part("pipboyscreen:0"), Some(PipBoyPart::Screen));
        assert_eq!(part("ItemsGlow:0"), Some(PipBoyPart::Glow(Page::Items)));
        assert_eq!(part("PipBoyButton01:1"), Some(PipBoyPart::Button(Page::Stats)));
        assert_eq!(part("PipBoyButton03:0"), Some(PipBoyPart::Button(Page::Data)));
        assert_eq!(part("ScreenLit:8"), Some(PipBoyPart::Glass));
        assert_eq!(part("glare:0"), Some(PipBoyPart::Glass));
        assert_eq!(part("PipBoyArm:0"), None);
    }

    #[test]
    fn the_screen_maps_onto_the_canvas() {
        assert_eq!(canvas_point(Vec2::ZERO), Some(Vec2::ZERO));
        let p = canvas_point(Vec2::new(0.4, 0.4)).unwrap();
        assert!((p - CANVAS / 2.0).length() < 1e-3);
        assert_eq!(canvas_point(Vec2::new(0.81, 0.1)), None);
    }
}
