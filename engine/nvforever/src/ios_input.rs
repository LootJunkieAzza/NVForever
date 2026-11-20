//! [documented] Physical iPad input uses Apple's GameController framework (ios_input.m).
//! Feed the normal Bevy input messages so the game's existing bindings still apply.

use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::input::{ButtonState, InputSystems};
use bevy::prelude::*;
use bevy::window::{CursorMoved, PrimaryWindow};
use std::sync::Mutex;

static EVENTS: Mutex<Vec<(u32, u32, f32, f32)>> = Mutex::new(Vec::new());

#[cfg(target_os = "ios")]
unsafe extern "C" {
    fn nvf_ios_input_poll(callback: extern "C" fn(u32, u32, f32, f32), capture: bool);
}

extern "C" fn receive(kind: u32, code: u32, x: f32, y: f32) {
    if let Ok(mut events) = EVENTS.lock() {
        events.push((kind, code, x, y));
    }
}

#[derive(Default)]
struct Held {
    keys: Vec<KeyCode>,
    buttons: Vec<MouseButton>,
    drawable: Option<UVec2>,
}

pub struct IosInputPlugin;

impl Plugin for IosInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, poll.before(InputSystems));
    }
}

// Exclusive systems run on the main thread, as UIKit requires.
fn poll(world: &mut World, mut held: Local<Held>) {
    let Some((entity, focused)) = world
        .query_filtered::<(Entity, &Window), With<PrimaryWindow>>()
        .single(world)
        .ok()
        .map(|(entity, window)| (entity, window.focused))
    else {
        return;
    };
    let window = world.get::<Window>(entity).unwrap();
    let drawable = window.resolution.physical_size();
    if held.drawable != Some(drawable) {
        info!("iPad drawable: {}x{}, logical {}x{}", drawable.x, drawable.y, window.width(), window.height());
        #[cfg(target_os = "ios")]
        bevy::winit::WINIT_WINDOWS.with_borrow(|windows| {
            use winit::platform::ios::WindowExtIOS;
            if let Some(window) = windows.get_window(entity) {
                window.set_prefers_status_bar_hidden(true);
            }
        });
        held.drawable = Some(drawable);
    }
    #[cfg(target_os = "ios")]
    {
        let capture = focused && world.get_resource::<nvf_play::player::IosMouseCapture>().is_some_and(|c| c.0);
        // SAFETY: main thread; the callback has static lifetime and only queues events.
        unsafe {
            nvf_ios_input_poll(receive, capture);
        }
    }
    let events = std::mem::take(&mut *EVENTS.lock().expect("physical input event queue"));
    if !focused {
        release(world, entity, &mut held);
        return;
    }
    for (kind, code, x, y) in events {
        let state = if x != 0.0 { ButtonState::Pressed } else { ButtonState::Released };
        match kind {
            0 => {
                if let Some(key) = key_code(code) {
                    let repeat = held.keys.contains(&key) && state == ButtonState::Pressed;
                    held.keys.retain(|k| *k != key);
                    if state == ButtonState::Pressed {
                        held.keys.push(key);
                    }
                    keyboard(world, entity, key, state, repeat);
                }
            }
            1 => {
                let button = match code {
                    0 => MouseButton::Left,
                    1 => MouseButton::Right,
                    _ => MouseButton::Middle,
                };
                held.buttons.retain(|b| *b != button);
                if state == ButtonState::Pressed {
                    held.buttons.push(button);
                }
                world.write_message(MouseButtonInput { button, state, window: entity });
            }
            2 => {
                world.write_message(MouseMotion { delta: Vec2::new(x, y) });
            }
            3 => {
                let mut window = world.get_mut::<Window>(entity).unwrap();
                let position = Vec2::new(x, y);
                let delta = window.cursor_position().map(|old| position - old);
                window.set_cursor_position(Some(position));
                world.write_message(CursorMoved { window: entity, position, delta });
            }
            4 => release(world, entity, &mut held),
            5 => {
                world.write_message(MouseWheel {
                    window: entity,
                    unit: MouseScrollUnit::Line,
                    x,
                    y,
                    phase: TouchPhase::Moved,
                });
            }
            _ => {}
        }
    }
}

fn keyboard(world: &mut World, window: Entity, key_code: KeyCode, state: ButtonState, repeat: bool) {
    // Layout-aware text still comes from UIKit/winit; do not inject duplicate characters.
    world.write_message(KeyboardInput {
        key_code,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state,
        text: None,
        repeat,
        window,
    });
}

fn release(world: &mut World, window: Entity, held: &mut Held) {
    for key in held.keys.drain(..) {
        keyboard(world, window, key, ButtonState::Released, false);
    }
    for button in held.buttons.drain(..) {
        world.write_message(MouseButtonInput {
            button,
            state: ButtonState::Released,
            window,
        });
    }
}

fn key_code(code: u32) -> Option<KeyCode> {
    use KeyCode::*;
    const LETTERS: [KeyCode; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    const SPECIAL: [KeyCode; 44] = [
        Escape,
        Tab,
        Space,
        Enter,
        Backspace,
        ShiftLeft,
        ShiftRight,
        ControlLeft,
        ControlRight,
        AltLeft,
        AltRight,
        CapsLock,
        Backquote,
        ArrowUp,
        ArrowDown,
        ArrowLeft,
        ArrowRight,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        Minus,
        Equal,
        BracketLeft,
        BracketRight,
        Backslash,
        Semicolon,
        Quote,
        Comma,
        Period,
        Slash,
        Home,
        End,
        PageUp,
        PageDown,
        Delete,
    ];
    match code {
        65..=90 => LETTERS.get((code - 65) as usize).copied(),
        48..=57 => DIGITS.get((code - 48) as usize).copied(),
        256.. => SPECIAL.get((code - 256) as usize).copied(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_controls_hold_release_and_cancel_without_sticking() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin, IosInputPlugin));
        app.add_message::<CursorMoved>();
        let window = app.world_mut().spawn((PrimaryWindow, Window::default())).id();
        receive(0, b'W' as u32, 1.0, 0.0);
        receive(0, b'A' as u32, 1.0, 0.0);
        receive(1, 1, 1.0, 0.0);
        app.update();
        assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyW));
        assert!(app.world().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Right));
        app.update();
        assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyW));
        receive(0, b'W' as u32, 0.0, 0.0);
        app.update();
        let keys = app.world().resource::<ButtonInput<KeyCode>>();
        assert!(!keys.pressed(KeyCode::KeyW));
        assert!(keys.pressed(KeyCode::KeyA));
        receive(2, 0, 7.0, -3.0);
        receive(3, 0, 23.0, 37.0);
        app.update();
        assert_eq!(app.world().resource::<bevy::input::mouse::AccumulatedMouseMotion>().delta, Vec2::new(7.0, -3.0));
        assert_eq!(app.world().get::<Window>(window).unwrap().cursor_position(), Some(Vec2::new(23.0, 37.0)));
        receive(4, 0, 0.0, 0.0);
        app.update();
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyA));
        assert!(!app.world().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Right));
        receive(0, b'D' as u32, 1.0, 0.0);
        app.update();
        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
        app.update();
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyD));
        receive(0, b'W' as u32, 1.0, 0.0);
        app.update();
        app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
        app.update();
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyW));
    }
}
