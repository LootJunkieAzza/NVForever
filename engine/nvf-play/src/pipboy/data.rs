//! The DATA page (docs/spec-notes/pipboy.md §5.3): Local Map, World Map, Quests, Misc
//! (notes) and Radio, laid out as `menus\main\map_menu.xml`.
//!
//! - **Heading**: the place the player is in and the game's date and time
//!   ([`model::date_time`], [`read::place_name`]).
//! - **Quests**: the quests with an objective shown, running ones first, newest on top
//!   ([`model::sort_quests`]); the active quest's square filled, those done dimmed; the
//!   selected quest's objectives at the right, each behind a square, filled when done.
//!   A click or Enter makes a quest the active one.
//! - **Misc**: the player's notes (AddNote) by name and the selected note's text, which
//!   the wheel scrolls; an image note's picture. "Show Active Quest Notes" keeps the
//!   notes of the active quest.
//! - **Radio**: the stations the Pip-Boy receives here (`crate::radio`'s list), the
//!   playing one's square filled; a click or Enter tunes to a station or, on the playing
//!   one, turns the radio off; the waveform panel at the right (a still wave while it
//!   plays, the flat line while it is off).
//! - **World Map**: the worldspace's map picture with the map markers the player knows
//!   (found within their radius, made visible by ShowMap or by their flag), the active
//!   quest's markers and the player's arrow; dragged to pan, the wheel zooms; the
//!   highlight box follows the pointer and takes a marker's name when over one. A click
//!   on a marker asks whether to fast travel there, in a box over the map ([`travel`],
//!   docs/spec-notes/fast-travel.md).
//! - **Local Map**: the doors leading elsewhere, the known map markers and the quest
//!   markers around the player, with the player's arrow, over an outline of the place
//!   made from the physics world's collision ([`outline`]; the original's picture of the
//!   place from above is not drawn). Indoors the map is drawn closer
//!   ([`outline::INTERIOR_SCALE`]).
//! - Both maps pan while the left button is held on them and the pointer moves (the
//!   pointer's own movement, not Bevy's drag: the map's nodes are drawn again as the
//!   highlight box follows the pointer, which ended a drag at once).
//!
//! Selection: Up/Down (W/S) and the pointer (the row under it is selected and boxed);
//! the test aid's `key=down|up|use` steps (`crate::menu::MenuInput`) do the same here.
//! Everything the page shows is gathered into a [`View`] each frame and drawn again only
//! when it changed. The page's state is [`DataState`], made on first use (the Pip-Boy's
//! plugin knows only [`draw`]).

mod model;
mod outline;
mod read;
pub mod travel;

use super::frame::{base, Align, Ctx, Kit, FADE, FILL, LINE, MAIN, TEXT, THICK};
use super::{Page, PageBody, PipBoy, PipBoyAid, PipBoyInput, PipBoyScreen};
use crate::interface::{amber, at};
use crate::menu::model::Command;
use crate::menu::MenuInput;
use crate::player::Player;
use crate::radio::{Radio, RadioPlaces};
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::{Location, PlayWorld};
use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use fnv_sim::FormId;
use model::{ObjectiveLine, QuestKey, WorldMap};
use outline::Outline;
use travel::{FastTravelAsk, FastTravelGo, Popup};
use nvf_assets::GameData;
use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap};

// ---- the tabs ---------------------------------------------------------------------------

const LOCAL: usize = 0;
const WORLD: usize = 1;
const QUESTS: usize = 2;
const NOTES: usize = 3;
const RADIO: usize = 4;

// ---- the menu file's places, inside the menus' rectangle [documented: map_menu.xml] ------

/// Heading: the title 70 in after a line of 50; a line from 180, 470 long; the place
/// right-aligned at 630 and the time at 845, both 20 down in font 2; lines fading down at
/// 650 and 855, a line from 660 to the end.
const TITLE_X: f32 = 70.0;
const LEAD_LINE: f32 = 50.0;
const LINE2: (f32, f32) = (180.0, 470.0);
const PLACE_X: f32 = 630.0;
const SPLIT_X: f32 = 650.0;
const LINE3_X: f32 = 660.0;
const TIME_X: f32 = 845.0;
const HEAD_TEXT_Y: f32 = 20.0;
/// The maps' window: 50 down, 855 × 500; the picture's point shown at its middle.
const MAP: Rect = Rect { min: Vec2::new(0.0, 50.0), max: Vec2::new(855.0, 550.0) };
const MAP_MID: Vec2 = Vec2::new(427.5, 250.0);
/// The highlight box: 160 square, its corners 40-long arms with 15-long fading tips;
/// the name in font 1 centred 80 in and 180 down; a dot of 8 at its middle.
const HBOX: f32 = 160.0;
const HBOX_ARM: f32 = 40.0;
const HBOX_TIP: f32 = 15.0;
const HBOX_NAME_Y: f32 = 180.0;
const DOT: f32 = 8.0;
/// A marker is drawn this much larger under the highlight box, and brightness 220
/// (255 under it).
const MARKER_GROW: f32 = 1.75;
const MARKER_BRIGHTNESS: (f32, f32) = (220.0, 255.0);
/// The map picture: alpha 128 at brightness 255.
const MAP_ALPHA: f32 = 128.0;
/// The player's arrow: `glow_cursor.dds`, 1.28 times as high as wide.
const CURSOR_FILE: &str = "Interface\\Icons\\Misc\\glow_cursor.dds";
const CURSOR_ASPECT: f32 = 1.28;
/// Lists: 15 left of the menus, 75 down, 400 × 468; a scroll bar of 32 on the left; a
/// row from the scroll bar to 5 short of the list's right edge, its text 40 in and 20
/// down in font 2 broken at 310, its square of 25 at 15, 20.
const LIST: Rect = Rect { min: Vec2::new(-15.0, 75.0), max: Vec2::new(385.0, 543.0) };
const LIST_W: f32 = 400.0;
const SCROLLBAR: f32 = 32.0;
const ARROW_H: f32 = 16.0;
const ROW_SHORT: f32 = 5.0;
const ROW_TEXT: Vec2 = Vec2::new(40.0, 20.0);
const MARK_AT: Vec2 = Vec2::new(15.0, 20.0);
const MARK: f32 = 25.0;
const WRAP: f32 = 310.0;
/// The buttons at the right: right-aligned at 855, from 75 down, each its text with 20
/// across and 15 up and down; the text 10 under the middle of the box's spare height;
/// a PC key's label 5 after the box.
const BUTTONS: Vec2 = Vec2::new(855.0, 75.0);
const BUTTON_HORBUF: f32 = 20.0;
const BUTTON_VERBUF: f32 = 15.0;
const BUTTON_TEXT_DROP: f32 = 10.0;
const KEY_GAP: f32 = 5.0;
/// The item data at the right: 400 wide at the menus' right edge, under the buttons'
/// place (75 + 120) less a button's height; a note's text 17 left of it and 20 down,
/// 400 × 385, in font 1, scrolled 20 a step; an image note's picture 360 square at its
/// right; the objectives 400 × 400 with the scroll bar on the right.
const DATA_X: f32 = 455.0;
const DATA_W: f32 = 400.0;
const BUTTON_RECT_H: f32 = 120.0;
const TEXT_AT: Vec2 = Vec2::new(-17.0, 20.0);
const TEXT_H: f32 = 385.0;
const TEXT_STEP: f32 = 20.0;
const TEXT_PAD: f32 = 10.0;
const IMAGE: f32 = 360.0;
const OBJECTIVES_H: f32 = 400.0;
/// The radio's waveform panel: 500, 125, 350 square; the tick meters 32 deep along its
/// foot and right side (2 past them); the flat line 2 high, 5 short of the width, 8
/// over the middle.
const WAVE: Rect = Rect { min: Vec2::new(500.0, 125.0), max: Vec2::new(850.0, 475.0) };
const TICKS: f32 = 32.0;

// ---- NVForever's own [design or hypothesis, pipboy.md §5.3] -----------------------------

/// A row is 54 high and its text's further lines 34 below the first [observed: fitted to
/// the owner's Quests and Radio screenshots, rows 33.5 px apart and two-line rows 54.5,
/// at 0.62 px a unit].
const ROW_H: f32 = 54.0;
const ROW_PITCH: f32 = 34.0;
/// The selection box starts 9 left of the row [observed: the owner's Radio screenshot
/// boxes "Mojave Music Radio" from 448 px, the rows' squares start at 456 px as ours].
const BOX_LEFT: f32 = 9.0;
/// A done quest's or objective's text and square at half strength [hypothesis: the
/// owner's screenshot shows the completed quest greyed].
const DONE_ALPHA: f32 = 0.5;
/// Map marker, player arrow and quest marker widths ("set in code" in the menu file)
/// [observed: fitted to the owner's World Map screenshot, the marker about 13 px, the
/// arrow's triangle 25 px and the quest marker's 12 × 23 px, at 0.62 px a unit; the
/// pictures fill part of their squares].
const MARKER: f32 = 24.0;
const CURSOR: f32 = 40.0;
const QUEST_MARKER: f32 = 56.0;
/// The quest marker: the HUD compass's objective marker [hypothesis: the shape on the
/// owner's screenshots]; 1.25 times as high as wide, set 15 right and 5 down "to match
/// the marker textures" [documented: map_menu.xml quest marker templates].
const QUEST_FILE: &str = "glow_hud_compass_objective_marker.dds";
const QUEST_ASPECT: f32 = 1.25;
const QUEST_NUDGE: Vec2 = Vec2::new(15.0, 5.0);
/// The pointer takes the highlight box to a marker this near [hypothesis].
const SNAP: f32 = 40.0;
/// A press and release on the World Map is a click (it asks to travel) while the pointer
/// moved at most this far between them [design].
const CLICK_SLOP: f32 = 6.0;
/// The fast-travel question box (fast-travel.md §3) [design: by the owner's screenshot]:
/// its width, the room round its text, its text's wrap, the room under an answer's text
/// and the answer's frame round the widest answer (the message box's 42, message-menu.md
/// §3).
const POPUP_W: f32 = 520.0;
const POPUP_PAD: f32 = 30.0;
const POPUP_ROW_PAD: f32 = 16.0;
const POPUP_FRAME_PAD: f32 = 42.0;
/// The local map: game units to one canvas unit at a magnification of 1 [observed:
/// fitted to the owner's Local Map screenshot, where the gas station's door (-75242, 4076)
/// and the quest marker at the saloon's door (-67714, 3620) stand 168 px apart; the
/// original's zoom at the time is not known].
const LOCAL_SCALE: f32 = 28.0;
/// The local map reads the doors of the cells this many across and down from the one at
/// its middle (enough for the window at a magnification of 1).
const LOCAL_CELLS: (i32, i32) = (3, 2);
/// Each wheel step zooms a map by this factor, between 1 and 4 times [design].
const ZOOM_STEP: f32 = 1.25;
const ZOOM: (f32, f32) = (1.0, 4.0);
/// The still wave drawn while the radio plays [design]: its height and waves across.
const WAVE_AMPLITUDE: f32 = 70.0;
const WAVE_CYCLES: f32 = 3.0;
const WAVE_SEGMENTS: usize = 72;
/// How often the page looks at the quests and the markers while the game runs, seconds.
const POLL: f32 = 0.25;
const DOOR_ICON: &str = "interface\\icons\\local map\\icon_local_door.dds";

// ---- state --------------------------------------------------------------------------------

/// What the player did on the page with the pointer, taken by [`draw`].
#[derive(Debug, Clone, Copy, PartialEq)]
enum Act {
    /// The pointer came onto a row of the tab's list.
    Hover(usize, usize),
    /// The pointer left the row.
    Leave(usize, usize),
    /// A row was clicked.
    Use(usize, usize),
    /// A button was clicked.
    Button(ButtonKind),
    /// The pointer came onto an answer of the question box, or clicked it.
    PopupOver(usize),
    PopupPick(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ButtonKind {
    PlaceMarker,
    ShowLocation,
    NotesFilter,
    Challenges,
}

/// A place name's cell: worldspace, interior, grid square.
type PlaceKey = (u32, Option<u32>, (i32, i32));
/// The local map's doors: interior, or the grid square at its middle.
type DoorKey = (Option<u32>, (i32, i32));

/// The DATA page's state.
#[derive(Resource, Default)]
pub struct DataState {
    /// Each tab's selected row and first row shown, and how many rows fit when last drawn.
    sel: [usize; 5],
    first: [usize; 5],
    fit: [usize; 5],
    /// The selected row is boxed (the pointer is on it, or a key moved it).
    boxed: bool,
    /// How far the note's text is scrolled, and how far it can be.
    text_first: f32,
    text_max: f32,
    /// The active quest: the player's pick while it runs, else the newest running quest
    /// [design].
    active_quest: Option<u32>,
    /// How many ForceActiveQuest calls the page has taken (`fnv_game::functions::ui`).
    forced_seen: u64,
    /// When each quest was first seen listed (a running count).
    seen: HashMap<u32, u64>,
    seen_count: u64,
    /// "Show Active Quest Notes" is on.
    notes_of_active: bool,
    /// The Notes tab shows the challenges instead ("Challenges", R; `crate::challenges`).
    challenges: bool,
    /// The point shown at each map's middle (world: the picture's 0–1; local: game x, y)
    /// and its magnification.
    centre: [Option<[f32; 2]>; 2],
    magnify: [f32; 2],
    /// Where the highlight box is on each map (in the map's window).
    cursor: [Option<Vec2>; 2],
    /// Map markers the player has been within the radius of.
    discovered: BTreeSet<u32>,
    /// Markers found since `crate::compass` last took them (for "You have discovered").
    newly_found: Vec<u32>,
    /// Where the player was last outdoors: worldspace, feet, heading.
    outdoors: Option<(u32, [f32; 3], f32)>,
    // The plugins' records, read when first wanted.
    markers: Option<Vec<read::Marker>>,
    notes: HashMap<u32, Option<read::Note>>,
    maps: HashMap<u32, Option<WorldMap>>,
    places: HashMap<PlaceKey, String>,
    doors: HashMap<DoorKey, Vec<read::Door>>,
    /// What was drawn last.
    shown: Option<View>,
    was_open: bool,
    last_tab: Option<usize>,
    /// The door outdoors into each interior a quest target is in (worldspace, place).
    doors_into: HashMap<u32, Option<(u32, [f32; 3])>>,
    poll: f32,
    acts: Vec<Act>,
    drag: Vec2,
    /// Where the pointer was while the left button is held on a map (dragging it).
    drag_from: Option<Vec2>,
    /// The Local Map's outline of the place, as far as it is made.
    outline: Option<Outline>,
    /// The landscape heights of the cells the outline read (by grid square; `None` for a
    /// square without land), kept while one outline is made.
    outline_land: HashMap<(i32, i32), Option<Vec<f32>>>,
    /// How many markers the world map showed when last logged.
    world_logged: Option<usize>,
    /// The World Map's marker under the highlight box, when last drawn.
    cursor_marker: Option<u32>,
    /// Where the left button went down on the World Map, and how far the pointer has
    /// moved since (a click asks to travel, a drag does not).
    press: Option<(Vec2, f32)>,
    /// The fast-travel question box over the World Map (`travel`).
    popup: Option<Popup>,
}

// ---- what is shown ------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mark {
    None,
    Empty,
    Filled,
}

#[derive(Debug, Clone, PartialEq)]
struct Row {
    text: String,
    mark: Mark,
    done: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Right {
    None,
    Objectives(Vec<Row>),
    Text { text: String, first: f32 },
    Image(String),
    Audio,
    Wave { on: bool },
}

#[derive(Debug, Clone, PartialEq)]
struct Button {
    text: String,
    key: Option<&'static str>,
    kind: ButtonKind,
}

#[derive(Debug, Clone, PartialEq)]
struct MapMark {
    /// A map marker's type and whether the player has been there; `None` for a door.
    icon: Option<(u8, bool)>,
    highlighted: bool,
    /// The marker's middle in the map's window.
    at: Vec2,
}

impl MapMark {
    fn picture(&self) -> String {
        match self.icon {
            Some((kind, found)) => model::marker_icon(kind, found, self.highlighted),
            None => DOOR_ICON.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct MapShown {
    /// The map picture and where it lies in the window.
    picture: Option<(String, Rect)>,
    /// The Local Map's outline picture and where it lies in the window.
    outline: Option<(Handle<Image>, Rect)>,
    marks: Vec<MapMark>,
    /// The player's arrow: its middle and heading (radians).
    player: Option<(Vec2, f32)>,
    /// The active quest's markers (their top left in the window).
    quests: Vec<Vec2>,
    /// The highlight box's middle and the name it shows.
    cursor: Vec2,
    label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Body {
    Map(MapShown),
    List { rows: Vec<Row>, first: usize, boxed: Option<usize>, right: Right },
}

/// Everything the page shows.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    tab: usize,
    place: String,
    when: String,
    buttons: Vec<Button>,
    body: Body,
    popup: Option<Popup>,
}

// ---- the system -------------------------------------------------------------------------

/// The game the page reads.
#[derive(SystemParam)]
pub struct Game<'w> {
    script: Option<ResMut<'w, ScriptState>>,
    sim: Option<Res<'w, SimState>>,
    world: Option<Res<'w, PlayWorld>>,
    location: Option<Res<'w, Location>>,
    player: Option<Res<'w, Player>>,
    radio: Option<Res<'w, Radio>>,
    places: Option<Res<'w, RadioPlaces>>,
    phys: Option<ResMut<'w, crate::phys::PhysState>>,
}

/// What the player presses.
#[derive(SystemParam)]
pub struct Input<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    wheel: MessageReader<'w, 's, MouseWheel>,
    menu: MessageReader<'w, 's, MenuInput>,
    pip_out: MessageWriter<'w, PipBoyInput>,
    time: Res<'w, Time<Real>>,
    buttons: Res<'w, ButtonInput<MouseButton>>,
    aid: ResMut<'w, PipBoyAid>,
    ask: MessageWriter<'w, FastTravelAsk>,
    go: MessageWriter<'w, FastTravelGo>,
}

/// The page's drawing tools.
#[derive(SystemParam)]
pub struct Tools<'w> {
    kit: ResMut<'w, Kit>,
    data: Option<Res<'w, GameData>>,
    images: ResMut<'w, Assets<Image>>,
    formats: Option<Res<'w, CompressedImageFormatSupport>>,
}

/// Keep track of the quests and markers, take the player's input while the page is
/// shown, and fill the page's body when what it shows changed.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    bodies: Query<(Entity, &PageBody)>,
    added: Query<(), Added<PageBody>>,
    pip: Res<PipBoy>,
    screen: Option<Res<PipBoyScreen>>,
    state: Option<ResMut<DataState>>,
    mut game: Game,
    mut input: Input,
    mut tools: Tools,
) {
    let Some(mut state) = state else {
        commands.init_resource::<DataState>();
        return;
    };
    let st = &mut *state;
    st.poll += input.time.delta_secs();
    if st.poll >= POLL {
        st.poll = 0.0;
        track(st, &game);
    }
    let body = bodies.iter().find(|(_, b)| b.page == Page::Data && pip.open());
    let Some((body, b)) = body else {
        st.was_open = false;
        st.popup = None;
        input.wheel.clear();
        input.menu.clear();
        return;
    };
    let tab = b.tab;
    if st.last_tab != Some(tab) {
        st.last_tab = Some(tab);
        st.boxed = false;
        st.popup = None;
    }
    if !st.was_open {
        st.was_open = true;
        opened(st, &game);
    }
    let lists = lists(st, &game);
    let pointer = screen.as_ref().and_then(|s| s.pointer);
    take_input(st, tab, &lists, &mut game, &mut input, pointer);
    if tab == LOCAL {
        advance_outline(st, &mut game, &mut tools.images);
    }
    let view = build(st, tab, &lists, &game, pointer);
    if added.get(body).is_err() && st.shown.as_ref() == Some(&view) {
        return;
    }
    let mut c = Ctx::new(&mut tools.kit, tools.data.as_deref(), &mut tools.images, tools.formats.as_deref());
    commands.entity(body).despawn_children().with_children(|p| show(p, &mut c, st, &view));
    st.shown = Some(view);
}

/// Note the order quests are listed in, the markers the player finds, and where the
/// player was last outdoors.
impl DataState {
    /// The active quest, for a save (`crate::save`).
    pub fn active_quest(&self) -> Option<u32> {
        self.active_quest
    }

    /// A save's active quest.
    pub fn restore_active_quest(world: &mut bevy::prelude::World, q: u32) {
        if !world.contains_resource::<DataState>() {
            world.init_resource::<DataState>();
        }
        world.resource_mut::<DataState>().active_quest = Some(q);
    }

    /// The map markers the player found, for a save (`crate::compass`).
    pub fn discovered(&self) -> &BTreeSet<u32> {
        &self.discovered
    }

    /// A save's found markers: found already, so no "You have discovered" for them.
    pub fn restore_discovered(world: &mut bevy::prelude::World, found: BTreeSet<u32>) {
        if !world.contains_resource::<DataState>() {
            world.init_resource::<DataState>();
        }
        let mut st = world.resource_mut::<DataState>();
        st.newly_found.retain(|f| !found.contains(f));
        st.discovered = found;
    }

    /// The markers found since the last call, with their names.
    pub fn take_newly_found(&mut self) -> Vec<(u32, String)> {
        let new = std::mem::take(&mut self.newly_found);
        let name = |f: u32| self.markers.as_ref().and_then(|ms| ms.iter().find(|m| m.form == f)).map(|m| m.name.clone()).unwrap_or_default();
        new.into_iter().map(|f| (f, name(f))).collect()
    }

    /// The named map markers of worldspace `world` the plugins place (read once the DATA
    /// page first tracked the player): reference, place and whether the player found it.
    pub fn markers_of(&self, world: u32) -> Vec<(u32, [f32; 3], bool)> {
        self.markers.iter().flatten().filter(|m| m.world == world).map(|m| (m.form, m.pos, self.discovered.contains(&m.form))).collect()
    }
}

fn track(st: &mut DataState, game: &Game) {
    if let Some(rt) = game.script.as_ref().and_then(|s| s.rt.as_ref()) {
        let mut new: Vec<u32> = rt.game.quests.iter().filter(|(q, s)| listed(&rt.game, **q, s) && !st.seen.contains_key(&q.0)).map(|(q, _)| q.0).collect();
        if !new.is_empty() {
            st.seen_count += 1;
            new.sort_unstable();
            for q in new {
                st.seen.insert(q, st.seen_count);
            }
        }
    }
    let (Some(loc), Some(player)) = (game.location.as_ref(), game.player.as_ref()) else { return };
    if loc.interior.is_none() {
        st.outdoors = Some((loc.world, player.feet, player.heading));
        let feet = player.feet;
        let world = loc.world;
        let found: Vec<u32> = markers(st, game)
            .into_iter()
            .flatten()
            .filter(|m| m.world == world && m.radius > 0.0 && (m.pos[0] - feet[0]).hypot(m.pos[1] - feet[1]) <= m.radius)
            .map(|m| m.form)
            .collect();
        for f in found {
            if st.discovered.insert(f) {
                info!("pipboy data: map marker {} found", FormId(f));
                st.newly_found.push(f);
            }
        }
    }
}

/// The map markers, read once.
fn markers<'a>(st: &'a mut DataState, game: &Game) -> Option<&'a Vec<read::Marker>> {
    if st.markers.is_none() {
        let (w, sim) = (game.world.as_ref()?, game.sim.as_ref()?);
        let ms = read::markers(w.lo, w.schema, sim.refs);
        info!("pipboy data: {} map markers", ms.len());
        st.markers = Some(ms);
    }
    st.markers.as_ref()
}

/// Whether a quest is in the player's list: it has a name and an objective shown or done
/// (docs/spec-notes/quest-messages.md §5).
fn listed(g: &fnv_game::game::Game, q: FormId, s: &fnv_game::quest::QuestState) -> bool {
    g.data.quests.get(&q).and_then(|d| d.name.as_deref()).is_some_and(|n| !n.trim().is_empty()) && s.objectives.values().any(|o| o.displayed || o.completed)
}

/// The Pip-Boy came up on this page: the maps centre on the player, the quest list on
/// the active quest.
fn opened(st: &mut DataState, game: &Game) {
    st.centre = [None, None];
    st.cursor = [None, None];
    st.boxed = false;
    st.text_first = 0.0;
    st.magnify = [st.magnify[0].max(ZOOM.0), st.magnify[1].max(ZOOM.0)];
    let l = lists(st, game);
    if let Some(i) = st.active_quest.and_then(|a| l.quests.iter().position(|q| q.form == a)) {
        st.sel[QUESTS] = i;
    }
}

/// The lists' rows, with what each stands for.
#[derive(Default)]
struct Lists {
    quests: Vec<QuestKey>,
    notes: Vec<read::Note>,
    stations: Vec<(u32, String)>,
    /// The station the Pip-Boy plays (its base), when on.
    playing: Option<u32>,
    /// The Notes tab lists these instead of the notes.
    challenges: Option<Vec<crate::challenges::Row>>,
}

fn lists(st: &mut DataState, game: &Game) -> Lists {
    let mut out = Lists::default();
    let Some(rt) = game.script.as_ref().and_then(|s| s.rt.as_ref()) else { return out };
    let g = &rt.game;
    out.quests = g
        .quests
        .iter()
        .filter(|(q, s)| listed(g, **q, s))
        .map(|(q, s)| QuestKey { seen: st.seen.get(&q.0).copied().unwrap_or(st.seen_count + 1), form: q.0, done: s.completed || s.failed })
        .collect();
    model::sort_quests(&mut out.quests);
    // A script's ForceActiveQuest makes its quest the active one [documented: GECK wiki
    // ForceActiveQuest]; the player's pick afterwards wins again.
    if g.ui.active_quest_count != st.forced_seen {
        st.forced_seen = g.ui.active_quest_count;
        if let Some(q) = g.ui.active_quest {
            st.active_quest = Some(q.0);
            info!("pipboy data: active quest {q} (ForceActiveQuest)");
        }
    }
    if !st.active_quest.is_some_and(|a| out.quests.iter().any(|q| q.form == a && !q.done)) {
        st.active_quest = out.quests.iter().find(|q| !q.done).map(|q| q.form);
    }
    if let Some(w) = game.world.as_ref() {
        for n in &g.player.notes {
            let note = st.notes.entry(n.0).or_insert_with(|| read::note(w.lo, w.schema, n.0));
            if let Some(note) = note {
                if !st.notes_of_active || (note.quest.is_some() && note.quest == st.active_quest) {
                    out.notes.push(note.clone());
                }
            }
        }
    }
    out.notes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.form.cmp(&b.form)));
    if st.challenges {
        out.challenges = Some(crate::challenges::rows(g));
    }
    if let (Some(radio), Some(places)) = (game.radio.as_ref(), game.places.as_ref()) {
        out.stations = radio
            .pipboy_list
            .iter()
            .map(|&b| {
                let s = places.forms.stations.get(&b);
                (b, s.and_then(|s| s.name.clone().or_else(|| s.edid.clone())).unwrap_or_else(|| FormId(b).to_string()))
            })
            .collect();
        let p = &g.radio.pipboy;
        out.playing = p.station.filter(|_| p.on).and_then(|s| places.sites.get(&s.0)).map(|s| s.base);
    }
    out
}

fn list_len(tab: usize, l: &Lists) -> usize {
    match tab {
        QUESTS => l.quests.len(),
        NOTES => l.challenges.as_ref().map_or(l.notes.len(), Vec::len),
        RADIO => l.stations.len(),
        _ => 0,
    }
}

/// Keys, the wheel, the test aid's steps and what the pointer did.
fn take_input(st: &mut DataState, tab: usize, l: &Lists, game: &mut Game, input: &mut Input, pointer: Option<Vec2>) {
    use KeyCode::*;
    if st.popup.is_some() {
        popup_input(st, input);
        return;
    }
    let len = list_len(tab, l);
    let mut moves: Vec<i32> = Vec::new();
    let mut use_it = input.keys.any_just_pressed([Enter, NumpadEnter, Space]);
    for (k, by) in [([ArrowUp, KeyW], -1), ([ArrowDown, KeyS], 1)] {
        if input.keys.any_just_pressed(k) {
            moves.push(by);
        }
    }
    for m in input.menu.read() {
        match m.0 {
            Command::Up => moves.push(-1),
            Command::Down => moves.push(1),
            Command::One => use_it = true,
            _ => {}
        }
    }
    for by in moves {
        if len > 0 {
            st.sel[tab] = model::step(st.sel[tab], by, len);
            st.boxed = true;
            st.text_first = 0.0;
        }
    }
    if tab == NOTES && input.keys.just_pressed(KeyR) {
        st.acts.push(Act::Button(ButtonKind::Challenges));
    }
    if tab == NOTES {
        if input.keys.just_pressed(PageDown) {
            st.text_first += TEXT_H;
        }
        if input.keys.just_pressed(PageUp) {
            st.text_first -= TEXT_H;
        }
    }
    // The wheel zooms a map, scrolls a note's text, else the list.
    let mut notches = 0.0;
    for w in input.wheel.read() {
        notches += match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y / 20.0,
        };
    }
    if notches != 0.0 {
        let over_text = pointer.is_some_and(|p| p.x >= MAIN.min.x + DATA_X + TEXT_AT.x);
        match tab {
            LOCAL | WORLD => st.magnify[tab] = (st.magnify[tab].max(ZOOM.0) * ZOOM_STEP.powf(notches)).clamp(ZOOM.0, ZOOM.1),
            NOTES if over_text => st.text_first -= notches * TEXT_STEP,
            _ => {
                let fit = st.fit[tab].max(1);
                st.first[tab] = (st.first[tab] as f32 - notches).round().clamp(0.0, len.saturating_sub(fit) as f32) as usize;
            }
        }
    }
    for a in std::mem::take(&mut st.acts) {
        match a {
            Act::Hover(t, i) if t == tab && i < len => {
                if st.sel[tab] != i {
                    st.text_first = 0.0;
                }
                st.sel[tab] = i;
                st.boxed = true;
            }
            Act::Leave(t, i) if t == tab && i == st.sel[tab] => st.boxed = false,
            Act::Use(t, i) if t == tab && i < len => {
                st.sel[tab] = i;
                use_row(st, tab, i, l, game);
            }
            Act::Button(k) => button(st, k, input),
            _ => {}
        }
    }
    st.text_first = st.text_first.clamp(0.0, st.text_max.max(0.0));
    if use_it && len > 0 {
        use_row(st, tab, st.sel[tab].min(len - 1), l, game);
    }
    if len > 0 {
        st.sel[tab] = st.sel[tab].min(len - 1);
        st.first[tab] = model::keep_in_view(st.sel[tab], st.first[tab].min(len - 1), st.fit[tab]);
    }
    // A click on the World Map (the left button let go where it went down) on a marker
    // asks to travel there (`travel`).
    if tab == WORLD {
        if input.buttons.just_pressed(MouseButton::Left) {
            st.press = in_map(pointer).and(pointer).map(|p| (p, 0.0));
        } else if let (Some((from, moved)), Some(p)) = (st.press.as_mut(), pointer) {
            *moved = moved.max(p.distance(*from));
        }
        if input.buttons.just_released(MouseButton::Left) {
            if let (Some((_, moved)), Some(m)) = (st.press.take(), st.cursor_marker) {
                if moved <= CLICK_SLOP {
                    input.ask.write(FastTravelAsk(m));
                }
            }
        }
    } else {
        st.press = None;
    }
    // Dragging a map moves the point at its middle: the left button held down on it, the
    // pointer moving (or the test aid's drag).
    let held = input.buttons.pressed(MouseButton::Left);
    if !held || !(tab == LOCAL || tab == WORLD) {
        st.drag_from = None;
    } else if input.buttons.just_pressed(MouseButton::Left) {
        st.drag_from = in_map(pointer).and(pointer);
    } else if let (Some(from), Some(p)) = (st.drag_from, pointer) {
        st.drag += p - from;
        st.drag_from = Some(p);
    }
    let aid_drag = std::mem::take(&mut input.aid.drag);
    st.drag += aid_drag;
    let drag = std::mem::take(&mut st.drag);
    if aid_drag != Vec2::ZERO {
        info!("pipboy data: map dragged by ({:.0}, {:.0}), centre was {:?}", aid_drag.x, aid_drag.y, st.centre.get(tab).copied().flatten());
    }
    if drag != Vec2::ZERO && (tab == LOCAL || tab == WORLD) {
        let mag = st.magnify[tab].max(ZOOM.0);
        let size = map_of(st, game).map_or(Vec2::splat(2048.0), |m| Vec2::from(m.usable)) * mag;
        if let Some(c) = st.centre[tab].as_mut() {
            if tab == WORLD {
                c[0] = (c[0] - drag.x / size.x).clamp(0.0, 1.0);
                c[1] = (c[1] - drag.y / size.y).clamp(0.0, 1.0);
            } else {
                let scale = local_scale(game.location.as_ref().is_some_and(|l| l.interior.is_some()));
                c[0] -= drag.x * scale / mag;
                c[1] += drag.y * scale / mag;
            }
        }
    }
}

/// A click or Enter on a row: a quest becomes the active one [hypothesis: the PC's
/// click does the controller's "Make Active Quest"]; a station is tuned to, or the radio
/// turned off on the one playing [hypothesis: "Tune"].
fn use_row(st: &mut DataState, tab: usize, i: usize, l: &Lists, game: &mut Game) {
    match tab {
        QUESTS => {
            if let Some(q) = l.quests.get(i).filter(|q| !q.done) {
                st.active_quest = Some(q.form);
                info!("pipboy data: active quest {}", FormId(q.form));
            }
        }
        RADIO => {
            let (Some((base, name)), Some(places)) = (l.stations.get(i), game.places.as_ref()) else { return };
            let Some(&station) = places.station_ref.get(base) else { return };
            let Some(rt) = game.script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
            let p = &mut rt.game.radio.pipboy;
            if l.playing == Some(*base) {
                p.on = false;
                info!("pipboy data: radio off");
            } else {
                (p.on, p.station) = (true, Some(FormId(station)));
                info!("pipboy data: radio tuned to {name}");
            }
        }
        _ => {}
    }
}

/// The question box's keys and clicks (fast-travel.md §3): Up and Down (W, S) move the
/// choice, Enter, Space or E take it, Y and N answer; the pointer chooses the answer it
/// is on and a click takes it. Nothing else on the page moves meanwhile.
fn popup_input(st: &mut DataState, input: &mut Input) {
    use KeyCode::*;
    input.wheel.clear();
    st.press = None;
    st.drag_from = None;
    input.aid.drag = Vec2::ZERO;
    let acts = std::mem::take(&mut st.acts);
    let Some(p) = st.popup.as_mut() else { return };
    let mut pick = None;
    if input.keys.any_just_pressed([ArrowUp, KeyW]) {
        p.step(-1);
    }
    if input.keys.any_just_pressed([ArrowDown, KeyS]) {
        p.step(1);
    }
    if input.keys.any_just_pressed([Enter, NumpadEnter, Space, KeyE]) {
        pick = Some(p.selected);
    }
    if input.keys.just_pressed(KeyY) {
        pick = Some(0);
    }
    if input.keys.just_pressed(KeyN) {
        pick = Some(p.buttons.len() - 1);
    }
    for m in input.menu.read() {
        match m.0 {
            Command::Up => p.step(-1),
            Command::Down => p.step(1),
            Command::One => pick = Some(p.selected),
            _ => {}
        }
    }
    for a in acts {
        match a {
            Act::PopupOver(i) if i < p.buttons.len() => p.selected = i,
            Act::PopupPick(i) if i < p.buttons.len() => pick = Some(i),
            _ => {}
        }
    }
    let Some(i) = pick else { return };
    info!("fast travel: answered {:?}", p.buttons[i]);
    if let Some(m) = p.take(i) {
        input.go.write(FastTravelGo(m));
    }
    st.popup = None;
}

fn button(st: &mut DataState, k: ButtonKind, input: &mut Input) {
    match k {
        ButtonKind::NotesFilter => {
            st.notes_of_active = !st.notes_of_active;
            st.sel[NOTES] = 0;
            st.first[NOTES] = 0;
        }
        ButtonKind::ShowLocation => {
            input.pip_out.write(PipBoyInput::Tab(WORLD));
        }
        ButtonKind::Challenges => {
            st.challenges = !st.challenges;
            st.sel[NOTES] = 0;
            st.first[NOTES] = 0;
            st.text_first = 0.0;
            info!("pipboy data: {} shown", if st.challenges { "challenges" } else { "notes" });
        }
        ButtonKind::PlaceMarker => info!("pipboy data: {k:?} is not built"),
    }
}

fn map_of(st: &mut DataState, game: &Game) -> Option<WorldMap> {
    let world = game.location.as_ref()?.world;
    if let Entry::Vacant(e) = st.maps.entry(world) {
        let w = game.world.as_ref()?;
        e.insert(read::world_map(w.lo, w.schema, w.index, world));
    }
    st.maps.get(&world).cloned().flatten()
}

/// Gather what the page shows.
fn build(st: &mut DataState, tab: usize, l: &Lists, game: &Game, pointer: Option<Vec2>) -> View {
    // Under the question box the map stands still.
    let pointer = pointer.filter(|_| st.popup.is_none());
    let place = place(st, game);
    let when = game.sim.as_ref().and_then(|s| s.world.calendar().ok()).map(|c| model::date_time(&c)).unwrap_or_default();
    let mut buttons = Vec::new();
    let body = match tab {
        LOCAL | WORLD => {
            buttons.push(Button { text: "Place Marker".into(), key: None, kind: ButtonKind::PlaceMarker });
            Body::Map(if tab == WORLD { world_map(st, game, pointer) } else { local_map(st, game, pointer) })
        }
        QUESTS => {
            if !l.quests.is_empty() {
                buttons.push(Button { text: "Show Location".into(), key: None, kind: ButtonKind::ShowLocation });
            }
            let rows = l
                .quests
                .iter()
                .map(|q| Row { text: quest_name(game, q.form), mark: if Some(q.form) == st.active_quest { Mark::Filled } else { Mark::None }, done: q.done })
                .collect();
            let right = l.quests.get(st.sel[QUESTS]).map_or(Right::None, |q| Right::Objectives(objectives(game, q.form)));
            Body::List { rows, first: st.first[QUESTS], boxed: st.boxed.then_some(st.sel[QUESTS]), right }
        }
        NOTES if l.challenges.is_some() => {
            // The challenges in place of the notes; R goes back [hypothesis: the button's
            // word then].
            buttons.push(Button { text: "Notes".into(), key: Some("R)"), kind: ButtonKind::Challenges });
            let list = l.challenges.as_deref().unwrap_or_default();
            let rows = list.iter().map(|c| Row { text: c.name.clone(), mark: Mark::None, done: c.completed }).collect();
            let right = list.get(st.sel[NOTES]).map_or(Right::None, |c| {
                let state = if c.completed { "Completed".to_string() } else { format!("{}/{}", c.progress, c.threshold) };
                Right::Text { text: format!("{}\n\n{state}", c.description), first: st.text_first }
            });
            Body::List { rows, first: st.first[NOTES], boxed: st.boxed.then_some(st.sel[NOTES]), right }
        }
        NOTES => {
            let text = if st.notes_of_active { "Show All Notes" } else { "Show Active Quest Notes" };
            if !l.notes.is_empty() || st.notes_of_active {
                buttons.push(Button { text: text.into(), key: None, kind: ButtonKind::NotesFilter });
            }
            buttons.push(Button { text: "Challenges".into(), key: Some("R)"), kind: ButtonKind::Challenges });
            let rows = l.notes.iter().map(|n| Row { text: n.name.clone(), mark: Mark::None, done: false }).collect();
            let right = match l.notes.get(st.sel[NOTES]) {
                Some(n) if n.kind == 1 => Right::Text { text: n.text.clone().unwrap_or_default(), first: st.text_first },
                Some(n) if n.kind == 2 => n.image.clone().map_or(Right::None, Right::Image),
                Some(_) => Right::Audio,
                None => Right::None,
            };
            Body::List { rows, first: st.first[NOTES], boxed: st.boxed.then_some(st.sel[NOTES]), right }
        }
        _ => {
            let rows = l.stations.iter().map(|(b, n)| Row { text: n.clone(), mark: if l.playing == Some(*b) { Mark::Filled } else { Mark::None }, done: false }).collect();
            Body::List { rows, first: st.first[RADIO], boxed: st.boxed.then_some(st.sel[RADIO]), right: Right::Wave { on: l.playing.is_some() } }
        }
    };
    View { tab, place, when, buttons, body, popup: st.popup.clone() }
}

fn quest_name(game: &Game, q: u32) -> String {
    game.script.as_ref().and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.data.quests.get(&FormId(q))).and_then(|d| d.name.clone()).unwrap_or_default()
}

/// The quest's objectives shown or done, with text.
fn objectives(game: &Game, q: u32) -> Vec<Row> {
    let Some(rt) = game.script.as_ref().and_then(|s| s.rt.as_ref()) else { return Vec::new() };
    let (Some(def), Some(state)) = (rt.game.data.quests.get(&FormId(q)), rt.game.quests.get(&FormId(q))) else { return Vec::new() };
    let mut lines: Vec<ObjectiveLine> = state
        .objectives
        .iter()
        .filter(|(_, o)| o.displayed || o.completed)
        .filter_map(|(i, o)| {
            let text = def.objectives.get(i)?.text.clone()?;
            (!text.trim().is_empty()).then(|| ObjectiveLine { index: *i, text: text.trim().to_string(), done: o.completed })
        })
        .collect();
    model::sort_objectives(&mut lines);
    lines.into_iter().map(|o| Row { text: o.text, mark: if o.done { Mark::Filled } else { Mark::Empty }, done: o.done }).collect()
}

/// The heading's place name, kept per cell.
fn place(st: &mut DataState, game: &Game) -> String {
    let (Some(loc), Some(w)) = (game.location.as_ref(), game.world.as_ref()) else { return String::new() };
    let feet = game.player.as_ref().map_or([0.0; 3], |p| p.feet);
    let grid = if loc.interior.is_some() { (0, 0) } else { ((feet[0] / model::CELL).floor() as i32, (feet[1] / model::CELL).floor() as i32) };
    st.places.entry((loc.world, loc.interior, grid)).or_insert_with(|| read::place_name(w.lo, w.schema, w.index, loc.world, loc.interior, feet)).clone()
}

/// Where the player's arrow goes on the map of `world`: where the player is, or outdoors
/// where they were last [hypothesis].
fn player_at(st: &DataState, game: &Game, world: u32) -> Option<([f32; 3], f32)> {
    let (loc, player) = (game.location.as_ref()?, game.player.as_ref()?);
    if loc.interior.is_none() && loc.world == world {
        return Some((player.feet, player.heading));
    }
    st.outdoors.filter(|o| o.0 == world).map(|o| (o.1, o.2))
}

/// The pointer in a map's window, when it is over it.
fn in_map(pointer: Option<Vec2>) -> Option<Vec2> {
    let p = pointer? - MAIN.min - MAP.min;
    (p.x >= 0.0 && p.y >= 0.0 && p.x < MAP.width() && p.y < MAP.height()).then_some(p)
}

/// The highlight box: at the pointer while it is over the map, else where it was left
/// (the window's middle at first); on a marker within reach it takes the marker's place
/// and name [hypothesis: the owner's screenshots show it away from the middle, once round
/// a door with the door's place name].
fn place_cursor(st: &mut DataState, tab: usize, pointer: Option<Vec2>, marks: &mut [MapMark], names: &[String]) -> (Vec2, Option<String>, Option<usize>) {
    let want = in_map(pointer).or(st.cursor[tab]).unwrap_or(MAP_MID);
    st.cursor[tab] = Some(want);
    let near = marks.iter().enumerate().map(|(i, m)| (i, m.at.distance(want))).filter(|(_, d)| *d <= SNAP).min_by(|a, b| a.1.total_cmp(&b.1));
    match near {
        Some((i, _)) => {
            marks[i].highlighted = true;
            (marks[i].at, names.get(i).cloned(), Some(i))
        }
        None => (want, None, None),
    }
}

/// Whether the player knows a marker (`Some`), and has been there (`true`): found within
/// its radius, or shown by ShowMap, ShowAllMapMarkers (which leaves out markers flagged
/// "Show All" Hidden, FNAM bit 4 [documented: xEdit's flag names, schemas/fnv/world.json],
/// when asked to) or by its Visible flag [hypothesis].
fn known(st: &DataState, game: &Game, m: &read::Marker) -> Option<bool> {
    let hidden = m.flags & 4 != 0;
    let shown = game.script.as_ref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.map_marker_shown(FormId(m.form), hidden));
    let found = st.discovered.contains(&m.form);
    let enabled = game.sim.as_ref().is_none_or(|s| s.present(m.form));
    (enabled && (found || shown || m.flags & 1 != 0)).then_some(found)
}

fn world_map(st: &mut DataState, game: &Game, pointer: Option<Vec2>) -> MapShown {
    let Some(world) = game.location.as_ref().map(|l| l.world) else { return empty_map() };
    let Some(m) = map_of(st, game) else { return empty_map() };
    let mag = st.magnify[WORLD].max(ZOOM.0);
    let size = Vec2::from(m.usable) * mag;
    let player = player_at(st, game, world);
    let centre = *st.centre[WORLD].get_or_insert_with(|| player.map_or([0.5, 0.5], |(p, _)| m.uv(p[0], p[1])));
    let left = Vec2::new(model::pan(centre[0], size.x, MAP_MID.x), model::pan(centre[1], size.y, MAP_MID.y));
    let on_map = |x: f32, y: f32| {
        let uv = m.uv(x, y);
        left + Vec2::new(uv[0] * size.x, uv[1] * size.y)
    };
    let mut marks = Vec::new();
    let mut names = Vec::new();
    let mut forms = Vec::new();
    let all = markers(st, game).cloned().unwrap_or_default();
    for mk in all.iter().filter(|mk| mk.world == world) {
        let Some(found) = known(st, game, mk) else { continue };
        marks.push(MapMark { icon: Some((mk.kind, found)), highlighted: false, at: on_map(mk.pos[0], mk.pos[1]) });
        names.push(mk.name.clone());
        forms.push(mk.form);
    }
    let quests = quest_targets(st, game)
        .into_iter()
        .filter(|t| t.interior.is_none() && t.world == world)
        .map(|t| quest_corner(on_map(t.pos[0], t.pos[1])))
        .collect();
    if st.world_logged != Some(marks.len()) {
        st.world_logged = Some(marks.len());
        info!("pipboy data: world map shows {} of {} markers of this worldspace", marks.len(), all.iter().filter(|mk| mk.world == world).count());
    }
    let (cursor, label, under) = place_cursor(st, WORLD, pointer, &mut marks, &names);
    st.cursor_marker = under.map(|i| forms[i]);
    MapShown {
        picture: Some((m.texture.clone(), Rect::from_corners(left, left + size))),
        outline: None,
        marks,
        player: player.map(|(p, h)| (on_map(p[0], p[1]), h)),
        quests,
        cursor,
        label,
    }
}

fn empty_map() -> MapShown {
    MapShown { picture: None, outline: None, marks: Vec::new(), player: None, quests: Vec::new(), cursor: MAP_MID, label: None }
}

/// The local map's scale, game units to a canvas unit at a magnification of 1: outdoors
/// [`LOCAL_SCALE`], indoors closer ([`outline::INTERIOR_SCALE`]).
fn local_scale(interior: bool) -> f32 {
    if interior {
        outline::INTERIOR_SCALE
    } else {
        LOCAL_SCALE
    }
}

/// How long the Local Map's outline may cast rays each frame, and how many at a time.
const OUTLINE_BUDGET: std::time::Duration = std::time::Duration::from_millis(6);
const OUTLINE_BATCH: usize = 256;

/// Make the Local Map's outline of the place (a grid of rays down through the physics
/// world, [`outline`]; outdoors a ray landing nowhere takes the cell's landscape height)
/// a few hundred rays a frame, and its picture when it is done; a new one when the map is
/// panned off it or the player is elsewhere.
fn advance_outline(st: &mut DataState, game: &mut Game, images: &mut Assets<Image>) {
    let (Some(loc), Some(player)) = (game.location.as_ref(), game.player.as_ref()) else { return };
    let (interior, world, feet) = (loc.interior, loc.world, player.feet);
    let scale = local_scale(interior.is_some());
    let centre = st.centre[LOCAL].unwrap_or([feet[0], feet[1]]);
    let key = Outline::key_for(interior, world, centre, feet[2], MAP.size(), scale);
    if st.outline.as_ref().is_none_or(|o| o.key != key) {
        st.outline = Some(Outline::new(interior, world, centre, feet[2], MAP.size(), scale));
        st.outline_land.clear();
    }
    let Some(o) = st.outline.as_mut().filter(|o| !o.done()) else { return };
    let lands = &mut st.outline_land;
    let data = game.world.as_ref().filter(|_| interior.is_none()).map(|w| (w.lo, w.schema, w.index));
    let grid = data.and_then(|(_, _, index)| index.worldspaces.get(&world)).map(|ws| &ws.grid);
    let mut ground = |x: f32, y: f32| -> Option<f32> {
        let ((lo, schema, index), grid) = (data?, grid?);
        let g = ((x / model::CELL).floor() as i32, (y / model::CELL).floor() as i32);
        let heights = lands.entry(g).or_insert_with(|| {
            let land = index.cells.get(grid.get(&g)?)?.land?;
            fnv_world::land::load_land(lo, schema, land, fnv_world::land::LandParts::HEIGHTS)?.heights
        });
        fnv_phys::terrain::height_at(heights.as_ref()?, x - g.0 as f32 * model::CELL, y - g.1 as f32 * model::CELL)
    };
    let Some(phys) = game.phys.as_mut() else { return };
    let player_key = phys.player.as_ref().map(|c| c.key());
    phys.world.update_queries();
    let q = phys.world.queries();
    let mut filter = fnv_phys::Filter { exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()), no_dynamic: true, ..Default::default() };
    filter.exclude.extend(player_key);
    let started = std::time::Instant::now();
    while !o.done() && started.elapsed() < OUTLINE_BUDGET {
        o.advance(OUTLINE_BATCH, |p, reach| q.ray(p, [0.0, 0.0, -1.0], reach, &filter).map(|h| h.point[2]).or_else(|| ground(p[0], p[1])));
    }
    if o.done() {
        o.image = Some(images.add(o.picture()));
        info!(
            "pipboy data: local map outline {}x{} samples {} apart, {} landed ({})",
            o.size[0],
            o.size[1],
            o.step,
            o.hits(),
            if o.interior { "interior" } else { "exterior" }
        );
    }
}

/// The local map: the player's surroundings at [`local_scale`], the outline of the place,
/// the doors leading elsewhere and the known map markers [design: the scale; the outline
/// stands for the original's picture from above].
fn local_map(st: &mut DataState, game: &Game, pointer: Option<Vec2>) -> MapShown {
    let (Some(loc), Some(player)) = (game.location.as_ref(), game.player.as_ref()) else { return empty_map() };
    let (feet, heading) = (player.feet, player.heading);
    let mag = st.magnify[LOCAL].max(ZOOM.0);
    let centre = *st.centre[LOCAL].get_or_insert([feet[0], feet[1]]);
    let scale = local_scale(loc.interior.is_some());
    let on_map = |x: f32, y: f32| MAP_MID + Vec2::new(x - centre[0], centre[1] - y) * mag / scale;
    let outline = st.outline.as_ref().and_then(|o| {
        let image = o.image.clone()?;
        let top_left = on_map(o.origin[0], o.origin[1]);
        let size = Vec2::new(o.size[0] as f32, o.size[1] as f32) * o.step * mag / scale;
        Some((image, Rect::from_corners(top_left, top_left + size)))
    });
    let inside = |p: Vec2| p.x >= -MARKER && p.y >= -MARKER && p.x <= MAP.width() + MARKER && p.y <= MAP.height() + MARKER;
    let grid = ((centre[0] / model::CELL).floor() as i32, (centre[1] / model::CELL).floor() as i32);
    let key = (loc.interior, if loc.interior.is_some() { (0, 0) } else { grid });
    if let Entry::Vacant(e) = st.doors.entry(key) {
        let doors = match (game.world.as_ref(), game.sim.as_ref()) {
            (Some(w), Some(sim)) => {
                let cells: Vec<u32> = match loc.interior {
                    Some(c) => vec![c],
                    None => {
                        let g = w.index.worldspaces.get(&loc.world).map(|ws| &ws.grid);
                        (-LOCAL_CELLS.0..=LOCAL_CELLS.0).flat_map(|dx| (-LOCAL_CELLS.1..=LOCAL_CELLS.1).map(move |dy| (grid.0 + dx, grid.1 + dy))).filter_map(|k| g.and_then(|g| g.get(&k)).copied()).collect()
                    }
                };
                read::doors(w.lo, w.schema, w.index, sim.refs, &cells)
            }
            _ => Vec::new(),
        };
        info!("pipboy data: local map: {} doors leading elsewhere", doors.len());
        e.insert(doors);
    }
    let mut marks = Vec::new();
    let mut names = Vec::new();
    for d in st.doors.get(&key).into_iter().flatten() {
        let at = on_map(d.pos[0], d.pos[1]);
        if inside(at) {
            marks.push(MapMark { icon: None, highlighted: false, at });
            names.push(d.name.clone());
        }
    }
    if loc.interior.is_none() {
        let all = markers(st, game).cloned().unwrap_or_default();
        for mk in all.iter().filter(|mk| mk.world == loc.world) {
            let Some(found) = known(st, game, mk) else { continue };
            let at = on_map(mk.pos[0], mk.pos[1]);
            if inside(at) {
                marks.push(MapMark { icon: Some((mk.kind, found)), highlighted: false, at });
                names.push(mk.name.clone());
            }
        }
    }
    // Quest markers stay inside the window [documented: map_menu.xml
    // LocalMapQuestMarkerTemplate, clamped to the view rectangle].
    let (w, h) = (QUEST_MARKER, QUEST_MARKER * QUEST_ASPECT);
    let quests = quest_targets(st, game)
        .into_iter()
        .filter(|t| t.interior == loc.interior && (t.interior.is_some() || t.world == loc.world))
        .map(|t| {
            let c = quest_corner(on_map(t.pos[0], t.pos[1]));
            Vec2::new(c.x.clamp(0.0, MAP.width() - w), c.y.clamp(0.0, MAP.height() - 0.75 * h))
        })
        .collect();
    let (cursor, label, _) = place_cursor(st, LOCAL, pointer, &mut marks, &names);
    MapShown { picture: None, outline, marks, player: Some((on_map(feet[0], feet[1]), heading)), quests, cursor, label }
}

/// A quest marker's top left for a target at `at` in the window.
fn quest_corner(at: Vec2) -> Vec2 {
    at - Vec2::new(QUEST_MARKER, QUEST_MARKER * QUEST_ASPECT) / 2.0 + QUEST_NUDGE
}

/// Where a quest target is: in an interior (`interior`), or outdoors in `world`.
struct Target {
    interior: Option<u32>,
    world: u32,
    pos: [f32; 3],
}

/// The targets of the active quest's objectives shown and not done, where they are now;
/// a target indoors also at the door outdoors into its interior [hypothesis: as the
/// owner's screenshots show the marker of "Talk to Sunny Smiles in the Prospector
/// Saloon" outdoors in Goodsprings]. The targets' conditions are not tested.
fn quest_targets(st: &mut DataState, game: &Game) -> Vec<Target> {
    let mut out = Vec::new();
    let (Some(q), Some(sim)) = (st.active_quest, game.sim.as_ref()) else { return out };
    let Some(rt) = game.script.as_ref().and_then(|s| s.rt.as_ref()) else { return out };
    let (Some(def), Some(state)) = (rt.game.data.quests.get(&FormId(q)), rt.game.quests.get(&FormId(q))) else { return out };
    let refs = sim.refs;
    for (i, o) in &state.objectives {
        if !o.displayed || o.completed {
            continue;
        }
        for t in def.objectives.get(i).into_iter().flat_map(|d| &d.targets) {
            let Some(r) = t.target else { continue };
            let (cell, pos) = match sim.world.peek(r) {
                Some(s) => (s.cell, s.pos),
                None => match refs.get(r) {
                    Some(p) => (p.cell, p.pos),
                    None => continue,
                },
            };
            let Some(cell) = cell else { continue };
            if let Some(w) = refs.cell_world.get(&cell) {
                out.push(Target { interior: None, world: w.0, pos });
                continue;
            }
            out.push(Target { interior: Some(cell.0), world: 0, pos });
            let door = *st.doors_into.entry(cell.0).or_insert_with(|| {
                refs.refs.values().find_map(|d| {
                    let t = d.teleport?;
                    (refs.get(t.door)?.cell == Some(cell)).then_some(())?;
                    Some((refs.cell_world.get(&d.cell?)?.0, d.pos))
                })
            });
            if let Some((w, p)) = door {
                out.push(Target { interior: None, world: w, pos: p });
            }
        }
    }
    out
}

// ---- drawing ------------------------------------------------------------------------------

/// A row of a tab's list (tab, row).
#[derive(Component, Clone, Copy)]
struct RowHit(usize, usize);

/// A button at the right.
#[derive(Component, Clone, Copy)]
struct ButtonHit(ButtonKind);

/// The map's window, dragged to pan.
#[derive(Component, Clone, Copy)]
struct MapHit;

/// An answer of the question box (0-based).
#[derive(Component, Clone, Copy)]
struct PopupHit(usize);

fn on_popup_over(e: On<Pointer<Over>>, hits: Query<&PopupHit>, state: Option<ResMut<DataState>>) {
    if let (Ok(h), Some(mut s)) = (hits.get(e.entity), state) {
        s.acts.push(Act::PopupOver(h.0));
    }
}

fn on_popup_click(e: On<Pointer<Click>>, hits: Query<&PopupHit>, state: Option<ResMut<DataState>>) {
    if let (Ok(h), Some(mut s)) = (hits.get(e.entity), state) {
        s.acts.push(Act::PopupPick(h.0));
    }
}

fn on_row_over(e: On<Pointer<Over>>, rows: Query<&RowHit>, state: Option<ResMut<DataState>>) {
    if let (Ok(r), Some(mut s)) = (rows.get(e.entity), state) {
        s.acts.push(Act::Hover(r.0, r.1));
    }
}

fn on_row_out(e: On<Pointer<Out>>, rows: Query<&RowHit>, state: Option<ResMut<DataState>>) {
    if let (Ok(r), Some(mut s)) = (rows.get(e.entity), state) {
        s.acts.push(Act::Leave(r.0, r.1));
    }
}

fn on_row_click(e: On<Pointer<Click>>, rows: Query<&RowHit>, state: Option<ResMut<DataState>>) {
    if let (Ok(r), Some(mut s)) = (rows.get(e.entity), state) {
        s.acts.push(Act::Use(r.0, r.1));
    }
}

fn on_button_click(e: On<Pointer<Click>>, buttons: Query<&ButtonHit>, state: Option<ResMut<DataState>>) {
    if let (Ok(b), Some(mut s)) = (buttons.get(e.entity), state) {
        s.acts.push(Act::Button(b.0));
    }
}

fn show(p: &mut ChildSpawnerCommands, c: &mut Ctx, st: &mut DataState, v: &View) {
    heading(p, c, &v.place, &v.when);
    buttons(p, c, &v.buttons);
    match &v.body {
        Body::Map(m) => map(p, c, m),
        Body::List { rows, first, boxed, right } => {
            st.fit[v.tab] = list(p, c, MAIN.min + LIST.min, LIST.height(), true, rows, *first, *boxed, Some(v.tab));
            let data_y = BUTTONS.y + if v.buttons.is_empty() { 0.0 } else { BUTTON_RECT_H } - button_height(c);
            let data = MAIN.min + Vec2::new(DATA_X, data_y);
            match right {
                Right::None => {}
                Right::Objectives(o) => {
                    list(p, c, data, OBJECTIVES_H, false, o, 0, None, None);
                }
                Right::Text { text, first } => st.text_max = note_text(p, c, data + TEXT_AT, text, *first),
                Right::Image(path) => {
                    c.image(p, path, at(data.x + DATA_W - IMAGE, data.y + TEXT_AT.y, IMAGE, IMAGE), amber(TEXT));
                }
                Right::Audio => {
                    let y = data.y + TEXT_AT.y + c.line_height(2) + 30.0;
                    c.image(p, "Interface\\Shared\\Meter\\tickmark_horiz.dds", at(data.x, y - TICKS + 2.0, DATA_W, TICKS), amber(LINE));
                }
                Right::Wave { on } => wave(p, c, *on),
            }
        }
    }
    if let Some(q) = &v.popup {
        popup(p, c, q);
    }
}

/// The question box in the middle of the map's window (fast-travel.md §3): black with a
/// line all round, the text centred at the top, the answers centred one under the other,
/// the chosen one framed. It takes the pointer over the whole page, so nothing under it
/// is clicked.
fn popup(p: &mut ChildSpawnerCommands, c: &mut Ctx, q: &Popup) {
    let lines = c.lines(2, &q.text, Some(POPUP_W - 2.0 * POPUP_PAD));
    let pitch = c.line_height(2) + 4.0;
    let row = c.line_height(2) + POPUP_ROW_PAD;
    let text_h = c.line_height(2) + pitch * lines.len().saturating_sub(1) as f32;
    let h = POPUP_PAD + text_h + POPUP_PAD + row * q.buttons.len() as f32 + POPUP_PAD;
    let mid = MAIN.min + MAP.min + MAP_MID;
    let r = Rect::new(mid.x - POPUP_W / 2.0, mid.y - h / 2.0, mid.x + POPUP_W / 2.0, mid.y + h / 2.0);
    p.spawn((at(0.0, 0.0, super::CANVAS.x, super::CANVAS.y), Pickable::default()));
    c.boxed(p, r, amber(LINE), Some(Color::BLACK));
    for (i, l) in lines.iter().enumerate() {
        c.set(p, 2, l, mid.x, r.min.y + POPUP_PAD + pitch * i as f32, Align::Centre, amber(TEXT));
    }
    let frame_w = q.buttons.iter().map(|b| c.width(2, b)).fold(0.0, f32::max) + POPUP_FRAME_PAD;
    let top = r.min.y + POPUP_PAD + text_h + POPUP_PAD;
    for (i, b) in q.buttons.iter().enumerate() {
        let y = top + row * i as f32;
        let frame = Rect::new(mid.x - frame_w / 2.0, y, mid.x + frame_w / 2.0, y + row - 1.0);
        if i == q.selected {
            c.boxed(p, frame, amber(LINE), Some(amber(FILL.0).with_alpha(FILL.1 / 255.0)));
        }
        c.text(p, 2, b, mid.x, y + POPUP_ROW_PAD / 2.0, Align::Centre, amber(TEXT));
        p.spawn((at(frame.min.x, frame.min.y, frame.width(), frame.height()), PopupHit(i), Pickable::default())).observe(on_popup_over).observe(on_popup_click);
    }
}

/// The heading: the title, the place and the date and time between their lines.
fn heading(p: &mut ChildSpawnerCommands, c: &mut Ctx, place: &str, when: &str) {
    let (x0, y0) = (MAIN.min.x, MAIN.min.y);
    let line = amber(LINE);
    let bright = amber(TEXT);
    c.fade(p, x0, y0, FADE, false, line);
    c.line(p, x0, y0, LEAD_LINE, THICK, line);
    let title_top = y0 - base(&c.kit.face(4).font) + THICK;
    let w = c.text(p, 4, Page::Data.title(), x0 + TITLE_X, title_top, Align::Left, bright);
    let from = LINE2.0.max(TITLE_X + w + 20.0);
    c.line(p, x0 + from, y0, LINE2.0 + LINE2.1 - from, THICK, line);
    c.text(p, 2, place, x0 + PLACE_X, y0 + HEAD_TEXT_Y, Align::Right, bright);
    c.fade(p, x0 + SPLIT_X, y0, FADE, false, line);
    c.line(p, x0 + LINE3_X, y0, MAIN.width() - LINE3_X, THICK, line);
    c.text(p, 2, when, x0 + TIME_X, y0 + HEAD_TEXT_Y, Align::Right, bright);
    c.fade(p, x0 + MAIN.width() - THICK, y0, FADE, false, line);
}

fn button_height(c: &Ctx) -> f32 {
    c.line_height(2) + BUTTON_VERBUF
}

/// The buttons at the right, one under the other.
fn buttons(p: &mut ChildSpawnerCommands, c: &mut Ctx, list: &[Button]) {
    let bright = amber(TEXT);
    let h = button_height(c);
    for (k, b) in list.iter().enumerate() {
        let w = c.width(2, &b.text) + BUTTON_HORBUF;
        let right = MAIN.min.x + BUTTONS.x;
        let top = MAIN.min.y + BUTTONS.y + h * k as f32;
        let text_top = top + (h - c.line_height(2)) / 2.0 + BUTTON_TEXT_DROP;
        c.text(p, 2, &b.text, right - BUTTON_HORBUF / 2.0, text_top, Align::Right, bright);
        if let Some(key) = b.key {
            c.text(p, 2, key, right + KEY_GAP, text_top, Align::Left, bright);
        }
        p.spawn((at(right - w, top, w, h), ButtonHit(b.kind), Pickable::default())).observe(on_button_click);
    }
}

/// A list's rows from `first` in a window `height` high at `origin` (canvas), the scroll
/// bar on the left or the right; rows of a tab's list take the pointer. Returns how many
/// rows fit.
#[allow(clippy::too_many_arguments)]
fn list(p: &mut ChildSpawnerCommands, c: &mut Ctx, origin: Vec2, height: f32, bar_left: bool, rows: &[Row], first: usize, boxed: Option<usize>, tab: Option<usize>) -> usize {
    let bright = amber(TEXT);
    let line = amber(LINE);
    let x0 = if bar_left { SCROLLBAR } else { 0.0 };
    let row_w = LIST_W - SCROLLBAR - ROW_SHORT;
    let mut fit = 0;
    p.spawn((Node { overflow: Overflow::clip(), ..at(origin.x, origin.y, LIST_W, height) }, Pickable::IGNORE)).with_children(|p| {
        let mut y = 0.0;
        for (i, r) in rows.iter().enumerate().skip(first) {
            let lines = c.lines(2, &r.text, Some(WRAP));
            let h = ROW_H + ROW_PITCH * lines.len().saturating_sub(1) as f32;
            if y + h > height && fit > 0 {
                break;
            }
            let colour = if r.done { bright.with_alpha(DONE_ALPHA) } else { bright };
            if boxed == Some(i) {
                c.boxed(p, Rect::new(x0 - BOX_LEFT, y, x0 + row_w, y + h - 1.0), line, None);
            }
            let mark = match r.mark {
                Mark::None => None,
                Mark::Empty => Some("Interface\\Shared\\Marker\\square.dds"),
                Mark::Filled => Some("Interface\\Shared\\Marker\\square_filled.dds"),
            };
            if let Some(m) = mark {
                if c.image(p, m, at(x0 + MARK_AT.x, y + MARK_AT.y, MARK, MARK), colour).is_none() {
                    p.spawn((at(x0 + MARK_AT.x, y + MARK_AT.y, MARK, MARK), BackgroundColor(colour)));
                }
            }
            for (k, l) in lines.iter().enumerate() {
                c.set(p, 2, l, x0 + ROW_TEXT.x, y + ROW_TEXT.y - THICK + ROW_PITCH * k as f32, Align::Left, colour);
            }
            if let Some(t) = tab {
                p.spawn((at(x0, y, row_w, h), RowHit(t, i), Pickable::default())).observe(on_row_over).observe(on_row_out).observe(on_row_click);
            }
            y += h;
            fit += 1;
        }
        if first > 0 || first + fit < rows.len() {
            let bar_x = if bar_left { 0.0 } else { LIST_W - SCROLLBAR };
            scroll_bar(p, c, bar_x, height, first as f32 / rows.len() as f32, fit as f32 / rows.len() as f32);
        }
    });
    fit
}

/// A scroll bar: an arrow at each end and a line between them for the part shown
/// [hypothesis: the marker's look; the arrows are the file's, 32 × 16].
pub(super) fn scroll_bar(p: &mut ChildSpawnerCommands, c: &mut Ctx, x: f32, height: f32, from: f32, part: f32) {
    let line = amber(LINE);
    c.image(p, "Interface\\Shared\\scrollbar\\arrow_up.dds", at(x, 0.0, SCROLLBAR, ARROW_H), line);
    c.image(p, "Interface\\Shared\\scrollbar\\arrow_down.dds", at(x, height - ARROW_H, SCROLLBAR, ARROW_H), line);
    let run = height - 2.0 * ARROW_H - 8.0;
    c.line(p, x + (SCROLLBAR - THICK) / 2.0, ARROW_H + 4.0 + run * from.clamp(0.0, 1.0), THICK, (run * part.clamp(0.0, 1.0)).max(8.0), line);
}

/// A note's text from `first` down, in a window at `origin`; returns how far it can
/// scroll.
fn note_text(p: &mut ChildSpawnerCommands, c: &mut Ctx, origin: Vec2, text: &str, first: f32) -> f32 {
    let bright = amber(TEXT);
    let text = text.replace("\r\n", "\n");
    let pitch = c.line_height(1);
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let l = c.lines(1, para, Some(DATA_W));
        if l.is_empty() {
            lines.push(None);
        }
        lines.extend(l.into_iter().map(Some));
    }
    let total = pitch * lines.len() as f32;
    // The window reaches a little over the first line so its letters are not cut
    // [design].
    p.spawn((Node { overflow: Overflow::clip(), ..at(origin.x, origin.y - TEXT_PAD, DATA_W, TEXT_H + TEXT_PAD) }, Pickable::IGNORE)).with_children(|p| {
        for (i, l) in lines.iter().enumerate() {
            let y = pitch * i as f32 - first;
            if y + pitch < 0.0 || y > TEXT_H {
                continue;
            }
            if let Some(l) = l {
                c.set(p, 1, l, 0.0, y + TEXT_PAD, Align::Left, bright);
            }
        }
    });
    if total > TEXT_H {
        let line = amber(LINE);
        let x = origin.x + DATA_W;
        c.image(p, "Interface\\Shared\\scrollbar\\arrow_up.dds", at(x, origin.y, SCROLLBAR, ARROW_H), line);
        c.image(p, "Interface\\Shared\\scrollbar\\arrow_down.dds", at(x, origin.y + TEXT_H - ARROW_H, SCROLLBAR, ARROW_H), line);
    }
    (total - TEXT_H).max(0.0)
}

/// The radio's waveform panel.
fn wave(p: &mut ChildSpawnerCommands, c: &mut Ctx, on: bool) {
    let line = amber(LINE);
    let o = MAIN.min + WAVE.min;
    let (w, h) = (WAVE.width(), WAVE.height());
    c.image(p, "Interface\\Shared\\Meter\\tickmark_horiz.dds", at(o.x, o.y + h - TICKS + 2.0, w, TICKS), line);
    c.image(p, "Interface\\Shared\\Meter\\tickmark_vert.dds", at(o.x + w - TICKS + 2.0, o.y, TICKS, h), line);
    let mid = o.y + (h - THICK) / 2.0 - 8.0;
    let len = w - 5.0;
    if !on {
        c.line(p, o.x, mid, len, THICK, line);
        return;
    }
    // A still sine wave [design: the original plays an animated model here].
    let at_x = |i: usize| i as f32 / WAVE_SEGMENTS as f32 * len;
    let at_y = |x: f32| mid - WAVE_AMPLITUDE * (x / len * WAVE_CYCLES * std::f32::consts::TAU).sin() * (1.0 - (2.0 * x / len - 1.0).powi(2)).sqrt();
    for i in 0..WAVE_SEGMENTS {
        let (xa, xb) = (at_x(i), at_x(i + 1));
        let (a, b) = (Vec2::new(xa, at_y(xa)), Vec2::new(xb, at_y(xb)));
        let d = b - a;
        let middle = (a + b) / 2.0;
        let length = d.length() + THICK / 2.0;
        p.spawn((
            at(o.x + middle.x - length / 2.0, middle.y - THICK / 2.0, length, THICK),
            BackgroundColor(line),
            UiTransform::from_rotation(Rot2::radians(d.y.atan2(d.x))),
            Pickable::IGNORE,
        ));
    }
}

/// A map in its window: the picture, the markers, the player's arrow and the highlight
/// box; the window takes drags.
fn map(p: &mut ChildSpawnerCommands, c: &mut Ctx, m: &MapShown) {
    let o = MAIN.min + MAP.min;
    p.spawn((Node { overflow: Overflow::clip(), ..at(o.x, o.y, MAP.width(), MAP.height()) }, Pickable::IGNORE)).with_children(|p| {
        if let Some((path, r)) = &m.picture {
            c.image(p, path, at(r.min.x, r.min.y, r.width(), r.height()), amber(TEXT).with_alpha(MAP_ALPHA / 255.0));
        }
        if let Some((image, r)) = &m.outline {
            p.spawn((at(r.min.x, r.min.y, r.width(), r.height()), ImageNode { color: amber(TEXT), image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) }, Pickable::IGNORE));
        }
        // In the menu file's depth order: markers 1, quest markers 4, the player's arrow
        // 7, the highlighted marker 11 [documented: map_menu.xml].
        let marker = |p: &mut ChildSpawnerCommands, c: &mut Ctx, mk: &MapMark| {
            let (size, brightness) = if mk.highlighted { (MARKER * MARKER_GROW, MARKER_BRIGHTNESS.1) } else { (MARKER, MARKER_BRIGHTNESS.0) };
            c.image(p, &mk.picture(), at(mk.at.x - size / 2.0, mk.at.y - size / 2.0, size, size), amber(brightness));
        };
        for mk in m.marks.iter().filter(|mk| !mk.highlighted) {
            marker(p, c, mk);
        }
        for q in &m.quests {
            c.image(p, QUEST_FILE, at(q.x, q.y, QUEST_MARKER, QUEST_MARKER * QUEST_ASPECT), amber(TEXT));
        }
        if let Some((spot, heading)) = m.player {
            if let Some((image, rect)) = c.picture(CURSOR_FILE) {
                let (w, h) = (CURSOR, CURSOR * CURSOR_ASPECT);
                p.spawn((
                    at(spot.x - w / 2.0, spot.y - h / 2.0, w, h),
                    ImageNode { rect, color: amber(TEXT), image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) },
                    UiTransform::from_rotation(Rot2::radians(heading)),
                ));
            }
        }
        for mk in m.marks.iter().filter(|mk| mk.highlighted) {
            marker(p, c, mk);
        }
        highlight(p, c, m.cursor, m.label.as_deref());
        p.spawn((at(0.0, 0.0, MAP.width(), MAP.height()), MapHit, Pickable::default()));
    });
}

/// The highlight box round `centre`: four corners of two arms each with fading tips, the
/// dot at its middle and the name under it.
fn highlight(p: &mut ChildSpawnerCommands, c: &mut Ctx, centre: Vec2, label: Option<&str>) {
    let line = amber(LINE);
    let o = centre - Vec2::splat(HBOX / 2.0);
    let (arm, tip, t) = (HBOX_ARM, HBOX_TIP, THICK);
    for (left, top) in [(true, true), (false, true), (true, false), (false, false)] {
        let x = if left { o.x } else { o.x + HBOX - t };
        let y = if top { o.y } else { o.y + HBOX - t };
        // Across from the corner, then a tip fading further along.
        c.line(p, if left { o.x } else { o.x + HBOX - arm }, y, arm, t, line);
        let (name, tip_x) = if left { ("fade_to_right.dds", o.x + arm) } else { ("fade_to_left.dds", o.x + HBOX - arm - tip) };
        c.image(p, name, at(tip_x, y, tip, t), line);
        // Down or up from the corner, with its tip.
        c.line(p, x, if top { o.y } else { o.y + HBOX - arm }, t, arm, line);
        let (name, tip_y) = if top { ("fade_to_bottom.dds", o.y + arm) } else { ("fade_to_top.dds", o.y + HBOX - arm - tip) };
        c.image(p, name, at(x, tip_y, t, tip), line);
    }
    c.image(p, "Interface\\Worldmap\\waypoint_node.dds", at(centre.x - DOT / 2.0, centre.y - DOT / 2.0, DOT, DOT), line);
    if let Some(name) = label {
        c.text(p, 1, name, centre.x, o.y + HBOX_NAME_Y, Align::Centre, amber(TEXT));
    }
}
