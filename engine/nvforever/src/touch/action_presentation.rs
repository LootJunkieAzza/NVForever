//! Caption/glyph changes on the existing action node; never input, layout or saved keys.
use super::{
    icons::{self, TouchIcon},
    TouchVisual, BUTTONS,
};
use bevy::prelude::*;
use nvf_play::{
    bindings::Control,
    touch_action_presentation::{AdaptiveTouchActions, AimPresentation, PublishPresentation},
};

pub(super) fn install(app: &mut App) {
    app.add_plugins(nvf_play::touch_action_presentation::TouchActionPresentationPlugin)
        .add_systems(
            Update,
            show.after(PublishPresentation).after(super::show_labels),
        );
}
fn show(
    presentation: Res<AimPresentation>,
    settings: Res<AdaptiveTouchActions>,
    parents: Query<&TouchVisual>,
    mut captions: Query<(&ChildOf, &TouchVisual, &mut Text)>,
    mut glyphs: Query<(&ChildOf, &mut TouchIcon)>,
) {
    let Some(action) = BUTTONS
        .iter()
        .position(|b| b.control == Some(Control::Block))
    else {
        return;
    };
    let mine = |parent: &ChildOf| matches!(parents.get(parent.parent()),Ok(TouchVisual::Button(i)) if *i==action);
    let label = presentation.0.label(settings.0);
    for (parent, visual, mut text) in &mut captions {
        if mine(parent) && matches!(visual, TouchVisual::Caption) && text.0 != label {
            text.0 = label.into();
        }
    }
    if let Some(index) = icons::semantic_index(presentation.0.glyph(settings.0)) {
        for (parent, mut icon) in &mut glyphs {
            if mine(parent) && icon.0 != index {
                icon.0 = index;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::{
        mouse::MouseButtonInput,
        touch::{TouchInput, TouchPhase},
        ButtonState, InputPlugin,
    };
    use bevy::window::{PrimaryWindow, WindowFocused};
    use nvf_play::{
        bindings::{Bindings, ControlInput, InputDevice},
        touch_action_presentation::AimRole,
    };
    #[derive(Resource)]
    struct Wanted(AimRole);
    #[derive(Resource, Default)]
    struct Seen {
        down: bool,
        pressed: bool,
        released: bool,
    }
    fn model(wanted: Res<Wanted>, mut model: ResMut<AimPresentation>) {
        model.0 = wanted.0;
    }
    fn observe(input: ControlInput, mut seen: ResMut<Seen>) {
        seen.down = input.pressed(Control::Block);
        seen.pressed = input.just_pressed(Control::Block);
        seen.released = input.just_released(Control::Block);
    }
    fn scene() -> (App, Entity, Vec2) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputPlugin, bevy::state::app::StatesPlugin))
            .insert_state(nvf_play::PlayState::Playing)
            .add_message::<WindowFocused>()
            .add_message::<bevy::window::AppLifecycle>()
            .init_resource::<Bindings>()
            .init_resource::<Seen>()
            .insert_resource(Wanted(AimRole::Aim))
            .add_plugins(super::super::TouchControlsPlugin)
            .add_systems(Update, model.after(PublishPresentation).before(show))
            .add_systems(Update, observe.after(show));
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
        // Publish physical image boxes without requiring a renderer/GPU in this model test.
        let mut query = app
            .world_mut()
            .query_filtered::<(&mut ComputedNode, &mut InheritedVisibility), With<TouchIcon>>();
        for (mut node, mut inherited) in query.iter_mut(app.world_mut()) {
            *inherited = InheritedVisibility::VISIBLE;
            node.size = Vec2::splat(48.0);
        }
        app.update();
        let index = BUTTONS
            .iter()
            .position(|b| b.control == Some(Control::Block))
            .unwrap();
        let point = app
            .world()
            .resource::<super::super::Layout>()
            .config
            .center(super::super::Target::Button(index), Vec2::splat(1000.0));
        (app, window, point)
    }
    fn finger(app: &mut App, window: Entity, point: Vec2, phase: TouchPhase) {
        app.world_mut().write_message(TouchInput {
            window,
            position: point,
            id: 9,
            phase,
            force: None,
        });
    }
    fn physical(app: &mut App, window: Entity, down: bool) {
        app.world_mut().write_message(MouseButtonInput {
            window,
            button: MouseButton::Right,
            state: if down {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
        });
    }
    fn shown(app: &mut App) -> (Entity, Entity, Entity, String, usize) {
        let world = app.world_mut();
        let action = BUTTONS
            .iter()
            .position(|b| b.control == Some(Control::Block))
            .unwrap();
        let mut q = world.query::<(Entity, &TouchVisual)>();
        let button = q
            .iter(world)
            .find_map(|(e, v)| matches!(v,TouchVisual::Button(i) if *i==action).then_some(e))
            .unwrap();
        let mut q = world.query::<(Entity, &ChildOf, &Text)>();
        let (caption, label) = q
            .iter(world)
            .find_map(|(e, p, t)| (p.parent() == button).then_some((e, t.0.clone())))
            .unwrap();
        let mut q = world.query::<(Entity, &ChildOf, &TouchIcon)>();
        let (glyph, index) = q
            .iter(world)
            .find_map(|(e, p, i)| (p.parent() == button).then_some((e, i.0)))
            .unwrap();
        (button, caption, glyph, label, index)
    }
    #[test]
    fn a_held_touch_survives_caption_and_icon_changes_and_releases_its_original_action() {
        let (mut app, w, p) = scene();
        let initial = shown(&mut app);
        let layout = app
            .world()
            .resource::<super::super::Layout>()
            .config
            .encode();
        let node = app.world().get::<Node>(initial.0).unwrap();
        let geometry = (node.left, node.top, node.width, node.height);
        finger(&mut app, w, p, TouchPhase::Started);
        app.update();
        assert!(app.world().resource::<Seen>().down && app.world().resource::<Seen>().pressed);
        for (role, label, semantic) in [
            (AimRole::Block, "Block", "block"),
            (AimRole::Scope, "Scope", "aim"),
            (AimRole::Aim, "Aim", "aim"),
        ] {
            app.world_mut().resource_mut::<Wanted>().0 = role;
            app.update();
            let now = shown(&mut app);
            assert_eq!((now.0, now.1, now.2), (initial.0, initial.1, initial.2));
            assert_eq!(now.3, label);
            assert_eq!(now.4, icons::semantic_index(semantic).unwrap());
            assert!(app.world().resource::<Seen>().down);
            assert!(
                !app.world().resource::<Seen>().pressed && !app.world().resource::<Seen>().released,
                "presentation cannot synthesize edges"
            );
            let node = app.world().get::<Node>(now.0).unwrap();
            assert_eq!((node.left, node.top, node.width, node.height), geometry);
        }
        finger(&mut app, w, p, TouchPhase::Ended);
        app.update();
        assert!(!app.world().resource::<Seen>().down && app.world().resource::<Seen>().released);
        assert_eq!(
            app.world()
                .resource::<super::super::Layout>()
                .config
                .encode(),
            layout
        );
        assert!(layout["buttons"].get("Aim").is_some());
        assert!(layout["buttons"].get("Block").is_none());
    }
    #[test]
    fn static_preference_reuses_the_existing_nodes_and_same_texture_handle() {
        let (mut app, _, _) = scene();
        app.world_mut().resource_mut::<Wanted>().0 = AimRole::Block;
        app.update();
        let initial = shown(&mut app);
        assert_eq!(initial.3, "Block");
        let handle = app
            .world()
            .get::<ImageNode>(initial.2)
            .unwrap()
            .image
            .clone();
        assert_ne!(
            handle,
            ImageNode::default().image,
            "the model fixture must allocate a real raster"
        );
        app.world_mut().resource_mut::<AdaptiveTouchActions>().0 = false;
        app.update();
        let now = shown(&mut app);
        assert_eq!((now.0, now.1, now.2), (initial.0, initial.1, initial.2));
        assert_eq!(
            (now.3.as_str(), now.4),
            ("Aim", icons::semantic_index("aim").unwrap())
        );
        assert_eq!(
            app.world().get::<ImageNode>(initial.2).unwrap().image,
            handle
        );
        assert!(!app.world().resource::<Seen>().down);
    }
    #[test]
    fn changing_presentation_and_lifting_touch_preserves_an_independent_mouse_owner() {
        let (mut app, w, p) = scene();
        physical(&mut app, w, true);
        app.update();
        finger(&mut app, w, p, TouchPhase::Started);
        app.update();
        app.world_mut().resource_mut::<Wanted>().0 = AimRole::Block;
        finger(&mut app, w, p, TouchPhase::Ended);
        app.update();
        assert!(app.world().resource::<Seen>().down);
        assert!(!app.world().resource::<Seen>().released);
        physical(&mut app, w, false);
        app.update();
        assert!(!app.world().resource::<Seen>().down && app.world().resource::<Seen>().released);
    }
    #[test]
    fn focus_cancellation_and_controller_hand_off_keep_the_original_release_route() {
        let (mut app, w, p) = scene();
        finger(&mut app, w, p, TouchPhase::Started);
        app.update();
        app.world_mut().resource_mut::<Wanted>().0 = AimRole::Block;
        app.world_mut().write_message(WindowFocused {
            window: w,
            focused: false,
        });
        app.update();
        assert!(!app.world().resource::<Seen>().down);
        app.world_mut().write_message(WindowFocused {
            window: w,
            focused: true,
        });
        finger(&mut app, w, p, TouchPhase::Ended);
        app.update();
        finger(&mut app, w, p, TouchPhase::Started);
        app.update();
        assert!(app.world().resource::<Seen>().down);
        app.world_mut().resource_mut::<Bindings>().device = InputDevice::Controller;
        app.update();
        assert!(!app.world().resource::<Seen>().down);
        finger(&mut app, w, p, TouchPhase::Ended);
        app.update();
        app.world_mut().resource_mut::<Bindings>().device = InputDevice::KeyboardMouse;
        app.update();
        assert!(!app.world().resource::<Seen>().down);
    }
    #[test]
    fn the_presentation_adapter_does_not_release_a_latched_aggregate_owner() {
        let (mut app, _, _) = scene();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        for role in [AimRole::Block, AimRole::Scope, AimRole::Aim] {
            app.world_mut().resource_mut::<AimPresentation>().0 = role;
            app.world_mut().run_system_cached(show).unwrap();
            assert!(app
                .world()
                .resource::<ButtonInput<MouseButton>>()
                .pressed(MouseButton::Right));
        }
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Right);
        app.world_mut().run_system_cached(show).unwrap();
        assert!(!app
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Right));
    }
    #[test]
    fn adaptive_latched_aim_survives_art_change_but_not_controller_takeover_or_return_tap() {
        use bevy::input::gamepad::{RawGamepadButtonChangedEvent, RawGamepadEvent};
        let (mut app, w, p) = scene();
        app.add_plugins(nvf_play::pad::InputSwitchPlugin);
        let pad = app.world_mut().spawn(Gamepad::default()).id();
        app.world_mut()
            .resource_mut::<super::super::Layout>()
            .config
            .aiming
            .toggle_aim = true;
        app.world_mut().resource_mut::<Wanted>().0 = AimRole::Block;
        app.update();
        let initial = shown(&mut app);
        finger(&mut app, w, p, TouchPhase::Started);
        finger(&mut app, w, p, TouchPhase::Ended);
        app.update();
        assert!(app.world().resource::<Seen>().down);
        app.world_mut().resource_mut::<Wanted>().0 = AimRole::Scope;
        app.update();
        let scope = shown(&mut app);
        assert_eq!(
            (scope.0, scope.1, scope.2),
            (initial.0, initial.1, initial.2)
        );
        assert_eq!(scope.3, "Scope");
        assert!(app.world().resource::<Seen>().down);
        let pad_edge = |app: &mut App, down| {
            app.world_mut().write_message(RawGamepadEvent::Button(
                RawGamepadButtonChangedEvent::new(
                    pad,
                    GamepadButton::South,
                    if down { 1.0 } else { 0.0 },
                ),
            ));
        };
        pad_edge(&mut app, true);
        app.update();
        assert_eq!(
            app.world().resource::<Bindings>().device,
            InputDevice::Controller
        );
        assert!(!app.world().resource::<Seen>().down);
        app.update();
        assert!(
            !app.world()
                .resource::<super::super::TouchState>()
                .aim_latched
        );
        assert_eq!(
            *app.world().get::<Visibility>(initial.0).unwrap(),
            Visibility::Hidden
        );
        pad_edge(&mut app, false);
        app.update();
        finger(&mut app, w, p, TouchPhase::Started);
        finger(&mut app, w, p, TouchPhase::Ended);
        app.update();
        assert_eq!(
            app.world().resource::<Bindings>().device,
            InputDevice::KeyboardMouse
        );
        assert!(!app.world().resource::<Seen>().down);
        assert!(
            !app.world()
                .resource::<super::super::TouchState>()
                .aim_latched
        );
        assert_eq!(
            *app.world().get::<Visibility>(initial.0).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(shown(&mut app).3, "Scope");
        finger(&mut app, w, p, TouchPhase::Started);
        finger(&mut app, w, p, TouchPhase::Ended);
        app.update();
        assert!(
            app.world().resource::<Seen>().down,
            "a subsequent deliberate tap can aim"
        );
    }

    #[test]
    fn adaptive_scope_presentation_cannot_rearm_a_latch_after_same_batch_drawer_commit() {
        let (mut app, w, p) = scene();
        app.world_mut()
            .resource_mut::<super::super::Layout>()
            .config
            .aiming
            .toggle_aim = true;
        app.world_mut()
            .resource_mut::<super::super::drawer::Options>()
            .enabled = true;
        app.world_mut().resource_mut::<Wanted>().0 = AimRole::Scope;
        app.update();
        let initial = shown(&mut app);
        finger(&mut app, w, p, TouchPhase::Started);
        finger(&mut app, w, p, TouchPhase::Ended);
        app.update();
        assert!(app.world().resource::<Seen>().down);
        let start = super::super::drawer::handle(Vec2::splat(1000.0), 24.0).center();
        for (phase, y) in [
            (TouchPhase::Started, start.y),
            (TouchPhase::Moved, start.y - 140.0),
        ] {
            app.world_mut().write_message(TouchInput {
                window: w,
                position: Vec2::new(start.x, y),
                id: 21,
                phase,
                force: None,
            });
            app.update();
        }
        app.world_mut().write_message(TouchInput {
            window: w,
            position: start - Vec2::new(0.0, 140.0),
            id: 21,
            phase: TouchPhase::Ended,
            force: None,
        });
        finger(&mut app, w, p, TouchPhase::Started);
        app.update();
        assert_eq!(
            app.world()
                .resource::<Messages<nvf_play::menu::OpenInventory>>()
                .len(),
            1
        );
        assert!(!app.world().resource::<Seen>().down);
        assert!(
            !app.world()
                .resource::<super::super::TouchState>()
                .aim_latched
        );
        assert!(app
            .world()
            .resource::<super::super::TouchState>()
            .contacts
            .is_empty());
        let now = shown(&mut app);
        assert_eq!((now.0, now.1, now.2), (initial.0, initial.1, initial.2));
        assert_eq!(now.3, "Scope");
        app.update();
        assert!(
            !app.world().resource::<Seen>().down,
            "discarded touch must not replay next frame"
        );
    }
}
