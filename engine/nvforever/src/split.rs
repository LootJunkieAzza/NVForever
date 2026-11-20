//! Split screen (private fork; plan: `C:\NVForever\morning-report\plans\split-screen.md`,
//! owner 2026-11-20): two players on one PC, each in a window of their own on half the
//! screen. Player 2's game is a second run of this program that joins this one as a co-op
//! guest over 127.0.0.1, so in the host's world player 2 is a stand-in like any guest
//! (`crate::coop` `stand_ins`), and only its camera, controls, HUD and menus run in its own
//! game [design: owner's choice of the two-window build, 2026-11-20].
//!
//! - **Starting it** (player 1): the pause menu's Add Player 2 (`nvf_play::split_screen`)
//!   turns split screen on in the running game; the main menu's Split Screen entry loads a
//!   save with it on; `--split` starts with it on. Once in the world, this game hosts (if it
//!   isn't already), moves its window to the top half (the left half side by side), and
//!   starts player 2's game on the other half.
//! - **Online too** (plan §10.1). A lobby this game opens for player 2 is for this PC only
//!   (`crate::coop::HOST_LOCAL_ENV`: 127.0.0.1, no router port, no firewall prompt); Create
//!   Lobby opens it to online players later, player 2 staying in. A game already hosting
//!   online keeps its lobby, and player 2 joins it over 127.0.0.1. A game that is a guest in
//!   another player's lobby passes player 2's game through to that host
//!   (`crate::coop::Coop::open_relay`): player 2 is a guest of that host's own, with its
//!   own id and stand-in, one of its 6 players, and its XP catch-up follows that host's
//!   level. When this game leaves that lobby, player 2 leaves too: split screen ends. A load restarts this game
//!   (`crate::handoff`), and the new run's command line has `--split` while split screen is
//!   on ([`with_split_arg`]); it finds player 2's game still running and takes it over
//!   rather than starting another.
//! - **Ending it**: End Split Screen (the pause menu), or player 2 going Back from the
//!   picker: player 2's game closes, and player 1's window gets the whole screen back as it
//!   was. A lobby opened only for player 2 closes once it is empty.
//! - **`--split-p2 <dir>`** (player 2, started by player 1): `<dir>` is player 1's session
//!   folder. This game's window is borderless on its half, owned by player 1's window (they
//!   move, minimise and stay on top together) and never takes focus, so the keyboard stays
//!   player 1's [design]. It quits when player 1's game has been gone for [`P1_GONE`].
//! - **Characters** (plan §7): player 2 plays one of four premades
//!   (`fnv_game::split_premade`). Player 1's game writes one save per premade into player
//!   2's home ([`premade_saves`]) and starts player 2's on its main menu, joining, with the
//!   join screen up: its list of saves is the picker. What player 2's game saves is kept
//!   with the host save (`saves\split\<stem>\premade-<n>.nvfsave`, [`premade_file`]).
//! - **Session files** in `<player 1's home>\split\`: `p1.txt` (player 1's process, window,
//!   the layout and the screen's rectangle) and `p2.txt` (player 2's process and the port
//!   its controller arrives on). Player 2's own home is `split\p2-home` beside them.
//! - **Controllers.** Player 1's game reads every controller. The first one that presses A
//!   (South) while player 2 has none, and isn't the one player 1 last used, becomes player
//!   2's. Player 1's game then never sees it (its `Gamepad` is taken off, so nothing in
//!   this game reads it) and relays its buttons and sticks to player 2's game every frame
//!   (127.0.0.1 UDP). Player 2's game reads no real controller (Windows may not deliver a
//!   controller's input to a window without focus, and player 2's never has it) and plays
//!   the relayed one as a controller of its own, so its menus and controls work unchanged.
//!   A controller that comes back after being unplugged (same name, vendor and product) is
//!   player 2's again.
//!
//! Log: `split: split screen on`, `split: split screen ended (…)`,
//! `split: player 2's game started (pid …)`, `split: player 2's controller is …`,
//! `split: player 2's controller unplugged`, `split: player 1's game is gone: quitting`.

use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

use bevy::input::gamepad::{gamepad_connection_system, gamepad_event_processing_system, Gamepad, GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent, RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, Monitor, PrimaryMonitor, PrimaryWindow, RawHandleWrapper, WindowMode};
use nvf_play::split_screen::{SplitAtStart, SplitLayout, SplitScreen};

// The field of view of a half (view.rs).
pub(crate) mod view;

/// How the screen is split [owner: a setting, top and bottom by default].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    #[default]
    TopBottom,
    SideBySide,
}

impl Layout {
    fn key(self) -> &'static str {
        match self {
            Layout::TopBottom => "top-bottom",
            Layout::SideBySide => "side-by-side",
        }
    }
    /// The layout the settings page's Split screen layout row chose.
    fn of_setting(l: SplitLayout) -> Layout {
        match l {
            SplitLayout::TopBottom => Layout::TopBottom,
            SplitLayout::SideBySide => Layout::SideBySide,
        }
    }
    fn setting(self) -> SplitLayout {
        match self {
            Layout::TopBottom => SplitLayout::TopBottom,
            Layout::SideBySide => SplitLayout::SideBySide,
        }
    }
    fn parse(s: &str) -> Layout {
        if s.trim() == "side-by-side" {
            Layout::SideBySide
        } else {
            Layout::TopBottom
        }
    }
}

/// Player 1's half and player 2's, as (top-left corner, size) in physical pixels, of a
/// screen at `pos` of `size`.
pub fn halves(pos: IVec2, size: UVec2, layout: Layout) -> [(IVec2, UVec2); 2] {
    match layout {
        Layout::TopBottom => {
            let top = size.y / 2;
            [(pos, UVec2::new(size.x, top)), (pos + IVec2::new(0, top as i32), UVec2::new(size.x, size.y - top))]
        }
        Layout::SideBySide => {
            let left = size.x / 2;
            [(pos, UVec2::new(left, size.y)), (pos + IVec2::new(left as i32, 0), UVec2::new(size.x - left, size.y))]
        }
    }
}

/// Set in player 2's game (and kept by its restarts, `crate::handoff`): its window never
/// takes the focus.
pub const P2_ENV: &str = "NVF_SPLIT_P2";
/// Player 2's name in the lobby (its game's `crate::coop::NAME_ENV`).
pub const P2_NAME: &str = "Player 2";

/// Player 2's game quits when player 1's has been gone this long: longer than a load's
/// restart takes to hand over (`crate::handoff`) [design].
const P1_GONE: f32 = 15.0;
/// Player 1's game ends split screen when player 2's has been gone this long: longer than
/// player 2's own restart takes, with a load's start-up, when it picks a character
/// [design].
const P2_GONE: f32 = 60.0;
/// Set while a lobby this game opened for player 2 is open (kept by a load's restart, as
/// the lobby is): ending split screen closes it once it is empty.
const OPENED_LOBBY_ENV: &str = "NVF_SPLIT_OPENED_LOBBY";

/// Whether split screen was turned on or off since this run started: 0 as the command line
/// says, [`ARG_ON`], [`ARG_OFF`].
static SPLIT_ARG: AtomicU8 = AtomicU8::new(0);
const ARG_ON: u8 = 1;
const ARG_OFF: u8 = 2;

/// This run's command line as a load's new run takes it (main.rs `command_line`): with
/// `--split` while split screen is on (or the main menu's Split Screen entry loads a save),
/// without it once it has ended.
pub fn with_split_arg(args: &mut Vec<String>) {
    split_arg(args, SPLIT_ARG.load(Ordering::Relaxed));
}

fn split_arg(args: &mut Vec<String>, state: u8) {
    match state {
        ARG_ON if !args.iter().any(|a| a == "--split") => args.push("--split".into()),
        ARG_OFF => args.retain(|a| a != "--split"),
        _ => {}
    }
}
/// How often the session files are read and written again (seconds).
const FILE_EVERY: f32 = 1.0;
/// The relay's packets: magic, then [`PadState::encode`].
const MAGIC: &[u8; 4] = b"NVSP";

/// The buttons and axes the relay carries, in its order.
const BUTTONS: [GamepadButton; 19] = [
    GamepadButton::South,
    GamepadButton::East,
    GamepadButton::North,
    GamepadButton::West,
    GamepadButton::C,
    GamepadButton::Z,
    GamepadButton::LeftTrigger,
    GamepadButton::LeftTrigger2,
    GamepadButton::RightTrigger,
    GamepadButton::RightTrigger2,
    GamepadButton::Select,
    GamepadButton::Start,
    GamepadButton::Mode,
    GamepadButton::LeftThumb,
    GamepadButton::RightThumb,
    GamepadButton::DPadUp,
    GamepadButton::DPadDown,
    GamepadButton::DPadLeft,
    GamepadButton::DPadRight,
];
const AXES: [GamepadAxis; 6] = [GamepadAxis::LeftStickX, GamepadAxis::LeftStickY, GamepadAxis::LeftZ, GamepadAxis::RightStickX, GamepadAxis::RightStickY, GamepadAxis::RightZ];

/// Player 2's controller as the relay carries it: which controller (for its button
/// names), whether it is plugged in, and every button's and axis's value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PadState {
    pub name: String,
    pub vendor: Option<u16>,
    pub product: Option<u16>,
    pub connected: bool,
    pub buttons: [f32; BUTTONS.len()],
    pub axes: [f32; AXES.len()],
}

impl PadState {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = MAGIC.to_vec();
        b.push(u8::from(self.connected));
        let name = &self.name.as_bytes()[..self.name.len().min(64)];
        b.push(name.len() as u8);
        b.extend_from_slice(name);
        for id in [self.vendor, self.product] {
            b.extend_from_slice(&id.map_or(u32::MAX, u32::from).to_le_bytes());
        }
        for v in self.buttons.iter().chain(&self.axes) {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    }

    pub fn decode(b: &[u8]) -> Option<PadState> {
        let b = b.strip_prefix(MAGIC)?;
        let (&connected, b) = b.split_first()?;
        let (&n, b) = b.split_first()?;
        let name = String::from_utf8_lossy(b.get(..n as usize)?).into_owned();
        let mut b = &b[n as usize..];
        let mut word = || -> Option<[u8; 4]> {
            let w = b.get(..4)?.try_into().ok()?;
            b = &b[4..];
            Some(w)
        };
        let id = |w: [u8; 4]| u16::try_from(u32::from_le_bytes(w)).ok();
        let vendor = id(word()?);
        let product = id(word()?);
        let mut s = PadState { name, vendor, product, connected: connected != 0, ..default() };
        for v in s.buttons.iter_mut().chain(s.axes.iter_mut()) {
            *v = f32::from_le_bytes(word()?);
        }
        Some(s)
    }
}

/// The command line's part (main.rs).
#[derive(Debug, Clone, Default)]
pub struct SplitArgs {
    /// `--split`: start split screen once in the world.
    pub start: bool,
    /// `--split-p2 <dir>`: this game is player 2's; the session folder.
    pub p2: Option<PathBuf>,
}

pub struct SplitPlugin(pub SplitArgs);

impl Plugin for SplitPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(view::SplitViewPlugin);
        if let Some(dir) = self.0.p2.clone() {
            // Its HUD and menus are drawn for its half (player 1's file says which).
            app.insert_resource(at_start(&dir, true));
            let socket = UdpSocket::bind("127.0.0.1:0").and_then(|s| s.set_nonblocking(true).map(|_| s)).map_err(|e| warn!("split: no controller relay: {e}")).ok();
            app.insert_resource(nvf_play::audio_out::PersonalSoundsOnly).insert_resource(nvf_play::split_screen::PlayerTwo).insert_resource(P2 { dir, socket, pad: None, last: PadState::default(), owner: None, placed: None, half: None, p1_seen: 0.0, file_at: f32::NEG_INFINITY, premade: loaded_stem().and_then(|s| premade_of(&s)), join_opened: false, restacked: false })
                .add_systems(PreUpdate, p2_receive.before(gamepad_connection_system))
                .add_systems(PreUpdate, p2_strip_real_pads.after(gamepad_connection_system).before(gamepad_event_processing_system))
                .add_systems(Update, (p2_session, p2_keep_character, p2_open_join, p2_back_ends, split_ui.after(p2_session)));
        } else {
            // Player 1's game, or one that may become it (the menus' entries).
            nvf_play::split_screen::register_layout(app, "Display");
            if self.0.start {
                if let Some(home) = fnv_install::EngineHome::default_root() {
                    app.insert_resource(at_start(&home.join("split"), false));
                }
            }
            app.insert_resource(SplitScreen { available: true, ask: self.0.start.then_some(true), ..default() })
                .add_systems(PreUpdate, p1_route_pads.run_if(resource_exists::<P1>).after(gamepad_connection_system).before(gamepad_event_processing_system))
                .add_systems(Update, (split_asks, p1_session.run_if(resource_exists::<P1>), close_lobby_after, split_ui).chain());
        }
    }
}

/// The HUD and the menus drawn for this game's half while split screen is on, and at the
/// run's own scales once it is over, as soon as either changes
/// (`nvf_play::interface::set_split_ui`): split screen started from the pause menu, its
/// layout changed on the settings page (both games: player 2's follows player 1's file),
/// split screen ended (player 1's game; player 2's closes). The HUD's pieces are made
/// again at the new scale (`nvf_play::interface::HudBuild`). A game that starts in split
/// screen has its half's from the start (`SplitAtStart`), kept until its window is on it.
fn split_ui(p1: Option<Res<P1>>, p2: Option<Res<P2>>, split: Option<Res<SplitScreen>>, mut build: Option<ResMut<nvf_play::interface::HudBuild>>) {
    let asked = split.as_deref().is_some_and(|s| s.ask == Some(true));
    let Some(half) = wanted_half(p1.as_deref().map(|p| p.half), p2.as_deref().map(|p| p.half), asked) else { return };
    nvf_play::interface::set_split_ui(half.map(|h| nvf_play::interface::split_ui_factors(h.as_vec2())), build.as_deref_mut());
}

/// The half this game's interface is to be drawn for: `Some(Some(size))` on a half,
/// `Some(None)` not split, `None` not known yet (no change): player 2's game (`p2`, its
/// half from player 1's file) and player 1's (`p1`, the half its window is put on) once
/// they know theirs; a game where split screen is neither on nor `asked` for, none.
fn wanted_half(p1: Option<Option<UVec2>>, p2: Option<Option<UVec2>>, asked: bool) -> Option<Option<UVec2>> {
    match (p2, p1) {
        (Some(half), _) | (None, Some(half)) => half.map(Some),
        (None, None) if asked => None,
        (None, None) => Some(None),
    }
}

/// A game starting in split screen: the screen as player 1's file last said (16:9 until
/// there is one), and the layout from it for player 2's game (player 1's own takes its
/// setting).
fn at_start(dir: &Path, p2: bool) -> SplitAtStart {
    let text = std::fs::read_to_string(dir.join("p1.txt")).unwrap_or_default();
    let screen = kv(&text, "screen").and_then(|v| {
        let n: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (n.len() == 4 && n[2] > 0.0 && n[3] > 0.0).then(|| Vec2::new(n[2], n[3]))
    });
    let layout = kv(&text, "layout").map(|l| Layout::parse(&l).setting());
    SplitAtStart { screen: screen.unwrap_or(Vec2::new(1920.0, 1080.0)), layout: if p2 { Some(layout.unwrap_or_default()) } else { None } }
}

// ---- Player 1 -------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum P1State {
    /// Not in the world yet.
    Waiting,
    /// Asked to host; waiting for the lobby to be open.
    Hosting,
    /// Player 2's game is running (or was started).
    Running,
}

#[derive(Default)]
struct PadRoute {
    /// The controller player 1 last pressed a button on.
    p1_last: Option<Entity>,
    /// Player 2's controller, while plugged in.
    p2: Option<Entity>,
    /// Player 2's state as relayed (also its name and ids, to know it again).
    state: PadState,
}

#[derive(Resource)]
struct P1 {
    dir: PathBuf,
    layout: Layout,
    state: P1State,
    socket: Option<UdpSocket>,
    p2_at: Option<SocketAddr>,
    file_at: f32,
    placed: Option<(IVec2, UVec2)>,
    /// The size of this game's half once it is on it (its HUD and menus are drawn for it,
    /// [`split_ui`]).
    half: Option<UVec2>,
    pads: PadRoute,
    /// The host save this session belongs to (its file's stem): the one loaded, then
    /// each one written.
    save: Option<String>,
    /// The lobby was full when player 2 was due to start (noted once).
    full_noted: bool,
    /// A save for the premades was asked for (there was none).
    asked_save: bool,
    /// When player 2's game was last seen running (seconds).
    p2_seen: f32,
    /// This game's window before split screen took half the screen.
    before: Option<WindowBefore>,
    /// This game is a guest in another player's lobby: player 2's game joins that host
    /// through this one (plan §10.1).
    relay: bool,
}

/// A window as it was, to give it back.
#[derive(Debug, Clone)]
struct WindowBefore {
    mode: WindowMode,
    decorations: bool,
    position: WindowPosition,
    size: UVec2,
    /// It already had its half (a run a load started in split screen, placed as the run
    /// before left it): given back maximised, with its frame.
    maximise: bool,
}

impl WindowBefore {
    fn of(w: &Window, half: (IVec2, UVec2)) -> WindowBefore {
        let size = UVec2::new(w.resolution.physical_width(), w.resolution.physical_height());
        let maximise = !w.decorations && w.mode == WindowMode::Windowed && size == half.1;
        WindowBefore { mode: w.mode, decorations: w.decorations || maximise, position: if maximise { WindowPosition::Centered(MonitorSelection::Current) } else { w.position }, size: if maximise { half.1 * 3 / 4 } else { size }, maximise }
    }

    fn give_back(&self, w: &mut Window) {
        w.decorations = self.decorations;
        w.position = self.position;
        w.resolution.set_physical_resolution(self.size.x, self.size.y);
        w.mode = self.mode;
        if self.maximise {
            w.set_maximized(true);
        }
    }
}

/// Lobby closing after split screen, once no guest is left (until `until`).
#[derive(Resource)]
struct CloseLobbyAfter {
    until: f32,
}

/// The menus' asks (`nvf_play::split_screen::SplitScreen`): turns split screen on (player
/// 1's session, [`p1_session`]) or off ([`end_split`]); keeps the entries' state and the
/// command line a load's new run gets ([`with_split_arg`]).
#[allow(clippy::too_many_arguments)]
fn split_asks(
    mut commands: Commands,
    time: Res<Time>,
    mut split: ResMut<SplitScreen>,
    p1: Option<Res<P1>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut connections: MessageWriter<GamepadConnectionEvent>,
    mut saved: MessageReader<nvf_play::save::SaveDone>,
    mut last_save: Local<Option<Option<String>>>,
    layout: Res<SplitLayout>,
) {
    let last_save = last_save.get_or_insert_with(loaded_stem);
    for d in saved.read().filter(|d| d.result.is_ok()) {
        *last_save = d.path.file_stem().map(|s| s.to_string_lossy().into_owned());
    }
    // A guest can play split screen too: player 2 joins its host through it (plan §10.1).
    if !split.available {
        split.available = true;
    }
    match split.ask.take() {
        Some(true) if p1.is_none() => {
            if let Some(home) = fnv_install::EngineHome::default_root() {
                let socket = UdpSocket::bind("127.0.0.1:0").map_err(|e| warn!("split: no controller relay: {e}")).ok();
                // Player 2's game plays only its own interface sounds, so this game hears
                // the world at player 2's stand-in too (coop.rs `second_listener` fills it).
                std::env::set_var("NVF_SPLIT_P1", "1");
                commands.insert_resource(nvf_play::sfx::SecondListener::default());
                commands.remove_resource::<CloseLobbyAfter>();
                commands.insert_resource(P1 {
                    dir: home.join("split"),
                    layout: Layout::of_setting(*layout),
                    state: P1State::Waiting,
                    socket,
                    p2_at: None,
                    file_at: f32::NEG_INFINITY,
                    placed: None,
                    half: None,
                    pads: PadRoute::default(),
                    save: last_save.clone(),
                    full_noted: false,
                    asked_save: false,
                    p2_seen: 0.0,
                    before: None,
                    relay: false,
                });
                split.on = true;
                info!("split: split screen on");
            } else {
                warn!("split: no home folder: split screen not started");
            }
        }
        Some(false) => {
            if let Some(p1) = p1 {
                let mut window = windows.single_mut().ok();
                end_split(&mut commands, &p1, window.as_deref_mut(), &mut connections, time.elapsed_secs(), "asked");
            }
            split.on = false;
        }
        _ => {}
    }
    SPLIT_ARG.store(if split.on || split.on_load { ARG_ON } else { ARG_OFF }, Ordering::Relaxed);
}

/// Ends split screen: player 2's game is told to close (`end=1` in `p1.txt`,
/// [`p2_session`]), this game's window is given back, player 2's controller is this
/// game's again, and the lobby opened for player 2 closes once empty.
fn end_split(commands: &mut Commands, p1: &P1, window: Option<&mut Window>, connections: &mut MessageWriter<GamepadConnectionEvent>, now: f32, why: &str) {
    let file = p1.dir.join("p1.txt");
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    write_file(&file, &format!("{text}end=1\n"));
    let _ = std::fs::remove_file(p1.dir.join("p2.txt"));
    if let (Some(w), Some(before)) = (window, &p1.before) {
        before.give_back(w);
    }
    if let Some(e) = p1.pads.p2 {
        let st = &p1.pads.state;
        connections.write(GamepadConnectionEvent::new(e, GamepadConnection::Connected { name: st.name.clone(), vendor_id: st.vendor, product_id: st.product }));
    }
    std::env::remove_var("NVF_SPLIT_P1");
    commands.remove_resource::<nvf_play::sfx::SecondListener>();
    commands.remove_resource::<P1>();
    if std::env::var_os(OPENED_LOBBY_ENV).is_some() {
        commands.insert_resource(CloseLobbyAfter { until: now + P2_GONE });
    }
    info!("split: split screen ended ({why})");
}

/// After split screen: the lobby opened for player 2 closes once player 2 has left it and
/// no one else is in it; it stays open for anyone still there.
fn close_lobby_after(mut commands: Commands, time: Res<Time>, after: Option<Res<CloseLobbyAfter>>, coop: Option<Res<crate::coop::Coop>>, mut lobby: MessageWriter<nvf_play::lobby::LobbyRequest>) {
    let Some(after) = after else { return };
    if coop.as_deref().is_some_and(|c| c.guest_count() == 0) {
        info!("split: closing the lobby opened for player 2");
        lobby.write(nvf_play::lobby::LobbyRequest::Close);
    } else if time.elapsed_secs() < after.until {
        return;
    }
    std::env::remove_var(OPENED_LOBBY_ENV);
    commands.remove_resource::<CloseLobbyAfter>();
}

fn button_index(b: GamepadButton) -> Option<usize> {
    BUTTONS.iter().position(|x| *x == b)
}

fn axis_index(a: GamepadAxis) -> Option<usize> {
    AXES.iter().position(|x| *x == a)
}

/// Before this game's controller systems: notes which controller player 1 uses, gives
/// player 2 the first other one that presses A, keeps that one out of this game, and
/// relays it.
fn p1_route_pads(mut commands: Commands, mut p1: ResMut<P1>, mut raw: MessageReader<RawGamepadEvent>, pads: Query<(Entity, &Gamepad, Option<&Name>)>) {
    let p1 = &mut *p1;
    let r = &mut p1.pads;
    let running = p1.state == P1State::Running;
    for e in raw.read() {
        match e {
            RawGamepadEvent::Connection(c) => match &c.connection {
                GamepadConnection::Connected { name, vendor_id, product_id } => {
                    let same = !r.state.name.is_empty() && r.state.name == *name && r.state.vendor == *vendor_id && r.state.product == *product_id;
                    if r.p2.is_none() && same && !r.state.connected {
                        r.p2 = Some(c.gamepad);
                        r.state.connected = true;
                        info!("split: player 2's controller is back ({name})");
                    }
                }
                GamepadConnection::Disconnected => {
                    if r.p2 == Some(c.gamepad) {
                        r.p2 = None;
                        r.state.connected = false;
                        r.state.buttons = Default::default();
                        r.state.axes = Default::default();
                        info!("split: player 2's controller unplugged");
                    }
                }
            },
            RawGamepadEvent::Button(RawGamepadButtonChangedEvent { gamepad, button, value }) => {
                if Some(*gamepad) == r.p2 {
                    if let Some(i) = button_index(*button) {
                        r.state.buttons[i] = *value;
                    }
                } else if *value > 0.5 {
                    // Player 2 takes a controller that presses A, not player 1's own.
                    if running && r.p2.is_none() && !r.state.connected && *button == GamepadButton::South && r.p1_last != Some(*gamepad) {
                        let (name, vendor, product) = pads.get(*gamepad).map_or((String::new(), None, None), |(_, g, n)| (n.map_or_else(String::new, |n| n.as_str().to_string()), g.vendor_id(), g.product_id()));
                        info!("split: player 2's controller is {name} ({gamepad})");
                        r.state = PadState { name, vendor, product, connected: true, ..default() };
                        r.p2 = Some(*gamepad);
                    } else {
                        r.p1_last = Some(*gamepad);
                    }
                }
            }
            RawGamepadEvent::Axis(RawGamepadAxisChangedEvent { gamepad, axis, value }) => {
                if Some(*gamepad) == r.p2 {
                    if let Some(i) = axis_index(*axis) {
                        r.state.axes[i] = *value;
                    }
                }
            }
        }
    }
    // Player 2's controller is not this game's.
    if let Some(p2) = r.p2.filter(|e| pads.contains(*e)) {
        commands.entity(p2).remove::<Gamepad>();
    }
    if let (Some(s), Some(at)) = (&p1.socket, p1.p2_at) {
        if r.state.connected || !r.state.name.is_empty() {
            let _ = s.send_to(&r.state.encode(), at);
        }
    }
}

/// Player 1's session: hosting, its window on its half, player 2's game, the files.
/// What starting player 2 reads from player 1's game: its scripts, world and lobby.
type GameParts<'w> = (Option<Res<'w, nvf_play::script::ScriptState>>, Option<Res<'w, nvf_play::PlayWorld>>, Option<Res<'w, crate::coop::Coop>>);

#[allow(clippy::too_many_arguments)]
fn p1_session(
    time: Res<Time>,
    mut p1: ResMut<P1>,
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
    sim: Option<Res<nvf_play::sim::SimState>>,
    mut lobby: MessageWriter<nvf_play::lobby::LobbyRequest>,
    mut windows: Query<(&mut Window, Option<&RawHandleWrapper>), With<PrimaryWindow>>,
    monitors: Query<&Monitor, With<PrimaryMonitor>>,
    mut saved: MessageReader<nvf_play::save::SaveDone>,
    (script, play, coop): GameParts,
    (mut split, mut save_asks, layout): (ResMut<SplitScreen>, MessageWriter<nvf_play::save::SaveRequest>, Res<SplitLayout>),
) {
    let now = time.elapsed_secs();
    // The settings page's layout, at once: both windows are placed again (player 2's
    // follows the file, written now).
    let want = Layout::of_setting(*layout);
    if p1.layout != want {
        info!("split: layout {}", want.key());
        p1.layout = want;
        p1.file_at = f32::NEG_INFINITY;
    }
    for d in saved.read().filter(|d| d.result.is_ok()) {
        p1.save = d.path.file_stem().map(|s| s.to_string_lossy().into_owned());
        p1.file_at = f32::NEG_INFINITY;
    }
    let in_world = sim.is_some() && !main_menu.as_deref().is_some_and(|m| m.open);
    // The screen splits at once [owner]: player 1's half as soon as split screen is on in
    // the world, before player 2's game is up (its half is black until it is).
    if in_world || p1.state != P1State::Waiting {
        if let (Ok((mut window, _)), Ok(monitor)) = (windows.single_mut(), monitors.single()) {
            let mine = halves(monitor.physical_position, UVec2::new(monitor.physical_width, monitor.physical_height), p1.layout)[0];
            if p1.before.is_none() {
                p1.before = Some(WindowBefore::of(&window, mine));
            }
            p1.half = Some(mine.1);
            if place(&mut window, mine) && p1.placed != Some(mine) {
                p1.placed = Some(mine);
                info!("split: player 1's window on {:?} at {}x{}", mine.0, mine.1.x, mine.1.y);
            }
        }
    }
    match p1.state {
        P1State::Waiting if in_world => {
            if coop.as_deref().is_some_and(|c| c.is_guest()) {
                info!("split: this game is a guest in another player's lobby: player 2 joins that host through this game");
                p1.relay = true;
            } else if hosting_port().is_none() {
                // For this PC only: no router port (UPnP), no firewall prompt. Create Lobby
                // opens it to online players later (`crate::coop`).
                info!("split: opening a lobby for player 2 (this PC only)");
                std::env::set_var(crate::coop::HOST_LOCAL_ENV, "1");
                lobby.write(nvf_play::lobby::LobbyRequest::Create);
                std::env::set_var(OPENED_LOBBY_ENV, "1");
            }
            p1.state = P1State::Hosting;
        }
        P1State::Hosting => {
            // The premades are made on this game's newest save: a game with none (a new
            // game) saves one first.
            let saves = fnv_install::EngineHome::user().map(|h| h.saves());
            if saves.as_deref().and_then(newest_save).is_none() {
                if !p1.asked_save {
                    p1.asked_save = true;
                    info!("split: no save yet: saving one for player 2's characters");
                    save_asks.write(nvf_play::save::SaveRequest { name: "Split screen".into(), kind: fnv_save::SaveKind::Auto });
                }
                return;
            }
            let port = if p1.relay { coop.as_deref().and_then(|c| c.open_relay()) } else { hosting_port() };
            if let Some(port) = port {
                let (Ok((_, handle)), Ok(monitor)) = (windows.single(), monitors.single()) else { return };
                let screen = (monitor.physical_position, UVec2::new(monitor.physical_width, monitor.physical_height));
                let p2_half = halves(screen.0, screen.1, p1.layout)[1];
                write_p1_file(&p1.dir, p1.layout, screen, handle.and_then(hwnd_of), p1.save.as_deref());
                if let Some(pid) = read_kv(&p1.dir.join("p2.txt"), "pid").and_then(|v| v.parse::<u32>().ok()).filter(|p| process_alive(*p)) {
                    info!("split: player 2's game is already running (pid {pid}): taking it over");
                } else if !p1.relay && coop.as_deref().is_some_and(|c| c.guest_count() >= nvf_coop::net::MAX_GUESTS) {
                    // Player 2 is one of the lobby's 6 players (the host and MAX_GUESTS
                    // guests): wait for a free place rather than be refused.
                    if !p1.full_noted {
                        p1.full_noted = true;
                        warn!("split: the lobby is full (6 players): player 2 joins when a place is free");
                    }
                    return;
                } else {
                    let data = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| rt.game.data);
                    let game = match (data, sim.as_deref(), play.as_deref()) {
                        (Some(data), Some(sim), Some(play)) => {
                            let opening = fnv_game::split_premade::Opening::find(data, sim.refs);
                            let location = opening.outside.map(|o| nvf_play::console::place_name(&play.0, o.cell.0));
                            Some(PremadeData { data, forms: sim.forms, plugins: nvf_play::save::plugin_list(&play.0), opening, location })
                        }
                        _ => None,
                    };
                    if let Err(e) = start_p2(&p1.dir, port, p2_half, p1.save.as_deref(), game.as_ref()) {
                        warn!("split: player 2's game not started: {e}");
                    }
                }
                p1.state = P1State::Running;
                p1.p2_seen = now;
            }
        }
        _ => {}
    }
    if p1.state != P1State::Running {
        return;
    }
    // This game left the lobby it was a guest in (or the host ended it): player 2's link
    // went through it, so player 2 leaves too (plan §10.1). A load's restart lets go of the
    // link too, but keeps the host to join again (`JOIN_ENV`).
    let left = !coop.as_deref().is_some_and(|c| c.is_guest()) && std::env::var_os(crate::coop::JOIN_ENV).is_none();
    if p1.relay && left && split.ask.is_none() {
        info!("split: this game left the lobby player 2 joined through it: player 2 leaves too");
        split.ask = Some(false);
        return;
    }
    let Ok((mut window, handle)) = windows.single_mut() else { return };
    let Ok(monitor) = monitors.single() else { return };
    let screen = (monitor.physical_position, UVec2::new(monitor.physical_width, monitor.physical_height));
    // Re-applied each frame (`place`).
    let mine = halves(screen.0, screen.1, p1.layout)[0];
    p1.half = Some(mine.1);
    if place(&mut window, mine) && p1.placed != Some(mine) {
        p1.placed = Some(mine);
        info!("split: player 1's window on {:?} at {}x{}", mine.0, mine.1.x, mine.1.y);
    }
    if now - p1.file_at >= FILE_EVERY {
        p1.file_at = now;
        write_p1_file(&p1.dir, p1.layout, screen, handle.and_then(hwnd_of), p1.save.as_deref());
        let p2 = std::fs::read_to_string(p1.dir.join("p2.txt")).unwrap_or_default();
        p1.p2_at = kv(&p2, "port").and_then(|v| v.parse::<u16>().ok()).map(|port| SocketAddr::from(([127, 0, 0, 1], port)));
        // Player 2 went Back from the picker ([`p2_back_ends`]), or its game is gone.
        if kv(&p2, "pid").and_then(|v| v.parse::<u32>().ok()).is_some_and(process_alive) {
            p1.p2_seen = now;
        }
        if kv(&p2, "left").as_deref() == Some("1") {
            info!("split: player 2 left");
            split.ask = Some(false);
        } else if now - p1.p2_seen > P2_GONE {
            info!("split: player 2's game is gone");
            split.ask = Some(false);
        }
    }
}

/// The port this game hosts on, while it hosts (`crate::coop` keeps it in `HOST_ENV`).
fn hosting_port() -> Option<u16> {
    std::env::var(crate::coop::HOST_ENV).ok().and_then(|p| p.trim().parse().ok())
}

/// A borderless window at `rect`; true when anything had to change. Called every frame:
/// leaving full screen restores the window's size from before it (winit), after this
/// frame's size, so a size that didn't hold is asked for again [observed: the first
/// two-window check, 2026-11-20, player 1's window kept 1280 wide].
fn place(window: &mut Window, (at, size): (IVec2, UVec2)) -> bool {
    let mut changed = false;
    if window.mode != WindowMode::Windowed {
        window.mode = WindowMode::Windowed;
        changed = true;
    }
    if window.decorations {
        window.decorations = false;
        changed = true;
    }
    if window.position != WindowPosition::At(at) {
        window.position = WindowPosition::At(at);
        changed = true;
    }
    if window.resolution.physical_width() != size.x || window.resolution.physical_height() != size.y {
        window.resolution.set_physical_resolution(size.x, size.y);
        changed = true;
    }
    changed
}

fn write_p1_file(dir: &Path, layout: Layout, (at, size): (IVec2, UVec2), hwnd: Option<isize>, save: Option<&str>) {
    let text = format!("pid={}\nhwnd={}\nlayout={}\nscreen={},{},{},{}\nsave={}\n", std::process::id(), hwnd.unwrap_or(0), layout.key(), at.x, at.y, size.x, size.y, save.unwrap_or(""));
    write_file(&dir.join("p1.txt"), &text);
}

/// Writes through a temporary file, so the other game never reads half of it.
fn write_file(path: &Path, text: &str) {
    let tmp = path.with_extension("tmp");
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path)).is_err() {
        debug!("split: {} not written", path.display());
    }
}

/// `key=value` from a session file.
fn read_kv(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    kv(&text, key)
}

fn kv(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim().to_string()))
}

/// What making the premades' saves needs of this game: its data (editor IDs, leveled
/// lists) and its load order.
struct PremadeData {
    data: &'static fnv_game::GameData,
    forms: &'static fnv_sim::FormStore,
    plugins: Vec<fnv_save::PluginEntry>,
    /// Doc Mitchell's opening done, for the premades' defaults.
    opening: fnv_game::split_premade::Opening,
    /// The name of the place outside Doc Mitchell's house, for the load list.
    location: Option<String>,
}

/// Starts player 2's game: this program again, joining this game, in its own home on
/// player 2's half. Its saves are the four premades ([`premade_saves`]), and it starts on
/// its main menu with the guest's join screen up ([`p2_open_join`]), so player 2 picks one
/// there (plan §7).
fn start_p2(dir: &Path, port: u16, (at, size): (IVec2, UVec2), host_save: Option<&str>, game: Option<&PremadeData>) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let home = dir.join("p2-home");
    std::fs::create_dir_all(home.join("saves")).map_err(|e| e.to_string())?;
    let mine = fnv_install::EngineHome::user().ok_or("no home folder")?;
    // Player 2 plays with player 1's settings.
    let _ = std::fs::copy(mine.config_file(), fnv_install::EngineHome::new(&home).config_file());
    let made = premade_saves(&mine.saves(), &home, host_save, game)?;
    let mut args = crate::restart_args(&std::env::args().skip(1).collect::<Vec<_>>());
    strip_coop_args(&mut args);
    args.extend(["--menu".into(), "--split-p2".into(), dir.display().to_string(), "--size".into(), format!("{}x{}", size.x, size.y), "--window-at".into(), format!("{},{}", at.x, at.y)]);
    let mut cmd = std::process::Command::new(exe);
    // Test aids `NVF_SPLIT_TEST_P2_<NAME>`: player 2's game gets each as its own
    // `NVF_PLAY_TEST_<NAME>` (e.g. `NVF_SPLIT_TEST_P2_LOBBY=25:pick=0`,
    // `NVF_SPLIT_TEST_P2_SHOT_DIR`, `NVF_SPLIT_TEST_P2_GOTO`), so the two games' aids
    // and shots of one moment don't collide; player 1's game keeps the plain ones.
    for (name, value) in std::env::vars_os() {
        let Some(rest) = name.to_str().and_then(|n| n.strip_prefix("NVF_SPLIT_TEST_P2_")) else { continue };
        cmd.env(format!("NVF_PLAY_TEST_{rest}"), value).env_remove(&name);
    }
    let child = cmd
        .args(&args)
        .env(fnv_install::HOME_ENV, &home)
        .env(crate::coop::JOIN_ENV, format!("127.0.0.1:{port}"))
        .env(crate::coop::NAME_ENV, P2_NAME)
        .env(P2_ENV, "1")
        .env_remove("NVF_SPLIT_P1")
        .env(nvf_play::coop_progress::SYNC_ENV, "1")
        .env_remove(crate::coop::HOST_ENV)
        .env_remove(crate::coop::HOST_LOCAL_ENV)
        .env_remove(crate::coop::RELAY_ENV)
        .env_remove(crate::coop::CODE_ENV)
        .env_remove(crate::coop::TOKEN_ENV)
        .spawn()
        .map_err(|e| e.to_string())?;
    info!("split: player 2's game started (pid {}) to pick from {made}", child.id());
    write_file(&dir.join("p2.txt"), &format!("pid={}\n", child.id()));
    Ok(())
}

/// Player 2's saves: one per premade, `<p2 home>\saves\<Name>.nvfsave`, named after its
/// premade so player 2's game knows which it plays ([`premade_of`]):
/// - the premade as it was played in host save `host_save` ([`premade_file`]), when
///   there is one;
/// - else its defaults on top of this game's newest save
///   (`fnv_game::split_premade::premade_snapshot`).
///
/// Any other save there (those player 2's game made following the host's) moves to
/// `<p2 home>\old-saves`, so the join screen lists the four only. Returns what was made,
/// for the log.
fn premade_saves(host_saves: &Path, p2_home: &Path, host_save: Option<&str>, game: Option<&PremadeData>) -> Result<String, String> {
    use fnv_game::split_premade::{premade_snapshot, resolve, PREMADES};
    let saves = p2_home.join("saves");
    let names: Vec<String> = PREMADES.iter().map(|p| format!("{}.{}", p.name, fnv_save::EXTENSION)).collect();
    if let Ok(entries) = std::fs::read_dir(&saves) {
        let old = p2_home.join("old-saves");
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if e.path().is_file() && !names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                let _ = std::fs::create_dir_all(&old);
                if let Err(err) = std::fs::rename(e.path(), old.join(&name)) {
                    warn!("split: {name} left in player 2's saves: {err}");
                }
            }
        }
    }
    // The base of the premades that have no file of this host save: this game's newest
    // save, read once.
    let mut base: Option<Result<Base, String>> = None;
    let mut made = Vec::new();
    for (n, p) in PREMADES.iter().enumerate() {
        let to = saves.join(&names[n]);
        if let Some(kept) = host_save.map(|stem| premade_file(host_saves, stem, n)).filter(|f| f.is_file()) {
            match std::fs::copy(&kept, &to) {
                Ok(_) => {
                    made.push(format!("{} (kept with {})", p.name, host_save.unwrap_or_default()));
                    continue;
                }
                Err(e) => warn!("split: {} not copied: {e}: its defaults instead", kept.display()),
            }
        }
        let Some(game) = game else { return Err("the game's data is not loaded yet".into()) };
        let b = match base.get_or_insert_with(|| read_base(host_saves, &game.plugins)) {
            Ok(b) => b,
            Err(e) => return Err(e.clone()),
        };
        let r = resolve(p, |e| game.data.by_edid(e), game.forms, b.header.saved_unix ^ (n as u64 + 1));
        if !r.missing.is_empty() {
            warn!("split: {}: not in the load order: {}", p.name, r.missing.join(", "));
        }
        let mut body = b.body.clone();
        body.game = premade_snapshot(&body.game, p, &r, &game.opening);
        body.engine.coop.clear();
        body.engine.magazine = None;
        body.engine.magazine_ammo = None;
        if game.opening.done.as_ref().is_some_and(|d| body.engine.active_quest == Some(d.quest.0)) {
            body.engine.active_quest = None;
        }
        let mut h = b.header.clone();
        h.kind = fnv_save::SaveKind::Manual;
        h.player_name = p.name.to_string();
        h.level = 1;
        h.coop.clear();
        h.plugins = game.plugins.clone();
        // Its place: right outside Doc Mitchell's house (where a load starts it).
        if let Some(o) = game.opening.outside {
            h.place = fnv_save::Place { world: o.world.map(|w| w.0).or(h.place.world), interior: None, feet: o.pos, heading_deg: o.rot[2].to_degrees(), pitch_deg: 0.0 };
            if let Some(l) = &game.location {
                h.location = l.clone();
            }
        }
        let bytes = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
        fnv_save::write(&to, &h, &b.shot, &bytes, false).map_err(|e| e.to_string())?;
        made.push(format!("{} (defaults)", p.name));
    }
    Ok(made.join(", "))
}

/// The save the premades' defaults are made on.
struct Base {
    header: fnv_save::SaveHeader,
    shot: Vec<u8>,
    body: nvf_play::save::SaveBody,
}

/// This game's newest save, remapped to the load order running now.
fn read_base(host_saves: &Path, plugins: &[fnv_save::PluginEntry]) -> Result<Base, String> {
    let path = newest_save(host_saves).ok_or("no save for player 2's characters yet: save once first")?;
    let shot = fnv_save::read(&path).map(|f| f.shot).unwrap_or_default();
    let (header, body, _) = nvf_play::save::read_for_load(&path, plugins)?;
    Ok(Base { header, shot, body })
}

/// Takes this game's own co-op and split arguments out of a command line for player 2's.
fn strip_coop_args(args: &mut Vec<String>) {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let name = a.split('=').next().unwrap_or(a);
        match name {
            "--split" => i += 1,
            "--host" | "--join" | "--coop-name" | "--split-p2" => i += if a.contains('=') { 1 } else { 2 },
            _ => {
                out.push(a.clone());
                i += 1;
            }
        }
    }
    *args = out;
}

/// The premade a save is of, from its name: player 2's saves are named after their
/// premades ([`premade_saves`]).
fn premade_of(stem: &str) -> Option<usize> {
    fnv_game::split_premade::by_name(stem).map(|(n, _)| n)
}

/// Whether this run plays one of player 2's premades: it was started with `--load
/// <Premade>.nvfsave` (player 2's picker loads one that way). Player 2's first run, the
/// picker, plays none.
pub fn plays_premade() -> bool {
    static PLAYS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PLAYS.get_or_init(|| loaded_stem().and_then(|s| premade_of(&s)).is_some())
}

/// The stem of the save this run was started with (`--load`), if any.
fn loaded_stem() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let path = args.iter().position(|a| a == "--load").and_then(|i| args.get(i + 1).cloned()).or_else(|| args.iter().find_map(|a| a.strip_prefix("--load=").map(String::from)))?;
    Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned())
}

/// Where player 2's character played in host save `stem` is kept: beside the host's saves,
/// `saves\split\<stem>\premade-<n>.nvfsave` (plan §7.2).
pub fn premade_file(host_saves: &Path, stem: &str, n: usize) -> PathBuf {
    host_saves.join("split").join(stem).join(format!("premade-{}.{}", n + 1, fnv_save::EXTENSION))
}

fn newest_save(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "nvfsave"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

// ---- Player 2 -------------------------------------------------------------------------

#[derive(Resource)]
struct P2 {
    dir: PathBuf,
    socket: Option<UdpSocket>,
    /// The relayed controller's entity in this game.
    pad: Option<Entity>,
    last: PadState,
    owner: Option<isize>,
    placed: Option<(IVec2, UVec2)>,
    /// The size of this game's half, from player 1's file ([`split_ui`]).
    half: Option<UVec2>,
    /// When player 1's game was last seen running (seconds).
    p1_seen: f32,
    file_at: f32,
    /// Which premade this game plays (from the save it loaded), if one.
    premade: Option<usize>,
    /// The guest's join screen was opened on the main menu ([`p2_open_join`]).
    join_opened: bool,
    /// The window was put back above player 1's once (logged once).
    restacked: bool,
}

/// On the relayed controller: not one of this machine's real ones.
#[derive(Component)]
struct RelayedPad;

/// Plays the relayed controller as this game's own: connected, its buttons and axes as
/// they change.
fn p2_receive(mut commands: Commands, mut p2: ResMut<P2>, mut connections: MessageWriter<GamepadConnectionEvent>, mut raw: MessageWriter<RawGamepadEvent>) {
    let p2 = &mut *p2;
    let Some(s) = &p2.socket else { return };
    let mut newest = None;
    let mut buf = [0u8; 512];
    while let Ok(n) = s.recv(&mut buf) {
        if let Some(st) = PadState::decode(&buf[..n]) {
            newest = Some(st);
        }
    }
    let Some(st) = newest else { return };
    if st.connected && p2.pad.is_none() {
        let e = commands.spawn(RelayedPad).id();
        let c = GamepadConnectionEvent::new(e, GamepadConnection::Connected { name: st.name.clone(), vendor_id: st.vendor, product_id: st.product });
        connections.write(c.clone());
        raw.write(RawGamepadEvent::Connection(c));
        p2.pad = Some(e);
        p2.last = PadState { connected: true, ..st.clone() };
        p2.last.buttons = Default::default();
        p2.last.axes = Default::default();
        info!("split: player 2's controller connected ({})", st.name);
    } else if !st.connected {
        if let Some(e) = p2.pad.take() {
            let c = GamepadConnectionEvent::new(e, GamepadConnection::Disconnected);
            connections.write(c.clone());
            raw.write(RawGamepadEvent::Connection(c));
            info!("split: player 2's controller unplugged");
        }
        p2.last = st;
        return;
    }
    let Some(e) = p2.pad else { return };
    for (i, b) in BUTTONS.iter().enumerate() {
        if st.buttons[i] != p2.last.buttons[i] {
            raw.write(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(e, *b, st.buttons[i])));
        }
    }
    for (i, a) in AXES.iter().enumerate() {
        if st.axes[i] != p2.last.axes[i] {
            raw.write(RawGamepadEvent::Axis(RawGamepadAxisChangedEvent::new(e, *a, st.axes[i])));
        }
    }
    p2.last = st;
}

/// This machine's real controllers are player 1's: this game reads only the relayed one.
fn p2_strip_real_pads(mut commands: Commands, pads: Query<Entity, (With<Gamepad>, Without<RelayedPad>)>) {
    for e in &pads {
        commands.entity(e).remove::<Gamepad>();
    }
}

/// Player 2's session: its window on its half, owned by player 1's; its files; quitting
/// when player 1's game is gone.
fn p2_session(time: Res<Time>, mut p2: ResMut<P2>, mut windows: Query<(&mut Window, Option<&RawHandleWrapper>), With<PrimaryWindow>>, mut exit: MessageWriter<AppExit>) {
    let now = time.elapsed_secs();
    if now - p2.file_at < FILE_EVERY {
        return;
    }
    p2.file_at = now;
    let Ok((mut window, handle)) = windows.single_mut() else { return };
    let text = std::fs::read_to_string(p2.dir.join("p1.txt")).unwrap_or_default();
    // Player 1 ended split screen ([`end_split`]).
    if kv(&text, "end").as_deref() == Some("1") {
        info!("split: player 1 ended split screen: quitting");
        exit.write(AppExit::Success);
        return;
    }
    let port = p2.socket.as_ref().and_then(|s| s.local_addr().ok()).map_or(0, |a| a.port());
    write_file(&p2.dir.join("p2.txt"), &format!("pid={}\nport={port}\n", std::process::id()));
    let p1_alive = kv(&text, "pid").and_then(|v| v.parse::<u32>().ok()).is_some_and(process_alive);
    if p1_alive || p2.p1_seen == 0.0 {
        p2.p1_seen = now.max(f32::MIN_POSITIVE);
    } else if now - p2.p1_seen > P1_GONE {
        info!("split: player 1's game is gone: quitting");
        exit.write(AppExit::Success);
        return;
    }
    let layout = kv(&text, "layout").map_or(Layout::TopBottom, |l| Layout::parse(&l));
    if let Some(v) = kv(&text, "screen") {
        let n: Vec<i64> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let [x, y, w, h] = n[..] {
            let half = halves(IVec2::new(x as i32, y as i32), UVec2::new(w.max(2) as u32, h.max(2) as u32), layout)[1];
            p2.half = Some(half.1);
            if place(&mut window, half) && p2.placed != Some(half) {
                p2.placed = Some(half);
                info!("split: player 2's window on {:?} at {}x{}", half.0, half.1.x, half.1.y);
            }
        }
    }
    let owner = kv(&text, "hwnd").and_then(|v| v.parse::<isize>().ok()).filter(|h| *h != 0);
    if let (Some(owner), Some(mine)) = (owner, handle.and_then(hwnd_of)) {
        if p2.owner != Some(owner) {
            own_by(mine, owner);
            p2.owner = Some(owner);
            info!("split: player 2's window owned by player 1's");
            raise_over(mine, owner);
        }
        // Setting the owner after the window was made doesn't stack it again, and a window
        // that is never activated is never brought up, so it could stay under other
        // windows [observed: the 11:27 and 11:46 checks, player 2's half showed the
        // desktop]: so this one is kept right above player 1's in the stack, whatever is in
        // front (a dialog can hold the foreground [observed: the 13:25 demo run]), without
        // taking the focus.
        if raise_over(mine, owner) && !p2.restacked {
            p2.restacked = true;
            info!("split: player 2's window stacked above player 1's");
        }
    }
}

/// Each save this game makes (it follows every host save, `nvf_play::coop_progress`) is
/// player 2's character in that host save: copied beside the host's saves, under the
/// host save's name, so loading that host save again offers it (plan §7.2) [owner: saved
/// when the host saves, tied to that host save].
fn p2_keep_character(p2: Res<P2>, mut saved: MessageReader<nvf_play::save::SaveDone>) {
    for d in saved.read().filter(|d| d.result.is_ok()) {
        let Some(n) = p2.premade else { continue };
        let Some(stem) = read_kv(&p2.dir.join("p1.txt"), "save").filter(|s| !s.is_empty()) else { continue };
        let Some(host_saves) = p2.dir.parent().map(|h| h.join("saves")) else { continue };
        let to = premade_file(&host_saves, &stem, n);
        let tmp = to.with_extension("tmp");
        let ok = to.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && std::fs::copy(&d.path, &tmp).is_ok() && std::fs::rename(&tmp, &to).is_ok();
        if ok {
            info!("split: player 2's {} kept with host save {stem}", fnv_game::split_premade::PREMADES[n].name);
        } else {
            warn!("split: player 2's character not kept with host save {stem}");
        }
    }
}

/// Player 2's game starts on its main menu, joining player 1's (`NVF_COOP_JOIN`). The
/// main menu doesn't open the join screen by itself (only its Join Lobby entry does,
/// `nvf_play::main_menu`), so this opens it once; its list of saves, the four premades,
/// is player 2's picker. Picking one loads it and joins (`crate::coop`,
/// `LobbyRequest::Load`), in a new run started with `--load <Name>.nvfsave`.
fn p2_open_join(mut p2: ResMut<P2>, main_menu: Option<Res<nvf_play::main_menu::MainMenu>>, screen: Option<ResMut<nvf_play::lobby::LobbyScreen>>) {
    if p2.join_opened || p2.premade.is_some() || !main_menu.is_some_and(|m| m.open) {
        return;
    }
    let Some(mut screen) = screen else { return };
    if !screen.is_open() {
        screen.open(nvf_play::lobby::Mode::Join);
        info!("split: player 2 picks a character");
    }
    p2.join_opened = true;
}

/// Back on the picker ends split screen [design: the simpler of reopening the picker and
/// ending; owner 2026-11-20 left the choice]: the join screen's Back asks to stop joining
/// (`LobbyRequest::Cancel`), so player 2's game tells player 1's it left (`left=1` in
/// `p2.txt`) and quits; player 1's gets the whole screen back ([`p1_session`]).
fn p2_back_ends(p2: Res<P2>, mut asks: MessageReader<nvf_play::lobby::LobbyRequest>, mut exit: MessageWriter<AppExit>) {
    let back = asks.read().any(|a| *a == nvf_play::lobby::LobbyRequest::Cancel);
    if !back || p2.premade.is_some() {
        return;
    }
    info!("split: player 2 went back from the picker: leaving split screen");
    write_file(&p2.dir.join("p2.txt"), &format!("pid={}\nleft=1\n", std::process::id()));
    exit.write(AppExit::Success);
}

// ---- Windows --------------------------------------------------------------------------

fn hwnd_of(h: &RawHandleWrapper) -> Option<isize> {
    match h.get_window_handle() {
        wgpu::rwh::RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
        _ => None,
    }
}

/// Makes `window` owned by `owner` (they move, minimise and stay on top together) and
/// never activated by a click, so the keyboard stays with the owner.
#[cfg(windows)]
fn own_by(window: isize, owner: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWLP_HWNDPARENT, GWL_EXSTYLE, WS_EX_NOACTIVATE};
    let hwnd = window as windows_sys::Win32::Foundation::HWND;
    // SAFETY: both are top-level windows of this machine's games; the calls only change
    // window attributes, and fail harmlessly on a window that has gone.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, owner);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE as isize);
    }
}

#[cfg(not(windows))]
fn own_by(_window: isize, _owner: isize) {}

/// Puts `window` directly above `owner` in the stack (at the top when `owner` is), without
/// activating it, so the keyboard stays where it is; true when it had to move and Windows
/// moved it.
#[cfg(windows)]
fn raise_over(window: isize, owner: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindow, GetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GW_HWNDPREV, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_TOPMOST};
    type Hwnd = windows_sys::Win32::Foundation::HWND;
    // SAFETY: top-level windows of the two players' games; the calls only read and change
    // the stacking order, and fail harmlessly on a window that has gone.
    unsafe {
        let above = GetWindow(owner as Hwnd, GW_HWNDPREV);
        if above as isize == window {
            return false;
        }
        // Inserting after an always-on-top window would make this one always on top too.
        let topmost = !above.is_null() && GetWindowLongPtrW(above, GWL_EXSTYLE) & WS_EX_TOPMOST as isize != 0;
        let after = if above.is_null() || topmost { HWND_TOP } else { above };
        SetWindowPos(window as Hwnd, after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) != 0
    }
}

#[cfg(not(windows))]
fn raise_over(_window: isize, _owner: isize) -> bool {
    false
}

/// Whether process `pid` is running.
#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: the handle is checked and closed here.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0;
        CloseHandle(h);
        ok && code == STILL_ACTIVE as u32
    }
}

#[cfg(not(windows))]
fn process_alive(_pid: u32) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_and_bottom_halves_cover_the_screen() {
        let [a, b] = halves(IVec2::new(-1920, 0), UVec2::new(1920, 1081), Layout::TopBottom);
        assert_eq!(a, (IVec2::new(-1920, 0), UVec2::new(1920, 540)));
        assert_eq!(b, (IVec2::new(-1920, 540), UVec2::new(1920, 541)));
    }

    #[test]
    fn side_by_side_halves_cover_the_screen() {
        let [a, b] = halves(IVec2::ZERO, UVec2::new(2561, 1440), Layout::SideBySide);
        assert_eq!(a, (IVec2::ZERO, UVec2::new(1280, 1440)));
        assert_eq!(b, (IVec2::new(1280, 0), UVec2::new(1281, 1440)));
    }

    #[test]
    fn the_relay_round_trips_a_controller() {
        let mut s = PadState { name: "Xbox Wireless Controller".into(), vendor: Some(0x045E), product: None, connected: true, ..default() };
        s.buttons[0] = 1.0;
        s.axes[3] = -0.25;
        assert_eq!(PadState::decode(&s.encode()), Some(s));
        assert_eq!(PadState::decode(b"XXXX"), None);
    }

    #[test]
    fn player_2s_command_line_drops_this_games_coop_arguments() {
        let mut a: Vec<String> = ["--play", "--split", "--host", "47960", "--coop-name=Host", "--data", "D"].map(String::from).to_vec();
        strip_coop_args(&mut a);
        assert_eq!(a, ["--play", "--data", "D"]);
    }

    #[test]
    fn a_loads_new_run_keeps_split_screen_as_it_is_now() {
        let line = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let mut a = line(&["--play", "--data", "D"]);
        split_arg(&mut a, ARG_ON);
        assert_eq!(a, line(&["--play", "--data", "D", "--split"]));
        split_arg(&mut a, ARG_ON);
        assert_eq!(a.iter().filter(|x| *x == "--split").count(), 1);
        split_arg(&mut a, ARG_OFF);
        assert_eq!(a, line(&["--play", "--data", "D"]));
        let mut b = line(&["--play", "--split"]);
        split_arg(&mut b, 0);
        assert_eq!(b, line(&["--play", "--split"]));
    }

    #[test]
    fn the_interface_follows_the_half_while_split_and_comes_back_after() {
        let half = UVec2::new(2560, 720);
        // Player 2's game: its half once player 1's file says it; not before.
        assert_eq!(wanted_half(None, Some(Some(half)), false), Some(Some(half)));
        assert_eq!(wanted_half(None, Some(None), false), None);
        // Player 1's: its half once its window is put on it (Add Player 2, a layout change).
        assert_eq!(wanted_half(Some(Some(UVec2::new(1280, 1440))), None, false), Some(Some(UVec2::new(1280, 1440))));
        assert_eq!(wanted_half(Some(None), None, false), None, "split on, not in the world yet: as it is");
        // Asked for, not on yet (a game started with --split): as it is.
        assert_eq!(wanted_half(None, None, true), None);
        // Split screen over (or never on): the run's own scales.
        assert_eq!(wanted_half(None, None, false), Some(None));
    }

    #[test]
    fn session_files_read_by_key() {
        let t = "pid=12\nhwnd=0\nlayout=side-by-side\n";
        assert_eq!(kv(t, "pid").as_deref(), Some("12"));
        assert_eq!(Layout::parse(&kv(t, "layout").unwrap()), Layout::SideBySide);
        assert_eq!(kv(t, "port"), None);
    }
}
