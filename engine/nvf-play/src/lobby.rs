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
use crate::start_menu::{color, text, wrapped, Faces, Shade, ITEM};
use crate::ui_pointer::{Button, PointerState};
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use nvf_assets::GameData;
use std::path::PathBuf;

/// The screen's width and top, interface units.
/// The panel's narrowest and widest (interface units: the screen is 960 tall), and the share
/// of the screen's width it takes between them [design: the owner, 2026-11-25: "use more of
/// the screen"].
const MIN_WIDTH: f32 = 820.0;
const MAX_WIDTH: f32 = 1600.0;
const WIDTH_SHARE: f32 = 0.8;
const TOP: f32 = 50.0;
const PAD: f32 = 24.0;
/// Small print's line step.
const STEP: f32 = 30.0;
/// Saves shown at once.
const SAVE_ROWS: usize = 8;
/// Emotes and their categories' headings shown at once.
const EMOTE_ROWS: usize = 10;
/// Public lobbies shown at once on the Browse tab.
const BROWSE_ROWS: usize = 10;
/// The longest title, password or search typed (the service cuts titles at 40).
const MAX_TEXT: usize = 40;
/// The Browse table's columns: heading and width (interface units), adding up to the
/// panel's inner width.
const COLUMNS: [&str; 7] = ["Fav", "Name", "Location", "Players", "Ping", "Region", ""];

/// The Browse table's column widths for a panel `inner` wide: Fav, Players, Ping, Region and
/// the lock fixed, Name and Location sharing the rest (a title runs to 40 characters, a place
/// to 32).
fn column_widths(inner: f32) -> [f32; 7] {
    let fixed = [62.0, 104.0, 96.0, 110.0, 52.0];
    let rest = (inner - fixed.iter().sum::<f32>()).max(300.0);
    [fixed[0], rest * 0.56, rest * 0.44, fixed[1], fixed[2], fixed[3], fixed[4]]
}
/// A Browse cell drawn as a shape, not text: the favourite marker and the lock.
const FAV_MARK: &str = "\u{1}fav";
const LOCK_MARK: &str = "\u{1}lock";
/// The panel's and the boxes' colours, as the bug report form has them (nvforever
/// `reports/form.rs`, the owner's model for these screens, 2026-11-25): a dim panel border,
/// faint box borders, bright and filled when chosen.
const BRIGHT: f32 = 255.0;
const DIM: f32 = 170.0;
const FAINT: f32 = 90.0;
const FILL: f32 = 40.0;
/// A chosen option (a toggle on, the tab shown, the main action): a solid amber fill with
/// dark words, so it reads apart from the cursor, which is a bright border.
const SOLID: f32 = 200.0;
/// The words on a solid fill.
fn dark() -> Color {
    Color::srgb(0.04, 0.03, 0.01)
}
/// A warning (Close Lobby, Leave Lobby, Ban): red, not amber.
fn warn_red(bright: bool) -> Color {
    if bright {
        Color::srgb(1.0, 0.38, 0.28)
    } else {
        Color::srgb(0.78, 0.26, 0.2)
    }
}
/// The Join screen's panel keeps one height whichever tab is up (the Browse tab's).
const JOIN_HEIGHT: f32 = ITEM + 8.0 + 3.0 * (ITEM + 8.0) + ITEM + BROWSE_ROWS as f32 * (ITEM - 2.0) + (ITEM + 8.0) + 2.0 * PAD;
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
    /// A guest waits for the host to let it in (join approval).
    pub awaiting_host: bool,
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
        self.browsing = false;
        self.open_row = None;
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
    /// Remove this guest (its id); it may join again.
    Kick(u8),
    /// Remove it and keep it out while the lobby lasts.
    Ban(u8),
    /// A text field, chosen to type in.
    Field(FieldId),
    /// The host's Private (false) or Public (true).
    SetPublic(bool),
    /// The Join screen's tab: Code (false) or Browse (true).
    ShowTab(bool),
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

/// The buttons, fields and rows that can be chosen, in the order the screen shows them (left
/// to right, top to bottom): the arrow keys walk them in this order.
fn acts(screen: &LobbyScreen, state: &LobbyState) -> Vec<Act> {
    match (screen.mode, screen.emotes) {
        (Some(Mode::Host), Some(None)) => return (0..crate::emotes::SLOTS).map(Act::Slot).chain([Act::Back]).collect(),
        (Some(Mode::Host), Some(Some(_))) => return screen.installed.iter().map(|&e| Act::Equip(e)).chain([Act::Empty, Act::Back]).collect(),
        _ => {}
    }
    match screen.mode {
        None => Vec::new(),
        Some(Mode::Host) if state.guest => vec![Act::Emotes, Act::Back, Act::Leave],
        Some(Mode::Host) => match state.hosting {
            Hosting::Off => [Act::SetPublic(false), Act::SetPublic(true)]
                .into_iter()
                .chain(screen.public.then_some(Act::Field(FieldId::Title)))
                .chain([Act::Field(FieldId::HostPassword), Act::FriendlyFire, Act::Emotes, Act::Back, Act::Create])
                .collect(),
            Hosting::Opening => vec![Act::Back],
            Hosting::Open { .. } => state
                .asking
                .iter()
                .flat_map(|(p, _)| [Act::Admit(*p), Act::TurnAway(*p)])
                .chain(state.guests.iter().flat_map(|(id, _)| [Act::Kick(*id), Act::Ban(*id)]))
                .chain([Act::FriendlyFire, Act::Emotes, Act::Back, Act::Close])
                .collect(),
        },
        Some(Mode::Join) => match (&state.joining, &screen.saves) {
            (Joining::Found, Some(saves)) => saves.iter().enumerate().filter(|(_, s)| s.ok).map(|(i, _)| Act::Save(i)).chain([Act::Back]).collect(),
            (Joining::Finding | Joining::Found, _) => vec![Act::Back],
            (Joining::Idle | Joining::Failed(_), _) if screen.browsing => browse_acts(screen, state),
            (Joining::Idle | Joining::Failed(_), _) => vec![Act::ShowTab(false), Act::ShowTab(true), Act::Field(FieldId::Code), Act::Field(FieldId::JoinPassword), Act::Back, Act::Join],
        },
    }
}

/// The Browse tab's choices: the tabs, the toolbar, then each row shown (the opened one's
/// own choices under it), then Back.
fn browse_acts(screen: &LobbyScreen, state: &LobbyState) -> Vec<Act> {
    let mut v = vec![Act::ShowTab(false), Act::ShowTab(true), Act::Refresh, Act::HideEmpty, Act::HideFull, Act::AllVersions, Act::Field(FieldId::Search)];
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
                "public" => Some(Act::SetPublic(true)),
                "browse" => {
                    if screen.mode.is_none() {
                        screen.open(Mode::Join);
                    }
                    Some(Act::ShowTab(true))
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
        Act::Kick(id) | Act::Ban(id) => {
            asks.write(LobbyRequest::Kick { id, ban: matches!(act, Act::Ban(_)) });
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
        Act::SetPublic(on) => screen.public = on,
        Act::ShowTab(browse) => {
            if screen.browsing != browse {
                screen.browsing = browse;
                screen.open_row = None;
            }
            if browse {
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
    /// The lobby code, large, in its own box.
    Big(String),
    Button(Act, String),
    Row(Act, String, Shade),
    /// The Browse table's heading, and a row of it: one text per column ([`COLUMNS`]).
    Head(Vec<String>),
    Cols(Act, Vec<String>, Shade),
    /// Items side by side on one line ([`Item`]); a [`Item::Gap`] sends the rest to the right.
    Bar(Vec<Item>),
    /// Empty height (the Browse table's rows not filled, so the footer stays put).
    Space(f32),
    /// Small print indented from the left edge, every wrapped line too.
    Note(String, Shade),
    /// The Browse table's scrollbar: the share of the rows above the window and in it, and
    /// the height it spans (from here down). Takes no height itself.
    Scroll(f32, f32, f32),
}

/// One item of a [`Piece::Bar`].
#[derive(Debug, Clone, PartialEq)]
enum Item {
    /// A boxed button.
    Button(Act, String, Look),
    /// A field this wide (0: the room left on the line).
    Field(Act, String, f32),
    /// Plain words, this wide (0: as wide as the words): a label dim, other words bright.
    Text(String, f32, Shade),
    /// The rest of the line's items go to its right end.
    Gap,
}

/// How a boxed button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    Normal,
    /// A toggle that is on, the tab shown, the segment chosen: filled.
    On,
    /// The screen's main action (Create Lobby, Join): filled and bright.
    Primary,
    /// Ends something (Close Lobby, Leave Lobby): its words bright, its box brighter.
    Warning,
}

/// A field's text as shown: a password as stars.
fn field_text(screen: &LobbyScreen, f: FieldId) -> String {
    let t = screen.text(f);
    match f {
        FieldId::JoinPassword | FieldId::HostPassword | FieldId::RowPassword => "*".repeat(t.chars().count()),
        _ => t.to_string(),
    }
}

/// A labelled field on a line: the label, then the field to the line's end.
fn labelled(screen: &LobbyScreen, label: &str, f: FieldId) -> Piece {
    Piece::Bar(vec![Item::Text(label.into(), LABEL, Shade::Dim), Item::Field(Act::Field(f), field_text(screen, f), 0.0)])
}

/// A labelled field's label width.
const LABEL: f32 = 230.0;

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

fn look(on: bool) -> Look {
    if on {
        Look::On
    } else {
        Look::Normal
    }
}

fn pieces(screen: &LobbyScreen, state: &LobbyState) -> Vec<Piece> {
    use Piece::*;
    let mut v = Vec::new();
    if let (Some(Mode::Host), Some(slot)) = (screen.mode, screen.emotes) {
        emote_pieces(&mut v, screen, slot);
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
            v.push(Bar(vec![Item::Button(Act::Emotes, "Emotes".into(), Look::Normal), Item::Gap, Item::Button(Act::Back, "Back".into(), Look::Normal), Item::Button(Act::Leave, "Leave Lobby".into(), Look::Warning)]));
        }
        Some(Mode::Host) => {
            v.push(Title("Co-op Lobby".into()));
            match &state.hosting {
                Hosting::Off => {
                    v.push(Line("Open a lobby on this game: friends join with its code, from their main menu's Join Lobby.".into(), Shade::Bright));
                    v.push(Bar(vec![
                        Item::Text("Visibility:".into(), LABEL, Shade::Dim),
                        Item::Button(Act::SetPublic(false), "Private".into(), look(!screen.public)),
                        Item::Button(Act::SetPublic(true), "Public".into(), look(screen.public)),
                    ]));
                    if screen.public {
                        v.push(labelled(screen, "Title:", FieldId::Title));
                    }
                    v.push(labelled(screen, "Password:", FieldId::HostPassword));
                    v.push(Line(if screen.public { "Listed in the server browser; empty title: your name's game. Empty password: none." } else { "Not listed: friends join with the code. Empty password: none." }.into(), Shade::Dim));
                    v.push(Bar(vec![Item::Text("Options:".into(), LABEL, Shade::Dim), Item::Button(Act::FriendlyFire, friendly_fire_label(state.friendly_fire), look(state.friendly_fire))]));
                    v.push(Bar(vec![Item::Button(Act::Emotes, "Emotes".into(), Look::Normal), Item::Button(Act::Back, "Back".into(), Look::Normal), Item::Gap, Item::Button(Act::Create, "Create Lobby".into(), Look::Primary)]));
                }
                Hosting::Opening => {
                    v.push(Line("Asking your router to let friends in...".into(), Shade::Bright));
                    v.push(Bar(vec![Item::Gap, Item::Button(Act::Back, "Back".into(), Look::Normal)]));
                }
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
                        v.push(Bar(vec![
                            Item::Text(format!("{name} wants to join."), 0.0, Shade::Bright),
                            Item::Gap,
                            Item::Button(Act::Admit(*peer), "Let In".into(), Look::Primary),
                            Item::Button(Act::TurnAway(*peer), "Turn Away".into(), Look::Normal),
                        ]));
                    }
                    v.push(Line(format!("Players ({} of {LOBBY_PLAYERS}):", state.players.len()), Shade::Dim));
                    if let Some(me) = state.players.first() {
                        v.push(Line(format!("  {me}"), Shade::Bright));
                    }
                    for (id, line) in &state.guests {
                        v.push(Bar(vec![
                            Item::Text(format!("  {line}"), 0.0, Shade::Bright),
                            Item::Gap,
                            Item::Button(Act::Kick(*id), "Kick".into(), Look::Normal),
                            Item::Button(Act::Ban(*id), "Ban".into(), Look::Warning),
                        ]));
                    }
                    v.push(Bar(vec![
                        Item::Button(Act::FriendlyFire, friendly_fire_label(state.friendly_fire), look(state.friendly_fire)),
                        Item::Button(Act::Emotes, "Emotes".into(), Look::Normal),
                        Item::Gap,
                        Item::Button(Act::Back, "Back".into(), Look::Normal),
                        Item::Button(Act::Close, "Close Lobby".into(), Look::Warning),
                    ]));
                }
            }
        }
        Some(Mode::Join) => {
            v.push(Title("Join Lobby".into()));
            match (&state.joining, &screen.saves) {
                (Joining::Idle | Joining::Failed(_), _) => {
                    v.push(Bar(vec![Item::Button(Act::ShowTab(false), "Join by Code".into(), look(!screen.browsing)), Item::Button(Act::ShowTab(true), "Browse Public Lobbies".into(), look(screen.browsing))]));
                    if screen.browsing {
                        browse_pieces(&mut v, screen, state);
                    } else {
                        v.push(labelled(screen, "Lobby code:", FieldId::Code));
                        v.push(labelled(screen, "Password (if any):", FieldId::JoinPassword));
                        if let Joining::Failed(why) = &state.joining {
                            v.push(Line(why.clone(), Shade::Bright));
                        }
                        v.push(Bar(vec![Item::Button(Act::Back, "Back".into(), Look::Normal), Item::Gap, Item::Button(Act::Join, "Join".into(), Look::Primary)]));
                    }
                }
                (Joining::Finding, _) | (Joining::Found, None) => {
                    v.push(Line(if state.awaiting_host { "The host has been asked to let you in. Waiting for their answer (up to two minutes)..." } else { "Looking for the host..." }.into(), Shade::Bright));
                    v.push(Bar(vec![Item::Button(Act::Back, "Back".into(), Look::Normal)]));
                }
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
                    v.push(Bar(vec![Item::Button(Act::Back, "Back".into(), Look::Normal)]));
                }
            }
        }
    }
    v
}

/// The Browse tab: a toolbar, the table of public lobbies in a fixed area of
/// [`BROWSE_ROWS`] rows (a window round the one chosen; the opened one's details and choices
/// under it), then the footer.
fn browse_pieces(v: &mut Vec<Piece>, screen: &LobbyScreen, state: &LobbyState) {
    use Piece::*;
    v.push(Bar(vec![
        Item::Button(Act::Refresh, "Refresh".into(), Look::Normal),
        Item::Button(Act::HideEmpty, "Hide Empty".into(), look(screen.hide_empty)),
        Item::Button(Act::HideFull, "Hide Full".into(), look(screen.hide_full)),
        Item::Button(Act::AllVersions, "Other Versions".into(), look(screen.all_versions)),
    ]));
    v.push(Bar(vec![Item::Text("Search:".into(), 110.0, Shade::Dim), Item::Field(Act::Field(FieldId::Search), field_text(screen, FieldId::Search), 0.0)]));
    let rows = shown(screen, state);
    // The table's area: the heading and BROWSE_ROWS rows, filled or not, so the footer
    // stays put (the opened row's details come out of it).
    let area = ITEM + BROWSE_ROWS as f32 * ROW_H;
    let mut used = 0.0;
    let mut window = (0, 0);
    let all = match &state.browse {
        Browse::Idle | Browse::Loading => {
            v.push(Line("Asking for the public lobbies...".into(), Shade::Bright));
            used += line_h(1);
            0
        }
        Browse::Failed(why) => {
            v.push(Line(why.clone(), Shade::Bright));
            used += line_h(2);
            0
        }
        Browse::Ready(all) if rows.is_empty() => {
            let why = if all.is_empty() { "No public lobbies right now. Host one, or join a friend's by code." } else { "No public lobby matches the filters." };
            v.push(Line(why.into(), Shade::Bright));
            used += line_h(1);
            all.len()
        }
        Browse::Ready(all) => {
            v.push(Head(COLUMNS.iter().map(|h| h.to_string()).collect()));
            used += ITEM;
            let list = acts(screen, state);
            let chosen_row = match list.get(screen.selected) {
                Some(Act::Lobby(i)) => *i,
                _ => rows.iter().position(|l| Some(&l.code) == screen.open_row.as_ref()).unwrap_or(0),
            };
            // An opened row takes the room of three rows below it.
            let room = if screen.open_row.is_some() { BROWSE_ROWS - 3 } else { BROWSE_ROWS };
            let first = chosen_row.saturating_sub(room - 1).min(rows.len().saturating_sub(room));
            window = (first, (first + room).min(rows.len()));
            if rows.len() > room {
                v.push(Scroll(first as f32 / rows.len() as f32, room as f32 / rows.len() as f32, BROWSE_ROWS as f32 * ROW_H));
            }
            for (i, l) in rows.iter().enumerate().skip(first).take(room) {
                let ping = l.ping_ms.map_or_else(|| "?".to_string(), |p| format!("~{p} ms"));
                let cells = vec![
                    if l.favourite { FAV_MARK.into() } else { String::new() },
                    l.title.clone(),
                    l.place.clone(),
                    format!("{}/{}", l.players, l.max_players),
                    ping,
                    l.region.clone(),
                    if l.locked { LOCK_MARK.into() } else { String::new() },
                ];
                v.push(Cols(Act::Lobby(i), cells, if l.cannot.is_none() { Shade::Bright } else { Shade::Dim }));
                used += ROW_H;
                if screen.open_row.as_deref() == Some(l.code.as_str()) {
                    let ff = if l.friendly_fire { "on" } else { "off" };
                    v.push(Note(format!("Host: {}. Friendly fire {ff}. Ping is estimated from distance.", l.host_name), Shade::Dim));
                    used += line_h(1);
                    let mut bar = Vec::new();
                    match &l.cannot {
                        Some(why) => {
                            bar.push(Item::Text(String::new(), INDENT - 10.0, Shade::Dim));
                            bar.push(Item::Text(why.clone(), 0.0, Shade::Bright));
                        }
                        None if l.locked => {
                            bar.push(Item::Text(String::new(), INDENT - 10.0, Shade::Dim));
                            bar.push(Item::Text("Password:".into(), 120.0, Shade::Dim));
                            bar.push(Item::Field(Act::Field(FieldId::RowPassword), field_text(screen, FieldId::RowPassword), 220.0));
                        }
                        None => {}
                    }
                    bar.push(Item::Gap);
                    if l.cannot.is_none() {
                        bar.push(Item::Button(Act::JoinRow, "Join".into(), Look::Primary));
                    }
                    bar.push(Item::Button(Act::Favourite, if l.favourite { "Unfavourite".into() } else { "Favourite".into() }, Look::Normal));
                    v.push(Bar(bar));
                    used += BAR_H;
                    // A join from this row that failed (a wrong password): said here.
                    if let Joining::Failed(why) = &state.joining {
                        v.push(Note(why.clone(), Shade::Bright));
                        used += line_h(1);
                    }
                }
            }
            all.len()
        }
    };
    if used < area {
        v.push(Space(area - used));
    }
    let tail = match (rows.len(), window) {
        (0, _) if all > 0 => format!("None of {all} shown (the filters)"),
        (0, _) => String::new(),
        (n, (a, b)) if b - a < n => format!("Lobbies {}-{b} of {n} (the arrows scroll){}", a + 1, if n < all { format!(", {all} in all") } else { String::new() }),
        (n, _) => format!("{n} of {all} shown"),
    };
    v.push(Bar(vec![Item::Button(Act::Back, "Back".into(), Look::Normal), Item::Gap, Item::Text(tail, 0.0, Shade::Dim)]));
}

/// The heights of a Browse row, a bar and a line of small print (`n` lines).
const ROW_H: f32 = ITEM - 2.0;
/// How far a [`Piece::Note`] sits in from the left.
const INDENT: f32 = 40.0;
const BAR_H: f32 = ITEM + 8.0;
fn line_h(n: usize) -> f32 {
    n as f32 * STEP + 6.0
}

/// `t` cut to `width` (interface units) with "..", as the font measures it.
fn fit(face: &crate::interface::Face, t: &str, width: f32) -> String {
    if face.font.width(t) <= width {
        return t.to_string();
    }
    let mut cut: String = t.to_string();
    while !cut.is_empty() && face.font.width(&format!("{cut}..")) > width {
        cut.pop();
    }
    format!("{}..", cut.trim_end())
}

/// `t` cut from the front to `width`: a field being typed in shows its end.
fn fit_end(face: &crate::interface::Face, t: &str, width: f32) -> String {
    let mut cut = t.to_string();
    while !cut.is_empty() && face.font.width(&cut) > width {
        cut.remove(0);
    }
    cut
}

/// A Browse cell's shape.
#[derive(Clone, Copy)]
enum Mark {
    Favourite,
    Lock,
}

/// The favourite marker (a filled diamond) or the lock (a body under a shackle), drawn as
/// boxes: the menu fonts may have no such glyphs.
fn marker(m: &mut ChildSpawnerCommands, mark: Mark, c: Color) {
    match mark {
        Mark::Favourite => {
            m.spawn((Node { position_type: PositionType::Absolute, left: u(10.0), top: u(10.0), width: u(14.0), height: u(14.0), ..default() }, BackgroundColor(c), UiTransform::from_rotation(Rot2::degrees(45.0))));
        }
        Mark::Lock => {
            m.spawn((Node { position_type: PositionType::Absolute, left: u(13.0), top: u(5.0), width: u(12.0), height: u(10.0), border: UiRect::all(Val::Px(2.0)), ..default() }, BorderColor::all(c)));
            m.spawn((Node { position_type: PositionType::Absolute, left: u(9.0), top: u(14.0), width: u(20.0), height: u(14.0), ..default() }, BackgroundColor(c)));
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


/// What was drawn last: the pieces, the choice, the panel's width.
type Drawn = (Vec<Piece>, Option<Act>, u32);

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
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut last: Local<Option<Drawn>>,
) {
    let now = pieces(&screen, &state);
    let list = acts(&screen, &state);
    let chosen = list.get(screen.selected).copied();
    // The panel's width follows the window's shape: interface units are 960 to its height.
    let screen_units = windows.single().ok().map_or(1280.0, |w| 960.0 * w.width() / w.height().max(1.0));
    let width = (screen_units * WIDTH_SHARE).clamp(MIN_WIDTH, MAX_WIDTH.min(screen_units - 40.0).max(MIN_WIDTH));
    if last.as_ref().is_some_and(|(p, c, w)| *p == now && *c == chosen && *w == width as u32) {
        return;
    }
    *last = Some((now.clone(), chosen, width as u32));
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
    let inner = width - 2.0 * PAD;
    let columns = column_widths(inner);
    // The pieces' heights.
    let height = |p: &Piece| match p {
        Piece::Title(_) => ITEM + 8.0,
        Piece::Line(t, _) => f.small.font.lines(t, Some(inner)).len().max(1) as f32 * STEP + 6.0,
        Piece::Big(_) => ITEM + 24.0,
        Piece::Button(..) => ITEM + 6.0,
        Piece::Row(..) | Piece::Head(_) => ITEM,
        Piece::Cols(..) => ROW_H,
        Piece::Bar(_) => BAR_H,
        Piece::Space(h) => *h,
        Piece::Note(t, _) => f.small.font.lines(t, Some(inner - INDENT)).len().max(1) as f32 * STEP + 6.0,
        Piece::Scroll(..) => 0.0,
    };
    let mut total: f32 = now.iter().map(height).sum::<f32>() + 2.0 * PAD;
    if screen.mode == Some(Mode::Join) && matches!(state.joining, Joining::Idle | Joining::Failed(_)) {
        total = total.max(JOIN_HEIGHT);
    }
    commands.spawn((LobbyRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, GlobalZIndex(120))).with_children(|root| {
        // The menus under it keep still (and take no pointer).
        root.spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::BLACK.with_alpha(0.85)), Button));
        let panel = Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            margin: UiRect::left(u(-width / 2.0)),
            top: u(TOP),
            width: u(width),
            height: u(total),
            ..default()
        };
        let panel = Node { border: UiRect::all(Val::Px(2.0)), ..panel };
        root.spawn((panel, crate::pause::form_panel(), BorderColor::all(amber(DIM)))).with_children(|p| {
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
                        let bw = (w + 80.0).min(inner);
                        let node = Node { position_type: PositionType::Absolute, left: u(PAD + (inner - bw) / 2.0), top: u(y), width: u(bw), height: u(ITEM + 16.0), border: UiRect::all(Val::Px(1.0)), ..default() };
                        p.spawn((node, BorderColor::all(amber(DIM)))).with_children(|n| text(n, &f.entry, t, 40.0, ITEM * 0.85, color(Shade::Bright)));
                    }
                    Piece::Space(_) => {}
                    Piece::Note(t, shade) => {
                        let node = Node { position_type: PositionType::Absolute, left: u(PAD + INDENT), top: u(y), width: u(inner - INDENT), height: u(h), ..default() };
                        p.spawn(node).with_children(|n| wrapped(n, &f.small, t, inner - INDENT, STEP, color(*shade)));
                    }
                    Piece::Scroll(from, len, span) => {
                        // A track at the table's right edge, its thumb where the window is.
                        let x = PAD + inner + 6.0;
                        p.spawn((Node { position_type: PositionType::Absolute, left: u(x), top: u(y), width: u(6.0), height: u(*span), ..default() }, BackgroundColor(amber(FAINT * 0.5))));
                        p.spawn((Node { position_type: PositionType::Absolute, left: u(x), top: u(y + span * from), width: u(6.0), height: u((span * len).max(12.0)), ..default() }, BackgroundColor(amber(DIM))));
                    }
                    Piece::Bar(items) => {
                        // The widths: buttons as wide as their words, a field 0 wide takes the
                        // room left; after a Gap the items sit at the line's right end.
                        let gap = 10.0;
                        let width = |it: &Item| match it {
                            Item::Button(_, t, _) => f.small.font.width(t) + 30.0,
                            Item::Field(_, _, w) => *w,
                            Item::Text(t, w, _) => if *w > 0.0 { *w } else { f.small.font.width(t) + 8.0 },
                            Item::Gap => 0.0,
                        };
                        let fixed: f32 = items.iter().map(width).sum::<f32>() + gap * items.len().saturating_sub(1) as f32;
                        let free = (inner - fixed).max(0.0);
                        let fills = items.iter().filter(|i| matches!(i, Item::Field(_, _, w) if *w == 0.0)).count();
                        let fill = if fills > 0 { free / fills as f32 } else { 0.0 };
                        let gap_at = items.iter().position(|i| *i == Item::Gap);
                        let right: f32 = gap_at.map_or(0.0, |g| items[g + 1..].iter().map(|i| width(i) + gap).sum::<f32>());
                        let mut x = 0.0;
                        for (k, it) in items.iter().enumerate() {
                            if Some(k) == gap_at {
                                x = (inner - right + gap).max(x);
                                continue;
                            }
                            let w = match it {
                                Item::Field(_, _, w) if *w == 0.0 => fill,
                                _ => width(it),
                            };
                            let at = |w: f32| Node { position_type: PositionType::Absolute, left: u(PAD + x), top: u(y + 2.0), width: u(w), height: u(ITEM - 4.0), ..default() };
                            match it {
                                Item::Text(t, _, shade) => {
                                    let fitted = fit(&f.small, t, w);
                                    p.spawn(at(w)).with_children(|n| text(n, &f.small, &fitted, 0.0, ITEM * 0.62, color(*shade)));
                                }
                                Item::Field(act, t, _) => {
                                    let on = Some(*act) == chosen;
                                    let shown = if on { format!("{t}_") } else { t.clone() };
                                    let shown = fit_end(&f.small, &shown, w - 20.0);
                                    let node = Node { border: UiRect::all(Val::Px(if on { 2.0 } else { 1.0 })), overflow: Overflow::clip(), ..at(w) };
                                    p.spawn((node, BorderColor::all(amber(if on { BRIGHT } else { FAINT })), Button, *act)).with_children(|n| text(n, &f.small, &shown, 10.0, ITEM * 0.62, color(Shade::Bright)));
                                }
                                Item::Button(act, t, look) => {
                                    // The cursor: a bright 2 px border. Chosen (On, Primary): a
                                    // solid fill with dark words. A warning: red.
                                    let cursor = Some(*act) == chosen;
                                    let (border, fill, words) = match look {
                                        Look::Normal => (amber(if cursor { BRIGHT } else { FAINT }), cursor.then_some(amber(FILL)), color(Shade::Bright)),
                                        Look::On | Look::Primary => (amber(if cursor { BRIGHT } else { SOLID }), Some(amber(SOLID)), dark()),
                                        Look::Warning => (warn_red(cursor), cursor.then_some(warn_red(false).with_alpha(0.25)), warn_red(true)),
                                    };
                                    let node = Node { border: UiRect::all(Val::Px(if cursor { 2.0 } else { 1.0 })), ..at(w) };
                                    let mut e = p.spawn((node, BorderColor::all(border), Button, *act));
                                    if let Some(c) = fill {
                                        e.insert(BackgroundColor(c));
                                    }
                                    e.with_children(|n| text(n, &f.small, t, 15.0, ITEM * 0.62, words));
                                }
                                Item::Gap => {}
                            }
                            x += w + gap;
                        }
                    }
                    Piece::Head(cells) | Piece::Cols(_, cells, _) => {
                        let (shade, act) = match piece {
                            Piece::Cols(a, _, s) => (*s, Some(*a)),
                            _ => (Shade::Dim, None),
                        };
                        let on = act.is_some() && act == chosen;
                        let node = Node { border: UiRect::all(Val::Px(if act.is_some() { 1.0 } else { 0.0 })), ..at(inner, ITEM - 4.0) };
                        let mut e = p.spawn((node, BorderColor::all(amber(if on { BRIGHT } else { FAINT }))));
                        if on {
                            e.insert(BackgroundColor(amber(FILL)));
                        }
                        if let Some(a) = act {
                            e.insert((Button, a));
                        }
                        e.with_children(|n| {
                            let mut x = 8.0;
                            for (w, c) in columns.iter().zip(cells) {
                                let cell = Node { position_type: PositionType::Absolute, left: u(x), top: u(0.0), width: u(*w - 8.0), height: u(ITEM - 6.0), overflow: Overflow::clip(), ..default() };
                                match c.as_str() {
                                    FAV_MARK => {
                                        n.spawn(cell).with_children(|m| marker(m, Mark::Favourite, color(shade)));
                                    }
                                    LOCK_MARK => {
                                        n.spawn(cell).with_children(|m| marker(m, Mark::Lock, color(shade)));
                                    }
                                    _ => {
                                        let fitted = fit(&f.small, c, *w - 12.0);
                                        n.spawn(cell).with_children(|m| text(m, &f.small, &fitted, 0.0, ITEM * 0.62, color(shade)));
                                    }
                                }
                                x += w;
                            }
                        });
                    }
                    Piece::Button(act, t) | Piece::Row(act, t, _) => {
                        let shade = if let Piece::Row(_, _, s) = piece { *s } else { Shade::Bright };
                        let row = matches!(piece, Piece::Row(..));
                        // A button is a box as wide as its words; a row spans the panel.
                        let face = if row { &f.small } else { &f.entry };
                        let w = if row { inner } else { (f.entry.font.width(t) + 28.0).clamp(120.0, inner) };
                        let node = Node { border: UiRect::all(Val::Px(1.0)), ..at(w, ITEM - 4.0) };
                        let base = if row { ITEM * 0.62 } else { ITEM * 0.68 };
                        let on = Some(*act) == chosen;
                        let mut e = p.spawn((node, BorderColor::all(amber(if on { BRIGHT } else { FAINT }))));
                        if on {
                            e.insert(BackgroundColor(amber(FILL)));
                        }
                        // Rows that cannot join take no pointer.
                        if shade == Shade::Bright {
                            e.insert((Button, *act));
                        }
                        e.with_children(|n| text(n, face, t, 14.0, base, color(shade)));
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
        assert_eq!(acts(&screen, &state), vec![Act::Emotes, Act::Back, Act::Leave]);
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Line("You are in Veronica's lobby.".into(), Shade::Bright)));
        assert!(p.contains(&Piece::Line("  Cass (you)".into(), Shade::Bright)) && p.contains(&Piece::Line("  Veronica (host)".into(), Shade::Bright)));
        assert!(bar_has(&p, Act::Leave));
        assert!(!bar_has(&p, Act::Create) && !bar_has(&p, Act::Close), "nothing of the host's");
        // Left: the normal choice again.
        assert!(p.contains(&Piece::Line("Friendly fire: Off (the host's choice)".into(), Shade::Dim)));
        let alone = LobbyState::default();
        assert_eq!(acts(&screen, &alone), vec![Act::SetPublic(false), Act::SetPublic(true), Act::Field(FieldId::HostPassword), Act::FriendlyFire, Act::Emotes, Act::Back, Act::Create]);
    }

    #[test]
    fn the_host_lists_the_same_players_one_a_line() {
        let screen = opened(Mode::Host);
        let state = LobbyState { hosting: Hosting::Open { code: "ABCD-EFGH".into(), note: None }, players: vec!["Veronica (host, you)".into(), "Cass".into()], ..default() };
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Line("  Veronica (host, you)".into(), Shade::Bright)));
        assert_eq!(acts(&screen, &state), vec![Act::FriendlyFire, Act::Emotes, Act::Back, Act::Close]);
        assert!(bar_item(&p, Act::FriendlyFire).is_some_and(|i| matches!(i, Item::Button(_, t, Look::Normal) if t == "Friendly Fire: Off")));
        let on = LobbyState { friendly_fire: true, ..state };
        assert!(bar_item(&pieces(&screen, &on), Act::FriendlyFire).is_some_and(|i| matches!(i, Item::Button(_, t, Look::On) if t == "Friendly Fire: On")));
    }

    /// Join approval: a player asking to join is named, with a button to let it in and one
    /// to turn it away, first on the host's list.
    #[test]
    fn the_host_lets_a_player_asking_in_or_turns_it_away() {
        let screen = opened(Mode::Host);
        let state = LobbyState { hosting: Hosting::Open { code: "K7Q-M2X".into(), note: None }, players: vec!["Veronica (host, you)".into()], asking: vec![(4, "Cass".into())], ..default() };
        let p = pieces(&screen, &state);
        assert!(p.iter().any(|x| matches!(x, Piece::Bar(i) if i.contains(&Item::Text("Cass wants to join.".into(), 0.0, Shade::Bright)) && i.contains(&Item::Button(Act::Admit(4), "Let In".into(), Look::Primary)))));
        assert!(bar_has(&p, Act::TurnAway(4)));
        assert_eq!(acts(&screen, &state), vec![Act::Admit(4), Act::TurnAway(4), Act::FriendlyFire, Act::Emotes, Act::Back, Act::Close]);
    }

    /// Kick and Ban sit on each guest's line, its path and ping in the words.
    #[test]
    fn the_host_kicks_or_bans_from_the_player_list() {
        let screen = opened(Mode::Host);
        let state = LobbyState { hosting: Hosting::Open { code: "K7Q-M2X".into(), note: None }, players: vec!["Veronica (host, you)".into(), "Cass (LAN, 12 ms)".into()], guests: vec![(1, "Cass (LAN, 12 ms)".into())], ..default() };
        assert_eq!(acts(&screen, &state), vec![Act::Kick(1), Act::Ban(1), Act::FriendlyFire, Act::Emotes, Act::Back, Act::Close]);
        let p = pieces(&screen, &state);
        assert!(p.iter().any(|x| matches!(x, Piece::Bar(i) if i.contains(&Item::Text("  Cass (LAN, 12 ms)".into(), 0.0, Shade::Bright)) && i.contains(&Item::Button(Act::Ban(1), "Ban".into(), Look::Warning)))), "{p:?}");
    }

    #[test]
    fn a_full_lobby_of_six_lists_every_player_and_fits_the_screen() {
        // The lobby takes the host and `nvf_coop::net::MAX_GUESTS` (5) guests.
        let screen = opened(Mode::Host);
        let names = ["Veronica (host, you)", "Cass", "Boone", "Arcade", "Lily", "Raul"];
        let state = LobbyState { hosting: Hosting::Open { code: "ABCD-EFGH".into(), note: None }, players: names.iter().map(|n| n.to_string()).collect(), guests: names[1..].iter().enumerate().map(|(i, n)| (i as u8 + 1, n.to_string())).collect(), ..default() };
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Line(format!("  {}", names[0]), Shade::Bright)));
        for n in &names[1..] {
            assert!(p.iter().any(|x| matches!(x, Piece::Bar(i) if i.contains(&Item::Text(format!("  {n}"), 0.0, Shade::Bright)))), "{n} listed");
        }
        assert!(p.contains(&Piece::Line("Players (6 of 6):".into(), Shade::Dim)));
        // The panel's height as `draw` measures it, a long line taken as three lines of
        // small print (the font is not loaded here): it stays inside the 960-unit screen
        // (at 540 px too, since interface units scale with the view's height).
        assert!(TOP + panel_height(&p) <= 960.0, "the six-player lobby reaches {}", TOP + panel_height(&p));
    }

    /// The panel's height as `draw` measures it, a long line taken as three lines of small
    /// print (the font is not loaded here).
    fn panel_height(p: &[Piece]) -> f32 {
        p.iter()
            .map(|p| match p {
                Piece::Title(_) => ITEM + 8.0,
                Piece::Line(t, _) => (if t.len() > 50 { 3.0 } else { 1.0 }) * STEP + 6.0,
                Piece::Big(_) => ITEM + 24.0,
                Piece::Button(..) => ITEM + 6.0,
                Piece::Row(..) | Piece::Head(_) => ITEM,
                Piece::Cols(..) => ROW_H,
                Piece::Bar(_) => BAR_H,
                Piece::Space(h) => *h,
                Piece::Note(t, _) => (if t.len() > 50 { 2.0 } else { 1.0 }) * STEP + 6.0,
                Piece::Scroll(..) => 0.0,
            })
            .sum::<f32>()
            + 2.0 * PAD
    }

    fn bar_item(p: &[Piece], act: Act) -> Option<&Item> {
        p.iter().find_map(|x| match x {
            Piece::Bar(items) => items.iter().find(|i| matches!(i, Item::Button(a, ..) | Item::Field(a, ..) if *a == act)),
            _ => None,
        })
    }

    fn bar_has(p: &[Piece], act: Act) -> bool {
        bar_item(p, act).is_some()
    }

    /// The choices the screen draws, in order (the table's rows left out).
    fn drawn_acts(p: &[Piece]) -> Vec<Act> {
        p.iter()
            .flat_map(|x| match x {
                Piece::Bar(items) => items.iter().filter_map(|i| match i {
                    Item::Button(a, ..) | Item::Field(a, ..) => Some(*a),
                    _ => None,
                }).collect(),
                Piece::Button(a, _) | Piece::Row(a, ..) => vec![*a],
                _ => Vec::new(),
            })
            .collect()
    }

    /// Every screen fits the 960-unit height, tabs and toolbars on one line each.
    #[test]
    fn every_screen_fits_and_draws_what_the_keys_walk() {
        let mut screen = opened(Mode::Join);
        let state = LobbyState { joining: Joining::Failed("No answer from the host: check the code, and that their lobby is open.".into()), ..default() };
        let p = pieces(&screen, &state);
        assert_eq!(drawn_acts(&p), acts(&screen, &state));
        assert!(TOP + panel_height(&p) <= 960.0);
        assert!(bar_item(&p, Act::Join).is_some_and(|i| matches!(i, Item::Button(_, _, Look::Primary))));
        screen.browsing = true;
        let many = LobbyState { browse: Browse::Ready((0..20).map(|i| lobby(&format!("L{i:02}"), 3, 10 + i)).collect()), ..default() };
        let p = pieces(&screen, &many);
        assert!(TOP + panel_height(&p) <= 960.0, "a long list keeps the footer on screen");
        assert_eq!(p.iter().filter(|x| matches!(x, Piece::Cols(..))).count(), BROWSE_ROWS);
        assert!(p.iter().any(|x| matches!(x, Piece::Scroll(..))), "a scrollbar when rows are hidden");
        assert!(p.iter().any(|x| matches!(x, Piece::Bar(i) if i.contains(&Item::Text("Lobbies 1-10 of 20 (the arrows scroll)".into(), 0.0, Shade::Dim)))), "{p:?}");
        let host = opened(Mode::Host);
        let p = pieces(&host, &LobbyState::default());
        assert_eq!(drawn_acts(&p), acts(&host, &LobbyState::default()));
        assert!(TOP + panel_height(&p) <= 960.0);
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
        assert!(bar_item(&p, Act::SetPublic(false)).is_some_and(|i| matches!(i, Item::Button(_, _, Look::On))), "Private chosen");
        assert!(!acts(&screen, &state).contains(&Act::Field(FieldId::Title)), "no title while private");
        screen.public = true;
        assert_eq!(acts(&screen, &state)[..4], [Act::SetPublic(false), Act::SetPublic(true), Act::Field(FieldId::Title), Act::Field(FieldId::HostPassword)]);
        assert!(bar_item(&pieces(&screen, &state), Act::Create).is_some_and(|i| matches!(i, Item::Button(_, _, Look::Primary))));
        for c in "Strip run".chars() {
            screen.type_char(FieldId::Title, c);
        }
        for c in "hunter2".chars() {
            screen.type_char(FieldId::HostPassword, c);
        }
        let p = pieces(&screen, &state);
        assert!(bar_item(&p, Act::Field(FieldId::HostPassword)) == Some(&Item::Field(Act::Field(FieldId::HostPassword), "*******".into(), 0.0)), "{p:?}");
        assert!(bar_item(&p, Act::Field(FieldId::Title)) == Some(&Item::Field(Act::Field(FieldId::Title), "Strip run".into(), 0.0)));
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
        assert_eq!(codes(&screen), ["FAR", "EMP", "LCK", "NEAR", "FULL"], "favourite first, then by ping; other versions hidden; a host alone is shown by default");
        screen.hide_empty = true;
        assert_eq!(codes(&screen), ["FAR", "LCK", "NEAR", "FULL"]);
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
        assert!(TOP + panel_height(&p) <= 960.0, "an opened row stays on screen: {}", TOP + panel_height(&p));
        // A failed join from the row is said under it.
        let failed = LobbyState { joining: Joining::Failed("Wrong password for this lobby.".into()), ..state.clone() };
        let fp = pieces(&screen, &failed);
        let row = fp.iter().position(|x| matches!(x, Piece::Cols(Act::Lobby(1), ..))).unwrap();
        assert!(fp[row..row + 4].contains(&Piece::Note("Wrong password for this lobby.".into(), Shade::Bright)), "{fp:?}");
        assert!(p.iter().any(|x| matches!(x, Piece::Cols(Act::Lobby(1), c, Shade::Bright) if c[6] == LOCK_MARK && c[1] == "LCK's game")), "{p:?}");
        assert!(p.iter().any(|x| matches!(x, Piece::Cols(Act::Lobby(0), c, _) if c[0] == FAV_MARK)), "the favourite's marker");
        assert!(bar_has(&p, Act::JoinRow));
        // Every choice the keys walk is drawn, and the other way round.
        assert_eq!(drawn_acts(&p), acts(&screen, &state).into_iter().filter(|a| !matches!(a, Act::Lobby(_))).collect::<Vec<_>>());
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
