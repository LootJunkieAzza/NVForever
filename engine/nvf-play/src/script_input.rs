//! The extender's input functions' side in the engine (task D6;
//! docs/spec-notes/ext-input.md): each frame, right after Bevy reads the devices, the
//! keys and mouse buttons held and the controls' bindings go to the script runtime
//! (`fnv_game::script_input`), the keys scripts disabled go to [`ScriptBlocked`] (which
//! `bindings::ControlInput` leaves out, so the player's controls ignore them), and the
//! keys scripts tapped are pressed for one frame.

use crate::bindings::{dik_key, Bindings, GAME_CONTROLS};
use bevy::prelude::*;
use fnv_game::script_input::MOUSE_BASE;

pub struct ScriptInputPlugin;

impl Plugin for ScriptInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptBlocked>()
            .add_systems(PreUpdate, script_input.after(bevy::input::InputSystems))
            .add_systems(Update, tell_magazine);
    }
}

/// The keys and mouse buttons scripts disabled (DisableKey, DisableControl).
#[derive(Resource, Default)]
pub struct ScriptBlocked {
    pub keys: Vec<KeyCode>,
    pub mouse: Vec<MouseButton>,
}

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

fn script_input(
    mut script: Option<ResMut<crate::script::ScriptState>>,
    bindings: Option<Res<Bindings>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut blocked: ResMut<ScriptBlocked>,
    mut tapped: Local<Vec<u16>>,
) {
    // Last frame's taps end.
    for c in tapped.drain(..) {
        match c.checked_sub(MOUSE_BASE) {
            Some(_) => mouse_of(c).into_iter().for_each(|b| mouse.release(b)),
            None => dik_key(c as u8).into_iter().for_each(|k| keys.release(k)),
        }
    }
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let input = &mut rt.game.input;
    input.held = keys.get_pressed().filter_map(|&k| key_code(k)).chain(mouse.get_pressed().filter_map(|&b| mouse_code(b))).collect();
    if let Some(b) = bindings {
        // The engine's controls are the extender's 0–27, in the same order.
        input.bindings = GAME_CONTROLS.iter().map(|(c, _, _)| {
            let bind = b.get(*c);
            (bind.key.and_then(key_code), bind.mouse.and_then(mouse_code))
        }).collect();
    }
    let codes = input.blocked();
    blocked.keys = codes.iter().filter(|&&c| c < MOUSE_BASE).filter_map(|&c| dik_key(c as u8)).collect();
    blocked.mouse = codes.iter().filter_map(|&c| mouse_of(c)).collect();
    for c in std::mem::take(&mut input.taps).into_iter().chain(input.script_held.iter().copied()) {
        match c.checked_sub(MOUSE_BASE) {
            Some(_) => mouse_of(c).into_iter().for_each(|b| mouse.press(b)),
            None => dik_key(c as u8).into_iter().for_each(|k| keys.press(k)),
        }
        if !input.script_held.contains(&c) {
            tapped.push(c);
        }
    }
}

/// The rounds in the player's magazine for GetPlayerCurrentAmmoRounds.
fn tell_magazine(combat: Option<Res<crate::combat::Combat>>, mut script: Option<ResMut<crate::script::ScriptState>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    rt.game.player_magazine = combat.and_then(|c| c.magazine_for_save()).map(|m| m.1);
}
