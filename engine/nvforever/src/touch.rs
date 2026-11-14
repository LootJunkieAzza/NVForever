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

mod action_presentation;
mod aiming;
mod drawer;
mod gyro;
mod icons;
mod layout;
mod modal;
mod practice;
mod profiles;
mod movement;

mod hotbar;

use layout::{Layout, Target};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TouchInputSystems;

pub struct TouchControlsPlugin;
impl Plugin for TouchControlsPlugin {
    fn build(&self, app: &mut App) {
        drawer::install(app);
        icons::install(app);
        action_presentation::install(app);
        practice::install(app);
        gyro::install(app);
        movement::install(app);

        hotbar::install(app);

        app.init_resource::<TouchState>()
            .init_resource::<Layout>()
            .init_resource::<modal::Capture>()
            .init_resource::<nvf_play::pad::TouchDeviceSwitch>()
            .init_resource::<nvf_play::control::HeldSneakInput>()
            .init_resource::<Assets<Image>>()
            .add_systems(
                PreUpdate,
                (capture_physical, input).chain().in_set(TouchInputSystems).before(InputSystems),
            )
            .add_systems(
                PreUpdate,
                merge_sources
                    .after(InputSystems)
                    .before(nvf_play::pad::InputSwitchSystems::Switch)
                    .before(bevy::ui::UiSystems::Focus),
            )
            .add_systems(Startup, (layout::load, labels).chain())
            .add_systems(Update, show_labels);
    }
}
#[derive(Clone, Copy)]
struct Contact {
    origin: Vec2,
    position: Vec2,
    role: Role,
    started: f64,
    dragged: bool,
}
#[derive(Clone, Copy)]
enum Role {
    Move,
    Look,
    Button(usize),
    RunLock,
}
#[derive(Resource, Default)]
struct TouchState {
    contacts: HashMap<u64, Contact>,
    keys: HashSet<KeyCode>,
    mouse: HashSet<MouseButton>,
    playing: bool,
    suspended: bool,
    saved_volume: Option<f32>,
    move_deadzone: Vec2,
    aim_latched: bool,
    toggle_aim: bool,
    hold_sneak: bool,
    movement_options: movement::Options,
    look_owner: Option<u64>,
    taps: movement::Taps,
    pulses: Vec<Control>,
    viewport: Option<Vec2>,
    interactive_actions: bool,
    ui_capture: bool,
    physical_keys: HashSet<KeyCode>,
    physical_mouse: HashSet<MouseButton>,
    managed_keys: HashSet<KeyCode>,
    managed_mouse: HashSet<MouseButton>,
    combined_keys: ButtonInput<KeyCode>,
    combined_mouse: ButtonInput<MouseButton>,
}
impl TouchState {
    fn cancel_touch(&mut self) {
        self.contacts.clear();
        self.aim_latched = false;
        self.look_owner = None;
        self.taps.clear();
        self.pulses.clear();
        self.interactive_actions = false;
    }
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
    ButtonSpec {
        text: "Gyro",
        x: 0.445,
        y: 0.28,
        control: None,
        key: None,
        playing: true,
    },
];
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
    state.cancel_touch();
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
    touch_device: Option<ResMut<nvf_play::pad::TouchDeviceSwitch>>,
    layout: Res<Layout>,
    practice: Res<practice::Practice>,
) {
    if let Some(mut touch_device) = touch_device {
        touch_device.set_editor_active(layout.editor.is_some() || practice.blocking());
        touch_device.set_actions(state.keys.iter().copied(), state.mouse.iter().copied());
    }
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
            Role::Move if state.playing && !state.movement_options.analog => {
                let d = c.position - c.origin;
                if d.x > state.move_deadzone.x {
                    controls.push(Control::SlideRight);
                }
                if d.x < -state.move_deadzone.x {
                    controls.push(Control::SlideLeft);
                }
                if d.y > state.move_deadzone.y {
                    controls.push(Control::Back);
                }
                if d.y < -state.move_deadzone.y {
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
                    if (control == Control::Block && state.toggle_aim)
                        || (control == Control::CrouchSneak && state.hold_sneak)
                    {
                        continue;
                    }
                    controls.push(control);
                }
            }
            _ => {}
        }
    }
    controls.extend(state.pulses.iter().copied());
    if state.playing && state.aim_latched {
        controls.push(Control::Block);
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
    (mut state, mut held_sneak, platform, mut movement): (
        ResMut<TouchState>, ResMut<nvf_play::control::HeldSneakInput>,
        Option<Res<nvf_play::pad::PlatformInputGate>>, movement::Bridge,
    ),
    mut layout: ResMut<Layout>,
    (ads, gyro_options): (Option<Res<nvf_play::aim_zoom::AimZoom>>, Res<gyro::Options>),
    mut touches: MessageReader<TouchInput>,
    mut lifecycle: MessageReader<AppLifecycle>,
    mut focus: MessageReader<WindowFocused>,
    volume: Option<Res<nvf_play::audio_out::SoundVolumes>>,
    window: Query<(Entity, &Window), With<PrimaryWindow>>,
    play: Option<Res<State<nvf_play::PlayState>>>,
    next_play: Option<Res<NextState<nvf_play::PlayState>>>,
    bindings: Option<Res<Bindings>>,
    mut motion: MessageWriter<MouseMotion>,
    (mut drawer,mut practice,mut modal,mut bar): (drawer::Bridge,practice::Bridge,modal::Bridge,hotbar::Bridge),
) {
    let modal_captures = modal.refresh();
    let previous_move = movement.clear_sample();
    state.pulses.clear(); state.interactive_actions = false; state.ui_capture = false;
    let options = movement.options();
    if state.movement_options != options { state.cancel_touch(); state.movement_options = options; }

    let Ok((window_id, window)) = window.single() else {
        bar.cancel();
        practice.cancel();
        drawer.drawer.cancel();
        touches.clear();
        state.cancel_touch();
        held_sneak.active = false;
        sync_touch(&mut state, bindings.as_deref(), false);
        return;
    };
    let size = Vec2::new(window.width().max(1.0), window.height().max(1.0));
    let resized = state.viewport.is_some_and(|previous| previous != size);
    state.viewport = Some(size);
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
    let mut lost_focus = false;
    for event in focus.read().filter(|f| f.window == window_id) {
        drawer.drawer.set_focus(event.focused);
        lost_focus |= !event.focused;
    }
    let suspended = state.suspended;
    let interactive = !suspended && window.focused && !lost_focus && size.min_element() >= 64.0;
    let controller = bindings
        .as_deref()
        .is_some_and(|b| b.device == nvf_play::bindings::InputDevice::Controller);
    let drawer_allowed =
        playing && interactive && !controller && layout.editor.is_none() && drawer.allowed();
    if !drawer_allowed {
        drawer.drawer.cancel();
    }
    if playing != state.playing || !interactive || controller {
        state.cancel_touch();
    }
    state.playing = playing;
    if lost_focus {
        clear_physical(&mut state);
    }
    if state.toggle_aim != layout.config.aiming.toggle_aim
        || state.hold_sneak != layout.config.aiming.hold_sneak
    {
        state.cancel_touch();
    }
    state.toggle_aim = layout.config.aiming.toggle_aim;
    state.hold_sneak = layout.config.aiming.hold_sneak;
    state.move_deadzone = Vec2::splat(
        layout.config.diameter(Target::Stick, size) * 0.5 * layout.config.aiming.dead_zone,
    ) / size;
    let mut reserved = Vec::new();
    if options.run_lock { reserved.push((movement::fixed_lock_rect(&layout,size),0.0)); }
    if layout.config.floating {
        if let Some(contact)=state.contacts.values().find(|c|matches!(c.role,Role::Move)) {
            let origin=contact.origin*size;
            let radius=layout.config.diameter(Target::Stick,size)*0.65;
            if origin != layout.config.center(Target::Stick,size) {
                reserved.push((Rect::from_center_size(origin,Vec2::splat(radius*2.0)),radius));
            }
        }
    }
    bar.prepare(size, interactive && !controller && layout.editor.is_none() && !modal_captures
        && platform.as_ref().is_none_or(|gate|gate.allows_actions()), playing,
        &layout.config,gyro_options.enabled,&reserved);
    state.ui_capture = bar.captures_actions();
    if state.ui_capture {
        state.cancel_touch(); held_sneak.active = false; drawer.drawer.cancel();
    }
    let mut pause = just_suspended && playing;
    sync_touch(&mut state, bindings.as_deref(), pause);
    if resized {
        practice.cancel(); state.cancel_touch(); drawer.drawer.cancel(); touches.clear(); held_sneak.active = false;
        sync_touch(&mut state, bindings.as_deref(), pause); return;
    }
    if platform.as_ref().is_some_and(|gate| !gate.allows_actions()) {
        bar.cancel();
        practice.cancel();
        drawer.drawer.cancel();
        state.cancel_touch();
        held_sneak.active = false;
        sync_touch(&mut state, bindings.as_deref(), false);
        touches.clear();
        return;
    }
    if practice.capture(&mut touches, window_id, size, interactive,
        effective_play == Some(nvf_play::PlayState::Menu)) {
        bar.cancel();
        drawer.drawer.cancel();
        state.cancel_touch();
        held_sneak.active = false;
        sync_touch(&mut state, bindings.as_deref(), false);
        return;
    }
    if modal_captures {
        bar.cancel();
        drawer.drawer.cancel();
        state.cancel_touch();
        held_sneak.active = false;
        sync_touch(&mut state, bindings.as_deref(), false);
        touches.clear();
        return;
    }
    // The first touch restores the overlay/device; do not let it also fire or open
    // an editor whose pause edge is consumed by the switching frame.
    if controller {
        bar.cancel();
        drawer.drawer.cancel();
        state.cancel_touch();
        held_sneak.active = false;
        sync_touch(&mut state, bindings.as_deref(), pause);
        touches.clear();
        return;
    }
    state.interactive_actions = playing && interactive && layout.editor.is_none() && !state.ui_capture;
    let mut drawer_committed = false;
    let mut gesture_committed = false;
    for t in touches.read() {
        if t.window != window_id || platform.as_ref().is_some_and(|gate| !gate.allows_touch(t.id)) {

            continue;
        }
        if !t.position.is_finite() { state.cancel_touch(); sync_touch(&mut state, bindings.as_deref(), false); continue; }
        let p = t.position / size;
        if state.ui_capture {
            // The UI owner still receives its close/drag events; background routes do not.
            bar.event(t,false);
            state.cancel_touch(); held_sneak.active = false; drawer.drawer.cancel();
            sync_touch(&mut state,bindings.as_deref(),pause);
            continue;
        }
        if layout.editor.is_some() {
            bar.cancel();
            drawer.drawer.cancel();
            layout.event(t, size);
            state.cancel_touch();
            sync_touch(&mut state, bindings.as_deref(), pause);
            continue;
        }
        if drawer_allowed {
            let unclaimed = layout.config.hit_filtered(t.position,size,playing,false,
                |i| i != gyro::BUTTON || gyro_options.enabled).is_none();
            let (consumed, opened) = drawer.event(t, size, unclaimed);
            if consumed {
                if opened {
                    drawer.drawer.cancel();
                    state.cancel_touch();
                    held_sneak.active = false;
                }
                sync_touch(&mut state, bindings.as_deref(), pause);
                if opened {
                    // Share queued-request admission before leaving this input
                    // batch, including its projected item pointer and raw gyro.
                    state.ui_capture = drawer.opens_tray() && bar.apply_tray_request(true);
                    drawer_committed = true;
                    break;
                }
                continue;
            }
        }
        let unclaimed=layout.config.hit_filtered(t.position,size,playing,false,
            |i| i != gyro::BUTTON || gyro_options.enabled).is_none()
            && !layout::utility_rect(false,size).contains(t.position)
            && !layout::utility_rect(true,size).contains(t.position);
        let consumed = bar.event(t,unclaimed);
        if bar.captures_actions() {
            // Capture can begin on a long-press release midway through this batch.
            state.ui_capture = true;
            state.cancel_touch(); held_sneak.active = false; drawer.drawer.cancel();
            sync_touch(&mut state,bindings.as_deref(),pause);
        }
        if consumed || state.ui_capture { state.taps.clear(); continue; }
        if t.phase == TouchPhase::Started && interactive {
            if layout::utility_rect(false, size).contains(t.position) {
                layout.open(playing);
                state.cancel_touch();
                pause |= playing;
                sync_touch(&mut state, bindings.as_deref(), pause);
                continue;
            }
            if layout::utility_rect(true, size).contains(t.position) {
                layout.config.hints = !layout.config.hints;
                continue;
            }
        }
        match t.phase {
            TouchPhase::Started if interactive => {
                if state.contacts.contains_key(&t.id) { continue; }
                let role = if playing && options.run_lock && movement::lock_available(&layout, &state, size) && !bar.blocks(movement::lock_rect(&layout,&state,size)) && movement::lock_rect(&layout, &state, size).contains(t.position) {
                    Some(Role::RunLock)
                } else { match layout.config.hit_filtered(t.position,size,playing,false,
                    |i| i != gyro::BUTTON || gyro_options.enabled) {

                    Some(Target::Button(i)) => Some(Role::Button(i)),
                    Some(Target::Stick) => Some(Role::Move),
                    None if playing && layout.config.floating && layout.config.movement_area(p) => {
                        Some(Role::Move)
                    }
                    None if playing && layout.config.look_area(p) => Some(Role::Look),
                    _ => None,
                }};
                let role = role.filter(|role| {
                    !(matches!(role, Role::Look) && state.look_owner.is_some()) && !state.contacts.values().any(|c| {
                        matches!(
                            (role, c.role),
                            (Role::Move, Role::Move) | (Role::Look, Role::Look)
                        )
                    })
                });
                if let Some(role) = role {
                    let multiple = !state.contacts.is_empty();
                    if !matches!(role, Role::Look) || multiple { state.taps.clear(); }
                    if multiple { for c in state.contacts.values_mut() { if matches!(c.role, Role::Look) { c.dragged = true; } } }
                    if (matches!(role, Role::Look) || (options.fire_drag && matches!(role, Role::Button(i) if BUTTONS[i].control == Some(Control::Use))))
                        && state.look_owner.is_none() {
                        state.look_owner = Some(t.id);
                    }
                    if let Role::Button(i) = role {
                        if state.toggle_aim
                            && BUTTONS[i].control == Some(Control::Block)
                            && !controller
                        {
                            let already_down = state.contacts.values().any(|c| matches!(c.role, Role::Button(j) if BUTTONS[j].control == Some(Control::Block)));
                            if !already_down {
                                state.aim_latched = !state.aim_latched;
                            }
                        }
                    }
                    state.contacts.insert(
                        t.id,
                        Contact {
                            origin: if matches!(role, Role::Move) && !layout.config.floating {
                                layout.config.center(Target::Stick, size) / size
                            } else {
                                p
                            },
                            position: p,
                            role, started: movement.now(), dragged: multiple && matches!(role, Role::Look),
                        },
                    );
                }
            }
            TouchPhase::Moved => {
                let owns_look = state.look_owner == Some(t.id);
                if let Some(c) = state.contacts.get_mut(&t.id) {
                    c.dragged |= ((p - c.origin) * size).length() > 12.0;
                    if owns_look && playing && interactive {
                        motion.write(MouseMotion {
                            delta: (p - c.position)
                                * size
                                * layout
                                    .config
                                    .aiming
                                    .look_scale(ads.as_deref().is_some_and(|a| a.aimed)),
                        });
                    }
                    c.position = p;
                }
            }
            TouchPhase::Ended => {
                let lock = movement::lock_rect(&layout, &state, size);
                let lock_available = movement::lock_available(&layout, &state, size) && !bar.blocks(lock);
                let contact = state.contacts.remove(&t.id);
                if state.look_owner == Some(t.id) { state.look_owner = None; }
                if let Some(c) = contact {
                    let dragged = c.dragged || ((p-c.origin)*size).length() > 12.0;
                    if playing && interactive && options.run_lock && lock_available && lock.contains(t.position)
                        && ((matches!(c.role, Role::Move) && !movement.auto_move()) || (matches!(c.role, Role::RunLock) && !dragged))
                        && !state.pulses.contains(&Control::AutoMove) {
                        state.pulses.push(Control::AutoMove);
                    }
                    if playing && interactive && options.vats_taps && matches!(c.role, Role::Look) && !dragged
                        && state.contacts.is_empty() && movement.now() - c.started <= 0.25 {
                        if state.taps.tap(movement.now(), t.position) {
                            state.cancel_touch(); state.pulses.push(Control::Vats);
                            gesture_committed = true; break;
                        }
                    } else if matches!(c.role, Role::Look) { state.taps.clear(); }
                }
            }
            TouchPhase::Canceled => {
                state.contacts.remove(&t.id);
                if state.look_owner == Some(t.id) { state.look_owner = None; }
                state.taps.clear();
                state.aim_latched = false;
            }
            _ => {}
        }
        sync_touch(&mut state, bindings.as_deref(), pause);
    }
    // Drop the unread tail too: breaking the iterator alone would replay it on the
    // next frame, before or after the ordinary menu consumer applies its request.
    if drawer_committed || gesture_committed {
        touches.clear();
    }
    if options.analog && state.interactive_actions && !state.ui_capture {
        let axes = state.contacts.values().find(|c| matches!(c.role, Role::Move)).map_or(Vec2::ZERO, |c|
            movement::axes((c.position-c.origin)*size, layout.config.diameter(Target::Stick, size)*0.5, layout.config.aiming.dead_zone));
        movement.publish(axes, previous_move);
    }
    sync_touch(&mut state, bindings.as_deref(), pause);
    held_sneak.active = state.hold_sneak && playing && interactive && !controller
        && layout.editor.is_none() && !state.ui_capture && state.contacts.values().any(|c| matches!(c.role, Role::Button(i) if BUTTONS[i].control == Some(Control::CrouchSneak)));
}

#[derive(Component, Clone, Copy)]
pub(super) enum TouchVisual {
    Button(usize),
    Caption,
    Glyph,
    Stick,
    Knob,
    Gear,
    Help,
    Instructions,
    EditorPanel,
    EditorAction(usize),
    EditorMessage,
    AimingValue(bool),
}
fn labels(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    for (i, b) in BUTTONS.iter().enumerate() {
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::MAX,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.06, 0.07, 0.7)),
                BorderColor::all(Color::srgba(0.7, 0.9, 0.85, 0.7)),
                GlobalZIndex(2000),
                TouchVisual::Button(i),
            ))
            .with_children(|p| {
                let semantic = if i == gyro::BUTTON { icons::semantic_index("gyro") } else { Some(i) };
                if let Some(index) = semantic {
                    p.spawn((
                        ImageNode::default(), icons::TouchIcon(index), TouchVisual::Glyph,
                        Node { width: percent(66.0), height: percent(66.0), ..default() },
                        Pickable::IGNORE,
                    ));
                } else {
                    // Explicit readable fallback; a local action index is not a glyph ID.
                    p.spawn((ImageNode::new(images.add(Image::transparent())), TouchVisual::Glyph,
                        Node { align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() },
                        Pickable::IGNORE)).with_children(|glyph| {
                            glyph.spawn((Text::new("GYR"), TextFont { font_size: FontSize::Px(14.0), ..default() },
                                TextColor(Color::WHITE), gyro::Fallback, Pickable::IGNORE));
                        });
                }

                p.spawn((
                    Text::new(b.text),
                    TextFont {
                        font_size: FontSize::Px(12.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    Node {
                        position_type: PositionType::Absolute,
                        top: percent(102.0),
                        ..default()
                    },
                    TouchVisual::Caption,
                    Pickable::IGNORE,
                ));
            });
    }
    for visual in [TouchVisual::Stick, TouchVisual::Knob] {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                border: UiRect::all(px(2.0)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BorderColor::all(Color::srgba(0.7, 0.9, 0.85, 0.8)),
            BackgroundColor(Color::srgba(0.04, 0.1, 0.1, 0.35)),
            GlobalZIndex(2000),
            visual,
            Pickable::IGNORE,
        ));
    }
    for (visual, index) in [(TouchVisual::Gear, 18), (TouchVisual::Help, 19)] {
        commands.spawn((
            ImageNode::default(),
            icons::TouchIcon(index),
            Node {
                position_type: PositionType::Absolute,
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.06, 0.07, 0.7)),
            GlobalZIndex(2100),
            visual,
        ));
    }
    commands.spawn((Text::new("Left thumb: drag the stick to move. Right thumb: swipe empty space to look.\nHold an icon to act. ? toggles labels. The cog edits your controls."),
        TextFont {font_size:FontSize::Px(14.0),..default()},TextColor(Color::WHITE),
        Node {position_type:PositionType::Absolute,left:percent(25.0),top:percent(18.0),width:percent(50.0),padding:UiRect::all(px(8.0)),..default()},
        BackgroundColor(Color::srgba(0.01,0.02,0.03,0.8)),GlobalZIndex(2100),TouchVisual::Instructions));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        BackgroundColor(Color::srgba(0.01, 0.02, 0.03, 0.94)),
        GlobalZIndex(2900),
        TouchVisual::EditorPanel,
    ));
    for (i, label) in layout::EDIT_ACTIONS.iter().enumerate() {
        commands.spawn((
            Text::new(*label),
            TextFont {
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextColor(Color::WHITE),
            Node {
                position_type: PositionType::Absolute,
                padding: UiRect::all(px(8.0)),
                border: UiRect::all(px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgba(0.7, 0.9, 0.85, 0.6)),
            BackgroundColor(Color::srgba(0.1, 0.18, 0.2, 1.0)),
            GlobalZIndex(3000),
            TouchVisual::EditorAction(i),
        ));
    }
    for ads in [false, true] {
        commands.spawn((
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextColor(Color::WHITE),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            GlobalZIndex(3001),
            TouchVisual::AimingValue(ads),
        ));
    }
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: percent(31.0),
            top: percent(2.0),
            width: percent(32.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.01, 0.02, 0.03, 0.94)),
        GlobalZIndex(3100),
        TouchVisual::EditorMessage,
    ));
}
#[allow(clippy::type_complexity)]
fn show_labels(
    (state, layout, practice, modal, gyro_options, gyro_state): (Res<TouchState>, Res<Layout>, Res<practice::Practice>, Res<modal::Capture>, Res<gyro::Options>, Res<gyro::State>),
    control: Option<Res<nvf_play::control::PlayerControl>>,
    window: Query<&Window, With<PrimaryWindow>>,
    bindings: Option<Res<Bindings>>,
    pads: Query<&Gamepad>,
    mut visuals: Query<(
        &TouchVisual,
        &mut Node,
        &mut Visibility,
        Option<&mut BackgroundColor>,
        Option<&mut Text>,
        Option<&mut ImageNode>,
        Option<&mut BorderColor>,
        Option<&mut TextColor>,
    )>,
) {
    let Ok(window) = window.single() else { return };
    let size = Vec2::new(window.width().max(32.0), window.height().max(32.0));
    let editing = layout.editor.is_some();
    let controller = bindings
        .is_some_and(|b| b.device == nvf_play::bindings::InputDevice::Controller)
        && !pads.is_empty();
    let aiming_page = layout.editor.as_ref().is_some_and(|e| e.aiming);
    let styles_page = layout.editor.as_ref().is_some_and(|e| e.styles);
    let palette = layout.config.appearance.palette();
    let color = |c: [f32; 3], a: f32| Color::srgba(c[0], c[1], c[2], a);
    let playing = layout.editor.as_ref().map_or(state.playing, |e| e.playing);
    let movement = state
        .contacts
        .values()
        .find(|c| matches!(c.role, Role::Move));
    let stick_center = movement.map_or_else(
        || layout.config.center(Target::Stick, size),
        |c| c.origin * size,
    );
    let stick_diameter = layout.config.diameter(Target::Stick, size);
    for (visual, mut node, mut visibility, mut background, text, image, border, text_color) in
        &mut visuals
    {
        let mut rect = None;
        let visible = match *visual {
            TouchVisual::Button(i) => {
                let target = Target::Button(i);
                let d = layout.config.diameter(target, size);
                let geometry=layout.config.appearance.geometry(d);
                rect = Some(Rect::from_center_size(
                    layout.config.center(target, size),
                    Vec2::new(geometry.width,geometry.height),
                ));
                node.border_radius = BorderRadius::all(px(geometry.radius));
                let pressed = state
                    .contacts
                    .values()
                    .any(|c| matches!(c.role,Role::Button(j) if j==i))
                    || (i == gyro::BUTTON && gyro_state.active(&gyro_options))
                    || (BUTTONS[i].control == Some(Control::Block) && state.aim_latched)
                    || (BUTTONS[i].control == Some(Control::CrouchSneak)
                        && control.as_deref().is_some_and(|c| c.sneak));
                let selected = layout.editor.as_ref().is_some_and(|e| e.selected == target);
                if let Some(bg) = background.as_deref_mut() {
                    bg.0 = if pressed || selected {
                        color(palette.pressed, layout.config.opacity)
                    } else {
                        color(
                            palette.background,
                            if layout.config.buttons[i].visible {
                                layout.config.opacity * palette.fill
                            } else {
                                0.2
                            },
                        )
                    };
                }
                !aiming_page
                    && BUTTONS[i].playing == playing
                    && (editing || (layout.config.buttons[i].visible && (i != gyro::BUTTON || gyro_options.enabled)))
            }
            TouchVisual::Caption => editing || layout.config.hints,
            TouchVisual::Glyph => true,
            TouchVisual::Stick => {
                rect = Some(Rect::from_center_size(
                    stick_center,
                    Vec2::splat(stick_diameter),
                ));
                playing && !aiming_page
            }
            TouchVisual::Knob => {
                let offset = movement.map_or(Vec2::ZERO, |c| {
                    ((c.position - c.origin) * size).clamp_length_max(stick_diameter * 0.32)
                });
                rect = Some(Rect::from_center_size(
                    stick_center + offset,
                    Vec2::splat(stick_diameter * 0.35),
                ));
                playing && !aiming_page
            }
            TouchVisual::Gear | TouchVisual::Help => {
                rect = Some(layout::utility_rect(
                    matches!(visual, TouchVisual::Help),
                    size,
                ));
                !editing
            }
            TouchVisual::Instructions => {
                if let Some(mut text) = text {
                    text.0=format!("{} thumb: {}. {} thumb: swipe empty space to look.\n{}{}? toggles labels. The cog edits your controls.",
                        if layout.config.stick.center.x > 0.5 {"Right"} else {"Left"},
                        if state.movement_options.analog {"small pushes walk; larger pushes run with Always Run"} else {"drag beyond the dead zone for directional movement"},
                        if layout.config.stick.center.x > 0.5 {"Left"} else {"Right"},
                        if state.movement_options.fire_drag {"Hold Fire and drag that finger to look. "} else {"Hold an icon to act. "},
                        if state.movement_options.run_lock {"Release the movement thumb on the arrow for Auto Move; tap it to stop. "} else {""});

                }
                !editing && layout.config.hints
            }
            TouchVisual::EditorPanel => {
                rect = Some(if aiming_page {
                    aiming::editor_panel(size)
                } else {
                    let first = layout::editor_rect(0, size);
                    let last = layout::editor_rect(layout::EDIT_ACTIONS.len() - 1, size);
                    Rect::from_corners(first.min - Vec2::splat(4.0), last.max + Vec2::splat(4.0))
                });
                editing
            }
            TouchVisual::AimingValue(ads) => {
                rect = Some(aiming::value_rect(ads, size));
                if let Some(mut text) = text {
                    text.0 = format!(
                        "{} {:.0}%",
                        if ads { "ADS" } else { "Look" },
                        if ads {
                            layout.config.aiming.ads
                        } else {
                            layout.config.aiming.look
                        } * 100.0
                    );
                }
                aiming_page
            }
            TouchVisual::EditorAction(i) => {
                if (aiming_page || styles_page) && i >= 12 {
                    *visibility = Visibility::Hidden;
                    continue;
                }
                rect = Some(if aiming_page {
                    aiming::editor_rect(i, size)
                } else {
                    layout::editor_rect(i, size)
                });
                if let Some(mut text) = text {
                    text.0 = if styles_page {
                        let slot = layout.editor.as_ref().map_or(0, |e| e.slot);
                        match i {
                            0 => format!("Theme\n{}", palette.label),
                            1 => format!("Shape\n{}", layout.config.appearance.shape_label()),
                            2 => format!(
                                "Icons\n{}",
                                layout
                                    .config
                                    .appearance
                                    .family
                                    .as_deref()
                                    .unwrap_or("Global")
                            ),
                            3 => format!(
                                "Preset\n{}",
                                profiles::PRESETS
                                    .iter()
                                    .find(|p| p.0 == layout.config.preset_name)
                                    .map_or("Custom", |p| p.1)
                            ),
                            4 => format!(
                                "Slot {}\n{}",
                                slot + 1,
                                layout.config.slot_name(slot).unwrap_or("Empty").chars().take(12).collect::<String>()
                            ),
                            _ => profiles::ACTIONS.get(i).unwrap_or(&"").to_string(),
                        }
                    } else if layout.editor.as_ref().is_some_and(|e| e.aiming) {
                        match i {
                            6 => {
                                format!("Dead zone {:.0}%", layout.config.aiming.dead_zone * 100.0)
                            }
                            7 => if layout.config.aiming.toggle_aim {
                                "Aim: toggle"
                            } else {
                                "Aim: hold"
                            }
                            .into(),
                            8 => if layout.config.aiming.hold_sneak {
                                "Sneak: hold"
                            } else {
                                "Sneak: toggle"
                            }
                            .into(),
                            _ => aiming::EDIT_ACTIONS[i].into(),
                        }
                    } else {
                        match i {
                            2 => format!("Opacity {:.0}%", layout.config.opacity * 100.0),
                            3 => if layout.config.southpaw {
                                "Left handed"
                            } else {
                                "Right handed"
                            }
                            .into(),
                            4 => if layout.config.floating {
                                "Floating stick"
                            } else {
                                "Fixed stick"
                            }
                            .into(),
                            5 => if playing { "Edit menus" } else { "Edit game" }.into(),
                            _ => layout::EDIT_ACTIONS[i].into(),
                        }
                    };
                }
                editing && (!(aiming_page || styles_page) || i < 12)
            }
            TouchVisual::EditorMessage => {
                if let Some(mut text) = text {
                    text.0 = layout.editor.as_ref().map_or_else(
                        || layout.message.clone(),
                        |e| {
                            if e.styles {
                                return format!(
                                    "Appearance and profiles · slot {}\n{}",
                                    e.slot + 1,
                                    layout.message
                                );
                            }
                            if e.aiming {
                                return format!(
                                    "Touch aiming · look {:.0}% · ADS {:.0}%\n{}",
                                    layout.config.aiming.look * 100.0,
                                    layout.config.aiming.ads * 100.0,
                                    layout.message
                                );
                            }
                            format!(
                                "{} · {}\n{}",
                                e.selected.name(),
                                if playing {
                                    "Game controls"
                                } else {
                                    "Menu controls"
                                },
                                layout.message
                            )
                        },
                    );
                }
                editing
            }
        };
        // Opacity belongs to the entire control, not just its dark backing.
        if matches!(
            visual,
            TouchVisual::Button(_)
                | TouchVisual::Caption
                | TouchVisual::Glyph
                | TouchVisual::Stick
                | TouchVisual::Knob
        ) {
            let alpha = layout.config.opacity;
            if let Some(mut image) = image {
                image.color = color(palette.foreground, alpha);
            }
            if let Some(mut border) = border {
                *border = BorderColor::all(color(palette.border, alpha));
            }
            if let Some(mut color) = text_color {
                color.0 = Color::srgba(
                    palette.foreground[0],
                    palette.foreground[1],
                    palette.foreground[2],
                    alpha,
                );
            }
            if matches!(visual, TouchVisual::Stick | TouchVisual::Knob) {
                if let Some(bg) = background.as_deref_mut() {
                    bg.0 = color(
                        palette.background,
                        alpha
                            * if matches!(visual, TouchVisual::Knob) {
                                0.8
                            } else {
                                0.35
                            },
                    );
                }
            }
        }
        *visibility =
            if visible && !state.ui_capture && !practice.blocking() && !modal.blocking() && (!controller || editing) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        if let Some(rect) = rect {
            node.left = px(rect.min.x);
            node.top = px(rect.min.y);
            node.width = px(rect.width());
            node.height = px(rect.height());
        }
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
        app.insert_resource(movement::Options::legacy());
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
    fn controls_scene() -> (App, Entity) {
        let (mut app, window) = scene();
        app.init_resource::<nvf_play::control::PlayerControl>()
            .add_systems(Update, nvf_play::control::read_controls);
        app.update();
        (app, window)
    }
    fn button_touch(app: &mut App, window: Entity, id: u64, phase: TouchPhase, control: Control) {
        let index = BUTTONS
            .iter()
            .position(|b| b.control == Some(control))
            .unwrap();
        let center = app
            .world()
            .resource::<Layout>()
            .config
            .center(Target::Button(index), Vec2::splat(1000.0));
        touch(app, window, id, phase, center.x, center.y);
    }
    fn tap_control(app: &mut App, window: Entity, id: u64, control: Control) {
        button_touch(app, window, id, TouchPhase::Started, control);
        button_touch(app, window, id, TouchPhase::Ended, control);
        app.update();
    }
    fn raw_key(app: &mut App, window: Entity, key: KeyCode, state: ButtonState) {
        app.world_mut().write_message(KeyboardInput {
            key_code: key,
            logical_key: Key::Character("owned input fixture".into()),
            state,
            text: None,
            repeat: false,
            window,
        });
    }
    fn aim_requested(app: &App) -> bool {
        app.world()
            .resource::<nvf_play::control::PlayerControl>()
            .aiming
    }
    fn sneak_requested(app: &App) -> bool {
        app.world()
            .resource::<nvf_play::control::PlayerControl>()
            .sneak
    }
    #[test]
    fn controller_takeover_cancels_touch_latches_and_first_touch_returns_without_actions() {
        use bevy::input::gamepad::{RawGamepadButtonChangedEvent, RawGamepadEvent};
        use nvf_play::bindings::InputDevice;
        let pad_edge = |app: &mut App, pad, down| {
            app.world_mut().write_message(RawGamepadEvent::Button(
                RawGamepadButtonChangedEvent::new(
                    pad,
                    GamepadButton::South,
                    if down { 1.0 } else { 0.0 },
                ),
            ));
        };
        for native_sneak in [false, true] {
            let (mut app, w) = controls_scene();
            app.add_plugins(nvf_play::pad::InputSwitchPlugin);
            let pad = app.world_mut().spawn(Gamepad::default()).id();
            {
                let mut layout = app.world_mut().resource_mut::<Layout>();
                layout.config.aiming.toggle_aim = true;
                layout.config.aiming.hold_sneak = true;
            }
            if native_sneak {
                let key = app
                    .world()
                    .resource::<Bindings>()
                    .get(Control::CrouchSneak)
                    .key
                    .unwrap();
                raw_key(&mut app, w, key, ButtonState::Pressed);
                app.update();
                raw_key(&mut app, w, key, ButtonState::Released);
                app.update();
            }
            app.update();
            assert_eq!(sneak_requested(&app), native_sneak);
            tap_control(&mut app, w, 1, Control::Block);
            app.update(); // The finger is gone; its aim latch must not block takeover.
            assert!(aim_requested(&app));
            pad_edge(&mut app, pad, true);
            app.update();
            assert_eq!(
                app.world().resource::<Bindings>().device,
                InputDevice::Controller,
                "accepted controller activity takes over a released touch aim latch"
            );
            assert!(!aim_requested(&app));
            pad_edge(&mut app, pad, false);
            app.update();
            app.update();
            assert!(!app.world().resource::<TouchState>().aim_latched);
            tap_control(&mut app, w, 2, Control::Block);
            assert_eq!(
                app.world().resource::<Bindings>().device,
                InputDevice::KeyboardMouse
            );
            assert!(!app
                .world()
                .resource::<nvf_play::pad::TouchDeviceSwitch>()
                .allows_actions());
            assert!(!aim_requested(&app) && !app.world().resource::<TouchState>().aim_latched);
            assert_eq!(sneak_requested(&app), native_sneak);
            tap_control(&mut app, w, 3, Control::Block);
            assert!(aim_requested(&app), "a subsequent aim tap works");
            tap_control(&mut app, w, 4, Control::Block);
            button_touch(&mut app, w, 5, TouchPhase::Started, Control::CrouchSneak);
            app.update();
            assert!(
                app.world()
                    .resource::<nvf_play::control::HeldSneakInput>()
                    .active
            );
            assert!(sneak_requested(&app));
            // A live touch keeps the automatic selector busy; an explicit choice
            // must still clear that touch source while preserving the native base.
            nvf_play::bindings::set_device(app.world_mut(), InputDevice::Controller);
            app.update();
            assert!(
                !app.world()
                    .resource::<nvf_play::control::HeldSneakInput>()
                    .active
            );
            assert!(app.world().resource::<TouchState>().contacts.is_empty());
            assert_eq!(sneak_requested(&app), native_sneak);
            button_touch(&mut app, w, 5, TouchPhase::Ended, Control::CrouchSneak);
            app.update();
            tap_control(&mut app, w, 6, Control::CrouchSneak);
            assert_eq!(
                app.world().resource::<Bindings>().device,
                InputDevice::KeyboardMouse
            );
            assert!(
                !app.world()
                    .resource::<nvf_play::control::HeldSneakInput>()
                    .active
            );
            assert_eq!(
                sneak_requested(&app),
                native_sneak,
                "first return tap cannot buffer sneak"
            );
            button_touch(&mut app, w, 7, TouchPhase::Started, Control::CrouchSneak);
            app.update();
            assert!(sneak_requested(&app), "a fresh hold works");
            button_touch(&mut app, w, 7, TouchPhase::Ended, Control::CrouchSneak);
            app.update();
            assert_eq!(sneak_requested(&app), native_sneak);
            nvf_play::bindings::set_device(app.world_mut(), InputDevice::Controller);
            app.update();
            let gear = layout::utility_rect(false, Vec2::splat(1000.0)).center();
            touch(&mut app, w, 8, TouchPhase::Started, gear.x, gear.y);
            touch(&mut app, w, 8, TouchPhase::Ended, gear.x, gear.y);
            app.update();
            assert!(
                app.world().resource::<Layout>().editor.is_none(),
                "return tap cannot open an unpaused editor"
            );
            assert!(!app
                .world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::Escape));
            touch(&mut app, w, 9, TouchPhase::Started, gear.x, gear.y);
            touch(&mut app, w, 9, TouchPhase::Ended, gear.x, gear.y);
            app.update();
            assert!(
                app.world().resource::<Layout>().editor.is_some(),
                "the subsequent cog tap opens editing"
            );
        }
    }
    fn drawer_visibility(app: &mut App) -> Visibility {
        let world = app.world_mut();
        let mut query = world.query_filtered::<&Visibility, With<drawer::Handle>>();
        *query.single(world).unwrap()
    }
    #[test]
    fn explicit_controller_takeover_cancels_drawer_and_first_swipe_only_restores_overlay() {
        use bevy::input::gamepad::{RawGamepadButtonChangedEvent, RawGamepadEvent};
        use nvf_play::bindings::InputDevice;
        let (mut app, w) = controls_scene();
        app.add_plugins(nvf_play::pad::InputSwitchPlugin);
        let pad = app.world_mut().spawn(Gamepad::default()).id();
        app.world_mut().resource_mut::<drawer::Options>().enabled = true;
        app.update();
        let start = drawer::handle(Vec2::splat(1000.0), 24.0).center();
        touch(&mut app, w, 10, TouchPhase::Started, start.x, start.y);
        app.update();
        app.world_mut()
            .write_message(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(
                pad,
                GamepadButton::South,
                1.0,
            )));
        app.update();
        assert_eq!(
            app.world().resource::<Bindings>().device,
            InputDevice::KeyboardMouse,
            "an owned contact retains the existing old-device-busy policy"
        );
        app.world_mut()
            .write_message(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(
                pad,
                GamepadButton::South,
                0.0,
            )));
        app.update();
        nvf_play::bindings::set_device(app.world_mut(), InputDevice::Controller);
        app.update();
        assert_eq!(drawer_visibility(&mut app), Visibility::Hidden);
        touch(&mut app, w, 10, TouchPhase::Ended, start.x, start.y - 140.0);
        app.update();
        assert_eq!(
            app.world()
                .resource::<Messages<nvf_play::menu::OpenInventory>>()
                .len(),
            0,
            "old release cannot open inventory after takeover"
        );
        for (phase, y) in [
            (TouchPhase::Started, start.y),
            (TouchPhase::Moved, start.y - 140.0),
            (TouchPhase::Ended, start.y - 140.0),
        ] {
            touch(&mut app, w, 11, phase, start.x, y);
        }
        app.update();
        assert_eq!(
            app.world().resource::<Bindings>().device,
            InputDevice::KeyboardMouse
        );
        assert!(!app
            .world()
            .resource::<nvf_play::pad::TouchDeviceSwitch>()
            .allows_actions());
        assert!(!app.world().resource::<TouchState>().interactive_actions,
            "the switching touch is consumed even while the overlay is restored");
        assert_eq!(drawer_visibility(&mut app), Visibility::Inherited);
        assert_eq!(
            app.world()
                .resource::<Messages<nvf_play::menu::OpenInventory>>()
                .len(),
            0,
            "first return swipe is consumed in full"
        );
        for (phase, y) in [
            (TouchPhase::Started, start.y),
            (TouchPhase::Moved, start.y - 140.0),
            (TouchPhase::Ended, start.y - 140.0),
        ] {
            touch(&mut app, w, 12, phase, start.x, y);
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<Messages<nvf_play::menu::OpenInventory>>()
                .len(),
            1,
            "a fresh owned swipe reaches the real shared request route once"
        );
    }
    #[test]
    fn drawer_commit_releases_touch_aim_and_hold_sneak_without_erasing_native_base() {
        for native_sneak in [false, true] {
            let (mut app, w) = controls_scene();
            app.world_mut().resource_mut::<drawer::Options>().enabled = true;
            {
                let mut layout = app.world_mut().resource_mut::<Layout>();
                layout.config.aiming.toggle_aim = true;
                layout.config.aiming.hold_sneak = true;
            }
            if native_sneak {
                let key = app
                    .world()
                    .resource::<Bindings>()
                    .get(Control::CrouchSneak)
                    .key
                    .unwrap();
                raw_key(&mut app, w, key, ButtonState::Pressed);
                app.update();
                raw_key(&mut app, w, key, ButtonState::Released);
                app.update();
            }
            tap_control(&mut app, w, 1, Control::Block);
            assert!(aim_requested(&app));
            button_touch(&mut app, w, 2, TouchPhase::Started, Control::CrouchSneak);
            app.update();
            assert!(
                app.world()
                    .resource::<nvf_play::control::HeldSneakInput>()
                    .active
                    && sneak_requested(&app)
            );
            let start = drawer::handle(Vec2::splat(1000.0), 24.0).center();
            for (phase, y) in [
                (TouchPhase::Started, start.y),
                (TouchPhase::Moved, start.y - 140.0),
                (TouchPhase::Ended, start.y - 140.0),
            ] {
                touch(&mut app, w, 10, phase, start.x, y);
                app.update();
            }
            assert_eq!(
                app.world()
                    .resource::<Messages<nvf_play::menu::OpenInventory>>()
                    .len(),
                1
            );
            assert!(!app.world().resource::<TouchState>().aim_latched && !aim_requested(&app));
            assert!(
                !app.world()
                    .resource::<nvf_play::control::HeldSneakInput>()
                    .active
            );
            assert_eq!(sneak_requested(&app), native_sneak);
        }
    }
    #[test]
    fn drawer_commit_drops_later_gameplay_starts_in_the_same_batch_and_keeps_physical_owners() {
        for action in [Control::Use, Control::Block, Control::CrouchSneak] {
            let (mut app, w) = controls_scene();
            app.world_mut().resource_mut::<drawer::Options>().enabled = true;
            {
                let mut layout = app.world_mut().resource_mut::<Layout>();
                layout.config.aiming.toggle_aim = true;
                layout.config.aiming.hold_sneak = true;
            }
            raw_key(&mut app, w, KeyCode::KeyW, ButtonState::Pressed);
            app.world_mut().write_message(MouseButtonInput {
                button: MouseButton::Middle,
                state: ButtonState::Pressed,
                window: w,
            });
            app.update();
            let start = drawer::handle(Vec2::splat(1000.0), 24.0).center();
            touch(&mut app, w, 10, TouchPhase::Started, start.x, start.y);
            app.update();
            touch(&mut app, w, 10, TouchPhase::Moved, start.x, start.y - 140.0);
            touch(&mut app, w, 10, TouchPhase::Ended, start.x, start.y - 140.0);
            button_touch(&mut app, w, 20, TouchPhase::Started, action);
            app.update();
            assert_eq!(
                app.world()
                    .resource::<Messages<nvf_play::menu::OpenInventory>>()
                    .len(),
                1
            );
            for _ in 0..2 {
                let state = app.world().resource::<TouchState>();
                assert!(
                    state.contacts.is_empty() && !state.aim_latched,
                    "post-commit {action:?} cannot own a gameplay contact"
                );
                assert!(
                    state.physical_keys.contains(&KeyCode::KeyW)
                        && state.physical_mouse.contains(&MouseButton::Middle)
                );
                assert!(!app
                    .world()
                    .resource::<ButtonInput<MouseButton>>()
                    .pressed(MouseButton::Left));
                assert!(
                    !aim_requested(&app)
                        && !app
                            .world()
                            .resource::<nvf_play::control::HeldSneakInput>()
                            .active
                );
                assert!(app
                    .world()
                    .resource::<ButtonInput<KeyCode>>()
                    .pressed(KeyCode::KeyW));
                assert!(app
                    .world()
                    .resource::<ButtonInput<MouseButton>>()
                    .pressed(MouseButton::Middle));
                app.update(); // An unread tail must not replay next frame either.
            }
        }
    }
    #[test]
    fn touch_look_uses_actual_ads_state_and_does_not_rescale_hardware_motion() {
        let (mut app, w) = controls_scene();
        app.init_resource::<nvf_play::aim_zoom::AimZoom>();
        {
            let mut l = app.world_mut().resource_mut::<Layout>();
            l.config.aiming.look = 2.0;
            l.config.aiming.ads = 0.25;
        }
        touch(&mut app, w, 1, TouchPhase::Started, 600.0, 400.0);
        button_touch(&mut app, w, 2, TouchPhase::Started, Control::Block);
        app.update();
        assert!(aim_requested(&app));
        assert!(
            !app.world().resource::<nvf_play::aim_zoom::AimZoom>().aimed,
            "a held aim binding alone does not establish ADS"
        );
        touch(&mut app, w, 1, TouchPhase::Moved, 620.0, 410.0);
        app.world_mut().write_message(MouseMotion {
            delta: Vec2::new(3.0, -2.0),
        });
        app.update();
        assert!(
            (app.world().resource::<AccumulatedMouseMotion>().delta - Vec2::new(43.0, 18.0))
                .length()
                < 0.001
        );
        app.world_mut()
            .resource_mut::<nvf_play::aim_zoom::AimZoom>()
            .aimed = true;
        touch(&mut app, w, 1, TouchPhase::Moved, 640.0, 420.0);
        app.world_mut().write_message(MouseMotion {
            delta: Vec2::new(3.0, -2.0),
        });
        app.update();
        assert!(
            (app.world().resource::<AccumulatedMouseMotion>().delta - Vec2::new(8.0, 0.5)).length()
                < 0.001
        );
    }
    #[test]
    fn toggle_aim_taps_release_only_the_touch_key_owner() {
        let (mut app, w) = controls_scene();
        app.world_mut()
            .resource_mut::<Layout>()
            .config
            .aiming
            .toggle_aim = true;
        tap_control(&mut app, w, 1, Control::Block);
        assert!(aim_requested(&app));
        let key = app
            .world()
            .resource::<Bindings>()
            .get(Control::Block)
            .key
            .unwrap();
        raw_key(&mut app, w, key, ButtonState::Pressed);
        app.update();
        tap_control(&mut app, w, 2, Control::Block);
        assert!(!app.world().resource::<TouchState>().aim_latched);
        assert!(aim_requested(&app), "the physical owner still holds Block");
        raw_key(&mut app, w, key, ButtonState::Released);
        app.update();
        assert!(!aim_requested(&app));
    }
    #[test]
    fn latched_aim_clears_on_cancel_menu_focus_suspend_and_editor() {
        for case in 0..6 {
            let (mut app, w) = controls_scene();
            app.world_mut()
                .resource_mut::<Layout>()
                .config
                .aiming
                .toggle_aim = true;
            tap_control(&mut app, w, 1, Control::Block);
            assert!(aim_requested(&app));
            match case {
                0 => touch(&mut app, w, 2, TouchPhase::Canceled, 600.0, 400.0),
                1 => app
                    .world_mut()
                    .resource_mut::<NextState<nvf_play::PlayState>>()
                    .set(nvf_play::PlayState::Menu),
                2 => {
                    app.world_mut().write_message(WindowFocused {
                        window: w,
                        focused: false,
                    });
                    button_touch(&mut app, w, 3, TouchPhase::Started, Control::Block);
                }
                3 => {
                    app.world_mut().write_message(AppLifecycle::Suspended);
                }
                4 => touch(&mut app, w, 2, TouchPhase::Started, 500.0, 75.0),
                _ => {
                    app.world_mut().write_message(KeyboardFocusLost);
                }
            }
            app.update();
            assert!(!aim_requested(&app), "cleanup case {case}");
            assert!(!app.world().resource::<TouchState>().aim_latched);
        }
    }
    #[test]
    fn hold_sneak_preserves_keyboard_toggle_base_and_restores_on_lift() {
        let (mut app, w) = controls_scene();
        app.world_mut()
            .resource_mut::<Layout>()
            .config
            .aiming
            .hold_sneak = true;
        let key = app
            .world()
            .resource::<Bindings>()
            .get(Control::CrouchSneak)
            .key
            .unwrap();
        button_touch(&mut app, w, 1, TouchPhase::Started, Control::CrouchSneak);
        app.update();
        assert!(sneak_requested(&app));
        assert!(
            !app.world().resource::<ButtonInput<KeyCode>>().pressed(key),
            "touch hold is not a fabricated toggle press"
        );
        raw_key(&mut app, w, key, ButtonState::Pressed);
        app.update();
        raw_key(&mut app, w, key, ButtonState::Released);
        app.update();
        button_touch(&mut app, w, 1, TouchPhase::Ended, Control::CrouchSneak);
        app.update();
        assert!(
            sneak_requested(&app),
            "the independent keyboard toggle survives touch release"
        );
        raw_key(&mut app, w, key, ButtonState::Pressed);
        app.update();
        raw_key(&mut app, w, key, ButtonState::Released);
        app.update();
        assert!(!sneak_requested(&app));
        button_touch(&mut app, w, 2, TouchPhase::Started, Control::CrouchSneak);
        app.update();
        assert!(sneak_requested(&app));
        button_touch(&mut app, w, 2, TouchPhase::Ended, Control::CrouchSneak);
        app.update();
        assert!(!sneak_requested(&app));
    }
    #[test]
    fn hold_sneak_fast_tap_cancellation_and_input_gates_do_not_toggle_the_base() {
        let (mut app, w) = controls_scene();
        app.world_mut()
            .resource_mut::<Layout>()
            .config
            .aiming
            .hold_sneak = true;
        tap_control(&mut app, w, 1, Control::CrouchSneak);
        assert!(!sneak_requested(&app));
        button_touch(&mut app, w, 2, TouchPhase::Started, Control::CrouchSneak);
        app.update();
        assert!(sneak_requested(&app));
        app.world_mut().write_message(AppLifecycle::Suspended);
        app.update();
        assert!(!sneak_requested(&app));
        app.world_mut().write_message(AppLifecycle::Running);
        app.update();
        app.insert_resource(nvf_play::bindings::KeyboardCapture(true));
        button_touch(&mut app, w, 3, TouchPhase::Started, Control::CrouchSneak);
        app.update();
        assert!(!sneak_requested(&app));
        app.world_mut()
            .remove_resource::<nvf_play::bindings::KeyboardCapture>();
        app.update();
        assert!(sneak_requested(&app));
        app.world_mut().resource_mut::<Bindings>().device =
            nvf_play::bindings::InputDevice::Controller;
        app.update();
        assert!(!sneak_requested(&app));
        assert!(
            !app.world()
                .resource::<nvf_play::control::HeldSneakInput>()
                .active
        );
    }
    #[test]
    fn touch_joystick_uses_the_configured_dead_zone_in_the_event_route() {
        let (mut app, w) = scene();
        app.world_mut()
            .resource_mut::<Layout>()
            .config
            .aiming
            .dead_zone = 0.35;
        touch(&mut app, w, 1, TouchPhase::Started, 200.0, 800.0);
        touch(&mut app, w, 1, TouchPhase::Moved, 200.0, 760.0);
        app.update();
        assert!(!app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW));
        touch(&mut app, w, 1, TouchPhase::Moved, 200.0, 740.0);
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW));
        app.world_mut()
            .resource_mut::<Layout>()
            .config
            .aiming
            .dead_zone = 0.5;
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .just_released(KeyCode::KeyW));
    }
    #[test]
    fn aiming_editor_settings_cancel_with_the_layout() {
        let (mut app, w) = scene();
        let original = app.world().resource::<Layout>().config.clone();
        touch(&mut app, w, 1, TouchPhase::Started, 500.0, 75.0);
        app.update();
        for action in [11, 1, 7, 8, 6] {
            let size = Vec2::splat(1000.0);
            let p = if app
                .world()
                .resource::<Layout>()
                .editor
                .as_ref()
                .unwrap()
                .aiming
            {
                aiming::editor_rect(action, size)
            } else {
                layout::editor_rect(action, size)
            }
            .center();
            touch(&mut app, w, 2, TouchPhase::Started, p.x, p.y);
            touch(&mut app, w, 2, TouchPhase::Ended, p.x, p.y);
            app.update();
        }
        let config = &app.world().resource::<Layout>().config;
        assert!(config.aiming.look > 1.0 && config.aiming.toggle_aim && config.aiming.hold_sneak);
        let p = aiming::editor_rect(10, Vec2::splat(1000.0)).center();
        touch(&mut app, w, 3, TouchPhase::Started, p.x, p.y);
        app.update();
        assert_eq!(app.world().resource::<Layout>().config, original);
    }
    #[test]
    fn toggle_aim_touch_from_controller_selection_does_not_buffer_a_latch() {
        let (mut app, w) = controls_scene();
        app.world_mut()
            .resource_mut::<Layout>()
            .config
            .aiming
            .toggle_aim = true;
        app.world_mut().resource_mut::<Bindings>().device =
            nvf_play::bindings::InputDevice::Controller;
        tap_control(&mut app, w, 1, Control::Block);
        assert!(!aim_requested(&app));
        app.world_mut().resource_mut::<Bindings>().device =
            nvf_play::bindings::InputDevice::KeyboardMouse;
        app.update();
        assert!(!aim_requested(&app));
        tap_control(&mut app, w, 2, Control::Block);
        assert!(aim_requested(&app));
    }
    #[test]
    fn drawer_release_requests_the_real_inventory_route_without_fire_or_look() {
        let (mut app, w) = scene();
        app.world_mut().resource_mut::<drawer::Options>().enabled = true;
        for (phase, y) in [
            (TouchPhase::Started, 940.0),
            (TouchPhase::Moved, 820.0),
            (TouchPhase::Ended, 820.0),
        ] {
            touch(&mut app, w, 1, phase, 530.0, y);
            app.update();
            assert!(!app
                .world()
                .resource::<ButtonInput<MouseButton>>()
                .pressed(MouseButton::Left));
            assert_eq!(
                app.world().resource::<AccumulatedMouseMotion>().delta,
                Vec2::ZERO
            );
        }
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<nvf_play::menu::OpenInventory>>()
                .drain()
                .count(),
            1
        );
        touch(&mut app, w, 1, TouchPhase::Ended, 530.0, 820.0);
        app.update();
        assert_eq!(
            app.world()
                .resource::<Messages<nvf_play::menu::OpenInventory>>()
                .len(),
            0
        );
    }
    #[test]
    fn drawer_never_steals_an_edited_fire_button_hit_target() {
        let (mut app, w) = scene();
        app.world_mut().resource_mut::<drawer::Options>().enabled = true;
        let fire = BUTTONS
            .iter()
            .position(|b| b.control == Some(Control::Use))
            .unwrap();
        app.world_mut().resource_mut::<Layout>().config.buttons[fire].center =
            Vec2::new(0.53, 0.94);
        touch(&mut app, w, 1, TouchPhase::Started, 530.0, 940.0);
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left));
        touch(&mut app, w, 1, TouchPhase::Ended, 530.0, 800.0);
        app.update();
        assert_eq!(
            app.world()
                .resource::<Messages<nvf_play::menu::OpenInventory>>()
                .len(),
            0
        );
    }
    #[test]
    fn drawer_cancels_on_pending_menu_suspend_focus_text_capture_and_editor() {
        for scenario in 0..5 {
            let (mut app, w) = scene();
            app.world_mut().resource_mut::<drawer::Options>().enabled = true;
            touch(&mut app, w, 1, TouchPhase::Started, 530.0, 940.0);
            app.update();
            match scenario {
                0 => app
                    .world_mut()
                    .resource_mut::<NextState<nvf_play::PlayState>>()
                    .set(nvf_play::PlayState::Menu),
                1 => {
                    app.world_mut().write_message(AppLifecycle::Suspended);
                }
                2 => {
                    app.world_mut().write_message(WindowFocused {
                        window: w,
                        focused: false,
                    });
                }
                3 => {
                    app.world_mut()
                        .insert_resource(nvf_play::bindings::KeyboardCapture(true));
                }
                _ => app.world_mut().resource_mut::<Layout>().open(true),
            }
            touch(&mut app, w, 1, TouchPhase::Ended, 530.0, 800.0);
            app.update();
            assert_eq!(
                app.world()
                    .resource::<Messages<nvf_play::menu::OpenInventory>>()
                    .len(),
                0,
                "scenario {scenario}"
            );
        }
    }
    #[test]
    fn drawer_remembers_focus_loss_until_an_explicit_gain() {
        let (mut app, w) = scene();
        app.world_mut().resource_mut::<drawer::Options>().enabled = true;
        app.world_mut().write_message(WindowFocused {
            window: w,
            focused: false,
        });
        app.update();
        for (phase, y) in [(TouchPhase::Started, 940.0), (TouchPhase::Ended, 800.0)] {
            touch(&mut app, w, 1, phase, 530.0, y);
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<Messages<nvf_play::menu::OpenInventory>>()
                .len(),
            0
        );
        app.world_mut().write_message(WindowFocused {
            window: w,
            focused: true,
        });
        app.update();
        for (phase, y) in [(TouchPhase::Started, 940.0), (TouchPhase::Ended, 800.0)] {
            touch(&mut app, w, 2, phase, 530.0, y);
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<Messages<nvf_play::menu::OpenInventory>>()
                .len(),
            1
        );
    }
    #[test]
    fn editing_releases_attack_pauses_and_cancel_restores_dragged_layout() {
        let (mut app, w) = scene();
        touch(&mut app, w, 1, TouchPhase::Started, 920.0, 900.0);
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left));
        touch(&mut app, w, 2, TouchPhase::Started, 500.0, 75.0);
        app.update();
        assert!(app.world().resource::<Layout>().editor.is_some());
        assert!(!app
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left));
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .just_pressed(KeyCode::Escape));
        let original = app.world().resource::<Layout>().config.clone();
        touch(&mut app, w, 3, TouchPhase::Started, 180.0, 760.0);
        app.update();
        touch(&mut app, w, 3, TouchPhase::Moved, 240.0, 820.0);
        app.update();
        assert_ne!(
            app.world().resource::<Layout>().config.stick.center,
            original.stick.center
        );
        assert!(!app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyS));
        touch(&mut app, w, 3, TouchPhase::Ended, 240.0, 820.0);
        let cancel = layout::editor_rect(10, Vec2::splat(1000.0)).center();
        touch(&mut app, w, 4, TouchPhase::Started, cancel.x, cancel.y);
        app.update();
        let layout = app.world().resource::<Layout>();
        assert!(layout.editor.is_none());
        assert_eq!(layout.config, original);
    }
    #[test]
    fn a_second_finger_changing_editor_mode_cancels_the_first_drag() {
        let (mut app, w) = scene();
        touch(&mut app, w, 1, TouchPhase::Started, 500.0, 75.0);
        app.update();
        let size = Vec2::splat(1000.0);
        let fire = app
            .world()
            .resource::<Layout>()
            .config
            .center(Target::Button(7), size);
        touch(&mut app, w, 2, TouchPhase::Started, fire.x, fire.y);
        app.update();
        let before = app.world().resource::<Layout>().config.stick.clone();
        let mode = layout::editor_rect(5, size).center();
        touch(&mut app, w, 3, TouchPhase::Started, mode.x, mode.y);
        app.update();
        touch(&mut app, w, 2, TouchPhase::Moved, 600.0, 700.0);
        app.update();
        assert_eq!(app.world().resource::<Layout>().config.stick, before);
    }
    #[test]
    fn opacity_applies_to_icons_borders_and_labels_not_only_backing() {
        let (mut app, _) = scene();
        app.world_mut().resource_mut::<Layout>().config.opacity = 0.35;
        app.update();
        let world = app.world_mut();
        let mut q = world.query::<(
            &TouchVisual,
            Option<&ImageNode>,
            Option<&BorderColor>,
            Option<&TextColor>,
        )>();
        let mut icons = 0;
        for (visual, image, border, text) in q.iter(world) {
            if matches!(visual, TouchVisual::Glyph) {
                assert!((image.unwrap().color.to_srgba().alpha - 0.35).abs() < 0.001);
                icons += 1;
            }
            if matches!(visual, TouchVisual::Button(_)) {
                assert!((border.unwrap().top.to_srgba().alpha - 0.35).abs() < 0.001);
            }
            if matches!(visual, TouchVisual::Caption) {
                assert!((text.unwrap().0.to_srgba().alpha - 0.35).abs() < 0.001);
            }
        }
        assert_eq!(icons, BUTTONS.len());
    }
    #[test]
    fn a_repositioned_floating_stick_still_moves_at_its_visible_anchor() {
        let (mut app, w) = scene();
        app.world_mut().resource_mut::<Layout>().config.stick.center = Vec2::new(0.60, 0.76);
        touch(&mut app, w, 1, TouchPhase::Started, 600.0, 760.0);
        app.update();
        touch(&mut app, w, 1, TouchPhase::Moved, 600.0, 660.0);
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW));
        assert_eq!(
            app.world().resource::<AccumulatedMouseMotion>().delta,
            Vec2::ZERO
        );
    }
    #[test]
    fn southpaw_fixed_stick_uses_its_visible_center_and_releases_on_lift() {
        let (mut app, w) = scene();
        {
            let mut l = app.world_mut().resource_mut::<Layout>();
            l.config = layout::Config::preset(true);
            l.config.floating = false;
        }
        let center = app
            .world()
            .resource::<Layout>()
            .config
            .center(Target::Stick, Vec2::splat(1000.0));
        touch(&mut app, w, 1, TouchPhase::Started, center.x, center.y);
        app.update();
        assert!(!app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW));
        touch(
            &mut app,
            w,
            1,
            TouchPhase::Moved,
            center.x,
            center.y - 100.0,
        );
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW));
        touch(
            &mut app,
            w,
            1,
            TouchPhase::Ended,
            center.x,
            center.y - 100.0,
        );
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .just_released(KeyCode::KeyW));
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
    #[test]
    fn dedicated_terminal_capture_releases_touch_only_owners_before_state_transition_and_drains_close() {
        for native_sneak in [false, true] {
            let (mut app, w) = controls_scene();
            app.world_mut().resource_mut::<Layout>().config.aiming.toggle_aim = true;
            app.world_mut().resource_mut::<Layout>().config.aiming.hold_sneak = true;
            app.world_mut().resource_mut::<drawer::Options>().enabled = true;
            if native_sneak {
                let key = app.world().resource::<Bindings>().get(Control::CrouchSneak).key.unwrap();
                raw_key(&mut app, w, key, ButtonState::Pressed);
                app.update();
                raw_key(&mut app, w, key, ButtonState::Released);
                app.update();
            }
            tap_control(&mut app, w, 41, Control::Block);
            button_touch(&mut app, w, 42, TouchPhase::Started, Control::CrouchSneak);
            app.update();
            assert!(aim_requested(&app));
            assert!(app.world().resource::<nvf_play::control::HeldSneakInput>().active);
            raw_key(&mut app, w, KeyCode::KeyW, ButtonState::Pressed);
            app.world_mut().write_message(MouseButtonInput { window: w, button: MouseButton::Middle, state: ButtonState::Pressed });
            app.init_resource::<nvf_play::terminal::touch::Settings>();
            app.world_mut().resource_mut::<nvf_play::terminal::touch::Settings>().enabled = true;
            app.insert_resource(nvf_play::terminal::TerminalScreen(Some(nvf_play::terminal::Session {
                reference: fnv_sim::FormId(1), base: fnv_sim::FormId(2),
                page: nvf_play::terminal::Page::LockedOut(["Local fixture".into(), "No game data".into()]),
            })));
            button_touch(&mut app, w, 43, TouchPhase::Started, Control::Use);
            app.update();
            assert_eq!(*app.world().resource::<State<nvf_play::PlayState>>().get(), nvf_play::PlayState::Playing);
            let state = app.world().resource::<TouchState>();
            assert!(state.contacts.is_empty() && !state.aim_latched && state.keys.is_empty() && state.mouse.is_empty());
            assert!(state.physical_keys.contains(&KeyCode::KeyW) && state.physical_mouse.contains(&MouseButton::Middle));
            assert!(!aim_requested(&app));
            assert!(!app.world().resource::<nvf_play::control::HeldSneakInput>().active);
            assert_eq!(sneak_requested(&app), native_sneak);
            assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyW));
            assert!(app.world().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Middle));
            assert!(!app.world().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Left));
            assert_eq!(drawer_visibility(&mut app), Visibility::Hidden);
            app.world_mut().resource_mut::<nvf_play::terminal::TerminalScreen>().0 = None;
            tap_control(&mut app, w, 44, Control::Block);
            assert!(!aim_requested(&app), "the first closing batch still belongs to the modal screen");
            tap_control(&mut app, w, 45, Control::Block);
            assert!(aim_requested(&app), "a fresh subsequent input returns to the normal layout");
        }
    }
    #[test]
    fn platform_loading_cover_cancels_touch_latch_sneak_and_drawer_but_keeps_physical_owners() {
        for native_sneak in [false, true] {
            let (mut app, w) = controls_scene();
            app.init_resource::<nvf_play::pad::PlatformInputGate>();
            app.world_mut().resource_mut::<Layout>().config.aiming.toggle_aim = true;
            app.world_mut().resource_mut::<Layout>().config.aiming.hold_sneak = true;
            if native_sneak {
                let key = app.world().resource::<Bindings>().get(Control::CrouchSneak).key.unwrap();
                raw_key(&mut app, w, key, ButtonState::Pressed); app.update();
                raw_key(&mut app, w, key, ButtonState::Released); app.update();
            }
            tap_control(&mut app, w, 80, Control::Block);
            button_touch(&mut app, w, 81, TouchPhase::Started, Control::CrouchSneak); app.update();
            assert!(aim_requested(&app) && app.world().resource::<nvf_play::control::HeldSneakInput>().active);
            raw_key(&mut app, w, KeyCode::KeyW, ButtonState::Pressed);
            app.world_mut().write_message(MouseButtonInput { window: w, button: MouseButton::Middle, state: ButtonState::Pressed });
            app.world_mut().resource_mut::<nvf_play::pad::PlatformInputGate>().set_blocked(true);
            button_touch(&mut app, w, 82, TouchPhase::Started, Control::Use); app.update();
            let state = app.world().resource::<TouchState>();
            assert!(state.contacts.is_empty() && !state.aim_latched);
            assert!(state.physical_keys.contains(&KeyCode::KeyW) && state.physical_mouse.contains(&MouseButton::Middle));
            assert!(!app.world().resource::<nvf_play::control::HeldSneakInput>().active && !aim_requested(&app));
            assert_eq!(sneak_requested(&app), native_sneak);
            app.world_mut().resource_mut::<nvf_play::pad::PlatformInputGate>().set_blocked(false);
            button_touch(&mut app, w, 82, TouchPhase::Ended, Control::Use); app.update();
            assert!(app.world().resource::<TouchState>().contacts.is_empty());
            tap_control(&mut app, w, 83, Control::Block);
            assert!(aim_requested(&app), "a fresh deliberate action works after cover release");
        }
    }

    #[test]
    fn dialogue_capture_releases_held_touch_sources_preserves_hardware_and_drains_closure() {
        for native_sneak in [false,true] {
            let (mut app,w)=controls_scene();
            app.world_mut().resource_mut::<Layout>().config.aiming.toggle_aim=true;
            app.world_mut().resource_mut::<Layout>().config.aiming.hold_sneak=true;
            if native_sneak {
                let key=app.world().resource::<Bindings>().get(Control::CrouchSneak).key.unwrap();
                raw_key(&mut app,w,key,ButtonState::Pressed);app.update();
                raw_key(&mut app,w,key,ButtonState::Released);app.update();
            }
            tap_control(&mut app,w,61,Control::Block);
            button_touch(&mut app,w,62,TouchPhase::Started,Control::CrouchSneak);app.update();
            assert!(aim_requested(&app));assert!(app.world().resource::<nvf_play::control::HeldSneakInput>().active);
            raw_key(&mut app,w,KeyCode::KeyW,ButtonState::Pressed);
            app.world_mut().write_message(MouseButtonInput {window:w,button:MouseButton::Middle,state:ButtonState::Pressed});
            app.insert_resource(nvf_play::dialogue_touch::TouchDialogue {enabled:true,..default()});
            app.world_mut().resource_mut::<NextState<nvf_play::PlayState>>().set(nvf_play::PlayState::Dialogue);
            button_touch(&mut app,w,63,TouchPhase::Started,Control::Use);app.update();
            assert!(app.world().resource::<TouchState>().contacts.is_empty());
            assert!(!aim_requested(&app));assert!(!app.world().resource::<nvf_play::control::HeldSneakInput>().active);
            assert_eq!(sneak_requested(&app),native_sneak);
            assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyW));
            assert!(app.world().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Middle));
            assert!(!app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::Enter));
            app.world_mut().resource_mut::<NextState<nvf_play::PlayState>>().set(nvf_play::PlayState::Playing);
            tap_control(&mut app,w,64,Control::Block);assert!(!aim_requested(&app));
            tap_control(&mut app,w,65,Control::Block);assert!(aim_requested(&app));
        }
    }

    #[test]
    fn dialogue_touch_view_releases_and_hides_underlying_virtual_menu_controls() {
        let (mut app,w)=scene();
        app.insert_resource(nvf_play::dialogue_touch::TouchDialogue {enabled:true,..default()});
        app.world_mut().resource_mut::<NextState<nvf_play::PlayState>>().set(nvf_play::PlayState::Dialogue);
        app.update();app.update();
        touch(&mut app,w,1,TouchPhase::Started,920.0,900.0);app.update();
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::Enter));
        assert!(app.world().resource::<TouchState>().contacts.is_empty());
        let world=app.world_mut();let mut q=world.query::<(&TouchVisual,&Visibility)>();
        assert!(q.iter(world).all(|(_,v)|*v==Visibility::Hidden));
    }

    #[test]
    fn local_gyro_action_uses_its_semantic_glyph_without_replacing_the_cog_or_help() {
        let (mut app,_)=scene();
        assert_eq!(BUTTONS[gyro::BUTTON].text,"Gyro");
        let world=app.world_mut();
        let mut glyphs=world.query::<(&ChildOf,&icons::TouchIcon)>();
        assert!(glyphs.iter(world).any(|(parent,icon)|
            matches!(world.get::<TouchVisual>(parent.parent()),Some(TouchVisual::Button(gyro::BUTTON)))
                && Some(icon.0)==icons::semantic_index("gyro")));
        let mut utilities=world.query::<(&TouchVisual,&icons::TouchIcon)>();
        assert!(utilities.iter(world).any(|(v,i)|matches!(v,TouchVisual::Gear)&&Some(i.0)==icons::semantic_index("settings")));
        assert!(utilities.iter(world).any(|(v,i)|matches!(v,TouchVisual::Help)&&Some(i.0)==icons::semantic_index("help")));
    }

    fn movement_scene() -> (App, Entity) {
        let (mut app, window) = scene();
        app.insert_resource(movement::Options::default())
            .init_resource::<nvf_play::player::MoveIntent>()
            .init_resource::<nvf_play::control::PlayerControl>()
            .init_resource::<nvf_play::control::ControlLocks>()
            .init_resource::<nvf_play::control::LockStats>()
            .init_resource::<nvf_play::furniture::PlayerSeat>()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(40)))
            .add_systems(Update, (nvf_play::player::read_input, nvf_play::control::read_controls, nvf_play::control::enforce_locks).chain());
        app.update(); (app, window)
    }
    fn native_velocity(app: &App) -> [f32; 2] {
        nvf_play::player::velocity(app.world().resource::<nvf_play::player::MoveIntent>(), 0.0, nvf_play::player::MoveSpeeds::OBSERVED)
    }
    fn analog_push(app: &mut App, w: Entity, id: u64, amount: f32) {
        let layout = app.world().resource::<Layout>();
        let radius = layout.config.diameter(Target::Stick, Vec2::splat(1000.0))*0.5;
        let dead = layout.config.aiming.dead_zone;
        touch(app,w,id,TouchPhase::Moved,200.0,700.0-radius*(dead+amount*(1.0-dead)));
        app.update();
    }
    #[test]
    fn default_touch_stick_changes_actual_native_velocity_continuously_and_keeps_legacy_scheme() {
        let (mut app,w)=movement_scene();
        touch(&mut app,w,1,TouchPhase::Started,200.0,700.0);app.update();
        analog_push(&mut app,w,1,0.25);let gentle=native_velocity(&app)[1];
        analog_push(&mut app,w,1,0.5);let stronger=native_velocity(&app)[1];
        assert!(gentle>0.0 && (stronger/gentle-2.0).abs()<0.01,"actual walk speed must scale, not merely its direction");
        analog_push(&mut app,w,1,1.0);
        assert!((native_velocity(&app)[1]-nvf_play::player::MoveSpeeds::OBSERVED.run).abs()<0.01);
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyW),"analog does not invent a hardware key");
        touch(&mut app,w,1,TouchPhase::Ended,200.0,500.0);app.update();assert_eq!(native_velocity(&app),[0.0;2]);
        app.world_mut().resource_mut::<movement::Options>().analog=false;app.update();
        touch(&mut app,w,2,TouchPhase::Started,200.0,700.0);app.update();analog_push(&mut app,w,2,0.25);
        assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyW));
        assert!((native_velocity(&app)[1]-nvf_play::player::MoveSpeeds::OBSERVED.run).abs()<0.01,"explicit legacy scheme preserves directional speed");
    }
    #[test]
    fn analog_touch_obeys_physical_direction_priority_script_locks_and_platform_auto_move_hold() {
        let (mut app,w)=movement_scene();
        touch(&mut app,w,1,TouchPhase::Started,200.0,700.0);app.update();analog_push(&mut app,w,1,0.25);
        raw_key(&mut app,w,KeyCode::KeyW,ButtonState::Pressed);app.update();
        assert!((native_velocity(&app)[1]-nvf_play::player::MoveSpeeds::OBSERVED.run).abs()<0.01);
        touch(&mut app,w,1,TouchPhase::Canceled,200.0,600.0);app.update();
        assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyW));
        app.world_mut().resource_mut::<nvf_play::control::ControlLocks>().movement=true;app.update();assert_eq!(native_velocity(&app),[0.0;2]);
        app.world_mut().resource_mut::<nvf_play::control::ControlLocks>().movement=false;
        raw_key(&mut app,w,KeyCode::KeyW,ButtonState::Released);app.update();
        touch(&mut app,w,2,TouchPhase::Started,200.0,700.0);app.update();
        app.insert_resource(nvf_play::script_input::ScriptBlocked{keys:vec![KeyCode::KeyW],..default()});
        analog_push(&mut app,w,2,0.8);assert_eq!(native_velocity(&app),[0.0;2],"disabled bound Forward cannot be bypassed by analog");
        app.world_mut().resource_mut::<nvf_play::control::PlayerControl>().auto_move=true;
        app.init_resource::<nvf_play::pad::PlatformInputGate>();app.world_mut().resource_mut::<nvf_play::pad::PlatformInputGate>().set_blocked(true);
        app.update();assert_eq!(native_velocity(&app),[0.0;2]);
        assert!(app.world().resource::<nvf_play::control::PlayerControl>().auto_move,"cover neutralizes intentional toggle without replacing it");
    }
    #[test]
    fn fire_drag_looks_while_native_trigger_stays_held_and_preserves_physical_fire_owner() {
        let (mut app,w)=movement_scene();app.world_mut().resource_mut::<nvf_play::control::PlayerControl>().weapon_out=true;
        button_touch(&mut app,w,1,TouchPhase::Started,Control::Use);app.update();
        assert!(app.world().resource::<nvf_play::control::PlayerControl>().trigger);
        let at=app.world().resource::<TouchState>().contacts[&1].position*1000.0;
        touch(&mut app,w,1,TouchPhase::Moved,at.x+50.0,at.y-15.0);app.update();
        let delta=app.world().resource::<AccumulatedMouseMotion>().delta;
        assert!((delta-Vec2::new(50.0,-15.0)).length()<0.01);
        assert!(app.world().resource::<nvf_play::control::PlayerControl>().trigger,"dragging outside the circle keeps the fire contact");
        physical_mouse(&mut app,w,ButtonState::Pressed);app.update();
        touch(&mut app,w,1,TouchPhase::Canceled,at.x+50.0,at.y-15.0);app.update();
        assert!(app.world().resource::<nvf_play::control::PlayerControl>().trigger);
        physical_mouse(&mut app,w,ButtonState::Released);app.update();assert!(!app.world().resource::<nvf_play::control::PlayerControl>().trigger);
        touch(&mut app,w,1,TouchPhase::Moved,at.x+100.0,at.y);app.update();assert_eq!(app.world().resource::<AccumulatedMouseMotion>().delta,Vec2::ZERO);
    }
    #[test]
    fn fire_drag_and_empty_look_have_one_owner_and_stationary_fire_scheme_remains_available() {
        let (mut app,w)=movement_scene();
        touch(&mut app,w,1,TouchPhase::Started,600.0,400.0);app.update();
        button_touch(&mut app,w,2,TouchPhase::Started,Control::Use);app.update();
        let at=app.world().resource::<TouchState>().contacts[&2].position*1000.0;
        touch(&mut app,w,2,TouchPhase::Moved,at.x+70.0,at.y);app.update();assert_eq!(app.world().resource::<AccumulatedMouseMotion>().delta,Vec2::ZERO);
        touch(&mut app,w,1,TouchPhase::Moved,620.0,410.0);app.update();assert!((app.world().resource::<AccumulatedMouseMotion>().delta-Vec2::new(20.0,10.0)).length()<0.01);
        app.world_mut().resource_mut::<movement::Options>().fire_drag=false;app.update();
        button_touch(&mut app,w,3,TouchPhase::Started,Control::Use);app.update();
        touch(&mut app,w,3,TouchPhase::Moved,at.x+60.0,at.y);app.update();assert_eq!(app.world().resource::<AccumulatedMouseMotion>().delta,Vec2::ZERO);
    }
    #[test]
    fn run_lock_uses_rebound_native_auto_move_and_fresh_push_or_target_tap_stops_it() {
        let (mut app,w)=movement_scene();
        app.world_mut().resource_mut::<movement::Options>().run_lock=true;
        app.world_mut().resource_mut::<Bindings>().rebind(Control::AutoMove,nvf_play::bindings::Input::Key(KeyCode::F8));app.update();
        touch(&mut app,w,1,TouchPhase::Started,200.0,700.0);app.update();
        let target=movement::lock_rect(app.world().resource::<Layout>(),app.world().resource::<TouchState>(),Vec2::splat(1000.0)).center();
        touch(&mut app,w,1,TouchPhase::Moved,target.x,target.y);app.update();
        touch(&mut app,w,1,TouchPhase::Ended,target.x,target.y);app.update();app.update();
        assert!(app.world().resource::<nvf_play::control::PlayerControl>().auto_move);assert_eq!(native_velocity(&app)[1],nvf_play::player::MoveSpeeds::OBSERVED.run);
        let target=movement::lock_rect(app.world().resource::<Layout>(),app.world().resource::<TouchState>(),Vec2::splat(1000.0)).center();
        touch(&mut app,w,2,TouchPhase::Started,target.x,target.y);touch(&mut app,w,2,TouchPhase::Ended,target.x,target.y);app.update();app.update();
        assert!(!app.world().resource::<nvf_play::control::PlayerControl>().auto_move);assert_eq!(native_velocity(&app),[0.0;2]);
        app.world_mut().resource_mut::<nvf_play::control::PlayerControl>().auto_move=true;
        touch(&mut app,w,3,TouchPhase::Started,200.0,700.0);app.update();analog_push(&mut app,w,3,0.5);
        assert!(!app.world().resource::<nvf_play::control::PlayerControl>().auto_move,"fresh admitted forward/back push is native Auto Move cancellation");
    }
    #[test]
    fn run_lock_cancel_out_and_back_and_disabled_binding_never_toggle_native_auto_move() {
        let (mut app,w)=movement_scene();app.world_mut().resource_mut::<movement::Options>().run_lock=true;app.update();
        let target=movement::lock_rect(app.world().resource::<Layout>(),app.world().resource::<TouchState>(),Vec2::splat(1000.0)).center();
        for cancel in [false,true] {
            touch(&mut app,w,4,TouchPhase::Started,target.x,target.y);app.update();
            if cancel {touch(&mut app,w,4,TouchPhase::Canceled,target.x,target.y);} else {
                touch(&mut app,w,4,TouchPhase::Moved,target.x+60.0,target.y);touch(&mut app,w,4,TouchPhase::Ended,target.x,target.y);
            }
            app.update();assert!(!app.world().resource::<nvf_play::control::PlayerControl>().auto_move);
        }
        let key=app.world().resource::<Bindings>().get(Control::AutoMove).key.unwrap();
        app.insert_resource(nvf_play::script_input::ScriptBlocked{keys:vec![key],..default()});
        touch(&mut app,w,5,TouchPhase::Started,target.x,target.y);touch(&mut app,w,5,TouchPhase::Ended,target.x,target.y);app.update();
        assert!(!app.world().resource::<nvf_play::control::PlayerControl>().auto_move);
    }
    #[derive(Resource,Default)]
    struct VatsPresses(usize);
    fn count_vats(input:nvf_play::bindings::ControlInput,mut seen:ResMut<VatsPresses>){seen.0+=usize::from(input.just_pressed(Control::Vats));}
    #[test]
    fn optional_vats_double_tap_uses_binding_once_and_drops_same_batch_fire_tail() {
        let (mut app,w)=movement_scene();app.init_resource::<VatsPresses>().add_systems(Update,count_vats);
        app.world_mut().resource_mut::<movement::Options>().vats_taps=true;
        app.world_mut().resource_mut::<Bindings>().rebind(Control::Vats,nvf_play::bindings::Input::Key(KeyCode::F9));
        app.world_mut().resource_mut::<nvf_play::control::PlayerControl>().weapon_out=true;app.update();
        touch(&mut app,w,1,TouchPhase::Started,600.0,400.0);touch(&mut app,w,1,TouchPhase::Ended,600.0,400.0);app.update();assert_eq!(app.world().resource::<VatsPresses>().0,0);
        touch(&mut app,w,2,TouchPhase::Started,602.0,400.0);touch(&mut app,w,2,TouchPhase::Ended,602.0,400.0);
        button_touch(&mut app,w,3,TouchPhase::Started,Control::Use);app.update();
        assert_eq!(app.world().resource::<VatsPresses>().0,1);assert!(!app.world().resource::<nvf_play::control::PlayerControl>().trigger);
        app.update();assert_eq!(app.world().resource::<VatsPresses>().0,1);assert!(!app.world().resource::<nvf_play::control::PlayerControl>().trigger);
        button_touch(&mut app,w,4,TouchPhase::Started,Control::Use);app.update();assert!(app.world().resource::<nvf_play::control::PlayerControl>().trigger);
    }
    #[test]
    fn vats_gesture_rejects_out_and_back_cancel_and_controller_return_touch() {
        let (mut app,w)=movement_scene();app.init_resource::<VatsPresses>().add_systems(Update,count_vats);
        app.world_mut().resource_mut::<movement::Options>().vats_taps=true;app.update();
        for cancel in [false,true] {
            touch(&mut app,w,1,TouchPhase::Started,600.0,400.0);app.update();
            if cancel {touch(&mut app,w,1,TouchPhase::Canceled,600.0,400.0);} else {
                touch(&mut app,w,1,TouchPhase::Moved,650.0,400.0);touch(&mut app,w,1,TouchPhase::Ended,600.0,400.0);
            }
            app.update();touch(&mut app,w,2,TouchPhase::Started,600.0,400.0);touch(&mut app,w,2,TouchPhase::Ended,600.0,400.0);app.update();
            assert_eq!(app.world().resource::<VatsPresses>().0,0);
            app.world_mut().resource_mut::<TouchState>().cancel_touch();
        }
        app.add_plugins(nvf_play::pad::InputSwitchPlugin);app.world_mut().spawn(Gamepad::default());
        app.world_mut().resource_mut::<Bindings>().device=nvf_play::bindings::InputDevice::Controller;
        touch(&mut app,w,3,TouchPhase::Started,200.0,700.0);app.update();
        assert_eq!(native_velocity(&app),[0.0;2]);assert!(!app.world().resource::<nvf_play::pad::TouchDeviceSwitch>().allows_actions());
        touch(&mut app,w,3,TouchPhase::Moved,200.0,600.0);app.update();assert_eq!(native_velocity(&app),[0.0;2]);
        assert_eq!(app.world().resource::<VatsPresses>().0,0);
    }

    #[derive(Resource,Default)]
    struct MovementQueries { control:bool, digital:bool, fresh:bool, script:bool, raw_held:bool }
    fn observe_movement(input:nvf_play::bindings::ControlInput,keys:Res<ButtonInput<KeyCode>>,mut seen:ResMut<MovementQueries>) {
        let script=fnv_game::script_input::ScriptInput { virtual_controls:nvf_play::script_input::admitted_virtual_controls(&input), ..Default::default() };
        seen.control=input.pressed(Control::Forward);seen.digital=input.digital_movement_pressed(Control::Forward);
        seen.fresh=input.just_pressed(Control::Forward);seen.script=script.control_pressed(0,false);
        seen.raw_held=keys.pressed(KeyCode::KeyW);
    }
    #[test]
    fn analog_is_script_visible_without_quantizing_its_magnitude_or_faking_a_rebound_key() {
        let (mut app,w)=movement_scene();app.init_resource::<MovementQueries>().add_systems(Update,observe_movement);
        app.world_mut().resource_mut::<Bindings>().rebind(Control::Forward,nvf_play::bindings::Input::Key(KeyCode::F10));
        touch(&mut app,w,1,TouchPhase::Started,200.0,700.0);app.update();analog_push(&mut app,w,1,0.25);
        let seen=app.world().resource::<MovementQueries>();
        assert!(seen.control && seen.script && seen.fresh && !seen.digital && !seen.raw_held);
        assert!((app.world().resource::<nvf_play::player::MoveIntent>().analog_scale-0.25).abs()<0.001);
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::F10));
        app.update();assert!(!app.world().resource::<MovementQueries>().fresh);
        raw_key(&mut app,w,KeyCode::F10,ButtonState::Pressed);app.update();
        assert!(app.world().resource::<MovementQueries>().digital);assert_eq!(app.world().resource::<nvf_play::player::MoveIntent>().analog_scale,1.0);
        raw_key(&mut app,w,KeyCode::F10,ButtonState::Released);
        app.insert_resource(nvf_play::script_input::ScriptBlocked{keys:vec![KeyCode::F10],..default()});app.update();
        let seen=app.world().resource::<MovementQueries>();assert!(!seen.control && !seen.script && !seen.digital);assert_eq!(native_velocity(&app),[0.0;2]);
        app.world_mut().resource_mut::<nvf_play::script_input::ScriptBlocked>().keys.clear();
        app.init_resource::<nvf_play::pad::PlatformInputGate>();app.world_mut().resource_mut::<nvf_play::pad::PlatformInputGate>().set_blocked(true);app.update();
        assert!(!app.world().resource::<MovementQueries>().control && !app.world().resource::<MovementQueries>().script);
    }
    #[test]
    fn auto_move_target_has_a_specific_large_node_and_hides_during_platform_capture() {
        let (mut app,w)=movement_scene();app.world_mut().resource_mut::<movement::Options>().run_lock=true;app.update();
        let marker=app.world_mut().query_filtered::<Entity,With<movement::LockMarker>>().single(app.world()).unwrap();
        assert_eq!(*app.world().get::<Visibility>(marker).unwrap(),Visibility::Inherited);
        let node=app.world().get::<Node>(marker).unwrap();assert_eq!((node.width,node.height),(px(56.0),px(56.0)));
        app.init_resource::<nvf_play::pad::PlatformInputGate>();app.world_mut().resource_mut::<nvf_play::pad::PlatformInputGate>().set_blocked(true);
        touch(&mut app,w,1,TouchPhase::Started,200.0,700.0);app.update();
        assert_eq!(*app.world().get::<Visibility>(marker).unwrap(),Visibility::Hidden);
        assert!(app.world().resource::<TouchState>().contacts.is_empty());assert_eq!(native_velocity(&app),[0.0;2]);

    }

    fn mixed_hotbar_phone_scene(size: Vec2, preset: &str, count: usize) -> (App, Entity) {
        use nvf_play::{hotkeys::Hotkeys, settings};
        let (mut app, window) = movement_scene();
        app.insert_resource(settings::SettingsFile(None));
        let forms = Box::leak(Box::new(fnv_sim::FormStore::default()));
        let refs = Box::leak(Box::new(fnv_sim::RefTable::default()));
        app.insert_resource(nvf_play::sim::SimState {
            forms, refs, world: fnv_sim::World::new(forms, refs, fnv_sim::SimConfig::default()),
        }).init_resource::<Hotkeys>();
        hotbar::reset_collision_fixture(app.world_mut());
        app.world_mut().get_mut::<Window>(window).unwrap().resolution.set(size.x, size.y);
        app.world_mut().resource_mut::<Layout>().config = layout::Config::default();
        app.world_mut().resource_mut::<Layout>().config.apply_preset(preset);
        for key in ["touch-hotbar", "touch-gyro-v1", "touch-auto-move-lock-v1"] {
            let index = app.world().resource::<settings::Settings>().index(key).unwrap();
            assert_eq!(settings::flip(app.world_mut(), index), Some(true), "production setting {key}");
        }
        let index = app.world().resource::<settings::Settings>().index("touch-hotbar-slots").unwrap();
        let wanted = count.to_string();
        let current = (app.world().resource::<settings::Settings>().choices(index).unwrap().chosen)(app.world());
        if current != wanted {
            assert!(settings::pick(app.world_mut(), index, &wanted));
        } else {
            assert_eq!(current, wanted, "default production choice is already selected");
        }
        app.update(); // Viewport/settings admission drains this frame.
        app.update();
        (app, window)
    }

    fn mixed_hotbar_tiles(app: &mut App) -> Vec<(String, Rect)> {
        let world = app.world_mut();
        let mut out = Vec::new();
        let mut query = world.query::<(&Text, &ChildOf)>();
        for (label, parent) in query.iter(world) {
            if world.get::<GlobalZIndex>(parent.parent()).is_none_or(|z| z.0 != 2200) { continue; }
            let Some(node) = world.get::<Node>(parent.parent()) else { continue };
            let (Val::Px(x), Val::Px(y), Val::Px(w), Val::Px(h)) = (node.left, node.top, node.width, node.height) else { panic!("hotbar tile lost pixel bounds") };
            if label.0 == "Pins" || label.0.split_whitespace().next().is_some_and(|n| n.parse::<usize>().is_ok()) {
                out.push((label.0.clone(), Rect::from_corners(Vec2::new(x,y), Vec2::new(x+w,y+h))));
            }
        }
        out
    }

    #[test]
    fn enabled_hotbar_preserves_real_jump_stick_and_fixed_gyro_routes_on_phone_presets() {
        for size in [Vec2::new(640.0,360.0), Vec2::new(792.0,363.0)] {
            for (preset, _) in profiles::PRESETS {
                for count in [4,6,8] {
                    let (mut app, window) = mixed_hotbar_phone_scene(size, preset, count);
                    let case = format!("{size:?}/{preset}/{count}");
                    let jump = BUTTONS.iter().position(|b| b.control == Some(Control::Jump)).unwrap();
                    let center = app.world().resource::<Layout>().config.center(Target::Button(jump),size);
                    touch(&mut app,window,300,TouchPhase::Started,center.x,center.y); app.update();
                    assert!(app.world().resource::<nvf_play::player::MoveIntent>().jump, "{case}: hotbar stole Jump center");
                    touch(&mut app,window,300,TouchPhase::Ended,center.x,center.y); app.update();
                    app.world_mut().resource_mut::<nvf_play::player::MoveIntent>().jump = false;
                    let config = &app.world().resource::<Layout>().config;
                    let center = config.center(Target::Stick,size);
                    let r = config.diameter(Target::Stick,size)*0.45;
                    for (probe, direction) in [Vec2::NEG_Y,Vec2::new(0.707,-0.707),Vec2::new(-0.707,-0.707)].into_iter().enumerate() {
                        let point = center + direction*r;
                        assert_eq!(app.world().resource::<Layout>().config.hit(point,size,true,false),Some(Target::Stick));
                        let id = 310+probe as u64;
                        touch(&mut app,window,id,TouchPhase::Started,point.x,point.y); app.update();
                        touch(&mut app,window,id,TouchPhase::Moved,point.x,point.y-24.0); app.update();
                        assert!(native_velocity(&app)[1] > 0.0, "{case}: hotbar stole admitted stick point {point:?}");
                        touch(&mut app,window,id,TouchPhase::Ended,point.x,point.y-24.0); app.update();
                    }
                    let gyro = app.world().resource::<Layout>().config.center(Target::Button(gyro::BUTTON),size);
                    assert!((gyro-Vec2::new(size.x*0.5,size.y*0.34)).length()<0.01,"{case}: fixed gyro anchor moved");
                    let mut reader = bevy::ecs::message::MessageCursor::<nvf_play::weapon_wheel::TouchHotkey>::default();
                    reader.read(app.world().resource::<Messages<nvf_play::weapon_wheel::TouchHotkey>>()).for_each(drop);
                    touch(&mut app,window,320,TouchPhase::Started,gyro.x,gyro.y); app.update();
                    assert!(app.world().resource::<TouchState>().contacts.get(&320).is_some_and(|c| matches!(c.role,Role::Button(i) if i==gyro::BUTTON)), "{case}: hotbar stole gyro target");
                    assert!(app.world().resource::<gyro::State>().active(app.world().resource::<gyro::Options>()));
                    touch(&mut app,window,320,TouchPhase::Ended,gyro.x,gyro.y); app.update();
                    assert_eq!(reader.read(app.world().resource::<Messages<nvf_play::weapon_wheel::TouchHotkey>>()).count(),0,"{case}: gyro also activated hotbar");
                }
            }
        }
    }

    #[test]
    fn rendered_hotbar_slots_and_pins_are_reachable_without_covering_native_targets() {
        for size in [Vec2::new(640.0,360.0),Vec2::new(792.0,363.0)] {
            for (preset, _) in profiles::PRESETS {
                for count in [4,6,8] {
                    let (mut app, window) = mixed_hotbar_phone_scene(size,preset,count);
                    let case = format!("{size:?}/{preset}/{count}");
                    let tiles = mixed_hotbar_tiles(&mut app);
                    assert_eq!(tiles.len(),count+1,"{case}: missing rendered slot/tray");
                    for (label,rect) in &tiles {
                        assert!(rect.width()>=48.0 && rect.height()>=48.0);
                        let config=&app.world().resource::<Layout>().config;
                        for i in (0..9).chain(std::iter::once(gyro::BUTTON)) {
                            let center=config.center(Target::Button(i),size);
                            let point=center.clamp(rect.min+Vec2::splat(0.25),rect.max-Vec2::splat(0.25));
                            assert!(config.hit(point,size,true,false).is_none(),"{case}/{label}: visible bar intersects gameplay control {i}");
                        }
                        let point=config.center(Target::Stick,size).clamp(rect.min+Vec2::splat(0.25),rect.max-Vec2::splat(0.25));
                        assert!(config.hit(point,size,true,false).is_none(),"{case}/{label}: visible bar intersects stick");
                        for help in [false,true] {
                            let overlap=rect.intersect(layout::utility_rect(help,size));
                            assert!(overlap.width()<=0.5 || overlap.height()<=0.5,"{case}/{label}: bar covers cog/help");
                        }
                        let lock=movement::lock_rect(app.world().resource::<Layout>(),app.world().resource::<TouchState>(),size);
                        if movement::lock_available(app.world().resource::<Layout>(),app.world().resource::<TouchState>(),size) {
                            let overlap=rect.intersect(lock);
                            assert!(overlap.width()<=0.5 || overlap.height()<=0.5,"{case}/{label}: bar covers AutoMove target");
                        }
                    }
                    let mut reader=bevy::ecs::message::MessageCursor::<nvf_play::weapon_wheel::TouchHotkey>::default();
                    for (i,(label,rect)) in tiles.iter().enumerate() {
                        let center=rect.center();let id=400+i as u64;
                        touch(&mut app,window,id,TouchPhase::Started,center.x,center.y);
                        touch(&mut app,window,id,TouchPhase::Ended,center.x,center.y);app.update();
                        let emitted=reader.read(app.world().resource::<Messages<nvf_play::weapon_wheel::TouchHotkey>>()).map(|m|m.0).collect::<Vec<_>>();
                        if label=="Pins" { assert!(hotbar::tray_open_collision_fixture(app.world())); assert!(emitted.is_empty()); }
                        else { assert_eq!(emitted,[label.split_whitespace().next().unwrap().parse::<usize>().unwrap()-1],"{case}/{label}: rendered slot not reachable exactly once"); }
                    }
                }
            }
        }
    }

    #[test]
    fn hotbar_context_move_is_owned_ui_and_never_also_activates_raw_gyro() {
        let (mut app,window)=mixed_hotbar_phone_scene(Vec2::new(640.0,360.0),"classic",8);
        let tiles=mixed_hotbar_tiles(&mut app);
        let first=tiles.iter().find(|(label,_)|label.starts_with("1 ")).unwrap().1.center();
        touch(&mut app,window,600,TouchPhase::Started,first.x,first.y);app.update();
        for _ in 0..14 {app.update();} // Actual production hold threshold, not synthetic Context state.
        touch(&mut app,window,600,TouchPhase::Ended,first.x,first.y);app.update();
        let point={
            let world=app.world_mut();
            let mut query=world.query::<(&Text,&ChildOf)>();
            let parent=query.iter(world).find(|(text,_)|text.0=="Move bar").unwrap().1.parent();
            let node=world.get::<Node>(parent).unwrap();
            let (Val::Px(x),Val::Px(y),Val::Px(w),Val::Px(h))=(node.left,node.top,node.width,node.height) else {panic!("context pixel bounds")};
            Vec2::new(x+w*0.5,y+h*0.5)
        };
        touch(&mut app,window,601,TouchPhase::Started,point.x,point.y);app.update();
        assert!(!app.world().resource::<gyro::State>().active(app.world().resource::<gyro::Options>()),
            "hotbar context and raw gyro reader cannot own the same finger");
        touch(&mut app,window,601,TouchPhase::Ended,point.x,point.y);app.update();
        let world=app.world_mut();
        assert!(world.query::<&Text>().iter(world).any(|t|t.0=="Drag the bar to reposition it."),"actual Move command must still execute");
    }


    fn choose_swipe_destination(app: &mut App, name: &str) {
        let registry=app.world().resource::<nvf_play::settings::Settings>();
        let row=registry.index("touch-bottom-swipe-target-v1").unwrap();
        let current=(registry.choices(row).unwrap().chosen)(app.world());
        if current != name { assert!(nvf_play::settings::pick(app.world_mut(),row,name)); }
        assert_eq!(app.world().resource::<drawer::Options>().destination.name(),name);
    }
    #[test]
    fn bottom_swipe_routes_one_real_destination_and_utility_capture_drops_raw_tail() {
        for target in ["Inventory","Utility tray"] {
            let (mut app,w)=mixed_hotbar_phone_scene(Vec2::new(640.,360.),"classic",4);
            app.world_mut().resource_mut::<drawer::Options>().enabled=true;
            choose_swipe_destination(&mut app,target);
            raw_key(&mut app,w,KeyCode::KeyW,ButtonState::Pressed);app.update();
            let start=drawer::handle(Vec2::new(640.,360.),24.).center();
            touch(&mut app,w,810,TouchPhase::Started,start.x,start.y);app.update();
            touch(&mut app,w,810,TouchPhase::Ended,start.x,start.y-120.);
            let gyro=app.world().resource::<Layout>().config.center(Target::Button(gyro::BUTTON),Vec2::new(640.,360.));
            touch(&mut app,w,811,TouchPhase::Started,gyro.x,gyro.y);
            touch(&mut app,w,811,TouchPhase::Ended,gyro.x,gyro.y);
            app.update();
            let inventory=app.world().resource::<Messages<nvf_play::menu::OpenInventory>>().len();
            assert_eq!(inventory,usize::from(target=="Inventory"),"single recognizer must not emit both destinations");
            assert_eq!(hotbar::tray_open_collision_fixture(app.world()),target=="Utility tray");
            assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyW),"swipe cannot erase hardware owner");
            assert!(app.world().resource::<TouchState>().contacts.is_empty());
            assert!(app.world().resource::<TouchState>().taps.is_empty());
            if target=="Utility tray" {
                assert!(app.world().resource::<TouchState>().ui_capture);
                assert!(!app.world().resource::<gyro::State>().active(app.world().resource::<gyro::Options>()),"raw gyro tail cannot coactivate with utility commit");
            }
        }
    }
    #[test]
    fn utility_swipe_cancels_on_target_inset_disable_and_mode_changes() {
        for scenario in 0..4 {
            let (mut app,w)=mixed_hotbar_phone_scene(Vec2::new(640.,360.),"classic",4);
            app.world_mut().resource_mut::<drawer::Options>().enabled=true;
            choose_swipe_destination(&mut app,"Utility tray");
            let start=drawer::handle(Vec2::new(640.,360.),24.).center();
            touch(&mut app,w,820,TouchPhase::Started,start.x,start.y);app.update();
            match scenario {
                0=>choose_swipe_destination(&mut app,"Inventory"),
                1=>app.world_mut().resource_mut::<drawer::SafeBottom>().0=48.,
                2=>app.world_mut().resource_mut::<drawer::Options>().enabled=false,
                _=>app.world_mut().resource_mut::<NextState<nvf_play::PlayState>>().set(nvf_play::PlayState::Menu),
            }
            touch(&mut app,w,820,TouchPhase::Ended,start.x,start.y-120.);app.update();
            assert_eq!(app.world().resource::<Messages<nvf_play::menu::OpenInventory>>().len(),0,"scenario{scenario}");
            assert!(!hotbar::tray_open_collision_fixture(app.world()),"scenario{scenario}: stale contact reopened tray");
        }
    }
    #[test]
    fn utility_swipe_is_disabled_with_hidden_tray_and_keeps_system_bottom_edge_free() {
        for scenario in 0..3 {
            let (mut app,w)=mixed_hotbar_phone_scene(Vec2::new(640.,360.),"classic",4);
            choose_swipe_destination(&mut app,"Utility tray");
            app.world_mut().resource_mut::<drawer::Options>().enabled=scenario!=0;
            app.world_mut().resource_mut::<drawer::SafeBottom>().0=48.;
            if scenario==1 {
                let row=app.world().resource::<nvf_play::settings::Settings>().index("touch-hotbar-tray").unwrap();
                assert_eq!(nvf_play::settings::flip(app.world_mut(),row),Some(false));
            }
            app.update();
            let start=if scenario==2 { Vec2::new(320.,358.) } else { drawer::handle(Vec2::new(640.,360.),48.).center() };
            touch(&mut app,w,830,TouchPhase::Started,start.x,start.y);app.update();
            touch(&mut app,w,830,TouchPhase::Ended,start.x,start.y-120.);app.update();
            assert_eq!(app.world().resource::<Messages<nvf_play::menu::OpenInventory>>().len(),0);
            assert!(!hotbar::tray_open_collision_fixture(app.world()),"disabled/hidden/system-edge scenario{scenario}");
        }
    }

    #[test]
    fn utility_handle_visibility_matches_platform_input_admission_and_fresh_return() {
        let (mut app,w)=mixed_hotbar_phone_scene(Vec2::new(640.,360.),"classic",4);
        app.init_resource::<nvf_play::pad::PlatformInputGate>();
        app.world_mut().resource_mut::<drawer::Options>().enabled=true;
        choose_swipe_destination(&mut app,"Utility tray");app.update();
        assert_eq!(drawer_visibility(&mut app),Visibility::Inherited);
        app.world_mut().resource_mut::<nvf_play::pad::PlatformInputGate>().set_blocked(true);
        app.update();assert_eq!(drawer_visibility(&mut app),Visibility::Hidden);
        let start=drawer::handle(Vec2::new(640.,360.),24.).center();
        touch(&mut app,w,840,TouchPhase::Started,start.x,start.y);
        touch(&mut app,w,840,TouchPhase::Ended,start.x,start.y-120.);app.update();
        assert!(!hotbar::tray_open_collision_fixture(app.world()));
        app.world_mut().resource_mut::<nvf_play::pad::PlatformInputGate>().set_blocked(false);
        app.update();assert_eq!(drawer_visibility(&mut app),Visibility::Inherited);
        touch(&mut app,w,841,TouchPhase::Started,start.x,start.y);app.update();
        touch(&mut app,w,841,TouchPhase::Ended,start.x,start.y-120.);app.update();
        assert!(hotbar::tray_open_collision_fixture(app.world()),"fresh unblocked gesture reaches the actual tray");
    }

    #[test]
    fn open_pins_cancellation_releases_hidden_capture_and_readmits_fresh_native_routes() {
        use nvf_play::{bindings::InputDevice, settings};
        for scenario in ["disabled", "controller", "focus", "modal", "platform"] {
            let size = Vec2::new(640., 360.);
            let (mut app, w) = mixed_hotbar_phone_scene(size, "classic", 4);
            app.add_plugins(nvf_play::pad::InputSwitchPlugin);
            app.world_mut().spawn(Gamepad::default());
            raw_key(&mut app, w, KeyCode::KeyW, ButtonState::Pressed);
            app.world_mut().write_message(MouseButtonInput {
                window: w, button: MouseButton::Middle, state: ButtonState::Pressed,
            });
            app.update();
            let pins = mixed_hotbar_tiles(&mut app).into_iter()
                .find(|(label, _)| label == "Pins").unwrap().1.center();
            touch(&mut app, w, 900, TouchPhase::Started, pins.x, pins.y);
            touch(&mut app, w, 900, TouchPhase::Ended, pins.x, pins.y);
            app.update(); app.update();
            assert!(hotbar::tray_open_collision_fixture(app.world()), "{scenario}: real Pins tap must open tray");
            assert!(app.world().resource::<TouchState>().ui_capture, "{scenario}: open tray owns gameplay input");
            match scenario {
                "disabled" => {
                    let row = app.world().resource::<settings::Settings>().index("touch-hotbar").unwrap();
                    assert_eq!(settings::flip(app.world_mut(), row), Some(false));
                }
                "controller" => nvf_play::bindings::set_device(app.world_mut(), InputDevice::Controller),
                "focus" => {
                    app.world_mut().get_mut::<Window>(w).unwrap().focused = false;
                    app.world_mut().write_message(WindowFocused { window: w, focused: false });
                    // Winit's focus-loss adapter emits this as well as WindowFocused;
                    // InputPlugin's keyboard system releases raw keys on this message.
                    app.world_mut().write_message(KeyboardFocusLost);
                }
                "modal" => {
                    app.init_resource::<nvf_play::terminal::touch::Settings>();
                    app.world_mut().resource_mut::<nvf_play::terminal::touch::Settings>().enabled = true;
                    app.insert_resource(nvf_play::terminal::TerminalScreen(Some(nvf_play::terminal::Session {
                        reference: fnv_sim::FormId(1), base: fnv_sim::FormId(2),
                        page: nvf_play::terminal::Page::LockedOut(["Local fixture".into(), "No game data".into()]),
                    })));
                }
                "platform" => {
                    app.init_resource::<nvf_play::pad::PlatformInputGate>();
                    app.world_mut().resource_mut::<nvf_play::pad::PlatformInputGate>().set_blocked(true);
                }
                _ => unreachable!(),
            }
            app.update();
            assert!(!hotbar::tray_open_collision_fixture(app.world()), "{scenario}: cancellation must close Pins");
            assert!(!app.world().resource::<TouchState>().ui_capture, "{scenario}: hidden hotbar cannot retain capture");
            let hardware_survives = scenario != "focus";
            assert_eq!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyW), hardware_survives,
                "{scenario}: preserve hardware except the existing focus-loss release contract");
            // Bevy's mouse input system consumes actual MouseButtonInput only;
            // focus loss cancels wrapper tracking without inventing a raw release.
            assert!(app.world().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Middle));
            if scenario == "focus" {
                let state = app.world().resource::<TouchState>();
                assert!(state.physical_keys.is_empty() && state.physical_mouse.is_empty(),
                    "focus loss must clear wrapper ownership, independently of raw hardware state");
            }
            match scenario {
                "disabled" => {
                    let row = app.world().resource::<settings::Settings>().index("touch-hotbar").unwrap();
                    assert_eq!(settings::flip(app.world_mut(), row), Some(true));
                }
                "controller" => {
                    let jump = BUTTONS.iter().position(|b| b.control == Some(Control::Jump)).unwrap();
                    let point = app.world().resource::<Layout>().config.center(Target::Button(jump), size);
                    touch(&mut app, w, 901, TouchPhase::Started, point.x, point.y);
                    touch(&mut app, w, 901, TouchPhase::Ended, point.x, point.y);
                    app.update();
                    assert_eq!(app.world().resource::<Bindings>().device, InputDevice::KeyboardMouse);
                    assert!(!app.world().resource::<nvf_play::pad::TouchDeviceSwitch>().allows_actions());
                    assert!(!app.world().resource::<nvf_play::player::MoveIntent>().jump, "return touch only switches device");
                }
                "focus" => {
                    app.world_mut().get_mut::<Window>(w).unwrap().focused = true;
                    app.world_mut().write_message(WindowFocused { window: w, focused: true });
                }
                "modal" => app.world_mut().resource_mut::<nvf_play::terminal::TerminalScreen>().0 = None,
                "platform" => app.world_mut().resource_mut::<nvf_play::pad::PlatformInputGate>().set_blocked(false),
                _ => unreachable!(),
            }
            app.update(); app.update(); // Includes the dedicated modal's closing-frame drain.
            assert!(!app.world().resource::<TouchState>().ui_capture);
            assert!(!hotbar::tray_open_collision_fixture(app.world()), "{scenario}: return cannot resurrect Pins");
            if scenario == "focus" {
                raw_key(&mut app, w, KeyCode::KeyW, ButtonState::Pressed);
                app.world_mut().write_message(MouseButtonInput {
                    window: w, button: MouseButton::Middle, state: ButtonState::Pressed,
                });
                app.update();
            }
            let jump = BUTTONS.iter().position(|b| b.control == Some(Control::Jump)).unwrap();
            let point = app.world().resource::<Layout>().config.center(Target::Button(jump), size);
            touch(&mut app, w, 902, TouchPhase::Started, point.x, point.y); app.update();
            assert!(app.world().resource::<nvf_play::player::MoveIntent>().jump, "{scenario}: fresh native Jump admitted");
            touch(&mut app, w, 902, TouchPhase::Ended, point.x, point.y); app.update();
            let point = app.world().resource::<Layout>().config.center(Target::Button(gyro::BUTTON), size);
            touch(&mut app, w, 903, TouchPhase::Started, point.x, point.y); app.update();
            assert!(app.world().resource::<gyro::State>().active(app.world().resource::<gyro::Options>()),
                "{scenario}: fresh raw gyro route admitted");
            assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyW));
            assert!(app.world().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Middle));
        }
    }

    #[test]
    fn queued_utility_capture_precedes_same_frame_fire_run_lock_vats_and_analog() {
        let size=Vec2::new(640.,360.);
        for action in 0..4 {
            let (mut app,w)=mixed_hotbar_phone_scene(size,"classic",4);
            app.init_resource::<VatsPresses>().add_systems(Update,count_vats);
            app.world_mut().resource_mut::<movement::Options>().vats_taps=true;
            app.world_mut().resource_mut::<nvf_play::control::PlayerControl>().weapon_out=true;
            app.update();
            let look=Vec2::new(600.,180.);
            if action==2 {
                touch(&mut app,w,851,TouchPhase::Started,look.x,look.y);
                touch(&mut app,w,851,TouchPhase::Ended,look.x,look.y);app.update();
                assert_eq!(app.world().resource::<VatsPresses>().0,0);
            }
            if action==3 {
                let stick=app.world().resource::<Layout>().config.center(Target::Stick,size);
                touch(&mut app,w,852,TouchPhase::Started,stick.x,stick.y);app.update();
                touch(&mut app,w,852,TouchPhase::Moved,stick.x,stick.y-40.);app.update();
                assert!(native_velocity(&app)[1]>0.,"fixture must move before capture");
            }
            raw_key(&mut app,w,KeyCode::F12,ButtonState::Pressed);
            app.world_mut().write_message(hotbar::UtilityTrayRequest(true));
            match action {
                0=>{
                    let i=BUTTONS.iter().position(|b|b.control==Some(Control::Use)).unwrap();
                    let p=app.world().resource::<Layout>().config.center(Target::Button(i),size);
                    touch(&mut app,w,853,TouchPhase::Started,p.x,p.y);
                }
                1=>{
                    let p=movement::fixed_lock_rect(app.world().resource::<Layout>(),size).center();
                    touch(&mut app,w,853,TouchPhase::Started,p.x,p.y);
                    touch(&mut app,w,853,TouchPhase::Ended,p.x,p.y);
                }
                2=>{
                    touch(&mut app,w,853,TouchPhase::Started,look.x,look.y);
                    touch(&mut app,w,853,TouchPhase::Ended,look.x,look.y);
                }
                _=>{
                    let p=app.world().resource::<Layout>().config.center(Target::Stick,size);
                    touch(&mut app,w,852,TouchPhase::Moved,p.x,p.y-48.);
                }
            }
            app.update();
            assert!(hotbar::tray_open_collision_fixture(app.world()));
            assert!(app.world().resource::<TouchState>().ui_capture);
            let control=app.world().resource::<nvf_play::control::PlayerControl>();
            assert!(!control.trigger && !control.auto_move,"case{action}: queued capture leaked a touch action");
            assert_eq!(app.world().resource::<VatsPresses>().0,0,"case{action}: VATS history leaked");
            assert_eq!(native_velocity(&app),[0.;2],"case{action}: analog sample leaked");
            let hardware=app.world().resource::<ButtonInput<KeyCode>>();
            assert!(hardware.pressed(KeyCode::F12) && hardware.just_pressed(KeyCode::F12),"real physical edge must remain");
        }
    }

    #[test]
    fn missing_window_cancels_queued_tray_request_without_replaying_it_on_return() {
        let (mut app, w) = mixed_hotbar_phone_scene(Vec2::new(640., 360.), "classic", 4);
        raw_key(&mut app, w, KeyCode::F12, ButtonState::Pressed);
        app.update();
        assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::F12));
        app.world_mut().write_message(hotbar::UtilityTrayRequest(true));
        app.world_mut().entity_mut(w).remove::<PrimaryWindow>();
        app.update();
        assert!(!hotbar::tray_open_collision_fixture(app.world()));
        assert!(!app.world().resource::<nvf_play::pipboy::touch_items::OverlayCapture>().0);
        app.world_mut().entity_mut(w).insert(PrimaryWindow);
        app.update();
        app.update();
        assert!(!hotbar::tray_open_collision_fixture(app.world()), "old request cannot replay after window restoration");
        assert!(!app.world().resource::<TouchState>().ui_capture);
        assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::F12));
        app.world_mut().write_message(hotbar::UtilityTrayRequest(true));
        app.update();
        assert!(hotbar::tray_open_collision_fixture(app.world()), "a fresh stable-frame request still opens the tray");
        assert!(app.world().resource::<TouchState>().ui_capture);
        assert!(app.world().resource::<nvf_play::pipboy::touch_items::OverlayCapture>().0);
        assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::F12));
    }


    #[derive(Resource, Default)]
    struct DefaultMenuTouchSeen { presses: usize, releases: usize, scrolls: usize, cancels: usize }
    fn observe_default_menu_touch(
        mut events: MessageReader<bevy::picking::pointer::PointerInput>,
        mut seen: ResMut<DefaultMenuTouchSeen>,
    ) {
        use bevy::picking::pointer::PointerAction;
        for event in events.read().filter(|e| e.pointer_id == nvf_play::pipboy::touch_items::POINTER) {
            match event.action {
                PointerAction::Press(_) => seen.presses += 1,
                PointerAction::Release(_) => seen.releases += 1,
                PointerAction::Scroll { y, .. } if y != 0. => seen.scrolls += 1,
                PointerAction::Cancel => seen.cancels += 1,
                _ => {}
            }
        }
    }
    fn default_menu_touch_scene(page: nvf_play::pipboy::Page, bar_enabled: bool) -> (App, Entity) {
        use nvf_play::pipboy::{self, touch_scroll::TouchPointerPlugin};
        let (mut app, window) = scene();
        app.insert_resource(nvf_play::settings::SettingsFile(None));
        // Authored empty player inventory, no installed game content or production test helper.
        let mut forms = fnv_sim::FormStore::default();
        let base = fnv_sim::forms::BaseForm {
            id: fnv_sim::FormId::PLAYER_BASE, sig: *b"NPC_", edid: None, name: None,
            script: None, prompt: None, voice_filter: false,
            data: fnv_sim::forms::BaseData::Actor(fnv_sim::forms::ActorBase { entries: Vec::new(), attributes: None }),
        };
        forms.sigs.insert(base.id, base.sig);
        forms.bases.insert(base.id, base);
        let forms = Box::leak(Box::new(forms));
        let refs = Box::leak(Box::new(fnv_sim::RefTable::default()));
        let mut sim = nvf_play::sim::SimState {
            forms, refs, world: fnv_sim::World::new(forms, refs, fnv_sim::SimConfig::default()),
        };
        let menu = nvf_play::menu::model::MenuModel::open_inventory(&mut sim.world).unwrap();
        app.insert_resource(sim).insert_resource(nvf_play::menu::ActiveMenu(Some(menu)));
        let mut pip = pipboy::PipBoy::default();
        pip.phase = pipboy::Phase::Up; pip.frame = pipboy::Frame::Screen; pip.page = page;
        app.insert_resource(pip).init_resource::<pipboy::PipBoyAid>()
            .init_resource::<pipboy::PointerMoved>().init_resource::<nvf_play::hotkeys::Hotkeys>()
            .init_resource::<DefaultMenuTouchSeen>();
        let camera = app.world_mut().spawn_empty().id();
        app.insert_resource(pipboy::PipBoyScreen {
            image: Handle::default(), camera, on_model: false, pointer: None, scale: 1.,
        });
        app.world_mut().spawn((pipboy::FlatScreen,
            ComputedNode { size: Vec2::splat(1000.), ..default() }, UiGlobalTransform::from_xy(500., 500.)));
        app.add_plugins(TouchPointerPlugin)
            .add_systems(Last, pipboy::flat_pointer)
            .add_systems(Update, observe_default_menu_touch);
        app.world_mut().resource_mut::<Layout>().config = layout::Config::default();
        let row = app.world().resource::<nvf_play::settings::Settings>().index("touch-hotbar").unwrap();
        assert!(!(app.world().resource::<nvf_play::settings::Settings>().list()[row].get)(app.world()),
            "basic menu test must start with actual default-OFF preference");
        if bar_enabled { assert_eq!(nvf_play::settings::flip(app.world_mut(), row), Some(true)); }
        app.world_mut().resource_mut::<NextState<nvf_play::PlayState>>().set(nvf_play::PlayState::Menu);
        app.update(); app.update();
        (app, window)
    }
    fn default_menu_point(app: &mut App) -> Vec2 {
        let size = Vec2::splat(1000.);
        let tiles = mixed_hotbar_tiles(app);
        let config = &app.world().resource::<Layout>().config;
        for y in [0.35, 0.45, 0.55, 0.65] {
            for x in [0.35, 0.45, 0.55, 0.65] {
                let point = Vec2::new(x, y) * size;
                if config.hit(point, size, false, false).is_none()
                    && !layout::utility_rect(false, size).contains(point)
                    && !layout::utility_rect(true, size).contains(point)
                    && tiles.iter().all(|(_, r)| !r.contains(point)) { return point; }
            }
        }
        panic!("actual menu configuration must leave an unclaimed canvas point");
    }
    fn flush_default_menu_touch(app: &mut App) { for _ in 0..5 { app.update(); } }
    #[test]
    fn default_off_hotbar_preserves_raw_pipboy_taps_on_all_pages_and_existing_menu_controls() {
        use nvf_play::pipboy::{self, touch_items::TouchItems};
        for enabled in [false, true] {
            for page in pipboy::Page::ALL {
                let (mut app, w) = default_menu_touch_scene(page, enabled);
                assert_eq!(!mixed_hotbar_tiles(&mut app).is_empty(), enabled, "bar presentation remains opt-in");
                let point = default_menu_point(&mut app);
                touch(&mut app, w, 950, TouchPhase::Started, point.x, point.y);
                touch(&mut app, w, 950, TouchPhase::Ended, point.x, point.y);
                app.update();
                assert!(app.world().resource::<TouchItems>().owns(950), "raw wrapper reader must admit {page:?}, enabled={enabled}");
                assert!(app.world().resource::<TouchState>().contacts.is_empty(), "menu pointer must not become gameplay/look input");
                flush_default_menu_touch(&mut app);
                let seen = app.world().resource::<DefaultMenuTouchSeen>();
                assert_eq!((seen.presses, seen.releases, seen.scrolls), (1, 1, 0), "real pointer tap {page:?}, enabled={enabled}");
                assert!(app.world().resource::<nvf_play::hotkeys::Hotkeys>().0.iter().all(Option::is_none));
                assert_eq!(app.world().resource::<Messages<nvf_play::weapon_wheel::TouchHotkey>>().len(), 0);
                let i = BUTTONS.iter().position(|b| !b.playing && b.key == Some(KeyCode::Enter)).unwrap();
                let point = app.world().resource::<Layout>().config.center(Target::Button(i), Vec2::splat(1000.));
                touch(&mut app, w, 951, TouchPhase::Started, point.x, point.y); app.update();
                assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::Enter), "existing Select control still owns its hit target");
                assert!(!app.world().resource::<TouchItems>().owns(951));
            }
        }
    }
    #[test]
    fn default_off_hotbar_raw_vertical_swipes_reach_real_scroll_on_every_pipboy_page() {
        use nvf_play::pipboy;
        for page in pipboy::Page::ALL {
            let (mut app, w) = default_menu_touch_scene(page, false);
            // Data maps intentionally reject vertical wheel input. Exercise real
            // list tabs (Stats Skills / Data Notes), rather than map zoom.
            app.world_mut().resource_mut::<pipboy::PipBoy>().tabs[page.index()] = match page {
                pipboy::Page::Stats => 2,
                pipboy::Page::Items => 0,
                pipboy::Page::Data => 3,
            };
            app.update();
            let point = default_menu_point(&mut app);
            touch(&mut app, w, 960, TouchPhase::Started, point.x, point.y); app.update();
            touch(&mut app, w, 960, TouchPhase::Moved, point.x, point.y - 80.); app.update();
            touch(&mut app, w, 960, TouchPhase::Ended, point.x, point.y - 80.); app.update();
            flush_default_menu_touch(&mut app);
            let seen = app.world().resource::<DefaultMenuTouchSeen>();
            assert!(seen.scrolls > 0, "raw {page:?} swipe must reach real projected scroll with bar OFF");
            assert_eq!(seen.releases, 0, "scroll cannot become an item use/tab click");
            assert!(mixed_hotbar_tiles(&mut app).is_empty());
            assert!(app.world().resource::<nvf_play::hotkeys::Hotkeys>().0.iter().all(Option::is_none));
        }
    }
    #[test]
    fn default_off_menu_touch_session_changes_cancel_old_release_then_accept_a_fresh_contact() {
        use nvf_play::pipboy::{self, touch_items::TouchItems};
        for cause in 0..3 {
            let (mut app, w) = default_menu_touch_scene(pipboy::Page::Items, false);
            let point = default_menu_point(&mut app);
            touch(&mut app, w, 970, TouchPhase::Started, point.x, point.y); app.update(); app.update();
            assert_eq!(app.world().resource::<DefaultMenuTouchSeen>().presses, 1);
            match cause {
                0 => app.world_mut().resource_mut::<pipboy::PipBoy>().page = pipboy::Page::Stats,
                1 => app.world_mut().resource_mut::<pipboy::PipBoy>().tabs[pipboy::Page::Items.index()] = 1,
                _ => app.world_mut().resource_mut::<pipboy::PipBoy>().frame = pipboy::Frame::Device,
            }
            touch(&mut app, w, 970, TouchPhase::Ended, point.x, point.y); app.update();
            flush_default_menu_touch(&mut app);
            assert!(!app.world().resource::<TouchItems>().owns(970));
            let seen = app.world().resource::<DefaultMenuTouchSeen>();
            assert_eq!(seen.releases, 0, "old release must not cross session cause{cause}");
            assert!(seen.cancels > 0);
            // Restore the flat presentation only after the stale contact has been drained.
            app.world_mut().resource_mut::<pipboy::PipBoy>().frame = pipboy::Frame::Screen;
            app.update();
            touch(&mut app, w, 971, TouchPhase::Started, point.x, point.y);
            touch(&mut app, w, 971, TouchPhase::Ended, point.x, point.y); app.update();
            flush_default_menu_touch(&mut app);
            assert_eq!(app.world().resource::<DefaultMenuTouchSeen>().releases, 1, "fresh session cause{cause}");
        }
    }


    #[test]
    fn drawer_stays_hidden_through_real_practice_close_drain_during_return_to_playing() {
        use nvf_play::{pause::{Page, Paused}, settings};
        let (mut app, _) = controls_scene();
        app.world_mut().resource_mut::<drawer::Options>().enabled = true;
        app.update();
        assert_eq!(drawer_visibility(&mut app), Visibility::Inherited);
        app.insert_resource(Paused(true)).insert_resource(Page::Settings { selected: None });
        app.world_mut().resource_mut::<NextState<nvf_play::PlayState>>().set(nvf_play::PlayState::Menu);
        app.update();
        let row = app.world().resource::<settings::Settings>().index("touch-practice-v1").unwrap();
        assert!(settings::pick(app.world_mut(), row, "Open practice"));
        app.update();
        assert!(app.world().resource::<practice::Practice>().blocking());
        assert_eq!(drawer_visibility(&mut app), Visibility::Hidden);
        app.world_mut().resource_mut::<Paused>().0 = false;
        app.world_mut().resource_mut::<NextState<nvf_play::PlayState>>().set(nvf_play::PlayState::Playing);
        app.update();
        assert!(app.world().resource::<TouchState>().playing);
        assert!(app.world().resource::<practice::Practice>().blocking(), "practice close owns its drain frames");
        assert_eq!(drawer_visibility(&mut app), Visibility::Hidden, "returning gameplay must not paint a handle through the closing lesson");
        app.update();
        assert!(app.world().resource::<practice::Practice>().blocking());
        assert_eq!(drawer_visibility(&mut app), Visibility::Hidden);
        app.update(); app.update();
        assert!(!app.world().resource::<practice::Practice>().blocking());
        assert_eq!(drawer_visibility(&mut app), Visibility::Inherited, "fresh gameplay restores the visible handle");
    }

}
