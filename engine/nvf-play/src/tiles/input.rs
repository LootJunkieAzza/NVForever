//! T8: the pointer, the mouse buttons and the keys fed to the menus drawn from tiles
//! (`fnv_game::ui_input`, `fnv_ui::input`), each frame before scripts [design:
//! ui-tile-tree.md §4.1 step 1].
//!
//! - **Which menus take input**: a menu drawn from its file that the player works. None
//!   does yet: the HUD (`[UI] TileMenus=HUDMainMenu`) is drawn from tiles but has no
//!   pointer in play [hypothesis: the original's HUD takes no clicks]. So nothing is fed
//!   and the frame does no tree work [design: the maintainer's review of #902], until a
//!   menu moves to the tile renderer, or the test aid below makes the HUD take input.
//! - **Units**: the cursor's logical pixels to interface units, 960 to the view's height,
//!   as the tile renderer draws (`super::draw`, `crate::interface::u`) [design].
//! - **Keys**: A to Z as `_PCButton_<letter>` [documented: the vanilla files' letters;
//!   LStewieAl's Tweaks adds digits, F-keys, Caps, Tab and Alt, so the unmodded game has
//!   none of those]; the arrow keys as `xup` … `xright` [documented: Input]; the
//!   controller's buttons as their `x…` traits (A `xbuttona`, the shoulders `xbuttonlb`
//!   `xbuttonrb`, the triggers `xbuttonlt` `xbuttonrt`, the sticks' buttons, Start, the
//!   D-pad) [design: the Xbox names the files use]; MenuTapKey's keys (`Game::ui_taps`)
//!   as one frame's presses [documented: MenuTapKey, "fakes a key press for one frame in
//!   menu mode"]. A press is a key going down [hypothesis].
//! - The text tiles without a size are not hit: the fonts' measure lives in the renderer
//!   (`super::draw`), and the HUD's measured texts have their width written into the tree
//!   (`FontKit::write_widths`) [design].
//!
//! Test aid `NVF_PLAY_TEST_POINTER=<seconds>:<x>:<y>[:click][,…]`: from that time the HUD
//! (drawn from tiles) takes input as an open menu would, with the pointer at (x, y) in
//! interface units, and `click` a left-button press on that frame; `<seconds>:off` lets
//! it go. The mouse, keys and controller then reach it too (`NVF_PLAY_TEST_KEYS`).

use crate::script::ScriptState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_ui::input::{no_measure, Input};

pub const AID: &str = "NVF_PLAY_TEST_POINTER";

/// One item of the test aid: a place and whether it clicks, or off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AidPointer {
    At { x: f64, y: f64, click: bool },
    Off,
}

/// `<x>:<y>[:click]` or `off`.
pub fn aid_pointer(rest: &str) -> Result<AidPointer, String> {
    if rest.trim().eq_ignore_ascii_case("off") {
        return Ok(AidPointer::Off);
    }
    let mut parts = rest.split(':').map(str::trim);
    let num = |s: Option<&str>| s.and_then(|s| s.parse::<f64>().ok()).ok_or_else(|| format!("{rest:?}: expected <x>:<y>[:click] or off"));
    let x = num(parts.next())?;
    let y = num(parts.next())?;
    let click = match parts.next() {
        None => false,
        Some(c) if c.eq_ignore_ascii_case("click") => true,
        Some(c) => return Err(format!("{rest:?}: unknown {c:?}")),
    };
    Ok(AidPointer::At { x, y, click })
}

/// The test aid's state: what is still to come, and the pointer it holds now.
#[derive(Resource, Default)]
pub(crate) struct PointerAid {
    due: Vec<(f32, AidPointer)>,
    now: Option<(f64, f64)>,
}

/// The letters' keys, A to Z.
const LETTERS: [(KeyCode, &str); 26] = [
    (KeyCode::KeyA, "A"), (KeyCode::KeyB, "B"), (KeyCode::KeyC, "C"), (KeyCode::KeyD, "D"), (KeyCode::KeyE, "E"), (KeyCode::KeyF, "F"),
    (KeyCode::KeyG, "G"), (KeyCode::KeyH, "H"), (KeyCode::KeyI, "I"), (KeyCode::KeyJ, "J"), (KeyCode::KeyK, "K"), (KeyCode::KeyL, "L"),
    (KeyCode::KeyM, "M"), (KeyCode::KeyN, "N"), (KeyCode::KeyO, "O"), (KeyCode::KeyP, "P"), (KeyCode::KeyQ, "Q"), (KeyCode::KeyR, "R"),
    (KeyCode::KeyS, "S"), (KeyCode::KeyT, "T"), (KeyCode::KeyU, "U"), (KeyCode::KeyV, "V"), (KeyCode::KeyW, "W"), (KeyCode::KeyX, "X"),
    (KeyCode::KeyY, "Y"), (KeyCode::KeyZ, "Z"),
];

const ARROWS: [(KeyCode, &str); 4] = [(KeyCode::ArrowUp, "xup"), (KeyCode::ArrowDown, "xdown"), (KeyCode::ArrowLeft, "xleft"), (KeyCode::ArrowRight, "xright")];

const PAD: [(GamepadButton, &str); 15] = [
    (GamepadButton::South, "xbuttona"),
    (GamepadButton::East, "xbuttonb"),
    (GamepadButton::West, "xbuttonx"),
    (GamepadButton::North, "xbuttony"),
    (GamepadButton::LeftTrigger2, "xbuttonlt"),
    (GamepadButton::RightTrigger2, "xbuttonrt"),
    (GamepadButton::LeftTrigger, "xbuttonlb"),
    (GamepadButton::RightTrigger, "xbuttonrb"),
    (GamepadButton::LeftThumb, "xbuttonls"),
    (GamepadButton::RightThumb, "xbuttonrs"),
    (GamepadButton::Start, "xbuttonstart"),
    (GamepadButton::DPadUp, "xup"),
    (GamepadButton::DPadDown, "xdown"),
    (GamepadButton::DPadLeft, "xleft"),
    (GamepadButton::DPadRight, "xright"),
];

/// A key's name for the tiles: a letter for `_PCButton_*`, or an arrow's trait.
pub fn key_name(k: KeyCode) -> Option<(bool, &'static str)> {
    LETTERS.iter().find(|(c, _)| *c == k).map(|(_, n)| (true, *n)).or_else(|| ARROWS.iter().find(|(c, _)| *c == k).map(|(_, n)| (false, *n)))
}

/// Adds a key once: a letter to the keys, an arrow or a button to the pads (the D-pad and
/// the arrow keys it stands for in menus, `crate::pad::menu_keys`, press `xup` once).
fn push(input: &mut Input, (letter, name): (bool, &'static str)) {
    let list = if letter { &mut input.keys } else { &mut input.pads };
    if !list.iter().any(|n| n == name) {
        list.push(name.to_string());
    }
}

/// A window position (logical pixels from the top left) in interface units.
pub fn to_units(at: Vec2, window_height: f32) -> (f64, f64) {
    let per = f64::from(window_height.max(1.0)) / 960.0;
    (f64::from(at.x) / per, f64::from(at.y) / per)
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Devices<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    pads: Query<'w, 's, &'static Gamepad>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
}

pub(crate) fn feed(mut script: ResMut<ScriptState>, menus: Res<super::vanilla::TileMenus>, aid: Option<ResMut<PointerAid>>, time: crate::pause::AidTime, devices: Devices) {
    let Some(rt) = script.rt.as_mut() else { return };
    if rt.game.ui_files.is_none() {
        return;
    }
    // The test aid's pointer, moved on its clock.
    let mut aid_click = false;
    let mut aid_at = None;
    if let Some(mut aid) = aid {
        let now = time.elapsed_secs();
        let mut fired: Vec<(f32, AidPointer)> = aid.due.iter().copied().filter(|(t, _)| *t <= now).collect();
        fired.sort_by(|a, b| a.0.total_cmp(&b.0));
        aid.due.retain(|(t, _)| *t > now);
        for (_, item) in fired {
            match item {
                AidPointer::At { x, y, click } => {
                    info!("test aid: tile pointer at {x} {y}{}", if click { ", click" } else { "" });
                    aid.now = Some((x, y));
                    aid_click |= click;
                }
                AidPointer::Off => {
                    info!("test aid: tile pointer off");
                    aid.now = None;
                }
            }
        }
        aid_at = aid.now;
    }
    // The menus that take input, back to front: none in play yet (see the module doc).
    let mut taking = Vec::new();
    if aid_at.is_some() && menus.hud {
        taking.extend(rt.game.ui_tree.menu(super::draw::HUD_MENU));
    }
    if taking.is_empty() {
        // A highlight left from the last menu that took input goes (mouseover back to 0);
        // otherwise nothing to do [design].
        if rt.game.ui_pointer.over.is_some() {
            fnv_game::ui_input::step(&mut rt.game, &[], &Input::default(), &no_measure);
            info!("tiles: pointer over nothing (no menu takes input)");
        }
        return;
    }
    // The aid's pointer, else the cursor in the window.
    let at = aid_at.or_else(|| devices.windows.single().ok().and_then(|w| Some(to_units(w.cursor_position()?, w.height()))));
    let mut input = Input {
        at,
        press: aid_click || devices.mouse.just_pressed(MouseButton::Left),
        shift: devices.keys.pressed(KeyCode::ShiftLeft) || devices.keys.pressed(KeyCode::ShiftRight),
        ..Input::default()
    };
    for k in devices.keys.get_just_pressed() {
        if let Some(n) = key_name(*k) {
            push(&mut input, n);
        }
    }
    for code in std::mem::take(&mut rt.game.ui_taps) {
        if let Some(n) = u8::try_from(code).ok().and_then(crate::bindings::dik_key).and_then(key_name) {
            push(&mut input, n);
        }
    }
    for (button, name) in PAD {
        if devices.pads.iter().any(|g| g.just_pressed(button)) {
            push(&mut input, (false, name));
        }
    }
    let report = fnv_game::ui_input::step(&mut rt.game, &taking, &input, &no_measure);
    let ui = &rt.game.ui_tree;
    if let Some((_, to)) = report.over {
        match to.filter(|&t| ui.is_live(t)) {
            Some(t) => {
                let id = ui.tile(t).get("id").map_or_else(|| "no id".to_string(), |v| format!("id {}", v.text()));
                let over = ui.tile(t).get(fnv_ui::input::MOUSEOVER).map_or(0.0, fnv_ui::Value::num);
                info!("tiles: pointer over {} ({id}), mouseover {over}", ui.path_of(t));
            }
            None => info!("tiles: pointer over nothing"),
        }
    }
    for c in &report.clicks {
        if ui.is_live(c.tile) {
            let clicked = ui.tile(c.tile).get(fnv_ui::input::CLICKED).map_or(0.0, fnv_ui::Value::num);
            info!("tiles: {}, clicked {clicked}", fnv_game::ui_input::click_line(ui, c));
        }
    }
    for line in &report.unfollowed {
        info!("tiles: key not followed: {line}");
    }
}

/// Read the test aid at start.
pub(crate) fn setup(app: &mut App) {
    let Ok(v) = std::env::var(AID) else { return };
    match crate::test_aid::timed(&v).and_then(|items| items.into_iter().map(|(s, r)| aid_pointer(r).map(|p| (s, p))).collect::<Result<Vec<_>, _>>()) {
        Ok(due) => {
            app.insert_resource(PointerAid { due, now: None });
        }
        Err(e) => warn!("{AID}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aid_items() {
        assert_eq!(aid_pointer("120:340"), Ok(AidPointer::At { x: 120.0, y: 340.0, click: false }));
        assert_eq!(aid_pointer(" 1.5 : 2 : click "), Ok(AidPointer::At { x: 1.5, y: 2.0, click: true }));
        assert_eq!(aid_pointer("OFF"), Ok(AidPointer::Off));
        assert!(aid_pointer("12").is_err());
        assert!(aid_pointer("1:2:press").is_err());
        let items = crate::test_aid::timed("3:10:20,4:10:20:click,9:off").unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(aid_pointer(items[1].1), Ok(AidPointer::At { x: 10.0, y: 20.0, click: true }));
    }

    #[test]
    fn keys_and_units() {
        assert_eq!(key_name(KeyCode::KeyE), Some((true, "E")));
        assert_eq!(key_name(KeyCode::ArrowDown), Some((false, "xdown")));
        assert_eq!(key_name(KeyCode::Digit1), None, "no digits in the unmodded game");
        // 960 units to the height: at 1280 x 720, a pixel is 4/3 of a unit.
        assert_eq!(to_units(Vec2::new(640.0, 360.0), 720.0), (853.3333333333334, 480.0));
        assert_eq!(to_units(Vec2::new(0.0, 960.0), 960.0), (0.0, 960.0));
    }
}
