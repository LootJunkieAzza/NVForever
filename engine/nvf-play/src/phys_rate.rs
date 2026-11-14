//! The physics rate (docs/spec-notes/physics.md §5): how many fixed steps a second the
//! physics world takes, chosen on the settings page's Physics rate (`physics-rate` in
//! NVForever.ini) or for one run with `--physics-hz`, and lowered by a guard while the steps
//! cost too much of the frame [design: the owner's approved plan, 2026-11-11].
//!
//! - **Choices**: 60 (the default, the rate the captures were taken at), 120, 144 and 160
//!   steps a second, or Match FPS: the frame rate the game holds, from 60 up to what
//!   the display's pace allows ([`display_hz`]: the frame-rate cap in effect, else the
//!   refresh rate; at most 160), following it down when the frames stay slow and back up
//!   when they recover ([`FpsFollow`]). `--physics-hz` takes any whole rate from 60 to 160
//!   or `fps` (`display`, the choice's old name, too), and wins over the setting for its run.
//!   240 was offered until 2026-11-11 and taken out [design: the owner, "remove the 240hz
//!   option if it would cause issues"]: in battle-50 its steps took 45 % of the frame, just
//!   under the guard's [`DOWN_SHARE`], pushes slid farthest there, and it gave nothing on a
//!   160 Hz screen; an INI line of a rate over 160 from then reads as 160
//!   ([`RateChoice::parse_stored`]).
//! - **Steps stay fixed**: each step is `1/rate` s of the game's clock, so a rate gives the
//!   same results every time (the parity lab and the tests stay at 60). While the clock
//!   runs slow (VATS, a kill camera) the step shrinks with it, `dt = 1/rate × speed`, never
//!   under [`crate::phys::SLOWEST_PACED`] of it, so the steps a *real* second stay the
//!   rate's and what physics moves does not go in jerks [design: the fork owner
//!   2026-11-10, "vats slows down time by X in the shooting/kill slowmo cutscenes so the
//!   physics ticks are increased by X"]; a clock run faster than [`crate::phys::FASTEST_PACED`] lengthens the step
//!   instead of adding steps, so the steps a real second never pass that many times the
//!   rate ([`crate::phys::paced_step`]).
//! - **Catch-up**: one frame runs at most [`crate::phys::MAX_FRAME_SECONDS`] of steps at the
//!   rate (15 at 60, 40 at 160), what the virtual clock passes on in its longest frame;
//!   time beyond is dropped. A frame longer than that cannot grow the backlog, so a busy
//!   moment slows the game down instead of spiralling.
//! - **The guard** ([`Guard`]): each frame the time spent in the player's `step` system
//!   (the steps and what runs between them) is measured, always and cheaply (two clock
//!   reads; `crate::phys_load` keeps the finer counts for `--log-frame-times`). When it
//!   was over [`DOWN_SHARE`] of the real frame time over [`DOWN_SECS`], the rate goes down
//!   a notch of [`RATES`], never under 60; after [`UP_SECS`] under [`UP_SHARE`] it goes back
//!   up a notch, towards the chosen rate, if the cost at the higher rate is foreseen under
//!   [`UP_FORESEEN`]. A fall soon after a rise doubles the wait before the next rise (up to
//!   [`UP_SECS_MOST`]), so the rate does not flap. Each switch is logged.
//! - **A change of rate in play** keeps the share of a step the clock is into
//!   (`fnv_phys::Stepper::rescale`): what is drawn between steps does not jump.
//!
//! Nothing here is saved in a game's save: the rate is a setting of the machine, and saves
//! load the same whatever it is.

use crate::phys::{paced_step, PhysState, MAX_FRAME_SECONDS};
use bevy::prelude::*;
use std::collections::VecDeque;

/// The rates the settings page offers, and the guard's notches, steps a second.
pub const RATES: [u32; 4] = [60, 120, 144, 160];
/// The lowest and the highest rate.
pub const MIN_HZ: u32 = 60;
pub const MAX_HZ: u32 = 160;
/// The highest rate builds before 2026-11-11 offered (module notes): an INI's rate up to it
/// reads as [`MAX_HZ`].
const OLD_MAX_HZ: u32 = 240;

/// The settings page's key.
pub const KEY: &str = "physics-rate";

/// A physics rate as chosen: so many steps a second, or the display's frame rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateChoice {
    Fixed(u32),
    /// Match FPS: the frame rate the game holds, up to what the display's pace allows
    /// ([`FpsFollow`]).
    Fps,
}

impl Default for RateChoice {
    fn default() -> Self {
        RateChoice::Fixed(MIN_HZ)
    }
}

impl RateChoice {
    /// The settings page's choices, in order.
    pub const LISTED: [RateChoice; 5] = [RateChoice::Fixed(60), RateChoice::Fixed(120), RateChoice::Fixed(144), RateChoice::Fixed(160), RateChoice::Fps];

    /// The name the page shows.
    pub fn name(self) -> String {
        match self {
            RateChoice::Fixed(hz) => format!("{hz} Hz"),
            RateChoice::Fps => "Match FPS".to_string(),
        }
    }

    /// As kept in NVForever.ini and given to `--physics-hz`: `60`, `fps`.
    pub fn stored(self) -> String {
        match self {
            RateChoice::Fixed(hz) => hz.to_string(),
            RateChoice::Fps => "fps".to_string(),
        }
    }

    /// A rate from `--physics-hz` or the INI, any case: a whole number of steps a second
    /// from [`MIN_HZ`] to [`MAX_HZ`] (`152`, `152 Hz`), or `fps` / `Match FPS`; `display` and
    /// `Match display`, the choice's name until 2026-11-11, are read as it too.
    pub fn parse(t: &str) -> Option<RateChoice> {
        let t = t.trim().to_ascii_lowercase();
        if matches!(t.as_str(), "fps" | "match fps" | "display" | "match display") {
            return Some(RateChoice::Fps);
        }
        let n = t.strip_suffix("hz").map_or(t.as_str(), str::trim_end);
        n.parse::<u32>().ok().filter(|hz| (MIN_HZ..=MAX_HZ).contains(hz)).map(RateChoice::Fixed)
    }

    /// [`RateChoice::parse`], only for the page's choices ([`RateChoice::LISTED`]).
    pub fn parse_listed(t: &str) -> Option<RateChoice> {
        RateChoice::parse(t).filter(|c| RateChoice::LISTED.contains(c))
    }

    /// [`RateChoice::parse_listed`] for the INI: a rate over [`MAX_HZ`] up to the old
    /// highest (`240` from a build before 2026-11-11) reads as [`MAX_HZ`], so such an INI
    /// loads without a complaint and is saved as `160` from then on.
    pub fn parse_stored(t: &str) -> Option<RateChoice> {
        RateChoice::parse_listed(t).or_else(|| {
            let t = t.trim().to_ascii_lowercase();
            let n = t.strip_suffix("hz").map_or(t.as_str(), str::trim_end);
            n.parse::<u32>().ok().filter(|hz| (MAX_HZ + 1..=OLD_MAX_HZ).contains(hz)).map(|_| RateChoice::Fixed(MAX_HZ))
        })
    }

    /// The rate it asks for at most, steps a second: Match FPS's ceiling is `display` (the
    /// frame rate [`display_hz`] found; [`MIN_HZ`] while that is not known).
    pub fn hz(self, display: Option<u32>) -> u32 {
        match self {
            RateChoice::Fixed(hz) => hz.clamp(MIN_HZ, MAX_HZ),
            RateChoice::Fps => display.unwrap_or(MIN_HZ).clamp(MIN_HZ, MAX_HZ),
        }
    }
}

/// `--physics-hz`'s parser (clap).
pub fn parse_arg(t: &str) -> Result<RateChoice, String> {
    RateChoice::parse(t).ok_or_else(|| format!("{t:?}: a whole number of steps a second from {MIN_HZ} to {MAX_HZ}, or fps"))
}

/// Match FPS's ceiling, before it is held to 60-160: the frame-rate cap
/// in effect (`cap`: the page's limit, variable refresh's and frame generation's
/// real-frame cap, the lowest of them; 0 for none); with vertical sync no more than the
/// refresh rate; with no cap the display's refresh rate (`refresh_mhz`, millihertz),
/// whether vsync holds the frames to it or not. `None`: no cap and no known refresh rate.
pub fn display_hz(cap: u32, vsync: bool, refresh_mhz: Option<u32>) -> Option<u32> {
    let refresh = refresh_mhz.filter(|&m| m > 0).map(|m| (m + 500) / 1000);
    match (cap, refresh) {
        (0, r) => r,
        (c, Some(r)) if vsync => Some(c.min(r)),
        (c, _) => Some(c),
    }
}

/// The display's pace, as the renderer (nvforever `frame_pace`) last told it: the frame
/// rate Match FPS goes up to at most ([`display_hz`]).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisplayPace(pub Option<u32>);

/// `--physics-hz` for this run: wins over the setting.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForcedRate(pub RateChoice);

/// The physics rate: the setting, the rate in effect, the guard.
#[derive(Resource, Debug, Default)]
pub struct PhysRate {
    /// The settings page's choice.
    pub setting: RateChoice,
    /// The rate the choice (or `--physics-hz`) asks for now, steps a second (0 before the
    /// first frame).
    pub wanted: u32,
    /// The rate in effect, steps a second: `wanted`, or lower while the guard holds it down.
    pub hz: u32,
    pub guard: Guard,
    /// This frame's time in the player's `step` system, seconds (`crate::phys_load::Meter`);
    /// `None`: no played frame.
    pub frame_secs: Option<f64>,
    /// The guard's switches since the start (for the exit report).
    pub switches: u32,
    /// The clock runs slow or fast and the step follows it ([`crate::phys::paced_step`]): the
    /// guard leaves such frames out, their cost is not the rate's.
    pub paced: bool,
    /// The rate the choice allows at most, steps a second: a fixed rate's own, Match FPS's
    /// ceiling ([`display_hz`]); 0 before the first frame.
    pub ceiling: u32,
    /// Match FPS is chosen: `wanted` follows [`FpsFollow`].
    pub following: bool,
    pub follow: FpsFollow,
    /// Match FPS's switches since the start (for the exit report).
    pub fps_switches: u32,
    /// Real seconds left during which Match FPS leaves the frames out after a cell came in
    /// ([`CELL_SETTLE_SECS`]), and the cells loaded so far.
    pub settle: f64,
    pub cells_loaded: usize,
}

/// Match FPS judges whole seconds of frames [design: the coordinator's spec for the owner's
/// "Match FPS", 2026-11-11].
/// - Down after [`FPS_DOWN_SECS`] whose frame rate was at least [`FPS_DOWN_SHARE`] under
///   the rate in effect: to that frame rate's [`fps_target`].
/// - Up after [`UP_SECS`] (the cost guard's wait, doubled likewise by a fall soon after a
///   rise) whose frame rate's target is above the rate: to that target. A target is
///   rounded down to tens, so a rise leaves at least the same 10 % before a fall.
pub const FPS_DOWN_SECS: usize = 2;
pub const FPS_DOWN_SHARE: f64 = 0.1;
/// Frames in the first seconds after a cell comes into the physics world are left out:
/// building it (the loader thread's work, its bodies) slows them for a moment
/// [design].
pub const CELL_SETTLE_SECS: f64 = 3.0;

/// Match FPS's rate for a sustained frame rate of `fps`: rounded down to a multiple of
/// 10, between 60 and `ceiling` (80 fps: 80 Hz; 147: 140; at or over a ceiling of 152:
/// 152).
pub fn fps_target(fps: f64, ceiling: u32) -> u32 {
    let ceiling = ceiling.clamp(MIN_HZ, MAX_HZ);
    if !fps.is_finite() || fps <= 0.0 {
        return MIN_HZ;
    }
    // A hair over, so a frame rate summed from frame times (149.99999…) rounds as meant.
    let fps = fps + 1e-6;
    if fps >= f64::from(ceiling) {
        return ceiling;
    }
    let tens = ((fps / 10.0).floor() as u32).saturating_mul(10);
    tens.clamp(MIN_HZ, ceiling)
}

/// Whether a played frame counts for Match FPS: it is chosen, and the clock is neither
/// slowed or sped up (`paced`) nor paused, nor did a cell come in within
/// [`CELL_SETTLE_SECS`] (`settle`: seconds of that left). Frames not played (a menu, a
/// conversation, a loading screen, no player, a cell not loaded yet) never reach it.
pub fn fps_counts(following: bool, paced: bool, paused: bool, settle: f64) -> bool {
    following && !paced && !paused && settle <= 0.0
}

/// A switch of Match FPS: from the rate, to the rate, the frame rate over the seconds it
/// judged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FpsMove {
    pub from: u32,
    pub to: u32,
    pub fps: f64,
    pub secs: usize,
}

/// Match FPS's rate: it follows the game's sustained frame rate between 60 and the ceiling
/// (module notes, [`FPS_DOWN_SECS`]). Fed only the frames that count (played, not paused,
/// not under a slowed clock, not just after a cell came in).
#[derive(Debug, Clone, Default)]
pub struct FpsFollow {
    /// The rate it asks for now (0: not started).
    pub hz: u32,
    /// The second being summed: frames, real seconds.
    now: (u32, f64),
    /// The last whole seconds, newest last.
    seconds: VecDeque<(u32, f64)>,
    clock: f64,
    last_rise: Option<f64>,
    up_secs: usize,
}

impl FpsFollow {
    /// Start again at `ceiling` (the choice or the ceiling changed).
    pub fn reset(&mut self, ceiling: u32) {
        *self = FpsFollow { hz: ceiling, ..FpsFollow::default() };
    }

    /// The seconds a rise waits for now.
    pub fn up_secs(&self) -> usize {
        self.up_secs.max(UP_SECS)
    }

    /// One frame that counts, `real` seconds long, under `ceiling`: a switch when one
    /// second's end decides one.
    pub fn frame(&mut self, real: f64, ceiling: u32) -> Option<FpsMove> {
        if !(real > 0.0 && real.is_finite()) {
            return None;
        }
        if self.hz == 0 || self.hz > ceiling {
            self.reset(ceiling);
        }
        self.now.0 += 1;
        self.now.1 += real;
        self.clock += real;
        if self.now.1 < 1.0 {
            return None;
        }
        self.seconds.push_back(std::mem::take(&mut self.now));
        while self.seconds.len() > UP_SECS_MOST {
            self.seconds.pop_front();
        }
        let fps_over = |n: usize, s: &VecDeque<(u32, f64)>| -> Option<f64> {
            (s.len() >= n).then(|| {
                let (f, r) = s.iter().rev().take(n).fold((0u32, 0.0), |(f, r), &(a, b)| (f + a, r + b));
                f64::from(f) / r
            })
        };
        let hz = self.hz;
        // Every one of the last seconds slow, not their mean: a second from before the
        // drop does not decide where it goes.
        let low = f64::from(hz) * (1.0 - FPS_DOWN_SHARE);
        let all_low = self.seconds.len() >= FPS_DOWN_SECS && self.seconds.iter().rev().take(FPS_DOWN_SECS).all(|&(f, r)| f64::from(f) / r <= low);
        if let Some(fps) = fps_over(FPS_DOWN_SECS, &self.seconds).filter(|_| all_low) {
            let to = fps_target(fps, ceiling);
            if to < hz {
                if self.last_rise.is_some_and(|t| self.clock - t < FLAP_SECS) {
                    self.up_secs = (self.up_secs() * 2).min(UP_SECS_MOST);
                }
                self.hz = to;
                self.seconds.clear();
                return Some(FpsMove { from: hz, to, fps, secs: FPS_DOWN_SECS });
            }
        }
        // Every one of the last seconds with room over the rate, to the lowest of their
        // targets: a slow second from before the recovery does not count against it.
        let up = self.up_secs();
        let lowest = (self.seconds.len() >= up).then(|| self.seconds.iter().rev().take(up).map(|&(f, r)| f64::from(f) / r).fold(f64::INFINITY, f64::min));
        if let Some(fps) = lowest {
            let to = fps_target(fps, ceiling);
            if to > hz {
                self.hz = to;
                self.seconds.clear();
                self.last_rise = Some(self.clock);
                return Some(FpsMove { from: hz, to, fps, secs: up });
            }
        }
        None
    }
}

/// The most steps one frame runs at `hz` steps a second: [`MAX_FRAME_SECONDS`] of them
/// (15 at 60, as `crate::phys::play_params` has).
pub fn catch_up_steps(hz: u32) -> u32 {
    (MAX_FRAME_SECONDS * hz as f32).ceil() as u32
}

/// Over this share of the frame time spent in the physics over [`DOWN_SECS`], the rate
/// goes down a notch [design: the owner's plan, "about half the frame time"].
pub const DOWN_SHARE: f64 = 0.5;
pub const DOWN_SECS: usize = 2;
/// Under this share over [`UP_SECS`] (doubled after each fall that soon follows a rise, up
/// to [`UP_SECS_MOST`]), the rate goes up a notch [design: the owner's plan, "under a
/// quarter for 5 s"].
pub const UP_SHARE: f64 = 0.25;
pub const UP_SECS: usize = 5;
pub const UP_SECS_MOST: usize = 80;
/// A rise is only taken when the share it is foreseen to cost (the share now × the higher
/// rate / the rate now) stays under this, so a rise from 60 straight to 160-worth of cost
/// is not tried [design].
pub const UP_FORESEEN: f64 = 0.4;
/// A fall within this many seconds of a rise counts as flapping [design].
pub const FLAP_SECS: f64 = 30.0;

/// The notch below `hz` ([`RATES`]); `None` at [`MIN_HZ`].
pub fn notch_below(hz: u32) -> Option<u32> {
    RATES.iter().rev().copied().find(|&r| r < hz)
}

/// The notch above `hz` towards `wanted` (never past it); `None` at `wanted`.
pub fn notch_above(hz: u32, wanted: u32) -> Option<u32> {
    (hz < wanted).then(|| RATES.iter().copied().find(|&r| r > hz).map_or(wanted, |r| r.min(wanted)))
}

/// A switch of the guard: from the rate, to the rate, and the share of the frame time
/// the physics took over the seconds it judged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GuardMove {
    Down { from: u32, to: u32, share: f64, secs: usize },
    Up { from: u32, to: u32, share: f64, secs: usize },
}

/// The fallback (module notes): whole seconds of (physics time, real time), judged at the
/// end of each.
#[derive(Debug, Clone, Default)]
pub struct Guard {
    /// The highest rate it lets run now; `None`: no limit.
    limit: Option<u32>,
    /// The second being summed: physics seconds, real seconds.
    now: (f64, f64),
    /// The last whole seconds, newest last.
    seconds: VecDeque<(f64, f64)>,
    /// Seconds of play counted since the start, and when the last rise was.
    clock: f64,
    last_rise: Option<f64>,
    /// The seconds a rise waits for now ([`UP_SECS`], doubled by flapping).
    up_secs: usize,
}

impl Guard {
    /// The rate it lets run when `wanted` is asked for.
    pub fn limit(&self, wanted: u32) -> u32 {
        self.limit.map_or(wanted, |l| l.min(wanted))
    }

    /// Start again (the choice changed): no limit, nothing measured.
    pub fn reset(&mut self) {
        *self = Guard::default();
    }

    /// The seconds a rise waits for now.
    pub fn up_secs(&self) -> usize {
        self.up_secs.max(UP_SECS)
    }

    /// One played frame at `hz` (of `wanted`): `physics` seconds in the physics of `real`
    /// seconds of frame. A switch when one second's end decides one.
    pub fn frame(&mut self, hz: u32, wanted: u32, physics: f64, real: f64) -> Option<GuardMove> {
        if !(real > 0.0 && real.is_finite() && physics.is_finite()) {
            return None;
        }
        self.now.0 += physics.max(0.0);
        self.now.1 += real;
        self.clock += real;
        if self.now.1 < 1.0 {
            return None;
        }
        self.seconds.push_back(std::mem::take(&mut self.now));
        while self.seconds.len() > UP_SECS_MOST {
            self.seconds.pop_front();
        }
        let share_over = |n: usize, s: &VecDeque<(f64, f64)>| -> Option<f64> {
            (s.len() >= n).then(|| {
                let (p, r) = s.iter().rev().take(n).fold((0.0, 0.0), |(p, r), &(a, b)| (p + a, r + b));
                p / r
            })
        };
        if let (Some(share), Some(to)) = (share_over(DOWN_SECS, &self.seconds), notch_below(hz)) {
            if share > DOWN_SHARE {
                if self.last_rise.is_some_and(|t| self.clock - t < FLAP_SECS) {
                    self.up_secs = (self.up_secs() * 2).min(UP_SECS_MOST);
                }
                self.limit = Some(to);
                self.seconds.clear();
                return Some(GuardMove::Down { from: hz, to, share, secs: DOWN_SECS });
            }
        }
        let up = self.up_secs();
        if let (Some(share), Some(to)) = (share_over(up, &self.seconds), notch_above(hz, wanted)) {
            if share < UP_SHARE && share * f64::from(to) / f64::from(hz.max(1)) < UP_FORESEEN {
                self.limit = (to < wanted).then_some(to);
                self.seconds.clear();
                self.last_rise = Some(self.clock);
                return Some(GuardMove::Up { from: hz, to, share, secs: up });
            }
        }
        None
    }
}

/// The resources and systems (`crate::phys::PhysPlugin` adds them).
pub(crate) fn setup(app: &mut App) {
    app.init_resource::<PhysRate>()
        .init_resource::<DisplayPace>()
        .add_systems(Update, pace.after(crate::PlaySet::Input).before(crate::PlaySet::Physics))
        .add_systems(Last, guard)
        .add_systems(Last, report);
    if let Ok(v) = std::env::var(SLOW_FRAMES_VAR) {
        match parse_slow_frames(&v) {
            Ok(w) => {
                app.insert_resource(SlowFrames(w)).add_systems(Last, slow_frames.after(guard));
            }
            Err(e) => warn!("{SLOW_FRAMES_VAR}: {e}"),
        }
    }
}

/// Test aid `NVF_PLAY_TEST_SLOW_FRAMES=<from>:<to>:<ms>[,…]`: from `from` to `to` real
/// seconds after the start, every frame is held `ms` milliseconds longer, as a heavy scene
/// would hold it (Match FPS's smoke check).
pub const SLOW_FRAMES_VAR: &str = "NVF_PLAY_TEST_SLOW_FRAMES";

/// [`SLOW_FRAMES_VAR`]'s windows: (from, to, held) in real seconds, and whether each was
/// told in the log as begun and as over.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct SlowFrames(pub Vec<(f64, f64, f64, u8)>);

/// [`SLOW_FRAMES_VAR`]'s value.
pub fn parse_slow_frames(v: &str) -> Result<Vec<(f64, f64, f64, u8)>, String> {
    v.split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            let p: Vec<f64> = s.split(':').map(|x| x.trim().parse::<f64>().map_err(|_| format!("{s:?}: <from>:<to>:<ms>, in seconds and milliseconds"))).collect::<Result<_, _>>()?;
            match p[..] {
                [a, b, ms] if a.is_finite() && b > a && ms > 0.0 && ms < 1000.0 => Ok((a, b, ms / 1000.0, 0)),
                _ => Err(format!("{s:?}: <from>:<to>:<ms>, from before to, 0 < ms < 1000")),
            }
        })
        .collect()
}

fn slow_frames(real: Res<Time<Real>>, mut w: ResMut<SlowFrames>) {
    let now = real.elapsed_secs_f64();
    for (from, to, held, told) in w.0.iter_mut() {
        if now >= *from && now < *to {
            if *told == 0 {
                info!("test aid {SLOW_FRAMES_VAR}: frames held {:.0} ms longer from {from:.1} s to {to:.1} s", *held * 1000.0);
                *told = 1;
            }
            std::thread::sleep(std::time::Duration::from_secs_f64(*held));
        } else if now >= *to && *told == 1 {
            info!("test aid {SLOW_FRAMES_VAR}: frames no longer held ({to:.1} s)");
            *told = 2;
        }
    }
}

/// The settings page's row, in the Display group. The renderer registers it after its own
/// Display rows (nvforever `settings::register`), so the rows before it keep the registry's
/// order and the INI the order its keys had: only the `physics-rate` line is new
/// [design: an INI shared with builds without the row reads and writes the same].
pub fn register_setting(app: &mut App) {
    crate::settings::add_choice_list(
        app,
        crate::settings::Setting {
            key: KEY,
            group: "Display",
            label: "Physics rate",
            note: "How many times a second the physics moves the world on: the player, the people, what falls and what is pushed. A higher rate answers the keys sooner and meets collisions more finely; its cost to the processor grows with the rate. Match FPS: as many as the frames a second the game draws, from 60 up to what the display sync and the frame rate limit allow (at most 160); it follows the frame rate down when it stays low and back up when it recovers. When the physics takes over half of each frame, the rate goes down a step (never under 60) until there is room again.",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        crate::settings::ChoiceList {
            names: |_| RateChoice::LISTED.iter().map(|c| c.name()).collect(),
            chosen: |w| w.resource::<PhysRate>().setting.name(),
            pick: pick_setting,
            save: |w| w.resource::<PhysRate>().setting.stored(),
            load: pick_setting,
            ask_keep: false,
        },
    );
    crate::settings::set_disabled_status(app, KEY, |w| w.contains_resource::<ForcedRate>().then_some("Set by --physics-hz for this run"));
}

/// The page's choice of this name or INI text; false: not one of its choices.
fn pick_setting(w: &mut World, name: &str) -> bool {
    let choice = RateChoice::LISTED.into_iter().find(|c| c.name().eq_ignore_ascii_case(name.trim())).or_else(|| RateChoice::parse_stored(name));
    match choice {
        Some(c) => {
            w.resource_mut::<PhysRate>().setting = c;
            true
        }
        None => false,
    }
}

/// What the log says the rate comes from.
fn source(forced: Option<RateChoice>, choice: RateChoice, display: Option<u32>) -> String {
    let by = if forced.is_some() { format!("--physics-hz {}", choice.stored()) } else { "the setting".to_string() };
    match choice {
        RateChoice::Fixed(_) => by,
        RateChoice::Fps => match display {
            Some(d) => format!("{by}, Match FPS: up to the display's {d} fps"),
            None => format!("{by}, Match FPS: the display's rate not known yet"),
        },
    }
}

/// The rate in effect and the step for the game's clock, before the physics steps
/// (module notes): the choice, the guard's limit, the clock's speed.
fn pace(virt: Res<Time<Virtual>>, forced: Option<Res<ForcedRate>>, screen: Res<DisplayPace>, mut rate: ResMut<PhysRate>, mut phys: ResMut<PhysState>, mut told: Local<Option<(f32, f32)>>) {
    let forced = forced.map(|f| f.0);
    let choice = forced.unwrap_or(rate.setting);
    let ceiling = choice.hz(screen.0);
    let following = choice == RateChoice::Fps;
    if ceiling != rate.ceiling || following != rate.following {
        // Another choice, or Match FPS's ceiling moved: the guard and Match FPS start again
        // from it.
        let was = rate.ceiling;
        (rate.ceiling, rate.following) = (ceiling, following);
        rate.guard.reset();
        rate.follow.reset(ceiling);
        if was == 0 {
            info!("phys: physics rate {ceiling} Hz ({})", source(forced, choice, screen.0));
        } else {
            info!("phys: physics rate {was} Hz -> {ceiling} Hz ({})", source(forced, choice, screen.0));
        }
    }
    // Match FPS asks for the frame rate it follows (`guard`), up to the ceiling.
    let wanted = if following { rate.follow.hz.clamp(MIN_HZ, ceiling) } else { ceiling };
    rate.wanted = wanted;
    let hz = rate.guard.limit(wanted);
    rate.hz = hz;
    let full = 1.0 / hz as f32;
    let speed = if virt.is_paused() { 1.0 } else { virt.relative_speed() };
    let dt = paced_step(full, speed);
    let phys = &mut *phys;
    phys.world.params.max_substeps = catch_up_steps(hz);
    if (phys.world.params.dt - dt).abs() > f32::EPSILON * dt {
        // The share of a step the clock is into is kept: nothing drawn between steps jumps.
        phys.stepper.rescale(phys.world.params.dt, dt);
        phys.world.set_step(dt);
    }
    // Once a change of pace, not every frame of an easing clock; at its end, the slowest
    // clock it reached and the step then.
    let paced = dt != full;
    rate.paced = paced;
    if told.is_some() != paced {
        if paced {
            info!("phys: the clock runs at {speed:.2}: the step follows it, {:.2} ms ({:.0} steps a real second)", dt * 1000.0, speed / dt);
        } else if let Some((slowest, step)) = *told {
            info!(
                "phys: the clock is back at full speed: the step is {:.2} ms again (the clock was at {slowest:.2} at its slowest, the step {:.2} ms, {:.0} steps a real second)",
                dt * 1000.0,
                step * 1000.0,
                slowest / step
            );
        }
        *told = paced.then_some((speed, dt));
    } else if let Some(t) = told.as_mut().filter(|t| speed < t.0) {
        *t = (speed, dt);
    }
}

/// `NVF_PHYS_RATE_GUARD=0`: the cost guard never switches the rate, so a measurement runs at
/// the rate asked for however much it costs. For measuring; not a test aid: it does not
/// make the run unattended.
pub const GUARD_VAR: &str = "NVF_PHYS_RATE_GUARD";

/// The guard's frame: this frame's physics time against its real time (module notes).
fn guard(real: Res<Time<Real>>, virt: Res<Time<Virtual>>, phys: Option<Res<PhysState>>, mut rate: ResMut<PhysRate>, mut off: Local<Option<bool>>) {
    let real_dt = real.delta_secs_f64();
    // A cell came into the physics world: Match FPS leaves the next seconds out.
    if let Some(cells) = phys.map(|p| p.stats.cells_loaded) {
        if cells != rate.cells_loaded {
            rate.cells_loaded = cells;
            rate.settle = CELL_SETTLE_SECS;
        }
    }
    rate.settle = (rate.settle - real_dt).max(0.0);
    let Some(physics) = rate.frame_secs.take() else { return };
    let (hz, wanted) = (rate.hz, rate.wanted);
    // A slowed clock (VATS, a kill camera) takes fewer or shorter steps: its frames say
    // nothing about what the rate costs, and are not judged; nor, by Match FPS, the frames
    // of a paused clock or just after a cell came in.
    if hz == 0 || rate.paced || virt.is_paused() {
        return;
    }
    if fps_counts(rate.following, rate.paced, virt.is_paused(), rate.settle) {
        let ceiling = rate.ceiling;
        if let Some(m) = rate.follow.frame(real_dt, ceiling) {
            rate.fps_switches += 1;
            info!("phys: physics rate {} Hz -> {} Hz: the frame rate held {:.0} fps over {} s (Match FPS, up to {ceiling} Hz)", m.from, m.to, m.fps, m.secs);
        }
    }
    let off = *off.get_or_insert_with(|| {
        let off = std::env::var(GUARD_VAR).is_ok_and(|v| v.trim() == "0");
        if off {
            info!("phys: the physics rate's cost guard is off ({GUARD_VAR}=0): the rate stays as asked");
        }
        off
    });
    if off {
        return;
    }
    let Some(m) = rate.guard.frame(hz, wanted, physics, real.delta_secs_f64()) else { return };
    rate.switches += 1;
    match m {
        GuardMove::Down { from, to, share, secs } => {
            rate.hz = to;
            warn!("phys: physics rate {from} Hz -> {to} Hz: the physics took {:.0}% of the frame time over {secs} s (over {:.0}%); back up once there is room", share * 100.0, DOWN_SHARE * 100.0);
        }
        GuardMove::Up { from, to, share, secs } => {
            rate.hz = to;
            info!("phys: physics rate {from} Hz -> {to} Hz: the physics took {:.0}% of the frame time over {secs} s (under {:.0}%), {wanted} Hz asked for", share * 100.0, UP_SHARE * 100.0);
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, rate: Res<PhysRate>) {
    if exit.read().last().is_none() || rate.wanted == 0 {
        return;
    }
    info!("physics rate: {} Hz at exit, {} Hz asked for; {} switches by the guard, {} by Match FPS", rate.hz, rate.wanted, rate.switches, rate.fps_switches);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_parse_from_the_command_line_and_the_ini() {
        assert_eq!(RateChoice::parse("152"), Some(RateChoice::Fixed(152)));
        assert_eq!(RateChoice::parse(" 120 Hz "), Some(RateChoice::Fixed(120)));
        assert_eq!(RateChoice::parse("DISPLAY"), Some(RateChoice::Fps));
        assert_eq!(RateChoice::parse("Match display"), Some(RateChoice::Fps));
        assert_eq!(RateChoice::parse("fps"), Some(RateChoice::Fps));
        assert_eq!(RateChoice::parse("Match FPS"), Some(RateChoice::Fps));
        // The owner's INI from before the rename reads as Match FPS, kept as fps from now on.
        assert_eq!(RateChoice::parse_listed("display"), Some(RateChoice::Fps));
        assert_eq!(RateChoice::Fps.stored(), "fps");
        assert_eq!(RateChoice::Fps.name(), "Match FPS");
        for bad in ["59", "161", "240", "241", "0", "-60", "60.5", "fast", ""] {
            assert_eq!(RateChoice::parse(bad), None, "{bad}");
        }
        assert!(parse_arg("30").is_err());
        assert!(parse_arg("240").is_err(), "--physics-hz goes up to 160");
        assert_eq!(parse_arg("display"), Ok(RateChoice::Fps));
        assert_eq!(parse_arg("fps"), Ok(RateChoice::Fps));
        // The page and the INI keep to the listed choices.
        assert_eq!(RateChoice::parse_listed("152"), None);
        for c in RateChoice::LISTED {
            assert_eq!(RateChoice::parse_listed(&c.stored()), Some(c));
            assert_eq!(RateChoice::parse_listed(&c.name()), Some(c));
        }
        assert_eq!(RateChoice::default(), RateChoice::Fixed(60));
    }

    #[test]
    fn an_ini_rate_of_the_old_240_loads_and_saves_as_160() {
        assert_eq!(RateChoice::parse_stored("240"), Some(RateChoice::Fixed(160)));
        assert_eq!(RateChoice::parse_stored("240 Hz"), Some(RateChoice::Fixed(160)));
        assert_eq!(RateChoice::parse_stored("144"), Some(RateChoice::Fixed(144)));
        assert_eq!(RateChoice::parse_stored("fps"), Some(RateChoice::Fps));
        for bad in ["152", "241", "30", "fast"] {
            assert_eq!(RateChoice::parse_stored(bad), None, "{bad}");
        }
        // The page's load: no complaint, and what it saves from then on.
        let mut w = World::new();
        w.init_resource::<PhysRate>();
        assert!(pick_setting(&mut w, "240"));
        assert_eq!(w.resource::<PhysRate>().setting, RateChoice::Fixed(160));
        assert_eq!(w.resource::<PhysRate>().setting.stored(), "160");
        assert!(!pick_setting(&mut w, "300"));
    }

    #[test]
    fn match_display_follows_the_cap_in_effect() {
        // Variable refresh at 160 Hz: its cap.
        assert_eq!(display_hz(152, true, Some(160_000)), Some(152));
        // Frame generation's real-frame cap.
        assert_eq!(display_hz(78, true, Some(160_000)), Some(78));
        // Vsync, no cap: the refresh rate (a 144 Hz mode reported as 143.998).
        assert_eq!(display_hz(0, true, Some(143_998)), Some(144));
        // A page limit over the refresh rate under vsync: the refresh rate.
        assert_eq!(display_hz(240, true, Some(60_000)), Some(60));
        // Uncapped without vsync: the monitor's refresh rate.
        assert_eq!(display_hz(0, false, Some(165_000)), Some(165));
        assert_eq!(display_hz(90, false, Some(165_000)), Some(90));
        // Nothing known.
        assert_eq!(display_hz(0, true, None), None);
        // Held to 60-160.
        assert_eq!(RateChoice::Fps.hz(Some(30)), 60);
        assert_eq!(RateChoice::Fps.hz(Some(360)), 160);
        assert_eq!(RateChoice::Fps.hz(None), 60);
        assert_eq!(RateChoice::Fixed(152).hz(None), 152);
    }

    #[test]
    fn the_catch_up_covers_a_quarter_second_at_any_rate() {
        assert_eq!(catch_up_steps(60), 15);
        assert_eq!(catch_up_steps(152), 38);
        assert_eq!(catch_up_steps(160), 40);
        // As play_params has at 60.
        assert_eq!(catch_up_steps(60), crate::phys::play_params().max_substeps);
    }

    #[test]
    fn a_slowed_clock_shortens_the_step_down_to_the_shortest_one() {
        use crate::phys::{paced_step, SHORTEST_STEP};
        let close = |a: f32, b: f32| (a - b).abs() < 1e-9;
        // At 60: half speed halves the step; a quarter is the shortest step itself.
        assert!(close(paced_step(1.0 / 60.0, 0.5), 1.0 / 120.0));
        assert!(close(paced_step(1.0 / 60.0, 0.25), SHORTEST_STEP));
        // A kill camera's 0.12, VATS marking's 0.05 and the slowest 0.02: never shorter.
        for full in [1.0 / 60.0f32, 1.0 / 152.0, 1.0 / 160.0] {
            for speed in [0.12f32, 0.05, 0.02, 0.001] {
                let dt = paced_step(full, speed);
                assert!(dt >= SHORTEST_STEP - 1e-9 && dt <= full, "{full} at {speed}: {dt}");
            }
        }
        // Full speed, stopped, a faster clock: as before.
        assert_eq!(paced_step(1.0 / 152.0, 1.0), 1.0 / 152.0);
        assert_eq!(paced_step(1.0 / 152.0, 0.0), 1.0 / 152.0);
        assert!(close(paced_step(1.0 / 60.0, 8.0), 2.0 / 60.0));
    }

    /// Every rate the menu offers, and Match display at 152: under every clock VATS and the
    /// kill cameras use, the step is never shorter than the shortest one, the steps a real
    /// second are the rate's or the shortest step's (240) times the clock's, and a body crawling at 5 units/s at
    /// Goodsprings moves at every step (the positions resolve it).
    #[test]
    fn every_rate_keeps_slow_bodies_moving_under_a_slowed_clock() {
        use crate::phys::{paced_step, SHORTEST_STEP};
        use fnv_collide::{CollisionObject, Iso, Motion, Shape};
        for hz in RATES.into_iter().chain([152]) {
            let full = 1.0 / hz as f32;
            for speed in [1.0f32, 0.5, 0.25, 0.05, 0.02] {
                let dt = paced_step(full, speed);
                assert!(dt >= SHORTEST_STEP - 1e-9, "{hz} Hz at {speed}: step {dt}");
                let per_second = speed / dt;
                let want = (hz as f32).min(speed / SHORTEST_STEP);
                assert!((per_second - want).abs() < want * 1e-4, "{hz} Hz at {speed}: {per_second} steps a real second, {want} wanted");
                let mut w = fnv_phys::PhysWorld::new(fnv_phys::PhysParams::default());
                w.set_step(dt);
                let body = CollisionObject {
                    node: None,
                    node_block: 0,
                    node_transform: Iso::IDENTITY,
                    body_transform: Iso::IDENTITY,
                    motion: Motion::Dynamic,
                    layer: 1,
                    mass: 10.0,
                    friction: 0.5,
                    restitution: 0.0,
                    shape: Shape::Box { half_extents: [3.0, 3.0, 3.0] }.into(),
                    shell: 0.0,
                    material: None,
                    wind: false,
                };
                let pose = fnv_phys::RefPose { form_id: 0x99, position: [-68000.0, 4800.0, 9000.0], rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], scale: 1.0 };
                w.add_ref(&pose, &[body]);
                w.step();
                w.set_ref_velocity(0x99, 0, 5.0);
                let mut x = w.ref_bodies(0x99)[0].translation[0];
                for i in 0..20 {
                    w.step();
                    let now = w.ref_bodies(0x99)[0].translation[0];
                    assert!(now > x, "{hz} Hz at {speed}: step {i} left the body at x {x}");
                    x = now;
                }
            }
        }
    }

    #[test]
    fn match_fps_targets_the_frame_rate_in_tens_between_60_and_the_ceiling() {
        assert_eq!(fps_target(80.0, 152), 80);
        assert_eq!(fps_target(84.9, 152), 80);
        assert_eq!(fps_target(147.0, 152), 140);
        assert_eq!(fps_target(152.0, 152), 152);
        assert_eq!(fps_target(400.0, 152), 152);
        assert_eq!(fps_target(400.0, 160), 160);
        assert_eq!(fps_target(400.0, 300), 160, "never over 160");
        assert_eq!(fps_target(45.0, 152), 60, "never under 60");
        assert_eq!(fps_target(f64::NAN, 152), 60);
        assert_eq!(fps_target(0.0, 152), 60);
    }

    /// `secs` seconds of frames at `fps` into `f` under `ceiling`; the moves it made.
    fn follow(f: &mut FpsFollow, fps: f64, secs: f64, ceiling: u32) -> Vec<FpsMove> {
        (0..(secs * fps).round() as usize).filter_map(|_| f.frame(1.0 / fps, ceiling)).collect()
    }

    #[test]
    fn match_fps_steps_down_when_the_frames_stay_slow_and_back_up_when_they_recover() {
        let mut f = FpsFollow::default();
        f.reset(152);
        // At the ceiling and a little under it (147, under 10 % less): it stays.
        assert!(follow(&mut f, 152.0, 3.0, 152).is_empty());
        assert!(follow(&mut f, 147.0, 10.0, 152).is_empty());
        // An intense scene at 84 fps: down to 80 after 2 s.
        let m = follow(&mut f, 84.0, 3.0, 152);
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!((m[0].from, m[0].to, m[0].secs), (152, 80, 2));
        assert_eq!(f.hz, 80);
        // A one-second dip does not count; 81 fps stays at 80.
        let mut g = FpsFollow::default();
        g.reset(152);
        assert!(follow(&mut g, 60.0, 0.9, 152).is_empty());
        assert!(follow(&mut g, 152.0, 1.5, 152).is_empty());
        assert!(follow(&mut f, 81.0, 20.0, 152).is_empty());
        // Recovered to 150: back up after 5 s, to 150.
        let m = follow(&mut f, 150.0, 6.0, 152);
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!((m[0].from, m[0].to, m[0].secs), (80, 150, UP_SECS));
        // The ceiling: 200 fps under a ceiling of 152 asks for 152 only.
        let m = follow(&mut f, 200.0, 6.0, 152);
        assert_eq!(m.last().map(|m| m.to), Some(152));
        assert_eq!(f.hz, 152);
    }

    #[test]
    fn match_fps_waits_longer_after_flapping() {
        // Each rise to 150 brings the frames back down to 100 (as if the physics cost them):
        // the waits before a rise double.
        let mut f = FpsFollow::default();
        f.reset(150);
        let mut rises = Vec::new();
        for _ in 0..400 {
            let fps = if f.hz == 150 { 100.0 } else { 150.0 };
            for m in follow(&mut f, fps, 1.0, 150) {
                if m.to > m.from {
                    rises.push(m.secs);
                }
            }
        }
        assert!(rises.len() >= 4, "{rises:?}");
        assert!(rises.windows(2).all(|w| w[1] >= w[0]) && *rises.last().unwrap() == UP_SECS_MOST, "{rises:?}");
    }

    #[test]
    fn match_fps_leaves_out_frames_that_say_nothing_about_the_frame_rate() {
        assert!(fps_counts(true, false, false, 0.0));
        assert!(!fps_counts(false, false, false, 0.0), "not chosen");
        assert!(!fps_counts(true, true, false, 0.0), "a slowed clock (VATS, a kill camera)");
        assert!(!fps_counts(true, false, true, 0.0), "a paused clock");
        assert!(!fps_counts(true, false, false, 1.5), "just after a cell came in");
        // A ceiling that moves under it starts it again there.
        let mut f = FpsFollow::default();
        f.reset(152);
        follow(&mut f, 84.0, 3.0, 152);
        assert_eq!(f.hz, 80);
        assert!(f.frame(1.0 / 60.0, 60).is_none());
        assert_eq!(f.hz, 60, "a ceiling under the rate takes it at once");
        assert!(f.frame(0.0, 152).is_none() && f.frame(f64::NAN, 152).is_none());
    }

    #[test]
    fn notches_step_through_the_listed_rates() {
        assert_eq!(notch_below(160), Some(144));
        assert_eq!(notch_below(152), Some(144));
        assert_eq!(notch_below(120), Some(60));
        assert_eq!(notch_below(60), None);
        assert_eq!(notch_above(60, 160), Some(120));
        assert_eq!(notch_above(144, 152), Some(152));
        assert_eq!(notch_above(160, 160), None);
    }

    /// `secs` seconds of frames of `frame` s each with `share` of them in the physics;
    /// the moves the guard made.
    fn run(g: &mut Guard, hz: &mut u32, wanted: u32, share: f64, secs: f64) -> Vec<GuardMove> {
        let frame = 1.0 / 152.0;
        let mut moves = Vec::new();
        for _ in 0..(secs / frame).round() as usize {
            if let Some(m) = g.frame(*hz, wanted, share * frame, frame) {
                *hz = match m {
                    GuardMove::Down { to, .. } | GuardMove::Up { to, .. } => to,
                };
                moves.push(m);
            }
            assert_eq!(*hz, g.limit(wanted));
        }
        moves
    }

    #[test]
    fn the_guard_steps_down_under_load_and_never_under_60() {
        let mut g = Guard::default();
        let mut hz = 160;
        // Light load: nothing moves.
        assert!(run(&mut g, &mut hz, 160, 0.3, 10.0).is_empty());
        // Over half the frame: a notch every 2 s, down to 60 and no further.
        let moves = run(&mut g, &mut hz, 160, 0.7, 30.0);
        let tos: Vec<u32> = moves.iter().map(|m| if let GuardMove::Down { to, .. } = m { *to } else { 0 }).collect();
        assert_eq!(tos, [144, 120, 60]);
        assert_eq!(hz, 60);
        // A brief spike does not count: the share is over 2 s.
        let mut g = Guard::default();
        let mut hz = 160;
        assert!(run(&mut g, &mut hz, 160, 0.9, 0.8).is_empty());
        assert!(run(&mut g, &mut hz, 160, 0.1, 2.0).is_empty());
    }

    #[test]
    fn the_guard_steps_back_up_after_sustained_headroom() {
        let mut g = Guard::default();
        let mut hz = 160;
        run(&mut g, &mut hz, 160, 0.8, 2.5);
        assert_eq!(hz, 144);
        // Room: back up after 5 s.
        let moves = run(&mut g, &mut hz, 160, 0.1, 5.5);
        assert!(matches!(moves[..], [GuardMove::Up { from: 144, to: 160, .. }]), "{moves:?}");
        assert_eq!(g.limit(160), 160);
        // From 60 the next notch doubles the cost: under a quarter, but foreseen over
        // UP_FORESEEN at 120, it stays.
        let mut g = Guard::default();
        let mut hz = 120;
        run(&mut g, &mut hz, 120, 0.8, 2.5);
        assert_eq!(hz, 60);
        assert!(run(&mut g, &mut hz, 120, 0.22, 20.0).is_empty());
        let moves = run(&mut g, &mut hz, 120, 0.15, 5.5);
        assert!(matches!(moves[..], [GuardMove::Up { from: 60, to: 120, .. }]), "{moves:?}");
    }

    #[test]
    fn the_guard_waits_longer_after_flapping() {
        // A load that is too much at 160 but fine at 144: each rise is followed by a fall.
        let mut g = Guard::default();
        let mut hz = 160;
        let mut rises = Vec::new();
        let mut t = 0.0;
        while t < 300.0 {
            let share = if hz == 160 { 0.6 } else { 0.2 };
            for m in run(&mut g, &mut hz, 160, share, 1.0) {
                if let GuardMove::Up { secs, .. } = m {
                    rises.push(secs);
                }
            }
            t += 1.0;
        }
        // The waits before a rise double: 5, 10, 20, 40, 80 s.
        assert!(rises.len() >= 4 && rises.len() <= 6, "{rises:?}");
        assert!(rises.windows(2).all(|w| w[1] >= w[0]) && *rises.last().unwrap() == UP_SECS_MOST, "{rises:?}");
    }

    #[test]
    fn a_new_choice_starts_the_guard_again() {
        let mut g = Guard::default();
        let mut hz = 160;
        run(&mut g, &mut hz, 160, 0.9, 2.5);
        assert_eq!(g.limit(160), 144);
        g.reset();
        assert_eq!(g.limit(120), 120);
        assert_eq!(g.limit(160), 160);
        assert_eq!(g.frame(60, 60, f64::NAN, 0.01), None);
        assert_eq!(g.frame(60, 60, 0.001, 0.0), None);
    }
}
