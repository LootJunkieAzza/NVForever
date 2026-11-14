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

mod icons;
mod layout;
use layout::{Layout, Target};

pub struct TouchControlsPlugin;
impl Plugin for TouchControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TouchState>()
            .init_resource::<Layout>()
            .init_resource::<Assets<Image>>()
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
            .add_systems(Startup, (layout::load, labels).chain())
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
    move_deadzone: Vec2,
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
    mut layout: ResMut<Layout>,
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
    state.move_deadzone = Vec2::splat(layout.config.diameter(Target::Stick, size) * 0.09) / size;
    let mut pause = just_suspended && playing;
    sync_touch(&mut state, bindings.as_deref(), pause);
    for t in touches.read() {
        if t.window != window_id {
            continue;
        }
        let p = t.position / size;
        if layout.editor.is_some() {
            layout.event(t, size);
            state.contacts.clear();
            sync_touch(&mut state, bindings.as_deref(), pause);
            continue;
        }
        if t.phase == TouchPhase::Started && !suspended {
            if layout::utility_rect(false, size).contains(t.position) {
                layout.open(playing);
                state.contacts.clear();
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
            TouchPhase::Started if !suspended => {
                let role = match layout.config.hit(t.position, size, playing, false) {
                    Some(Target::Button(i)) => Some(Role::Button(i)),
                    Some(Target::Stick) => Some(Role::Move),
                    None if playing && layout.config.floating && layout.config.movement_area(p) => {
                        Some(Role::Move)
                    }
                    None if playing && layout.config.look_area(p) => Some(Role::Look),
                    _ => None,
                };
                let role = role.filter(|role| {
                    !state.contacts.values().any(|c| {
                        matches!(
                            (role, c.role),
                            (Role::Move, Role::Move) | (Role::Look, Role::Look)
                        )
                    })
                });
                if let Some(role) = role {
                    state.contacts.insert(
                        t.id,
                        Contact {
                            origin: if matches!(role, Role::Move) && !layout.config.floating {
                                layout.config.center(Target::Stick, size) / size
                            } else {
                                p
                            },
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

#[derive(Component, Clone, Copy)]
enum TouchVisual {
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
}
fn labels(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    for (i, b) in BUTTONS.iter().enumerate() {
        let image = images.add(icons::image(i));
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
                p.spawn((
                    ImageNode::new(image),
                    TouchVisual::Glyph,
                    Node {
                        width: percent(66.0),
                        height: percent(66.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ));
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
            ImageNode::new(images.add(icons::image(index))),
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
    state: Res<TouchState>,
    layout: Res<Layout>,
    window: Query<&Window, With<PrimaryWindow>>,
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
                rect = Some(Rect::from_center_size(
                    layout.config.center(target, size),
                    Vec2::splat(d),
                ));
                let pressed = state
                    .contacts
                    .values()
                    .any(|c| matches!(c.role,Role::Button(j) if j==i));
                let selected = layout.editor.as_ref().is_some_and(|e| e.selected == target);
                if let Some(bg) = background.as_deref_mut() {
                    bg.0 = if pressed || selected {
                        Color::srgba(0.18, 0.55, 0.43, 0.9)
                    } else {
                        Color::srgba(
                            0.02,
                            0.06,
                            0.07,
                            if layout.config.buttons[i].visible {
                                layout.config.opacity
                            } else {
                                0.2
                            },
                        )
                    };
                }
                BUTTONS[i].playing == playing && (editing || layout.config.buttons[i].visible)
            }
            TouchVisual::Caption => editing || layout.config.hints,
            TouchVisual::Glyph => true,
            TouchVisual::Stick => {
                rect = Some(Rect::from_center_size(
                    stick_center,
                    Vec2::splat(stick_diameter),
                ));
                playing
            }
            TouchVisual::Knob => {
                let offset = movement.map_or(Vec2::ZERO, |c| {
                    ((c.position - c.origin) * size).clamp_length_max(stick_diameter * 0.32)
                });
                rect = Some(Rect::from_center_size(
                    stick_center + offset,
                    Vec2::splat(stick_diameter * 0.35),
                ));
                playing
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
                    text.0=format!("{} thumb: drag the stick to move. {} thumb: swipe empty space to look.\nHold an icon to act. ? toggles labels. The cog edits your controls.",if layout.config.stick.center.x > 0.5 {"Right"} else {"Left"},if layout.config.stick.center.x > 0.5 {"Left"} else {"Right"});
                }
                !editing && layout.config.hints
            }
            TouchVisual::EditorPanel => {
                let first = layout::editor_rect(0, size);
                let last = layout::editor_rect(11, size);
                rect = Some(Rect::from_corners(
                    first.min - Vec2::splat(4.0),
                    last.max + Vec2::splat(4.0),
                ));
                editing
            }
            TouchVisual::EditorAction(i) => {
                rect = Some(layout::editor_rect(i, size));
                if let Some(mut text) = text {
                    text.0 = match i {
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
                    };
                }
                editing
            }
            TouchVisual::EditorMessage => {
                if let Some(mut text) = text {
                    text.0 = layout.editor.as_ref().map_or_else(
                        || layout.message.clone(),
                        |e| {
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
                image.color = Color::WHITE.with_alpha(alpha);
            }
            if let Some(mut border) = border {
                *border = BorderColor::all(Color::srgb(0.7, 0.9, 0.85).with_alpha(alpha));
            }
            if let Some(mut color) = text_color {
                color.0 = Color::WHITE.with_alpha(alpha);
            }
            if matches!(visual, TouchVisual::Stick | TouchVisual::Knob) {
                if let Some(bg) = background.as_deref_mut() {
                    bg.0 = Color::srgba(
                        0.04,
                        0.1,
                        0.1,
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
        *visibility = if visible {
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
}
