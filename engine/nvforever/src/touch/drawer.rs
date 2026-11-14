//! Optional, owned bottom handle -> the existing OpenInventory admission route.
//! [design] Only the handle starts a gesture. System edges, look, controls and item
//! taps are never global swipe listeners. Tray/VATS recognition is a later adapter.
use bevy::{
    ecs::system::SystemParam,
    input::touch::{TouchInput, TouchPhase},
    prelude::*,
    window::PrimaryWindow,
};
use nvf_play::settings::{self, Setting, Slider};
use std::collections::HashSet;

#[derive(Resource, Clone, Copy)]
pub(super) struct Options {
    pub enabled: bool,
    pub threshold: f32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: 96.0,
        }
    }
}
#[derive(Resource)]
pub(super) struct SafeBottom(pub f32);
impl Default for SafeBottom {
    fn default() -> Self {
        Self(24.0)
    }
}
#[derive(Clone, Copy)]
struct Drag {
    id: u64,
    origin: Vec2,
    position: Vec2,
    size: Vec2,
    canceled: bool,
}
#[derive(Resource, Default)]
pub(super) struct Drawer {
    drag: Option<Drag>,
    swallowed: HashSet<u64>,
    focused: Option<bool>,
}

pub(super) fn handle(size: Vec2, bottom: f32) -> Rect {
    let width = (size.x - 32.0).clamp(48.0, 144.0);
    Rect::from_center_size(
        Vec2::new(size.x * 0.5, size.y - bottom.max(0.0) - 36.0),
        Vec2::new(width, 48.0),
    )
}
impl Drawer {
    pub fn set_focus(&mut self, focused: bool) {
        self.focused = Some(focused);
    }
    pub fn cancel(&mut self) {
        self.drag = None;
        self.swallowed.clear();
    }
    fn progress(&self) -> f32 {
        self.drag
            .map_or(0.0, |d| (d.origin.y - d.position.y).max(0.0))
    }
    /// (consumed, open request). A canceled horizontal drag cannot later become a
    /// vertical swipe, and another finger cannot inherit the original finger's role.
    fn event(
        &mut self,
        event: &TouchInput,
        size: Vec2,
        area: Rect,
        threshold: f32,
        unclaimed: bool,
    ) -> (bool, bool) {
        if self.swallowed.contains(&event.id) {
            if matches!(event.phase, TouchPhase::Ended | TouchPhase::Canceled) {
                self.swallowed.remove(&event.id);
            }
            return (true, false);
        }
        if event.phase == TouchPhase::Started && unclaimed && area.contains(event.position) {
            if self.drag.is_some() {
                self.swallowed.insert(event.id);
            } else {
                self.drag = Some(Drag {
                    id: event.id,
                    origin: event.position,
                    position: event.position,
                    size,
                    canceled: false,
                });
            }
            return (true, false);
        }
        let Some(mut drag) = self.drag.filter(|d| d.id == event.id) else {
            return (false, false);
        };
        drag.position = event.position;
        let delta = drag.position - drag.origin;
        drag.canceled |= drag.size != size
            || delta.x.abs() > 32.0
            || delta.y > 24.0
            || !event.position.is_finite();
        let opens = event.phase == TouchPhase::Ended
            && !drag.canceled
            && -delta.y >= threshold
            && -delta.y > delta.x.abs() * 1.5;
        self.drag = if matches!(event.phase, TouchPhase::Ended | TouchPhase::Canceled) {
            None
        } else {
            Some(drag)
        };
        (true, opens)
    }
}

#[derive(SystemParam)]
pub(super) struct Bridge<'w> {
    pub drawer: ResMut<'w, Drawer>,
    pub options: Res<'w, Options>,
    pub safe: Res<'w, SafeBottom>,
    pub capture: Option<Res<'w, nvf_play::bindings::KeyboardCapture>>,
    open: MessageWriter<'w, nvf_play::menu::OpenInventory>,
}
impl Bridge<'_> {
    pub fn allowed(&self) -> bool {
        self.options.enabled
            && self.drawer.focused != Some(false)
            && !self.capture.as_ref().is_some_and(|c| c.0)
    }
    pub fn event(&mut self, event: &TouchInput, size: Vec2, unclaimed: bool) -> (bool, bool) {
        let result = self.drawer.event(
            event,
            size,
            handle(size, self.safe.0),
            self.options.threshold,
            unclaimed,
        );
        if result.1 {
            self.open.write(nvf_play::menu::OpenInventory);
        }
        result
    }
}

#[derive(Component)]
pub(super) struct Handle;
#[derive(Component)]
struct Hint;
fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Handle,
            Node {
                position_type: PositionType::Absolute,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(2001),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: px(44.0),
                    height: px(5.0),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.9, 1.0, 0.95, 0.9)),
                Pickable::IGNORE,
            ));
            p.spawn((
                Hint,
                Text::new("Pip-Boy"),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                Pickable::IGNORE,
            ));
        });
}
type HandleFilter = (With<Handle>, Without<Hint>);
type VisualState<'w> = (
    Res<'w, Options>,
    Res<'w, Drawer>,
    Res<'w, SafeBottom>,
    Res<'w, super::TouchState>,
    Res<'w, super::Layout>,
    Res<'w, super::modal::Capture>,
);

fn visual(
    (options, drawer, safe, state, layout, modal): VisualState<'_>,
    capture: Option<Res<nvf_play::bindings::KeyboardCapture>>,
    bindings: Option<Res<nvf_play::bindings::Bindings>>,
    pads: Query<&Gamepad>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut handle_nodes: Query<(&mut Node, &mut Visibility), HandleFilter>,
    mut hints: Query<&mut Visibility, (With<Hint>, Without<Handle>)>,
) {
    let Ok(window) = windows.single() else { return };
    let size = Vec2::new(window.width(), window.height());
    let rect = handle(size, safe.0);
    let controller = bindings
        .is_some_and(|b| b.device == nvf_play::bindings::InputDevice::Controller)
        && !pads.is_empty();
    let show = options.enabled
        && !modal.blocking()
        && !controller
        && state.playing
        && window.focused
        && drawer.focused != Some(false)
        && !state.suspended
        && layout.editor.is_none()
        && !capture.is_some_and(|c| c.0)
        && rect.min.y >= 0.0;
    for (mut node, mut visible) in &mut handle_nodes {
        *visible = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        node.left = px(rect.min.x);
        node.top = px(rect.min.y - drawer.progress().min(options.threshold * 1.25));
        node.width = px(rect.width());
        node.height = px(rect.height());
    }
    for mut visible in &mut hints {
        *visible = if layout.config.hints {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[cfg(target_os = "android")]
fn insets(mut safe: ResMut<SafeBottom>, time: Res<Time<Real>>, mut last: Local<f64>) {
    let now = time.elapsed_secs_f64();
    if *last != 0.0 && now - *last < 0.2 {
        return;
    }
    *last = now;
    if let Ok(value) = crate::android::activity_string("touchSafeBottom", None) {
        if let Ok(bottom) = value.parse::<f32>() {
            if bottom.is_finite() {
                safe.0 = bottom.clamp(0.0, 512.0);
            }
        }
    }
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<Options>()
        .init_resource::<Drawer>()
        .init_resource::<SafeBottom>()
        .add_message::<nvf_play::menu::OpenInventory>()
        .add_systems(Startup, spawn)
        .add_systems(Update, visual);
    #[cfg(target_os = "android")]
    app.add_systems(Update, insets);
    settings::add_setting(app, Setting {
        key: "touch-pipboy-swipe-v1", group: "Controller", label: "Swipe to Pip-Boy",
        note: "Show a bottom handle. Drag it upward and release to open the Pip-Boy. Android system navigation stays available.",
        get: |w| w.resource::<Options>().enabled, set: |w,v| w.resource_mut::<Options>().enabled = v, slider: None,
    });
    settings::add_setting(
        app,
        Setting::slider(
            "touch-pipboy-distance-v1",
            "Controller",
            "Pip-Boy swipe distance",
            "Upward distance needed before releasing the bottom handle opens the Pip-Boy.",
            Slider::choice(
                39,
                |w| (w.resource::<Options>().threshold - 48.0) / 152.0,
                |w, v| {
                    w.resource_mut::<Options>().threshold =
                        48.0 + (v.clamp(0.0, 1.0) * 38.0).round() * 4.0
                },
                |v| format!("{:.0} dp", 48.0 + (v * 38.0).round() * 4.0),
            ),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(id: u64, phase: TouchPhase, position: Vec2) -> TouchInput {
        TouchInput {
            id,
            phase,
            position,
            window: Entity::PLACEHOLDER,
            force: None,
        }
    }
    #[test]
    fn a_handle_swipe_commits_once_only_after_release() {
        let size = Vec2::new(900., 450.);
        let area = handle(size, 24.);
        let start = area.center();
        let mut d = Drawer::default();
        assert_eq!(
            d.event(&event(1, TouchPhase::Started, start), size, area, 96., true),
            (true, false)
        );
        assert_eq!(
            d.event(
                &event(1, TouchPhase::Moved, start - Vec2::Y * 120.),
                size,
                area,
                96.,
                true
            ),
            (true, false)
        );
        assert_eq!(
            d.event(
                &event(1, TouchPhase::Ended, start - Vec2::Y * 120.),
                size,
                area,
                96.,
                true
            ),
            (true, true)
        );
        assert_eq!(
            d.event(
                &event(1, TouchPhase::Ended, start - Vec2::Y * 120.),
                size,
                area,
                96.,
                true
            ),
            (false, false)
        );
    }
    #[test]
    fn look_control_system_edge_and_short_taps_do_not_open() {
        let size = Vec2::new(900., 450.);
        let area = handle(size, 32.);
        let mut d = Drawer::default();
        for start in [Vec2::new(450., 449.), Vec2::new(700., 300.)] {
            assert_eq!(
                d.event(&event(1, TouchPhase::Started, start), size, area, 96., true),
                (false, false)
            );
        }
        let start = area.center();
        assert_eq!(
            d.event(
                &event(1, TouchPhase::Started, start),
                size,
                area,
                96.,
                false
            ),
            (false, false)
        );
        d.event(&event(1, TouchPhase::Started, start), size, area, 96., true);
        assert_eq!(
            d.event(
                &event(1, TouchPhase::Ended, start - Vec2::Y * 30.),
                size,
                area,
                96.,
                true
            ),
            (true, false)
        );
        assert!(area.max.y <= size.y - 32.);
    }
    #[test]
    fn horizontal_intent_cancel_resize_and_second_finger_never_open() {
        let size = Vec2::new(900., 450.);
        let area = handle(size, 24.);
        let start = area.center();
        let mut d = Drawer::default();
        d.event(&event(1, TouchPhase::Started, start), size, area, 96., true);
        assert_eq!(
            d.event(&event(2, TouchPhase::Started, start), size, area, 96., true),
            (true, false)
        );
        assert_eq!(
            d.event(
                &event(2, TouchPhase::Ended, start - Vec2::Y * 150.),
                size,
                area,
                96.,
                true
            ),
            (true, false)
        );
        d.event(
            &event(1, TouchPhase::Moved, start + Vec2::X * 40.),
            size,
            area,
            96.,
            true,
        );
        assert_eq!(
            d.event(
                &event(1, TouchPhase::Ended, start - Vec2::Y * 150.),
                size,
                area,
                96.,
                true
            ),
            (true, false)
        );
        d.event(&event(1, TouchPhase::Started, start), size, area, 96., true);
        assert_eq!(
            d.event(
                &event(1, TouchPhase::Canceled, start - Vec2::Y * 150.),
                size,
                area,
                96.,
                true
            ),
            (true, false)
        );
        d.event(&event(1, TouchPhase::Started, start), size, area, 96., true);
        assert_eq!(
            d.event(
                &event(1, TouchPhase::Ended, start - Vec2::Y * 150.),
                size + Vec2::X,
                area,
                96.,
                true
            ),
            (true, false)
        );
    }
}
