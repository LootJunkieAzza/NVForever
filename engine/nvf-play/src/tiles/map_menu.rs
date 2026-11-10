//! The Pip-Boy's DATA page drawn from `menus\main\map_menu.xml`, behind `[UI] TileMenus=MapMenu`
//! [design: ui-tile-tree.md §6.2 step 3; the contract: docs/spec-notes/pipboy.md §5.3.1; issue
//! #1513]. Off (the default), none of this runs and the hand-built page (`crate::pipboy::data`)
//! prepares and draws as it always did.
//!
//! - **The tabs it draws** ([`TILED_TABS`]): Quests. The Local Map, the World Map, Misc and Radio
//!   keep the whole hand-built page while they show, as STATS' tabs not yet drawn from its file do.
//! - **The one decision** ([`MapTiles::ready`]): the switch on, DATA showing on a tab it draws, the
//!   page prepared for that tab (`crate::pipboy::data::prepare`, before this), no question box up,
//!   the file's tiles there ([`built`]), the tabs made and a usable measured layout ([`usable`]).
//!   Painting (`super::draw`), the frame (`crate::pipboy::frame::draw`) and input read it and
//!   nothing else; [`settle`] publishes it before them. A pass that fails leaves the whole
//!   hand-built page, says why once, and after [`FAILING_PASSES`] in a row is tried again only when
//!   what it would show or the tree changes ([`TreeMark`]).
//! - **The rules stay the hand-built page's**: what the page shows is `pipboy::data`'s prepared view
//!   ([`crate::pipboy::data::Prepared`]); this module writes it as the file's "set in code" traits
//!   and says what a tile stands for; the player's acts go back through `DataState`'s acts as the
//!   hand-built nodes' do. Only the quests list's scroll is its own (`super::list`), which the
//!   model keeps in rows, as ITEMS' list (`super::inventory_menu`).
//! - **Exactly one presentation publishes** what the page's rules read of the drawing: how many
//!   rows show, how far the objectives scroll and their window ([`DataState::publish_drawn`]), from
//!   a pass that went through only.
//! - **The hints** are the hand-built page's set (`data::tab_buttons`, #1897 / #1952), bound to the
//!   file's A, X and Y tiles by what each does, never by its place in the list ([`Slot::of`]).
//! - **Input** ([`feed`], before the scripts): the pointer is the Pip-Boy's own in canvas units
//!   (`crate::pipboy::PipBoyScreen::pointer`), read and never written; ignored with Controller
//!   chosen, as on the hand-built page. The keys, the wheel and the sticks stay `pipboy::data`'s
//!   ([`HAND_BUILT_KEYS`]); a file's binding acts for a script's tap, not for a physical press.

use super::draw::{Look, Piece};
use super::input::{frame_keys, log_report, Devices};
use super::list::{self, BarPiece, ListView, RowTile, Wanted};
use super::menus::{self, SizesWritten, Tiled};
use super::stats_menu::{self, find_named, modal_over, shows_whole, FlatChanges, PageFonts};
use super::vanilla::TileMenus;
use crate::pipboy::data::{self, Act, BarPart, Body, ButtonKind, DataState, Mark, Prepared, Right};
use crate::pipboy::sounds::TabHovered;
use crate::pipboy::{Frame, Page, PipBoy, PipBoyAid, PipBoyInput, PipBoyScreen};
use crate::script::ScriptState;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use fnv_ui::{TileId, Ui, Value};
use std::collections::{HashMap, HashSet};

/// The menu's name [documented: map_menu.xml].
pub const MAP_MENU: &str = "MapMenu";
/// Test aid: a tile or template of the file to take as missing, as `stats_menu::CUT_AID`:
/// `<name>`, or `<name>@<seconds>` to stop at that time, for the page's recovery.
pub const CUT_AID: &str = "NVF_PLAY_TEST_MAP_TILE_CUT";

macro_rules! glow {
    ($path:literal) => {
        concat!("MapMenu/GLOW_BRANCH/", $path)
    };
}
macro_rules! main_rect {
    ($path:literal) => {
        concat!("MapMenu/GLOW_BRANCH/MM_MainRect/", $path)
    };
}

// The tiles the model writes and the input reads, by path [documented: map_menu.xml].
const BACKGROUND: &str = glow!("MM_Background");
const TABLINE: &str = glow!("MM_Tabline");
const TITLE: &str = main_rect!("MM_HeadlineRect/MM_Headline_Title");
const LINE: &str = main_rect!("MM_HeadlineRect/MM_Headline_h2");
const PLACE: &str = main_rect!("MM_HeadlineRect/MM_Headline_LocationInfo");
const TIME: &str = main_rect!("MM_HeadlineRect/MM_Headline_TimeDateInfo");
const QUESTS_LIST: &str = main_rect!("MM_QuestsList");
const CHALLENGE_TYPE: &str = main_rect!("MM_Headline_ChallengeType");
const BUTTON_RECT: &str = main_rect!("MM_ButtonRect");
const DATA_RECT: &str = main_rect!("MM_DataRect");
const OBJECTIVES: &str = main_rect!("MM_DataRect/MM_QuestObjectivesList");
/// The lists' own names, for what a click is on (`list::hit`).
const QUESTS_NAME: &str = "MM_QuestsList";
const OBJECTIVES_NAME: &str = "MM_QuestObjectivesList";
const ROW_TEMPLATE: &str = "MM_ListMarkerTemplate";
const ROW: &str = "MM_ListItem";
const ROW_TEXT: &str = "ListItemText";
const ROW_MARK: &str = "MM_Template_ItemMarker";
const TAB_TEMPLATE: &str = "TabButtonTemplate";
const TAB_ROW: &str = "TabButton";
/// Inside a tab, as ITEMS' (`super::inventory_menu`) [hypothesis: the same prefab].
const TAB_INNER: &str = "stb_text_box";
/// `MM_DataRect`'s `_ItemType` for the objectives [documented: `MM_QuestObjectivesList`'s
/// `visible`].
const ITEM_OBJECTIVES: f64 = 4.0;
/// A done row's `_TextAlpha`: half strength, as the hand-built page dims it (`data::DONE_ALPHA`)
/// [design]; the template's own is 255 [documented].
const DONE_ALPHA: f64 = 128.0;
const FULL_ALPHA: f64 = 255.0;

/// The tabs drawn from the file; the others keep the hand-built page [design, #1513].
pub const TILED_TABS: [usize; 1] = [data::QUESTS];

/// The file's three hint tiles [documented: map_menu.xml `MM_ButtonRect`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    A,
    X,
    Y,
}

const BUTTONS: [(&str, Slot); 3] = [("MM_ButtonA", Slot::A), ("MM_ButtonX", Slot::X), ("MM_ButtonY", Slot::Y)];

impl Slot {
    /// The tile a hint stands on, by what it does: Enter's work on A (Travel, Set Active, Play,
    /// Tune), the challenges' toggle on Y (the root's Y and R bind it), the rest on X (Place Marker,
    /// Show Location, the notes' filter, Next Challenge; the root's X binds it) [documented: the
    /// tiles' `_Title_*` and the root's bindings].
    pub(crate) fn of(kind: ButtonKind) -> Slot {
        if kind.is_a() {
            Slot::A
        } else if kind == ButtonKind::Challenges {
            Slot::Y
        } else {
            Slot::X
        }
    }
}

fn button_path(name: &str) -> String {
    format!("{BUTTON_RECT}/{name}")
}

/// Every path the model writes or input reads.
fn required_paths() -> Vec<String> {
    let mut paths: Vec<String> = [MAP_MENU, BACKGROUND, TABLINE, TITLE, LINE, PLACE, TIME, QUESTS_LIST, BUTTON_RECT, DATA_RECT, OBJECTIVES].iter().map(|s| (*s).to_string()).collect();
    paths.push(format!("{QUESTS_LIST}/{}", list::HIGHLIGHT));
    for l in [QUESTS_LIST, OBJECTIVES] {
        for piece in [list::MARKER, list::UP, list::DOWN, list::PAGE_UP, list::PAGE_DOWN] {
            paths.push(format!("{l}/{}/{piece}", list::SCROLLBAR));
        }
    }
    paths.extend(BUTTONS.iter().map(|(name, _)| button_path(name)));
    paths
}

/// Whether the file has every tile and template the page needs; a file a mod restructured keeps
/// the hand-built page whole. `cut`: [`CUT_AID`].
pub fn built(ui: &Ui, cut: Option<&str>) -> Tiled {
    let paths = required_paths();
    if let Some(cut) = cut {
        if cut.eq_ignore_ascii_case(ROW_TEMPLATE) || cut.eq_ignore_ascii_case(TAB_TEMPLATE) {
            return Tiled::No(format!("{cut} missing"));
        }
        if let Some(path) = paths.iter().find(|p| p.rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case(cut))) {
            return Tiled::No(format!("{path} missing"));
        }
    }
    if let Some(missing) = paths.iter().find(|p| ui.find_tile(p).is_none()) {
        return Tiled::No(format!("{missing} missing"));
    }
    let rows = list::template_has(ui, MAP_MENU, ROW_TEMPLATE, ROW, &[("text", ROW_TEXT), ("image", ROW_MARK)]);
    if !rows.yes() {
        return rows;
    }
    let tab_template = ui.menu(MAP_MENU).is_some_and(|m| ui.tile(m).templates.iter().any(|t| t.name.eq_ignore_ascii_case(TAB_TEMPLATE)));
    if !tab_template {
        return Tiled::No(format!("{TAB_TEMPLATE} missing"));
    }
    Tiled::Yes
}

/// A list row as written: what it stands for, its words, its square filled and whether it is done.
#[derive(Clone, Debug, PartialEq)]
pub struct RowShown {
    pub key: u32,
    pub text: String,
    pub filled: bool,
    pub done: bool,
}

/// A hint as written on its tile.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HintShown {
    pub slot: Slot,
    pub kind: ButtonKind,
    pub word: String,
    pub key: String,
    pub dim: bool,
}

/// What the page shows of the model, as written last. A plain snapshot, so tests need no world.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shown {
    pub tab: usize,
    pub device: bool,
    pub place: String,
    pub when: String,
    pub rows: Vec<RowShown>,
    /// The selected row, and whether it is boxed (the pointer on it, or a key moved it).
    pub selected: Option<usize>,
    pub boxed: bool,
    pub pointer_pick: bool,
    pub navigated: u32,
    /// The selected quest's objectives (their place is their key), none without a quest selected.
    pub objectives: Option<Vec<RowShown>>,
    /// How far the objectives are scrolled (`text_first`).
    pub pane_first: f32,
    pub hints: Vec<HintShown>,
}

impl Shown {
    /// What the prepared page `p` shows, in the frame `device`; why it cannot be drawn from the
    /// file otherwise.
    pub(crate) fn of(p: &Prepared, device: bool) -> Result<Shown, String> {
        let v = &p.view;
        if v.popup.is_some() {
            return Err("the question box is up".into());
        }
        let Body::List { rows, boxed, right, .. } = &v.body else { return Err("the tab shows no list".into()) };
        if rows.len() != p.keys.len() {
            return Err("the rows and their records differ".into());
        }
        let row = |key: u32, r: &data::Row| RowShown { key, text: r.text.clone(), filled: r.mark == Mark::Filled, done: r.done };
        let (objectives, pane_first) = match right {
            Right::Objectives { rows, first } => (Some(rows.iter().enumerate().map(|(i, r)| row(i as u32, r)).collect()), *first),
            Right::None => (None, 0.0),
            _ => return Err("the right pane is not the objectives".into()),
        };
        let hints = v.buttons.iter().map(|b| HintShown { slot: Slot::of(b.kind), kind: b.kind, word: b.hint.word.to_string(), key: b.hint.key.clone(), dim: b.hint.dim }).collect();
        Ok(Shown {
            tab: v.tab,
            device,
            place: v.place.clone(),
            when: v.when.clone(),
            rows: rows.iter().zip(&p.keys).map(|(r, k)| row(*k, r)).collect(),
            selected: (!rows.is_empty()).then(|| p.sel.min(rows.len() - 1)),
            boxed: boxed.is_some(),
            pointer_pick: p.pointer_pick,
            navigated: p.navigated,
            objectives,
            pane_first,
            hints,
        })
    }
}

/// Sets a trait by path as the game's code does (`Ui::write_changed`): what a script set
/// meanwhile stands until the model changes.
fn put(ui: &mut Ui, path: &str, name: &str, v: Value) {
    if let Some(t) = ui.find_tile(path) {
        ui.write_changed(t, name, v);
    }
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

fn on(b: bool) -> Value {
    Value::Num(if b { 1.0 } else { 0.0 })
}

fn num(ui: &Ui, t: TileId, name: &str) -> f64 {
    ui.tile(t).get(name).map_or(0.0, Value::num)
}

/// One row's code-fed traits: its words on the row, which the template's text copies; its square
/// by `_selected` (and `_ShowEmptyMarker` for one shown unselected); its words' and square's
/// strength by `_TextAlpha` [documented: `MM_ListMarkerTemplate`].
fn write_row(ui: &mut Ui, tile: TileId, r: &RowShown, empty_marker: bool) {
    ui.write_changed(tile, "string", text(&r.text));
    ui.write_changed(tile, "_selected", on(r.filled));
    ui.write_changed(tile, "_ShowEmptyMarker", on(empty_marker));
    ui.write_changed(tile, "_TextAlpha", Value::Num(if r.done { DONE_ALPHA } else { FULL_ALPHA }));
}

/// Write what the page shows of `s`: the heading, the quests and the selected quest's objectives as
/// template rows, and the hints. The rows' places and the scrolls are [`page`]'s, once measured.
fn write_model(ui: &mut Ui, s: &Shown, w: &mut Written) -> Result<(), String> {
    put(ui, BACKGROUND, "visible", on(false));
    put(ui, CHALLENGE_TYPE, "visible", on(false));
    put(ui, PLACE, "string", text(&s.place));
    put(ui, TIME, "string", text(&s.when));
    // The list takes the pointer (its rows' `target`, its highlight) [documented: list_box.xml].
    put(ui, QUESTS_LIST, "_enabled", on(true));
    let wanted: Vec<Wanted<'_, u32>> = s.rows.iter().map(|r| Wanted { key: &r.key, name: ROW }).collect();
    if !list::reconcile(ui, QUESTS_LIST, ROW_TEMPLATE, &mut w.rows, &wanted) {
        return Err(format!("{ROW_TEMPLATE} made no quest's row"));
    }
    for (row, r) in w.rows.iter().zip(&s.rows) {
        write_row(ui, row.tile, r, false);
    }
    put(ui, DATA_RECT, "visible", on(s.objectives.is_some()));
    put(ui, DATA_RECT, "_ItemType", Value::Num(if s.objectives.is_some() { ITEM_OBJECTIVES } else { 0.0 }));
    let objectives = s.objectives.as_deref().unwrap_or_default();
    let keys: Vec<usize> = (0..objectives.len()).collect();
    let wanted: Vec<Wanted<'_, usize>> = keys.iter().map(|k| Wanted { key: k, name: ROW }).collect();
    if !list::reconcile(ui, OBJECTIVES, ROW_TEMPLATE, &mut w.objectives, &wanted) {
        return Err(format!("{ROW_TEMPLATE} made no objective's row"));
    }
    for (row, r) in w.objectives.iter().zip(objectives) {
        write_row(ui, row.tile, r, true);
    }
    ui.update();
    Ok(())
}

/// The hints on the file's tiles: each tile shows its hint or none, the hint's word on `string`, its
/// key on `_PCButtonText`, lit or dim by `target` (the file's line alpha follows it) [documented:
/// text_box.xml; design: the hand-built page's rules, not the file's expressions]. The file works
/// `visible` and `string` out by expressions, which work them out again when what they read
/// changes (the tab, a list's rows): so these are put right on every pass, not written once
/// [design: as `inventory_menu::keep`].
fn keep_hints(ui: &mut Ui, hints: &[HintShown]) {
    for (name, slot) in BUTTONS {
        let Some(t) = ui.find_tile(&button_path(name)) else { continue };
        let hint = hints.iter().find(|h| h.slot == slot);
        keep(ui, t, "visible", if hint.is_some() { 1.0 } else { 0.0 });
        if let Some(h) = hint {
            keep_text(ui, t, "string", &h.word);
            keep_text(ui, t, "_PCButtonText", &h.key);
            keep(ui, t, "target", if h.dim { 0.0 } else { 1.0 });
        }
    }
}

/// [`keep`] for words.
fn keep_text(ui: &mut Ui, t: TileId, name: &str, want: &str) {
    if ui.tile(t).get(name).map(|v| v.text().to_string()).as_deref() != Some(want) {
        ui.set_trait(t, name, text(want));
    }
}

/// The five tabs made from their template under the tab line, each kept for good; whether any was
/// made.
fn ensure_tabs(ui: &mut Ui, tabs: &mut Vec<RowTile<usize>>) -> Result<bool, String> {
    if tabs.len() == 5 && tabs.iter().all(|t| ui.is_live(t.tile)) {
        return Ok(false);
    }
    let keys = [0, 1, 2, 3, 4];
    let wanted: Vec<Wanted<'_, usize>> = keys.iter().map(|key| Wanted { key, name: TAB_ROW }).collect();
    if !list::reconcile(ui, TABLINE, TAB_TEMPLATE, tabs, &wanted) {
        return Err(format!("{TAB_TEMPLATE} made no tab"));
    }
    ui.update();
    Ok(true)
}

/// What the tab line is told: as ITEMS' tabs (`inventory_menu::write_tabs`), the words the
/// hand-built page's (`Page::tabs`).
fn write_tabs(ui: &mut Ui, tabs: &[RowTile<usize>], tab: usize) {
    put(ui, TABLINE, "_ButtonCount", Value::Num(5.0));
    put(ui, TABLINE, "_CurrentTab", Value::Num(tab as f64));
    let words = Page::Data.tabs();
    for row in tabs {
        ui.write_changed(row.tile, "string", text(words[row.key]));
        if find_named(ui, row.tile, TAB_INNER).is_some() {
            ui.write_changed(row.tile, fnv_ui::input::NO_ORIGIN, Value::Num(1.0));
        }
    }
}

/// Puts a value right that the file may work out otherwise, writing only when it differs.
fn keep(ui: &mut Ui, t: TileId, name: &str, want: f64) {
    if ui.tile(t).get(name).map(Value::num) != Some(want) {
        ui.set_trait(t, name, Value::Num(want));
    }
}

/// The tabs' `_x` and the line's `_LeftLineLength`, by the tabs' own widths, as ITEMS'
/// (`inventory_menu::tab_xs`).
fn place_tabs(ui: &mut Ui, tabs: &[RowTile<usize>]) {
    let Some(line) = ui.find_tile(TABLINE) else { return };
    let widths: Vec<f64> = tabs.iter().map(|r| num(ui, r.tile, "width")).collect();
    let (gap, xs) = super::inventory_menu::tab_xs(num(ui, line, "width"), &widths);
    keep(ui, line, "_LeftLineLength", gap);
    for (row, x) in tabs.iter().zip(xs) {
        keep(ui, row.tile, "_x", x);
    }
}

/// The objectives' scroll as DATA's one right pane (`data::scroll_pane`): the rows move by exactly
/// the pane's units, `_scroll_delta` 1 and the bar's value the pane's scroll; its count how far it
/// can scroll and 1 [design]. Returns how far it can scroll.
fn write_pane(ui: &mut Ui, list_tile: TileId, heights: &[f64], first: f32) -> f64 {
    let total: f64 = heights.iter().sum();
    let most = (total - num(ui, list_tile, "height")).max(0.0);
    ui.set_trait(list_tile, "_scroll_delta", Value::Num(1.0));
    let value = f64::from(first).clamp(0.0, most);
    if let Some(bar) = list::child(ui, list_tile, list::SCROLLBAR) {
        ui.set_trait(bar, "_number_of_items", Value::Num(most.ceil() + 1.0));
        ui.set_trait(bar, "_number_of_visible_items", Value::Num(1.0));
        list::write_bar_value(ui, bar, value);
    }
    list::write_highlight(ui, list_tile, None, value);
    most
}

/// What a tile of the page stands for, by the tile or a parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataTile {
    Row(usize),
    Tab(usize),
    Button(Slot),
    /// A piece of the quests list's own scroll bar.
    ListBar(BarPiece),
    /// A piece of the objectives' scroll bar: the right pane's.
    PaneBar(BarPiece),
}

/// What `tile` stands for: a tab, a quest's row, a hint, or a piece of either list's scroll bar.
pub fn data_tile(ui: &Ui, w: &Written, tile: TileId) -> Option<DataTile> {
    let mut at = Some(tile);
    while let Some(t) = at {
        if let Some(row) = w.tabs.iter().find(|r| r.tile == t) {
            return Some(DataTile::Tab(row.key));
        }
        at = ui.tile(t).parent;
    }
    let none: &[RowTile<u32>] = &[];
    match list::hit(ui, tile, &[(QUESTS_NAME, &w.rows), (OBJECTIVES_NAME, none)], &BUTTONS) {
        list::Hit::Row(0, i) => Some(DataTile::Row(i)),
        list::Hit::Bar(0, piece) => Some(DataTile::ListBar(piece)),
        list::Hit::Bar(1, piece) => Some(DataTile::PaneBar(piece)),
        list::Hit::Target(slot) => Some(DataTile::Button(slot)),
        _ => None,
    }
}

/// What [`usable`] measured and went through.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Checked {
    pub rows: usize,
    pub rows_in_clip: usize,
    pub hit_tests: usize,
    /// The objectives' window (canvas units), from what clips their rows.
    pub pane: Option<Rect>,
    /// The first tab or lit hint that did not take the pointer, and what did: warned of, not a
    /// fallback.
    pub missed: Option<String>,
}

/// Whether what is about to be painted, `pieces`, can be used, on every final pass: every tab's
/// words among the pieces and whole; the quests' rows whose words lie whole inside the list's
/// window, at least one when there are quests, taking the pointer at their middles; a list that
/// scrolls has a window. The tabs but the one showing and the lit hints are asked the same, and
/// one that does not take the pointer is told in [`Checked::missed`]: its key does the work
/// [design]. One walk of the menu serves every hit test.
pub fn usable(ui: &Ui, menu: TileId, w: &Written, tab: usize, pieces: &[Piece]) -> Result<Checked, String> {
    let is_text = |x: &&Piece| matches!(x.look, Look::Text { .. });
    let texts: HashMap<TileId, &Piece> = pieces.iter().filter(is_text).map(|x| (x.tile, x)).collect();
    let quests = ui.find_tile(QUESTS_LIST).ok_or("the quests list is gone")?;
    let scrolls = list::child(ui, quests, list::SCROLLBAR).is_some_and(|b| num(ui, b, "_number_of_items") > 1.0);
    let mut wanted: Vec<(DataTile, (f64, f64))> = Vec::new();
    for (i, row) in w.rows.iter().enumerate() {
        let Some(piece) = list::child(ui, row.tile, ROW_TEXT).and_then(|t| texts.get(&t)) else { continue };
        if scrolls && piece.clip.is_none() {
            return Err("the quests list scrolls and has no window".into());
        }
        if let Some(middle) = shows_whole(piece) {
            wanted.push((DataTile::Row(i), middle));
        }
    }
    let rows_in_clip = wanted.len();
    if !w.rows.is_empty() && rows_in_clip == 0 {
        return Err("no quest's row shows whole".into());
    }
    let mut optional = Vec::new();
    for row in &w.tabs {
        let piece = pieces.iter().filter(is_text).find(|x| stats_menu::inside(ui, x.tile, row.tile)).ok_or_else(|| format!("tab {} is not shown", row.key))?;
        let middle = shows_whole(piece).ok_or_else(|| format!("tab {} does not fit", row.key))?;
        if row.key != tab {
            optional.push((DataTile::Tab(row.key), middle));
        }
    }
    for (name, slot) in BUTTONS {
        let Some(b) = ui.find_tile(&button_path(name)) else { continue };
        if !ui.tile(b).get("visible").is_some_and(Value::truthy) || !ui.tile(b).get("target").is_some_and(Value::truthy) {
            continue;
        }
        if let Some(middle) = pieces.iter().filter(is_text).find(|x| stats_menu::inside(ui, x.tile, b)).and_then(shows_whole) {
            optional.push((DataTile::Button(slot), middle));
        }
    }
    let found = fnv_ui::input::hit_candidates(ui, menu, &fnv_ui::input::no_measure);
    let mut missed = None;
    for &(what, at) in wanted.iter().chain(&optional) {
        let hit = fnv_ui::input::top_hit(ui, &found, at);
        if hit.and_then(|h| data_tile(ui, w, h.tile)) != Some(what) {
            let there = hit.map_or_else(|| "nothing".to_string(), |h| ui.path_of(h.tile));
            let line = format!("{what:?} does not take the pointer at ({:.0}, {:.0}): {there} does", at.0, at.1);
            match what {
                DataTile::Row(_) => return Err(line),
                _ => missed = missed.or(Some(line)),
            }
        }
    }
    let pane = if w.shown.as_ref().is_some_and(|s| s.objectives.is_some()) { Some(objectives_window(ui)?) } else { None };
    // A quest with objectives shows at least one of them whole in that window: the pieces are what
    // is painted, so a list or text hidden (its own `visible` or one above it) or placed outside the
    // window shows none, and the pane would be blank.
    if let Some(window) = pane.filter(|_| !w.objectives.is_empty()) {
        let inside = |p: &Piece| {
            let r = p.rect;
            f64::from(window.min.x) - 1.0 <= r.left && f64::from(window.min.y) - 1.0 <= r.top && r.left + r.width <= f64::from(window.max.x) + 1.0 && r.top + r.height <= f64::from(window.max.y) + 1.0
        };
        let shows = w.objectives.iter().filter_map(|r| list::child(ui, r.tile, ROW_TEXT)).filter_map(|t| texts.get(&t)).any(|p| shows_whole(p).is_some() && inside(p));
        if !shows {
            return Err("no objective shows in its window".into());
        }
    }
    Ok(Checked { rows: w.rows.len(), rows_in_clip, hit_tests: wanted.len() + optional.len(), pane, missed })
}

/// The objectives' window as laid out (canvas units): what the list clips its children to, read
/// off its scroll bar, whose place is the list's and which is there with or without rows
/// [documented: list_box.xml]; why it cannot serve otherwise: no clip (a list without its window
/// would show rows scrolled out of it), no size, or not wholly on the canvas. The wheel over it and
/// a page of it are its own ([`DataState::publish_drawn`]).
fn objectives_window(ui: &Ui) -> Result<Rect, String> {
    let bar = ui.find_tile(OBJECTIVES).and_then(|l| list::child(ui, l, list::SCROLLBAR)).ok_or("the objectives' scroll bar is gone")?;
    let item = super::layout::layout(ui, bar, &|_, _, _| (0.0, 0.0), true).into_iter().find(|i| i.tile == bar).ok_or("the objectives' scroll bar is not laid out")?;
    let c = item.clip.ok_or("the objectives have no window")?;
    let canvas = (f64::from(crate::pipboy::CANVAS.x), f64::from(crate::pipboy::CANVAS.y));
    if c.width <= 0.0 || c.height <= 0.0 || c.left < 0.0 || c.top < 0.0 || c.left + c.width > canvas.0 || c.top + c.height > canvas.1 {
        return Err("the objectives' window does not fit".into());
    }
    Ok(Rect::new(c.left as f32, c.top as f32, (c.left + c.width) as f32, (c.top + c.height) as f32))
}

/// What [`page`] keeps: the rows, what was last written, and the reasons warned of.
#[derive(Default)]
pub struct Written {
    menu: Option<TileId>,
    /// The tiles [`built`] looked at, while the file is known to have them.
    checked: Vec<TileId>,
    tabs: Vec<RowTile<usize>>,
    /// The quests' rows, by FormID, and the objectives', by place.
    rows: Vec<RowTile<u32>>,
    objectives: Vec<RowTile<usize>>,
    /// The quests list's own scroll.
    view: ListView<u32>,
    sizes: SizesWritten,
    shown: Option<Shown>,
    /// What the last pass was asked to show, kept through a failure ([`Written::waits`]).
    asked: Option<Shown>,
    flat: Option<FlatChanges>,
    /// How far the objectives can scroll, as last measured.
    pane_most: f64,
    /// What is shown changed since the log last said so.
    tell: bool,
    counted: Checked,
    places_told: Option<String>,
    /// Final passes in a row that failed, the tab and frame the last was for, the tree it met, and
    /// its reason.
    failing: u32,
    tried: Option<(usize, bool)>,
    tried_tree: Option<TreeMark>,
    reason: String,
    fallbacks: HashSet<String>,
    /// The last failure was the layout's ([`usable`]): places and traits a script or the file can
    /// put right without changing the tree's shape, so it is tried again now and then
    /// ([`LAYOUT_RETRY_PASSES`]); a missing tile or template waits for the tree to change.
    layout_failure: bool,
    /// Passes waited since the last try of a layout failure.
    waited: u32,
}

/// The decision every reader shares, and what the page keeps.
#[derive(Resource, Default)]
pub struct MapTiles {
    /// The menu, while the page is ready to display this frame ([`settle`]).
    pub ready: Option<TileId>,
    /// The menu painted by the last drawing (`super::draw`): what the player sees.
    pub shown: Option<TileId>,
    /// The pieces [`usable`] checked this frame, for the drawing to paint.
    pub(crate) pieces: Vec<Piece>,
    /// [`CUT_AID`] and its time.
    pub cut: Option<String>,
    pub cut_until: Option<f32>,
    /// A menu over the page when this frame began (`stats_menu::modal_over`), while it is drawn
    /// from its tiles: the shell's keys wait (`crate::pipboy::takes_keys`).
    pub modal: bool,
    /// A menu over the DATA page, on any tab, when this frame began: set before the scripts, so it
    /// holds through the frame that menu closes in. The page's own input waits
    /// (`crate::pipboy::data::prepare`), with the switch on only.
    pub over_data: bool,
    /// Another menu took this frame's tile input (`super::input::feed`).
    pub others: bool,
    /// The quests list's marker held, and the objectives' (the right pane's, by the file's bar).
    drag: Option<list::Drag>,
    pane_drag: Option<list::Drag>,
    /// The page's draw state last logged: the tab, and drawn from tiles or why not.
    told: Option<(usize, Result<(), String>)>,
    written: Written,
}

/// The tiles of [`required_paths`], or why the page cannot be drawn.
fn check_file(ui: &Ui, cut: Option<&str>) -> Result<Vec<TileId>, String> {
    match built(ui, cut) {
        Tiled::No(reason) => Err(reason),
        Tiled::Yes => Ok(required_paths().iter().filter_map(|p| ui.find_tile(p)).collect()),
    }
}

/// One pass over the page; the menu when all of it went through. `new`: what the model shows, when
/// it may have changed. `late`: the pass before the drawing, which measures and checks.
pub(crate) fn page(ui: &mut Ui, new: Option<Shown>, late: bool, tiles: &mut MapTiles, fonts: &mut dyn PageFonts) -> Result<TileId, String> {
    let menu = ui.menu(MAP_MENU).ok_or_else(|| format!("{MAP_MENU} is not in the tree"))?;
    // A new menu tile (the tree built again): its rows, sizes and writes are all to do again.
    if tiles.written.menu != Some(menu) {
        let fallbacks = std::mem::take(&mut tiles.written.fallbacks);
        tiles.written = Written { menu: Some(menu), fallbacks, ..Default::default() };
        tiles.drag = None;
    }
    let w = &mut tiles.written;
    w.layout_failure = false;
    if w.checked.is_empty() || !w.checked.iter().all(|&t| ui.is_live(t)) {
        w.checked = check_file(ui, tiles.cut.as_deref())?;
    }
    let made = ensure_tabs(ui, &mut w.tabs)?;
    let mut changed = false;
    if let Some(s) = new {
        let rows_dead = !w.rows.iter().all(|r| ui.is_live(r.tile)) || !w.objectives.iter().all(|r| ui.is_live(r.tile));
        if rows_dead || w.shown.as_ref() != Some(&s) {
            write_model(ui, &s, w)?;
            w.shown = Some(s);
            w.tell = true;
            changed = true;
        }
    }
    let Some(s) = w.shown.clone() else { return Err("nothing to show yet".into()) };
    let quests = ui.find_tile(QUESTS_LIST).ok_or("the quests list is gone")?;
    let objectives = ui.find_tile(OBJECTIVES).ok_or("the objectives list is gone")?;
    if changed || made || late {
        write_tabs(ui, &w.tabs, s.tab);
        ui.update();
        // After what the hints' expressions read, before their words are measured.
        keep_hints(ui, &s.hints);
        let texts = menus::texts(ui, menu);
        ui.update();
        for _ in 0..3 {
            if fonts.write_sizes(ui, &texts, &mut w.sizes) == 0 {
                break;
            }
            ui.update();
        }
        place_tabs(ui, &w.tabs);
        list::settle_list(ui, quests, &w.rows, ROW_TEXT, &mut w.view, s.selected, s.tab as u32, s.pointer_pick, s.navigated);
        let heights = list::measure_rows(ui, &w.objectives, ROW_TEXT);
        w.pane_most = write_pane(ui, objectives, &heights, s.pane_first);
        let title = ui.find_tile(TITLE).ok_or("the title is gone")?;
        let line = ui.find_tile(LINE).ok_or("the heading's line is gone")?;
        stats_menu::write_frame_of(ui, title, line, s.device, &mut w.flat);
        super::item_menu::mark_menu(ui, MAP_MENU, &[QUESTS_LIST]);
    }
    keep_hints(ui, &s.hints);
    // The scrolls over the file's own count of the clicks.
    w.view.rewrite(ui, quests);
    if !s.boxed {
        list::write_highlight(ui, quests, None, w.view.value);
    }
    if let Some(bar) = list::child(ui, objectives, list::SCROLLBAR) {
        list::write_bar_value(ui, bar, f64::from(s.pane_first).clamp(0.0, w.pane_most));
    }
    ui.update();
    if late {
        let pieces = fonts.set_out(ui, menu);
        let places = tab_places(ui, &w.tabs);
        if w.places_told.as_ref() != Some(&places) {
            info!("tiles: {MAP_MENU} tabs stand: {places}");
            w.places_told = Some(places);
        }
        w.counted = match usable(ui, menu, w, s.tab, &pieces) {
            Ok(c) => c,
            Err(reason) => {
                w.layout_failure = true;
                return Err(reason);
            }
        };
        if let Some(missed) = w.counted.missed.clone() {
            if w.fallbacks.insert(told_once(&missed)) {
                warn!("{MAP_MENU}: {missed}");
            }
        }
        tiles.pieces = pieces;
        if std::mem::take(&mut w.tell) {
            info!("{}", describe(w));
        }
    }
    Ok(menu)
}

/// Where the tabs stand as the tree holds them, for the log when it changes.
fn tab_places(ui: &Ui, tabs: &[RowTile<usize>]) -> String {
    let n = |x: f64| Value::Num(x).text();
    let places: Vec<String> = tabs.iter().map(|r| format!("{} at {}, {} size {} x {}", r.key, n(num(ui, r.tile, "x")), n(num(ui, r.tile, "y")), n(num(ui, r.tile, "width")), n(num(ui, r.tile, "height")))).collect();
    places.join("; ")
}

/// The tab's name for the log, as the hand-built page's hints line names it.
fn tab_name(tab: usize) -> &'static str {
    ["local map", "world map", "quests", "misc", "radio"].get(tab).copied().unwrap_or("?")
}

/// A line for the log when what the page shows changes (smoke reads these).
fn describe(w: &Written) -> String {
    let Some(s) = w.shown.as_ref() else { return format!("tiles: {MAP_MENU} page written") };
    let selected = s.selected.and_then(|i| s.rows.get(i)).map_or("none", |r| r.text.as_str());
    let hints: Vec<String> = s.hints.iter().map(|h| format!("{:?} {} {}{}", h.slot, h.word, h.key, if h.dim { " (dim)" } else { "" })).collect();
    format!(
        "tiles: {MAP_MENU} page from tiles: tab {}, {} rows, \"{selected}\" selected, {} objectives, hints: {}, {}",
        tab_name(s.tab),
        s.rows.len(),
        s.objectives.as_ref().map_or(0, Vec::len),
        if hints.is_empty() { "none".to_string() } else { hints.join(", ") },
        if s.device { "device" } else { "screen menu" },
    )
}

/// What a warning is told once by: its line without the place and the row.
fn told_once(line: &str) -> String {
    line.split(" at (").next().unwrap_or(line).chars().filter(|c| !c.is_ascii_digit()).collect()
}

/// Whether the page is eligible to prepare: the switch on and DATA showing on a tab it draws.
pub fn eligible(switch: TileMenus, pip: &PipBoy) -> bool {
    switch.map && pip.shown() && pip.page == Page::Data && TILED_TABS.contains(&pip.tab())
}

/// The keys the hand-built page acts on itself (`crate::pipboy::data`'s keys, `crate::pipboy::keys`),
/// by the tile input's names, and the controller's buttons standing in for them: the file's root
/// binds X, Y and R to its hint tiles [documented], which would act twice.
pub const HAND_BUILT_KEYS: [&str; 19] = ["W", "S", "A", "D", "E", "R", "X", "xup", "xdown", "xleft", "xright", "xbuttona", "xbuttonb", "xbuttonx", "xbuttony", "xbuttonlb", "xbuttonrb", "xbuttonlt", "xbuttonrt"];

/// Whether a click a key made on the page's tiles acts: as `menus::key_click_acts`, and never for a
/// controller's button the hand-built page answers to.
fn key_acts(key: &str, physical: &[&str]) -> bool {
    menus::key_click_acts(MAP_MENU, key, physical) && !(key.to_ascii_lowercase().starts_with("xbutton") && menus::hand_built_takes(MAP_MENU, key))
}

/// The menu, when the page takes this frame's tile input: painted last frame, and the page up with
/// nothing over it. Every reader of the frame's input decides by this.
pub fn takes(tiles: &MapTiles, switch: TileMenus, pip: &PipBoy, game: &fnv_game::Game<'_>, paused: bool) -> Option<TileId> {
    if !pip.open() || !eligible(switch, pip) || paused || modal_over(game.menu) {
        return None;
    }
    tiles.shown.filter(|&m| game.ui_files.is_some() && game.ui_tree.is_live(m))
}

/// What a pass over the page reads.
#[derive(SystemParam)]
pub(crate) struct MapPage<'w> {
    script: Option<ResMut<'w, ScriptState>>,
    switch: Option<Res<'w, TileMenus>>,
    pip: Option<Res<'w, PipBoy>>,
    data: Option<ResMut<'w, DataState>>,
    tiles: ResMut<'w, MapTiles>,
    fonts: super::draw::FontKit<'w>,
    time: crate::pause::AidTime<'w>,
}

/// A pass; `None` when the page is not eligible or has nothing to show yet (nothing is touched).
fn pass(p: &mut MapPage<'_>, late: bool) -> Option<Result<TileId, String>> {
    let switch = p.switch.as_deref().copied().unwrap_or_default();
    let pip = p.pip.as_deref()?;
    if !eligible(switch, pip) {
        return None;
    }
    let (tab, device) = (pip.tab(), pip.frame == Frame::Device);
    let script = p.script.as_mut()?.bypass_change_detection();
    script.rt.as_ref()?.game.ui_files.as_ref()?;
    let data = p.data.as_mut()?;
    // What the preparation gathered this frame for this tab (`data::prepare` ran before this).
    let new = match data.prepared().filter(|pr| pr.view.tab == tab) {
        Some(pr) => match Shown::of(pr, device) {
            Ok(s) => Some(s),
            Err(reason) => return Some(Err(reason)),
        },
        None => None,
    };
    if new.is_none() && p.tiles.written.shown.is_none() {
        return None;
    }
    let notches = data.take_list_notches();
    if notches != 0.0 {
        let v = p.tiles.written.view.value;
        p.tiles.written.view.scroll_to(v - f64::from(notches).round());
    }
    let tree = TreeMark::of(&script.rt.as_ref()?.game.ui_tree);
    // What it would show changed since the last pass, written or not (a failure forgets what was
    // written, not what was asked for).
    let changed = new.is_some() && new != p.tiles.written.asked;
    if new.is_some() {
        p.tiles.written.asked.clone_from(&new);
    }
    if p.tiles.written.waits(tab, device, tree, changed) {
        return Some(Err(p.tiles.written.reason.clone()));
    }
    let rt = script.rt.as_mut()?;
    let out = page(&mut rt.game.ui_tree, new, late, &mut p.tiles, &mut p.fonts);
    if late && out.is_ok() {
        // The one presentation this frame: what the page's rules read of the drawing.
        let c = &p.tiles.written.counted;
        data.publish_drawn(tab, c.rows_in_clip, p.tiles.written.pane_most as f32, c.pane);
    }
    Some(out)
}

/// Before the scripts, after the tile input: the scrolls written again over the file's own count of
/// the clicks.
pub(crate) fn write(mut p: MapPage) {
    // A failure here is told by `settle`, which decides.
    let _ = pass(&mut p, false);
}

/// Before the drawing, after the page's preparation: the tree worked out, and the one decision
/// every reader of this frame shares.
pub(crate) fn settle(mut p: MapPage) {
    if cut_over(p.switch.as_deref().copied().unwrap_or_default(), p.tiles.cut_until, p.time.elapsed_secs()) {
        info!("test aid: {CUT_AID} over");
        p.tiles.cut = None;
        p.tiles.cut_until = None;
        p.tiles.written.checked.clear();
        if let Some(game) = p.script.as_mut().and_then(|s| s.bypass_change_detection().rt.as_mut()).map(|rt| &mut rt.game) {
            rebuild_menu(game);
        }
    }
    let switch = p.switch.as_deref().copied().unwrap_or_default();
    let on_data = switch.map && p.pip.as_deref().is_some_and(|pip| pip.shown() && pip.page == Page::Data);
    let tab = p.pip.as_deref().map_or(0, PipBoy::tab);
    let (ready, state) = match pass(&mut p, true) {
        Some(Ok(menu)) => {
            p.tiles.written.failing = 0;
            (Some(menu), Some(Ok(())))
        }
        Some(Err(reason)) => {
            let tried = p.pip.as_deref().map(|pip| (pip.tab(), pip.frame == Frame::Device));
            let tree = p.script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| TreeMark::of(&rt.game.ui_tree));
            let w = &mut p.tiles.written;
            w.failed(tried, tree);
            let line = format!("{MAP_MENU} drawn by hand: {reason}");
            if w.failing >= FAILING_PASSES && w.fallbacks.insert(told_once(&line)) {
                warn!("{line}");
            }
            let state = (w.failing >= FAILING_PASSES).then(|| Err(reason.clone()));
            w.reason = reason;
            (None, state)
        }
        None => {
            p.tiles.written.failing = 0;
            let state = (on_data && !TILED_TABS.contains(&tab)).then(|| Err("not drawn from the file yet".to_string()));
            (None, state)
        }
    };
    // One line a change of the page's draw state, on DATA (smoke reads these).
    if let Some(state) = state.filter(|_| on_data) {
        let now = Some((tab, state.clone()));
        if p.tiles.told != now {
            match &state {
                Ok(()) => info!("tiles: {MAP_MENU} draws {} from tiles", tab_name(tab)),
                Err(reason) => info!("tiles: {MAP_MENU} leaves {} hand-built: {reason}", tab_name(tab)),
            }
            p.tiles.told = now;
        }
    } else if !on_data {
        p.tiles.told = None;
    }
    if ready.is_none() {
        // The next time it is written whole; the rows stay.
        p.tiles.written.shown = None;
        p.tiles.pieces.clear();
    }
    if p.tiles.ready != ready {
        p.tiles.ready = ready;
    }
}

/// Whether [`CUT_AID`]'s time is over now: only with the switch on, as nothing else of the page runs
/// off (its end builds the menu again).
fn cut_over(switch: TileMenus, until: Option<f32>, now: f32) -> bool {
    switch.map && until.is_some_and(|until| now >= until)
}

/// Final passes in a row that fail before the reason is warned of, and before the page waits for
/// a change to try again.
const FAILING_PASSES: u32 = 3;
/// Passes (two a frame) a layout's failure waits before it is tried again with nothing changed:
/// about half a second at 60 frames a second, one measured pass then [design].
const LAYOUT_RETRY_PASSES: u32 = 60;

/// What the tree was at a failed pass: the menu's tile and the tree's shape (`Ui::shape`) [design].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TreeMark {
    menu: Option<TileId>,
    shape: u64,
}

impl TreeMark {
    fn of(ui: &Ui) -> TreeMark {
        TreeMark { menu: ui.menu(MAP_MENU), shape: ui.shape() }
    }
}

impl Written {
    /// A final pass failed, for this tab and frame, in this tree.
    fn failed(&mut self, tried: Option<(usize, bool)>, tree: Option<TreeMark>) {
        self.failing += 1;
        self.tried = tried;
        self.tried_tree = tree;
    }

    /// The page has fallen back and waits: nothing it would show changed, the tab and frame are the
    /// failure's, and the tree is as that failure met it. A layout's failure is tried again every
    /// [`LAYOUT_RETRY_PASSES`] all the same: what it met (a place, `visible`, `target`) can change
    /// without the tree's shape changing.
    fn waits(&mut self, tab: usize, device: bool, tree: TreeMark, changed: bool) -> bool {
        let same = self.failing >= FAILING_PASSES && !changed && self.tried == Some((tab, device)) && self.tried_tree == Some(tree);
        if !same {
            self.waited = 0;
            return false;
        }
        if self.layout_failure {
            self.waited += 1;
            if self.waited >= LAYOUT_RETRY_PASSES {
                self.waited = 0;
                return false;
            }
        }
        true
    }
}

/// [`CUT_AID`]'s end: the menu built again from its file, as at load.
fn rebuild_menu(game: &mut fnv_game::Game<'_>) {
    let (Some(files), Some(path)) = (game.ui_files.as_deref(), game.ui_menus.path(MAP_MENU)) else { return };
    match game.ui_tree.load_menu(files, path) {
        Ok(_) => {
            game.ui_tree.update();
            info!("test aid: {MAP_MENU} built again from {path}");
        }
        Err(e) => warn!("test aid: {MAP_MENU} not built again: {e}"),
    }
}

/// What [`feed`] keeps between frames.
#[derive(Default)]
pub(crate) struct Feed {
    /// The page took input last frame, in this frame of the Pip-Boy.
    taking: Option<Frame>,
}

/// What the page's tile input reads of the Pip-Boy.
#[derive(SystemParam)]
pub(crate) struct FeedPip<'w> {
    pip: Res<'w, PipBoy>,
    screen: Option<Res<'w, PipBoyScreen>>,
    aid: Res<'w, PipBoyAid>,
    paused: Option<Res<'w, crate::pause::Paused>>,
    bindings: Option<Res<'w, crate::bindings::Bindings>>,
}

/// What the page's tile input sends: where the hand-built page sends the same.
#[derive(SystemParam)]
pub(crate) struct FeedOut<'w> {
    tabs: MessageWriter<'w, PipBoyInput>,
    hovered: MessageWriter<'w, TabHovered>,
    data: Option<ResMut<'w, DataState>>,
}

/// The right pane's bar piece for a piece of the file's scroll bar: an arrow a wheel notch, a page
/// area a page of the window as measured (`data::bar_act`); the marker is held by the file's own
/// bar instead ([`pane_grab`]).
fn pane_part(piece: BarPiece) -> Option<BarPart> {
    Some(match piece {
        BarPiece::Up => BarPart::Up,
        BarPiece::Down => BarPart::Down,
        BarPiece::PageUp => BarPart::PageUp,
        BarPiece::PageDown => BarPart::PageDown,
        BarPiece::Marker => return None,
    })
}

/// The page's tile input, on a frame it takes it ([`takes`]): see the module notes.
pub(crate) fn feed(mut script: ResMut<ScriptState>, switch: Res<TileMenus>, from: FeedPip, mut tiles: ResMut<MapTiles>, devices: Devices, mut out: FeedOut, mut state: Local<Feed>) {
    if !switch.map {
        return;
    }
    let FeedPip { pip, screen, aid, paused, bindings } = from;
    let Some(rt) = script.rt.as_mut() else { return };
    let game = &mut rt.game;
    let over_data = pip.open() && pip.page == Page::Data && modal_over(game.menu);
    if tiles.over_data != over_data {
        tiles.over_data = over_data;
    }
    let modal = over_data && eligible(*switch, &pip);
    if tiles.modal != modal {
        tiles.modal = modal;
    }
    let is_paused = paused.as_ref().is_some_and(|p| p.0);
    let menu = takes(&tiles, *switch, &pip, game, is_paused).filter(|_| !tiles.others);
    let (Some(menu), Some(data)) = (menu, out.data.as_mut()) else {
        *state = Feed::default();
        tiles.drag = None;
        tiles.pane_drag = None;
        return;
    };
    // With Controller chosen the hand-built page ignores the mouse pointer; so does this one.
    let controller = bindings.as_ref().is_some_and(|b| b.device == crate::bindings::InputDevice::Controller);
    let pointer = screen.as_ref().and_then(|s| s.pointer).filter(|_| !controller);
    let at = pointer.map(|p| (f64::from(p.x), f64::from(p.y)));
    if state.taking != Some(pip.frame) {
        *state = Feed { taking: Some(pip.frame) };
    }
    let aid_press = aid.click == 1 && !aid.right;
    let held = aid_press || devices.mouse.pressed(MouseButton::Left);
    let mut input = fnv_ui::input::Input { at, press: aid_press || devices.mouse.just_pressed(MouseButton::Left), ..Default::default() };
    let pad_menu = crate::pad::MenuAt { code: Some(fnv_game::game::menus::MAP), tab: pip.tab() };
    let physical = frame_keys(&mut input, &devices, game, pad_menu);
    let report = fnv_game::ui_input::step(game, &[menu], &input, &fnv_ui::input::no_measure);
    let ui = &game.ui_tree;
    log_report(ui, &report);
    let tab = pip.tab();
    // What each click and the pointer's move stand for, read before anything changes.
    let w = &tiles.written;
    let clicks: Vec<(DataTile, bool)> = report
        .clicks
        .iter()
        .filter(|c| ui.is_live(c.tile))
        .filter(|c| !matches!(&c.by, fnv_ui::input::By::Key(k) if !key_acts(k, &physical)))
        .filter_map(|c| data_tile(ui, w, c.tile).map(|t| (t, matches!(c.by, fnv_ui::input::By::Pointer))))
        .collect();
    let over = report.over.map(|(from, to)| (from.and_then(|t| data_tile(ui, w, t)), to.and_then(|t| data_tile(ui, w, t))));
    let hints: Vec<(Slot, ButtonKind)> = w.shown.as_ref().map(|s| s.hints.iter().map(|h| (h.slot, h.kind)).collect()).unwrap_or_default();
    if !held {
        tiles.drag = None;
        tiles.pane_drag = None;
    }
    let mut row_acted = false;
    let mut pressed_marker = false;
    for (tile, by_pointer) in clicks {
        info!("tiles: {MAP_MENU} {tile:?} on its tiles");
        match tile {
            DataTile::Row(i) => {
                let Some(act) = row_act(&tiles.written, tab, i, RowAct::Use).filter(|_| !row_acted) else { continue };
                row_acted = true;
                data.push_act(act);
            }
            DataTile::Tab(i) => {
                out.tabs.write(PipBoyInput::Tab(i));
            }
            DataTile::Button(slot) => {
                if let Some((_, kind)) = hints.iter().find(|(s, _)| *s == slot) {
                    data.push_act(Act::Button(*kind));
                }
            }
            DataTile::ListBar(BarPiece::Marker) => {
                pressed_marker = true;
                let list_tile = ui.find_tile(QUESTS_LIST);
                tiles.drag = at.filter(|_| held && by_pointer).zip(list_tile).and_then(|((_, y), l)| list::marker_grab(ui, menu, l, y));
            }
            DataTile::ListBar(piece) => {
                let value = tiles.written.view.value;
                if let Some(v) = ui.find_tile(QUESTS_LIST).and_then(|l| list::after_piece(ui, l, piece, value)) {
                    tiles.written.view.scroll_to(v);
                }
            }
            DataTile::PaneBar(BarPiece::Marker) => {
                pressed_marker = true;
                tiles.pane_drag = at.filter(|_| held && by_pointer).and_then(|(_, y)| pane_grab(ui, menu, y));
            }
            DataTile::PaneBar(piece) => {
                if let Some(part) = pane_part(piece) {
                    data.push_act(Act::Bar(part));
                }
            }
        }
    }
    if let (false, Some((_, y))) = (pressed_marker, at) {
        if let Some(v) = tiles.drag.and_then(|d| list::value_under(ui, d, y)) {
            tiles.written.view.scroll_to(v);
        }
        if let Some(v) = tiles.pane_drag.and_then(|d| list::value_under(ui, d, y)) {
            data.push_act(Act::PaneTo(v as f32));
        }
    }
    if let Some((was, is)) = over {
        let w = &tiles.written;
        // Off the selected row: no longer boxed, as the hand-built row's pointer leaving.
        if let Some(DataTile::Row(j)) = was.filter(|_| was != is) {
            if let Some(act) = row_act(w, tab, j, RowAct::Leave) {
                data.push_act(act);
            }
        }
        match is {
            Some(DataTile::Tab(i)) => {
                out.hovered.write(TabHovered(i));
            }
            // The row the pointer comes onto is selected and boxed; not one a file's own key
            // binding moved the highlight onto.
            Some(DataTile::Row(i)) if !report.over_by_key && !row_acted => {
                if let Some(act) = row_act(w, tab, i, RowAct::Hover) {
                    data.push_act(act);
                }
            }
            _ => {}
        }
    }
}

/// What the pointer did on a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowAct {
    Hover,
    Leave,
    Use,
}

/// The act for the row tile `i` of the list as written: by the record it stands for (its quest's
/// FormID), which the page finds in its list as that is when it takes the act
/// (`data::by_record`), never by the row's place: a script may change the list in between.
fn row_act(w: &Written, tab: usize, i: usize, what: RowAct) -> Option<Act> {
    let key = w.rows.get(i)?.key;
    Some(match what {
        RowAct::Hover => Act::HoverRecord(tab, key),
        RowAct::Leave => Act::LeaveRecord(tab, key),
        RowAct::Use => Act::UseRecord(tab, key),
    })
}

/// A press at height `y` on the objectives' scroll bar's marker, held to drag it by the file's own
/// bar: its marker's place over its `_step_height` is the pane's scroll (the bar's value is the
/// pane's, [`write_pane`]) [documented: scrollbar_vert.xml's `_drag_value`].
fn pane_grab(ui: &Ui, menu: TileId, y: f64) -> Option<list::Drag> {
    let l = ui.find_tile(OBJECTIVES)?;
    list::marker_grab_on(ui, menu, l, list::child(ui, l, list::SCROLLBAR)?, y)
}

/// The resource, with the test aid read.
pub(crate) fn setup(app: &mut App) {
    let (cut, cut_until) = std::env::var(CUT_AID).ok().map_or((None, None), |v| {
        let (name, until) = stats_menu::parse_cut(&v);
        (Some(name), until)
    });
    app.insert_resource(MapTiles { cut, cut_until, ..Default::default() });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipboy::frame::Hint;
    use fnv_ui::MemSource;

    /// [design] A small layout written for these tests, with its own structure, numbers and
    /// expressions; it does not come from the game's files and says nothing about them. It keeps
    /// the tile, template and path names the model and the input look up, which are the engine's
    /// interface ([`required_paths`]), and only the dependencies the tests exercise: a row stands at
    /// its `_y` less its list's scroll, its text copies the row's `string`, a tab stands at its `_x`
    /// with its `string` in a text under it, a hint's text copies the hint's `string`. So these
    /// tests do not cover the file's own places or how its expressions read what is written
    /// (`_CurrentTab` and the lists' `visible`, the lists' `_enabled` and the rows' `target`,
    /// `_selected`, `_ShowEmptyMarker` and `_TextAlpha` in the row template, `_ItemType`, the hints'
    /// `_y` and dim look): the smoke scenarios `tiles-data*` check those on the real file.
    fn fixture_xml(change: impl FnOnce(String) -> String) -> String {
        let bar = r#"<image name="lb_scrollbar"><_step_size>1</_step_size><_jump_size>3</_jump_size><_current_value>0</_current_value>
              <hotrect name="scrollbar_vert_page_up"/><hotrect name="scrollbar_vert_page_down"/>
              <image name="scrollbar_vert_marker"/><image name="scrollbar_vert_up"/><image name="scrollbar_vert_down"/></image>"#;
        // The objectives' bar: its marker 16 down the bar, a value every 2 units of its drag.
        let pane_bar = bar.replace(r#"<image name="scrollbar_vert_marker"/>"#, r#"<image name="scrollbar_vert_marker"><y>16</y><width>20</width><height>10</height><_step_height>2</_step_height></image>"#);
        let offset = r#"<_offset><copy src="child(lb_scrollbar)" trait="_current_value"/><mul src="me()" trait="_scroll_delta"/></_offset>"#;
        let button = |name: &str, y: u32| format!(r#"<image name="{name}"><y>{y}</y><width>150</width><height>30</height><target>&true;</target><text name="button_text"><string><copy src="parent()" trait="string"/></string></text></image>"#);
        let buttons: String = [("MM_ButtonA", 0), ("MM_ButtonX", 40), ("MM_ButtonY", 80)].iter().map(|(n, y)| button(n, *y)).collect();
        let xml = format!(
            r#"<menu name="MapMenu"><rect name="NOGLOW_BRANCH"/><rect name="GLOW_BRANCH">
            <image name="MM_Background"/>
            <rect name="MM_MainRect"><x>40</x><y>40</y>
              <rect name="MM_HeadlineRect">
                <text name="MM_Headline_Title"><string>Words</string></text>
                <rect name="MM_Headline_h2"><x>200</x><_length>300</_length></rect>
                <text name="MM_Headline_LocationInfo"><x>500</x></text>
                <text name="MM_Headline_TimeDateInfo"><x>700</x></text>
              </rect>
              <hotrect name="MM_QuestsList"><y>60</y><width>380</width><height>240</height><clipwindow>&true;</clipwindow>
                <_scroll_delta>12</_scroll_delta>{offset}
                <hotrect name="lb_highlight_box"/>{bar}
              </hotrect>
              <text name="MM_Headline_ChallengeType"></text>
              <rect name="MM_ButtonRect"><x>600</x><y>60</y>{buttons}</rect>
              <rect name="MM_DataRect"><x>450</x><y>200</y><visible>&false;</visible>
                <hotrect name="MM_QuestObjectivesList"><width>380</width><height>200</height><clipwindow>&true;</clipwindow>
                  <_scroll_delta>9</_scroll_delta>{offset}
                  <hotrect name="lb_highlight_box"/>{pane_bar}
                </hotrect>
              </rect>
            </rect>
            <rect name="MM_Tabline"><x>40</x><y>650</y><width>900</width></rect>
            </rect>
            <template name="MM_ListMarkerTemplate"><hotrect name="MM_ListItem"><target>&true;</target><x>25</x><width>330</width>
              <_y>0</_y><_VerticalSpacing>10</_VerticalSpacing>
              <y><copy src="me()" trait="_y"/><sub src="parent()" trait="_offset"/></y>
              <text name="ListItemText"><string><copy src="parent()" trait="string"/></string></text>
              <image name="MM_Template_ItemMarker"/>
            </hotrect></template>
            <template name="TabButtonTemplate"><hotrect name="TabButton"><target>&true;</target><width>100</width><height>30</height><_x>0</_x>
              <x><copy src="me()" trait="_x"/></x>
              <text name="button_text"><string><copy src="parent()" trait="string"/></string></text>
            </hotrect></template></menu>"#
        );
        change(xml)
    }

    fn tree_changed(change: impl FnOnce(String) -> String) -> Ui {
        let mut ui = Ui::new();
        ui.load_menu(&MemSource::default().with("m.xml", &fixture_xml(change)), "m.xml").unwrap();
        ui.update();
        ui
    }

    fn tree() -> Ui {
        tree_changed(|x| x)
    }

    /// A measure of 10 a character and 30 a line (`|` breaks one).
    fn measure(_: f64, text: &str, _: Option<f64>) -> (f64, f64) {
        (10.0 * text.chars().count() as f64, 30.0 * text.split('|').count() as f64)
    }

    struct TestFonts;

    impl PageFonts for TestFonts {
        fn write_sizes(&mut self, ui: &mut Ui, texts: &[TileId], written: &mut SizesWritten) -> usize {
            menus::write_text_sizes_of(ui, texts, written, &measure)
        }
        fn set_out(&mut self, ui: &Ui, menu: TileId) -> Vec<Piece> {
            super::super::draw::pieces(ui, menu, &measure, super::super::draw::Select::Whole { kept: &[], strip: None }, true).0
        }
    }

    /// Each row's height in the fixture: a line's 30 and its own spacing of 10.
    const ROW_H: f64 = 40.0;

    fn quest(i: usize, filled: bool, done: bool) -> RowShown {
        RowShown { key: 100 + i as u32, text: format!("Quest {i}"), filled, done }
    }

    fn x_hint() -> HintShown {
        HintShown { slot: Slot::X, kind: ButtonKind::ShowLocation, word: "Look There".into(), key: "X)".into(), dim: false }
    }

    /// Quests `n` of them, the first active and the last done, `sel` selected and boxed, with `objectives`
    /// objectives (the second done), and the keyboard's one hint.
    fn shown(n: usize, sel: usize, objectives: usize) -> Shown {
        Shown {
            tab: data::QUESTS,
            device: true,
            place: "A Place".into(),
            when: "1.2.34, 5:06".into(),
            rows: (0..n).map(|i| quest(i, i == 0, n > 1 && i == n - 1)).collect(),
            selected: (n > 0).then_some(sel),
            boxed: true,
            pointer_pick: false,
            navigated: 0,
            objectives: (n > 0).then(|| (0..objectives).map(|i| RowShown { key: i as u32, text: format!("Step {i}"), filled: i == 1, done: i == 1 }).collect()),
            pane_first: 0.0,
            hints: vec![x_hint()],
        }
    }

    fn run(ui: &mut Ui, s: Option<Shown>, t: &mut MapTiles) -> Result<TileId, String> {
        page(ui, s, true, t, &mut TestFonts)
    }

    fn at(ui: &Ui, path: &str, name: &str) -> f64 {
        ui.get_float(&format!("{path}/{name}")).unwrap_or(-999.0)
    }

    fn words(ui: &Ui, path: &str, name: &str) -> String {
        ui.get_string(&format!("{path}/{name}")).unwrap_or_default()
    }

    fn trait_of(ui: &Ui, t: TileId, name: &str) -> f64 {
        ui.tile(t).get(name).map_or(-999.0, Value::num)
    }

    /// The middle of a tile's text piece, as painted.
    fn middle(t: &MapTiles, tile: TileId) -> (f64, f64) {
        let p = t.pieces.iter().find(|p| p.tile == tile).unwrap();
        (p.rect.left + p.rect.width / 2.0, p.rect.top + p.rect.height / 2.0)
    }

    #[test]
    fn the_model_writes_the_heading_quests_objectives_tabs_and_hints() {
        let (mut ui, mut t) = (tree(), MapTiles::default());
        let menu = run(&mut ui, Some(shown(3, 1, 2)), &mut t).unwrap();
        assert_eq!(ui.menu(MAP_MENU), Some(menu));
        assert_eq!((words(&ui, PLACE, "string"), words(&ui, TIME, "string")), ("A Place".to_string(), "1.2.34, 5:06".to_string()));
        assert_eq!((at(&ui, BACKGROUND, "visible"), at(&ui, CHALLENGE_TYPE, "visible")), (0.0, 0.0));
        assert_eq!(at(&ui, QUESTS_LIST, "_enabled"), 1.0, "the list takes the pointer");
        // The quests, in the model's order, each its row's words, square and strength.
        let rows = t.written.rows.clone();
        assert_eq!(rows.iter().map(|r| r.key).collect::<Vec<_>>(), vec![100, 101, 102]);
        let text_of = |ui: &Ui, row: TileId| list::child(ui, row, ROW_TEXT).and_then(|x| ui.tile(x).get("string").map(|v| v.text().to_string()));
        assert_eq!(text_of(&ui, rows[1].tile).as_deref(), Some("Quest 1"), "the template's text copies the row's words");
        assert_eq!((trait_of(&ui, rows[0].tile, "_selected"), trait_of(&ui, rows[1].tile, "_selected")), (1.0, 0.0), "the active quest's square");
        assert_eq!((trait_of(&ui, rows[0].tile, "_TextAlpha"), trait_of(&ui, rows[2].tile, "_TextAlpha")), (FULL_ALPHA, DONE_ALPHA), "a done quest at half strength");
        assert_eq!(trait_of(&ui, rows[0].tile, "_ShowEmptyMarker"), 0.0);
        assert_eq!(ui.tile(rows[2].tile).get("_y").map(Value::num), Some(2.0 * ROW_H));
        assert_eq!((at(&ui, QUESTS_LIST, "_highlight_y"), at(&ui, QUESTS_LIST, "_selected_height")), (ROW_H, ROW_H), "the selected row boxed");
        // The objectives at the right.
        assert_eq!((at(&ui, DATA_RECT, "visible"), at(&ui, DATA_RECT, "_ItemType")), (1.0, ITEM_OBJECTIVES));
        let steps = t.written.objectives.clone();
        assert_eq!(steps.len(), 2);
        assert_eq!(text_of(&ui, steps[0].tile).as_deref(), Some("Step 0"));
        assert_eq!((trait_of(&ui, steps[0].tile, "_ShowEmptyMarker"), trait_of(&ui, steps[1].tile, "_selected"), trait_of(&ui, steps[1].tile, "_TextAlpha")), (1.0, 1.0, DONE_ALPHA));
        // The tabs: five, the hand-built page's words, the tab showing told.
        assert_eq!(t.written.tabs.len(), 5);
        for (row, word) in t.written.tabs.iter().zip(Page::Data.tabs()) {
            assert_eq!(ui.tile(row.tile).get("string").map(|v| v.text().to_string()).as_deref(), Some(word));
        }
        assert_eq!((at(&ui, TABLINE, "_ButtonCount"), at(&ui, TABLINE, "_CurrentTab")), (5.0, data::QUESTS as f64));
        // The keyboard's hints: X alone, its word, key and light.
        assert_eq!((at(&ui, &button_path("MM_ButtonA"), "visible"), at(&ui, &button_path("MM_ButtonY"), "visible")), (0.0, 0.0));
        let x = button_path("MM_ButtonX");
        assert_eq!((at(&ui, &x, "visible"), at(&ui, &x, "target")), (1.0, 1.0));
        assert_eq!((words(&ui, &x, "string"), words(&ui, &x, "_PCButtonText")), ("Look There".to_string(), "X)".to_string()));
        // Measured and checked: three rows whole, the objectives' window from what clips them.
        assert_eq!((t.written.counted.rows, t.written.counted.rows_in_clip), (3, 3));
        let pane = t.written.counted.pane.expect("the objectives' window");
        assert_eq!((pane.min.x, pane.min.y, pane.width(), pane.height()), (490.0, 240.0, 380.0, 200.0));
        assert!(describe(&t.written).contains("tab quests, 3 rows, \"Quest 1\" selected, 2 objectives, hints: X Look There X)"), "{}", describe(&t.written));
    }

    #[test]
    fn a_rows_tile_stays_its_quests_and_a_click_reads_where_it_stands_now() {
        let (mut ui, mut t) = (tree(), MapTiles::default());
        run(&mut ui, Some(shown(3, 0, 0)), &mut t).unwrap();
        let tile_of = |t: &MapTiles, key: u32| t.written.rows.iter().find(|r| r.key == key).map(|r| r.tile);
        let (first, second) = (tile_of(&t, 100).unwrap(), tile_of(&t, 101).unwrap());
        // Another order and a quest gone: the first quest's tile is kept, the gone one's unloaded.
        let mut s = shown(3, 0, 0);
        s.rows = vec![quest(2, false, false), quest(0, true, false)];
        s.selected = Some(1);
        run(&mut ui, Some(s), &mut t).unwrap();
        assert_eq!(tile_of(&t, 100), Some(first));
        assert!(!ui.is_live(second), "the gone quest's tile");
        assert_eq!(data_tile(&ui, &t.written, first), Some(DataTile::Row(1)));
        let text = list::child(&ui, first, ROW_TEXT).unwrap();
        assert_eq!(data_tile(&ui, &t.written, text), Some(DataTile::Row(1)), "inside a row is the row");
        // What a click or the pointer on that tile sends names its quest, not its place: the page
        // finds the quest in its list as that is when it takes the act (`data::by_record`).
        assert_eq!(row_act(&t.written, data::QUESTS, 1, RowAct::Use), Some(Act::UseRecord(data::QUESTS, 100)));
        assert_eq!(row_act(&t.written, data::QUESTS, 0, RowAct::Hover), Some(Act::HoverRecord(data::QUESTS, 102)));
        assert_eq!(row_act(&t.written, data::QUESTS, 1, RowAct::Leave), Some(Act::LeaveRecord(data::QUESTS, 100)));
        assert_eq!(row_act(&t.written, data::QUESTS, 2, RowAct::Use), None, "no such row");
    }

    #[test]
    fn tiles_stand_for_rows_tabs_hints_and_each_lists_bar() {
        let (mut ui, mut t) = (tree(), MapTiles::default());
        let menu = run(&mut ui, Some(shown(2, 0, 1)), &mut t).unwrap();
        let w = &t.written;
        assert_eq!(data_tile(&ui, w, w.tabs[3].tile), Some(DataTile::Tab(3)));
        let x = ui.find_tile(&button_path("MM_ButtonX")).unwrap();
        assert_eq!(data_tile(&ui, w, find_named(&ui, x, "button_text").unwrap()), Some(DataTile::Button(Slot::X)));
        let piece = |list: &str, name: &str| ui.find_tile(&format!("{list}/{}/{name}", list::SCROLLBAR)).unwrap();
        assert_eq!(data_tile(&ui, w, piece(QUESTS_LIST, list::MARKER)), Some(DataTile::ListBar(BarPiece::Marker)));
        assert_eq!(data_tile(&ui, w, piece(OBJECTIVES, list::UP)), Some(DataTile::PaneBar(BarPiece::Up)));
        assert_eq!(data_tile(&ui, w, w.objectives[0].tile), None, "the objectives take no click");
        // The pointer on a row's words and on the hint finds them, as painted.
        let row_text = list::child(&ui, w.rows[1].tile, ROW_TEXT).unwrap();
        let hit = fnv_ui::input::hit(&ui, &[menu], middle(&t, row_text), &fnv_ui::input::no_measure).unwrap();
        assert_eq!(data_tile(&ui, w, hit.tile), Some(DataTile::Row(1)));
        let x_text = find_named(&ui, x, "button_text").unwrap();
        let hit = fnv_ui::input::hit(&ui, &[menu], middle(&t, x_text), &fnv_ui::input::no_measure).unwrap();
        assert_eq!(data_tile(&ui, w, hit.tile), Some(DataTile::Button(Slot::X)));
    }

    #[test]
    fn the_hints_stand_on_their_tiles_by_what_they_do() {
        for kind in [ButtonKind::Travel, ButtonKind::MakeActive, ButtonKind::PlayAudio, ButtonKind::Tune] {
            assert_eq!(Slot::of(kind), Slot::A, "{kind:?}");
        }
        for kind in [ButtonKind::PlaceMarker, ButtonKind::ShowLocation, ButtonKind::NotesFilter, ButtonKind::NextChallenge] {
            assert_eq!(Slot::of(kind), Slot::X, "{kind:?}");
        }
        assert_eq!(Slot::of(ButtonKind::Challenges), Slot::Y);
        // A controller's set: A then X, whatever their order; a dim hint is no target.
        let (mut ui, mut t) = (tree(), MapTiles::default());
        let mut s = shown(2, 1, 0);
        s.hints = vec![x_hint(), HintShown { slot: Slot::A, kind: ButtonKind::MakeActive, word: "Pick".into(), key: "A)".into(), dim: true }];
        run(&mut ui, Some(s), &mut t).unwrap();
        let a = button_path("MM_ButtonA");
        assert_eq!((at(&ui, &a, "visible"), at(&ui, &a, "target"), words(&ui, &a, "string")), (1.0, 0.0, "Pick".to_string()));
        assert_eq!(words(&ui, &button_path("MM_ButtonX"), "string"), "Look There");
    }

    #[test]
    fn the_objectives_move_by_the_panes_own_units() {
        let (mut ui, mut t) = (tree(), MapTiles::default());
        // Seven steps of 40 in a window of 200: 80 to scroll.
        let mut s = shown(1, 0, 7);
        s.pane_first = 30.0;
        run(&mut ui, Some(s.clone()), &mut t).unwrap();
        assert_eq!(t.written.pane_most, 80.0);
        assert_eq!(at(&ui, OBJECTIVES, "_scroll_delta"), 1.0);
        let bar = format!("{OBJECTIVES}/{}", list::SCROLLBAR);
        assert_eq!((at(&ui, &bar, "_current_value"), at(&ui, &bar, "_number_of_items"), at(&ui, &bar, "_number_of_visible_items")), (30.0, 81.0, 1.0));
        let second = t.written.objectives[1].tile;
        assert_eq!(trait_of(&ui, second, "y"), ROW_H - 30.0, "the rows moved by the pane's scroll");
        // Further than it can go: held at its end.
        s.pane_first = 500.0;
        run(&mut ui, Some(s), &mut t).unwrap();
        assert_eq!(at(&ui, &bar, "_current_value"), 80.0);
        assert_eq!(pane_part(BarPiece::PageDown), Some(BarPart::PageDown));
        // The marker held: the pane's scroll is where the file's bar puts it, its marker's place
        // over its step (the fixture's 16 down and 2 a value): 30 lower is (16 + 30) / 2.
        let menu = ui.menu(MAP_MENU).unwrap();
        let marker = ui.find_tile(&format!("{bar}/{}", list::MARKER)).unwrap();
        let top = super::super::layout::layout(&ui, marker, &|_, _, _| (0.0, 0.0), true).into_iter().find(|i| i.tile == marker).unwrap().rect.top;
        let drag = pane_grab(&ui, menu, top + 3.0).expect("the marker taken");
        assert_eq!(list::value_under(&ui, drag, top + 3.0), Some(8.0), "where it stands");
        assert_eq!(list::value_under(&ui, drag, top + 33.0), Some(23.0));
        assert_eq!(list::value_under(&ui, drag, top + 500.0), Some(80.0), "held at the end");
        // A page is the window as measured: the fixture's 200.
        assert_eq!(t.written.counted.pane.map(|r| r.height()), Some(200.0));
        assert_eq!(pane_part(BarPiece::Marker), None, "the marker is the file's bar's to drag");
    }

    #[test]
    fn the_quests_list_scrolls_by_itself_and_a_key_brings_the_row_into_view() {
        let (mut ui, mut t) = (tree(), MapTiles::default());
        // Ten rows of 40 in a window of 240.
        run(&mut ui, Some(shown(10, 0, 0)), &mut t).unwrap();
        assert!(t.written.view.counts.most() > 0.0);
        assert_eq!(t.written.counted.rows_in_clip, 6);
        // A key picks the last row: it is brought into view.
        let mut s = shown(10, 9, 0);
        s.navigated = 1;
        run(&mut ui, Some(s.clone()), &mut t).unwrap();
        let most = t.written.view.counts.most();
        assert_eq!(t.written.view.value, most, "scrolled to the end");
        // The pointer's pick does not move it.
        assert!(t.written.view.scroll_to(0.0));
        s.selected = Some(8);
        s.pointer_pick = true;
        run(&mut ui, Some(s), &mut t).unwrap();
        assert_eq!(t.written.view.value, 0.0);
        assert_eq!(at(&ui, &format!("{QUESTS_LIST}/{}", list::SCROLLBAR), "_current_value"), 0.0);
    }

    #[test]
    fn an_unboxed_selection_shows_no_box_and_no_quest_shows_no_objectives() {
        let (mut ui, mut t) = (tree(), MapTiles::default());
        let mut s = shown(2, 1, 1);
        s.boxed = false;
        run(&mut ui, Some(s), &mut t).unwrap();
        assert_eq!((at(&ui, QUESTS_LIST, "_highlight_y"), at(&ui, QUESTS_LIST, "_selected_height")), (-1.0, 0.0));
        // No quest: an empty list is a page, no objectives, no hint.
        let mut s = shown(0, 0, 0);
        s.hints.clear();
        run(&mut ui, Some(s), &mut t).unwrap();
        assert!(t.written.rows.is_empty() && t.written.objectives.is_empty());
        assert_eq!(at(&ui, DATA_RECT, "visible"), 0.0);
        assert_eq!(at(&ui, &button_path("MM_ButtonX"), "visible"), 0.0);
        assert_eq!(t.written.counted.pane, None);
    }

    #[test]
    fn a_missing_menu_tile_or_template_falls_back_and_comes_back() {
        // No menu.
        let mut t = MapTiles::default();
        assert_eq!(run(&mut Ui::new(), Some(shown(1, 0, 0)), &mut t), Err(format!("{MAP_MENU} is not in the tree")));
        // A hint tile cut by the test aid, then not.
        let (mut ui, mut t) = (tree(), MapTiles { cut: Some("MM_ButtonX".into()), ..Default::default() });
        let err = run(&mut ui, Some(shown(1, 0, 0)), &mut t).unwrap_err();
        assert!(err.contains("MM_ButtonX missing"), "{err}");
        assert!(!t.written.layout_failure, "a missing tile waits for the tree");
        t.cut = None;
        t.written.checked.clear();
        assert!(run(&mut ui, Some(shown(1, 0, 0)), &mut t).is_ok(), "recovered");
        // A file without the row template, or without the objectives' list.
        let mut ui = tree_changed(|x| x.replace("MM_ListMarkerTemplate", "OtherTemplate"));
        assert_eq!(run(&mut ui, Some(shown(1, 0, 0)), &mut MapTiles::default()), Err(format!("{ROW_TEMPLATE} missing")));
        let mut ui = tree_changed(|x| x.replace("MM_QuestObjectivesList", "SomethingElse"));
        assert_eq!(run(&mut ui, Some(shown(1, 0, 0)), &mut MapTiles::default()), Err(format!("{OBJECTIVES} missing")));
        // The tree built again: everything written again in the new menu.
        let (mut ui, mut t) = (tree(), MapTiles::default());
        let old = run(&mut ui, Some(shown(2, 0, 0)), &mut t).unwrap();
        ui.load_menu(&MemSource::default().with("m.xml", &fixture_xml(|x| x)), "m.xml").unwrap();
        ui.update();
        let new = run(&mut ui, Some(shown(2, 0, 0)), &mut t).unwrap();
        assert_ne!(old, new);
        assert!(t.written.rows.iter().all(|r| ui.is_live(r.tile)) && t.written.rows.len() == 2);
    }

    #[test]
    fn an_unusable_layout_falls_back() {
        // A list that scrolls without a window.
        let mut ui = tree_changed(|x| x.replacen("<clipwindow>&true;</clipwindow>", "", 1));
        assert_eq!(run(&mut ui, Some(shown(10, 0, 0)), &mut MapTiles::default()), Err("the quests list scrolls and has no window".to_string()));
        // A list off the canvas: no row shows whole.
        let mut ui = tree_changed(|x| x.replace(r#"<hotrect name="MM_QuestsList"><y>60</y>"#, r#"<hotrect name="MM_QuestsList"><y>2000</y>"#));
        assert_eq!(run(&mut ui, Some(shown(2, 0, 0)), &mut MapTiles::default()), Err("no quest's row shows whole".to_string()));
        // Objectives with no window, or a window off the canvas: no usable pane.
        let mut ui = tree_changed(|x| x.replace(r#"<hotrect name="MM_QuestObjectivesList"><width>380</width><height>200</height><clipwindow>&true;</clipwindow>"#, r#"<hotrect name="MM_QuestObjectivesList"><width>380</width><height>200</height>"#));
        let mut t = MapTiles::default();
        assert_eq!(run(&mut ui, Some(shown(2, 0, 1)), &mut t), Err("the objectives have no window".to_string()));
        assert!(t.written.layout_failure, "a layout's failure");
        let mut ui = tree_changed(|x| x.replace(r#"<rect name="MM_DataRect"><x>450</x><y>200</y>"#, r#"<rect name="MM_DataRect"><x>450</x><y>700</y>"#));
        assert_eq!(run(&mut ui, Some(shown(2, 0, 1)), &mut MapTiles::default()), Err("the objectives' window does not fit".to_string()));
        // Objectives whose list is hidden: none shows in the window.
        let mut ui = tree_changed(|x| x.replace(r#"<hotrect name="MM_QuestObjectivesList"><width>380</width>"#, r#"<hotrect name="MM_QuestObjectivesList"><visible>&false;</visible><width>380</width>"#));
        let mut t = MapTiles::default();
        let err = run(&mut ui, Some(shown(2, 0, 2)), &mut t).unwrap_err();
        // Whether a hidden list's window is laid out at all is the layout's; either way no pane.
        assert!(err == "no objective shows in its window" || err == "the objectives have no window", "{err}");
        assert!(t.written.layout_failure);
        // Their words hidden, or placed outside the window, after a pass that went through: the
        // next final pass falls back.
        for (name, value) in [("visible", 0.0), ("x", 2000.0), ("y", -500.0)] {
            let (mut ui, mut t) = (tree(), MapTiles::default());
            assert!(run(&mut ui, Some(shown(2, 0, 2)), &mut t).is_ok());
            for row in t.written.objectives.clone() {
                let words = list::child(&ui, row.tile, ROW_TEXT).unwrap();
                ui.set_trait(words, name, Value::Num(value));
            }
            ui.update();
            assert_eq!(run(&mut ui, None, &mut t), Err("no objective shows in its window".to_string()), "{name} {value}");
        }
        // A quest with no objective shown asks for the window only.
        assert!(run(&mut tree(), Some(shown(2, 0, 0)), &mut MapTiles::default()).is_ok());
        // No quest, no objectives: no pane is asked for.
        let mut s = shown(0, 0, 0);
        s.hints.clear();
        assert!(run(&mut ui, Some(s), &mut MapTiles::default()).is_ok());
        // Nothing to show yet: no snapshot, nothing written.
        let (mut ui, mut t) = (tree(), MapTiles::default());
        assert_eq!(run(&mut ui, None, &mut t), Err("nothing to show yet".to_string()));
    }

    /// A failure waits for a change once it has lasted: a missing name for the tree to change, a
    /// layout's failure only so long, as what it met can change without the tree's shape.
    #[test]
    fn a_layout_failure_is_tried_again_now_and_then_a_missing_name_waits_for_the_tree() {
        let tree = TreeMark { menu: None, shape: 7 };
        let mut w = Written { failing: FAILING_PASSES, tried: Some((data::QUESTS, true)), tried_tree: Some(tree), ..Default::default() };
        assert!((0..500).all(|_| w.waits(data::QUESTS, true, tree, false)), "a missing name waits");
        assert!(!w.waits(data::QUESTS, true, TreeMark { shape: 8, ..tree }, false), "the tree changed");
        assert!(!w.waits(data::QUESTS, true, tree, true), "what it would show changed");
        assert!(!w.waits(data::QUESTS, false, tree, false), "the other frame");
        w.layout_failure = true;
        w.waited = 0;
        let tries: Vec<usize> = (1..=150).filter(|_| !w.waits(data::QUESTS, true, tree, false)).collect();
        assert_eq!(tries.len(), 2, "tried again every {LAYOUT_RETRY_PASSES} passes");
        assert!(!Written { failing: FAILING_PASSES - 1, ..Default::default() }.waits(data::QUESTS, true, tree, false), "not before it lasted");
    }

    #[test]
    fn the_cut_aid_ends_only_with_the_switch_on() {
        let on = TileMenus { map: true, ..Default::default() };
        assert!(cut_over(on, Some(20.0), 20.0));
        assert!(!cut_over(on, Some(20.0), 19.0));
        assert!(!cut_over(on, None, 99.0));
        assert!(!cut_over(TileMenus::default(), Some(20.0), 99.0), "off: the menu is not built again");
    }

    #[test]
    fn only_quests_with_the_switch_on_is_eligible() {
        let mut pip = PipBoy::default();
        (pip.phase, pip.page) = (crate::pipboy::Phase::Up, Page::Data);
        pip.tabs[Page::Data.index()] = data::QUESTS;
        let on = TileMenus { map: true, ..Default::default() };
        assert!(eligible(on, &pip));
        assert!(!eligible(TileMenus::default(), &pip), "off: nothing runs");
        assert!(!eligible(TileMenus { stats: true, inventory: true, ..Default::default() }, &pip));
        for tab in [data::LOCAL, data::WORLD, data::NOTES, data::RADIO] {
            pip.tabs[Page::Data.index()] = tab;
            assert!(!eligible(on, &pip), "tab {tab} keeps the hand-built page");
        }
        pip.tabs[Page::Data.index()] = data::QUESTS;
        pip.page = Page::Items;
        assert!(!eligible(on, &pip));
        pip.page = Page::Data;
        pip.phase = crate::pipboy::Phase::Down;
        assert!(!eligible(on, &pip), "not shown");
    }

    #[test]
    fn the_prepared_page_is_read_as_the_tile_page_writes_it() {
        let hint = |kind, word: &'static str, dim| data::Button { hint: Hint::new(word, "K)".into(), dim), kind };
        let row = |text: &str, mark, done| data::Row { text: text.into(), mark, done };
        let view = data::View {
            tab: data::QUESTS,
            place: "A Place".into(),
            when: "now".into(),
            buttons: vec![hint(ButtonKind::MakeActive, "Pick", true), hint(ButtonKind::ShowLocation, "Look", false)],
            body: Body::List { rows: vec![row("Q1", Mark::Filled, false), row("Q2", Mark::None, true)], first: 0, boxed: Some(1), right: Right::Objectives { rows: vec![row("S1", Mark::Empty, false)], first: 12.0 } },
            popup: None,
        };
        let p = Prepared { view: view.clone(), keys: vec![7, 9], sel: 5, pointer_pick: true, navigated: 3 };
        let s = Shown::of(&p, false).unwrap();
        assert_eq!(s.rows, vec![RowShown { key: 7, text: "Q1".into(), filled: true, done: false }, RowShown { key: 9, text: "Q2".into(), filled: false, done: true }]);
        assert_eq!((s.selected, s.boxed, s.pointer_pick, s.navigated, s.pane_first, s.device), (Some(1), true, true, 3, 12.0, false));
        assert_eq!(s.objectives.as_ref().map(|o| (o.len(), o[0].filled)), Some((1, false)));
        assert_eq!(s.hints.iter().map(|h| (h.slot, h.dim)).collect::<Vec<_>>(), vec![(Slot::A, true), (Slot::X, false)]);
        // The question box, another body or rows without their records: not drawn from the file.
        let popup = Prepared { view: data::View { popup: Some(crate::pipboy::data::travel::Popup::travel(1, "Go?".into())), ..view.clone() }, ..p.clone() };
        assert_eq!(Shown::of(&popup, true), Err("the question box is up".to_string()));
        let short = Prepared { keys: vec![7], ..p.clone() };
        assert!(Shown::of(&short, true).is_err());
        let text = Prepared { view: data::View { body: Body::List { rows: Vec::new(), first: 0, boxed: None, right: Right::Text { text: "x".into(), first: 0.0 } }, ..view }, keys: Vec::new(), ..p };
        assert_eq!(Shown::of(&text, true), Err("the right pane is not the objectives".to_string()));
    }
}
