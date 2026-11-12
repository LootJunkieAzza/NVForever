//! The device screen's page change (docs/spec-notes/pipboy.md §3, "Page changes"): when the
//! page changes on the arm, the picture washes out to a dim, nearly flat glow and comes back
//! over about a quarter of a second, and now and then it also shudders or rolls once through
//! the vertical hold; a tab change brightens it a little. All the numbers are in this file,
//! each from the private jobs pipboy_fx_switch_a, _b and _forced [observed: 1280 x 720, 216
//! page changes, 30 tab changes, 36 with the chances forced; see pipboy.md §3].
//!
//! What it does is a [`ScreenLook`], which nvforever's `pipboy_arm.rs` puts on the screen's
//! material each frame it changes (nvf_assets `NifSurface::pipboy_a`, `pipboy_b`;
//! `nif_material.wesl` `pipboy_picture`), so it applies to the whole picture, whichever
//! painter drew the page, and the page itself is redrawn at once as before. The flat image
//! shown where the model is missing, and the screen menu, have none of it [design].
//!
//! Held under the pause menu, as the beam is: the time it has run stops and the picture
//! stays as it is; it goes on when the menu closes [design]. Logged once a change:
//! `pipboy: page change transition <kind> (<ms> ms)`, `pipboy: tab change transition
//! brighten (<ms> ms)`.

use super::device_screen::{SCREEN_NOISE, TEXT_SOFTEN};
use super::{Page, PipBoy, TARGET};
use super::CANVAS;
use bevy::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// The chance that a page change rolls the picture once through the vertical hold
/// [documented: `Fallout_default.ini` `[InterfaceFX]` `fVertHoldChance=0.08`, under
/// ";pipboy menu transitions"]. That this is what it sets, and that it is rolled once a
/// change, are [hypothesis]: with it set to 1 every change rolled [observed: private job
/// pipboy_fx_forced, 12 of 12], and at the defaults 13 of 180 changes did (7 %).
pub const VERTICAL_HOLD_CHANCE: f32 = 0.08;
/// The chance that it shudders [documented: the same, `fShudderChance=0.20`]; set to 1
/// every change shuddered [observed: 12 of 12], at the defaults 35 of 180 (19 %). Rolled
/// when the hold is not [hypothesis].
pub const SHUDDER_CHANCE: f32 = 0.20;

/// Canvas units to a row of the original's 1280 x 720 shots: its screen, about 416 rows
/// there, shows about 731 of the canvas's 768 [hypothesis: the shots' screen rectangle
/// against pipboy.md §3 "Mapping"].
pub const CANVAS_PER_ROW: f32 = 1.8;

/// What a page change does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The wash alone.
    Plain,
    /// The wash, and the picture jumps up and down a few times, less each time.
    Shudder,
    /// The wash, and the picture rolls up once through its whole height and wraps.
    VerticalHold,
    /// A tab change: a small brighten, fading.
    Tab,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Plain => "plain",
            Kind::Shudder => "shudder",
            Kind::VerticalHold => "vertical hold",
            Kind::Tab => "brighten",
        }
    }

    /// How long it lasts, in milliseconds: the picture is settled again by then [observed:
    /// medians of the bursts' settling, 223 ms plain, about 245 ms shuddering, the roll's last
    /// step at 250 to 290 ms, a tab's brighten gone by about 200 ms].
    pub fn millis(self) -> f32 {
        match self {
            Kind::Plain => 225.0,
            Kind::Shudder => 245.0,
            Kind::VerticalHold => 270.0,
            Kind::Tab => 200.0,
        }
    }
}

/// The kind of a page change for two rolls in 0..1: the hold first, then the shudder.
pub fn kind_for(hold_roll: f32, shudder_roll: f32) -> Kind {
    if hold_roll < VERTICAL_HOLD_CHANCE {
        Kind::VerticalHold
    } else if shudder_roll < SHUDDER_CHANCE {
        Kind::Shudder
    } else {
        Kind::Plain
    }
}

/// The wash of a plain page change: (ms, gain on the display values, display levels added
/// of 255), between them in a straight line. The medians over 20 ms of the plain changes,
/// the gain and the level fitted per band of rows against the new page [observed: private
/// jobs pipboy_fx_switch_a and _b, the 120 changes without a roll or a shudder]: the
/// picture keeps about an eighth of its detail on a lift of about 16 levels at once, about a
/// fifth on 10 from 70 to 130 ms, comes back from there, and is whole again at 225 ms. The longer kinds stretch it to their
/// length [hypothesis: their washes run about that much longer].
const PAGE_WASH: [(f32, f32, f32); 10] = [
    (0.0, 0.15, 16.0),
    (40.0, 0.12, 16.0),
    (70.0, 0.13, 11.5),
    (100.0, 0.19, 9.5),
    (130.0, 0.27, 9.7),
    (150.0, 0.34, 10.6),
    (170.0, 0.44, 11.7),
    (190.0, 0.58, 10.1),
    (210.0, 0.74, 7.4),
    (225.0, 1.0, 0.0),
];

/// A tab change's brighten at its start: gain and levels added, fading in a straight line to
/// nothing over [`Kind::Tab`]'s length [observed: private job pipboy_fx_switch_a, 30 tab
/// changes: about 1.05 and +3.5 at once, +2 at 100 ms, gone by 200].
const TAB_BRIGHTEN: (f32, f32) = (1.055, 3.8);

/// The shudder: (ms, rows of the 1280 x 720 shots the picture stands down, up negative),
/// between them in a straight line [observed: private job pipboy_fx_forced, the 12 changes
/// with fShudderChance 1, the same steps every time within a row or two; down taken as the
/// analyser's positive].
const SHUDDER: [(f32, f32); 11] = [
    (0.0, 0.0),
    (30.0, 0.0),
    (50.0, -14.0),
    (95.0, 15.0),
    (120.0, 10.0),
    (142.0, -7.0),
    (163.0, -9.0),
    (185.0, -5.0),
    (207.0, 3.0),
    (230.0, 3.0),
    (245.0, 0.0),
];

/// The vertical hold: the picture rolls up through its whole height from [`HOLD_START`] to
/// [`HOLD_END`] ms and stands where it began [observed: private job pipboy_fx_forced, the 12
/// changes with fVertHoldChance 1: the picture moved up about 43 rows a game loop and
/// wrapped, and came to rest from below at 250 to 290 ms; a straight roll is
/// [hypothesis]].
const HOLD_START: f32 = 40.0;
const HOLD_END: f32 = 255.0;

/// Between the keys of a table, in a straight line; held at its ends.
fn between<const N: usize>(keys: &[(f32, f32); N], ms: f32) -> f32 {
    let (first, last) = (keys[0], keys[N - 1]);
    if ms <= first.0 {
        return first.1;
    }
    for w in keys.windows(2) {
        let ((t0, v0), (t1, v1)) = (w[0], w[1]);
        if ms <= t1 {
            return v0 + (v1 - v0) * (ms - t0) / (t1 - t0).max(1e-6);
        }
    }
    last.1
}

/// The wash of a page change of `kind` at `ms` into it: (gain, display levels added).
pub fn page_wash(kind: Kind, ms: f32) -> (f32, f32) {
    let ms = ms * Kind::Plain.millis() / kind.millis();
    let gain = between(&PAGE_WASH.map(|(t, g, _)| (t, g)), ms);
    let add = between(&PAGE_WASH.map(|(t, _, a)| (t, a)), ms);
    (gain, add)
}

/// A tab change's brighten at `ms`: (gain, display levels added).
pub fn tab_brighten(ms: f32) -> (f32, f32) {
    let left = (1.0 - ms / Kind::Tab.millis()).clamp(0.0, 1.0);
    (1.0 + (TAB_BRIGHTEN.0 - 1.0) * left, TAB_BRIGHTEN.1 * left)
}

/// How far the picture stands down at `ms` into a shudder, in canvas units.
pub fn shudder_down(ms: f32) -> f32 {
    between(&SHUDDER, ms) * CANVAS_PER_ROW
}

/// How far the picture has rolled up at `ms` into a vertical hold, as a share of the
/// canvas's height: 0 to 1, then back where it began.
pub fn hold_roll(ms: f32) -> f32 {
    if ms <= HOLD_START || ms >= HOLD_END {
        return 0.0;
    }
    (ms - HOLD_START) / (HOLD_END - HOLD_START)
}

/// What the screen's material does to the picture now (module docs): the roll and the wash
/// of a change, and the static softening and noise of the screen treatment
/// (`device_screen.rs`).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct ScreenLook {
    /// How far the picture is rolled up, as a share of the canvas's height (it wraps).
    pub roll: f32,
    /// The gain on its display values.
    pub gain: f32,
    /// Display value added (0–1).
    pub add: f32,
    /// [`TEXT_SOFTEN`].
    pub soften: f32,
    /// [`SCREEN_NOISE`].
    pub noise: f32,
}

impl Default for ScreenLook {
    fn default() -> Self {
        ScreenLook { roll: 0.0, gain: 1.0, add: 0.0, soften: TEXT_SOFTEN, noise: SCREEN_NOISE }
    }
}

impl ScreenLook {
    /// The look `ms` into a change of `kind`.
    pub fn during(kind: Kind, ms: f32) -> ScreenLook {
        let (gain, add) = match kind {
            Kind::Tab => tab_brighten(ms),
            _ => page_wash(kind, ms),
        };
        let roll = match kind {
            Kind::Shudder => -shudder_down(ms) / CANVAS.y,
            Kind::VerticalHold => hold_roll(ms),
            _ => 0.0,
        };
        ScreenLook { roll, gain, add: add / 255.0, ..default() }
    }

    /// As the material takes it (nvf_assets `NifSurface::pipboy_a`, `pipboy_b`).
    pub fn surface(&self) -> (Vec4, Vec4) {
        (Vec4::new(self.roll, 1.0 - self.gain, self.add, CANVAS.y / TARGET.y as f32), Vec4::new(self.soften, self.noise, CANVAS.x / TARGET.x as f32, 0.0))
    }
}

/// A change under way.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Running {
    kind: Kind,
    /// Milliseconds it has run, the pause menu's time left out.
    ms: f32,
}

/// The page changes' state.
#[derive(Resource)]
pub struct PageChange {
    /// The page and tab last seen up on the arm; `None` while it is not.
    last: Option<(Page, usize)>,
    running: Option<Running>,
    rng: ChaCha8Rng,
    /// Every page change of this kind, as the forced job set the chances (test aid
    /// `NVF_PLAY_TEST_PIPBOY` `glitch=`); `None` rolls the chances.
    pub forced: Option<Kind>,
}

impl Default for PageChange {
    fn default() -> Self {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64);
        PageChange::seeded(seed)
    }
}

impl PageChange {
    pub fn seeded(seed: u64) -> PageChange {
        PageChange { last: None, running: None, rng: ChaCha8Rng::seed_from_u64(seed), forced: None }
    }

    /// A roll in 0..1.
    fn roll(&mut self) -> f32 {
        (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// The kind of the next page change.
    fn page_kind(&mut self) -> Kind {
        let (hold, shudder) = (self.roll(), self.roll());
        self.forced.unwrap_or_else(|| kind_for(hold, shudder))
    }

    /// One frame: `now` the page and tab up on the arm (`None` while it is not), `dt_ms` the
    /// real time since the last, `paused` the pause menu open. Returns the change started
    /// this frame, if any, and the look.
    fn step(&mut self, now: Option<(Page, usize)>, dt_ms: f32, paused: bool) -> (Option<Kind>, ScreenLook) {
        let Some(now) = now else {
            self.last = None;
            self.running = None;
            return (None, ScreenLook::default());
        };
        let mut started = None;
        if !paused {
            match self.last {
                Some((page, _)) if page != now.0 => started = Some(self.page_kind()),
                Some((_, tab)) if tab != now.1 => started = Some(Kind::Tab),
                _ => {}
            }
            self.last = Some(now);
            match (started, self.running.as_mut()) {
                (Some(kind), _) => self.running = Some(Running { kind, ms: 0.0 }),
                (None, Some(r)) => r.ms += dt_ms,
                (None, None) => {}
            }
            if self.running.is_some_and(|r| r.ms >= r.kind.millis()) {
                self.running = None;
            }
        }
        let look = self.running.map_or_else(ScreenLook::default, |r| ScreenLook::during(r.kind, r.ms));
        (started, look)
    }
}

/// Each frame: start a change when the page or the tab on the arm changed, run it, and put
/// the look in [`ScreenLook`] (module docs).
pub fn run(time: Res<Time<Real>>, paused: Res<crate::pause::Paused>, pip: Res<PipBoy>, mut change: ResMut<PageChange>, mut look: ResMut<ScreenLook>) {
    let up = pip.open() && pip.on_arm();
    let now = up.then(|| (pip.page, pip.tab()));
    let (started, next) = change.step(now, time.delta_secs() * 1000.0, paused.0);
    if let Some(kind) = started {
        let ms = kind.millis();
        match kind {
            Kind::Tab => info!("pipboy: tab change transition {} ({ms} ms)", kind.name()),
            _ => info!("pipboy: page change transition {} ({ms} ms)", kind.name()),
        }
    }
    if *look != next {
        *look = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chances_pick_the_hold_first_then_the_shudder() {
        assert_eq!(kind_for(0.0, 0.0), Kind::VerticalHold);
        assert_eq!(kind_for(0.079, 0.9), Kind::VerticalHold);
        assert_eq!(kind_for(0.08, 0.19), Kind::Shudder);
        assert_eq!(kind_for(0.5, 0.2), Kind::Plain);
        // Over many seeded rolls, about the INI's shares (the shudder's only when the hold
        // is not rolled: 0.92 × 0.2).
        let mut c = PageChange::seeded(1234);
        let n = 20_000;
        let mut counts = [0usize; 3];
        for _ in 0..n {
            match c.page_kind() {
                Kind::VerticalHold => counts[0] += 1,
                Kind::Shudder => counts[1] += 1,
                Kind::Plain => counts[2] += 1,
                Kind::Tab => unreachable!(),
            }
        }
        let share = |k: usize| counts[k] as f32 / n as f32;
        assert!((share(0) - 0.08).abs() < 0.01, "{counts:?}");
        assert!((share(1) - 0.184).abs() < 0.015, "{counts:?}");
        // The same seed, the same changes.
        let (mut a, mut b) = (PageChange::seeded(9), PageChange::seeded(9));
        let ka: Vec<Kind> = (0..50).map(|_| a.page_kind()).collect();
        let kb: Vec<Kind> = (0..50).map(|_| b.page_kind()).collect();
        assert_eq!(ka, kb);
        // Forced: every one.
        a.forced = Some(Kind::Shudder);
        assert!((0..50).all(|_| a.page_kind() == Kind::Shudder));
    }

    #[test]
    fn the_wash_drops_the_detail_at_once_and_gives_it_back_by_its_end() {
        let (g0, a0) = page_wash(Kind::Plain, 0.0);
        assert!(g0 < 0.2 && a0 > 12.0, "{g0} {a0}");
        let (g, a) = page_wash(Kind::Plain, 100.0);
        assert!((g - 0.19).abs() < 1e-4 && (a - 9.5).abs() < 1e-3);
        // Half way between keys, a straight line.
        let (g, _) = page_wash(Kind::Plain, 85.0);
        assert!((g - 0.16).abs() < 1e-3, "{g}");
        assert_eq!(page_wash(Kind::Plain, 225.0), (1.0, 0.0));
        assert_eq!(page_wash(Kind::Plain, 400.0), (1.0, 0.0));
        // The gain only rises after 40 ms.
        let gains: Vec<f32> = (40..=225).step_by(5).map(|t| page_wash(Kind::Plain, t as f32).0).collect();
        assert!(gains.windows(2).all(|w| w[1] >= w[0] - 1e-6), "{gains:?}");
        // A longer kind is the same wash stretched.
        let hold = Kind::VerticalHold.millis();
        assert_eq!(page_wash(Kind::VerticalHold, hold * 100.0 / 225.0), page_wash(Kind::Plain, 100.0));
        assert_eq!(page_wash(Kind::VerticalHold, hold), (1.0, 0.0));
    }

    #[test]
    fn a_tab_brightens_a_little_and_fades() {
        let (g, a) = tab_brighten(0.0);
        assert!(g > 1.0 && g < 1.1 && a > 2.0 && a < 5.0, "{g} {a}");
        let (g, a) = tab_brighten(100.0);
        assert!((g - (1.0 + (TAB_BRIGHTEN.0 - 1.0) / 2.0)).abs() < 1e-5 && (a - TAB_BRIGHTEN.1 / 2.0).abs() < 1e-5);
        assert_eq!(tab_brighten(200.0), (1.0, 0.0));
    }

    #[test]
    fn the_shudder_jumps_less_each_time_and_the_hold_rolls_once() {
        assert_eq!(shudder_down(0.0), 0.0);
        assert!((shudder_down(95.0) - 15.0 * CANVAS_PER_ROW).abs() < 1e-4);
        assert!(shudder_down(50.0) < 0.0, "up first");
        assert_eq!(shudder_down(245.0), 0.0);
        assert_eq!(shudder_down(300.0), 0.0);
        // Its swings shrink after the largest.
        let peaks = [95.0, 163.0, 207.0].map(|t| shudder_down(t).abs());
        assert!(peaks[0] > peaks[1] && peaks[1] > peaks[2], "{peaks:?}");
        // The hold: still, then up through the whole height in a straight line, then back.
        assert_eq!(hold_roll(0.0), 0.0);
        assert_eq!(hold_roll(HOLD_START), 0.0);
        let mid = hold_roll((HOLD_START + HOLD_END) / 2.0);
        assert!((mid - 0.5).abs() < 1e-5, "{mid}");
        assert!(hold_roll(HOLD_END - 1.0) > 0.99);
        assert_eq!(hold_roll(HOLD_END), 0.0);
        assert!(HOLD_END < Kind::VerticalHold.millis());
        // About 43 rows of the shots a game loop of about 22 ms [observed]: the canvas's
        // height in about 210 ms.
        let rows_per_ms = CANVAS.y / CANVAS_PER_ROW / (HOLD_END - HOLD_START);
        assert!((rows_per_ms * 22.0 - 43.0).abs() < 4.0, "{rows_per_ms}");
    }

    #[test]
    fn the_look_at_rest_is_the_static_treatment_and_the_material_takes_it() {
        let rest = ScreenLook::default();
        assert_eq!((rest.roll, rest.gain, rest.add), (0.0, 1.0, 0.0));
        let (a, b) = rest.surface();
        assert_eq!(a, Vec4::new(0.0, 0.0, 0.0, 0.8));
        assert_eq!(b, Vec4::new(TEXT_SOFTEN, SCREEN_NOISE, 0.8, 0.0), "the canvas's width share for the taps' bounds");
        // The shudder moves the picture down by rolling it back (negative roll); the
        // hold rolls it up.
        assert!(ScreenLook::during(Kind::Shudder, 95.0).roll < 0.0);
        assert!(ScreenLook::during(Kind::VerticalHold, 150.0).roll > 0.0);
        assert_eq!(ScreenLook::during(Kind::Plain, 150.0).roll, 0.0);
        let l = ScreenLook::during(Kind::Plain, 0.0);
        assert!((l.add - 16.0 / 255.0).abs() < 1e-6 && (l.surface().0.y - 0.85).abs() < 1e-5);
        assert_eq!(ScreenLook::during(Kind::Tab, 0.0).roll, 0.0);
    }

    #[test]
    fn a_change_starts_on_a_new_page_or_tab_runs_its_length_and_holds_under_the_pause() {
        let mut c = PageChange::seeded(5);
        c.forced = Some(Kind::Plain);
        // Coming up: no change, only what it shows.
        assert_eq!(c.step(Some((Page::Items, 0)), 16.0, false), (None, ScreenLook::default()));
        assert_eq!(c.step(Some((Page::Items, 0)), 16.0, false).0, None);
        // A new page.
        let (k, look) = c.step(Some((Page::Stats, 0)), 16.0, false);
        assert_eq!(k, Some(Kind::Plain));
        assert_eq!(look, ScreenLook::during(Kind::Plain, 0.0));
        // Runs on the real time; held under the pause, the time it has run kept.
        let (_, look) = c.step(Some((Page::Stats, 0)), 100.0, false);
        assert_eq!(look, ScreenLook::during(Kind::Plain, 100.0));
        let (k, held) = c.step(Some((Page::Stats, 0)), 500.0, true);
        assert_eq!((k, held), (None, look));
        let (_, look) = c.step(Some((Page::Stats, 0)), 50.0, false);
        assert_eq!(look, ScreenLook::during(Kind::Plain, 150.0));
        // Over at its length.
        let (_, look) = c.step(Some((Page::Stats, 0)), 100.0, false);
        assert_eq!(look, ScreenLook::default());
        // A tab: the brighten.
        assert_eq!(c.step(Some((Page::Stats, 2)), 16.0, false).0, Some(Kind::Tab));
        // A page change during it starts the page's.
        assert_eq!(c.step(Some((Page::Data, 0)), 16.0, false).0, Some(Kind::Plain));
        // Down: nothing, and coming up again on another page is no change.
        assert_eq!(c.step(None, 16.0, false), (None, ScreenLook::default()));
        assert_eq!(c.step(Some((Page::Items, 1)), 16.0, false), (None, ScreenLook::default()));
    }
}
