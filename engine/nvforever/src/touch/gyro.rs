//! Optional sensor motion, admitted separately from mouse/device activity.
use super::{layout::Target, TouchState};
use bevy::{
    input::touch::{TouchInput, TouchPhase},
    prelude::*,
    window::PrimaryWindow,
};
use nvf_play::{
    bindings::{Bindings, InputDevice},
    settings::{self, ChoiceList, Setting, Slider},
};
use std::collections::HashSet;

pub(super) const BUTTON: usize = 18;
const KEYS: [&str; 6] = [
    "touch-gyro-v1",
    "touch-gyro-sensitivity-v1",
    "touch-gyro-ads-v1",
    "touch-gyro-toggle-v1",
    "touch-gyro-ads-only-v1",
    "touch-gyro-recenter-v1",
];

#[derive(Resource, Clone, Copy, PartialEq)]
pub(super) struct Options {
    pub enabled: bool,
    sensitivity: f32,
    ads: f32,
    toggle: bool,
    ads_only: bool,
    recenter: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: false,
            sensitivity: 1.0,
            ads: 0.5,
            toggle: false,
            ads_only: true,
            recenter: 0,
        }
    }
}

#[derive(Default)]
struct Sample {
    version: u8,
    available: bool,
    epoch: u64,
    timestamp: u64,
    rotation: u8,
    x: f32,
    y: f32,
}
impl Sample {
    #[cfg(any(target_os = "android", test))]
    fn decode(text: &str) -> Result<Self, String> {
        if text.len() > 512 {
            return Err("oversized sensor sample".into());
        }
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|_| "invalid sensor sample")?;
        let object = value.as_object().ok_or("invalid sensor object")?;
        let keys = [
            "version",
            "available",
            "epoch",
            "timestamp",
            "rotation",
            "x",
            "y",
        ];
        if object.len() != keys.len() || keys.iter().any(|key| !object.contains_key(*key)) {
            return Err("invalid sensor fields".into());
        }
        let uint = |key: &str| value[key].as_u64().ok_or("invalid sensor integer");
        let number = |key: &str| value[key].as_f64().ok_or("invalid sensor number");
        let s = Self {
            version: u8::try_from(uint("version")?).map_err(|_| "invalid sensor version")?,
            available: value["available"]
                .as_bool()
                .ok_or("invalid sensor availability")?,
            epoch: uint("epoch")?,
            timestamp: uint("timestamp")?,
            rotation: u8::try_from(uint("rotation")?).map_err(|_| "invalid sensor rotation")?,
            x: number("x")? as f32,
            y: number("y")? as f32,
        };
        if !s.valid() {
            return Err("invalid sensor sample bounds".into());
        }
        Ok(s)
    }
    fn valid(&self) -> bool {
        self.version == 1
            && self.rotation <= 3
            && Vec2::new(self.x, self.y).is_finite()
            && self.x.abs().max(self.y.abs()) <= 0.5
    }
}
// Keep the default1x exactly on a serialized slider tick while retaining both bounds.
fn sensitivity_value(part: f32) -> f32 {
    if part <= 0.5 {
        0.25 + part * 1.5
    } else {
        1.0 + (part - 0.5) * 4.0
    }
}
fn sensitivity_part(value: f32) -> f32 {
    if value <= 1.0 {
        (value - 0.25) / 1.5
    } else {
        0.5 + (value - 1.0) / 4.0
    }
}

type Poll = Box<dyn FnMut(&str) -> Result<Sample, String> + Send + Sync>;
#[derive(Resource)]
struct Backend(Poll);
impl Default for Backend {
    fn default() -> Self {
        Self(Box::new(|command| {
            #[cfg(target_os = "android")]
            {
                Sample::decode(&crate::android::activity_string(
                    "gyroSample",
                    Some(command),
                )?)
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = command;
                Ok(Sample {
                    version: 1,
                    ..default()
                })
            }
        }))
    }
}

#[derive(Resource, Default)]
pub(super) struct State {
    contacts: HashSet<u64>,
    held: Option<u64>,
    latched: bool,
    previous: Option<Options>,
    viewport: Option<(Entity, Vec2)>,
    epoch: Option<u64>,
    timestamp: u64,
    polled: bool,
    requested: bool,
    available: bool,
    error: bool,
}
impl State {
    fn cancel(&mut self) {
        self.contacts.clear();
        self.held = None;
        self.latched = false;
    }
    pub(super) fn active(&self, options: &Options) -> bool {
        if options.toggle {
            self.latched
        } else {
            self.held.is_some()
        }
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct PollGyro;

pub(super) fn install(app: &mut App) {
    app.init_resource::<Options>()
        .init_resource::<State>()
        .init_resource::<Backend>()
        .init_resource::<nvf_play::player::GyroLook>()
        .add_systems(
            PreUpdate,
            poll.after(super::input)
                .after(nvf_play::pad::InputSwitchSystems::Switch)
                .in_set(PollGyro),
        )
        .add_systems(Update, captions.after(super::show_labels));
    for (index, label, note) in [
        (
            0,
            "Gyro aiming",
            "Optional motion aiming. Off by default; a game-rotation sensor is required. Move/resize the Gyro target in the touch editor.",
        ),
        (
            3,
            "Gyro activation: toggle",
            "Off: hold the Gyro target. On: tap to activate and tap again to stop. Menus and input handoffs always stop activation.",
        ),
        (
            4,
            "Gyro only while aiming",
            "Apply motion only when the engine actually has weapon sights raised. Releasing aim discards pending motion.",
        ),
    ] {
        settings::add_setting(
            app,
            Setting {
                key: KEYS[index],
                group: "Controller",
                label,
                note,
                slider: None,
                get: match index {
                    0 => |w| w.resource::<Options>().enabled,
                    3 => |w| w.resource::<Options>().toggle,
                    _ => |w| w.resource::<Options>().ads_only,
                },
                set: match index {
                    0 => |w, v| w.resource_mut::<Options>().enabled = v,
                    3 => |w, v| w.resource_mut::<Options>().toggle = v,
                    _ => |w, v| w.resource_mut::<Options>().ads_only = v,
                },
            },
        );
    }
    for (index, label, note, slider) in [
        (
            1,
            "Gyro sensitivity",
            "Motion gain from 0.25x to 3x; mouse and controller settings remain independent.",
            Slider {
                get: |w| sensitivity_part(w.resource::<Options>().sensitivity),
                set: |w, v| {
                    if v.is_finite() {
                        w.resource_mut::<Options>().sensitivity =
                            sensitivity_value(v.clamp(0.0, 1.0));
                    }
                },
                min: 0.0,
                step: 0.05,
                shown: Some(|v| format!("{:.2}x", sensitivity_value(v))),
            },
        ),
        (
            2,
            "Gyro ADS scale",
            "Multiply motion gain by 0.1x to 1x while AimZoom reports actual sights.",
            Slider {
                get: |w| w.resource::<Options>().ads,
                set: |w, v| {
                    if v.is_finite() {
                        w.resource_mut::<Options>().ads = v.clamp(0.1, 1.0);
                    }
                },
                min: 0.1,
                step: 0.05,
                shown: Some(|v| format!("{:.2}x", v)),
            },
        ),
    ] {
        settings::add_setting(
            app,
            Setting {
                key: KEYS[index],
                group: "Controller",
                label,
                note,
                get: |_| false,
                set: |_, _| {},
                slider: Some(slider),
            },
        );
    }
    settings::add_choice_list(
        app,
        Setting {
            key: KEYS[5],
            group: "Controller",
            label: "Recenter gyro",
            note: "Discard the motion reference and pending input. Your current view stays where it is.",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: |_| vec!["Ready".into(), "Recenter now".into()],
            chosen: |_| "Ready".into(),
            save: |_| "Ready".into(),
            load: |_, name| name == "Ready",
            ask_keep: false,
            pick: |w, name| {
                if name != "Recenter now" {
                    return name == "Ready";
                }
                let next = w.resource::<Options>().recenter.wrapping_add(1);
                w.resource_mut::<Options>().recenter = next;
                w.resource_mut::<State>().cancel();
                w.resource_mut::<nvf_play::player::GyroLook>().radians = Vec2::ZERO;
                true
            },
        },
    );
    settings::set_choice_action(app, KEYS[5], |name| name == "Recenter now");
}

type CaptureContext<'w> = (
    Res<'w, TouchState>,
    Res<'w, super::Layout>,
    Res<'w, super::practice::Practice>,
    Res<'w, super::modal::Capture>,
);
#[allow(clippy::too_many_arguments)]
fn poll(
    options: Res<Options>,
    mut state: ResMut<State>,
    mut backend: ResMut<Backend>,
    mut look: ResMut<nvf_play::player::GyroLook>,
    mut touches: MessageReader<TouchInput>,
    (touch, layout, practice, modal): CaptureContext<'_>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    bindings: Option<Res<Bindings>>,
    ads: Option<Res<nvf_play::aim_zoom::AimZoom>>,
    (device, platform): (
        Option<Res<nvf_play::pad::TouchDeviceSwitch>>,
        Option<Res<nvf_play::pad::PlatformInputGate>>,
    ),
    (mut lifecycle, mut focus): (
        MessageReader<bevy::window::AppLifecycle>,
        MessageReader<bevy::window::WindowFocused>,
    ),
) {
    look.radians = Vec2::ZERO;
    let viewport = windows
        .single()
        .ok()
        .map(|(id, w)| (id, Vec2::new(w.width(), w.height())));
    let interrupted = lifecycle.read().any(|e| {
        matches!(
            e,
            bevy::window::AppLifecycle::Suspended | bevy::window::AppLifecycle::Running
        )
    }) | focus
        .read()
        .any(|e| viewport.is_some_and(|(id, _)| e.window == id));
    let changed = interrupted || state.previous != Some(*options) || state.viewport != viewport;
    state.previous = Some(*options);
    state.viewport = viewport;
    let allowed = options.enabled
        && touch.playing
        && !touch.suspended
        && windows.single().is_ok_and(|(_, w)| w.focused)
        && layout.editor.is_none()
        && !practice.blocking()
        && !modal.blocking()
        && layout.config.buttons[BUTTON].visible
        && bindings
            .as_deref()
            .is_some_and(|b| b.device == InputDevice::KeyboardMouse)
        && device
            .as_deref()
            .is_none_or(nvf_play::pad::TouchDeviceSwitch::allows_actions)
        && platform
            .as_deref()
            .is_none_or(nvf_play::pad::PlatformInputGate::allows_actions);
    if changed || !allowed {
        state.cancel();
        touches.clear();
    } else if let Some((window, size)) = viewport {
        for t in touches.read().filter(|t| t.window == window) {
            if platform
                .as_deref()
                .is_some_and(|gate| !gate.allows_touch(t.id))
            {
                state.cancel();
                continue;
            }
            if !t.position.is_finite() {
                state.cancel();
                continue;
            }
            match t.phase {
                TouchPhase::Started
                    if layout.config.hit(t.position, size, true, false)
                        == Some(Target::Button(BUTTON)) =>
                {
                    if !state.contacts.insert(t.id) || state.contacts.len() != 1 {
                        state.held = None;
                        state.latched = false;
                        continue;
                    }
                    state.held = Some(t.id);
                    if options.toggle {
                        state.latched = !state.latched;
                    }
                }
                TouchPhase::Ended => {
                    state.contacts.remove(&t.id);
                    if state.held == Some(t.id) {
                        state.held = None;
                    }
                }
                TouchPhase::Canceled => {
                    if state.contacts.contains(&t.id) {
                        state.cancel();
                    }
                }
                _ => {}
            }
        }
    }
    let aimed = ads.as_deref().is_some_and(|a| a.aimed);
    let requested = allowed && !changed && state.active(&options) && (!options.ads_only || aimed);
    if !state.polled || options.enabled || state.requested || changed || state.error {
        let command = if changed {
            "reset"
        } else if requested {
            "on"
        } else {
            "off"
        };
        match (backend.0)(command) {
            Ok(sample) if sample.valid() => {
                state.available = sample.available;
                state.error = false;
                let same_epoch = state.epoch == Some(sample.epoch);
                let fresh = same_epoch && sample.timestamp > state.timestamp;
                if same_epoch && sample.timestamp < state.timestamp {
                    state.cancel();
                    state.epoch = None;
                    state.error = true;
                } else {
                    state.epoch = Some(sample.epoch);
                    state.timestamp = sample.timestamp;
                }
                if requested && fresh && sample.available {
                    let gain = options.sensitivity * if aimed { options.ads } else { 1.0 };
                    look.radians = (Vec2::new(sample.x, sample.y) * gain)
                        .clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
                }
            }
            _ => {
                state.error = true;
                state.available = false;
                state.cancel();
                state.epoch = None;
            }
        }
        state.polled = true;
    }
    state.requested = requested;
}

pub(super) fn caption(state: &State, options: &Options) -> &'static str {
    if state.error {
        "Gyro unavailable"
    } else if !state.available {
        "No gyro sensor"
    } else if state.active(options) {
        "Gyro ON"
    } else if options.toggle {
        "Gyro: tap"
    } else {
        "Gyro: hold"
    }
}

#[derive(Component)]
pub(super) struct Fallback;
fn captions(
    state: Res<State>,
    options: Res<Options>,
    layout: Res<super::Layout>,
    parents: Query<&super::TouchVisual>,
    mut text: Query<(&ChildOf, &super::TouchVisual, &mut Text)>,
    mut fallback: Query<&mut TextColor, With<Fallback>>,
) {
    for mut color in &mut fallback {
        color.0 = Color::WHITE.with_alpha(layout.config.opacity);
    }
    for (parent, visual, mut text) in &mut text {
        if matches!(visual, super::TouchVisual::Caption)
            && matches!(
                parents.get(parent.parent()),
                Ok(super::TouchVisual::Button(BUTTON))
            )
        {
            text.0 = caption(&state, &options).into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        input::InputPlugin,
        window::{AppLifecycle, WindowFocused},
    };
    fn scene(toggle: bool) -> (App, Entity, Vec2) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputPlugin, bevy::state::app::StatesPlugin))
            .insert_state(nvf_play::PlayState::Playing)
            .init_resource::<Bindings>()
            .add_message::<AppLifecycle>()
            .add_message::<WindowFocused>()
            .add_plugins(super::super::TouchControlsPlugin)
            .init_resource::<nvf_play::aim_zoom::AimZoom>();
        let mut timestamp = 100;
        let mut epoch = 1;
        let mut previous = String::new();
        app.insert_resource(Backend(Box::new(move |command| {
            timestamp += 1;
            let stable = command == previous;
            if !stable {
                epoch += 1;
                previous = command.into();
            }
            Ok(Sample {
                version: 1,
                available: true,
                epoch,
                timestamp,
                rotation: 0,
                x: if command == "on" && stable { 0.02 } else { 0.0 },
                y: if command == "on" && stable {
                    -0.01
                } else {
                    0.0
                },
            })
        })));
        let w = app
            .world_mut()
            .spawn((
                Window {
                    resolution: bevy::window::WindowResolution::new(1000, 1000),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<super::super::Layout>()
            .config = Default::default();
        {
            let mut options = app.world_mut().resource_mut::<Options>();
            options.enabled = true;
            options.ads_only = false;
            options.toggle = toggle;
        }
        app.update();
        let p = app
            .world()
            .resource::<super::super::Layout>()
            .config
            .center(Target::Button(BUTTON), Vec2::splat(1000.0));
        (app, w, p)
    }
    fn finger(app: &mut App, w: Entity, p: Vec2, id: u64, phase: TouchPhase) {
        app.world_mut().write_message(TouchInput {
            window: w,
            position: p,
            id,
            phase,
            force: None,
        });
    }
    fn activate(app: &mut App, w: Entity, p: Vec2, toggle: bool) {
        finger(app, w, p, 1, TouchPhase::Started);
        if toggle {
            finger(app, w, p, 1, TouchPhase::Ended);
        }
        app.update();
        app.update();
    }
    fn delta(app: &App) -> Vec2 {
        app.world().resource::<nvf_play::player::GyroLook>().radians
    }
    #[test]
    fn hold_and_toggle_routes_stop_without_synthesizing_mouse_keys_or_game_actions() {
        for toggle in [false, true] {
            let (mut app, w, p) = scene(toggle);
            activate(&mut app, w, p, toggle);
            assert_eq!(delta(&app), Vec2::new(0.02, -0.01));
            assert!(app
                .world()
                .resource::<ButtonInput<KeyCode>>()
                .get_pressed()
                .next()
                .is_none());
            assert!(app
                .world()
                .resource::<ButtonInput<MouseButton>>()
                .get_pressed()
                .next()
                .is_none());
            assert!(
                app.world()
                    .resource::<bevy::input::mouse::AccumulatedMouseMotion>()
                    .delta
                    == Vec2::ZERO
            );
            if toggle {
                finger(&mut app, w, p, 2, TouchPhase::Started);
            } else {
                finger(&mut app, w, p, 1, TouchPhase::Ended);
            }
            app.update();
            assert_eq!(delta(&app), Vec2::ZERO);
            assert!(!app
                .world()
                .resource::<State>()
                .active(app.world().resource::<Options>()));
        }
    }
    #[test]
    fn actual_sight_state_scales_motion_and_ads_only_discards_the_unscoped_reference() {
        let (mut app, w, p) = scene(true);
        activate(&mut app, w, p, true);
        assert_eq!(delta(&app), Vec2::new(0.02, -0.01));
        app.world_mut()
            .resource_mut::<nvf_play::aim_zoom::AimZoom>()
            .aimed = true;
        app.update();
        assert_eq!(delta(&app), Vec2::new(0.01, -0.005));
        app.world_mut().resource_mut::<Options>().ads_only = true;
        app.update();
        activate(&mut app, w, p, true);
        assert_eq!(delta(&app), Vec2::new(0.01, -0.005));
        app.world_mut()
            .resource_mut::<nvf_play::aim_zoom::AimZoom>()
            .aimed = false;
        app.update();
        assert_eq!(delta(&app), Vec2::ZERO);
        app.world_mut()
            .resource_mut::<nvf_play::aim_zoom::AimZoom>()
            .aimed = true;
        app.update();
        assert_eq!(
            delta(&app),
            Vec2::ZERO,
            "a new sensor reference is required"
        );
        app.update();
        assert_eq!(delta(&app), Vec2::new(0.01, -0.005));
    }
    #[test]
    fn focus_pairs_cancel_and_platform_quarantine_cannot_rearm_a_held_contact() {
        let (mut app, w, p) = scene(true);
        activate(&mut app, w, p, true);
        app.world_mut().write_message(WindowFocused {
            window: w,
            focused: false,
        });
        app.world_mut().write_message(WindowFocused {
            window: w,
            focused: true,
        });
        app.update();
        assert_eq!(delta(&app), Vec2::ZERO);
        assert!(!app.world().resource::<State>().latched);
        activate(&mut app, w, p, true);
        app.init_resource::<nvf_play::pad::PlatformInputGate>();
        app.world_mut()
            .resource_mut::<nvf_play::pad::PlatformInputGate>()
            .set_blocked(true);
        finger(&mut app, w, p, 7, TouchPhase::Started);
        app.update();
        assert_eq!(delta(&app), Vec2::ZERO);
        app.world_mut()
            .resource_mut::<nvf_play::pad::PlatformInputGate>()
            .set_blocked(false);
        finger(&mut app, w, p, 7, TouchPhase::Moved);
        app.update();
        assert_eq!(delta(&app), Vec2::ZERO);
        assert!(!app.world().resource::<State>().latched);
    }
    #[test]
    fn controller_return_touch_only_restores_device_and_fresh_gyro_tap_works() {
        let (mut app, w, p) = scene(true);
        app.add_plugins(nvf_play::pad::InputSwitchPlugin);
        app.world_mut().resource_mut::<Bindings>().device = InputDevice::Controller;
        finger(&mut app, w, p, 4, TouchPhase::Started);
        finger(&mut app, w, p, 4, TouchPhase::Ended);
        app.update();
        assert_eq!(
            app.world().resource::<Bindings>().device,
            InputDevice::KeyboardMouse
        );
        assert!(!app
            .world()
            .resource::<nvf_play::pad::TouchDeviceSwitch>()
            .allows_actions());
        assert_eq!(delta(&app), Vec2::ZERO);
        assert!(!app.world().resource::<State>().latched);
        activate(&mut app, w, p, true);
        assert_eq!(delta(&app), Vec2::new(0.02, -0.01));
    }
    #[test]
    fn modal_entry_and_canceled_or_overlapping_gyro_contacts_clear_toggle_activation() {
        for cancel in [0, 1, 2] {
            let (mut app, w, p) = scene(true);
            activate(&mut app, w, p, true);
            match cancel {
                0 => {
                    app.init_resource::<nvf_play::terminal::touch::Settings>();
                    app.world_mut()
                        .resource_mut::<nvf_play::terminal::touch::Settings>()
                        .enabled = true;
                    app.insert_resource(nvf_play::terminal::TerminalScreen(Some(
                        nvf_play::terminal::Session {
                            reference: fnv_sim::FormId(1),
                            base: fnv_sim::FormId(2),
                            page: nvf_play::terminal::Page::LockedOut([
                                "Synthetic".into(),
                                "Fixture".into(),
                            ]),
                        },
                    )));
                }
                1 => {
                    finger(&mut app, w, p, 3, TouchPhase::Started);
                    finger(&mut app, w, p, 3, TouchPhase::Canceled);
                }
                _ => {
                    finger(&mut app, w, p, 3, TouchPhase::Started);
                    finger(&mut app, w, p, 4, TouchPhase::Started);
                }
            }
            app.update();
            assert_eq!(delta(&app), Vec2::ZERO);
            assert!(!app.world().resource::<State>().latched);
        }
    }
    #[test]
    fn gyro_layout_key_and_existing_action_ids_survive_legacy_layout_import() {
        let mut config = super::super::layout::Config::default();
        let names: [&str; 18] = [
            "Pause", "Pip-Boy", "VATS", "Reload", "Jump", "Use", "Aim", "Fire", "Sneak", "Up",
            "Down", "Left", "Right", "OK", "Back", "Tab", "Page +", "Page -",
        ];
        assert_eq!(
            super::super::BUTTONS[..BUTTON]
                .iter()
                .map(|b| b.text)
                .collect::<Vec<_>>(),
            names
        );
        let mut legacy = config.encode();
        legacy["buttons"].as_object_mut().unwrap().remove("Gyro");
        assert_eq!(
            super::super::layout::Config::decode(&legacy).unwrap(),
            config
        );
        config.buttons[BUTTON].center = Vec2::new(0.2, 0.4);
        config.buttons[BUTTON].scale = 1.5;
        assert_eq!(
            super::super::layout::Config::decode(&config.encode()).unwrap(),
            config
        );
        assert!(config.encode()["buttons"].get("Gyro").is_some());
        assert!(config.encode()["buttons"].get("Gyro ON").is_none());
    }
    #[test]
    fn malformed_sensor_samples_are_rejected_with_bounded_finite_fields() {
        assert!(Sample::decode(&" ".repeat(513)).is_err());
        assert!(Sample::decode(
            r#"{"version":1,"available":true,"epoch":1,"timestamp":1,"rotation":0,"x":1e300,"y":0}"#
        )
        .is_err());
        assert!(Sample::decode(
            r#"{"version":1,"available":true,"epoch":1,"timestamp":1,"rotation":4,"x":0,"y":0}"#
        )
        .is_err());
        assert!(Sample::decode(
            r#"{"version":1,"available":true,"epoch":1,"timestamp":1,"rotation":0,"x":0.01,"y":0}"#
        )
        .is_ok());
    }
    #[test]
    fn settings_persist_bounded_gain_mode_and_ads_without_persisting_a_recenter_command() {
        let mut app = App::new();
        install(&mut app);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("nvf-gyro-{}-{nonce}.ini", std::process::id()));
        app.insert_resource(settings::SettingsFile(Some(path.clone())));
        let index = |app: &App, key| {
            app.world()
                .resource::<settings::Settings>()
                .index(key)
                .unwrap()
        };
        assert!(!app.world().resource::<Options>().enabled);
        for key in [KEYS[0], KEYS[3]] {
            let i = index(&app, key);
            assert_eq!(settings::flip(app.world_mut(), i), Some(true));
        }
        for key in [KEYS[1], KEYS[2]] {
            let i = index(&app, key);
            settings::set_slider(app.world_mut(), i, 1.0);
        }
        let i = index(&app, KEYS[5]);
        assert!(settings::pick(app.world_mut(), i, "Recenter now"));
        let stored = std::fs::read_to_string(&path).unwrap();
        assert!(stored.contains("touch-gyro-recenter-v1=Ready"));
        let mut other = App::new();
        install(&mut other);
        other.insert_resource(settings::SettingsFile(Some(path.clone())));
        settings::apply_stored(other.world_mut());
        let options = other.world().resource::<Options>();
        assert!(options.enabled && options.toggle && options.ads_only);
        assert_eq!(options.sensitivity, 3.0);
        assert_eq!(options.ads, 1.0);
        assert_eq!(options.recenter, 0);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn suspend_resume_in_one_batch_cancels_activation_and_requires_fresh_input() {
        let (mut app, w, p) = scene(true);
        activate(&mut app, w, p, true);
        app.world_mut().write_message(AppLifecycle::Suspended);
        app.world_mut().write_message(AppLifecycle::Running);
        app.update();
        assert_eq!(delta(&app), Vec2::ZERO);
        assert!(!app.world().resource::<State>().latched);
        app.update();
        assert_eq!(delta(&app), Vec2::ZERO);
        activate(&mut app, w, p, true);
        assert_eq!(delta(&app), Vec2::new(0.02, -0.01));
    }
    #[test]
    fn sensor_read_failure_cancels_activation_instead_of_reusing_prior_frame_motion() {
        let (mut app, w, p) = scene(true);
        activate(&mut app, w, p, true);
        app.insert_resource(Backend(Box::new(|_| Err("synthetic unavailable".into()))));
        app.update();
        assert_eq!(delta(&app), Vec2::ZERO);
        assert!(!app.world().resource::<State>().latched);
        app.update();
        assert_eq!(delta(&app), Vec2::ZERO);
    }
    #[test]
    fn saving_a_different_setting_keeps_default_gyro_gains_exact_on_reload() {
        let mut app = App::new();
        install(&mut app);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nvf-gyro-defaults-{}-{nonce}.ini",
            std::process::id()
        ));
        app.insert_resource(settings::SettingsFile(Some(path.clone())));
        let i = app
            .world()
            .resource::<settings::Settings>()
            .index(KEYS[0])
            .unwrap();
        settings::flip(app.world_mut(), i);
        let mut loaded = App::new();
        install(&mut loaded);
        loaded.insert_resource(settings::SettingsFile(Some(path.clone())));
        settings::apply_stored(loaded.world_mut());
        assert_eq!(loaded.world().resource::<Options>().sensitivity, 1.0);
        assert_eq!(loaded.world().resource::<Options>().ads, 0.5);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn quarantined_raw_touch_cannot_reactivate_gyro_after_the_platform_cover_closes() {
        let (mut app, w, p) = scene(true);
        app.init_resource::<nvf_play::pad::PlatformInputGate>()
            .add_plugins(nvf_play::pad::InputSwitchPlugin);
        app.world_mut()
            .resource_mut::<nvf_play::pad::PlatformInputGate>()
            .set_blocked(true);
        finger(&mut app, w, p, 73, TouchPhase::Started);
        app.update();
        app.world_mut()
            .resource_mut::<nvf_play::pad::PlatformInputGate>()
            .set_blocked(false);
        finger(&mut app, w, p, 73, TouchPhase::Started);
        finger(&mut app, w, p, 73, TouchPhase::Ended);
        app.update();
        assert!(!app
            .world()
            .resource::<nvf_play::pad::PlatformInputGate>()
            .allows_touch(73));
        assert_eq!(delta(&app), Vec2::ZERO);
        assert!(!app.world().resource::<State>().latched);
        app.update();
        activate(&mut app, w, p, true);
        assert_eq!(delta(&app), Vec2::new(0.02, -0.01));
    }

    #[test]
    fn default_gyro_target_clears_utilities_and_gameplay_buttons_in_every_preset() {
        use super::super::{
            layout::{utility_rect, Config},
            profiles,
        };
        for size in [
            Vec2::new(792.0, 363.0),
            Vec2::new(640.0, 360.0),
            Vec2::new(576.0, 270.0),
            Vec2::new(320.0, 240.0),
            Vec2::new(1280.0, 720.0),
            Vec2::new(1920.0, 1080.0),
            Vec2::new(320.0, 792.0),
            Vec2::splat(1000.0),
        ] {
            let mut configurations = vec![Config::preset(false), Config::preset(true)];
            for (name, _) in profiles::PRESETS {
                let mut c = Config::default();
                c.apply_preset(name);
                configurations.push(c);
            }
            for c in configurations {
                let target = Target::Button(BUTTON);
                let center = c.center(target, size);
                let rect = Rect::from_center_size(center, Vec2::splat(c.diameter(target, size)));
                for help in [false, true] {
                    assert!(
                        rect.intersect(utility_rect(help, size)).is_empty(),
                        "gyro must clear the entire utility hit rectangle at {size:?}"
                    );
                }
                for i in 0..9 {
                    let other = Target::Button(i);
                    let bounds = Rect::from_center_size(
                        c.center(other, size),
                        Vec2::splat(c.diameter(other, size)),
                    );
                    assert!(
                        rect.intersect(bounds).is_empty(),
                        "gyro/{} overlap at {size:?}",
                        super::super::BUTTONS[i].text
                    );
                }
                assert_eq!(c.hit(center, size, true, false), Some(target));
            }
        }
    }
    #[test]
    fn default_gyro_center_routes_hold_and_toggle_while_the_cog_still_opens_the_editor() {
        use super::super::{
            layout::{utility_rect, Config},
            profiles,
        };
        for (preset, _) in profiles::PRESETS {
            for toggle in [false, true] {
                let (mut app, w, _) = scene(toggle);
                let size = Vec2::new(792.0, 363.0);
                app.world_mut()
                    .get_mut::<Window>(w)
                    .unwrap()
                    .resolution
                    .set(size.x, size.y);
                let mut config = Config::default();
                config.apply_preset(preset);
                let p = config.center(Target::Button(BUTTON), size);
                app.world_mut()
                    .resource_mut::<super::super::Layout>()
                    .config = config;
                app.update();
                app.update();
                activate(&mut app, w, p, toggle);
                assert!(
                    app.world()
                        .resource::<super::super::Layout>()
                        .editor
                        .is_none(),
                    "{preset}: gyro activation must not route to cog"
                );
                assert_eq!(
                    delta(&app),
                    Vec2::new(0.02, -0.01),
                    "{preset}: actual gyro route"
                );
                assert_eq!(app.world().resource::<State>().latched, toggle);
                if toggle {
                    finger(&mut app, w, p, 2, TouchPhase::Started);
                    finger(&mut app, w, p, 2, TouchPhase::Ended);
                } else {
                    finger(&mut app, w, p, 1, TouchPhase::Ended);
                }
                app.update();
                app.update();
                assert_eq!(delta(&app), Vec2::ZERO);
                assert!(!app.world().resource::<State>().latched);
                let cog = utility_rect(false, size).center();
                finger(&mut app, w, cog, 3, TouchPhase::Started);
                app.update();
                assert!(
                    app.world()
                        .resource::<super::super::Layout>()
                        .editor
                        .is_some(),
                    "{preset}: cog must still route to actual editor"
                );
                assert_eq!(delta(&app), Vec2::ZERO);
                assert!(!app.world().resource::<State>().latched);
            }
        }
    }
    #[test]
    fn changing_the_gyro_default_does_not_reposition_existing_saved_targets() {
        use super::super::layout::Config;
        let mut config = Config::default();
        config.buttons[BUTTON].center = Vec2::new(0.485, 0.09);
        config.buttons[BUTTON].scale = 1.6;
        config.buttons[BUTTON].visible = false;
        let saved = config.encode();
        let loaded = Config::decode(&saved).unwrap();
        assert_eq!(loaded.encode(), saved);
        assert_eq!(loaded.buttons[BUTTON].center, Vec2::new(0.485, 0.09));
        assert_ne!(
            loaded.buttons[BUTTON].center,
            Config::default().buttons[BUTTON].center
        );
    }
}
