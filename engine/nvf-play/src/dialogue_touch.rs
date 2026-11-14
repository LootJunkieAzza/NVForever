//! Touch dialogue view and release-only gestures [design: android-dialogue-accessibility.md].
use crate::dialogue::{ListId, ViewInput};
use crate::dialogue_menu::DialogueMenu;
use crate::ui_pointer::{Button, PointerState};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::ui::{CalculatedClip, ComputedUiTargetCamera, UiGlobalTransform};
use bevy::window::{AppLifecycle, PrimaryWindow, WindowFocused};
use std::collections::HashSet;

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct TouchDialogue {
    pub enabled: bool,
    pub choice_scale: f32,
    pub subtitle_scale: f32,
    pub confirm: bool,
}
impl Default for TouchDialogue {
    fn default() -> Self {
        Self {
            enabled: cfg!(target_os = "android"),
            choice_scale: 1.0,
            subtitle_scale: 1.0,
            confirm: true,
        }
    }
}
impl TouchDialogue {
    /// The conversation owns raw touches; the underlying virtual menu keys must sleep.
    pub fn captures(&self, state: crate::PlayState) -> bool {
        self.enabled && state == crate::PlayState::Dialogue
    }
}
fn scale(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.75, 2.0)
    } else {
        1.0
    }
}
fn part(v: f32) -> f32 {
    (scale(v) - 0.75) / 1.25
}
fn from_part(v: f32) -> f32 {
    scale(
        0.75 + if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.2
        } * 1.25,
    )
}
fn shown(v: f32) -> String {
    format!("{:.0}%", from_part(v) * 100.0)
}
fn mode(w: &World) -> String {
    if w.resource::<TouchDialogue>().confirm {
        "Select then confirm"
    } else {
        "Direct tap"
    }
    .into()
}
fn set_mode(w: &mut World, name: &str) -> bool {
    let confirm = match name {
        "Select then confirm" => true,
        "Direct tap" => false,
        _ => return false,
    };
    w.resource_mut::<TouchDialogue>().confirm = confirm;
    true
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<TouchDialogue>()
        .init_resource::<TouchFrame>()
        .init_resource::<Gestures>()
        .init_resource::<RevealRequest>()
        .add_message::<TouchInput>()
        .add_message::<WindowFocused>()
        .add_message::<AppLifecycle>()
        .add_systems(
            Update,
            read.before(crate::dialogue::input)
                .in_set(crate::PlaySet::Scripts),
        )
        .add_systems(
            Update,
            (draw, reveal_selected)
                .chain()
                .in_set(crate::PlaySet::Ui)
                .after(crate::dialogue_menu::DrawMenu),
        );
    use crate::settings::{add_choice_list, add_setting, ChoiceList, Setting, Slider};
    add_setting(app, Setting { key: "touch-dialogue", group: crate::pause::GROUP_MENUS, label: "Touch dialogue",
        note: "Wide wrapped dialogue choices, large targets and swipe scrolling. Uses the same offered choices and game actions. On by default on Android.",
        switches: "",
        get: |w| w.resource::<TouchDialogue>().enabled, set: |w,v| w.resource_mut::<TouchDialogue>().enabled=v, slider: None });
    add_setting(app, Setting { key: "dialogue-choice-text-size", group: crate::pause::GROUP_MENUS, label: "Dialogue choice text",
        note: "Choice text size in the touch dialogue view, independently of conversation subtitles (75-200%).",
        switches: "",
        get: |_| false, set: |_,_| {}, slider: Some(Slider { get: |w| part(w.resource::<TouchDialogue>().choice_scale),
            set: |w,v| w.resource_mut::<TouchDialogue>().choice_scale=from_part(v), min:0.0,step:0.04,shown:Some(shown) }) });
    add_setting(app, Setting { key: "dialogue-subtitle-text-size", group: crate::pause::GROUP_MENUS, label: "Dialogue subtitle text",
        note: "Speaking-line text size in the touch dialogue view (75-200%). Does not change choices or world HUD subtitles.",
        switches: "",
        get: |_| false, set: |_,_| {}, slider: Some(Slider { get: |w| part(w.resource::<TouchDialogue>().subtitle_scale),
            set: |w,v| w.resource_mut::<TouchDialogue>().subtitle_scale=from_part(v), min:0.0,step:0.04,shown:Some(shown) }) });
    add_choice_list(app, Setting { key:"dialogue-touch-choice",group:crate::pause::GROUP_MENUS,label:"Touch dialogue choices",
        note:"Select then confirm: tap a choice, then Confirm selection. Direct tap: a released tap answers. A swipe never answers. Keyboard/controller and real mouse keep their usual actions.",
        switches: "",
        get: |_|false,set:|_,_|{},slider:None }, ChoiceList { names: |_|vec!["Select then confirm".into(),"Direct tap".into()],
            chosen:mode,pick:set_mode,save:mode,load:set_mode,ask_keep:false });
}

#[derive(Resource, Default)]
pub(crate) struct TouchFrame {
    pub active: bool,
    pub asked: ViewInput,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Choice(usize),
    Confirm,
    Skip,
}
#[derive(Component)]
struct Action {
    kind: Kind,
    list: Option<ListId>,
}
#[derive(Component)]
struct Root;
#[derive(Component)]
struct Viewport;
#[derive(Component)]
struct Row {
    index: usize,
    short: bool,
}
#[derive(Component)]
struct ConfirmCaption;
#[derive(Resource, Default)]
struct RevealRequest(bool);
#[derive(Clone, PartialEq)]
struct Guard {
    window: Entity,
    list: Option<ListId>,
    settings: TouchDialogue,
    size: Vec2,
    density: f32,
    device: crate::bindings::InputDevice,
}
struct Gesture {
    id: u64,
    origin: Vec2,
    last: Vec2,
    target: Option<Kind>,
    scrolling: bool,
    in_view: bool,
    selection: usize,
}
#[derive(Resource)]
struct Gestures {
    guard: Option<Guard>,
    gesture: Option<Gesture>,
    contacts: HashSet<u64>,
    armed: Option<usize>,
    focused: bool,
    running: bool,
}
impl Default for Gestures {
    fn default() -> Self {
        Self {
            guard: None,
            gesture: None,
            contacts: HashSet::new(),
            armed: None,
            focused: true,
            running: true,
        }
    }
}
fn available(
    state: Option<&State<crate::PlayState>>,
    paused: Option<&crate::pause::Paused>,
    script: Option<&crate::script::ScriptState>,
    creator: Option<&crate::creator::CreatorMenu>,
) -> bool {
    state.is_some_and(|s| *s.get() == crate::PlayState::Dialogue)
        && !paused.is_some_and(|p| p.0)
        && !creator.is_some_and(|c| c.is_open())
        && !script
            .and_then(|s| s.rt.as_ref())
            .is_some_and(|rt| crate::dialogue::held(&rt.game))
}
type TargetData = (
    &'static Action,
    &'static ComputedNode,
    &'static UiGlobalTransform,
    Option<&'static InheritedVisibility>,
    Option<&'static CalculatedClip>,
    Option<&'static ComputedUiTargetCamera>,
);
fn ui_point(
    point: Vec2,
    target: Option<&ComputedUiTargetCamera>,
    cameras: &Query<&Camera>,
) -> Vec2 {
    point
        - target
            .and_then(|t| t.get())
            .and_then(|c| cameras.get(c).ok())
            .and_then(Camera::physical_viewport_rect)
            .map_or(Vec2::ZERO, |r| r.min.as_vec2())
}
fn hit(
    node: &ComputedNode,
    transform: UiGlobalTransform,
    visible: Option<&InheritedVisibility>,
    clip: Option<&CalculatedClip>,
    point: Vec2,
) -> bool {
    visible.is_none_or(|v| v.get())
        && node.contains_point(transform, point)
        && clip.is_none_or(|c| c.contains_point(point))
}
fn clamp_scroll(node: &ComputedNode, position: &mut ScrollPosition) {
    position.y = position.y.clamp(
        0.0,
        ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.0),
    );
}
type InputGates<'w> = (
    Option<Res<'w, crate::bindings::Bindings>>,
    Option<Res<'w, crate::pad::TouchDeviceSwitch>>,
    Option<Res<'w, crate::pad::PlatformInputGate>>,
);
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn read(
    settings: Res<TouchDialogue>,
    menu: Res<DialogueMenu>,
    (bindings, touch_switch, platform): InputGates<'_>,
    state: Option<Res<State<crate::PlayState>>>,
    paused: Option<Res<crate::pause::Paused>>,
    script: Option<Res<crate::script::ScriptState>>,
    creator: Option<Res<crate::creator::CreatorMenu>>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    targets: Query<(Entity, TargetData)>,
    pointers: Query<(Entity, Ref<PointerState>)>,
    mut viewport: Query<
        (
            &ComputedNode,
            &UiGlobalTransform,
            &mut ScrollPosition,
            Option<&ComputedUiTargetCamera>,
        ),
        With<Viewport>,
    >,
    mut touches: MessageReader<TouchInput>,
    mut lifecycle: (MessageReader<WindowFocused>, MessageReader<AppLifecycle>),
    mut gestures: ResMut<Gestures>,
    mut frame: ResMut<TouchFrame>,
    cameras: Query<&Camera>,
) {
    *frame = TouchFrame::default();
    let Ok((window_id, window)) = windows.single() else {
        gestures.gesture = None;
        gestures.armed = None;
        gestures.contacts.clear();
        touches.clear();
        return;
    };
    let mut interrupted = false;
    for event in lifecycle.0.read().filter(|e| e.window == window_id) {
        gestures.focused = event.focused;
        interrupted = true;
    }
    for event in lifecycle.1.read() {
        match event {
            AppLifecycle::Suspended => {
                gestures.running = false;
                interrupted = true;
            }
            AppLifecycle::Running => {
                gestures.running = true;
                interrupted = true;
            }
            _ => {}
        }
    }
    let enabled = settings.enabled
        && available(
            state.as_deref(),
            paused.as_deref(),
            script.as_deref(),
            creator.as_deref(),
        )
        && menu.view.is_some();
    frame.active = settings.enabled;
    let device = bindings
        .as_deref()
        .map_or(crate::bindings::InputDevice::KeyboardMouse, |b| b.device);
    let guard = Guard {
        window: window_id,
        list: menu.list.clone(),
        settings: *settings,
        size: Vec2::new(window.width(), window.height()),
        density: window.scale_factor(),
        device,
    };
    let fresh = gestures.guard.as_ref() != Some(&guard);
    gestures.guard = Some(guard);
    let events: Vec<_> = touches
        .read()
        .filter(|e| e.window == window_id)
        .cloned()
        .collect();
    let input_blocked = touch_switch.as_deref().is_some_and(|s| !s.allows_actions())
        || platform.as_deref().is_some_and(|p| {
            !p.allows_actions() || events.iter().any(|event| !p.allows_touch(event.id))
        });
    if !enabled
        || !window.focused
        || !gestures.focused
        || !gestures.running
        || interrupted
        || fresh
        || input_blocked
    {
        gestures.gesture = None;
        gestures.armed = None;
        if interrupted || !window.focused || !gestures.focused || !gestures.running || input_blocked
        {
            gestures.contacts.clear();
        }
        if input_blocked {
            return;
        }
        for event in events {
            if event.phase == TouchPhase::Started {
                gestures.contacts.insert(event.id);
            } else if matches!(event.phase, TouchPhase::Ended | TouchPhase::Canceled) {
                gestures.contacts.remove(&event.id);
            }
        }
        return;
    }
    frame.asked.list = menu.list.clone();
    let density = window.scale_factor();
    let target_at = |p: Vec2| {
        targets.iter().find_map(|(_, (a, n, t, v, c, camera))| {
            (a.list == menu.list && hit(n, *t, v, c, ui_point(p * density, camera, &cameras)))
                .then_some(a.kind)
        })
    };
    let mut touched = false;
    let mut drop_tail = false;
    for event in &events {
        touched = true;
        if !event.position.is_finite() {
            gestures.gesture = None;
            gestures.contacts.remove(&event.id);
            continue;
        }
        if device == crate::bindings::InputDevice::Controller {
            gestures.gesture = None;
            gestures.armed = None;
            match event.phase {
                TouchPhase::Started => {
                    gestures.contacts.insert(event.id);
                }
                TouchPhase::Ended | TouchPhase::Canceled => {
                    gestures.contacts.remove(&event.id);
                }
                TouchPhase::Moved => {}
            }
            continue;
        }
        match event.phase {
            TouchPhase::Started => {
                let fresh_contact = gestures.contacts.insert(event.id);
                if !fresh_contact || gestures.contacts.len() > 1 {
                    gestures.gesture = None;
                    continue;
                }
                let in_view = viewport.iter().any(|(n, t, _, camera)| {
                    n.contains_point(*t, ui_point(event.position * density, camera, &cameras))
                });
                gestures.gesture = Some(Gesture {
                    id: event.id,
                    origin: event.position,
                    last: event.position,
                    target: target_at(event.position),
                    scrolling: false,
                    in_view,
                    selection: menu.selected,
                });
            }
            TouchPhase::Moved => {
                let Some(g) = gestures.gesture.as_mut().filter(|g| g.id == event.id) else {
                    continue;
                };
                if event.position.distance(g.origin) > 8.0 {
                    g.scrolling = true;
                }
                if g.scrolling && g.in_view {
                    for (node, _, mut position, _) in &mut viewport {
                        position.y +=
                            (g.last.y - event.position.y) * density * node.inverse_scale_factor();
                        clamp_scroll(node, &mut position);
                    }
                }
                g.last = event.position;
            }
            TouchPhase::Canceled => {
                drop_tail = gestures.gesture.as_ref().is_some_and(|g| g.scrolling);
                gestures.contacts.remove(&event.id);
                gestures.gesture = None;
                if drop_tail {
                    break;
                }
            }
            TouchPhase::Ended => {
                gestures.contacts.remove(&event.id);
                let Some(g) = gestures.gesture.take().filter(|g| g.id == event.id) else {
                    continue;
                };
                if g.scrolling {
                    // Computed row transforms still describe the pre-scroll layout.
                    // All raw messages were collected above, so dropping this tail
                    // cannot replay a queued tap on the following frame.
                    drop_tail = true;
                    break;
                }
                if event.position.distance(g.origin) <= 8.0 && g.target == target_at(event.position)
                {
                    if let Some(kind) = g.target {
                        if kind != Kind::Confirm
                            || (gestures.armed == Some(menu.selected)
                                && g.selection == menu.selected)
                        {
                            activate(kind, true, &settings, &menu, &mut frame.asked);
                            if let Kind::Choice(i) = kind {
                                gestures.armed = Some(i);
                            }
                        }
                    }
                }
            }
        }
    }
    if drop_tail {
        gestures.gesture = None;
        gestures.contacts.clear();
        gestures.armed = None;
    }
    // Mouse feedback stays on the original press edge; touches use only the guarded release route.
    if !touched && gestures.gesture.is_none() && gestures.contacts.is_empty() {
        for (entity, pointer) in &pointers {
            if !pointer.is_changed() {
                continue;
            }
            let Ok((_, (action, _, _, _, _, _))) = targets.get(entity) else {
                continue;
            };
            if action.list != menu.list {
                continue;
            }
            match *pointer {
                PointerState::Pressed => {
                    activate(action.kind, false, &settings, &menu, &mut frame.asked)
                }
                PointerState::Hovered => {
                    if let Kind::Choice(i) = action.kind {
                        frame.asked.hover = Some(i);
                    }
                }
                PointerState::None => {}
            }
        }
    }
}
fn activate(
    kind: Kind,
    touch: bool,
    settings: &TouchDialogue,
    menu: &DialogueMenu,
    asked: &mut ViewInput,
) {
    if asked.pick.is_some() || asked.skip {
        return;
    }
    match kind {
        Kind::Choice(i) if menu.view.as_ref().is_some_and(|v| i < v.choices.len()) => {
            asked.hover = Some(i);
            if !touch || !settings.confirm {
                asked.pick = Some(i);
            }
        }
        Kind::Confirm
            if menu
                .view
                .as_ref()
                .is_some_and(|v| menu.selected < v.choices.len()) =>
        {
            asked.pick = Some(menu.selected)
        }
        Kind::Skip => asked.skip = true,
        _ => {}
    }
}

#[derive(Clone, PartialEq)]
struct DrawKey {
    view: crate::dialogue_menu::MenuView,
    list: Option<ListId>,
    settings: TouchDialogue,
}
fn row_colors(short: bool, selected: bool) -> (Color, Color) {
    if selected {
        (
            if short {
                Color::srgb(0.8, 0.2, 0.15)
            } else {
                Color::srgb(0.7, 0.95, 0.8)
            },
            Color::srgba(0.2, 0.4, 0.3, 0.65),
        )
    } else {
        (
            Color::srgb(0.15, 0.25, 0.22),
            Color::srgba(0.04, 0.08, 0.07, 0.5),
        )
    }
}
fn text(p: &mut ChildSpawnerCommands, value: impl Into<String>, size: f32, color: Color) {
    p.spawn((
        Text::new(value.into()),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
        Node {
            width: percent(100.0),
            ..default()
        },
    ));
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    menu: Res<DialogueMenu>,
    settings: Res<TouchDialogue>,
    roots: Query<Entity, With<Root>>,
    mut rows: Query<(&Row, &mut BorderColor, &mut BackgroundColor)>,
    mut shown: Local<Option<DrawKey>>,
    state: Option<Res<State<crate::PlayState>>>,
    paused: Option<Res<crate::pause::Paused>>,
    script: Option<Res<crate::script::ScriptState>>,
    creator: Option<Res<crate::creator::CreatorMenu>>,
    gestures: Res<Gestures>,
    mut captions: Query<&mut Text, With<ConfirmCaption>>,
    mut reveal: ResMut<RevealRequest>,
) {
    let confirm_label = if gestures.armed == Some(menu.selected) {
        "Confirm selection"
    } else {
        "Select a choice first"
    };
    let key = menu
        .view
        .as_ref()
        .filter(|_| {
            settings.enabled
                && available(
                    state.as_deref(),
                    paused.as_deref(),
                    script.as_deref(),
                    creator.as_deref(),
                )
        })
        .map(|v| DrawKey {
            view: v.clone(),
            list: menu.list.clone(),
            settings: *settings,
        });
    if key != *shown {
        reveal.0 = true;
        for root in &roots {
            commands.entity(root).despawn();
        }
        if let Some(key) = &key {
            let v = &key.view;
            commands
                .spawn((
                    Root,
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(2.0),
                        width: percent(96.0),
                        bottom: px(12.0),
                        height: percent(65.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(12.0)),
                        row_gap: px(8.0),
                        ..default()
                    },
                    GlobalZIndex(2400),
                    BackgroundColor(Color::srgba(0.015, 0.025, 0.025, 0.95)),
                ))
                .with_children(|p| {
                    text(p, &v.speaker, 18.0, Color::WHITE);
                    text(
                        p,
                        if v.choices.is_empty() {
                            "Swipe to read. Tap and release to continue."
                        } else if settings.confirm {
                            "Swipe to scroll. Tap a choice, then Confirm selection."
                        } else {
                            "Swipe to scroll. Tap and release a choice to answer."
                        },
                        14.0,
                        Color::srgb(0.7, 0.85, 0.8),
                    );
                    p.spawn((
                        Viewport,
                        Node {
                            width: percent(100.0),
                            flex_direction: FlexDirection::Column,
                            flex_grow: 1.0,
                            min_height: px(0.0),
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },
                        ScrollPosition::default(),
                    ))
                    .with_children(|p| {
                        p.spawn(Node {
                            width: percent(100.0),
                            flex_direction: FlexDirection::Column,
                            flex_shrink: 0.0,
                            row_gap: px(8.0),
                            ..default()
                        })
                        .with_children(|p| {
                            if let Some(line) = &v.line {
                                p.spawn((
                                    Button,
                                    Action {
                                        kind: Kind::Skip,
                                        list: menu.list.clone(),
                                    },
                                    Node {
                                        width: percent(100.0),
                                        min_height: px(56.0),
                                        padding: UiRect::all(px(10.0)),
                                        flex_shrink: 0.0,
                                        ..default()
                                    },
                                ))
                                .with_children(|p| {
                                    text(
                                        p,
                                        line,
                                        20.0 * scale(settings.subtitle_scale),
                                        Color::WHITE,
                                    )
                                });
                            }
                            for (index, row) in v.choices.iter().enumerate() {
                                p.spawn((
                                    Button,
                                    Action {
                                        kind: Kind::Choice(index),
                                        list: menu.list.clone(),
                                    },
                                    Row {
                                        index,
                                        short: row.short,
                                    },
                                    Node {
                                        width: percent(100.0),
                                        min_height: px(56.0),
                                        padding: UiRect::all(px(10.0)),
                                        border: UiRect::all(px(2.0)),
                                        flex_shrink: 0.0,
                                        ..default()
                                    },
                                    BorderColor::all(
                                        row_colors(row.short, index == menu.selected).0,
                                    ),
                                    BackgroundColor(
                                        row_colors(row.short, index == menu.selected).1,
                                    ),
                                ))
                                .with_children(|p| {
                                    text(
                                        p,
                                        &row.label,
                                        20.0 * scale(settings.choice_scale),
                                        if row.darkened {
                                            Color::srgb(0.55, 0.65, 0.6)
                                        } else {
                                            Color::srgb(0.9, 0.95, 0.85)
                                        },
                                    );
                                });
                            }
                        });
                    });
                    if settings.confirm && !v.choices.is_empty() {
                        p.spawn((
                            Button,
                            Action {
                                kind: Kind::Confirm,
                                list: menu.list.clone(),
                            },
                            Node {
                                width: percent(100.0),
                                min_height: px(56.0),
                                padding: UiRect::all(px(12.0)),
                                flex_shrink: 0.0,
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.12, 0.25, 0.22)),
                        ))
                        .with_children(|p| {
                            p.spawn((
                                ConfirmCaption,
                                Text::new(confirm_label),
                                TextFont {
                                    font_size: FontSize::Px(20.0),
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });
                    }
                });
        }
        *shown = key;
    }
    for (row, mut border, mut background) in &mut rows {
        let (line, fill) = row_colors(row.short, row.index == menu.selected);
        *border = BorderColor::all(line);
        background.0 = fill;
    }
    for mut caption in &mut captions {
        if caption.0 != confirm_label {
            caption.0 = confirm_label.into();
        }
    }
}
fn reveal_selected(
    menu: Res<DialogueMenu>,
    mut previous: Local<Option<(Option<ListId>, usize)>>,
    mut reveal: ResMut<RevealRequest>,
    rows: Query<(&Row, &ComputedNode, &UiGlobalTransform)>,
    mut viewport: Query<(&ComputedNode, &UiGlobalTransform, &mut ScrollPosition), With<Viewport>>,
) {
    let key = (menu.list.clone(), menu.selected);
    if !reveal.0 && previous.as_ref() == Some(&key) {
        return;
    }
    let Some((_, row, transform)) = rows
        .iter()
        .find(|(r, n, _)| r.index == menu.selected && !n.is_empty())
    else {
        return;
    };
    for (view, t, mut scroll) in &mut viewport {
        let top = t.transform_point2(Vec2::ZERO).y - view.size().y * 0.5;
        let bottom = top + view.size().y;
        let row_top = transform.transform_point2(Vec2::ZERO).y - row.size().y * 0.5;
        let delta = if row_top < top {
            row_top - top
        } else {
            (row_top + row.size().y - bottom).max(0.0)
        };
        scroll.y += delta * view.inverse_scale_factor();
        clamp_scroll(view, &mut scroll);
    }
    *previous = Some(key);
    reveal.0 = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialogue_menu::{ChoiceRow, MenuView};
    fn scene(confirm: bool) -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::input::InputPlugin,
            bevy::state::app::StatesPlugin,
        ))
        .insert_state(crate::PlayState::Dialogue)
        .init_resource::<crate::bindings::Bindings>()
        .insert_resource(TouchDialogue {
            enabled: true,
            confirm,
            ..default()
        })
        .init_resource::<DialogueMenu>();
        install(&mut app);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    resolution: bevy::window::WindowResolution::new(600, 600),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        let list = Some(ListId::for_test(1, &[10, 20, 30]));
        {
            let mut menu = app.world_mut().resource_mut::<DialogueMenu>();
            menu.view = Some(MenuView {
                speaker: "Speaker".into(),
                choices: vec![
                    ChoiceRow {
                        label: "First".into(),
                        ..default()
                    },
                    ChoiceRow {
                        label: "Second".into(),
                        short: true,
                        ..default()
                    },
                    ChoiceRow {
                        label: "Third".into(),
                        darkened: true,
                        ..default()
                    },
                ],
                ..default()
            });
            menu.list = list.clone();
        }
        for (kind, y) in [
            (Kind::Choice(2), 100.0),
            (Kind::Confirm, 200.0),
            (Kind::Skip, 300.0),
        ] {
            app.world_mut().spawn((
                Action {
                    kind,
                    list: list.clone(),
                },
                ComputedNode {
                    size: Vec2::new(300.0, 56.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(200.0, y),
                PointerState::None,
            ));
        }
        let viewport = app
            .world_mut()
            .spawn((
                Viewport,
                ComputedNode {
                    size: Vec2::splat(400.0),
                    content_size: Vec2::new(400.0, 1000.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(200.0, 200.0),
                ScrollPosition::default(),
            ))
            .id();
        app.update();
        app.update();
        (app, window, viewport)
    }
    fn event(app: &mut App, window: Entity, id: u64, phase: TouchPhase, x: f32, y: f32) {
        app.world_mut().write_message(TouchInput {
            window,
            id,
            phase,
            position: Vec2::new(x, y),
            force: None,
        });
    }
    fn tap(app: &mut App, window: Entity, id: u64, y: f32) {
        event(app, window, id, TouchPhase::Started, 200.0, y);
        event(app, window, id, TouchPhase::Ended, 200.0, y);
        app.update();
    }
    fn asked(app: &App) -> ViewInput {
        app.world().resource::<TouchFrame>().asked.clone()
    }
    #[test]
    fn direct_touch_chooses_on_release_and_keeps_the_offered_index() {
        let (mut app, w, _) = scene(false);
        event(&mut app, w, 1, TouchPhase::Started, 200.0, 100.0);
        app.update();
        assert_eq!(asked(&app).pick, None);
        event(&mut app, w, 1, TouchPhase::Ended, 200.0, 100.0);
        app.update();
        assert_eq!((asked(&app).hover, asked(&app).pick), (Some(2), Some(2)));
        let list = app.world().resource::<DialogueMenu>().list.clone();
        assert_eq!(asked(&app).for_list(list.as_ref()).pick, Some(2));
        assert_eq!(
            asked(&app)
                .for_list(Some(&ListId::for_test(2, &[10, 20, 30])))
                .pick,
            None,
            "runtime stamp gate still rejects replacement lists"
        );
    }
    #[test]
    fn select_then_confirm_needs_a_separate_released_confirmation() {
        let (mut app, w, _) = scene(true);
        tap(&mut app, w, 9, 200.0);
        assert_eq!(asked(&app).pick, None, "touch confirmation starts unarmed");
        tap(&mut app, w, 1, 100.0);
        assert_eq!((asked(&app).hover, asked(&app).pick), (Some(2), None));
        app.world_mut().resource_mut::<DialogueMenu>().selected = 2;
        tap(&mut app, w, 2, 200.0);
        assert_eq!(asked(&app).pick, Some(2));
    }
    #[test]
    fn confirmation_cannot_follow_a_keyboard_selection_change_while_held() {
        let (mut app, w, _) = scene(true);
        tap(&mut app, w, 1, 100.0);
        app.world_mut().resource_mut::<DialogueMenu>().selected = 2;
        event(&mut app, w, 2, TouchPhase::Started, 200.0, 200.0);
        app.update();
        app.world_mut().resource_mut::<DialogueMenu>().selected = 0;
        event(&mut app, w, 2, TouchPhase::Ended, 200.0, 200.0);
        app.update();
        assert_eq!(asked(&app).pick, None);
    }
    #[test]
    fn pause_and_dialogue_entry_do_not_reuse_a_held_contact() {
        let (mut app, w, _) = scene(false);
        event(&mut app, w, 1, TouchPhase::Started, 200.0, 100.0);
        app.update();
        app.insert_resource(crate::pause::Paused(true));
        app.update();
        app.world_mut().resource_mut::<crate::pause::Paused>().0 = false;
        event(&mut app, w, 1, TouchPhase::Ended, 200.0, 100.0);
        app.update();
        assert_eq!(asked(&app).pick, None);
        app.insert_state(crate::PlayState::Playing);
        event(&mut app, w, 2, TouchPhase::Started, 200.0, 100.0);
        app.update();
        app.insert_state(crate::PlayState::Dialogue);
        event(&mut app, w, 2, TouchPhase::Ended, 200.0, 100.0);
        app.update();
        assert_eq!(asked(&app).pick, None);
        tap(&mut app, w, 3, 100.0);
        assert_eq!(asked(&app).pick, Some(2));
    }
    #[test]
    fn speaking_line_skips_only_with_a_fresh_released_tap() {
        let (mut app, w, _) = scene(false);
        {
            let mut menu = app.world_mut().resource_mut::<DialogueMenu>();
            let v = menu.view.as_mut().unwrap();
            v.choices.clear();
            v.line = Some("An offered response".into());
        }
        app.update();
        tap(&mut app, w, 1, 300.0);
        assert!(asked(&app).skip);
        event(&mut app, w, 2, TouchPhase::Started, 200.0, 300.0);
        app.update();
        app.world_mut().resource_mut::<DialogueMenu>().list = Some(ListId::for_test(9, &[]));
        event(&mut app, w, 2, TouchPhase::Ended, 200.0, 300.0);
        app.update();
        assert!(!asked(&app).skip);
    }
    #[test]
    fn keyboard_selection_reveals_a_row_once_without_fighting_manual_scroll() {
        let (mut app, _, viewport) = scene(false);
        app.world_mut().spawn((
            Row {
                index: 2,
                short: false,
            },
            ComputedNode {
                size: Vec2::new(300.0, 56.0),
                ..default()
            },
            UiGlobalTransform::from_xy(200.0, 650.0),
        ));
        app.world_mut().resource_mut::<DialogueMenu>().selected = 2;
        app.update();
        assert!(app.world().get::<ScrollPosition>(viewport).unwrap().y > 0.0);
        app.world_mut()
            .get_mut::<ScrollPosition>(viewport)
            .unwrap()
            .y = 40.0;
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(viewport).unwrap().y, 40.0);
        assert_eq!(app.world().resource::<DialogueMenu>().selected, 2);
    }
    #[test]
    fn swiping_scrolls_without_answering_even_after_returning_to_the_row() {
        let (mut app, w, viewport) = scene(false);
        event(&mut app, w, 1, TouchPhase::Started, 200.0, 100.0);
        event(&mut app, w, 1, TouchPhase::Moved, 200.0, 30.0);
        app.update();
        assert!(app.world().get::<ScrollPosition>(viewport).unwrap().y > 0.0);
        event(&mut app, w, 1, TouchPhase::Moved, 200.0, 100.0);
        event(&mut app, w, 1, TouchPhase::Ended, 200.0, 100.0);
        app.update();
        assert_eq!((asked(&app).hover, asked(&app).pick), (None, None));
    }
    #[test]
    fn canceled_and_multifinger_contacts_never_answer() {
        let (mut app, w, _) = scene(false);
        event(&mut app, w, 1, TouchPhase::Started, 200.0, 100.0);
        event(&mut app, w, 2, TouchPhase::Started, 200.0, 100.0);
        event(&mut app, w, 1, TouchPhase::Ended, 200.0, 100.0);
        event(&mut app, w, 3, TouchPhase::Started, 200.0, 100.0);
        event(&mut app, w, 2, TouchPhase::Ended, 200.0, 100.0);
        event(&mut app, w, 3, TouchPhase::Ended, 200.0, 100.0);
        app.update();
        assert_eq!(asked(&app).pick, None);
        event(&mut app, w, 4, TouchPhase::Started, 200.0, 100.0);
        event(&mut app, w, 4, TouchPhase::Canceled, 200.0, 100.0);
        app.update();
        assert_eq!(asked(&app).pick, None);
        tap(&mut app, w, 5, 100.0);
        assert_eq!(asked(&app).pick, Some(2));
    }
    #[test]
    fn entry_and_new_response_require_a_fresh_contact() {
        let (mut app, w, _) = scene(false);
        event(&mut app, w, 1, TouchPhase::Started, 200.0, 100.0);
        app.update();
        app.world_mut().resource_mut::<DialogueMenu>().list =
            Some(ListId::for_test(2, &[10, 20, 30]));
        event(&mut app, w, 1, TouchPhase::Ended, 200.0, 100.0);
        app.update();
        assert_eq!(asked(&app).pick, None);
        // The old view's targets remain stamped for the old list until drawing replaces them.
        tap(&mut app, w, 2, 100.0);
        assert_eq!(asked(&app).pick, None);
        let list = app.world().resource::<DialogueMenu>().list.clone();
        let mut q = app.world_mut().query::<&mut Action>();
        for mut a in q.iter_mut(app.world_mut()) {
            a.list = list.clone();
        }
        tap(&mut app, w, 3, 100.0);
        assert_eq!(asked(&app).pick, Some(2));
    }
    #[test]
    fn a_release_on_another_target_or_outside_does_not_answer() {
        let (mut app, w, _) = scene(false);
        for (id, x, y) in [(1, 200.0, 200.0), (2, 590.0, 100.0)] {
            event(&mut app, w, id, TouchPhase::Started, 200.0, 100.0);
            event(&mut app, w, id, TouchPhase::Ended, x, y);
            app.update();
            assert_eq!(asked(&app).pick, None);
        }
    }
    #[test]
    fn focus_suspend_resize_and_mode_changes_cancel_pending_releases() {
        for cause in 0..4 {
            let (mut app, w, _) = scene(false);
            event(&mut app, w, 1, TouchPhase::Started, 200.0, 100.0);
            app.update();
            match cause {
                0 => {
                    app.world_mut().write_message(WindowFocused {
                        window: w,
                        focused: false,
                    });
                    app.world_mut().write_message(WindowFocused {
                        window: w,
                        focused: true,
                    });
                }
                1 => {
                    app.world_mut().write_message(AppLifecycle::Suspended);
                    app.world_mut().write_message(AppLifecycle::Running);
                }
                2 => app
                    .world_mut()
                    .get_mut::<Window>(w)
                    .unwrap()
                    .resolution
                    .set(700.0, 600.0),
                _ => app.world_mut().resource_mut::<TouchDialogue>().choice_scale = 1.5,
            }
            event(&mut app, w, 1, TouchPhase::Ended, 200.0, 100.0);
            app.update();
            assert_eq!(asked(&app).pick, None, "cause {cause}");
            tap(&mut app, w, 2, 100.0);
            assert_eq!(asked(&app).pick, Some(2), "fresh contact after {cause}");
        }
    }
    #[test]
    fn controller_ownership_and_first_returning_touch_cannot_answer() {
        let (mut app, w, _) = scene(false);
        app.world_mut()
            .resource_mut::<crate::bindings::Bindings>()
            .device = crate::bindings::InputDevice::Controller;
        app.update();
        tap(&mut app, w, 1, 100.0);
        assert_eq!(asked(&app).pick, None);
        app.world_mut()
            .resource_mut::<crate::bindings::Bindings>()
            .device = crate::bindings::InputDevice::KeyboardMouse;
        tap(&mut app, w, 2, 100.0);
        assert_eq!(asked(&app).pick, None);
        tap(&mut app, w, 3, 100.0);
        assert_eq!(asked(&app).pick, Some(2));
    }
    #[test]
    fn clipping_and_physical_density_control_actual_hit_testing() {
        let (mut app, w, _) = scene(false);
        app.world_mut()
            .get_mut::<Window>(w)
            .unwrap()
            .resolution
            .set_scale_factor_override(Some(2.0));
        app.update();
        event(&mut app, w, 1, TouchPhase::Started, 100.0, 50.0);
        event(&mut app, w, 1, TouchPhase::Ended, 100.0, 50.0);
        app.update();
        assert_eq!(
            asked(&app).pick,
            Some(2),
            "logical point becomes physical node coordinates once"
        );
        let mut q = app.world_mut().query::<Entity>();
        let ids: Vec<_> = q
            .iter(app.world())
            .filter(|e| {
                app.world()
                    .get::<Action>(*e)
                    .is_some_and(|a| a.kind == Kind::Choice(2))
            })
            .collect();
        for e in ids {
            app.world_mut()
                .entity_mut(e)
                .insert(CalculatedClip::FullyClipped);
        }
        event(&mut app, w, 2, TouchPhase::Started, 100.0, 50.0);
        event(&mut app, w, 2, TouchPhase::Ended, 100.0, 50.0);
        app.update();
        assert_eq!(asked(&app).pick, None);
    }
    #[test]
    fn real_mouse_press_keeps_direct_choice_even_in_confirm_touch_mode() {
        let (mut app, _, _) = scene(true);
        let mut q = app.world_mut().query::<(&Action, &mut PointerState)>();
        for (a, mut p) in q.iter_mut(app.world_mut()) {
            if a.kind == Kind::Choice(2) {
                *p = PointerState::Pressed;
            }
        }
        app.update();
        assert_eq!(asked(&app).pick, Some(2));
    }
    #[test]
    fn controller_pointer_feedback_keeps_the_existing_choice_route() {
        let (mut app, _, _) = scene(true);
        app.world_mut()
            .resource_mut::<crate::bindings::Bindings>()
            .device = crate::bindings::InputDevice::Controller;
        app.update();
        let mut q = app.world_mut().query::<(&Action, &mut PointerState)>();
        for (a, mut pointer) in q.iter_mut(app.world_mut()) {
            if a.kind == Kind::Choice(2) {
                *pointer = PointerState::Pressed;
            }
        }
        app.update();
        assert_eq!(asked(&app).pick, Some(2));
    }
    #[test]
    fn renderer_keeps_full_choice_text_order_and_independent_sizes() {
        let (mut app, _, _) = scene(true);
        let long =
            "A long offered choice that must remain complete and wrap within the full-width row. "
                .repeat(6);
        app.world_mut()
            .resource_mut::<DialogueMenu>()
            .view
            .as_mut()
            .unwrap()
            .choices[1]
            .label = long.clone();
        app.world_mut().resource_mut::<TouchDialogue>().choice_scale = 1.5;
        app.update();
        let mut q = app.world_mut().query::<(&Text, &TextFont, &TextLayout)>();
        assert!(q.iter(app.world()).any(|(t, f, l)| t.0 == long
            && f.font_size == FontSize::Px(30.0)
            && l.linebreak == bevy::text::LineBreak::WordOrCharacter));
        let mut q = app.world_mut().query::<(&Row, &Node)>();
        let mut rows: Vec<_> = q
            .iter(app.world())
            .map(|(r, n)| (r.index, n.width, n.min_height))
            .collect();
        rows.sort_by_key(|r| r.0);
        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .enumerate()
            .all(|(i, r)| r.0 == i && r.1 == percent(100.0) && r.2 == px(56.0)));
        {
            let mut menu = app.world_mut().resource_mut::<DialogueMenu>();
            let v = menu.view.as_mut().unwrap();
            v.choices.clear();
            v.line = Some("A speaking subtitle".into());
        }
        app.world_mut()
            .resource_mut::<TouchDialogue>()
            .subtitle_scale = 0.75;
        app.update();
        let mut q = app.world_mut().query::<(&Text, &TextFont)>();
        assert!(q
            .iter(app.world())
            .any(|(t, f)| t.0 == "A speaking subtitle" && f.font_size == FontSize::Px(15.0)));
    }
    #[test]
    fn settings_roundtrip_through_the_existing_ini_flow() {
        let path = std::env::temp_dir().join(format!(
            "nvf-dialogue-touch-{}-{}.ini",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = App::new();
        install(&mut app);
        app.insert_resource(crate::settings::SettingsFile(Some(path.clone())));
        crate::settings::apply_stored(app.world_mut());
        let index = |app: &App, key: &str| {
            app.world()
                .resource::<crate::settings::Settings>()
                .index(key)
                .unwrap()
        };
        let i = index(&app, "dialogue-choice-text-size");
        crate::settings::set_slider(app.world_mut(), i, 1.0);
        let i = index(&app, "dialogue-subtitle-text-size");
        crate::settings::set_slider(app.world_mut(), i, 0.0);
        let i = index(&app, "dialogue-touch-choice");
        assert!(crate::settings::pick(app.world_mut(), i, "Direct tap"));
        let before = *app.world().resource::<TouchDialogue>();
        let mut loaded = App::new();
        install(&mut loaded);
        loaded.insert_resource(crate::settings::SettingsFile(Some(path.clone())));
        crate::settings::apply_stored(loaded.world_mut());
        assert_eq!(*loaded.world().resource::<TouchDialogue>(), before);
        std::fs::remove_file(path).unwrap();
        for value in [f32::NAN, f32::INFINITY, -100.0, 100.0] {
            assert!((0.75..=2.0).contains(&from_part(value)));
        }
    }
    #[test]
    fn actual_device_selector_consumes_the_return_touch_before_dialogue_can_answer() {
        let (mut app, w, _) = scene(false);
        app.init_resource::<crate::pad::TouchDeviceSwitch>()
            .add_plugins(crate::pad::InputSwitchPlugin);
        app.world_mut()
            .resource_mut::<crate::bindings::Bindings>()
            .device = crate::bindings::InputDevice::Controller;
        app.update();
        tap(&mut app, w, 91, 100.0);
        assert_eq!(
            app.world().resource::<crate::bindings::Bindings>().device,
            crate::bindings::InputDevice::KeyboardMouse
        );
        assert!(!app
            .world()
            .resource::<crate::pad::TouchDeviceSwitch>()
            .allows_actions());
        assert_eq!(asked(&app).pick, None);
        tap(&mut app, w, 92, 100.0);
        assert_eq!(asked(&app).pick, Some(2));
    }
    #[test]
    fn platform_touch_quarantine_rejects_a_duplicate_start_and_release_after_loading() {
        let (mut app, w, _) = scene(false);
        app.init_resource::<crate::pad::PlatformInputGate>()
            .add_plugins(crate::pad::InputSwitchPlugin);
        app.world_mut()
            .resource_mut::<crate::pad::PlatformInputGate>()
            .set_blocked(true);
        event(&mut app, w, 81, TouchPhase::Started, 200.0, 100.0);
        app.update();
        assert_eq!(asked(&app).pick, None);
        app.world_mut()
            .resource_mut::<crate::pad::PlatformInputGate>()
            .set_blocked(false);
        event(&mut app, w, 81, TouchPhase::Started, 200.0, 100.0);
        event(&mut app, w, 81, TouchPhase::Ended, 200.0, 100.0);
        app.update();
        assert!(app
            .world()
            .resource::<crate::pad::PlatformInputGate>()
            .allows_actions());
        assert!(!app
            .world()
            .resource::<crate::pad::PlatformInputGate>()
            .allows_touch(81));
        assert_eq!(asked(&app).pick, None);
        app.update();
        assert_eq!(asked(&app).pick, None);
        tap(&mut app, w, 82, 100.0);
        assert_eq!(asked(&app).pick, Some(2));
    }
    #[test]
    fn a_resume_frame_cannot_answer_with_an_entire_new_queued_tap() {
        let (mut app, w, _) = scene(false);
        app.world_mut().write_message(AppLifecycle::Suspended);
        app.update();
        app.world_mut().write_message(AppLifecycle::Running);
        event(&mut app, w, 61, TouchPhase::Started, 200.0, 100.0);
        event(&mut app, w, 61, TouchPhase::Ended, 200.0, 100.0);
        app.update();
        assert_eq!(asked(&app).pick, None);
        tap(&mut app, w, 62, 100.0);
        assert_eq!(asked(&app).pick, Some(2));
    }
    #[test]
    fn scrolling_drains_a_same_batch_following_tap_and_never_replays_a_stale_row_pick() {
        for finish in [TouchPhase::Ended, TouchPhase::Canceled] {
            let (mut app, w, viewport) = scene(false);
            event(&mut app, w, 51, TouchPhase::Started, 200.0, 100.0);
            event(&mut app, w, 51, TouchPhase::Moved, 200.0, 40.0);
            event(&mut app, w, 51, finish, 200.0, 40.0);
            event(&mut app, w, 52, TouchPhase::Started, 200.0, 100.0);
            event(&mut app, w, 52, TouchPhase::Ended, 200.0, 100.0);
            app.update();
            assert!(app.world().get::<ScrollPosition>(viewport).unwrap().y > 0.0);
            assert_eq!(asked(&app).pick, None);
            app.update();
            assert_eq!(asked(&app).pick, None);
            // Publish the next layout's row under the same pixel without changing the list.
            let mut q = app.world_mut().query::<&mut Action>();
            for mut action in q.iter_mut(app.world_mut()) {
                if action.kind == Kind::Choice(2) {
                    action.kind = Kind::Choice(1);
                }
            }
            tap(&mut app, w, 53, 100.0);
            assert_eq!(asked(&app).pick, Some(1));
        }
    }
}
