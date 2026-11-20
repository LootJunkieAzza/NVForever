//! Split screen (private fork; plan: `C:\NVForever\morning-report\plans\split-screen.md`,
//! owner 2026-11-20): two players on one PC, each in a window of their own on half the
//! screen. Player 2's game is a second run of this program that joins this one as a co-op
//! guest over 127.0.0.1, so in the host's world player 2 is a stand-in like any guest
//! (`crate::coop` `stand_ins`), and only its camera, controls, HUD and menus run in its own
//! game [design: owner's choice of the two-window build, 2026-11-20].
//!
//! - **`--split`** (player 1): once in the world, this game hosts (if it isn't already),
//!   moves its window to the top half (the left half side by side), and starts player 2's
//!   game on the other half. A load restarts this game (`crate::handoff`); the new run
//!   finds player 2's game still running and takes it over rather than starting another.
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
//! Log: `split: player 2's game started (pid …)`, `split: player 2's controller is …`,
//! `split: player 2's controller unplugged`, `split: player 1's game is gone: quitting`.

use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};

use bevy::input::gamepad::{gamepad_connection_system, gamepad_event_processing_system, Gamepad, GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent, RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent};
use bevy::prelude::*;
use bevy::window::{Monitor, PrimaryMonitor, PrimaryWindow, RawHandleWrapper, WindowMode};

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

/// Player 2's game quits when player 1's has been gone this long: longer than a load's
/// restart takes to hand over (`crate::handoff`) [design].
const P1_GONE: f32 = 15.0;
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
        if let Some(dir) = self.0.p2.clone() {
            let socket = UdpSocket::bind("127.0.0.1:0").and_then(|s| s.set_nonblocking(true).map(|_| s)).map_err(|e| warn!("split: no controller relay: {e}")).ok();
            app.insert_resource(nvf_play::audio_out::PersonalSoundsOnly).insert_resource(P2 { dir, socket, pad: None, last: PadState::default(), owner: None, placed: None, p1_seen: 0.0, file_at: f32::NEG_INFINITY, premade: loaded_stem().and_then(|s| premade_of(&s)), join_opened: false })
                .add_systems(PreUpdate, p2_receive.before(gamepad_connection_system))
                .add_systems(PreUpdate, p2_strip_real_pads.after(gamepad_connection_system).before(gamepad_event_processing_system))
                .add_systems(Update, (p2_session, p2_keep_character, p2_open_join));
        } else if self.0.start {
            let Some(home) = fnv_install::EngineHome::default_root() else {
                warn!("split: no home folder: split screen not started");
                return;
            };
            let socket = UdpSocket::bind("127.0.0.1:0").map_err(|e| warn!("split: no controller relay: {e}")).ok();
            app.insert_resource(P1 { dir: home.join("split"), layout: Layout::TopBottom, state: P1State::Waiting, socket, p2_at: None, file_at: f32::NEG_INFINITY, placed: None, pads: PadRoute::default(), save: loaded_stem() })
                .add_systems(PreUpdate, p1_route_pads.after(gamepad_connection_system).before(gamepad_event_processing_system))
                .add_systems(Update, p1_session);
        }
    }
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
    pads: PadRoute,
    /// The host save this session belongs to (its file's stem): the one loaded, then
    /// each one written.
    save: Option<String>,
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
    (script, play): (Option<Res<nvf_play::script::ScriptState>>, Option<Res<nvf_play::PlayWorld>>),
) {
    let now = time.elapsed_secs();
    for d in saved.read().filter(|d| d.result.is_ok()) {
        p1.save = d.path.file_stem().map(|s| s.to_string_lossy().into_owned());
        p1.file_at = f32::NEG_INFINITY;
    }
    let in_world = sim.is_some() && !main_menu.as_deref().is_some_and(|m| m.open);
    match p1.state {
        P1State::Waiting if in_world => {
            if hosting_port().is_none() {
                info!("split: opening a lobby for player 2");
                lobby.write(nvf_play::lobby::LobbyRequest::Create);
            }
            p1.state = P1State::Hosting;
        }
        P1State::Hosting => {
            if let Some(port) = hosting_port() {
                let (Ok((_, handle)), Ok(monitor)) = (windows.single(), monitors.single()) else { return };
                let screen = (monitor.physical_position, UVec2::new(monitor.physical_width, monitor.physical_height));
                let p2_half = halves(screen.0, screen.1, p1.layout)[1];
                write_p1_file(&p1.dir, p1.layout, screen, handle.and_then(hwnd_of), p1.save.as_deref());
                if let Some(pid) = read_kv(&p1.dir.join("p2.txt"), "pid").and_then(|v| v.parse::<u32>().ok()).filter(|p| process_alive(*p)) {
                    info!("split: player 2's game is already running (pid {pid}): taking it over");
                } else {
                    let data = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| rt.game.data);
                    let game = match (data, sim.as_deref(), play.as_deref()) {
                        (Some(data), Some(sim), Some(play)) => Some(PremadeData { data, forms: sim.forms, plugins: nvf_play::save::plugin_list(&play.0) }),
                        _ => None,
                    };
                    if let Err(e) = start_p2(&p1.dir, port, p2_half, p1.save.as_deref(), game.as_ref()) {
                        warn!("split: player 2's game not started: {e}");
                    }
                }
                p1.state = P1State::Running;
            }
        }
        _ => {}
    }
    if p1.state != P1State::Running {
        return;
    }
    let Ok((mut window, handle)) = windows.single_mut() else { return };
    let Ok(monitor) = monitors.single() else { return };
    let screen = (monitor.physical_position, UVec2::new(monitor.physical_width, monitor.physical_height));
    let mine = halves(screen.0, screen.1, p1.layout)[0];
    if place(&mut window, mine) && p1.placed != Some(mine) {
        p1.placed = Some(mine);
        info!("split: player 1's window on {:?} at {}x{}", mine.0, mine.1.x, mine.1.y);
    }
    if now - p1.file_at >= FILE_EVERY {
        p1.file_at = now;
        write_p1_file(&p1.dir, p1.layout, screen, handle.and_then(hwnd_of), p1.save.as_deref());
        p1.p2_at = read_kv(&p1.dir.join("p2.txt"), "port").and_then(|v| v.parse::<u16>().ok()).map(|port| SocketAddr::from(([127, 0, 0, 1], port)));
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
    let child = std::process::Command::new(exe)
        .args(&args)
        .env(fnv_install::HOME_ENV, &home)
        .env(crate::coop::JOIN_ENV, format!("127.0.0.1:{port}"))
        .env(crate::coop::NAME_ENV, "Player 2")
        .env(P2_ENV, "1")
        .env(nvf_play::coop_progress::SYNC_ENV, "1")
        .env_remove(crate::coop::HOST_ENV)
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
        body.game = premade_snapshot(&body.game, p, &r);
        body.engine.coop.clear();
        body.engine.magazine = None;
        body.engine.magazine_ammo = None;
        let mut h = b.header.clone();
        h.kind = fnv_save::SaveKind::Manual;
        h.player_name = p.name.to_string();
        h.level = 1;
        h.coop.clear();
        h.plugins = game.plugins.clone();
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
    /// When player 1's game was last seen running (seconds).
    p1_seen: f32,
    file_at: f32,
    /// Which premade this game plays (from the save it loaded), if one.
    premade: Option<usize>,
    /// The guest's join screen was opened on the main menu ([`p2_open_join`]).
    join_opened: bool,
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
    let port = p2.socket.as_ref().and_then(|s| s.local_addr().ok()).map_or(0, |a| a.port());
    write_file(&p2.dir.join("p2.txt"), &format!("pid={}\nport={port}\n", std::process::id()));
    let text = std::fs::read_to_string(p2.dir.join("p1.txt")).unwrap_or_default();
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
    fn session_files_read_by_key() {
        let t = "pid=12\nhwnd=0\nlayout=side-by-side\n";
        assert_eq!(kv(t, "pid").as_deref(), Some("12"));
        assert_eq!(Layout::parse(&kv(t, "layout").unwrap()), Layout::SideBySide);
        assert_eq!(kv(t, "port"), None);
    }
}
