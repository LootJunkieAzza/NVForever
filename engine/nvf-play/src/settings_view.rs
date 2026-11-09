//! What the settings page shows, as data (#1953; docs/design/settings-page.md): which rows
//! a tab (and later a search or the Changed list) lists, under which headings, which row
//! is chosen, and how the keys move over them. `crate::pause` draws it and turns the keys
//! and the pointer into these steps; nothing here draws or reads input.
//!
//! **A row** is a registered setting by its index in the registry (`crate::settings`,
//! whose order stays the one the values, the INI and the test aids go by), or one of the
//! Controls tab's own rows (`crate::bindings`: the input device, each control's key or
//! button, Invert Y, the sensitivity, Rumble), each by a name of its own ([`RowId`]), never
//! by where it stands. A heading is not a row: it takes one line of the window and the
//! scroll bar counts it, but it cannot be chosen ([`ViewEntry`]) [design].
//!
//! **What is shown** is worked out each frame from the game's state ([`Visibility`]): a
//! colour's red, green and blue only while the colour is Custom
//! (`crate::ui_colour::channel_shown`), the controller's rows of the Controls tab only with
//! the controller as the input device ([`crate::settings_layout::controller_only`]). A
//! hidden row keeps its tab and heading; a choice on it moves to the row that stands for
//! it ([`parent_of`]: the colour's list, the Input Device row) [design].
//!
//! **The keys** [design: #1953]: Up and Down step over the rows, round the ends, passing
//! the headings ([`step_selectable`]); Page Up and Page Down go to the first row under the
//! heading before or after ([`step_heading`]); a tab without headings is one section. The
//! window of [`WINDOW_ROWS`] lines keeps the chosen row in view, and its heading too when it
//! is the heading's first row ([`follow`]); a heading is never drawn alone on the window's
//! last line ([`drawn_slots`]).

use crate::bindings::{Bindings, Control, InputDevice, GAME_CONTROLS};
use crate::settings::Setting;
use crate::settings_layout::{controller_only, resolve_layout, LayoutEntry, Placement, ResolvedLayout, CONTROLS_TAB, TAB_ORDER};
use bevy::prelude::*;

/// The lines of the window at once [documented: `options_container`
/// `_number_of_visible_items` 9].
pub const WINDOW_ROWS: usize = 9;

/// One of the Controls tab's own rows (`crate::bindings`), by what it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlRow {
    /// Input Device: keyboard and mouse, or a controller.
    Device,
    /// A control's key and mouse button (`KeyboardMouse`) or its controller button.
    Binding(Control, InputDevice),
    Invert,
    /// The look sensitivity of a device: Mouse Sensitivity or Stick Sensitivity.
    Sensitivity(InputDevice),
    /// With a controller only.
    Rumble,
}

impl ControlRow {
    /// The Controls tab's own rows with `device` in use: the device, every control, Invert
    /// Y, the device's sensitivity, and with a controller Rumble.
    pub fn rows(device: InputDevice) -> Vec<ControlRow> {
        let mut out = vec![ControlRow::Device];
        out.extend(GAME_CONTROLS.iter().map(|(c, _, _)| ControlRow::Binding(*c, device)));
        out.extend([ControlRow::Invert, ControlRow::Sensitivity(device)]);
        if device == InputDevice::Controller {
            out.push(ControlRow::Rumble);
        }
        out
    }

    /// The row's place in [`ControlRow::rows`], the same with either device (the UI sounds
    /// hear a move by it).
    fn ordinal(self) -> usize {
        match self {
            ControlRow::Device => 0,
            ControlRow::Binding(c, _) => 1 + GAME_CONTROLS.iter().position(|(k, _, _)| *k == c).unwrap_or(0),
            ControlRow::Invert => GAME_CONTROLS.len() + 1,
            ControlRow::Sensitivity(_) => GAME_CONTROLS.len() + 2,
            ControlRow::Rumble => GAME_CONTROLS.len() + 3,
        }
    }

    /// The row as the log and the test aids name it (`device`, `bind:Jump`,
    /// `padbind:Jump`, `invert`, `sensitivity`, `rumble`).
    pub fn name(self) -> String {
        match self {
            ControlRow::Device => "device".into(),
            ControlRow::Binding(c, InputDevice::KeyboardMouse) => format!("bind:{}", Bindings::name(c)),
            ControlRow::Binding(c, InputDevice::Controller) => format!("padbind:{}", Bindings::name(c)),
            ControlRow::Invert => "invert".into(),
            ControlRow::Sensitivity(_) => "sensitivity".into(),
            ControlRow::Rumble => "rumble".into(),
        }
    }
}

/// A row of the settings page: a registered setting or one of the Controls tab's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowId {
    /// Its index in `crate::settings::Settings`.
    Setting(usize),
    Control(ControlRow),
}

/// What the UI sounds take for a Controls row: past any setting's index.
const CONTROL_SOUND_BASE: usize = 10_000;
/// What the UI sounds take for no row (an empty tab).
pub const NO_ROW_SOUND: usize = usize::MAX;

impl RowId {
    /// The row as the log names it: a setting's key, a Controls row's [`ControlRow::name`].
    pub fn name(self, list: &[Setting]) -> String {
        match self {
            RowId::Setting(i) => list.get(i).map_or_else(|| format!("row {i}"), |s| s.key.to_string()),
            RowId::Control(c) => c.name(),
        }
    }

    /// The number the UI sounds tell rows apart by (`crate::ui_sounds`).
    pub fn sound_id(self) -> usize {
        match self {
            RowId::Setting(i) => i,
            RowId::Control(c) => CONTROL_SOUND_BASE + c.ordinal(),
        }
    }
}

/// A line of the page's list: a heading over rows, or a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewEntry {
    Heading(Placement),
    Row(RowId),
}

/// What the settings page lists: a tab, the search's results or the Changed list (those
/// two filled by #1953 points 5 and 6; until then they list nothing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsScope {
    Tab(&'static str),
    Search,
    Changed,
}

impl Default for SettingsScope {
    fn default() -> SettingsScope {
        SettingsScope::Tab(TAB_ORDER[0])
    }
}

impl SettingsScope {
    /// Its name in English (the log's; the page translates it).
    pub fn name(self) -> &'static str {
        match self {
            SettingsScope::Tab(t) => t,
            SettingsScope::Search => SEARCH,
            SettingsScope::Changed => CHANGED,
        }
    }
}

/// The rail's first two lines [design: #1953].
pub const SEARCH: &str = "Search";
pub const CHANGED: &str = "Changed";

/// A line of the rail left of the column: Search, Changed, the tabs, then the page's
/// Defaults and Back (drawn at the bottom right, reached by the rail's keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailItem {
    Search,
    Changed,
    Tab(&'static str),
    Defaults,
    Back,
}

impl RailItem {
    /// Its name in English (the log's).
    pub fn name(self) -> &'static str {
        match self {
            RailItem::Search => SEARCH,
            RailItem::Changed => CHANGED,
            RailItem::Tab(t) => t,
            RailItem::Defaults => "Defaults",
            RailItem::Back => "Back",
        }
    }

    /// The rail's line for scope `s`.
    pub fn of(s: SettingsScope) -> RailItem {
        match s {
            SettingsScope::Tab(t) => RailItem::Tab(t),
            SettingsScope::Search => RailItem::Search,
            SettingsScope::Changed => RailItem::Changed,
        }
    }
}

/// Where the keys are on the settings page: on the rows, or on the rail (F6, a
/// controller's Y) [design: #1953].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SettingsFocus {
    #[default]
    Rows,
    Rail(RailItem),
}

/// The rail's lines: Search, Changed, every tab of `layout`, Defaults, Back.
pub fn rail(layout: &ResolvedLayout) -> Vec<RailItem> {
    let mut out = vec![RailItem::Search, RailItem::Changed];
    out.extend(layout.tab_names().into_iter().map(RailItem::Tab));
    out.extend([RailItem::Defaults, RailItem::Back]);
    out
}

/// What the game's state shows of the rows (module notes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Visibility {
    pub device: InputDevice,
    /// The HUD's and the Pip-Boy's colour are Custom (their sliders shown).
    pub colours: [bool; 2],
}

impl Visibility {
    /// As the world has it now.
    pub fn of_world(world: &World) -> Visibility {
        let device = world.get_resource::<Bindings>().map_or(InputDevice::default(), |b| b.device);
        Visibility::of(device, world.get_resource::<crate::ui_colour::CustomColours>())
    }

    /// With `device` in use and the colours' Custom choices `flags`.
    pub fn of(device: InputDevice, flags: Option<&crate::ui_colour::CustomColours>) -> Visibility {
        Visibility { device, colours: [crate::ui_colour::custom_shown(flags, 0), crate::ui_colour::custom_shown(flags, 1)] }
    }

    /// Whether registered row `s` is shown.
    pub fn shows(&self, s: &Setting) -> bool {
        if let Some((which, _)) = crate::ui_colour::channel_of(s.key) {
            return self.colours[which];
        }
        !controller_only(s.key) || self.device == InputDevice::Controller
    }
}

/// The row that stands for registered row `i` while it is hidden: its colour's list, or
/// the Input Device row for a controller's row; `None` when it is shown or has none.
pub fn parent_of(list: &[Setting], i: usize, vis: Visibility) -> Option<RowId> {
    let s = list.get(i)?;
    if vis.shows(s) {
        return None;
    }
    if let Some((_, colour)) = crate::ui_colour::channel_of(s.key) {
        return list.iter().position(|r| r.key == colour).map(RowId::Setting);
    }
    controller_only(s.key).then_some(RowId::Control(ControlRow::Device))
}

/// The tab a row is on: a setting's by the table, a Controls row's Controls.
pub fn home_tab(layout: &ResolvedLayout, id: RowId) -> &'static str {
    match id {
        RowId::Setting(i) => layout.placement_of(i).map_or(TAB_ORDER[0], |p| p.tab),
        RowId::Control(_) => CONTROLS_TAB,
    }
}

/// The lines `scope` lists: a tab's headings and shown rows (the Controls tab's own rows
/// first, no heading over them); nothing for the search and the Changed list yet.
pub fn project(layout: &ResolvedLayout, list: &[Setting], scope: SettingsScope, vis: Visibility) -> Vec<ViewEntry> {
    let SettingsScope::Tab(tab) = scope else { return Vec::new() };
    let mut out = Vec::new();
    if tab == CONTROLS_TAB {
        out.extend(ControlRow::rows(vis.device).into_iter().map(|c| ViewEntry::Row(RowId::Control(c))));
    }
    out.extend(layout.tab_entries(tab, |i| list.get(i).is_some_and(|s| vis.shows(s))).into_iter().map(|e| match e {
        LayoutEntry::Heading(p) => ViewEntry::Heading(p),
        LayoutEntry::Setting(i) => ViewEntry::Row(RowId::Setting(i)),
    }));
    out
}

/// The line `id` is on.
pub fn slot_of(entries: &[ViewEntry], id: RowId) -> Option<usize> {
    entries.iter().position(|e| *e == ViewEntry::Row(id))
}

/// The first row (none: nothing to choose).
pub fn first_row(entries: &[ViewEntry]) -> Option<RowId> {
    entries.iter().find_map(|e| match e {
        ViewEntry::Row(id) => Some(*id),
        ViewEntry::Heading(_) => None,
    })
}

/// The rows, in order.
fn rows(entries: &[ViewEntry]) -> Vec<RowId> {
    entries
        .iter()
        .filter_map(|e| match e {
            ViewEntry::Row(id) => Some(*id),
            ViewEntry::Heading(_) => None,
        })
        .collect()
}

/// The row `step` rows after (1) or before (-1) `selected`, round the ends, headings passed;
/// none chosen: the first (or last) row.
pub fn step_selectable(entries: &[ViewEntry], selected: Option<RowId>, step: i32) -> Option<RowId> {
    let rows = rows(entries);
    if rows.is_empty() {
        return None;
    }
    let n = rows.len() as i32;
    match selected.and_then(|s| rows.iter().position(|r| *r == s)) {
        Some(at) => Some(rows[(at as i32 + step).rem_euclid(n) as usize]),
        None if step < 0 => rows.last().copied(),
        None => rows.first().copied(),
    }
}

/// The sections: each heading (or the tab's start, without one) and its rows' lines.
fn sections(entries: &[ViewEntry]) -> Vec<(Option<Placement>, Vec<usize>)> {
    let mut out: Vec<(Option<Placement>, Vec<usize>)> = Vec::new();
    for (k, e) in entries.iter().enumerate() {
        match e {
            ViewEntry::Heading(p) => out.push((Some(*p), Vec::new())),
            ViewEntry::Row(_) => match out.last_mut() {
                Some((_, rows)) => rows.push(k),
                None => out.push((None, vec![k])),
            },
        }
    }
    out.retain(|(_, rows)| !rows.is_empty());
    out
}

/// Page Down (`step` 1) or Page Up (-1): the first row under the heading after or before
/// `selected`'s, round the ends, and that heading (`None`: a section without one). A tab
/// without headings is one section: its first row. Nothing to choose: `None`.
pub fn step_heading(entries: &[ViewEntry], selected: Option<RowId>, step: i32) -> Option<(RowId, Option<Placement>)> {
    let secs = sections(entries);
    if secs.is_empty() {
        return None;
    }
    let n = secs.len() as i32;
    let slot = selected.and_then(|s| slot_of(entries, s));
    let to = match slot.and_then(|k| secs.iter().position(|(_, rows)| rows.contains(&k))) {
        Some(at) => (at as i32 + step).rem_euclid(n) as usize,
        None if step < 0 => secs.len() - 1,
        None => 0,
    };
    let (heading, rows) = &secs[to];
    match entries[rows[0]] {
        ViewEntry::Row(id) => Some((id, *heading)),
        ViewEntry::Heading(_) => None,
    }
}

/// The heading over `id` (`None`: a section without one).
pub fn heading_of(entries: &[ViewEntry], id: RowId) -> Option<Placement> {
    let k = slot_of(entries, id)?;
    entries[..k].iter().rev().find_map(|e| match e {
        ViewEntry::Heading(p) => Some(*p),
        ViewEntry::Row(_) => None,
    })
}

/// The first line shown of `entries` from `first`, [`WINDOW_ROWS`] at once, moved as little
/// as keeps the chosen line `selected` in view, and its heading too when it is the
/// heading's first row.
pub fn follow(entries: &[ViewEntry], first: usize, selected: Option<usize>) -> usize {
    let n = entries.len();
    let last = n.saturating_sub(WINDOW_ROWS);
    let Some(s) = selected.filter(|s| *s < n) else { return first.min(last) };
    let mut first = crate::menu::model::keep_in_view(first, s, n, WINDOW_ROWS);
    if s > 0 && matches!(entries[s - 1], ViewEntry::Heading(_)) {
        first = first.min(s - 1);
    }
    first.min(last)
}

/// The lines drawn of a window from `first`: [`WINDOW_ROWS`] of them, less a heading on
/// the last line (it is drawn with its first row in the next window).
pub fn drawn_slots(entries: &[ViewEntry], first: usize) -> std::ops::Range<usize> {
    let first = first.min(entries.len());
    let mut end = (first + WINDOW_ROWS).min(entries.len());
    if end > first + 1 && matches!(entries[end - 1], ViewEntry::Heading(_)) {
        end -= 1;
    }
    first..end
}

/// The chosen row once the window moved to `first` (the wheel, the scroll bar): itself when
/// still drawn, else the drawn row nearest to it.
pub fn clamp_into_window(entries: &[ViewEntry], first: usize, selected: Option<RowId>) -> Option<RowId> {
    let drawn = drawn_slots(entries, first);
    let at = selected.and_then(|s| slot_of(entries, s));
    if at.is_some_and(|k| drawn.contains(&k)) {
        return selected;
    }
    let shown: Vec<(usize, RowId)> = drawn
        .filter_map(|k| match entries[k] {
            ViewEntry::Row(id) => Some((k, id)),
            ViewEntry::Heading(_) => None,
        })
        .collect();
    match at {
        // Above the window: its top row; below it: its bottom row.
        Some(k) if shown.first().is_some_and(|(f, _)| k < *f) => shown.first().map(|r| r.1),
        Some(_) => shown.last().map(|r| r.1),
        None => shown.first().map(|r| r.1),
    }
}

/// The settings page's state made whole for `layout`: on a tab, the tab of the chosen row
/// (a test aid or a search may choose one on another tab); the chosen row itself when
/// listed, else the row that stands for it (hidden, [`parent_of`]; a Controls row of the
/// other device, that row of the device in use), else the first row; and the lines listed.
pub fn normalize(layout: &ResolvedLayout, list: &[Setting], scope: SettingsScope, selected: Option<RowId>, vis: Visibility) -> (SettingsScope, Option<RowId>, Vec<ViewEntry>) {
    let scope = match (scope, selected) {
        (SettingsScope::Tab(_), Some(id)) => SettingsScope::Tab(home_tab(layout, id)),
        (SettingsScope::Tab(t), None) if layout.tab(t).is_none() => SettingsScope::default(),
        (s, _) => s,
    };
    let entries = project(layout, list, scope, vis);
    let stand_in = selected.and_then(|id| match id {
        RowId::Setting(i) => parent_of(list, i, vis),
        RowId::Control(ControlRow::Binding(c, _)) => Some(RowId::Control(ControlRow::Binding(c, vis.device))),
        RowId::Control(ControlRow::Sensitivity(_)) => Some(RowId::Control(ControlRow::Sensitivity(vis.device))),
        RowId::Control(ControlRow::Rumble) => Some(RowId::Control(ControlRow::Device)),
        RowId::Control(_) => None,
    });
    let listed = |id: &RowId| slot_of(&entries, *id).is_some();
    let selected = selected.filter(listed).or(stand_in.filter(listed)).or_else(|| first_row(&entries));
    (scope, selected, entries)
}

/// The table applied to the registry, kept until a row is added ([`resolve_layout`]).
#[derive(Resource, Default)]
pub struct SettingsLayout {
    rows: Option<usize>,
    pub layout: ResolvedLayout,
}

impl SettingsLayout {
    /// The layout of `list`, worked out again when the registry grew.
    pub fn refresh(&mut self, list: &[Setting]) -> &ResolvedLayout {
        if self.rows != Some(list.len()) {
            self.layout = resolve_layout(list);
            self.rows = Some(list.len());
        }
        &self.layout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn registry(rows: &[(&'static str, &'static str)]) -> Vec<Setting> {
        let mut s = Settings::default();
        for (key, group) in rows {
            s.add(Setting { key, group, label: key, note: "", get: |_| false, set: |_, _| {}, slider: None });
        }
        s.list().to_vec()
    }

    const ROWS: [(&str, &str); 12] = [
        ("difficulty", "Gameplay"),
        ("hardcore", "Gameplay"),
        ("menu-pause", "Gameplay"),
        ("head-bob", "Display"),
        ("autosave-travel", "Saving"),
        ("autosave-wait", "Saving"),
        ("hud-colour", "Interface"),
        ("hud-red", "Interface"),
        ("hud-green", "Interface"),
        ("hud-blue", "Interface"),
        ("button-style", "Controller"),
        ("god-mode", "Debug"),
    ];

    fn key(list: &[Setting], k: &str) -> RowId {
        RowId::Setting(list.iter().position(|s| s.key == k).unwrap())
    }

    fn names(list: &[Setting], entries: &[ViewEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                ViewEntry::Heading(p) => format!("[{}]", p.heading.unwrap_or(p.tab)),
                ViewEntry::Row(id) => id.name(list),
            })
            .collect()
    }

    #[test]
    fn a_tab_lists_its_headings_over_its_shown_rows() {
        let list = registry(&ROWS);
        let l = resolve_layout(&list);
        let vis = Visibility::default();
        let g = project(&l, &list, SettingsScope::Tab("Gameplay"), vis);
        assert_eq!(names(&list, &g), ["[Game]", "difficulty", "hardcore", "[Saving]", "autosave-travel", "autosave-wait", "[Dialogue]", "menu-pause", "[Camera]", "head-bob"]);
        // The colour's sliders only for Custom, the HUD's alone.
        let d = |colours| names(&list, &project(&l, &list, SettingsScope::Tab("Display"), Visibility { colours, ..vis }));
        assert_eq!(d([false, false]), ["[Interface]", "hud-colour"]);
        assert_eq!(d([true, false]), ["[Interface]", "hud-colour", "hud-red", "hud-green", "hud-blue"]);
        assert_eq!(d([false, true]), ["[Interface]", "hud-colour"]);
        // The Controls tab: its own rows first, the controller's rows with a controller only.
        let c = |device| names(&list, &project(&l, &list, SettingsScope::Tab(CONTROLS_TAB), Visibility { device, ..vis }));
        let kb = c(InputDevice::KeyboardMouse);
        assert_eq!(kb.len(), GAME_CONTROLS.len() + 3);
        assert_eq!((kb[0].as_str(), kb[1].as_str(), kb.last().unwrap().as_str()), ("device", "bind:Forward", "sensitivity"));
        let pad = c(InputDevice::Controller);
        assert_eq!(pad[GAME_CONTROLS.len() + 1..], ["invert", "sensitivity", "rumble", "button-style"]);
        assert!(pad[1].starts_with("padbind:"));
        // An empty tab, the search and the Changed list (not yet filled): nothing.
        assert!(project(&l, &list, SettingsScope::Tab("Tweaks"), vis).is_empty());
        assert!(project(&l, &list, SettingsScope::Search, vis).is_empty());
    }

    #[test]
    fn up_and_down_pass_the_headings_round_the_ends() {
        let list = registry(&ROWS);
        let l = resolve_layout(&list);
        let g = project(&l, &list, SettingsScope::Tab("Gameplay"), Visibility::default());
        let k = |n| key(&list, n);
        assert_eq!(step_selectable(&g, Some(k("hardcore")), 1), Some(k("autosave-travel")));
        assert_eq!(step_selectable(&g, Some(k("autosave-travel")), -1), Some(k("hardcore")));
        assert_eq!(step_selectable(&g, Some(k("head-bob")), 1), Some(k("difficulty")), "round the end");
        assert_eq!(step_selectable(&g, Some(k("difficulty")), -1), Some(k("head-bob")));
        assert_eq!(step_selectable(&g, None, 1), Some(k("difficulty")));
        assert_eq!(step_selectable(&[], None, 1), None, "an empty tab");
        let one = [ViewEntry::Heading(Placement::tab("x")), ViewEntry::Row(k("god-mode"))];
        assert_eq!(step_selectable(&one, Some(k("god-mode")), 1), Some(k("god-mode")));
    }

    #[test]
    fn page_up_and_down_go_from_heading_to_heading() {
        let list = registry(&ROWS);
        let l = resolve_layout(&list);
        let g = project(&l, &list, SettingsScope::Tab("Gameplay"), Visibility::default());
        let k = |n| key(&list, n);
        let to = |from: &str, step| step_heading(&g, Some(key(&list, from)), step).map(|(id, p)| (id.name(&list), p.and_then(|p| p.heading)));
        assert_eq!(to("difficulty", 1), Some(("autosave-travel".into(), Some("Saving"))));
        assert_eq!(to("autosave-wait", 1), Some(("menu-pause".into(), Some("Dialogue"))));
        assert_eq!(to("menu-pause", -1), Some(("autosave-travel".into(), Some("Saving"))));
        assert_eq!(to("head-bob", 1), Some(("difficulty".into(), Some("Game"))), "round the end");
        assert_eq!(to("difficulty", -1), Some(("head-bob".into(), Some("Camera"))));
        // A tab without headings is one section: its first row; an empty one, nothing.
        let debug = project(&l, &list, SettingsScope::Tab("Debug"), Visibility::default());
        assert_eq!(step_heading(&debug, Some(k("god-mode")), 1), Some((k("god-mode"), None)));
        assert_eq!(step_heading(&[], None, 1), None);
        // A section whose rows are all hidden has no heading to stop at.
        let d = project(&l, &list, SettingsScope::Tab("Display"), Visibility::default());
        assert_eq!(heading_of(&d, k("hud-colour")).and_then(|p| p.heading), Some("Interface"));
        assert_eq!(step_heading(&d, Some(k("hud-colour")), 1), Some((k("hud-colour"), Some(heading_of(&d, k("hud-colour")).unwrap()))));
    }

    #[test]
    fn the_window_keeps_the_choice_and_its_heading_and_never_ends_on_a_heading() {
        let h = |t| ViewEntry::Heading(Placement { tab: "T", subgroup: None, heading: Some(t) });
        let r = |i| ViewEntry::Row(RowId::Setting(i));
        // [A] 0..7, [B] 7..12: the heading B is on line 9, the window's 10th.
        let mut e = vec![h("A")];
        e.extend((0..8).map(r));
        e.push(h("B"));
        e.extend((8..12).map(r));
        assert_eq!(e.len(), 14);
        // From the top: line 9 (B) would be the last of a window from 1; it is left out.
        assert_eq!(drawn_slots(&e, 1), 1..9);
        assert_eq!(drawn_slots(&e, 0), 0..9);
        // Down onto B's first row (line 10): the window shows B too.
        let first = follow(&e, 0, Some(10));
        assert!(first <= 9 && first + WINDOW_ROWS > 10, "{first}");
        // Up to A's first row (line 1): A's heading comes back into view.
        assert_eq!(follow(&e, 5, Some(1)), 0);
        // Never past the last full window.
        assert_eq!(follow(&e, 20, None), e.len() - WINDOW_ROWS);
        // The wheel moved the window off the choice: the nearest drawn row is chosen.
        assert_eq!(clamp_into_window(&e, 3, Some(RowId::Setting(0))), Some(RowId::Setting(2)));
        assert_eq!(clamp_into_window(&e, 0, Some(RowId::Setting(11))), Some(RowId::Setting(7)));
        assert_eq!(clamp_into_window(&e, 0, Some(RowId::Setting(3))), Some(RowId::Setting(3)));
    }

    #[test]
    fn a_hidden_or_missing_choice_moves_to_the_row_that_stands_for_it() {
        let list = registry(&ROWS);
        let l = resolve_layout(&list);
        let k = |n| key(&list, n);
        let vis = Visibility::default();
        // A test aid chooses a hidden slider on another tab: its colour, on its tab.
        let (scope, sel, _) = normalize(&l, &list, SettingsScope::Tab("Gameplay"), Some(k("hud-red")), vis);
        assert_eq!((scope, sel), (SettingsScope::Tab("Display"), Some(k("hud-colour"))));
        // Shown: itself.
        let (_, sel, _) = normalize(&l, &list, SettingsScope::Tab("Display"), Some(k("hud-red")), Visibility { colours: [true, false], ..vis });
        assert_eq!(sel, Some(k("hud-red")));
        // A controller's row with keyboard and mouse: the Input Device row.
        let (scope, sel, _) = normalize(&l, &list, SettingsScope::Tab("Gameplay"), Some(k("button-style")), vis);
        assert_eq!((scope, sel), (SettingsScope::Tab(CONTROLS_TAB), Some(RowId::Control(ControlRow::Device))));
        // A control's controller button once the keyboard is chosen: its key.
        let jump = RowId::Control(ControlRow::Binding(Control::Jump, InputDevice::Controller));
        let (_, sel, _) = normalize(&l, &list, SettingsScope::Tab(CONTROLS_TAB), Some(jump), vis);
        assert_eq!(sel, Some(RowId::Control(ControlRow::Binding(Control::Jump, InputDevice::KeyboardMouse))));
        // Nothing chosen: the first row; an empty tab keeps none.
        let (_, sel, _) = normalize(&l, &list, SettingsScope::Tab("Gameplay"), None, vis);
        assert_eq!(sel, Some(k("difficulty")));
        let (scope, sel, entries) = normalize(&l, &list, SettingsScope::Tab("Tweaks"), None, vis);
        assert_eq!((scope, sel, entries.len()), (SettingsScope::Tab("Tweaks"), None, 0));
        assert_eq!(parent_of(&list, list.iter().position(|s| s.key == "god-mode").unwrap(), vis), None, "shown: none");
    }

    #[test]
    fn each_row_has_a_name_and_a_sound_of_its_own() {
        let list = registry(&ROWS);
        let all: Vec<RowId> = ControlRow::rows(InputDevice::Controller).into_iter().map(RowId::Control).chain((0..list.len()).map(RowId::Setting)).collect();
        let mut sounds: Vec<usize> = all.iter().map(|r| r.sound_id()).collect();
        sounds.sort_unstable();
        sounds.dedup();
        assert_eq!(sounds.len(), all.len());
        assert!(!sounds.contains(&NO_ROW_SOUND));
        assert_eq!(RowId::Control(ControlRow::Binding(Control::Jump, InputDevice::KeyboardMouse)).name(&list), "bind:Jump");
        assert_eq!(RowId::Control(ControlRow::Binding(Control::Jump, InputDevice::Controller)).name(&list), "padbind:Jump");
        assert_eq!(key(&list, "god-mode").name(&list), "god-mode");
        let l = resolve_layout(&list);
        assert_eq!(rail(&l).len(), 2 + TAB_ORDER.len() + 2);
    }
}
