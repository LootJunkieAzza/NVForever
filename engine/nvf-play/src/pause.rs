//! The pause menu: Esc in the world opens it instead of quitting (the original opens its
//! pause menu there [observed by the coordinator playing the original, 2026-10-09]). It is
//! a `PlayState::Menu` like the container and inventory menus, so the world, the game
//! clock and mouse look stop and the cursor is released (`crate::menu`).
//!
//! **The look is the original's** (`crate::start_menu`, the main menu shares it) [owner,
//! todo 5 item 8; observed: the owner's screenshots of the original's pause menu, its
//! settings pages and its quit question, C:\fnv-private\Pause Menu; start_menu.xml]: the
//! pause screen over the held world, the column of entries at the right (Continue, Save,
//! Load, Settings, Quit; the original's Help is left out), the highlight box round the
//! chosen one. A page opened from the column dims the column but for its own entry; a
//! click on that entry again closes the page (back to the column), and a click on Quit
//! while its question is up cancels it [owner 2026-10-16: the entries toggle their page].
//! Up/Down choose, Enter or Space take, a click takes, the pointer chooses the entry it
//! moves onto; Esc resumes; N saves, L opens the load page, S the settings, Q the quit
//! question.
//!
//! **Save** (N) saves to a new slot named `Save <n>` (one more than the highest such name
//! in the saves folder) through `crate::save::SaveRequest`; the line under the column
//! says when it is written. **The load page** (L) shows the chosen save's thumbnail and
//! details (player and level, place and game date, play time, when it was saved) over the
//! saves' rows, newest first (`fnv_save::list`), left of the column. Up/Down (W/S) or the
//! mouse wheel choose, the pointer chooses the row it moves onto, Enter, Space or a click
//! load it through `crate::save::LoadRequest` (nvforever starts again with `--load`); Esc,
//! Backspace or Back go back. Only the chosen save's thumbnail is read and decoded.
//!
//! **The settings page** lists the on/off switches of [`crate::settings`] the original's
//! way: their groups as categories left of the column (Gameplay, World, Display,
//! Graphics, Debug: NVForever's switches, not the original's), the chosen category's
//! switches in the middle over the darkened band, each with ON or OFF as the game is
//! using it now, the chosen switch's note under them, Defaults and Back at the bottom
//! right, the version at the bottom left. Up/Down (or W/S) choose a row within the
//! category, Tab or Page Down the next category (Page Up the one before), a click on a
//! category shows it; Enter, Space, Left or Right flip the row, as does a click on it; a
//! flip acts at once and is kept in `NVForever.ini` (`crate::settings`). Defaults puts
//! back every switch as the command line left it before the INI was read
//! (`crate::settings::StartStates`; without it, as the page was first shown in this run).
//! Esc or Backspace go back.
//!
//! **The Controls tab**, the settings page's last category [design; owner 2026-10-16:
//! keys can be mapped from the settings], lists every control of `crate::bindings` with
//! its key and mouse button. Up/Down (W/S) choose; Enter, Space or a click on a row make
//! it wait for its new key ("Press a key..."): the next key or mouse button pressed is
//! bound (`crate::bindings::rebind`: a control that had it gets the old one), Esc
//! cancels. Delete clears the row's key; Defaults puts back the game's keys. Each change
//! acts at once and is kept in `[Controls]` of `NVForever.ini`. Tab after the last
//! category goes to Controls and from there to the first again.
//!
//! **Quit** asks first [observed: the original's question and its choices]: Main Menu
//! (nvforever starts again with `--menu`, [`MainMenuRequest`]), Exit Game, Cancel.
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
//! click on its row); the key `page` only shows the page, `controls` the Controls tab,
//! and `bind:<control>=<input>` (INI names, e.g. `bind:Jump=J`) binds a control there as
//! Enter on its row and that key would.
//! `NVF_PLAY_TEST_PAUSE_MENU=<seconds>:<action>[,…]`: with the pause menu open, `save` (the
//! Save button), `load` (the load page), `down`, `up`, `pick` (Enter on the load page or
//! the quit question), `back` (from the load or the settings page, or the question),
//! `defaults` (the settings page's Defaults),
//! `quit` (the question), `shot` (a screenshot `pause_<page>.png` in the working folder).

use crate::interface::{bc_supported, u, Face};
use crate::settings::{add_setting, Setting, Settings};
use crate::start_menu::{color, highlight_colors, text as sm_text, wrapped, Faces, Shade, BASE, COLUMN, ITEM, LINE, RIGHT, SUB_RIGHT, SUB_WIDTH};
use crate::{PlaySet, PlayState};
use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use fnv_save::{SaveHeader, SaveKind};
use nvf_assets::GameData;
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

/// Pause the virtual clock while the picture is held ([`picture_held`]), or while the
/// console's free camera holds the world (`crate::free_cam`, mode 1), and let it run
/// again after. Nothing else pauses it.
fn freeze_picture(mode: Res<MenuPause>, paused: Res<Paused>, state: Res<State<PlayState>>, free_cam: Option<Res<crate::free_cam::FreeCam>>, mut clock: ResMut<Time<Virtual>>) {
    let held_by_cam = free_cam.is_some_and(|f| f.holds_world());
    let stop = picture_held(*mode, paused.0, *state.get()) || held_by_cam;
    if clock.is_paused() != stop {
        let why = if paused.0 {
            "the pause menu"
        } else if held_by_cam {
            "the free camera"
        } else {
            "--menu-pause original"
        };
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
    /// The settings page's Controls tab: the chosen control (index in
    /// `crate::bindings::GAME_CONTROLS`), and whether it waits for its new key.
    Controls {
        selected: usize,
        listening: bool,
    },
}

impl Page {
    /// On the settings page, the chosen row (`crate::ui_sounds` hears it move).
    pub(crate) fn settings_row(self) -> Option<usize> {
        match self {
            Page::Main | Page::Load { .. } => None,
            Page::Settings { selected } => Some(selected),
            Page::Controls { selected, .. } => Some(CONTROL_ROWS_BASE + selected),
        }
    }

    /// Whether `label`, an entry of the column, is the one this page stands for: a click
    /// on it closes the page again [owner 2026-10-16].
    fn opened_by(self, label: &str) -> bool {
        match self {
            Page::Main => false,
            Page::Settings { .. } | Page::Controls { .. } => label.starts_with("Settings"),
            Page::Load { .. } => label.starts_with("Load"),
        }
    }
}

/// [`Page::settings_row`] of a control's row: past any switch's.
const CONTROL_ROWS_BASE: usize = 10_000;

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
    /// An entry of the column at the right (its index).
    Column(usize),
    /// A category of the settings page (the index of its first switch).
    Category(usize),
    /// The row of a switch (its index in [`Settings`]).
    Switch(usize),
    /// The settings page's Controls category.
    ControlsTab,
    /// A control's row on the Controls tab (its index in `GAME_CONTROLS`).
    ControlRow(usize),
    /// A save's row on the load page (its index in [`LoadList`]).
    LoadRow(usize),
    /// The settings page's Defaults.
    Defaults,
    /// From the settings or load page back to the first page.
    Back,
    /// A choice of the quit question (its index in [`CONFIRM`]).
    Confirm(usize),
}

/// A switch's state, kept as text too (tests read it).
#[derive(Component)]
struct SwitchState(#[allow(dead_code)] usize);

/// The chosen switch's note, kept as text too (tests read it).
#[derive(Component)]
struct SwitchNote;

/// The column's entries in the world [observed: the owner's screenshots of the original's
/// pause menu: Continue, Save, Load, Settings, Help, Quit; Help is left out, NVForever has
/// no help pages].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Continue,
    Save,
    Load,
    Settings,
    Quit,
}

const ENTRIES: [Entry; 5] = [Entry::Continue, Entry::Save, Entry::Load, Entry::Settings, Entry::Quit];

impl Entry {
    fn label(self) -> &'static str {
        match self {
            Entry::Continue => "Continue",
            Entry::Save => "Save",
            Entry::Load => "Load",
            Entry::Settings => "Settings",
            Entry::Quit => "Quit",
        }
    }
}

/// The quit question and its choices [observed: the owner's screenshot of the original's
/// quit question].
const WARNING: &str = "WARNING:  Any unsaved progress will be lost.";
const QUESTION: &str = "Are you sure you want to quit?";
const CONFIRM: [&str; 3] = ["Main Menu", "Exit Game", "Cancel"];

/// The settings page's rows: this many shown at once [documented: `options_container`
/// `_number_of_visible_items` 9], each this high [observed: 39 of 720 pixels], the list
/// this wide and its left edge this far left of the view's middle, the switch's state
/// centred this far into a row [design: the original's list is 520 wide and centred, its
/// values at 360; NVForever's labels are longer].
const OPTION_ROWS: usize = 9;
/// The settings page's last category: the controls' keys [design].
const CONTROLS_TAB: &str = "Controls";
/// A control's value while it waits for its new key.
const WAITING: &str = "Press a key...";
const OPTION_ROW: f32 = 52.0;
const OPTIONS_WIDTH: f32 = 820.0;
const OPTIONS_LEFT: f32 = -470.0;
const VALUE_AT: f32 = 700.0;
/// The darkened band behind a page's list [documented: `container_background`
/// `fuzzy.dds` at alpha 160 the height of the screen, a little wider than the list].
const BAND: &str = "textures\\interface\\shared\\background\\fuzzy.dds";
const BAND_ALPHA: f32 = 160.0 / 255.0;
const BAND_MARGIN: f32 = 30.0;
/// The black under the band [design: the band alone left the main menu's logo and
/// pictures showing through, observed 2026-10-16].
const BAND_SHEET: f32 = 0.55;
/// The load page: rows shown at once, the panel's width, its rows' top, the thumbnail and
/// the details' lines [documented: `saveload_container` width 650, y 521; the player's
/// name, level and play time at y 347, 388, 429; the thumbnail above the name; design:
/// eight rows].
const LOAD_ROWS: usize = 8;
const LOAD_WIDTH: f32 = 650.0;
const LOAD_TOP: f32 = 521.0;
const THUMB_DRAWN: Vec2 = Vec2::new(448.0, 252.0);
const DETAILS_TOP: f32 = 347.0;
const DETAILS_STEP: f32 = 41.0;
/// The page's Defaults and Back at the bottom right [observed: the owner's screenshots].
const BACK_TOP: f32 = 960.0 - 30.0 - ITEM;
/// The pause screen behind the menu in the world: the original shows a model,
/// `meshes\interface\pausescreen\pausescreen01.nif`, whose textures are these [observed:
/// the file's texture names]; drawn here flat over the held picture, the half picture
/// at the left and mirrored at the right [hypothesis: the model's two slides], the world
/// behind darkened [design]. The model's glare (`loading_glare.dds`) is left out: drawn
/// over the picture, not added to it, it came out a dark slab [observed 2026-10-16].
const SCREEN: &str = "textures\\interface\\pause\\pausescreen01.dds";
const SCREEN_ALPHA: f32 = 0.82;
/// How much of the pause screen picture lies past the view's edges, as parts of the drawn
/// picture: its outer side (the left of the half picture, mirrored at the right), its top
/// and its bottom, so the frame's edge meets the view's [observed: the owner's screenshot
/// of 2026-10-16 marking where the edge belongs, at about 7% in from the sides, 5% from
/// the top and 9% from the bottom of the stretched picture].
const SCREEN_CROP_SIDE: f32 = 0.147;
const SCREEN_CROP_TOP: f32 = 0.051;
const SCREEN_CROP_BOTTOM: f32 = 0.088;
const WORLD_SHADE: f32 = 0.35;

/// The settings page's group of switches about conversations, sound and menus.
pub const GROUP_MENUS: &str = "Gameplay";

/// The fonts and pictures of the pause menu, read once.
#[derive(Resource)]
struct PauseLook {
    faces: Faces,
    band: Option<Handle<Image>>,
    screen: Option<Handle<Image>>,
}

impl PauseLook {
    fn load(world: &mut World) -> PauseLook {
        let bc = bc_supported(world.get_resource::<CompressedImageFormatSupport>());
        world.resource_scope(|world, mut images: Mut<Assets<Image>>| {
            let data = world.get_resource::<GameData>();
            let pic = |path: &str, images: &mut Assets<Image>| data.and_then(|d| crate::interface::texture(path, d, images, bc)).map(|(h, _)| h);
            let look = PauseLook { faces: Faces::load(data, &mut images, bc), band: pic(BAND, &mut images), screen: pic(SCREEN, &mut images) };
            info!("pause menu: the original's look (band {}, pause screen {})", look.band.is_some(), look.screen.is_some());
            look
        })
    }

    #[cfg(test)]
    fn plain() -> PauseLook {
        PauseLook { faces: Faces::plain(), band: None, screen: None }
    }
}

/// What the keys have on the first page and in the quit question, and what Defaults goes
/// back to (taken when the settings page is first shown).
#[derive(Resource, Default)]
struct PauseView {
    main_selected: usize,
    confirm: Option<usize>,
    defaults: Option<Vec<bool>>,
    drawn: Option<Drawn>,
}

/// Everything the page drawn shows: drawn again when it changes.
#[derive(Clone, PartialEq, Debug)]
struct Drawn {
    page: Page,
    main_selected: usize,
    confirm: Option<usize>,
    states: Vec<bool>,
    status: String,
    /// The column's labels and whether each can be taken.
    column: Vec<(&'static str, bool)>,
    /// Under the main menu (its entries make the column; no pause screen).
    under_main: bool,
    /// The load page's thumbnail: the row it is of and whether there is one.
    thumb: Option<(usize, bool)>,
    /// The Controls tab's values, in `GAME_CONTROLS`' order.
    controls: Vec<String>,
}

impl Drawn {
    /// The column's entry that stands for the page.
    fn page_entry(&self) -> usize {
        let find = |label: &str| self.column.iter().position(|(l, _)| l.starts_with(label)).unwrap_or(0);
        match self.page {
            Page::Main if self.confirm.is_some() => find("Quit"),
            Page::Main => self.main_selected,
            Page::Settings { .. } | Page::Controls { .. } => find("Settings"),
            Page::Load { .. } => find("Load"),
        }
    }
}

/// The settings page's categories: each group's name and its first switch.
fn categories(list: &[Setting]) -> Vec<(&'static str, usize)> {
    let mut out: Vec<(&'static str, usize)> = Vec::new();
    for (i, s) in list.iter().enumerate() {
        if out.last().is_none_or(|(g, _)| *g != s.group) {
            out.push((s.group, i));
        }
    }
    out
}

/// The rows of `selected`'s group: its first and one past its last.
fn group_rows(list: &[Setting], selected: usize) -> std::ops::Range<usize> {
    let Some(g) = list.get(selected).map(|s| s.group) else { return 0..0 };
    let first = list.iter().position(|s| s.group == g).unwrap_or(0);
    let end = list.iter().rposition(|s| s.group == g).map_or(first, |i| i + 1);
    first..end
}

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
            .init_resource::<PauseView>()
            .add_message::<MainMenuRequest>()
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
                    (choose, show_status, draw).chain().run_if(in_state(PlayState::Menu)),
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


fn full() -> Node {
    Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }
}

/// A node `width` wide whose right edge is `right` from the view's.
fn from_right(right: f32, top: f32, width: f32, height: f32) -> Node {
    Node { position_type: PositionType::Absolute, right: u(right), top: u(top), width: u(width), height: u(height), ..default() }
}

/// A node whose left edge is `left` from the view's middle.
fn from_middle(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(u(left)), top: u(top), width: u(width), height: u(height), ..default() }
}

/// A right-aligned list entry: the whole row takes the pointer, the box round it when
/// `boxed`.
#[allow(clippy::too_many_arguments)]
fn right_entry(p: &mut ChildSpawnerCommands, face: &Face, label: &str, node: Node, width: f32, shade: Shade, boxed_on: bool, b: PauseButton) {
    let w = face.font.width(label);
    p.spawn((Button, b, boxed(node, boxed_on))).with_children(|t| sm_text(t, face, label, width - w - 10.0, BASE, color(shade)));
}

/// A node with the highlight box round it when `on`.
fn boxed(node: Node, on: bool) -> (Node, BorderColor, BackgroundColor) {
    let (line, fill) = highlight_colors();
    if on {
        (Node { border: UiRect::all(u(LINE)), ..node }, BorderColor::all(line), BackgroundColor(fill))
    } else {
        (node, BorderColor::all(Color::NONE), BackgroundColor(Color::NONE))
    }
}

/// The darkened band behind a page's list.
fn band(root: &mut ChildSpawnerCommands, look: &PauseLook, node: Node) {
    // Under it a sheet of black, so the list reads over a bright main menu screen too.
    root.spawn((node.clone(), BackgroundColor(Color::BLACK.with_alpha(BAND_SHEET))));
    match &look.band {
        Some(b) => {
            root.spawn((node, ImageNode { color: Color::WHITE.with_alpha(BAND_ALPHA), image_mode: NodeImageMode::Stretch, ..ImageNode::new(b.clone()) }));
        }
        None => {
            root.spawn((node, BackgroundColor(Color::BLACK.with_alpha(BAND_ALPHA * 0.6))));
        }
    }
}

/// Draw the page `d` (the one before it is gone already).
fn spawn_page(commands: &mut Commands, d: &Drawn, look: &PauseLook, settings: &[Setting], list: &LoadList) {
    if d.under_main && d.page == Page::Main {
        return;
    }
    let f = &look.faces;
    commands.spawn((PauseRoot, full(), GlobalZIndex(110))).with_children(|root| {
        // The pause screen over the held world.
        if !d.under_main {
            root.spawn((full(), BackgroundColor(Color::BLACK.with_alpha(WORLD_SHADE))));
            if let Some(s) = &look.screen {
                // Each half drawn larger than the view's half, its cropped parts off the view.
                let width = 50.0 / (1.0 - SCREEN_CROP_SIDE);
                let height = 100.0 / (1.0 - SCREEN_CROP_TOP - SCREEN_CROP_BOTTOM);
                let half = |left: bool| Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(if left { 50.0 - width } else { 50.0 }),
                    top: Val::Percent(-SCREEN_CROP_TOP * height),
                    width: Val::Percent(width),
                    height: Val::Percent(height),
                    ..default()
                };
                let img = |flip: bool| ImageNode { color: Color::WHITE.with_alpha(SCREEN_ALPHA), flip_x: flip, image_mode: NodeImageMode::Stretch, ..ImageNode::new(s.clone()) };
                root.spawn((half(true), img(false)));
                root.spawn((half(false), img(true)));
            } else {
                root.spawn((full(), BackgroundColor(Color::srgba(0.30, 0.24, 0.14, SCREEN_ALPHA))));
            }
        }
        // The band behind a page's list.
        match d.page {
            Page::Settings { .. } | Page::Controls { .. } => band(root, look, Node { top: u(0.0), height: Val::Percent(100.0), ..from_middle(OPTIONS_LEFT - BAND_MARGIN, 0.0, OPTIONS_WIDTH + 2.0 * BAND_MARGIN, 0.0) }),
            Page::Load { .. } => band(root, look, Node { height: Val::Percent(100.0), ..from_right(RIGHT + COLUMN, 0.0, LOAD_WIDTH + BAND_MARGIN, 0.0) }),
            Page::Main => {}
        }
        // The column: the keys' list on the first page, else half bright but for the page's entry.
        let top = (960.0 - d.column.len() as f32 * ITEM) / 2.0;
        let on_column = d.page == Page::Main && d.confirm.is_none();
        let lit = d.page_entry();
        for (i, (label, ok)) in d.column.iter().enumerate() {
            let shade = if (on_column && *ok) || (!on_column && i == lit) { Shade::Bright } else { Shade::Dim };
            right_entry(root, &f.entry, label, from_right(RIGHT, top + i as f32 * ITEM, COLUMN - 5.0, ITEM), COLUMN - 5.0, shade, on_column && i == d.main_selected, PauseButton::Column(i));
        }
        match d.page {
            Page::Main => {
                if let Some(sel) = d.confirm {
                    // The question: its lines end left of the choices [observed: the owner's screenshot].
                    let qw = f.entry.font.width(QUESTION);
                    let at = 280.0;
                    root.spawn(from_middle(at - 20.0 - qw, 480.0 - ITEM, 900.0, ITEM)).with_children(|t| sm_text(t, &f.entry, WARNING, 0.0, BASE, color(Shade::Bright)));
                    root.spawn(from_middle(at - 20.0 - qw, 480.0, qw, ITEM)).with_children(|t| sm_text(t, &f.entry, QUESTION, 0.0, BASE, color(Shade::Bright)));
                    let width = CONFIRM.iter().map(|c| f.entry.font.width(c)).fold(0.0, f32::max) + 30.0;
                    for (i, c) in CONFIRM.iter().enumerate() {
                        root.spawn((Button, PauseButton::Confirm(i), boxed(from_middle(at, 480.0 + i as f32 * ITEM, width, ITEM), i == sel)))
                        .with_children(|t| sm_text(t, &f.entry, c, 10.0, BASE, color(Shade::Bright)));
                    }
                } else if !d.status.is_empty() {
                    root.spawn((PauseStatusText, from_right(RIGHT, top + d.column.len() as f32 * ITEM + 10.0, 700.0, ITEM))).with_children(|t| {
                        let w = f.small.font.width(&d.status);
                        sm_text(t, &f.small, &d.status, 700.0 - w - 10.0, 30.0, color(Shade::Bright));
                    });
                }
            }
            Page::Settings { selected } => settings_page(root, d, f, settings, selected, top),
            Page::Controls { selected, listening } => controls_page(root, d, f, settings, selected, listening, top),
            Page::Load { selected } => load_page(root, d, f, list, selected),
        }
        // Back (and Defaults) at the bottom right.
        if d.page != Page::Main {
            right_entry(root, &f.entry, "Back", from_right(RIGHT, BACK_TOP, 200.0, ITEM), 200.0, Shade::Bright, false, PauseButton::Back);
            if matches!(d.page, Page::Settings { .. } | Page::Controls { .. }) {
                right_entry(root, &f.entry, "Defaults", from_right(RIGHT, BACK_TOP - ITEM, 200.0, ITEM), 200.0, Shade::Bright, false, PauseButton::Defaults);
            }
        }
    });
}

/// The settings page: the categories left of the column, the chosen category's switches
/// in the middle with the chosen one's note under them, the version at the bottom left.
fn settings_page(root: &mut ChildSpawnerCommands, d: &Drawn, f: &Faces, list: &[Setting], selected: usize, top: f32) {
    category_column(root, f, list, list.get(selected).map(|s| s.group), top);
    if list.is_empty() {
        root.spawn(from_middle(OPTIONS_LEFT, top, OPTIONS_WIDTH, OPTION_ROW)).with_children(|t| sm_text(t, &f.entry, "Nothing to switch.", 10.0, BASE, color(Shade::Dim)));
    }
    let rows = group_rows(list, selected);
    // A window of the group's rows round the chosen one.
    let first = rows.start.max((selected + 1).saturating_sub(OPTION_ROWS)).min(rows.end.saturating_sub(OPTION_ROWS).max(rows.start));
    for (n, i) in (first..rows.end).take(OPTION_ROWS).enumerate() {
        let s = &list[i];
        let on = d.states.get(i).copied().unwrap_or(false);
        root.spawn((Button, PauseButton::Switch(i), boxed(from_middle(OPTIONS_LEFT, top + n as f32 * OPTION_ROW, OPTIONS_WIDTH, OPTION_ROW), i == selected)))
        .with_children(|r| {
            sm_text(r, &f.entry, s.label, 10.0, BASE + 2.0, color(Shade::Bright));
            let value = if on { "ON" } else { "OFF" };
            let vw = f.entry.font.width(value);
            r.spawn((SwitchState(i), Text::new(value), Node { display: Display::None, ..default() }));
            sm_text(r, &f.entry, value, VALUE_AT - vw / 2.0, BASE + 2.0, color(Shade::Bright));
        });
    }
    // The chosen switch's note under the rows.
    let note = list.get(selected).map_or("", |s| s.note);
    root.spawn((SwitchNote, Text::new(note), Node { display: Display::None, ..default() }));
    let shown = (rows.end - first).min(OPTION_ROWS) as f32;
    root.spawn(from_middle(OPTIONS_LEFT + 10.0, top + shown * OPTION_ROW + 20.0, OPTIONS_WIDTH - 20.0, 160.0)).with_children(|t| wrapped(t, &f.small, note, OPTIONS_WIDTH - 20.0, 26.0, color(Shade::Bright)));
    version(root, f);
}

/// The categories left of the column: the switches' groups, then Controls; `current`
/// bright (`None`: Controls).
fn category_column(root: &mut ChildSpawnerCommands, f: &Faces, list: &[Setting], current: Option<&str>, top: f32) {
    let cats = categories(list);
    let tabs = cats.iter().map(|&(name, first)| (name, Some(name) == current, PauseButton::Category(first))).chain([(CONTROLS_TAB, current.is_none(), PauseButton::ControlsTab)]);
    for (i, (name, on, b)) in tabs.enumerate() {
        let w = SUB_WIDTH.max(f.entry.font.width(name) + 20.0);
        let shade = if on { Shade::Bright } else { Shade::Dim };
        right_entry(root, &f.entry, name, from_right(SUB_RIGHT, top + i as f32 * ITEM, w, ITEM), w, shade, false, b);
    }
}

/// The settings page's Controls tab: every control with its key and mouse button, a
/// window of [`OPTION_ROWS`] round the chosen one, the chosen one's note under them.
#[allow(clippy::too_many_arguments)]
fn controls_page(root: &mut ChildSpawnerCommands, d: &Drawn, f: &Faces, list: &[Setting], selected: usize, listening: bool, top: f32) {
    use crate::bindings::{label, GAME_CONTROLS};
    category_column(root, f, list, None, top);
    let n = GAME_CONTROLS.len();
    let first = (selected + 1).saturating_sub(OPTION_ROWS).min(n.saturating_sub(OPTION_ROWS));
    for (row, i) in (first..n).take(OPTION_ROWS).enumerate() {
        let (c, _, _) = GAME_CONTROLS[i];
        let value = if listening && i == selected { WAITING.to_string() } else { d.controls.get(i).cloned().unwrap_or_default() };
        root.spawn((Button, PauseButton::ControlRow(i), boxed(from_middle(OPTIONS_LEFT, top + row as f32 * OPTION_ROW, OPTIONS_WIDTH, OPTION_ROW), i == selected)))
        .with_children(|r| {
            sm_text(r, &f.entry, label(c), 10.0, BASE + 2.0, color(Shade::Bright));
            let vw = f.entry.font.width(&value);
            r.spawn((SwitchState(i), Text::new(value.clone()), Node { display: Display::None, ..default() }));
            sm_text(r, &f.entry, &value, (VALUE_AT - vw / 2.0).min(OPTIONS_WIDTH - vw - 14.0), BASE + 2.0, color(Shade::Bright));
        });
    }
    let note = match GAME_CONTROLS.get(selected) {
        Some(&(c, _, _)) if listening => format!("Press the new key or mouse button for {}. Esc cancels.", label(c)),
        Some(&(c, name, _)) => format!(
            "Enter or a click: bind a new key or mouse button to {} (a key replaces its key, a button its button; a control that had it gets the old one). Delete clears its key. Kept as \"{name}\" in [Controls] of NVForever.ini.",
            label(c)
        ),
        None => String::new(),
    };
    root.spawn((SwitchNote, Text::new(note.clone()), Node { display: Display::None, ..default() }));
    let shown = (n - first).min(OPTION_ROWS) as f32;
    root.spawn(from_middle(OPTIONS_LEFT + 10.0, top + shown * OPTION_ROW + 20.0, OPTIONS_WIDTH - 20.0, 160.0)).with_children(|t| wrapped(t, &f.small, &note, OPTIONS_WIDTH - 20.0, 26.0, color(Shade::Bright)));
    version(root, f);
}

/// The version at the bottom left [observed: the original shows its own there].
fn version(root: &mut ChildSpawnerCommands, f: &Faces) {
    root.spawn(Node { position_type: PositionType::Absolute, left: u(30.0), top: u(960.0 - 60.0), width: u(400.0), height: u(40.0), ..default() }).with_children(|t| {
        sm_text(t, &f.small, concat!("NVForever ", env!("CARGO_PKG_VERSION")), 0.0, 26.0, color(Shade::Bright));
    });
}

/// The load page: the chosen save's thumbnail and details over the saves' rows (a window
/// of [`LOAD_ROWS`] round the chosen one), left of the column.
fn load_page(root: &mut ChildSpawnerCommands, d: &Drawn, f: &Faces, list: &LoadList, selected: usize) {
    let panel = |top: f32, height: f32| from_right(RIGHT + COLUMN, top, LOAD_WIDTH, height);
    if list.saves.is_empty() {
        root.spawn(panel(LOAD_TOP, ITEM)).with_children(|t| sm_text(t, &f.entry, "No saves yet.", 10.0, BASE, color(Shade::Dim)));
        return;
    }
    let n = list.saves.len();
    let first = selected.saturating_sub(LOAD_ROWS / 2).min(n.saturating_sub(LOAD_ROWS));
    // The thumbnail, stretched to its frame.
    let frame = Node { position_type: PositionType::Absolute, right: u(RIGHT + COLUMN + LOAD_WIDTH - THUMB_DRAWN.x - 10.0), top: u(DETAILS_TOP - THUMB_DRAWN.y - 10.0), width: u(THUMB_DRAWN.x), height: u(THUMB_DRAWN.y), ..default() };
    match list.thumb.as_ref().filter(|(t, _)| *t == selected && d.thumb.is_some_and(|t| t.1)).and_then(|(_, h)| h.clone()) {
        Some(img) => {
            root.spawn((frame, ImageNode::new(img).with_mode(NodeImageMode::Stretch)));
        }
        None => {
            root.spawn((frame, BackgroundColor(Color::BLACK.with_alpha(0.5)))).with_children(|t| sm_text(t, &f.small, "No picture", 20.0, 40.0, color(Shade::Dim)));
        }
    }
    if let Some((path, h)) = list.saves.get(selected) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let place = if h.location.is_empty() { "Unknown place".to_string() } else { h.location.clone() };
        let lines = [
            format!("{}, level {}", h.player_name, h.level),
            format!("{place}, {}", crate::save::game_date(h)),
            format!("Played {}", play_time(h.play_seconds)),
            format!("{}: saved {} ({})", stem(path), saved_ago(h.saved_unix, now), kind_name(h.kind)),
        ];
        for (i, line) in lines.iter().enumerate() {
            let face = if i == 0 { &f.entry } else { &f.small };
            root.spawn((DetailLine, Text::new(line.clone()), Node { display: Display::None, ..default() }));
            root.spawn(panel(DETAILS_TOP + i as f32 * DETAILS_STEP, DETAILS_STEP)).with_children(|t| sm_text(t, face, line, 10.0, 30.0, color(Shade::Bright)));
        }
    }
    for (row, (i, (path, h))) in list.saves.iter().enumerate().skip(first).take(LOAD_ROWS).enumerate() {
        root.spawn((Button, PauseButton::LoadRow(i), boxed(panel(LOAD_TOP + row as f32 * ITEM, ITEM), i == selected))).with_children(|r| {
            sm_text(r, &f.entry, &stem(path), 10.0, BASE, color(Shade::Bright));
            let date = format!("{}  {}", kind_name(h.kind), crate::save::game_date(h));
            let w = f.small.font.width(&date);
            sm_text(r, &f.small, &date, LOAD_WIDTH - w - 14.0, BASE, color(Shade::Dim));
        });
    }
}

/// A line of the details, kept as text too (tests read it).
#[derive(Component)]
struct DetailLine;


#[allow(clippy::too_many_arguments)]
fn open(
    keys: Res<ButtonInput<KeyCode>>,
    time: AidTime,
    mut aid: ResMut<PauseAid>,
    mut paused: ResMut<Paused>,
    mut page: ResMut<Page>,
    mut view: ResMut<PauseView>,
    mut status: ResMut<PauseStatus>,
    mut next: ResMut<NextState<PlayState>>,
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
    view.main_selected = 0;
    view.confirm = None;
    *status = PauseStatus::default();
    next.set(PlayState::Menu);
}

/// A page another menu asks the pause menu to show (the main menu's Load Game and
/// Settings, `crate::main_menu`); taken on the next frame.
#[derive(Resource, Default)]
pub struct PageRequest(pub Option<Page>);

/// Asked from the quit question: back to the main menu (nvforever starts again with
/// `--menu`).
#[derive(Message, Clone, Copy, Debug)]
pub struct MainMenuRequest;

/// The row after `selected` (`step` 1) or before it (-1) of `rows`, round the ends.
fn step_row(selected: usize, step: i32, rows: usize) -> usize {
    if rows == 0 {
        return 0;
    }
    (selected.min(rows - 1) as i32 + step).rem_euclid(rows as i32) as usize
}

/// The switch `step` rows from `selected` within its group, round its ends.
fn step_in_group(list: &[Setting], selected: usize, step: i32) -> usize {
    let rows = group_rows(list, selected);
    if rows.is_empty() {
        return 0;
    }
    rows.start + step_row(selected - rows.start, step, rows.len())
}

/// The first switch of the category after (`step` 1) or before (-1) `selected`'s.
fn step_category(list: &[Setting], selected: usize, step: i32) -> usize {
    let cats = categories(list);
    let Some(g) = list.get(selected).map(|s| s.group) else { return 0 };
    let at = cats.iter().position(|(n, _)| *n == g).unwrap_or(0);
    cats.get(step_row(at, step, cats.len())).map_or(0, |c| c.1)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn choose(
    keys: Res<ButtonInput<KeyCode>>,
    time: AidTime,
    mut aid: ResMut<PauseAid>,
    paused: Res<Paused>,
    mut page: ResMut<Page>,
    mut view: ResMut<PauseView>,
    settings: Res<Settings>,
    buttons: Query<(&Interaction, &PauseButton), Changed<Interaction>>,
    mut next: ResMut<NextState<PlayState>>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
    mut sounds: MessageWriter<crate::ui_sounds::UiSound>,
    mut menu: SaveMenu,
    (mut request, main_menu, intro, mut to_main, mouse): (ResMut<PageRequest>, Option<Res<crate::main_menu::MainMenu>>, Option<Res<crate::intro::Intro>>, MessageWriter<MainMenuRequest>, Res<ButtonInput<MouseButton>>),
) {
    use crate::bindings::{Bindings, Input, GAME_CONTROLS};
    use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
    // Held for the intro film, which takes the keys (`crate::intro`).
    if !paused.0 || intro.is_some_and(|i| i.is_playing()) {
        return;
    }
    // Under the main menu the first page is the main menu's own (`crate::main_menu`): its
    // pages come from here, and going back shows it again.
    let under_main = main_menu.as_ref().is_some_and(|m| m.open);
    if let Some(p) = request.0.take() {
        if matches!(p, Page::Load { .. }) {
            menu.list.open(menu.cfg.dir.as_deref());
        }
        info!("pause menu: {p:?} page for another menu");
        *page = p;
    }
    if under_main && *page == Page::Main {
        return;
    }
    let now = time.elapsed_secs();
    if fire(&mut aid.resume, now) {
        info!("test aid NVF_PLAY_TEST_PAUSE: Esc (resume)");
        info!("pause menu: resume");
        next.set(PlayState::Playing);
        return;
    }
    let list = settings.list();
    let mut flips = Vec::new();
    // The settings aid: show the page and flip the switch, as a click on its row.
    let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut aid.switches).into_iter().partition(|(t, _)| *t <= now);
    aid.switches = later;
    for (_, key) in due {
        info!("test aid NVF_PLAY_TEST_SETTINGS: {key}");
        if let Some(b) = key.strip_prefix("bind:") {
            // `bind:<control>=<key or button>`: as Enter on its row and that key.
            let parsed = b.split_once('=').and_then(|(c, i)| Some((Bindings::by_name(c)?, crate::bindings::parse_input(i)?)));
            match parsed {
                Some((c, input)) => {
                    let selected = GAME_CONTROLS.iter().position(|(k, _, _)| *k == c).unwrap_or(0);
                    *page = Page::Controls { selected, listening: false };
                    commands.queue(move |world: &mut World| crate::bindings::rebind(world, c, input));
                }
                None => warn!("NVF_PLAY_TEST_SETTINGS: {key:?}: expected bind:<control>=<key or mouse button name>"),
            }
            continue;
        }
        match (settings.index(&key), key.as_str()) {
            (Some(i), _) => {
                *page = Page::Settings { selected: i };
                flips.push(i);
            }
            (None, "page") => *page = Page::Settings { selected: 0 },
            (None, "controls") => *page = Page::Controls { selected: 0, listening: false },
            (None, _) => warn!("NVF_PLAY_TEST_SETTINGS: no switch {key:?}; there are {:?}", list.iter().map(|s| s.key).collect::<Vec<_>>()),
        }
    }
    // The menu aid's actions due now, as the keys they stand for.
    let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut aid.actions).into_iter().partition(|(t, _)| *t <= now);
    aid.actions = later;
    let mut acts: Vec<String> = Vec::new();
    for (_, a) in due {
        info!("test aid NVF_PLAY_TEST_PAUSE_MENU: {a}");
        if a == "shot" {
            let name = format!("{}pause_{}.png", if under_main { "main_menu_" } else { "" }, match *page {
                Page::Main if view.confirm.is_some() => "quit",
                Page::Main => "main",
                Page::Settings { .. } => "settings",
                Page::Controls { .. } => "controls",
                Page::Load { .. } => "load",
            });
            info!("pause menu: screenshot {name}");
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(name));
        }
        acts.push(a);
    }
    let act = |name: &str| acts.iter().any(|a| a == name);
    // A control waiting for its new key takes the next key or mouse button, whatever it
    // is over; Esc cancels.
    if let Page::Controls { selected, listening } = &mut *page {
        if *listening {
            let c = GAME_CONTROLS.get(*selected).map(|x| x.0);
            let key = keys.get_just_pressed().copied().find(|k| crate::bindings::key_name(*k).is_some() || *k == KeyCode::Escape);
            let input = match key {
                Some(KeyCode::Escape) => {
                    info!("controls: rebinding cancelled");
                    *listening = false;
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Back });
                    return;
                }
                Some(k) if crate::bindings::RESERVED_KEYS.contains(&k) => None,
                Some(k) => Some(Input::Key(k)),
                None => mouse.get_just_pressed().next().map(|m| Input::Mouse(*m)),
            };
            if let (Some(c), Some(input)) = (c, input) {
                *listening = false;
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
                commands.queue(move |world: &mut World| crate::bindings::rebind(world, c, input));
            }
            return;
        }
    }
    let mut clicked = None;
    // A list takes the row the pointer moves onto (not one it rests on while the keys
    // move the choice, nor the one a redrawn page puts under it).
    let moved = menu.motion.as_ref().is_some_and(|m| m.delta != Vec2::ZERO);
    let (main_before, confirm_before) = (view.main_selected, view.confirm);
    for (interaction, b) in &buttons {
        match (interaction, b, &mut *page) {
            (Interaction::Pressed, ..) => clicked = Some(*b),
            (Interaction::Hovered, PauseButton::Switch(i), Page::Settings { selected }) if moved => *selected = *i,
            (Interaction::Hovered, PauseButton::ControlRow(i), Page::Controls { selected, .. }) if moved => *selected = *i,
            (Interaction::Hovered, PauseButton::LoadRow(i), Page::Load { selected }) if moved => *selected = *i,
            (Interaction::Hovered, PauseButton::Column(i), Page::Main) if moved && view.confirm.is_none() && !under_main => view.main_selected = *i,
            (Interaction::Hovered, PauseButton::Confirm(i), Page::Main) if moved && view.confirm.is_some() => view.confirm = Some(*i),
            _ => {}
        }
    }
    if act("defaults") && matches!(*page, Page::Settings { .. } | Page::Controls { .. }) {
        clicked = Some(PauseButton::Defaults);
    }
    // A click on the column's entry of another page (the page's own entry closes it), or
    // on a category.
    match clicked {
        Some(PauseButton::Column(i)) if !under_main && *page != Page::Main => {
            let closes = ENTRIES.get(i).is_some_and(|e| page.opened_by(e.label()));
            *page = Page::Main;
            view.main_selected = i;
            if closes {
                info!("pause menu: {} again, back to the column", ENTRIES[i].label());
                clicked = None;
            } else if ENTRIES.get(i).is_some_and(|e| matches!(e, Entry::Load | Entry::Settings)) {
                clicked = Some(PauseButton::Column(i));
            } else {
                clicked = None;
            }
        }
        Some(PauseButton::Column(i)) if under_main => {
            let label = main_menu.as_ref().and_then(|m| m.column().get(i).map(|c| c.0));
            match label {
                Some(l) if page.opened_by(l) => {
                    info!("pause menu: {l} again, back to the main menu");
                    *page = Page::Main;
                }
                Some(l) if l.starts_with("Load") => *page = Page::Load { selected: 0 },
                Some(l) if l.starts_with("Settings") => *page = Page::Settings { selected: 0 },
                _ => {}
            }
            if matches!(*page, Page::Load { .. }) {
                menu.list.open(menu.cfg.dir.as_deref());
            }
            clicked = None;
        }
        Some(PauseButton::Category(first)) => {
            *page = Page::Settings { selected: first };
            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Picked });
        }
        Some(PauseButton::ControlsTab) => {
            *page = Page::Controls { selected: 0, listening: false };
            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Picked });
        }
        Some(PauseButton::Defaults) if matches!(*page, Page::Controls { .. }) => {
            commands.queue(crate::bindings::reset);
            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
        }
        Some(PauseButton::Defaults) => {
            if let Some(d) = view.defaults.clone() {
                info!("pause menu: settings back to their defaults");
                commands.queue(move |world: &mut World| {
                    let list = world.resource::<Settings>().list().to_vec();
                    for (i, s) in list.iter().enumerate() {
                        if d.get(i).is_some_and(|&on| on != (s.get)(world)) {
                            crate::settings::flip(world, i);
                        }
                    }
                });
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
            }
        }
        _ => {}
    }
    match &mut *page {
        Page::Main if view.confirm.is_some() => {
            let sel = view.confirm.unwrap_or(2);
            let mut take = None;
            if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || act("back") {
                take = Some(2);
            } else if let Some(PauseButton::Confirm(i)) = clicked {
                take = Some(i);
            } else if matches!(clicked, Some(PauseButton::Column(i)) if ENTRIES.get(i) == Some(&Entry::Quit)) {
                // Quit again: as Cancel.
                take = Some(2);
            } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) || act("up") {
                view.confirm = Some(step_row(sel, -1, CONFIRM.len()));
            } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) || act("down") {
                view.confirm = Some(step_row(sel, 1, CONFIRM.len()));
            } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || act("pick") {
                take = Some(sel);
            }
            match take {
                Some(0) => {
                    info!("pause menu: quit to the main menu");
                    to_main.write(MainMenuRequest);
                }
                Some(1) => {
                    info!("pause menu: quit");
                    exit.write(AppExit::Success);
                }
                Some(_) => {
                    info!("pause menu: quit cancelled");
                    view.confirm = None;
                    sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Back });
                }
                None => {}
            }
            if take.is_some_and(|t| t < 2) {
                sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Picked });
            }
        }
        Page::Main => {
            let n = ENTRIES.len();
            let mut take = match clicked {
                Some(PauseButton::Column(i)) => ENTRIES.get(i).copied(),
                _ => None,
            };
            if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
                view.main_selected = step_row(view.main_selected, -1, n);
            } else if keys.any_just_pressed([KeyCode::ArrowDown]) {
                view.main_selected = step_row(view.main_selected, 1, n);
            } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
                take = ENTRIES.get(view.main_selected).copied();
            }
            if keys.just_pressed(KeyCode::KeyQ) || act("quit") {
                take = Some(Entry::Quit);
            } else if keys.just_pressed(KeyCode::KeyN) || act("save") {
                take = Some(Entry::Save);
            } else if keys.just_pressed(KeyCode::KeyL) || act("load") {
                take = Some(Entry::Load);
            } else if keys.just_pressed(KeyCode::KeyS) {
                take = Some(Entry::Settings);
            } else if keys.just_pressed(KeyCode::Escape) {
                take = Some(Entry::Continue);
            }
            if let Some(e) = take {
                view.main_selected = ENTRIES.iter().position(|x| *x == e).unwrap_or(0);
                if e != Entry::Continue {
                    sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Picked });
                }
            }
            match take {
                Some(Entry::Quit) => {
                    info!("pause menu: quit?");
                    view.confirm = Some(2);
                }
                Some(Entry::Save) => {
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
                }
                Some(Entry::Load) => {
                    info!("pause menu: load page");
                    menu.list.open(menu.cfg.dir.as_deref());
                    *page = Page::Load { selected: 0 };
                }
                Some(Entry::Settings) => {
                    info!("pause menu: settings");
                    *page = Page::Settings { selected: 0 };
                }
                Some(Entry::Continue) => {
                    info!("pause menu: resume");
                    next.set(PlayState::Playing);
                }
                None => {}
            }
        }
        Page::Settings { selected } => {
            if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) || act("back") {
                *page = Page::Main;
            } else if let Some(PauseButton::Switch(i)) = clicked {
                *selected = i;
                flips.push(i);
            } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
                *selected = step_in_group(list, *selected, -1);
            } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
                *selected = step_in_group(list, *selected, 1);
            } else if keys.any_just_pressed([KeyCode::Tab, KeyCode::PageDown]) {
                // After the last category, Controls.
                let next = step_category(list, *selected, 1);
                if next <= *selected {
                    *page = Page::Controls { selected: 0, listening: false };
                } else {
                    *selected = next;
                }
            } else if keys.just_pressed(KeyCode::PageUp) {
                let before = step_category(list, *selected, -1);
                if before >= group_rows(list, *selected).start {
                    *page = Page::Controls { selected: 0, listening: false };
                } else {
                    *selected = before;
                }
            } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space, KeyCode::ArrowLeft, KeyCode::ArrowRight]) && *selected < list.len() {
                flips.push(*selected);
            }
        }
        Page::Controls { selected, listening } => {
            let n = GAME_CONTROLS.len();
            let control = GAME_CONTROLS.get(*selected).map(|x| x.0);
            if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) || act("back") {
                *page = Page::Main;
            } else if let Some(PauseButton::ControlRow(i)) = clicked {
                *selected = i;
                *listening = true;
            } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
                *selected = step_row(*selected, -1, n);
            } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
                *selected = step_row(*selected, 1, n);
            } else if keys.any_just_pressed([KeyCode::Tab, KeyCode::PageDown]) {
                // After Controls, the first category again.
                *page = Page::Settings { selected: 0 };
            } else if keys.just_pressed(KeyCode::PageUp) {
                *page = Page::Settings { selected: categories(list).last().map_or(0, |c| c.1) };
            } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
                *listening = true;
            } else if let (true, Some(c)) = (keys.just_pressed(KeyCode::Delete), control) {
                commands.queue(move |world: &mut World| crate::bindings::clear_key(world, c));
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
            }
            if let (Page::Controls { listening: true, selected }, Some(_)) = (*page, control) {
                info!("controls: {} waits for its new key", Bindings::name(GAME_CONTROLS[selected].0));
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
    // The pointer or the keys moved the first page's choice or the question's: its sound.
    if view.main_selected != main_before && *page == Page::Main || (view.confirm.is_some() && confirm_before.is_some() && view.confirm != confirm_before) {
        sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Focus });
    }
    for i in flips {
        sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
        commands.queue(move |world: &mut World| {
            crate::settings::flip(world, i);
        });
    }
}

/// The main page's status line: the save asked from this menu, once written.
fn show_status(mut done: MessageReader<crate::save::SaveDone>, mut status: ResMut<PauseStatus>) {
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
}

/// What the page shows now.
fn drawn_now(world: &mut World) -> Drawn {
    let page = *world.resource::<Page>();
    let list = world.resource::<Settings>().list().to_vec();
    let states: Vec<bool> = list.iter().map(|s| (s.get)(world)).collect();
    if matches!(page, Page::Settings { .. }) && world.resource::<PauseView>().defaults.is_none() {
        let start = world.get_resource::<crate::settings::StartStates>().map(|s| s.0.clone()).filter(|s| s.len() == states.len());
        world.resource_mut::<PauseView>().defaults = Some(start.unwrap_or_else(|| states.clone()));
    }
    let main = world.get_resource::<crate::main_menu::MainMenu>().filter(|m| m.open).map(|m| m.column());
    let view = world.resource::<PauseView>();
    let thumb = world.resource::<LoadList>().thumb.as_ref().map(|(i, h)| (*i, h.is_some()));
    Drawn {
        page,
        main_selected: view.main_selected,
        confirm: view.confirm,
        states,
        status: world.resource::<PauseStatus>().line.clone(),
        under_main: main.is_some(),
        column: main.unwrap_or_else(|| ENTRIES.iter().map(|e| (e.label(), true)).collect()),
        thumb,
        controls: world.get_resource::<crate::bindings::Bindings>().map(|b| b.all().map(|(_, x)| x.text()).collect()).unwrap_or_default(),
    }
}

/// Draw the page again when what it shows changed.
fn draw(world: &mut World) {
    if !world.resource::<Paused>().0 {
        return;
    }
    let now = drawn_now(world);
    if world.resource::<PauseView>().drawn.as_ref() == Some(&now) {
        return;
    }
    if !world.contains_resource::<PauseLook>() {
        let look = PauseLook::load(world);
        world.insert_resource(look);
    }
    let roots: Vec<Entity> = world.query_filtered::<Entity, With<PauseRoot>>().iter(world).collect();
    for e in roots {
        world.despawn(e);
    }
    let settings = world.resource::<Settings>().list().to_vec();
    world.resource_scope(|world, look: Mut<PauseLook>| {
        world.resource_scope(|world, list: Mut<LoadList>| {
            let mut commands = world.commands();
            spawn_page(&mut commands, &now, &look, &settings, &list);
        });
    });
    world.flush();
    world.resource_mut::<PauseView>().drawn = Some(now);
}

fn close(mut commands: Commands, mut paused: ResMut<Paused>, mut page: ResMut<Page>, mut list: ResMut<LoadList>, mut view: ResMut<PauseView>, roots: Query<Entity, With<PauseRoot>>) {
    paused.0 = false;
    *page = Page::Main;
    *list = LoadList::default();
    view.drawn = None;
    view.confirm = None;
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
    fn the_keys_stay_in_a_category_and_tab_goes_to_the_next() {
        let s = |key: &'static str, group: &'static str| Setting { key, group, label: key, note: "", get: |_| false, set: |_, _| {} };
        let list = [s("a", "Gameplay"), s("b", "Gameplay"), s("c", "Display"), s("d", "Display"), s("e", "Display"), s("f", "Debug")];
        assert_eq!(categories(&list), vec![("Gameplay", 0), ("Display", 2), ("Debug", 5)]);
        assert_eq!(group_rows(&list, 3), 2..5);
        assert_eq!(step_in_group(&list, 4, 1), 2);
        assert_eq!(step_in_group(&list, 2, -1), 4);
        assert_eq!(step_in_group(&list, 5, 1), 5);
        assert_eq!(step_category(&list, 1, 1), 2);
        assert_eq!(step_category(&list, 5, 1), 0);
        assert_eq!(step_category(&list, 0, -1), 5);
    }

    #[test]
    fn a_page_is_closed_by_its_own_entry() {
        // The pause menu's labels and the main menu's ("Load Game").
        assert!(Page::Settings { selected: 3 }.opened_by("Settings"));
        assert!(Page::Load { selected: 0 }.opened_by("Load"));
        assert!(Page::Load { selected: 0 }.opened_by("Load Game"));
        assert!(!Page::Load { selected: 0 }.opened_by("Settings"));
        assert!(!Page::Settings { selected: 0 }.opened_by("Load Game"));
        assert!(!Page::Main.opened_by("Settings"));
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
        let drawn = Drawn { page: Page::Load { selected: 1 }, main_selected: 0, confirm: None, states: vec![], status: String::new(), column: ENTRIES.iter().map(|e| (e.label(), true)).collect(), under_main: false, thumb: None, controls: vec![] };
        app.world_mut().run_system_cached_with(|In(d): In<Drawn>, mut commands: Commands, list: Res<LoadList>| spawn_page(&mut commands, &d, &PauseLook::plain(), &[], &list), drawn).unwrap();
        let world = app.world_mut();
        let mut rows = world.query::<(&PauseButton, &BorderColor)>();
        let rows: Vec<(PauseButton, bool)> = rows.iter(world).filter(|(b, _)| matches!(b, PauseButton::LoadRow(_))).map(|(b, c)| (*b, c.top != Color::NONE)).collect();
        assert_eq!(rows.len(), 2);
        // The chosen row has the box round it, and only it.
        assert!(rows.contains(&(PauseButton::LoadRow(1), true)) && rows.contains(&(PauseButton::LoadRow(0), false)));
        // Load is lit in the column, the others dimmed.
        let mut texts = world.query::<(&Text, &TextColor)>();
        let all: Vec<(String, Color)> = texts.iter(world).map(|(t, c)| (t.0.clone(), c.0)).collect();
        assert!(all.contains(&("Load".to_string(), color(Shade::Bright))) && all.contains(&("Save".to_string(), color(Shade::Dim))), "{all:?}");
        let mut details = world.query_filtered::<&Text, With<DetailLine>>();
        let details: Vec<String> = details.iter(world).map(|t| t.0.clone()).collect();
        assert!(details.iter().any(|t| t == "Courier, level 1"), "{details:?}");
        // The stand-in font sets no spaces.
        assert!(all.iter().any(|t| t.0 == "Nopicture"), "{all:?}");
    }

    #[derive(Resource, Default)]
    struct Lamp(bool);

    #[test]
    fn the_settings_page_shows_each_switch_as_the_game_has_it() {
        let mut app = App::new();
        app.init_resource::<Lamp>().init_resource::<Paused>().insert_resource(Page::Settings { selected: 0 });
        add_setting(&mut app, Setting { key: "lamp", group: "Test", label: "Lamp", note: "A lamp.", get: |w| w.resource::<Lamp>().0, set: |w, on| w.resource_mut::<Lamp>().0 = on });
        app.init_resource::<PauseView>().init_resource::<LoadList>().init_resource::<PauseStatus>().insert_resource(PauseLook::plain());
        app.world_mut().resource_mut::<Paused>().0 = true;
        app.add_systems(Update, draw);
        let state = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            q.single(world).unwrap().1 .0.clone()
        };
        assert_eq!(state(&mut app), "OFF");
        crate::settings::flip(app.world_mut(), 0);
        assert_eq!(state(&mut app), "ON");
        // Defaults go back to the state the page was first shown with.
        assert_eq!(app.world().resource::<PauseView>().defaults, Some(vec![false]));
        let world = app.world_mut();
        let mut note = world.query_filtered::<&Text, With<SwitchNote>>();
        assert_eq!(note.single(world).unwrap().0, "A lamp.");
    }

    #[test]
    fn the_controls_tab_shows_each_key_and_waits_for_a_new_one() {
        use crate::bindings::{Bindings, Control, Input};
        let jump = crate::bindings::GAME_CONTROLS.iter().position(|(c, _, _)| *c == Control::Jump).unwrap();
        let mut app = App::new();
        app.init_resource::<Paused>().init_resource::<Settings>().init_resource::<Bindings>().insert_resource(Page::Controls { selected: jump, listening: false });
        app.init_resource::<PauseView>().init_resource::<LoadList>().init_resource::<PauseStatus>().insert_resource(PauseLook::plain());
        app.world_mut().resource_mut::<Paused>().0 = true;
        app.add_systems(Update, draw);
        let value = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            q.iter(world).find(|(s, _)| s.0 == jump).map(|(_, t)| t.0.clone())
        };
        assert_eq!(value(&mut app).as_deref(), Some("Space"));
        crate::bindings::rebind(app.world_mut(), Control::Jump, Input::Key(KeyCode::KeyJ));
        assert_eq!(value(&mut app).as_deref(), Some("J"));
        *app.world_mut().resource_mut::<Page>() = Page::Controls { selected: jump, listening: true };
        assert_eq!(value(&mut app).as_deref(), Some(WAITING));
        assert!(Page::Controls { selected: 0, listening: false }.opened_by("Settings"));
    }
}
