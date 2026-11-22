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
//!   move, minimise and stay on top together) and never keeps the focus, so the keyboard
//!   stays player 1's (Focus, below) [design]. It quits when player 1's game has been gone for [`P1_GONE`].
//! - **Characters** (plan §7): player 2 plays one of four premades
//!   (`fnv_game::split_premade`). Player 1's game writes one save per premade into player
//!   2's home ([`premade_saves`]) and starts player 2's on its main menu, joining, with the
//!   join screen up: its list of saves is the picker. What player 2's game saves is kept
//!   with the host save (`saves\split\<stem>\premade-<n>.nvfsave`, [`premade_file`]).
//! - **Session files** in `<player 1's home>\split\`: `p1.txt` (player 1's process, window,
//!   the layout, the screen's rectangle and the loopback port player 2's game joins, which
//!   it follows when a load's restart had to take another) and `p2.txt` (player 2's process and the port
//!   its controller arrives on). Player 2's own home is `split\p2-home` beside them.
//! - **Controllers** [owner 2026-11-20: "each connected controller/ mouse and keyboard
//!   should be assigned to a window when you start up split screen"]. Player 1's game
//!   reads every controller and gives them out on split screen's first frame, however it
//!   started (Add Player 2, the main menu's entry, `--split`, a load's restart;
//!   [`assign_pads`]): the keyboard and mouse are player 1's; so is the controller player 1
//!   last used ([`LastPad`]), or, for a player 1 who plays on a controller (the Input Device
//!   setting) and used none yet, the first connected; player 2 gets the next connected one.
//!   A load's new run gives player 2 the controller it had (`p2pad` in `p1.txt`). One
//!   plugged in later while player 2 has none is player 2's ([`plugged_in_goes_to`]); the
//!   first other one that presses A (South) while player 2 has none is too, a fallback.
//!   The assignment is logged and shown in player 1's corner messages. Player 1's game never
//!   acts on player 2's controller (its `Gamepad` is taken off, so nothing in this game
//!   reads it) and relays its buttons and sticks to player 2's game every frame (127.0.0.1
//!   UDP). Player 2's game reads no real controller and plays the relayed one as a
//!   controller of its own, so its menus and controls work unchanged. A controller that
//!   comes back after being unplugged (same name, vendor and product) is player 2's again.
//!   Ending split screen gives every device back to player 1's game.
//! - **Focus.** Windows sends the keyboard only to the window with the focus [documented:
//!   Win32 keyboard input, the foreground window's thread], winit sends raw mouse motion
//!   only while one of the program's windows has it (`DeviceEvents::WhenFocused`, its
//!   default) and Windows Gaming Input, which reads the controllers (gilrs), gives a program
//!   controller input only while a window of its own has it [documented: gilrs 0.11
//!   lib.rs, "Windows Gaming Input requires an in focus window"]. So both players' input
//!   rests on player 1's window keeping the focus, and player 2's never using what it gets:
//!   - player 2's game drops this PC's keyboard and mouse before anything sees them
//!     (`bevy::winit::hardware_input`, the vendored bevy_winit), whichever window has the
//!     focus; test aids' own input still works there;
//!   - player 2's window is borderless from its start and never activated by a click
//!     (`WS_EX_NOACTIVATE`); winit shows a window again, activating it, when a flag such
//!     as its frame changes after its first show, which is how player 2's window could take
//!     the focus [documented: winit 0.30 `WindowFlags::apply_diff`] (the owner's report of
//!     both players' input in one window);
//!   - should player 2's window have the focus all the same, its game hands it straight
//!     back to player 1's window, the next frame ([`p2_give_focus_back`]; a program may give
//!     the focus away while it holds it).
//!
//! - **Player 2's start** ([`start_p2`]): its command line is player 1's as acted on
//!   ([`p2_args`]; a game started with no arguments, as from Mod Organizer 2, has its
//!   `--play` only there), its own console goes to `split\p2-console.txt`, so what it says
//!   before its log is up is kept, and player 1's game watches its process: one that ends
//!   with an error ends split screen at once, with player 2's console's last lines in player
//!   1's log and a corner message ([`p2_ended`]) [observed: the owner's runs of 2026-11-21
//!   from Mod Organizer 2, player 2's game refused its command line and no window came].
//! - **The taskbar**: neither half covers the monitor, so Windows kept the taskbar over
//!   their bottom edge; player 1's window is marked full screen for the taskbar while split
//!   ([`mark_fullscreen`]) and unmarked when it ends; that alone did not lower it on
//!   Windows 11, so player 1's window is also topmost while either half has the focus
//!   ([`set_on_top`]).
//! - **The backdrop** ([`backdrop`]): a black window over the whole monitor, under both
//!   halves, from split screen's first frame till it ends, so nothing behind shows while
//!   the windows move or player 2's game loads (it says player 2 is joining meanwhile).
//!
//! Log: `split: split screen on`, `split: split screen ended (…)`,
//! `split: player 2's command line: …`, `split: player 2's game (pid …) ended (…)`,
//! `split: player 1's window marked full screen for the taskbar …`,
//! `split: player 2's game started (pid …)`, `split: backdrop up under the halves …`,
//! `split: backdrop: …`, `split: backdrop gone (…)`, `split: the halves are over the taskbar`, `split: controllers assigned (…)`,
//! `split: player 2's controller is …`, `split: player 2's controller unplugged`,
//! `split: every controller is player 1's again …`, `split: focus: …`, `split: input: …`,
//! `split: player 1's game is gone: quitting`.

use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

use bevy::input::gamepad::{gamepad_connection_system, gamepad_event_processing_system, Gamepad, GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent, RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, Monitor, PrimaryMonitor, PrimaryWindow, RawHandleWrapper, WindowMode};
use nvf_play::split_screen::{SplitAtStart, SplitLayout, SplitScreen};

// The field of view of a half (view.rs).
pub(crate) mod backdrop;
pub(crate) mod sound;
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
        trace::setup(app);
        if let Some(dir) = self.0.p2.clone() {
            // Its HUD and menus are drawn for its half (player 1's file says which).
            app.insert_resource(at_start(&dir, true));
            let socket = UdpSocket::bind("127.0.0.1:0").and_then(|s| s.set_nonblocking(true).map(|_| s)).map_err(|e| warn!("split: no controller relay: {e}")).ok();
            // This PC's keyboard and mouse are player 1's, whichever window has the focus
            // (module notes, Focus): dropped before anything in this game sees them.
            bevy::winit::hardware_input::set_ignored(true);
            info!("split: player 2's game ignores this PC's keyboard and mouse (they are player 1's); it plays the controller player 1's game relays");
            app.insert_resource(nvf_play::audio_out::SplitSound::default()).insert_resource(nvf_play::split_screen::PlayerTwo).insert_resource(P2 { dir, socket, pad: None, last: PadState::default(), owner: None, mine: None, placed: None, half: None, p1_seen: 0.0, file_at: f32::NEG_INFINITY, premade: loaded_stem().and_then(|s| premade_of(&s)), join_opened: false, restacked: false })
                .init_resource::<InputCount>()
                .add_systems(PreUpdate, p2_receive.before(gamepad_connection_system))
                .add_systems(PreUpdate, p2_strip_real_pads.after(gamepad_connection_system).before(gamepad_event_processing_system))
                .add_systems(PreUpdate, p2_count_input.before(bevy::input::InputSystems))
                .add_systems(Update, (p2_session, p2_give_focus_back.after(p2_session), p2_keep_character, p2_open_join, p2_back_ends, split_ui.after(p2_session), sound::p2_sound));
        } else {
            // Player 1's game, or one that may become it (the menus' entries).
            nvf_play::split_screen::register_layout(app, "Display");
            if self.0.start {
                if let Some(home) = fnv_install::EngineHome::default_root() {
                    app.insert_resource(at_start(&home.join("split"), false));
                }
            }
            app.insert_resource(SplitScreen { available: true, ask: self.0.start.then_some(true), ..default() })
                .init_resource::<LastPad>()
                .init_resource::<InputCount>()
                .add_systems(PreUpdate, note_last_pad.after(gamepad_connection_system).before(gamepad_event_processing_system))
                .add_systems(PreUpdate, p1_route_pads.run_if(resource_exists::<P1>).after(note_last_pad).before(gamepad_event_processing_system))
                .add_systems(PreUpdate, p1_count_input.run_if(resource_exists::<P1>).before(bevy::input::InputSystems))
                .add_systems(Update, (split_asks, p1_session.run_if(resource_exists::<P1>), p1_watch_focus.run_if(resource_exists::<P1>), close_lobby_after, split_ui, p1_backdrop, sound::p1_sound).chain());
            test_pads::setup(app);
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
    /// The controllers were assigned (split screen's first frame, [`assign_pads`]).
    assigned: bool,
    /// Player 1's controller, if it has one (it always has the keyboard and mouse).
    p1: Option<Entity>,
    /// Player 2's controller, while plugged in.
    p2: Option<Entity>,
    /// Player 2's state as relayed (also its name and ids, to know it again).
    state: PadState,
    /// Player 2's controller's place among the connected ones when it was given (for
    /// `p1.txt`: a load's new run gives it the same one, [`kept_p2_pad`]).
    p2_place: Option<usize>,
    /// Player 2's controller's input was relayed (logged once).
    relayed_noted: bool,
}

/// The controller this game's player last used (a button pressed, or a stick or trigger
/// pushed well past its dead zone), before and during split screen: player 1's at split
/// screen's start ([`assign_pads`]).
#[derive(Resource, Default)]
struct LastPad(Option<Entity>);

/// A controller's name, vendor and product.
type PadKind = (String, Option<u16>, Option<u16>);

/// Who gets which controller when split screen starts (module notes, Controllers): the
/// keyboard and mouse are always player 1's. Player 1 keeps the controller it last used
/// (`last`); a player 1 who plays on a controller (`pad_player`: the Input Device setting)
/// and used none yet takes the first one. Player 2 takes the next one connected. `connected`
/// is in the order the controllers were connected; `kept` is player 2's from before a
/// load's restart ([`kept_p2_pad`]), which stays player 2's. Returns (player 1's, player 2's).
fn assign_pads(connected: &[Entity], last: Option<Entity>, pad_player: bool, kept: Option<Entity>) -> (Option<Entity>, Option<Entity>) {
    let kept = kept.filter(|k| connected.contains(k));
    let p1 = last.filter(|l| connected.contains(l) && Some(*l) != kept).or_else(|| if pad_player { connected.iter().copied().find(|e| Some(*e) != kept) } else { None });
    let p2 = kept.or_else(|| connected.iter().copied().find(|e| Some(*e) != p1));
    (p1, p2)
}

/// Who takes a controller plugged in while split screen is on: player 1 when it plays on
/// a controller (`pad_player`) and has none, else player 2 when it has none; otherwise no
/// one (it stays this game's, as an unassigned one does, and pressing A on it gives it to
/// player 2 once player 2 has none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PluggedIn {
    PlayerOne,
    PlayerTwo,
    NoOne,
}

fn plugged_in_goes_to(p1_has: bool, p2_has: bool, pad_player: bool) -> PluggedIn {
    if pad_player && !p1_has {
        PluggedIn::PlayerOne
    } else if !p2_has {
        PluggedIn::PlayerTwo
    } else {
        PluggedIn::NoOne
    }
}

/// Player 2's controller as a load's new run of player 1's game finds it again: the one at
/// the same place among the connected ones (`place`), if it is the same kind (name, vendor
/// and product), else the first of that kind (`kinds[i]` is connected controller `i`'s).
fn kept_p2_pad(kinds: &[PadKind], place: Option<usize>, kind: &PadKind) -> Option<usize> {
    if kind.0.is_empty() {
        return None;
    }
    place.filter(|p| kinds.get(*p) == Some(kind)).or_else(|| kinds.iter().position(|k| k == kind))
}

/// Keyboard and mouse input this PC gave this game (counted each frame, logged once a
/// second while split screen is on): the evidence of which game it reaches.
#[derive(Resource, Default)]
struct InputCount {
    keys: u32,
    buttons: u32,
    motion: u32,
    logged_at: f32,
    dropped_logged: u64,
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
    /// The loopback port player 2's game joins (this game's lobby, or its relay), for
    /// `p1.txt`: a load's restart may have to take another one ([`p2_session`]).
    port: Option<u16>,
    /// Player 2's window (from `p2.txt`), to name it in the focus log.
    p2_hwnd: Option<isize>,
    /// Whether this game's window had the focus (logged as it changes).
    focused: Option<bool>,
    /// Player 2's game as this game started it, watched for an early end
    /// ([`p2_ended`]); `None` once it ended or when this game took over one already running.
    p2_child: Option<std::process::Child>,
    /// This game's window, once marked full screen for the taskbar ([`mark_fullscreen`]).
    marked: Option<isize>,
    /// This game's window is kept over the taskbar now ([`set_on_top`]).
    on_top: bool,
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
                    port: None,
                    p2_hwnd: None,
                    focused: None,
                    p2_child: None,
                    marked: None,
                    on_top: false,
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
    if let Some(h) = p1.marked {
        mark_fullscreen(h, false);
        if p1.on_top {
            set_on_top(h, false);
        }
    }
    if let (Some(w), Some(before)) = (window, &p1.before) {
        before.give_back(w);
    }
    // Player 2's window goes at once, not when its game has quit: it sits over this game's
    // window, so it covered player 1's returning picture for a moment [observed: the
    // two-window check of 2026-11-22, its half stayed up about 0.4 s after the end].
    if let Some(h) = p1.p2_hwnd {
        hide_window(h);
    }
    if let Some(e) = p1.pads.p2 {
        let st = &p1.pads.state;
        connections.write(GamepadConnectionEvent::new(e, GamepadConnection::Connected { name: st.name.clone(), vendor_id: st.vendor, product_id: st.product }));
    }
    info!("split: every controller is player 1's again (and the keyboard and mouse)");
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

/// Notes the controller this game's player uses ([`LastPad`]); not player 2's.
fn note_last_pad(mut last: ResMut<LastPad>, mut raw: MessageReader<RawGamepadEvent>, p1: Option<Res<P1>>) {
    let p2 = p1.as_deref().and_then(|p| p.pads.p2);
    for e in raw.read() {
        let used = match e {
            RawGamepadEvent::Button(b) if b.value > 0.5 => Some(b.gamepad),
            RawGamepadEvent::Axis(a) if a.value.abs() > 0.5 => Some(a.gamepad),
            _ => None,
        };
        if let Some(g) = used.filter(|g| Some(*g) != p2) {
            last.0 = Some(g);
        }
    }
}

fn pad_kind(g: &Gamepad, n: Option<&Name>) -> PadKind {
    (n.map_or_else(String::new, |n| n.as_str().to_string()), g.vendor_id(), g.product_id())
}

/// Before this game's controller systems: gives the controllers out when split screen
/// starts ([`assign_pads`]) and as they are plugged in ([`plugged_in_goes_to`]), gives
/// player 2 the first other one that presses A while it has none, keeps player 2's out of
/// this game, and relays it.
fn p1_route_pads(
    mut commands: Commands,
    mut p1: ResMut<P1>,
    mut raw: MessageReader<RawGamepadEvent>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    last: Res<LastPad>,
    bindings: Option<Res<nvf_play::bindings::Bindings>>,
    mut corner: Option<ResMut<nvf_play::corner_messages::CornerMessages>>,
) {
    let p1 = &mut *p1;
    let pad_player = bindings.is_some_and(|b| b.device == nvf_play::bindings::InputDevice::Controller);
    let r = &mut p1.pads;
    let mut say = |n: String| {
        info!("split: {n}");
        if let Some(c) = corner.as_mut() {
            c.push(n.into());
        }
    };
    if !r.assigned {
        r.assigned = true;
        // Under the stand-in controllers' test aid only they are given out (a real one
        // plugged into the test machine would come first otherwise).
        let mut connected: Vec<(Entity, PadKind)> = pads.iter().filter(|(_, g, _)| test_pads::gives_out(g.vendor_id())).map(|(e, g, n)| (e, pad_kind(g, n))).collect();
        connected.sort_by_key(|(e, _)| e.index_u32());
        let kinds: Vec<PadKind> = connected.iter().map(|(_, k)| k.clone()).collect();
        let entities: Vec<Entity> = connected.iter().map(|(e, _)| *e).collect();
        // A load's new run: player 2's game is still running, with the controller it had.
        let kept = kept_from_file(&p1.dir).and_then(|(place, kind)| kept_p2_pad(&kinds, place, &kind)).map(|i| entities[i]);
        let (mine, theirs) = assign_pads(&entities, last.0, pad_player, kept);
        let kind_of = |e: Entity| entities.iter().position(|x| *x == e).map(|i| kinds[i].clone()).unwrap_or_default();
        r.p1 = mine;
        if let Some(e) = theirs {
            let (name, vendor, product) = kind_of(e);
            r.state = PadState { name, vendor, product, connected: true, ..default() };
            r.p2 = Some(e);
            r.p2_place = entities.iter().position(|x| *x == e);
        }
        let p1_part = mine.map_or_else(|| "the keyboard and mouse".to_string(), |e| format!("the keyboard and mouse and {} ({e})", kind_of(e).0));
        let p2_part = theirs.map_or_else(|| "no controller yet (plug one in, or press A on one)".to_string(), |e| format!("{} ({e}){}", kind_of(e).0, if kept.is_some() { ", as before the load" } else { "" }));
        info!(
            "split: controllers assigned ({} connected, player 1 last used {:?}, plays on {}): player 1 has {p1_part}; player 2 has {p2_part}",
            entities.len(),
            last.0,
            if pad_player { "a controller" } else { "the keyboard and mouse" }
        );
        let p1_says = mine.map_or_else(|| "keyboard and mouse".to_string(), |e| format!("keyboard, mouse and {}", kind_of(e).0));
        let p2_says = theirs.map_or_else(|| "press A on a controller".to_string(), |e| kind_of(e).0);
        say(format!("Player 1: {p1_says}. Player 2: {p2_says}."));
    }
    for e in raw.read() {
        match e {
            RawGamepadEvent::Connection(c) => match &c.connection {
                GamepadConnection::Connected { name, vendor_id, product_id } => {
                    let same = !r.state.name.is_empty() && r.state.name == *name && r.state.vendor == *vendor_id && r.state.product == *product_id;
                    if r.p2.is_none() && same && !r.state.connected {
                        r.p2 = Some(c.gamepad);
                        r.state.connected = true;
                        say(format!("Player 2's controller is back ({name})"));
                    } else if r.p1 != Some(c.gamepad) && r.p2 != Some(c.gamepad) {
                        match plugged_in_goes_to(r.p1.is_some(), r.p2.is_some(), pad_player) {
                            PluggedIn::PlayerTwo => {
                                r.state = PadState { name: name.clone(), vendor: *vendor_id, product: *product_id, connected: true, ..default() };
                                r.p2 = Some(c.gamepad);
                                r.p2_place = None;
                                say(format!("Player 2's controller is {name} ({}): plugged in", c.gamepad));
                            }
                            PluggedIn::PlayerOne => {
                                r.p1 = Some(c.gamepad);
                                info!("split: player 1's controller is {name} ({}): plugged in", c.gamepad);
                            }
                            PluggedIn::NoOne => {}
                        }
                    }
                }
                GamepadConnection::Disconnected => {
                    if r.p2 == Some(c.gamepad) {
                        r.p2 = None;
                        r.state.connected = false;
                        r.state.buttons = Default::default();
                        r.state.axes = Default::default();
                        say("Player 2's controller unplugged".to_string());
                    } else if r.p1 == Some(c.gamepad) {
                        r.p1 = None;
                        info!("split: player 1's controller unplugged");
                    }
                }
            },
            RawGamepadEvent::Button(RawGamepadButtonChangedEvent { gamepad, button, value }) => {
                if Some(*gamepad) == r.p2 {
                    if let Some(i) = button_index(*button) {
                        r.state.buttons[i] = *value;
                    }
                    if !r.relayed_noted && *value > 0.5 {
                        r.relayed_noted = true;
                        info!("split: player 2's controller pressed {button:?}: relayed to player 2's game, not read by this one");
                    }
                } else if *value > 0.5 {
                    // The fallback for a controller that came without being given: one that
                    // presses A while player 2 has none, and isn't player 1's, is player 2's.
                    if r.p2.is_none() && !r.state.connected && *button == GamepadButton::South && r.p1 != Some(*gamepad) {
                        let (name, vendor, product) = pads.get(*gamepad).map_or_else(|_| PadKind::default(), |(_, g, n)| pad_kind(g, n));
                        say(format!("Player 2's controller is {name} ({gamepad}): A pressed"));
                        r.state = PadState { name, vendor, product, connected: true, ..default() };
                        r.p2 = Some(*gamepad);
                        r.p2_place = None;
                    } else if r.p1.is_none() {
                        r.p1 = Some(*gamepad);
                        info!("split: player 1's controller is {gamepad}: player 1 used it");
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

/// Player 2's controller as `p1.txt` keeps it ([`p2_pad_value`]) while player 2's game is
/// running: for a load's new run of this game.
fn kept_from_file(dir: &Path) -> Option<(Option<usize>, PadKind)> {
    let alive = read_kv(&dir.join("p2.txt"), "pid").and_then(|v| v.parse::<u32>().ok()).is_some_and(process_alive);
    if !alive {
        return None;
    }
    parse_p2_pad(&read_kv(&dir.join("p1.txt"), "p2pad")?)
}

/// `p2pad=<place>|<name>|<vendor>|<product>` in `p1.txt`.
fn p2_pad_value(r: &PadRoute) -> Option<String> {
    let st = &r.state;
    let id = |v: Option<u16>| v.map_or_else(String::new, |v| v.to_string());
    (r.p2.is_some() && !st.name.is_empty()).then(|| format!("{}|{}|{}|{}", r.p2_place.map_or_else(String::new, |p| p.to_string()), st.name.replace(['|', '\n'], " "), id(st.vendor), id(st.product)))
}

fn parse_p2_pad(v: &str) -> Option<(Option<usize>, PadKind)> {
    let mut it = v.split('|');
    let place = it.next()?.trim().parse().ok();
    let name = it.next()?.to_string();
    let vendor = it.next().and_then(|x| x.trim().parse().ok());
    let product = it.next().and_then(|x| x.trim().parse().ok());
    Some((place, (name, vendor, product)))
}

/// Counts the keyboard and mouse input this PC gave player 1's game, and logs it once a
/// second while there was some, with whether its window has the focus.
fn p1_count_input(
    time: Res<Time>,
    mut n: ResMut<InputCount>,
    mut keys: MessageReader<bevy::input::keyboard::KeyboardInput>,
    mut buttons: MessageReader<bevy::input::mouse::MouseButtonInput>,
    mut motion: MessageReader<bevy::input::mouse::MouseMotion>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    n.keys += keys.read().filter(|k| k.window != Entity::PLACEHOLDER && k.state.is_pressed()).count() as u32;
    n.buttons += buttons.read().filter(|b| b.window != Entity::PLACEHOLDER && b.state.is_pressed()).count() as u32;
    n.motion += motion.read().count() as u32;
    let now = time.elapsed_secs();
    if now - n.logged_at >= 1.0 && n.keys + n.buttons + n.motion > 0 {
        let focused = windows.single().is_ok_and(|w| w.focused);
        info!("split: input: player 1's game read {} key presses, {} mouse clicks and {} mouse moves from this PC (its window focused: {focused})", n.keys, n.buttons, n.motion);
        *n = InputCount { logged_at: now, ..default() };
    }
}

/// Logs where the focus is while split screen is on: the keyboard, the mouse's moves and
/// the controllers (Windows Gaming Input) reach only the game whose window has it (module
/// notes, Focus).
fn p1_watch_focus(mut p1: ResMut<P1>, windows: Query<&Window, With<PrimaryWindow>>) {
    let Ok(w) = windows.single() else { return };
    // Over the taskbar while the halves are played: while this game's window has the
    // focus, or player 2's (which gives it back). Another program's window, or the
    // taskbar itself (the Windows key, a click on it), taking the focus puts it back
    // under them, so they show as over any game.
    // The windows are put there with the backdrop ([`p1_backdrop`], later in the chain).
    if p1.marked.is_some() {
        let fg = (!w.focused).then(foreground_window).flatten();
        let want = w.focused || (fg.is_some() && fg == p1.p2_hwnd);
        if want != p1.on_top {
            p1.on_top = want;
            info!("split: the halves go {} the taskbar", if want { "over" } else { "back under" });
        }
    }
    if p1.focused == Some(w.focused) {
        return;
    }
    p1.focused = Some(w.focused);
    if w.focused {
        info!("split: focus: player 1's window has it (the keyboard, mouse and controllers reach this game)");
    } else {
        let fg = foreground_window();
        let whose = if fg.is_some() && fg == p1.p2_hwnd { "player 2's window (which gives it back)" } else { "another program's window" };
        info!("split: focus: player 1's window lost it to {whose}");
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
    mut corner: Option<ResMut<nvf_play::corner_messages::CornerMessages>>,
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
        if let (Ok((mut window, handle)), Ok(monitor)) = (windows.single_mut(), monitors.single()) {
            let mine = halves(monitor.physical_position, UVec2::new(monitor.physical_width, monitor.physical_height), p1.layout)[0];
            if p1.before.is_none() {
                p1.before = Some(WindowBefore::of(&window, mine));
            }
            p1.half = Some(mine.1);
            let changed = place(&mut window, mine);
            if changed && p1.placed != Some(mine) {
                p1.placed = Some(mine);
                info!("split: player 1's window on {:?} at {}x{}", mine.0, mine.1.x, mine.1.y);
            }
            // The halves are the screen: no taskbar over them ([`mark_fullscreen`]). Once
            // the window is on its half: leaving full screen unmarks it (winit's own mark
            // of a full-screen window), after this frame's systems.
            if p1.marked.is_none() && !changed {
                if let Some(h) = handle.and_then(hwnd_of) {
                    p1.marked = Some(h);
                    if mark_fullscreen(h, true) {
                        info!("split: player 1's window marked full screen for the taskbar (it goes under the halves)");
                    } else {
                        warn!("split: the taskbar did not take player 1's window as full screen");
                    }
                }
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
                p1.port = Some(port);
                let (Ok((_, handle)), Ok(monitor)) = (windows.single(), monitors.single()) else { return };
                let screen = (monitor.physical_position, UVec2::new(monitor.physical_width, monitor.physical_height));
                let p2_half = halves(screen.0, screen.1, p1.layout)[1];
                write_p1_file(&p1.dir, p1.layout, screen, handle.and_then(hwnd_of), p1.save.as_deref(), p1.port, p2_pad_value(&p1.pads).as_deref());
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
                    // A guest's game (`--join`) is in the world before its scripts' data is
                    // up: player 2's characters wait for it, rather than player 2 never
                    // starting [observed: the 19:53 three-window check].
                    if game.is_none() {
                        return;
                    }
                    match start_p2(&p1.dir, port, p2_half, p1.save.as_deref(), game.as_ref()) {
                        Ok(child) => p1.p2_child = Some(child),
                        Err(e) => {
                            warn!("split: player 2's game not started: {e}");
                            if let Some(c) = corner.as_mut() {
                                c.push(P2_STOPPED.to_string().into());
                            }
                            split.ask = Some(false);
                            return;
                        }
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
    // Player 2's game ended with an error (it could not start, or it crashed): split
    // screen ends at once and the player is told, rather than player 1 waiting on a half
    // that stays black [observed: the owner's run of 2026-11-21 04:07, no window].
    let p1 = &mut *p1;
    let p2_file = p1.dir.join("p2.txt");
    let current = || read_kv(&p2_file, "pid").and_then(|v| v.parse::<u32>().ok()).filter(|p| process_alive(*p));
    if let Some(why) = p2_ended(&mut p1.p2_child, &p1.dir, current) {
        warn!("split: {why}");
        if let Some(c) = corner.as_mut() {
            c.push(P2_STOPPED.to_string().into());
        }
        split.ask = Some(false);
        return;
    }
    // This game left the lobby it was a guest in (or the host ended it): player 2's link
    // went through it, so player 2 leaves too (plan §10.1). Never for a load's restart,
    // however this game joined (`--join`, the join screen): see [`relay_lost`].
    if relay_lost(p1.relay, coop.as_deref().map(|c| c.left_lobby())) && split.ask.is_none() {
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
    // Not before the black backdrop is on the screen under it ([`backdrop::ready`]).
    if (p1.placed.is_some() || backdrop::ready()) && place(&mut window, mine) && p1.placed != Some(mine) {
        p1.placed = Some(mine);
        info!("split: player 1's window on {:?} at {}x{}", mine.0, mine.1.x, mine.1.y);
    }
    if now - p1.file_at >= FILE_EVERY {
        p1.file_at = now;
        write_p1_file(&p1.dir, p1.layout, screen, handle.and_then(hwnd_of), p1.save.as_deref(), p1.port, p2_pad_value(&p1.pads).as_deref());
        let p2 = std::fs::read_to_string(p1.dir.join("p2.txt")).unwrap_or_default();
        p1.p2_at = kv(&p2, "port").and_then(|v| v.parse::<u16>().ok()).map(|port| SocketAddr::from(([127, 0, 0, 1], port)));
        p1.p2_hwnd = kv(&p2, "hwnd").and_then(|v| v.parse::<isize>().ok()).filter(|h| *h != 0);
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

/// Player 2 joined through this game (`relay`) and this game is no longer a guest there:
/// it left the lobby, the host ended it, or the host refused or never answered it
/// (`left_lobby`, `crate::coop::Coop::left_lobby`; `None`: no co-op at all). A load's
/// restart is none of these: the old run keeps its link until its process ends, and the
/// new run joins the same host again from its command line or `JOIN_ENV`.
fn relay_lost(relay: bool, left_lobby: Option<bool>) -> bool {
    relay && left_lobby.unwrap_or(true)
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

/// `join`: the loopback port player 2's game joins ([`P1::port`]), as `join=127.0.0.1:<port>`.
/// `p2pad`: player 2's controller ([`p2_pad_value`]).
#[allow(clippy::too_many_arguments)]
fn write_p1_file(dir: &Path, layout: Layout, (at, size): (IVec2, UVec2), hwnd: Option<isize>, save: Option<&str>, join: Option<u16>, p2pad: Option<&str>) {
    let join = join.map(|p| format!("join=127.0.0.1:{p}\n")).unwrap_or_default();
    let pad = p2pad.map(|p| format!("p2pad={p}\n")).unwrap_or_default();
    let text = format!("pid={}\nhwnd={}\nlayout={}\nscreen={},{},{},{}\nsave={}\n{join}{pad}",std::process::id(), hwnd.unwrap_or(0), layout.key(), at.x, at.y, size.x, size.y, save.unwrap_or(""));
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
fn start_p2(dir: &Path, port: u16, (at, size): (IVec2, UVec2), host_save: Option<&str>, game: Option<&PremadeData>) -> Result<std::process::Child, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let home = dir.join("p2-home");
    std::fs::create_dir_all(home.join("saves")).map_err(|e| e.to_string())?;
    let mine = fnv_install::EngineHome::user().ok_or("no home folder")?;
    let theirs = fnv_install::EngineHome::new(&home);
    // Player 2 plays with player 1's settings (the game's folder among them) and load order.
    let _ = std::fs::copy(mine.config_file(), theirs.config_file());
    if mine.plugins_file().is_file() {
        let _ = std::fs::copy(mine.plugins_file(), theirs.plugins_file());
    }
    let made = premade_saves(&mine.saves(), &home, host_save, game)?;
    let args = p2_args(&crate::command_line(), dir, (at, size));
    // What player 2's game writes before its log is up (a refused command line, a game
    // folder it cannot use) and its console after, where player 1's game can read it
    // ([`p2_ended`]): a game started from Mod Organizer 2 or a shortcut has no console.
    let console_path = dir.join(P2_CONSOLE);
    let console = std::fs::File::create(&console_path).map_err(|e| format!("{}: {e}", console_path.display()))?;
    let console2 = console.try_clone().map_err(|e| e.to_string())?;
    info!("split: player 2's command line: {} {}", exe.display(), args.join(" "));
    let mut cmd = std::process::Command::new(exe);
    cmd.stdin(std::process::Stdio::null()).stdout(console).stderr(console2);
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
        .env_remove(test_pads::ENV)
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
    Ok(child)
}

/// Player 2's console, in player 1's session folder ([`start_p2`]).
const P2_CONSOLE: &str = "p2-console.txt";

/// What player 1 is told when player 2's game could not start or stopped (its corner
/// messages; the reason is in the log).
const P2_STOPPED: &str = "Player 2's game stopped. Split screen ended; the log says why.";

/// Player 2's command line, from this game's as acted on (`line`: main.rs `command_line`,
/// with the mode a start without one gets, `--play --menu`): without this game's load,
/// window and co-op arguments, on its main menu on its half. `--menu` needs `--play`, which
/// a game started with no arguments (from Mod Organizer 2, a shortcut, a double-click) has
/// only on the acted-on line, not on the one it was started with [observed: the owner's
/// run of 2026-11-21 04:07; the same command line refused with "the following required
/// arguments were not provided: --play", exit code 2, before any log].
fn p2_args(line: &[String], dir: &Path, (at, size): (IVec2, UVec2)) -> Vec<String> {
    let mut args = crate::restart_args(line);
    strip_coop_args(&mut args);
    if !args.iter().any(|a| a == "--play") {
        args.insert(0, "--play".into());
    }
    args.extend(["--menu".into(), "--split-p2".into(), dir.display().to_string(), "--size".into(), format!("{}x{}", size.x, size.y), "--window-at".into(), format!("{},{}", at.x, at.y)]);
    args
}

/// Player 2's game, when it has ended with an error: why, with the end of its console
/// ([`P2_CONSOLE`]), for the log. A clean end is not one (a load's restart hands over to a
/// new run, which writes its own `pid` to `p2.txt`; an end of that one is seen there,
/// `P2_GONE`), nor an error end once another run of player 2's (`current`) has taken over.
/// The child is let go once it has ended.
fn p2_ended(child: &mut Option<std::process::Child>, dir: &Path, current: impl Fn() -> Option<u32>) -> Option<String> {
    let status = child.as_mut()?.try_wait().ok()??;
    let pid = child.take().map_or(0, |c| c.id());
    if status.success() {
        info!("split: player 2's game (pid {pid}) ended ({status})");
        return None;
    }
    if let Some(other) = current().filter(|p| *p != pid) {
        info!("split: player 2's game (pid {pid}) ended ({status}); its new run (pid {other}) plays on");
        return None;
    }
    let console = std::fs::read(dir.join(P2_CONSOLE)).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    Some(format!("player 2's game (pid {pid}) ended ({status}); its console: {}", console_tail(&console, 12)))
}

/// The last `n` lines of a console's text that say something, without its colour codes,
/// on one line.
fn console_tail(text: &str, n: usize) -> String {
    let plain = strip_ansi(text);
    let lines: Vec<&str> = plain.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    let tail = &lines[lines.len().saturating_sub(n)..];
    if tail.is_empty() {
        "(nothing)".into()
    } else {
        tail.join(" | ")
    }
}

/// `text` without its ANSI escape sequences (`ESC [ ... letter`).
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
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
    /// This game's window.
    mine: Option<isize>,
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

/// This PC's keyboard and mouse never reach player 2's game (`bevy::winit::hardware_input`
/// drops them): logs how many were dropped, once a second while more come, and warns of
/// any that reached it all the same (only test aids' own input may).
fn p2_count_input(
    time: Res<Time>,
    mut n: ResMut<InputCount>,
    mut keys: MessageReader<bevy::input::keyboard::KeyboardInput>,
    mut buttons: MessageReader<bevy::input::mouse::MouseButtonInput>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut motion: MessageReader<bevy::input::mouse::MouseMotion>,
) {
    n.keys += keys.read().filter(|k| k.window != Entity::PLACEHOLDER).count() as u32;
    n.buttons += buttons.read().filter(|b| b.window != Entity::PLACEHOLDER).count() as u32 + wheel.read().filter(|w| w.window != Entity::PLACEHOLDER).count() as u32;
    n.motion += motion.read().count() as u32;
    let now = time.elapsed_secs();
    if now - n.logged_at < 1.0 {
        return;
    }
    let dropped = bevy::winit::hardware_input::dropped();
    let reached = n.keys + n.buttons + n.motion;
    if reached > 0 {
        warn!("split: input: {} key, {} mouse button and {} mouse move events from this PC reached player 2's game", n.keys, n.buttons, n.motion);
    }
    if dropped != n.dropped_logged {
        info!("split: input: player 2's game dropped {} keyboard and mouse events from this PC so far ({} more); none reached it", dropped, dropped - n.dropped_logged);
    }
    *n = InputCount { logged_at: now, dropped_logged: dropped, ..default() };
}

/// Player 2's window never keeps the focus: whatever gave it to it (a click before its
/// window was made unable to take it, Windows showing it again, another program), this
/// game hands it straight back to player 1's window, which it may do while it holds it
/// (module notes, Focus). Checked every frame.
fn p2_give_focus_back(p2: Res<P2>, mut given: Local<u32>, mut logged_at: Local<f32>, time: Res<Time>) {
    let (Some(mine), Some(owner)) = (p2.mine, p2.owner) else { return };
    if foreground_window() != Some(mine) {
        return;
    }
    let ok = give_foreground(owner);
    *given += 1;
    let now = time.elapsed_secs();
    if *given == 1 || now - *logged_at >= 1.0 {
        *logged_at = now;
        info!("split: focus: player 2's window had it: given back to player 1's window ({}, {} time(s) so far)", if ok { "done" } else { "Windows refused" }, *given);
    }
}

/// Player 2's session: its window on its half, owned by player 1's; its files; quitting
/// when player 1's game is gone.
fn p2_session(time: Res<Time>, mut p2: ResMut<P2>, mut windows: Query<(&mut Window, Option<&RawHandleWrapper>), With<PrimaryWindow>>, mut exit: MessageWriter<AppExit>, coop: Option<ResMut<crate::coop::Coop>>) {
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
    p2.mine = handle.and_then(hwnd_of);
    write_file(&p2.dir.join("p2.txt"), &format!("pid={}\nport={port}\nhwnd={}\n", std::process::id(), p2.mine.unwrap_or(0)));
    let p1_alive = kv(&text, "pid").and_then(|v| v.parse::<u32>().ok()).is_some_and(process_alive);
    if p1_alive || p2.p1_seen == 0.0 {
        p2.p1_seen = now.max(f32::MIN_POSITIVE);
    } else if now - p2.p1_seen > P1_GONE {
        info!("split: player 1's game is gone: quitting");
        exit.write(AppExit::Success);
        return;
    }
    // Player 1's game came back from a load's restart on another port (its old one was
    // taken): join it there, keeping this player's place.
    if let (Some(at), Some(mut coop)) = (kv(&text, "join").and_then(|v| v.parse::<SocketAddr>().ok()), coop) {
        if coop.follow_host(at) {
            info!("split: player 1's game is at {at} now");
        }
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

/// The black backdrop under both halves ([`backdrop`]): up from split screen's first
/// frame (this runs after [`split_asks`] and before player 1's window goes to its half,
/// which Bevy's window systems apply later in the frame) till split screen ends; it says
/// player 2 is joining while player 2's window isn't showing.
fn p1_backdrop(p1: Option<Res<P1>>, mut panel: Local<backdrop::Backdrop>, windows: Query<&RawHandleWrapper, With<PrimaryWindow>>, monitors: Query<&Monitor, With<PrimaryMonitor>>, _main_thread: bevy::ecs::system::NonSendMarker) {
    let Some(p1) = p1 else {
        panel.hide("split screen ended");
        return;
    };
    let (Ok(handle), Ok(monitor)) = (windows.single(), monitors.single()) else { return };
    let Some(mine) = hwnd_of(handle) else { return };
    let screen = (monitor.physical_position, UVec2::new(monitor.physical_width, monitor.physical_height));
    let p2_half = halves(screen.0, screen.1, p1.layout)[1];
    let joining = !p1.p2_hwnd.is_some_and(backdrop::shown);
    panel.show(mine, p1.p2_hwnd, backdrop::Look { screen, p2_half, joining }, p1.on_top);
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

/// The window that has the focus (the foreground window), if any.
#[cfg(windows)]
fn foreground_window() -> Option<isize> {
    // SAFETY: reads which window is in the foreground; no pointers are passed.
    let w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    (!w.is_null()).then_some(w as isize)
}

#[cfg(not(windows))]
fn foreground_window() -> Option<isize> {
    None
}

/// Gives the focus to `window`; true when Windows did. A process may do this while one of
/// its windows has the focus [documented: Win32 `SetForegroundWindow`'s conditions].
#[cfg(windows)]
fn give_foreground(window: isize) -> bool {
    // SAFETY: `window` is player 1's top-level window; the call fails harmlessly if it has
    // gone.
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(window as windows_sys::Win32::Foundation::HWND) != 0 }
}

#[cfg(not(windows))]
fn give_foreground(_window: isize) -> bool {
    false
}

/// Marks player 1's window as a full-screen one (`on`), or as an ordinary one again, for the
/// taskbar: while a window so marked is the active one, the taskbar goes to the bottom of
/// the stack instead of staying over it [documented: Win32 `ITaskbarList2::MarkFullscreenWindow`].
/// Windows takes only a window that covers its whole monitor as full screen on its own, and
/// neither half does, so without it the taskbar stayed over the halves' bottom edge
/// [observed: the owner's screenshot, 2026-11-21]. Done on a thread of its own, in a
/// single-threaded COM apartment of its own (the taskbar's object asks for one). True when
/// the taskbar took it.
#[cfg(windows)]
fn mark_fullscreen(window: isize, on: bool) -> bool {
    use std::ffi::c_void;
    use windows_sys::core::{GUID, HRESULT};
    use windows_sys::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    /// `ITaskbarList2`'s table of functions: `IUnknown`'s, `ITaskbarList`'s, then its own
    /// [documented: shobjidl_core.h, the order the interfaces declare them].
    #[repr(C)]
    struct TaskbarList2 {
        query_interface: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        hr_init: unsafe extern "system" fn(*mut c_void) -> HRESULT,
        add_tab: usize,
        delete_tab: usize,
        activate_tab: usize,
        set_active_alt: usize,
        mark_fullscreen_window: unsafe extern "system" fn(*mut c_void, *mut c_void, i32) -> HRESULT,
    }
    /// IID_ITaskbarList2 [documented: shobjidl_core.h].
    const IID_ITASKBARLIST2: GUID = GUID::from_u128(0x602d4995_b13a_429b_a66e_1935e44f4317);
    let run = move || {
        // SAFETY: COM is set up and torn down on this thread only; the object is checked,
        // used through its documented table and released before the apartment ends.
        unsafe {
            let init = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
            let mut list: *mut c_void = std::ptr::null_mut();
            let made = CoCreateInstance(&windows_sys::Win32::UI::Shell::TaskbarList, std::ptr::null_mut(), CLSCTX_INPROC_SERVER, &IID_ITASKBARLIST2, &mut list);
            let mut ok = false;
            if made >= 0 && !list.is_null() {
                let table = &**(list as *mut *const TaskbarList2);
                ok = (table.hr_init)(list) >= 0 && (table.mark_fullscreen_window)(list, window as *mut c_void, i32::from(on)) >= 0;
                (table.release)(list);
            }
            if init >= 0 {
                CoUninitialize();
            }
            ok
        }
    };
    std::thread::Builder::new().name("split taskbar".into()).spawn(run).ok().and_then(|t| t.join().ok()).unwrap_or(false)
}

#[cfg(not(windows))]
fn mark_fullscreen(_window: isize, _on: bool) -> bool {
    false
}

/// Hides window `window` (player 2's, another process's) without waiting on its game.
#[cfg(windows)]
fn hide_window(window: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindowAsync, SW_HIDE};
    // SAFETY: asks another process's window to hide; fails harmlessly once it is gone.
    unsafe {
        ShowWindowAsync(window as _, SW_HIDE);
    }
}
#[cfg(not(windows))]
fn hide_window(_window: isize) {}

/// Puts player 1's window over the taskbar (`on`: always on top), or back among the other
/// windows; used as split screen ends. While split screen is on, the backdrop keeps the
/// whole stack instead ([`backdrop`]): player 2's window, owned from another process, did
/// not follow player 1's into the topmost band [observed: the two-window check of
/// 2026-11-22]. Marking the window full screen for the taskbar ([`mark_fullscreen`]) was not
/// enough: the taskbar stayed over the halves' bottom edge with the mark taken [observed:
/// the owner's split screen game and screenshot, 2026-11-21 23:19]. True when Windows did it.
#[cfg(windows)]
fn set_on_top(window: isize, on: bool) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};
    // SAFETY: `window` is this process's own top-level window; only its z-order changes.
    unsafe { SetWindowPos(window as _, if on { HWND_TOPMOST } else { HWND_NOTOPMOST }, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) != 0 }
}

#[cfg(not(windows))]
fn set_on_top(_window: isize, _on: bool) -> bool {
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

/// Test aid `NVF_PLAY_TEST_SPLIT_PADS=<n>[,<seconds>:<pad>:<input>[:<hold seconds>]...]`
/// (player 1's game): `n` stand-in controllers ("Test controller 1" ...) connected from the
/// start as a real one is, and their inputs at world seconds (`nvf_play::pause::AidTime`),
/// let go after `hold` (0.2 s by default), as raw controller input, so they go through
/// split screen's controller assignment and relay like real ones (no second real
/// controller needed). An input is a button (`South`, `Start`, ... as in [`BUTTONS`]) or a
/// stick pushed, `<axis>=<value>` (`LeftStickY=1`, `RightStickX=-0.8`, as in [`AXES`]).
/// Player 2's game is started without it.
pub(crate) mod test_pads {
    use super::*;

    pub const ENV: &str = "NVF_PLAY_TEST_SPLIT_PADS";

    /// The stand-ins' vendor id.
    const VENDOR: u16 = 0x7E57;

    /// Whether split screen gives out a controller of vendor `vendor` when it starts: any,
    /// unless this aid is set, when only its stand-ins are.
    pub(super) fn gives_out(vendor: Option<u16>) -> bool {
        std::env::var_os(ENV).is_none() || vendor == Some(VENDOR)
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Press {
        Button(GamepadButton),
        Axis(GamepadAxis, f32),
    }

    impl Press {
        fn parse(s: &str) -> Option<Press> {
            if let Some((a, v)) = s.split_once('=') {
                let axis = AXES.iter().copied().find(|x| format!("{x:?}").eq_ignore_ascii_case(a.trim()))?;
                return Some(Press::Axis(axis, v.trim().parse::<f32>().ok()?.clamp(-1.0, 1.0)));
            }
            BUTTONS.iter().copied().find(|b| format!("{b:?}").eq_ignore_ascii_case(s.trim())).map(Press::Button)
        }

        fn event(self, pad: Entity, on: bool) -> RawGamepadEvent {
            match self {
                Press::Button(b) => RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(pad, b, if on { 1.0 } else { 0.0 })),
                Press::Axis(a, v) => RawGamepadEvent::Axis(RawGamepadAxisChangedEvent::new(pad, a, if on { v } else { 0.0 })),
            }
        }
    }

    #[derive(Resource, Default)]
    struct TestPads {
        count: usize,
        pads: Vec<Entity>,
        due: Vec<(f32, usize, Press, f32)>,
        down: Vec<(f32, usize, Press)>,
    }

    fn parse(v: &str) -> Result<TestPads, String> {
        let mut items = v.split(',').map(str::trim).filter(|i| !i.is_empty());
        let count = items.next().ok_or("no count")?.parse::<usize>().map_err(|e| e.to_string())?;
        let mut due = Vec::new();
        for i in items {
            let f: Vec<&str> = i.split(':').collect();
            let (Some(at), Some(pad), Some(b)) = (f.first().and_then(|x| x.parse::<f32>().ok()), f.get(1).and_then(|x| x.parse::<usize>().ok()), f.get(2).and_then(|x| Press::parse(x))) else {
                return Err(format!("{i}: not <seconds>:<pad>:<button or axis=value>[:<hold>]"));
            };
            if pad >= count {
                return Err(format!("{i}: pad {pad} of {count}"));
            }
            due.push((at, pad, b, f.get(3).and_then(|x| x.parse::<f32>().ok()).unwrap_or(0.2)));
        }
        Ok(TestPads { count, due, ..default() })
    }

    pub(super) fn setup(app: &mut App) {
        let Ok(v) = std::env::var(ENV) else { return };
        match parse(&v) {
            Ok(t) => {
                app.insert_resource(t).add_systems(PreUpdate, drive.before(gamepad_connection_system));
            }
            Err(e) => warn!("{ENV}: {e}"),
        }
    }

    fn drive(mut commands: Commands, mut t: ResMut<TestPads>, time: nvf_play::pause::AidTime, mut connections: MessageWriter<GamepadConnectionEvent>, mut raw: MessageWriter<RawGamepadEvent>) {
        let t = &mut *t;
        if t.pads.len() < t.count {
            for n in 0..t.count {
                let e = commands.spawn_empty().id();
                let c = GamepadConnectionEvent::new(e, GamepadConnection::Connected { name: format!("Test controller {}", n + 1), vendor_id: Some(VENDOR), product_id: Some(n as u16) });
                connections.write(c);
                t.pads.push(e);
                info!("test aid: {ENV}: Test controller {} connected ({e})", n + 1);
            }
            return;
        }
        let Some(now) = time.world_secs() else { return };
        t.down.retain(|&(until, pad, b)| {
            if now < until {
                return true;
            }
            raw.write(b.event(t.pads[pad], false));
            info!("test aid: {ENV}: Test controller {} let go of {b:?}", pad + 1);
            false
        });
        let mut i = 0;
        while i < t.due.len() {
            let (at, pad, b, hold) = t.due[i];
            if now < at {
                i += 1;
                continue;
            }
            t.due.remove(i);
            raw.write(b.event(t.pads[pad], true));
            t.down.push((now + hold, pad, b));
            info!("test aid: {ENV}: Test controller {} pressed {b:?} ({})", pad + 1, t.pads[pad]);
        }
    }

    #[cfg(test)]
    #[test]
    fn the_aid_reads_its_pads_and_presses() {
        let t = parse("2, 3:0:South, 80.5:1:start:1, 90:0:LeftStickY=1:5, 91:1:rightstickx=-0.5").unwrap();
        assert_eq!(t.count, 2);
        assert_eq!(
            t.due,
            vec![
                (3.0, 0, Press::Button(GamepadButton::South), 0.2),
                (80.5, 1, Press::Button(GamepadButton::Start), 1.0),
                (90.0, 0, Press::Axis(GamepadAxis::LeftStickY, 1.0), 5.0),
                (91.0, 1, Press::Axis(GamepadAxis::RightStickX, -0.5), 0.2),
            ]
        );
        assert!(parse("1,3:1:South").is_err(), "pad out of range");
        assert!(parse("1,3:0:Jump").is_err());
        assert!(parse("1,3:0:Tilt=1").is_err());
    }
}

/// Test aid `NVF_PLAY_TEST_SPLIT_TRACE=<every seconds>` (player 2's game takes it as
/// `NVF_SPLIT_TEST_P2_SPLIT_TRACE`): logs `split trace:` lines, the player's feet, heading,
/// level speed over the interval, what it asks for (walk, sneak, aiming) and the frame
/// rate, to measure movement and looking in either half. Not a game behaviour; no effect
/// unless set.
pub(crate) mod trace {
    use super::*;
    use nvf_play::control::PlayerControl;
    use nvf_play::player::{MoveIntent, Player};

    pub const ENV: &str = "NVF_PLAY_TEST_SPLIT_TRACE";

    #[derive(Resource)]
    struct Trace {
        every: f32,
        next: f32,
        last: Option<([f32; 3], f32)>,
        frames: u32,
        /// No test aid: split screen only, while the player moves ([`QUIET_EVERY`]).
        quiet: bool,
    }

    /// Without the test aid, a game in split screen still logs how its player moves, every
    /// [`QUIET_EVERY`] seconds while they move (`split: moves: ...`): the owner's playtest of
    /// 2026-11-21 had player 1 slow while split ("P1 was walking slow and sprinting would get
    /// close to normal speed"), not seen in the night pass's runs (stand-in controller and
    /// keyboard, the game's own data), so the next run with the owner's load order says why.
    pub const QUIET_EVERY: f32 = 10.0;

    pub(super) fn setup(app: &mut App) {
        let aid = std::env::var(ENV).ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| *v > 0.0);
        app.insert_resource(Trace { every: aid.unwrap_or(QUIET_EVERY), next: 0.0, last: None, frames: 0, quiet: aid.is_none() }).add_systems(Last, log);
    }

    #[allow(clippy::too_many_arguments)]
    fn log(
        mut t: ResMut<Trace>,
        real: Res<Time<Real>>,
        virt: Res<Time<Virtual>>,
        player: Option<Res<Player>>,
        intent: Option<Res<MoveIntent>>,
        control: Option<Res<PlayerControl>>,
        bindings: Option<Res<nvf_play::bindings::Bindings>>,
        split: (Option<Res<P1>>, Option<Res<P2>>),
    ) {
        t.frames += 1;
        let now = real.elapsed_secs();
        if now < t.next {
            return;
        }
        if t.quiet && split.0.is_none() && split.1.is_none() {
            t.last = None;
            t.next = now + t.every;
            return;
        }
        let Some(p) = player else { return };
        let (dist, secs) = t.last.map_or((0.0, 0.0), |(at, then)| (((p.feet[0] - at[0]).powi(2) + (p.feet[1] - at[1]).powi(2)).sqrt(), now - then));
        let speed = if secs > 0.0 { dist / secs } else { 0.0 };
        let fps = if secs > 0.0 { t.frames as f32 / secs } else { 0.0 };
        let i = intent.as_deref().copied().unwrap_or_default();
        let aiming = control.as_deref().is_some_and(|c| c.aiming && c.weapon_out);
        let moving = speed > 1.0 || i.forward != 0.0 || i.right != 0.0;
        if t.quiet {
            if moving {
                let c = control.as_deref();
                info!(
                    "split: moves: {speed:.0} units/s level (run {:.0}, walk {:.0}), forward {:.2} right {:.2}, walk {} (always run {}), sneak {}, aiming {aiming}, on {}, game clock x{:.2}, {fps:.0} fps",
                    nvf_play::player::MoveSpeeds::OBSERVED.run,
                    nvf_play::player::MoveSpeeds::OBSERVED.walk,
                    i.forward,
                    i.right,
                    i.walk,
                    c.is_some_and(|c| c.always_run),
                    i.sneak,
                    if bindings.as_deref().is_some_and(|b| b.device == nvf_play::bindings::InputDevice::Controller) { "a controller" } else { "the keyboard and mouse" },
                    virt.relative_speed(),
                );
            }
            t.last = Some((p.feet, now));
            t.frames = 0;
            t.next = now + t.every;
            return;
        }
        info!(
            "split trace: feet ({:.0}, {:.0}, {:.0}) heading {:.1} deg, {speed:.1} units/s level, forward {:.2} right {:.2} walk {} sneak {} aiming {aiming}, {fps:.0} fps",
            p.feet[0],
            p.feet[1],
            p.feet[2],
            p.heading.to_degrees(),
            i.forward,
            i.right,
            i.walk,
            i.sneak
        );
        t.last = Some((p.feet, now));
        t.frames = 0;
        t.next = now + t.every;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pads(n: u32) -> Vec<Entity> {
        (0..n).map(|i| Entity::from_raw_u32(10 + i).unwrap()).collect()
    }

    #[test]
    fn controllers_are_assigned_when_split_screen_starts() {
        let c = pads(3);
        // Keyboard and mouse player 1, no controller used: the first controller is player 2's.
        assert_eq!(assign_pads(&c, None, false, None), (None, Some(c[0])));
        // Player 1 last used the second one: it keeps it; player 2 takes the next connected.
        assert_eq!(assign_pads(&c, Some(c[1]), false, None), (Some(c[1]), Some(c[0])));
        assert_eq!(assign_pads(&c, Some(c[0]), false, None), (Some(c[0]), Some(c[1])));
        // Only controllers (player 1 plays on one): the first is player 1's, the second player 2's.
        assert_eq!(assign_pads(&c[..2], None, true, None), (Some(c[0]), Some(c[1])));
        // One controller, player 1's own: player 2 has none yet.
        assert_eq!(assign_pads(&c[..1], Some(c[0]), false, None), (Some(c[0]), None));
        assert_eq!(assign_pads(&c[..1], None, true, None), (Some(c[0]), None));
        // None connected.
        assert_eq!(assign_pads(&[], None, true, None), (None, None));
        // A controller last used but unplugged since is no one's.
        assert_eq!(assign_pads(&c[1..], Some(c[0]), false, None), (None, Some(c[1])));
        // A load's new run: player 2 keeps the one it had, even if player 1 used it last.
        assert_eq!(assign_pads(&c, Some(c[2]), true, Some(c[2])), (Some(c[0]), Some(c[2])));
        assert_eq!(assign_pads(&c, Some(c[0]), false, Some(c[1])), (Some(c[0]), Some(c[1])));
    }

    #[test]
    fn a_controller_plugged_in_later_goes_to_player_2_while_it_has_none() {
        assert_eq!(plugged_in_goes_to(false, false, false), PluggedIn::PlayerTwo);
        assert_eq!(plugged_in_goes_to(true, false, true), PluggedIn::PlayerTwo);
        assert_eq!(plugged_in_goes_to(false, true, false), PluggedIn::NoOne);
        // A controller player with none yet takes it first (the "only controllers" rule).
        assert_eq!(plugged_in_goes_to(false, false, true), PluggedIn::PlayerOne);
        assert_eq!(plugged_in_goes_to(true, true, true), PluggedIn::NoOne);
    }

    #[test]
    fn a_loads_new_run_finds_player_2s_controller_again() {
        let k = |n: &str, p: u16| (n.to_string(), Some(0x045E_u16), Some(p));
        let kinds = vec![k("Xbox", 1), k("Xbox", 1), k("DualSense", 2)];
        assert_eq!(kept_p2_pad(&kinds, Some(1), &k("Xbox", 1)), Some(1), "same place, same kind");
        assert_eq!(kept_p2_pad(&kinds, Some(0), &k("DualSense", 2)), Some(2), "moved: the first of its kind");
        assert_eq!(kept_p2_pad(&kinds, None, &k("Other", 3)), None);
        let line = "1|Xbox Wireless Controller|1118|";
        assert_eq!(parse_p2_pad(line), Some((Some(1), ("Xbox Wireless Controller".to_string(), Some(1118), None))));
        let mut r = PadRoute { p2: Some(Entity::from_raw_u32(3).unwrap()), p2_place: Some(1), ..default() };
        r.state.name = "Xbox Wireless Controller".into();
        r.state.vendor = Some(1118);
        assert_eq!(p2_pad_value(&r).as_deref(), Some(line));
        r.p2 = None;
        assert_eq!(p2_pad_value(&r), None, "not while player 2 has none");
    }

    #[test]
    fn this_pcs_keyboard_and_mouse_are_the_dropped_input() {
        use bevy::winit::hardware_input::is_hardware_input;
        use winit::event::WindowEvent;
        assert!(is_hardware_input(&WindowEvent::ModifiersChanged(Default::default())));
        assert!(!is_hardware_input(&WindowEvent::Focused(true)));
        assert!(!is_hardware_input(&WindowEvent::CloseRequested));
    }

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

    /// A game started with no arguments (Mod Organizer 2, a shortcut) has `--play` only on
    /// its acted-on line; player 2's line must parse either way [observed: the owner's run
    /// of 2026-11-21, `--menu` without `--play` refused].
    #[test]
    fn player_2s_command_line_parses_however_player_1_started() {
        use clap::Parser;
        let half = (IVec2::new(0, 720), UVec2::new(2560, 720));
        let dir = Path::new(r"C:\home\split");
        let parses = |line: Vec<String>| {
            let args = p2_args(&line, dir, half);
            let full: Vec<String> = std::iter::once("nvforever".to_string()).chain(args.iter().cloned()).collect();
            crate::Args::try_parse_from(&full).map(|_| args).map_err(|e| e.to_string())
        };
        let owner = crate::with_default_mode(vec!["nvforever".into()])[1..].to_vec();
        for line in [vec![], owner, ["--play", "--load", "x.nvfsave", "--split", "--host", "47960"].map(String::from).to_vec(), ["--data", "D", "--play", "--menu"].map(String::from).to_vec()] {
            let args = parses(line.clone()).unwrap_or_else(|e| panic!("{line:?}: {e}"));
            assert_eq!(args.iter().filter(|a| *a == "--play").count(), 1, "{args:?}");
            assert_eq!(args.iter().filter(|a| *a == "--menu").count(), 1, "{args:?}");
            assert!(!args.iter().any(|a| a == "--load" || a == "--split" || a == "--host"), "{args:?}");
        }
    }

    #[test]
    fn player_2s_console_end_is_read_without_its_colours() {
        let text = "\u{1b}[2m2026\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m one\n\nerror: the following required arguments were not provided:\n  --play\n\nUsage: nvforever.exe --play --menu\n";
        assert_eq!(console_tail(text, 3), "error: the following required arguments were not provided: |   --play | Usage: nvforever.exe --play --menu");
        assert_eq!(console_tail(text, 9), "2026  INFO one | error: the following required arguments were not provided: |   --play | Usage: nvforever.exe --play --menu");
        assert_eq!(console_tail("", 3), "(nothing)");
    }

    #[cfg(windows)]
    #[test]
    fn player_2s_game_ending_with_an_error_is_seen_and_a_clean_end_is_not() {
        let dir = std::env::temp_dir().join(format!("nvf-split-ended-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(P2_CONSOLE), "error: refused\n").unwrap();
        let ended = |code: u32, current: Option<u32>| {
            let mut child = Some(std::process::Command::new("cmd").args(["/c", &format!("exit {code}")]).spawn().unwrap());
            child.as_mut().unwrap().wait().unwrap();
            let why = p2_ended(&mut child, &dir, || current);
            assert!(child.is_none(), "let go once ended");
            why
        };
        let why = ended(2, None).expect("an error end is seen");
        assert!(why.contains("exit code: 2") && why.ends_with("its console: error: refused"), "{why}");
        assert_eq!(ended(0, None), None, "a clean end (a load's restart) is not");
        assert_eq!(ended(1, Some(u32::MAX)), None, "another run of player 2's plays on");
        let mut none = None;
        assert_eq!(p2_ended(&mut none, &dir, || None), None);
        let _ = std::fs::remove_dir_all(&dir);
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

    /// Player 1's file says where player 2's game joins, so a load's restart that had to
    /// take another port is followed ([`p2_session`]).
    #[test]
    fn player_1s_file_says_where_player_2_joins() {
        let dir = std::env::temp_dir().join(format!("nvf-split-join-{}", std::process::id()));
        write_p1_file(&dir, Layout::TopBottom, (IVec2::ZERO, UVec2::new(2560, 1440)), None, Some("Save 1"), Some(51_234), Some("1|Xbox|1118|"));
        let at = read_kv(&dir.join("p1.txt"), "join").and_then(|v| v.parse::<SocketAddr>().ok());
        assert_eq!(at, Some(SocketAddr::from(([127, 0, 0, 1], 51_234))));
        write_p1_file(&dir, Layout::TopBottom, (IVec2::ZERO, UVec2::new(2560, 1440)), None, None, None, None);
        assert_eq!(read_kv(&dir.join("p1.txt"), "join"), None, "not before player 1 has a port");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Player 2 leaves with player 1's lobby only when player 1 left it, never for a
    /// load's restart, whether player 1 joined with `--join` or the join screen (its link
    /// stays up until the old run ends, and `left_lobby` is only set by leaving).
    #[test]
    fn player_2_leaves_with_the_lobby_not_with_a_restart() {
        assert!(!relay_lost(true, Some(false)), "a guest still, or restarting");
        assert!(relay_lost(true, Some(true)), "left, ended, refused or never answered");
        assert!(!relay_lost(false, Some(true)), "player 2 joins this game's own lobby");
        assert!(relay_lost(true, None));
    }
}
