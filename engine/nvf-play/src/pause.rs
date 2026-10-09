//! The pause menu: Esc in the world opens it instead of quitting (the original opens its
//! pause menu there [observed by the coordinator playing the original, 2026-10-09]; its
//! layout and entries are not copied: NVForever offers Resume and Quit). It is a
//! `PlayState::Menu` like the container and inventory menus, so the world, the game
//! clock and mouse look stop and the cursor is released (`crate::menu`). Esc, Enter or
//! Space resume, Q quits; both buttons are clickable.

use crate::{PlaySet, PlayState};
use bevy::prelude::*;

/// Whether the pause menu is the open menu.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paused(pub bool);

#[derive(Component)]
struct PauseRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum PauseButton {
    Resume,
    Quit,
}

pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Paused>()
            .add_systems(
                Update,
                (open.run_if(in_state(PlayState::Playing)), choose.run_if(in_state(PlayState::Menu))).in_set(PlaySet::Ui),
            )
            .add_systems(OnExit(PlayState::Menu), close);
    }
}

fn open(keys: Res<ButtonInput<KeyCode>>, mut paused: ResMut<Paused>, mut next: ResMut<NextState<PlayState>>, mut commands: Commands) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    info!("pause menu: open");
    paused.0 = true;
    next.set(PlayState::Menu);
    let font = |size: f32| TextFont { font_size: FontSize::Px(size), ..default() };
    let button = |label: &str, b: PauseButton| {
        (
            Button,
            b,
            Node { padding: UiRect::axes(Val::Px(18.0), Val::Px(6.0)), ..default() },
            BackgroundColor(Color::srgba(0.20, 0.28, 0.20, 0.9)),
            children![(Text::new(label), font(18.0), TextColor(Color::srgb(0.85, 0.95, 0.80)))],
        )
    };
    commands.spawn((
        PauseRoot,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        GlobalZIndex(110),
        children![(
            Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(12.0), padding: UiRect::all(Val::Px(20.0)), align_items: AlignItems::Center, ..default() },
            BackgroundColor(Color::srgba(0.05, 0.08, 0.06, 0.94)),
            children![
                (Text::new("Paused"), font(26.0), TextColor(Color::srgb(1.0, 0.75, 0.30))),
                button("Resume (Esc)", PauseButton::Resume),
                button("Quit NVForever (Q)", PauseButton::Quit),
            ],
        )],
    ));
}

fn choose(
    keys: Res<ButtonInput<KeyCode>>,
    paused: Res<Paused>,
    buttons: Query<(&Interaction, &PauseButton), Changed<Interaction>>,
    mut next: ResMut<NextState<PlayState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !paused.0 {
        return;
    }
    let clicked = buttons.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, b)| *b);
    if keys.just_pressed(KeyCode::KeyQ) || clicked == Some(PauseButton::Quit) {
        info!("pause menu: quit");
        exit.write(AppExit::Success);
    } else if keys.any_just_pressed([KeyCode::Escape, KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || clicked == Some(PauseButton::Resume) {
        info!("pause menu: resume");
        next.set(PlayState::Playing);
    }
}

fn close(mut commands: Commands, mut paused: ResMut<Paused>, roots: Query<Entity, With<PauseRoot>>) {
    paused.0 = false;
    for e in &roots {
        commands.entity(e).despawn();
    }
}
