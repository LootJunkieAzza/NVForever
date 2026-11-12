//! Saved device selection on use and consumption of the switching input [design, #2200].

use super::STICK_DEAD;
use crate::bindings::{Bindings, InputDevice};
use bevy::prelude::*;
use bevy::input::{gamepad::GamepadInput, Axis};
use std::collections::HashMap;

/// Fresh hardware activity; analog flags mean a crossing from neutral [design, #2200].
#[derive(Default, Clone, Copy)]
struct Activity {
    key: bool,
    mouse: bool,
    button: bool,
    stick: bool,
    trigger: bool,
}

/// Simultaneous activity keeps the selected device; connection/motion is absent from
/// this rule. Causes have a stable priority for logging [design, #2200].
fn switch_on_use(current: InputDevice, enabled: bool, a: Activity) -> Option<(InputDevice, &'static str)> {
    if !enabled {
        return None;
    }
    let keyboard = a.key || a.mouse;
    let controller = a.button || a.stick || a.trigger;
    match (current, keyboard, controller) {
        (InputDevice::KeyboardMouse, false, true) => Some((InputDevice::Controller, if a.button { "button" } else if a.stick { "stick" } else { "trigger" })),
        (InputDevice::Controller, true, false) => Some((InputDevice::KeyboardMouse, if a.key { "key" } else { "mouse button" })),
        _ => None,
    }
}

fn analog_active(g: &Gamepad) -> [bool; 4] {
    [g.left_stick().length() > STICK_DEAD, g.right_stick().length() > STICK_DEAD,
        g.get(GamepadButton::LeftTrigger2).unwrap_or(0.0) > STICK_DEAD,
        g.get(GamepadButton::RightTrigger2).unwrap_or(0.0) > STICK_DEAD]
}

/// Pip-Boy picking reads raw clicks in `First`, before selection in `PreUpdate`
/// (`crate::pipboy::drive_pointer`). Consume an incoming mouse press there, and its
/// release even after the device changes. Simultaneous use keeps Controller, for which
/// this incoming mouse click still stays consumed [design, #2200].
pub(crate) fn consume_pointer_switch(bindings: Option<&Bindings>, held: &mut Vec<MouseButton>, input: &bevy::input::mouse::MouseButtonInput) -> bool {
    if held.contains(&input.button) {
        if !input.state.is_pressed() {
            held.retain(|b| *b != input.button);
        }
        return true;
    }
    if input.state.is_pressed() && bindings.is_some_and(|b| b.automatic_input_switching && b.device == InputDevice::Controller) {
        held.push(input.button);
        return true;
    }
    false
}

#[derive(Clone)]
struct PadState {
    digital: ButtonInput<GamepadButton>,
    analog: Vec<(GamepadInput, f32)>,
}

impl PadState {
    fn read(g: &Gamepad) -> Self {
        Self { digital: g.digital().clone(), analog: g.get_analog_axes().map(|a| (*a, g.get_unclamped(*a).unwrap_or(0.0))).collect() }
    }
}

/// Physical state hidden from consumers until release/neutral. Pad state is restored
/// before Bevy updates it, so masking cannot hide a later release [design, #2200].
#[derive(Resource, Default)]
pub(crate) struct SwitchingInput {
    keys: Vec<KeyCode>,
    mouse: Vec<MouseButton>,
    pads: HashMap<Entity, PadState>,
    previous: HashMap<Entity, [bool; 4]>,
    keys_this_frame: Vec<KeyCode>,
}

impl SwitchingInput {
    /// Raw text/repeat readers also consume the switching key [design, #2200].
    pub(crate) fn allows(&self, event: &bevy::input::keyboard::KeyboardInput) -> bool {
        !self.keys_this_frame.contains(&event.key_code)
    }
}

pub(super) fn restore(
    held: Res<SwitchingInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut pads: Query<(Entity, &mut Gamepad)>,
) {
    for k in &held.keys {
        keys.press(*k);
    }
    for m in &held.mouse {
        mouse.press(*m);
    }
    for (e, mut g) in &mut pads {
        if let Some(original) = held.pads.get(&e) {
            *g.digital_mut() = original.digital.clone();
            *g.analog_mut() = Axis::default();
            for (axis, value) in &original.analog {
                g.analog_mut().set(*axis, *value);
            }
        }
    }
}

/// Runs after physical input/aids, before controller menu keys and UI focus [design,
/// #2200]. A controller stays consumed until all its buttons/sticks return to neutral.
pub(crate) fn automatic(world: &mut World) {
    let Some(b) = world.get_resource::<Bindings>() else { return };
    let (current, enabled) = (b.device, b.automatic_input_switching);
    let listening = world.get_resource::<crate::pause::Listening>().is_some_and(|l| l.0.is_some());
    let mut held = world.remove_resource::<SwitchingInput>().unwrap_or_default();
    let mut activity = Activity {
        key: world.resource::<ButtonInput<KeyCode>>().get_just_pressed().any(|k| !held.keys.contains(k)),
        mouse: world.resource::<ButtonInput<MouseButton>>().get_just_pressed().any(|m| !held.mouse.contains(m)),
        ..default()
    };
    let mut pads = world.query::<(Entity, &mut Gamepad)>();
    let mut previous = HashMap::new();
    for (e, g) in pads.iter(world) {
        let now = analog_active(g);
        let before = held.previous.get(&e).copied().unwrap_or_default();
        if !held.pads.contains_key(&e) {
            activity.button |= g.get_just_pressed().next().is_some();
            activity.stick |= (0..2).any(|i| now[i] && !before[i]);
            activity.trigger |= (2..4).any(|i| now[i] && !before[i]);
        }
        previous.insert(e, now);
    }
    held.previous = previous;
    let switch = switch_on_use(current, enabled && !listening, activity);
    if let Some((device, cause)) = switch {
        crate::bindings::set_device(world, device);
        info!("controller: input device switched to {} ({cause})", device.name());
        if device == InputDevice::KeyboardMouse {
            let keys = world.resource::<ButtonInput<KeyCode>>();
            held.keys.extend(keys.get_pressed().chain(keys.get_just_pressed()).copied());
            held.keys.sort_unstable();
            held.keys.dedup();
            let mouse = world.resource::<ButtonInput<MouseButton>>();
            held.mouse.extend(mouse.get_pressed().chain(mouse.get_just_pressed()).copied());
        } else {
            for (e, g) in pads.iter(world) {
                if g.get_pressed().next().is_some() || g.get_just_pressed().next().is_some() || analog_active(g).into_iter().any(|v| v) {
                    held.pads.insert(e, PadState::read(g));
                }
            }
        }
    }
    held.keys_this_frame = held.keys.clone();
    let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
    held.keys.retain(|k| {
        let down = keys.pressed(*k);
        keys.reset(*k);
        down
    });
    let mut mouse = world.resource_mut::<ButtonInput<MouseButton>>();
    held.mouse.retain(|m| {
        let down = mouse.pressed(*m);
        mouse.reset(*m);
        down
    });
    held.pads.retain(|e, original| {
        let Ok((_, mut g)) = pads.get_mut(world, *e) else { return false };
        let down = g.get_pressed().next().is_some() || analog_active(&g).into_iter().any(|v| v);
        *original = PadState::read(&g);
        g.digital_mut().reset_all();
        let axes: Vec<_> = g.get_analog_axes().copied().collect();
        for axis in axes {
            g.analog_mut().set(axis, 0.0);
        }
        down
    });
    world.insert_resource(held);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin));
        app.init_resource::<Bindings>().init_resource::<SwitchingInput>().init_resource::<super::super::MenuKeys>();
        app.insert_resource(crate::pause::Paused(true));
        app.add_systems(PreUpdate, restore.before(bevy::input::InputSystems));
        app.add_systems(PreUpdate, automatic.after(bevy::input::InputSystems));
        app.add_systems(PreUpdate, super::super::menu_keys.after(automatic));
        let pad = app.world_mut().spawn(Gamepad::default()).id();
        (app, pad)
    }

    fn button(app: &mut App, pad: Entity, down: bool) {
        use bevy::input::gamepad::{RawGamepadButtonChangedEvent, RawGamepadEvent};
        app.world_mut().write_message(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(pad, GamepadButton::South, if down { 1.0 } else { 0.0 })));
    }

    fn key(app: &mut App, down: bool, repeat: bool) {
        use bevy::input::{keyboard::{Key, KeyboardInput}, ButtonState};
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::Enter, logical_key: Key::Enter,
            state: if down { ButtonState::Pressed } else { ButtonState::Released },
            text: None, repeat, window: Entity::PLACEHOLDER,
        });
    }

    #[test]
    fn switching_presses_stay_consumed_until_release_then_work_on_the_next_press() {
        let (mut app, pad) = app();
        button(&mut app, pad, true);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
        assert!(!app.world().get::<Gamepad>(pad).unwrap().pressed(GamepadButton::South));
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().just_pressed(KeyCode::Enter));
        app.update();
        assert!(!app.world().get::<Gamepad>(pad).unwrap().pressed(GamepadButton::South));
        button(&mut app, pad, false);
        app.update();
        assert!(!app.world().get::<Gamepad>(pad).unwrap().just_released(GamepadButton::South));
        button(&mut app, pad, true);
        app.update();
        assert!(app.world().get::<Gamepad>(pad).unwrap().just_pressed(GamepadButton::South));
        assert!(app.world().resource::<ButtonInput<KeyCode>>().just_pressed(KeyCode::Enter));
        button(&mut app, pad, false);
        app.update();
        key(&mut app, true, false);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::Enter));
        key(&mut app, true, true);
        app.update();
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::Enter));
        key(&mut app, false, false);
        app.update();
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().just_released(KeyCode::Enter));
        key(&mut app, true, false);
        app.update();
        assert!(app.world().resource::<ButtonInput<KeyCode>>().just_pressed(KeyCode::Enter));
    }

    #[test]
    fn analog_switching_consumes_until_neutral_and_a_held_stick_does_not_steal_back() {
        let (mut app, pad) = app();
        app.update();
        for input in [bevy::input::gamepad::GamepadInput::Axis(GamepadAxis::LeftStickX),
            GamepadAxis::RightStickY.into(), GamepadButton::LeftTrigger2.into(), GamepadButton::RightTrigger2.into()] {
            app.world_mut().resource_mut::<Bindings>().device = InputDevice::KeyboardMouse;
            app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(input, STICK_DEAD);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
            app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(input, STICK_DEAD + 0.01);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
            assert_eq!(app.world().get::<Gamepad>(pad).unwrap().get(input), Some(0.0));
            key(&mut app, true, false);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
            key(&mut app, false, false);
            // [design / file ref] Change the saved physical sample before `restore`.
            let mut held = app.world_mut().resource_mut::<SwitchingInput>();
            for (axis, value) in &mut held.pads.get_mut(&pad).unwrap().analog {
                if *axis == input {
                    *value = 0.0;
                }
            }
            drop(held);
            app.update();
            assert!(app.world().resource::<SwitchingInput>().pads.is_empty());
        }
    }

    #[test]
    fn connection_rebinding_disabled_policy_and_simultaneous_input_do_not_switch() {
        let (mut app, pad) = app();
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
        button(&mut app, pad, true);
        key(&mut app, true, false);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
        button(&mut app, pad, false);
        key(&mut app, false, false);
        app.update();
        app.world_mut().resource_mut::<Bindings>().automatic_input_switching = false;
        button(&mut app, pad, true);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
        app.world_mut().resource_mut::<Bindings>().automatic_input_switching = true;
        app.world_mut().insert_resource(crate::pause::Listening(Some((crate::bindings::Control::Jump, InputDevice::KeyboardMouse))));
        button(&mut app, pad, false);
        app.update();
        button(&mut app, pad, true);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
    }

    #[test]
    fn mouse_motion_does_not_switch_but_a_click_switches_and_is_consumed() {
        use bevy::input::{mouse::{MouseButtonInput, MouseMotion}, ButtonState};
        let (mut app, _) = app();
        app.world_mut().resource_mut::<Bindings>().device = InputDevice::Controller;
        app.world_mut().write_message(MouseMotion { delta: Vec2::new(0.1, -0.1) });
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
        for (i, state) in [ButtonState::Pressed, ButtonState::Released, ButtonState::Pressed].into_iter().enumerate() {
            app.world_mut().write_message(MouseButtonInput { button: MouseButton::Left, state, window: Entity::PLACEHOLDER });
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
            if i == 0 {
                assert!(!app.world().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Left));
                app.update();
                assert!(!app.world().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Left));
            }
            if state == ButtonState::Released {
                assert!(!app.world().resource::<ButtonInput<MouseButton>>().just_released(MouseButton::Left));
            }
        }
        assert!(app.world().resource::<ButtonInput<MouseButton>>().just_pressed(MouseButton::Left));
    }

    #[test]
    fn the_raw_pipboy_switching_click_and_its_release_are_consumed() {
        use bevy::input::{mouse::MouseButtonInput, ButtonState};
        let mut b = Bindings::default();
        b.device = InputDevice::Controller;
        let mut held = Vec::new();
        let press = MouseButtonInput { button: MouseButton::Left, state: ButtonState::Pressed, window: Entity::PLACEHOLDER };
        assert!(consume_pointer_switch(Some(&b), &mut held, &press));
        b.device = InputDevice::KeyboardMouse;
        let release = MouseButtonInput { state: ButtonState::Released, ..press };
        assert!(consume_pointer_switch(Some(&b), &mut held, &release));
        assert!(!consume_pointer_switch(Some(&b), &mut held, &press));
        b.device = InputDevice::Controller;
        b.automatic_input_switching = false;
        assert!(!consume_pointer_switch(Some(&b), &mut held, &press));
    }

    #[test]
    fn switch_on_use_needs_fresh_activity_from_only_the_other_device() {
        use InputDevice::{Controller as Pad, KeyboardMouse as Keys};
        for a in [Activity { button: true, ..default() }, Activity { stick: true, ..default() }, Activity { trigger: true, ..default() }] {
            assert_eq!(switch_on_use(Keys, true, a).map(|s| s.0), Some(Pad));
            assert_eq!(switch_on_use(Pad, true, a), None);
            assert_eq!(switch_on_use(Keys, false, a), None);
            assert_eq!(switch_on_use(Keys, true, Activity { key: true, ..a }), None);
        }
        for a in [Activity { key: true, ..default() }, Activity { mouse: true, ..default() }] {
            assert_eq!(switch_on_use(Pad, true, a).map(|s| s.0), Some(Keys));
            assert_eq!(switch_on_use(Keys, true, a), None);
            assert_eq!(switch_on_use(Pad, false, a), None);
            assert_eq!(switch_on_use(Pad, true, Activity { button: true, ..a }), None);
        }
        assert_eq!(switch_on_use(Keys, true, Activity::default()), None);
        assert_eq!(switch_on_use(Pad, true, Activity::default()), None);
    }
}
