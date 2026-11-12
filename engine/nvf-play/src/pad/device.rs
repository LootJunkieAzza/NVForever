//! Saved device selection on use and consumption of the switching input [design, #2200].

use super::STICK_DEAD;
use crate::bindings::{Bindings, InputDevice};
use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::input::gamepad::GamepadInput;
use std::{borrow::Cow, hash::Hash};
use std::collections::HashMap;

/// Hardware activity and old-device busy flags; analog activity means a crossing
/// from neutral [design, #2200].
#[derive(Default, Clone, Copy)]
struct Activity {
    key: bool,
    mouse: bool,
    button: bool,
    stick: bool,
    trigger: bool,
    keyboard_busy: bool,
    controller_busy: bool,
}

/// The old device must be idle; simultaneous activity keeps the selected device.
/// Connection/motion is absent. Causes have a stable priority [design, #2200].
fn switch_on_use(current: InputDevice, enabled: bool, a: Activity) -> Option<(InputDevice, &'static str)> {
    let old_busy = match current {
        InputDevice::KeyboardMouse => a.keyboard_busy,
        InputDevice::Controller => a.controller_busy,
    };
    if !enabled || old_busy {
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

/// Held at frame start or released in this frame: either keeps the old device busy.
/// Fresh presses are handled by the simultaneous-activity rule [design, #2200].
fn digital_busy<T: Copy + Eq + Hash + Send + Sync + 'static>(input: &ButtonInput<T>) -> bool {
    input.get_pressed().any(|b| !input.just_pressed(*b)) || input.get_just_released().next().is_some()
}

fn analog_active(g: &Gamepad) -> [bool; 4] {
    [g.left_stick().length() > STICK_DEAD, g.right_stick().length() > STICK_DEAD,
        g.get(GamepadButton::LeftTrigger2).unwrap_or(0.0) > STICK_DEAD,
        g.get(GamepadButton::RightTrigger2).unwrap_or(0.0) > STICK_DEAD]
}

/// The switching press is ignored by actions for one frame; releases and subsequent
/// holds/repeats are always available. No hardware state is masked [design, #2200].
#[derive(Resource, Default)]
pub struct SwitchingInput {
    physical_keys: ButtonInput<KeyCode>,
    keys: Vec<KeyCode>,
    mouse: Vec<MouseButton>,
    buttons: HashMap<Entity, Vec<GamepadButton>>,
    analog: HashMap<Entity, [bool; 4]>,
    previous: HashMap<Entity, [bool; 4]>,
    previous_buttons: bool,
}

impl SwitchingInput {
    pub(crate) fn physical_key_pressed(&self, key: KeyCode) -> bool { self.physical_keys.pressed(key) }

    /// Hardware/aids only; translated controller menu keys never enter detection
    /// [design, #2200; crate::test_aid::real_presses, super::menu_keys].
    pub(crate) fn keyboard_input(&mut self, key: KeyCode, pressed: bool) {
        if pressed { self.physical_keys.press(key); } else { self.physical_keys.release(key); }
    }

    /// Shared raw-message filter, including text/repeats on the switching frame.
    /// Releases pass even when a press/release pair arrives together [design, #2200].
    pub fn allows(&self, event: &bevy::input::keyboard::KeyboardInput) -> bool {
        !event.state.is_pressed() || !self.keys.contains(&event.key_code)
    }

    /// Raw picking events use exactly the same press filter [design, #2200].
    pub fn allows_mouse(&self, event: &bevy::input::mouse::MouseButtonInput) -> bool {
        !event.state.is_pressed() || !self.mouse.contains(&event.button)
    }

    /// A derived action view; the original release bits survive unchanged, including
    /// a complete press/release pair in one frame [design, #2200].
    pub fn filter<'a, T: ActionButton>(&self, raw: &'a ButtonInput<T>) -> Cow<'a, ButtonInput<T>> {
        let blocked = T::blocked(self);
        if blocked.is_empty() { return Cow::Borrowed(raw); }
        let mut view = raw.clone();
        for &button in blocked {
            view.reset(button);
            if raw.just_released(button) {
                view.press(button);
                view.release(button);
                view.clear_just_pressed(button);
            }
        }
        Cow::Owned(view)
    }
}

/// Types accepted by the central action filter [design, #2200].
pub trait ActionButton: Copy + Eq + Hash + Send + Sync + 'static {
    fn blocked(filter: &SwitchingInput) -> &[Self];
}
impl ActionButton for KeyCode {
    fn blocked(filter: &SwitchingInput) -> &[Self] { &filter.keys }
}
impl ActionButton for MouseButton {
    fn blocked(filter: &SwitchingInput) -> &[Self] { &filter.mouse }
}

/// Action readers use this parameter instead of raw ButtonInput. Tests/tools without
/// the controller plugin retain ordinary Bevy input [design, #2200].
#[derive(SystemParam)]
pub struct ActionButtons<'w, T: ActionButton> {
    raw: Res<'w, ButtonInput<T>>,
    filter: Option<Res<'w, SwitchingInput>>,
}
impl<T: ActionButton> ActionButtons<'_, T> {
    pub fn snapshot(&self) -> Cow<'_, ButtonInput<T>> {
        self.filter.as_ref().map_or_else(|| Cow::Borrowed(&*self.raw), |f| f.filter(&self.raw))
    }
    pub fn pressed(&self, b: T) -> bool { self.raw.pressed(b) && !self.blocked(b) }
    pub fn just_pressed(&self, b: T) -> bool { self.raw.just_pressed(b) && !self.blocked(b) }
    pub fn just_released(&self, b: T) -> bool { self.raw.just_released(b) }
    fn blocked(&self, b: T) -> bool { self.filter.as_ref().is_some_and(|f| T::blocked(f).contains(&b)) }
    pub fn any_pressed(&self, buttons: impl IntoIterator<Item = T>) -> bool { buttons.into_iter().any(|b| self.pressed(b)) }
    pub fn any_just_pressed(&self, buttons: impl IntoIterator<Item = T>) -> bool { buttons.into_iter().any(|b| self.just_pressed(b)) }
    pub fn get_pressed(&self) -> impl Iterator<Item = &T> { self.raw.get_pressed().filter(|b| !self.blocked(**b)) }
    pub fn get_just_pressed(&self) -> impl Iterator<Item = &T> { self.raw.get_just_pressed().filter(|b| !self.blocked(**b)) }
    pub fn get_just_released(&self) -> impl Iterator<Item = &T> { self.raw.get_just_released() }
}

/// Pad action view, using the same filter as keys/mouse [design, #2200].
#[derive(SystemParam)]
pub struct ActionPads<'w, 's> {
    raw: Query<'w, 's, (Entity, &'static Gamepad)>,
    filter: Option<Res<'w, SwitchingInput>>,
}
impl ActionPads<'_, '_> {
    /// Filter current stick activity while retaining physical previous edges
    /// [design, #2200; super::update_stick_buttons].
    pub fn filter_sticks(&self, raw: &super::StickButtons) -> super::StickButtons {
        super::StickButtons {
            now: std::array::from_fn(|i| raw.now[i] && self.iter().any(|g| super::StickDir::ALL[i].amount(g.left_stick()) >= super::STICK_PRESS)),
            before: raw.before,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = ActionPad<'_>> {
        self.raw.iter().map(|(entity, raw)| ActionPad { entity, raw, filter: self.filter.as_deref() })
    }
}
pub struct ActionPad<'a> {
    entity: Entity,
    raw: &'a Gamepad,
    filter: Option<&'a SwitchingInput>,
}
impl ActionPad<'_> {
    fn blocked(&self, b: GamepadButton) -> bool { self.filter.and_then(|f| f.buttons.get(&self.entity)).is_some_and(|v| v.contains(&b)) }
    pub fn pressed(&self, b: GamepadButton) -> bool { self.raw.pressed(b) && !self.blocked(b) }
    pub fn just_pressed(&self, b: GamepadButton) -> bool { self.raw.just_pressed(b) && !self.blocked(b) }
    pub fn just_released(&self, b: GamepadButton) -> bool { self.raw.just_released(b) }
    fn analog_blocked(&self, i: usize) -> bool { self.filter.and_then(|f| f.analog.get(&self.entity)).is_some_and(|a| a[i]) }
    pub fn left_stick(&self) -> Vec2 { if self.analog_blocked(0) { Vec2::ZERO } else { self.raw.left_stick() } }
    pub fn right_stick(&self) -> Vec2 { if self.analog_blocked(1) { Vec2::ZERO } else { self.raw.right_stick() } }
    pub fn get(&self, input: impl Into<GamepadInput>) -> Option<f32> {
        let input = input.into();
        let blocked = match input {
            GamepadInput::Button(b) => self.blocked(b) || match b {
                GamepadButton::LeftTrigger2 => self.analog_blocked(2),
                GamepadButton::RightTrigger2 => self.analog_blocked(3),
                _ => false,
            },
            GamepadInput::Axis(GamepadAxis::LeftStickX | GamepadAxis::LeftStickY) => self.analog_blocked(0),
            GamepadInput::Axis(GamepadAxis::RightStickX | GamepadAxis::RightStickY) => self.analog_blocked(1),
            _ => false,
        };
        if blocked { Some(0.0) } else { self.raw.get(input) }
    }
}

/// Capture physical input independently of menu translation, mirroring focus loss
/// from Bevy's keyboard_input_system [design, #2200; bevy_input/keyboard.rs].
pub(crate) fn read_keyboard(
    mut events: MessageReader<bevy::input::keyboard::KeyboardInput>,
    mut focus_lost: MessageReader<bevy::input::keyboard::KeyboardFocusLost>,
    mut held: ResMut<SwitchingInput>,
) {
    held.physical_keys.clear();
    for event in events.read() { held.keyboard_input(event.key_code, event.state.is_pressed()); }
    if !focus_lost.is_empty() { held.physical_keys.release_all(); focus_lost.clear(); }
}

/// Runs once after hardware/aids and before action readers. Only this frame's fresh
/// incoming edges enter the filter; releases never do [design, #2200].
pub(crate) fn automatic(world: &mut World) {
    let Some(b) = world.get_resource::<Bindings>() else { return };
    let (current, enabled) = (b.device, b.automatic_input_switching);
    let listening = world.get_resource::<crate::pause::Listening>().is_some_and(|l| l.0.is_some());
    let mut held = world.remove_resource::<SwitchingInput>().unwrap_or_default();
    held.keys.clear(); held.mouse.clear(); held.buttons.clear(); held.analog.clear();
    let mut activity = Activity {
        key: held.physical_keys.get_just_pressed().next().is_some(),
        mouse: world.resource::<ButtonInput<MouseButton>>().get_just_pressed().next().is_some(),
        keyboard_busy: digital_busy(&held.physical_keys) || digital_busy(world.resource::<ButtonInput<MouseButton>>()),
        // Include pads removed this frame: disconnect must not split a pending hold
        // from the old device's readers [design, #2200].
        controller_busy: held.previous_buttons || held.previous.values().flatten().any(|active| *active),
        ..default()
    };
    let mut pads = world.query::<(Entity, &Gamepad)>();
    let mut previous = HashMap::new();
    let mut buttons_down = false;
    for (e, g) in pads.iter(world) {
        let now = analog_active(g);
        let before = held.previous.get(&e).copied().unwrap_or_default();
        activity.controller_busy |= digital_busy(g.digital());
        buttons_down |= g.get_pressed().next().is_some();
        activity.button |= g.get_just_pressed().next().is_some();
        activity.stick |= (0..2).any(|i| now[i] && !before[i]);
        activity.trigger |= (2..4).any(|i| now[i] && !before[i]);
        previous.insert(e, now);
    }
    let switch = switch_on_use(current, enabled && !listening, activity);
    if let Some((device, cause)) = switch {
        crate::bindings::set_device(world, device);
        info!("controller: input device switched to {} ({cause})", device.name());
        if device == InputDevice::KeyboardMouse {
            held.keys.extend(held.physical_keys.get_just_pressed().copied());
            held.mouse.extend(world.resource::<ButtonInput<MouseButton>>().get_just_pressed().copied());
        } else {
            for (e, g) in pads.iter(world) {
                held.buttons.insert(e, g.get_just_pressed().copied().collect());
                let now = previous[&e];
                let before = held.previous.get(&e).copied().unwrap_or_default();
                held.analog.insert(e, std::array::from_fn(|i| now[i] && !before[i]));
            }
        }
    }
    held.previous = previous;
    held.previous_buttons = buttons_down;
    world.insert_resource(held);
}

/// Hardware selection/filter schedule, shared by PadPlugin and headless readers
/// [design, #2200].
pub struct InputSwitchPlugin;
impl Plugin for InputSwitchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SwitchingInput>()
            .add_systems(PreUpdate, read_keyboard.after(bevy::input::InputSystems).before(crate::test_aid::real_presses))
            .add_systems(PreUpdate, automatic.after(read_keyboard).after(crate::test_aid::real_presses)
                .before(crate::full_screen_key::read).before(bevy::ui::UiSystems::Focus)
                .before(bevy::picking::PickingSystems::ProcessInput));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin, InputSwitchPlugin));
        app.init_resource::<Bindings>().init_resource::<super::super::MenuKeys>();
        app.insert_resource(crate::pause::Paused(true));
        app.add_systems(PreUpdate, super::super::menu_keys.after(automatic));
        let pad = app.world_mut().spawn(Gamepad::default()).id();
        (app, pad)
    }

    pub(crate) fn button(app: &mut App, pad: Entity, down: bool) {
        use bevy::input::gamepad::{RawGamepadButtonChangedEvent, RawGamepadEvent};
        app.world_mut().write_message(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(pad, GamepadButton::South, if down { 1.0 } else { 0.0 })));
    }

    fn key(app: &mut App, down: bool, repeat: bool) {
        keyboard(app, KeyCode::Enter, down, repeat);
    }

    fn keyboard(app: &mut App, code: KeyCode, down: bool, repeat: bool) {
        use bevy::input::{keyboard::{Key, KeyboardInput}, ButtonState};
        app.world_mut().write_message(KeyboardInput {
            key_code: code, logical_key: match code {
                KeyCode::Enter => Key::Enter,
                KeyCode::ArrowRight => Key::ArrowRight,
                KeyCode::AltLeft | KeyCode::AltRight => Key::Alt,
                _ => Key::Character("x".into()),
            },
            state: if down { ButtonState::Pressed } else { ButtonState::Released },
            text: None, repeat, window: Entity::PLACEHOLDER,
        });
    }

    pub(crate) fn playing_app(device: InputDevice) -> (App, Entity) {
        let (mut app, pad) = app();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .insert_state(crate::PlayState::Playing)
            .init_resource::<crate::control::PlayerControl>()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(50)))
            .add_systems(Update, crate::control::read_controls);
        app.world_mut().resource_mut::<Bindings>().device = device;
        app.world_mut().resource_mut::<crate::pause::Paused>().0 = false;
        (app, pad)
    }

    pub(crate) fn control_button(app: &mut App, pad: Entity, device: InputDevice, control: crate::bindings::Control, down: bool) {
        use bevy::input::{gamepad::{RawGamepadButtonChangedEvent, RawGamepadEvent}, mouse::MouseButtonInput, ButtonState};
        let bindings = app.world().resource::<Bindings>();
        if device == InputDevice::Controller {
            let Some(super::super::PadInput::Button(b)) = bindings.pad(control) else { panic!("test needs a pad button") };
            app.world_mut().write_message(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(pad, b, if down { 1.0 } else { 0.0 })));
        } else {
            let binding = bindings.get(control);
            if let Some(code) = binding.key {
                keyboard(app, code, down, false);
            } else {
                app.world_mut().write_message(MouseButtonInput {
                    button: binding.mouse.expect("test needs a mouse button"),
                    state: if down { ButtonState::Pressed } else { ButtonState::Released }, window: Entity::PLACEHOLDER,
                });
            }
        }
    }

    fn switch_from(app: &mut App, pad: Entity, device: InputDevice, down: bool) {
        if device == InputDevice::KeyboardMouse {
            button(app, pad, down);
        } else {
            keyboard(app, KeyCode::KeyX, down, false);
        }
    }

    fn manual_switch_from(app: &mut App, pad: Entity, device: InputDevice) {
        switch_from(app, pad, device, true);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, device, "a held old device refuses automatic switching");
        let target = if device == InputDevice::KeyboardMouse { InputDevice::Controller } else { InputDevice::KeyboardMouse };
        crate::bindings::set_device(app.world_mut(), target);
    }

    #[test]
    fn manual_switching_cancels_held_orbit_without_a_pov_tap_and_fresh_taps_still_work() {
        use crate::{bindings::Control, control::{PlayerControl, Pov}};
        for origin in [InputDevice::KeyboardMouse, InputDevice::Controller] {
            for moved in [false, true] {
                let (mut app, pad) = playing_app(origin);
                control_button(&mut app, pad, origin, Control::TogglePov, true);
                app.update();
                assert!(app.world().resource::<PlayerControl>().orbit.held);
                {
                    let mut c = app.world_mut().resource_mut::<PlayerControl>();
                    c.orbit.moved = moved;
                    c.orbit.yaw = if moved { 0.7 } else { 0.0 };
                }
                manual_switch_from(&mut app, pad, origin);
                let target = app.world().resource::<Bindings>().device;
                assert_ne!(target, origin);
                let c = app.world().resource::<PlayerControl>();
                assert!(!c.orbit.held && !c.orbit.moved);
                assert_eq!(c.pov, Pov::First);
                assert_eq!(c.orbit.yaw, if moved { 0.7 } else { 0.0 });
                control_button(&mut app, pad, origin, Control::TogglePov, false);
                switch_from(&mut app, pad, origin, false);
                app.update();
                if target == InputDevice::Controller {
                    app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(GamepadAxis::RightStickX, 0.7);
                    app.update();
                    assert!(!app.world().resource::<PlayerControl>().orbit.held);
                    assert_eq!(app.world().resource::<PlayerControl>().pov, Pov::First);
                    app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(GamepadAxis::RightStickX, 0.0);
                }
                control_button(&mut app, pad, target, Control::TogglePov, true);
                app.update();
                assert!(app.world().resource::<PlayerControl>().orbit.held);
                control_button(&mut app, pad, target, Control::TogglePov, false);
                app.update();
                assert_eq!(app.world().resource::<PlayerControl>().pov, Pov::Third);
            }
        }
    }

    #[test]
    fn manual_switching_cancels_ready_item_without_reloading_or_holstering() {
        use crate::{bindings::Control, control::PlayerControl};
        for origin in [InputDevice::KeyboardMouse, InputDevice::Controller] {
            let (mut app, pad) = playing_app(origin);
            app.world_mut().resource_mut::<PlayerControl>().weapon_out = true;
            control_button(&mut app, pad, origin, Control::ReadyItem, true);
            app.update();
            assert!(app.world().resource::<PlayerControl>().ready_held.is_some());
            manual_switch_from(&mut app, pad, origin);
            control_button(&mut app, pad, origin, Control::ReadyItem, false);
            switch_from(&mut app, pad, origin, false);
            for _ in 0..15 {
                app.update();
                let c = app.world().resource::<PlayerControl>();
                assert!(c.ready_held.is_none() && c.weapon_out && !c.reload);
            }
            let target = app.world().resource::<Bindings>().device;
            control_button(&mut app, pad, target, Control::ReadyItem, true);
            app.update();
            control_button(&mut app, pad, target, Control::ReadyItem, false);
            app.update();
            assert!(app.world().resource::<PlayerControl>().reload);
        }
    }

    #[test]
    fn switching_ends_aim_block_and_attack_holds_and_allows_fresh_actions() {
        use crate::{bindings::Control, control::PlayerControl};
        for origin in [InputDevice::KeyboardMouse, InputDevice::Controller] {
            for action in [Control::Block, Control::Use] {
                let (mut app, pad) = playing_app(origin);
                app.world_mut().resource_mut::<PlayerControl>().weapon_out = true;
                control_button(&mut app, pad, origin, action, true);
                app.update();
                let c = app.world().resource::<PlayerControl>();
                assert!(if action == Control::Block { c.aiming } else { c.trigger });
                manual_switch_from(&mut app, pad, origin);
                control_button(&mut app, pad, origin, action, false);
                switch_from(&mut app, pad, origin, false);
                app.update();
                let c = app.world().resource::<PlayerControl>();
                assert!(!c.aiming && !c.trigger);
                let target = app.world().resource::<Bindings>().device;
                control_button(&mut app, pad, target, action, true);
                app.update();
                let c = app.world().resource::<PlayerControl>();
                assert!(if action == Control::Block { c.aiming } else { c.trigger });
            }
        }
    }

    #[test]
    fn switching_preserves_sneak_and_run_toggles_and_tracks_old_key_releases() {
        use crate::{bindings::Control, control::PlayerControl};
        let (mut app, pad) = playing_app(InputDevice::KeyboardMouse);
        for action in [Control::CrouchSneak, Control::AlwaysRun, Control::AutoMove, Control::Run] {
            control_button(&mut app, pad, InputDevice::KeyboardMouse, action, true);
        }
        app.update();
        assert!(app.world().resource::<PlayerControl>().sneak);
        let always_run = app.world().resource::<PlayerControl>().always_run;
        assert!(app.world().resource::<PlayerControl>().auto_move);
        button(&mut app, pad, true);
        app.update();
        for action in [Control::CrouchSneak, Control::AlwaysRun, Control::AutoMove, Control::Run] {
            control_button(&mut app, pad, InputDevice::KeyboardMouse, action, false);
        }
        button(&mut app, pad, false);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
        button(&mut app, pad, true);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
        assert!(app.world().resource::<SwitchingInput>().physical_keys.get_pressed().next().is_none());
        assert!(app.world().resource::<ButtonInput<KeyCode>>().get_pressed().next().is_none());
        let c = app.world().resource::<PlayerControl>();
        assert!(c.sneak && c.auto_move);
        assert_eq!(c.always_run, always_run);
    }

    #[test]
    fn manual_device_change_cancels_holds_but_same_device_assignment_preserves_them() {
        use crate::control::{Orbit, PlayerControl};
        let (mut app, _) = playing_app(InputDevice::KeyboardMouse);
        let before = PlayerControl {
            orbit: Orbit { held: true, moved: true, yaw: 0.7, pitch: 0.2, free: true },
            ready_held: Some(0.2), aiming: true, trigger: true, sneak: true, auto_move: true,
            ..default()
        };
        app.world_mut().insert_resource(before);
        crate::bindings::set_device(app.world_mut(), InputDevice::KeyboardMouse);
        assert_eq!(*app.world().resource::<PlayerControl>(), before);
        crate::bindings::set_device(app.world_mut(), InputDevice::Controller);
        assert_eq!(*app.world().resource::<PlayerControl>(), PlayerControl {
            orbit: Orbit { held: false, moved: false, ..before.orbit },
            ready_held: None, aiming: false, trigger: false, ..before
        });
    }

    #[derive(Resource, Default)]
    struct Seen {
        keys: ButtonInput<KeyCode>,
        mouse: ButtonInput<MouseButton>,
        pad: [bool; 3],
        stick: Vec2,
        repeats: Vec<bool>,
    }

    fn see(keys: ActionButtons<KeyCode>, mouse: ActionButtons<MouseButton>, pads: ActionPads,
        mut typed: MessageReader<bevy::input::keyboard::KeyboardInput>, filter: Res<SwitchingInput>, mut seen: ResMut<Seen>) {
        seen.keys = keys.snapshot().into_owned();
        seen.mouse = mouse.snapshot().into_owned();
        if let Some(g) = pads.iter().next() {
            seen.pad = [g.pressed(GamepadButton::South), g.just_pressed(GamepadButton::South), g.just_released(GamepadButton::South)];
            seen.stick = g.left_stick();
        }
        let typed: Vec<_> = typed.read().filter(|e| filter.allows(e)).cloned().collect();
        let repeats = super::super::sideways(&typed, &seen.keys);
        seen.repeats.extend(repeats);
    }

    fn observed_app(device: InputDevice) -> (App, Entity) {
        let (mut app, pad) = app();
        app.world_mut().resource_mut::<Bindings>().device = device;
        app.init_resource::<Seen>().add_systems(Update, see);
        (app, pad)
    }

    #[test]
    fn switching_filters_only_the_fresh_edge_and_always_delivers_releases() {
        let (mut app, pad) = observed_app(InputDevice::KeyboardMouse);
        button(&mut app, pad, true);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
        assert_eq!(app.world().resource::<Seen>().pad, [false; 3]);
        assert!(app.world().get::<Gamepad>(pad).unwrap().pressed(GamepadButton::South));
        assert!(!app.world().resource::<Seen>().keys.just_pressed(KeyCode::Enter));
        app.update();
        assert_eq!(app.world().resource::<Seen>().pad, [true, false, false]);
        button(&mut app, pad, false);
        key(&mut app, true, false);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
        assert_eq!(app.world().resource::<Seen>().pad, [false, false, true]);
        key(&mut app, false, false);
        app.update();
        key(&mut app, true, false);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
        let seen = app.world().resource::<Seen>();
        assert!(!seen.keys.just_pressed(KeyCode::Enter));
        key(&mut app, true, true);
        app.update();
        assert!(app.world().resource::<Seen>().keys.pressed(KeyCode::Enter));
        key(&mut app, false, false);
        app.update();
        assert!(app.world().resource::<Seen>().keys.just_released(KeyCode::Enter));
        key(&mut app, true, false);
        app.update();
        assert!(app.world().resource::<Seen>().keys.just_pressed(KeyCode::Enter));
    }

    #[test]
    fn switching_filters_initial_menu_adjustment_but_later_repeats_work() {
        let (mut app, _) = observed_app(InputDevice::Controller);
        keyboard(&mut app, KeyCode::ArrowRight, true, false);
        app.update();
        assert!(app.world().resource::<Seen>().repeats.is_empty());
        keyboard(&mut app, KeyCode::ArrowRight, true, true);
        app.update();
        keyboard(&mut app, KeyCode::ArrowRight, false, false);
        app.update();
        assert_eq!(app.world().resource::<Seen>().repeats, vec![true]);
        assert!(app.world().resource::<Seen>().keys.just_released(KeyCode::ArrowRight));
    }

    #[test]
    fn switching_from_held_menu_a_releases_synthetic_enter_and_a_fresh_enter_works() {
        let (mut app, pad) = observed_app(InputDevice::Controller);
        button(&mut app, pad, true);
        app.update();
        assert!(app.world().resource::<Seen>().keys.pressed(KeyCode::Enter));
        keyboard(&mut app, KeyCode::KeyX, true, false);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
        crate::bindings::set_device(app.world_mut(), InputDevice::KeyboardMouse);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
        assert!(app.world().resource::<super::super::MenuKeys>().held.is_empty());
        assert!(!app.world().resource::<Seen>().keys.pressed(KeyCode::Enter));
        assert!(app.world().resource::<Seen>().keys.just_released(KeyCode::Enter));
        button(&mut app, pad, false);
        keyboard(&mut app, KeyCode::KeyX, false, false);
        app.update();
        key(&mut app, true, false);
        app.update();
        assert!(app.world().resource::<Seen>().keys.just_pressed(KeyCode::Enter));
    }

    #[test]
    fn physical_enter_sharing_a_synthetic_enter_keeps_its_hold_and_release() {
        let (mut app, pad) = observed_app(InputDevice::Controller);
        button(&mut app, pad, true);
        app.update();
        key(&mut app, true, false);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
        crate::bindings::set_device(app.world_mut(), InputDevice::KeyboardMouse);
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
        assert!(app.world().resource::<Seen>().keys.pressed(KeyCode::Enter));
        button(&mut app, pad, false);
        app.update();
        assert!(app.world().resource::<Seen>().keys.pressed(KeyCode::Enter));
        key(&mut app, false, false);
        app.update();
        assert!(app.world().resource::<Seen>().keys.just_released(KeyCode::Enter));
        key(&mut app, true, false);
        app.update();
        assert!(app.world().resource::<Seen>().keys.just_pressed(KeyCode::Enter));
    }

    #[test]
    fn alt_release_after_two_switches_leaves_plain_enter_for_the_menu() {
        let (mut app, pad) = observed_app(InputDevice::KeyboardMouse);
        app.init_resource::<crate::full_screen_key::Alts>().init_resource::<crate::full_screen_key::Asked>();
        app.add_systems(PreUpdate, crate::full_screen_key::read.after(automatic).before(super::super::menu_keys));
        for alt in [KeyCode::AltLeft, KeyCode::AltRight] {
            keyboard(&mut app, alt, true, false);
            app.update();
            button(&mut app, pad, true);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
            crate::bindings::set_device(app.world_mut(), InputDevice::Controller);
            button(&mut app, pad, false);
            app.update();
            keyboard(&mut app, KeyCode::KeyX, true, false);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
            keyboard(&mut app, alt, false, false);
            keyboard(&mut app, KeyCode::KeyX, false, false);
            app.update();
            assert!(app.world().resource::<Seen>().keys.just_released(alt));
            key(&mut app, true, false);
            app.update();
            assert!(app.world().resource::<Seen>().keys.just_pressed(KeyCode::Enter));
            key(&mut app, false, false);
            app.update();
        }
    }

    #[test]
    fn focus_loss_clears_physical_detection_for_the_next_press() {
        let (mut app, _) = observed_app(InputDevice::Controller);
        key(&mut app, true, false);
        app.update();
        app.world_mut().write_message(bevy::input::keyboard::KeyboardFocusLost);
        app.update();
        assert!(app.world().resource::<Seen>().keys.just_released(KeyCode::Enter));
        assert!(app.world().resource::<SwitchingInput>().physical_keys.get_pressed().next().is_none());
        key(&mut app, true, false);
        app.update();
        assert!(app.world().resource::<Seen>().keys.just_pressed(KeyCode::Enter));
    }

    #[test]
    fn a_switching_stick_crossing_does_not_become_a_delayed_press() {
        use crate::bindings::{Control, ControlInput};
        #[derive(Resource, Default)]
        struct Direction([bool; 3]);
        fn read(input: ControlInput, mut seen: ResMut<Direction>) {
            seen.0 = [input.pressed(Control::Forward), input.just_pressed(Control::Forward), input.just_released(Control::Forward)];
        }
        let (mut app, pad) = observed_app(InputDevice::KeyboardMouse);
        app.init_resource::<super::super::StickButtons>().init_resource::<Direction>()
            .add_systems(PreUpdate, super::super::update_stick_buttons.after(automatic))
            .add_systems(Update, read);
        app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(GamepadAxis::LeftStickY, 0.8);
        app.update();
        assert_eq!(app.world().resource::<Direction>().0, [false; 3]);
        app.update();
        assert_eq!(app.world().resource::<Direction>().0, [true, false, false]);
        keyboard(&mut app, KeyCode::KeyX, true, false);
        app.update();
        app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(GamepadAxis::LeftStickY, 0.0);
        app.update();
        assert_eq!(app.world().resource::<Direction>().0, [false, false, true]);
    }

    #[test]
    fn analog_crossing_filters_one_frame_and_a_held_stick_does_not_steal_back() {
        let (mut app, pad) = observed_app(InputDevice::KeyboardMouse);
        app.update();
        for input in [GamepadInput::Axis(GamepadAxis::LeftStickX), GamepadAxis::RightStickY.into(),
            GamepadButton::LeftTrigger2.into(), GamepadButton::RightTrigger2.into()] {
            app.world_mut().resource_mut::<Bindings>().device = InputDevice::KeyboardMouse;
            app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(input, STICK_DEAD);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
            app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(input, STICK_DEAD + 0.01);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
            assert_eq!(app.world().get::<Gamepad>(pad).unwrap().get(input), Some(STICK_DEAD + 0.01));
            assert!(app.world().resource::<SwitchingInput>().analog[&pad].into_iter().any(|b| b));
            key(&mut app, true, false);
            app.update();
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
            app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(input, 0.0);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
            key(&mut app, false, false);
            app.update();
            key(&mut app, true, false);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
            assert!(app.world().resource::<SwitchingInput>().analog.is_empty());
            key(&mut app, false, false);
            app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(input, 0.0);
            app.update();
        }
    }

    #[test]
    fn connection_rebinding_disabled_policy_and_simultaneous_input_do_not_switch() {
        let (mut app, pad) = observed_app(InputDevice::KeyboardMouse);
        app.update();
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
    fn mouse_motion_does_not_switch_and_switching_click_keeps_its_release() {
        use bevy::input::{mouse::{MouseButtonInput, MouseMotion}, ButtonState};
        let (mut app, _) = observed_app(InputDevice::Controller);
        app.world_mut().write_message(MouseMotion { delta: Vec2::new(0.1, -0.1) });
        app.update();
        assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
        for state in [ButtonState::Pressed, ButtonState::Released, ButtonState::Pressed] {
            app.world_mut().write_message(MouseButtonInput { button: MouseButton::Left, state, window: Entity::PLACEHOLDER });
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
            assert_eq!(app.world().resource::<Seen>().mouse.just_released(MouseButton::Left), state == ButtonState::Released);
        }
        assert!(app.world().resource::<Seen>().mouse.just_pressed(MouseButton::Left));
    }

    #[test]
    fn switching_press_release_pairs_preserve_all_release_bits_and_raw_events() {
        use bevy::input::{mouse::MouseButtonInput, ButtonState};
        let mut filter = SwitchingInput { keys: vec![KeyCode::Enter], mouse: vec![MouseButton::Left], ..default() };
        let mut raw = ButtonInput::default();
        raw.press(KeyCode::Enter); raw.release(KeyCode::Enter);
        assert!(!filter.filter(&raw).just_pressed(KeyCode::Enter));
        assert!(filter.filter(&raw).just_released(KeyCode::Enter));
        let event = MouseButtonInput { button: MouseButton::Left, state: ButtonState::Released, window: Entity::PLACEHOLDER };
        assert!(filter.allows_mouse(&event));
        assert!(!filter.allows_mouse(&MouseButtonInput { state: ButtonState::Pressed, ..event }));
        filter.mouse.clear();
        assert!(filter.allows_mouse(&MouseButtonInput { state: ButtonState::Pressed, ..event }));
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

    #[test]
    fn old_device_busy_refuses_every_incoming_cause() {
        for a in [Activity { button: true, ..default() }, Activity { stick: true, ..default() }, Activity { trigger: true, ..default() }] {
            assert_eq!(switch_on_use(InputDevice::KeyboardMouse, true, Activity { keyboard_busy: true, ..a }), None);
        }
        for a in [Activity { key: true, ..default() }, Activity { mouse: true, ..default() }] {
            assert_eq!(switch_on_use(InputDevice::Controller, true, Activity { controller_busy: true, ..a }), None);
        }
        let mut keys = ButtonInput::default();
        assert!(!digital_busy(&keys));
        keys.press(KeyCode::KeyF);
        assert!(!digital_busy(&keys));
        keys.clear();
        assert!(digital_busy(&keys));
        keys.release(KeyCode::KeyF);
        assert!(digital_busy(&keys));
        keys.press(KeyCode::KeyF);
        assert!(digital_busy(&keys), "release/repress cannot hide the start-of-frame hold");
        keys.release(KeyCode::KeyF);
        keys.clear();
        keys.press(KeyCode::KeyF);
        keys.release(KeyCode::KeyF);
        assert!(digital_busy(&keys), "a same-frame tap is also busy");
        keys.clear();
        assert!(!digital_busy(&keys));
    }

    #[test]
    fn held_old_device_and_release_frame_refuse_switch_until_a_fresh_press() {
        use crate::bindings::Control;
        for origin in [InputDevice::KeyboardMouse, InputDevice::Controller] {
            for action in [Control::Forward, Control::Block, Control::TogglePov] {
                let (mut app, pad) = observed_app(origin);
                // Forward is a stick on Controller; the other cases use digital buttons
                // [design, #2200; Bindings::pad].
                if origin == InputDevice::Controller && action == Control::Forward {
                    app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(GamepadAxis::LeftStickY, 0.8);
                } else {
                    control_button(&mut app, pad, origin, action, true);
                }
                app.update();
                switch_from(&mut app, pad, origin, true);
                app.update();
                assert_eq!(app.world().resource::<Bindings>().device, origin);
                switch_from(&mut app, pad, origin, false);
                if origin == InputDevice::Controller && action == Control::Forward {
                    app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(GamepadAxis::LeftStickY, 0.0);
                } else {
                    control_button(&mut app, pad, origin, action, false);
                }
                switch_from(&mut app, pad, origin, true);
                app.update();
                assert_eq!(app.world().resource::<Bindings>().device, origin, "old release/neutral frame");
                app.update();
                assert_eq!(app.world().resource::<Bindings>().device, origin, "no queued switch on continued hold");
                switch_from(&mut app, pad, origin, false);
                app.update();
                switch_from(&mut app, pad, origin, true);
                app.update();
                assert_ne!(app.world().resource::<Bindings>().device, origin);
            }
        }
    }

    #[test]
    fn disconnect_of_a_busy_pad_waits_out_the_old_device_frame() {
        for analog in [false, true] {
            let (mut app, pad) = observed_app(InputDevice::Controller);
            if analog {
                app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(GamepadAxis::LeftStickX, 0.8);
            } else {
                button(&mut app, pad, true);
            }
            app.update();
            app.world_mut().despawn(pad);
            key(&mut app, true, false);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
            key(&mut app, false, false);
            app.update();
            key(&mut app, true, false);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
        }
    }

    #[test]
    fn refused_analog_crossings_need_neutral_and_a_fresh_crossing_after_old_keys_release() {
        for input in [GamepadInput::Axis(GamepadAxis::LeftStickX), GamepadAxis::RightStickY.into(),
            GamepadButton::LeftTrigger2.into(), GamepadButton::RightTrigger2.into()] {
            let (mut app, pad) = observed_app(InputDevice::KeyboardMouse);
            keyboard(&mut app, KeyCode::KeyW, true, false);
            app.update();
            app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(input, STICK_DEAD + 0.01);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
            keyboard(&mut app, KeyCode::KeyW, false, false);
            app.update();
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
            app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(input, 0.0);
            app.update();
            app.world_mut().get_mut::<Gamepad>(pad).unwrap().analog_mut().set(input, STICK_DEAD + 0.01);
            app.update();
            assert_eq!(app.world().resource::<Bindings>().device, InputDevice::Controller);
        }
    }
}
