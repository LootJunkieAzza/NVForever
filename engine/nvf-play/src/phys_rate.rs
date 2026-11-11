//! The physics rate (docs/spec-notes/physics.md §5): how many fixed steps a second the
//! physics world takes, chosen on the settings page's Physics rate (`physics-rate` in
//! NVForever.ini) or for one run with `--physics-hz`, and lowered by a guard while the steps
//! cost too much of the frame [design: the owner's approved plan, 2026-11-11].
//!
//! - **Choices**: 60 (the default, the rate the captures were taken at), 120, 144, 160 and
//!   240 steps a second, or Match display: the frame rate the display's pace holds
//!   ([`display_hz`]), within 60 to 240. `--physics-hz` takes any whole rate from 60 to
//!   240 or `display`, and wins over the setting for its run.
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
//!   rate (15 at 60, 60 at 240), what the virtual clock passes on in its longest frame;
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
pub const RATES: [u32; 5] = [60, 120, 144, 160, 240];
/// The lowest and the highest rate.
pub const MIN_HZ: u32 = 60;
pub const MAX_HZ: u32 = 240;

/// The settings page's key.
pub const KEY: &str = "physics-rate";

/// A physics rate as chosen: so many steps a second, or the display's frame rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateChoice {
    Fixed(u32),
    Display,
}

impl Default for RateChoice {
    fn default() -> Self {
        RateChoice::Fixed(MIN_HZ)
    }
}

impl RateChoice {
    /// The settings page's choices, in order.
    pub const LISTED: [RateChoice; 6] = [RateChoice::Fixed(60), RateChoice::Fixed(120), RateChoice::Fixed(144), RateChoice::Fixed(160), RateChoice::Fixed(240), RateChoice::Display];

    /// The name the page shows.
    pub fn name(self) -> String {
        match self {
            RateChoice::Fixed(hz) => format!("{hz} Hz"),
            RateChoice::Display => "Match display".to_string(),
        }
    }

    /// As kept in NVForever.ini and given to `--physics-hz`: `60`, `display`.
    pub fn stored(self) -> String {
        match self {
            RateChoice::Fixed(hz) => hz.to_string(),
            RateChoice::Display => "display".to_string(),
        }
    }

    /// A rate from `--physics-hz` or the INI, any case: a whole number of steps a second
    /// from [`MIN_HZ`] to [`MAX_HZ`] (`152`, `152 Hz`), or `display` / `Match display`.
    pub fn parse(t: &str) -> Option<RateChoice> {
        let t = t.trim().to_ascii_lowercase();
        if t == "display" || t == "match display" {
            return Some(RateChoice::Display);
        }
        let n = t.strip_suffix("hz").map_or(t.as_str(), str::trim_end);
        n.parse::<u32>().ok().filter(|hz| (MIN_HZ..=MAX_HZ).contains(hz)).map(RateChoice::Fixed)
    }

    /// [`RateChoice::parse`], only for the page's choices ([`RateChoice::LISTED`]).
    pub fn parse_listed(t: &str) -> Option<RateChoice> {
        RateChoice::parse(t).filter(|c| RateChoice::LISTED.contains(c))
    }

    /// The rate it asks for, steps a second: Match display follows `display` (the frame
    /// rate [`display_hz`] found; [`MIN_HZ`] while that is not known).
    pub fn hz(self, display: Option<u32>) -> u32 {
        match self {
            RateChoice::Fixed(hz) => hz.clamp(MIN_HZ, MAX_HZ),
            RateChoice::Display => display.unwrap_or(MIN_HZ).clamp(MIN_HZ, MAX_HZ),
        }
    }
}

/// `--physics-hz`'s parser (clap).
pub fn parse_arg(t: &str) -> Result<RateChoice, String> {
    RateChoice::parse(t).ok_or_else(|| format!("{t:?}: a whole number of steps a second from {MIN_HZ} to {MAX_HZ}, or display"))
}

/// The frame rate Match display follows, before it is held to 60-240: the frame-rate cap
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
/// rate Match display follows ([`display_hz`]).
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
/// rate / the rate now) stays under this, so a rise from 60 straight to 240-worth of cost
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

/// The settings page's row and the systems.
pub(crate) fn setup(app: &mut App) {
    app.init_resource::<PhysRate>()
        .init_resource::<DisplayPace>()
        .add_systems(Update, pace.after(crate::PlaySet::Input).before(crate::PlaySet::Physics))
        .add_systems(Last, guard)
        .add_systems(Last, report);
    crate::settings::add_choice_list(
        app,
        crate::settings::Setting {
            key: KEY,
            group: "Display",
            label: "Physics rate",
            note: "How many times a second the physics moves the world on: the player, the people, what falls and what is pushed. A higher rate answers the keys sooner and meets collisions more finely; its cost to the processor grows with the rate. Match display: the frame rate the display sync and the frame rate limit hold, from 60 to 240. When the physics takes over half of each frame, the rate goes down a step (never under 60) until there is room again.",
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
    let choice = RateChoice::LISTED.into_iter().find(|c| c.name().eq_ignore_ascii_case(name.trim())).or_else(|| RateChoice::parse_listed(name));
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
        RateChoice::Display => match display {
            Some(d) => format!("{by}, Match display: the display's {d} fps"),
            None => format!("{by}, Match display: the display's rate not known yet"),
        },
    }
}

/// The rate in effect and the step for the game's clock, before the physics steps
/// (module notes): the choice, the guard's limit, the clock's speed.
fn pace(virt: Res<Time<Virtual>>, forced: Option<Res<ForcedRate>>, screen: Res<DisplayPace>, mut rate: ResMut<PhysRate>, mut phys: ResMut<PhysState>, mut told: Local<Option<f32>>) {
    let forced = forced.map(|f| f.0);
    let choice = forced.unwrap_or(rate.setting);
    let wanted = choice.hz(screen.0);
    if wanted != rate.wanted {
        let was = rate.wanted;
        rate.wanted = wanted;
        rate.guard.reset();
        if was == 0 {
            info!("phys: physics rate {wanted} Hz ({})", source(forced, choice, screen.0));
        } else {
            info!("phys: physics rate {was} Hz -> {wanted} Hz ({})", source(forced, choice, screen.0));
        }
    }
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
    // Once a change of pace, not every frame of an easing clock.
    let paced = dt != full;
    if told.is_some() != paced {
        *told = paced.then_some(dt);
        if paced {
            info!("phys: the clock runs at {speed:.2}: the step follows it, {:.2} ms ({:.0} steps a real second)", dt * 1000.0, speed / dt);
        } else {
            info!("phys: the clock is back at full speed: the step is {:.2} ms again", dt * 1000.0);
        }
    }
}

/// `NVF_PHYS_RATE_GUARD=0`: the cost guard never switches the rate, so a measurement runs at
/// the rate asked for however much it costs. For measuring; not a test aid: it does not
/// make the run unattended.
pub const GUARD_VAR: &str = "NVF_PHYS_RATE_GUARD";

/// The guard's frame: this frame's physics time against its real time (module notes).
fn guard(real: Res<Time<Real>>, mut rate: ResMut<PhysRate>, mut off: Local<Option<bool>>) {
    let Some(physics) = rate.frame_secs.take() else { return };
    let (hz, wanted) = (rate.hz, rate.wanted);
    if hz == 0 {
        return;
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
    info!("physics rate: {} Hz at exit, {} Hz asked for; {} switches by the guard", rate.hz, rate.wanted, rate.switches);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_parse_from_the_command_line_and_the_ini() {
        assert_eq!(RateChoice::parse("152"), Some(RateChoice::Fixed(152)));
        assert_eq!(RateChoice::parse(" 120 Hz "), Some(RateChoice::Fixed(120)));
        assert_eq!(RateChoice::parse("DISPLAY"), Some(RateChoice::Display));
        assert_eq!(RateChoice::parse("Match display"), Some(RateChoice::Display));
        for bad in ["59", "241", "0", "-60", "60.5", "fast", ""] {
            assert_eq!(RateChoice::parse(bad), None, "{bad}");
        }
        assert!(parse_arg("30").is_err());
        assert_eq!(parse_arg("display"), Ok(RateChoice::Display));
        // The page and the INI keep to the listed choices.
        assert_eq!(RateChoice::parse_listed("152"), None);
        for c in RateChoice::LISTED {
            assert_eq!(RateChoice::parse_listed(&c.stored()), Some(c));
            assert_eq!(RateChoice::parse_listed(&c.name()), Some(c));
        }
        assert_eq!(RateChoice::default(), RateChoice::Fixed(60));
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
        // Held to 60-240.
        assert_eq!(RateChoice::Display.hz(Some(30)), 60);
        assert_eq!(RateChoice::Display.hz(Some(360)), 240);
        assert_eq!(RateChoice::Display.hz(None), 60);
        assert_eq!(RateChoice::Fixed(152).hz(None), 152);
    }

    #[test]
    fn the_catch_up_covers_a_quarter_second_at_any_rate() {
        assert_eq!(catch_up_steps(60), 15);
        assert_eq!(catch_up_steps(152), 38);
        assert_eq!(catch_up_steps(240), 60);
        // As play_params has at 60.
        assert_eq!(catch_up_steps(60), crate::phys::play_params().max_substeps);
    }

    #[test]
    fn notches_step_through_the_listed_rates() {
        assert_eq!(notch_below(240), Some(160));
        assert_eq!(notch_below(152), Some(144));
        assert_eq!(notch_below(120), Some(60));
        assert_eq!(notch_below(60), None);
        assert_eq!(notch_above(60, 240), Some(120));
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
        let mut hz = 240;
        // Light load: nothing moves.
        assert!(run(&mut g, &mut hz, 240, 0.3, 10.0).is_empty());
        // Over half the frame: a notch every 2 s, down to 60 and no further.
        let moves = run(&mut g, &mut hz, 240, 0.7, 30.0);
        let tos: Vec<u32> = moves.iter().map(|m| if let GuardMove::Down { to, .. } = m { *to } else { 0 }).collect();
        assert_eq!(tos, [160, 144, 120, 60]);
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
        let mut hz = 240;
        run(&mut g, &mut hz, 240, 0.9, 2.5);
        assert_eq!(g.limit(240), 160);
        g.reset();
        assert_eq!(g.limit(120), 120);
        assert_eq!(g.limit(240), 240);
        assert_eq!(g.frame(60, 60, f64::NAN, 0.01), None);
        assert_eq!(g.frame(60, 60, 0.001, 0.0), None);
    }
}
