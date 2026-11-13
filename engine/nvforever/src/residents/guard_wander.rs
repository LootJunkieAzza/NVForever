//! Opt-in smoke aid for a Guard package's wander (#2027):
//! `NVF_PLAY_TEST_GUARD_WANDER=<actor>[:<seconds>]` (an Editor ID or a hex FormID).
//!
//! Once the actor runs a Guard package with a wander (`fnv_ai::Sandbox::guard`), its feet
//! are measured after every AI update against the wander's centre and radius
//! (`Sandbox::area`), for `<seconds>` of `Game::time` (default [`WINDOW_SECONDS`]). Every
//! [`REPORT_SECONDS`] and at the end of the window it logs one line:
//!
//! `guard wander check: actor=<id> t=<s> samples=<n> moves=<n> walked=<units>
//! max_distance=<units> radius=<units> violations=<n> package_type=<code>`
//!
//! `moves` counts the walks begun (the AI's activity turning to Travelling), `walked` the
//! horizontal distance between samples, `max_distance` the farthest sample from the
//! centre and `violations` the samples farther than the radius plus [`SLACK`]. The window
//! ends with `guard wander check: done …` (the same fields). `package_type` is the
//! current package's GetCurrentAIPackage code (35 for a Guard, `PackageType::runtime_code`).
//! It only reads the AI and game state; it never moves the actor or changes its package.

use super::Core;
use bevy::prelude::*;
use fnv_ai::Activity;
use fnv_game::game::Game;
use fnv_game::package::PackageType;
use fnv_sim::{FormId, RefTable};

const ENV: &str = "NVF_PLAY_TEST_GUARD_WANDER";
/// The window measured, seconds of `Game::time` from the wander's start [design, #2027:
/// about the issue's 360-second capture].
const WINDOW_SECONDS: f64 = 300.0;
/// Seconds between report lines [design].
const REPORT_SECONDS: f64 = 30.0;
/// Units past the radius a sample may lie before it counts as a violation: the walk's
/// arrival tolerance and a collision capsule pushed by physics [design, #2027].
const SLACK: f32 = 32.0;

pub(super) struct Probe {
    actor: FormId,
    window: f64,
    began: Option<f64>,
    last_report: f64,
    last: Option<[f32; 3]>,
    travelling: bool,
    samples: u64,
    moves: u64,
    walked: f32,
    max_distance: f32,
    radius: f32,
    violations: u64,
    done: bool,
}

impl Probe {
    pub(super) fn from_env(refs: &RefTable) -> Option<Self> {
        let value = std::env::var(ENV).ok()?;
        let parsed = Self::parse(&value, refs);
        if parsed.is_none() {
            warn!("guard wander check: invalid {ENV}; expected actor[:seconds]");
        }
        parsed
    }

    fn parse(value: &str, refs: &RefTable) -> Option<Self> {
        let mut fields = value.split(':').map(str::trim);
        let name = fields.next().filter(|n| !n.is_empty())?;
        let actor = refs.by_edid(name).or_else(|| u32::from_str_radix(name.trim_start_matches("0x").trim_start_matches("0X"), 16).ok().map(FormId))?;
        let window = match fields.next() {
            Some(s) => s.parse::<f64>().ok().filter(|s| *s > 0.0)?,
            None => WINDOW_SECONDS,
        };
        if fields.next().is_some() {
            return None;
        }
        Some(Self { actor, window, began: None, last_report: 0.0, last: None, travelling: false, samples: 0, moves: 0, walked: 0.0, max_distance: 0.0, radius: 0.0, violations: 0, done: false })
    }

    fn line(&self, game: &Game, done: bool) {
        let package = game.extra.get(&self.actor).and_then(|e| e.current_package).and_then(|p| game.data.package_defs.get(&p)).and_then(|d| d.ty.runtime_code()).map_or(-1, i32::from);
        info!(
            "guard wander check: {}actor={} t={:.0} samples={} moves={} walked={:.0} max_distance={:.1} radius={:.0} violations={} package_type={package}",
            if done { "done " } else { "" },
            self.actor,
            self.began.map_or(0.0, |b| game.time - b),
            self.samples,
            self.moves,
            self.walked,
            self.max_distance,
            self.radius,
            self.violations,
        );
    }

    /// After the AI's update: one sample while the actor runs a Guard package's wander.
    pub(super) fn tick(&mut self, core: &Core, game: &Game) {
        if self.done {
            return;
        }
        let guarding = game.extra.get(&self.actor).and_then(|e| e.current_package).and_then(|p| game.data.package_defs.get(&p)).is_some_and(|d| d.ty == PackageType::Guard);
        let Some(a) = core.ai.agents.get(&self.actor) else { return };
        let Some(sandbox) = a.sandbox.as_ref().filter(|_| guarding) else {
            // Not yet (on its way, or another package); after the start, a gap in the
            // samples, not their end.
            self.last = None;
            return;
        };
        let (centre, radius) = sandbox.area();
        let began = *self.began.get_or_insert_with(|| {
            info!("guard wander check: armed actor={} centre=({:.0}, {:.0}, {:.0}) radius={radius:.0}", self.actor, centre.pos[0], centre.pos[1], centre.pos[2]);
            game.time
        });
        self.radius = radius;
        let feet = a.walker.feet;
        let d = (feet[0] - centre.pos[0]).hypot(feet[1] - centre.pos[1]);
        self.samples += 1;
        self.max_distance = self.max_distance.max(d);
        if d > radius + SLACK {
            self.violations += 1;
        }
        if let Some(last) = self.last {
            self.walked += (feet[0] - last[0]).hypot(feet[1] - last[1]);
        }
        self.last = Some(feet);
        let travelling = a.activity == Activity::Travelling;
        if travelling && !self.travelling {
            self.moves += 1;
        }
        self.travelling = travelling;
        if game.time - began >= self.window {
            self.line(game, true);
            self.done = true;
        } else if game.time - self.last_report >= REPORT_SECONDS {
            self.last_report = game.time;
            self.line(game, false);
        }
    }
}
