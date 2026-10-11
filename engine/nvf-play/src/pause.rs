//! The pause menu: Esc in the world opens it instead of quitting (the original opens its
//! pause menu there [observed by the coordinator playing the original, 2026-10-09]; its
//! layout and entries are not copied: NVForever offers Resume, Settings and Quit). It is
//! a `PlayState::Menu` like the container and inventory menus, so the world, the game
//! clock and mouse look stop and the cursor is released (`crate::menu`). Esc, Enter or
//! Space resume, S opens the settings, Q quits; the buttons are clickable.
//!
//! **The settings page** lists the on/off switches of [`crate::settings`], each showing
//! the state the game is using now. Up/Down (or W/S) choose a row, Enter, Space, Left or
//! Right flip it, as does a click on it; a flip acts at once. Esc or Backspace go back
//! to the first page.
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
//! first time (as Esc) and resumes at the second (as Esc again, from either page), on
//! the aids' clock. `NVF_PLAY_TEST_SETTINGS=<seconds>:<key>[,…]`: at each time, with the
//! pause menu open, shows the settings page and flips the switch of that key (as a
//! click on its row); the key `page` only shows the page.

use crate::settings::{add_setting, Setting, Settings};
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

/// `NVF_PLAY_TEST_PAUSE`: when to open the pause menu and when to resume;
/// `NVF_PLAY_TEST_SETTINGS`: when to flip which switch of the settings page.
#[derive(Resource, Default, Debug, PartialEq)]
struct PauseAid {
    open: Vec<f32>,
    resume: Vec<f32>,
    switches: Vec<(f32, String)>,
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

/// Parse `<seconds>:<key>[,…]`.
fn parse_settings_aid(v: &str) -> Result<Vec<(f32, String)>, String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let (t, key) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <seconds>:<key>"))?;
            let t = t.trim().parse::<f32>().map_err(|_| format!("{item:?}: bad seconds {t:?}"))?;
            Ok((t, key.trim().to_string()))
        })
        .collect()
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

/// The page of the pause menu that is up; on the settings page, the row the keys are on.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    #[default]
    Main,
    Settings {
        selected: usize,
    },
}

#[derive(Component)]
struct PauseRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum PauseButton {
    Resume,
    Settings,
    Quit,
    /// From the settings page back to the first page.
    Back,
    /// The row of a switch (its index in [`Settings`]).
    Switch(usize),
}

/// The text of a row that says whether its switch is on.
#[derive(Component)]
struct SwitchState(usize);

/// The line under the rows that explains the chosen switch.
#[derive(Component)]
struct SwitchNote;

const TITLE: Color = Color::srgb(1.0, 0.75, 0.30);
const TEXT: Color = Color::srgb(0.85, 0.95, 0.80);
const DIM: Color = Color::srgb(0.55, 0.62, 0.52);
const BUTTON: Color = Color::srgba(0.20, 0.28, 0.20, 0.9);
const ROW: Color = Color::srgba(0.10, 0.14, 0.10, 0.9);
const ROW_CHOSEN: Color = Color::srgba(0.26, 0.36, 0.24, 0.95);
const ON: Color = Color::srgb(0.55, 1.0, 0.45);
/// Width of the settings page's rows.
const SETTINGS_WIDTH: f32 = 540.0;

/// The settings page's group of switches about conversations and menus.
pub const GROUP_MENUS: &str = "Dialogue and menus";

pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        let mut aid = match std::env::var("NVF_PLAY_TEST_PAUSE") {
            Ok(v) => parse_pause_aid(&v).unwrap_or_else(|e| {
                warn!("NVF_PLAY_TEST_PAUSE: {e}");
                PauseAid::default()
            }),
            Err(_) => PauseAid::default(),
        };
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_SETTINGS") {
            match parse_settings_aid(&v) {
                Ok(s) => aid.switches = s,
                Err(e) => warn!("NVF_PLAY_TEST_SETTINGS: {e}"),
            }
        }
        app.init_resource::<Paused>()
            .init_resource::<Page>()
            .init_resource::<MenuPause>()
            .init_resource::<PlayClock>()
            .insert_resource(aid)
            .add_systems(Update, tick_play_clock.before(PlaySet::Input))
            .add_systems(Update, freeze_picture.after(PlaySet::Ui).before(PlaySet::Sync))
            .add_systems(
                Update,
                // Esc pauses a conversation too; resuming goes back to it (`crate::dialogue`).
                // Over a conversation, a barter menu takes Esc for itself (`crate::menu`).
                (
                    open.run_if(in_state(PlayState::Playing).or_else(in_state(PlayState::Dialogue)).and_then(crate::menu::no_menu)),
                    (choose, show_switches).chain().run_if(in_state(PlayState::Menu)),
                )
                    .in_set(PlaySet::Ui),
            )
            .add_systems(OnExit(PlayState::Menu), close);
        add_setting(
            app,
            Setting {
                key: "menu-pause",
                group: GROUP_MENUS,
                label: "Freeze the picture in dialogue and menus",
                note: "On: while a conversation, the inventory, a container or a message box is open, animations, wind, water and sky stand still, as in the original. Off: only game logic waits and the world lives on. This menu always holds everything. (--menu-pause original|living)",
                get: |w| *w.resource::<MenuPause>() == MenuPause::Original,
                set: |w, on| *w.resource_mut::<MenuPause>() = if on { MenuPause::Original } else { MenuPause::Living },
            },
        );
    }
}

fn font(size: f32) -> TextFont {
    TextFont { font_size: FontSize::Px(size), ..default() }
}

fn button(label: &str, b: PauseButton) -> impl Bundle {
    (
        Button,
        b,
        Node { padding: UiRect::axes(Val::Px(18.0), Val::Px(6.0)), ..default() },
        BackgroundColor(BUTTON),
        children![(Text::new(label), font(18.0), TextColor(TEXT))],
    )
}

/// Draw a page of the pause menu (the one before it is gone already).
fn spawn_page(commands: &mut Commands, page: Page, settings: &Settings) {
    let backdrop = Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };
    let panel = Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(12.0), padding: UiRect::all(Val::Px(20.0)), align_items: AlignItems::Center, ..default() };
    commands.spawn((PauseRoot, backdrop, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)), GlobalZIndex(110))).with_children(|root| {
        root.spawn((panel, BackgroundColor(Color::srgba(0.05, 0.08, 0.06, 0.94)))).with_children(|p| match page {
            Page::Main => {
                p.spawn((Text::new("Paused"), font(26.0), TextColor(TITLE)));
                p.spawn(button("Resume (Esc)", PauseButton::Resume));
                p.spawn(button("Settings (S)", PauseButton::Settings));
                p.spawn(button("Quit NVForever (Q)", PauseButton::Quit));
            }
            Page::Settings { selected } => settings_page(p, settings, selected),
        });
    });
}

/// The settings page: a row per switch under its group's heading, the chosen switch's
/// note, the keys. [`show_switches`] fills in what changes: each switch's state, the
/// chosen row, the note.
fn settings_page(p: &mut ChildSpawnerCommands, settings: &Settings, selected: usize) {
    p.spawn((Text::new("Settings"), font(26.0), TextColor(TITLE)));
    let list = Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), width: Val::Px(SETTINGS_WIDTH), ..default() };
    p.spawn(list).with_children(|rows| {
        let mut group = "";
        for (i, s) in settings.list().iter().enumerate() {
            if s.group != group {
                group = s.group;
                rows.spawn((Node { margin: UiRect::top(Val::Px(6.0)), ..default() }, children![(Text::new(group), font(14.0), TextColor(TITLE))]));
            }
            let row = Node { justify_content: JustifyContent::SpaceBetween, column_gap: Val::Px(16.0), padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), ..default() };
            rows.spawn((Button, PauseButton::Switch(i), row, BackgroundColor(if i == selected { ROW_CHOSEN } else { ROW }))).with_children(|r| {
                r.spawn((Text::new(s.label), font(16.0), TextColor(TEXT)));
                r.spawn((SwitchState(i), Text::new(""), font(16.0), TextColor(DIM)));
            });
        }
        if settings.list().is_empty() {
            rows.spawn((Text::new("Nothing to switch."), font(16.0), TextColor(DIM)));
        }
    });
    // Four lines high whatever the note, so the panel keeps its size from row to row.
    let note = settings.list().get(selected).map_or("", |s| s.note);
    p.spawn((Node { width: Val::Px(SETTINGS_WIDTH), min_height: Val::Px(68.0), ..default() }, children![(SwitchNote, Text::new(note), font(13.0), TextColor(DIM))]));
    p.spawn((Text::new("Up/Down choose   Enter or Space switch   Esc back"), font(13.0), TextColor(DIM)));
    p.spawn(button("Back (Esc)", PauseButton::Back));
}

#[allow(clippy::too_many_arguments)]
fn open(
    keys: Res<ButtonInput<KeyCode>>,
    time: AidTime,
    mut aid: ResMut<PauseAid>,
    mut paused: ResMut<Paused>,
    mut page: ResMut<Page>,
    settings: Res<Settings>,
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
    *page = Page::Main;
    next.set(PlayState::Menu);
    spawn_page(&mut commands, *page, &settings);
}

/// The row after `selected` (`step` 1) or before it (-1) of `rows`, round the ends.
fn step_row(selected: usize, step: i32, rows: usize) -> usize {
    if rows == 0 {
        return 0;
    }
    (selected.min(rows - 1) as i32 + step).rem_euclid(rows as i32) as usize
}

#[allow(clippy::too_many_arguments)]
fn choose(
    keys: Res<ButtonInput<KeyCode>>,
    time: AidTime,
    mut aid: ResMut<PauseAid>,
    paused: Res<Paused>,
    mut page: ResMut<Page>,
    settings: Res<Settings>,
    buttons: Query<(&Interaction, &PauseButton), Changed<Interaction>>,
    roots: Query<Entity, With<PauseRoot>>,
    mut next: ResMut<NextState<PlayState>>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    if !paused.0 {
        return;
    }
    let now = time.elapsed_secs();
    if fire(&mut aid.resume, now) {
        info!("test aid NVF_PLAY_TEST_PAUSE: Esc (resume)");
        info!("pause menu: resume");
        next.set(PlayState::Playing);
        return;
    }
    let before = *page;
    let mut flips = Vec::new();
    // The settings aid: show the page and flip the switch, as a click on its row.
    let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut aid.switches).into_iter().partition(|(t, _)| *t <= now);
    aid.switches = later;
    for (_, key) in due {
        info!("test aid NVF_PLAY_TEST_SETTINGS: {key}");
        match (settings.index(&key), key.as_str()) {
            (Some(i), _) => {
                *page = Page::Settings { selected: i };
                flips.push(i);
            }
            (None, "page") => *page = Page::Settings { selected: 0 },
            (None, _) => warn!("NVF_PLAY_TEST_SETTINGS: no switch {key:?}; there are {:?}", settings.list().iter().map(|s| s.key).collect::<Vec<_>>()),
        }
    }
    let mut clicked = None;
    for (interaction, b) in &buttons {
        match (interaction, b, &mut *page) {
            (Interaction::Pressed, ..) => clicked = Some(*b),
            // The row under the pointer is the chosen one.
            (Interaction::Hovered, PauseButton::Switch(i), Page::Settings { selected }) => *selected = *i,
            _ => {}
        }
    }
    match &mut *page {
        Page::Main => {
            if keys.just_pressed(KeyCode::KeyQ) || clicked == Some(PauseButton::Quit) {
                info!("pause menu: quit");
                exit.write(AppExit::Success);
            } else if keys.just_pressed(KeyCode::KeyS) || clicked == Some(PauseButton::Settings) {
                info!("pause menu: settings");
                *page = Page::Settings { selected: 0 };
            } else if keys.any_just_pressed([KeyCode::Escape, KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || clicked == Some(PauseButton::Resume) {
                info!("pause menu: resume");
                next.set(PlayState::Playing);
            }
        }
        Page::Settings { selected } => {
            let rows = settings.list().len();
            if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) {
                *page = Page::Main;
            } else if let Some(PauseButton::Switch(i)) = clicked {
                *selected = i;
                flips.push(i);
            } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
                *selected = step_row(*selected, -1, rows);
            } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
                *selected = step_row(*selected, 1, rows);
            } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space, KeyCode::ArrowLeft, KeyCode::ArrowRight]) && *selected < rows {
                flips.push(*selected);
            }
        }
    }
    for i in flips {
        commands.queue(move |world: &mut World| {
            crate::settings::flip(world, i);
        });
    }
    // Another page: draw it. A change of row or of a switch only repaints (`show_switches`).
    if std::mem::discriminant(&before) != std::mem::discriminant(&*page) {
        for e in &roots {
            commands.entity(e).despawn();
        }
        spawn_page(&mut commands, *page, &settings);
    }
}

/// On the settings page: show each switch as the game has it now, mark the chosen row
/// and explain its switch.
fn show_switches(
    world: &mut World,
    states: &mut QueryState<(&SwitchState, &mut Text, &mut TextColor), Without<SwitchNote>>,
    rows: &mut QueryState<(&PauseButton, &mut BackgroundColor)>,
    notes: &mut QueryState<&mut Text, With<SwitchNote>>,
) {
    let Page::Settings { selected } = *world.resource::<Page>() else { return };
    let list = world.resource::<Settings>().list().to_vec();
    let on: Vec<bool> = list.iter().map(|s| (s.get)(world)).collect();
    for (s, mut text, mut color) in states.iter_mut(world) {
        let Some(&on) = on.get(s.0) else { continue };
        let (label, shade) = if on { ("On", ON) } else { ("Off", DIM) };
        if text.0 != label {
            text.0 = label.into();
            color.0 = shade;
        }
    }
    for (b, mut bg) in rows.iter_mut(world) {
        let PauseButton::Switch(i) = b else { continue };
        let shade = if *i == selected { ROW_CHOSEN } else { ROW };
        if bg.0 != shade {
            bg.0 = shade;
        }
    }
    let note = list.get(selected).map_or("", |s| s.note);
    for mut text in notes.iter_mut(world) {
        if text.0 != note {
            text.0 = note.into();
        }
    }
}

fn close(mut commands: Commands, mut paused: ResMut<Paused>, mut page: ResMut<Page>, roots: Query<Entity, With<PauseRoot>>) {
    paused.0 = false;
    *page = Page::Main;
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
        assert_eq!(a, PauseAid { open: vec![20.0, 30.0], resume: vec![24.0, 31.5], switches: vec![] });
        assert!(!fire(&mut a.open, 19.9));
        assert!(fire(&mut a.open, 20.0));
        assert!(!fire(&mut a.open, 20.1));
        assert!(fire(&mut a.open, 40.0) && a.open.is_empty());
        assert!(parse_pause_aid("20").is_err());
        assert!(parse_pause_aid("24:20").is_err());
        assert!(parse_pause_aid("x:20").is_err());
        assert_eq!(parse_pause_aid("").unwrap(), PauseAid::default());
    }

    #[test]
    fn the_settings_aid_parses() {
        assert_eq!(parse_settings_aid("21:page, 22.5:menu-pause").unwrap(), vec![(21.0, "page".to_string()), (22.5, "menu-pause".to_string())]);
        assert!(parse_settings_aid("21").is_err());
        assert!(parse_settings_aid("x:voice").is_err());
        assert!(parse_settings_aid("").unwrap().is_empty());
    }

    #[test]
    fn the_keys_go_round_the_rows() {
        assert_eq!(step_row(0, 1, 3), 1);
        assert_eq!(step_row(2, 1, 3), 0);
        assert_eq!(step_row(0, -1, 3), 2);
        assert_eq!(step_row(7, -1, 3), 1);
        assert_eq!(step_row(0, 1, 0), 0);
    }

    #[derive(Resource, Default)]
    struct Lamp(bool);

    #[test]
    fn the_settings_page_shows_each_switch_as_the_game_has_it() {
        let mut app = App::new();
        app.init_resource::<Lamp>().init_resource::<Paused>().insert_resource(Page::Settings { selected: 0 });
        add_setting(&mut app, Setting { key: "lamp", group: "Test", label: "Lamp", note: "A lamp.", get: |w| w.resource::<Lamp>().0, set: |w, on| w.resource_mut::<Lamp>().0 = on });
        app.add_systems(Update, show_switches);
        app.world_mut().run_system_cached(|mut commands: Commands, settings: Res<Settings>| spawn_page(&mut commands, Page::Settings { selected: 0 }, &settings)).unwrap();
        let state = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            q.single(world).unwrap().1 .0.clone()
        };
        assert_eq!(state(&mut app), "Off");
        crate::settings::flip(app.world_mut(), 0);
        assert_eq!(state(&mut app), "On");
        let world = app.world_mut();
        let mut note = world.query_filtered::<&Text, With<SwitchNote>>();
        assert_eq!(note.single(world).unwrap().0, "A lamp.");
    }
}
