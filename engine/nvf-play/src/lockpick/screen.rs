//! The lockpicking menu's text and buttons as `menus\lockpick_menu.xml` sets them out
//! [documented], over the lock's picture ([`super::LockView`], drawn by the engine).
//! Lengths are the interface's units (960 to the view's height).
//!
//! - The main rectangle is the view less four times the safe zone, centred; the safe zone
//!   is 15 [hypothesis: the file's `cropx`/`cropy` are the safe zone, `iSafeZoneX/Y` 15,
//!   docs/spec-notes/dialogue-menu.md §3], so everything is 30 in from the view's edges.
//! - The information box: 400 × 175, 10 in from the main rectangle's left and bottom, a
//!   bracket of lines (a line fading upwards at the top left, the left side, the bottom
//!   and a line fading to the right), and three rows in font 7 at 20 in and 35, 80 and
//!   125 down: the player's skill, the bobby pins and the lock's level, each title on
//!   the left and its value right-aligned 80 short of the box's width.
//! - The buttons: right-aligned 50 from the main rectangle's right, the first 72 under
//!   the information box's top: Force Lock (F) and Exit (E) under it.
//! - The status line: font 7, right-aligned with the buttons, 450 down the main rectangle.
//! - All in the HUD's colour, without glow (`&hudmain;`, `NOGLOW_BRANCH`).

use super::{LockView, LockpickInput, LockpickScreen, Session};
use crate::interface::{amber, at, face, plain_font, set_line, u, Face, Pictures};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_font::Line;
use fnv_game::lockpick::Phase;
use nvf_assets::GameData;

/// Font 7 [documented: the menu file's `<font> 7 </font>`; Fallout_default.ini
/// `sFontFile_7`].
const FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";
/// How far font 7's letters stand under a text's top, and its line (dialogue-menu.md).
const BASE: f32 = 19.5;
const LINE: f32 = 36.0;

/// From the view's edges to the main rectangle: twice the safe zone [hypothesis].
const EDGE: f32 = 30.0;
const INFO: Vec2 = Vec2::new(400.0, 175.0);
const INFO_IN: f32 = 10.0;
const ROW_X: f32 = 20.0;
const ROWS_Y: [f32; 3] = [35.0, 80.0, 125.0];
const VALUE_X: f32 = INFO.x - 80.0 + ROW_X;
/// Lines are 2 thick [documented: menus\globals.xml, on a PC]; the fading ones 30 long.
const THICK: f32 = 2.0;
const FADE: f32 = 30.0;
const BUTTONS_RIGHT: f32 = 50.0;
const BUTTONS_DOWN: f32 = 72.0;
/// A button is its text and 9 above and below [documented: `_verbuf` 9 of text_box.xml].
const BUTTON_H: f32 = LINE + 18.0;
/// The key hint after a button's text ("Exit E)") [design: as the barter menu's].
const HINT_GAP: f32 = 16.0;
const STATUS_Y: f32 = 450.0;
/// A button that cannot be used is drawn at half brightness [design].
const DIM: f32 = 128.0;
/// How dark the world is behind the menu when the engine draws no lock [design].
const SHADE: f32 = 0.88;

/// The menu's root node.
#[derive(Component)]
pub struct LockpickRoot;

/// A button and what it does.
#[derive(Component, Clone, Copy)]
pub struct LockButton(pub LockpickInput);

/// The font and the interface's pictures, read when the menu first opens.
pub struct Kit {
    font: Face,
    pictures: Pictures,
}

fn text_line(face: &Face, s: &str) -> Line {
    face.font.lines(s, None).into_iter().next().unwrap_or_default()
}

/// What the screen shows; drawn again when it changes.
fn view_key(s: &Session, has_view: bool) -> String {
    format!("{}:{}:{}:{:?}:{:?}:{}:{}", s.game.skill, s.game.pins, s.game.level.name(), s.status, s.game.phase == Phase::Picking && s.ended.is_none(), s.force_label(), has_view)
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    screen: Res<LockpickScreen>,
    view: Option<Res<LockView>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    roots: Query<Entity, With<LockpickRoot>>,
    mut kit: Local<Option<Kit>>,
    mut shown: Local<Option<String>>,
) {
    let key = screen.0.as_ref().map(|s| view_key(s, view.is_some()));
    if key == *shown {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    *shown = key;
    let Some(s) = screen.0.as_ref() else { return };
    let bc = crate::interface::bc_supported(formats.as_deref());
    let kit = kit.get_or_insert_with(|| Kit { font: face(FONT, plain_font(36.0, 15.0), data.as_deref(), &mut images, bc), pictures: Pictures::load(data.as_deref()) });
    spawn(&mut commands, kit, s, view.as_deref(), data.as_deref(), &mut images, bc);
}

struct Ctx<'a> {
    kit: &'a mut Kit,
    data: Option<&'a GameData>,
    images: &'a mut Assets<Image>,
    bc: bool,
}

impl Ctx<'_> {
    /// A picture of the interface stretched over `node` (a plain fill without it).
    fn image(&mut self, p: &mut ChildSpawnerCommands, path: &str, node: Node, color: Color) {
        match self.kit.pictures.picture(path, self.data, self.images, self.bc) {
            Some((image, rect)) => {
                p.spawn((node, ImageNode { rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }));
            }
            None => {
                p.spawn((node, BackgroundColor(color)));
            }
        }
    }

    /// Text in font 7 with its top at `top`; `right`: `x` is its right end. Its width.
    fn text(&mut self, p: &mut ChildSpawnerCommands, s: &str, x: f32, top: f32, right: bool, color: Color) -> f32 {
        let line = text_line(&self.kit.font, s);
        let x = if right { x - line.width } else { x };
        set_line(p, &self.kit.font, &line, x, top + BASE, color);
        line.width
    }
}

fn spawn(commands: &mut Commands, kit: &mut Kit, s: &Session, view: Option<&LockView>, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) {
    let mut c = Ctx { kit, data, images, bc };
    let bright = amber(255.0);
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    let mut root = commands.spawn((LockpickRoot, full.clone(), GlobalZIndex(106)));
    match view {
        // The lock as the engine draws it fills the view.
        Some(v) => {
            root.insert(BackgroundColor(Color::BLACK));
            root.with_children(|r| {
                r.spawn((full.clone(), ImageNode::new(v.image.clone())));
            });
        }
        None => {
            root.insert(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, SHADE)));
        }
    }
    root.with_children(|r| {
        // The main rectangle.
        let main = Node { position_type: PositionType::Absolute, left: u(EDGE), right: u(EDGE), top: u(EDGE), bottom: u(EDGE), ..default() };
        r.spawn(main).with_children(|m| {
            // The information box.
            let info = Node { position_type: PositionType::Absolute, left: u(INFO_IN), bottom: u(INFO_IN), width: u(INFO.x), height: u(INFO.y), ..default() };
            m.spawn(info).with_children(|b| {
                c.image(b, "interface\\shared\\line\\fade_to_top.dds", at(0.0, 0.0, THICK, FADE), bright);
                c.image(b, "interface\\shared\\line\\solid.dds", at(0.0, FADE - THICK, THICK, INFO.y - FADE), bright);
                c.image(b, "interface\\shared\\line\\solid.dds", at(0.0, INFO.y - THICK, INFO.x - FADE, THICK), bright);
                c.image(b, "interface\\shared\\line\\fade_to_right.dds", at(INFO.x - FADE - THICK, INFO.y - THICK, FADE, THICK), bright);
                let rows = [("Lockpick".to_string(), format!("{:.0}", s.game.skill)), ("Bobby Pins".into(), s.game.pins.to_string()), ("Lock Level".into(), s.game.level.name().to_string())];
                for ((title, value), y) in rows.iter().zip(ROWS_Y) {
                    c.text(b, title, ROW_X, y, false, bright);
                    c.text(b, value, VALUE_X, y, true, bright);
                }
            });
            // The buttons, right-aligned 50 from the right, from 72 under the box's top.
            let force_on = s.game.phase == Phase::Picking && s.ended.is_none();
            let buttons = [(s.force_label(), "F)", LockpickInput::Force(None), force_on), ("Exit".to_string(), "E)", LockpickInput::Exit, true)];
            let first = Node { position_type: PositionType::Absolute, right: u(BUTTONS_RIGHT), bottom: u(INFO_IN + INFO.y - BUTTONS_DOWN - 2.0 * BUTTON_H), width: u(0.0), height: u(2.0 * BUTTON_H), ..default() };
            m.spawn(first).with_children(|b| {
                for (i, (label, hint, cmd, on)) in buttons.into_iter().enumerate() {
                    let color = amber(if on { 255.0 } else { DIM });
                    let y = i as f32 * BUTTON_H;
                    let w = c.text(b, &label, 0.0, y + 9.0, true, color);
                    c.text(b, hint, HINT_GAP, y + 9.0, false, color);
                    if on {
                        b.spawn((Button, LockButton(cmd), at(-w - 10.0, y, w + 20.0 + HINT_GAP + 30.0, BUTTON_H)));
                    }
                }
            });
            // The status line.
            if let Some(status) = &s.status {
                let line = Node { position_type: PositionType::Absolute, right: u(BUTTONS_RIGHT), top: u(STATUS_Y), width: u(0.0), height: u(LINE), ..default() };
                m.spawn(line).with_children(|b| {
                    c.text(b, status, 0.0, 0.0, true, bright);
                });
            }
        });
    });
}
