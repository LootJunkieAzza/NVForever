//! The game's controls and their default keys and mouse buttons (docs/design/
//! player-body.md §1.2). Every control of the game's `[Controls]` INI section is here,
//! bound as the game binds it by default; systems ask for a control
//! ([`ControlInput::pressed`]) instead of a key.
//!
//! The defaults are the owner's `FalloutPrefs.ini` `[Controls]` values [observed
//! 2026-10-09; hypothesis: they are the game's defaults, nothing in them is rebound].
//! Each value is four bytes `00 KK MM JJ` [observed: the layout fits every line]:
//! a DirectInput keyboard scan code `KK` [documented: Microsoft DirectInput `DIK_*`
//! keyboard constants], a mouse button `MM` (0 left, 1 right, 2 middle; `FF` none —
//! `Use` = 0 attacks, `Block` = 1 aims, by their names) and a gamepad button `JJ`
//! (not used). NVForever never reads the game's INIs at run time
//! (docs/design/install-layout.md); the values are built in.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

/// The game's controls, in the INI's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Control {
    Forward,
    Back,
    SlideLeft,
    SlideRight,
    /// Attack.
    Use,
    Activate,
    /// Aim down the sights (guns) or block (melee and fists).
    Block,
    /// Draw the weapon, reload it, or (held) holster it.
    ReadyItem,
    CrouchSneak,
    /// Walk while Always Run is on (run while it is off).
    Run,
    AlwaysRun,
    AutoMove,
    Jump,
    /// First / third person; held with the mouse moving, circle the camera.
    TogglePov,
    /// The Pip-Boy.
    MenuMode,
    /// Wait.
    Rest,
    Vats,
    Hotkey1,
    /// The INI names the second hotkey `Ammo Swap`.
    AmmoSwap,
    Hotkey3,
    Hotkey4,
    Hotkey5,
    Hotkey6,
    Hotkey7,
    Hotkey8,
    QuickSave,
    QuickLoad,
    Grab,
}

/// `[Controls]` of the owner's `FalloutPrefs.ini`: (control, INI name, value)
/// [observed 2026-10-09].
pub const GAME_CONTROLS: [(Control, &str, u32); 28] = [
    (Control::Forward, "Forward", 0x0011FF13),
    (Control::Back, "Back", 0x001FFF14),
    (Control::SlideLeft, "Slide Left", 0x001EFF17),
    (Control::SlideRight, "Slide Right", 0x0020FF16),
    (Control::Use, "Use", 0x00FF0011),
    (Control::Activate, "Activate", 0x0012FF0A),
    (Control::Block, "Block", 0x00380110),
    (Control::ReadyItem, "Ready Item", 0x0013FF0C),
    (Control::CrouchSneak, "Crouch/Sneak", 0x001DFF08),
    (Control::Run, "Run", 0x002AFFFF),
    (Control::AlwaysRun, "Always Run", 0x003AFFFF),
    (Control::AutoMove, "Auto Move", 0x0010FFFF),
    (Control::Jump, "Jump", 0x0039FF0D),
    (Control::TogglePov, "Toggle POV", 0x0021020F),
    (Control::MenuMode, "Menu Mode", 0x000FFF0B),
    (Control::Rest, "Rest", 0x0014FF07),
    (Control::Vats, "Vats", 0x002FFF0E),
    (Control::Hotkey1, "Hotkey1", 0x0002FFFF),
    (Control::AmmoSwap, "Ammo Swap", 0x0003FF01),
    (Control::Hotkey3, "Hotkey3", 0x0004FFFF),
    (Control::Hotkey4, "Hotkey4", 0x0005FFFF),
    (Control::Hotkey5, "Hotkey5", 0x0006FFFF),
    (Control::Hotkey6, "Hotkey6", 0x0007FFFF),
    (Control::Hotkey7, "Hotkey7", 0x0008FFFF),
    (Control::Hotkey8, "Hotkey8", 0x0009FFFF),
    (Control::QuickSave, "QuickSave", 0x003FFFFF),
    (Control::QuickLoad, "QuickLoad", 0x0043FFFF),
    (Control::Grab, "Grab", 0x002CFF09),
];

/// The rest of the owner's `[Controls]`: `bAlwaysRunByDefault=1`, `bInvertYValues=0`,
/// `fMouseSensitivity=0.0020` [observed 2026-10-09].
pub const ALWAYS_RUN_BY_DEFAULT: bool = true;
pub const INVERT_Y: bool = false;
/// Radians of view turn per mouse count [hypothesis: the unit of `fMouseSensitivity`].
pub const MOUSE_SENSITIVITY: f32 = 0.0020;

/// The key of a DirectInput keyboard scan code [documented: Microsoft `DIK_*` values].
pub fn dik_key(code: u8) -> Option<KeyCode> {
    use KeyCode::*;
    Some(match code {
        0x01 => Escape,
        0x02 => Digit1,
        0x03 => Digit2,
        0x04 => Digit3,
        0x05 => Digit4,
        0x06 => Digit5,
        0x07 => Digit6,
        0x08 => Digit7,
        0x09 => Digit8,
        0x0A => Digit9,
        0x0B => Digit0,
        0x0C => Minus,
        0x0D => Equal,
        0x0E => Backspace,
        0x0F => Tab,
        0x10 => KeyQ,
        0x11 => KeyW,
        0x12 => KeyE,
        0x13 => KeyR,
        0x14 => KeyT,
        0x15 => KeyY,
        0x16 => KeyU,
        0x17 => KeyI,
        0x18 => KeyO,
        0x19 => KeyP,
        0x1A => BracketLeft,
        0x1B => BracketRight,
        0x1C => Enter,
        0x1D => ControlLeft,
        0x1E => KeyA,
        0x1F => KeyS,
        0x20 => KeyD,
        0x21 => KeyF,
        0x22 => KeyG,
        0x23 => KeyH,
        0x24 => KeyJ,
        0x25 => KeyK,
        0x26 => KeyL,
        0x27 => Semicolon,
        0x28 => Quote,
        0x29 => Backquote,
        0x2A => ShiftLeft,
        0x2B => Backslash,
        0x2C => KeyZ,
        0x2D => KeyX,
        0x2E => KeyC,
        0x2F => KeyV,
        0x30 => KeyB,
        0x31 => KeyN,
        0x32 => KeyM,
        0x33 => Comma,
        0x34 => Period,
        0x35 => Slash,
        0x36 => ShiftRight,
        0x37 => NumpadMultiply,
        0x38 => AltLeft,
        0x39 => Space,
        0x3A => CapsLock,
        0x3B => F1,
        0x3C => F2,
        0x3D => F3,
        0x3E => F4,
        0x3F => F5,
        0x40 => F6,
        0x41 => F7,
        0x42 => F8,
        0x43 => F9,
        0x44 => F10,
        0x57 => F11,
        0x58 => F12,
        0x9D => ControlRight,
        0xB8 => AltRight,
        0xC8 => ArrowUp,
        0xCB => ArrowLeft,
        0xCD => ArrowRight,
        0xD0 => ArrowDown,
        _ => return None,
    })
}

/// The mouse button of an INI mouse byte (0 left, 1 right, 2 middle, 3–7 extra; `FF` none).
pub fn ini_mouse(code: u8) -> Option<MouseButton> {
    Some(match code {
        0 => MouseButton::Left,
        1 => MouseButton::Right,
        2 => MouseButton::Middle,
        3 => MouseButton::Back,
        4 => MouseButton::Forward,
        5..=7 => MouseButton::Other(u16::from(code)),
        _ => return None,
    })
}

/// A control's key and mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Binding {
    pub key: Option<KeyCode>,
    pub mouse: Option<MouseButton>,
}

impl Binding {
    /// From an INI value `00 KK MM JJ`.
    pub fn from_ini(v: u32) -> Binding {
        Binding { key: dik_key((v >> 16) as u8), mouse: ini_mouse((v >> 8) as u8) }
    }
}

/// The bindings in use.
#[derive(Resource, Debug, Clone)]
pub struct Bindings {
    map: Vec<(Control, Binding)>,
    pub always_run_by_default: bool,
    pub invert_y: bool,
    pub mouse_sensitivity: f32,
}

impl Default for Bindings {
    fn default() -> Self {
        Bindings::game_defaults()
    }
}

impl Bindings {
    pub fn game_defaults() -> Bindings {
        Bindings {
            map: GAME_CONTROLS.iter().map(|&(c, _, v)| (c, Binding::from_ini(v))).collect(),
            always_run_by_default: ALWAYS_RUN_BY_DEFAULT,
            invert_y: INVERT_Y,
            mouse_sensitivity: MOUSE_SENSITIVITY,
        }
    }

    pub fn get(&self, c: Control) -> Binding {
        self.map.iter().find(|(k, _)| *k == c).map_or_else(Binding::default, |(_, b)| *b)
    }

    /// The control's name in the game's INI.
    pub fn name(c: Control) -> &'static str {
        GAME_CONTROLS.iter().find(|(k, _, _)| *k == c).map_or("?", |(_, n, _)| n)
    }
}

/// The controls' state this frame.
#[derive(SystemParam)]
pub struct ControlInput<'w> {
    pub keys: Res<'w, ButtonInput<KeyCode>>,
    pub mouse: Res<'w, ButtonInput<MouseButton>>,
    pub bindings: Res<'w, Bindings>,
}

impl ControlInput<'_> {
    pub fn pressed(&self, c: Control) -> bool {
        let b = self.bindings.get(c);
        b.key.is_some_and(|k| self.keys.pressed(k)) || b.mouse.is_some_and(|m| self.mouse.pressed(m))
    }

    pub fn just_pressed(&self, c: Control) -> bool {
        let b = self.bindings.get(c);
        b.key.is_some_and(|k| self.keys.just_pressed(k)) || b.mouse.is_some_and(|m| self.mouse.just_pressed(m))
    }

    /// Released this frame, and neither of its inputs is still down.
    pub fn just_released(&self, c: Control) -> bool {
        let b = self.bindings.get(c);
        let released = b.key.is_some_and(|k| self.keys.just_released(k)) || b.mouse.is_some_and(|m| self.mouse.just_released(m));
        released && !self.pressed(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_values_decode() {
        let b = Bindings::game_defaults();
        assert_eq!(b.get(Control::Forward), Binding { key: Some(KeyCode::KeyW), mouse: None });
        assert_eq!(b.get(Control::Use), Binding { key: None, mouse: Some(MouseButton::Left) });
        assert_eq!(b.get(Control::Block), Binding { key: Some(KeyCode::AltLeft), mouse: Some(MouseButton::Right) });
        assert_eq!(b.get(Control::TogglePov), Binding { key: Some(KeyCode::KeyF), mouse: Some(MouseButton::Middle) });
        assert_eq!(b.get(Control::CrouchSneak).key, Some(KeyCode::ControlLeft));
        assert_eq!(b.get(Control::AlwaysRun).key, Some(KeyCode::CapsLock));
        assert_eq!(b.get(Control::AutoMove).key, Some(KeyCode::KeyQ));
        assert_eq!(b.get(Control::MenuMode).key, Some(KeyCode::Tab));
        assert_eq!(b.get(Control::AmmoSwap).key, Some(KeyCode::Digit2));
        assert_eq!(b.get(Control::QuickLoad).key, Some(KeyCode::F9));
        assert_eq!(Bindings::name(Control::Rest), "Rest");
        // Every control has a key or a button.
        for (c, _, _) in GAME_CONTROLS {
            let x = b.get(c);
            assert!(x.key.is_some() || x.mouse.is_some(), "{c:?}");
        }
    }

    #[test]
    fn input_follows_bindings() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>().init_resource::<ButtonInput<MouseButton>>().init_resource::<Bindings>();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Middle);
        let mut sys = bevy::ecs::system::SystemState::<ControlInput>::new(app.world_mut());
        let c = sys.get(app.world()).expect("resources");
        assert!(c.pressed(Control::TogglePov) && c.just_pressed(Control::TogglePov));
        assert!(!c.pressed(Control::Use));
    }
}
