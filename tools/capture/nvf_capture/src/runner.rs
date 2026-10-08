//! Capture jobs and the frame-driven step runner (no game dependency, unit-tested).
//!
//! Protocol and file formats: docs/spec-notes/xnvse-capture.md §4.

use serde::Deserialize;
use serde_json::{json, Value as Json};

pub const JOB_FORMAT: &str = "fnv.capture.steps/1";
/// DirectX scan codes. [documented: GECK wiki `IsKeyPressed`, DirectX scan code table]
pub const KEY_END: u32 = 207;
pub const KEY_HOME: u32 = 199;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub format: String,
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub start: String,
    pub steps: Vec<Step>,
}

/// One step; the variant is chosen by its key (`query`, `script`, `console`, `manual`,
/// `frames`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Step {
    Query(Query),
    Script(ScriptStep),
    Console(ConsoleStep),
    Manual(ManualStep),
    Frames(FramesStep),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub query: String,
    pub label: String,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptStep {
    pub script: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsoleStep {
    pub console: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualStep {
    pub manual: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FramesStep {
    pub frames: u32,
}

impl Job {
    pub fn parse(text: &str) -> Result<Job, String> {
        let job: Job = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if job.format != JOB_FORMAT {
            return Err(format!("unsupported job format {:?} (expected {JOB_FORMAT})", job.format));
        }
        if job.name.is_empty() || !job.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
            return Err(format!("job name {:?} must be non-empty [A-Za-z0-9_.-]", job.name));
        }
        Ok(job)
    }
}

/// A function script's result.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
    Nothing,
}

impl Value {
    fn to_json(&self) -> Json {
        match self {
            Value::Number(n) if n.is_finite() => json!(n),
            Value::Number(_) | Value::Nothing => Json::Null,
            Value::Text(s) => json!(s),
        }
    }
}

/// What the runner needs from the game (implemented over xNVSE in `lib.rs`, mocked in tests).
pub trait Host {
    /// Compile and run a "Begin Function {} … End" script; its SetFunctionValue result.
    fn script(&mut self, text: &str) -> Result<Value, String>;
    /// Run one console line.
    fn console(&mut self, line: &str) -> Result<(), String>;
    fn key_down(&mut self, scancode: u32) -> bool;
    /// Show a short message on screen.
    fn show(&mut self, text: &str);
}

/// Something that happened to a job during a tick; the caller handles files.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Output lines for the job (JSON objects, one per line in the .jsonl file).
    Output { job: String, line: Json },
    Finished { job: String },
    Aborted { job: String },
    /// Automatic mode only: no job is left for this session (sent once per load).
    AllDone,
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Idle,
    /// Waiting for End (or, in automatic mode, `wait` more game loops).
    Announced { wait: u32 },
    Running { step: usize, wait: u32, manual: bool },
}

pub struct Runner {
    pending: Vec<Job>,
    state: State,
    /// Key state in the previous tick, for press edges: (End, Home).
    prev: (bool, bool),
    /// Automatic mode: start announced jobs after this many game loops instead of
    /// waiting for End (docs/spec-notes/xnvse-capture.md §4). Manual steps still wait.
    auto_start: Option<u32>,
    pub plugin_version: u32,
    pub nvse_version: u32,
    pub runtime_version: u32,
}

impl Runner {
    pub fn new(plugin_version: u32, nvse_version: u32, runtime_version: u32) -> Runner {
        Runner { pending: Vec::new(), state: State::Idle, prev: (false, false), auto_start: None, plugin_version, nvse_version, runtime_version }
    }

    /// Switch to automatic mode (see `auto_start`).
    pub fn set_auto_start(&mut self, frames: Option<u32>) {
        self.auto_start = frames;
    }

    pub fn current(&self) -> Option<&Job> {
        match self.state {
            State::Idle => None,
            _ => self.pending.first(),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, State::Running { .. })
    }

    /// A save finished loading. Replaces the pending jobs (rescanned by the caller),
    /// aborting a running job, and announces the first one.
    pub fn game_loaded(&mut self, jobs: Vec<Job>, host: &mut dyn Host) -> Vec<Event> {
        let mut ev = Vec::new();
        if let (State::Running { .. }, Some(j)) = (&self.state, self.pending.first()) {
            ev.push(Event::Output { job: j.name.clone(), line: json!({"kind": "aborted", "reason": "a save was loaded"}) });
            ev.push(Event::Aborted { job: j.name.clone() });
        }
        self.pending = jobs;
        self.announce(host, &mut ev);
        ev
    }

    fn announce(&mut self, host: &mut dyn Host, ev: &mut Vec<Event>) {
        match self.pending.first() {
            Some(j) => {
                let title = if j.title.is_empty() { &j.name } else { &j.title };
                match self.auto_start {
                    Some(wait) => {
                        host.show(&format!("NVF capture: {title}. Starting automatically."));
                        self.state = State::Announced { wait };
                    }
                    None => {
                        host.show(&format!("NVF capture: {title}. {} Press End to start, Home to skip.", j.start));
                        self.state = State::Announced { wait: 0 };
                    }
                }
            }
            None => {
                if self.auto_start.is_some() {
                    ev.push(Event::AllDone);
                }
                self.state = State::Idle;
            }
        }
    }

    /// One game loop.
    pub fn tick(&mut self, host: &mut dyn Host) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.state == State::Idle {
            return ev;
        }
        let (end, home) = (host.key_down(KEY_END), host.key_down(KEY_HOME));
        let (end_pressed, home_pressed) = (end && !self.prev.0, home && !self.prev.1);
        self.prev = (end, home);
        let job = self.pending.first().expect("not idle").clone();
        match self.state.clone() {
            State::Idle => {}
            State::Announced { wait } => {
                let auto_go = match self.auto_start {
                    Some(_) if wait > 0 => {
                        self.state = State::Announced { wait: wait - 1 };
                        return ev;
                    }
                    Some(_) => true,
                    None => false,
                };
                if end_pressed || auto_go {
                    ev.push(Event::Output {
                        job: job.name.clone(),
                        line: json!({"kind": "start", "job": job.name, "plugin_version": self.plugin_version,
                                     "nvse_version": format!("0x{:08X}", self.nvse_version),
                                     "runtime_version": format!("0x{:08X}", self.runtime_version)}),
                    });
                    host.show(&format!("NVF capture: running {} ({} steps).", job.name, job.steps.len()));
                    self.state = State::Running { step: 0, wait: 0, manual: false };
                } else if home_pressed {
                    host.show(&format!("NVF capture: skipped {}.", job.name));
                    self.pending.remove(0);
                    self.announce(host, &mut ev);
                }
            }
            State::Running { step, wait, manual } => {
                if wait > 0 {
                    self.state = State::Running { step, wait: wait - 1, manual };
                    return ev;
                }
                if manual {
                    if end_pressed {
                        self.state = State::Running { step: step + 1, wait: 0, manual: false };
                    }
                    return ev;
                }
                if step >= job.steps.len() {
                    ev.push(Event::Output { job: job.name.clone(), line: json!({"kind": "end", "steps": job.steps.len()}) });
                    ev.push(Event::Finished { job: job.name.clone() });
                    host.show(&format!("NVF capture: {} finished.", job.name));
                    self.pending.remove(0);
                    self.announce(host, &mut ev);
                    return ev;
                }
                let mut next = State::Running { step: step + 1, wait: 0, manual: false };
                match &job.steps[step] {
                    Step::Query(q) => {
                        let mut line = json!({"kind": "result", "step": step, "label": q.label, "field": q.field});
                        match host.script(&q.query) {
                            Ok(v) => line["value"] = v.to_json(),
                            Err(e) => line["error"] = json!(e),
                        }
                        ev.push(Event::Output { job: job.name.clone(), line });
                    }
                    Step::Script(s) => {
                        if let Err(e) = host.script(&s.script) {
                            ev.push(Event::Output { job: job.name.clone(), line: json!({"kind": "error", "step": step, "error": e}) });
                        }
                    }
                    Step::Console(c) => {
                        if let Err(e) = host.console(&c.console) {
                            ev.push(Event::Output { job: job.name.clone(), line: json!({"kind": "error", "step": step, "error": e}) });
                        }
                    }
                    Step::Manual(m) => {
                        host.show(&format!("NVF capture: {} Then press End.", m.manual));
                        next = State::Running { step, wait: 0, manual: true };
                    }
                    Step::Frames(f) => next = State::Running { step: step + 1, wait: f.frames, manual: false },
                }
                self.state = next;
            }
        }
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct Mock {
        keys: VecDeque<(bool, bool)>,
        scripts: Vec<String>,
        console: Vec<String>,
        shown: Vec<String>,
    }

    impl Host for Mock {
        fn script(&mut self, text: &str) -> Result<Value, String> {
            self.scripts.push(text.into());
            match text {
                "count" => Ok(Value::Number(5.0)),
                "text" => Ok(Value::Text("a|b".into())),
                "none" => Ok(Value::Nothing),
                _ => Err("compile failed".into()),
            }
        }
        fn console(&mut self, line: &str) -> Result<(), String> {
            self.console.push(line.into());
            Ok(())
        }
        fn key_down(&mut self, scancode: u32) -> bool {
            // One (End, Home) pair per tick, consumed on the Home query.
            let (e, h) = self.keys.front().copied().unwrap_or((false, false));
            if scancode == KEY_HOME {
                self.keys.pop_front();
                h
            } else {
                e
            }
        }
        fn show(&mut self, text: &str) {
            self.shown.push(text.into());
        }
    }

    const JOB: &str = r#"{"format": "fnv.capture.steps/1", "name": "t1", "title": "Test", "start": "Load a save.",
      "steps": [{"query": "count", "label": "caps", "field": "count"},
                {"script": "bad"},
                {"console": "player.setlevel 5"},
                {"manual": "Wait 24 hours."},
                {"frames": 2},
                {"query": "text", "label": "mods", "field": "names"},
                {"query": "none", "label": "x", "field": "y"}]}"#;

    fn lines(ev: &[Event]) -> Vec<Json> {
        ev.iter().filter_map(|e| if let Event::Output { line, .. } = e { Some(line.clone()) } else { None }).collect()
    }

    #[test]
    fn parses_every_step_kind_and_rejects_bad_jobs() {
        let j = Job::parse(JOB).unwrap();
        assert_eq!(j.steps.len(), 7);
        assert!(matches!(j.steps[0], Step::Query(_)));
        assert!(matches!(j.steps[1], Step::Script(_)));
        assert!(matches!(j.steps[2], Step::Console(_)));
        assert!(matches!(j.steps[3], Step::Manual(_)));
        assert_eq!(j.steps[4], Step::Frames(FramesStep { frames: 2 }));
        assert!(Job::parse(&JOB.replace("fnv.capture.steps/1", "x")).is_err());
        assert!(Job::parse(&JOB.replace("\"t1\"", "\"../evil\"")).is_err());
        assert!(Job::parse(&JOB.replace("\"frames\": 2", "\"frames\": 2, \"extra\": 1")).is_err());
    }

    #[test]
    fn runs_a_job_step_by_step() {
        let mut m = Mock::default();
        let mut r = Runner::new(1, 0x0604_0090, 0x0400_20D0);
        assert!(r.tick(&mut m).is_empty(), "idle before a load");
        r.game_loaded(vec![Job::parse(JOB).unwrap()], &mut m);
        assert!(m.shown[0].contains("Press End to start"));
        let mut all = Vec::new();
        // tick 1: nothing pressed; tick 2: End pressed -> start.
        m.keys.extend([(false, false), (true, false)]);
        all.extend(r.tick(&mut m));
        all.extend(r.tick(&mut m));
        assert!(r.is_running());
        // Holding End must not skip the manual step later: release first.
        for _ in 0..4 {
            m.keys.push_back((false, false));
            all.extend(r.tick(&mut m)); // query, script, console, manual shown
        }
        assert!(m.shown.last().unwrap().contains("Wait 24 hours"));
        for _ in 0..3 {
            m.keys.push_back((false, false));
            all.extend(r.tick(&mut m)); // still waiting for End
        }
        m.keys.extend([(true, false), (false, false), (false, false), (false, false)]);
        for _ in 0..4 {
            all.extend(r.tick(&mut m)); // End -> frames step sets wait 2 -> two waits
        }
        for _ in 0..4 {
            m.keys.push_back((false, false));
            all.extend(r.tick(&mut m));
        }
        let out = lines(&all);
        assert_eq!(out[0]["kind"], "start");
        assert_eq!(out[0]["nvse_version"], "0x06040090");
        assert_eq!(out[1], json!({"kind": "result", "step": 0, "label": "caps", "field": "count", "value": 5.0}));
        assert_eq!(out[2]["kind"], "error");
        assert_eq!(out[2]["step"], 1);
        assert_eq!(out[3]["value"], "a|b");
        assert_eq!(out[4]["value"], Json::Null);
        assert_eq!(out[5]["kind"], "end");
        assert!(all.contains(&Event::Finished { job: "t1".into() }));
        assert_eq!(m.console, vec!["player.setlevel 5"]);
        assert!(r.current().is_none());
        assert!(!r.is_running());
    }

    #[test]
    fn home_skips_and_a_load_aborts() {
        let mut m = Mock::default();
        let mut r = Runner::new(1, 0, 0);
        let a = Job::parse(JOB).unwrap();
        let mut b = a.clone();
        b.name = "t2".into();
        r.game_loaded(vec![a.clone(), b.clone()], &mut m);
        m.keys.push_back((false, true));
        r.tick(&mut m);
        assert_eq!(r.current().unwrap().name, "t2");
        m.keys.push_back((true, false));
        r.tick(&mut m);
        assert!(r.is_running());
        let ev = r.game_loaded(vec![a, b], &mut m);
        assert!(ev.contains(&Event::Aborted { job: "t2".into() }));
        assert!(!r.is_running());
        assert_eq!(r.current().unwrap().name, "t1");
    }

    #[test]
    fn automatic_mode_starts_after_a_delay_and_reports_all_done() {
        let mut m = Mock::default();
        let mut r = Runner::new(1, 0, 0);
        r.set_auto_start(Some(2));
        let job = Job::parse(r#"{"format": "fnv.capture.steps/1", "name": "a1",
            "steps": [{"query": "count", "label": "caps", "field": "count"}, {"manual": "Do it."}]}"#).unwrap();
        r.game_loaded(vec![job], &mut m);
        assert!(m.shown[0].contains("Starting automatically"));
        let mut all = Vec::new();
        for _ in 0..2 {
            all.extend(r.tick(&mut m)); // the start delay, no key pressed
        }
        assert!(!r.is_running());
        all.extend(r.tick(&mut m));
        assert!(r.is_running());
        all.extend(r.tick(&mut m)); // query
        for _ in 0..5 {
            all.extend(r.tick(&mut m)); // a manual step still waits for End
        }
        assert!(r.is_running());
        assert!(!all.contains(&Event::AllDone));
        m.keys.extend([(true, false), (false, false)]);
        all.extend(r.tick(&mut m));
        all.extend(r.tick(&mut m)); // end of job
        assert!(all.contains(&Event::Finished { job: "a1".into() }));
        assert_eq!(all.last(), Some(&Event::AllDone));
        assert_eq!(lines(&all)[1]["value"], 5.0);
        // No pending jobs at a load: done at once; never in manual mode.
        assert_eq!(r.game_loaded(Vec::new(), &mut m), vec![Event::AllDone]);
        let mut manual = Runner::new(1, 0, 0);
        assert!(manual.game_loaded(Vec::new(), &mut m).is_empty());
    }
}
