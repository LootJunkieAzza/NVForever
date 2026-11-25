//! Co-op lobby screens (private fork, not upstream; the network side is nvforever
//! `coop.rs`). The pause menu's Lobby entry opens the host's screen: Create Lobby, then
//! the code to read out to friends and who has joined. The main menu's Join Lobby entry
//! opens the guest's: the code typed in, Join, and once the host answers, the guest's
//! saves to choose from; only saves past the opening (out of Doc Mitchell's house, not in
//! character creation) can join.
//!
//! A guest in a session opens the same pause-menu entry and sees who is in the lobby, the
//! host marked, with Leave Lobby instead of Create Lobby (queue item 82).
//!
//! The screen is drawn over the menu that opened it and takes the keys while it is up
//! ([`LobbyScreen::holds_input`]); it asks the network side for things with
//! [`LobbyRequest`] and shows what [`LobbyState`] says.
//!
//! Test aid `NVF_PLAY_TEST_LOBBY="<seconds>:<action>,..."` (real seconds from the start):
//! `code=<code>` types a code, `join`, `open` (the lobby as the pause menu opens it),
//! `create`, `close`, `back`, `admit` / `turn-away` (the first player asking to join),
//! `kick` / `ban` (the host's first guest), `public` (the host's lobby listed),
//! `title=<text>`, `password=<text>` (the host's, or the join's on the Join screen), `browse`
//! (the Join screen's Browse tab), `refresh`, `row=<n>` (open the n-th row shown, from 0),
//! `join-row`, `search=<text>`,
//! `pick=<n>` (the n-th
//! save that can join, from 0), `shot=<file>` (a screenshot); for the Emotes menu,
//! `emotes` (open it), `slot=<n>` (1-8: choose a slot), `equip=<emote>` (put the emote,
//! by key, in the slot chosen), `empty` (empty it).
//!
//! The server browser (docs/design/coop-lobby-browser.md): a host chooses Private or Public
//! and may set a title and a password; the Join screen has a Code tab (the code, and the
//! password when the lobby has one) and a Browse tab listing public lobbies, with filters,
//! search and favourites. A row opens to Join (and a password box when locked).
//!
//! The Emotes menu (queue item 137) chooses the eight emotes of the emote wheel
//! (`crate::emotes`): a slot, then the emote for it, from every emote the load order has,
//! by category. One already in another slot swaps with it; a slot can be emptied.

use crate::interface::{amber, bc_supported, u};
use crate::start_menu::{color, highlight, text, wrapped, Faces, Shade, ITEM, LINE};
use crate::ui_pointer::{Button, PointerState};
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use nvf_assets::GameData;
use std::path::PathBuf;

/// The screen's width and top, interface units.
const WIDTH: f32 = 820.0;
const TOP: f32 = 150.0;
const PAD: f32 = 24.0;
/// Small print's line step.
const STEP: f32 = 30.0;
/// Saves shown at once.
const SAVE_ROWS: usize = 8;
/// Emotes and their categories' headings shown at once.
const EMOTE_ROWS: usize = 10;
/// Public lobbies shown at once on the Browse tab.
const BROWSE_ROWS: usize = 8;
/// The longest title, password or search typed (the service cuts titles at 40).
const MAX_TEXT: usize = 40;
/// The Browse table's columns: heading and width (interface units), adding up to the
/// panel's inner width.
const COLUMNS: [(&str, f32); 7] = [("", 40.0), ("Name", 250.0), ("Location", 170.0), ("Players", 90.0), ("Ping", 90.0), ("Region", 80.0), ("", 52.0)];
/// Players a lobby takes: the host and `nvf_coop::net::MAX_GUESTS` guests (nvforever's
/// co-op tests check the two agree).
pub const LOBBY_PLAYERS: usize = 6;
/// The longest code typed (12 characters and the dashes, with room for spaces).
const MAX_TYPED: usize = 26;

/// Which screen is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Host,
    Join,
}

/// What the screen asks of the network side.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum LobbyRequest {
    /// Open a lobby on this game, listed or not, with a password or not.
    Create(HostOptions),
    /// Close it (guests are told the game ended).
    Close,
    /// A guest leaves its host's lobby.
    Leave,
    /// The host sets Friendly Fire (item 122): whether players can hurt each other.
    FriendlyFire(bool),
    /// Look for the host of this code (with the lobby's password, empty for none).
    Find { code: String, password: String },
    /// Ask the service for the public lobbies.
    Browse(BrowseQuery),
    /// Add (`on`) or remove a host, by its key's fingerprint, from the favourites.
    Favourite { host_fp: String, on: bool },
    /// Stop looking (or leave the host found).
    Cancel,
    /// Load this save and join the host found.
    Load { path: PathBuf, name: String },
    /// The host's answer to player `peer` asking to join (join approval).
    Admit { peer: u32, yes: bool },
    /// The host removes guest `id` (Kick); with `ban`, not to come back while the lobby lasts.
    Kick { id: u8, ban: bool },
}

/// The host's lobby.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Hosting {
    #[default]
    Off,
    /// The router is being asked to open the port.
    Opening,
    Open {
        code: String,
        /// Why friends outside this network may not get in, if anything.
        note: Option<String>,
    },
}

/// A guest's search for its host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Joining {
    #[default]
    Idle,
    Finding,
    /// The host answered and keeps a place.
    Found,
    Failed(String),
}

/// The host's choices for a new lobby (docs/design/coop-lobby-browser.md §4, §7).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostOptions {
    /// Listed in the server browser.
    pub public: bool,
    /// The browser's name for it; empty: "<name>'s game".
    pub title: String,
    /// Empty: none.
    pub password: String,
}

/// What the Browse tab asks the service for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrowseQuery {
    pub search: String,
    /// Lobbies of other versions and load orders too (greyed).
    pub all_versions: bool,
}

/// A public lobby, as the Browse tab lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublicLobby {
    pub code: String,
    pub title: String,
    pub host_name: String,
    pub place: String,
    pub players: u8,
    pub max_players: u8,
    pub locked: bool,
    pub friendly_fire: bool,
    pub region: String,
    /// Estimated (docs/design/coop-lobby-browser.md §5), milliseconds.
    pub ping_ms: Option<u32>,
    /// The host key's fingerprint, for favourites.
    pub host_fp: String,
    pub favourite: bool,
    /// Why this game cannot join it (another version or load order); None: it can.
    pub cannot: Option<String>,
}

/// The Browse tab's list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Browse {
    #[default]
    Idle,
    Loading,
    Failed(String),
    Ready(Vec<PublicLobby>),
}

/// What the network side says (written by nvforever `coop.rs`).
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct LobbyState {
    pub hosting: Hosting,
    /// The players in the lobby, this game's first.
    pub players: Vec<String>,
    pub joining: Joining,
    /// This game is a guest in another player's lobby.
    pub guest: bool,
    /// Friendly Fire, as the host set it (item 122; off by default).
    pub friendly_fire: bool,
    /// Players asking to join the host's lobby (join approval): the service's number for
    /// each, and its name.
    pub asking: Vec<(u32, String)>,
    /// The host's guests, by id, for Kick and Ban.
    pub guests: Vec<(u8, String)>,
    /// The host's lobby is listed in the server browser, and has a password.
    pub public: bool,
    pub locked: bool,
    /// The Browse tab's public lobbies.
    pub browse: Browse,
}

/// A save on the guest's list.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SaveRow {
    path: PathBuf,
    name: String,
    line: String,
    /// Past the opening: it can join.
    ok: bool,
}

/// The screen: which is up, what is typed, the saves listed.
#[derive(Resource, Default)]
pub struct LobbyScreen {
    mode: Option<Mode>,
    typed: String,
    saves: Option<Vec<SaveRow>>,
    selected: usize,
    /// The frame count as of the screen's last run, and when it opened and closed: the
    /// keys of those frames were the menu's (or are the menu's again).
    frame: u32,
    opened: u32,
    closed: Option<u32>,
    /// The Emotes menu is up: `Some(slot)` while choosing the emote for that slot.
    emotes: Option<Option<usize>>,
    /// The host's Remove a Player list is up.
    removing: bool,
    /// The text fields other than the code (`typed`).
    join_password: String,
    title: String,
    host_password: String,
    search: String,
    row_password: String,
    /// The host's choice of Public.
    public: bool,
    /// The Join screen's Browse tab is up, not the Code tab.
    browsing: bool,
    /// The Browse tab's filters.
    hide_empty: bool,
    hide_full: bool,
    all_versions: bool,
    /// The row opened, by code.
    open_row: Option<String>,
    /// The loadout and the emotes the load order has, as of this frame (from
    /// `crate::emotes`).
    loadout: [Option<usize>; crate::emotes::SLOTS],
    installed: Vec<usize>,
}

impl LobbyScreen {
    /// Open `mode`'s screen (from a menu's entry).
    pub fn open(&mut self, mode: Mode) {
        info!("lobby: {mode:?} screen");
        self.mode = Some(mode);
        self.selected = 0;
        self.saves = None;
        self.emotes = None;
        self.removing = false;
        self.browsing = false;
        self.open_row = None;
        self.hide_empty = true;
        self.opened = self.frame;
    }

    pub fn is_open(&self) -> bool {
        self.mode.is_some()
    }

    /// Whether the menus under it should leave the keys and the pointer alone this frame.
    pub fn holds_input(&self) -> bool {
        self.mode.is_some() || self.closed.is_some_and(|f| self.frame <= f + 1)
    }

    fn close(&mut self) {
        self.mode = None;
        self.closed = Some(self.frame);
    }
}

/// `NVF_PLAY_TEST_LOBBY`: (seconds, action), soonest first.
#[derive(Resource, Default)]
struct LobbyAid(Vec<(f32, String)>);

#[derive(Resource)]
struct LobbyLook(Faces);

#[derive(Component)]
struct LobbyRoot;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    Create,
    Close,
    Leave,
    /// Friendly Fire on or off (the host's).
    FriendlyFire,
    Join,
    Back,
    Save(usize),
    /// Open the Emotes menu.
    Emotes,
    /// Choose a slot (0-based) of the loadout.
    Slot(usize),
    /// Put this emote (catalogue index) in the slot chosen.
    Equip(usize),
    /// Empty the slot chosen.
    Empty,
    /// Let the player asking to join in (the service's number for it).
    Admit(u32),
    /// Turn it away.
    TurnAway(u32),
    /// Open the host's Remove a Player list.
    Remove,
    /// Remove this guest (its id); it may join again.
    Kick(u8),
    /// Remove it and keep it out while the lobby lasts.
    Ban(u8),
    /// A text field, chosen to type in.
    Field(FieldId),
    /// The host's Private / Public.
    Visibility,
    /// The Join screen's other tab (Code, Browse).
    Tab,
    Refresh,
    HideEmpty,
    HideFull,
    AllVersions,
    /// Open (or close) the n-th public lobby of the list as shown.
    Lobby(usize),
    /// Add or remove the opened lobby's host from the favourites.
    Favourite,
    /// Join the opened lobby.
    JoinRow,
}

/// The text fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldId {
    Code,
    JoinPassword,
    Title,
    HostPassword,
    Search,
    RowPassword,
}

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        let mut aid = Vec::new();
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_LOBBY") {
            for part in v.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                match part.split_once(':').and_then(|(t, a)| Some((t.trim().parse::<f32>().ok()?, a.trim().to_string()))) {
                    Some(x) => aid.push(x),
                    None => warn!("NVF_PLAY_TEST_LOBBY: {part:?}: expected <seconds>:<action>"),
                }
            }
            aid.sort_by(|a, b| a.0.total_cmp(&b.0));
            // Not for the game a chosen save starts (it inherits the environment).
            std::env::remove_var("NVF_PLAY_TEST_LOBBY");
        }
        app.init_resource::<LobbyScreen>()
            .init_resource::<LobbyState>()
            .insert_resource(LobbyAid(aid))
            .add_message::<LobbyRequest>()
            .add_systems(Update, (input, draw).chain().in_set(PlaySet::Ui));
    }
}

/// The buttons and rows that can be chosen, in order.
fn acts(screen: &LobbyScreen, state: &LobbyState) -> Vec<Act> {
    match (screen.mode, screen.emotes) {
        (Some(Mode::Host), Some(None)) => return (0..crate::emotes::SLOTS).map(Act::Slot).chain([Act::Back]).collect(),
        (Some(Mode::Host), Some(Some(_))) => return screen.installed.iter().map(|&e| Act::Equip(e)).chain([Act::Empty, Act::Back]).collect(),
        _ => {}
    }
    if screen.mode == Some(Mode::Host) && screen.removing && !state.guest {
        return state.guests.iter().flat_map(|(id, _)| [Act::Kick(*id), Act::Ban(*id)]).chain([Act::Back]).collect();
    }
    match screen.mode {
        None => Vec::new(),
        Some(Mode::Host) if state.guest => vec![Act::Leave, Act::Emotes, Act::Back],
        Some(Mode::Host) => match state.hosting {
            Hosting::Off => [Act::Visibility]
                .into_iter()
                .chain(screen.public.then_some(Act::Field(FieldId::Title)))
                .chain([Act::Field(FieldId::HostPassword), Act::Create, Act::FriendlyFire, Act::Emotes, Act::Back])
                .collect(),
            Hosting::Opening => vec![Act::Back],
            Hosting::Open { .. } => state
                .asking
                .iter()
                .flat_map(|(p, _)| [Act::Admit(*p), Act::TurnAway(*p)])
                .chain((!state.guests.is_empty()).then_some(Act::Remove))
                .chain([Act::Close, Act::FriendlyFire, Act::Emotes, Act::Back])
                .collect(),
        },
        Some(Mode::Join) => match (&state.joining, &screen.saves) {
            (Joining::Found, Some(saves)) => saves.iter().enumerate().filter(|(_, s)| s.ok).map(|(i, _)| Act::Save(i)).chain([Act::Back]).collect(),
            (Joining::Finding | Joining::Found, _) => vec![Act::Back],
            (Joining::Idle | Joining::Failed(_), _) if screen.browsing => browse_acts(screen, state),
            (Joining::Idle | Joining::Failed(_), _) => vec![Act::Tab, Act::Field(FieldId::Code), Act::Field(FieldId::JoinPassword), Act::Join, Act::Back],
        },
    }
}

/// The Browse tab's choices: the tab, the filters, search, then each row shown (and the
/// opened one's own choices under it).
fn browse_acts(screen: &LobbyScreen, state: &LobbyState) -> Vec<Act> {
    let mut v = vec![Act::Tab, Act::Refresh, Act::HideEmpty, Act::HideFull, Act::AllVersions, Act::Field(FieldId::Search)];
    let rows = shown(screen, state);
    for (i, l) in rows.iter().enumerate() {
        v.push(Act::Lobby(i));
        if screen.open_row.as_deref() == Some(l.code.as_str()) {
            if l.locked && l.cannot.is_none() {
                v.push(Act::Field(FieldId::RowPassword));
            }
            if l.cannot.is_none() {
                v.push(Act::JoinRow);
            }
            v.push(Act::Favourite);
        }
    }
    v.push(Act::Back);
    v
}

/// The public lobbies the Browse tab shows: the filters applied, favourites first, then the
/// nearest (estimated ping), then the fullest.
fn shown<'a>(screen: &LobbyScreen, state: &'a LobbyState) -> Vec<&'a PublicLobby> {
    let Browse::Ready(all) = &state.browse else { return Vec::new() };
    let q = screen.search.trim().to_lowercase();
    let mut v: Vec<&PublicLobby> = all
        .iter()
        .filter(|l| !(screen.hide_empty && l.players <= 1))
        .filter(|l| !(screen.hide_full && l.players >= l.max_players))
        .filter(|l| screen.all_versions || l.cannot.is_none())
        .filter(|l| q.is_empty() || [&l.title, &l.host_name, &l.place].iter().any(|t| t.to_lowercase().contains(&q)))
        .collect();
    v.sort_by_key(|l| (!l.favourite, l.ping_ms.unwrap_or(u32::MAX), std::cmp::Reverse(l.players)));
    v
}

/// The text field the choice stands on, if any: keys typed go there.
fn typing(screen: &LobbyScreen, state: &LobbyState) -> Option<FieldId> {
    match acts(screen, state).get(screen.selected) {
        Some(Act::Field(f)) => Some(*f),
        _ => None,
    }
}

impl LobbyScreen {
    fn text_mut(&mut self, f: FieldId) -> &mut String {
        match f {
            FieldId::Code => &mut self.typed,
            FieldId::JoinPassword => &mut self.join_password,
            FieldId::Title => &mut self.title,
            FieldId::HostPassword => &mut self.host_password,
            FieldId::Search => &mut self.search,
            FieldId::RowPassword => &mut self.row_password,
        }
    }

    fn text(&self, f: FieldId) -> &str {
        match f {
            FieldId::Code => &self.typed,
            FieldId::JoinPassword => &self.join_password,
            FieldId::Title => &self.title,
            FieldId::HostPassword => &self.host_password,
            FieldId::Search => &self.search,
            FieldId::RowPassword => &self.row_password,
        }
    }

    /// A character typed into field `f`: a code takes letters, digits and dashes, upper case.
    fn type_char(&mut self, f: FieldId, c: char) {
        let (max, ok) = match f {
            FieldId::Code => (MAX_TYPED, c.is_ascii_alphanumeric() || c == '-'),
            _ => (MAX_TEXT, !c.is_control()),
        };
        let t = self.text_mut(f);
        if ok && t.chars().count() < max {
            t.push(if f == FieldId::Code { c.to_ascii_uppercase() } else { c });
        }
    }

    fn browse_query(&self) -> BrowseQuery {
        BrowseQuery { search: self.search.trim().to_string(), all_versions: self.all_versions }
    }
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut commands: Commands,
    real: Res<Time<Real>>,
    frames: Option<Res<bevy::diagnostic::FrameCount>>,
    mut aid: ResMut<LobbyAid>,
    mut screen: ResMut<LobbyScreen>,
    state: Res<LobbyState>,
    keys: Res<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    buttons: Query<(&PointerState, &Act), Changed<PointerState>>,
    mut asks: MessageWriter<LobbyRequest>,
    cfg: Option<Res<crate::save::SaveConfig>>,
    sim: Option<Res<crate::sim::SimState>>,
    (mut loadout, book): (ResMut<crate::emotes::EmoteSlots>, Res<crate::emotes::EmoteBook>),
) {
    screen.frame = frames.map_or(screen.frame.wrapping_add(1), |f| f.0);
    if screen.loadout != loadout.slots {
        screen.loadout = loadout.slots;
    }
    let installed: Vec<usize> = (0..crate::emotes::CATALOG.len()).filter(|&i| book.available(i)).collect();
    if screen.installed != installed {
        screen.installed = installed;
    }
    let typed: Vec<KeyboardInput> = typed.read().cloned().collect();
    let now = real.elapsed_secs();
    let mut aids = Vec::new();
    while aid.0.first().is_some_and(|(t, _)| *t <= now) {
        let (_, a) = aid.0.remove(0);
        info!("test aid NVF_PLAY_TEST_LOBBY: {a}");
        aids.push(a);
    }
    for a in aids {
        if let Some(file) = a.strip_prefix("shot=") {
            // A bare name goes where the other shots go (`NVF_PLAY_TEST_SHOT_DIR`).
            let mut path = std::path::PathBuf::from(file);
            if path.is_relative() {
                if let Some(dir) = std::env::var_os("NVF_PLAY_TEST_SHOT_DIR").filter(|d| !d.is_empty()) {
                    path = std::path::PathBuf::from(dir).join(path);
                }
            }
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
        } else if let Some(code) = a.strip_prefix("code=") {
            if screen.mode.is_none() {
                screen.open(Mode::Join);
            }
            screen.typed = code.to_string();
        } else if let Some(t) = a.strip_prefix("title=") {
            screen.title = t.to_string();
        } else if let Some(t) = a.strip_prefix("password=") {
            match screen.mode {
                Some(Mode::Join) if screen.open_row.is_some() => screen.row_password = t.to_string(),
                Some(Mode::Join) => screen.join_password = t.to_string(),
                _ => screen.host_password = t.to_string(),
            }
        } else if let Some(t) = a.strip_prefix("search=") {
            screen.search = t.to_string();
        } else if let Some(n) = a.strip_prefix("row=").and_then(|n| n.parse::<usize>().ok()) {
            take(&mut screen, &state, Act::Lobby(n), &mut asks, &mut loadout);
        } else if let Some(n) = a.strip_prefix("pick=").and_then(|n| n.parse::<usize>().ok()) {
            let save = screen.saves.as_ref().and_then(|s| s.iter().enumerate().filter(|(_, s)| s.ok).nth(n)).map(|(i, _)| i);
            match save {
                Some(i) => take(&mut screen, &state, Act::Save(i), &mut asks, &mut loadout),
                None => warn!("NVF_PLAY_TEST_LOBBY: no save {n} can join"),
            }
        } else if let Some(n) = a.strip_prefix("slot=").and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=crate::emotes::SLOTS).contains(n)) {
            if screen.mode.is_none() {
                screen.open(Mode::Host);
            }
            screen.emotes = Some(None);
            take(&mut screen, &state, Act::Slot(n - 1), &mut asks, &mut loadout);
        } else if let Some(k) = a.strip_prefix("equip=") {
            match crate::emotes::by_key(k) {
                Some(e) if screen.emotes.is_some_and(|s| s.is_some()) => take(&mut screen, &state, Act::Equip(e), &mut asks, &mut loadout),
                Some(_) => warn!("NVF_PLAY_TEST_LOBBY: equip={k}: no slot chosen"),
                None => warn!("NVF_PLAY_TEST_LOBBY: no emote {k:?}"),
            }
        } else {
            let act = match a.as_str() {
                "join" => Some(Act::Join),
                // The pause menu's Co-op Lobby entry: the lobby as it stands.
                "open" => {
                    if screen.mode.is_none() {
                        screen.open(Mode::Host);
                    }
                    None
                }
                "create" => {
                    if screen.mode.is_none() {
                        screen.open(Mode::Host);
                    }
                    Some(Act::Create)
                }
                "close" => Some(Act::Close),
                "public" => {
                    screen.public = true;
                    None
                }
                "browse" => {
                    if screen.mode.is_none() {
                        screen.open(Mode::Join);
                    }
                    (!screen.browsing).then_some(Act::Tab)
                }
                "refresh" => Some(Act::Refresh),
                "join-row" => Some(Act::JoinRow),
                "admit" => state.asking.first().map(|(p, _)| Act::Admit(*p)),
                "turn-away" => state.asking.first().map(|(p, _)| Act::TurnAway(*p)),
                "kick" => state.guests.first().map(|(id, _)| Act::Kick(*id)),
                "ban" => state.guests.first().map(|(id, _)| Act::Ban(*id)),
                "leave" => Some(Act::Leave),
                "friendly-fire" => Some(Act::FriendlyFire),
                "back" => Some(Act::Back),
                "emotes" => {
                    if screen.mode.is_none() {
                        screen.open(Mode::Host);
                    }
                    Some(Act::Emotes)
                }
                "empty" => Some(Act::Empty),
                other => {
                    warn!("NVF_PLAY_TEST_LOBBY: no action {other:?}");
                    None
                }
            };
            if let Some(act) = act {
                take(&mut screen, &state, act, &mut asks, &mut loadout);
            }
        }
    }
    if screen.mode.is_none() {
        return;
    }
    // The saves, once the host is found.
    if state.joining == Joining::Found && screen.saves.is_none() {
        let intro = sim.as_deref().and_then(|s| s.refs.cell_by_edid("GSDocMitchellHouse")).map(|c| c.0);
        let list = cfg.as_deref().and_then(|c| c.dir.as_deref()).map(fnv_save::list).unwrap_or_default();
        let rows: Vec<SaveRow> = list
            .into_iter()
            .map(|(path, h)| {
                let ok = past_opening(&path, &h, intro);
                let place = if h.location.is_empty() { "Unknown place".to_string() } else { h.location.clone() };
                let line = format!("{}, level {}, {place}{}", h.player_name, h.level, if ok { "" } else { " (still in the opening)" });
                SaveRow { path, name: h.player_name.clone(), line, ok }
            })
            .collect();
        info!("lobby: {} saves, {} past the opening", rows.len(), rows.iter().filter(|r| r.ok).count());
        screen.saves = Some(rows);
        screen.selected = 0;
    }
    if !matches!(state.joining, Joining::Found) && screen.saves.is_some() {
        screen.saves = None;
    }
    // The frame it opened in, the keys were the menu's.
    if screen.frame <= screen.opened + 1 {
        return;
    }
    let list = acts(&screen, &state);
    if list.is_empty() {
        return;
    }
    screen.selected = screen.selected.min(list.len() - 1);
    let mut chosen = None;
    for (p, act) in &buttons {
        if let Some(i) = list.iter().position(|a| a == act) {
            match p {
                PointerState::Hovered => screen.selected = i,
                PointerState::Pressed => chosen = Some(*act),
                PointerState::None => {}
            }
        }
    }
    if let Some(field) = typing(&screen, &state) {
        for k in typed.iter().filter(|k| k.state.is_pressed()) {
            match &k.logical_key {
                Key::Backspace => {
                    screen.text_mut(field).pop();
                }
                Key::Enter => {
                    chosen = match field {
                        FieldId::Code | FieldId::JoinPassword => Some(Act::Join),
                        FieldId::Search => Some(Act::Refresh),
                        FieldId::RowPassword => Some(Act::JoinRow),
                        // The host's fields: on to the next choice.
                        FieldId::Title | FieldId::HostPassword => {
                            screen.selected = (screen.selected + 1) % list.len();
                            None
                        }
                    };
                }
                _ => {
                    if let Some(t) = k.text.as_deref() {
                        for c in t.chars() {
                            screen.type_char(field, c);
                        }
                    }
                }
            }
        }
    } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
        chosen = list.get(screen.selected).copied();
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        screen.selected = (screen.selected + list.len() - 1) % list.len();
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        screen.selected = (screen.selected + 1) % list.len();
    }
    if keys.just_pressed(KeyCode::Escape) {
        chosen = Some(Act::Back);
    }
    if let Some(act) = chosen {
        take(&mut screen, &state, act, &mut asks, &mut loadout);
    }
}

fn take(screen: &mut LobbyScreen, state: &LobbyState, act: Act, asks: &mut MessageWriter<LobbyRequest>, loadout: &mut crate::emotes::EmoteSlots) {
    info!("lobby: {act:?}");
    match act {
        Act::Emotes => {
            screen.emotes = Some(None);
            screen.selected = 0;
        }
        Act::Slot(n) => {
            screen.emotes = Some(Some(n));
            // The emote in it chosen, else the first.
            screen.selected = loadout.slots[n].and_then(|e| screen.installed.iter().position(|&i| i == e)).unwrap_or(0);
        }
        Act::Equip(e) => {
            if let Some(Some(n)) = screen.emotes {
                loadout.equip(n, e);
                info!("emotes: slot {} is now {} ({})", n + 1, crate::emotes::CATALOG[e].key, loadout.line());
                loadout.save();
                screen.emotes = Some(None);
                screen.selected = n;
            }
        }
        Act::Empty => {
            if let Some(Some(n)) = screen.emotes {
                loadout.empty(n);
                info!("emotes: slot {} emptied ({})", n + 1, loadout.line());
                loadout.save();
                screen.emotes = Some(None);
                screen.selected = n;
            }
        }
        Act::Remove => {
            screen.removing = true;
            screen.selected = 0;
        }
        Act::Kick(id) | Act::Ban(id) => {
            asks.write(LobbyRequest::Kick { id, ban: matches!(act, Act::Ban(_)) });
            screen.removing = false;
            screen.selected = 0;
        }
        Act::Back if screen.removing => {
            screen.removing = false;
            screen.selected = 0;
        }
        Act::Back if screen.emotes.is_some() => {
            let back_to = screen.emotes.flatten();
            screen.emotes = if back_to.is_some() { Some(None) } else { None };
            screen.selected = back_to.unwrap_or(0);
        }
        Act::Create => {
            asks.write(LobbyRequest::Create(HostOptions { public: screen.public, title: screen.title.trim().to_string(), password: screen.host_password.clone() }));
            // Not kept: a new lobby starts with no password (§7).
            screen.host_password.clear();
        }
        Act::Field(_) => {}
        Act::Visibility => screen.public = !screen.public,
        Act::Tab => {
            screen.browsing = !screen.browsing;
            screen.selected = 0;
            screen.open_row = None;
            if screen.browsing {
                asks.write(LobbyRequest::Browse(screen.browse_query()));
            }
        }
        Act::Refresh => {
            screen.open_row = None;
            asks.write(LobbyRequest::Browse(screen.browse_query()));
        }
        Act::HideEmpty => screen.hide_empty = !screen.hide_empty,
        Act::HideFull => screen.hide_full = !screen.hide_full,
        Act::AllVersions => {
            screen.all_versions = !screen.all_versions;
            asks.write(LobbyRequest::Browse(screen.browse_query()));
        }
        Act::Lobby(i) => {
            let code = shown(screen, state).get(i).map(|l| l.code.clone());
            screen.open_row = if screen.open_row == code { None } else { code };
            screen.row_password.clear();
        }
        Act::Favourite => {
            if let Some(l) = shown(screen, state).into_iter().find(|l| Some(&l.code) == screen.open_row.as_ref()) {
                asks.write(LobbyRequest::Favourite { host_fp: l.host_fp.clone(), on: !l.favourite });
            }
        }
        Act::JoinRow => {
            if let Some(l) = shown(screen, state).into_iter().find(|l| Some(&l.code) == screen.open_row.as_ref()) {
                info!("lobby: joining public lobby {} ({})", l.code, l.title);
                asks.write(LobbyRequest::Find { code: l.code.clone(), password: screen.row_password.clone() });
            }
        }
        Act::Admit(peer) => {
            asks.write(LobbyRequest::Admit { peer, yes: true });
        }
        Act::TurnAway(peer) => {
            asks.write(LobbyRequest::Admit { peer, yes: false });
        }
        Act::Close => {
            asks.write(LobbyRequest::Close);
        }
        Act::Leave => {
            asks.write(LobbyRequest::Leave);
        }
        Act::FriendlyFire => {
            asks.write(LobbyRequest::FriendlyFire(!state.friendly_fire));
        }
        Act::Join => {
            let code = screen.typed.trim().to_string();
            if !code.is_empty() {
                asks.write(LobbyRequest::Find { code, password: screen.join_password.clone() });
            }
        }
        Act::Back => match (screen.mode, &state.joining) {
            // Back from the search or the saves: to the code again.
            (Some(Mode::Join), Joining::Finding | Joining::Found) => {
                asks.write(LobbyRequest::Cancel);
                screen.selected = 0;
            }
            (Some(Mode::Join), _) => {
                asks.write(LobbyRequest::Cancel);
                screen.close();
            }
            _ => screen.close(),
        },
        Act::Save(i) => {
            if let Some(s) = screen.saves.as_ref().and_then(|v| v.get(i)).filter(|s| s.ok) {
                info!("lobby: joining with {}", s.path.display());
                asks.write(LobbyRequest::Load { path: s.path.clone(), name: s.name.clone() });
            }
        }
    }
}

/// A save past the opening: out of Doc Mitchell's house (`intro`, its cell) and not in
/// character creation (SetInCharGen, saved only while on).
fn past_opening(path: &std::path::Path, h: &fnv_save::SaveHeader, intro: Option<u32>) -> bool {
    if intro.is_some_and(|c| h.place.interior == Some(c)) {
        return false;
    }
    match fnv_save::read(path) {
        Ok(f) => !f.body.windows(18).any(|w| w == b"\"in_chargen\":true"),
        Err(_) => false,
    }
}

/// What the screen shows, kept to see when it changes.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Title(String),
    Line(String, Shade),
    Big(String),
    /// A text field: its choice, what it shows (a password as stars).
    Field(Act, String),
    Button(Act, String),
    Row(Act, String, Shade),
    /// The Browse table's heading, and a row of it: one text per column ([`COLUMNS`]).
    Head(Vec<String>),
    Cols(Act, Vec<String>, Shade),
}

/// A field's text as shown: a password as stars.
fn field(screen: &LobbyScreen, f: FieldId) -> Piece {
    let t = screen.text(f);
    let shown = match f {
        FieldId::JoinPassword | FieldId::HostPassword | FieldId::RowPassword => "*".repeat(t.chars().count()),
        _ => t.to_string(),
    };
    Piece::Field(Act::Field(f), shown)
}

fn on_off(on: bool) -> &'static str {
    if on {
        "On"
    } else {
        "Off"
    }
}

/// The host's Friendly Fire button [design: item 122, a lobby option; off by default].
fn friendly_fire_label(on: bool) -> String {
    format!("Friendly Fire: {}", on_off(on))
}

fn pieces(screen: &LobbyScreen, state: &LobbyState) -> Vec<Piece> {
    use Piece::*;
    let mut v = Vec::new();
    if let (Some(Mode::Host), Some(slot)) = (screen.mode, screen.emotes) {
        emote_pieces(&mut v, screen, slot);
        v.push(Button(Act::Back, "Back".into()));
        return v;
    }
    if screen.mode == Some(Mode::Host) && screen.removing && !state.guest {
        v.push(Title("Remove a Player".into()));
        v.push(Line("Kick: they leave and may join again. Ban: they cannot come back while this lobby is open.".into(), Shade::Dim));
        for (id, name) in &state.guests {
            v.push(Row(Act::Kick(*id), format!("Kick {name}"), Shade::Bright));
            v.push(Row(Act::Ban(*id), format!("Ban {name}"), Shade::Bright));
        }
        v.push(Button(Act::Back, "Back".into()));
        return v;
    }
    match screen.mode {
        None => {}
        Some(Mode::Host) if state.guest => {
            v.push(Title("Co-op Lobby".into()));
            let host = state.players.iter().find_map(|p| p.strip_suffix(" (host)"));
            v.push(Line(host.map_or_else(|| "You are in another player's lobby.".to_string(), |h| format!("You are in {h}'s lobby.")), Shade::Bright));
            players(&mut v, state);
            v.push(Line(format!("Friendly fire: {} (the host's choice)", on_off(state.friendly_fire)), Shade::Dim));
            v.push(Button(Act::Leave, "Leave Lobby".into()));
            v.push(Button(Act::Emotes, "Emotes".into()));
        }
        Some(Mode::Host) => {
            v.push(Title("Co-op Lobby".into()));
            match &state.hosting {
                Hosting::Off => {
                    v.push(Line("Open a lobby on this game: friends join with its code, from their main menu's Join Lobby.".into(), Shade::Bright));
                    v.push(Button(Act::Visibility, format!("Visibility: {}", if screen.public { "Public" } else { "Private" })));
                    if screen.public {
                        v.push(Line("Listed in the server browser as (empty: your name's game):".into(), Shade::Dim));
                        v.push(field(screen, FieldId::Title));
                    }
                    v.push(Line("Password (empty: none):".into(), Shade::Dim));
                    v.push(field(screen, FieldId::HostPassword));
                    v.push(Button(Act::Create, "Create Lobby".into()));
                    v.push(Button(Act::FriendlyFire, friendly_fire_label(state.friendly_fire)));
                    v.push(Button(Act::Emotes, "Emotes".into()));
                }
                Hosting::Opening => v.push(Line("Asking your router to let friends in...".into(), Shade::Bright)),
                Hosting::Open { code, note } => {
                    v.push(Line("Lobby code:".into(), Shade::Dim));
                    v.push(Big(code.clone()));
                    v.push(Line("Tell your friends this code. It is also in lobby-code.txt in the NVForever folder.".into(), Shade::Dim));
                    if let Some(n) = note {
                        v.push(Line(n.clone(), Shade::Bright));
                    }
                    match (state.public, state.locked) {
                        (true, true) => v.push(Line("Listed in the server browser, with a password.".into(), Shade::Dim)),
                        (true, false) => v.push(Line("Listed in the server browser.".into(), Shade::Dim)),
                        (false, true) => v.push(Line("Not listed; joining needs the code and the password.".into(), Shade::Dim)),
                        (false, false) => {}
                    }
                    for (peer, name) in &state.asking {
                        v.push(Line(format!("{name} wants to join."), Shade::Bright));
                        v.push(Button(Act::Admit(*peer), format!("Let {name} In")));
                        v.push(Button(Act::TurnAway(*peer), "Turn Away".into()));
                    }
                    players(&mut v, state);
                    if !state.guests.is_empty() {
                        v.push(Button(Act::Remove, "Remove a Player".into()));
                    }
                    v.push(Button(Act::Close, "Close Lobby".into()));
                    v.push(Button(Act::FriendlyFire, friendly_fire_label(state.friendly_fire)));
                    v.push(Button(Act::Emotes, "Emotes".into()));
                }
            }
        }
        Some(Mode::Join) => {
            v.push(Title("Join Lobby".into()));
            match (&state.joining, &screen.saves) {
                (Joining::Idle | Joining::Failed(_), _) if screen.browsing => {
                    browse_pieces(&mut v, screen, state);
                    if let Joining::Failed(why) = &state.joining {
                        v.push(Line(why.clone(), Shade::Bright));
                    }
                }
                (Joining::Idle | Joining::Failed(_), _) => {
                    v.push(Button(Act::Tab, "Browse Public Lobbies".into()));
                    v.push(Line("Type the host's lobby code:".into(), Shade::Bright));
                    v.push(field(screen, FieldId::Code));
                    v.push(Line("Password, if the lobby has one:".into(), Shade::Dim));
                    v.push(field(screen, FieldId::JoinPassword));
                    if let Joining::Failed(why) = &state.joining {
                        v.push(Line(why.clone(), Shade::Bright));
                    }
                    v.push(Button(Act::Join, "Join".into()));
                }
                (Joining::Finding, _) | (Joining::Found, None) => v.push(Line("Looking for the host...".into(), Shade::Bright)),
                (Joining::Found, Some(saves)) => {
                    v.push(Line("Found the host. Choose the save you play with:".into(), Shade::Bright));
                    if !saves.iter().any(|s| s.ok) {
                        v.push(Line("None of your saves is past Doc Mitchell's yet: play the opening first.".into(), Shade::Bright));
                    }
                    let list = acts(screen, state);
                    // A window of the rows round the chosen one.
                    let chosen_row = match list.get(screen.selected) {
                        Some(Act::Save(i)) => *i,
                        _ => 0,
                    };
                    let first = chosen_row.saturating_sub(SAVE_ROWS - 1).min(saves.len().saturating_sub(SAVE_ROWS));
                    for (i, s) in saves.iter().enumerate().skip(first).take(SAVE_ROWS) {
                        v.push(Row(Act::Save(i), s.line.clone(), if s.ok { Shade::Bright } else { Shade::Dim }));
                    }
                }
            }
        }
    }
    if screen.mode.is_some() {
        v.push(Button(Act::Back, "Back".into()));
    }
    v
}

/// The Browse tab: the filters, then the table of public lobbies, a window of
/// [`BROWSE_ROWS`] round the one chosen, the opened one's choices under it.
fn browse_pieces(v: &mut Vec<Piece>, screen: &LobbyScreen, state: &LobbyState) {
    use Piece::*;
    let tick = |on: bool| if on { "[x]" } else { "[ ]" };
    v.push(Button(Act::Tab, "Join by Code".into()));
    v.push(Button(Act::Refresh, "Refresh".into()));
    v.push(Row(Act::HideEmpty, format!("{} Hide empty lobbies", tick(screen.hide_empty)), Shade::Bright));
    v.push(Row(Act::HideFull, format!("{} Hide full lobbies", tick(screen.hide_full)), Shade::Bright));
    v.push(Row(Act::AllVersions, format!("{} Show lobbies of other versions and load orders", tick(screen.all_versions)), Shade::Bright));
    v.push(Line("Search:".into(), Shade::Dim));
    v.push(field(screen, FieldId::Search));
    let rows = shown(screen, state);
    match &state.browse {
        Browse::Idle | Browse::Loading => v.push(Line("Asking for the public lobbies...".into(), Shade::Bright)),
        Browse::Failed(why) => v.push(Line(why.clone(), Shade::Bright)),
        Browse::Ready(all) if rows.is_empty() => {
            let why = if all.is_empty() { "No public lobbies right now. Host one, or join a friend's by code." } else { "No public lobby matches the filters." };
            v.push(Line(why.into(), Shade::Bright));
        }
        Browse::Ready(all) => {
            v.push(Head(COLUMNS.iter().map(|(h, _)| h.to_string()).collect()));
            let list = acts(screen, state);
            let chosen_row = match list.get(screen.selected) {
                Some(Act::Lobby(i)) => *i,
                _ => rows.iter().position(|l| Some(&l.code) == screen.open_row.as_ref()).unwrap_or(0),
            };
            let first = chosen_row.saturating_sub(BROWSE_ROWS - 1).min(rows.len().saturating_sub(BROWSE_ROWS));
            for (i, l) in rows.iter().enumerate().skip(first).take(BROWSE_ROWS) {
                let ping = l.ping_ms.map_or_else(|| "?".to_string(), |p| format!("~{p} ms"));
                let cells = vec![
                    if l.favourite { "<3".into() } else { String::new() },
                    l.title.clone(),
                    l.place.clone(),
                    format!("{}/{}", l.players, l.max_players),
                    ping,
                    l.region.clone(),
                    if l.locked { "Lock".into() } else { String::new() },
                ];
                v.push(Cols(Act::Lobby(i), cells, if l.cannot.is_none() { Shade::Bright } else { Shade::Dim }));
                if screen.open_row.as_deref() == Some(l.code.as_str()) {
                    let ff = if l.friendly_fire { "on" } else { "off" };
                    v.push(Line(format!("Host: {}. Friendly fire {ff}. Ping is estimated from distance.", l.host_name), Shade::Dim));
                    match &l.cannot {
                        Some(why) => v.push(Line(why.clone(), Shade::Bright)),
                        None => {
                            if l.locked {
                                v.push(Line("Password:".into(), Shade::Dim));
                                v.push(field(screen, FieldId::RowPassword));
                            }
                            v.push(Button(Act::JoinRow, "Join".into()));
                        }
                    }
                    v.push(Button(Act::Favourite, if l.favourite { "Remove from Favourites".into() } else { "Add to Favourites".into() }));
                }
            }
            v.push(Line(format!("{} of {} public lobbies shown", rows.len(), all.len()), Shade::Dim));
        }
    }
}

/// The Emotes menu: the slots, or (`slot` chosen) the emotes for it by category, a window
/// of [`EMOTE_ROWS`] round the one chosen.
fn emote_pieces(v: &mut Vec<Piece>, screen: &LobbyScreen, slot: Option<usize>) {
    use crate::emotes::{Category, CATALOG};
    let name = |e: Option<usize>| e.map_or("Empty".to_string(), |e| CATALOG[e].name.to_string());
    v.push(Piece::Title("Emotes".into()));
    let Some(n) = slot else {
        v.push(Piece::Line("The emote wheel's slots (Emote held opens it). Choose a slot, then the emote to put in it.".into(), Shade::Bright));
        for (i, e) in screen.loadout.iter().enumerate() {
            v.push(Piece::Row(Act::Slot(i), format!("{}   {}", i + 1, name(*e)), Shade::Bright));
        }
        return;
    };
    v.push(Piece::Line(format!("Slot {}: {}. Choose its emote:", n + 1, name(screen.loadout[n])), Shade::Bright));
    // Every emote installed, under its category's heading.
    let mut rows: Vec<Piece> = Vec::new();
    for c in Category::ALL {
        let mut first = true;
        for &e in screen.installed.iter().filter(|&&e| CATALOG[e].category == c) {
            if std::mem::take(&mut first) {
                rows.push(Piece::Line(c.label().to_string(), Shade::Dim));
            }
            let held = screen.loadout.iter().position(|s| *s == Some(e)).map_or(String::new(), |k| format!("   (slot {})", k + 1));
            rows.push(Piece::Row(Act::Equip(e), format!("  {}{held}", CATALOG[e].name), Shade::Bright));
        }
    }
    rows.push(Piece::Row(Act::Empty, "  Empty this slot".into(), Shade::Bright));
    let list = acts(screen, &LobbyState::default());
    let chosen = list.get(screen.selected).copied();
    let at = rows.iter().position(|r| matches!(r, Piece::Row(a, ..) if Some(*a) == chosen)).unwrap_or(0);
    let first = at.saturating_sub(EMOTE_ROWS / 2).min(rows.len().saturating_sub(EMOTE_ROWS));
    let total = rows.len();
    // Where the list goes on past the window.
    let more = |n: usize, way: &str| Piece::Line(format!("  ({n} more {way}: Up and Down scroll)"), Shade::Dim);
    if first > 0 {
        v.push(more(first, "above"));
    }
    v.extend(rows.into_iter().skip(first).take(EMOTE_ROWS));
    if first + EMOTE_ROWS < total {
        v.push(more(total - first - EMOTE_ROWS, "below"));
    }
}

/// Who is in the lobby, one a line (the host and this player marked by the network side).
fn players(v: &mut Vec<Piece>, state: &LobbyState) {
    v.push(Piece::Line(format!("Players ({} of {LOBBY_PLAYERS}):", state.players.len()), Shade::Dim));
    for p in &state.players {
        v.push(Piece::Line(format!("  {p}"), Shade::Bright));
    }
}

/// The panel's backing: dark, fading out at its edges, no border [design: the owner's
/// pick for our own UI, 2026-11-06, mockup 1B; `crate::interface::soft_backing_image`].
const BACKING: f32 = 0.92;
const FEATHER: f32 = 30.0;

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    screen: Res<LobbyScreen>,
    state: Res<LobbyState>,
    look: Option<Res<LobbyLook>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    roots: Query<Entity, With<LobbyRoot>>,
    mut last: Local<Option<(Vec<Piece>, Option<Act>)>>,
) {
    let now = pieces(&screen, &state);
    let list = acts(&screen, &state);
    let chosen = list.get(screen.selected).copied();
    if last.as_ref().is_some_and(|(p, c)| *p == now && *c == chosen) {
        return;
    }
    *last = Some((now.clone(), chosen));
    for e in &roots {
        commands.entity(e).despawn();
    }
    if now.is_empty() {
        return;
    }
    let Some(look) = look else {
        let faces = Faces::load(data.as_deref(), &mut images, bc_supported(formats.as_deref()));
        commands.insert_resource(LobbyLook(faces));
        // Drawn next frame, with the fonts.
        *last = None;
        return;
    };
    let f = &look.0;
    let inner = WIDTH - 2.0 * PAD;
    // The pieces' heights.
    let height = |p: &Piece| match p {
        Piece::Title(_) => ITEM + 8.0,
        Piece::Line(t, _) => f.small.font.lines(t, Some(inner)).len().max(1) as f32 * STEP + 6.0,
        Piece::Big(_) => ITEM + 16.0,
        Piece::Field(..) | Piece::Button(..) => ITEM + 6.0,
        Piece::Row(..) | Piece::Head(_) | Piece::Cols(..) => ITEM,
    };
    let total: f32 = now.iter().map(height).sum::<f32>() + 2.0 * PAD;
    commands.spawn((LobbyRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, GlobalZIndex(120))).with_children(|root| {
        // The menus under it keep still (and take no pointer).
        root.spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::BLACK.with_alpha(0.55)), Button));
        let panel = Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            margin: UiRect::left(u(-WIDTH / 2.0)),
            top: u(TOP),
            width: u(WIDTH),
            height: u(total),
            ..default()
        };
        let backing = images.add(crate::interface::soft_backing_image(WIDTH, total, FEATHER, BACKING));
        root.spawn(panel).with_children(|p| {
            p.spawn((crate::interface::soft_backing_node(0.0, 0.0, WIDTH, total, FEATHER), ImageNode { image_mode: NodeImageMode::Stretch, ..ImageNode::new(backing) }));
            let mut y = PAD;
            for piece in &now {
                let h = height(piece);
                let at = |w: f32, h: f32| Node { position_type: PositionType::Absolute, left: u(PAD), top: u(y), width: u(w), height: u(h), ..default() };
                match piece {
                    Piece::Title(t) => {
                        p.spawn(at(inner, ITEM)).with_children(|n| text(n, &f.entry, t, 0.0, ITEM * 0.72, color(Shade::Bright)));
                    }
                    Piece::Line(t, shade) => {
                        p.spawn(at(inner, h)).with_children(|n| wrapped(n, &f.small, t, inner, STEP, color(*shade)));
                    }
                    Piece::Big(t) => {
                        let w = f.entry.font.width(t);
                        p.spawn(at(inner, ITEM + 8.0)).with_children(|n| text(n, &f.entry, t, ((inner - w) / 2.0).max(0.0), ITEM * 0.8, color(Shade::Bright)));
                    }
                    Piece::Field(act, t) => {
                        let on = Some(*act) == chosen;
                        let node = Node { border: UiRect::all(u(LINE)), ..at(inner, ITEM) };
                        let shown = if on { format!("{t}_") } else { t.clone() };
                        let mut e = if on { p.spawn(highlight(node)) } else { p.spawn(node) };
                        e.insert((BorderColor::all(amber(200.0)), Button, *act)).with_children(|n| text(n, &f.entry, &shown, 12.0, ITEM * 0.72, color(Shade::Bright)));
                    }
                    Piece::Head(cells) | Piece::Cols(_, cells, _) => {
                        let (shade, act) = match piece {
                            Piece::Cols(a, _, s) => (*s, Some(*a)),
                            _ => (Shade::Dim, None),
                        };
                        let node = at(inner, ITEM);
                        let mut e = if act.is_some() && act == chosen { p.spawn(highlight(node)) } else { p.spawn(node) };
                        if let Some(a) = act {
                            e.insert((Button, a));
                        }
                        e.with_children(|n| {
                            let mut x = 8.0;
                            for ((_, w), c) in COLUMNS.iter().zip(cells) {
                                let fit: String = c.chars().take((w / 11.0) as usize).collect();
                                n.spawn(Node { position_type: PositionType::Absolute, left: u(x), top: u(0.0), width: u(*w), height: u(ITEM), ..default() }).with_children(|m| text(m, &f.small, &fit, 0.0, ITEM * 0.66, color(shade)));
                                x += w;
                            }
                        });
                    }
                    Piece::Button(act, t) | Piece::Row(act, t, _) => {
                        let shade = if let Piece::Row(_, _, s) = piece { *s } else { Shade::Bright };
                        let (face, w) = if matches!(piece, Piece::Row(..)) { (&f.small, inner) } else { (&f.entry, 320.0) };
                        let node = at(w, ITEM);
                        let base = if matches!(piece, Piece::Row(..)) { ITEM * 0.66 } else { ITEM * 0.72 };
                        let mut e = if Some(*act) == chosen { p.spawn(highlight(node)) } else { p.spawn(node) };
                        // Rows that cannot join take no pointer.
                        if shade == Shade::Bright {
                            e.insert((Button, *act));
                        }
                        e.with_children(|n| text(n, face, t, 12.0, base, color(shade)));
                    }
                }
                y += h;
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened(mode: Mode) -> LobbyScreen {
        let mut s = LobbyScreen::default();
        s.open(mode);
        s
    }

    #[test]
    fn a_guest_sees_the_players_and_can_leave_not_create() {
        let screen = opened(Mode::Host);
        let state = LobbyState { players: vec!["Cass (you)".into(), "Veronica (host)".into()], guest: true, ..default() };
        assert_eq!(acts(&screen, &state), vec![Act::Leave, Act::Emotes, Act::Back]);
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Line("You are in Veronica's lobby.".into(), Shade::Bright)));
        assert!(p.contains(&Piece::Line("  Cass (you)".into(), Shade::Bright)) && p.contains(&Piece::Line("  Veronica (host)".into(), Shade::Bright)));
        assert!(p.contains(&Piece::Button(Act::Leave, "Leave Lobby".into())));
        assert!(!p.iter().any(|x| matches!(x, Piece::Button(Act::Create | Act::Close, _))), "nothing of the host's");
        // Left: the normal choice again.
        assert!(p.contains(&Piece::Line("Friendly fire: Off (the host's choice)".into(), Shade::Dim)));
        let alone = LobbyState::default();
        assert_eq!(acts(&screen, &alone), vec![Act::Visibility, Act::Field(FieldId::HostPassword), Act::Create, Act::FriendlyFire, Act::Emotes, Act::Back]);
    }

    #[test]
    fn the_host_lists_the_same_players_one_a_line() {
        let screen = opened(Mode::Host);
        let state = LobbyState { hosting: Hosting::Open { code: "ABCD-EFGH".into(), note: None }, players: vec!["Veronica (host, you)".into(), "Cass".into()], ..default() };
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Line("  Cass".into(), Shade::Bright)));
        assert_eq!(acts(&screen, &state), vec![Act::Close, Act::FriendlyFire, Act::Emotes, Act::Back]);
        assert!(p.contains(&Piece::Button(Act::FriendlyFire, "Friendly Fire: Off".into())));
        let on = LobbyState { friendly_fire: true, ..state };
        assert!(pieces(&screen, &on).contains(&Piece::Button(Act::FriendlyFire, "Friendly Fire: On".into())));
    }

    /// Join approval: a player asking to join is named, with a button to let it in and one
    /// to turn it away, first on the host's list.
    #[test]
    fn the_host_lets_a_player_asking_in_or_turns_it_away() {
        let screen = opened(Mode::Host);
        let state = LobbyState { hosting: Hosting::Open { code: "K7Q-M2X".into(), note: None }, players: vec!["Veronica (host, you)".into()], asking: vec![(4, "Cass".into())], ..default() };
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Line("Cass wants to join.".into(), Shade::Bright)));
        assert!(p.contains(&Piece::Button(Act::Admit(4), "Let Cass In".into())));
        assert!(p.contains(&Piece::Button(Act::TurnAway(4), "Turn Away".into())));
        assert_eq!(acts(&screen, &state), vec![Act::Admit(4), Act::TurnAway(4), Act::Close, Act::FriendlyFire, Act::Emotes, Act::Back]);
    }

    /// Kick and Ban: Remove a Player shows with guests in the lobby, and lists each with
    /// Kick and Ban; choosing one asks the network side and closes the list.
    #[test]
    fn the_host_removes_a_player() {
        let mut screen = opened(Mode::Host);
        let alone = LobbyState { hosting: Hosting::Open { code: "K7Q-M2X".into(), note: None }, players: vec!["Veronica (host, you)".into()], ..default() };
        assert!(!acts(&screen, &alone).contains(&Act::Remove), "nobody to remove");
        let state = LobbyState { players: vec!["Veronica (host, you)".into(), "Cass".into()], guests: vec![(1, "Cass".into())], ..alone };
        assert_eq!(acts(&screen, &state), vec![Act::Remove, Act::Close, Act::FriendlyFire, Act::Emotes, Act::Back]);
        screen.removing = true;
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Row(Act::Kick(1), "Kick Cass".into(), Shade::Bright)));
        assert!(p.contains(&Piece::Row(Act::Ban(1), "Ban Cass".into(), Shade::Bright)));
        assert_eq!(acts(&screen, &state), vec![Act::Kick(1), Act::Ban(1), Act::Back]);
    }

    #[test]
    fn a_full_lobby_of_six_lists_every_player_and_fits_the_screen() {
        // The lobby takes the host and `nvf_coop::net::MAX_GUESTS` (5) guests.
        let screen = opened(Mode::Host);
        let names = ["Veronica (host, you)", "Cass", "Boone", "Arcade", "Lily", "Raul"];
        let state = LobbyState { hosting: Hosting::Open { code: "ABCD-EFGH".into(), note: None }, players: names.iter().map(|n| n.to_string()).collect(), ..default() };
        let p = pieces(&screen, &state);
        for n in names {
            assert!(p.contains(&Piece::Line(format!("  {n}"), Shade::Bright)), "{n} listed");
        }
        assert!(p.contains(&Piece::Line("Players (6 of 6):".into(), Shade::Dim)));
        // The panel's height as `draw` measures it, a long line taken as three lines of
        // small print (the font is not loaded here): it stays inside the 960-unit screen
        // (at 540 px too, since interface units scale with the view's height).
        let height = |p: &Piece| match p {
            Piece::Title(_) => ITEM + 8.0,
            Piece::Line(t, _) => (if t.len() > 50 { 3.0 } else { 1.0 }) * STEP + 6.0,
            Piece::Big(_) => ITEM + 16.0,
            Piece::Field(..) | Piece::Button(..) => ITEM + 6.0,
            Piece::Row(..) | Piece::Head(_) | Piece::Cols(..) => ITEM,
        };
        let total: f32 = p.iter().map(height).sum::<f32>() + 2.0 * PAD;
        assert!(TOP + total <= 960.0, "the six-player lobby reaches {}", TOP + total);
    }

    fn lobby(code: &str, players: u8, ping: u32) -> PublicLobby {
        PublicLobby { code: code.into(), title: format!("{code}'s game"), host_name: "Host".into(), place: "Goodsprings".into(), players, max_players: 6, region: "OC/AU".into(), ping_ms: Some(ping), host_fp: format!("fp{code}"), ..default() }
    }

    /// The host's options: Private by default; Public shows the title field; the password
    /// is typed as stars and goes with Create, then is not kept.
    #[test]
    fn the_host_chooses_visibility_title_and_password() {
        let mut screen = opened(Mode::Host);
        let state = LobbyState::default();
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Button(Act::Visibility, "Visibility: Private".into())));
        assert!(!acts(&screen, &state).contains(&Act::Field(FieldId::Title)), "no title while private");
        screen.public = true;
        assert_eq!(acts(&screen, &state)[..3], [Act::Visibility, Act::Field(FieldId::Title), Act::Field(FieldId::HostPassword)]);
        for c in "Strip run".chars() {
            screen.type_char(FieldId::Title, c);
        }
        for c in "hunter2".chars() {
            screen.type_char(FieldId::HostPassword, c);
        }
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Field(Act::Field(FieldId::HostPassword), "*******".into())), "{p:?}");
        assert!(p.contains(&Piece::Field(Act::Field(FieldId::Title), "Strip run".into())));
        // The code field takes only a code's characters, upper case.
        for c in "k7q-m2x!".chars() {
            screen.type_char(FieldId::Code, c);
        }
        assert_eq!(screen.typed, "K7Q-M2X");
        let open = LobbyState { hosting: Hosting::Open { code: "K7Q-M2X".into(), note: None }, public: true, locked: true, ..default() };
        assert!(pieces(&screen, &open).contains(&Piece::Line("Listed in the server browser, with a password.".into(), Shade::Dim)));
    }

    /// The Browse tab: filters, favourites first then the nearest, a row opens to Join with a
    /// password box when locked; lobbies this game cannot join are hidden unless asked for.
    #[test]
    fn the_browse_tab_lists_filters_and_opens_lobbies() {
        let mut screen = opened(Mode::Join);
        screen.browsing = true;
        let mut far = lobby("FAR", 3, 180);
        far.favourite = true;
        let mut locked = lobby("LCK", 2, 20);
        locked.locked = true;
        let mut other = lobby("OLD", 2, 10);
        other.cannot = Some("The host runs another version of NVForever.".into());
        let state = LobbyState { browse: Browse::Ready(vec![lobby("EMP", 1, 5), lobby("NEAR", 4, 30), far, locked, other, lobby("FULL", 6, 40)]), ..default() };
        let codes = |s: &LobbyScreen| shown(s, &state).iter().map(|l| l.code.clone()).collect::<Vec<_>>();
        assert_eq!(codes(&screen), ["FAR", "LCK", "NEAR", "FULL"], "favourite first, then by ping; empty and other versions hidden");
        screen.hide_full = true;
        assert_eq!(codes(&screen), ["FAR", "LCK", "NEAR"]);
        screen.all_versions = true;
        assert!(codes(&screen).contains(&"OLD".to_string()));
        screen.all_versions = false;
        screen.search = "near".into();
        assert_eq!(codes(&screen), ["NEAR"]);
        screen.search.clear();
        // Open the locked one: its password field and Join follow its row.
        screen.open_row = Some("LCK".into());
        let a = acts(&screen, &state);
        let at = a.iter().position(|x| *x == Act::Lobby(1)).unwrap();
        assert_eq!(a[at + 1..at + 4], [Act::Field(FieldId::RowPassword), Act::JoinRow, Act::Favourite]);
        let p = pieces(&screen, &state);
        assert!(p.iter().any(|x| matches!(x, Piece::Cols(Act::Lobby(1), c, Shade::Bright) if c[6] == "Lock" && c[1] == "LCK's game")), "{p:?}");
        assert!(p.contains(&Piece::Button(Act::JoinRow, "Join".into())));
        // Nothing public: says so.
        let none = LobbyState { browse: Browse::Ready(Vec::new()), ..default() };
        assert!(pieces(&screen, &none).contains(&Piece::Line("No public lobbies right now. Host one, or join a friend's by code.".into(), Shade::Bright)));
    }

    #[test]
    fn the_emotes_menu_chooses_a_slot_then_its_emote() {
        use crate::emotes::{by_key, EmoteSlots, CATALOG};
        let mut screen = opened(Mode::Host);
        let mut loadout = EmoteSlots::default();
        screen.loadout = loadout.slots;
        screen.installed = (0..CATALOG.len()).collect();
        let state = LobbyState::default();
        screen.emotes = Some(None);
        assert_eq!(acts(&screen, &state).len(), 9);
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Row(Act::Slot(2), "3   Sit".into(), Shade::Bright)));
        let sit = by_key("sit").unwrap();
        screen.emotes = Some(Some(0));
        screen.selected = acts(&screen, &state).iter().position(|a| *a == Act::Equip(sit)).unwrap();
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Line("Sit and Lay".into(), Shade::Dim)), "{p:?}");
        assert!(p.contains(&Piece::Row(Act::Equip(sit), "  Sit   (slot 3)".into(), Shade::Bright)));
        assert!(p.len() <= 5 + EMOTE_ROWS);
        assert!(p.iter().any(|x| matches!(x, Piece::Line(t, Shade::Dim) if t.contains("more below"))), "{p:?}");
        // Slot 1 takes Sit: Sit's slot gets Wave.
        loadout.equip(0, sit);
        assert_eq!(loadout.slots[0], Some(sit));
        assert_eq!(loadout.slots[2], by_key("wave"));
    }
}
