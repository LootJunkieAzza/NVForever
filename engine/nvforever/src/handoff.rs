//! Loading a save, or going back to the main menu, while playing, without the window
//! closing and another opening (docs/design/save-load.md, "Loading while playing").
//! NVForever still starts again, so a save always lands in a world built fresh from the
//! plugins and nothing of the run before carries over; the change of program is hidden
//! behind a loading screen [design, owner 2026-10-16]. The two runs overlap only for the
//! new run's first moments, not for its whole load [owner 2026-11-01, #1357: the old run
//! held its whole game, about 3.7 GB, while the new one loaded]:
//!
//! 1. This run brings a loading screen up (`nvf_play::loading::CoverRequest`, its picture
//!    one for the save's place), which holds the game. Once it is drawn, the new run
//!    starts with `--handoff <file>` and this window's place and size (`--window-at`,
//!    `--size`, `--window-maximized`); this run then updates a few times a second only.
//! 2. The new run opens its window hidden at that place ([`Hidden`]). A save it cannot
//!    load (made with plugins that cannot be loaded now, damaged, of another format) opens
//!    the main menu instead, and once the window is up a message box says why
//!    ([`LoadFailed`]; owner 2026-11-01), as for `--load` given by hand. As soon as its own
//!    loading screen (or the main menu) is up and drawn for [`SETTLE_SECS`], it shows its
//!    window over the old one, takes the focus and writes the file; it loads behind its
//!    screen from then on, alone. (Holding its script data build until then took the
//!    swap's peak from 7.6 to 6.1 GB but the load from 6-12 s to 16 s: not done.)
//!    Its world (`world::stream`, `readahead::read_ahead`) waits until the old process
//!    has ended ([`OldRun`]; at most [`OLD_RUN_WAIT_SECS`] after the file is written),
//!    so the two runs never hold a world each: before that, streaming went on in the
//!    frames before the window was shown, and on a 4 GB laptop card a lobby join held
//!    both runs' games for 13 s and lost both GPU devices [owner's playtest
//!    2026-11-08]. The old run lets its caches go as soon as the new run has started
//!    (`memory_budget::flush_now`).
//!    Since queue item 132 (owner's playtest 2026-11-09) the new run, once its game data
//!    is read and before it opens its GPU device, writes a second file ([`gpu_file`]) and
//!    waits for this run's process to end ([`before_gpu`], at most [`OLD_GPU_WAIT_SECS`]):
//!    the two runs never have a GPU device each, and the old window goes a few seconds
//!    before the new one shows. Steps 2 and 3 below are then what is left of the old
//!    order, kept for a new run that could not wait.
//! 3. This run sees the file and ends: the frame's exit logs are written (`Last`), then
//!    the process ends at once ([`EndNow`]) instead of taking its world apart piece by
//!    piece, which held its memory for about 2 s more.
//!
//! If the new run ends before it is shown (it stops at the start: a damaged archive, say),
//! the screen comes down and this game goes on; when the new run's log says why it
//! stopped, the player is told in a message box (not in unattended runs) and the log line
//! carries it. A new run that fails later, behind its own screen, is on its own: its crash
//! report is the player's. Closing this window while it waits ends the new run too. A new
//! run still hidden after [`SHOW_AFTER_SECS`] shows itself anyway.
//!
//! The new run gets this run's arguments but none of its test aids (`NVF_PLAY_TEST_*`:
//! a console line that loads would load again in every new run), except test aid
//! `NVF_PLAY_TEST_HANDOFF_EXIT=1`: the new run ends once it has taken over and its world
//! is in (the save put back) or its main menu is open, so a smoke run does not leave it
//! running; and the aids that
//! only measure (`nvf_play::MEASURING_AIDS`), so a session played by hand logs its hitches
//! after a load.
//!
//! Log: `load: the new run is about to open its GPU device after <s> s; ending this one`
//! (the old run), `handoff: the old run (process <pid>) ended <s> s after it was told;
//! this run's GPU device opens` (the new run; `is still going ... opens anyway` past the
//! time limit), `handoff: the world waits for the old run (process <pid>) to end`, `handoff: the
//! old run has ended after <s> s; the world loads` (or `... is still going <s> s after it
//! was told; the world loads anyway`),
//! `load: handing over to a new run with --load <path>` (`with --menu`), `load:
//! <why>; opening the main menu` (the new run's, a save it could not load), `handoff: window shown after <s>
//! s` (`, before its screen was up` past the time limit), `load: the new run is up after
//! <s> s; ending this one`, `load: the new run ended (<status>) before it was ready[:
//! <why>]; staying`.

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::{UpdateMode, WinitSettings, WINIT_WINDOWS};
use nvf_play::loading::{CoverRequest, Loading, Uncover};
use nvf_play::save::{LoadRequest, Restoring, SaveWrites};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Real seconds (and at least [`SETTLE_FRAMES`] frames) the new run stays hidden after
/// its loading screen (or the main menu) is up, so the first frame shown is the screen
/// [engineering choice, as `nvf_play::loading::SETTLE_SECS`].
pub const SETTLE_SECS: f64 = 0.25;
pub const SETTLE_FRAMES: u32 = 3;

/// A new run shows its window after this many seconds whatever is still loading
/// [engineering choice: as long as a loading screen waits, `nvf_play::loading`].
pub const SHOW_AFTER_SECS: f64 = 120.0;

/// Frames the old run's loading screen is up before the new run starts
/// [engineering choice: the screen is on screen before the long frame of starting it].
const COVER_FRAMES: u32 = 3;

/// The one test aid the new run keeps (module notes).
const EXIT_AID: &str = "NVF_PLAY_TEST_HANDOFF_EXIT";

/// How often the old run updates while it waits [engineering choice: often enough to see
/// the new run's file soon, rarely enough to leave the computer to the new run].
const WAIT_TICK: Duration = Duration::from_millis(50);

/// Where the old window is, for the new run's arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// The window's outer top-left corner on the desktop.
    pub at: Option<IVec2>,
    /// Its drawing area in pixels.
    pub size: UVec2,
    pub maximized: bool,
}

impl Placement {
    /// The arguments that open the new run's window the same way.
    pub fn args(&self) -> Vec<String> {
        let mut out = vec!["--size".to_string(), format!("{}x{}", self.size.x, self.size.y)];
        if let Some(p) = self.at {
            // One word: a value starting with '-' is not taken for an option.
            out.push(format!("--window-at={},{}", p.x, p.y));
        }
        if self.maximized {
            out.push("--window-maximized".into());
        }
        out
    }
}

/// `--window-at X,Y`.
pub fn parse_at(s: &str) -> Option<IVec2> {
    let (x, y) = s.split_once(',')?;
    Some(IVec2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// The new run, while this one waits for it. Dropped while still waiting (this run ending
/// some other way, as by its window being closed), it ends the new run too.
struct Successor {
    child: Child,
    file: PathBuf,
    started: Instant,
    /// The update mode before the wait.
    settings: WinitSettings,
    /// The new run is up: it goes on alone.
    handed_over: bool,
}

impl Drop for Successor {
    fn drop(&mut self) {
        if !self.handed_over && matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = std::fs::remove_file(&self.file);
        let _ = std::fs::remove_file(gpu_file(&self.file));
    }
}

/// What the old run is doing about a load or a return to the main menu.
#[derive(Default)]
enum Leaving {
    #[default]
    No,
    /// Asked; saves in flight are still being written. `None`: the main menu.
    Asked(Option<LoadRequest>),
    /// The screen is up since this many frames.
    Covered(Option<LoadRequest>, u32),
    Waiting(Successor),
    /// The new run is up: this one ends after this frame's `Last` ([`EndNow`]).
    HandedOver,
}

#[derive(Resource, Default)]
pub struct Handoff(Leaving);

impl Handoff {
    /// The loading screen of a load (or of the way to the main menu) is up and a new run
    /// takes over, or is about to start: what this run holds for others (the co-op host's
    /// port, `crate::coop`) goes now. False again when the new run failed and this one
    /// stays.
    pub fn leaving(&self) -> bool {
        matches!(self.0, Leaving::Covered(..) | Leaving::Waiting(_) | Leaving::HandedOver)
    }
}

/// The new run's arguments: this run's without those the save or the window decide
/// (`keep`), and what to open.
pub fn new_run_args(keep: &[String], what: Option<&LoadRequest>, placement: Option<Placement>, file: &Path) -> Vec<String> {
    let mut args = keep.to_vec();
    match what {
        Some(r) => args.extend(["--load".to_string(), r.path.display().to_string()]),
        None => args.push("--menu".into()),
    }
    args.extend(placement.map(|p| p.args()).unwrap_or_default());
    args.push("--handoff".into());
    args.push(file.display().to_string());
    args
}

/// Where the save starts, for the loading screen's picture.
fn destination_of(r: &LoadRequest, here: nvf_play::Location) -> Option<fnv_world::loadscreen::Destination> {
    let f = fnv_save::read_header(&r.path).ok()?;
    let pl = &f.header.place;
    Some(fnv_world::loadscreen::Destination {
        interior: pl.interior,
        exterior: pl.interior.is_none().then(|| (pl.world.unwrap_or(here.world), fnv_world::cell_of(pl.feet[0], pl.feet[1]))),
    })
}

fn placement(window: &Window, entity: Entity) -> Placement {
    let size = UVec2::new(window.resolution.physical_width(), window.resolution.physical_height());
    WINIT_WINDOWS.with_borrow(|windows| match windows.get_window(entity) {
        Some(w) => Placement { at: w.outer_position().ok().map(|p| IVec2::new(p.x, p.y)), size, maximized: w.is_maximized() },
        None => Placement { at: None, size, maximized: false },
    })
}

/// A message box saying why a load did not happen, on its own thread so the game goes on
/// drawing behind it; none in unattended runs, which would wait on it.
fn tell_player(why: &str) {
    // Unattended: a test aid drives this run (as nvf_play's test_aid::Unattended).
    if std::env::vars_os().any(|(k, _)| k.to_str().is_some_and(nvf_play::drives_run)) {
        info!("load: message box not shown (unattended run): the save could not be loaded: {why}");
        return;
    }
    info!("load: message box: the save could not be loaded: {why}");
    // rfd is a Windows-only dependency; elsewhere the log line carries the reason.
    #[cfg(windows)]
    {
        let text = format!("The save could not be loaded.

{why}");
        std::thread::spawn(move || {
            rfd::MessageDialog::new().set_title("NVForever").set_level(rfd::MessageLevel::Warning).set_description(text).show();
        });
    }
    #[cfg(not(windows))]
    let _ = why;
}

/// A file name no other handoff uses.
fn ready_file() -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!("nvforever-handoff-{}-{nanos}.ready", std::process::id()))
}

/// The old run's side (module notes, steps 1 and 3): loads asked for while playing
/// (`LoadRequest`) and the pause menu's return to the main menu (`MainMenuRequest`).
/// A save not yet on disk (its thumbnail, its writer thread) is waited for.
#[allow(clippy::too_many_arguments)]
pub fn leave(
    mut commands: Commands,
    (mut asked, mut to_menu): (MessageReader<LoadRequest>, MessageReader<nvf_play::pause::MainMenuRequest>),
    (writes, hold): (Res<SaveWrites>, Option<Res<crate::coop::HoldLeave>>),
    mut handoff: ResMut<Handoff>,
    (mut cover, mut uncover, mut exit): (MessageWriter<CoverRequest>, MessageWriter<Uncover>, MessageWriter<AppExit>),
    location: Res<nvf_play::Location>,
    window: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut winit: ResMut<WinitSettings>,
    _main_thread: NonSendMarker,
) {
    let request = asked.read().last().cloned().map(Some).or(to_menu.read().last().map(|_| None));
    let h = &mut handoff.0;
    if let Some(r) = request {
        match h {
            Leaving::No => *h = Leaving::Asked(r),
            _ => info!("load: already going; {} ignored", r.map_or("the main menu".to_string(), |r| r.path.display().to_string())),
        }
    }
    match std::mem::take(h) {
        Leaving::No => {}
        Leaving::HandedOver => *h = Leaving::HandedOver,
        Leaving::Asked(r) if writes.busy() => *h = Leaving::Asked(r),
        // A co-op guest going to the main menu saves its own game first (`crate::coop`).
        Leaving::Asked(None) if hold.as_deref().is_some_and(|h| h.holds()) => *h = Leaving::Asked(None),
        Leaving::Asked(r) => {
            cover.write(CoverRequest { to: r.as_ref().and_then(|r| destination_of(r, *location)) });
            *h = Leaving::Covered(r, 0);
        }
        Leaving::Covered(r, n) if n < COVER_FRAMES => *h = Leaving::Covered(r, n + 1),
        Leaving::Covered(r, _) => {
            let file = ready_file();
            let _ = std::fs::remove_file(&file);
            let keep = crate::restart_args(&crate::command_line());
            let args = new_run_args(&keep, r.as_ref(), window.single().ok().map(|(e, w)| placement(w, e)), &file);
            let started = std::env::current_exe()
                .map_err(|e| format!("cannot find this program to start it again: {e}"))
                .and_then(|exe| {
                    let mut cmd = std::process::Command::new(&exe);
                    cmd.args(&args);
                    for (k, _) in std::env::vars_os() {
                        if k.to_str().is_some_and(|k| nvf_play::drives_run(k) && k != EXIT_AID) {
                            cmd.env_remove(&k);
                        }
                    }
                    cmd.spawn().map_err(|e| format!("could not start {}: {e}", exe.display()))
                });
            match started {
                Ok(child) => {
                    info!("load: handing over to a new run with {}", r.as_ref().map_or("--menu".to_string(), |r| format!("--load {}", r.path.display())));
                    // What this run's caches hold goes at once: the new run's world
                    // waits for this process to end (`OldRun`), but its start overlaps.
                    commands.insert_resource(crate::memory_budget::flush_now());
                    let settings = winit.clone();
                    *winit = WinitSettings { focused_mode: UpdateMode::reactive_low_power(WAIT_TICK), unfocused_mode: UpdateMode::reactive_low_power(WAIT_TICK) };
                    *h = Leaving::Waiting(Successor { child, file, started: Instant::now(), settings, handed_over: false });
                }
                Err(e) => {
                    warn!("load: {e}; staying");
                    uncover.write(Uncover);
                }
            }
        }
        Leaving::Waiting(mut s) => {
            if gpu_file(&s.file).exists() {
                info!("load: the new run is about to open its GPU device after {:.2} s; ending this one", s.started.elapsed().as_secs_f64());
                s.handed_over = true;
                exit.write(AppExit::Success);
                *h = Leaving::HandedOver;
                return;
            }
            if s.file.exists() {
                info!("load: the new run is up after {:.2} s; ending this one", s.started.elapsed().as_secs_f64());
                s.handed_over = true;
                exit.write(AppExit::Success);
                *h = Leaving::HandedOver;
                return;
            }
            let ended = match s.child.try_wait() {
                Ok(None) => {
                    *h = Leaving::Waiting(s);
                    return;
                }
                Ok(Some(status)) => match crate::crash::stopped_at_start(s.child.id()) {
                    Some(why) => {
                        tell_player(&why);
                        format!("the new run ended ({status}) before it was ready: {why}")
                    }
                    None => format!("the new run ended ({status}) before it was ready"),
                },
                Err(e) => format!("cannot tell whether the new run is still going ({e})"),
            };
            warn!("load: {ended}; staying");
            *winit = s.settings.clone();
            uncover.write(Uncover);
        }
    }
}

/// The schedule after `Last` in which a run that handed over ends ([`end_now`]).
#[derive(bevy::ecs::schedule::ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EndNow;

/// Run [`end_now`] after every `Last` system of each frame.
pub fn add_end_now(app: &mut App) {
    app.init_schedule(EndNow);
    app.world_mut().resource_mut::<bevy::app::MainScheduleOrder>().insert_after(Last, EndNow);
    app.add_systems(EndNow, end_now);
}

/// The old run, once the new one is up: its exit logs are written, its saves on disk
/// (waited for before the screen came up), a frame capture finished (`--frame-telemetry`:
/// its writer otherwise finishes in the teardown this skips); the process ends here, the
/// system freeing its memory at once.
fn end_now(world: &mut World) {
    if matches!(world.resource::<Handoff>().0, Leaving::HandedOver) {
        crate::frame_telemetry::finish_for_exit(world);
        std::process::exit(0);
    }
}

/// A save given with `--load` that could not be loaded: the run opened the main menu
/// instead and tells the player why once its window is up ([`tell_load_failed`]).
#[derive(Resource)]
pub struct LoadFailed(pub String);

/// The message box for [`LoadFailed`], once the window is shown (a handoff's new run
/// shows it over the old run's window first).
pub fn tell_load_failed(mut commands: Commands, failed: Option<Res<LoadFailed>>, hidden: Option<Res<Hidden>>, windows: Query<&Window, With<PrimaryWindow>>) {
    let Some(failed) = failed else { return };
    if hidden.as_ref().is_some_and(|h| !h.told) || !windows.single().is_ok_and(|w| w.visible) {
        return;
    }
    tell_player(&failed.0);
    commands.remove_resource::<LoadFailed>();
}

/// The new run while its window is hidden (`--handoff <file>`; module notes, step 2).
#[derive(Resource)]
pub struct Hidden {
    file: PathBuf,
    maximize: bool,
    started: Instant,
    /// Since when (and for how many frames) the screen has been up.
    ready: Option<(Instant, u32)>,
    /// Frames since the window was shown: the focus follows a frame later (a hidden window
    /// takes none), then the file.
    shown: Option<u32>,
    /// Test aid `NVF_PLAY_TEST_HANDOFF_EXIT`.
    exit_after: bool,
    /// The file is written: with the test aid, the run waits for its world, then ends.
    told: bool,
}

impl Hidden {
    pub fn new(file: PathBuf, maximize: bool) -> Self {
        let exit_after = std::env::var(EXIT_AID).is_ok_and(|v| v == "1");
        Hidden { file, maximize, started: Instant::now(), ready: None, shown: None, exit_after, told: false }
    }
}

/// The new run waits this long for the old one to end after telling it, then loads its
/// world anyway (a warning) [engineering choice: the old run ends within a frame or two
/// of seeing the file; this only covers one that hangs].
pub const OLD_RUN_WAIT_SECS: f64 = 10.0;

/// False while a new run's world waits for the old run to end ([`OldRun`]).
static OLD_RUN_GONE: AtomicBool = AtomicBool::new(true);

/// Whether the world may load: no old run is still holding its memory (module notes,
/// step 2). `world::stream` and `readahead::read_ahead` do nothing until then.
pub fn old_run_gone() -> bool {
    OLD_RUN_GONE.load(Ordering::Relaxed)
}

/// The old run's process id, from the file name it gave ([`ready_file`]).
pub fn old_pid(file: &Path) -> Option<u32> {
    let name = file.file_name()?.to_str()?;
    let rest = name.strip_prefix("nvforever-handoff-")?;
    rest.split_once('-')?.0.parse().ok()
}

/// The new run's hold on the old one (`--handoff`): the old process, opened at the start
/// so a later process with the same id is never taken for it. Until it has ended, this
/// run's world does not load ([`old_run_gone`]), so the two runs never hold a world each
/// [owner's playtest 2026-11-08: a lobby join on a 4 GB laptop card ran both out of GPU
/// memory, the new run streaming its world in for 13 s while the old run's game was
/// still there].
#[derive(Resource)]
pub struct OldRun {
    /// The process handle (`HANDLE`, as an integer to be `Send`).
    handle: usize,
    pid: u32,
    opened: Instant,
    /// When this run wrote the file (or its window was shown without one).
    told: Option<Instant>,
}

impl OldRun {
    /// Open the old run named by `file`; `None` when it cannot be (it has ended already,
    /// or not on Windows): the world loads at once.
    pub fn open(file: &Path) -> Option<OldRun> {
        let pid = old_pid(file)?;
        let handle = open_process(pid)?;
        OLD_RUN_GONE.store(false, Ordering::Relaxed);
        info!("handoff: the world waits for the old run (process {pid}) to end");
        Some(OldRun { handle, pid, opened: Instant::now(), told: None })
    }
}

impl Drop for OldRun {
    fn drop(&mut self) {
        close_process(self.handle);
        OLD_RUN_GONE.store(true, Ordering::Relaxed);
    }
}

#[cfg(windows)]
fn open_process(pid: u32) -> Option<usize> {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
    // SAFETY: plain Win32 call; a null handle is checked.
    let h = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    (!h.is_null()).then_some(h as usize)
}

#[cfg(not(windows))]
fn open_process(_pid: u32) -> Option<usize> {
    None
}

/// Whether the process behind `handle` has ended.
#[cfg(windows)]
fn process_ended(handle: usize) -> bool {
    use windows_sys::Win32::Foundation::{HANDLE, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    // SAFETY: the handle is open until `close_process`.
    unsafe { WaitForSingleObject(handle as HANDLE, 0) != WAIT_TIMEOUT }
}

#[cfg(not(windows))]
fn process_ended(_handle: usize) -> bool {
    true
}

#[cfg(windows)]
fn close_process(handle: usize) {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    // SAFETY: closed once, from `OldRun`'s drop.
    unsafe { CloseHandle(handle as HANDLE) };
}

#[cfg(not(windows))]
fn close_process(_handle: usize) {}

/// The new run waits this long for the old run to end before opening its own GPU device,
/// then opens it anyway (a warning) [engineering choice: the old run ends within a frame or
/// two of seeing the file; this only covers one that hangs].
pub const OLD_GPU_WAIT_SECS: f64 = 15.0;

/// The file the new run writes when it is about to open its GPU device: the old run ends
/// when it appears ([`before_gpu`]).
pub fn gpu_file(file: &Path) -> PathBuf {
    let mut s = file.as_os_str().to_owned();
    s.push(".gpu");
    PathBuf::from(s)
}

/// What the new run's wait before its GPU device came to, for the log once it is up.
static BEFORE_GPU: std::sync::Mutex<Option<(bool, String)>> = std::sync::Mutex::new(None);

/// The new run, after its game data is read and before its GPU device opens: tell the
/// old run ([`gpu_file`]) and wait until its process has ended, so the two runs never
/// have a GPU device each [owner's playtest 2026-11-09, queue item 132: on a 4 GB laptop
/// card the old run, still holding 3 GB, lost its device while the new run opened its
/// own and its main menu]. The old run's window goes when it ends; the new one shows
/// once its loading screen (or main menu) is drawn.
pub fn before_gpu(file: &Path) {
    let note = match old_pid(file).and_then(|pid| open_process(pid).map(|h| (pid, h))) {
        None => (true, "the old run had already ended; this run's GPU device opens".to_string()),
        Some((pid, h)) => {
            let started = Instant::now();
            let note = match std::fs::write(gpu_file(file), b"") {
                Err(e) => (false, format!("cannot tell the old run (process {pid}) to end ({}: {e}); this run's GPU device opens with it still going", gpu_file(file).display())),
                Ok(()) if wait_process(h, OLD_GPU_WAIT_SECS) => (true, format!("the old run (process {pid}) ended {:.2} s after it was told; this run's GPU device opens", started.elapsed().as_secs_f64())),
                Ok(()) => (false, format!("the old run (process {pid}) is still going {OLD_GPU_WAIT_SECS} s after it was told; this run's GPU device opens anyway")),
            };
            close_process(h);
            note
        }
    };
    *BEFORE_GPU.lock().unwrap_or_else(|e| e.into_inner()) = Some(note);
}

/// Log what [`before_gpu`] came to (a startup system: the log is not up before).
pub fn log_before_gpu() {
    match BEFORE_GPU.lock().unwrap_or_else(|e| e.into_inner()).take() {
        Some((true, n)) => info!("handoff: {n}"),
        Some((false, n)) => warn!("handoff: {n}"),
        None => {}
    }
}

/// Wait up to `secs` for the process behind `handle` to end; whether it did.
#[cfg(windows)]
fn wait_process(handle: usize, secs: f64) -> bool {
    use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    // SAFETY: the handle is open until `close_process`.
    unsafe { WaitForSingleObject(handle as HANDLE, (secs * 1000.0) as u32) == WAIT_OBJECT_0 }
}

#[cfg(not(windows))]
fn wait_process(_handle: usize, _secs: f64) -> bool {
    true
}

/// Whether the old run has ended or the wait is over: `Some(true)` it ended, `Some(false)`
/// this run stops waiting.
fn old_run_done(ended: bool, told: Option<Instant>, now: Instant) -> Option<bool> {
    if ended {
        return Some(true);
    }
    told.filter(|t| now.duration_since(*t).as_secs_f64() >= OLD_RUN_WAIT_SECS).map(|_| false)
}

/// The new run's side, each frame until the old run has ended: then the world loads.
pub fn watch_old_run(mut commands: Commands, old: Option<ResMut<OldRun>>, hidden: Option<Res<Hidden>>) {
    let Some(mut old) = old else { return };
    let now = Instant::now();
    if old.told.is_none() && hidden.is_none_or(|h| h.told) {
        old.told = Some(now);
    }
    match old_run_done(process_ended(old.handle), old.told, now) {
        Some(true) => info!("handoff: the old run has ended after {:.2} s; the world loads", old.opened.elapsed().as_secs_f64()),
        Some(false) => warn!("handoff: the old run (process {}) is still going {OLD_RUN_WAIT_SECS} s after it was told; the world loads anyway", old.pid),
        None => return,
    }
    commands.remove_resource::<OldRun>();
}

/// Whether a hidden new run's screen is up, and for how long; `Some(late)` once the window
/// is to be shown (`late`: the time limit was reached first).
fn settle(h: &mut Hidden, up: bool, now: Instant) -> Option<bool> {
    if up {
        let (since, frames) = h.ready.get_or_insert((now, 0));
        *frames += 1;
        if *frames >= SETTLE_FRAMES && now.duration_since(*since).as_secs_f64() >= SETTLE_SECS {
            return Some(false);
        }
    } else {
        h.ready = None;
    }
    (now.duration_since(h.started).as_secs_f64() >= SHOW_AFTER_SECS).then_some(true)
}

/// The new run's side: once its loading screen (or the main menu) is up, show the window
/// over the old run's, take the focus and tell the old run.
#[allow(clippy::too_many_arguments)]
pub fn reveal(
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    hidden: Option<ResMut<Hidden>>,
    mut loading: ResMut<Loading>,
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
    streamer: Res<crate::world::Streamer>,
    phys: Option<Res<nvf_play::phys::PhysState>>,
    restoring: Restoring,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Some(mut h) = hidden else { return };
    let Ok(mut window) = windows.single_mut() else { return };
    match h.shown {
        None => {
            if !loading.unseen {
                loading.unseen = true;
            }
            let up = loading.active() || main_menu.is_some_and(|m| m.open);
            let Some(late) = settle(&mut h, up, Instant::now()) else { return };
            info!("handoff: window shown after {:.2} s{}", h.started.elapsed().as_secs_f64(), if late { ", before its screen was up" } else { "" });
            window.visible = true;
            if h.maximize {
                window.set_maximized(true);
            }
            loading.unseen = false;
            h.shown = Some(0);
        }
        Some(0) => {
            window.focused = true;
            h.shown = Some(1);
        }
        Some(_) if !h.told => {
            if let Err(e) = std::fs::write(&h.file, b"") {
                warn!("handoff: cannot tell the old run ({}: {e}); it ends when its own window closes", h.file.display());
            }
            h.told = true;
            if !h.exit_after {
                commands.remove_resource::<Hidden>();
            }
        }
        Some(_) => {
            // Test aid: the world in, as a player would first see it, or the main menu.
            let menu = main_menu.is_some_and(|m| m.open);
            let world_in = streamer.settled && phys.is_none_or(|p| p.loading_done()) && !restoring.busy() && !loading.starting();
            if menu || world_in {
                info!("test aid {EXIT_AID}: the new run took over and its {} is in; ending it", if menu { "main menu" } else { "world" });
                commands.remove_resource::<Hidden>();
                exit.write(AppExit::Success);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_run_finds_the_old_runs_process_in_the_file_name() {
        let f = std::env::temp_dir().join("nvforever-handoff-44808-1762617963920963000.ready");
        assert_eq!(old_pid(&f), Some(44808));
        assert_eq!(old_pid(Path::new("ready")), None);
        assert_eq!(old_pid(Path::new("nvforever-handoff-x-1.ready")), None);
        let f = ready_file();
        assert_eq!(old_pid(&f), Some(std::process::id()), "{}", f.display());
    }

    #[test]
    fn the_gpu_file_sits_beside_the_handoff_file() {
        let f = std::env::temp_dir().join("nvforever-handoff-44808-17.ready");
        assert_eq!(gpu_file(&f), std::env::temp_dir().join("nvforever-handoff-44808-17.ready.gpu"));
        assert_eq!(old_pid(&gpu_file(&f)), Some(44808));
    }

    #[test]
    fn the_new_run_waits_for_no_old_run_when_there_is_none() {
        // No such process: nothing to wait for, logged as ended.
        let f = std::env::temp_dir().join(format!("nvforever-handoff-{}-1.ready", u32::MAX - 1));
        before_gpu(&f);
        let note = BEFORE_GPU.lock().unwrap().take();
        assert!(matches!(&note, Some((true, n)) if n.contains("already ended")), "{note:?}");
        assert!(!gpu_file(&f).exists());
    }

    #[test]
    fn the_world_waits_for_the_old_run_or_gives_up_after_telling_it() {
        let t0 = Instant::now();
        let s = |secs: f64| t0 + Duration::from_secs_f64(secs);
        assert_eq!(old_run_done(true, None, s(0.0)), Some(true));
        // Not told yet: waits however long the start takes.
        assert_eq!(old_run_done(false, None, s(300.0)), None);
        assert_eq!(old_run_done(false, Some(s(5.0)), s(5.0 + OLD_RUN_WAIT_SECS - 0.1)), None);
        assert_eq!(old_run_done(false, Some(s(5.0)), s(5.0 + OLD_RUN_WAIT_SECS)), Some(false));
    }

    #[test]
    fn the_new_run_opens_where_the_old_window_is() {
        let keep = vec!["--play".to_string(), "--msaa".into(), "8".into()];
        let r = LoadRequest { path: PathBuf::from(r"C:\saves\quicksave.nvfsave") };
        let p = Placement { at: Some(IVec2::new(-1910, 40)), size: UVec2::new(1600, 900), maximized: true };
        let args = new_run_args(&keep, Some(&r), Some(p), Path::new("ready"));
        assert_eq!(args, ["--play", "--msaa", "8", "--load", r"C:\saves\quicksave.nvfsave", "--size", "1600x900", "--window-at=-1910,40", "--window-maximized", "--handoff", "ready"]);
        let args = new_run_args(&keep, None, None, Path::new("ready"));
        assert_eq!(args, ["--play", "--msaa", "8", "--menu", "--handoff", "ready"]);
        assert_eq!(parse_at("-1910,40"), Some(IVec2::new(-1910, 40)));
        assert_eq!(parse_at("12"), None);
    }

    #[test]
    fn the_window_waits_for_the_world_then_settles() {
        let t0 = Instant::now();
        let mut h = Hidden::new(PathBuf::new(), false);
        h.started = t0;
        assert_eq!(settle(&mut h, false, t0), None);
        let late = |s: f64| t0 + Duration::from_secs_f64(s);
        // In, but not for long enough (frames, then time).
        assert_eq!(settle(&mut h, true, late(1.0)), None);
        assert_eq!(settle(&mut h, true, late(1.1)), None);
        assert_eq!(settle(&mut h, true, late(1.2)), None);
        assert_eq!(settle(&mut h, true, late(1.3)), Some(false));
        // Out again starts the wait over.
        let mut h = Hidden::new(PathBuf::new(), false);
        h.started = t0;
        for s in [1.0, 1.1, 1.2] {
            settle(&mut h, true, late(s));
        }
        assert_eq!(settle(&mut h, false, late(1.3)), None);
        assert_eq!(settle(&mut h, true, late(1.4)), None);
        // Never in: shown at the time limit.
        assert_eq!(settle(&mut h, false, late(SHOW_AFTER_SECS)), Some(true));
    }
}
