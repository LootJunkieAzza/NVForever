//! The pause menu: Esc in the world opens it instead of quitting (the original opens its
//! pause menu there [observed by the coordinator playing the original, 2026-10-09]; its
//! layout and entries are not copied: NVForever offers Resume and Quit). It is a
//! `PlayState::Menu` like the container and inventory menus, so the world, the game
//! clock and mouse look stop and the cursor is released (`crate::menu`). Esc, Enter or
//! Space resume, Q quits; both buttons are clickable.
//!
//! **What a menu holds** (docs/design/dialogue.md §8, Pausing). Every menu holds game
//! logic by its own means (`PlayState`, the runtime's menu mode). Whether the picture
//! stands still too is one switch for all of them, [`MenuPause`] (`nvforever
//! --menu-pause living|original`, default `living`): the dialogue screen, the inventory
//! and container menus, a script's message boxes and the menus scripts open [owner
//! decision 2026-10-10]. `original` pauses Bevy's virtual clock while such a menu is
//! open ([`freeze_picture`]), so animations, wind, water and clouds stand still.
//! **The pause menu is the exception: it always holds everything**, in both modes: the
//! picture (the virtual clock), scripts (`script.rs` does not tick under it) and sound
//! (`voice.rs` pauses the lines being said and resumes them).
//!
//! While the virtual clock stands still, what must go on reads [`AidTime`]: the timed
//! test aids, the menus' own timers and the scripts' frame time (MenuMode blocks and a
//! conversation's lines run in real time under an `original` menu). `nvforever
//! --run-seconds` counts real time then as well, so an unattended run always ends.
//!
//! Test aid `NVF_PLAY_TEST_PAUSE=<seconds>:<seconds>[,…]`: opens the pause menu at the
//! first time (as Esc) and resumes at the second (as Esc again), on the aids' clock.

use crate::{PlaySet, PlayState};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

/// What an open menu holds besides game logic (`nvforever --menu-pause`; the old name
/// `--dialogue-pause` still works). Not the pause menu's business: that one always holds
/// everything.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuPause {
    /// Only game logic stands still; the picture lives on.
    #[default]
    Living,
    /// The virtual clock stops: animations, wind, water and sky stand still too.
    Original,
}

/// Whether the picture stands still: always under the pause menu; under any other menu
/// (every state but `Playing`) with [`MenuPause::Original`].
pub fn picture_held(mode: MenuPause, paused: bool, state: PlayState) -> bool {
    paused || (mode == MenuPause::Original && state != PlayState::Playing)
}

/// Real time spent while the virtual clock stood still, and this frame's share of it.
#[derive(Resource, Default, Debug)]
pub struct PlayClock {
    stopped: f32,
    frame: Option<f32>,
}

impl PlayClock {
    /// Real seconds spent so far while the virtual clock stood still.
    pub fn stopped_secs(&self) -> f32 {
        self.stopped
    }
}

/// The clock of what goes on while a menu holds the picture: timed test aids, menu
/// timers, the scripts' frame time.
#[derive(SystemParam)]
pub struct AidTime<'w> {
    time: Res<'w, Time>,
    clock: Res<'w, PlayClock>,
}

impl AidTime<'_> {
    /// Seconds since the start: the virtual clock's, plus the real time it stood still.
    pub fn elapsed_secs(&self) -> f32 {
        self.time.elapsed_secs() + self.clock.stopped
    }

    /// The frame's time: the virtual clock's, or real time (at most 0.25 s, the virtual
    /// clock's own cap) while it stands still.
    pub fn delta_secs_f64(&self) -> f64 {
        self.clock.frame.map_or_else(|| self.time.delta_secs_f64(), f64::from)
    }
}

fn tick_play_clock(virt: Res<Time<Virtual>>, real: Res<Time<Real>>, mut clock: ResMut<PlayClock>) {
    clock.frame = virt.is_paused().then(|| real.delta_secs().min(0.25));
    clock.stopped += clock.frame.unwrap_or(0.0);
}

/// Pause the virtual clock while the picture is held ([`picture_held`]) and let it run
/// again after. Nothing else pauses it.
fn freeze_picture(mode: Res<MenuPause>, paused: Res<Paused>, state: Res<State<PlayState>>, mut clock: ResMut<Time<Virtual>>) {
    let stop = picture_held(*mode, paused.0, *state.get());
    if clock.is_paused() != stop {
        let why = if paused.0 { "the pause menu" } else { "--menu-pause original" };
        if stop {
            info!("menu pause: the picture stands still ({why})");
            clock.pause();
        } else {
            info!("menu pause: the picture runs again");
            clock.unpause();
        }
    }
}

/// `NVF_PLAY_TEST_PAUSE`: when to open the pause menu and when to resume.
#[derive(Resource, Default, Debug, PartialEq)]
struct PauseAid {
    open: Vec<f32>,
    resume: Vec<f32>,
}

/// Parse `<open seconds>:<resume seconds>[,…]`.
fn parse_pause_aid(v: &str) -> Result<PauseAid, String> {
    let mut aid = PauseAid::default();
    for item in v.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (a, b) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <open seconds>:<resume seconds>"))?;
        let num = |s: &str| s.trim().parse::<f32>().map_err(|_| format!("{item:?}: bad seconds {s:?}"));
        let (a, b) = (num(a)?, num(b)?);
        if b <= a {
            return Err(format!("{item:?}: the menu resumes after it opens"));
        }
        aid.open.push(a);
        aid.resume.push(b);
    }
    Ok(aid)
}

/// Whether a time of `due` has come (each fires once).
fn fire(due: &mut Vec<f32>, now: f32) -> bool {
    let before = due.len();
    due.retain(|&t| t > now);
    due.len() < before
}

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
        let aid = match std::env::var("NVF_PLAY_TEST_PAUSE") {
            Ok(v) => parse_pause_aid(&v).unwrap_or_else(|e| {
                warn!("NVF_PLAY_TEST_PAUSE: {e}");
                PauseAid::default()
            }),
            Err(_) => PauseAid::default(),
        };
        app.init_resource::<Paused>()
            .init_resource::<MenuPause>()
            .init_resource::<PlayClock>()
            .insert_resource(aid)
            .add_systems(Update, tick_play_clock.before(PlaySet::Input))
            .add_systems(Update, freeze_picture.after(PlaySet::Ui).before(PlaySet::Sync))
            .add_systems(
                Update,
                // Esc pauses a conversation too; resuming goes back to it (`crate::dialogue`).
                (open.run_if(in_state(PlayState::Playing).or_else(in_state(PlayState::Dialogue))), choose.run_if(in_state(PlayState::Menu))).in_set(PlaySet::Ui),
            )
            .add_systems(OnExit(PlayState::Menu), close);
    }
}

fn open(
    keys: Res<ButtonInput<KeyCode>>,
    time: AidTime,
    mut aid: ResMut<PauseAid>,
    mut paused: ResMut<Paused>,
    mut next: ResMut<NextState<PlayState>>,
    mut commands: Commands,
) {
    let by_aid = fire(&mut aid.open, time.elapsed_secs());
    if !keys.just_pressed(KeyCode::Escape) && !by_aid {
        return;
    }
    if by_aid {
        info!("test aid NVF_PLAY_TEST_PAUSE: Esc");
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
    time: AidTime,
    mut aid: ResMut<PauseAid>,
    paused: Res<Paused>,
    buttons: Query<(&Interaction, &PauseButton), Changed<Interaction>>,
    mut next: ResMut<NextState<PlayState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !paused.0 {
        return;
    }
    let by_aid = fire(&mut aid.resume, time.elapsed_secs());
    if by_aid {
        info!("test aid NVF_PLAY_TEST_PAUSE: Esc (resume)");
    }
    let clicked = buttons.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, b)| *b).or(by_aid.then_some(PauseButton::Resume));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pause_menu_always_holds_the_picture_and_other_menus_by_the_switch() {
        use PlayState::*;
        for mode in [MenuPause::Living, MenuPause::Original] {
            // The pause menu (a `Menu` state with `Paused`), whatever the switch says.
            assert!(picture_held(mode, true, Menu));
            // In the world nothing is held.
            assert!(!picture_held(mode, false, Playing));
        }
        // The inventory, a container, a message box (`Menu`) and the dialogue screen.
        assert!(!picture_held(MenuPause::Living, false, Menu));
        assert!(!picture_held(MenuPause::Living, false, Dialogue));
        assert!(picture_held(MenuPause::Original, false, Menu));
        assert!(picture_held(MenuPause::Original, false, Dialogue));
    }

    #[test]
    fn the_pause_aid_parses_and_fires_once() {
        let mut a = parse_pause_aid("20:24, 30:31.5").unwrap();
        assert_eq!(a, PauseAid { open: vec![20.0, 30.0], resume: vec![24.0, 31.5] });
        assert!(!fire(&mut a.open, 19.9));
        assert!(fire(&mut a.open, 20.0));
        assert!(!fire(&mut a.open, 20.1));
        assert!(fire(&mut a.open, 40.0) && a.open.is_empty());
        assert!(parse_pause_aid("20").is_err());
        assert!(parse_pause_aid("24:20").is_err());
        assert!(parse_pause_aid("x:20").is_err());
        assert_eq!(parse_pause_aid("").unwrap(), PauseAid::default());
    }
}
