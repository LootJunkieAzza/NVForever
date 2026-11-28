//! Co-op screens (the network side is nvforever `coop.rs`). The pause menu's Co-op entry
//! opens the host's screen: the Co-op switch, Create Lobby, then the code to read out to
//! friends and who has joined. The main menu's Join Lobby entry opens the guest's: the code
//! typed in, Join, and once the host answers, who is in the lobby.
//!
//! **The Co-op switch** ([`CoopSwitch`]): off by default, so a single-player game shows
//! nothing of co-op but this switch [design]. While it is off the Co-op screen shows Create
//! Lobby greyed, the main menu hides Join Lobby (`crate::main_menu`) and the pause menu hides
//! its Co-op entry. The switch is a row of the settings page, `coop` (Gameplay, Co-op), so it
//! is kept in the settings file with the rest; the Co-op screen's switch flips that row.
//! It cannot be turned off while a lobby is open or this game is a guest: the lobby is
//! closed or left first [design].
//!
//! A guest in a session opens the same pause-menu entry and sees who is in the lobby, the
//! host marked, with Leave Lobby instead of Create Lobby. A guest also has Quest Sync (its
//! quest progression sync, on or off) and Questlines (the lines it sits out), with a
//! confirm box when that would leave shared quests part way through
//! (docs/design/coop-quest-sync.md; the choices live in `crate::coop_progress`, which
//! fills [`QuestSyncView`]).
//!
//! The host's screen also lists the players asking to join by online code (Let In, Turn
//! Away: the "Ask before players join" setting) and, for each guest, Kick (removed now) and
//! Ban (removed, and kept out while the lobby lasts) [design: docs/design/coop-networking.md
//! §10].
//!
//! The screen is drawn over the menu that opened it and takes the keys while it is up
//! ([`LobbyScreen::holds_input`]); it asks the network side for things with
//! [`LobbyRequest`] and shows what [`LobbyState`] says.
//!
//! Test aid `NVF_PLAY_TEST_LOBBY="<seconds>:<action>,..."` (real seconds from the start):
//! `code=<code>` types a code, `join`, `open` (the lobby as the pause menu opens it),
//! `create`, `close`, `leave`, `back`, `shot=<file>` (a screenshot), `coop=on` /
//! `coop=off` (the switch, for this run only: not kept); for a guest's quest sync,
//! `quest-sync` (the toggle), `questlines` (the menu), `line=<key>` (a line of
//! `fnv_game::coop_questlines::LINES`, by key), `confirm`, `cancel`.

use crate::interface::{amber, bc_supported, u};
use crate::start_menu::{color, highlight, text, wrapped, Faces, Shade, ITEM, LINE};
use crate::ui_pointer::{Button, PointerState};
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use fnv_util::words::{tr, trf};
use nvf_assets::GameData;

/// The screen's width and top, interface units.
const WIDTH: f32 = 820.0;
const TOP: f32 = 150.0;
const PAD: f32 = 24.0;
/// Small print's line step.
const STEP: f32 = 30.0;
/// The Co-op screen's title (the pause menu's entry's name).
const TITLE: &str = "Co-op";
/// Players a lobby takes: the host and `nvf_coop::net::MAX_GUESTS` guests (nvforever's
/// co-op tests check the two agree).
pub const LOBBY_PLAYERS: usize = 6;
/// The longest code or address typed.
const MAX_TYPED: usize = 64;
/// The Questlines menu's rows shown at once; Up and Down scroll the rest [design].
const LINE_ROWS: usize = 12;

/// Which screen is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Host,
    Join,
}

/// What the screen asks of the network side.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum LobbyRequest {
    /// Open a lobby on this game.
    Create,
    /// Close it (guests are told the game ended).
    Close,
    /// A guest leaves its host's lobby.
    Leave,
    /// Look for the host of this code or address.
    Find(String),
    /// Stop looking (or leave the host found).
    Cancel,
    /// The host removes guest `id`; with `ban`, keeps it out while the lobby lasts.
    Kick { id: u8, ban: bool },
    /// The host's answer to player `peer` asking to join (its number in the service's lobby).
    Admit { peer: u32, yes: bool },
    /// A guest turns its quest progression sync on or off (docs/design/coop-quest-sync.md §1).
    QuestSync(bool),
    /// A guest sits out questline `line` (a row of `fnv_game::coop_questlines::LINES`), or
    /// takes it again.
    SitOut { line: usize, out: bool },
}

/// A guest's quest progression sync as the lobby shows it (written by
/// `crate::coop_progress` while the screen is open; docs/design/coop-quest-sync.md).
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestSyncView {
    /// This game is a guest with a character (the rows show).
    pub guest: bool,
    pub on: Option<bool>,
    /// The lines sat out (`fnv_game::coop_questlines::Mask`).
    pub sat_out: u64,
    /// The shown names of the shared quests part way through, and those of each line.
    pub busy: Vec<String>,
    pub busy_by_line: Vec<(usize, Vec<String>)>,
}

/// What the confirm box asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Confirm {
    SyncOff,
    SitOut(usize),
}

/// The host's lobby.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Hosting {
    #[default]
    Off,
    Open {
        code: String,
        /// Anything friends should know to get in, if anything.
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

/// What the network side says (written by nvforever `coop.rs`).
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct LobbyState {
    pub hosting: Hosting,
    /// The players in the lobby, this game's first.
    pub players: Vec<String>,
    pub joining: Joining,
    /// This game is a guest in another player's lobby.
    pub guest: bool,
    /// The host's guests, by id, as the list names them (for Kick and Ban).
    pub guests: Vec<(u8, String)>,
    /// The host's: players asking to join (their number, name).
    pub asks: Vec<(u32, String)>,
}

/// The Co-op switch (see the module notes): off by default; its row of the settings page
/// is [`COOP_KEY`], kept in the settings file.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct CoopSwitch {
    pub on: bool,
}

/// The settings row that is the switch.
pub const COOP_KEY: &str = "coop";

impl CoopSwitch {
    /// A test aid's state: `on` (any case); anything else is off.
    pub fn read_line(line: &str) -> bool {
        line.trim().eq_ignore_ascii_case("on")
    }
}

/// The switch's row of the settings page.
fn coop_row() -> crate::settings::Setting {
    crate::settings::Setting {
        key: COOP_KEY,
        group: crate::pause::GROUP_MENUS,
        label: "Co-op",
        note: "On: the main menu shows Join Lobby and the pause menu a Co-op screen to open a lobby. It stays on while a lobby is open or joined.",
        switches: "",
        get: |w| w.get_resource::<CoopSwitch>().is_some_and(|s| s.on),
        set: |w, on| {
            if let Some(mut s) = w.get_resource_mut::<CoopSwitch>() {
                s.on = on;
                info!("co-op: switched {}", on_off(on).to_ascii_lowercase());
            }
        },
        slider: None,
    }
}

/// `NVF_PLAY_TEST_COOP=on|off` for a test run, after the settings file was read: its state
/// wins, and is not kept.
pub fn load_switch(mut switch: ResMut<CoopSwitch>) {
    if let Ok(aid) = std::env::var("NVF_PLAY_TEST_COOP") {
        switch.on = CoopSwitch::read_line(&aid);
        info!("test aid NVF_PLAY_TEST_COOP: co-op {}", on_off(switch.on).to_ascii_lowercase());
    }
}

/// The Co-op screen's switch: the switch's settings row flipped, and kept in the
/// settings file.
pub fn toggle_coop(world: &mut World) {
    let row = world.get_resource::<crate::settings::Settings>().and_then(|s| s.index(COOP_KEY));
    match row {
        Some(i) => {
            crate::settings::flip(world, i);
        }
        None => {
            if let Some(mut s) = world.get_resource_mut::<CoopSwitch>() {
                s.on = !s.on;
            }
        }
    }
}

/// The screen: which is up, what is typed.
#[derive(Resource, Default)]
pub struct LobbyScreen {
    mode: Option<Mode>,
    typed: String,
    selected: usize,
    /// The frame count as of the screen's last run, and when it opened and closed: the
    /// keys of those frames were the menu's (or are the menu's again).
    frame: u32,
    opened: u32,
    closed: Option<u32>,
    /// The Co-op switch as of this frame.
    coop: bool,
    /// The Co-op switch's button was taken: turned over by [`input`].
    coop_ask: bool,
    /// A guest's quest sync as of this frame ([`QuestSyncView`]), the Questlines menu up,
    /// and the confirm box up.
    sync: QuestSyncView,
    lines: bool,
    confirm: Option<Confirm>,
}

impl LobbyScreen {
    /// Open `mode`'s screen (from a menu's entry).
    pub fn open(&mut self, mode: Mode) {
        info!("lobby: {mode:?} screen");
        self.mode = Some(mode);
        self.selected = 0;
        self.lines = false;
        self.confirm = None;
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
    Join,
    Back,
    /// The Co-op switch.
    Coop,
    Kick(u8),
    Ban(u8),
    LetIn(u32),
    TurnAway(u32),
    /// A guest's Quest Sync toggle.
    QuestSync,
    /// Open the Questlines menu.
    Questlines,
    /// Sit this line (a row of `fnv_game::coop_questlines::LINES`) out, or take it again.
    Line(usize),
    /// The confirm box's yes and no.
    Confirm,
    Cancel,
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
        }
        app.init_resource::<LobbyScreen>()
            .init_resource::<LobbyState>()
            .init_resource::<CoopSwitch>()
            .init_resource::<QuestSyncView>()
            .insert_resource(LobbyAid(aid))
            .add_message::<LobbyRequest>()
            .add_systems(PostStartup, load_switch.after(crate::settings::apply_stored))
            .add_systems(Update, (input, draw).chain().in_set(PlaySet::Ui));
        crate::settings::add_setting(app, coop_row());
        // Greyed while a lobby is open or joined: it stays on (module notes).
        crate::settings::set_disabled_status(app, COOP_KEY, |w| w.get_resource::<LobbyState>().and_then(coop_held));
    }
}

/// The buttons that can be chosen, in order.
fn acts(screen: &LobbyScreen, state: &LobbyState) -> Vec<Act> {
    if screen.mode == Some(Mode::Host) && state.guest {
        if screen.confirm.is_some() {
            return vec![Act::Confirm, Act::Cancel];
        }
        if screen.lines {
            return (0..fnv_game::coop_questlines::LINES.len()).map(Act::Line).chain([Act::Back]).collect();
        }
    }
    match screen.mode {
        None => Vec::new(),
        Some(Mode::Host) if state.guest && screen.sync.guest => vec![Act::Leave, Act::QuestSync, Act::Questlines, Act::Back],
        Some(Mode::Host) if state.guest => vec![Act::Leave, Act::Back],
        Some(Mode::Host) if !screen.coop && coop_held(state).is_none() => vec![Act::Coop, Act::Back],
        Some(Mode::Host) => {
            let switch = coop_held(state).is_none().then_some(Act::Coop);
            match state.hosting {
                Hosting::Off => switch.into_iter().chain([Act::Create, Act::Back]).collect(),
                Hosting::Open { .. } => host_acts(state).into_iter().chain([Act::Close, Act::Back]).collect(),
            }
        }
        Some(Mode::Join) => match &state.joining {
            Joining::Found => vec![Act::Leave, Act::Back],
            Joining::Finding => vec![Act::Back],
            Joining::Idle | Joining::Failed(_) => vec![Act::Join, Act::Back],
        },
    }
}

/// The host's buttons for the players asking and the guests, in the screen's order.
fn host_acts(state: &LobbyState) -> Vec<Act> {
    let asks = state.asks.iter().flat_map(|(p, _)| [Act::LetIn(*p), Act::TurnAway(*p)]);
    let guests = state.guests.iter().flat_map(|(id, _)| [Act::Kick(*id), Act::Ban(*id)]);
    asks.chain(guests).collect()
}

/// Why the Co-op switch cannot be turned off now, if it cannot: a lobby is open or this
/// game is a guest (see the module notes).
fn coop_held(state: &LobbyState) -> Option<&'static str> {
    if state.guest {
        Some("leave the lobby to turn it off")
    } else if state.hosting != Hosting::Off {
        Some("close the lobby to turn it off")
    } else {
        None
    }
}

/// The code-typing step of the join screen.
fn typing(screen: &LobbyScreen, state: &LobbyState) -> bool {
    screen.mode == Some(Mode::Join) && matches!(state.joining, Joining::Idle | Joining::Failed(_))
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
    mut switch: ResMut<CoopSwitch>,
    sync: Option<Res<QuestSyncView>>,
) {
    screen.frame = frames.map_or(screen.frame.wrapping_add(1), |f| f.0);
    let sync = sync.as_deref().cloned().unwrap_or_default();
    if screen.sync != sync {
        screen.sync = sync;
    }
    // Not a guest any more: its menus go.
    if !state.guest && (screen.lines || screen.confirm.is_some()) {
        screen.lines = false;
        screen.confirm = None;
    }
    // The switch turned over last frame.
    if std::mem::take(&mut screen.coop_ask) {
        commands.queue(toggle_coop);
    }
    if screen.coop != switch.on {
        screen.coop = switch.on;
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
        } else if let Some(v) = a.strip_prefix("coop=") {
            switch.on = CoopSwitch::read_line(v);
            screen.coop = switch.on;
            info!("test aid NVF_PLAY_TEST_LOBBY: co-op {} (not kept)", on_off(switch.on).to_ascii_lowercase());
        } else if let Some(k) = a.strip_prefix("line=") {
            match fnv_game::coop_questlines::index(k) {
                Some(i) => take(&mut screen, &state, Act::Line(i), &mut asks),
                None => warn!("NVF_PLAY_TEST_LOBBY: no questline {k:?}"),
            }
        } else {
            let act = match a.as_str() {
                "join" => Some(Act::Join),
                // The pause menu's Co-op entry: the lobby as it stands.
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
                "leave" => Some(Act::Leave),
                "back" => Some(Act::Back),
                "quest-sync" => Some(Act::QuestSync),
                "questlines" => Some(Act::Questlines),
                "confirm" => Some(Act::Confirm),
                "cancel" => Some(Act::Cancel),
                other => {
                    warn!("NVF_PLAY_TEST_LOBBY: no action {other:?}");
                    None
                }
            };
            if let Some(act) = act {
                take(&mut screen, &state, act, &mut asks);
            }
        }
    }
    if screen.mode.is_none() {
        return;
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
    if typing(&screen, &state) {
        for k in typed.iter().filter(|k| k.state.is_pressed()) {
            match &k.logical_key {
                Key::Backspace => {
                    screen.typed.pop();
                }
                Key::Enter => chosen = Some(Act::Join),
                _ => {
                    if let Some(t) = k.text.as_deref() {
                        // A code, or an address (`ip:port`, an IPv6 address in brackets).
                        for c in t.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | ':' | '[' | ']')) {
                            if screen.typed.len() < MAX_TYPED {
                                screen.typed.push(c.to_ascii_uppercase());
                            }
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
        take(&mut screen, &state, act, &mut asks);
    }
}

fn take(screen: &mut LobbyScreen, state: &LobbyState, act: Act, asks: &mut MessageWriter<LobbyRequest>) {
    info!("lobby: {act:?}");
    if let Some(ask) = sync_press(screen, state, act) {
        if let Some(ask) = ask {
            info!("lobby: {ask:?}");
            asks.write(ask);
        }
        return;
    }
    match act {
        // Taken by `sync_press`.
        Act::QuestSync | Act::Questlines | Act::Line(_) | Act::Confirm | Act::Cancel => {}
        Act::Create => {
            asks.write(LobbyRequest::Create);
        }
        Act::Close => {
            asks.write(LobbyRequest::Close);
        }
        Act::Leave => {
            asks.write(LobbyRequest::Leave);
        }
        Act::Kick(id) => {
            asks.write(LobbyRequest::Kick { id, ban: false });
        }
        Act::Ban(id) => {
            asks.write(LobbyRequest::Kick { id, ban: true });
        }
        Act::LetIn(peer) => {
            asks.write(LobbyRequest::Admit { peer, yes: true });
        }
        Act::TurnAway(peer) => {
            asks.write(LobbyRequest::Admit { peer, yes: false });
        }
        Act::Coop => match coop_held(state) {
            Some(why) => info!("lobby: co-op stays on: {why}"),
            None => screen.coop_ask = true,
        },
        Act::Join => {
            let code = screen.typed.trim().to_string();
            if !code.is_empty() {
                asks.write(LobbyRequest::Find(code));
            }
        }
        Act::Back => match (screen.mode, &state.joining) {
            // Back from the search: to the code again.
            (Some(Mode::Join), Joining::Finding) => {
                asks.write(LobbyRequest::Cancel);
                screen.selected = 0;
            }
            // Back from a lobby joined: the screen closes, the guest stays in.
            (Some(Mode::Join), Joining::Found) => screen.close(),
            (Some(Mode::Join), _) => {
                asks.write(LobbyRequest::Cancel);
                screen.close();
            }
            _ => screen.close(),
        },
    }
}

/// A guest's Quest Sync toggle, Questlines menu and confirm box (docs/design/coop-quest-sync.md
/// §1, §4): `Some` when `act` was one of theirs (with what to ask, if anything), `None` for
/// every other act. Turning sync off, or sitting a line out, while shared quests are part way
/// through opens the confirm box first, Cancel chosen.
fn sync_press(screen: &mut LobbyScreen, state: &LobbyState, act: Act) -> Option<Option<LobbyRequest>> {
    use fnv_game::coop_questlines::bit;
    if !(screen.mode == Some(Mode::Host) && state.guest) {
        return None;
    }
    let back_to = |screen: &mut LobbyScreen, to: Act| {
        screen.selected = acts(screen, state).iter().position(|a| *a == to).unwrap_or(0);
    };
    let ask = match act {
        Act::QuestSync if screen.sync.on == Some(true) && !screen.sync.busy.is_empty() => {
            screen.confirm = Some(Confirm::SyncOff);
            screen.selected = 1;
            None
        }
        Act::QuestSync => Some(LobbyRequest::QuestSync(screen.sync.on != Some(true))),
        Act::Questlines => {
            screen.lines = true;
            screen.selected = 0;
            None
        }
        Act::Line(i) if screen.sync.sat_out & bit(i) != 0 => Some(LobbyRequest::SitOut { line: i, out: false }),
        Act::Line(i) if screen.sync.on == Some(true) && screen.sync.busy_by_line.iter().any(|(l, _)| *l == i) => {
            screen.confirm = Some(Confirm::SitOut(i));
            screen.selected = 1;
            None
        }
        Act::Line(i) => Some(LobbyRequest::SitOut { line: i, out: true }),
        Act::Confirm | Act::Cancel | Act::Back if screen.confirm.is_some() => {
            let c = screen.confirm.take();
            let yes = act == Act::Confirm;
            match c {
                Some(Confirm::SitOut(i)) => {
                    back_to(screen, Act::Line(i));
                    yes.then_some(LobbyRequest::SitOut { line: i, out: true })
                }
                _ => {
                    back_to(screen, Act::QuestSync);
                    yes.then_some(LobbyRequest::QuestSync(false))
                }
            }
        }
        Act::Confirm | Act::Cancel => None,
        Act::Back if screen.lines => {
            screen.lines = false;
            back_to(screen, Act::Questlines);
            None
        }
        _ => return None,
    };
    Some(ask)
}

/// The Quest Sync button [design: our own words; the original has no such option].
fn quest_sync_label(on: Option<bool>) -> String {
    trf("Quest Sync: {state}", &[("state", &on_off(on == Some(true)))])
}

/// What the screen shows, kept to see when it changes.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Title(String),
    Line(String, Shade),
    Big(String),
    Field(String),
    Button(Act, String),
    /// A row of a list, as wide as the panel, in small print (the Questlines menu).
    Row(Act, String),
    /// A button that cannot be taken now: greyed, no pointer.
    Off(String),
}

fn on_off(on: bool) -> &'static str {
    if on {
        tr("On")
    } else {
        tr("Off")
    }
}

/// The Co-op switch's button.
fn coop_label(on: bool) -> String {
    format!("Co-op: {}", on_off(on))
}

fn pieces(screen: &LobbyScreen, state: &LobbyState) -> Vec<Piece> {
    use Piece::*;
    let mut v = Vec::new();
    if screen.mode == Some(Mode::Host) && state.guest {
        if let Some(c) = screen.confirm {
            confirm_pieces(&mut v, screen, c);
            return v;
        }
        if screen.lines {
            line_pieces(&mut v, screen);
            v.push(Button(Act::Back, tr("Back").into()));
            return v;
        }
    }
    match screen.mode {
        None => {}
        Some(Mode::Host) if state.guest => {
            v.push(Title(TITLE.into()));
            v.push(Line(format!("{} ({})", coop_label(true), coop_held(state).unwrap_or_default()), Shade::Dim));
            in_lobby(&mut v, state);
            if screen.sync.guest {
                v.push(Line(tr("Quest progression sync: whether the host's quest progress also counts in your own save.").into(), Shade::Dim));
                v.push(Button(Act::QuestSync, quest_sync_label(screen.sync.on)));
                let n = screen.sync.sat_out.count_ones();
                v.push(Button(Act::Questlines, if n == 0 { tr("Questlines").into() } else { trf("Questlines ({n} out)", &[("n", &n)]) }));
            }
        }
        Some(Mode::Host) => {
            v.push(Title(TITLE.into()));
            let held = coop_held(state);
            match held {
                Some(why) => v.push(Line(format!("{} ({why})", coop_label(true)), Shade::Dim)),
                None => v.push(Button(Act::Coop, coop_label(screen.coop))),
            }
            match &state.hosting {
                Hosting::Off if !screen.coop && held.is_none() => {
                    v.push(Line("Turn co-op on to open a lobby.".into(), Shade::Bright));
                    v.push(Off("Create Lobby".into()));
                }
                Hosting::Off => {
                    v.push(Line("Open a lobby on this game: friends join with its code, from their main menu's Join Lobby.".into(), Shade::Bright));
                    v.push(Button(Act::Create, "Create Lobby".into()));
                }
                Hosting::Open { code, note } => {
                    v.push(Line("Lobby code:".into(), Shade::Dim));
                    v.push(Big(code.clone()));
                    v.push(Line("Tell your friends this code. It is also in lobby-code.txt in the NVForever folder.".into(), Shade::Dim));
                    if let Some(n) = note {
                        v.push(Line(n.clone(), Shade::Bright));
                    }
                    players(&mut v, state);
                    for (peer, name) in &state.asks {
                        v.push(Line(format!("{name} asks to join:"), Shade::Bright));
                        v.push(Button(Act::LetIn(*peer), format!("Let {name} In")));
                        v.push(Button(Act::TurnAway(*peer), format!("Turn {name} Away")));
                    }
                    for (id, name) in &state.guests {
                        v.push(Button(Act::Kick(*id), format!("Kick {name}")));
                        v.push(Button(Act::Ban(*id), format!("Ban {name}")));
                    }
                    v.push(Button(Act::Close, "Close Lobby".into()));
                }
            }
        }
        Some(Mode::Join) => {
            v.push(Title("Join Lobby".into()));
            match &state.joining {
                Joining::Idle | Joining::Failed(_) => {
                    v.push(Line("Type the host's lobby code, or its address (ip:port):".into(), Shade::Bright));
                    v.push(Field(screen.typed.clone()));
                    if let Joining::Failed(why) = &state.joining {
                        v.push(Line(why.clone(), Shade::Bright));
                    }
                    v.push(Button(Act::Join, "Join".into()));
                }
                Joining::Finding => v.push(Line("Looking for the host...".into(), Shade::Bright)),
                Joining::Found => in_lobby(&mut v, state),
            }
        }
    }
    if screen.mode.is_some() {
        v.push(Button(Act::Back, tr("Back").into()));
    }
    v
}

/// A guest's view of the lobby it is in: whose it is, who is there, and Leave Lobby.
fn in_lobby(v: &mut Vec<Piece>, state: &LobbyState) {
    let host = state.players.iter().find_map(|p| p.strip_suffix(tr(" (host)")));
    v.push(Piece::Line(host.map_or_else(|| "You are in another player's lobby.".to_string(), |h| format!("You are in {h}'s lobby.")), Shade::Bright));
    players(v, state);
    v.push(Piece::Button(Act::Leave, "Leave Lobby".into()));
}

/// The Questlines menu (docs/design/coop-quest-sync.md §1-2): every line under its kind's
/// heading, Synced or Sitting out, a window of [`LINE_ROWS`] round the one chosen
/// [design: our own words].
fn line_pieces(v: &mut Vec<Piece>, screen: &LobbyScreen) {
    use fnv_game::coop_questlines::{bit, Kind, LINES};
    v.push(Piece::Title(tr("Questlines").into()));
    v.push(Piece::Line(tr("Sit out a questline and its quests stay as your own save has them. The story before it splits off stays shared.").into(), Shade::Bright));
    if screen.sync.on != Some(true) {
        v.push(Piece::Line(tr("Quest sync is off: these apply once it is on.").into(), Shade::Dim));
    }
    let mut rows = Vec::new();
    for k in Kind::ALL {
        let mut first = true;
        for (i, l) in LINES.iter().enumerate().filter(|(_, l)| l.kind == k) {
            if std::mem::take(&mut first) {
                rows.push(Piece::Line(k.label().to_string(), Shade::Dim));
            }
            let out = screen.sync.sat_out & bit(i) != 0;
            rows.push(Piece::Row(Act::Line(i), format!("  {}   {}", tr(l.name), if out { tr("Sitting out") } else { tr("Synced") })));
        }
    }
    let chosen = (screen.selected < LINES.len()).then_some(Act::Line(screen.selected));
    let at = rows.iter().position(|r| matches!(r, Piece::Row(a, _) if Some(*a) == chosen)).unwrap_or(0);
    let first = at.saturating_sub(LINE_ROWS / 2).min(rows.len().saturating_sub(LINE_ROWS));
    let total = rows.len();
    if first > 0 {
        v.push(Piece::Line(trf("  ({n} more above: Up and Down scroll)", &[("n", &first)]), Shade::Dim));
    }
    v.extend(rows.into_iter().skip(first).take(LINE_ROWS));
    if first + LINE_ROWS < total {
        v.push(Piece::Line(trf("  ({n} more below: Up and Down scroll)", &[("n", &(total - first - LINE_ROWS))]), Shade::Dim));
    }
}

/// The confirm box: the shared quests part way through, what happens to them, Confirm or
/// Cancel (docs/design/coop-quest-sync.md §4).
fn confirm_pieces(v: &mut Vec<Piece>, screen: &LobbyScreen, c: Confirm) {
    use fnv_game::coop_questlines::{warning, LINES};
    let (title, text, yes) = match c {
        Confirm::SyncOff => (tr("Turn Off Quest Sync?").to_string(), warning(&screen.sync.busy, None), tr("Turn Off")),
        Confirm::SitOut(i) => {
            let name = LINES.get(i).map_or(tr("this questline"), |l| tr(l.name));
            let names = screen.sync.busy_by_line.iter().find(|(l, _)| *l == i).map(|(_, n)| n.clone()).unwrap_or_default();
            (trf("Sit Out {name}?", &[("name", &name)]), warning(&names, Some(name)), tr("Sit Out"))
        }
    };
    v.push(Piece::Title(title));
    v.push(Piece::Line(text, Shade::Bright));
    v.push(Piece::Button(Act::Confirm, yes.into()));
    v.push(Piece::Button(Act::Cancel, tr("Cancel").into()));
}

/// Who is in the lobby, one a line (the host and this player marked by the network side).
fn players(v: &mut Vec<Piece>, state: &LobbyState) {
    v.push(Piece::Line(format!("Players ({} of {LOBBY_PLAYERS}):", state.players.len()), Shade::Dim));
    for p in &state.players {
        v.push(Piece::Line(format!("  {p}"), Shade::Bright));
    }
}

/// The panel's backing: dark, mostly opaque [design].
const BACKING: f32 = 0.92;

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
    let total: f32 = now.iter().map(|p| height(p, |t| f.small.font.lines(t, Some(inner)).len())).sum::<f32>() + 2.0 * PAD;
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
        root.spawn((panel, BackgroundColor(Color::BLACK.with_alpha(BACKING)))).with_children(|p| {
            let mut y = PAD;
            for piece in &now {
                let h = height(piece, |t| f.small.font.lines(t, Some(inner)).len());
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
                    Piece::Field(t) => {
                        let node = Node { border: UiRect::all(u(LINE)), ..at(inner, ITEM) };
                        p.spawn((node, BorderColor::all(amber(200.0)))).with_children(|n| text(n, &f.entry, &format!("{t}_"), 12.0, ITEM * 0.72, color(Shade::Bright)));
                    }
                    Piece::Off(t) => {
                        p.spawn(at(320.0, ITEM)).with_children(|n| text(n, &f.entry, t, 12.0, ITEM * 0.72, color(Shade::Dim)));
                    }
                    Piece::Button(act, t) => {
                        let node = at(320.0, ITEM);
                        let mut e = if Some(*act) == chosen { p.spawn(highlight(node)) } else { p.spawn(node) };
                        e.insert((Button, *act));
                        e.with_children(|n| text(n, &f.entry, t, 12.0, ITEM * 0.72, color(Shade::Bright)));
                    }
                    Piece::Row(act, t) => {
                        let node = at(inner, h);
                        let mut e = if Some(*act) == chosen { p.spawn(highlight(node)) } else { p.spawn(node) };
                        e.insert((Button, *act));
                        e.with_children(|n| text(n, &f.small, t, 12.0, STEP * 0.8, color(Shade::Bright)));
                    }
                }
                y += h;
            }
        });
    });
}

/// A piece's height, interface units; `lines` counts a text's lines of small print.
fn height(p: &Piece, lines: impl Fn(&str) -> usize) -> f32 {
    match p {
        Piece::Title(_) => ITEM + 8.0,
        Piece::Line(t, _) => lines(t).max(1) as f32 * STEP + 6.0,
        Piece::Big(_) => ITEM + 16.0,
        Piece::Field(_) | Piece::Button(..) | Piece::Off(_) => ITEM + 6.0,
        Piece::Row(..) => STEP + 6.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened(mode: Mode) -> LobbyScreen {
        let mut s = LobbyScreen::default();
        s.open(mode);
        s.coop = true;
        s
    }

    /// Co-op is off by default: the screen offers the switch, with Create Lobby greyed; on,
    /// it can be taken, and the switch stays.
    #[test]
    fn co_op_is_off_by_default_and_greys_the_lobby() {
        assert!(!CoopSwitch::default().on, "off by default");
        assert!(CoopSwitch::read_line("on") && CoopSwitch::read_line(" ON ") && !CoopSwitch::read_line("off") && !CoopSwitch::read_line(""));
        let mut screen = LobbyScreen::default();
        screen.open(Mode::Host);
        let state = LobbyState::default();
        assert_eq!(acts(&screen, &state), vec![Act::Coop, Act::Back]);
        let p = pieces(&screen, &state);
        assert_eq!(p[0], Piece::Title("Co-op".into()));
        assert!(p.contains(&Piece::Button(Act::Coop, "Co-op: Off".into())));
        assert!(p.contains(&Piece::Off("Create Lobby".into())), "{p:?}");
        assert!(!p.iter().any(|x| matches!(x, Piece::Button(Act::Create, _))), "nothing greyed can be taken");
        // On: the lobby can be taken.
        screen.coop = true;
        assert_eq!(acts(&screen, &state), vec![Act::Coop, Act::Create, Act::Back]);
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Button(Act::Coop, "Co-op: On".into())));
        assert!(!p.iter().any(|x| matches!(x, Piece::Off(_))));
    }

    /// The switch cannot go off while a lobby is open or this game is a guest: its row
    /// says why and cannot be taken.
    #[test]
    fn co_op_stays_on_while_a_lobby_is_up() {
        let screen = opened(Mode::Host);
        let open = LobbyState { hosting: Hosting::Open { code: "ABCD-EFGH".into(), note: None }, ..default() };
        assert!(!acts(&screen, &open).contains(&Act::Coop));
        assert!(pieces(&screen, &open).contains(&Piece::Line("Co-op: On (close the lobby to turn it off)".into(), Shade::Dim)));
        let guest = LobbyState { guest: true, ..default() };
        assert!(!acts(&screen, &guest).contains(&Act::Coop));
    }

    #[test]
    fn a_guest_sees_the_players_and_can_leave_not_create() {
        let screen = opened(Mode::Host);
        let state = LobbyState { players: vec!["Cass (you)".into(), "Veronica (host)".into()], guest: true, ..default() };
        assert_eq!(acts(&screen, &state), vec![Act::Leave, Act::Back]);
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Line("You are in Veronica's lobby.".into(), Shade::Bright)));
        assert!(p.contains(&Piece::Line("  Cass (you)".into(), Shade::Bright)) && p.contains(&Piece::Line("  Veronica (host)".into(), Shade::Bright)));
        assert!(p.contains(&Piece::Button(Act::Leave, "Leave Lobby".into())));
        assert!(!p.iter().any(|x| matches!(x, Piece::Button(Act::Create | Act::Close, _))), "nothing of the host's");
        // Left: the normal choice again.
        let alone = LobbyState::default();
        assert_eq!(acts(&screen, &alone), vec![Act::Coop, Act::Create, Act::Back]);
    }

    /// The join screen: the code, then the search, then the lobby joined.
    #[test]
    fn the_join_screen_types_a_code_then_shows_the_lobby() {
        let mut screen = opened(Mode::Join);
        screen.typed = "192.168.1.20:47960".into();
        let idle = LobbyState::default();
        assert!(typing(&screen, &idle));
        assert_eq!(acts(&screen, &idle), vec![Act::Join, Act::Back]);
        assert!(pieces(&screen, &idle).contains(&Piece::Field("192.168.1.20:47960".into())));
        let finding = LobbyState { joining: Joining::Finding, ..default() };
        assert!(!typing(&screen, &finding));
        assert_eq!(acts(&screen, &finding), vec![Act::Back]);
        let found = LobbyState { joining: Joining::Found, guest: true, players: vec!["Cass (you)".into(), "Veronica (host)".into()], ..default() };
        assert_eq!(acts(&screen, &found), vec![Act::Leave, Act::Back]);
        assert!(pieces(&screen, &found).contains(&Piece::Line("You are in Veronica's lobby.".into(), Shade::Bright)));
        let failed = LobbyState { joining: Joining::Failed("No answer.".into()), ..default() };
        assert!(pieces(&screen, &failed).contains(&Piece::Line("No answer.".into(), Shade::Bright)));
    }

    #[test]
    fn the_host_lets_players_in_and_kicks_or_bans_guests() {
        let screen = opened(Mode::Host);
        let state = LobbyState { hosting: Hosting::Open { code: "K7Q-M2X".into(), note: None }, guests: vec![(1, "Cass".into())], asks: vec![(7, "Boone".into())], ..default() };
        let list = acts(&screen, &state);
        assert_eq!(list, vec![Act::LetIn(7), Act::TurnAway(7), Act::Kick(1), Act::Ban(1), Act::Close, Act::Back]);
        let buttons: Vec<Act> = pieces(&screen, &state).into_iter().filter_map(|p| if let Piece::Button(a, _) = p { Some(a) } else { None }).collect();
        assert_eq!(buttons, list, "the buttons in the order the keys go through them");
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
        // small print (the font is not loaded here): it stays inside the 960-unit screen.
        let total: f32 = p.iter().map(|x| height(x, |t| if t.len() > 50 { 3 } else { 1 })).sum::<f32>() + 2.0 * PAD;
        assert!(TOP + total <= 960.0, "the six-player lobby reaches {}", TOP + total);
    }

    fn guest_state() -> LobbyState {
        LobbyState { players: vec!["Cass (you)".into(), "Veronica (host)".into()], guest: true, ..default() }
    }

    fn guest_screen(on: Option<bool>, busy: &[&str]) -> LobbyScreen {
        let mut s = opened(Mode::Host);
        s.sync = QuestSyncView { guest: true, on, busy: busy.iter().map(|b| b.to_string()).collect(), ..default() };
        s
    }

    /// The toggle (docs/design/coop-quest-sync.md §1, §4): off and on with nothing part way
    /// through asks at once; off with shared quests part way through opens the confirm box,
    /// Cancel chosen, naming them; Confirm turns it off, Cancel and Back leave it on.
    #[test]
    fn a_guests_quest_sync_toggle_warns_before_leaving_quests_part_way() {
        let state = guest_state();
        // No character yet (the view says nothing): no rows.
        assert_eq!(acts(&opened(Mode::Host), &state), vec![Act::Leave, Act::Back]);
        let mut screen = guest_screen(Some(true), &[]);
        assert_eq!(acts(&screen, &state), vec![Act::Leave, Act::QuestSync, Act::Questlines, Act::Back]);
        assert!(pieces(&screen, &state).contains(&Piece::Button(Act::QuestSync, "Quest Sync: On".into())));
        assert_eq!(sync_press(&mut screen, &state, Act::QuestSync), Some(Some(LobbyRequest::QuestSync(false))));
        let mut off = guest_screen(Some(false), &[]);
        assert!(pieces(&off, &state).contains(&Piece::Button(Act::QuestSync, "Quest Sync: Off".into())));
        assert_eq!(sync_press(&mut off, &state, Act::QuestSync), Some(Some(LobbyRequest::QuestSync(true))));
        // Never chosen: turning it on.
        let mut unset = guest_screen(None, &[]);
        assert_eq!(sync_press(&mut unset, &state, Act::QuestSync), Some(Some(LobbyRequest::QuestSync(true))));
        // Part way through: the box first.
        let mut busy = guest_screen(Some(true), &["Heartache by the Number", "Ring-a-Ding-Ding!"]);
        assert_eq!(sync_press(&mut busy, &state, Act::QuestSync), Some(None));
        assert_eq!(busy.confirm, Some(Confirm::SyncOff));
        assert_eq!(acts(&busy, &state), vec![Act::Confirm, Act::Cancel]);
        assert_eq!(busy.selected, 1, "Cancel chosen");
        let p = pieces(&busy, &state);
        assert!(p.contains(&Piece::Title("Turn Off Quest Sync?".into())));
        assert!(p.iter().any(|x| matches!(x, Piece::Line(t, _) if t.contains("Heartache by the Number, Ring-a-Ding-Ding!") && t.contains("stay at their current stage"))), "{p:?}");
        assert!(p.contains(&Piece::Button(Act::Confirm, "Turn Off".into())) && p.contains(&Piece::Button(Act::Cancel, "Cancel".into())));
        assert_eq!(sync_press(&mut busy, &state, Act::Cancel), Some(None));
        assert_eq!(busy.confirm, None);
        assert_eq!(acts(&busy, &state)[busy.selected], Act::QuestSync, "back on the toggle");
        sync_press(&mut busy, &state, Act::QuestSync);
        assert_eq!(sync_press(&mut busy, &state, Act::Back), Some(None), "Escape cancels");
        sync_press(&mut busy, &state, Act::QuestSync);
        assert_eq!(sync_press(&mut busy, &state, Act::Confirm), Some(Some(LobbyRequest::QuestSync(false))));
        // Not a guest's act: left to `take`.
        assert_eq!(sync_press(&mut busy, &state, Act::Leave), None);
        // A host's screen: none of it.
        let mut host = opened(Mode::Host);
        assert_eq!(sync_press(&mut host, &LobbyState::default(), Act::Back), None);
    }

    /// The Questlines menu: every line under its heading, a click sits one out or takes it
    /// again, a line with quests part way through asks first, Back returns to its button.
    #[test]
    fn the_questlines_menu_sits_lines_out_with_a_warning() {
        use fnv_game::coop_questlines::{bit, index, LINES};
        let state = guest_state();
        let house = index("house").unwrap();
        let mut screen = guest_screen(Some(true), &[]);
        assert_eq!(sync_press(&mut screen, &state, Act::Questlines), Some(None));
        assert_eq!(acts(&screen, &state).len(), LINES.len() + 1);
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Title("Questlines".into())));
        assert!(p.contains(&Piece::Line("Main story".into(), Shade::Dim)));
        assert!(p.contains(&Piece::Row(Act::Line(house), "  Mr. House   Synced".into())), "{p:?}");
        assert!(p.iter().any(|x| matches!(x, Piece::Line(t, Shade::Dim) if t.contains("more below"))), "the list scrolls: {p:?}");
        // The menu fits the screen (a long line taken as three lines of small print).
        let total: f32 = p.iter().map(|x| height(x, |t| if t.len() > 50 { 3 } else { 1 })).sum::<f32>() + 2.0 * PAD;
        assert!(TOP + total <= 960.0, "the Questlines menu reaches {}", TOP + total);
        assert_eq!(sync_press(&mut screen, &state, Act::Line(house)), Some(Some(LobbyRequest::SitOut { line: house, out: true })));
        // Sat out (as the view says next frame): shown, and a click takes it again.
        screen.sync.sat_out = bit(house);
        screen.selected = house;
        assert!(pieces(&screen, &state).contains(&Piece::Row(Act::Line(house), "  Mr. House   Sitting out".into())));
        assert_eq!(sync_press(&mut screen, &state, Act::Line(house)), Some(Some(LobbyRequest::SitOut { line: house, out: false })));
        // The last line chosen: the window has scrolled to it.
        screen.selected = LINES.len() - 1;
        let last = LINES.len() - 1;
        assert!(pieces(&screen, &state).iter().any(|x| matches!(x, Piece::Row(Act::Line(i), _) if *i == last)));
        screen.selected = house;
        // A House quest part way through: the box, naming it, then back on its row.
        screen.sync.sat_out = 0;
        screen.sync.busy = vec!["The House Always Wins, I".into()];
        screen.sync.busy_by_line = vec![(house, vec!["The House Always Wins, I".into()])];
        assert_eq!(sync_press(&mut screen, &state, Act::Line(house)), Some(None));
        let p = pieces(&screen, &state);
        assert!(p.contains(&Piece::Title("Sit Out Mr. House?".into())), "{p:?}");
        assert!(p.iter().any(|x| matches!(x, Piece::Line(t, _) if t.contains("If you sit out Mr. House now") && t.contains("The House Always Wins, I"))));
        assert_eq!(sync_press(&mut screen, &state, Act::Confirm), Some(Some(LobbyRequest::SitOut { line: house, out: true })));
        assert!(screen.lines && screen.confirm.is_none());
        assert_eq!(acts(&screen, &state)[screen.selected], Act::Line(house));
        // Another line with nothing part way through: no box.
        let ncr = index("ncr").unwrap();
        assert_eq!(sync_press(&mut screen, &state, Act::Line(ncr)), Some(Some(LobbyRequest::SitOut { line: ncr, out: true })));
        // Back: the guest's lobby again, on Questlines; its button counts what is sat out.
        assert_eq!(sync_press(&mut screen, &state, Act::Back), Some(None));
        assert!(!screen.lines);
        assert_eq!(acts(&screen, &state)[screen.selected], Act::Questlines);
        screen.sync.sat_out = bit(house) | bit(ncr);
        assert!(pieces(&screen, &state).contains(&Piece::Button(Act::Questlines, "Questlines (2 out)".into())));
        // Sync off: the menu says its choices wait.
        screen.sync.on = Some(false);
        sync_press(&mut screen, &state, Act::Questlines);
        assert!(pieces(&screen, &state).contains(&Piece::Line("Quest sync is off: these apply once it is on.".into(), Shade::Dim)));
        // And sitting a line out while off asks no box (nothing follows the host then).
        assert_eq!(sync_press(&mut screen, &state, Act::Line(0)), Some(Some(LobbyRequest::SitOut { line: 0, out: true })));
    }
}
