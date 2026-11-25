//! Where a start's first seconds go (#1732, #3127): what `main()` takes before Bevy's
//! log plugin is up ([`PreWindowTimes`]), what the main world's Startup schedules and its
//! first frame take, and when the first render frame ends. The log's times give the rest.
//!
//! Log, every run: `start: before the window: <s> s (plugins <s> s, …)`,
//! `start: Startup <s> s; frame 1 <s> s: <s> s before Update, <s> s in
//! Update, <s> s after it` and `start: first render frame finished <s> s after frame 1
//! ended` (with a pipeline gate: the runs that have one).
//!
//! **What this cannot say** is which system inside took the time: systems run side by side,
//! and Bevy's per-system spans need its `trace` feature (`--features profile`,
//! `NVF_PLAY_TEST_PROFILE=1`, profile.rs). One log has about 1 s in frame 1 with no line at
//! all, between `grab:` and `imposters:` [observed: Goodsprings start, #1716's oscap runs].
//!
//! Marks are small schedules around the main ones, as handoff.rs's `EndNow` is. After the
//! report each is one early return a frame.

use crate::pipeline_gate::PipelineGate;
use bevy::ecs::schedule::{ScheduleLabel, SingleThreadedExecutor};
use bevy::prelude::*;
use std::time::Instant;

#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Mark {
    StartupBegins,
    StartupEnds,
    UpdateBegins,
    UpdateEnds,
    FrameEnds,
}

/// Durations (seconds) of the startup phases in `main()` between `crash::install` and
/// Bevy's `LogPlugin` (#3127) [design, #3127]. Logged once the log plugin is up (and
/// printed to stderr before it) and attached to the start's `slow_load` report (#2407).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct PreWindowTimes {
    /// Total elapsed seconds from `crash::install` until `LogPlugin` is added.
    pub total: f64,
    /// Opening and validating plugins (`world::open_plugins`).
    pub plugins: f64,
    /// Reading and remapping a `--load` save (`start_from_save`); `None` without `--load`.
    pub save: Option<f64>,
    /// Mounting archives and loading the NIF schema (`GameData::open_for_plugins_*`).
    pub archives: f64,
    /// Building the ESM load order, world index, BOS and regions (`WorldData::open`).
    pub index: f64,
    /// Preparing the generated LOD job and LOD grid origin (`lodgen_job`, `lod::grid_origin`).
    pub lod: f64,
    /// Building actor records and character-creator data (`ActorWorld::build`, `Creator::build`).
    pub actors: Option<f64>,
    /// Building the residents table (`Residents::build`).
    pub residents: Option<f64>,
    /// Building furniture data (`Furnishings::build`).
    pub furniture: Option<f64>,
    /// Building the navmesh graph (`NavState::build`).
    pub nav: Option<f64>,
}

impl PreWindowTimes {
    /// Sum of the individually timed phases, seconds.
    pub fn accounted(&self) -> f64 {
        self.plugins
            + self.save.unwrap_or(0.0)
            + self.archives
            + self.index
            + self.lod
            + self.actors.unwrap_or(0.0)
            + self.residents.unwrap_or(0.0)
            + self.furniture.unwrap_or(0.0)
            + self.nav.unwrap_or(0.0)
    }

    /// Time between `crash::install` and `LogPlugin` outside the named phases, seconds.
    pub fn other(&self) -> f64 {
        (self.total - self.accounted()).max(0.0)
    }

    /// Total pre-window time in milliseconds.
    pub fn total_ms(&self) -> f64 {
        self.total * 1000.0
    }

    /// One-line summary for stderr and the run log.
    pub fn line(&self) -> String {
        let mut parts = Vec::with_capacity(10);
        parts.push(format!("plugins {:.2} s", self.plugins));
        if let Some(s) = self.save {
            parts.push(format!("save {s:.2} s"));
        }
        parts.push(format!("archives {:.2} s", self.archives));
        parts.push(format!("index {:.2} s", self.index));
        parts.push(format!("lod {:.2} s", self.lod));
        if let Some(s) = self.actors {
            parts.push(format!("actors {s:.2} s"));
        }
        if let Some(s) = self.residents {
            parts.push(format!("residents {s:.2} s"));
        }
        if let Some(s) = self.furniture {
            parts.push(format!("furniture {s:.2} s"));
        }
        if let Some(s) = self.nav {
            parts.push(format!("nav {s:.2} s"));
        }
        parts.push(format!("other {:.2} s", self.other()));
        format!("start: before the window: {:.2} s ({})", self.total, parts.join(", "))
    }

    /// Structured milliseconds breakdown for `slow_load` reports (#2407, #3127).
    pub fn to_json(&self) -> serde_json::Value {
        let ms = |s: f64| serde_json::json!((s * 1000.0).round());
        let mut map = serde_json::Map::new();
        map.insert("total_ms".into(), ms(self.total));
        map.insert("plugins_ms".into(), ms(self.plugins));
        if let Some(s) = self.save {
            map.insert("save_ms".into(), ms(s));
        }
        map.insert("archives_ms".into(), ms(self.archives));
        map.insert("index_ms".into(), ms(self.index));
        map.insert("lod_ms".into(), ms(self.lod));
        if let Some(s) = self.actors {
            map.insert("actors_ms".into(), ms(s));
        }
        if let Some(s) = self.residents {
            map.insert("residents_ms".into(), ms(s));
        }
        if let Some(s) = self.furniture {
            map.insert("furniture_ms".into(), ms(s));
        }
        if let Some(s) = self.nav {
            map.insert("nav_ms".into(), ms(s));
        }
        map.insert("other_ms".into(), ms(self.other()));
        serde_json::Value::Object(map)
    }
}

/// When each mark was passed in the first frame.
#[derive(Resource, Default)]
struct Times {
    startup: Option<Instant>,
    frame: Option<Instant>,
    update: Option<Instant>,
    update_end: Option<Instant>,
    frame_end: Option<Instant>,
    done: bool,
}

/// The line for Startup and frame 1, from seconds.
fn line(startup: f64, before: f64, update: f64, after: f64) -> String {
    format!("start: Startup {startup:.2} s; frame 1 {:.2} s: {before:.2} s before Update, {update:.2} s in Update, {after:.2} s after it", before + update + after)
}

fn secs(from: Option<Instant>, to: Option<Instant>) -> f64 {
    from.zip(to).map_or(0.0, |(a, b)| b.duration_since(a).as_secs_f64())
}

fn startup_begins(mut t: ResMut<Times>, pre: Option<Res<PreWindowTimes>>) {
    t.startup = Some(Instant::now());
    if let Some(pre) = pre {
        info!("{}", pre.line());
    }
}

fn startup_ends(mut t: ResMut<Times>) {
    t.frame = Some(Instant::now());
}

fn update_begins(mut t: ResMut<Times>) {
    if t.update.is_none() {
        t.update = Some(Instant::now());
    }
}

fn update_ends(mut t: ResMut<Times>) {
    if t.update_end.is_none() {
        t.update_end = Some(Instant::now());
    }
}

fn frame_ends(mut t: ResMut<Times>, gate: Option<Res<PipelineGate>>) {
    if t.done {
        return;
    }
    let now = Instant::now();
    if t.frame_end.is_none() {
        t.frame_end = Some(now);
        info!("{}", line(secs(t.startup, t.frame), secs(t.frame, t.update), secs(t.update, t.update_end), secs(t.update_end, Some(now))));
    }
    match gate {
        Some(gate) if gate.frames() == 0 => {}
        Some(_) => {
            info!("start: first render frame finished {:.2} s after frame 1 ended", secs(t.frame_end, Some(now)));
            t.done = true;
        }
        None => t.done = true,
    }
}

pub struct StartTimesPlugin;

impl Plugin for StartTimesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Times>();
        for mark in [Mark::StartupBegins, Mark::StartupEnds, Mark::UpdateBegins, Mark::UpdateEnds, Mark::FrameEnds] {
            app.init_schedule(mark);
            app.edit_schedule(mark, |s| {
                s.set_executor(SingleThreadedExecutor::new());
            });
        }
        let mut order = app.world_mut().resource_mut::<bevy::app::MainScheduleOrder>();
        order.insert_startup_before(PreStartup, Mark::StartupBegins);
        order.insert_startup_after(PostStartup, Mark::StartupEnds);
        order.insert_before(Update, Mark::UpdateBegins);
        order.insert_after(Update, Mark::UpdateEnds);
        order.insert_after(Last, Mark::FrameEnds);
        app.add_systems(Mark::StartupBegins, startup_begins)
            .add_systems(Mark::StartupEnds, startup_ends)
            .add_systems(Mark::UpdateBegins, update_begins)
            .add_systems(Mark::UpdateEnds, update_ends)
            .add_systems(Mark::FrameEnds, frame_ends);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_adds_up_the_frame() {
        assert_eq!(line(0.81, 0.2, 1.5, 0.1), "start: Startup 0.81 s; frame 1 1.80 s: 0.20 s before Update, 1.50 s in Update, 0.10 s after it");
    }

    #[test]
    fn the_marks_come_in_order_and_are_reported_once() {
        let mut app = App::new();
        app.insert_resource(PreWindowTimes { total: 1.5, plugins: 0.4, archives: 0.3, index: 0.5, lod: 0.1, ..Default::default() });
        app.add_plugins(StartTimesPlugin);
        app.update();
        app.update();
        let t = app.world().resource::<Times>();
        let at = [t.startup, t.frame, t.update, t.update_end, t.frame_end].map(|m| m.expect("every mark passed"));
        assert!(at.windows(2).all(|w| w[0] <= w[1]), "in order");
        assert!(t.done, "no render world to wait for here");
    }

    #[test]
    fn pre_window_line_and_json_cover_fresh_start_and_save_load() {
        let fresh = PreWindowTimes {
            total: 12.5,
            plugins: 1.25,
            save: None,
            archives: 2.0,
            index: 5.5,
            lod: 0.25,
            actors: Some(2.0),
            residents: Some(1.0),
            furniture: None,
            nav: None,
        };
        assert_eq!(
            fresh.line(),
            "start: before the window: 12.50 s (plugins 1.25 s, archives 2.00 s, index 5.50 s, lod 0.25 s, actors 2.00 s, residents 1.00 s, other 0.50 s)"
        );
        let j = fresh.to_json();
        assert_eq!(j["total_ms"], 12500.0);
        assert_eq!(j["plugins_ms"], 1250.0);
        assert!(j.get("save_ms").is_none());
        assert_eq!(j["archives_ms"], 2000.0);
        assert_eq!(j["index_ms"], 5500.0);
        assert_eq!(j["lod_ms"], 250.0);
        assert_eq!(j["actors_ms"], 2000.0);
        assert_eq!(j["residents_ms"], 1000.0);
        assert!(j.get("furniture_ms").is_none());
        assert!(j.get("nav_ms").is_none());
        assert_eq!(j["other_ms"], 500.0);

        let load = PreWindowTimes {
            total: 48.0,
            plugins: 10.0,
            save: Some(0.5),
            archives: 8.0,
            index: 20.0,
            lod: 0.1,
            actors: Some(4.0),
            residents: Some(3.0),
            furniture: Some(1.0),
            nav: Some(1.4),
        };
        assert_eq!(
            load.line(),
            "start: before the window: 48.00 s (plugins 10.00 s, save 0.50 s, archives 8.00 s, index 20.00 s, lod 0.10 s, actors 4.00 s, residents 3.00 s, furniture 1.00 s, nav 1.40 s, other 0.00 s)"
        );
        let jl = load.to_json();
        assert_eq!(jl["save_ms"], 500.0);
        assert_eq!(jl["furniture_ms"], 1000.0);
        assert_eq!(jl["nav_ms"], 1400.0);
        assert_eq!(jl["other_ms"], 0.0);
    }
}
