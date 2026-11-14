//! Touch and physical input share aggregate key/mouse state without releasing each other (#2436).
//! Positions and sensitivity are NVForever design choices, not original-game facts.
use bevy::input::{
    keyboard::{KeyboardFocusLost, KeyboardInput},
    mouse::{MouseButtonInput, MouseMotion},
    touch::{TouchInput, TouchPhase},
    ButtonState, InputSystems,
};
use bevy::prelude::*;
use bevy::window::{AppLifecycle, PrimaryWindow, WindowFocused};
use nvf_play::bindings::{Bindings, Control};
use std::collections::{HashMap, HashSet};

pub struct TouchControlsPlugin;
impl Plugin for TouchControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TouchState>()
            .add_systems(
                PreUpdate,
                (capture_physical, input).chain().before(InputSystems),
            )
            .add_systems(
                PreUpdate,
                merge_sources
                    .after(InputSystems)
                    .before(bevy::ui::UiSystems::Focus),
            )
            .add_systems(Startup, labels)
            .add_systems(Update, show_labels);
    }
}
#[derive(Clone, Copy)]
struct Contact {
    origin: Vec2,
    position: Vec2,
    role: Role,
}
#[derive(Clone, Copy)]
enum Role {
    Move,
    Look,
    Button(usize),
}
#[derive(Resource, Default)]
struct TouchState {
    contacts: HashMap<u64, Contact>,
    keys: HashSet<KeyCode>,
    mouse: HashSet<MouseButton>,
    playing: bool,
    suspended: bool,
    saved_volume: Option<f32>,
    physical_keys: HashSet<KeyCode>,
    physical_mouse: HashSet<MouseButton>,
    managed_keys: HashSet<KeyCode>,
    managed_mouse: HashSet<MouseButton>,
    combined_keys: ButtonInput<KeyCode>,
    combined_mouse: ButtonInput<MouseButton>,
}
#[derive(Clone, Copy)]
struct ButtonSpec {
    text: &'static str,
    x: f32,
    y: f32,
    control: Option<Control>,
    key: Option<KeyCode>,
    playing: bool,
}
const BUTTONS: &[ButtonSpec] = &[
    ButtonSpec {
        text: "Pause",
        x: 0.88,
        y: 0.03,
        control: None,
        key: Some(KeyCode::Escape),
        playing: true,
    },
    ButtonSpec {
        text: "Pip-Boy",
        x: 0.76,
        y: 0.03,
        control: Some(Control::MenuMode),
        key: None,
        playing: true,
    },
    ButtonSpec {
        text: "VATS",
        x: 0.64,
        y: 0.03,
        control: Some(Control::Vats),
        key: None,
        playing: true,
    },
    ButtonSpec {
        text: "Reload",
        x: 0.88,
        y: 0.56,
        control: Some(Control::ReadyItem),
        key: None,
        playing: true,
    },
    ButtonSpec {
        text: "Jump",
        x: 0.76,
        y: 0.56,
        control: Some(Control::Jump),
        key: None,
        playing: true,
    },
    ButtonSpec {
        text: "Use",
        x: 0.88,
        y: 0.70,
        control: Some(Control::Activate),
        key: None,
        playing: true,
    },
    ButtonSpec {
        text: "Aim",
        x: 0.76,
        y: 0.70,
        control: Some(Control::Block),
        key: None,
        playing: true,
    },
    ButtonSpec {
        text: "Fire",
        x: 0.88,
        y: 0.84,
        control: Some(Control::Use),
        key: None,
        playing: true,
    },
    ButtonSpec {
        text: "Sneak",
        x: 0.76,
        y: 0.84,
        control: Some(Control::CrouchSneak),
        key: None,
        playing: true,
    },
    ButtonSpec {
        text: "Up",
        x: 0.12,
        y: 0.56,
        control: None,
        key: Some(KeyCode::ArrowUp),
        playing: false,
    },
    ButtonSpec {
        text: "Down",
        x: 0.12,
        y: 0.84,
        control: None,
        key: Some(KeyCode::ArrowDown),
        playing: false,
    },
    ButtonSpec {
        text: "Left",
        x: 0.00,
        y: 0.70,
        control: None,
        key: Some(KeyCode::ArrowLeft),
        playing: false,
    },
    ButtonSpec {
        text: "Right",
        x: 0.24,
        y: 0.70,
        control: None,
        key: Some(KeyCode::ArrowRight),
        playing: false,
    },
    ButtonSpec {
        text: "OK",
        x: 0.88,
        y: 0.84,
        control: None,
        key: Some(KeyCode::Enter),
        playing: false,
    },
    ButtonSpec {
        text: "Back",
        x: 0.76,
        y: 0.84,
        control: None,
        key: Some(KeyCode::Escape),
        playing: false,
    },
    ButtonSpec {
        text: "Tab",
        x: 0.88,
        y: 0.03,
        control: None,
        key: Some(KeyCode::Tab),
        playing: false,
    },
    ButtonSpec {
        text: "Page +",
        x: 0.88,
        y: 0.56,
        control: None,
        key: Some(KeyCode::PageUp),
        playing: false,
    },
    ButtonSpec {
        text: "Page -",
        x: 0.88,
        y: 0.70,
        control: None,
        key: Some(KeyCode::PageDown),
        playing: false,
    },
];
fn button_at(p: Vec2, playing: bool) -> Option<usize> {
    BUTTONS.iter().position(|b| {
        b.playing == playing && p.x >= b.x && p.x < b.x + 0.11 && p.y >= b.y && p.y < b.y + 0.12
    })
}

/// Record only real hardware events, before Bevy consumes them. Touch never injects
/// fake release messages into this stream; raw typing and rebinding stay physical.
fn capture_physical(
    mut state: ResMut<TouchState>,
    mut keys: MessageReader<KeyboardInput>,
    mut mouse: MessageReader<MouseButtonInput>,
    mut focus_lost: MessageReader<KeyboardFocusLost>,
    previous_keys: Res<ButtonInput<KeyCode>>,
    previous_mouse: Res<ButtonInput<MouseButton>>,
) {
    state.combined_keys = (*previous_keys).clone();
    state.combined_mouse = (*previous_mouse).clone();
    state.combined_keys.clear();
    state.combined_mouse.clear();
    for event in keys.read() {
        if event.state == ButtonState::Pressed {
            state.physical_keys.insert(event.key_code);
        } else {
            state.physical_keys.remove(&event.key_code);
        }
        let held =
            state.physical_keys.contains(&event.key_code) || state.keys.contains(&event.key_code);
        if held {
            state.combined_keys.press(event.key_code);
        } else {
            state.combined_keys.release(event.key_code);
        }
    }
    for event in mouse.read() {
        if event.state == ButtonState::Pressed {
            state.physical_mouse.insert(event.button);
        } else {
            state.physical_mouse.remove(&event.button);
        }
        let held =
            state.physical_mouse.contains(&event.button) || state.mouse.contains(&event.button);
        if held {
            state.combined_mouse.press(event.button);
        } else {
            state.combined_mouse.release(event.button);
        }
    }
    if focus_lost.read().next().is_some() {
        clear_physical(&mut state);
    }
}
fn clear_physical(state: &mut TouchState) {
    state.physical_keys.clear();
    state.physical_mouse.clear();
    let keys: Vec<_> = state.combined_keys.get_pressed().copied().collect();
    for key in keys {
        if !state.keys.contains(&key) {
            state.combined_keys.release(key);
        }
    }
    let mouse: Vec<_> = state.combined_mouse.get_pressed().copied().collect();
    for button in mouse {
        if !state.mouse.contains(&button) {
            state.combined_mouse.release(button);
        }
    }
}

/// Copy the complete combined state and both edges, including press+release within
/// one frame. A touch-held action masks a physical release until the touch ends.
fn aggregate<T: Copy + Eq + std::hash::Hash + Send + Sync + 'static>(
    input: &mut ButtonInput<T>,
    managed: &HashSet<T>,
    combined: &ButtonInput<T>,
) {
    for &button in managed {
        input.reset(button);
        if combined.pressed(button) || combined.just_released(button) {
            input.press(button);
        }
        if combined.just_released(button) {
            input.release(button);
        }
        if combined.pressed(button) {
            input.press(button);
        }
        if !combined.just_pressed(button) {
            input.clear_just_pressed(button);
        }
    }
}
fn merge_sources(
    state: Res<TouchState>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    aggregate(&mut keys, &state.managed_keys, &state.combined_keys);
    aggregate(&mut mouse, &state.managed_mouse, &state.combined_mouse);
}

/// Update virtual ownership after each touch event, preserving quick taps even if
/// both edges arrive in one frame. Hardware events are applied first, then touches.
fn sync_touch(state: &mut TouchState, bindings: Option<&Bindings>, pause: bool) {
    let mut wanted_keys = HashSet::new();
    if pause {
        wanted_keys.insert(KeyCode::Escape);
    }
    let mut wanted_mouse = HashSet::new();
    let mut controls = Vec::new();
    for c in state.contacts.values() {
        match c.role {
            Role::Move if state.playing => {
                let d = c.position - c.origin;
                if d.x > 0.025 {
                    controls.push(Control::SlideRight);
                }
                if d.x < -0.025 {
                    controls.push(Control::SlideLeft);
                }
                if d.y > 0.04 {
                    controls.push(Control::Back);
                }
                if d.y < -0.04 {
                    controls.push(Control::Forward);
                }
            }
            Role::Button(i) => {
                let b = BUTTONS[i];
                if b.playing != state.playing {
                    continue;
                }
                if let Some(key) = b.key {
                    wanted_keys.insert(key);
                }
                if let Some(control) = b.control {
                    controls.push(control);
                }
            }
            _ => {}
        }
    }
    if let Some(bindings) = bindings {
        for control in controls {
            let b = bindings.get(control);
            if let Some(k) = b.key {
                wanted_keys.insert(k);
            } else if let Some(m) = b.mouse {
                wanted_mouse.insert(m);
            }
        }
    }
    for &key in state.keys.symmetric_difference(&wanted_keys) {
        if wanted_keys.contains(&key) || state.physical_keys.contains(&key) {
            state.combined_keys.press(key);
        } else {
            state.combined_keys.release(key);
        }
    }
    for &button in state.mouse.symmetric_difference(&wanted_mouse) {
        if wanted_mouse.contains(&button) || state.physical_mouse.contains(&button) {
            state.combined_mouse.press(button);
        } else {
            state.combined_mouse.release(button);
        }
    }
    state.managed_keys.extend(wanted_keys.iter().copied());
    state.managed_mouse.extend(wanted_mouse.iter().copied());
    state.keys = wanted_keys;
    state.mouse = wanted_mouse;
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut state: ResMut<TouchState>,
    mut touches: MessageReader<TouchInput>,
    mut lifecycle: MessageReader<AppLifecycle>,
    mut focus: MessageReader<WindowFocused>,
    volume: Option<Res<nvf_play::audio_out::SoundVolumes>>,
    window: Query<(Entity, &Window), With<PrimaryWindow>>,
    play: Option<Res<State<nvf_play::PlayState>>>,
    next_play: Option<Res<NextState<nvf_play::PlayState>>>,
    bindings: Option<Res<Bindings>>,
    mut motion: MessageWriter<MouseMotion>,
) {
    let Ok((window_id, window)) = window.single() else {
        return;
    };
    let size = Vec2::new(window.width().max(1.0), window.height().max(1.0));
    // Bevy applies StateTransition after PreUpdate. Release old-mode contacts for
    // the pending transition now, before the new state's gameplay/UI reads input.
    let effective_play = match next_play.as_deref() {
        Some(NextState::Pending(next) | NextState::PendingIfDifferent(next)) => Some(*next),
        _ => play.map(|state| *state.get()),
    };
    let playing = effective_play == Some(nvf_play::PlayState::Playing);
    let mut just_suspended = false;
    for event in lifecycle.read() {
        match event {
            AppLifecycle::Suspended => {
                just_suspended = !state.suspended;
                state.suspended = true;
                clear_physical(&mut state);
                if let Some(volume) = &volume {
                    if state.saved_volume.is_none() {
                        state.saved_volume = Some(volume.master());
                    }
                    volume.set_master(0.0);
                }
            }
            AppLifecycle::Running => {
                state.suspended = false;
                if let (Some(saved), Some(volume)) = (state.saved_volume.take(), &volume) {
                    volume.set_master(saved);
                }
            }
            _ => {}
        }
    }
    let lost_focus = focus.read().any(|f| f.window == window_id && !f.focused);
    let suspended = state.suspended;
    if playing != state.playing || suspended || lost_focus {
        state.contacts.clear();
    }
    state.playing = playing;
    if lost_focus {
        clear_physical(&mut state);
    }
    let pause = just_suspended && playing;
    sync_touch(&mut state, bindings.as_deref(), pause);
    for t in touches.read() {
        if t.window != window_id {
            continue;
        }
        let p = t.position / size;
        match t.phase {
            TouchPhase::Started if !suspended => {
                let role = button_at(p, playing).map(Role::Button).or({
                    if !playing {
                        None
                    } else if p.x < 0.45 && p.y > 0.5 {
                        Some(Role::Move)
                    } else if p.x > 0.45 {
                        Some(Role::Look)
                    } else {
                        None
                    }
                });
                if let Some(role) = role {
                    state.contacts.insert(
                        t.id,
                        Contact {
                            origin: p,
                            position: p,
                            role,
                        },
                    );
                }
            }
            TouchPhase::Moved => {
                if let Some(c) = state.contacts.get_mut(&t.id) {
                    if matches!(c.role, Role::Look) && playing {
                        motion.write(MouseMotion {
                            delta: (p - c.position) * size,
                        });
                    }
                    c.position = p;
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                state.contacts.remove(&t.id);
            }
            _ => {}
        }
        sync_touch(&mut state, bindings.as_deref(), pause);
    }
    sync_touch(&mut state, bindings.as_deref(), pause);
}

#[derive(Component)]
struct TouchLabel(bool);
fn labels(mut commands: Commands) {
    for b in BUTTONS {
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(b.x * 100.0),
                    top: percent(b.y * 100.0),
                    width: percent(11.0),
                    height: percent(12.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.08, 0.04, 0.65)),
                GlobalZIndex(1000),
                TouchLabel(b.playing),
            ))
            .with_child((
                Text::new(b.text),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::srgb(0.7, 1.0, 0.7)),
            ));
    }
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: percent(4.0),
            bottom: percent(8.0),
            ..default()
        },
        GlobalZIndex(1000),
        TouchLabel(true),
        Text::new("Drag to move"),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
    ));
}
fn show_labels(state: Res<TouchState>, mut labels: Query<(&TouchLabel, &mut Visibility)>) {
    for (label, mut visibility) in &mut labels {
        *visibility = if label.0 == state.playing {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::{keyboard::Key, mouse::AccumulatedMouseMotion, InputPlugin};
    use bevy::state::app::StatesPlugin;

    fn scene() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputPlugin, StatesPlugin));
        app.init_state::<nvf_play::PlayState>();
        app.insert_resource(Bindings::default());
        app.add_message::<AppLifecycle>()
            .add_message::<WindowFocused>();
        app.add_plugins(TouchControlsPlugin);
        let window = app
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
        (app, window)
    }
    fn touch(app: &mut App, window: Entity, id: u64, phase: TouchPhase, x: f32, y: f32) {
        app.world_mut().write_message(TouchInput {
            phase,
            position: Vec2::new(x, y),
            window,
            force: None,
            id,
        });
    }
    #[test]
    fn touch_move_look_and_fire_work_together_then_cancel_releases() {
        let (mut app, w) = scene();
        touch(&mut app, w, 1, TouchPhase::Started, 200.0, 800.0);
        touch(&mut app, w, 2, TouchPhase::Started, 600.0, 400.0);
        touch(&mut app, w, 3, TouchPhase::Started, 920.0, 900.0);
        app.update();
        touch(&mut app, w, 1, TouchPhase::Moved, 200.0, 700.0);
        touch(&mut app, w, 2, TouchPhase::Moved, 620.0, 410.0);
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW));
        assert!(app
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left));
        assert!(
            (app.world().resource::<AccumulatedMouseMotion>().delta - Vec2::new(20.0, 10.0))
                .length()
                < 0.01
        );
        touch(&mut app, w, 1, TouchPhase::Canceled, 200.0, 700.0);
        touch(&mut app, w, 3, TouchPhase::Ended, 920.0, 900.0);
        app.update();
        assert!(!app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW));
        assert!(!app
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left));
    }
    #[test]
    fn entering_a_menu_releases_game_controls_and_uses_menu_navigation() {
        let (mut app, w) = scene();
        touch(&mut app, w, 1, TouchPhase::Started, 920.0, 900.0);
        app.update();
        app.world_mut()
            .resource_mut::<NextState<nvf_play::PlayState>>()
            .set(nvf_play::PlayState::Menu);
        app.update();
        assert!(!app
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left));
        touch(&mut app, w, 2, TouchPhase::Started, 150.0, 600.0);
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .just_pressed(KeyCode::ArrowUp));
    }
    #[test]
    fn suspension_releases_held_controls_and_requests_pause() {
        let (mut app, w) = scene();
        touch(&mut app, w, 1, TouchPhase::Started, 920.0, 900.0);
        app.update();
        app.world_mut().write_message(AppLifecycle::Suspended);
        app.update();
        assert!(!app
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left));
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .just_pressed(KeyCode::Escape));
        app.update();
        assert!(!app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::Escape));
    }
    fn physical_key(app: &mut App, window: Entity, state: ButtonState) {
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::KeyW,
            logical_key: Key::Character("w".into()),
            state,
            text: None,
            repeat: false,
            window,
        });
    }
    fn physical_mouse(app: &mut App, window: Entity, state: ButtonState) {
        app.world_mut().write_message(MouseButtonInput {
            button: MouseButton::Left,
            state,
            window,
        });
    }
    fn move_finger(app: &mut App, w: Entity) {
        touch(app, w, 1, TouchPhase::Started, 200.0, 800.0);
        touch(app, w, 1, TouchPhase::Moved, 200.0, 700.0);
    }
    #[test]
    fn lifting_movement_touch_preserves_a_physical_key_and_its_edges() {
        let (mut app, w) = scene();
        physical_key(&mut app, w, ButtonState::Pressed);
        app.update();
        move_finger(&mut app, w);
        app.update();
        touch(&mut app, w, 1, TouchPhase::Ended, 200.0, 700.0);
        app.update();
        let keys = app.world().resource::<ButtonInput<KeyCode>>();
        assert!(keys.pressed(KeyCode::KeyW));
        assert!(!keys.just_released(KeyCode::KeyW) && !keys.just_pressed(KeyCode::KeyW));
        physical_key(&mut app, w, ButtonState::Released);
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .just_released(KeyCode::KeyW));
    }
    #[test]
    fn releasing_physical_key_preserves_movement_touch_without_false_edges() {
        let (mut app, w) = scene();
        move_finger(&mut app, w);
        app.update();
        physical_key(&mut app, w, ButtonState::Pressed);
        app.update();
        physical_key(&mut app, w, ButtonState::Released);
        app.update();
        let keys = app.world().resource::<ButtonInput<KeyCode>>();
        assert!(keys.pressed(KeyCode::KeyW));
        assert!(!keys.just_released(KeyCode::KeyW) && !keys.just_pressed(KeyCode::KeyW));
        touch(&mut app, w, 1, TouchPhase::Ended, 200.0, 700.0);
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .just_released(KeyCode::KeyW));
    }
    #[test]
    fn touch_and_mouse_attack_holds_release_only_after_both_sources_release() {
        for touch_releases_first in [false, true] {
            let (mut app, w) = scene();
            physical_mouse(&mut app, w, ButtonState::Pressed);
            app.update();
            touch(&mut app, w, 1, TouchPhase::Started, 920.0, 900.0);
            app.update();
            if touch_releases_first {
                touch(&mut app, w, 1, TouchPhase::Ended, 920.0, 900.0);
            } else {
                physical_mouse(&mut app, w, ButtonState::Released);
            }
            app.update();
            let mouse = app.world().resource::<ButtonInput<MouseButton>>();
            assert!(mouse.pressed(MouseButton::Left));
            assert!(
                !mouse.just_pressed(MouseButton::Left) && !mouse.just_released(MouseButton::Left)
            );
            if touch_releases_first {
                physical_mouse(&mut app, w, ButtonState::Released);
            } else {
                touch(&mut app, w, 1, TouchPhase::Ended, 920.0, 900.0);
            }
            app.update();
            assert!(app
                .world()
                .resource::<ButtonInput<MouseButton>>()
                .just_released(MouseButton::Left));
        }
    }
    #[test]
    fn fast_physical_key_and_mouse_taps_keep_both_edges_after_touch_ownership() {
        let (mut app, w) = scene();
        move_finger(&mut app, w);
        touch(&mut app, w, 2, TouchPhase::Started, 920.0, 900.0);
        app.update();
        touch(&mut app, w, 1, TouchPhase::Ended, 200.0, 700.0);
        touch(&mut app, w, 2, TouchPhase::Ended, 920.0, 900.0);
        app.update();
        physical_key(&mut app, w, ButtonState::Pressed);
        physical_key(&mut app, w, ButtonState::Released);
        physical_mouse(&mut app, w, ButtonState::Pressed);
        physical_mouse(&mut app, w, ButtonState::Released);
        app.update();
        let keys = app.world().resource::<ButtonInput<KeyCode>>();
        assert!(!keys.pressed(KeyCode::KeyW));
        assert!(keys.just_pressed(KeyCode::KeyW) && keys.just_released(KeyCode::KeyW));
        let mouse = app.world().resource::<ButtonInput<MouseButton>>();
        assert!(!mouse.pressed(MouseButton::Left));
        assert!(mouse.just_pressed(MouseButton::Left) && mouse.just_released(MouseButton::Left));
    }
    #[test]
    fn a_touch_held_through_a_physical_tap_masks_both_physical_edges() {
        let (mut app, w) = scene();
        move_finger(&mut app, w);
        app.update();
        physical_key(&mut app, w, ButtonState::Pressed);
        physical_key(&mut app, w, ButtonState::Released);
        app.update();
        let keys = app.world().resource::<ButtonInput<KeyCode>>();
        assert!(keys.pressed(KeyCode::KeyW));
        assert!(!keys.just_pressed(KeyCode::KeyW) && !keys.just_released(KeyCode::KeyW));
    }
    #[test]
    fn touch_start_and_end_in_one_frame_preserve_tap_edges() {
        let (mut app, w) = scene();
        touch(&mut app, w, 1, TouchPhase::Started, 920.0, 900.0);
        touch(&mut app, w, 1, TouchPhase::Ended, 920.0, 900.0);
        app.update();
        let mouse = app.world().resource::<ButtonInput<MouseButton>>();
        assert!(!mouse.pressed(MouseButton::Left));
        assert!(mouse.just_pressed(MouseButton::Left) && mouse.just_released(MouseButton::Left));
    }
}
