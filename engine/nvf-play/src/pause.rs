//! The pause menu: Esc in the world opens it instead of quitting (the original opens its
//! pause menu there [observed by the coordinator playing the original, 2026-10-09]; its
//! layout and entries are not copied: NVForever offers Resume, Save, Load, Settings and
//! Quit). It is a `PlayState::Menu` like the container and inventory menus, so the world,
//! the game clock and mouse look stop and the cursor is released (`crate::menu`). Esc,
//! Enter or Space resume, N saves, L opens the load page, S the settings, Q quits; the
//! buttons are clickable.
//!
//! **Save** (N) saves to a new slot named `Save <n>` (one more than the highest such name
//! in the saves folder) through `crate::save::SaveRequest`; the line under the buttons
//! says when it is written. **The load page** (L) lists the saves newest first
//! (`fnv_save::list`): name, kind and game date in the rows, and for the chosen one its
//! thumbnail, player and level, place, game date, play time and when it was saved.
//! Up/Down (W/S) or the mouse wheel choose, the pointer chooses the row it moves onto,
//! Enter, Space or a click load it through `crate::save::LoadRequest` (nvforever starts
//! again with `--load`); Esc or Backspace go back. Only the chosen save's thumbnail is
//! read and decoded.
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
//! `NVF_PLAY_TEST_PAUSE_MENU=<seconds>:<action>[,…]`: with the pause menu open, `save` (the
//! Save button), `load` (the load page), `down`, `up`, `pick` (Enter on the load page),
//! `back` (from the load or the settings page).

use crate::settings::{add_setting, Setting, Settings};
use crate::{PlaySet, PlayState};
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use fnv_save::{SaveHeader, SaveKind};
use std::path::{Path, PathBuf};

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
    /// `NVF_PLAY_TEST_PAUSE_MENU`: save, load, down, up, pick, back.
    actions: Vec<(f32, String)>,
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
pub enum Page {
    #[default]
    Main,
    Settings {
        selected: usize,
    },
    /// The load page; the chosen save (index in [`LoadList`]).
    Load {
        selected: usize,
    },
}

impl Page {
    /// On the settings page, the chosen row (`crate::ui_sounds` hears it move).
    pub(crate) fn settings_row(self) -> Option<usize> {
        match self {
            Page::Main | Page::Load { .. } => None,
            Page::Settings { selected } => Some(selected),
        }
    }
}

/// The saves the load page lists, newest first, and the chosen one's thumbnail.
#[derive(Resource, Default)]
struct LoadList {
    saves: Vec<(PathBuf, SaveHeader)>,
    /// The thumbnail of the save at this index (None: it has none or it did not decode).
    thumb: Option<(usize, Option<Handle<Image>>)>,
}

impl LoadList {
    fn open(&mut self, dir: Option<&Path>) {
        self.saves = dir.map(fnv_save::list).unwrap_or_default();
        self.thumb = None;
        let names: Vec<String> = self.saves.iter().take(8).map(|(p, h)| format!("{:?} ({})", stem(p), kind_name(h.kind))).collect();
        info!("pause menu: load page: {} save(s), newest first: {}", self.saves.len(), names.join(", "));
    }

    /// Read and decode the thumbnail of save `i` unless it is the one held.
    fn ensure_thumb(&mut self, i: usize, images: &mut Assets<Image>) {
        if self.thumb.as_ref().is_some_and(|(t, _)| *t == i) {
            return;
        }
        let Some((path, _)) = self.saves.get(i) else { return };
        let decoded = fnv_save::read_header(path).ok().and_then(|f| crate::save::decode_thumbnail(&f.shot));
        let handle = decoded.map(|(w, h, rgba)| {
            info!("pause menu: load page row {i} {:?}: thumbnail {w}x{h}", stem(path));
            let size = bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
            images.add(Image::new(size, bevy::render::render_resource::TextureDimension::D2, rgba, bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb, bevy::asset::RenderAssetUsages::default()))
        });
        if handle.is_none() {
            info!("pause menu: load page row {i} {:?}: no thumbnail", stem(path));
        }
        self.thumb = Some((i, handle));
    }
}

/// The line under the main page's buttons: how the save asked from this menu went.
#[derive(Resource, Default)]
struct PauseStatus {
    waiting_for: Option<PathBuf>,
    line: String,
}

#[derive(Component)]
struct PauseStatusText;

/// What the Save button and the load page use.
#[derive(SystemParam)]
struct SaveMenu<'w> {
    list: ResMut<'w, LoadList>,
    images: ResMut<'w, Assets<Image>>,
    cfg: Res<'w, crate::save::SaveConfig>,
    saves: MessageWriter<'w, crate::save::SaveRequest>,
    loads: MessageWriter<'w, crate::save::LoadRequest>,
    status: ResMut<'w, PauseStatus>,
    motion: Option<Res<'w, AccumulatedMouseMotion>>,
    scroll: Option<Res<'w, AccumulatedMouseScroll>>,
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

fn kind_name(k: SaveKind) -> &'static str {
    match k {
        SaveKind::Manual => "manual",
        SaveKind::Quick => "quick",
        SaveKind::Auto => "auto",
    }
}

/// The name of a new save: `Save <n>`, one more than the highest such name in `stems`.
fn next_save_name<'a>(stems: impl IntoIterator<Item = &'a str>) -> String {
    let n = stems.into_iter().filter_map(|s| s.strip_prefix("Save ")).filter_map(|n| n.parse::<u32>().ok()).max().unwrap_or(0);
    format!("Save {}", n + 1)
}

/// Play time as hours and minutes.
fn play_time(seconds: f64) -> String {
    let m = (seconds.max(0.0) / 60.0) as u64;
    if m < 60 {
        format!("{m} min")
    } else {
        format!("{} h {:02} min", m / 60, m % 60)
    }
}

/// How long ago a save was made (real time).
fn saved_ago(saved_unix: u64, now_unix: u64) -> String {
    let s = now_unix.saturating_sub(saved_unix);
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86400 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86400),
    }
}

#[derive(Component)]
struct PauseRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum PauseButton {
    Resume,
    Settings,
    Quit,
    Save,
    Load,
    /// From the settings or load page back to the first page.
    Back,
    /// The row of a switch (its index in [`Settings`]).
    Switch(usize),
    /// A save's row on the load page (its index in [`LoadList`]).
    LoadRow(usize),
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
/// Up to this many switches the settings page has one column, beyond it two of this width.
const ONE_COLUMN_ROWS: usize = 14;
const COLUMN_WIDTH: f32 = 470.0;
/// The load page: rows shown at once, the list's width, the thumbnail as drawn.
const LOAD_ROWS: usize = 10;
const LOAD_LIST_WIDTH: f32 = 420.0;
const THUMB_DRAWN: Vec2 = Vec2::new(448.0, 252.0);
/// The load page's rows and details: room for LOAD_ROWS rows and the two "more" lines.
const LOAD_HEIGHT: f32 = 400.0;

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
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_PAUSE_MENU") {
            match parse_settings_aid(&v) {
                Ok(s) => aid.actions = s,
                Err(e) => warn!("NVF_PLAY_TEST_PAUSE_MENU: {e}"),
            }
        }
        app.init_resource::<Paused>()
            .init_resource::<PageRequest>()
            .init_resource::<Page>()
            .init_resource::<LoadList>()
            .init_resource::<PauseStatus>()
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
                    (choose, show_switches, show_status).chain().run_if(in_state(PlayState::Menu)),
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
fn spawn_page(commands: &mut Commands, page: Page, settings: &Settings, list: &LoadList, status: &str) {
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
                p.spawn(button("Save game (N)", PauseButton::Save));
                p.spawn(button("Load game (L)", PauseButton::Load));
                p.spawn(button("Settings (S)", PauseButton::Settings));
                p.spawn(button("Quit NVForever (Q)", PauseButton::Quit));
                p.spawn((PauseStatusText, Text::new(status), font(14.0), TextColor(DIM)));
            }
            Page::Settings { selected } => settings_page(p, settings, selected),
            Page::Load { selected } => load_page(p, list, selected),
        });
    });
}

/// The settings page: a row per switch under its group's heading, the chosen switch's
/// note, the keys. [`show_switches`] fills in what changes: each switch's state, the
/// chosen row, the note.
fn settings_page(p: &mut ChildSpawnerCommands, settings: &Settings, selected: usize) {
    p.spawn((Text::new("Settings"), font(26.0), TextColor(TITLE)));
    let list = settings.list();
    // A long list goes into two columns, split between groups near its middle.
    let split = if list.len() > ONE_COLUMN_ROWS {
        (1..list.len()).filter(|&i| list[i].group != list[i - 1].group).min_by_key(|&i| i.abs_diff(list.len() / 2)).unwrap_or(list.len())
    } else {
        list.len()
    };
    let width = if split < list.len() { COLUMN_WIDTH } else { SETTINGS_WIDTH };
    p.spawn(Node { column_gap: Val::Px(20.0), align_items: AlignItems::FlexStart, ..default() }).with_children(|columns| {
        for range in [0..split, split..list.len()] {
            if range.is_empty() {
                continue;
            }
            let column = Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), width: Val::Px(width), ..default() };
            columns.spawn(column).with_children(|rows| {
                let mut group = "";
                for (i, s) in list.iter().enumerate().skip(range.start).take(range.len()) {
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
            });
        }
        if list.is_empty() {
            columns.spawn((Node { width: Val::Px(SETTINGS_WIDTH), ..default() }, children![(Text::new("Nothing to switch."), font(16.0), TextColor(DIM))]));
        }
    });
    // Four lines high whatever the note, so the panel keeps its size from row to row.
    let note = settings.list().get(selected).map_or("", |s| s.note);
    p.spawn((Node { width: Val::Px(SETTINGS_WIDTH), min_height: Val::Px(68.0), ..default() }, children![(SwitchNote, Text::new(note), font(13.0), TextColor(DIM))]));
    p.spawn((Text::new("Up/Down choose   Enter or Space switch   Esc back"), font(13.0), TextColor(DIM)));
    p.spawn(button("Back (Esc)", PauseButton::Back));
}

/// The load page: the saves' rows (a window of [`LOAD_ROWS`] around the chosen one) and
/// the chosen save's thumbnail and details.
fn load_page(p: &mut ChildSpawnerCommands, list: &LoadList, selected: usize) {
    p.spawn((Text::new("Load"), font(26.0), TextColor(TITLE)));
    if list.saves.is_empty() {
        p.spawn((Text::new("No saves yet."), font(16.0), TextColor(DIM)));
    } else {
        let n = list.saves.len();
        let first = selected.saturating_sub(LOAD_ROWS / 2).min(n.saturating_sub(LOAD_ROWS));
        // A set height: left to its content, the row came out far taller than the rows and
        // the details in it [observed 2026-10-15].
        p.spawn(Node { column_gap: Val::Px(20.0), align_items: AlignItems::FlexStart, height: Val::Px(LOAD_HEIGHT), ..default() }).with_children(|cols| {
            cols.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), width: Val::Px(LOAD_LIST_WIDTH), ..default() }).with_children(|rows| {
                if first > 0 {
                    rows.spawn((Text::new(format!("{first} newer above")), font(13.0), TextColor(DIM)));
                }
                for (i, (path, h)) in list.saves.iter().enumerate().skip(first).take(LOAD_ROWS) {
                    let row = Node { justify_content: JustifyContent::SpaceBetween, column_gap: Val::Px(12.0), padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), ..default() };
                    rows.spawn((Button, PauseButton::LoadRow(i), row, BackgroundColor(if i == selected { ROW_CHOSEN } else { ROW }))).with_children(|r| {
                        r.spawn((Text::new(stem(path)), font(16.0), TextColor(TEXT)));
                        r.spawn((Text::new(format!("{}  {}", kind_name(h.kind), crate::save::game_date(h))), font(14.0), TextColor(DIM)));
                    });
                }
                let below = n - (first + LOAD_ROWS).min(n);
                if below > 0 {
                    rows.spawn((Text::new(format!("{below} older below")), font(13.0), TextColor(DIM)));
                }
            });
            cols.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), width: Val::Px(THUMB_DRAWN.x), ..default() }).with_children(|d| {
                let frame = Node { width: Val::Px(THUMB_DRAWN.x), height: Val::Px(THUMB_DRAWN.y), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() };
                match list.thumb.as_ref().filter(|(t, _)| *t == selected).and_then(|(_, h)| h.clone()) {
                    Some(img) => {
                        // Stretched to the frame: no measure of its own, so the node keeps the frame's size.
                        d.spawn((frame, ImageNode::new(img).with_mode(bevy::ui::widget::NodeImageMode::Stretch)));
                    }
                    None => {
                        d.spawn((frame, BackgroundColor(ROW), children![(Text::new("No picture"), font(14.0), TextColor(DIM))]));
                    }
                }
                if let Some((path, h)) = list.saves.get(selected) {
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                    d.spawn((Text::new(stem(path)), font(18.0), TextColor(TITLE)));
                    for line in [
                        format!("{}, level {}", h.player_name, h.level),
                        if h.location.is_empty() { "Unknown place".to_string() } else { h.location.clone() },
                        format!("Game date {}", crate::save::game_date(h)),
                        format!("Played {}", play_time(h.play_seconds)),
                        format!("Saved {} ({})", saved_ago(h.saved_unix, now), kind_name(h.kind)),
                    ] {
                        d.spawn((Text::new(line), font(14.0), TextColor(TEXT)));
                    }
                }
            });
        });
    }
    p.spawn((Text::new("Up/Down choose   Enter load   Esc back"), font(13.0), TextColor(DIM)));
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
    list: Res<LoadList>,
    mut status: ResMut<PauseStatus>,
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
    *status = PauseStatus::default();
    next.set(PlayState::Menu);
    spawn_page(&mut commands, *page, &settings, &list, "");
}

/// A page another menu asks the pause menu to show (the main menu's Load Game and
/// Settings, `crate::main_menu`); taken on the next frame.
#[derive(Resource, Default)]
pub struct PageRequest(pub Option<Page>);

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
    mut sounds: MessageWriter<crate::ui_sounds::UiSound>,
    mut menu: SaveMenu,
    (mut request, main_menu): (ResMut<PageRequest>, Option<Res<crate::main_menu::MainMenu>>),
) {
    if !paused.0 {
        return;
    }
    // Under the main menu the first page is the main menu's own (`crate::main_menu`): its
    // pages come from here, and going back shows it again.
    let under_main = main_menu.is_some_and(|m| m.open);
    let mut redraw = false;
    if let Some(p) = request.0.take() {
        if matches!(p, Page::Load { .. }) {
            menu.list.open(menu.cfg.dir.as_deref());
        }
        info!("pause menu: {p:?} page for another menu");
        *page = p;
        redraw = true;
    }
    if under_main && *page == Page::Main {
        if redraw {
            for e in &roots {
                commands.entity(e).despawn();
            }
        }
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
    // The menu aid's actions due now, as the keys they stand for.
    let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut aid.actions).into_iter().partition(|(t, _)| *t <= now);
    aid.actions = later;
    let mut acts: Vec<String> = Vec::new();
    for (_, a) in due {
        info!("test aid NVF_PLAY_TEST_PAUSE_MENU: {a}");
        acts.push(a);
    }
    let act = |name: &str| acts.iter().any(|a| a == name);
    let mut clicked = None;
    // The load page takes the row the pointer moves onto (not one it rests on while the
    // keys move the choice, nor the one a redrawn page puts under it).
    let moved = menu.motion.as_ref().is_some_and(|m| m.delta != Vec2::ZERO);
    for (interaction, b) in &buttons {
        match (interaction, b, &mut *page) {
            (Interaction::Pressed, ..) => clicked = Some(*b),
            // The row under the pointer is the chosen one.
            (Interaction::Hovered, PauseButton::Switch(i), Page::Settings { selected }) => *selected = *i,
            (Interaction::Hovered, PauseButton::LoadRow(i), Page::Load { selected }) if moved => *selected = *i,
            _ => {}
        }
    }
    match &mut *page {
        Page::Main => {
            if keys.just_pressed(KeyCode::KeyQ) || clicked == Some(PauseButton::Quit) {
                info!("pause menu: quit");
                exit.write(AppExit::Success);
            } else if keys.just_pressed(KeyCode::KeyN) || clicked == Some(PauseButton::Save) || act("save") {
                let stems: Vec<String> = menu.cfg.dir.as_deref().and_then(|d| std::fs::read_dir(d).ok()).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == fnv_save::EXTENSION)).map(|p| stem(&p)).collect();
                let name = next_save_name(stems.iter().map(String::as_str));
                info!("pause menu: saving as {name:?}");
                match menu.cfg.path_of(&name) {
                    Ok(path) => {
                        menu.status.waiting_for = Some(path);
                        menu.status.line = format!("Saving \"{name}\"...");
                        menu.saves.write(crate::save::SaveRequest { name, kind: SaveKind::Manual });
                    }
                    Err(e) => menu.status.line = format!("Cannot save: {e}"),
                }
            } else if keys.just_pressed(KeyCode::KeyL) || clicked == Some(PauseButton::Load) || act("load") {
                info!("pause menu: load page");
                menu.list.open(menu.cfg.dir.as_deref());
                *page = Page::Load { selected: 0 };
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
            if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) || act("back") {
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
        Page::Load { selected } => {
            let rows = menu.list.saves.len();
            let wheel = menu.scroll.as_ref().map_or(0.0, |s| s.delta.y);
            let mut pick = None;
            if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) || act("back") {
                *page = Page::Main;
            } else if let Some(PauseButton::LoadRow(i)) = clicked {
                *selected = i;
                pick = Some(i);
            } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) || wheel > 0.0 || act("up") {
                *selected = (*selected).min(rows.saturating_sub(1)).saturating_sub(1);
            } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) || wheel < 0.0 || act("down") {
                *selected = (*selected + 1).min(rows.saturating_sub(1));
            } else if (keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || act("pick")) && *selected < rows {
                pick = Some(*selected);
            }
            if let Some((path, _)) = pick.and_then(|i| menu.list.saves.get(i)) {
                info!("pause menu: load {}", path.display());
                menu.loads.write(crate::save::LoadRequest { path: path.clone() });
            }
        }
    }
    if let Page::Load { selected } = *page {
        let SaveMenu { list, images, .. } = &mut menu;
        list.ensure_thumb(selected, images);
    }
    for i in flips {
        sounds.write(crate::ui_sounds::UiSound { menu: crate::ui_sounds::UiMenu::Settings, event: crate::ui_sounds::UiEvent::Flipped });
        commands.queue(move |world: &mut World| {
            crate::settings::flip(world, i);
        });
    }
    // Another page, or another save chosen on the load page: draw it. On the settings page
    // a change of row or of a switch only repaints (`show_switches`).
    if redraw || std::mem::discriminant(&before) != std::mem::discriminant(&*page) || (matches!(*page, Page::Load { .. }) && before != *page) {
        for e in &roots {
            commands.entity(e).despawn();
        }
        if !(under_main && *page == Page::Main) {
            spawn_page(&mut commands, *page, &settings, &menu.list, &menu.status.line);
        }
    }
}

/// The main page's status line: the save asked from this menu, once written.
fn show_status(mut done: MessageReader<crate::save::SaveDone>, mut status: ResMut<PauseStatus>, mut texts: Query<&mut Text, With<PauseStatusText>>) {
    for d in done.read() {
        if status.waiting_for.as_ref() != Some(&d.path) {
            continue;
        }
        status.waiting_for = None;
        status.line = match &d.result {
            Ok(()) => format!("Saved \"{}\"", stem(&d.path)),
            Err(e) => format!("Save failed: {e}"),
        };
    }
    for mut t in &mut texts {
        if t.0 != status.line {
            t.0 = status.line.clone();
        }
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

fn close(mut commands: Commands, mut paused: ResMut<Paused>, mut page: ResMut<Page>, mut list: ResMut<LoadList>, roots: Query<Entity, With<PauseRoot>>) {
    paused.0 = false;
    *page = Page::Main;
    *list = LoadList::default();
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
        assert_eq!(a, PauseAid { open: vec![20.0, 30.0], resume: vec![24.0, 31.5], ..default() });
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

    #[test]
    fn a_new_save_takes_the_next_number() {
        assert_eq!(next_save_name([]), "Save 1");
        assert_eq!(next_save_name(["quicksave", "Save 2", "Save 10", "Save x", "autosave"]), "Save 11");
    }

    #[test]
    fn the_load_page_says_times_plainly() {
        assert_eq!(play_time(59.0), "0 min");
        assert_eq!(play_time(3.0 * 3600.0 + 7.0 * 60.0), "3 h 07 min");
        assert_eq!(saved_ago(1000, 1030), "just now");
        assert_eq!(saved_ago(1000, 1000 + 5 * 60), "5 min ago");
        assert_eq!(saved_ago(1000, 1000 + 2 * 86400), "2 days ago");
        assert_eq!(saved_ago(2000, 1000), "just now");
    }

    #[test]
    fn the_load_page_lists_the_saves_and_shows_the_chosen_one() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>();
        let mut list = LoadList::default();
        for (name, t) in [("Save 2", 20), ("autosave", 10)] {
            let mut h = SaveHeader::new(if name == "autosave" { SaveKind::Auto } else { SaveKind::Manual });
            h.saved_unix = t;
            h.player_name = "Courier".into();
            h.location = "Goodsprings".into();
            list.saves.push((PathBuf::from(format!("{name}.nvfsave")), h));
        }
        app.insert_resource(list);
        app.init_resource::<Settings>();
        app.world_mut().run_system_cached(|mut commands: Commands, settings: Res<Settings>, list: Res<LoadList>| spawn_page(&mut commands, Page::Load { selected: 1 }, &settings, &list, "")).unwrap();
        let world = app.world_mut();
        let mut rows = world.query::<(&PauseButton, &BackgroundColor)>();
        let rows: Vec<(PauseButton, Color)> = rows.iter(world).filter(|(b, _)| matches!(b, PauseButton::LoadRow(_))).map(|(b, c)| (*b, c.0)).collect();
        assert_eq!(rows.len(), 2);
        assert!(rows.contains(&(PauseButton::LoadRow(1), ROW_CHOSEN)));
        let mut texts = world.query::<&Text>();
        let all: Vec<String> = texts.iter(world).map(|t| t.0.clone()).collect();
        assert!(all.iter().any(|t| t == "Courier, level 1"), "{all:?}");
        assert!(all.iter().any(|t| t == "No picture"), "{all:?}");
    }

    #[derive(Resource, Default)]
    struct Lamp(bool);

    #[test]
    fn the_settings_page_shows_each_switch_as_the_game_has_it() {
        let mut app = App::new();
        app.init_resource::<Lamp>().init_resource::<Paused>().insert_resource(Page::Settings { selected: 0 });
        add_setting(&mut app, Setting { key: "lamp", group: "Test", label: "Lamp", note: "A lamp.", get: |w| w.resource::<Lamp>().0, set: |w, on| w.resource_mut::<Lamp>().0 = on });
        app.add_systems(Update, show_switches);
        app.world_mut().run_system_cached(|mut commands: Commands, settings: Res<Settings>| spawn_page(&mut commands, Page::Settings { selected: 0 }, &settings, &LoadList::default(), "")).unwrap();
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
