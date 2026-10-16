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
//!
//! **The player's own keys** are kept in `[Controls]` of the home's `NVForever.ini`
//! (beside `[Settings]`, `crate::settings::SettingsFile`), one line per control by its
//! game INI name, the key and mouse button by name: `Forward=W`, `Block=Left Alt, Right
//! Mouse`, `Use=Left Mouse`, `Auto Move=None` ([`KEY_NAMES`], [`MOUSE_NAMES`]), and
//! `Always Run By Default`, `Invert Y` (1/0) and `Mouse Sensitivity`. Each start reads
//! them ([`apply_stored`], `PostStartup`); a line missing from an INI that exists is
//! written with the control's key then, so the file always lists every control to edit.
//! A value that does not parse is skipped with a warning (the default stays). The pause
//! menu's Controls page (`crate::pause`) rebinds a control ([`rebind`]): the new key or
//! button is taken from any other control that had it, which gets the old one instead
//! (a swap, so no control is left without its input by accident), and the section is
//! written again. Esc and the console's key (`` ` ``) cannot be bound [design: they
//! open the pause menu and the console].

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use fnv_install::{guard_write, ini_get, ini_set};
use std::path::Path;

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

    /// The control of an INI name (without case).
    pub fn by_name(name: &str) -> Option<Control> {
        GAME_CONTROLS.iter().find(|(_, n, _)| n.eq_ignore_ascii_case(name.trim())).map(|(c, _, _)| *c)
    }

    pub fn set(&mut self, c: Control, b: Binding) {
        match self.map.iter_mut().find(|(k, _)| *k == c) {
            Some((_, x)) => *x = b,
            None => self.map.push((c, b)),
        }
    }

    /// Bind `input` to `c`; any other control that had it gets `c`'s old input of that
    /// kind instead (a swap). The controls that gave it up.
    pub fn rebind(&mut self, c: Control, input: Input) -> Vec<Control> {
        let old = self.get(c);
        let mut swapped = Vec::new();
        for (k, b) in self.map.iter_mut().filter(|(k, _)| *k != c) {
            match input {
                Input::Key(key) if b.key == Some(key) => b.key = old.key,
                Input::Mouse(m) if b.mouse == Some(m) => b.mouse = old.mouse,
                _ => continue,
            }
            swapped.push(*k);
        }
        let mut new = old;
        match input {
            Input::Key(key) => new.key = Some(key),
            Input::Mouse(m) => new.mouse = Some(m),
        }
        self.set(c, new);
        swapped
    }

    /// Every control's binding, in the INI's order.
    pub fn all(&self) -> impl Iterator<Item = (Control, Binding)> + '_ {
        GAME_CONTROLS.iter().map(|&(c, _, _)| (c, self.get(c)))
    }
}

/// The control's name on the Controls page [design: what it does in NVForever, not the
/// original menu's wording].
pub fn label(c: Control) -> &'static str {
    use Control::*;
    match c {
        Forward => "Forward",
        Back => "Back",
        SlideLeft => "Strafe Left",
        SlideRight => "Strafe Right",
        Use => "Attack",
        Activate => "Activate",
        Block => "Aim / Block",
        ReadyItem => "Ready / Reload",
        CrouchSneak => "Sneak",
        Run => "Run / Walk",
        AlwaysRun => "Always Run",
        AutoMove => "Auto Move",
        Jump => "Jump",
        TogglePov => "Toggle View",
        MenuMode => "Pip-Boy",
        Rest => "Wait",
        Vats => "V.A.T.S.",
        Hotkey1 => "Hotkey 1",
        AmmoSwap => "Ammo Swap",
        Hotkey3 => "Hotkey 3",
        Hotkey4 => "Hotkey 4",
        Hotkey5 => "Hotkey 5",
        Hotkey6 => "Hotkey 6",
        Hotkey7 => "Hotkey 7",
        Hotkey8 => "Hotkey 8",
        QuickSave => "Quick Save",
        QuickLoad => "Quick Load",
        Grab => "Grab",
    }
}

/// A key or a mouse button, as the Controls page takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// The keys a control can be bound to and their names in the INI and on the Controls
/// page [design]. Punctuation is named in words, so a value's comma only ever parts the
/// key from the mouse button.
pub const KEY_NAMES: &[(KeyCode, &str)] = {
    use KeyCode::*;
    &[
        (KeyA, "A"), (KeyB, "B"), (KeyC, "C"), (KeyD, "D"), (KeyE, "E"), (KeyF, "F"), (KeyG, "G"),
        (KeyH, "H"), (KeyI, "I"), (KeyJ, "J"), (KeyK, "K"), (KeyL, "L"), (KeyM, "M"), (KeyN, "N"),
        (KeyO, "O"), (KeyP, "P"), (KeyQ, "Q"), (KeyR, "R"), (KeyS, "S"), (KeyT, "T"), (KeyU, "U"),
        (KeyV, "V"), (KeyW, "W"), (KeyX, "X"), (KeyY, "Y"), (KeyZ, "Z"),
        (Digit1, "1"), (Digit2, "2"), (Digit3, "3"), (Digit4, "4"), (Digit5, "5"),
        (Digit6, "6"), (Digit7, "7"), (Digit8, "8"), (Digit9, "9"), (Digit0, "0"),
        (F1, "F1"), (F2, "F2"), (F3, "F3"), (F4, "F4"), (F5, "F5"), (F6, "F6"),
        (F7, "F7"), (F8, "F8"), (F9, "F9"), (F10, "F10"), (F11, "F11"), (F12, "F12"),
        (Space, "Space"), (Tab, "Tab"), (Enter, "Enter"), (Backspace, "Backspace"), (CapsLock, "Caps Lock"),
        (ShiftLeft, "Left Shift"), (ShiftRight, "Right Shift"), (ControlLeft, "Left Ctrl"), (ControlRight, "Right Ctrl"),
        (AltLeft, "Left Alt"), (AltRight, "Right Alt"),
        (ArrowUp, "Up"), (ArrowDown, "Down"), (ArrowLeft, "Left"), (ArrowRight, "Right"),
        (Insert, "Insert"), (Delete, "Delete"), (Home, "Home"), (End, "End"), (PageUp, "Page Up"), (PageDown, "Page Down"),
        (Minus, "Minus"), (Equal, "Equals"), (BracketLeft, "Left Bracket"), (BracketRight, "Right Bracket"),
        (Semicolon, "Semicolon"), (Quote, "Apostrophe"), (Backslash, "Backslash"), (Comma, "Comma"), (Period, "Period"), (Slash, "Slash"),
        (Numpad0, "Num 0"), (Numpad1, "Num 1"), (Numpad2, "Num 2"), (Numpad3, "Num 3"), (Numpad4, "Num 4"),
        (Numpad5, "Num 5"), (Numpad6, "Num 6"), (Numpad7, "Num 7"), (Numpad8, "Num 8"), (Numpad9, "Num 9"),
        (NumpadMultiply, "Num *"), (NumpadAdd, "Num +"), (NumpadSubtract, "Num -"), (NumpadDivide, "Num /"),
        (NumpadDecimal, "Num ."), (NumpadEnter, "Num Enter"),
    ]
};

/// The mouse buttons' names (the INI's 0, 1, 2, 3, 4; more as `Mouse 6`…).
pub const MOUSE_NAMES: [(MouseButton, &str); 5] = [
    (MouseButton::Left, "Left Mouse"),
    (MouseButton::Right, "Right Mouse"),
    (MouseButton::Middle, "Middle Mouse"),
    (MouseButton::Back, "Mouse 4"),
    (MouseButton::Forward, "Mouse 5"),
];

/// Keys no control may take: Esc (the pause menu) and the console's key.
pub const RESERVED_KEYS: [KeyCode; 2] = [KeyCode::Escape, KeyCode::Backquote];

pub fn key_name(k: KeyCode) -> Option<&'static str> {
    KEY_NAMES.iter().find(|(c, _)| *c == k).map(|(_, n)| *n)
}

pub fn mouse_name(m: MouseButton) -> String {
    match MOUSE_NAMES.iter().find(|(b, _)| *b == m) {
        Some((_, n)) => n.to_string(),
        None => match m {
            MouseButton::Other(n) => format!("Mouse {}", n + 1),
            _ => "Mouse ?".into(),
        },
    }
}

/// The input of a name (without case).
pub fn parse_input(name: &str) -> Option<Input> {
    let name = name.trim();
    if let Some((k, _)) = KEY_NAMES.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)) {
        return Some(Input::Key(*k));
    }
    if let Some((m, _)) = MOUSE_NAMES.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)) {
        return Some(Input::Mouse(*m));
    }
    let n: u16 = name.get(..6).filter(|p| p.eq_ignore_ascii_case("mouse ")).and_then(|_| name[6..].trim().parse().ok())?;
    (6..=16).contains(&n).then_some(Input::Mouse(MouseButton::Other(n - 1)))
}

impl Binding {
    /// As the INI and the Controls page write it: `W`, `Left Alt, Right Mouse`, `None`.
    pub fn text(&self) -> String {
        let parts: Vec<String> = self.key.and_then(key_name).map(String::from).into_iter().chain(self.mouse.map(mouse_name)).collect();
        if parts.is_empty() {
            "None".into()
        } else {
            parts.join(", ")
        }
    }

    /// From [`Binding::text`]: at most one key and one mouse button; `None` or nothing
    /// for neither.
    pub fn parse(text: &str) -> Result<Binding, String> {
        let mut b = Binding::default();
        for part in text.split(',').map(str::trim).filter(|p| !p.is_empty() && !p.eq_ignore_ascii_case("none")) {
            match parse_input(part) {
                Some(Input::Key(k)) if RESERVED_KEYS.contains(&k) => return Err(format!("{part:?} is kept for the menus")),
                Some(Input::Key(_)) if b.key.is_some() => return Err(format!("{part:?}: one key only")),
                Some(Input::Mouse(_)) if b.mouse.is_some() => return Err(format!("{part:?}: one mouse button only")),
                Some(Input::Key(k)) => b.key = Some(k),
                Some(Input::Mouse(m)) => b.mouse = Some(m),
                None => return Err(format!("{part:?} is not a key or mouse button name")),
            }
        }
        Ok(b)
    }
}

/// The INI section the controls are kept in.
pub const SECTION: &str = "Controls";
const ALWAYS_RUN_KEY: &str = "Always Run By Default";
const INVERT_Y_KEY: &str = "Invert Y";
const SENSITIVITY_KEY: &str = "Mouse Sensitivity";

fn bit(on: bool) -> &'static str {
    if on {
        "1"
    } else {
        "0"
    }
}

/// `text` (an INI, or empty) with every control in `[Controls]`.
pub fn with_controls(text: &str, b: &Bindings) -> String {
    let mut t = b.all().fold(text.to_string(), |t, (c, x)| ini_set(&t, SECTION, Bindings::name(c), &x.text()));
    t = ini_set(&t, SECTION, ALWAYS_RUN_KEY, bit(b.always_run_by_default));
    t = ini_set(&t, SECTION, INVERT_Y_KEY, bit(b.invert_y));
    ini_set(&t, SECTION, SENSITIVITY_KEY, &format!("{:.4}", b.mouse_sensitivity))
}

/// What `[Controls]` of `text` changes in `b`: the lines found, and what changed
/// (`Jump Space -> J`); values that do not parse go to `bad`.
pub fn read_controls(text: &str, b: &mut Bindings, bad: &mut Vec<String>) -> (usize, Vec<String>) {
    let (mut found, mut changed) = (0, Vec::new());
    for (c, name, _) in GAME_CONTROLS {
        let Some(v) = ini_get(text, SECTION, name) else { continue };
        found += 1;
        match Binding::parse(&v) {
            Ok(x) if x != b.get(c) => {
                changed.push(format!("{name} {} -> {}", b.get(c).text(), x.text()));
                b.set(c, x);
            }
            Ok(_) => {}
            Err(e) => bad.push(format!("{name}={v} ({e})")),
        }
    }
    let switch = |key: &str, at: &mut bool, found: &mut usize, changed: &mut Vec<String>, bad: &mut Vec<String>| {
        let Some(v) = ini_get(text, SECTION, key) else { return };
        *found += 1;
        match v.trim() {
            "1" | "0" => {
                let on = v.trim() == "1";
                if on != *at {
                    changed.push(format!("{key} {}", bit(on)));
                    *at = on;
                }
            }
            _ => bad.push(format!("{key}={v} (not 1 or 0)")),
        }
    };
    switch(ALWAYS_RUN_KEY, &mut b.always_run_by_default, &mut found, &mut changed, bad);
    switch(INVERT_Y_KEY, &mut b.invert_y, &mut found, &mut changed, bad);
    if let Some(v) = ini_get(text, SECTION, SENSITIVITY_KEY) {
        found += 1;
        match v.trim().parse::<f32>() {
            Ok(s) if s.is_finite() && s > 0.0 => {
                if (s - b.mouse_sensitivity).abs() > 1e-7 {
                    changed.push(format!("{SENSITIVITY_KEY} {s}"));
                    b.mouse_sensitivity = s;
                }
            }
            _ => bad.push(format!("{SENSITIVITY_KEY}={v} (not a number above 0)")),
        }
    }
    (found, changed)
}

/// The number of `[Controls]` lines a full section has.
const LINES: usize = GAME_CONTROLS.len() + 3;

/// Read the player's keys from the INI (module notes) and fill in the lines it lacks.
pub fn apply_stored(world: &mut World) {
    let Some(path) = world.get_resource::<crate::settings::SettingsFile>().and_then(|f| f.0.clone()) else { return };
    if !world.contains_resource::<Bindings>() {
        return;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            warn!("controls: {} not read: {e}", path.display());
            return;
        }
    };
    let mut bad = Vec::new();
    let (found, changed) = read_controls(&text, &mut world.resource_mut::<Bindings>(), &mut bad);
    let always_run = world.resource::<Bindings>().always_run_by_default;
    if let Some(mut c) = world.get_resource_mut::<crate::control::PlayerControl>() {
        c.always_run = always_run;
    }
    for b in &bad {
        warn!("controls: [{SECTION}] {b} in {}; the default kept", path.display());
    }
    let what = if changed.is_empty() { String::new() } else { format!(": {}", changed.join(", ")) };
    info!("controls: {found} of {LINES} line(s) read from {}, {} changed{what}", path.display(), changed.len());
    if found < LINES {
        store(world);
        info!("controls: the {} missing line(s) written to [{SECTION}]", LINES - found);
    }
}

/// Bind `input` to `c` (the Controls page), log it and keep it in the INI.
pub fn rebind(world: &mut World, c: Control, input: Input) {
    let Some(mut b) = world.get_resource_mut::<Bindings>() else { return };
    let before = b.get(c).text();
    let swapped = b.rebind(c, input);
    let after = b.get(c).text();
    let gave: Vec<String> = swapped.iter().map(|&o| format!("{} now {}", Bindings::name(o), b.get(o).text())).collect();
    let gave = if gave.is_empty() { String::new() } else { format!(" ({})", gave.join(", ")) };
    info!("controls: {} bound to {after} (was {before}){gave}", Bindings::name(c));
    store(world);
}

/// Take the key off `c` (the Controls page's Delete), keeping its mouse button.
pub fn clear_key(world: &mut World, c: Control) {
    let Some(mut b) = world.get_resource_mut::<Bindings>() else { return };
    let mut x = b.get(c);
    x.key = None;
    b.set(c, x);
    info!("controls: {} key cleared, now {}", Bindings::name(c), x.text());
    store(world);
}

/// Every control back to the game's default keys (the Controls page's Defaults); the
/// mouse look settings are kept.
pub fn reset(world: &mut World) {
    let Some(mut b) = world.get_resource_mut::<Bindings>() else { return };
    let d = Bindings::game_defaults();
    b.map = d.map;
    info!("controls: every control back to the game's keys");
    store(world);
}

/// Write `[Controls]` to the INI, keeping its other lines.
fn store(world: &World) {
    let Some(path) = world.get_resource::<crate::settings::SettingsFile>().and_then(|f| f.0.clone()) else { return };
    let Some(b) = world.get_resource::<Bindings>() else { return };
    if let Err(e) = write(&path, b) {
        warn!("controls: not saved: {e}");
    }
}

fn write(path: &Path, b: &Bindings) -> Result<(), String> {
    guard_write(path, None).map_err(|e| e.to_string())?;
    let old = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, with_controls(&old, b)).map_err(|e| format!("{}: {e}", path.display()))
}

/// The controls' state this frame. While the console's free camera flies
/// (`crate::free_cam`) its keys are the camera's: no control reads as pressed.
#[derive(SystemParam)]
pub struct ControlInput<'w> {
    pub keys: Res<'w, ButtonInput<KeyCode>>,
    pub mouse: Res<'w, ButtonInput<MouseButton>>,
    pub bindings: Res<'w, Bindings>,
    pub free_cam: Option<Res<'w, crate::free_cam::FreeCam>>,
}

impl ControlInput<'_> {
    /// The free camera has the keys.
    pub fn suppressed(&self) -> bool {
        self.free_cam.as_ref().is_some_and(|f| f.active)
    }

    pub fn pressed(&self, c: Control) -> bool {
        if self.suppressed() {
            return false;
        }
        let b = self.bindings.get(c);
        b.key.is_some_and(|k| self.keys.pressed(k)) || b.mouse.is_some_and(|m| self.mouse.pressed(m))
    }

    pub fn just_pressed(&self, c: Control) -> bool {
        if self.suppressed() {
            return false;
        }
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
    fn bindings_are_written_by_name_and_read_back() {
        let b = Bindings::game_defaults();
        assert_eq!(b.get(Control::Block).text(), "Left Alt, Right Mouse");
        assert_eq!(b.get(Control::Use).text(), "Left Mouse");
        assert_eq!(Binding::default().text(), "None");
        for (c, x) in b.all() {
            assert_eq!(Binding::parse(&x.text()), Ok(x), "{c:?}");
        }
        assert_eq!(Binding::parse(" left alt , RIGHT MOUSE "), Ok(b.get(Control::Block)));
        assert_eq!(Binding::parse("Mouse 7"), Ok(Binding { key: None, mouse: Some(MouseButton::Other(6)) }));
        assert_eq!(Binding::parse(""), Ok(Binding::default()));
        assert!(Binding::parse("W, E").is_err());
        assert!(Binding::parse("Banana").is_err());
        assert!(Binding::parse("Escape").is_err());
        assert_eq!(Bindings::by_name("crouch/sneak"), Some(Control::CrouchSneak));

        let ini = "[Game]\nPath=F:\\FNV\n";
        let text = with_controls(ini, &b);
        assert!(text.starts_with("[Game]\nPath=F:\\FNV\n\n[Controls]\nForward=W\nBack=S\n"), "{text}");
        assert!(text.contains("\nMouse Sensitivity=0.0020\n"));
        let mut back = Bindings::game_defaults();
        let (found, changed) = read_controls(&text, &mut back, &mut Vec::new());
        assert_eq!((found, changed.len()), (LINES, 0));

        let edited = ini_set(&ini_set(&text, SECTION, "Jump", "J"), SECTION, "Invert Y", "1");
        let edited = ini_set(&edited, SECTION, "Vats", "Purple");
        let mut bad = Vec::new();
        let (_, changed) = read_controls(&edited, &mut back, &mut bad);
        assert_eq!(changed, ["Jump Space -> J", "Invert Y 1"]);
        assert_eq!(back.get(Control::Jump).key, Some(KeyCode::KeyJ));
        assert!(back.invert_y);
        assert_eq!(back.get(Control::Vats).key, Some(KeyCode::KeyV), "a bad line keeps the default");
        assert_eq!(bad.len(), 1);
    }

    #[test]
    fn a_rebind_swaps_with_the_control_that_had_the_input() {
        let mut b = Bindings::game_defaults();
        // Jump to W: Forward had W and gets Space.
        assert_eq!(b.rebind(Control::Jump, Input::Key(KeyCode::KeyW)), [Control::Forward]);
        assert_eq!(b.get(Control::Jump).key, Some(KeyCode::KeyW));
        assert_eq!(b.get(Control::Forward).key, Some(KeyCode::Space));
        // A free key takes nothing from anyone; the mouse button is kept.
        assert!(b.rebind(Control::Block, Input::Key(KeyCode::KeyK)).is_empty());
        assert_eq!(b.get(Control::Block), Binding { key: Some(KeyCode::KeyK), mouse: Some(MouseButton::Right) });
        // Attack to the right button: Block had it and gets Attack's left button.
        assert_eq!(b.rebind(Control::Use, Input::Mouse(MouseButton::Right)), [Control::Block]);
        assert_eq!(b.get(Control::Block).mouse, Some(MouseButton::Left));
        assert_eq!(b.get(Control::Use).mouse, Some(MouseButton::Right));
    }

    #[test]
    fn the_ini_is_filled_in_and_a_rebind_is_kept() {
        let dir = std::env::temp_dir().join(format!("nvf-controls-{}", std::process::id()));
        let path = dir.join("NVForever.ini");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "[Game]\nPath=F:\\FNV\n\n[Controls]\nJump=J\n").unwrap();
        let start = || {
            let mut app = App::new();
            app.init_resource::<Bindings>();
            app.insert_resource(crate::settings::SettingsFile(Some(path.clone())));
            app.add_systems(PostStartup, apply_stored);
            app.update();
            app
        };
        let mut app = start();
        assert_eq!(app.world().resource::<Bindings>().get(Control::Jump).key, Some(KeyCode::KeyJ));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[Controls]\nJump=J\nForward=W\n") && text.contains("Path=F:\\FNV"), "{text}");
        rebind(app.world_mut(), Control::Activate, Input::Key(KeyCode::KeyF));
        let app = start();
        let b = app.world().resource::<Bindings>();
        assert_eq!(b.get(Control::Activate).key, Some(KeyCode::KeyF));
        assert_eq!(b.get(Control::TogglePov).key, Some(KeyCode::KeyE), "the swap is kept too");
        let _ = std::fs::remove_dir_all(&dir);
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
