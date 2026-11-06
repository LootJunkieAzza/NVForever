//! The extender's input functions' side in the engine (task D6;
//! docs/spec-notes/ext-input.md): each frame, right after Bevy reads the devices, the
//! keys and mouse buttons held and the controls' bindings go to the script runtime
//! (`fnv_game::script_input`), the keys scripts disabled go to [`ScriptBlocked`] (which
//! `bindings::ControlInput` leaves out, so the player's controls ignore them), and the
//! keys scripts tapped are pressed for one frame.

use crate::bindings::{dik_key, Binding, Bindings, GAME_CONTROLS, GAME_INI_CONTROLS};
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use std::collections::HashSet;
use fnv_game::script_input::MOUSE_BASE;

pub struct ScriptInputPlugin;

impl Plugin for ScriptInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptBlocked>()
            .add_systems(PreUpdate, (script_input, key_events).chain().after(bevy::input::InputSystems))
            .add_systems(Update, (tell_magazine, tell_player_state, mod_file_roots, tell_view));
    }
}

/// The keys and mouse buttons scripts disabled (DisableKey, DisableControl).
#[derive(Resource, Default)]
pub struct ScriptBlocked {
    pub keys: Vec<KeyCode>,
    pub mouse: Vec<MouseButton>,
    /// ToggleVanityWheel 0: the wheel does not zoom the third-person camera
    /// (`fnv_game::nvse_jam`).
    pub wheel: bool,
}

/// The extender's codes of the wheel turned up and down [documented: ext-input.md §1,
/// 264 wheel up, 265 wheel down].
const WHEEL_UP: u16 = MOUSE_BASE + 8;
const WHEEL_DOWN: u16 = MOUSE_BASE + 9;

/// The extender's code of a mouse button: 256 left, 257 right, 258 middle, then the extra
/// buttons [documented: Template:DirectX Scancodes].
fn mouse_code(b: MouseButton) -> Option<u16> {
    Some(MOUSE_BASE
        + match b {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
            MouseButton::Other(n) if n < 8 => n,
            MouseButton::Other(_) => return None,
        })
}

fn mouse_of(code: u16) -> Option<MouseButton> {
    [MouseButton::Left, MouseButton::Right, MouseButton::Middle, MouseButton::Back, MouseButton::Forward, MouseButton::Other(5), MouseButton::Other(6), MouseButton::Other(7)]
        .into_iter()
        .find(|&b| mouse_code(b) == Some(code))
}

fn key_code(k: KeyCode) -> Option<u16> {
    (0u8..=255).find(|&c| dik_key(c) == Some(k)).map(u16::from)
}

/// The keys and mouse buttons scripts pressed that were up when they did: only these are
/// theirs to let go. One the player presses on the device while a script holds it becomes
/// the player's.
#[derive(Default)]
struct Pressed {
    keys: HashSet<KeyCode>,
    mouse: HashSet<MouseButton>,
}

impl Pressed {
    fn player_pressed(&mut self, typed: &mut MessageReader<KeyboardInput>, clicked: &mut MessageReader<MouseButtonInput>) {
        for k in typed.read().filter(|k| k.state == ButtonState::Pressed) {
            self.keys.remove(&k.key_code);
        }
        for b in clicked.read().filter(|b| b.state == ButtonState::Pressed) {
            self.mouse.remove(&b.button);
        }
    }

    fn press(&mut self, c: u16, keys: &mut ButtonInput<KeyCode>, mouse: &mut ButtonInput<MouseButton>) {
        match c.checked_sub(MOUSE_BASE) {
            Some(_) => mouse_of(c).into_iter().for_each(|b| {
                if !mouse.pressed(b) {
                    self.mouse.insert(b);
                    mouse.press(b);
                }
            }),
            None => dik_key(c as u8).into_iter().for_each(|k| {
                if !keys.pressed(k) {
                    self.keys.insert(k);
                    keys.press(k);
                }
            }),
        }
    }

    /// Let go of a key or mouse button the scripts pressed, unless the player holds it
    /// [hypothesis: ReleaseControl, ReleaseKey and a tap's end let go of the script's
    /// press only, and the key the player holds stays down. JAM's sprint holds Forward
    /// and releases it as the sprint ends in first person, and the player holding W walks
    /// on].
    fn release(&mut self, c: u16, keys: &mut ButtonInput<KeyCode>, mouse: &mut ButtonInput<MouseButton>) {
        match c.checked_sub(MOUSE_BASE) {
            Some(_) => mouse_of(c).into_iter().filter(|b| self.mouse.remove(b)).for_each(|b| mouse.release(b)),
            None => dik_key(c as u8).into_iter().filter(|k| self.keys.remove(k)).for_each(|k| keys.release(k)),
        }
    }
}

fn script_input(
    mut script: Option<ResMut<crate::script::ScriptState>>,
    mut bindings: Option<ResMut<Bindings>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut blocked: ResMut<ScriptBlocked>,
    scroll: Option<Res<bevy::input::mouse::AccumulatedMouseScroll>>,
    mut last: Local<(Vec<u16>, Vec<u16>)>,
    mut typed: MessageReader<KeyboardInput>,
    mut clicked: MessageReader<MouseButtonInput>,
    mut ours: Local<Pressed>,
) {
    ours.player_pressed(&mut typed, &mut clicked);
    // Last frame's taps and the keys the scripts held.
    let (tapped, script_held) = &mut *last;
    // Last frame's taps end.
    for c in tapped.drain(..) {
        ours.release(c, &mut keys, &mut mouse);
    }
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let input = &mut rt.game.input;
    // Keys scripts held last frame and no longer hold (ReleaseKey, ReleaseControl) are let go.
    for c in script_held.drain(..).filter(|c| !input.script_held.contains(c)) {
        ours.release(c, &mut keys, &mut mouse);
    }
    // SetControl's changes reach the player's controls first.
    if let Some(b) = bindings.as_deref_mut() {
        for (c, mouse, code) in std::mem::take(&mut input.rebinds) {
            let Some(&(control, _, _)) = GAME_CONTROLS.get(c) else { continue };
            let old = b.get(control);
            let new = if mouse {
                Binding { mouse: code.and_then(mouse_of), ..old }
            } else {
                Binding { key: code.and_then(|k| u8::try_from(k).ok()).and_then(dik_key), ..old }
            };
            b.set(control, new);
        }
    }
    input.held = keys.get_pressed().filter_map(|&k| key_code(k)).chain(mouse.get_pressed().filter_map(|&b| mouse_code(b))).collect();
    // The wheel turned this frame is held for the frame (IsKeyPressed 264/265, JIP's key
    // events) [hypothesis: one frame a notch].
    match scroll.map_or(0.0, |s| s.delta.y) {
        d if d > 0.0 => _ = input.held.insert(WHEEL_UP),
        d if d < 0.0 => _ = input.held.insert(WHEEL_DOWN),
        _ => {}
    }
    blocked.wheel = rt.game.jam.vanity_wheel_off;
    let input = &mut rt.game.input;
    if let Some(b) = bindings.as_deref() {
        // The engine's controls are the extender's 0–27, in the same order.
        input.bindings = GAME_CONTROLS[..GAME_INI_CONTROLS].iter().map(|(c, _, _)| {
            let bind = b.get(*c);
            (bind.key.and_then(key_code), bind.mouse.and_then(mouse_code))
        }).collect();
    }
    set_blocked(&mut blocked, input);
    for c in std::mem::take(&mut input.taps).into_iter().chain(input.script_held.iter().copied()) {
        ours.press(c, &mut keys, &mut mouse);
        if !input.script_held.contains(&c) {
            tapped.push(c);
        }
    }
    script_held.extend(input.script_held.iter().copied());
}

/// The keys and buttons the player's controls leave out: those scripts disabled.
fn set_blocked(blocked: &mut ScriptBlocked, input: &fnv_game::script_input::ScriptInput) {
    let codes = input.blocked();
    blocked.keys = codes.iter().filter(|&&c| c < MOUSE_BASE).filter_map(|&c| dik_key(c as u8)).collect();
    blocked.mouse = codes.iter().filter_map(|&c| mouse_of(c)).collect();
}

/// JIP's key events of this frame (`fnv_game::nvse_jam`), run before the player's controls
/// read the keys, and the keys their handlers disabled left out of those controls at once
/// [hypothesis: the original's extender sees a key before the game does, so a handler's
/// DisableControl holds the key's own control from its first frame: JAM's sprint
/// (Left Shift, also the game's Run key) disables Run in its handler and needs the player
/// still running on the next frame].
fn key_events(mut script: Option<ResMut<crate::script::ScriptState>>, sim: Option<ResMut<crate::sim::SimState>>, mut blocked: ResMut<ScriptBlocked>) {
    let (Some(rt), Some(mut sim)) = (script.as_deref_mut().and_then(|s| s.rt.as_mut()), sim) else { return };
    if !rt.game.jam_keys_changed() {
        return;
    }
    crate::script::with_world(rt, &mut sim, |rt| rt.game.jam_key_events());
    set_blocked(&mut blocked, &rt.game.input);
}

/// The rounds in the player's magazine for GetPlayerCurrentAmmoRounds.
fn tell_magazine(mut combat: Option<ResMut<crate::combat::Combat>>, mut script: Option<ResMut<crate::script::ScriptState>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    // SetPlayerCurrentAmmoRounds: the rounds go in the weapon in hand.
    if let (Some(n), Some(c)) = (rt.game.other.set_magazine.take(), combat.as_deref_mut()) {
        if let Some((w, _)) = c.magazine_for_save() {
            c.restore_magazine((w, n));
        }
    }
    rt.game.player_magazine = combat.and_then(|c| c.magazine_for_save()).map(|m| m.1);
}

/// The pad's buttons as the extender's controller functions number them
/// (`fnv_game::nvse_jip2::Pad`).
const PAD_BITS: [(GamepadButton, u32); 14] = [
    (GamepadButton::DPadUp, 0x1),
    (GamepadButton::DPadDown, 0x2),
    (GamepadButton::DPadLeft, 0x4),
    (GamepadButton::DPadRight, 0x8),
    (GamepadButton::Start, 0x10),
    (GamepadButton::Select, 0x20),
    (GamepadButton::LeftThumb, 0x40),
    (GamepadButton::RightThumb, 0x80),
    (GamepadButton::LeftTrigger, 0x100),
    (GamepadButton::RightTrigger, 0x200),
    (GamepadButton::South, 0x1000),
    (GamepadButton::East, 0x2000),
    (GamepadButton::West, 0x4000),
    (GamepadButton::North, 0x8000),
];

/// Tell the scripts the player's auto-move and aiming and the first connected pad
/// (GetAutoMove, GetPCUsingScope/IronSights, the controller functions).
fn tell_player_state(control: Option<Res<crate::control::PlayerControl>>, pads: Query<&Gamepad>, mut script: Option<ResMut<crate::script::ScriptState>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let told = &mut rt.game.jip2.told;
    if let Some(c) = control {
        told.auto_move = c.auto_move;
        told.aiming = c.aiming && c.weapon_out;
    }
    let pad = &mut told.pad;
    match pads.iter().next() {
        Some(g) => {
            pad.connected = true;
            pad.buttons = PAD_BITS.iter().filter(|(b, _)| g.pressed(*b)).map(|(_, m)| m).sum();
            pad.triggers = [g.get(GamepadButton::LeftTrigger2).unwrap_or(0.0), g.get(GamepadButton::RightTrigger2).unwrap_or(0.0)];
            let (l, r) = (g.left_stick(), g.right_stick());
            pad.sticks = [l.x, l.y, r.x, r.y];
        }
        None => {
            pad.connected = false;
            pad.buttons = 0;
            pad.triggers = [0.0; 2];
            pad.sticks = [0.0; 4];
        }
    }
}

/// The player's camera for WorldToScreen (`fnv_game::nvse_jam::ViewTold`): Bevy world to
/// clip space.
fn tell_view(cams: Query<(&Camera, &GlobalTransform), With<crate::PlayerCamera>>, mut script: Option<ResMut<crate::script::ScriptState>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    rt.game.jam.view = cams.iter().find(|(c, _)| c.is_active).map(|(c, t)| fnv_game::nvse_jam::ViewTold {
        clip_from_world: (c.clip_from_view() * t.to_matrix().inverse()).to_cols_array(),
    });
}

/// Once the game's Data is open: mods' files are read from NVForever's mods layer, then
/// the game's Data, and the archives answer FileExists (fnv_game::mod_files).
fn mod_file_roots(data: Option<Res<nvf_assets::GameData>>, mut done: Local<bool>) {
    let Some(d) = data.filter(|_| !*done) else { return };
    *done = true;
    let vfs = d.vfs.clone();
    let reader_vfs = d.vfs.clone();
    // A model's sequences for GetAnimSequenceFrequency.
    let (seq_vfs, schema) = (d.vfs.clone(), d.schema.clone());
    fnv_game::nvse_jam::set_sequence_lookup(std::sync::Arc::new(move |model: &str| {
        let path = format!("meshes\\{}", model.trim_start_matches(['\\', '/']));
        let data = seq_vfs.read(&path)?.ok()?;
        let f = fnv_nif::NifFile::parse(&data, Some(&schema)).ok()?;
        let dec = fnv_nif::scene::Decoded::new(&f, &data, &schema);
        Some(fnv_nif::anim::sequences(&f, &dec).into_iter().filter_map(|s| Some((s.name?, s.frequency))).collect())
    }));
    let mods = fnv_install::EngineHome::default_root().map(|r| fnv_install::EngineHome::new(r).mods());
    let data_dir = vfs.data_dir().to_path_buf();
    fnv_game::mod_files::update_roots(move |r| {
        r.read = mods.into_iter().chain(std::iter::once(data_dir)).collect();
        r.reader = Some(std::sync::Arc::new(move |p: &str| reader_vfs.read(p).and_then(Result::ok)));
        r.lookup = Some(std::sync::Arc::new(move |p: &str| {
            if vfs.loose_path(p).is_some() {
                1
            } else if vfs.exists(p) {
                2
            } else {
                0
            }
        }));
    });
}
